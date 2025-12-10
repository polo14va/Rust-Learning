use jsonwebtoken::{encode, decode, Header, Validation, EncodingKey, DecodingKey, TokenData};
use chrono::{Utc, Duration};
use crate::{models::Claims, error::AppError};

use std::env;

fn get_jwt_secret() -> Vec<u8> {
    env::var("JWT_SECRET")
        .unwrap_or_else(|_| "fallback_secret_key".to_string())
        .into_bytes()
}

fn get_jwt_expiration_hours() -> i64 {
    env::var("JWT_EXPIRATION_HOURS")
        .unwrap_or_else(|_| "24".to_string())
        .parse()
        .unwrap_or(24)
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
        .checked_add_signed(Duration::hours(get_jwt_expiration_hours()))
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
