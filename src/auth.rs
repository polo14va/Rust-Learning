use argon2::{password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString}, Argon2};
use base64ct::{Base64UrlUnpadded, Encoding};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, TokenData, Validation};
use rand::rngs::OsRng;
use redis::AsyncCommands;
use rsa::{
    pkcs1::{DecodeRsaPrivateKey, DecodeRsaPublicKey, EncodeRsaPrivateKey, EncodeRsaPublicKey},
    RsaPrivateKey, RsaPublicKey,
};
use rsa::traits::PublicKeyParts;
use sha2::{Digest, Sha256};
use std::{env, sync::Arc};
use uuid::Uuid;

use crate::{error::AppError, models::{Claims, JwtKeys}};

const DEFAULT_ACCESS_TOKEN_MINUTES: i64 = 15;
const DEFAULT_REFRESH_TOKEN_DAYS: i64 = 7;
const DEFAULT_SESSION_MINUTES: usize = 60;

pub fn hash_password(password: &str) -> Result<String, AppError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::InternalError(format!("Error hashing password: {}", e)))
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool, AppError> {
    // Compatibilidad: aceptar hashes antiguos Bcrypt
    if hash.starts_with("$2") {
        return bcrypt::verify(password, hash)
            .map_err(|e| AppError::InternalError(format!("Error verifying password: {}", e)));
    }

    let parsed = PasswordHash::new(hash)
        .map_err(|e| AppError::InternalError(format!("Invalid password hash: {}", e)))?;
    let argon2 = Argon2::default();
    Ok(argon2
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

fn load_private_key_pem() -> Result<(String, String), AppError> {
    if let Ok(private_pem) = env::var("JWT_PRIVATE_KEY_PEM") {
        // Derivar la clave pública a partir de la privada si no se provee
        let private = RsaPrivateKey::from_pkcs1_pem(&private_pem)
            .map_err(|e| AppError::InternalError(format!("Invalid RSA private key: {}", e)))?;
        let public_pem = private
            .to_public_key()
            .to_pkcs1_pem(rsa::pkcs8::LineEnding::LF)
            .map_err(|e| AppError::InternalError(format!("Invalid RSA public key: {}", e)))?
            .to_string();
        return Ok((private_pem, public_pem));
    }

    // Fallback: generar una clave efímera para entornos locales
    let mut rng = OsRng;
    let private = RsaPrivateKey::new(&mut rng, 2048)
        .map_err(|e| AppError::InternalError(format!("Failed to generate RSA key: {}", e)))?;
    let private_pem = private
        .to_pkcs1_pem(rsa::pkcs8::LineEnding::LF)
        .map_err(|e| AppError::InternalError(format!("Failed to encode RSA key: {}", e)))?
        .to_string();
    let public_pem = private
        .to_public_key()
        .to_pkcs1_pem(rsa::pkcs8::LineEnding::LF)
        .map_err(|e| AppError::InternalError(format!("Failed to encode RSA pub key: {}", e)))?
        .to_string();

    tracing::warn!("JWT_PRIVATE_KEY_PEM no encontrado. Se generó una clave efímera (solo para desarrollo).");
    Ok((private_pem, public_pem))
}

fn to_base64_url(data: &[u8]) -> String {
    Base64UrlUnpadded::encode_string(data)
}

fn derive_jwk_components(public_key: &RsaPublicKey) -> (String, String) {
    let n = to_base64_url(&public_key.n().to_bytes_be());
    let e = to_base64_url(&public_key.e().to_bytes_be());
    (n, e)
}

pub fn load_jwt_keys() -> Result<JwtKeys, AppError> {
    let (private_pem, public_pem) = load_private_key_pem()?;
    let encoding = EncodingKey::from_rsa_pem(private_pem.as_bytes())
        .map_err(|e| AppError::InternalError(format!("Invalid RSA private key: {}", e)))?;
    let decoding = DecodingKey::from_rsa_pem(public_pem.as_bytes())
        .map_err(|e| AppError::InternalError(format!("Invalid RSA public key: {}", e)))?;

    let public = RsaPublicKey::from_pkcs1_pem(&public_pem)
        .map_err(|e| AppError::InternalError(format!("Invalid RSA public key: {}", e)))?;
    let (n, e) = derive_jwk_components(&public);

    let mut hasher = Sha256::new();
    hasher.update(public_pem.as_bytes());
    let kid = to_base64_url(&hasher.finalize());

    Ok(JwtKeys {
        encoding: Arc::new(encoding),
        decoding: Arc::new(decoding),
        kid,
        alg: Algorithm::RS256,
        n,
        e,
    })
}

pub fn create_access_token(
    username: &str,
    scope: &str,
    client_id: &str,
    issuer: &str,
    keys: &JwtKeys,
    ttl_minutes: Option<i64>,
) -> Result<String, AppError> {
    let expires_in = ttl_minutes.unwrap_or(DEFAULT_ACCESS_TOKEN_MINUTES);
    let exp = Utc::now()
        .checked_add_signed(Duration::minutes(expires_in))
        .expect("valid timestamp")
        .timestamp();

    let claims = Claims {
        sub: username.to_owned(),
        exp: exp as usize,
        iss: Some(issuer.to_string()),
        aud: Some(client_id.to_string()),
        scope: Some(scope.to_string()),
        iat: Some(Utc::now().timestamp() as usize),
        nonce: None,
    };

    let mut header = Header::new(keys.alg);
    header.kid = Some(keys.kid.clone());

    encode(&header, &claims, &keys.encoding)
        .map_err(|e| AppError::InternalError(format!("Error creating access token: {}", e)))
}

pub fn create_id_token(
    username: &str,
    client_id: &str,
    issuer: &str,
    keys: &JwtKeys,
    nonce: Option<String>,
    ttl_minutes: Option<i64>,
) -> Result<String, AppError> {
    let expires_in = ttl_minutes.unwrap_or(DEFAULT_ACCESS_TOKEN_MINUTES);
    let exp = Utc::now()
        .checked_add_signed(Duration::minutes(expires_in))
        .expect("valid timestamp")
        .timestamp();

    let claims = Claims {
        sub: username.to_owned(),
        exp: exp as usize,
        iss: Some(issuer.to_string()),
        aud: Some(client_id.to_string()),
        scope: Some("openid".to_string()),
        iat: Some(Utc::now().timestamp() as usize),
        nonce,
    };

    let mut header = Header::new(keys.alg);
    header.kid = Some(keys.kid.clone());

    encode(&header, &claims, &keys.encoding)
        .map_err(|e| AppError::InternalError(format!("Error creating id token: {}", e)))
}

pub fn validate_jwt(token: &str, keys: &JwtKeys) -> Result<TokenData<Claims>, jsonwebtoken::errors::Error> {
    let validation = Validation::new(keys.alg);
    decode::<Claims>(token, &keys.decoding, &validation)
}

pub fn create_refresh_token() -> String {
    Uuid::new_v4().to_string()
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RefreshSession {
    pub username: String,
    pub client_id: String,
    pub scope: String,
}

pub async fn store_refresh_token(
    redis_client: &redis::Client,
    session: &RefreshSession,
    refresh_token: &str,
) -> Result<(), AppError> {
    let ttl_seconds = env::var("REFRESH_TOKEN_TTL_DAYS")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(DEFAULT_REFRESH_TOKEN_DAYS)
        * 24
        * 60
        * 60;

    let mut conn = redis_client
        .get_async_connection()
        .await
        .map_err(|e| AppError::InternalError(format!("Redis connection error: {}", e)))?;

    let key = format!("refresh_token:{}", refresh_token);
    let payload = serde_json::to_string(session)
        .map_err(|e| AppError::InternalError(format!("Error serializing refresh session: {}", e)))?;

    let _: () = conn
        .set_ex(&key, payload, ttl_seconds as u64)
        .await
        .map_err(|e| AppError::InternalError(format!("Redis SET error: {}", e)))?;

    Ok(())
}

pub async fn validate_refresh_token(
    redis_client: &redis::Client,
    refresh_token: &str,
) -> Result<Option<RefreshSession>, AppError> {
    let mut conn = redis_client
        .get_async_connection()
        .await
        .map_err(|e| AppError::InternalError(format!("Redis connection error: {}", e)))?;

    let key = format!("refresh_token:{}", refresh_token);
    let value: Option<String> = conn
        .get(&key)
        .await
        .map_err(|e| AppError::InternalError(format!("Redis GET error: {}", e)))?;

    if let Some(v) = value {
        let session: RefreshSession = serde_json::from_str(&v)
            .map_err(|e| AppError::InternalError(format!("Invalid refresh session: {}", e)))?;
        Ok(Some(session))
    } else {
        Ok(None)
    }
}

pub async fn revoke_refresh_token(
    redis_client: &redis::Client,
    refresh_token: &str,
) -> Result<(), AppError> {
    let mut conn = redis_client
        .get_async_connection()
        .await
        .map_err(|e| AppError::InternalError(format!("Redis connection error: {}", e)))?;

    let key = format!("refresh_token:{}", refresh_token);
    let _: () = conn
        .del(&key)
        .await
        .map_err(|e| AppError::InternalError(format!("Redis DEL error: {}", e)))?;

    Ok(())
}

pub async fn create_session(
    redis_client: &redis::Client,
    username: &str,
) -> Result<String, AppError> {
    let ttl = env::var("SESSION_TTL_MINUTES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(DEFAULT_SESSION_MINUTES);

    let mut conn = redis_client
        .get_async_connection()
        .await
        .map_err(|e| AppError::InternalError(format!("Redis connection error: {}", e)))?;

    let session_id = Uuid::new_v4().to_string();
    let key = format!("sso:session:{session_id}");
    let _: () = conn
        .set_ex(&key, username, (ttl * 60) as u64)
        .await
        .map_err(|e| AppError::InternalError(format!("Redis SET error: {}", e)))?;

    Ok(session_id)
}

pub async fn validate_session(
    redis_client: &redis::Client,
    session_id: &str,
) -> Result<Option<String>, AppError> {
    let mut conn = redis_client
        .get_async_connection()
        .await
        .map_err(|e| AppError::InternalError(format!("Redis connection error: {}", e)))?;

    let key = format!("sso:session:{session_id}");
    let username: Option<String> = conn
        .get(&key)
        .await
        .map_err(|e| AppError::InternalError(format!("Redis GET error: {}", e)))?;

    Ok(username)
}

pub async fn revoke_session(
    redis_client: &redis::Client,
    session_id: &str,
) -> Result<(), AppError> {
    let mut conn = redis_client
        .get_async_connection()
        .await
        .map_err(|e| AppError::InternalError(format!("Redis connection error: {}", e)))?;

    let key = format!("sso:session:{session_id}");
    let _: () = conn
        .del(&key)
        .await
        .map_err(|e| AppError::InternalError(format!("Redis DEL error: {}", e)))?;

    Ok(())
}
