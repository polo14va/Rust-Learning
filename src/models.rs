use serde::{Serialize, Deserialize};
use sqlx::{FromRow, PgPool};

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: i32,
    pub username: String,
    pub email: String,
    #[serde(skip)] // No queremos enviar el hash en el JSON de respuesta
    pub password_hash: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // Subject (Username)
    pub exp: usize,  // Expiration time
}

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct DashboardStat {
    pub metric_name: String,
    pub value: i32,
}

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct RecentActivity {
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct SystemAlert {
    pub message: String,
    pub severity: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DashboardData {
    pub stats: Vec<DashboardStat>,
    pub activities: Vec<RecentActivity>,
    pub alerts: Vec<SystemAlert>,
}

// Estado compartido de la aplicación
// Clone es barato porque solo incrementa el contador de Arc
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub redis_client: redis::Client, // Cliente de Redis (es thread-safe y barato de clonar)
}
