use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

// Nuestro tipo de error personalizado
#[derive(Debug)]
pub enum AppError {
    DatabaseError(sqlx::Error),
    // Aquí puedes añadir más tipos: NotFound, ValidationError, etc.
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
                // En un entorno real, loguearíamos el error detallado internamente
                // y mostraríamos un mensaje genérico al usuario.
                println!("Database Error: {}", err);
                (StatusCode::INTERNAL_SERVER_ERROR, "Error interno de base de datos")
            }
        };

        let body = Json(json!({
            "error": error_message,
        }));

        (status, body).into_response()
    }
}
