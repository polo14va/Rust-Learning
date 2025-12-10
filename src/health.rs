use axum::{extract::State, Json};
use serde::Serialize;
use crate::models::AppState;

#[derive(Serialize)]
pub struct HealthResponse {
    status: String,
    database: String,
    redis: String,
}

pub async fn health_check(State(state): State<AppState>) -> Json<HealthResponse> {
    // Check Postgres
    let db_status = match sqlx::query("SELECT 1").fetch_one(&state.pool).await {
        Ok(_) => "healthy",
        Err(_) => "unhealthy",
    };

    // Check Redis
    let redis_status = match state.redis_client.get_async_connection().await {
        Ok(_) => "healthy",
        Err(_) => "unhealthy",
    };

    let overall_status = if db_status == "healthy" && redis_status == "healthy" {
        "healthy"
    } else {
        "degraded"
    };

    Json(HealthResponse {
        status: overall_status.to_string(),
        database: db_status.to_string(),
        redis: redis_status.to_string(),
    })
}
