use axum::{
    extract::State,
    Json,
};
use tokio::time::Instant;
use crate::{models::{User, DashboardData, AppState, LoginRequest, LoginResponse}, db, error::AppError, cache, auth, rate_limit};

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
            // 3. Generar token
            let token = auth::create_jwt(&user.username)?;
            return Ok(Json(LoginResponse { token }));
        }
    }

    Err(AppError::AuthError("Credenciales inválidas".to_string()))
}

// Endpoint temporal para crear usuarios (SOLO PARA DESARROLLO)
pub async fn register(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, AppError> {
    // Rate limiting por username
    let rate_key = format!("rate_limit:register:{}", payload.username);
    if !rate_limit::check_rate_limit(&state.redis_client, &rate_key).await? {
        return Err(AppError::AuthError("Too many requests. Try again later.".to_string()));
    }

    let hash = auth::hash_password(&payload.password)?;
    
    sqlx::query("INSERT INTO users (username, email, password_hash) VALUES ($1, $2, $3)")
        .bind(&payload.username)
        .bind(format!("{}@test.com", &payload.username))
        .bind(&hash)
        .execute(&state.pool)
        .await?;
    
    let token = auth::create_jwt(&payload.username)?;
    Ok(Json(LoginResponse { token }))
}
