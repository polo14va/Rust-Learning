use axum::{
    routing::{get, post},
    Router,
    middleware as axum_middleware,
};
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use std::env;
use crate::models::AppState;  // Importamos el struct AppState

mod models;
mod error;
mod db;
mod handlers;
mod cache;
mod auth;
mod middleware;
mod health;
mod rate_limit;
mod builders;
mod metrics;  // Métricas de Prometheus
mod metrics_middleware;  // Middleware de métricas HTTP

#[tokio::main]
async fn main() {
    // 0. Cargar variables de entorno y configurar logging
    dotenvy::dotenv().ok();
    
    tracing_subscriber::fmt()
        .with_target(false)
        .compact()
        .init();

    tracing::info!("Starting Rust API...");

    // 1. Configuración BBDD
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@host.docker.internal:5432/rust_db".to_string());

    tracing::info!("Connecting to database...");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Fallo al conectar a Postgres");

    // Migraciones
    sqlx::migrate!()
        .run(&pool)
        .await
        .expect("Fallo de migración");

    // 2. Conectar a Redis
    let redis_url = env::var("REDIS_URL")
        .unwrap_or_else(|_| "redis://host.docker.internal/".to_string());
    
    tracing::info!("Connecting to Redis at: {}", redis_url);
    let redis_client = redis::Client::open(redis_url.as_str()).expect("Error creando cliente Redis");

    let shared_state = AppState {
        pool,
        redis_client,
    };

    // 3. Router
    let protected_routes = Router::new()
        .route("/dashboard", get(handlers::get_dashboard))
        .layer(axum::middleware::from_fn_with_state(shared_state.clone(), middleware::auth_middleware));

    let app = Router::new()
        .merge(protected_routes)
        .route("/", get(root))
        .route("/health", get(health::health_check))
        .route("/metrics", get(metrics_handler))  // Endpoint de métricas
        .route("/users", get(handlers::list_users))
        .route("/login", post(handlers::login))
        .route("/register", post(handlers::register))
        .route("/refresh", post(handlers::refresh))
        .layer(axum_middleware::from_fn(metrics_middleware::metrics_middleware))  // Métricas automáticas
        .route("/logout", post(handlers::logout))
        .with_state(shared_state);

    // 4. Server
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::info!("Server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn root() -> &'static str {
    "Rust API Advanced (Caching Implemented)"
}

// Handler para exponer métricas de Prometheus
async fn metrics_handler() -> Result<String, (axum::http::StatusCode, String)> {
    metrics::export_metrics()
        .map_err(|e| {
            tracing::error!("Failed to export metrics: {}", e);
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to export metrics: {}", e),
            )
        })
}
