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

    #[tokio::test]
    async fn test_call_tool() {
        let server = McpServer::new();
        let result = server.call_tool("vc_fleet_status", serde_json::json!({})).await;
        assert!(result.is_ok());
    }
}
