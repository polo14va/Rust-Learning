use axum::{
    extract::Request,
    http::{StatusCode, header},
    middleware::Next,
    response::Response,
};
use crate::auth;

pub async fn auth_middleware(
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
                match auth::validate_jwt(token) {
                    Ok(_token_data) => {
                        // TODO: Podríamos inyectar el usuario en la request extensions aquí
                        Ok(next.run(request).await)
                    },
                    Err(_) => Err(StatusCode::UNAUTHORIZED),
                }
            } else {
                Err(StatusCode::UNAUTHORIZED)
            }
        },
        None => Err(StatusCode::UNAUTHORIZED),
    }
}
