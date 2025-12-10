use axum::{
    extract::State,
    Json,
};
use sqlx::PgPool;
use crate::{models::User, db, error::AppError};

// El handler ahora es muy limpio. Solo orquesta la llamada a DB y el retorno.
// El tipo de retorno `Result<Json<...>, AppError>` es la clave de un manejo de errores robusto.
pub async fn list_users(State(pool): State<PgPool>) -> Result<Json<Vec<User>>, AppError> {
    let users = db::get_all_users(&pool).await?;
    Ok(Json(users))
}
