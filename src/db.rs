use sqlx::PgPool;
use crate::{models::User, error::AppError};

// Función pura para obtener usuarios. 
// Devuelve un Result con nuestro AppError para que el handler lo maneje limpiamente.
pub async fn get_all_users(pool: &PgPool) -> Result<Vec<User>, AppError> {
    let users = sqlx::query_as::<_, User>("SELECT id, username, email FROM users")
        .fetch_all(pool)
        .await?; // El operador '?' convierte sqlx::Error a AppError gracias al `impl From`
    
    Ok(users)
}
