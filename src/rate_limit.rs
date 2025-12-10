use redis::AsyncCommands;
use crate::error::AppError;
use std::env;

pub async fn check_rate_limit(
    redis_client: &redis::Client,
    key: &str,
) -> Result<bool, AppError> {
    let max_requests: u32 = env::var("RATE_LIMIT_PER_SECOND")
        .unwrap_or_else(|_| "10".to_string())
        .parse()
        .unwrap_or(10);

    let window_seconds = 60; // Ventana de tiempo

    let mut conn = redis_client
        .get_async_connection()
        .await
        .map_err(|e| {
            tracing::error!("Redis connection error: {}", e);
            AppError::DatabaseError(sqlx::Error::Protocol("Redis error".into()))
        })?;

    // Incrementar contador
    let count: u32 = conn.incr(key, 1).await.map_err(|e| {
        tracing::error!("Redis INCR error: {}", e);
        AppError::DatabaseError(sqlx::Error::Protocol("Redis error".into()))
    })?;

    // Si es la primera request, establecer TTL
    if count == 1 {
        let _: () = conn.expire(key, window_seconds).await.map_err(|e| {
            tracing::error!("Redis EXPIRE error: {}", e);
            AppError::DatabaseError(sqlx::Error::Protocol("Redis error".into()))
        })?;
    }

    // Verificar si excede el límite
    if count > max_requests {
        tracing::warn!("Rate limit exceeded for key: {}", key);
        return Ok(false);
    }

    Ok(true)
}
