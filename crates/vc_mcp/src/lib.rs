//! vc_mcp - MCP server for Vibe Cockpit
//!
//! This crate provides:
//! - MCP (Model Context Protocol) server implementation
//! - Tool registration for agent queries
//! - Resource exposure for fleet data
//! - Request/response handling

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// MCP server errors
#[derive(Error, Debug)]
pub enum McpError {
    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Execution error: {0}")]
    ExecutionError(String),

    #[error("Query error: {0}")]
    QueryError(#[from] vc_query::QueryError),
}

/// MCP tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// MCP resource definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource {
    pub uri: String,
    pub name: String,
    pub description: String,
    pub mime_type: String,
}

/// MCP server implementation
pub struct McpServer {
    tools: Vec<McpTool>,
    resources: Vec<McpResource>,
}

impl McpServer {
    /// Create a new MCP server
    pub fn new() -> Self {
        Self {
            tools: Self::default_tools(),
            resources: Self::default_resources(),
        }
    }

    /// Get default tools
    fn default_tools() -> Vec<McpTool> {
        vec![
            McpTool {
                name: "vc_fleet_status".to_string(),
                description: "Get current fleet status including machines, agents, and health scores".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "machine": {
                            "type": "string",
                            "description": "Optional machine ID to filter"
                        }
                    }
                }),
            },
            McpTool {
                name: "vc_triage".to_string(),
                description: "Get triage recommendations for the fleet".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            McpTool {
                name: "vc_alerts".to_string(),
                description: "Get active alerts".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "severity": {
                            "type": "string",
                            "enum": ["info", "warning", "critical"]
                        }
                    }
                }),
            },
            McpTool {
                name: "vc_oracle".to_string(),
                description: "Get predictions from the Oracle (rate limits, forecasts)".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "prediction_type": {
                            "type": "string",
                            "enum": ["rate_limit", "cost", "health"]
                        }
                    }
                }),
            },
        ]
    }

    /// Get default resources
    fn default_resources() -> Vec<McpResource> {
        vec![
            McpResource {
                uri: "vc://fleet/overview".to_string(),
                name: "Fleet Overview".to_string(),
                description: "Current fleet status and health".to_string(),
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
    pub fn list_tools(&self) -> &[McpTool] {
        &self.tools
    }

    /// List available resources
    pub fn list_resources(&self) -> &[McpResource] {
        &self.resources
    }

    /// Execute a tool
    pub async fn call_tool(
        &self,
        name: &str,
        _args: serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        match name {
            "vc_fleet_status" => {
                // Placeholder
                Ok(serde_json::json!({
                    "total_machines": 0,
                    "online": 0,
                    "health_score": 1.0
                }))
            }
            "vc_triage" => {
                Ok(serde_json::json!({
                    "recommendations": []
                }))
            }
            "vc_alerts" => {
                Ok(serde_json::json!({
                    "alerts": []
                }))
            }
            "vc_oracle" => {
                Ok(serde_json::json!({
                    "predictions": []
                }))
            }
            _ => Err(McpError::ToolNotFound(name.to_string())),
        }
    }

    /// Read a resource
    pub async fn read_resource(&self, uri: &str) -> Result<serde_json::Value, McpError> {
        match uri {
            "vc://fleet/overview" => {
                Ok(serde_json::json!({
                    "machines": [],
                    "health": 1.0
                }))
            }
            "vc://machines" => {
                Ok(serde_json::json!([]))
            }
            _ => Err(McpError::InvalidRequest(format!("Unknown resource: {uri}"))),
        }
    }
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // McpServer tests
    #[test]
    fn test_default_tools() {
        let server = McpServer::new();
        assert!(!server.list_tools().is_empty());
    }

    #[test]
    fn test_default_resources() {
        let server = McpServer::new();
        assert!(!server.list_resources().is_empty());
    }

    #[test]
    fn test_server_default() {
        let server = McpServer::default();
        assert!(!server.list_tools().is_empty());
        assert!(!server.list_resources().is_empty());
    }

    #[test]
    fn test_expected_tool_names() {
        let server = McpServer::new();
        let tool_names: Vec<&str> = server.list_tools().iter().map(|t| t.name.as_str()).collect();

        assert!(tool_names.contains(&"vc_fleet_status"));
        assert!(tool_names.contains(&"vc_triage"));
        assert!(tool_names.contains(&"vc_alerts"));
        assert!(tool_names.contains(&"vc_oracle"));
    }

    #[test]
    fn test_expected_resource_uris() {
        let server = McpServer::new();
        let uris: Vec<&str> = server.list_resources().iter().map(|r| r.uri.as_str()).collect();

        assert!(uris.contains(&"vc://fleet/overview"));
        assert!(uris.contains(&"vc://machines"));
    }

    // Tool call tests
    #[tokio::test]
    async fn test_call_tool_fleet_status() {
        let server = McpServer::new();
        let result = server.call_tool("vc_fleet_status", serde_json::json!({})).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert!(value.get("total_machines").is_some());
        assert!(value.get("health_score").is_some());
    }

    #[tokio::test]
    async fn test_call_tool_triage() {
        let server = McpServer::new();
        let result = server.call_tool("vc_triage", serde_json::json!({})).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert!(value.get("recommendations").is_some());
    }

    #[tokio::test]
    async fn test_call_tool_alerts() {
        let server = McpServer::new();
        let result = server.call_tool("vc_alerts", serde_json::json!({})).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert!(value.get("alerts").is_some());
    }

    #[tokio::test]
    async fn test_call_tool_oracle() {
        let server = McpServer::new();
        let result = server.call_tool("vc_oracle", serde_json::json!({})).await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert!(value.get("predictions").is_some());
    }

    #[tokio::test]
    async fn test_call_tool_not_found() {
        let server = McpServer::new();
        let result = server.call_tool("nonexistent_tool", serde_json::json!({})).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            McpError::ToolNotFound(name) => assert_eq!(name, "nonexistent_tool"),
            _ => panic!("Expected ToolNotFound error"),
        }
    }

    // Resource read tests
    #[tokio::test]
    async fn test_read_resource_fleet_overview() {
        let server = McpServer::new();
        let result = server.read_resource("vc://fleet/overview").await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert!(value.get("machines").is_some());
        assert!(value.get("health").is_some());
    }

    #[tokio::test]
    async fn test_read_resource_machines() {
        let server = McpServer::new();
        let result = server.read_resource("vc://machines").await;
        assert!(result.is_ok());

        let value = result.unwrap();
        assert!(value.is_array());
    }

    #[tokio::test]
    async fn test_read_resource_not_found() {
        let server = McpServer::new();
        let result = server.read_resource("vc://nonexistent").await;
        assert!(result.is_err());

        match result.unwrap_err() {
            McpError::InvalidRequest(msg) => assert!(msg.contains("Unknown resource")),
            _ => panic!("Expected InvalidRequest error"),
        }
    }

    // McpTool tests
    #[test]
    fn test_mcp_tool_creation() {
        let tool = McpTool {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        };

        assert_eq!(tool.name, "test_tool");
        assert!(tool.input_schema.is_object());
    }

    #[test]
    fn test_mcp_tool_serialization() {
        let tool = McpTool {
            name: "serialize_test".to_string(),
            description: "Testing serialization".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        };

        let json = serde_json::to_string(&tool).unwrap();
        let parsed: McpTool = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.name, tool.name);
        assert_eq!(parsed.description, tool.description);
    }

    // McpResource tests
    #[test]
    fn test_mcp_resource_creation() {
        let resource = McpResource {
            uri: "vc://test/resource".to_string(),
            name: "Test Resource".to_string(),
            description: "A test resource".to_string(),
            mime_type: "application/json".to_string(),
        };

        assert_eq!(resource.uri, "vc://test/resource");
        assert_eq!(resource.mime_type, "application/json");
    }

    #[test]
    fn test_mcp_resource_serialization() {
        let resource = McpResource {
            uri: "vc://ser/test".to_string(),
            name: "Serialize Test".to_string(),
            description: "Testing".to_string(),
            mime_type: "text/plain".to_string(),
        };

        let json = serde_json::to_string(&resource).unwrap();
        let parsed: McpResource = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.uri, resource.uri);
        assert_eq!(parsed.mime_type, resource.mime_type);
    }

    proptest! {
        #[test]
        fn test_mcp_tool_roundtrip(
            name in "[a-zA-Z0-9_]{1,32}",
            description in ".{0,64}"
        ) {
            let tool = McpTool {
                name,
                description,
                input_schema: serde_json::json!({"type": "object"}),
            };

            let json = serde_json::to_string(&tool).unwrap();
            let parsed: McpTool = serde_json::from_str(&json).unwrap();

            prop_assert_eq!(parsed.name, tool.name);
            prop_assert_eq!(parsed.description, tool.description);
        }
    }

    proptest! {
        #[test]
        fn test_mcp_resource_roundtrip(
            uri in "vc://[a-zA-Z0-9/_-]{1,48}",
            name in "[a-zA-Z0-9 _-]{1,32}",
            description in ".{0,64}",
            mime_type in "application/[a-zA-Z0-9.+-]{1,24}"
        ) {
            let resource = McpResource {
                uri,
                name,
                description,
                mime_type,
            };

            let json = serde_json::to_string(&resource).unwrap();
            let parsed: McpResource = serde_json::from_str(&json).unwrap();

            prop_assert_eq!(parsed.uri, resource.uri);
            prop_assert_eq!(parsed.name, resource.name);
            prop_assert_eq!(parsed.description, resource.description);
            prop_assert_eq!(parsed.mime_type, resource.mime_type);
        }
    }

    // McpError tests
    #[test]
    fn test_error_tool_not_found() {
        let err = McpError::ToolNotFound("missing_tool".to_string());
        assert!(err.to_string().contains("Tool not found"));
        assert!(err.to_string().contains("missing_tool"));
    }

    #[test]
    fn test_error_invalid_request() {
        let err = McpError::InvalidRequest("bad request".to_string());
        assert!(err.to_string().contains("Invalid request"));
    }

    #[test]
    fn test_error_execution_error() {
        let err = McpError::ExecutionError("timeout".to_string());
        assert!(err.to_string().contains("Execution error"));
    }
}
