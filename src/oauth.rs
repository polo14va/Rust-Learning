use axum::{
    extract::{OriginalUri, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    Form, Json,
};
use axum_extra::extract::cookie::{Cookie, SameSite};
use axum_extra::extract::CookieJar;
use base64::{engine::general_purpose, Engine as _};
use base64ct::Encoding;
use chrono::{Duration, Utc};
use redis::AsyncCommands;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use time::Duration as CookieDuration;
use urlencoding;
use uuid::Uuid;

const CONSENT_TTL_DAYS: u64 = 30;

use crate::{
    auth,
    db,
    error::AppError,
    rate_limit,
    templates,
    models::{
        AppState, AuthorizationCode, IntrospectionResponse, JwkKey, OAuthClient, OpenIdConfiguration,
        RefreshTokenRecord, TokenResponse, UserInfoResponse,
    },
};

#[derive(Deserialize)]
pub struct AuthorizeParams {
    pub response_type: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub scope: Option<String>,
    pub state: Option<String>,
    pub code_challenge: Option<String>,
    pub code_challenge_method: Option<String>,
    pub nonce: Option<String>,
}

#[derive(Deserialize)]
pub struct TokenRequest {
    pub grant_type: String,
    pub code: Option<String>,
    pub redirect_uri: Option<String>,
    pub code_verifier: Option<String>,
    pub refresh_token: Option<String>,
    pub scope: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
}

#[derive(Deserialize)]
pub struct IntrospectRequest {
    pub token: String,
}

#[derive(Deserialize)]
pub struct RevokeRequest {
    pub token: String,
}

#[derive(Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
    pub next: Option<String>,
}

#[derive(Deserialize)]
pub struct ConsentForm {
    pub client_id: String,
    pub redirect_uri: String,
    pub scope: String,
    pub state: Option<String>,
    pub code_challenge: Option<String>,
    pub code_challenge_method: Option<String>,
    pub nonce: Option<String>,
    pub decision: String,
    pub response_type: Option<String>,
}

#[derive(Deserialize)]
pub struct LoginPageParams {
    pub next: Option<String>,
    pub error: Option<String>,
}

pub async fn login_page(Query(params): Query<LoginPageParams>) -> Html<String> {
    let next_hidden = params
        .next
        .as_ref()
        .map(|n| format!(r#"<input type="hidden" name="next" value="{}"/>"#, html_escape(n)))
        .unwrap_or_default();
    let error_html = params
        .error
        .as_ref()
        .map(|e| format!(r#"<div class="error">{}</div>"#, html_escape(e)))
        .unwrap_or_default();

    let page = templates::render_login_page(&next_hidden, &error_html);
    Html(page)
}

pub async fn options_ok() -> StatusCode {
    StatusCode::NO_CONTENT
}

pub async fn login_form(
    State(state): State<AppState>,
    jar: CookieJar,
    Form(form): Form<LoginForm>,
) -> Result<(CookieJar, impl IntoResponse), AppError> {
    let rate_key = format!("rate_limit:login_ui:{}", form.username);
    if !rate_limit::check_rate_limit(&state.redis_client, &rate_key).await? {
        return Err(AppError::AuthError("Too many requests. Try again later.".to_string()));
    }

    let user = db::get_user_by_username(&state.pool, &form.username)
        .await?
        .ok_or_else(|| AppError::AuthError("Credenciales inválidas".to_string()))?;

    if !auth::verify_password(&form.password, &user.password_hash)? {
        let mut redirect = String::from("/login");
        let mut first = true;
        if let Some(n) = form.next.as_ref() {
            redirect.push_str(&format!("?next={}", urlencoding::encode(n)));
            first = false;
        }
        redirect.push_str(if first { "?error=" } else { "&error=" });
        redirect.push_str("Credenciales%20inv%C3%A1lidas");

        let redirect = Redirect::temporary(&redirect);
        return Ok((jar, redirect));
    }

    // Crear sesión SSO y cookie
    let session_id = auth::create_session(&state.redis_client, &user.username).await?;
    let cookie = Cookie::build(("sso_session", session_id))
        .http_only(true)
        .secure(false)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(CookieDuration::minutes(60))
        .build();
    let updated_jar = jar.add(cookie);

    let redirect_target = form
        .next
        .and_then(|n| decode_next(&n))
        .unwrap_or_else(|| "/".to_string());

    Ok((updated_jar, Redirect::temporary(&redirect_target)))
}

pub async fn authorize(
    State(state): State<AppState>,
    jar: CookieJar,
    uri: OriginalUri,
    Query(params): Query<AuthorizeParams>,
) -> Result<Response, AppError> {
    if params.response_type != "code" {
        return Err(AppError::AuthError("Unsupported response_type".to_string()));
    }

    let client = db::get_oauth_client(&state.pool, &params.client_id)
        .await?
        .ok_or_else(|| AppError::AuthError("Invalid client_id".to_string()))?;

    validate_redirect_uri(&client, &params.redirect_uri)?;
    validate_scope(&client, params.scope.as_deref())?;

    // Validar sesión SSO
    let username = match jar.get("sso_session") {
        Some(cookie) => auth::validate_session(&state.redis_client, cookie.value()).await?,
        None => None,
    };

    let username = match username {
        Some(u) => u,
        None => {
            let uri_string = uri.to_string();
            let next = urlencoding::encode(uri_string.as_str());
            let login_url = format!("/login?next={}", next);
            return Ok(Redirect::temporary(&login_url).into_response());
        }
    };

    let scope = params.scope.clone().unwrap_or_else(|| client.scopes.clone());

    if !has_consent(&state.redis_client, &username, &client.client_id, &scope).await? {
        let page = render_consent_page(&client, &params, &scope, &username);
        return Ok(Html(page).into_response());
    }

    let code = issue_authorization_code(
        &state,
        &params.client_id,
        &username,
        &params.redirect_uri,
        &scope,
        params.code_challenge.clone(),
        params.code_challenge_method.clone(),
        params.nonce.clone(),
    )
    .await?;

    let mut separator = "?";
    if params.redirect_uri.contains('?') {
        separator = "&";
    }
    let mut redirect_url = format!("{}{}code={}", params.redirect_uri, separator, code);
    if let Some(state) = params.state {
        redirect_url.push_str(&format!("&state={}", state));
    }

    Ok(Redirect::temporary(&redirect_url).into_response())
}

pub async fn consent_page(
    State(state): State<AppState>,
    jar: CookieJar,
    uri: OriginalUri,
    Query(params): Query<AuthorizeParams>,
) -> Result<Response, AppError> {
    if params.response_type != "code" {
        return Err(AppError::AuthError("Unsupported response_type".to_string()));
    }

    let client = db::get_oauth_client(&state.pool, &params.client_id)
        .await?
        .ok_or_else(|| AppError::AuthError("Invalid client_id".to_string()))?;

    validate_redirect_uri(&client, &params.redirect_uri)?;
    validate_scope(&client, params.scope.as_deref())?;

    let username = match jar.get("sso_session") {
        Some(cookie) => auth::validate_session(&state.redis_client, cookie.value()).await?,
        None => None,
    };

    let username = match username {
        Some(u) => u,
        None => {
            let uri_string = uri.to_string();
            let next = urlencoding::encode(uri_string.as_str());
            let login_url = format!("/login?next={}", next);
            return Ok(Redirect::temporary(&login_url).into_response());
        }
    };

    let scope = params.scope.clone().unwrap_or_else(|| client.scopes.clone());
    let page = render_consent_page(&client, &params, &scope, &username);
    Ok(Html(page).into_response())
}

pub async fn submit_consent(
    State(state): State<AppState>,
    jar: CookieJar,
    Form(form): Form<ConsentForm>,
) -> Result<impl IntoResponse, AppError> {
    let username = match jar.get("sso_session") {
        Some(cookie) => auth::validate_session(&state.redis_client, cookie.value()).await?,
        None => None,
    };

    let username = match username {
        Some(u) => u,
        None => {
            let login_url = "/login?next=/consent".to_string();
            return Ok(Redirect::temporary(&login_url));
        }
    };

    if let Some(rt) = form.response_type.as_deref() {
        if rt != "code" {
            return Err(AppError::AuthError("Unsupported response_type".to_string()));
        }
    }

    let client = db::get_oauth_client(&state.pool, &form.client_id)
        .await?
        .ok_or_else(|| AppError::AuthError("Invalid client_id".to_string()))?;

    validate_redirect_uri(&client, &form.redirect_uri)?;
    validate_scope(&client, Some(&form.scope))?;

    if form.decision == "deny" {
        let mut separator = if form.redirect_uri.contains('?') { "&" } else { "?" };
        let mut redirect_url = format!("{}{}error=access_denied", form.redirect_uri, separator);
        if let Some(state) = &form.state {
            separator = "&";
            redirect_url.push_str(&format!("{}state={}", separator, state));
        }
        return Ok(Redirect::temporary(&redirect_url));
    }

    store_consent(&state.redis_client, &username, &form.client_id, &form.scope).await?;

    let code = issue_authorization_code(
        &state,
        &form.client_id,
        &username,
        &form.redirect_uri,
        &form.scope,
        form.code_challenge.clone(),
        form.code_challenge_method.clone(),
        form.nonce.clone(),
    )
    .await?;

    let mut separator = if form.redirect_uri.contains('?') { "&" } else { "?" };
    let mut redirect_url = format!("{}{}code={}", form.redirect_uri, separator, code);
    if let Some(state) = form.state {
        separator = "&";
        redirect_url.push_str(&format!("{}state={}", separator, state));
    }

    Ok(Redirect::temporary(&redirect_url))
}

pub async fn token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(body): Form<TokenRequest>,
) -> Result<Json<TokenResponse>, AppError> {
    let (client_id, client_secret) = extract_client_credentials(&headers, &body)?;
    let client = db::get_oauth_client(&state.pool, &client_id)
        .await?
        .ok_or_else(|| AppError::AuthError("Invalid client".to_string()))?;

    if client.client_secret != client_secret {
        return Err(AppError::AuthError("Invalid client credentials".to_string()));
    }

    match body.grant_type.as_str() {
        "authorization_code" => {
            let code = body
                .code
                .clone()
                .ok_or_else(|| AppError::AuthError("Missing code".to_string()))?;
            let redirect_uri = body
                .redirect_uri
                .clone()
                .ok_or_else(|| AppError::AuthError("Missing redirect_uri".to_string()))?;

            let record = db::consume_authorization_code(&state.pool, &code)
                .await?
                .ok_or_else(|| AppError::AuthError("Invalid or expired code".to_string()))?;

            if record.client_id != client_id {
                return Err(AppError::AuthError("Code/client mismatch".to_string()));
            }
            if record.redirect_uri != redirect_uri {
                return Err(AppError::AuthError("redirect_uri mismatch".to_string()));
            }

            if let Some(challenge) = &record.code_challenge {
                let verifier = body
                    .code_verifier
                    .clone()
                    .ok_or_else(|| AppError::AuthError("Missing code_verifier".to_string()))?;
                if !verify_pkce(&verifier, challenge, record.code_challenge_method.as_deref()) {
                    return Err(AppError::AuthError("Invalid code_verifier".to_string()));
                }
            }

            let scope = record.scope.clone();
            let access_token = auth::create_access_token(
                &record.username,
                &scope,
                &client_id,
                &state.issuer,
                &state.keys,
                None,
            )?;

            let refresh_token = auth::create_refresh_token();
            let session = auth::RefreshSession {
                username: record.username.clone(),
                client_id: client_id.clone(),
                scope: scope.clone(),
            };
            auth::store_refresh_token(&state.redis_client, &session, &refresh_token).await?;

            let expires_at = Utc::now() + Duration::days(7);
            let refresh_record = RefreshTokenRecord {
                refresh_token: refresh_token.clone(),
                client_id: client_id.clone(),
                username: record.username.clone(),
                scope: scope.clone(),
                expires_at,
                revoked: false,
            };
            db::store_refresh_token_record(&state.pool, &refresh_record).await?;

            let id_token = auth::create_id_token(
                &record.username,
                &client_id,
                &state.issuer,
                &state.keys,
                record.nonce.clone(),
                None,
            )
            .ok();

            let response = TokenResponse {
                access_token,
                token_type: "Bearer".to_string(),
                expires_in: 60 * 15,
                refresh_token: Some(refresh_token),
                id_token,
                scope: Some(scope),
            };

            Ok(Json(response))
        }
        "refresh_token" => {
            let refresh_token = body
                .refresh_token
                .clone()
                .ok_or_else(|| AppError::AuthError("Missing refresh_token".to_string()))?;

            let session = auth::validate_refresh_token(&state.redis_client, &refresh_token)
                .await?
                .ok_or_else(|| AppError::AuthError("Invalid or expired refresh token".to_string()))?;

            if session.client_id != client_id {
                return Err(AppError::AuthError("Client mismatch".to_string()));
            }

            if let Some(record) = db::get_refresh_token_record(&state.pool, &refresh_token).await? {
                if record.revoked || record.expires_at < Utc::now() {
                    return Err(AppError::AuthError("Refresh token expired or revoked".to_string()));
                }
            }

            let scope = session.scope.clone();
            let access_token = auth::create_access_token(
                &session.username,
                &scope,
                &client_id,
                &state.issuer,
                &state.keys,
                None,
            )?;

            let response = TokenResponse {
                access_token,
                token_type: "Bearer".to_string(),
                expires_in: 60 * 15,
                refresh_token: Some(refresh_token),
                id_token: None,
                scope: Some(scope),
            };
            Ok(Json(response))
        }
        "client_credentials" => {
            validate_scope(&client, body.scope.as_deref())?;
            let scope = body
                .scope
                .unwrap_or_else(|| client.scopes.clone());
            let access_token = auth::create_access_token(
                &client_id,
                &scope,
                &client_id,
                &state.issuer,
                &state.keys,
                None,
            )?;

            let response = TokenResponse {
                access_token,
                token_type: "Bearer".to_string(),
                expires_in: 60 * 15,
                refresh_token: None,
                id_token: None,
                scope: Some(scope),
            };
            Ok(Json(response))
        }
        _ => Err(AppError::AuthError("Unsupported grant_type".to_string())),
    }
}

pub async fn introspect(
    State(state): State<AppState>,
    Form(body): Form<IntrospectRequest>,
) -> Result<Json<IntrospectionResponse>, AppError> {
    match auth::validate_jwt(&body.token, &state.keys) {
        Ok(data) => {
            let claims = data.claims;
            let response = IntrospectionResponse {
                active: true,
                sub: Some(claims.sub),
                client_id: claims.aud,
                scope: claims.scope,
                exp: Some(claims.exp as i64),
            };
            Ok(Json(response))
        }
        Err(_) => Ok(Json(IntrospectionResponse {
            active: false,
            sub: None,
            client_id: None,
            scope: None,
            exp: None,
        })),
    }
}

pub async fn revoke(
    State(state): State<AppState>,
    Form(body): Form<RevokeRequest>,
) -> Result<impl IntoResponse, AppError> {
    auth::revoke_refresh_token(&state.redis_client, &body.token).await?;
    db::revoke_refresh_token_record(&state.pool, &body.token).await?;
    Ok(StatusCode::OK)
}

pub async fn userinfo(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<UserInfoResponse>, AppError> {
    let token = extract_bearer(&headers)?;
    let data = auth::validate_jwt(&token, &state.keys)
        .map_err(|_| AppError::AuthError("Invalid token".to_string()))?;
    let username = data.claims.sub;

    let user = db::get_user_by_username(&state.pool, &username)
        .await?
        .ok_or_else(|| AppError::AuthError("User not found".to_string()))?;

    Ok(Json(UserInfoResponse {
        sub: username.clone(),
        preferred_username: username,
        email: user.email,
    }))
}

pub async fn openid_configuration(State(state): State<AppState>) -> Json<OpenIdConfiguration> {
    let issuer = state.issuer.clone();
    let config = OpenIdConfiguration {
        issuer: issuer.clone(),
        authorization_endpoint: format!("{}/authorize", issuer),
        token_endpoint: format!("{}/token", issuer),
        userinfo_endpoint: format!("{}/userinfo", issuer),
        jwks_uri: format!("{}/.well-known/jwks.json", issuer),
        response_types_supported: vec!["code".to_string()],
        subject_types_supported: vec!["public".to_string()],
        id_token_signing_alg_values_supported: vec!["RS256".to_string()],
        token_endpoint_auth_methods_supported: vec!["client_secret_basic".to_string(), "client_secret_post".to_string()],
        grant_types_supported: vec![
            "authorization_code".to_string(),
            "refresh_token".to_string(),
            "client_credentials".to_string(),
        ],
    };

    Json(config)
}

pub async fn jwks(State(state): State<AppState>) -> Json<serde_json::Value> {
    let key = JwkKey {
        kty: "RSA".to_string(),
        use_: "sig".to_string(),
        kid: state.keys.kid.clone(),
        alg: "RS256".to_string(),
        n: state.keys.n.clone(),
        e: state.keys.e.clone(),
    };

    Json(serde_json::json!({ "keys": [ key ] }))
}

// --- Helpers ---

async fn issue_authorization_code(
    state: &AppState,
    client_id: &str,
    username: &str,
    redirect_uri: &str,
    scope: &str,
    code_challenge: Option<String>,
    code_challenge_method: Option<String>,
    nonce: Option<String>,
) -> Result<String, AppError> {
    let code = Uuid::new_v4().to_string();
    let expires_at = Utc::now() + Duration::minutes(5);

    let auth_code = AuthorizationCode {
        code: code.clone(),
        client_id: client_id.to_string(),
        username: username.to_string(),
        redirect_uri: redirect_uri.to_string(),
        scope: scope.to_string(),
        code_challenge,
        code_challenge_method,
        nonce,
        expires_at,
    };

    db::store_authorization_code(&state.pool, &auth_code).await?;
    Ok(code)
}

async fn has_consent(
    redis_client: &redis::Client,
    username: &str,
    client_id: &str,
    scope: &str,
) -> Result<bool, AppError> {
    let mut conn = redis_client
        .get_async_connection()
        .await
        .map_err(|e| AppError::InternalError(format!("Redis connection error: {}", e)))?;
    let key = format!("consent:{}:{}:{}", username, client_id, scope);
    let exists: bool = conn
        .exists(&key)
        .await
        .map_err(|e| AppError::InternalError(format!("Redis EXISTS error: {}", e)))?;
    Ok(exists)
}

async fn store_consent(
    redis_client: &redis::Client,
    username: &str,
    client_id: &str,
    scope: &str,
) -> Result<(), AppError> {
    let mut conn = redis_client
        .get_async_connection()
        .await
        .map_err(|e| AppError::InternalError(format!("Redis connection error: {}", e)))?;
    let key = format!("consent:{}:{}:{}", username, client_id, scope);
    let _: () = conn
        .set_ex(&key, "1", CONSENT_TTL_DAYS * 24 * 3600)
        .await
        .map_err(|e| AppError::InternalError(format!("Redis SET error: {}", e)))?;
    Ok(())
}

fn render_consent_page(
    client: &OAuthClient,
    params: &AuthorizeParams,
    scope: &str,
    username: &str,
) -> String {
    let scopes_html: String = scope
        .split_whitespace()
        .map(|s| format!("<li>{}</li>", html_escape(s)))
        .collect();

    let hidden_state = params
        .state
        .as_ref()
        .map(|s| format!(r#"<input type="hidden" name="state" value="{}"/>"#, html_escape(s)))
        .unwrap_or_default();
    let hidden_code_challenge = params
        .code_challenge
        .as_ref()
        .map(|c| format!(r#"<input type="hidden" name="code_challenge" value="{}"/>"#, html_escape(c)))
        .unwrap_or_default();
    let hidden_code_challenge_method = params
        .code_challenge_method
        .as_ref()
        .map(|c| format!(r#"<input type="hidden" name="code_challenge_method" value="{}"/>"#, html_escape(c)))
        .unwrap_or_default();
    let hidden_nonce = params
        .nonce
        .as_ref()
        .map(|n| format!(r#"<input type="hidden" name="nonce" value="{}"/>"#, html_escape(n)))
        .unwrap_or_default();

    templates::render_consent_page(
        &html_escape(&client.name),
        &html_escape(username),
        &scopes_html,
        &html_escape(&client.client_id),
        &html_escape(&params.redirect_uri),
        &html_escape(scope),
        &hidden_state,
        &hidden_code_challenge,
        &hidden_code_challenge_method,
        &hidden_nonce,
    )
}

fn validate_redirect_uri(client: &OAuthClient, redirect_uri: &str) -> Result<(), AppError> {
    let allowed: Vec<&str> = client.redirect_uris.split(',').map(|s| s.trim()).collect();
    if allowed.iter().any(|uri| uri == &redirect_uri) {
        Ok(())
    } else {
        Err(AppError::AuthError("Invalid redirect_uri".to_string()))
    }
}

fn validate_scope(client: &OAuthClient, scope: Option<&str>) -> Result<(), AppError> {
    if let Some(requested) = scope {
        let allowed: Vec<&str> = client.scopes.split_whitespace().collect();
        for s in requested.split_whitespace() {
            if !allowed.iter().any(|allowed_scope| allowed_scope == &s) {
                return Err(AppError::AuthError(format!("Scope '{}' not allowed", s)));
            }
        }
    }
    Ok(())
}

fn verify_pkce(verifier: &str, challenge: &str, method: Option<&str>) -> bool {
    match method {
        Some("S256") => {
            let digest = Sha256::digest(verifier.as_bytes());
            let hashed = base64ct::Base64UrlUnpadded::encode_string(&digest);
            hashed == challenge
        }
        _ => verifier == challenge,
    }
}

fn extract_client_credentials(
    headers: &HeaderMap,
    body: &TokenRequest,
) -> Result<(String, String), AppError> {
    if let Some(header_value) = headers.get(header::AUTHORIZATION) {
        if let Ok(value) = header_value.to_str() {
            if let Some(basic) = value.strip_prefix("Basic ") {
                let decoded = general_purpose::STANDARD
                    .decode(basic)
                    .map_err(|_| AppError::AuthError("Invalid basic auth".to_string()))?;
                let decoded_str = String::from_utf8(decoded)
                    .map_err(|_| AppError::AuthError("Invalid basic auth".to_string()))?;
                if let Some((id, secret)) = decoded_str.split_once(':') {
                    return Ok((id.to_string(), secret.to_string()));
                }
            }
        }
    }

    match (&body.client_id, &body.client_secret) {
        (Some(id), Some(secret)) => Ok((id.clone(), secret.clone())),
        _ => Err(AppError::AuthError("Missing client authentication".to_string())),
    }
}

fn extract_bearer(headers: &HeaderMap) -> Result<String, AppError> {
    let header_value = headers
        .get(header::AUTHORIZATION)
        .ok_or_else(|| AppError::AuthError("Missing authorization header".to_string()))?;
    let value = header_value
        .to_str()
        .map_err(|_| AppError::AuthError("Invalid authorization header".to_string()))?;
    if let Some(token) = value.strip_prefix("Bearer ") {
        Ok(token.to_string())
    } else {
        Err(AppError::AuthError("Invalid authorization header".to_string()))
    }
}

fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn decode_next(next: &str) -> Option<String> {
    let decoded = urlencoding::decode(next).ok()?.into_owned();
    // Evitar open redirects: solo rutas relativas
    if decoded.starts_with('/') {
        Some(decoded)
    } else {
        None
    }
}
