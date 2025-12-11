use serde::{Serialize, Deserialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;
use jsonwebtoken::Algorithm;

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
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // Subject (Username)
    pub exp: usize,  // Expiration time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iss: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aud: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iat: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
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

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct OAuthClient {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uris: String,
    pub scopes: String,
    pub grant_types: String,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct AuthorizationCode {
    pub code: String,
    pub client_id: String,
    pub username: String,
    pub redirect_uri: String,
    pub scope: String,
    pub code_challenge: Option<String>,
    pub code_challenge_method: Option<String>,
    pub nonce: Option<String>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct RefreshTokenRecord {
    pub refresh_token: String,
    pub client_id: String,
    pub username: String,
    pub scope: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub revoked: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserInfoResponse {
    pub sub: String,
    pub preferred_username: String,
    pub email: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IntrospectionResponse {
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenIdConfiguration {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub userinfo_endpoint: String,
    pub jwks_uri: String,
    pub response_types_supported: Vec<String>,
    pub subject_types_supported: Vec<String>,
    pub id_token_signing_alg_values_supported: Vec<String>,
    pub token_endpoint_auth_methods_supported: Vec<String>,
    pub grant_types_supported: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JwkKey {
    pub kty: String,
    #[serde(rename = "use")]
    pub use_: String,
    pub kid: String,
    pub alg: String,
    pub n: String,
    pub e: String,
}

#[derive(Clone)]
pub struct JwtKeys {
    pub encoding: Arc<jsonwebtoken::EncodingKey>,
    pub decoding: Arc<jsonwebtoken::DecodingKey>,
    pub kid: String,
    pub alg: Algorithm,
    pub n: String,
    pub e: String,
}

// Estado compartido de la aplicación
// Clone es barato porque solo incrementa el contador de Arc
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub redis_client: redis::Client, // Cliente de Redis (es thread-safe y barato de clonar)
    pub keys: JwtKeys,
    pub issuer: String,
}
