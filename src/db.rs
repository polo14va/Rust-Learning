use sqlx::PgPool;
use crate::{models::{User, DashboardStat, RecentActivity, SystemAlert, OAuthClient, AuthorizationCode, RefreshTokenRecord}, error::AppError};


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

// --- OAuth / OIDC ---

pub async fn get_oauth_client(pool: &PgPool, client_id: &str) -> Result<Option<OAuthClient>, AppError> {
    let client = sqlx::query_as::<_, OAuthClient>(
        "SELECT client_id, client_secret, redirect_uris, scopes, grant_types, name FROM oauth_clients WHERE client_id = $1",
    )
    .bind(client_id)
    .fetch_optional(pool)
    .await?;

    Ok(client)
}

pub async fn store_authorization_code(
    pool: &PgPool,
    code: &AuthorizationCode,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO oauth_authorization_codes (code, client_id, username, redirect_uri, scope, code_challenge, code_challenge_method, nonce, expires_at) 
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(&code.code)
    .bind(&code.client_id)
    .bind(&code.username)
    .bind(&code.redirect_uri)
    .bind(&code.scope)
    .bind(&code.code_challenge)
    .bind(&code.code_challenge_method)
    .bind(&code.nonce)
    .bind(&code.expires_at)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn consume_authorization_code(
    pool: &PgPool,
    code: &str,
) -> Result<Option<AuthorizationCode>, AppError> {
    let record = sqlx::query_as::<_, AuthorizationCode>(
        "DELETE FROM oauth_authorization_codes 
         WHERE code = $1 AND expires_at > NOW()
         RETURNING code, client_id, username, redirect_uri, scope, code_challenge, code_challenge_method, nonce, expires_at",
    )
    .bind(code)
    .fetch_optional(pool)
    .await?;

    Ok(record)
}

pub async fn store_refresh_token_record(
    pool: &PgPool,
    record: &RefreshTokenRecord,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO oauth_refresh_tokens (refresh_token, client_id, username, scope, expires_at) 
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(&record.refresh_token)
    .bind(&record.client_id)
    .bind(&record.username)
    .bind(&record.scope)
    .bind(&record.expires_at)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_refresh_token_record(
    pool: &PgPool,
    refresh_token: &str,
) -> Result<Option<RefreshTokenRecord>, AppError> {
    let record = sqlx::query_as::<_, RefreshTokenRecord>(
        "SELECT refresh_token, client_id, username, scope, expires_at, revoked 
         FROM oauth_refresh_tokens 
         WHERE refresh_token = $1",
    )
    .bind(refresh_token)
    .fetch_optional(pool)
    .await?;

    Ok(record)
}

pub async fn revoke_refresh_token_record(
    pool: &PgPool,
    refresh_token: &str,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE oauth_refresh_tokens SET revoked = TRUE WHERE refresh_token = $1",
    )
    .bind(refresh_token)
    .execute(pool)
    .await?;

    Ok(())
}

// --- Bootstrap helpers ---

/// Asegura que un cliente OAuth existe; si no, lo inserta (idempotente).
pub async fn ensure_client_exists(
    pool: &PgPool,
    client_id: &str,
    client_secret: &str,
    redirect_uris: &str,
    scopes: &str,
    grant_types: &str,
    name: &str,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO oauth_clients (client_id, client_secret, redirect_uris, scopes, grant_types, name)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (client_id) DO NOTHING",
    )
    .bind(client_id)
    .bind(client_secret)
    .bind(redirect_uris)
    .bind(scopes)
    .bind(grant_types)
    .bind(name)
    .execute(pool)
    .await?;

    Ok(())
}
