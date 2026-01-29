//! afsc collector - Automated Flywheel Setup Checker metrics
//!
//! This collector uses the CLI Incremental Window pattern to collect
//! flywheel setup health and installer run metrics from afsc.
//!
//! ## Integration Method
//! - `automated_flywheel_setup_checker status --format json` for overall health
//! - `automated_flywheel_setup_checker list --format jsonl` for run history
//! - `automated_flywheel_setup_checker validate --format jsonl` for events
//! - `automated_flywheel_setup_checker classify-error --format jsonl` for error clusters
//!
//! ## Tables Populated
//! - `afsc_status_snapshot`: Overall health status
//! - `afsc_run_facts`: Individual run records
//! - `afsc_event_logs`: Streaming events
//! - `afsc_error_clusters`: Aggregated error patterns

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Instant;

use crate::{CollectContext, CollectError, CollectResult, Collector, Cursor, RowBatch, Warning};

/// Status output from `afsc status --format json`
#[derive(Debug, Default, Deserialize)]
pub struct AfscStatus {
    #[serde(default)]
    pub overall_health: String,
    #[serde(default)]
    pub installers: AfscInstallerSummary,
    #[serde(default)]
    pub last_run: Option<AfscLastRun>,
    #[serde(default)]
    pub uptime_seconds: Option<i64>,
    #[serde(default)]
    pub version: Option<String>,
}

/// Installer summary within status
#[derive(Debug, Default, Deserialize)]
pub struct AfscInstallerSummary {
    #[serde(default)]
    pub total: i32,
    #[serde(default)]
    pub healthy: i32,
    #[serde(default)]
    pub failed: i32,
    #[serde(default)]
    pub pending: i32,
}

/// Last run info within status
#[derive(Debug, Deserialize)]
pub struct AfscLastRun {
    pub timestamp: Option<String>,
    pub status: Option<String>,
    pub duration_ms: Option<i64>,
}

/// Run record from `afsc list --format jsonl`
#[derive(Debug, Deserialize, Serialize)]
pub struct AfscRunRecord {
    #[serde(default)]
    pub run_id: String,
    #[serde(default)]
    pub timestamp: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub duration_ms: Option<i64>,
    #[serde(default)]
    pub installer_name: Option<String>,
    #[serde(default)]
    pub installer_version: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub error_category: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
}

/// Event from `afsc validate --format jsonl`
#[derive(Debug, Deserialize, Serialize)]
pub struct AfscEvent {
    #[serde(default)]
    pub timestamp: String,
    #[serde(default)]
    pub event_type: String,
    #[serde(default)]
    pub severity: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub installer_name: Option<String>,
    #[serde(default)]
    pub component: Option<String>,
}

/// Error cluster from `afsc classify-error --format jsonl`
#[derive(Debug, Deserialize, Serialize)]
pub struct AfscErrorCluster {
    #[serde(default)]
    pub error_category: String,
    #[serde(default)]
    pub occurrence_count: i32,
    #[serde(default)]
    pub first_seen: Option<String>,
    #[serde(default)]
    pub last_seen: Option<String>,
    #[serde(default)]
    pub affected_installers: Vec<String>,
    #[serde(default)]
    pub example_errors: Vec<String>,
}

/// afsc collector for flywheel setup health monitoring
///
/// Collects installer run metrics and error patterns using the
/// CLI Incremental Window pattern (time-bounded collection).
pub struct AfscCollector;

impl AfscCollector {
    /// Create a new afsc collector
    pub fn new() -> Self {
        Self
    }

    /// Parse JSONL output into records
    fn parse_jsonl<T: for<'de> Deserialize<'de>>(
        &self,
        output: &str,
        warnings: &mut Vec<Warning>,
    ) -> Vec<T> {
        let mut records = Vec::new();
        for (line_num, line) in output.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<T>(line) {
                Ok(record) => records.push(record),
                Err(e) => {
                    warnings.push(Warning::warn(format!(
                        "Failed to parse line {}: {}",
                        line_num + 1,
                        e
                    )));
                }
            }
        }
        records
    }
}

impl Default for AfscCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Collector for AfscCollector {
    fn name(&self) -> &'static str {
        "afsc"
    }

    fn schema_version(&self) -> u32 {
        1
    }

    fn required_tool(&self) -> Option<&'static str> {
        Some("automated_flywheel_setup_checker")
    }

    fn supports_incremental(&self) -> bool {
        true // Uses time-bounded incremental window
    }

    async fn collect(&self, ctx: &CollectContext) -> Result<CollectResult, CollectError> {
        let start = Instant::now();
        let mut warnings = Vec::new();
        let mut batches = Vec::new();

        // Check if afsc is available
        if !self.check_availability(ctx).await {
            return Err(CollectError::ToolNotFound(
                "automated_flywheel_setup_checker".to_string(),
            ));
        }

        // 1. Collect status snapshot
        let status_result = ctx
            .executor
            .run_timeout("automated_flywheel_setup_checker status --format json", ctx.timeout)
            .await;

        if let Ok(output) = status_result {
            match serde_json::from_str::<AfscStatus>(&output) {
                Ok(status) => {
                    let row = serde_json::json!({
                        "machine_id": ctx.machine_id,
                        "collected_at": ctx.collected_at.to_rfc3339(),
                        "overall_health": status.overall_health,
                        "installers_total": status.installers.total,
                        "installers_healthy": status.installers.healthy,
                        "installers_failed": status.installers.failed,
                        "last_run_at": status.last_run.as_ref().and_then(|r| r.timestamp.clone()),
                        "last_run_status": status.last_run.as_ref().and_then(|r| r.status.clone()),
                        "uptime_seconds": status.uptime_seconds,
                        "raw_json": output,
                    });
                    batches.push(RowBatch { table: "afsc_status_snapshot".to_string(), rows: vec![row] });
                }
                Err(e) => {
                    warnings.push(
                        Warning::error(format!("Failed to parse afsc status: {}", e))
                            .with_context(output.chars().take(500).collect::<String>()),
                    );
                }
            }
        } else if let Err(e) = status_result {
            warnings.push(Warning::warn(format!("afsc status command failed: {}", e)));
        }

        // 2. Collect run history (with time window if cursor provided)
        let list_cmd = if let Some(last_ts) = ctx.timestamp_cursor() {
            format!(
                "automated_flywheel_setup_checker list --format jsonl --since {}",
                last_ts.to_rfc3339()
            )
        } else {
            "automated_flywheel_setup_checker list --format jsonl --limit 100".to_string()
        };

        let list_result = ctx.executor.run_timeout(&list_cmd, ctx.timeout).await;

        if let Ok(output) = list_result {
            let runs: Vec<AfscRunRecord> = self.parse_jsonl(&output, &mut warnings);

            let run_rows: Vec<serde_json::Value> = runs
                .iter()
                .map(|run| {
                    serde_json::json!({
                        "machine_id": ctx.machine_id,
                        "run_id": run.run_id,
                        "ts": run.timestamp,
                        "status": run.status,
                        "duration_ms": run.duration_ms,
                        "error_category": run.error_category,
                        "installer_name": run.installer_name,
                        "installer_version": run.installer_version,
                        "exit_code": run.exit_code,
                        "error_message": run.error_message,
                        "raw_json": serde_json::to_string(&run).ok(),
                    })
                })
                .collect();

            if !run_rows.is_empty() {
                batches.push(RowBatch { table: "afsc_run_facts".to_string(), rows: run_rows });
            }
        } else if let Err(e) = list_result {
            warnings.push(Warning::warn(format!("afsc list command failed: {}", e)));
        }

        // 3. Collect validation events
        let validate_result = ctx
            .executor
            .run_timeout(
                "automated_flywheel_setup_checker validate --format jsonl",
                ctx.timeout,
            )
            .await;

        if let Ok(output) = validate_result {
            let events: Vec<AfscEvent> = self.parse_jsonl(&output, &mut warnings);

            let event_rows: Vec<serde_json::Value> = events
                .iter()
                .map(|event| {
                    serde_json::json!({
                        "machine_id": ctx.machine_id,
                        "ts": event.timestamp,
                        "event_type": event.event_type,
                        "severity": event.severity,
                        "message": event.message,
                        "installer_name": event.installer_name,
                        "component": event.component,
                        "raw_json": serde_json::to_string(&event).ok(),
                    })
                })
                .collect();

            if !event_rows.is_empty() {
                batches.push(RowBatch { table: "afsc_event_logs".to_string(), rows: event_rows });
            }
        } else if let Err(e) = validate_result {
            warnings.push(Warning::warn(format!(
                "afsc validate command failed: {}",
                e
            )));
        }

        // 4. Collect error clusters
        let classify_result = ctx
            .executor
            .run_timeout(
                "automated_flywheel_setup_checker classify-error --format jsonl",
                ctx.timeout,
            )
            .await;

        if let Ok(output) = classify_result {
            let clusters: Vec<AfscErrorCluster> = self.parse_jsonl(&output, &mut warnings);

            let cluster_rows: Vec<serde_json::Value> = clusters
                .iter()
                .map(|cluster| {
                    serde_json::json!({
                        "machine_id": ctx.machine_id,
                        "collected_at": ctx.collected_at.to_rfc3339(),
                        "error_category": cluster.error_category,
                        "occurrence_count": cluster.occurrence_count,
                        "first_seen": cluster.first_seen,
                        "last_seen": cluster.last_seen,
                        "affected_installers": serde_json::to_string(&cluster.affected_installers).ok(),
                        "example_errors_json": serde_json::to_string(&cluster.example_errors).ok(),
                    })
                })
                .collect();

            if !cluster_rows.is_empty() {
                batches.push(RowBatch { table: "afsc_error_clusters".to_string(), rows: cluster_rows });
            }
        } else if let Err(e) = classify_result {
            warnings.push(Warning::warn(format!(
                "afsc classify-error command failed: {}",
                e
            )));
        }

        // Build result with cursor for incremental collection
        // Use the current collection time as the cursor for the next run
        let new_cursor = if !batches.is_empty() {
            Some(Cursor::now())
        } else {
            None
        };

        let success = !batches.is_empty() || warnings.iter().all(|w| w.level != crate::WarningLevel::Error);

        Ok(CollectResult {
            rows: batches,
            new_cursor,
            raw_artifacts: vec![],
            warnings,
            duration: start.elapsed(),
            success,
            error: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_status_json() {
        let json = r#"{
            "overall_health": "healthy",
            "installers": {
                "total": 10,
                "healthy": 8,
                "failed": 1,
                "pending": 1
            },
            "last_run": {
                "timestamp": "2026-01-29T10:00:00Z",
                "status": "success",
                "duration_ms": 5432
            },
            "uptime_seconds": 86400,
            "version": "1.2.3"
        }"#;

        let status: AfscStatus = serde_json::from_str(json).unwrap();
        assert_eq!(status.overall_health, "healthy");
        assert_eq!(status.installers.total, 10);
        assert_eq!(status.installers.healthy, 8);
        assert_eq!(status.installers.failed, 1);
        assert!(status.last_run.is_some());
        assert_eq!(status.uptime_seconds, Some(86400));
    }

    #[test]
    fn test_parse_status_minimal() {
        let json = r#"{"overall_health": "unknown"}"#;
        let status: AfscStatus = serde_json::from_str(json).unwrap();
        assert_eq!(status.overall_health, "unknown");
        assert_eq!(status.installers.total, 0);
    }

    #[test]
    fn test_parse_run_record() {
        let json = r#"{
            "run_id": "run-123",
            "timestamp": "2026-01-29T10:00:00Z",
            "status": "success",
            "duration_ms": 1234,
            "installer_name": "rust-tools",
            "installer_version": "1.0.0",
            "exit_code": 0
        }"#;

        let record: AfscRunRecord = serde_json::from_str(json).unwrap();
        assert_eq!(record.run_id, "run-123");
        assert_eq!(record.status, "success");
        assert_eq!(record.duration_ms, Some(1234));
        assert_eq!(record.exit_code, Some(0));
    }

    #[test]
    fn test_parse_run_record_with_error() {
        let json = r#"{
            "run_id": "run-456",
            "timestamp": "2026-01-29T11:00:00Z",
            "status": "failed",
            "duration_ms": 5000,
            "installer_name": "node-tools",
            "error_category": "timeout",
            "error_message": "Installation timed out after 5 seconds",
            "exit_code": 124
        }"#;

        let record: AfscRunRecord = serde_json::from_str(json).unwrap();
        assert_eq!(record.status, "failed");
        assert_eq!(record.error_category, Some("timeout".to_string()));
        assert_eq!(record.exit_code, Some(124));
    }

    #[test]
    fn test_parse_event() {
        let json = r#"{
            "timestamp": "2026-01-29T10:00:00Z",
            "event_type": "validation",
            "severity": "warn",
            "message": "Config file missing recommended settings",
            "installer_name": "rust-tools",
            "component": "config"
        }"#;

        let event: AfscEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.event_type, "validation");
        assert_eq!(event.severity, "warn");
        assert_eq!(event.installer_name, Some("rust-tools".to_string()));
    }

    #[test]
    fn test_parse_error_cluster() {
        let json = r#"{
            "error_category": "timeout",
            "occurrence_count": 5,
            "first_seen": "2026-01-28T00:00:00Z",
            "last_seen": "2026-01-29T10:00:00Z",
            "affected_installers": ["node-tools", "python-tools"],
            "example_errors": ["timed out after 30s", "operation exceeded deadline"]
        }"#;

        let cluster: AfscErrorCluster = serde_json::from_str(json).unwrap();
        assert_eq!(cluster.error_category, "timeout");
        assert_eq!(cluster.occurrence_count, 5);
        assert_eq!(cluster.affected_installers.len(), 2);
        assert_eq!(cluster.example_errors.len(), 2);
    }

    #[test]
    fn test_parse_jsonl() {
        let collector = AfscCollector::new();
        let mut warnings = Vec::new();

        let jsonl = r#"{"run_id": "1", "timestamp": "2026-01-29T10:00:00Z", "status": "success"}
{"run_id": "2", "timestamp": "2026-01-29T11:00:00Z", "status": "failed"}
invalid json line
{"run_id": "3", "timestamp": "2026-01-29T12:00:00Z", "status": "success"}"#;

        let records: Vec<AfscRunRecord> = collector.parse_jsonl(jsonl, &mut warnings);

        assert_eq!(records.len(), 3);
        assert_eq!(warnings.len(), 1); // One invalid line
        assert_eq!(records[0].run_id, "1");
        assert_eq!(records[2].run_id, "3");
    }

    #[test]
    fn test_collector_name() {
        let collector = AfscCollector::new();
        assert_eq!(collector.name(), "afsc");
    }

    #[test]
    fn test_collector_supports_incremental() {
        let collector = AfscCollector::new();
        assert!(collector.supports_incremental());
    }

    #[test]
    fn test_collector_required_tool() {
        let collector = AfscCollector::new();
        assert_eq!(
            collector.required_tool(),
            Some("automated_flywheel_setup_checker")
        );
    }

    #[test]
    fn test_default_impl() {
        let collector = AfscCollector::default();
        assert_eq!(collector.name(), "afsc");
    }

    #[test]
    fn test_schema_version() {
        let collector = AfscCollector::new();
        assert_eq!(collector.schema_version(), 1);
    }
}
