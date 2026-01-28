//! rch collector - Remote Compilation Helper metrics
//!
//! This collector uses the JSONL Tail ingestion pattern to collect
//! compilation metrics from rch (Remote Compilation Helper).
//!
//! ## Integration Method
//! Reads JSONL from `~/.rch/compilations.jsonl`
//!
//! ## Tables Populated
//! - `rch_compilations`: Individual compilation records
//! - `rch_metrics`: Aggregated metrics snapshot (from rch status)

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Instant;

use crate::{CollectContext, CollectError, CollectResult, Collector, Cursor, RowBatch, Warning};

/// Default path to the rch compilations JSONL file
pub const DEFAULT_JSONL_PATH: &str = "~/.rch/compilations.jsonl";

/// A compilation record from rch JSONL
#[derive(Debug, Deserialize, Serialize)]
pub struct RchCompilation {
    /// Timestamp of compilation
    #[serde(default)]
    pub ts: Option<String>,

    /// Crate name being compiled
    #[serde(rename = "crate", default)]
    pub crate_name: String,

    /// Crate version
    #[serde(default)]
    pub version: Option<String>,

    /// Build profile (debug, release)
    #[serde(default)]
    pub profile: String,

    /// Target triple
    #[serde(default)]
    pub target: Option<String>,

    /// Compilation duration in milliseconds
    #[serde(default)]
    pub duration_ms: u64,

    /// Whether this was a cache hit
    #[serde(default)]
    pub cache_hit: bool,

    /// Cache key if applicable
    #[serde(default)]
    pub cache_key: Option<String>,

    /// Worker that performed the compilation
    #[serde(default)]
    pub worker: Option<String>,

    /// Exit code (0 = success)
    #[serde(default)]
    pub exit_code: Option<i32>,

    /// Error message if failed
    #[serde(default)]
    pub error: Option<String>,

    /// CPU time in milliseconds
    #[serde(default)]
    pub cpu_time_ms: Option<u64>,

    /// Peak memory usage in MB
    #[serde(default)]
    pub peak_memory_mb: Option<u64>,
}

/// Output from `rch status --json`
#[derive(Debug, Deserialize)]
pub struct RchStatus {
    /// Queue depth
    #[serde(default)]
    pub queue_depth: i32,

    /// Active workers
    #[serde(default)]
    pub workers_active: i32,

    /// Total workers
    #[serde(default)]
    pub workers_total: i32,

    /// Jobs completed
    #[serde(default)]
    pub jobs_completed: i64,

    /// Jobs failed
    #[serde(default)]
    pub jobs_failed: i64,

    /// Average job duration in ms
    #[serde(default)]
    pub avg_job_duration_ms: i64,
}

/// rch collector for remote compilation metrics
///
/// Collects compilation metrics from rch JSONL logs using the
/// JSONL Tail pattern (reads new lines since last offset).
pub struct RchCollector {
    /// Path to the JSONL file (with ~ expansion)
    jsonl_path: String,
}

impl RchCollector {
    /// Create a new collector with the default JSONL path
    pub fn new() -> Self {
        Self {
            jsonl_path: DEFAULT_JSONL_PATH.to_string(),
        }
    }

    /// Create a collector with a custom JSONL path
    pub fn with_path(path: impl Into<String>) -> Self {
        Self {
            jsonl_path: path.into(),
        }
    }

    /// Expand ~ to home directory in the path
    fn expand_path(&self) -> String {
        if self.jsonl_path.starts_with("~/") {
            if let Ok(home) = std::env::var("HOME") {
                return self.jsonl_path.replacen("~", &home, 1);
            }
        }
        self.jsonl_path.clone()
    }
}

impl Default for RchCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Collector for RchCollector {
    fn name(&self) -> &'static str {
        "rch"
    }

    fn schema_version(&self) -> u32 {
        1
    }

    fn required_tool(&self) -> Option<&'static str> {
        Some("rch")
    }

    fn supports_incremental(&self) -> bool {
        true
    }

    async fn collect(&self, ctx: &CollectContext) -> Result<CollectResult, CollectError> {
        let start = Instant::now();
        let mut warnings = Vec::new();
        let jsonl_path = self.expand_path();

        // Check if rch is available
        if !self.check_availability(ctx).await {
            return Err(CollectError::ToolNotFound("rch".to_string()));
        }

        // Get file info and check if it exists
        let file_stat = ctx.executor.stat(&jsonl_path, ctx.timeout).await?;
        if !file_stat.exists {
            warnings.push(Warning::info(format!(
                "rch JSONL file not found: {}",
                jsonl_path
            )));
            return Ok(CollectResult::empty()
                .with_warning(Warning::info("JSONL file not found"))
                .with_duration(start.elapsed()));
        }

        // Get current offset from cursor
        let (last_inode, last_offset) = ctx.file_offset_cursor().unwrap_or((0, 0));

        // Check for file rotation (inode changed)
        let current_inode = file_stat.inode;
        let start_offset = if current_inode != last_inode {
            // File was rotated, start from beginning
            warnings.push(Warning::info("JSONL file rotated, starting from beginning"));
            0
        } else {
            last_offset
        };

        // Read new lines from the file
        let content_bytes = if start_offset > 0 {
            ctx.executor
                .read_file_range(&jsonl_path, start_offset, ctx.timeout)
                .await?
        } else {
            ctx.executor.read_file(&jsonl_path, ctx.timeout).await?
        };

        // Convert to string
        let content = String::from_utf8_lossy(&content_bytes);

        // Parse JSONL lines
        let mut compilation_rows = Vec::new();
        let mut bytes_read = 0u64;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                bytes_read += 1; // newline
                continue;
            }

            bytes_read += line.len() as u64 + 1; // +1 for newline

            match serde_json::from_str::<RchCompilation>(line) {
                Ok(compilation) => {
                    compilation_rows.push(serde_json::json!({
                        "machine_id": ctx.machine_id,
                        "collected_at": ctx.collected_at.to_rfc3339(),
                        "worker_host": compilation.worker,
                        "crate_name": compilation.crate_name,
                        "crate_version": compilation.version,
                        "profile": compilation.profile,
                        "target_triple": compilation.target,
                        "started_at": compilation.ts,
                        "duration_ms": compilation.duration_ms,
                        "cache_hit": compilation.cache_hit,
                        "cache_key": compilation.cache_key,
                        "exit_code": compilation.exit_code,
                        "error_msg": compilation.error,
                        "cpu_time_ms": compilation.cpu_time_ms,
                        "peak_memory_mb": compilation.peak_memory_mb,
                        "raw_json": line,
                    }));
                }
                Err(e) => {
                    warnings.push(Warning::warn(format!(
                        "Failed to parse JSONL line: {}",
                        e
                    )));
                }
            }

            // Limit rows per collection
            if compilation_rows.len() >= ctx.max_rows {
                break;
            }
        }

        // Also try to get worker status via rch status
        let mut metrics_rows = Vec::new();
        match ctx
            .executor
            .run_timeout("rch status --json", ctx.timeout)
            .await
        {
            Ok(output) => {
                if let Ok(status) = serde_json::from_str::<RchStatus>(&output) {
                    metrics_rows.push(serde_json::json!({
                        "machine_id": ctx.machine_id,
                        "collected_at": ctx.collected_at.to_rfc3339(),
                        "queue_depth": status.queue_depth,
                        "workers_active": status.workers_active,
                        "workers_total": status.workers_total,
                        "jobs_completed": status.jobs_completed,
                        "jobs_failed": status.jobs_failed,
                        "avg_job_duration_ms": status.avg_job_duration_ms,
                        "raw_json": output,
                    }));
                }
            }
            Err(e) => {
                warnings.push(Warning::warn(format!(
                    "Failed to get rch status: {}",
                    e
                )));
            }
        }

        // Build result
        let new_offset = start_offset + bytes_read;
        let mut batches = Vec::new();

        if !compilation_rows.is_empty() {
            batches.push(RowBatch {
                table: "rch_compilations".to_string(),
                rows: compilation_rows,
            });
        }

        if !metrics_rows.is_empty() {
            batches.push(RowBatch {
                table: "rch_metrics".to_string(),
                rows: metrics_rows,
            });
        }

        let mut result = CollectResult::with_rows(batches)
            .with_cursor(Cursor::file_offset(current_inode, new_offset))
            .with_duration(start.elapsed());

        for warning in warnings {
            result = result.with_warning(warning);
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collector_name() {
        let collector = RchCollector::new();
        assert_eq!(collector.name(), "rch");
    }

    #[test]
    fn test_collector_schema_version() {
        let collector = RchCollector::new();
        assert_eq!(collector.schema_version(), 1);
    }

    #[test]
    fn test_collector_supports_incremental() {
        let collector = RchCollector::new();
        assert!(collector.supports_incremental());
    }

    #[test]
    fn test_collector_required_tool() {
        let collector = RchCollector::new();
        assert_eq!(collector.required_tool(), Some("rch"));
    }

    #[test]
    fn test_default_jsonl_path() {
        let collector = RchCollector::new();
        assert_eq!(collector.jsonl_path, DEFAULT_JSONL_PATH);
    }

    #[test]
    fn test_custom_jsonl_path() {
        let collector = RchCollector::with_path("/custom/path/compilations.jsonl");
        assert_eq!(collector.jsonl_path, "/custom/path/compilations.jsonl");
    }

    #[test]
    fn test_path_expansion() {
        let collector = RchCollector::new();
        let expanded = collector.expand_path();

        if std::env::var("HOME").is_ok() {
            assert!(!expanded.starts_with("~/"));
            assert!(expanded.contains(".rch"));
        }
    }

    #[test]
    fn test_path_expansion_no_tilde() {
        let collector = RchCollector::with_path("/absolute/path/compilations.jsonl");
        let expanded = collector.expand_path();
        assert_eq!(expanded, "/absolute/path/compilations.jsonl");
    }

    #[test]
    fn test_default_impl() {
        let collector = RchCollector::default();
        assert_eq!(collector.jsonl_path, DEFAULT_JSONL_PATH);
    }

    #[test]
    fn test_parse_compilation_full() {
        let json = r#"{
            "ts": "2026-01-27T10:00:00Z",
            "crate": "serde",
            "version": "1.0.200",
            "profile": "release",
            "target": "x86_64-unknown-linux-gnu",
            "duration_ms": 12340,
            "cache_hit": false,
            "cache_key": "abc123",
            "worker": "mini-1",
            "exit_code": 0,
            "cpu_time_ms": 11000,
            "peak_memory_mb": 512
        }"#;

        let record: RchCompilation = serde_json::from_str(json).unwrap();
        assert_eq!(record.crate_name, "serde");
        assert_eq!(record.version, Some("1.0.200".to_string()));
        assert_eq!(record.profile, "release");
        assert_eq!(record.duration_ms, 12340);
        assert!(!record.cache_hit);
        assert_eq!(record.worker, Some("mini-1".to_string()));
        assert_eq!(record.exit_code, Some(0));
    }

    #[test]
    fn test_parse_compilation_minimal() {
        let json = r#"{
            "crate": "tokio",
            "profile": "debug",
            "duration_ms": 5000
        }"#;

        let record: RchCompilation = serde_json::from_str(json).unwrap();
        assert_eq!(record.crate_name, "tokio");
        assert_eq!(record.profile, "debug");
        assert_eq!(record.duration_ms, 5000);
        assert!(record.version.is_none());
        assert!(record.worker.is_none());
        assert!(!record.cache_hit);
    }

    #[test]
    fn test_parse_compilation_cache_hit() {
        let json = r#"{
            "crate": "libc",
            "profile": "release",
            "duration_ms": 100,
            "cache_hit": true,
            "cache_key": "xyz789"
        }"#;

        let record: RchCompilation = serde_json::from_str(json).unwrap();
        assert!(record.cache_hit);
        assert_eq!(record.cache_key, Some("xyz789".to_string()));
        assert_eq!(record.duration_ms, 100);
    }

    #[test]
    fn test_parse_compilation_with_error() {
        let json = r#"{
            "crate": "broken-crate",
            "profile": "debug",
            "duration_ms": 1000,
            "exit_code": 1,
            "error": "compilation failed: missing dependency"
        }"#;

        let record: RchCompilation = serde_json::from_str(json).unwrap();
        assert_eq!(record.exit_code, Some(1));
        assert_eq!(
            record.error,
            Some("compilation failed: missing dependency".to_string())
        );
    }

    #[test]
    fn test_parse_status() {
        let json = r#"{
            "queue_depth": 5,
            "workers_active": 3,
            "workers_total": 4,
            "jobs_completed": 1250,
            "jobs_failed": 12,
            "avg_job_duration_ms": 8500
        }"#;

        let status: RchStatus = serde_json::from_str(json).unwrap();
        assert_eq!(status.queue_depth, 5);
        assert_eq!(status.workers_active, 3);
        assert_eq!(status.workers_total, 4);
        assert_eq!(status.jobs_completed, 1250);
        assert_eq!(status.jobs_failed, 12);
        assert_eq!(status.avg_job_duration_ms, 8500);
    }

    #[test]
    fn test_parse_status_empty() {
        let json = r#"{}"#;

        let status: RchStatus = serde_json::from_str(json).unwrap();
        assert_eq!(status.queue_depth, 0);
        assert_eq!(status.workers_active, 0);
        assert_eq!(status.workers_total, 0);
    }

    #[test]
    fn test_compilation_serialize() {
        let record = RchCompilation {
            ts: Some("2026-01-27T10:00:00Z".to_string()),
            crate_name: "test".to_string(),
            version: Some("1.0.0".to_string()),
            profile: "debug".to_string(),
            target: None,
            duration_ms: 1000,
            cache_hit: true,
            cache_key: None,
            worker: None,
            exit_code: Some(0),
            error: None,
            cpu_time_ms: None,
            peak_memory_mb: None,
        };

        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("\"crate\":\"test\""));
        assert!(json.contains("\"cache_hit\":true"));
    }
}
