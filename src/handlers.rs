use axum::{
    extract::{ConnectInfo, State},
    http::{header, HeaderMap},
    Json,
};
use tokio::time::Instant;
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use chrono::{Duration, Utc};
use std::{env, net::SocketAddr};
use time::Duration as CookieDuration;
use redis::AsyncCommands;
use crate::{
    models::{User, DashboardData, AppState, LoginRequest, LoginResponse, RefreshRequest, RefreshTokenRecord},
    db, error::AppError, cache, auth, rate_limit, email,
    builders::UserRegistration,  // TYPE-STATE BUILDER
};

pub async fn list_users(State(state): State<AppState>) -> Result<Json<Vec<User>>, AppError> {
    // Accedemos al pool a través del state
    let users = db::get_all_users(&state.pool).await?;
    Ok(Json(users))
}

pub async fn get_dashboard(State(state): State<AppState>) -> Result<Json<DashboardData>, AppError> {
    // 1. INTENTAR LEER DE REDIS (Cache Distribuido)
    if let Some(data) = cache::get_dashboard_data(&state.redis_client).await? {
        println!("REDIS CACHE HIT!");
        return Ok(Json(data));
    }

    println!("REDIS CACHE MISS! Consultando base de datos...");

    // 2. CONSULTAR DATOS REALES
    let start_join = Instant::now();
    let (stats_result, activities_result, alerts_result) = tokio::join!(
        db::get_stats(&state.pool),
        db::get_activities(&state.pool),
        db::get_alerts(&state.pool)
    );
    println!("CONCURRENCIA: Queries tardaron {:?}", start_join.elapsed());

    let data = DashboardData {
        stats: stats_result?,
        activities: activities_result?,
        alerts: alerts_result?,
    };

    // 3. ACTUALIZAR REDIS
    // No bloqueamos la respuesta esperando a que se guarde en caché (fire and forget idealmente, 
    // pero aquí lo haremos await para simplicidad y asegurar que se guardó).
    cache::set_dashboard_data(&state.redis_client, &data).await?;
    println!("Datos guardados en Redis (TTL 60s)");

    Ok(Json(data))
}

pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(payload): Json<LoginRequest>,
) -> Result<(CookieJar, Json<LoginResponse>), AppError> {
    let client_ip = client_ip_from_headers(&headers, addr);
    if is_login_locked(&state.redis_client, &payload.username, &client_ip).await? {
        tracing::warn!(target: "audit", event = "login_locked", username = %payload.username, ip = %client_ip);
        return Err(AppError::AuthError("Cuenta bloqueada temporalmente".to_string()));
    }

    // Rate limiting por username + IP
    let rate_key = format!("rate_limit:login:{}", payload.username);
    if !rate_limit::check_rate_limit(&state.redis_client, &rate_key).await? {
        return Err(AppError::AuthError("Too many requests. Try again later.".to_string()));
    }
    let ip_rate_key = format!("rate_limit:login_ip:{}", client_ip);
    if !rate_limit::check_rate_limit(&state.redis_client, &ip_rate_key).await? {
        return Err(AppError::AuthError("Too many requests. Try again later.".to_string()));
    }

    // 1. Buscar usuario
    let user = db::get_user_by_username(&state.pool, &payload.username).await?;

    if let Some(user) = user {
        // 2. Verificar password
        if auth::verify_password(&payload.password, &user.password_hash)? {
            clear_login_failures(&state.redis_client, &payload.username, &client_ip).await?;

            // 3. Generar tokens
            let scope = default_user_scope();
            let access_token = auth::create_access_token(
                &user.username,
                &scope,
                "first-party",
                &state.issuer,
                &state.keys,
                Some(&user.role),
                None,
            )?;
            let refresh_token = auth::create_refresh_token();

            // Persistimos en base de datos para auditoría
            let expires_at = Utc::now() + Duration::days(7);
            let record = RefreshTokenRecord {
                refresh_token: refresh_token.clone(),
                client_id: "first-party".to_string(),
                username: user.username.clone(),
                scope: scope.to_string(),
                expires_at,
                revoked: false,
            };
            db::store_refresh_token_record(&state.pool, &record).await?;

            let user_agent = user_agent_from_headers(&headers);
            let session_id = auth::create_session(
                &state.redis_client,
                &user.username,
                &user_agent,
            )
            .await?;

            // 4. Guardar refresh token en Redis (atado a sesión + UA)
            let session = auth::build_refresh_session(
                &user.username,
                "first-party",
                &scope,
                Some(session_id.clone()),
                Some(&user_agent),
            );
            auth::store_refresh_token(&state.redis_client, &session, &refresh_token).await?;

            tracing::info!(
                target: "audit",
                event = "login_success",
                username = %user.username,
                client_id = "first-party",
                ip = %client_ip
            );
            let cookie = Cookie::build(("sso_session", session_id))
                .http_only(true)
                .secure(cookie_secure())
                .same_site(cookie_same_site())
                .path("/")
                .max_age(CookieDuration::minutes(60))
                .build();
            let updated_jar = jar.add(cookie);
            
            return Ok((updated_jar, Json(LoginResponse { access_token, refresh_token })));
        }
        tracing::debug!(
            target: "audit",
            event = "login_password_mismatch",
            username = %payload.username,
            ip = %client_ip
        );
    } else {
        tracing::debug!(
            target: "audit",
            event = "login_user_not_found",
            username = %payload.username,
            ip = %client_ip
        );
    }

    record_login_failure(&state.redis_client, &payload.username, &client_ip).await?;
    tracing::warn!(
        target: "audit",
        event = "login_failure",
        username = %payload.username,
        ip = %client_ip
    );
    Err(AppError::AuthError("Credenciales inválidas".to_string()))
}

// Endpoint temporal para crear usuarios (SOLO PARA DESARROLLO)
// ============================================================================
// HANDLER: Register (usando TYPE-STATE PATTERN)
// ============================================================================
//
// ANTES (sin Type-State):
//   let hash = auth::hash_password(&payload.password)?;
//   sqlx::query(...).bind(&payload.username).bind(&hash).execute(...)
//
//   PROBLEMA: Podríamos olvidar validar username o password
//
// AHORA (con Type-State):
//   El compilador OBLIGA a configurar username + password antes de .build()
//   Si olvidas alguno, el código NO COMPILA
//
// ============================================================================
pub async fn register(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(payload): Json<LoginRequest>,
) -> Result<(CookieJar, Json<LoginResponse>), AppError> {
    if !registration_enabled() {
        return Err(AppError::ValidationError("Registration disabled".to_string()));
    }

    // Rate limiting por username + IP
    let rate_key = format!("rate_limit:register:{}", payload.username);
    if !rate_limit::check_rate_limit(&state.redis_client, &rate_key).await? {
        return Err(AppError::AuthError("Too many requests. Try again later.".to_string()));
    }
    let ip_rate_key = format!("rate_limit:register_ip:{}", client_ip_from_headers(&headers, addr));
    if !rate_limit::check_rate_limit(&state.redis_client, &ip_rate_key).await? {
        return Err(AppError::AuthError("Too many requests. Try again later.".to_string()));
    }

    // ========================================================================
    // TYPE-STATE PATTERN EN ACCIÓN
    // ========================================================================
    // Este builder GARANTIZA que username y password están configurados
    // Si intentas hacer .build() sin .username() o .password(), NO COMPILA
    
    let (username, password, email) = UserRegistration::new()
        .username(&payload.username)  // NoUsername -> NoPassword
        .password(&payload.password)  // NoPassword -> Ready
        .email(format!("{}@test.com", &payload.username))  // Opcional
        .build();  // Solo Ready tiene .build()

    validate_password_policy(&password)?;
    
    // Ahora username y password están GARANTIZADOS por el compilador
    // No necesitamos Option::unwrap() ni validaciones runtime
    
    let hash = auth::hash_password(&password)?;
    
    sqlx::query("INSERT INTO users (username, email, password_hash) VALUES ($1, $2, $3)")
        .bind(&username)
        .bind(&email)
        .bind(&hash)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            if let Some(db_err) = e.as_database_error() {
                // 23505 = unique_violation
                if db_err.code() == Some(std::borrow::Cow::Borrowed("23505")) {
                    tracing::warn!(target: "register", "Username already exists: {}", username);
                    return AppError::ValidationError("Username already exists".to_string());
                }
            }
            tracing::error!(target: "register", "Failed to insert user: {}", e);
            AppError::DatabaseError(e)
        })?;
    
    let scope = default_user_scope();
    let token = auth::create_access_token(
        &username,
        &scope,
        "first-party",
        &state.issuer,
        &state.keys,
        Some("user"),
        None,
    )?;
    let refresh_token = auth::create_refresh_token();
    let expires_at = Utc::now() + Duration::days(7);
    let record = RefreshTokenRecord {
        refresh_token: refresh_token.clone(),
        client_id: "first-party".to_string(),
        username: username.clone(),
        scope: scope.to_string(),
        expires_at,
        revoked: false,
    };
    db::store_refresh_token_record(&state.pool, &record).await?;

    let user_agent = user_agent_from_headers(&headers);
    let session_id = auth::create_session(
        &state.redis_client,
        &username,
        &user_agent,
    )
    .await?;
    let session = auth::build_refresh_session(
        &username,
        "first-party",
        &scope,
        Some(session_id.clone()),
        Some(&user_agent),
    );
    auth::store_refresh_token(&state.redis_client, &session, &refresh_token).await?;

    let cookie = Cookie::build(("sso_session", session_id))
        .http_only(true)
        .secure(cookie_secure())
        .same_site(cookie_same_site())
        .path("/")
        .max_age(CookieDuration::minutes(60))
        .build();
    let updated_jar = jar.add(cookie);

    tracing::info!(
        target: "audit",
        event = "register_success",
        username = %username,
        client_id = "first-party",
        ip = %client_ip_from_headers(&headers, addr)
    );
    
    // Email simulado (bienvenida/alta)
    let email_body = format!(
        "Hola {username},\n\nTu cuenta se ha creado correctamente y ya puedes usar SSO/OIDC.\n\nScopes por defecto: {scope}\n\n-- Equipo de autenticación"
    );
    let _ = email::send_email(&email.unwrap_or_else(|| "user@example.com".to_string()), "Bienvenido a SSO", &email_body).await;
    
    Ok((updated_jar, Json(LoginResponse { access_token: token, refresh_token })))
}

pub async fn refresh(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(payload): Json<RefreshRequest>,
) -> Result<Json<LoginResponse>, AppError> {
    // Rate limiting por IP
    let ip_rate_key = format!("rate_limit:refresh_ip:{}", client_ip_from_headers(&headers, addr));
    if !rate_limit::check_rate_limit(&state.redis_client, &ip_rate_key).await? {
        return Err(AppError::AuthError("Too many requests. Try again later.".to_string()));
    }
    let user_agent = user_agent_from_headers(&headers);
    let session_cookie = jar.get("sso_session").map(|cookie| cookie.value().to_string());
    let session = auth::validate_refresh_token(
        &state.redis_client,
        &payload.refresh_token,
        Some(&user_agent),
        session_cookie.as_deref(),
    )
        .await?;
    let session = match session {
        Some(session) => session,
        None => {
            tracing::warn!(
                target: "audit",
                event = "refresh_failure",
                reason = "invalid_or_expired",
                ip = %client_ip_from_headers(&headers, addr)
            );
            return Err(AppError::AuthError("Invalid or expired refresh token".to_string()));
        }
    };

    if let Some(record) = db::get_refresh_token_record(&state.pool, &payload.refresh_token).await? {
        if record.revoked || record.expires_at < Utc::now() {
            return Err(AppError::AuthError("Refresh token expired or revoked".to_string()));
        }
    }

    // Generar nuevo access token
    let user = db::get_user_by_username(&state.pool, &session.username)
        .await?
        .ok_or_else(|| AppError::AuthError("User not found".to_string()))?;
    let role = Some(user.role.as_str());
    let access_token = auth::create_access_token(
        &session.username,
        &session.scope,
        &session.client_id,
        &state.issuer,
        &state.keys,
        role,
        None,
    )?;

    // Rotar refresh token
    let new_refresh_token = auth::create_refresh_token();
    auth::store_refresh_token(&state.redis_client, &session, &new_refresh_token).await?;

    let expires_at = Utc::now() + Duration::days(7);
    let record = RefreshTokenRecord {
        refresh_token: new_refresh_token.clone(),
        client_id: session.client_id.clone(),
        username: session.username.clone(),
        scope: session.scope.clone(),
        expires_at,
        revoked: false,
    };
    db::store_refresh_token_record(&state.pool, &record).await?;
    auth::revoke_refresh_token(&state.redis_client, &payload.refresh_token).await?;
    db::revoke_refresh_token_record(&state.pool, &payload.refresh_token).await?;
    tracing::info!(
        target: "audit",
        event = "refresh_success",
        username = %session.username,
        client_id = %session.client_id,
        ip = %client_ip_from_headers(&headers, addr)
    );

    Ok(Json(LoginResponse {
        access_token,
        refresh_token: new_refresh_token,
    }))
}

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Json(payload): Json<RefreshRequest>,
) -> Result<(CookieJar, Json<serde_json::Value>), AppError> {
    let token = extract_bearer(&headers)?;
    let data = auth::validate_jwt(&token, &state.keys, &state.issuer, None)
        .map_err(|_| AppError::AuthError("Invalid token".to_string()))?;

    let aud = data.claims.aud.as_deref();
    if !audience_allowed(aud) {
        let client_ok = match aud {
            Some(client_id) => db::oauth_client_exists(&state.pool, client_id).await?,
            None => false,
        };
        if !client_ok {
            return Err(AppError::AuthError("Invalid audience".to_string()));
        }
    }

    let record = db::get_refresh_token_record(&state.pool, &payload.refresh_token).await?;
    let record = record.ok_or_else(|| AppError::AuthError("Invalid refresh token".to_string()))?;
    if record.username != data.claims.sub {
        return Err(AppError::AuthError("Refresh token mismatch".to_string()));
    }
    if record.client_id != data.claims.aud.clone().unwrap_or_default() {
        return Err(AppError::AuthError("Client mismatch".to_string()));
    }

    // Revocar refresh token
    auth::revoke_refresh_token(&state.redis_client, &payload.refresh_token).await?;
    db::revoke_refresh_token_record(&state.pool, &payload.refresh_token).await?;

    // Revocar sesión SSO si existe cookie
    let mut updated_jar = jar;
    if let Some(cookie) = updated_jar.get("sso_session") {
        let session_id = cookie.value().to_string();
        auth::revoke_session(&state.redis_client, &session_id).await?;
        updated_jar = updated_jar.remove(Cookie::from("sso_session"));
    }
    
    Ok((updated_jar, Json(serde_json::json!({ "message": "Logged out successfully" }))))
}

pub async fn logout_all(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
) -> Result<(CookieJar, Json<serde_json::Value>), AppError> {
    let token = extract_bearer(&headers)?;
    let data = auth::validate_jwt(&token, &state.keys, &state.issuer, None)
        .map_err(|_| AppError::AuthError("Invalid token".to_string()))?;

    let aud = data.claims.aud.as_deref();
    if !audience_allowed(aud) {
        let client_ok = match aud {
            Some(client_id) => db::oauth_client_exists(&state.pool, client_id).await?,
            None => false,
        };
        if !client_ok {
            return Err(AppError::AuthError("Invalid audience".to_string()));
        }
    }

    let username = data.claims.sub;
    auth::revoke_all_refresh_tokens(&state.redis_client, &username).await?;
    db::revoke_all_refresh_tokens_for_user(&state.pool, &username).await?;
    auth::revoke_all_sessions(&state.redis_client, &username).await?;

    let mut updated_jar = jar;
    if let Some(cookie) = updated_jar.get("sso_session") {
        let session_id = cookie.value().to_string();
        let _ = auth::revoke_session(&state.redis_client, &session_id).await;
        updated_jar = updated_jar.remove(Cookie::from("sso_session"));
    }

    Ok((updated_jar, Json(serde_json::json!({ "message": "Logged out from all sessions" }))))
}

fn default_user_scope() -> String {
    env::var("DEFAULT_USER_SCOPE")
        .unwrap_or_else(|_| "openid profile email offline_access dashboard.read".to_string())
}

fn registration_enabled() -> bool {
    env::var("ENABLE_REGISTRATION")
        .map(|v| v == "true")
        .unwrap_or(false)
}

fn cookie_secure() -> bool {
    let mut secure = env::var("COOKIE_SECURE")
        .map(|v| v == "true")
        .unwrap_or(true);
    if matches!(cookie_same_site(), SameSite::None) {
        secure = true;
    }
    secure
}

fn cookie_same_site() -> SameSite {
    match env::var("COOKIE_SAMESITE")
        .unwrap_or_else(|_| "Strict".to_string())
        .to_lowercase()
        .as_str()
    {
        "none" => SameSite::None,
        "lax" => SameSite::Lax,
        _ => SameSite::Strict,
    }
}

fn password_min_length() -> usize {
    env::var("PASSWORD_MIN_LENGTH")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(10)
}

fn password_requires_upper() -> bool {
    env::var("PASSWORD_REQUIRE_UPPER")
        .map(|v| v == "true")
        .unwrap_or(true)
}

fn password_requires_lower() -> bool {
    env::var("PASSWORD_REQUIRE_LOWER")
        .map(|v| v == "true")
        .unwrap_or(true)
}

fn password_requires_digit() -> bool {
    env::var("PASSWORD_REQUIRE_DIGIT")
        .map(|v| v == "true")
        .unwrap_or(true)
}

fn password_requires_special() -> bool {
    env::var("PASSWORD_REQUIRE_SPECIAL")
        .map(|v| v == "true")
        .unwrap_or(true)
}

fn validate_password_policy(password: &str) -> Result<(), AppError> {
    if password.len() < password_min_length() {
        return Err(AppError::ValidationError("Password too short".to_string()));
    }
    if password_requires_upper() && !password.chars().any(|c| c.is_ascii_uppercase()) {
        return Err(AppError::ValidationError("Password must include uppercase".to_string()));
    }
    if password_requires_lower() && !password.chars().any(|c| c.is_ascii_lowercase()) {
        return Err(AppError::ValidationError("Password must include lowercase".to_string()));
    }
    if password_requires_digit() && !password.chars().any(|c| c.is_ascii_digit()) {
        return Err(AppError::ValidationError("Password must include a number".to_string()));
    }
    if password_requires_special() && !password.chars().any(|c| !c.is_ascii_alphanumeric()) {
        return Err(AppError::ValidationError("Password must include a special character".to_string()));
    }
    Ok(())
}

fn login_max_failed_attempts() -> u32 {
    env::var("LOGIN_MAX_FAILED_ATTEMPTS")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(5)
}

fn login_lockout_minutes() -> i64 {
    env::var("LOGIN_LOCKOUT_MINUTES")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(10)
}

async fn is_login_locked(
    redis_client: &redis::Client,
    username: &str,
    ip: &str,
) -> Result<bool, AppError> {
    let mut conn = redis_client
        .get_async_connection()
        .await
        .map_err(|e| AppError::InternalError(format!("Redis connection error: {}", e)))?;

    let user_key = format!("auth_lockout:user:{}", username);
    let user_locked: bool = conn
        .exists(&user_key)
        .await
        .map_err(|e| AppError::InternalError(format!("Redis EXISTS error: {}", e)))?;
    if user_locked {
        return Ok(true);
    }

    if ip != "unknown" {
        let ip_key = format!("auth_lockout:ip:{}", ip);
        let ip_locked: bool = conn
            .exists(&ip_key)
            .await
            .map_err(|e| AppError::InternalError(format!("Redis EXISTS error: {}", e)))?;
        if ip_locked {
            return Ok(true);
        }
    }

    Ok(false)
}

async fn record_login_failure(
    redis_client: &redis::Client,
    username: &str,
    ip: &str,
) -> Result<(), AppError> {
    let max_attempts = login_max_failed_attempts();
    let ttl_seconds = login_lockout_minutes() * 60;

    let mut conn = redis_client
        .get_async_connection()
        .await
        .map_err(|e| AppError::InternalError(format!("Redis connection error: {}", e)))?;

    let user_fail_key = format!("auth_fail:user:{}", username);
    let user_count: u32 = conn
        .incr(&user_fail_key, 1)
        .await
        .map_err(|e| AppError::InternalError(format!("Redis INCR error: {}", e)))?;
    if user_count == 1 {
        let _: () = conn
            .expire(&user_fail_key, ttl_seconds)
            .await
            .map_err(|e| AppError::InternalError(format!("Redis EXPIRE error: {}", e)))?;
    }
    if user_count >= max_attempts {
        let lock_key = format!("auth_lockout:user:{}", username);
        let _: () = conn
            .set_ex(&lock_key, "1", ttl_seconds as u64)
            .await
            .map_err(|e| AppError::InternalError(format!("Redis SET error: {}", e)))?;
    }

    if ip != "unknown" {
        let ip_fail_key = format!("auth_fail:ip:{}", ip);
        let ip_count: u32 = conn
            .incr(&ip_fail_key, 1)
            .await
            .map_err(|e| AppError::InternalError(format!("Redis INCR error: {}", e)))?;
        if ip_count == 1 {
            let _: () = conn
                .expire(&ip_fail_key, ttl_seconds)
                .await
                .map_err(|e| AppError::InternalError(format!("Redis EXPIRE error: {}", e)))?;
        }
        if ip_count >= max_attempts {
            let lock_key = format!("auth_lockout:ip:{}", ip);
            let _: () = conn
                .set_ex(&lock_key, "1", ttl_seconds as u64)
                .await
                .map_err(|e| AppError::InternalError(format!("Redis SET error: {}", e)))?;
        }
    }

    Ok(())
}

async fn clear_login_failures(
    redis_client: &redis::Client,
    username: &str,
    ip: &str,
) -> Result<(), AppError> {
    let mut conn = redis_client
        .get_async_connection()
        .await
        .map_err(|e| AppError::InternalError(format!("Redis connection error: {}", e)))?;
    let user_keys = [
        format!("auth_fail:user:{}", username),
        format!("auth_lockout:user:{}", username),
    ];
    for key in user_keys {
        let _: () = conn
            .del(key)
            .await
            .map_err(|e| AppError::InternalError(format!("Redis DEL error: {}", e)))?;
    }

    if ip != "unknown" {
        let ip_keys = [
            format!("auth_fail:ip:{}", ip),
            format!("auth_lockout:ip:{}", ip),
        ];
        for key in ip_keys {
            let _: () = conn
                .del(key)
                .await
                .map_err(|e| AppError::InternalError(format!("Redis DEL error: {}", e)))?;
        }
    }
    Ok(())
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

fn audience_allowed(aud: Option<&str>) -> bool {
    let allowed = env::var("RESOURCE_AUDIENCE").unwrap_or_else(|_| "first-party".to_string());
    let allowed: Vec<&str> = allowed.split(',').map(|v| v.trim()).collect();
    match aud {
        Some(value) => allowed.iter().any(|a| a == &value),
        None => false,
    }
}

fn user_agent_from_headers(headers: &HeaderMap) -> String {
    headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("unknown")
        .to_string()
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
