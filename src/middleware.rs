use axum::{
    extract::Request,
    extract::State,
    http::{header, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use crate::{auth, db, models::AppState};
use serde_json::json;
use std::collections::HashMap;
use std::env;

pub async fn auth_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    if request.method() == Method::OPTIONS {
        return next.run(request).await;
    }

    let path = request.uri().path();
    if path == "/userinfo" || path == "/userinfo/" {
        return next.run(request).await;
    }
    let requirement = match route_requirement(path) {
        Some(requirement) => requirement,
        None => return next.run(request).await,
    };

    // 1. Buscar header Authorization
    let auth_header = request.headers()
        .get(header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .unwrap_or("");

    let token = auth_header
        .strip_prefix("Bearer ")
        .unwrap_or("");
    if token.is_empty() {
        return json_error(StatusCode::UNAUTHORIZED, "Missing bearer token");
    }

    let data = match auth::validate_jwt(token, &state.keys, &state.issuer, None) {
        Ok(data) => data,
        Err(err) => {
            tracing::warn!("JWT validation failed: {}", err);
            return json_error(StatusCode::UNAUTHORIZED, "Invalid token");
        }
    };

    let allowed_aud = env::var("RESOURCE_AUDIENCE").unwrap_or_else(|_| "first-party".to_string());
    let allowed: Vec<&str> = allowed_aud
        .split(',')
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .collect();
    let aud = data.claims.aud.as_deref();
    let aud_allowed = match aud {
        Some(value) => allowed.iter().any(|a| a == &value),
        None => false,
    };
    if !aud_allowed {
        let allow_client_aud = path == "/userinfo" || path == "/dashboard";
        let client_ok = if allow_client_aud {
            if let Some(client_id) = aud {
                db::oauth_client_exists(&state.pool, client_id).await.unwrap_or(false)
            } else {
                false
            }
        } else {
            false
        };
        if !client_ok {
            return json_error(StatusCode::UNAUTHORIZED, "Invalid audience");
        }
    }

    if !requirement.scopes.is_empty()
        && !has_scopes(data.claims.scope.as_deref(), &requirement.scopes)
    {
        return json_error(StatusCode::FORBIDDEN, "Missing scope");
    }

    if !requirement.roles.is_empty()
        && !has_roles(data.claims.role.as_deref(), &requirement.roles)
    {
        return json_error(StatusCode::FORBIDDEN, "Missing role");
    }

    next.run(request).await
}

#[derive(Debug, Clone)]
struct RouteRequirement {
    scopes: Vec<String>,
    roles: Vec<String>,
}

fn route_requirement(path: &str) -> Option<RouteRequirement> {
    if let Some(requirement) = find_requirement_from_env(path) {
        return Some(requirement);
    }
    default_requirements(path)
}

fn default_requirements(path: &str) -> Option<RouteRequirement> {
    match path {
        "/dashboard" => Some(RouteRequirement {
            scopes: vec![env::var("DASHBOARD_SCOPE").unwrap_or_else(|_| "dashboard.read".to_string())],
            roles: vec!["user".to_string(), "admin".to_string()],
        }),
        "/users" => Some(RouteRequirement {
            scopes: vec!["users.read".to_string()],
            roles: vec!["admin".to_string()],
        }),
        "/userinfo" => Some(RouteRequirement {
            scopes: vec!["openid".to_string()],
            roles: vec!["user".to_string(), "admin".to_string(), "service".to_string()],
        }),
        _ => None,
    }
}

fn find_requirement_from_env(path: &str) -> Option<RouteRequirement> {
    let scopes_map = parse_map(env::var("ROUTE_SCOPE_MAP").unwrap_or_default());
    let roles_map = parse_map(env::var("ROUTE_ROLE_MAP").unwrap_or_default());
    let scopes = match find_entry(path, &scopes_map) {
        Some(values) => values,
        None => Vec::new(),
    };
    let roles = match find_entry(path, &roles_map) {
        Some(values) => values,
        None => Vec::new(),
    };
    if scopes.is_empty() && roles.is_empty() {
        return None;
    }
    Some(RouteRequirement { scopes, roles })
}

fn parse_map(value: String) -> HashMap<String, Vec<String>> {
    let mut map = HashMap::new();
    for entry in value.split(';').map(|v| v.trim()).filter(|v| !v.is_empty()) {
        let (path, scopes) = match entry.split_once('=') {
            Some(parts) => parts,
            None => continue,
        };
        let scopes = scopes
            .split(',')
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .collect::<Vec<_>>();
        map.insert(path.to_string(), scopes);
    }
    map
}

fn find_entry(path: &str, map: &HashMap<String, Vec<String>>) -> Option<Vec<String>> {
    for (key, values) in map {
        if let Some(prefix) = key.strip_suffix('*') {
            if path.starts_with(prefix) {
                return Some(values.clone());
            }
        } else if key == path {
            return Some(values.clone());
        }
    }
    None
}

fn has_scopes(claimed: Option<&str>, required: &[String]) -> bool {
    let claimed = match claimed {
        Some(value) => value,
        None => return false,
    };
    let scopes: Vec<&str> = claimed.split_whitespace().collect();
    required.iter().all(|req| scopes.iter().any(|s| s == &req))
}

fn has_roles(claimed: Option<&str>, required: &[String]) -> bool {
    let claimed = match claimed {
        Some(value) => value,
        None => return false,
    };
    let roles: Vec<&str> = claimed
        .split(|c| c == ',' || c == ' ')
        .filter(|v| !v.is_empty())
        .collect();
    required.iter().any(|req| roles.iter().any(|role| role == &req))
}

fn json_error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}
