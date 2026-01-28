//! vc_collect - Data collectors for Vibe Cockpit
//!
//! This crate provides:
//! - The Collector trait for implementing data sources
//! - Built-in collectors for various tools (sysmoni, ru, caut, etc.)
//! - Execution context and result handling
//! - Cursor management for incremental collection
//!
//! # Collector Design Principles
//!
//! 1. **Idempotent inserts**: Same source payload should not create duplicates
//! 2. **Incremental by default**: Avoid rescanning large histories every poll
//! 3. **Versioned outputs**: Every collector has `schema_version` for evolution
//! 4. **Fail-soft**: Broken collector degrades cockpit (shows "stale"), doesn't crash
//! 5. **Timeout-bounded**: No collector can hang the system

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

pub mod collectors;
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

    #[error("SQLite error: {0}")]
    SqliteError(String),

    #[error("HTTP error: {0}")]
    HttpError(String),

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Other error: {0}")]
    Other(String),
}

/// Cursor types for incremental collection patterns
///
/// Different data sources have different incremental collection strategies.
/// This enum captures the various cursor types used to track collection state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "value")]
pub enum Cursor {
    /// For time-bounded queries (e.g., "since timestamp")
    /// Used by: sysmoni, caut, cass
    Timestamp(DateTime<Utc>),

    /// For JSONL tail (inode + byte offset)
    /// Used by: mcp_agent_mail, bv/br
    FileOffset {
        /// File inode for rotation detection
        inode: u64,
        /// Byte offset in file
        offset: u64,
    },

    /// For SQLite incremental (last seen primary key)
    /// Used by: cass, caam, mcp_agent_mail
    PrimaryKey(i64),

    /// For custom cursor formats (JSON-encoded string)
    /// Used by: collectors with complex state
    Opaque(String),
}

impl Cursor {
    /// Create a timestamp cursor from now
    pub fn now() -> Self {
        Self::Timestamp(Utc::now())
    }

    /// Create a file offset cursor
    pub fn file_offset(inode: u64, offset: u64) -> Self {
        Self::FileOffset { inode, offset }
    }

    /// Create a primary key cursor
    pub fn primary_key(pk: i64) -> Self {
        Self::PrimaryKey(pk)
    }

    /// Create an opaque cursor from any serializable value
    pub fn opaque<T: Serialize>(value: &T) -> Result<Self, serde_json::Error> {
        Ok(Self::Opaque(serde_json::to_string(value)?))
    }

    /// Parse an opaque cursor into a typed value
    pub fn parse_opaque<T: for<'de> Deserialize<'de>>(&self) -> Option<T> {
        match self {
            Self::Opaque(s) => serde_json::from_str(s).ok(),
            _ => None,
        }
    }

    /// Convert to JSON string for storage
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Parse from JSON string
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}

/// Result of a collection run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectResult {
    /// Batches of rows to insert into tables
    pub rows: Vec<RowBatch>,

    /// Updated cursor state (for incremental collectors)
    pub new_cursor: Option<Cursor>,

    /// Raw artifacts for debugging/archival
    pub raw_artifacts: Vec<RawArtifact>,

    /// Non-fatal warnings encountered
    pub warnings: Vec<Warning>,

    /// Collection duration
    #[serde(with = "duration_serde")]
    pub duration: Duration,

    /// Whether collection succeeded
    pub success: bool,

    /// Error message if failed
    pub error: Option<String>,
}

/// Warning from collection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Warning {
    /// Warning level
    pub level: WarningLevel,
    /// Warning message
    pub message: String,
    /// Additional context
    pub context: Option<String>,
}

impl Warning {
    /// Create an info warning
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            level: WarningLevel::Info,
            message: message.into(),
            context: None,
        }
    }

    /// Create a warn-level warning
    pub fn warn(message: impl Into<String>) -> Self {
        Self {
            level: WarningLevel::Warn,
            message: message.into(),
            context: None,
        }
    }

    /// Create an error-level warning
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            level: WarningLevel::Error,
            message: message.into(),
            context: None,
        }
    }

    /// Add context to this warning
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }
}

/// Warning severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WarningLevel {
    Info,
    Warn,
    Error,
}

/// Serde helper for Duration
mod duration_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S: Serializer>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error> {
        duration.as_millis().serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Duration, D::Error> {
        let millis = u64::deserialize(deserializer)?;
        Ok(Duration::from_millis(millis))
    }
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

    /// Create a successful result with rows
    pub fn with_rows(rows: Vec<RowBatch>) -> Self {
        Self {
            rows,
            new_cursor: None,
            raw_artifacts: vec![],
            warnings: vec![],
            duration: Duration::ZERO,
            success: true,
            error: None,
        }
    }

    /// Set the cursor for this result
    pub fn with_cursor(mut self, cursor: Cursor) -> Self {
        self.new_cursor = Some(cursor);
        self
    }

    /// Set the duration for this result
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    /// Add a warning to this result
    pub fn with_warning(mut self, warning: Warning) -> Self {
        self.warnings.push(warning);
        self
    }

    /// Add a raw artifact to this result
    pub fn with_artifact(mut self, artifact: RawArtifact) -> Self {
        self.raw_artifacts.push(artifact);
        self
    }

    /// Total number of rows collected
    pub fn total_rows(&self) -> usize {
        self.rows.iter().map(|b| b.rows.len()).sum()
    }

    /// Get the number of tables with data
    pub fn table_count(&self) -> usize {
        self.rows.len()
    }

    /// Check if any warnings were generated
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
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
    pub cursor: Option<Cursor>,

    /// Collected at timestamp
    pub collected_at: DateTime<Utc>,

    /// Time window for incremental queries (e.g., "last 10 minutes")
    pub poll_window: Duration,

    /// Maximum bytes to read from command output
    pub max_bytes: usize,

    /// Maximum rows to insert per collection
    pub max_rows: usize,

    /// Command executor
    pub executor: Arc<executor::Executor>,
}

impl CollectContext {
    /// Default max bytes (1 MB)
    pub const DEFAULT_MAX_BYTES: usize = 1_048_576;

    /// Default max rows (10,000)
    pub const DEFAULT_MAX_ROWS: usize = 10_000;

    /// Default poll window (10 minutes)
    pub const DEFAULT_POLL_WINDOW: Duration = Duration::from_secs(600);

    /// Create a new context for local collection
    pub fn local(machine_id: impl Into<String>, timeout: Duration) -> Self {
        Self {
            machine_id: machine_id.into(),
            is_local: true,
            timeout,
            cursor: None,
            collected_at: Utc::now(),
            poll_window: Self::DEFAULT_POLL_WINDOW,
            max_bytes: Self::DEFAULT_MAX_BYTES,
            max_rows: Self::DEFAULT_MAX_ROWS,
            executor: Arc::new(executor::Executor::local()),
        }
    }

    /// Create a new context for remote collection
    pub fn remote(
        machine_id: impl Into<String>,
        timeout: Duration,
        ssh_config: executor::SshConfig,
    ) -> Self {
        Self {
            machine_id: machine_id.into(),
            is_local: false,
            timeout,
            cursor: None,
            collected_at: Utc::now(),
            poll_window: Self::DEFAULT_POLL_WINDOW,
            max_bytes: Self::DEFAULT_MAX_BYTES,
            max_rows: Self::DEFAULT_MAX_ROWS,
            executor: Arc::new(executor::Executor::remote(ssh_config)),
        }
    }

    /// Set the cursor for this context
    pub fn with_cursor(mut self, cursor: Cursor) -> Self {
        self.cursor = Some(cursor);
        self
    }

    /// Set the poll window for this context
    pub fn with_poll_window(mut self, window: Duration) -> Self {
        self.poll_window = window;
        self
    }

    /// Set max bytes for this context
    pub fn with_max_bytes(mut self, max: usize) -> Self {
        self.max_bytes = max;
        self
    }

    /// Set max rows for this context
    pub fn with_max_rows(mut self, max: usize) -> Self {
        self.max_rows = max;
        self
    }

    /// Get the timestamp cursor if present
    pub fn timestamp_cursor(&self) -> Option<DateTime<Utc>> {
        match &self.cursor {
            Some(Cursor::Timestamp(ts)) => Some(*ts),
            _ => None,
        }
    }

    /// Get the file offset cursor if present
    pub fn file_offset_cursor(&self) -> Option<(u64, u64)> {
        match &self.cursor {
            Some(Cursor::FileOffset { inode, offset }) => Some((*inode, *offset)),
            _ => None,
        }
    }

    /// Get the primary key cursor if present
    pub fn primary_key_cursor(&self) -> Option<i64> {
        match &self.cursor {
            Some(Cursor::PrimaryKey(pk)) => Some(*pk),
            _ => None,
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
            // check_tool returns Result<bool, _> where bool = tool exists
            // unwrap_or(false) returns false on error or if tool doesn't exist
            Some(tool) => ctx.executor.check_tool(tool).await.unwrap_or(false),
            None => true,
        }
    }
}

/// Registry of available collectors
pub struct CollectorRegistry {
    collectors: HashMap<String, Arc<dyn Collector>>,
}

impl CollectorRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            collectors: HashMap::new(),
        }
    }

    /// Register a collector
    pub fn register(&mut self, collector: Arc<dyn Collector>) {
        let name = collector.name().to_string();
        self.collectors.insert(name, collector);
    }

    /// Register a collector from a boxed trait object
    pub fn register_boxed(&mut self, collector: Box<dyn Collector>) {
        let name = collector.name().to_string();
        self.collectors.insert(name, Arc::from(collector));
    }

    /// Get a collector by name
    pub fn get(&self, name: &str) -> Option<Arc<dyn Collector>> {
        self.collectors.get(name).cloned()
    }

    /// List all registered collector names
    pub fn names(&self) -> Vec<&str> {
        self.collectors.keys().map(|s| s.as_str()).collect()
    }

    /// Get the count of registered collectors
    pub fn len(&self) -> usize {
        self.collectors.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.collectors.is_empty()
    }

    /// Iterate over all collectors
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Arc<dyn Collector>)> {
        self.collectors.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Create registry with all built-in collectors
    pub fn with_builtins() -> Self {
        let mut registry = Self::new();
        // Always-on baseline collector
        registry.register(Arc::new(collectors::FallbackProbeCollector));

        // Register the dummy collector for testing
        registry.register(Arc::new(collectors::DummyCollector));

        // Real collectors
        registry.register(Arc::new(collectors::RuCollector));

        // More collectors will be registered here as they're implemented
        // registry.register(Arc::new(collectors::SysmoniCollector));
        // registry.register(Arc::new(collectors::CautCollector));
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
        assert!(!result.has_warnings());
    }

    #[test]
    fn test_collect_result_failed() {
        let result = CollectResult::failed("test error");
        assert!(!result.success);
        assert_eq!(result.error, Some("test error".to_string()));
    }

    #[test]
    fn test_collect_result_with_rows() {
        let rows = vec![RowBatch {
            table: "test".to_string(),
            rows: vec![serde_json::json!({"key": "value"})],
        }];
        let result = CollectResult::with_rows(rows);
        assert!(result.success);
        assert_eq!(result.total_rows(), 1);
        assert_eq!(result.table_count(), 1);
    }

    #[test]
    fn test_collect_result_builder() {
        let result = CollectResult::empty()
            .with_cursor(Cursor::now())
            .with_duration(Duration::from_millis(100))
            .with_warning(Warning::info("test warning"));

        assert!(result.success);
        assert!(result.new_cursor.is_some());
        assert_eq!(result.duration, Duration::from_millis(100));
        assert!(result.has_warnings());
    }

    #[test]
    fn test_collector_registry() {
        let registry = CollectorRegistry::new();
        assert!(registry.names().is_empty());
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_collector_registry_with_builtins() {
        let registry = CollectorRegistry::with_builtins();
        assert!(!registry.is_empty());
        assert!(registry.get("dummy").is_some());
    }

    // Cursor tests
    #[test]
    fn test_cursor_timestamp() {
        let now = Utc::now();
        let cursor = Cursor::Timestamp(now);
        let json = cursor.to_json().unwrap();
        let parsed = Cursor::from_json(&json).unwrap();
        assert_eq!(cursor, parsed);
    }

    #[test]
    fn test_cursor_file_offset() {
        let cursor = Cursor::file_offset(12345, 67890);
        let json = cursor.to_json().unwrap();
        let parsed = Cursor::from_json(&json).unwrap();
        assert_eq!(cursor, parsed);

        match parsed {
            Cursor::FileOffset { inode, offset } => {
                assert_eq!(inode, 12345);
                assert_eq!(offset, 67890);
            }
            _ => panic!("Expected FileOffset cursor"),
        }
    }

    #[test]
    fn test_cursor_primary_key() {
        let cursor = Cursor::primary_key(42);
        let json = cursor.to_json().unwrap();
        let parsed = Cursor::from_json(&json).unwrap();
        assert_eq!(cursor, parsed);

        match parsed {
            Cursor::PrimaryKey(pk) => assert_eq!(pk, 42),
            _ => panic!("Expected PrimaryKey cursor"),
        }
    }

    #[test]
    fn test_cursor_opaque() {
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct CustomState {
            page: u32,
            token: String,
        }

        let state = CustomState {
            page: 5,
            token: "abc123".to_string(),
        };

        let cursor = Cursor::opaque(&state).unwrap();
        let parsed: CustomState = cursor.parse_opaque().unwrap();
        assert_eq!(state, parsed);
    }

    #[test]
    fn test_collect_context_local() {
        let ctx = CollectContext::local("test-machine", Duration::from_secs(30));
        assert_eq!(ctx.machine_id, "test-machine");
        assert!(ctx.is_local);
        assert_eq!(ctx.timeout, Duration::from_secs(30));
        assert!(ctx.cursor.is_none());
    }

    #[test]
    fn test_collect_context_with_cursor() {
        let ctx = CollectContext::local("test", Duration::from_secs(30))
            .with_cursor(Cursor::primary_key(100));

        assert_eq!(ctx.primary_key_cursor(), Some(100));
        assert!(ctx.timestamp_cursor().is_none());
        assert!(ctx.file_offset_cursor().is_none());
    }

    #[test]
    fn test_warning_levels() {
        let info = Warning::info("info message");
        assert_eq!(info.level, WarningLevel::Info);

        let warn = Warning::warn("warn message").with_context("some context");
        assert_eq!(warn.level, WarningLevel::Warn);
        assert_eq!(warn.context, Some("some context".to_string()));

        let error = Warning::error("error message");
        assert_eq!(error.level, WarningLevel::Error);
    }

    #[tokio::test]
    async fn test_dummy_collector() {
        let collector = collectors::DummyCollector;
        let ctx = CollectContext::local("test", Duration::from_secs(30));

        assert_eq!(collector.name(), "dummy");
        assert_eq!(collector.schema_version(), 1);
        assert!(!collector.supports_incremental());
        assert!(collector.required_tool().is_none());

        let result = collector.collect(&ctx).await.unwrap();
        assert!(result.success);
        assert_eq!(result.total_rows(), 1);
    }
}
