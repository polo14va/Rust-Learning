// ============================================================================
// MIDDLEWARE DE MÉTRICAS
// ============================================================================
//
// Este middleware intercepta todas las requests HTTP y registra métricas
// automáticamente en Prometheus.
//
// ============================================================================

use axum::{
    body::Body,
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use std::time::Instant;

/// Middleware que registra métricas HTTP automáticamente
pub async fn metrics_middleware(
    req: Request,
    next: Next,
) -> Response {
    let start = Instant::now();
    let method = req.method().to_string();
    let path = req.uri().path().to_string();

    // Ejecutar el handler
    let response = next.run(req).await;
    
    // Registrar métricas
    let duration = start.elapsed().as_secs_f64();
    let status = response.status().as_u16();
    
    crate::metrics::record_http_request(&method, &path, status, duration);
    
    response
}
