// ============================================================================
// MÉTRICAS DE PROMETHEUS
// ============================================================================
//
// Este módulo define y expone métricas para monitoreo con Prometheus.
//
// Métricas disponibles:
// - http_requests_total: Total de requests HTTP por método, path y status
// - http_request_duration_seconds: Latencia de requests
// - cache_hits_total / cache_misses_total: Performance del cache
// - auth_attempts_total: Intentos de autenticación
// - rate_limit_exceeded_total: Rate limiting triggers
//
// ============================================================================

use lazy_static::lazy_static;
use prometheus::{
    register_counter_vec, register_histogram_vec, register_int_counter_vec, CounterVec,
    Encoder, HistogramVec, IntCounterVec, TextEncoder,
};

// ============================================================================
// DEFINICIÓN DE MÉTRICAS
// ============================================================================

lazy_static! {
    // HTTP Requests
    pub static ref HTTP_REQUESTS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "http_requests_total",
        "Total number of HTTP requests",
        &["method", "path", "status"]
    )
    .unwrap();

    // HTTP Request Duration
    pub static ref HTTP_REQUEST_DURATION: HistogramVec = register_histogram_vec!(
        "http_request_duration_seconds",
        "HTTP request latency in seconds",
        &["method", "path"],
        vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0]
    )
    .unwrap();

    // Cache Metrics
    pub static ref CACHE_HITS: IntCounterVec = register_int_counter_vec!(
        "cache_hits_total",
        "Total cache hits",
        &["cache_type"]
    )
    .unwrap();

    pub static ref CACHE_MISSES: IntCounterVec = register_int_counter_vec!(
        "cache_misses_total",
        "Total cache misses",
        &["cache_type"]
    )
    .unwrap();

    // Auth Metrics
    pub static ref AUTH_ATTEMPTS: IntCounterVec = register_int_counter_vec!(
        "auth_attempts_total",
        "Total authentication attempts",
        &["result"] // success, failure
    )
    .unwrap();

    pub static ref JWT_TOKENS_ISSUED: IntCounterVec = register_int_counter_vec!(
        "jwt_tokens_issued_total",
        "Total JWT tokens issued",
        &["token_type"] // access, refresh
    )
    .unwrap();

    // Rate Limiting
    pub static ref RATE_LIMIT_EXCEEDED: IntCounterVec = register_int_counter_vec!(
        "rate_limit_exceeded_total",
        "Total rate limit exceeded events",
        &["endpoint"]
    )
    .unwrap();

    // Database Metrics
    pub static ref DB_QUERY_DURATION: HistogramVec = register_histogram_vec!(
        "db_query_duration_seconds",
        "Database query duration in seconds",
        &["query_type"],
        vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0]
    )
    .unwrap();
}

// ============================================================================
// FUNCIONES HELPER
// ============================================================================

/// Exporta todas las métricas en formato Prometheus
pub fn export_metrics() -> Result<String, Box<dyn std::error::Error>> {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer)?;
    Ok(String::from_utf8(buffer)?)
}

/// Registra un request HTTP
pub fn record_http_request(method: &str, path: &str, status: u16, duration: f64) {
    HTTP_REQUESTS_TOTAL
        .with_label_values(&[method, path, &status.to_string()])
        .inc();
    
    HTTP_REQUEST_DURATION
        .with_label_values(&[method, path])
        .observe(duration);
}

/// Registra un cache hit
pub fn record_cache_hit(cache_type: &str) {
    CACHE_HITS.with_label_values(&[cache_type]).inc();
}

/// Registra un cache miss
pub fn record_cache_miss(cache_type: &str) {
    CACHE_MISSES.with_label_values(&[cache_type]).inc();
}

/// Registra un intento de autenticación
pub fn record_auth_attempt(success: bool) {
    let result = if success { "success" } else { "failure" };
    AUTH_ATTEMPTS.with_label_values(&[result]).inc();
}

/// Registra un token JWT emitido
pub fn record_jwt_issued(token_type: &str) {
    JWT_TOKENS_ISSUED.with_label_values(&[token_type]).inc();
}

/// Registra un rate limit excedido
pub fn record_rate_limit_exceeded(endpoint: &str) {
    RATE_LIMIT_EXCEEDED.with_label_values(&[endpoint]).inc();
}

/// Registra duración de query a DB
pub fn record_db_query(query_type: &str, duration: f64) {
    DB_QUERY_DURATION
        .with_label_values(&[query_type])
        .observe(duration);
}
