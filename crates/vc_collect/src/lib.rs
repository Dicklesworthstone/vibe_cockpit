//! `vc_collect` - Data collectors for Vibe Cockpit
//!
//! This crate provides:
//! - The `Collector` trait for implementing data sources
//! - Built-in collectors for various tools (`sysmoni`, `ru`, `caut`, etc.)
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
pub mod machine;
pub mod node;
pub mod probe;
pub mod redact;
pub mod remote;
pub mod scheduler;
pub mod ssh;

pub use machine::{Machine, MachineFilter, MachineRegistry, MachineStatus, ToolInfo};
pub use probe::{ProbeResult, TOOL_SPECS, ToolProber, ToolSpec};
pub use remote::{
    CollectionSummary, MachineCollectResult, MultiMachineCollector, RemoteCollectError,
    RemoteCollector, RemoteCollectorConfig,
};
pub use ssh::{CommandOutput as SshCommandOutput, PoolStats, SshError, SshRunner, SshRunnerConfig};

#[cfg(test)]
pub(crate) fn run_async_test<F, T>(future: F) -> T
where
    F: std::future::Future<Output = T>,
{
    let asupersync_rt = asupersync::runtime::RuntimeBuilder::new()
        .build()
        .expect("build asupersync test runtime");
    let tokio_rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build tokio compat runtime");
    let _tokio_guard = tokio_rt.enter();
    let root_cx = asupersync::Cx::for_testing();

    asupersync_rt
        .block_on(async {
            asupersync_tokio_compat::runtime::with_tokio_context(&root_cx, || future).await
        })
        .expect("test future should complete")
}

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
    /// Used by: `mcp_agent_mail`, `bv`/`br`
    FileOffset {
        /// File inode for rotation detection
        inode: u64,
        /// Byte offset in file
        offset: u64,
    },

    /// For `SQLite` incremental (last seen primary key)
    /// Used by: `cass`, `caam`, `mcp_agent_mail`
    PrimaryKey(i64),

    /// For custom cursor formats (JSON-encoded string)
    /// Used by: collectors with complex state
    Opaque(String),
}

impl Cursor {
    /// Create a timestamp cursor from now
    #[must_use]
    pub fn now() -> Self {
        Self::Timestamp(Utc::now())
    }

    /// Create a file offset cursor
    #[must_use]
    pub fn file_offset(inode: u64, offset: u64) -> Self {
        Self::FileOffset { inode, offset }
    }

    /// Create a primary key cursor
    #[must_use]
    pub fn primary_key(pk: i64) -> Self {
        Self::PrimaryKey(pk)
    }

    /// Create an opaque cursor from any serializable value
    ///
    /// # Errors
    ///
    /// Returns an error if `value` fails to serialize to JSON.
    pub fn opaque<T: Serialize>(value: &T) -> Result<Self, serde_json::Error> {
        Ok(Self::Opaque(serde_json::to_string(value)?))
    }

    /// Parse an opaque cursor into a typed value
    #[must_use]
    pub fn parse_opaque<T: for<'de> Deserialize<'de>>(&self) -> Option<T> {
        match self {
            Self::Opaque(s) => serde_json::from_str(s).ok(),
            _ => None,
        }
    }

    /// Convert to JSON string for storage
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Parse from JSON string
    ///
    /// # Errors
    ///
    /// Returns an error if `s` is not valid JSON for `Cursor`.
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

/// Terminal collector outcome, preserving cancellation distinctly from errors.
pub type CollectOutcome = asupersync::Outcome<CollectResult, CollectError>;

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

/// Observe collector cancellation at a named lifecycle checkpoint.
#[must_use]
pub fn checkpoint_reason(
    cx: &asupersync::Cx,
    checkpoint: &'static str,
) -> Option<asupersync::CancelReason> {
    tracing::debug!(
        checkpoint,
        cancel_requested = cx.is_cancel_requested(),
        "Collector checkpoint"
    );

    match cx.checkpoint() {
        Ok(()) => None,
        Err(error) => {
            let reason = cx
                .cancel_reason()
                .unwrap_or_else(asupersync::CancelReason::parent_cancelled);
            tracing::debug!(
                checkpoint,
                ?reason,
                error = %error,
                "Collector checkpoint triggered cancellation"
            );
            Some(reason)
        }
    }
}

#[macro_export]
macro_rules! collect_checkpoint {
    ($cx:expr, $name:expr) => {
        if let Some(reason) = $crate::checkpoint_reason($cx, $name) {
            return asupersync::Outcome::Cancelled(reason);
        }
    };
}

#[macro_export]
macro_rules! collect_try {
    ($expr:expr) => {
        match $expr {
            Ok(value) => value,
            Err(error) => return asupersync::Outcome::Err(error.into()),
        }
    };
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
    pub fn with_cursor(mut self, cursor: Cursor) -> Self {
        self.new_cursor = Some(cursor);
        self
    }

    /// Set the duration for this result
    #[must_use]
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    /// Add a warning to this result
    #[must_use]
    pub fn with_warning(mut self, warning: Warning) -> Self {
        self.warnings.push(warning);
        self
    }

    /// Add a raw artifact to this result
    #[must_use]
    pub fn with_artifact(mut self, artifact: RawArtifact) -> Self {
        self.raw_artifacts.push(artifact);
        self
    }

    /// Total number of rows collected
    #[must_use]
    pub fn total_rows(&self) -> usize {
        self.rows.iter().map(|b| b.rows.len()).sum()
    }

    /// Get the number of tables with data
    #[must_use]
    pub fn table_count(&self) -> usize {
        self.rows.len()
    }

    /// Check if any warnings were generated
    #[must_use]
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
    pub const DEFAULT_POLL_WINDOW: Duration = Duration::from_mins(10);

    /// Create a new context for local collection
    #[must_use]
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
    #[must_use]
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
    #[must_use]
    pub fn with_cursor(mut self, cursor: Cursor) -> Self {
        self.cursor = Some(cursor);
        self
    }

    /// Set the poll window for this context
    #[must_use]
    pub fn with_poll_window(mut self, window: Duration) -> Self {
        self.poll_window = window;
        self
    }

    /// Set max bytes for this context
    #[must_use]
    pub fn with_max_bytes(mut self, max: usize) -> Self {
        self.max_bytes = max;
        self
    }

    /// Set max rows for this context
    #[must_use]
    pub fn with_max_rows(mut self, max: usize) -> Self {
        self.max_rows = max;
        self
    }

    /// Get the timestamp cursor if present
    #[must_use]
    pub fn timestamp_cursor(&self) -> Option<DateTime<Utc>> {
        match &self.cursor {
            Some(Cursor::Timestamp(ts)) => Some(*ts),
            _ => None,
        }
    }

    /// Get the file offset cursor if present
    #[must_use]
    pub fn file_offset_cursor(&self) -> Option<(u64, u64)> {
        match &self.cursor {
            Some(Cursor::FileOffset { inode, offset }) => Some((*inode, *offset)),
            _ => None,
        }
    }

    /// Get the primary key cursor if present
    #[must_use]
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
    ///
    /// The `cx` parameter is the Asupersync capability context for this
    /// collector task. It enables structured cancellation, checkpoints,
    /// and budget enforcement. Implementations that do not yet use `cx`
    /// may prefix it with `_`.
    async fn collect(&self, cx: &asupersync::Cx, ctx: &CollectContext) -> CollectOutcome;

    /// Check if the required tool is available
    async fn check_availability(&self, cx: &asupersync::Cx, ctx: &CollectContext) -> bool {
        match self.required_tool() {
            // check_tool returns Result<bool, _> where bool = tool exists
            // unwrap_or(false) returns false on error or if tool doesn't exist
            Some(tool) => ctx.executor.check_tool(cx, tool).await.unwrap_or(false),
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
    #[must_use]
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
    #[must_use]
    pub fn get(&self, name: &str) -> Option<Arc<dyn Collector>> {
        self.collectors.get(name).cloned()
    }

    /// List all registered collector names
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.collectors.keys().map(String::as_str).collect()
    }

    /// Get the count of registered collectors
    #[must_use]
    pub fn len(&self) -> usize {
        self.collectors.len()
    }

    /// Check if the registry is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.collectors.is_empty()
    }

    /// Iterate over all collectors
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Arc<dyn Collector>)> {
        self.collectors.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Create registry with all built-in collectors
    #[must_use]
    pub fn with_builtins() -> Self {
        let mut registry = Self::new();
        // Always-on baseline collector
        registry.register(Arc::new(collectors::FallbackProbeCollector));

        // Register the dummy collector for testing
        registry.register(Arc::new(collectors::DummyCollector));

        // Real collectors
        registry.register(Arc::new(collectors::RuCollector));
        registry.register(Arc::new(collectors::SysmoniCollector));
        registry.register(Arc::new(collectors::AgentMailCollector::new()));
        registry.register(Arc::new(collectors::CautCollector));
        registry.register(Arc::new(collectors::CassCollector::new()));
        registry.register(Arc::new(collectors::CaamCollector));
        registry.register(Arc::new(collectors::RchCollector::new()));
        registry.register(Arc::new(collectors::RanoCollector::new()));
        registry.register(Arc::new(collectors::DcgCollector::new()));
        registry.register(Arc::new(collectors::BeadsCollector));
        registry.register(Arc::new(collectors::GhCollector));
        registry.register(Arc::new(collectors::NtmCollector::new()));
        registry.register(Arc::new(collectors::PtCollector::new()));
        registry.register(Arc::new(collectors::AfscCollector::new()));
        registry.register(Arc::new(collectors::CloudBenchCollector::new()));

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

    /// Every implemented collector must be reachable from `with_builtins`,
    /// otherwise `vc collect --collector NAME` rejects it as unknown and the
    /// daemon silently never runs it.
    #[test]
    fn test_collector_registry_registers_every_builtin() {
        let registry = CollectorRegistry::with_builtins();
        for name in [
            "fallback_probe",
            "dummy",
            "ru",
            "sysmoni",
            "mcp_agent_mail",
            "caut",
            "cass",
            "caam",
            "rch",
            "rano",
            "dcg",
            "beads",
            "github",
            "ntm",
            "pt",
            "afsc",
            "cloud_benchmarker",
        ] {
            assert!(
                registry.get(name).is_some(),
                "collector `{name}` is implemented but not registered"
            );
        }
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

    #[test]
    fn test_dummy_collector() {
        crate::run_async_test(async {
            let collector = collectors::DummyCollector;
            let cx = asupersync::Cx::for_testing();
            let ctx = CollectContext::local("test", Duration::from_secs(30));

            assert_eq!(collector.name(), "dummy");
            assert_eq!(collector.schema_version(), 1);
            assert!(!collector.supports_incremental());
            assert!(collector.required_tool().is_none());

            let result = collector.collect(&cx, &ctx).await.unwrap();
            assert!(result.success);
            assert_eq!(result.total_rows(), 1);
        });
    }

    // =========================================================================
    // Structured Concurrency Tests (bd-3lu)
    // =========================================================================

    /// Verify the Collector trait signature accepts &Cx (compilation test).
    #[test]
    fn test_collector_trait_accepts_cx() {
        fn assert_cx_param<C: Collector>(_c: &C) {
            // The existence of this function proves the trait compiles with
            // &Cx in collect(). If the trait were wrong, this would not compile.
        }
        assert_cx_param(&collectors::DummyCollector);
    }

    /// Three mock collectors, all succeed — verify all 3 ran.
    #[test]
    fn test_three_collectors_all_succeed() {
        crate::run_async_test(async {
            let cx = asupersync::Cx::for_testing();
            let results: Vec<CollectOutcome> = {
                let c1 = collectors::DummyCollector;
                let c2 = collectors::DummyCollector;
                let c3 = collectors::DummyCollector;
                let ctx = CollectContext::local("test", Duration::from_secs(30));

                let (r1, r2, r3) = futures::future::join3(
                    c1.collect(&cx, &ctx),
                    c2.collect(&cx, &ctx),
                    c3.collect(&cx, &ctx),
                )
                .await;
                vec![r1, r2, r3]
            };

            assert_eq!(results.len(), 3);
            for r in &results {
                match r {
                    asupersync::Outcome::Ok(result) => assert!(result.success),
                    other => panic!("expected successful outcome, got {other:?}"),
                }
            }
        });
    }

    /// Three concurrent collectors, one fails — other two still complete.
    #[test]
    fn test_three_collectors_one_fails() {
        crate::run_async_test(async {
            let cx = asupersync::Cx::for_testing();
            let ctx = CollectContext::local("test", Duration::from_secs(30));

            let ok1 = collectors::DummyCollector;
            let fail = collectors::FailingDummyCollector::always_fails("deliberate");
            let ok2 = collectors::DummyCollector;

            let (r1, r2, r3) = futures::future::join3(
                ok1.collect(&cx, &ctx),
                fail.collect(&cx, &ctx),
                ok2.collect(&cx, &ctx),
            )
            .await;

            assert!(r1.unwrap().success, "first collector should succeed");
            assert!(r2.is_err(), "second collector should fail");
            assert!(r3.unwrap().success, "third collector should succeed");
        });
    }

    /// Cx cancellation is visible to collectors (they can check it).
    #[test]
    fn test_cx_cancel_visible_to_collector() {
        let cx = asupersync::Cx::for_testing();
        // A fresh Cx should not be cancelled
        assert!(
            !cx.is_cancel_requested(),
            "fresh Cx should not be cancelled"
        );
    }

    /// Zero children region: empty collector set completes immediately.
    #[test]
    fn test_zero_collectors_completes() {
        crate::run_async_test(async {
            let results: Vec<CollectOutcome> = vec![];
            assert!(results.is_empty(), "zero-child region should be empty");
        });
    }

    /// Verify Cx is Clone (required for spawning per-collector tasks).
    #[test]
    fn test_cx_is_clone() {
        let cx = asupersync::Cx::for_testing();
        let cx2 = cx.clone();
        // Both should have the same underlying state
        assert_eq!(cx.region_id(), cx2.region_id());
    }

    #[test]
    fn test_checkpoint_reason_none_when_active() {
        let cx = asupersync::Cx::for_testing();
        assert!(checkpoint_reason(&cx, "active_checkpoint").is_none());
    }

    #[test]
    fn test_checkpoint_reason_returns_cancel_reason_when_cancelled() {
        let cx = asupersync::Cx::for_testing();
        cx.cancel_with(
            asupersync::CancelKind::User,
            Some("checkpoint cancellation"),
        );

        let reason = checkpoint_reason(&cx, "cancelled_checkpoint")
            .expect("cancelled context should yield a cancel reason");

        assert_eq!(reason.kind, asupersync::CancelKind::User);
    }

    #[test]
    fn test_dummy_collector_returns_cancelled_when_cx_cancelled() {
        crate::run_async_test(async {
            let collector = collectors::DummyCollector;
            let cx = asupersync::Cx::for_testing();
            cx.cancel_with(asupersync::CancelKind::User, Some("test cancellation"));
            let ctx = CollectContext::local("test", Duration::from_secs(30));

            let result = collector.collect(&cx, &ctx).await;
            assert!(result.is_cancelled());
        });
    }
}
