//! vc_web - Web server and API for Vibe Cockpit
//!
//! This crate provides:
//! - axum-based HTTP server
//! - JSON API endpoints
//! - Static file serving for dashboard
//! - WebSocket support for real-time updates

use axum::{
    extract::{Path, Query, State},
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use futures::SinkExt;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use vc_config::WebConfig;
use vc_query::{FleetOverview, QueryBuilder};
use vc_store::VcStore;

/// Web server errors
#[derive(Error, Debug)]
pub enum WebError {
    #[error("Server error: {0}")]
    ServerError(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Query error: {0}")]
    QueryError(#[from] vc_query::QueryError),

    #[error("Store error: {0}")]
    StoreError(#[from] vc_store::StoreError),
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            WebError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            WebError::QueryError(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            WebError::StoreError(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            WebError::ServerError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
        };

        let body = serde_json::json!({
            "error": message,
            "status": status.as_u16()
        });

        (status, Json(body)).into_response()
    }
}

/// Shared application state
pub struct AppState {
    /// Database store
    pub store: VcStore,
    /// Server start time for uptime calculation
    pub start_time: Instant,
}

impl AppState {
    /// Create new app state with the given store
    pub fn new(store: VcStore) -> Self {
        Self {
            store,
            start_time: Instant::now(),
        }
    }

    /// Create app state with in-memory store for testing
    pub fn new_memory() -> Result<Self, vc_store::StoreError> {
        Ok(Self::new(VcStore::open_memory()?))
    }
}

pub struct WebServer {
    state: Arc<AppState>,
    config: WebConfig,
}

impl WebServer {
    pub fn new(store: VcStore, config: WebConfig) -> Self {
        Self {
            state: Arc::new(AppState::new(store)),
            config,
        }
    }

    pub fn router(&self) -> Router {
        create_router(self.state.clone())
    }

    pub async fn run(&self) -> Result<(), WebError> {
        let addr = format!("{}:{}", self.config.bind_address, self.config.port);
        let listener = TcpListener::bind(&addr)
            .await
            .map_err(|err| WebError::ServerError(err.to_string()))?;
        tracing::info!(%addr, "Starting vc_web server");
        axum::serve(listener, self.router())
            .with_graceful_shutdown(shutdown_signal())
            .await
            .map_err(|err| WebError::ServerError(err.to_string()))?;
        Ok(())
    }
}

async fn shutdown_signal() {
    if tokio::signal::ctrl_c().await.is_ok() {
        tracing::info!("Shutdown signal received");
    }
}

/// Health check response
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_secs: u64,
}

/// Query parameters for pagination
#[derive(Debug, Deserialize)]
pub struct PaginationParams {
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

fn default_limit() -> usize {
    50
}

/// Create the router with all routes
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        // Health and overview
        .route("/api/health", get(health_handler))
        .route("/api/overview", get(overview_handler))
        .route("/api/fleet", get(fleet_handler))
        // Machines
        .route("/api/machines", get(machines_handler))
        .route("/api/machines/{id}", get(machine_by_id_handler))
        .route("/api/machines/{id}/health", get(machine_health_handler))
        .route("/api/machines/{id}/collectors", get(machine_collectors_handler))
        // Alerts
        .route("/api/alerts", get(alerts_handler))
        .route("/api/alerts/rules", get(alert_rules_handler))
        // Accounts
        .route("/api/accounts", get(accounts_handler))
        // Sessions
        .route("/api/sessions", get(sessions_handler))
        // Guardian
        .route("/api/guardian/playbooks", get(guardian_playbooks_handler))
        .route("/api/guardian/runs", get(guardian_runs_handler))
        .route("/api/guardian/pending", get(guardian_pending_handler))
        // WebSocket
        .route("/ws", get(ws_handler))
        // Middleware
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}

// =============================================================================
// Health & Overview Endpoints
// =============================================================================

/// Health check endpoint
async fn health_handler(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_secs: state.start_time.elapsed().as_secs(),
    })
}

/// Fleet overview endpoint - returns FleetOverview from vc_query
async fn overview_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<FleetOverview>, WebError> {
    let builder = QueryBuilder::new(&state.store);
    let overview = builder.fleet_overview()?;
    Ok(Json(overview))
}

/// Fleet handler (alias for overview, returns JSON object)
async fn fleet_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, WebError> {
    let builder = QueryBuilder::new(&state.store);
    let overview = builder.fleet_overview()?;
    Ok(Json(serde_json::json!({
        "total_machines": overview.total_machines,
        "online_machines": overview.online_machines,
        "offline_machines": overview.offline_machines,
        "fleet_health": overview.fleet_health_score,
        "active_alerts": overview.active_alerts,
        "pending_approvals": overview.pending_approvals
    })))
}

// =============================================================================
// Machines Endpoints
// =============================================================================

/// Machines list endpoint
async fn machines_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<serde_json::Value>, WebError> {
    let builder = QueryBuilder::new(&state.store);
    let machines = builder.machines()?;

    // Apply pagination
    let total = machines.len();
    let paginated: Vec<_> = machines
        .into_iter()
        .skip(params.offset)
        .take(params.limit)
        .collect();

    Ok(Json(serde_json::json!({
        "machines": paginated,
        "total": total,
        "limit": params.limit,
        "offset": params.offset
    })))
}

/// Get machine by ID
async fn machine_by_id_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    let sql = format!(
        "SELECT * FROM machines WHERE machine_id = '{}' LIMIT 1",
        id.replace('\'', "''")
    );
    let results = state.store.query_json(&sql)?;

    if let Some(machine) = results.into_iter().next() {
        Ok(Json(machine))
    } else {
        Err(WebError::NotFound(format!("Machine not found: {}", id)))
    }
}

/// Get machine health
async fn machine_health_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<vc_query::HealthScore>, WebError> {
    let builder = QueryBuilder::new(&state.store);
    let health = builder.machine_health(&id)?;
    Ok(Json(health))
}

/// Get collector status for a machine
async fn machine_collectors_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<serde_json::Value>, WebError> {
    let sql = format!(
        "SELECT * FROM collector_status WHERE machine_id = '{}' ORDER BY collector_name LIMIT {} OFFSET {}",
        id.replace('\'', "''"),
        params.limit,
        params.offset
    );
    let collectors = state.store.query_json(&sql)?;

    Ok(Json(serde_json::json!({
        "machine_id": id,
        "collectors": collectors,
        "limit": params.limit,
        "offset": params.offset
    })))
}

// =============================================================================
// Alerts Endpoints
// =============================================================================

/// Alerts list endpoint
async fn alerts_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<serde_json::Value>, WebError> {
    let builder = QueryBuilder::new(&state.store);
    let alerts = builder.recent_alerts(params.limit)?;

    Ok(Json(serde_json::json!({
        "alerts": alerts,
        "limit": params.limit
    })))
}

/// Alert rules endpoint
async fn alert_rules_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<serde_json::Value>, WebError> {
    let sql = format!(
        "SELECT * FROM alert_rules ORDER BY rule_id LIMIT {} OFFSET {}",
        params.limit, params.offset
    );
    let rules = state.store.query_json(&sql)?;

    Ok(Json(serde_json::json!({
        "rules": rules,
        "limit": params.limit,
        "offset": params.offset
    })))
}

// =============================================================================
// Accounts Endpoints
// =============================================================================

/// Accounts list endpoint
async fn accounts_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, WebError> {
    let sql = "SELECT * FROM account_profile_snapshots ORDER BY collected_at DESC LIMIT 100";
    let accounts = state.store.query_json(sql)?;

    Ok(Json(serde_json::json!({
        "accounts": accounts
    })))
}

// =============================================================================
// Sessions Endpoints
// =============================================================================

/// Sessions list endpoint
async fn sessions_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<serde_json::Value>, WebError> {
    let sql = format!(
        "SELECT * FROM agent_sessions ORDER BY collected_at DESC LIMIT {} OFFSET {}",
        params.limit, params.offset
    );
    let sessions = state.store.query_json(&sql)?;

    Ok(Json(serde_json::json!({
        "sessions": sessions,
        "limit": params.limit,
        "offset": params.offset
    })))
}

// =============================================================================
// Guardian Endpoints
// =============================================================================

/// Guardian playbooks endpoint
async fn guardian_playbooks_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, WebError> {
    let sql = "SELECT * FROM guardian_playbooks ORDER BY playbook_id";
    let playbooks = state.store.query_json(sql)?;

    Ok(Json(serde_json::json!({
        "playbooks": playbooks
    })))
}

/// Guardian runs endpoint
async fn guardian_runs_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<serde_json::Value>, WebError> {
    let sql = format!(
        "SELECT * FROM guardian_runs ORDER BY started_at DESC LIMIT {} OFFSET {}",
        params.limit, params.offset
    );
    let runs = state.store.query_json(&sql)?;

    Ok(Json(serde_json::json!({
        "runs": runs,
        "limit": params.limit,
        "offset": params.offset
    })))
}

/// Guardian pending approvals endpoint
async fn guardian_pending_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<serde_json::Value>, WebError> {
    let sql = format!(
        "SELECT * FROM guardian_runs WHERE status = 'pending_approval' ORDER BY started_at DESC LIMIT {} OFFSET {}",
        params.limit, params.offset
    );
    let pending = state.store.query_json(&sql)?;

    Ok(Json(serde_json::json!({
        "pending": pending,
        "limit": params.limit,
        "offset": params.offset
    })))
}

// =============================================================================
// WebSocket Endpoint
// =============================================================================

async fn ws_handler(
    State(state): State<Arc<AppState>>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| ws_session(socket, state))
}

async fn ws_session(mut socket: WebSocket, state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    loop {
        interval.tick().await;
        let payload = serde_json::json!({
            "type": "heartbeat",
            "uptime_secs": state.start_time.elapsed().as_secs()
        });
        if socket
            .send(Message::Text(payload.to_string()))
            .await
            .is_err()
        {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use proptest::prelude::*;
    use tower::ServiceExt;

    /// Helper to create test app state with in-memory store
    fn test_state() -> Arc<AppState> {
        Arc::new(AppState::new_memory().unwrap())
    }

    // ==========================================================================
    // HealthResponse tests
    // ==========================================================================

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

    // ==========================================================================
    // WebError tests
    // ==========================================================================

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

    #[tokio::test]
    async fn test_web_error_into_response_not_found() {
        let err = WebError::NotFound("missing".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_web_error_into_response_server_error() {
        let err = WebError::ServerError("crashed".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    // ==========================================================================
    // AppState tests
    // ==========================================================================

    #[test]
    fn test_app_state_creation() {
        let state = AppState::new_memory().unwrap();
        assert!(state.start_time.elapsed().as_secs() < 1);
    }

    #[test]
    fn test_app_state_uptime() {
        let state = AppState::new_memory().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(state.start_time.elapsed().as_millis() >= 10);
    }

    // ==========================================================================
    // PaginationParams tests
    // ==========================================================================

    #[test]
    fn test_pagination_defaults() {
        let params: PaginationParams = serde_json::from_str("{}").unwrap();
        assert_eq!(params.limit, 50);
        assert_eq!(params.offset, 0);
    }

    #[test]
    fn test_pagination_custom() {
        let params: PaginationParams =
            serde_json::from_str(r#"{"limit": 10, "offset": 20}"#).unwrap();
        assert_eq!(params.limit, 10);
        assert_eq!(params.offset, 20);
    }

    // ==========================================================================
    // Router tests
    // ==========================================================================

    #[test]
    fn test_create_router() {
        let state = test_state();
        let router = create_router(state);
        let _ = router;
    }

    // ==========================================================================
    // Endpoint tests
    // ==========================================================================

    #[tokio::test]
    async fn test_health_endpoint() {
        let state = test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/health")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: HealthResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.status, "ok");
    }

    #[tokio::test]
    async fn test_overview_endpoint() {
        let state = test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/overview")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: FleetOverview = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.fleet_health_score, 1.0);
    }

    #[tokio::test]
    async fn test_fleet_endpoint() {
        let state = test_state();
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
        let state = test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/machines")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("machines").is_some());
        assert!(json.get("total").is_some());
    }

    #[tokio::test]
    async fn test_machines_pagination() {
        let state = test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/machines?limit=10&offset=5")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["limit"], 10);
        assert_eq!(json["offset"], 5);
    }

    #[tokio::test]
    async fn test_machine_by_id_not_found() {
        let state = test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/machines/nonexistent-machine")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_machine_health_endpoint() {
        let state = test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/machines/test-machine/health")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_machine_collectors_endpoint() {
        let state = test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/machines/test-machine/collectors")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("collectors").is_some());
    }

    #[tokio::test]
    async fn test_alerts_endpoint() {
        let state = test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/alerts")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("alerts").is_some());
    }

    #[tokio::test]
    async fn test_alert_rules_endpoint() {
        let state = test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/alerts/rules")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("rules").is_some());
    }

    #[tokio::test]
    async fn test_accounts_endpoint() {
        let state = test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/accounts")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("accounts").is_some());
    }

    #[tokio::test]
    async fn test_sessions_endpoint() {
        let state = test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/sessions")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("sessions").is_some());
    }

    #[tokio::test]
    async fn test_guardian_playbooks_endpoint() {
        let state = test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/guardian/playbooks")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("playbooks").is_some());
    }

    #[tokio::test]
    async fn test_guardian_runs_endpoint() {
        let state = test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/guardian/runs")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("runs").is_some());
    }

    #[tokio::test]
    async fn test_guardian_pending_endpoint() {
        let state = test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/guardian/pending")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("pending").is_some());
    }

    #[tokio::test]
    async fn test_not_found_endpoint() {
        let state = test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/nonexistent")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
