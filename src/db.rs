use sqlx::PgPool;
use crate::{models::{User, DashboardStat, RecentActivity, SystemAlert}, error::AppError};


// --- Users ---
pub async fn get_all_users(pool: &PgPool) -> Result<Vec<User>, AppError> {
    let users = sqlx::query_as::<_, User>("SELECT id, username, email, password_hash FROM users")
        .fetch_all(pool)
        .await?;
    Ok(users)
}

pub async fn get_user_by_username(pool: &PgPool, username: &str) -> Result<Option<User>, AppError> {
    let user = sqlx::query_as::<_, User>("SELECT id, username, email, password_hash FROM users WHERE username = $1")
        .bind(username)
        .fetch_optional(pool)
        .await?;
    Ok(user)
}

// --- Dashboard (Simuladas como lentas) ---

pub async fn get_stats(pool: &PgPool) -> Result<Vec<DashboardStat>, AppError> {
    let start = std::time::Instant::now();
    
    let stats = sqlx::query_as::<_, DashboardStat>("SELECT metric_name, value FROM dashboard_stats")
        .fetch_all(pool)
        .await?;
    println!("DB: get_stats tardó {:?}", start.elapsed());
    Ok(stats)
}

pub async fn get_activities(pool: &PgPool) -> Result<Vec<RecentActivity>, AppError> {
    let start = std::time::Instant::now();
    let activities = sqlx::query_as::<_, RecentActivity>("SELECT description FROM recent_activities")
        .fetch_all(pool)
        .await?;
    println!("DB: get_activities tardó {:?}", start.elapsed());
    Ok(activities)
}

pub async fn get_alerts(pool: &PgPool) -> Result<Vec<SystemAlert>, AppError> {
    let start = std::time::Instant::now();
    let alerts = sqlx::query_as::<_, SystemAlert>("SELECT message, severity FROM system_alerts")
        .fetch_all(pool)
        .await?;
    println!("DB: get_alerts tardó {:?}", start.elapsed());
    Ok(alerts)
}
