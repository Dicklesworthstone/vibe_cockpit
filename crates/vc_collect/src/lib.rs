//! vc_collect - Data collectors for Vibe Cockpit
//!
//! This crate provides:
//! - The Collector trait for implementing data sources
//! - Built-in collectors for various tools (sysmoni, ru, caut, etc.)
//! - Execution context and result handling
//! - Cursor management for incremental collection

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use thiserror::Error;

pub mod executor;

/// Collection errors
#[derive(Error, Debug)]
pub enum CollectError {
    #[error("Command execution failed: {0}")]
    ExecutionError(String),

    #[error("Failed to parse output: {0}")]
    ParseError(String),

    #[error("Timeout after {0:?}")]
    Timeout(Duration),

    #[error("Tool not available: {0}")]
    ToolNotFound(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Other error: {0}")]
    Other(String),
}

/// Result of a collection run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectResult {
    /// Batches of rows to insert into tables
    pub rows: Vec<RowBatch>,

    /// Updated cursor state (for incremental collectors)
    pub new_cursor: Option<String>,

    /// Raw artifacts for debugging/archival
    pub raw_artifacts: Vec<RawArtifact>,

    /// Non-fatal warnings encountered
    pub warnings: Vec<String>,

    /// Collection duration
    pub duration: Duration,

    /// Whether collection succeeded
    pub success: bool,

    /// Error message if failed
    pub error: Option<String>,
}

impl CollectResult {
    /// Create a successful empty result
    pub fn empty() -> Self {
        Self {
            rows: vec![],
            new_cursor: None,
            raw_artifacts: vec![],
            warnings: vec![],
            duration: Duration::ZERO,
            success: true,
            error: None,
        }
    }

    /// Create a failed result
    pub fn failed(error: impl Into<String>) -> Self {
        Self {
            rows: vec![],
            new_cursor: None,
            raw_artifacts: vec![],
            warnings: vec![],
            duration: Duration::ZERO,
            success: false,
            error: Some(error.into()),
        }
    }

    /// Total number of rows collected
    pub fn total_rows(&self) -> usize {
        self.rows.iter().map(|b| b.rows.len()).sum()
    }
}

/// A batch of rows for a specific table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RowBatch {
    /// Target table name
    pub table: String,

    /// Rows as JSON values
    pub rows: Vec<serde_json::Value>,
}

/// Raw artifact for debugging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawArtifact {
    /// Artifact name/identifier
    pub name: String,

    /// Content type (json, text, binary)
    pub content_type: String,

    /// Raw content
    pub content: String,
}

/// Context provided to collectors during execution
#[derive(Debug, Clone)]
pub struct CollectContext {
    /// Machine being collected from
    pub machine_id: String,

    /// Whether this is a local or remote machine
    pub is_local: bool,

    /// Collection timeout
    pub timeout: Duration,

    /// Previous cursor state (for incremental collectors)
    pub cursor: Option<String>,

    /// Collected at timestamp
    pub collected_at: DateTime<Utc>,

    /// Command executor
    pub executor: executor::Executor,
}

impl CollectContext {
    /// Create a new context for local collection
    pub fn local(machine_id: impl Into<String>, timeout: Duration) -> Self {
        Self {
            machine_id: machine_id.into(),
            is_local: true,
            timeout,
            cursor: None,
            collected_at: Utc::now(),
            executor: executor::Executor::local(),
        }
    }
}

/// The core Collector trait
#[async_trait]
pub trait Collector: Send + Sync {
    /// Unique name for this collector
    fn name(&self) -> &'static str;

    /// Schema version for data format
    fn schema_version(&self) -> u32 {
        1
    }

    /// Required tool binary (if any)
    fn required_tool(&self) -> Option<&'static str> {
        None
    }

    /// Whether this collector supports incremental collection
    fn supports_incremental(&self) -> bool {
        false
    }

    /// Perform data collection
    async fn collect(&self, ctx: &CollectContext) -> Result<CollectResult, CollectError>;

    /// Check if the required tool is available
    async fn check_availability(&self, ctx: &CollectContext) -> bool {
        match self.required_tool() {
            Some(tool) => ctx.executor.check_tool(tool).await.is_ok(),
            None => true,
        }
    }
}

/// Registry of available collectors
pub struct CollectorRegistry {
    collectors: HashMap<String, Box<dyn Collector>>,
}

impl CollectorRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            collectors: HashMap::new(),
        }
    }

    /// Register a collector
    pub fn register(&mut self, collector: Box<dyn Collector>) {
        let name = collector.name().to_string();
        self.collectors.insert(name, collector);
    }

    /// Get a collector by name
    pub fn get(&self, name: &str) -> Option<&dyn Collector> {
        self.collectors.get(name).map(|c| c.as_ref())
    }

    /// List all registered collector names
    pub fn names(&self) -> Vec<&str> {
        self.collectors.keys().map(|s| s.as_str()).collect()
    }

    /// Create registry with all built-in collectors
    pub fn with_builtins() -> Self {
        let mut registry = Self::new();
        // Collectors will be registered here as they're implemented
        // registry.register(Box::new(collectors::sysmoni::SysmoniCollector));
        // registry.register(Box::new(collectors::ru::RuCollector));
        // etc.
        registry
    }
}

impl Default for CollectorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_result_empty() {
        let result = CollectResult::empty();
        assert!(result.success);
        assert_eq!(result.total_rows(), 0);
    }

    #[test]
    fn test_collect_result_failed() {
        let result = CollectResult::failed("test error");
        assert!(!result.success);
        assert_eq!(result.error, Some("test error".to_string()));
    }

    #[test]
    fn test_collector_registry() {
        let registry = CollectorRegistry::new();
        assert!(registry.names().is_empty());
    }
}
