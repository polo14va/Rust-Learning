use axum::{
    extract::{ConnectInfo, OriginalUri, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    Form, Json,
};
use axum_extra::extract::CookieJar;
use base64::{engine::general_purpose, Engine as _};
use base64ct::Encoding;
use chrono::{Duration, Utc};
use redis::AsyncCommands;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{env, net::SocketAddr};
use urlencoding;
use uuid::Uuid;

const CONSENT_TTL_DAYS: u64 = 30;

use crate::{
    auth,
    db,
    error::AppError,
    templates,
    models::{
        AppState, AuthorizationCode, IntrospectionResponse, OAuthClient, OpenIdConfiguration,
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
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
}

#[derive(Deserialize)]
pub struct RevokeRequest {
    pub token: String,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
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
    pub mode: Option<String>,
}

pub async fn login_page(Query(params): Query<LoginPageParams>) -> Html<String> {
    let next_hidden = params
        .next
        .as_ref()
        .and_then(|n| sanitize_next(n))
        .map(|n| format!(r#"<input type="hidden" name="next" value="{}"/>"#, html_escape(&n)))
        .unwrap_or_default();
    let error_html = params
        .error
        .as_ref()
        .map(|e| format!(r#"<div class="error">{}</div>"#, html_escape(e)))
        .unwrap_or_default();
    let mode_attr = params
        .mode
        .as_deref()
        .unwrap_or("cookie");

    let page = templates::render_login_page(&next_hidden, &error_html, mode_attr);
    Html(page)
}

pub async fn options_ok() -> StatusCode {
    StatusCode::NO_CONTENT
}

pub async fn authorize(
    State(state): State<AppState>,
    headers: HeaderMap,
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
    ensure_state(&params.state)?;
    if pkce_required_for_client(&client.client_id, &client.client_secret) {
        ensure_pkce(params.code_challenge.as_ref(), params.code_challenge_method.as_ref())?;
    }

    // Validar sesión SSO
    let session = match jar.get("sso_session") {
        Some(cookie) => auth::validate_session_info(
            &state.redis_client,
            cookie.value(),
            user_agent_from_headers(&headers),
        )
        .await?,
        None => None,
    };

    let session = match session {
        Some(session) => session,
        None => {
            let uri_string = uri.to_string();
            let next = urlencoding::encode(uri_string.as_str());
            let login_url = format!("/login?next={}", next);
            return Ok(Redirect::temporary(&login_url).into_response());
        }
    };

    let scope = params.scope.clone().unwrap_or_else(|| client.scopes.clone());
    ensure_nonce(&scope, params.nonce.as_deref())?;
    if requires_strong_auth(&scope, session.created_at) {
        let uri_string = uri.to_string();
        let next = urlencoding::encode(uri_string.as_str());
        let login_url = format!("/login?next={}&error=Reauth%20required", next);
        return Ok(Redirect::temporary(&login_url).into_response());
    }

    if !has_consent(&state.redis_client, &session.username, &client.client_id, &scope).await? {
        let page = render_consent_page(&client, &params, &scope, &session.username);
        return Ok(Html(page).into_response());
    }

    let code = issue_authorization_code(
        &state,
        &params.client_id,
        &session.username,
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
    headers: HeaderMap,
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

    let session = match jar.get("sso_session") {
        Some(cookie) => auth::validate_session_info(
            &state.redis_client,
            cookie.value(),
            user_agent_from_headers(&headers),
        )
        .await?,
        None => None,
    };

    let session = match session {
        Some(session) => session,
        None => {
            let uri_string = uri.to_string();
            let next = urlencoding::encode(uri_string.as_str());
            let login_url = format!("/login?next={}", next);
            return Ok(Redirect::temporary(&login_url).into_response());
        }
    };

    let scope = params.scope.clone().unwrap_or_else(|| client.scopes.clone());
    ensure_nonce(&scope, params.nonce.as_deref())?;
    if requires_strong_auth(&scope, session.created_at) {
        let uri_string = uri.to_string();
        let next = urlencoding::encode(uri_string.as_str());
        let login_url = format!("/login?next={}&error=Reauth%20required", next);
        return Ok(Redirect::temporary(&login_url).into_response());
    }
    let page = render_consent_page(&client, &params, &scope, &session.username);
    Ok(Html(page).into_response())
}

pub async fn submit_consent(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Form(form): Form<ConsentForm>,
) -> Result<impl IntoResponse, AppError> {
    let client_ip = client_ip_from_headers(&headers, addr);
    let session = match jar.get("sso_session") {
        Some(cookie) => auth::validate_session_info(
            &state.redis_client,
            cookie.value(),
            user_agent_from_headers(&headers),
        )
        .await?,
        None => None,
    };

    let session = match session {
        Some(session) => session,
        None => {
            let login_url = "/login?next=/consent".to_string();
            return Ok(Redirect::to(&login_url));
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
    ensure_state(&form.state)?;
    ensure_nonce(&form.scope, form.nonce.as_deref())?;
    if pkce_required_for_client(&client.client_id, &client.client_secret) {
        ensure_pkce(form.code_challenge.as_ref(), form.code_challenge_method.as_ref())?;
    }

    if form.decision == "deny" {
        tracing::info!(
            target: "audit",
            event = "consent_denied",
            username = %session.username,
            client_id = %form.client_id,
            ip = %client_ip
        );
        let mut separator = if form.redirect_uri.contains('?') { "&" } else { "?" };
        let mut redirect_url = format!("{}{}error=access_denied", form.redirect_uri, separator);
        if let Some(state) = &form.state {
            separator = "&";
            redirect_url.push_str(&format!("{}state={}", separator, state));
        }
        return Ok(Redirect::to(&redirect_url));
    }

    if requires_strong_auth(&form.scope, session.created_at) {
        return Err(AppError::AuthError("Re-auth required".to_string()));
    }

    store_consent(&state.redis_client, &session.username, &form.client_id, &form.scope).await?;
    tracing::info!(
        target: "audit",
        event = "consent_granted",
        username = %session.username,
        client_id = %form.client_id,
        scope = %form.scope,
        ip = %client_ip
    );

    let code = issue_authorization_code(
        &state,
        &form.client_id,
        &session.username,
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

    Ok(Redirect::to(&redirect_url))
}

pub async fn token(
    State(state): State<AppState>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Form(body): Form<TokenRequest>,
) -> Result<Json<TokenResponse>, AppError> {
    let (client_id, client_secret) = extract_client_credentials(&headers, &body)?;
    let client = db::get_oauth_client(&state.pool, &client_id)
        .await?
        .ok_or_else(|| AppError::AuthError("Invalid client".to_string()))?;
    let client_ip = client_ip_from_headers(&headers, addr);
    let user_agent = user_agent_from_headers(&headers);

    if client.client_secret != client_secret {
        return Err(AppError::AuthError("Invalid client credentials".to_string()));
    }
    validate_grant_type(&client, &body.grant_type)?;

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

            if pkce_required_for_client(&client_id, &client_secret) {
                ensure_pkce(record.code_challenge.as_ref(), record.code_challenge_method.as_ref())?;
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
            if let Some(nonce_value) = record.nonce.as_deref() {
                consume_nonce(&state.redis_client, &client_id, nonce_value).await?;
            }

            let scope = record.scope.clone();
            let user = db::get_user_by_username(&state.pool, &record.username)
                .await?
                .ok_or_else(|| AppError::AuthError("User not found".to_string()))?;
            let role = Some(user.role.as_str());
            let access_token = auth::create_access_token(
                &record.username,
                &scope,
                &client_id,
                &state.issuer,
                &state.keys,
                role,
                None,
            )?;

            let refresh_token = auth::create_refresh_token();
            let session = auth::build_refresh_session(
                &record.username,
                &client_id,
                &scope,
                None,
                Some(user_agent),
            );
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

            tracing::info!(
                target: "audit",
                event = "token_issued",
                grant_type = "authorization_code",
                client_id = %client_id,
                username = %record.username,
                ip = %client_ip
            );
            Ok(Json(response))
        }
        "refresh_token" => {
            let refresh_token = body
                .refresh_token
                .clone()
                .ok_or_else(|| AppError::AuthError("Missing refresh_token".to_string()))?;

            let session = auth::validate_refresh_token(
                &state.redis_client,
                &refresh_token,
                Some(user_agent),
                None,
            )
            .await?;
            let session = match session {
                Some(session) => session,
                None => {
                    tracing::warn!(
                        target: "audit",
                        event = "token_refresh_failure",
                        reason = "invalid_or_expired",
                        client_id = %client_id,
                        ip = %client_ip
                    );
                    return Err(AppError::AuthError("Invalid or expired refresh token".to_string()));
                }
            };

            if session.client_id != client_id {
                return Err(AppError::AuthError("Client mismatch".to_string()));
            }

            if let Some(record) = db::get_refresh_token_record(&state.pool, &refresh_token).await? {
                if record.revoked || record.expires_at < Utc::now() {
                    return Err(AppError::AuthError("Refresh token expired or revoked".to_string()));
                }
            }

            let scope = session.scope.clone();
            let user = db::get_user_by_username(&state.pool, &session.username)
                .await?
                .ok_or_else(|| AppError::AuthError("User not found".to_string()))?;
            let role = Some(user.role.as_str());
            let access_token = auth::create_access_token(
                &session.username,
                &scope,
                &client_id,
                &state.issuer,
                &state.keys,
                role,
                None,
            )?;

            let new_refresh_token = auth::create_refresh_token();
            let new_session = auth::RefreshSession {
                username: session.username.clone(),
                client_id: client_id.clone(),
                scope: scope.clone(),
                session_id: session.session_id.clone(),
                ua_hash: session.ua_hash.clone(),
            };
            auth::store_refresh_token(&state.redis_client, &new_session, &new_refresh_token).await?;

            let expires_at = Utc::now() + Duration::days(7);
            let refresh_record = RefreshTokenRecord {
                refresh_token: new_refresh_token.clone(),
                client_id: client_id.clone(),
                username: session.username.clone(),
                scope: scope.clone(),
                expires_at,
                revoked: false,
            };
            db::store_refresh_token_record(&state.pool, &refresh_record).await?;
            auth::revoke_refresh_token(&state.redis_client, &refresh_token).await?;
            db::revoke_refresh_token_record(&state.pool, &refresh_token).await?;

            let response = TokenResponse {
                access_token,
                token_type: "Bearer".to_string(),
                expires_in: 60 * 15,
                refresh_token: Some(new_refresh_token),
                id_token: None,
                scope: Some(scope),
            };
            tracing::info!(
                target: "audit",
                event = "token_issued",
                grant_type = "refresh_token",
                client_id = %client_id,
                username = %session.username,
                ip = %client_ip
            );
            Ok(Json(response))
        }
        "client_credentials" => {
            let scope = effective_client_scopes(&client, body.scope.as_deref())?;
            let role = client_role(&client.client_id);
            let access_token = auth::create_access_token(
                &client_id,
                &scope,
                &client_id,
                &state.issuer,
                &state.keys,
                Some(role.as_str()),
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
            tracing::info!(
                target: "audit",
                event = "token_issued",
                grant_type = "client_credentials",
                client_id = %client_id,
                ip = %client_ip
            );
            Ok(Json(response))
        }
        _ => Err(AppError::AuthError("Unsupported grant_type".to_string())),
    }
}

pub async fn introspect(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(body): Form<IntrospectRequest>,
) -> Result<Json<IntrospectionResponse>, AppError> {
    let (client_id, client_secret) = extract_client_credentials_from(
        &headers,
        body.client_id.as_ref(),
        body.client_secret.as_ref(),
    )?;
    let client = db::get_oauth_client(&state.pool, &client_id)
        .await?
        .ok_or_else(|| AppError::AuthError("Invalid client".to_string()))?;

    if client.client_secret != client_secret {
        return Err(AppError::AuthError("Invalid client credentials".to_string()));
    }
    require_client_scope(&client, "INTROSPECT_SCOPE", "introspect")?;
    require_client_role(&client.client_id, "INTROSPECT_ROLES", "service,admin")?;

    match auth::validate_jwt(&body.token, &state.keys, &state.issuer, Some(&client_id)) {
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
    headers: HeaderMap,
    Form(body): Form<RevokeRequest>,
) -> Result<impl IntoResponse, AppError> {
    let (client_id, client_secret) = extract_client_credentials_from(
        &headers,
        body.client_id.as_ref(),
        body.client_secret.as_ref(),
    )?;
    let client = db::get_oauth_client(&state.pool, &client_id)
        .await?
        .ok_or_else(|| AppError::AuthError("Invalid client".to_string()))?;

    if client.client_secret != client_secret {
        return Err(AppError::AuthError("Invalid client credentials".to_string()));
    }

    require_client_scope(&client, "REVOKE_SCOPE", "revoke")?;
    require_client_role(&client.client_id, "REVOKE_ROLES", "service,admin")?;

    if let Some(record) = db::get_refresh_token_record(&state.pool, &body.token).await? {
        if record.client_id == client_id {
            auth::revoke_refresh_token(&state.redis_client, &body.token).await?;
            db::revoke_refresh_token_record(&state.pool, &body.token).await?;
        }
    }
    Ok(StatusCode::OK)
}

pub async fn userinfo(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<UserInfoResponse>, AppError> {
    let token = extract_bearer(&headers)?;
    let data = auth::validate_jwt(&token, &state.keys, &state.issuer, None)
        .map_err(|_| AppError::AuthError("Invalid token".to_string()))?;
    let aud = data.claims.aud.clone().unwrap_or_default();
    if aud != "first-party" && !db::oauth_client_exists(&state.pool, &aud).await? {
        return Err(AppError::AuthError("Invalid audience".to_string()));
    }
    let scope = data.claims.scope.as_deref().unwrap_or("");
    if !scope.split_whitespace().any(|s| s == "openid") {
        return Err(AppError::AuthError("Missing openid scope".to_string()));
    }
    let username = data.claims.sub;

    let user = db::get_user_by_username(&state.pool, &username)
        .await?
        .ok_or_else(|| AppError::AuthError("User not found".to_string()))?;
    let email = if scope.split_whitespace().any(|s| s == "email") {
        Some(user.email)
    } else {
        None
    };
    let role = if scope.split_whitespace().any(|s| s == "roles") {
        Some(user.role)
    } else {
        None
    };

    Ok(Json(UserInfoResponse {
        sub: username.clone(),
        preferred_username: username,
        email,
        role,
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
    Json(serde_json::json!({ "keys": state.keys.jwks.clone() }))
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
    if let Some(nonce_value) = nonce.as_deref() {
        store_nonce(&state.redis_client, client_id, nonce_value).await?;
    }

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
    let trimmed = redirect_uri.trim();
    if trimmed.contains('#') {
        return Err(AppError::AuthError("Invalid redirect_uri".to_string()));
    }
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return Err(AppError::AuthError("Invalid redirect_uri".to_string()));
    }

    let allowed: Vec<&str> = client.redirect_uris.split(',').map(|s| s.trim()).collect();
    if allowed.iter().any(|uri| uri == &trimmed) {
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

fn effective_client_scopes(client: &OAuthClient, requested: Option<&str>) -> Result<String, AppError> {
    let allowed_client: Vec<&str> = client.scopes.split_whitespace().collect();
    let policy_scopes = client_scope_policy(&client.client_id);
    let allowed_policy: Vec<&str> = match policy_scopes.as_deref() {
        Some(value) => value.split_whitespace().collect(),
        None => Vec::new(),
    };
    let allowed: Vec<&str> = if allowed_policy.is_empty() {
        allowed_client
    } else {
        allowed_client
            .into_iter()
            .filter(|s| allowed_policy.iter().any(|p| p == s))
            .collect()
    };

    let requested = requested
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| client.scopes.as_str());
    for scope in requested.split_whitespace() {
        if !allowed.iter().any(|allowed_scope| allowed_scope == &scope) {
            return Err(AppError::AuthError(format!("Scope '{}' not allowed", scope)));
        }
    }
    Ok(requested.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn client_scope_policy(client_id: &str) -> Option<String> {
    let map = env::var("CLIENT_SCOPE_MAP").unwrap_or_default();
    for entry in map.split(';').map(|v| v.trim()).filter(|v| !v.is_empty()) {
        let (id, scopes) = entry.split_once('=')?;
        if id.trim() == client_id {
            return Some(scopes.trim().to_string());
        }
    }
    None
}

fn client_role(client_id: &str) -> String {
    let map = env::var("CLIENT_ROLE_MAP").unwrap_or_default();
    for entry in map.split(';').map(|v| v.trim()).filter(|v| !v.is_empty()) {
        let (id, role) = match entry.split_once('=') {
            Some(parts) => parts,
            None => continue,
        };
        if id.trim() == client_id {
            return role.trim().to_string();
        }
    }
    "service".to_string()
}

fn require_client_scope(
    client: &OAuthClient,
    env_key: &str,
    default_scope: &str,
) -> Result<(), AppError> {
    let required = env::var(env_key).unwrap_or_else(|_| default_scope.to_string());
    let required: Vec<&str> = required.split_whitespace().collect();
    if required.is_empty() {
        return Ok(());
    }
    let allowed: Vec<&str> = client.scopes.split_whitespace().collect();
    for scope in required {
        if !allowed.iter().any(|s| s == &scope) {
            return Err(AppError::AuthError("Client lacks required scope".to_string()));
        }
    }
    Ok(())
}

fn require_client_role(client_id: &str, env_key: &str, default_roles: &str) -> Result<(), AppError> {
    let required = env::var(env_key).unwrap_or_else(|_| default_roles.to_string());
    let required: Vec<&str> = required
        .split(',')
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .collect();
    if required.is_empty() {
        return Ok(());
    }
    let role = client_role(client_id);
    if required.iter().any(|r| r == &role) {
        Ok(())
    } else {
        Err(AppError::AuthError("Client role not allowed".to_string()))
    }
}

fn validate_grant_type(client: &OAuthClient, grant_type: &str) -> Result<(), AppError> {
    let allowed: Vec<&str> = client.grant_types.split(',').map(|s| s.trim()).collect();
    if allowed.iter().any(|gt| gt == &grant_type) {
        Ok(())
    } else {
        Err(AppError::AuthError("Unsupported grant_type".to_string()))
    }
}

fn ensure_state(state: &Option<String>) -> Result<(), AppError> {
    if state.is_some() {
        Ok(())
    } else {
        Err(AppError::AuthError("Missing state".to_string()))
    }
}

fn ensure_nonce(scope: &str, nonce: Option<&str>) -> Result<(), AppError> {
    if scope.split_whitespace().any(|s| s == "openid") && nonce.is_none() {
        return Err(AppError::AuthError("Missing nonce".to_string()));
    }
    Ok(())
}

fn ensure_pkce(code_challenge: Option<&String>, method: Option<&String>) -> Result<(), AppError> {
    let required = env::var("REQUIRE_PKCE")
        .map(|v| v == "true")
        .unwrap_or(true);
    if !required {
        return Ok(());
    }
    let challenge = code_challenge
        .as_ref()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| AppError::AuthError("Missing PKCE code_challenge".to_string()))?;
    let method = method
        .as_ref()
        .map(|v| v.trim())
        .unwrap_or("S256");
    if method != "S256" {
        return Err(AppError::AuthError("Unsupported PKCE method".to_string()));
    }
    if challenge.len() < 43 {
        return Err(AppError::AuthError("Invalid PKCE code_challenge".to_string()));
    }
    Ok(())
}

fn pkce_required_for_client(client_id: &str, client_secret: &str) -> bool {
    let public_clients = env::var("PUBLIC_CLIENTS").unwrap_or_default();
    let is_public = client_secret.trim().is_empty()
        || public_clients
            .split(',')
            .map(|v| v.trim())
            .any(|v| v == client_id);
    let require_all = env::var("REQUIRE_PKCE")
        .map(|v| v == "true")
        .unwrap_or(true);
    require_all || is_public
}

fn requires_strong_auth(scope: &str, session_created_at: i64) -> bool {
    let sensitive = env::var("SENSITIVE_SCOPES").unwrap_or_default();
    let sensitive_scopes: Vec<&str> = sensitive
        .split_whitespace()
        .filter(|v| !v.is_empty())
        .collect();
    if sensitive_scopes.is_empty() {
        return false;
    }

    let requested: Vec<&str> = scope.split_whitespace().collect();
    if !requested.iter().any(|s| sensitive_scopes.iter().any(|v| v == s)) {
        return false;
    }

    let max_age = env::var("SENSITIVE_SESSION_MAX_AGE_SECONDS")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(300);
    let age = Utc::now().timestamp().saturating_sub(session_created_at);
    age > max_age
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

fn nonce_ttl_seconds() -> usize {
    env::var("NONCE_TTL_SECONDS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(600)
}

fn nonce_cache_key(client_id: &str, nonce: &str) -> String {
    let digest = Sha256::digest(nonce.as_bytes());
    let hash = base64ct::Base64UrlUnpadded::encode_string(&digest);
    format!("oidc:nonce:{}:{}", client_id, hash)
}

async fn store_nonce(
    redis_client: &redis::Client,
    client_id: &str,
    nonce: &str,
) -> Result<(), AppError> {
    let mut conn = redis_client
        .get_async_connection()
        .await
        .map_err(|e| AppError::InternalError(format!("Redis connection error: {}", e)))?;
    let key = nonce_cache_key(client_id, nonce);
    let created: bool = conn
        .set_nx(&key, "1")
        .await
        .map_err(|e| AppError::InternalError(format!("Redis SETNX error: {}", e)))?;
    if !created {
        return Err(AppError::AuthError("Nonce already used".to_string()));
    }
    let _: () = conn
        .expire(&key, nonce_ttl_seconds() as i64)
        .await
        .map_err(|e| AppError::InternalError(format!("Redis EXPIRE error: {}", e)))?;
    Ok(())
}

async fn consume_nonce(
    redis_client: &redis::Client,
    client_id: &str,
    nonce: &str,
) -> Result<(), AppError> {
    let mut conn = redis_client
        .get_async_connection()
        .await
        .map_err(|e| AppError::InternalError(format!("Redis connection error: {}", e)))?;
    let key = nonce_cache_key(client_id, nonce);
    let removed: i32 = conn
        .del(&key)
        .await
        .map_err(|e| AppError::InternalError(format!("Redis DEL error: {}", e)))?;
    if removed == 0 {
        return Err(AppError::AuthError("Nonce expired or already used".to_string()));
    }
    Ok(())
}

fn user_agent_from_headers(headers: &HeaderMap) -> &str {
    headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("unknown")
}

fn client_ip_from_headers(headers: &HeaderMap, remote: SocketAddr) -> String {
    if let Some(value) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(first) = value.split(',').next() {
            return first.trim().to_string();
        }
    }
    if let Some(value) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
        return value.to_string();
    }
    remote.ip().to_string()
}

fn extract_client_credentials(
    headers: &HeaderMap,
    body: &TokenRequest,
) -> Result<(String, String), AppError> {
    extract_client_credentials_from(headers, body.client_id.as_ref(), body.client_secret.as_ref())
}

fn extract_client_credentials_from(
    headers: &HeaderMap,
    client_id: Option<&String>,
    client_secret: Option<&String>,
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

    match (client_id, client_secret) {
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

fn sanitize_next(next: &str) -> Option<String> {
    let value = next.trim();
    if value.starts_with('/') && !value.starts_with("//") {
        Some(value.to_string())
    } else {
        None
    }
}
