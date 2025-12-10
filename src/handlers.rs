use axum::{
    extract::State,
    Json,
};
use tokio::time::Instant;
use crate::{
    models::{User, DashboardData, AppState, LoginRequest, LoginResponse, RefreshRequest},
    db, error::AppError, cache, auth, rate_limit,
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
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, AppError> {
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
            let access_token = auth::create_jwt(&user.username)?;
            let refresh_token = auth::create_refresh_token();
            
            // 4. Guardar refresh token en Redis
            auth::store_refresh_token(&state.redis_client, &user.username, &refresh_token).await?;
            
            return Ok(Json(LoginResponse { access_token, refresh_token }));
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
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, AppError> {
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
        .await?;
    
    let token = auth::create_jwt(&username)?;
    let refresh_token = auth::create_refresh_token();
    
    auth::store_refresh_token(&state.redis_client, &username, &refresh_token).await?;
    
    Ok(Json(LoginResponse { access_token: token, refresh_token }))
}

pub async fn refresh(
    State(state): State<AppState>,
    Json(payload): Json<RefreshRequest>,
) -> Result<Json<LoginResponse>, AppError> {
    // Validar refresh token
    let username = auth::validate_refresh_token(&state.redis_client, &payload.refresh_token)
        .await?
        .ok_or_else(|| AppError::AuthError("Invalid or expired refresh token".to_string()))?;

    // Generar nuevo access token
    let access_token = auth::create_jwt(&username)?;
    
    // Mantener el mismo refresh token (o generar uno nuevo si prefieres rotación)
    Ok(Json(LoginResponse {
        access_token,
        refresh_token: payload.refresh_token,
    }))
}

pub async fn logout(
    State(state): State<AppState>,
    Json(payload): Json<RefreshRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Revocar refresh token
    auth::revoke_refresh_token(&state.redis_client, &payload.refresh_token).await?;
    
    Ok(Json(serde_json::json!({ "message": "Logged out successfully" })))
}
