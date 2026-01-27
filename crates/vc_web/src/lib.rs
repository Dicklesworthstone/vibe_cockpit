//! vc_web - Web server and API for Vibe Cockpit
//!
//! This crate provides:
//! - axum-based HTTP server
//! - JSON API endpoints
//! - Static file serving for dashboard
//! - WebSocket support for real-time updates

use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

/// Web server errors
#[derive(Error, Debug)]
pub enum WebError {
    #[error("Server error: {0}")]
    ServerError(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Query error: {0}")]
    QueryError(#[from] vc_query::QueryError),
}

/// Shared application state
pub struct AppState {
    // Will hold store, config, etc.
}

/// Health check response
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_secs: u64,
}

/// Create the router with all routes
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/health", get(health_handler))
        .route("/api/fleet", get(fleet_handler))
        .route("/api/machines", get(machines_handler))
        .route("/api/alerts", get(alerts_handler))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// Health check endpoint
async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_secs: 0, // Will be implemented
    })
}

/// Fleet overview endpoint
async fn fleet_handler(State(_state): State<Arc<AppState>>) -> Result<Json<serde_json::Value>, StatusCode> {
    // Placeholder
    Ok(Json(serde_json::json!({
        "total_machines": 0,
        "online_machines": 0,
        "fleet_health": 1.0
    })))
}

/// Machines list endpoint
async fn machines_handler(State(_state): State<Arc<AppState>>) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    // Placeholder
    Ok(Json(vec![]))
}

/// Alerts list endpoint
async fn alerts_handler(State(_state): State<Arc<AppState>>) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    // Placeholder
    Ok(Json(vec![]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_response() {
        let resp = HealthResponse {
            status: "ok".to_string(),
            version: "0.1.0".to_string(),
            uptime_secs: 100,
        };
        assert_eq!(resp.status, "ok");
    }
}
