use axum::{
    routing::{get, post},
    Router,
    middleware as axum_middleware,
};
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use std::env;
use crate::models::AppState;  // Importamos el struct AppState
use tower_http::cors::CorsLayer;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::services::ServeDir;
use axum::http::{header, HeaderName, HeaderValue, Method};

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
mod oauth;
mod templates;
mod email;

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

    // Registrar cliente interno first-party para refrescos y SSO
    db::ensure_client_exists(
        &pool,
        "first-party",
        "first-secret",
        "http://localhost:3000/callback",
        "openid profile email offline_access dashboard.read",
        "authorization_code,refresh_token,client_credentials",
        "First Party Internal Client",
    )
    .await
    .expect("No se pudo asegurar el cliente first-party");

    // 2. Conectar a Redis
    let redis_url = env::var("REDIS_URL")
        .unwrap_or_else(|_| "redis://host.docker.internal/".to_string());

    tracing::info!("Connecting to Redis at: {}", redis_url);
    let redis_client = redis::Client::open(redis_url.as_str()).expect("Error creando cliente Redis");

    // 2. Cargar claves JWT (RSA) y issuer
    let keys = auth::load_jwt_keys().expect("Failed to load JWT keys");
    let issuer = env::var("OIDC_ISSUER").unwrap_or_else(|_| "http://localhost:3000".to_string());

    let shared_state = AppState {
        pool,
        redis_client,
        keys,
        issuer,
    };

    // 3. Router
    let cors = CorsLayer::new()
        .allow_origin([
            HeaderValue::from_static("http://localhost:8000"),
            HeaderValue::from_static("http://127.0.0.1:8000"),
        ])
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([
            HeaderName::from_static("content-type"),
            HeaderName::from_static("authorization"),
        ])
        .allow_credentials(true);

    let frame_policy = SetResponseHeaderLayer::if_not_present(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_str(&format!(
            "frame-ancestors {}",
            env::var("FRAME_ANCESTORS")
                .unwrap_or_else(|_| "'self' http://localhost:8000 http://127.0.0.1:8000".to_string())
        ))
        .unwrap_or_else(|_| HeaderValue::from_static("frame-ancestors 'self'")),
    );
    let referrer_policy = SetResponseHeaderLayer::if_not_present(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_str(
            &env::var("REFERRER_POLICY")
                .unwrap_or_else(|_| "strict-origin-when-cross-origin".to_string()),
        )
        .unwrap_or_else(|_| HeaderValue::from_static("no-referrer")),
    );
    let permissions_policy = SetResponseHeaderLayer::if_not_present(
        HeaderName::from_static("permissions-policy"),
        HeaderValue::from_str(
            &env::var("PERMISSIONS_POLICY").unwrap_or_else(|_| {
                "geolocation=(), microphone=(), camera=(), payment=(), interest-cohort=()".to_string()
            }),
        )
        .unwrap_or_else(|_| HeaderValue::from_static("geolocation=()")),
    );
    let content_type_options = SetResponseHeaderLayer::if_not_present(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    let hsts_enabled = env::var("ENABLE_HSTS")
        .map(|v| v == "true")
        .unwrap_or(false);
    let hsts_value = format!(
        "max-age={}; includeSubDomains; preload",
        env::var("HSTS_MAX_AGE").unwrap_or_else(|_| "63072000".to_string())
    );
    let hsts_layer = SetResponseHeaderLayer::if_not_present(
        header::STRICT_TRANSPORT_SECURITY,
        HeaderValue::from_str(&hsts_value).unwrap_or_else(|_| HeaderValue::from_static("max-age=63072000")),
    );

    let app = Router::new()
        .nest_service(
            "/demo",
            ServeDir::new("web-demo").append_index_html_on_directories(true),
        )
        .route("/", get(root))
        .route("/health", get(health::health_check))
        .route("/metrics", get(metrics_handler))  // Endpoint de métricas
        .route("/users", get(handlers::list_users))
        .route("/dashboard", get(handlers::get_dashboard).options(oauth::options_ok))
        .route("/login", get(oauth::login_page).post(handlers::login).options(oauth::options_ok))
        .route("/login/", get(oauth::login_page).options(oauth::options_ok)) // soporte trailing slash
        .route("/register", post(handlers::register))
        .route("/refresh", post(handlers::refresh))
        .route("/logout", post(handlers::logout))
        .route("/logout/all", post(handlers::logout_all))
        // OAuth2 / OIDC
        .route("/authorize", get(oauth::authorize))
        .route("/token", post(oauth::token))
        .route("/introspect", post(oauth::introspect))
        .route("/revoke", post(oauth::revoke))
        .route("/userinfo", get(oauth::userinfo).options(oauth::options_ok))
        .route("/consent", get(oauth::consent_page).post(oauth::submit_consent).options(oauth::options_ok))
        .route("/consent/", get(oauth::consent_page).options(oauth::options_ok))
        .route("/.well-known/openid-configuration", get(oauth::openid_configuration))
        .route("/.well-known/jwks.json", get(oauth::jwks))
        .layer(axum::middleware::from_fn_with_state(shared_state.clone(), middleware::auth_middleware))
        .layer(axum_middleware::from_fn(metrics_middleware::metrics_middleware))  // Métricas automáticas
        .layer(content_type_options)
        .layer(permissions_policy)
        .layer(referrer_policy)
        .layer(frame_policy)
        .layer(cors)
        .with_state(shared_state);
    let app = if hsts_enabled { app.layer(hsts_layer) } else { app };

    // 4. Server
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::info!("Server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .unwrap();
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
