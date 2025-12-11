use redis::AsyncCommands;
use crate::{models::DashboardData, error::AppError};

// Helper para convertir el error de Redis a nuestro AppError
fn redis_error(e: redis::RedisError) -> AppError {
    // Para simplificar, asumimos que cualquier error de redis es un error "interno" 
    // Podrías crear AppError::CacheError(e)
    println!("Redis Error: {}", e);
    AppError::DatabaseError(sqlx::Error::Protocol(format!("Redis Error: {}", e).into())) 
}

pub async fn get_dashboard_data(client: &redis::Client) -> Result<Option<DashboardData>, AppError> {
    let mut conn = client.get_async_connection().await.map_err(redis_error)?;
    
    // Obtenemos el string JSON
    let cached_json: Option<String> = conn.get("dashboard_data").await.map_err(redis_error)?;

    if let Some(json_str) = cached_json {
        // CACHE HIT - Registrar métrica
        crate::metrics::record_cache_hit("redis");
        
        let data: DashboardData = serde_json::from_str(&json_str).unwrap(); // Unwrap seguro si confiamos en lo que guardamos
        return Ok(Some(data));
    }

    // CACHE MISS - Registrar métrica
    crate::metrics::record_cache_miss("redis");
    
    Ok(None)
}

pub async fn set_dashboard_data(client: &redis::Client, data: &DashboardData) -> Result<(), AppError> {
    let mut conn = client.get_async_connection().await.map_err(redis_error)?;
    
    let json_str = serde_json::to_string(data).unwrap();

    // Guardamos con TTL de 60 segundos (SETEX)
    let _: () = conn.set_ex("dashboard_data", json_str, 60).await.map_err(redis_error)?;

    Ok(())
}
