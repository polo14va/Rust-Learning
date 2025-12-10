use axum::{
    extract::State,
    Json,
};
use sqlx::PgPool;
use crate::{models::{User, DashboardData}, db, error::AppError};

pub async fn list_users(State(pool): State<PgPool>) -> Result<Json<Vec<User>>, AppError> {
    let users = db::get_all_users(&pool).await?;
    Ok(Json(users))
}

pub async fn get_dashboard(State(pool): State<PgPool>) -> Result<Json<DashboardData>, AppError> {
    // Aquí ocurre la magia de la concurrencia.
    // Lanzamos las 3 tareas al mismo tiempo.
    // Tokio las ejecuta concurrentemente (interleaved) en el thread pool.
    
    let (stats_result, activities_result, alerts_result) = tokio::join!(
        db::get_stats(&pool),
        db::get_activities(&pool),
        db::get_alerts(&pool)
    );

    // Si estuviéramos en Java/Sync, esto tardaría: 2s + 1s + 2s = 5s
    // Con `join!`, tarda el máximo de las tres: 2s.

    // Desempaquetamos los resultados (fail fast: si una falla, retornamos error)
    let stats = stats_result?;
    let activities = activities_result?;
    let alerts = alerts_result?;

    Ok(Json(DashboardData {
        stats,
        activities,
        alerts,
    }))
}
