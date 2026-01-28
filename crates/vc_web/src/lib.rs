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
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use proptest::prelude::*;
    use tower::ServiceExt;

    // HealthResponse tests
    #[test]
    fn test_health_response() {
        let resp = HealthResponse {
            status: "ok".to_string(),
            version: "0.1.0".to_string(),
            uptime_secs: 100,
        };
        assert_eq!(resp.status, "ok");
    }

    #[test]
    fn test_health_response_serialization() {
        let resp = HealthResponse {
            status: "healthy".to_string(),
            version: "1.0.0".to_string(),
            uptime_secs: 3600,
        };

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("healthy"));
        assert!(json.contains("3600"));

        let parsed: HealthResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.status, resp.status);
        assert_eq!(parsed.version, resp.version);
        assert_eq!(parsed.uptime_secs, resp.uptime_secs);
    }

    #[test]
    fn test_health_response_deserialization() {
        let json = r#"{"status":"ok","version":"0.2.0","uptime_secs":500}"#;
        let resp: HealthResponse = serde_json::from_str(json).unwrap();

        assert_eq!(resp.status, "ok");
        assert_eq!(resp.version, "0.2.0");
        assert_eq!(resp.uptime_secs, 500);
    }

    proptest! {
        #[test]
        fn test_health_response_roundtrip(
            status in "[a-z]{1,16}",
            version in "[0-9.]{1,12}",
            uptime_secs in 0u64..1_000_000u64
        ) {
            let resp = HealthResponse {
                status,
                version,
                uptime_secs,
            };

            let json = serde_json::to_string(&resp).unwrap();
            let parsed: HealthResponse = serde_json::from_str(&json).unwrap();

            prop_assert_eq!(parsed.status, resp.status);
            prop_assert_eq!(parsed.version, resp.version);
            prop_assert_eq!(parsed.uptime_secs, resp.uptime_secs);
        }
    }

    // WebError tests
    #[test]
    fn test_web_error_server_error() {
        let err = WebError::ServerError("internal failure".to_string());
        assert!(err.to_string().contains("Server error"));
        assert!(err.to_string().contains("internal failure"));
    }

    #[test]
    fn test_web_error_not_found() {
        let err = WebError::NotFound("resource/123".to_string());
        assert!(err.to_string().contains("Not found"));
        assert!(err.to_string().contains("resource/123"));
    }

    // AppState tests
    #[test]
    fn test_app_state_creation() {
        let _state = AppState {};
        // Just ensure it compiles and can be created
    }

    // Router tests
    #[test]
    fn test_create_router() {
        let state = Arc::new(AppState {});
        let router = create_router(state);
        // Router created successfully if we get here
        let _ = router;
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let state = Arc::new(AppState {});
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/health")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_fleet_endpoint() {
        let state = Arc::new(AppState {});
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/fleet")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_machines_endpoint() {
        let state = Arc::new(AppState {});
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/machines")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_alerts_endpoint() {
        let state = Arc::new(AppState {});
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/alerts")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_not_found_endpoint() {
        let state = Arc::new(AppState {});
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/nonexistent")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
