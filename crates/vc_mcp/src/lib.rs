//! `vc_mcp` - MCP server for Vibe Cockpit
//!
//! Implements the Model Context Protocol (MCP) as a JSON-RPC 2.0 server
//! over stdio. Exposes `vibe_cockpit` data and actions as MCP tools and
//! resources for external AI agent access.
//!
//! ## Tools
//! - `vc_fleet_status` - Fleet overview with machine counts and health
//! - `vc_query_machines` - List machines with optional filters
//! - `vc_query_alerts` - List active and recent alerts
//! - `vc_query_sessions` - Search session history
//! - `vc_query_incidents` - List incidents
//! - `vc_query_nl` - Natural language query interface
//! - `vc_collector_status` - Collector health status
//! - `vc_playbook_drafts` - List pending playbook drafts
//! - `vc_audit_log` - Recent audit events
//!
//! ## Resources
//! - `vc://fleet/overview` - Fleet status snapshot
//! - `vc://machines` - Machine list
//!
//! ## Transport
//! JSON-RPC 2.0 over stdin/stdout (standard MCP transport)

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tracing::debug;
use vc_store::VcStore;

// ============================================================================
// Error types
// ============================================================================

/// MCP server errors
#[derive(Error, Debug)]
pub enum McpError {
    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Execution error: {0}")]
    ExecutionError(String),

    #[error("Store error: {0}")]
    StoreError(#[from] vc_store::StoreError),

    #[error("Query error: {0}")]
    QueryError(#[from] vc_query::QueryError),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
}

// ============================================================================
// MCP protocol types
// ============================================================================

/// MCP tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

/// MCP resource definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource {
    pub uri: String,
    pub name: String,
    pub description: String,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
}

/// MCP tool result content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

/// MCP tool call result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub content: Vec<ToolContent>,
    #[serde(rename = "isError", skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// JSON-RPC 2.0 request
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// JSON-RPC 2.0 response
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 error
#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
}

// ============================================================================
// Server capability info
// ============================================================================

/// Server info returned during initialization
#[derive(Debug, Serialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

/// Server capabilities
#[derive(Debug, Serialize)]
pub struct ServerCapabilities {
    pub tools: serde_json::Value,
    pub resources: serde_json::Value,
}

// ============================================================================
// MCP Server
// ============================================================================

/// MCP server implementation backed by `VcStore`
pub struct McpServer {
    store: Arc<VcStore>,
    tools: Vec<McpTool>,
    resources: Vec<McpResource>,
}

impl McpServer {
    /// Create a new MCP server with a `VcStore` backend.
    #[must_use]
    pub fn new(store: Arc<VcStore>) -> Self {
        Self {
            store,
            tools: Self::define_tools(),
            resources: Self::define_resources(),
        }
    }

    /// Define available tools
    #[allow(clippy::too_many_lines)]
    fn define_tools() -> Vec<McpTool> {
        vec![
            McpTool {
                name: "vc_fleet_status".to_string(),
                description: "Get current fleet status including machine count, health scores, and online/offline breakdown".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "machine": {
                            "type": "string",
                            "description": "Optional machine ID to filter by"
                        }
                    }
                }),
            },
            McpTool {
                name: "vc_query_machines".to_string(),
                description: "List machines with optional status filter".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "status": {
                            "type": "string",
                            "description": "Filter by status (online, offline, degraded)"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum results (default 50)"
                        }
                    }
                }),
            },
            McpTool {
                name: "vc_query_alerts".to_string(),
                description: "List active alerts with optional severity filter".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "severity": {
                            "type": "string",
                            "enum": ["info", "warning", "critical"],
                            "description": "Filter by severity level"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum results (default 50)"
                        }
                    }
                }),
            },
            McpTool {
                name: "vc_query_sessions".to_string(),
                description: "Search agent session history".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "machine": {
                            "type": "string",
                            "description": "Filter by machine ID"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum results (default 50)"
                        }
                    }
                }),
            },
            McpTool {
                name: "vc_query_incidents".to_string(),
                description: "List incidents with optional status filter".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "status": {
                            "type": "string",
                            "description": "Filter by status (open, resolved, closed)"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum results (default 50)"
                        }
                    }
                }),
            },
            McpTool {
                name: "vc_query_nl".to_string(),
                description: "Ask a natural language question about the fleet".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "question": {
                            "type": "string",
                            "description": "Natural language question (e.g. 'how many critical alerts are there?')"
                        }
                    },
                    "required": ["question"]
                }),
            },
            McpTool {
                name: "vc_collector_status".to_string(),
                description: "Get collector health status".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "limit": {
                            "type": "integer",
                            "description": "Maximum results (default 50)"
                        }
                    }
                }),
            },
            McpTool {
                name: "vc_playbook_drafts".to_string(),
                description: "List pending playbook drafts for review".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "status": {
                            "type": "string",
                            "description": "Filter by status (pending_review, approved, rejected, activated)"
                        }
                    }
                }),
            },
            McpTool {
                name: "vc_audit_log".to_string(),
                description: "Get recent audit events".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "limit": {
                            "type": "integer",
                            "description": "Maximum results (default 50)"
                        }
                    }
                }),
            },
        ]
    }

    /// Define available resources
    fn define_resources() -> Vec<McpResource> {
        vec![
            McpResource {
                uri: "vc://fleet/overview".to_string(),
                name: "Fleet Overview".to_string(),
                description: "Current fleet status and health summary".to_string(),
                mime_type: "application/json".to_string(),
            },
            McpResource {
                uri: "vc://machines".to_string(),
                name: "Machine List".to_string(),
                description: "All registered machines".to_string(),
                mime_type: "application/json".to_string(),
            },
        ]
    }

    /// List available tools
    #[must_use]
    pub fn list_tools(&self) -> &[McpTool] {
        &self.tools
    }

    /// List available resources
    #[must_use]
    pub fn list_resources(&self) -> &[McpResource] {
        &self.resources
    }

    /// Execute a tool call
    ///
    /// # Errors
    ///
    /// Returns [`McpError::ToolNotFound`] when `name` is unknown.
    pub fn call_tool(&self, name: &str, args: &serde_json::Value) -> Result<ToolResult, McpError> {
        debug!(tool = name, "Executing MCP tool");

        let result = match name {
            "vc_fleet_status" => self.tool_fleet_status(args),
            "vc_query_machines" => self.tool_query_machines(args),
            "vc_query_alerts" => self.tool_query_alerts(args),
            "vc_query_sessions" => self.tool_query_sessions(args),
            "vc_query_incidents" => self.tool_query_incidents(args),
            "vc_query_nl" => self.tool_query_nl(args),
            "vc_collector_status" => self.tool_collector_status(args),
            "vc_playbook_drafts" => self.tool_playbook_drafts(args),
            "vc_audit_log" => self.tool_audit_log(args),
            _ => return Err(McpError::ToolNotFound(name.to_string())),
        };

        match result {
            Ok(value) => Ok(ToolResult {
                content: vec![ToolContent {
                    content_type: "text".to_string(),
                    text: serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string()),
                }],
                is_error: None,
            }),
            Err(e) => Ok(ToolResult {
                content: vec![ToolContent {
                    content_type: "text".to_string(),
                    text: format!("Error: {e}"),
                }],
                is_error: Some(true),
            }),
        }
    }

    /// Read a resource
    ///
    /// # Errors
    ///
    /// Returns [`McpError::InvalidRequest`] when `uri` is unknown.
    pub fn read_resource(&self, uri: &str) -> Result<serde_json::Value, McpError> {
        debug!(uri, "Reading MCP resource");

        match uri {
            "vc://fleet/overview" => self.tool_fleet_status(&serde_json::json!({})),
            "vc://machines" => self.tool_query_machines(&serde_json::json!({})),
            _ => Err(McpError::InvalidRequest(format!("Unknown resource: {uri}"))),
        }
    }

    // ========================================================================
    // Tool implementations
    // ========================================================================

    #[allow(clippy::unnecessary_wraps)]
    fn tool_fleet_status(&self, args: &serde_json::Value) -> Result<serde_json::Value, McpError> {
        let machine_filter = args.get("machine").and_then(|v| v.as_str());

        let sql = if let Some(machine) = machine_filter {
            format!(
                "SELECT machine_id, hostname, status, last_seen, health_score \
                 FROM machines WHERE machine_id = '{machine}' \
                 ORDER BY hostname LIMIT 50"
            )
        } else {
            "SELECT machine_id, hostname, status, last_seen, health_score \
             FROM machines ORDER BY hostname LIMIT 100"
                .to_string()
        };

        let machines = self.store.query_json(&sql).unwrap_or_default();
        let total = machines.len();
        let online = machines
            .iter()
            .filter(|m| m.get("status").and_then(|s| s.as_str()) == Some("online"))
            .count();

        Ok(serde_json::json!({
            "total_machines": total,
            "online": online,
            "offline": total - online,
            "machines": machines
        }))
    }

    #[allow(clippy::unnecessary_wraps)]
    fn tool_query_machines(&self, args: &serde_json::Value) -> Result<serde_json::Value, McpError> {
        let limit = args
            .get("limit")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(50);
        let status = args.get("status").and_then(|v| v.as_str());

        let sql = if let Some(status) = status {
            format!(
                "SELECT * FROM machines WHERE status = '{status}' \
                 ORDER BY hostname LIMIT {limit}"
            )
        } else {
            format!("SELECT * FROM machines ORDER BY hostname LIMIT {limit}")
        };

        let machines = self.store.query_json(&sql).unwrap_or_default();
        Ok(serde_json::json!({ "machines": machines, "count": machines.len() }))
    }

    #[allow(clippy::unnecessary_wraps)]
    fn tool_query_alerts(&self, args: &serde_json::Value) -> Result<serde_json::Value, McpError> {
        let limit = args
            .get("limit")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(50);
        let severity = args.get("severity").and_then(|v| v.as_str());

        let sql = if let Some(severity) = severity {
            format!(
                "SELECT * FROM alerts WHERE severity = '{severity}' \
                 ORDER BY fired_at DESC LIMIT {limit}"
            )
        } else {
            format!("SELECT * FROM alerts ORDER BY fired_at DESC LIMIT {limit}")
        };

        let alerts = self.store.query_json(&sql).unwrap_or_default();
        Ok(serde_json::json!({ "alerts": alerts, "count": alerts.len() }))
    }

    #[allow(clippy::unnecessary_wraps)]
    fn tool_query_sessions(&self, args: &serde_json::Value) -> Result<serde_json::Value, McpError> {
        let limit = args
            .get("limit")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(50);
        let machine = args.get("machine").and_then(|v| v.as_str());

        let sql = if let Some(machine) = machine {
            format!(
                "SELECT * FROM sessions WHERE machine_id = '{machine}' \
                 ORDER BY started_at DESC LIMIT {limit}"
            )
        } else {
            format!("SELECT * FROM sessions ORDER BY started_at DESC LIMIT {limit}")
        };

        let sessions = self.store.query_json(&sql).unwrap_or_default();
        Ok(serde_json::json!({ "sessions": sessions, "count": sessions.len() }))
    }

    #[allow(clippy::unnecessary_wraps)]
    fn tool_query_incidents(
        &self,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        let limit = args
            .get("limit")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(50);
        let status = args.get("status").and_then(|v| v.as_str());

        let sql = if let Some(status) = status {
            format!(
                "SELECT * FROM incidents WHERE status = '{status}' \
                 ORDER BY created_at DESC LIMIT {limit}"
            )
        } else {
            format!("SELECT * FROM incidents ORDER BY created_at DESC LIMIT {limit}")
        };

        let incidents = self.store.query_json(&sql).unwrap_or_default();
        Ok(serde_json::json!({ "incidents": incidents, "count": incidents.len() }))
    }

    fn tool_query_nl(&self, args: &serde_json::Value) -> Result<serde_json::Value, McpError> {
        let question = args
            .get("question")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidRequest("'question' parameter required".to_string()))?;

        let engine = vc_query::NlEngine::new(self.store.clone());
        let result = engine.ask(question)?;

        Ok(serde_json::json!({
            "question": result.original_question,
            "explanation": result.explanation,
            "intent": format!("{:?}", result.intent),
            "sql": result.generated_sql,
            "results": result.results,
            "result_count": result.result_count,
        }))
    }

    #[allow(clippy::unnecessary_wraps)]
    fn tool_collector_status(
        &self,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        let limit = args
            .get("limit")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(50);

        let sql = format!("SELECT * FROM collector_health ORDER BY checked_at DESC LIMIT {limit}");

        let collectors = self.store.query_json(&sql).unwrap_or_default();
        Ok(serde_json::json!({ "collectors": collectors, "count": collectors.len() }))
    }

    #[allow(clippy::unnecessary_wraps)]
    fn tool_playbook_drafts(
        &self,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        let status = args.get("status").and_then(|v| v.as_str());

        let drafts = self
            .store
            .list_playbook_drafts(status, 100)
            .unwrap_or_default();
        let count = drafts.len();
        Ok(serde_json::json!({ "drafts": drafts, "count": count }))
    }

    #[allow(clippy::unnecessary_wraps)]
    fn tool_audit_log(&self, args: &serde_json::Value) -> Result<serde_json::Value, McpError> {
        let limit = args
            .get("limit")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(50);

        let sql = format!("SELECT * FROM audit_events ORDER BY timestamp DESC LIMIT {limit}");

        let events = self.store.query_json(&sql).unwrap_or_default();
        Ok(serde_json::json!({ "events": events, "count": events.len() }))
    }

    // ========================================================================
    // JSON-RPC handler
    // ========================================================================

    /// Handle a JSON-RPC request and return a response
    #[must_use]
    pub fn handle_request(&self, request: &JsonRpcRequest) -> JsonRpcResponse {
        let result = match request.method.as_str() {
            "initialize" => Ok(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {},
                    "resources": {}
                },
                "serverInfo": {
                    "name": "vibe-cockpit",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })),

            "notifications/initialized" => {
                // Client acknowledged initialization - no response needed for notifications
                return JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id.clone(),
                    result: Some(serde_json::json!(null)),
                    error: None,
                };
            }

            "tools/list" => {
                let tools: Vec<serde_json::Value> = self
                    .tools
                    .iter()
                    .filter_map(|t| serde_json::to_value(t).ok())
                    .collect();
                Ok(serde_json::json!({ "tools": tools }))
            }

            "tools/call" => {
                let name = request
                    .params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let empty_args = serde_json::json!({});
                let args = request.params.get("arguments").unwrap_or(&empty_args);

                match self.call_tool(name, args) {
                    Ok(result) => serde_json::to_value(result).map_err(McpError::from),
                    Err(e) => Ok(serde_json::json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Error: {e}")
                        }],
                        "isError": true
                    })),
                }
            }

            "resources/list" => {
                let resources: Vec<serde_json::Value> = self
                    .resources
                    .iter()
                    .filter_map(|r| serde_json::to_value(r).ok())
                    .collect();
                Ok(serde_json::json!({ "resources": resources }))
            }

            "resources/read" => {
                let uri = request
                    .params
                    .get("uri")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                match self.read_resource(uri) {
                    Ok(value) => Ok(serde_json::json!({
                        "contents": [{
                            "uri": uri,
                            "mimeType": "application/json",
                            "text": serde_json::to_string_pretty(&value).unwrap_or_default()
                        }]
                    })),
                    Err(e) => Err(e),
                }
            }

            "ping" => Ok(serde_json::json!({})),

            _ => Err(McpError::InvalidRequest(format!(
                "Unknown method: {}",
                request.method
            ))),
        };

        match result {
            Ok(value) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id.clone(),
                result: Some(value),
                error: None,
            },
            Err(e) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id.clone(),
                result: None,
                error: Some(JsonRpcError {
                    code: -32603,
                    message: e.to_string(),
                }),
            },
        }
    }

    /// Run the MCP server on stdio (blocking).
    ///
    /// # Errors
    ///
    /// Returns an error when reading input, parsing/serializing JSON, or
    /// writing output fails.
    pub fn run_stdio(&self) -> Result<(), McpError> {
        use std::io::{BufRead, Write};

        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        let reader = stdin.lock();
        let mut writer = stdout.lock();

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            let request: JsonRpcRequest = match serde_json::from_str(&line) {
                Ok(req) => req,
                Err(e) => {
                    let error_resp = JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: None,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32700,
                            message: format!("Parse error: {e}"),
                        }),
                    };
                    let resp_json = serde_json::to_string(&error_resp)?;
                    writeln!(writer, "{resp_json}")?;
                    writer.flush()?;
                    continue;
                }
            };

            let response = self.handle_request(&request);
            let resp_json = serde_json::to_string(&response)?;
            writeln!(writer, "{resp_json}")?;
            writer.flush()?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_server() -> McpServer {
        let store = Arc::new(VcStore::open_memory().unwrap());
        McpServer::new(store)
    }

    // ========================================================================
    // Tool/resource listing tests
    // ========================================================================

    #[test]
    fn test_list_tools() {
        let server = test_server();
        let tools = server.list_tools();
        assert!(tools.len() >= 9);
    }

    #[test]
    fn test_expected_tool_names() {
        let server = test_server();
        let names: Vec<&str> = server
            .list_tools()
            .iter()
            .map(|t| t.name.as_str())
            .collect();

        assert!(names.contains(&"vc_fleet_status"));
        assert!(names.contains(&"vc_query_machines"));
        assert!(names.contains(&"vc_query_alerts"));
        assert!(names.contains(&"vc_query_sessions"));
        assert!(names.contains(&"vc_query_incidents"));
        assert!(names.contains(&"vc_query_nl"));
        assert!(names.contains(&"vc_collector_status"));
        assert!(names.contains(&"vc_playbook_drafts"));
        assert!(names.contains(&"vc_audit_log"));
    }

    #[test]
    fn test_list_resources() {
        let server = test_server();
        let resources = server.list_resources();
        assert_eq!(resources.len(), 2);
    }

    #[test]
    fn test_expected_resource_uris() {
        let server = test_server();
        let uris: Vec<&str> = server
            .list_resources()
            .iter()
            .map(|r| r.uri.as_str())
            .collect();

        assert!(uris.contains(&"vc://fleet/overview"));
        assert!(uris.contains(&"vc://machines"));
    }

    // ========================================================================
    // Tool schemas
    // ========================================================================

    #[test]
    fn test_tool_schemas_are_objects() {
        let server = test_server();
        for tool in server.list_tools() {
            assert!(
                tool.input_schema.is_object(),
                "Tool {} schema is not an object",
                tool.name
            );
            assert_eq!(
                tool.input_schema.get("type").and_then(|v| v.as_str()),
                Some("object"),
                "Tool {} schema type is not 'object'",
                tool.name
            );
        }
    }

    // ========================================================================
    // Tool call tests (empty database)
    // ========================================================================

    #[test]
    fn test_call_fleet_status() {
        let server = test_server();
        let result = server.call_tool("vc_fleet_status", &serde_json::json!({}));
        assert!(result.is_ok());
        let r = result.unwrap();
        assert!(!r.content.is_empty());
        assert!(r.is_error.is_none());
    }

    #[test]
    fn test_call_fleet_status_with_machine() {
        let server = test_server();
        let result = server.call_tool("vc_fleet_status", &serde_json::json!({"machine": "orko"}));
        assert!(result.is_ok());
    }

    #[test]
    fn test_call_query_machines() {
        let server = test_server();
        let result = server.call_tool("vc_query_machines", &serde_json::json!({}));
        assert!(result.is_ok());
        let text = &result.unwrap().content[0].text;
        let parsed: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(parsed["count"], 0);
    }

    #[test]
    fn test_call_query_machines_with_filters() {
        let server = test_server();
        let result = server.call_tool(
            "vc_query_machines",
            &serde_json::json!({"status": "online", "limit": 10}),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_call_query_alerts() {
        let server = test_server();
        let result = server.call_tool("vc_query_alerts", &serde_json::json!({}));
        assert!(result.is_ok());
        let text = &result.unwrap().content[0].text;
        let parsed: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(parsed["count"], 0);
    }

    #[test]
    fn test_call_query_alerts_with_severity() {
        let server = test_server();
        let result = server.call_tool(
            "vc_query_alerts",
            &serde_json::json!({"severity": "critical"}),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_call_query_sessions() {
        let server = test_server();
        let result = server.call_tool("vc_query_sessions", &serde_json::json!({}));
        assert!(result.is_ok());
    }

    #[test]
    fn test_call_query_sessions_with_machine() {
        let server = test_server();
        let result = server.call_tool(
            "vc_query_sessions",
            &serde_json::json!({"machine": "orko", "limit": 5}),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_call_query_incidents() {
        let server = test_server();
        let result = server.call_tool("vc_query_incidents", &serde_json::json!({}));
        assert!(result.is_ok());
    }

    #[test]
    fn test_call_query_incidents_with_status() {
        let server = test_server();
        let result = server.call_tool("vc_query_incidents", &serde_json::json!({"status": "open"}));
        assert!(result.is_ok());
    }

    #[test]
    fn test_call_query_nl() {
        let server = test_server();
        let result = server.call_tool(
            "vc_query_nl",
            &serde_json::json!({"question": "how many machines are online?"}),
        );
        assert!(result.is_ok());
        let text = &result.unwrap().content[0].text;
        let parsed: serde_json::Value = serde_json::from_str(text).unwrap();
        assert!(parsed.get("explanation").is_some());
        assert!(parsed.get("intent").is_some());
    }

    #[test]
    fn test_call_query_nl_missing_question() {
        let server = test_server();
        let result = server.call_tool("vc_query_nl", &serde_json::json!({}));
        // Returns ToolResult with is_error=true, not Err
        assert!(result.is_ok());
        let r = result.unwrap();
        assert_eq!(r.is_error, Some(true));
        assert!(r.content[0].text.contains("question"));
    }

    #[test]
    fn test_call_collector_status() {
        let server = test_server();
        let result = server.call_tool("vc_collector_status", &serde_json::json!({}));
        assert!(result.is_ok());
    }

    #[test]
    fn test_call_playbook_drafts() {
        let server = test_server();
        let result = server.call_tool("vc_playbook_drafts", &serde_json::json!({}));
        assert!(result.is_ok());
        let text = &result.unwrap().content[0].text;
        let parsed: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(parsed["count"], 0);
    }

    #[test]
    fn test_call_playbook_drafts_with_status() {
        let server = test_server();
        let result = server.call_tool(
            "vc_playbook_drafts",
            &serde_json::json!({"status": "pending_review"}),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_call_audit_log() {
        let server = test_server();
        let result = server.call_tool("vc_audit_log", &serde_json::json!({}));
        assert!(result.is_ok());
    }

    #[test]
    fn test_call_audit_log_with_limit() {
        let server = test_server();
        let result = server.call_tool("vc_audit_log", &serde_json::json!({"limit": 10}));
        assert!(result.is_ok());
    }

    #[test]
    fn test_call_tool_not_found() {
        let server = test_server();
        let result = server.call_tool("nonexistent", &serde_json::json!({}));
        assert!(result.is_err());
        match result.unwrap_err() {
            McpError::ToolNotFound(name) => assert_eq!(name, "nonexistent"),
            e => panic!("Expected ToolNotFound, got: {e}"),
        }
    }

    // ========================================================================
    // Resource tests
    // ========================================================================

    #[test]
    fn test_read_resource_fleet_overview() {
        let server = test_server();
        let result = server.read_resource("vc://fleet/overview");
        assert!(result.is_ok());
        let value = result.unwrap();
        assert!(value.get("total_machines").is_some());
    }

    #[test]
    fn test_read_resource_machines() {
        let server = test_server();
        let result = server.read_resource("vc://machines");
        assert!(result.is_ok());
        let value = result.unwrap();
        assert!(value.get("machines").is_some());
    }

    #[test]
    fn test_read_resource_not_found() {
        let server = test_server();
        let result = server.read_resource("vc://nonexistent");
        assert!(result.is_err());
        match result.unwrap_err() {
            McpError::InvalidRequest(msg) => assert!(msg.contains("Unknown resource")),
            e => panic!("Expected InvalidRequest, got: {e}"),
        }
    }

    // ========================================================================
    // JSON-RPC handler tests
    // ========================================================================

    #[test]
    fn test_jsonrpc_initialize() {
        let server = test_server();
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(1)),
            method: "initialize".to_string(),
            params: serde_json::json!({}),
        };

        let resp = server.handle_request(&req);
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert_eq!(result["protocolVersion"], "2024-11-05");
        assert!(result.get("capabilities").is_some());
        assert!(result.get("serverInfo").is_some());
    }

    #[test]
    fn test_jsonrpc_tools_list() {
        let server = test_server();
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(2)),
            method: "tools/list".to_string(),
            params: serde_json::json!({}),
        };

        let resp = server.handle_request(&req);
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert!(tools.len() >= 9);
    }

    #[test]
    fn test_jsonrpc_tools_call() {
        let server = test_server();
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(3)),
            method: "tools/call".to_string(),
            params: serde_json::json!({
                "name": "vc_fleet_status",
                "arguments": {}
            }),
        };

        let resp = server.handle_request(&req);
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert!(result.get("content").is_some());
    }

    #[test]
    fn test_jsonrpc_tools_call_not_found() {
        let server = test_server();
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(4)),
            method: "tools/call".to_string(),
            params: serde_json::json!({
                "name": "nonexistent_tool",
                "arguments": {}
            }),
        };

        let resp = server.handle_request(&req);
        // Tool errors are returned as ToolResult with isError=true
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert_eq!(result["isError"], true);
    }

    #[test]
    fn test_jsonrpc_resources_list() {
        let server = test_server();
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(5)),
            method: "resources/list".to_string(),
            params: serde_json::json!({}),
        };

        let resp = server.handle_request(&req);
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        let resources = result["resources"].as_array().unwrap();
        assert_eq!(resources.len(), 2);
    }

    #[test]
    fn test_jsonrpc_resources_read() {
        let server = test_server();
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(6)),
            method: "resources/read".to_string(),
            params: serde_json::json!({
                "uri": "vc://fleet/overview"
            }),
        };

        let resp = server.handle_request(&req);
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert!(result.get("contents").is_some());
    }

    #[test]
    fn test_jsonrpc_resources_read_not_found() {
        let server = test_server();
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(7)),
            method: "resources/read".to_string(),
            params: serde_json::json!({
                "uri": "vc://nonexistent"
            }),
        };

        let resp = server.handle_request(&req);
        assert!(resp.error.is_some());
    }

    #[test]
    fn test_jsonrpc_ping() {
        let server = test_server();
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(8)),
            method: "ping".to_string(),
            params: serde_json::json!({}),
        };

        let resp = server.handle_request(&req);
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_jsonrpc_unknown_method() {
        let server = test_server();
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(9)),
            method: "bogus/method".to_string(),
            params: serde_json::json!({}),
        };

        let resp = server.handle_request(&req);
        assert!(resp.error.is_some());
        assert!(resp.error.unwrap().message.contains("Unknown method"));
    }

    #[test]
    fn test_jsonrpc_initialized_notification() {
        let server = test_server();
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: "notifications/initialized".to_string(),
            params: serde_json::json!({}),
        };

        let resp = server.handle_request(&req);
        assert!(resp.error.is_none());
    }

    // ========================================================================
    // Serialization tests
    // ========================================================================

    #[test]
    fn test_mcp_tool_serialization() {
        let tool = McpTool {
            name: "test".to_string(),
            description: "A test tool".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        };

        let json = serde_json::to_string(&tool).unwrap();
        assert!(json.contains("inputSchema"));
        let parsed: McpTool = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "test");
    }

    #[test]
    fn test_mcp_resource_serialization() {
        let resource = McpResource {
            uri: "vc://test".to_string(),
            name: "Test".to_string(),
            description: "A test resource".to_string(),
            mime_type: "application/json".to_string(),
        };

        let json = serde_json::to_string(&resource).unwrap();
        assert!(json.contains("mimeType"));
        let parsed: McpResource = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.uri, "vc://test");
    }

    #[test]
    fn test_tool_result_serialization() {
        let result = ToolResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: "hello".to_string(),
            }],
            is_error: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(!json.contains("isError"));
        let parsed: ToolResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.content[0].text, "hello");
    }

    #[test]
    fn test_tool_result_with_error_serialization() {
        let result = ToolResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: "Error: something".to_string(),
            }],
            is_error: Some(true),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("isError"));
    }

    // ========================================================================
    // Error type tests
    // ========================================================================

    #[test]
    fn test_error_tool_not_found() {
        let err = McpError::ToolNotFound("missing".to_string());
        assert!(err.to_string().contains("Tool not found"));
        assert!(err.to_string().contains("missing"));
    }

    #[test]
    fn test_error_invalid_request() {
        let err = McpError::InvalidRequest("bad".to_string());
        assert!(err.to_string().contains("Invalid request"));
    }

    #[test]
    fn test_error_execution_error() {
        let err = McpError::ExecutionError("timeout".to_string());
        assert!(err.to_string().contains("Execution error"));
    }

    // ========================================================================
    // Full JSON-RPC roundtrip test
    // ========================================================================

    #[test]
    fn test_jsonrpc_full_session() {
        let server = test_server();

        // 1. Initialize
        let init = server.handle_request(&JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(1)),
            method: "initialize".to_string(),
            params: serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "test-client", "version": "0.1.0" }
            }),
        });
        assert!(init.error.is_none());

        // 2. List tools
        let tools_resp = server.handle_request(&JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(2)),
            method: "tools/list".to_string(),
            params: serde_json::json!({}),
        });
        let tools = tools_resp.result.unwrap();
        let tool_count = tools["tools"].as_array().unwrap().len();
        assert!(tool_count >= 9);

        // 3. Call a tool
        let call_resp = server.handle_request(&JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(3)),
            method: "tools/call".to_string(),
            params: serde_json::json!({
                "name": "vc_query_nl",
                "arguments": { "question": "show all machines" }
            }),
        });
        assert!(call_resp.error.is_none());
        let result = call_resp.result.unwrap();
        assert!(result.get("content").is_some());

        // 4. Read a resource
        let res_resp = server.handle_request(&JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(4)),
            method: "resources/read".to_string(),
            params: serde_json::json!({ "uri": "vc://machines" }),
        });
        assert!(res_resp.error.is_none());
    }
}
