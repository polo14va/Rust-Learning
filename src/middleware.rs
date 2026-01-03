use axum::{
    extract::Request,
    extract::State,
    http::{StatusCode, header},
    middleware::Next,
    response::Response,
};
use axum_extra::extract::cookie::CookieJar;
use crate::{auth, models::AppState};

pub async fn auth_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // 1. Buscar header Authorization
    let auth_header = request.headers()
        .get(header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok());

    match auth_header {
        Some(auth_header) => {
            // Esperamos formato "Bearer <token>"
            if let Some(token) = auth_header.strip_prefix("Bearer ") {
                // 2. Validar Token
                match auth::validate_jwt(token, &state.keys) {
                    Ok(_token_data) => {
                        // TODO: Podríamos inyectar el usuario en la request extensions aquí
                        Ok(next.run(request).await)
                    },
                    Err(err) => {
                        tracing::warn!("JWT validation failed: {}", err);
                        let jar = CookieJar::from_headers(request.headers());
                        if let Some(cookie) = jar.get("sso_session") {
                            match auth::validate_session(&state.redis_client, cookie.value()).await {
                                Ok(Some(_username)) => Ok(next.run(request).await),
                                _ => Err(StatusCode::UNAUTHORIZED),
                            }
                        } else {
                            Err(StatusCode::UNAUTHORIZED)
                        }
                    },
                }
            } else {
                Err(StatusCode::UNAUTHORIZED)
            }
        },
        None => {
            let jar = CookieJar::from_headers(request.headers());
            if let Some(cookie) = jar.get("sso_session") {
                match auth::validate_session(&state.redis_client, cookie.value()).await {
                    Ok(Some(_username)) => Ok(next.run(request).await),
                    _ => Err(StatusCode::UNAUTHORIZED),
                }
            } else {
                Err(StatusCode::UNAUTHORIZED)
            }
        },
    }
}
