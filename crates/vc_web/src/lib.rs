//! vc_web - Web server and API for Vibe Cockpit
//!
//! This crate provides:
//! - axum-based HTTP server
//! - JSON API endpoints
//! - Static file serving for dashboard
//! - WebSocket support for real-time updates
//! - Token-based authentication with RBAC

pub mod auth;

use axum::{
    Router,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::get,
};
use serde::{Deserialize, Serialize};
use std::path::Path as FsPath;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::net::TcpListener;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing::warn;
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
        let mut router = create_router(self.state.clone());
        if let Some(cors) = build_cors_layer(&self.config) {
            router = router.layer(cors);
        }
        router
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

/// Maximum allowed limit for pagination to prevent DoS
const MAX_PAGINATION_LIMIT: usize = 1000;
/// Maximum allowed offset for pagination
const MAX_PAGINATION_OFFSET: usize = 1_000_000;

/// Query parameters for pagination
#[derive(Debug, Deserialize)]
pub struct PaginationParams {
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

impl PaginationParams {
    /// Get bounded limit (clamped to MAX_PAGINATION_LIMIT)
    pub fn bounded_limit(&self) -> usize {
        self.limit.min(MAX_PAGINATION_LIMIT).max(1)
    }

    /// Get bounded offset (clamped to MAX_PAGINATION_OFFSET)
    pub fn bounded_offset(&self) -> usize {
        self.offset.min(MAX_PAGINATION_OFFSET)
    }
}

fn default_limit() -> usize {
    50
}

fn build_cors_layer(config: &WebConfig) -> Option<CorsLayer> {
    if !config.cors_enabled {
        return None;
    }

    let layer = CorsLayer::new()
        .allow_methods(Any)
        .allow_headers(Any)
        .expose_headers(Any);

    if config
        .cors_origins
        .iter()
        .any(|origin| origin.trim() == "*")
    {
        return Some(layer.allow_origin(Any));
    }

    let mut origins = Vec::new();
    for origin in &config.cors_origins {
        match HeaderValue::from_str(origin) {
            Ok(value) => origins.push(value),
            Err(_) => warn!(origin = %origin, "Invalid CORS origin; skipping"),
        }
    }

    if origins.is_empty() {
        Some(layer.allow_origin(Any))
    } else {
        Some(layer.allow_origin(AllowOrigin::list(origins)))
    }
}

fn resolve_static_dir() -> Option<String> {
    if let Ok(dir) = std::env::var("VC_WEB_STATIC_DIR") {
        if FsPath::new(&dir).is_dir() {
            return Some(dir);
        }
        warn!(dir = %dir, "VC_WEB_STATIC_DIR does not exist; skipping static files");
    }

    for candidate in ["web/dist", "web", "public"] {
        if FsPath::new(candidate).is_dir() {
            return Some(candidate.to_string());
        }
    }

    None
}

/// Create the router with all routes
pub fn create_router(state: Arc<AppState>) -> Router {
    let router = Router::new()
        // Health and overview
        .route("/api/health", get(health_handler))
        .route("/api/overview", get(overview_handler))
        .route("/api/fleet", get(fleet_handler))
        // Machines
        .route("/api/machines", get(machines_handler))
        .route("/api/machines/{id}", get(machine_by_id_handler))
        .route("/api/machines/{id}/health", get(machine_health_handler))
        .route(
            "/api/machines/{id}/collectors",
            get(machine_collectors_handler),
        )
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
        // Prometheus metrics
        .route("/metrics", get(metrics_handler))
        // WebSocket
        .route("/ws", get(ws_handler))
        // Middleware
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    if let Some(dir) = resolve_static_dir() {
        router.fallback_service(ServeDir::new(dir).append_index_html_on_directories(true))
    } else {
        router
    }
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

    // Apply pagination with bounds checking
    let total = machines.len();
    let limit = params.bounded_limit();
    let offset = params.bounded_offset();
    let paginated: Vec<_> = machines.into_iter().skip(offset).take(limit).collect();

    Ok(Json(serde_json::json!({
        "machines": paginated,
        "total": total,
        "limit": limit,
        "offset": offset
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
    let limit = params.bounded_limit();
    let offset = params.bounded_offset();
    let sql = format!(
        "SELECT * FROM collector_status WHERE machine_id = '{}' ORDER BY collector_name LIMIT {} OFFSET {}",
        id.replace('\'', "''"),
        limit,
        offset
    );
    let collectors = state.store.query_json(&sql)?;

    Ok(Json(serde_json::json!({
        "machine_id": id,
        "collectors": collectors,
        "limit": limit,
        "offset": offset
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
    let limit = params.bounded_limit();
    let builder = QueryBuilder::new(&state.store);
    let alerts = builder.recent_alerts(limit)?;

    Ok(Json(serde_json::json!({
        "alerts": alerts,
        "limit": limit
    })))
}

/// Alert rules endpoint
async fn alert_rules_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<serde_json::Value>, WebError> {
    let limit = params.bounded_limit();
    let offset = params.bounded_offset();
    let sql = format!(
        "SELECT * FROM alert_rules ORDER BY rule_id LIMIT {} OFFSET {}",
        limit, offset
    );
    let rules = state.store.query_json(&sql)?;

    Ok(Json(serde_json::json!({
        "rules": rules,
        "limit": limit,
        "offset": offset
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
    let limit = params.bounded_limit();
    let offset = params.bounded_offset();
    let sql = format!(
        "SELECT * FROM agent_sessions ORDER BY collected_at DESC LIMIT {} OFFSET {}",
        limit, offset
    );
    let sessions = state.store.query_json(&sql)?;

    Ok(Json(serde_json::json!({
        "sessions": sessions,
        "limit": limit,
        "offset": offset
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
    let limit = params.bounded_limit();
    let offset = params.bounded_offset();
    let sql = format!(
        "SELECT * FROM guardian_runs ORDER BY started_at DESC LIMIT {} OFFSET {}",
        limit, offset
    );
    let runs = state.store.query_json(&sql)?;

    Ok(Json(serde_json::json!({
        "runs": runs,
        "limit": limit,
        "offset": offset
    })))
}

/// Guardian pending approvals endpoint
async fn guardian_pending_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<serde_json::Value>, WebError> {
    let limit = params.bounded_limit();
    let offset = params.bounded_offset();
    let sql = format!(
        "SELECT * FROM guardian_runs WHERE status = 'pending_approval' ORDER BY started_at DESC LIMIT {} OFFSET {}",
        limit, offset
    );
    let pending = state.store.query_json(&sql)?;

    Ok(Json(serde_json::json!({
        "pending": pending,
        "limit": limit,
        "offset": offset
    })))
}

// =============================================================================
// Prometheus Metrics Endpoint
// =============================================================================

/// Serve Prometheus-format metrics
async fn metrics_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut lines = Vec::new();

    // -- Collector freshness --
    let collectors = state
        .store
        .query_json(
            "SELECT machine_id, collector_name, \
             EXTRACT(EPOCH FROM (current_timestamp - checked_at)) AS freshness_secs \
             FROM collector_health \
             WHERE checked_at = (SELECT MAX(ch2.checked_at) FROM collector_health ch2 \
                WHERE ch2.machine_id = collector_health.machine_id \
                AND ch2.collector_name = collector_health.collector_name)"
        )
        .unwrap_or_default();

    if !collectors.is_empty() {
        lines.push("# HELP vc_collector_freshness_seconds Seconds since last collector check".to_string());
        lines.push("# TYPE vc_collector_freshness_seconds gauge".to_string());
        for c in &collectors {
            let machine = c["machine_id"].as_str().unwrap_or("unknown");
            let collector = c["collector_name"].as_str().unwrap_or("unknown");
            let secs = c["freshness_secs"].as_f64().unwrap_or(0.0);
            lines.push(format!(
                "vc_collector_freshness_seconds{{machine=\"{machine}\",collector=\"{collector}\"}} {secs:.1}"
            ));
        }
    }

    // -- Collector success total --
    let success_counts = state
        .store
        .query_json(
            "SELECT machine_id, collector_name, \
             COUNT(*) FILTER (WHERE status = 'healthy') AS success_count \
             FROM collector_health GROUP BY machine_id, collector_name"
        )
        .unwrap_or_default();

    if !success_counts.is_empty() {
        lines.push("# HELP vc_collector_success_total Total successful collector runs".to_string());
        lines.push("# TYPE vc_collector_success_total counter".to_string());
        for c in &success_counts {
            let machine = c["machine_id"].as_str().unwrap_or("unknown");
            let collector = c["collector_name"].as_str().unwrap_or("unknown");
            let count = c["success_count"].as_i64().unwrap_or(0);
            lines.push(format!(
                "vc_collector_success_total{{machine=\"{machine}\",collector=\"{collector}\"}} {count}"
            ));
        }
    }

    // -- Open alerts by severity --
    let alert_counts = state
        .store
        .query_json(
            "SELECT severity, COUNT(*) AS cnt FROM alerts \
             WHERE acked_at IS NULL GROUP BY severity"
        )
        .unwrap_or_default();

    lines.push("# HELP vc_alerts_open_total Number of open (unacknowledged) alerts".to_string());
    lines.push("# TYPE vc_alerts_open_total gauge".to_string());
    if alert_counts.is_empty() {
        lines.push("vc_alerts_open_total{severity=\"info\"} 0".to_string());
        lines.push("vc_alerts_open_total{severity=\"warning\"} 0".to_string());
        lines.push("vc_alerts_open_total{severity=\"critical\"} 0".to_string());
    } else {
        for a in &alert_counts {
            let severity = a["severity"].as_str().unwrap_or("unknown");
            let count = a["cnt"].as_i64().unwrap_or(0);
            lines.push(format!(
                "vc_alerts_open_total{{severity=\"{severity}\"}} {count}"
            ));
        }
    }

    // -- Health scores per machine --
    let health_scores = state
        .store
        .query_json(
            "SELECT machine_id, overall_score FROM health_scores \
             WHERE computed_at = (SELECT MAX(hs2.computed_at) FROM health_scores hs2 \
                WHERE hs2.machine_id = health_scores.machine_id)"
        )
        .unwrap_or_default();

    if !health_scores.is_empty() {
        lines.push("# HELP vc_health_score Machine health score (0-100)".to_string());
        lines.push("# TYPE vc_health_score gauge".to_string());
        for h in &health_scores {
            let machine = h["machine_id"].as_str().unwrap_or("unknown");
            let score = h["overall_score"].as_f64().unwrap_or(0.0);
            lines.push(format!(
                "vc_health_score{{machine=\"{machine}\"}} {score:.1}"
            ));
        }
    }

    // -- Machine count --
    let machine_count: i64 = state
        .store
        .query_scalar("SELECT COUNT(*) FROM machines")
        .unwrap_or(0);
    lines.push("# HELP vc_machines_total Total registered machines".to_string());
    lines.push("# TYPE vc_machines_total gauge".to_string());
    lines.push(format!("vc_machines_total {machine_count}"));

    // -- Uptime --
    let uptime_secs = state.start_time.elapsed().as_secs_f64();
    lines.push("# HELP vc_uptime_seconds Server uptime in seconds".to_string());
    lines.push("# TYPE vc_uptime_seconds counter".to_string());
    lines.push(format!("vc_uptime_seconds {uptime_secs:.1}"));

    // Return as text/plain (Prometheus text format)
    let body = lines.join("\n") + "\n";
    (
        [(axum::http::header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
        body,
    )
}

/// Generate Prometheus metrics text from a VcStore (for testing/reuse)
pub fn generate_metrics_text(store: &VcStore) -> String {
    let mut lines = Vec::new();

    // Alert counts
    let alert_counts = store
        .query_json(
            "SELECT severity, COUNT(*) AS cnt FROM alerts \
             WHERE acked_at IS NULL GROUP BY severity"
        )
        .unwrap_or_default();

    lines.push("# HELP vc_alerts_open_total Number of open (unacknowledged) alerts".to_string());
    lines.push("# TYPE vc_alerts_open_total gauge".to_string());
    if alert_counts.is_empty() {
        lines.push("vc_alerts_open_total{severity=\"info\"} 0".to_string());
        lines.push("vc_alerts_open_total{severity=\"warning\"} 0".to_string());
        lines.push("vc_alerts_open_total{severity=\"critical\"} 0".to_string());
    } else {
        for a in &alert_counts {
            let severity = a["severity"].as_str().unwrap_or("unknown");
            let count = a["cnt"].as_i64().unwrap_or(0);
            lines.push(format!(
                "vc_alerts_open_total{{severity=\"{severity}\"}} {count}"
            ));
        }
    }

    // Machine count
    let machine_count: i64 = store
        .query_scalar("SELECT COUNT(*) FROM machines")
        .unwrap_or(0);
    lines.push("# HELP vc_machines_total Total registered machines".to_string());
    lines.push("# TYPE vc_machines_total gauge".to_string());
    lines.push(format!("vc_machines_total {machine_count}"));

    lines.join("\n") + "\n"
}

// =============================================================================
// WebSocket Endpoint
// =============================================================================

async fn ws_handler(State(state): State<Arc<AppState>>, ws: WebSocketUpgrade) -> impl IntoResponse {
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
            .send(Message::Text(payload.to_string().into()))
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
    async fn test_overview_placeholder_values() {
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
        assert_eq!(json.total_machines, 0);
        assert_eq!(json.active_alerts, 0);
        assert_eq!(json.pending_approvals, 0);
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
    async fn test_fleet_endpoint_payload() {
        let state = test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/fleet")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("total_machines").is_some());
        assert!(json.get("fleet_health").is_some());
        assert!(json.get("active_alerts").is_some());
        assert!(json.get("pending_approvals").is_some());
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
    async fn test_machines_total_count() {
        let state = test_state();
        state
            .store
            .insert_json(
                "machines",
                &serde_json::json!({
                    "machine_id": "machine-1",
                    "hostname": "alpha-host"
                }),
            )
            .unwrap();
        state
            .store
            .insert_json(
                "machines",
                &serde_json::json!({
                    "machine_id": "machine-2",
                    "hostname": "bravo-host"
                }),
            )
            .unwrap();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/machines")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total"], 2);
    }

    #[tokio::test]
    async fn test_machines_pagination_limits_results() {
        let state = test_state();
        state
            .store
            .insert_json(
                "machines",
                &serde_json::json!({
                    "machine_id": "machine-1",
                    "hostname": "alpha-host"
                }),
            )
            .unwrap();
        state
            .store
            .insert_json(
                "machines",
                &serde_json::json!({
                    "machine_id": "machine-2",
                    "hostname": "bravo-host"
                }),
            )
            .unwrap();
        state
            .store
            .insert_json(
                "machines",
                &serde_json::json!({
                    "machine_id": "machine-3",
                    "hostname": "charlie-host"
                }),
            )
            .unwrap();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/machines?limit=1&offset=1")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let machines = json["machines"].as_array().unwrap();
        assert_eq!(machines.len(), 1);
        assert_eq!(json["total"], 3);
        assert_eq!(json["offset"], 1);
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
    async fn test_machines_ordering() {
        let state = test_state();
        state
            .store
            .insert_json(
                "machines",
                &serde_json::json!({
                    "machine_id": "machine-b",
                    "hostname": "bravo-host"
                }),
            )
            .unwrap();
        state
            .store
            .insert_json(
                "machines",
                &serde_json::json!({
                    "machine_id": "machine-a",
                    "hostname": "alpha-host"
                }),
            )
            .unwrap();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/machines?limit=2")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let machines = json["machines"].as_array().unwrap();
        assert_eq!(machines.len(), 2);
        assert_eq!(machines[0]["hostname"], "alpha-host");
        assert_eq!(machines[1]["hostname"], "bravo-host");
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
    async fn test_machine_by_id_found() {
        let state = test_state();
        state
            .store
            .insert_json(
                "machines",
                &serde_json::json!({
                    "machine_id": "machine-1",
                    "hostname": "alpha-host"
                }),
            )
            .unwrap();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/machines/machine-1")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["machine_id"], "machine-1");
        assert_eq!(json["hostname"], "alpha-host");
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
        state
            .store
            .insert_json(
                "collector_status",
                &serde_json::json!({
                    "machine_id": "machine-1",
                    "collector_name": "sysmoni",
                    "status": "ok"
                }),
            )
            .unwrap();
        state
            .store
            .insert_json(
                "collector_status",
                &serde_json::json!({
                    "machine_id": "machine-1",
                    "collector_name": "fallback",
                    "status": "ok"
                }),
            )
            .unwrap();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/machines/machine-1/collectors")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("collectors").is_some());
        let collectors = json["collectors"].as_array().unwrap();
        assert_eq!(collectors.len(), 2);
        assert_eq!(collectors[0]["collector_name"], "fallback");
        assert_eq!(collectors[1]["collector_name"], "sysmoni");
    }

    #[tokio::test]
    async fn test_machine_collectors_empty() {
        let state = test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/machines/machine-1/collectors")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let collectors = json["collectors"].as_array().unwrap();
        assert!(collectors.is_empty());
    }

    #[tokio::test]
    async fn test_machine_collectors_pagination() {
        let state = test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/machines/test-machine/collectors?limit=5&offset=2")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["limit"], 5);
        assert_eq!(json["offset"], 2);
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
    async fn test_alerts_ordering() {
        let state = test_state();
        state
            .store
            .insert_json(
                "alert_history",
                &serde_json::json!({
                    "id": 1,
                    "rule_id": "rule-1",
                    "fired_at": "2026-01-28T10:00:00Z",
                    "severity": "warning",
                    "title": "Older alert"
                }),
            )
            .unwrap();
        state
            .store
            .insert_json(
                "alert_history",
                &serde_json::json!({
                    "id": 2,
                    "rule_id": "rule-2",
                    "fired_at": "2026-01-28T12:00:00Z",
                    "severity": "critical",
                    "title": "Newer alert"
                }),
            )
            .unwrap();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/alerts?limit=2")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let alerts = json["alerts"].as_array().unwrap();
        assert_eq!(alerts.len(), 2);
        assert_eq!(alerts[0]["id"], 2);
        assert_eq!(alerts[1]["id"], 1);
    }

    #[tokio::test]
    async fn test_alerts_limit() {
        let state = test_state();
        state
            .store
            .insert_json(
                "alert_history",
                &serde_json::json!({
                    "id": 1,
                    "rule_id": "rule-1",
                    "fired_at": "2026-01-28T10:00:00Z",
                    "severity": "warning",
                    "title": "Alert 1"
                }),
            )
            .unwrap();
        state
            .store
            .insert_json(
                "alert_history",
                &serde_json::json!({
                    "id": 2,
                    "rule_id": "rule-2",
                    "fired_at": "2026-01-28T12:00:00Z",
                    "severity": "critical",
                    "title": "Alert 2"
                }),
            )
            .unwrap();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/alerts?limit=1")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let alerts = json["alerts"].as_array().unwrap();
        assert_eq!(alerts.len(), 1);
        assert_eq!(json["limit"], 1);
    }

    #[tokio::test]
    async fn test_alerts_limit_excludes_older() {
        let state = test_state();
        state
            .store
            .insert_json(
                "alert_history",
                &serde_json::json!({
                    "id": 1,
                    "rule_id": "rule-1",
                    "fired_at": "2026-01-28T09:00:00Z",
                    "severity": "warning",
                    "title": "Oldest"
                }),
            )
            .unwrap();
        state
            .store
            .insert_json(
                "alert_history",
                &serde_json::json!({
                    "id": 2,
                    "rule_id": "rule-2",
                    "fired_at": "2026-01-28T10:00:00Z",
                    "severity": "warning",
                    "title": "Older"
                }),
            )
            .unwrap();
        state
            .store
            .insert_json(
                "alert_history",
                &serde_json::json!({
                    "id": 3,
                    "rule_id": "rule-3",
                    "fired_at": "2026-01-28T12:00:00Z",
                    "severity": "critical",
                    "title": "Newest"
                }),
            )
            .unwrap();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/alerts?limit=2")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let alerts = json["alerts"].as_array().unwrap();
        assert_eq!(alerts.len(), 2);
        assert_eq!(alerts[0]["id"], 3);
        assert_eq!(alerts[1]["id"], 2);
    }
    #[tokio::test]
    async fn test_alert_rules_endpoint() {
        let state = test_state();
        state
            .store
            .insert_json(
                "alert_rules",
                &serde_json::json!({
                    "rule_id": "rule-2",
                    "name": "Memory High",
                    "severity": "critical",
                    "condition_type": "threshold",
                    "condition_config": "{\"metric\":\"mem\"}"
                }),
            )
            .unwrap();
        state
            .store
            .insert_json(
                "alert_rules",
                &serde_json::json!({
                    "rule_id": "rule-1",
                    "name": "CPU High",
                    "severity": "warning",
                    "condition_type": "threshold",
                    "condition_config": "{\"metric\":\"cpu\"}"
                }),
            )
            .unwrap();
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
        let rules = json["rules"].as_array().unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0]["rule_id"], "rule-1");
        assert_eq!(rules[1]["rule_id"], "rule-2");
    }

    #[tokio::test]
    async fn test_alert_rules_pagination() {
        let state = test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/alerts/rules?limit=3&offset=1")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["limit"], 3);
        assert_eq!(json["offset"], 1);
    }

    #[tokio::test]
    async fn test_alert_rules_ordering() {
        let state = test_state();
        state
            .store
            .insert_json(
                "alert_rules",
                &serde_json::json!({
                    "rule_id": "b-rule",
                    "name": "B Rule",
                    "severity": "warning",
                    "condition_type": "threshold",
                    "condition_config": "{\"metric\":\"cpu\"}"
                }),
            )
            .unwrap();
        state
            .store
            .insert_json(
                "alert_rules",
                &serde_json::json!({
                    "rule_id": "a-rule",
                    "name": "A Rule",
                    "severity": "warning",
                    "condition_type": "threshold",
                    "condition_config": "{\"metric\":\"cpu\"}"
                }),
            )
            .unwrap();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/alerts/rules")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let rules = json["rules"].as_array().unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0]["rule_id"], "a-rule");
        assert_eq!(rules[1]["rule_id"], "b-rule");
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
    async fn test_accounts_ordering() {
        let state = test_state();
        state
            .store
            .insert_json(
                "account_profile_snapshots",
                &serde_json::json!({
                    "machine_id": "machine-1",
                    "collected_at": "2026-01-28T10:00:00Z",
                    "provider": "openai",
                    "account_id": "acct-1",
                    "email": "older@example.com"
                }),
            )
            .unwrap();
        state
            .store
            .insert_json(
                "account_profile_snapshots",
                &serde_json::json!({
                    "machine_id": "machine-1",
                    "collected_at": "2026-01-28T12:00:00Z",
                    "provider": "openai",
                    "account_id": "acct-2",
                    "email": "newer@example.com"
                }),
            )
            .unwrap();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/accounts")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let accounts = json["accounts"].as_array().unwrap();
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0]["account_id"], "acct-2");
        assert_eq!(accounts[1]["account_id"], "acct-1");
    }

    #[tokio::test]
    async fn test_accounts_limit() {
        let state = test_state();
        state
            .store
            .insert_json(
                "account_profile_snapshots",
                &serde_json::json!({
                    "machine_id": "machine-1",
                    "collected_at": "2026-01-28T10:00:00Z",
                    "provider": "openai",
                    "account_id": "acct-1",
                    "email": "older@example.com"
                }),
            )
            .unwrap();
        state
            .store
            .insert_json(
                "account_profile_snapshots",
                &serde_json::json!({
                    "machine_id": "machine-1",
                    "collected_at": "2026-01-28T12:00:00Z",
                    "provider": "openai",
                    "account_id": "acct-2",
                    "email": "newer@example.com"
                }),
            )
            .unwrap();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/accounts")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let accounts = json["accounts"].as_array().unwrap();
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0]["account_id"], "acct-2");
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
    async fn test_sessions_ordering() {
        let state = test_state();
        state
            .store
            .insert_json(
                "agent_sessions",
                &serde_json::json!({
                    "machine_id": "machine-1",
                    "session_id": "sess-1",
                    "collected_at": "2026-01-28T10:00:00Z",
                    "program": "codex-cli"
                }),
            )
            .unwrap();
        state
            .store
            .insert_json(
                "agent_sessions",
                &serde_json::json!({
                    "machine_id": "machine-1",
                    "session_id": "sess-2",
                    "collected_at": "2026-01-28T12:00:00Z",
                    "program": "claude-code"
                }),
            )
            .unwrap();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/sessions?limit=2")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let sessions = json["sessions"].as_array().unwrap();
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0]["session_id"], "sess-2");
        assert_eq!(sessions[1]["session_id"], "sess-1");
    }

    #[tokio::test]
    async fn test_sessions_limit() {
        let state = test_state();
        state
            .store
            .insert_json(
                "agent_sessions",
                &serde_json::json!({
                    "machine_id": "machine-1",
                    "session_id": "sess-1",
                    "collected_at": "2026-01-28T10:00:00Z",
                    "program": "codex-cli"
                }),
            )
            .unwrap();
        state
            .store
            .insert_json(
                "agent_sessions",
                &serde_json::json!({
                    "machine_id": "machine-1",
                    "session_id": "sess-2",
                    "collected_at": "2026-01-28T12:00:00Z",
                    "program": "claude-code"
                }),
            )
            .unwrap();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/sessions?limit=1")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let sessions = json["sessions"].as_array().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(json["limit"], 1);
    }

    #[tokio::test]
    async fn test_sessions_offset() {
        let state = test_state();
        state
            .store
            .insert_json(
                "agent_sessions",
                &serde_json::json!({
                    "machine_id": "machine-1",
                    "session_id": "sess-1",
                    "collected_at": "2026-01-28T10:00:00Z",
                    "program": "codex-cli"
                }),
            )
            .unwrap();
        state
            .store
            .insert_json(
                "agent_sessions",
                &serde_json::json!({
                    "machine_id": "machine-1",
                    "session_id": "sess-2",
                    "collected_at": "2026-01-28T11:00:00Z",
                    "program": "claude-code"
                }),
            )
            .unwrap();
        state
            .store
            .insert_json(
                "agent_sessions",
                &serde_json::json!({
                    "machine_id": "machine-1",
                    "session_id": "sess-3",
                    "collected_at": "2026-01-28T12:00:00Z",
                    "program": "gemini-cli"
                }),
            )
            .unwrap();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/sessions?limit=1&offset=1")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let sessions = json["sessions"].as_array().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0]["session_id"], "sess-2");
        assert_eq!(json["offset"], 1);
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
    async fn test_guardian_playbooks_ordering() {
        let state = test_state();
        state
            .store
            .insert_json(
                "guardian_playbooks",
                &serde_json::json!({
                    "playbook_id": "b-playbook",
                    "name": "B Playbook",
                    "trigger_condition": "manual",
                    "steps": "[]"
                }),
            )
            .unwrap();
        state
            .store
            .insert_json(
                "guardian_playbooks",
                &serde_json::json!({
                    "playbook_id": "a-playbook",
                    "name": "A Playbook",
                    "trigger_condition": "manual",
                    "steps": "[]"
                }),
            )
            .unwrap();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/guardian/playbooks")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let playbooks = json["playbooks"].as_array().unwrap();
        assert_eq!(playbooks.len(), 2);
        assert_eq!(playbooks[0]["playbook_id"], "a-playbook");
        assert_eq!(playbooks[1]["playbook_id"], "b-playbook");
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
    async fn test_guardian_runs_ordering() {
        let state = test_state();
        state
            .store
            .insert_json(
                "guardian_runs",
                &serde_json::json!({
                    "id": 1,
                    "playbook_id": "rate-limit-switch",
                    "started_at": "2026-01-28T10:00:00Z",
                    "status": "success"
                }),
            )
            .unwrap();
        state
            .store
            .insert_json(
                "guardian_runs",
                &serde_json::json!({
                    "id": 2,
                    "playbook_id": "rate-limit-switch",
                    "started_at": "2026-01-28T12:00:00Z",
                    "status": "failed"
                }),
            )
            .unwrap();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/guardian/runs?limit=2")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let runs = json["runs"].as_array().unwrap();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0]["id"], 2);
        assert_eq!(runs[1]["id"], 1);
    }

    #[tokio::test]
    async fn test_guardian_pending_endpoint() {
        let state = test_state();
        state
            .store
            .insert_json(
                "guardian_runs",
                &serde_json::json!({
                    "id": 42,
                    "playbook_id": "rate-limit-switch",
                    "started_at": "2026-01-28T12:00:00Z",
                    "status": "pending_approval"
                }),
            )
            .unwrap();
        state
            .store
            .insert_json(
                "guardian_runs",
                &serde_json::json!({
                    "id": 43,
                    "playbook_id": "rate-limit-switch",
                    "started_at": "2026-01-28T13:00:00Z",
                    "status": "success"
                }),
            )
            .unwrap();
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
        let pending = json["pending"].as_array().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0]["id"], 42);
    }

    #[tokio::test]
    async fn test_guardian_pending_pagination() {
        let state = test_state();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/guardian/pending?limit=4&offset=2")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["limit"], 4);
        assert_eq!(json["offset"], 2);
    }

    #[tokio::test]
    async fn test_guardian_pending_limit() {
        let state = test_state();
        state
            .store
            .insert_json(
                "guardian_runs",
                &serde_json::json!({
                    "id": 41,
                    "playbook_id": "rate-limit-switch",
                    "started_at": "2026-01-28T09:00:00Z",
                    "status": "pending_approval"
                }),
            )
            .unwrap();
        state
            .store
            .insert_json(
                "guardian_runs",
                &serde_json::json!({
                    "id": 42,
                    "playbook_id": "rate-limit-switch",
                    "started_at": "2026-01-28T10:00:00Z",
                    "status": "pending_approval"
                }),
            )
            .unwrap();
        let app = create_router(state);

        let request = Request::builder()
            .uri("/api/guardian/pending?limit=1")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let pending = json["pending"].as_array().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0]["id"], 42);
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

    // =============================================================================
    // Prometheus metrics tests
    // =============================================================================

    #[tokio::test]
    async fn test_metrics_endpoint() {
        let state = Arc::new(AppState::new_memory().unwrap());
        let app = create_router(state);

        let request = Request::builder()
            .uri("/metrics")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let text = String::from_utf8(body.to_vec()).unwrap();

        // Should contain Prometheus format headers
        assert!(text.contains("# HELP"));
        assert!(text.contains("# TYPE"));
        assert!(text.contains("vc_alerts_open_total"));
        assert!(text.contains("vc_machines_total"));
        assert!(text.contains("vc_uptime_seconds"));
    }

    #[tokio::test]
    async fn test_metrics_content_type() {
        let state = Arc::new(AppState::new_memory().unwrap());
        let app = create_router(state);

        let request = Request::builder()
            .uri("/metrics")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        let content_type = response.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(content_type.contains("text/plain"));
    }

    #[tokio::test]
    async fn test_metrics_empty_db_defaults() {
        let state = Arc::new(AppState::new_memory().unwrap());
        let app = create_router(state);

        let request = Request::builder()
            .uri("/metrics")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let text = String::from_utf8(body.to_vec()).unwrap();

        // Empty DB should show zero alerts
        assert!(text.contains("vc_alerts_open_total{severity=\"info\"} 0"));
        assert!(text.contains("vc_alerts_open_total{severity=\"warning\"} 0"));
        assert!(text.contains("vc_alerts_open_total{severity=\"critical\"} 0"));
        assert!(text.contains("vc_machines_total 0"));
    }

    #[test]
    fn test_generate_metrics_text() {
        let store = VcStore::open_memory().unwrap();
        let text = generate_metrics_text(&store);

        assert!(text.contains("# HELP vc_alerts_open_total"));
        assert!(text.contains("# TYPE vc_alerts_open_total gauge"));
        assert!(text.contains("vc_machines_total 0"));
    }
}
