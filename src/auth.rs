use argon2::{password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString}, Argon2};
use base64ct::{Base64UrlUnpadded, Encoding};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, decode_header, encode, Algorithm, DecodingKey, EncodingKey, Header, TokenData, Validation};
use rand::rngs::OsRng;
use redis::AsyncCommands;
use rsa::{
    pkcs1::{DecodeRsaPrivateKey, DecodeRsaPublicKey, EncodeRsaPrivateKey, EncodeRsaPublicKey},
    pkcs8::{DecodePrivateKey, DecodePublicKey},
    RsaPrivateKey, RsaPublicKey,
};
use rsa::traits::PublicKeyParts;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, env, sync::Arc};
use uuid::Uuid;

use crate::{error::AppError, models::{Claims, JwtKeys}};

const DEFAULT_ACCESS_TOKEN_MINUTES: i64 = 15;
const DEFAULT_REFRESH_TOKEN_DAYS: i64 = 7;
const DEFAULT_SESSION_MINUTES: usize = 60;
const PEM_LIST_SEPARATOR: &str = "|||";

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

fn normalize_pem(value: &str) -> String {
    value.replace("\\n", "\n")
}

fn split_pem_list(raw: &str) -> Vec<String> {
    raw.split(PEM_LIST_SEPARATOR)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(normalize_pem)
        .collect()
}

fn parse_private_key(pem: &str) -> Result<RsaPrivateKey, AppError> {
    RsaPrivateKey::from_pkcs1_pem(pem)
        .or_else(|_| RsaPrivateKey::from_pkcs8_pem(pem))
        .map_err(|e| AppError::InternalError(format!("Invalid RSA private key: {}", e)))
}

fn parse_public_key(pem: &str) -> Result<RsaPublicKey, AppError> {
    RsaPublicKey::from_pkcs1_pem(pem)
        .or_else(|_| RsaPublicKey::from_public_key_pem(pem))
        .map_err(|e| AppError::InternalError(format!("Invalid RSA public key: {}", e)))
}

fn derive_public_pem(private: &RsaPrivateKey) -> Result<String, AppError> {
    private
        .to_public_key()
        .to_pkcs1_pem(rsa::pkcs8::LineEnding::LF)
        .map_err(|e| AppError::InternalError(format!("Invalid RSA public key: {}", e)))
        .map(|pem| pem.to_string())
}

fn load_private_key_pem() -> Result<(String, String), AppError> {
    if let Ok(private_pem_raw) = env::var("JWT_PRIVATE_KEY_PEM") {
        let private_pem = normalize_pem(&private_pem_raw);
        // Derivar la clave pública a partir de la privada si no se provee
        let private = parse_private_key(&private_pem)?;
        let public_pem = if let Ok(public_pem_raw) = env::var("JWT_PUBLIC_KEY_PEM") {
            normalize_pem(&public_pem_raw)
        } else {
            derive_public_pem(&private)?
        };
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
    let public_pem = derive_public_pem(&private)?;

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

fn compute_kid(public_pem: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(public_pem.as_bytes());
    to_base64_url(&hasher.finalize())
}

pub fn load_jwt_keys() -> Result<JwtKeys, AppError> {
    let private_keys = if let Ok(raw) = env::var("JWT_PRIVATE_KEYS_PEM") {
        let mut keys = Vec::new();
        for pem in split_pem_list(&raw) {
            let private = parse_private_key(&pem)?;
            keys.push((pem, private));
        }
        keys
    } else if env::var("JWT_PRIVATE_KEY_PEM").is_ok() {
        let (private_pem, _) = load_private_key_pem()?;
        let private = parse_private_key(&private_pem)?;
        vec![(private_pem, private)]
    } else {
        let (private_pem, _) = load_private_key_pem()?;
        let private = parse_private_key(&private_pem)?;
        vec![(private_pem, private)]
    };

    if private_keys.is_empty() {
        return Err(AppError::InternalError("No JWT private keys available".to_string()));
    }

    let mut public_pems: Vec<String> = Vec::new();
    if let Ok(raw) = env::var("JWT_PUBLIC_KEYS_PEM") {
        public_pems.extend(split_pem_list(&raw));
    }
    if let Ok(raw) = env::var("JWT_PUBLIC_KEY_PEM") {
        public_pems.push(normalize_pem(&raw));
    }

    let mut encoding_keys: HashMap<String, Arc<EncodingKey>> = HashMap::new();
    for (private_pem, private) in &private_keys {
        let public_pem = derive_public_pem(private)?;
        let kid = compute_kid(&public_pem);
        let encoding = EncodingKey::from_rsa_pem(private_pem.as_bytes())
            .map_err(|e| AppError::InternalError(format!("Invalid RSA private key: {}", e)))?;
        encoding_keys.insert(kid, Arc::new(encoding));
        public_pems.push(public_pem);
    }

    let mut decoding_keys: HashMap<String, Arc<DecodingKey>> = HashMap::new();
    let mut jwks = Vec::new();
    for public_pem in public_pems {
        let public = parse_public_key(&public_pem)?;
        let (n, e) = derive_jwk_components(&public);
        let kid = compute_kid(&public_pem);
        if decoding_keys.contains_key(&kid) {
            continue;
        }
        let decoding = DecodingKey::from_rsa_pem(public_pem.as_bytes())
            .map_err(|e| AppError::InternalError(format!("Invalid RSA public key: {}", e)))?;
        decoding_keys.insert(kid.clone(), Arc::new(decoding));
        jwks.push(crate::models::JwkKey {
            kty: "RSA".to_string(),
            use_: "sig".to_string(),
            kid,
            alg: "RS256".to_string(),
            n,
            e,
        });
    }

    let active_kid = match env::var("JWT_ACTIVE_KID") {
        Ok(value) => value,
        Err(_) => encoding_keys
            .keys()
            .next()
            .cloned()
            .ok_or_else(|| AppError::InternalError("No active JWT key available".to_string()))?,
    };
    let encoding = encoding_keys
        .get(&active_kid)
        .cloned()
        .ok_or_else(|| AppError::InternalError("JWT_ACTIVE_KID not found in private keys".to_string()))?;

    jwks.sort_by(|a, b| {
        let a_is_active = a.kid == active_kid;
        let b_is_active = b.kid == active_kid;
        b_is_active.cmp(&a_is_active)
    });

    Ok(JwtKeys {
        alg: Algorithm::RS256,
        active_kid,
        encoding,
        decoding_keys,
        jwks,
    })
}

pub fn create_access_token(
    username: &str,
    scope: &str,
    client_id: &str,
    issuer: &str,
    keys: &JwtKeys,
    role: Option<&str>,
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
        role: role.map(|value| value.to_string()),
        nonce: None,
    };

    let mut header = Header::new(keys.alg);
    header.kid = Some(keys.active_kid.clone());

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
        role: None,
        nonce,
    };

    let mut header = Header::new(keys.alg);
    header.kid = Some(keys.active_kid.clone());

    encode(&header, &claims, &keys.encoding)
        .map_err(|e| AppError::InternalError(format!("Error creating id token: {}", e)))
}

pub fn validate_jwt(
    token: &str,
    keys: &JwtKeys,
    issuer: &str,
    audience: Option<&str>,
) -> Result<TokenData<Claims>, jsonwebtoken::errors::Error> {
    let mut validation = Validation::new(keys.alg);
    validation.set_issuer(&[issuer]);
    validation.validate_aud = audience.is_some();
    if let Some(aud) = audience {
        validation.set_audience(&[aud]);
    }
    let header = decode_header(token)?;
    if let Some(kid) = header.kid.as_deref() {
        if let Some(decoding) = keys.decoding_keys.get(kid) {
            return decode::<Claims>(token, decoding, &validation);
        }
    }

    if let Some(decoding) = keys.decoding_keys.get(&keys.active_kid) {
        if let Ok(data) = decode::<Claims>(token, decoding, &validation) {
            return Ok(data);
        }
    }

    let mut last_err = None;
    for decoding in keys.decoding_keys.values() {
        match decode::<Claims>(token, decoding, &validation) {
            Ok(data) => return Ok(data),
            Err(err) => last_err = Some(err),
        }
    }
    Err(last_err.unwrap_or_else(|| {
        jsonwebtoken::errors::Error::from(jsonwebtoken::errors::ErrorKind::InvalidToken)
    }))
}

pub fn create_refresh_token() -> String {
    Uuid::new_v4().to_string()
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RefreshSession {
    pub username: String,
    pub client_id: String,
    pub scope: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ua_hash: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SessionData {
    username: String,
    ua_hash: String,
    #[serde(default)]
    created_at: i64,
}

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub username: String,
    pub created_at: i64,
}

fn hash_user_agent(user_agent: &str) -> String {
    let digest = Sha256::digest(user_agent.as_bytes());
    to_base64_url(&digest)
}

pub fn build_refresh_session(
    username: &str,
    client_id: &str,
    scope: &str,
    session_id: Option<String>,
    user_agent: Option<&str>,
) -> RefreshSession {
    RefreshSession {
        username: username.to_string(),
        client_id: client_id.to_string(),
        scope: scope.to_string(),
        session_id,
        ua_hash: user_agent.map(hash_user_agent),
    }
}

fn bind_refresh_token_ua() -> bool {
    env::var("BIND_REFRESH_TOKEN_UA")
        .map(|v| v == "true")
        .unwrap_or(true)
}

fn bind_refresh_token_session() -> bool {
    env::var("BIND_REFRESH_TOKEN_SESSION")
        .map(|v| v == "true")
        .unwrap_or(true)
}

fn refresh_session_binding_ok(
    session: &RefreshSession,
    user_agent: Option<&str>,
    session_id: Option<&str>,
) -> bool {
    if bind_refresh_token_ua() {
        if let Some(expected) = session.ua_hash.as_deref() {
            let actual = user_agent.map(hash_user_agent);
            if actual.as_deref() != Some(expected) {
                return false;
            }
        }
    }
    if bind_refresh_token_session() {
        if let Some(expected) = session.session_id.as_deref() {
            if session_id != Some(expected) {
                return false;
            }
        }
    }
    true
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

    let user_key = format!("refresh_tokens:user:{}", session.username);
    let _: () = conn
        .sadd(&user_key, refresh_token)
        .await
        .map_err(|e| AppError::InternalError(format!("Redis SADD error: {}", e)))?;
    let _: () = conn
        .expire(&user_key, ttl_seconds as i64)
        .await
        .map_err(|e| AppError::InternalError(format!("Redis EXPIRE error: {}", e)))?;

    Ok(())
}

pub async fn validate_refresh_token(
    redis_client: &redis::Client,
    refresh_token: &str,
    user_agent: Option<&str>,
    session_id: Option<&str>,
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
        if !refresh_session_binding_ok(&session, user_agent, session_id) {
            drop(conn);
            let _ = revoke_refresh_token(redis_client, refresh_token).await;
            return Ok(None);
        }
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
    if let Ok(Some(value)) = conn.get::<_, Option<String>>(&key).await {
        if let Ok(session) = serde_json::from_str::<RefreshSession>(&value) {
            let user_key = format!("refresh_tokens:user:{}", session.username);
            let _: () = conn
                .srem(&user_key, refresh_token)
                .await
                .map_err(|e| AppError::InternalError(format!("Redis SREM error: {}", e)))?;
        }
    }
    let _: () = conn
        .del(&key)
        .await
        .map_err(|e| AppError::InternalError(format!("Redis DEL error: {}", e)))?;

    Ok(())
}

pub async fn create_session(
    redis_client: &redis::Client,
    username: &str,
    user_agent: &str,
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
    let payload = SessionData {
        username: username.to_string(),
        ua_hash: hash_user_agent(user_agent),
        created_at: Utc::now().timestamp(),
    };
    let value = serde_json::to_string(&payload)
        .map_err(|e| AppError::InternalError(format!("Error serializing session: {}", e)))?;
    let _: () = conn
        .set_ex(&key, value, (ttl * 60) as u64)
        .await
        .map_err(|e| AppError::InternalError(format!("Redis SET error: {}", e)))?;

    let user_key = format!("sso:sessions:{}", username);
    let _: () = conn
        .sadd(&user_key, &session_id)
        .await
        .map_err(|e| AppError::InternalError(format!("Redis SADD error: {}", e)))?;
    let _: () = conn
        .expire(&user_key, (ttl * 60) as i64)
        .await
        .map_err(|e| AppError::InternalError(format!("Redis EXPIRE error: {}", e)))?;

    Ok(session_id)
}

pub async fn validate_session_info(
    redis_client: &redis::Client,
    session_id: &str,
    user_agent: &str,
) -> Result<Option<SessionInfo>, AppError> {
    let mut conn = redis_client
        .get_async_connection()
        .await
        .map_err(|e| AppError::InternalError(format!("Redis connection error: {}", e)))?;

    let key = format!("sso:session:{session_id}");
    let value: Option<String> = conn
        .get(&key)
        .await
        .map_err(|e| AppError::InternalError(format!("Redis GET error: {}", e)))?;

    let value = match value {
        Some(value) => value,
        None => return Ok(None),
    };

    let session: SessionData = match serde_json::from_str(&value) {
        Ok(session) => session,
        Err(_) => return Ok(None),
    };
    if session.ua_hash != hash_user_agent(user_agent) {
        return Ok(None);
    }

    Ok(Some(SessionInfo {
        username: session.username,
        created_at: session.created_at,
    }))
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
    if let Ok(Some(value)) = conn.get::<_, Option<String>>(&key).await {
        if let Ok(session) = serde_json::from_str::<SessionData>(&value) {
            let user_key = format!("sso:sessions:{}", session.username);
            let _: () = conn
                .srem(&user_key, session_id)
                .await
                .map_err(|e| AppError::InternalError(format!("Redis SREM error: {}", e)))?;
        }
    }
    let _: () = conn
        .del(&key)
        .await
        .map_err(|e| AppError::InternalError(format!("Redis DEL error: {}", e)))?;

    Ok(())
}

pub async fn revoke_all_refresh_tokens(
    redis_client: &redis::Client,
    username: &str,
) -> Result<(), AppError> {
    let mut conn = redis_client
        .get_async_connection()
        .await
        .map_err(|e| AppError::InternalError(format!("Redis connection error: {}", e)))?;
    let user_key = format!("refresh_tokens:user:{}", username);
    let tokens: Vec<String> = conn
        .smembers(&user_key)
        .await
        .map_err(|e| AppError::InternalError(format!("Redis SMEMBERS error: {}", e)))?;
    for token in tokens {
        let key = format!("refresh_token:{}", token);
        let _: () = conn
            .del(&key)
            .await
            .map_err(|e| AppError::InternalError(format!("Redis DEL error: {}", e)))?;
    }
    let _: () = conn
        .del(&user_key)
        .await
        .map_err(|e| AppError::InternalError(format!("Redis DEL error: {}", e)))?;
    Ok(())
}

pub async fn revoke_all_sessions(
    redis_client: &redis::Client,
    username: &str,
) -> Result<(), AppError> {
    let mut conn = redis_client
        .get_async_connection()
        .await
        .map_err(|e| AppError::InternalError(format!("Redis connection error: {}", e)))?;
    let user_key = format!("sso:sessions:{}", username);
    let sessions: Vec<String> = conn
        .smembers(&user_key)
        .await
        .map_err(|e| AppError::InternalError(format!("Redis SMEMBERS error: {}", e)))?;
    for session_id in sessions {
        let key = format!("sso:session:{session_id}");
        let _: () = conn
            .del(&key)
            .await
            .map_err(|e| AppError::InternalError(format!("Redis DEL error: {}", e)))?;
    }
    let _: () = conn
        .del(&user_key)
        .await
        .map_err(|e| AppError::InternalError(format!("Redis DEL error: {}", e)))?;
    Ok(())
}
