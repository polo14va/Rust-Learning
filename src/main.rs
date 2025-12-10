use axum::{
    routing::get,
    Router,
};
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use std::env;

// Declaración de módulos públicos
mod models;
mod error;
mod db;
mod handlers;

#[tokio::main]
async fn main() {
    // 1. Configuración
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@host.docker.internal:5432/rust_db".to_string());

    println!("Conectando a BBDD...");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Fallo al conectar a Postgres");

    // Migraciones al vuelo
    sqlx::migrate!()
        .run(&pool)
        .await
        .expect("Fallo de migración");

    // 2. Router (Wiring)
    let app = Router::new()
        .route("/", get(root))
        .route("/users", get(handlers::list_users))
        .route("/dashboard", get(handlers::get_dashboard)) // Nuevo endpoint concurrente
        .with_state(pool);

    // 3. Server
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    println!("Escuchando en http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn root() -> &'static str {
    "Rust API Refactorizada (Layered Architecture)"
}
