//! pt collector - Process Tracker metrics
//!
//! This collector uses the CLI Incremental pattern to collect
//! process lifecycle and resource usage from the `pt` tool.
//!
//! ## Integration Method
//! ```bash
//! pt list --robot --json --since <timestamp>
//! ```
//!
//! ## Tables Populated
//! - `pt_processes`: Process lifecycle and metadata
//! - `pt_snapshots`: Resource usage snapshots

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Instant;

use crate::{
    CollectContext, CollectError, CollectOutcome, CollectResult, Collector, Cursor, RowBatch,
    Warning,
};

/// A process record from pt output
#[derive(Debug, Deserialize, Serialize)]
pub struct PtProcess {
    /// Process ID
    #[serde(default)]
    pub pid: i32,

    /// Parent process ID
    #[serde(default)]
    pub ppid: Option<i32>,

    /// Process name
    #[serde(default)]
    pub name: String,

    /// Full command line
    #[serde(default)]
    pub cmdline: Option<String>,

    /// User running the process
    #[serde(default)]
    pub user: Option<String>,

    /// Process start time
    #[serde(default)]
    pub started_at: Option<String>,

    /// Process end time (if terminated)
    #[serde(default)]
    pub ended_at: Option<String>,

    /// Exit code (if terminated)
    #[serde(default)]
    pub exit_code: Option<i32>,

    /// CPU percentage
    #[serde(default)]
    pub cpu_percent: Option<f64>,

    /// Memory usage in MB
    #[serde(default)]
    pub memory_mb: Option<f64>,

    /// Memory percentage
    #[serde(default)]
    pub memory_percent: Option<f64>,

    /// Thread count
    #[serde(default)]
    pub threads: Option<i32>,

    /// Number of open files
    #[serde(default)]
    pub open_files: Option<i32>,

    /// Bytes read
    #[serde(default)]
    pub io_read_bytes: Option<i64>,

    /// Bytes written
    #[serde(default)]
    pub io_write_bytes: Option<i64>,

    /// Process status (running, sleeping, stopped, zombie)
    #[serde(default)]
    pub status: Option<String>,

    /// Process category (agent, build, runtime, other)
    #[serde(default)]
    pub category: Option<String>,

    /// Linked session ID (if applicable)
    #[serde(default)]
    pub session_id: Option<String>,
}

/// Output from `pt list --robot --json`
#[derive(Debug, Deserialize)]
pub struct PtOutput {
    /// Active processes
    #[serde(default)]
    pub processes: Vec<PtProcess>,

    /// Recently ended processes
    #[serde(default)]
    pub ended: Vec<PtEndedProcess>,
}

/// An ended process record
#[derive(Debug, Deserialize)]
pub struct PtEndedProcess {
    /// Process ID
    #[serde(default)]
    pub pid: i32,

    /// Exit code
    #[serde(default)]
    pub exit_code: Option<i32>,

    /// End time
    #[serde(default)]
    pub ended_at: Option<String>,
}

/// pt collector for process tracking
///
/// Collects process lifecycle and resource usage using a
/// timestamp-based cursor for incremental collection.
pub struct PtCollector {
    /// Default lookback window (e.g., "1h")
    lookback_window: String,
}

impl PtCollector {
    /// Create a new collector with default 1-hour lookback
    #[must_use]
    pub fn new() -> Self {
        Self {
            lookback_window: "1h".to_string(),
        }
    }

    /// Create a collector with a custom lookback window
    #[must_use]
    pub fn with_window(window: impl Into<String>) -> Self {
        Self {
            lookback_window: window.into(),
        }
    }

    /// Categorize a process based on its name and command line
    #[must_use]
    pub fn categorize_process(name: &str, cmdline: Option<&str>) -> &'static str {
        let name_lower = name.to_lowercase();
        let cmdline_lower = cmdline.map(str::to_lowercase).unwrap_or_default();

        // Check for AI agent processes
        if name_lower.contains("claude")
            || name_lower.contains("codex")
            || name_lower.contains("gemini")
            || name_lower.contains("grok")
            || cmdline_lower.contains("claude-code")
            || cmdline_lower.contains("codex-cli")
        {
            return "agent";
        }

        // Check for build processes
        if name_lower.contains("cargo")
            || name_lower.contains("rustc")
            || name_lower.contains("npm")
            || name_lower.contains("yarn")
            || name_lower.contains("pnpm")
            || name_lower.contains("tsc")
            || name_lower.contains("webpack")
            || name_lower.contains("esbuild")
            || name_lower.contains("make")
            || name_lower.contains("cmake")
            || name_lower.contains("gcc")
            || name_lower.contains("clang")
            || name_lower.contains("go build")
        {
            return "build";
        }

        // Check for runtime processes
        if name_lower.contains("node")
            || name_lower.contains("python")
            || name_lower.contains("ruby")
            || name_lower.contains("java")
            || name_lower.contains("dotnet")
            || name_lower.contains("php")
        {
            return "runtime";
        }

        "other"
    }
}

impl Default for PtCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Collector for PtCollector {
    fn name(&self) -> &'static str {
        "pt"
    }

    fn schema_version(&self) -> u32 {
        1
    }

    fn required_tool(&self) -> Option<&'static str> {
        Some("pt")
    }

    fn supports_incremental(&self) -> bool {
        true
    }

    #[allow(clippy::too_many_lines)]
    async fn collect(&self, cx: &asupersync::Cx, ctx: &CollectContext) -> CollectOutcome {
        let start = Instant::now();
        let mut warnings = Vec::new();
        crate::collect_checkpoint!(cx, "collect_start");

        // Check if pt is available
        if !self.check_availability(cx, ctx).await {
            return asupersync::Outcome::Err(CollectError::ToolNotFound("pt".to_string()));
        }

        // Get last timestamp from cursor for incremental collection
        let since_opt = ctx.timestamp_cursor();

        // Build the command
        let cmd = if let Some(since) = since_opt {
            format!(
                "pt list --robot --json --since {}",
                since.format("%Y-%m-%dT%H:%M:%SZ")
            )
        } else {
            format!("pt list --robot --json --since {}", self.lookback_window)
        };

        // Run the command
        crate::collect_checkpoint!(cx, "pre_pt_list_command");
        let output = match ctx.executor.run_timeout(cx, &cmd, ctx.timeout).await {
            Ok(out) => out,
            Err(e) => {
                warnings.push(Warning::warn(format!("pt list failed: {e}")));
                let result = CollectResult::empty()
                    .with_warning(Warning::warn(format!("pt list failed: {e}")))
                    .with_duration(start.elapsed());
                crate::collect_checkpoint!(cx, "collect_complete");
                return asupersync::Outcome::Ok(result);
            }
        };

        // Parse the JSON output
        crate::collect_checkpoint!(cx, "post_pt_list_command_pre_parse");
        let pt_output: PtOutput = match serde_json::from_str(&output) {
            Ok(o) => o,
            Err(e) => {
                warnings.push(Warning::warn(format!("Failed to parse pt output: {e}")));
                let result = CollectResult::empty()
                    .with_warning(Warning::warn(format!("Failed to parse pt output: {e}")))
                    .with_duration(start.elapsed());
                crate::collect_checkpoint!(cx, "collect_complete");
                return asupersync::Outcome::Ok(result);
            }
        };

        let mut process_rows = Vec::new();
        let mut snapshot_rows = Vec::new();
        let mut max_ts: Option<DateTime<Utc>> = since_opt;

        // Process active processes
        for proc in &pt_output.processes {
            // Track max timestamp for cursor
            if let Some(ts_str) = &proc.started_at
                && let Ok(ts) = DateTime::parse_from_rfc3339(ts_str)
            {
                let ts_utc = ts.with_timezone(&Utc);
                if max_ts.is_none() || Some(ts_utc) > max_ts {
                    max_ts = Some(ts_utc);
                }
            }

            // Determine category
            let category = proc
                .category
                .as_deref()
                .unwrap_or_else(|| Self::categorize_process(&proc.name, proc.cmdline.as_deref()));

            // Process row
            process_rows.push(serde_json::json!({
                "machine_id": ctx.machine_id,
                "collected_at": ctx.collected_at.to_rfc3339(),
                "pid": proc.pid,
                "ppid": proc.ppid,
                "name": proc.name,
                "cmdline": proc.cmdline,
                "user": proc.user,
                "started_at": proc.started_at,
                "ended_at": proc.ended_at,
                "exit_code": proc.exit_code,
                "status": proc.status,
                "category": category,
                "session_id": proc.session_id,
            }));

            // Snapshot row for current resource usage
            if proc.cpu_percent.is_some() || proc.memory_mb.is_some() {
                snapshot_rows.push(serde_json::json!({
                    "machine_id": ctx.machine_id,
                    "collected_at": ctx.collected_at.to_rfc3339(),
                    "pid": proc.pid,
                    "snapshot_at": ctx.collected_at.to_rfc3339(),
                    "cpu_percent": proc.cpu_percent,
                    "memory_mb": proc.memory_mb,
                    "memory_percent": proc.memory_percent,
                    "threads": proc.threads,
                    "open_files": proc.open_files,
                    "io_read_bytes": proc.io_read_bytes,
                    "io_write_bytes": proc.io_write_bytes,
                }));
            }

            // Limit rows per collection
            if process_rows.len() >= ctx.max_rows {
                break;
            }
        }

        // Process ended processes (update existing records)
        for ended in &pt_output.ended {
            if let Some(ts_str) = &ended.ended_at
                && let Ok(ts) = DateTime::parse_from_rfc3339(ts_str)
            {
                let ts_utc = ts.with_timezone(&Utc);
                if max_ts.is_none() || Some(ts_utc) > max_ts {
                    max_ts = Some(ts_utc);
                }
            }

            // Add as a process row with ended info
            process_rows.push(serde_json::json!({
                "machine_id": ctx.machine_id,
                "collected_at": ctx.collected_at.to_rfc3339(),
                "pid": ended.pid,
                "ended_at": ended.ended_at,
                "exit_code": ended.exit_code,
                "status": "ended",
            }));
        }

        // Build result
        let mut batches = Vec::new();
        if !process_rows.is_empty() {
            batches.push(RowBatch {
                table: "pt_processes".to_string(),
                rows: process_rows,
            });
        }
        if !snapshot_rows.is_empty() {
            batches.push(RowBatch {
                table: "pt_snapshots".to_string(),
                rows: snapshot_rows,
            });
        }

        let mut result = CollectResult::with_rows(batches).with_duration(start.elapsed());

        // Update cursor if we have a new max timestamp
        if let Some(ts) = max_ts {
            result = result.with_cursor(Cursor::Timestamp(ts));
        } else if let Some(cursor) = &ctx.cursor {
            // Preserve existing cursor
            result = result.with_cursor(cursor.clone());
        }

        // Add warnings
        for warning in warnings {
            result = result.with_warning(warning);
        }

        crate::collect_checkpoint!(cx, "collect_complete");
        asupersync::Outcome::Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collector_name() {
        let collector = PtCollector::new();
        assert_eq!(collector.name(), "pt");
    }

    #[test]
    fn test_collector_schema_version() {
        let collector = PtCollector::new();
        assert_eq!(collector.schema_version(), 1);
    }

    #[test]
    fn test_collector_supports_incremental() {
        let collector = PtCollector::new();
        assert!(collector.supports_incremental());
    }

    #[test]
    fn test_collector_required_tool() {
        let collector = PtCollector::new();
        assert_eq!(collector.required_tool(), Some("pt"));
    }

    #[test]
    fn test_default_lookback_window() {
        let collector = PtCollector::new();
        assert_eq!(collector.lookback_window, "1h");
    }

    #[test]
    fn test_custom_lookback_window() {
        let collector = PtCollector::with_window("30m");
        assert_eq!(collector.lookback_window, "30m");
    }

    #[test]
    fn test_default_impl() {
        let collector = PtCollector::default();
        assert_eq!(collector.lookback_window, "1h");
    }

    #[test]
    fn test_categorize_agent_claude() {
        assert_eq!(
            PtCollector::categorize_process("claude-code", None),
            "agent"
        );
        assert_eq!(
            PtCollector::categorize_process("node", Some("claude-code --project /foo")),
            "agent"
        );
    }

    #[test]
    fn test_categorize_agent_codex() {
        assert_eq!(PtCollector::categorize_process("codex-cli", None), "agent");
        assert_eq!(
            PtCollector::categorize_process("node", Some("/usr/bin/codex-cli")),
            "agent"
        );
    }

    #[test]
    fn test_categorize_agent_gemini() {
        assert_eq!(PtCollector::categorize_process("gemini", None), "agent");
    }

    #[test]
    fn test_categorize_agent_grok() {
        assert_eq!(PtCollector::categorize_process("grok", None), "agent");
    }

    #[test]
    fn test_categorize_build_cargo() {
        assert_eq!(PtCollector::categorize_process("cargo", None), "build");
    }

    #[test]
    fn test_categorize_build_rustc() {
        assert_eq!(PtCollector::categorize_process("rustc", None), "build");
    }

    #[test]
    fn test_categorize_build_npm() {
        assert_eq!(PtCollector::categorize_process("npm", None), "build");
    }

    #[test]
    fn test_categorize_build_make() {
        assert_eq!(PtCollector::categorize_process("make", None), "build");
    }

    #[test]
    fn test_categorize_runtime_node() {
        assert_eq!(PtCollector::categorize_process("node", None), "runtime");
    }

    #[test]
    fn test_categorize_runtime_python() {
        assert_eq!(PtCollector::categorize_process("python3", None), "runtime");
    }

    #[test]
    fn test_categorize_runtime_ruby() {
        assert_eq!(PtCollector::categorize_process("ruby", None), "runtime");
    }

    #[test]
    fn test_categorize_other() {
        assert_eq!(PtCollector::categorize_process("vim", None), "other");
        assert_eq!(PtCollector::categorize_process("ls", None), "other");
        assert_eq!(PtCollector::categorize_process("systemd", None), "other");
    }

    #[test]
    fn test_parse_process_full() {
        let json = r#"{
            "pid": 12345,
            "ppid": 1234,
            "name": "claude-code",
            "cmdline": "claude-code --project /data/projects/vc",
            "user": "ubuntu",
            "started_at": "2026-01-27T10:00:00Z",
            "cpu_percent": 45.2,
            "memory_mb": 512.3,
            "memory_percent": 5.1,
            "threads": 8,
            "open_files": 42,
            "io_read_bytes": 1024000,
            "io_write_bytes": 512000,
            "status": "running",
            "category": "agent",
            "session_id": "sess-123"
        }"#;

        let proc: PtProcess = serde_json::from_str(json).unwrap();
        assert_eq!(proc.pid, 12345);
        assert_eq!(proc.ppid, Some(1234));
        assert_eq!(proc.name, "claude-code");
        assert_eq!(proc.cpu_percent, Some(45.2));
        assert_eq!(proc.memory_mb, Some(512.3));
        assert_eq!(proc.threads, Some(8));
        assert_eq!(proc.category, Some("agent".to_string()));
    }

    #[test]
    fn test_parse_process_minimal() {
        let json = r#"{
            "pid": 100,
            "name": "test"
        }"#;

        let proc: PtProcess = serde_json::from_str(json).unwrap();
        assert_eq!(proc.pid, 100);
        assert_eq!(proc.name, "test");
        assert!(proc.ppid.is_none());
        assert!(proc.cpu_percent.is_none());
        assert!(proc.memory_mb.is_none());
    }

    #[test]
    fn test_parse_process_empty() {
        let json = r"{}";

        let proc: PtProcess = serde_json::from_str(json).unwrap();
        assert_eq!(proc.pid, 0);
        assert_eq!(proc.name, "");
        assert!(proc.started_at.is_none());
    }

    #[test]
    fn test_parse_ended_process() {
        let json = r#"{
            "pid": 12340,
            "exit_code": 0,
            "ended_at": "2026-01-27T10:05:00Z"
        }"#;

        let ended: PtEndedProcess = serde_json::from_str(json).unwrap();
        assert_eq!(ended.pid, 12340);
        assert_eq!(ended.exit_code, Some(0));
        assert_eq!(ended.ended_at, Some("2026-01-27T10:05:00Z".to_string()));
    }

    #[test]
    fn test_parse_pt_output() {
        let json = r#"{
            "processes": [
                {"pid": 100, "name": "test1", "cpu_percent": 10.5},
                {"pid": 200, "name": "test2", "memory_mb": 256.0}
            ],
            "ended": [
                {"pid": 50, "exit_code": 0, "ended_at": "2026-01-27T10:00:00Z"}
            ]
        }"#;

        let output: PtOutput = serde_json::from_str(json).unwrap();
        assert_eq!(output.processes.len(), 2);
        assert_eq!(output.ended.len(), 1);
        assert_eq!(output.processes[0].pid, 100);
        assert_eq!(output.ended[0].exit_code, Some(0));
    }

    #[test]
    fn test_parse_pt_output_empty() {
        let json = r"{}";

        let output: PtOutput = serde_json::from_str(json).unwrap();
        assert!(output.processes.is_empty());
        assert!(output.ended.is_empty());
    }

    #[test]
    fn test_process_serialize() {
        let proc = PtProcess {
            pid: 123,
            ppid: Some(1),
            name: "test".to_string(),
            cmdline: Some("test --flag".to_string()),
            user: Some("root".to_string()),
            started_at: Some("2026-01-27T10:00:00Z".to_string()),
            ended_at: None,
            exit_code: None,
            cpu_percent: Some(50.0),
            memory_mb: Some(100.0),
            memory_percent: Some(1.0),
            threads: Some(4),
            open_files: Some(10),
            io_read_bytes: Some(1000),
            io_write_bytes: Some(500),
            status: Some("running".to_string()),
            category: Some("other".to_string()),
            session_id: None,
        };

        let json = serde_json::to_string(&proc).unwrap();
        assert!(json.contains("\"pid\":123"));
        assert!(json.contains("\"name\":\"test\""));
        assert!(json.contains("\"cpu_percent\":50.0"));
    }
}
