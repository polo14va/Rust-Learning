use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use tracing::error;

// Nuestro tipo de error personalizado
#[derive(Debug)]
pub enum AppError {
    DatabaseError(sqlx::Error),
    AuthError(String),
    InternalError(String),
    ValidationError(String),
}

// Permitimos usar `?` para convertir automáticamente sqlx::Error en AppError
impl From<sqlx::Error> for AppError {
    fn from(inner: sqlx::Error) -> Self {
        AppError::DatabaseError(inner)
    }
}

// Le enseñamos a Axum cómo convertir nuestro error en una respuesta HTTP
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::DatabaseError(err) => {
                error!(target: "app_error", "Database Error: {}", err);
                (StatusCode::INTERNAL_SERVER_ERROR, "Error interno de base de datos".to_string())
            },
            AppError::AuthError(msg) => {
                (StatusCode::UNAUTHORIZED, msg)
            },
            AppError::InternalError(msg) => {
                error!(target: "app_error", "Internal Error: {}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, msg)
            },
            AppError::ValidationError(msg) => {
                (StatusCode::BAD_REQUEST, msg)
            }
        };

        let body = Json(json!({
            "error": error_message,
        }));

        (status, body).into_response()
    }
}
