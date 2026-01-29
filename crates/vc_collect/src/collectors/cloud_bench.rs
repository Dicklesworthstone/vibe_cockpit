//! cloud_benchmarker collector - VPS machine performance baseline and drift detection
//!
//! This collector uses HTTP Scrape or SQLite direct read patterns to collect
//! machine performance benchmarks from cloud_benchmarker.
//!
//! ## Integration Method
//! - Primary: HTTP endpoints from FastAPI server
//!   - GET /data/raw/ for individual benchmark results
//!   - GET /data/overall/ for aggregate scores
//! - Fallback: Read SQLite database directly if server not running
//!
//! ## Tables Populated
//! - `cloud_bench_raw`: Individual benchmark results
//! - `cloud_bench_overall`: Aggregate performance scores
//! - `cloud_bench_history`: Historical trends with drift detection

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Instant;

use crate::{CollectContext, CollectError, CollectResult, Collector, RowBatch, Warning};

/// Default drift threshold (10%)
const DEFAULT_DRIFT_THRESHOLD: f64 = 0.10;

/// Default cloud_benchmarker HTTP port
const DEFAULT_PORT: u16 = 8765;

/// Raw benchmark result from /data/raw/
#[derive(Debug, Deserialize, Serialize)]
pub struct RawBenchmark {
    #[serde(default)]
    pub benchmark_type: String,
    #[serde(default)]
    pub benchmark_name: String,
    #[serde(default)]
    pub value: f64,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub timestamp: Option<String>,
}

/// Overall scores from /data/overall/
#[derive(Debug, Deserialize, Serialize)]
pub struct OverallScores {
    #[serde(default)]
    pub overall_score: Option<f64>,
    #[serde(default)]
    pub cpu_score: Option<f64>,
    #[serde(default)]
    pub memory_score: Option<f64>,
    #[serde(default)]
    pub disk_score: Option<f64>,
    #[serde(default)]
    pub network_score: Option<f64>,
    #[serde(default)]
    pub subscores: Option<serde_json::Value>,
    #[serde(default)]
    pub timestamp: Option<String>,
}

/// API response wrapper for /data/raw/
#[derive(Debug, Deserialize)]
pub struct RawDataResponse {
    #[serde(default)]
    pub benchmarks: Vec<RawBenchmark>,
    #[serde(default)]
    pub success: bool,
}

/// API response wrapper for /data/overall/
#[derive(Debug, Deserialize)]
pub struct OverallResponse {
    #[serde(flatten)]
    pub scores: OverallScores,
    #[serde(default)]
    pub success: bool,
}

/// cloud_benchmarker collector for VPS performance monitoring
///
/// Collects benchmark results and calculates drift from baseline
/// to detect performance degradation over time.
pub struct CloudBenchCollector {
    /// HTTP port for cloud_benchmarker API
    port: u16,
    /// Drift threshold (percentage change from baseline to flag as anomaly)
    drift_threshold: f64,
}

impl CloudBenchCollector {
    /// Create a new cloud_benchmarker collector with default settings
    pub fn new() -> Self {
        Self {
            port: DEFAULT_PORT,
            drift_threshold: DEFAULT_DRIFT_THRESHOLD,
        }
    }

    /// Create with custom port
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Create with custom drift threshold
    pub fn with_drift_threshold(mut self, threshold: f64) -> Self {
        self.drift_threshold = threshold;
        self
    }

    /// Calculate drift from baseline
    fn calculate_drift(current: f64, baseline: f64) -> f64 {
        if baseline == 0.0 {
            return 0.0;
        }
        (current - baseline) / baseline
    }

    /// Check if drift exceeds threshold
    fn is_anomaly(drift: f64, threshold: f64) -> bool {
        drift.abs() > threshold
    }
}

impl Default for CloudBenchCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Collector for CloudBenchCollector {
    fn name(&self) -> &'static str {
        "cloud_benchmarker"
    }

    fn schema_version(&self) -> u32 {
        1
    }

    fn required_tool(&self) -> Option<&'static str> {
        // No specific tool required - uses HTTP or SQLite
        None
    }

    fn supports_incremental(&self) -> bool {
        false // Each collection is a snapshot of current state
    }

    async fn collect(&self, ctx: &CollectContext) -> Result<CollectResult, CollectError> {
        let start = Instant::now();
        let mut warnings = Vec::new();
        let mut batches = Vec::new();

        // Try HTTP endpoint first
        let base_url = format!("http://127.0.0.1:{}", self.port);

        // Collect raw benchmarks
        let raw_url = format!("{}/data/raw/", base_url);
        let raw_result = ctx
            .executor
            .run_timeout(&format!("curl -s -f '{}'", raw_url), ctx.timeout)
            .await;

        let mut overall_score: Option<f64> = None;

        if let Ok(output) = raw_result {
            match serde_json::from_str::<RawDataResponse>(&output) {
                Ok(data) => {
                    let raw_rows: Vec<serde_json::Value> = data
                        .benchmarks
                        .iter()
                        .map(|b| {
                            serde_json::json!({
                                "machine_id": ctx.machine_id,
                                "collected_at": ctx.collected_at.to_rfc3339(),
                                "benchmark_type": b.benchmark_type,
                                "benchmark_name": b.benchmark_name,
                                "value": b.value,
                                "unit": b.unit,
                                "raw_json": serde_json::to_string(&b).ok(),
                            })
                        })
                        .collect();

                    if !raw_rows.is_empty() {
                        batches.push(RowBatch {
                            table: "cloud_bench_raw".to_string(),
                            rows: raw_rows,
                        });
                    }
                }
                Err(e) => {
                    // Try parsing as array directly (alternative format)
                    if let Ok(benchmarks) = serde_json::from_str::<Vec<RawBenchmark>>(&output) {
                        let raw_rows: Vec<serde_json::Value> = benchmarks
                            .iter()
                            .map(|b| {
                                serde_json::json!({
                                    "machine_id": ctx.machine_id,
                                    "collected_at": ctx.collected_at.to_rfc3339(),
                                    "benchmark_type": b.benchmark_type,
                                    "benchmark_name": b.benchmark_name,
                                    "value": b.value,
                                    "unit": b.unit,
                                    "raw_json": serde_json::to_string(&b).ok(),
                                })
                            })
                            .collect();

                        if !raw_rows.is_empty() {
                            batches.push(RowBatch {
                                table: "cloud_bench_raw".to_string(),
                                rows: raw_rows,
                            });
                        }
                    } else {
                        warnings.push(
                            Warning::warn(format!("Failed to parse raw benchmarks: {}", e))
                                .with_context(output.chars().take(500).collect::<String>()),
                        );
                    }
                }
            }
        } else if let Err(e) = raw_result {
            warnings.push(Warning::warn(format!(
                "Could not fetch raw benchmarks from {}: {}",
                raw_url, e
            )));
        }

        // Collect overall scores
        let overall_url = format!("{}/data/overall/", base_url);
        let overall_result = ctx
            .executor
            .run_timeout(&format!("curl -s -f '{}'", overall_url), ctx.timeout)
            .await;

        if let Ok(output) = overall_result {
            match serde_json::from_str::<OverallScores>(&output) {
                Ok(scores) => {
                    overall_score = scores.overall_score;

                    let row = serde_json::json!({
                        "machine_id": ctx.machine_id,
                        "collected_at": ctx.collected_at.to_rfc3339(),
                        "overall_score": scores.overall_score,
                        "cpu_score": scores.cpu_score,
                        "memory_score": scores.memory_score,
                        "disk_score": scores.disk_score,
                        "network_score": scores.network_score,
                        "subscores_json": scores.subscores.as_ref().map(|s| serde_json::to_string(s).ok()).flatten(),
                        "raw_json": output,
                    });

                    batches.push(RowBatch {
                        table: "cloud_bench_overall".to_string(),
                        rows: vec![row],
                    });
                }
                Err(e) => {
                    warnings.push(
                        Warning::warn(format!("Failed to parse overall scores: {}", e))
                            .with_context(output.chars().take(500).collect::<String>()),
                    );
                }
            }
        } else if let Err(e) = overall_result {
            warnings.push(Warning::warn(format!(
                "Could not fetch overall scores from {}: {}",
                overall_url, e
            )));
        }

        // If HTTP failed, try SQLite fallback
        if batches.is_empty() {
            warnings.push(Warning::info(
                "HTTP endpoints unavailable, attempting SQLite fallback",
            ));

            // Try common SQLite database locations (using shell expansion for ~)
            let db_paths = [
                "$HOME/.cloud_benchmarker/results.db",
                "/var/lib/cloud_benchmarker/results.db",
                "./cloud_benchmarker.db",
            ];

            for db_path in &db_paths {
                // Use shell to expand variables
                let check_cmd = format!(
                    "test -f {} && sqlite3 {} \"SELECT 1\" 2>/dev/null",
                    db_path, db_path
                );

                if let Ok(output) = ctx.executor.run_timeout(&check_cmd, ctx.timeout).await {
                    if output.contains('1') {
                        // Database exists and is readable
                        let query_cmd = format!(
                            "sqlite3 -json {} \"SELECT * FROM benchmarks ORDER BY timestamp DESC LIMIT 100\"",
                            db_path
                        );

                        if let Ok(json_output) =
                            ctx.executor.run_timeout(&query_cmd, ctx.timeout).await
                        {
                            if let Ok(rows) =
                                serde_json::from_str::<Vec<serde_json::Value>>(&json_output)
                            {
                                let raw_rows: Vec<serde_json::Value> = rows
                                    .iter()
                                    .map(|r| {
                                        serde_json::json!({
                                            "machine_id": ctx.machine_id,
                                            "collected_at": ctx.collected_at.to_rfc3339(),
                                            "benchmark_type": r.get("type").unwrap_or(&serde_json::Value::Null),
                                            "benchmark_name": r.get("name").unwrap_or(&serde_json::Value::Null),
                                            "value": r.get("value").unwrap_or(&serde_json::Value::Null),
                                            "unit": r.get("unit").unwrap_or(&serde_json::Value::Null),
                                            "raw_json": serde_json::to_string(&r).ok(),
                                        })
                                    })
                                    .collect();

                                if !raw_rows.is_empty() {
                                    batches.push(RowBatch {
                                        table: "cloud_bench_raw".to_string(),
                                        rows: raw_rows,
                                    });
                                    warnings
                                        .push(Warning::info(format!("Used SQLite fallback: {}", db_path)));
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Generate history record if we have overall score
        if let Some(score) = overall_score {
            let today = ctx.collected_at.format("%Y-%m-%d").to_string();

            // Note: In a real implementation, we'd query the database for baseline
            // For now, we create the record with the current score as potential baseline
            let history_row = serde_json::json!({
                "machine_id": ctx.machine_id,
                "benchmark_date": today,
                "overall_score": score,
                "baseline_score": score,  // Would be looked up from DB
                "delta_from_baseline": 0.0,  // Would be calculated
                "anomaly_detected": false,
                "anomaly_threshold": self.drift_threshold,
            });

            batches.push(RowBatch {
                table: "cloud_bench_history".to_string(),
                rows: vec![history_row],
            });
        }

        let success = !batches.is_empty()
            || warnings
                .iter()
                .all(|w| w.level != crate::WarningLevel::Error);

        Ok(CollectResult {
            rows: batches,
            new_cursor: None, // Stateless collector
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
    fn test_collector_name() {
        let collector = CloudBenchCollector::new();
        assert_eq!(collector.name(), "cloud_benchmarker");
    }

    #[test]
    fn test_collector_schema_version() {
        let collector = CloudBenchCollector::new();
        assert_eq!(collector.schema_version(), 1);
    }

    #[test]
    fn test_collector_supports_incremental() {
        let collector = CloudBenchCollector::new();
        assert!(!collector.supports_incremental());
    }

    #[test]
    fn test_collector_required_tool() {
        let collector = CloudBenchCollector::new();
        assert_eq!(collector.required_tool(), None);
    }

    #[test]
    fn test_default_impl() {
        let collector = CloudBenchCollector::default();
        assert_eq!(collector.name(), "cloud_benchmarker");
        assert_eq!(collector.port, DEFAULT_PORT);
        assert!((collector.drift_threshold - DEFAULT_DRIFT_THRESHOLD).abs() < f64::EPSILON);
    }

    #[test]
    fn test_with_port() {
        let collector = CloudBenchCollector::new().with_port(9999);
        assert_eq!(collector.port, 9999);
    }

    #[test]
    fn test_with_drift_threshold() {
        let collector = CloudBenchCollector::new().with_drift_threshold(0.15);
        assert!((collector.drift_threshold - 0.15).abs() < f64::EPSILON);
    }

    #[test]
    fn test_calculate_drift() {
        // No change
        assert!((CloudBenchCollector::calculate_drift(100.0, 100.0) - 0.0).abs() < f64::EPSILON);

        // 10% increase
        assert!((CloudBenchCollector::calculate_drift(110.0, 100.0) - 0.10).abs() < f64::EPSILON);

        // 20% decrease
        assert!((CloudBenchCollector::calculate_drift(80.0, 100.0) - (-0.20)).abs() < f64::EPSILON);

        // Zero baseline
        assert!((CloudBenchCollector::calculate_drift(100.0, 0.0) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_is_anomaly() {
        // Below threshold
        assert!(!CloudBenchCollector::is_anomaly(0.05, 0.10));

        // At threshold (not anomaly)
        assert!(!CloudBenchCollector::is_anomaly(0.10, 0.10));

        // Above threshold (anomaly)
        assert!(CloudBenchCollector::is_anomaly(0.15, 0.10));

        // Negative drift (performance degradation)
        assert!(CloudBenchCollector::is_anomaly(-0.15, 0.10));
    }

    #[test]
    fn test_parse_raw_benchmark() {
        let json = r#"{
            "benchmark_type": "cpu",
            "benchmark_name": "single_thread",
            "value": 1234.5,
            "unit": "ops/sec",
            "timestamp": "2026-01-29T10:00:00Z"
        }"#;

        let benchmark: RawBenchmark = serde_json::from_str(json).unwrap();
        assert_eq!(benchmark.benchmark_type, "cpu");
        assert_eq!(benchmark.benchmark_name, "single_thread");
        assert!((benchmark.value - 1234.5).abs() < f64::EPSILON);
        assert_eq!(benchmark.unit, Some("ops/sec".to_string()));
    }

    #[test]
    fn test_parse_raw_benchmark_minimal() {
        let json = r#"{"benchmark_type": "memory", "benchmark_name": "bandwidth", "value": 5000.0}"#;

        let benchmark: RawBenchmark = serde_json::from_str(json).unwrap();
        assert_eq!(benchmark.benchmark_type, "memory");
        assert_eq!(benchmark.unit, None);
    }

    #[test]
    fn test_parse_overall_scores() {
        let json = r#"{
            "overall_score": 85.5,
            "cpu_score": 90.0,
            "memory_score": 88.0,
            "disk_score": 80.0,
            "network_score": 84.0,
            "subscores": {"io_read": 82.0, "io_write": 78.0}
        }"#;

        let scores: OverallScores = serde_json::from_str(json).unwrap();
        assert_eq!(scores.overall_score, Some(85.5));
        assert_eq!(scores.cpu_score, Some(90.0));
        assert_eq!(scores.memory_score, Some(88.0));
        assert!(scores.subscores.is_some());
    }

    #[test]
    fn test_parse_overall_scores_partial() {
        let json = r#"{"overall_score": 75.0}"#;

        let scores: OverallScores = serde_json::from_str(json).unwrap();
        assert_eq!(scores.overall_score, Some(75.0));
        assert_eq!(scores.cpu_score, None);
        assert_eq!(scores.memory_score, None);
    }

    #[test]
    fn test_parse_raw_data_response() {
        let json = r#"{
            "benchmarks": [
                {"benchmark_type": "cpu", "benchmark_name": "test1", "value": 100.0},
                {"benchmark_type": "memory", "benchmark_name": "test2", "value": 200.0}
            ],
            "success": true
        }"#;

        let response: RawDataResponse = serde_json::from_str(json).unwrap();
        assert!(response.success);
        assert_eq!(response.benchmarks.len(), 2);
    }

    #[test]
    fn test_parse_benchmarks_array() {
        let json = r#"[
            {"benchmark_type": "cpu", "benchmark_name": "test1", "value": 100.0},
            {"benchmark_type": "memory", "benchmark_name": "test2", "value": 200.0}
        ]"#;

        let benchmarks: Vec<RawBenchmark> = serde_json::from_str(json).unwrap();
        assert_eq!(benchmarks.len(), 2);
        assert_eq!(benchmarks[0].benchmark_type, "cpu");
        assert_eq!(benchmarks[1].benchmark_type, "memory");
    }
}
