use axum::{
    extract::State,
    Json,
};
use tokio::time::Instant;
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use chrono::{Duration, Utc};
use time::Duration as CookieDuration;
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
    Json(payload): Json<LoginRequest>,
) -> Result<(CookieJar, Json<LoginResponse>), AppError> {
    // Rate limiting por username
    let rate_key = format!("rate_limit:login:{}", payload.username);
    if !rate_limit::check_rate_limit(&state.redis_client, &rate_key).await? {
        return Err(AppError::AuthError("Too many requests. Try again later.".to_string()));
    }

    // 1. Buscar usuario
    let user = db::get_user_by_username(&state.pool, &payload.username).await?;

    if let Some(user) = user {
        // 2. Verificar password
        if auth::verify_password(&payload.password, &user.password_hash)? {
            // 3. Generar tokens
            let scope = "openid profile email offline_access";
            let access_token = auth::create_access_token(&user.username, scope, "first-party", &state.issuer, &state.keys, None)?;
            let refresh_token = auth::create_refresh_token();
            
            // 4. Guardar refresh token en Redis
            let session = auth::RefreshSession { username: user.username.clone(), client_id: "first-party".to_string(), scope: scope.to_string() };
            auth::store_refresh_token(&state.redis_client, &session, &refresh_token).await?;

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

            // Crear sesión SSO y cookie HttpOnly
            let session_id = auth::create_session(&state.redis_client, &user.username).await?;
            let cookie = Cookie::build(("sso_session", session_id))
                .http_only(true)
                .secure(false)
                .same_site(SameSite::Lax)
                .path("/")
                .max_age(CookieDuration::minutes(60))
                .build();
            let updated_jar = jar.add(cookie);
            
            return Ok((updated_jar, Json(LoginResponse { access_token, refresh_token })));
        }
    }

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
    Json(payload): Json<LoginRequest>,
) -> Result<(CookieJar, Json<LoginResponse>), AppError> {
    // Rate limiting por username
    let rate_key = format!("rate_limit:register:{}", payload.username);
    if !rate_limit::check_rate_limit(&state.redis_client, &rate_key).await? {
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
    
    let scope = "openid profile email offline_access";
    let token = auth::create_access_token(&username, scope, "first-party", &state.issuer, &state.keys, None)?;
    let refresh_token = auth::create_refresh_token();
    
    let session = auth::RefreshSession { username: username.clone(), client_id: "first-party".to_string(), scope: scope.to_string() };
    auth::store_refresh_token(&state.redis_client, &session, &refresh_token).await?;

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

    let session_id = auth::create_session(&state.redis_client, &username).await?;
    let cookie = Cookie::build(("sso_session", session_id))
        .http_only(true)
        .secure(false)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(CookieDuration::minutes(60))
        .build();
    let updated_jar = jar.add(cookie);
    
    // Email simulado (bienvenida/alta)
    let email_body = format!(
        "Hola {username},\n\nTu cuenta se ha creado correctamente y ya puedes usar SSO/OIDC.\n\nScopes por defecto: {scope}\n\n-- Equipo de autenticación"
    );
    let _ = email::send_email(&email.unwrap_or_else(|| "user@example.com".to_string()), "Bienvenido a SSO", &email_body).await;
    
    Ok((updated_jar, Json(LoginResponse { access_token: token, refresh_token })))
}

pub async fn refresh(
    State(state): State<AppState>,
    Json(payload): Json<RefreshRequest>,
) -> Result<Json<LoginResponse>, AppError> {
    // Validar refresh token
    let session = auth::validate_refresh_token(&state.redis_client, &payload.refresh_token)
        .await?
        .ok_or_else(|| AppError::AuthError("Invalid or expired refresh token".to_string()))?;

    if let Some(record) = db::get_refresh_token_record(&state.pool, &payload.refresh_token).await? {
        if record.revoked || record.expires_at < Utc::now() {
            return Err(AppError::AuthError("Refresh token expired or revoked".to_string()));
        }
    }

    // Generar nuevo access token
    let access_token = auth::create_access_token(
        &session.username,
        &session.scope,
        &session.client_id,
        &state.issuer,
        &state.keys,
        None,
    )?;
    
    // Mantener el mismo refresh token (o generar uno nuevo si prefieres rotación)
    Ok(Json(LoginResponse {
        access_token,
        refresh_token: payload.refresh_token,
    }))
}

pub async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(payload): Json<RefreshRequest>,
) -> Result<(CookieJar, Json<serde_json::Value>), AppError> {
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
