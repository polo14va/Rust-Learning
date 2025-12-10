use sqlx::PgPool;
use crate::{models::{User, DashboardStat, RecentActivity, SystemAlert}, error::AppError};
use tokio::time::{sleep, Duration};

// --- Users ---
pub async fn get_all_users(pool: &PgPool) -> Result<Vec<User>, AppError> {
    let users = sqlx::query_as::<_, User>("SELECT id, username, email FROM users")
        .fetch_all(pool)
        .await?;
    Ok(users)
}

// --- Dashboard (Simuladas como lentas) ---

pub async fn get_stats(pool: &PgPool) -> Result<Vec<DashboardStat>, AppError> {
    // Simular trabajo pesado
    sleep(Duration::from_secs(2)).await;
    
    let stats = sqlx::query_as::<_, DashboardStat>("SELECT metric_name, value FROM dashboard_stats")
        .fetch_all(pool)
        .await?;
    Ok(stats)
}

pub async fn get_activities(pool: &PgPool) -> Result<Vec<RecentActivity>, AppError> {
    sleep(Duration::from_secs(1)).await;
    
    let activities = sqlx::query_as::<_, RecentActivity>("SELECT description FROM recent_activities")
        .fetch_all(pool)
        .await?;
    Ok(activities)
}

pub async fn get_alerts(pool: &PgPool) -> Result<Vec<SystemAlert>, AppError> {
    sleep(Duration::from_secs(2)).await;
    
    let alerts = sqlx::query_as::<_, SystemAlert>("SELECT message, severity FROM system_alerts")
        .fetch_all(pool)
        .await?;
    Ok(alerts)
}
