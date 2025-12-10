use serde::{Serialize, Deserialize};
use sqlx::FromRow;

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: i32,
    pub username: String,
    pub email: String,
}

#[derive(Debug, Serialize, FromRow)]
pub struct DashboardStat {
    pub metric_name: String,
    pub value: i32,
}

#[derive(Debug, Serialize, FromRow)]
pub struct RecentActivity {
    pub description: String,
    // Note: In real setup, deal with Time types. String for simplicity or sqlx::types::chrono
    // Simple simplification: We won't select created_at to avoid chrono dependency for now
    // or we assume it matches String (not ideal). let's skip returning date for simplicity 
    // or add 'chrono' feature to sqlx. 
    // To keep it simple without adding chrono dependency now:
    // pub created_at: String 
}

#[derive(Debug, Serialize, FromRow)]
pub struct SystemAlert {
    pub message: String,
    pub severity: String,
}

#[derive(Debug, Serialize)]
pub struct DashboardData {
    pub stats: Vec<DashboardStat>,
    pub activities: Vec<RecentActivity>,
    pub alerts: Vec<SystemAlert>,
}
