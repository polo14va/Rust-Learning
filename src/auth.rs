use jsonwebtoken::{encode, decode, Header, Validation, EncodingKey, DecodingKey, TokenData};
use chrono::{Utc, Duration};
use crate::{models::Claims, error::AppError};
use redis::AsyncCommands;
use uuid::Uuid;

use std::env;

fn get_jwt_secret() -> Vec<u8> {
    env::var("JWT_SECRET")
        .unwrap_or_else(|_| "fallback_secret_key".to_string())
        .into_bytes()
}

fn get_jwt_expiration_hours() -> i64 {
    // Cambiado a 15 minutos para refresh token pattern
    env::var("JWT_EXPIRATION_MINUTES")
        .unwrap_or_else(|_| "15".to_string())
        .parse::<i64>()
        .unwrap_or(15)
}

pub fn hash_password(password: &str) -> Result<String, AppError> {
    bcrypt::hash(password, bcrypt::DEFAULT_COST).map_err(|e| {
        println!("Error hashing password: {}", e);
        AppError::DatabaseError(sqlx::Error::Protocol("Error hashing password".into())) // Simplificación error
    })
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool, AppError> {
    bcrypt::verify(password, hash).map_err(|e| {
        println!("Error verifying password: {}", e);
        AppError::DatabaseError(sqlx::Error::Protocol("Error verifying password".into()))
    })
}

pub fn create_jwt(username: &str) -> Result<String, AppError> {
    let expiration = Utc::now()
        .checked_add_signed(Duration::minutes(get_jwt_expiration_hours()))
        .expect("valid timestamp")
        .timestamp();

    let claims = Claims {
        sub: username.to_owned(),
        exp: expiration as usize,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(&get_jwt_secret()),
    )
    .map_err(|e| {
        println!("Error creating JWT: {}", e);
        AppError::DatabaseError(sqlx::Error::Protocol("Error creating JWT".into()))
    })
}

pub fn validate_jwt(token: &str) -> Result<TokenData<Claims>, jsonwebtoken::errors::Error> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(&get_jwt_secret()),
        &Validation::default(),
    )
}

// --- Refresh Tokens ---

pub fn create_refresh_token() -> String {
    Uuid::new_v4().to_string()
}

pub async fn store_refresh_token(
    redis_client: &redis::Client,
    username: &str,
    refresh_token: &str,
) -> Result<(), AppError> {
    let mut conn = redis_client
        .get_async_connection()
        .await
        .map_err(|e| {
            tracing::error!("Redis connection error: {}", e);
            AppError::DatabaseError(sqlx::Error::Protocol("Redis error".into()))
        })?;

    let key = format!("refresh_token:{}", refresh_token);
    let ttl_seconds = 7 * 24 * 60 * 60; // 7 días

    let _: () = conn.set_ex(&key, username, ttl_seconds).await.map_err(|e| {
        tracing::error!("Redis SET error: {}", e);
        AppError::DatabaseError(sqlx::Error::Protocol("Redis error".into()))
    })?;

    Ok(())
}

pub async fn validate_refresh_token(
    redis_client: &redis::Client,
    refresh_token: &str,
) -> Result<Option<String>, AppError> {
    let mut conn = redis_client
        .get_async_connection()
        .await
        .map_err(|e| {
            tracing::error!("Redis connection error: {}", e);
            AppError::DatabaseError(sqlx::Error::Protocol("Redis error".into()))
        })?;

    let key = format!("refresh_token:{}", refresh_token);
    let username: Option<String> = conn.get(&key).await.map_err(|e| {
        tracing::error!("Redis GET error: {}", e);
        AppError::DatabaseError(sqlx::Error::Protocol("Redis error".into()))
    })?;

    Ok(username)
}

pub async fn revoke_refresh_token(
    redis_client: &redis::Client,
    refresh_token: &str,
) -> Result<(), AppError> {
    let mut conn = redis_client
        .get_async_connection()
        .await
        .map_err(|e| {
            tracing::error!("Redis connection error: {}", e);
            AppError::DatabaseError(sqlx::Error::Protocol("Redis error".into()))
        })?;

    let key = format!("refresh_token:{}", refresh_token);
    let _: () = conn.del(&key).await.map_err(|e| {
        tracing::error!("Redis DEL error: {}", e);
        AppError::DatabaseError(sqlx::Error::Protocol("Redis error".into()))
    })?;

    Ok(())
}
