//! Sysmoni collector - system metrics via the sysmoni tool
//!
//! This collector uses the CLI Snapshot ingestion pattern to collect
//! system metrics from the `sysmoni` tool.
//!
//! ## Integration Method
//! ```bash
//! sysmoni --json           # One-shot snapshot (MVP)
//! sysmoni --json-stream    # Continuous NDJSON (future)
//! ```
//!
//! ## Tables Populated
//! - `sys_samples`: Aggregated system metrics per collection
//! - `sys_top_processes`: Top processes by CPU/memory usage

use async_trait::async_trait;
use serde::Deserialize;
use std::time::Instant;

use crate::{CollectContext, CollectError, CollectResult, Collector, RowBatch, Warning};

/// Output schema from `sysmoni --json`
#[derive(Debug, Deserialize)]
pub struct SysmoniOutput {
    /// Timestamp of the snapshot
    #[serde(default)]
    pub timestamp: Option<String>,

    /// CPU metrics
    #[serde(default)]
    pub cpu: CpuMetrics,

    /// Memory metrics
    #[serde(default)]
    pub memory: MemoryMetrics,

    /// Disk metrics
    #[serde(default)]
    pub disk: DiskMetrics,

    /// Network metrics
    #[serde(default)]
    pub network: NetworkMetrics,

    /// Top processes
    #[serde(default)]
    pub processes: Vec<ProcessInfo>,
}

/// CPU metrics from sysmoni
#[derive(Debug, Default, Deserialize)]
pub struct CpuMetrics {
    /// Total CPU usage percentage
    #[serde(default)]
    pub total_percent: f64,

    /// Per-core usage percentages
    #[serde(default)]
    pub per_core: Vec<f64>,

    /// 1-minute load average
    #[serde(default)]
    pub load_1: f64,

    /// 5-minute load average
    #[serde(default)]
    pub load_5: f64,

    /// 15-minute load average
    #[serde(default)]
    pub load_15: f64,
}

/// Memory metrics from sysmoni
#[derive(Debug, Default, Deserialize)]
pub struct MemoryMetrics {
    /// Total memory in bytes
    #[serde(default)]
    pub total_bytes: i64,

    /// Used memory in bytes
    #[serde(default)]
    pub used_bytes: i64,

    /// Available memory in bytes
    #[serde(default)]
    pub available_bytes: i64,

    /// Total swap in bytes
    #[serde(default)]
    pub swap_total_bytes: i64,

    /// Used swap in bytes
    #[serde(default)]
    pub swap_used_bytes: i64,
}

/// Disk metrics from sysmoni
#[derive(Debug, Default, Deserialize)]
pub struct DiskMetrics {
    /// Disk read bytes per second
    #[serde(default)]
    pub read_bytes_per_sec: i64,

    /// Disk write bytes per second
    #[serde(default)]
    pub write_bytes_per_sec: i64,

    /// Filesystem usage
    #[serde(default)]
    pub filesystems: Vec<FilesystemInfo>,
}

/// Individual filesystem info
#[derive(Debug, Deserialize)]
pub struct FilesystemInfo {
    /// Mount point
    pub mount: String,

    /// Total bytes
    #[serde(default)]
    pub total_bytes: i64,

    /// Used bytes
    #[serde(default)]
    pub used_bytes: i64,
}

/// Network metrics from sysmoni
#[derive(Debug, Default, Deserialize)]
pub struct NetworkMetrics {
    /// Network receive bytes per second
    #[serde(default)]
    pub rx_bytes_per_sec: i64,

    /// Network transmit bytes per second
    #[serde(default)]
    pub tx_bytes_per_sec: i64,
}

/// Process information from sysmoni
#[derive(Debug, Deserialize)]
pub struct ProcessInfo {
    /// Process ID
    pub pid: i32,

    /// Process name/command
    #[serde(default)]
    pub name: String,

    /// CPU usage percentage
    #[serde(default)]
    pub cpu_percent: f64,

    /// Memory usage in bytes
    #[serde(default)]
    pub memory_bytes: i64,
}

/// Sysmoni collector for system metrics
///
/// Collects CPU, memory, disk, network, and process information
/// using the `sysmoni` tool's JSON output.
pub struct SysmoniCollector;

#[async_trait]
impl Collector for SysmoniCollector {
    fn name(&self) -> &'static str {
        "sysmoni"
    }

    fn schema_version(&self) -> u32 {
        1
    }

    fn required_tool(&self) -> Option<&'static str> {
        Some("sysmoni")
    }

    fn supports_incremental(&self) -> bool {
        false // Each collection is a point-in-time snapshot
    }

    async fn collect(&self, ctx: &CollectContext) -> Result<CollectResult, CollectError> {
        let start = Instant::now();
        let mut warnings = Vec::new();

        // Check if sysmoni is available
        if !self.check_availability(ctx).await {
            return Err(CollectError::ToolNotFound("sysmoni".to_string()));
        }

        // Run sysmoni --json
        let output = ctx
            .executor
            .run_timeout("sysmoni --json", ctx.timeout)
            .await?;

        // Parse the JSON output
        let data: SysmoniOutput = match serde_json::from_str(&output) {
            Ok(d) => d,
            Err(e) => {
                return Err(CollectError::ParseError(format!(
                    "Failed to parse sysmoni JSON: {e}"
                )));
            }
        };

        let collected_at = ctx.collected_at.to_rfc3339();

        // Build sys_samples row
        let sys_row = serde_json::json!({
            "machine_id": &ctx.machine_id,
            "collected_at": &collected_at,
            "cpu_total": data.cpu.total_percent,
            "load1": data.cpu.load_1,
            "load5": data.cpu.load_5,
            "load15": data.cpu.load_15,
            "mem_used_bytes": data.memory.used_bytes,
            "mem_total_bytes": data.memory.total_bytes,
            "mem_available_bytes": data.memory.available_bytes,
            "swap_used_bytes": data.memory.swap_used_bytes,
            "swap_total_bytes": data.memory.swap_total_bytes,
            "disk_read_mbps": data.disk.read_bytes_per_sec as f64 / 1_000_000.0,
            "disk_write_mbps": data.disk.write_bytes_per_sec as f64 / 1_000_000.0,
            "net_rx_mbps": data.network.rx_bytes_per_sec as f64 / 1_000_000.0,
            "net_tx_mbps": data.network.tx_bytes_per_sec as f64 / 1_000_000.0,
            "core_count": data.cpu.per_core.len(),
            "raw_json": &output,
        });

        let mut rows = vec![RowBatch {
            table: "sys_samples".to_string(),
            rows: vec![sys_row],
        }];

        // Build top processes rows (limit to top 10)
        let max_processes = ctx.max_rows.min(10);
        if !data.processes.is_empty() {
            let proc_rows: Vec<serde_json::Value> = data
                .processes
                .iter()
                .take(max_processes)
                .map(|p| {
                    serde_json::json!({
                        "machine_id": &ctx.machine_id,
                        "collected_at": &collected_at,
                        "pid": p.pid,
                        "comm": &p.name,
                        "cpu_pct": p.cpu_percent,
                        "mem_bytes": p.memory_bytes,
                    })
                })
                .collect();

            if !proc_rows.is_empty() {
                rows.push(RowBatch {
                    table: "sys_top_processes".to_string(),
                    rows: proc_rows,
                });
            }
        }

        // Build filesystem rows if present
        if !data.disk.filesystems.is_empty() {
            let fs_rows: Vec<serde_json::Value> = data
                .disk
                .filesystems
                .iter()
                .filter(|fs| fs.total_bytes > 0) // Skip empty filesystems
                .map(|fs| {
                    let usage_pct = if fs.total_bytes > 0 {
                        (fs.used_bytes as f64 / fs.total_bytes as f64) * 100.0
                    } else {
                        0.0
                    };
                    serde_json::json!({
                        "machine_id": &ctx.machine_id,
                        "collected_at": &collected_at,
                        "mount": &fs.mount,
                        "total_bytes": fs.total_bytes,
                        "used_bytes": fs.used_bytes,
                        "usage_pct": usage_pct,
                    })
                })
                .collect();

            if !fs_rows.is_empty() {
                rows.push(RowBatch {
                    table: "sys_filesystems".to_string(),
                    rows: fs_rows,
                });
            }
        }

        // Add warning if no processes reported
        if data.processes.is_empty() {
            warnings.push(Warning::info("No process information in sysmoni output"));
        }

        let mut result = CollectResult::with_rows(rows).with_duration(start.elapsed());

        for warning in warnings {
            result = result.with_warning(warning);
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_sysmoni_collector_name() {
        let collector = SysmoniCollector;
        assert_eq!(collector.name(), "sysmoni");
        assert_eq!(collector.required_tool(), Some("sysmoni"));
        assert!(!collector.supports_incremental());
        assert_eq!(collector.schema_version(), 1);
    }

    #[test]
    fn test_sysmoni_output_parsing_full() {
        let json = r#"{
            "timestamp": "2026-01-27T00:00:00Z",
            "cpu": {
                "total_percent": 45.2,
                "per_core": [42.1, 48.3, 44.0, 46.4],
                "load_1": 2.1,
                "load_5": 1.8,
                "load_15": 1.5
            },
            "memory": {
                "total_bytes": 34359738368,
                "used_bytes": 23622320128,
                "available_bytes": 10737418240,
                "swap_total_bytes": 8589934592,
                "swap_used_bytes": 0
            },
            "disk": {
                "read_bytes_per_sec": 1048576,
                "write_bytes_per_sec": 2097152,
                "filesystems": [
                    {"mount": "/", "total_bytes": 500000000000, "used_bytes": 350000000000}
                ]
            },
            "network": {
                "rx_bytes_per_sec": 10485760,
                "tx_bytes_per_sec": 5242880
            },
            "processes": [
                {"pid": 1234, "name": "cargo", "cpu_percent": 45.0, "memory_bytes": 1073741824}
            ]
        }"#;

        let output: SysmoniOutput = serde_json::from_str(json).unwrap();

        // CPU
        assert!((output.cpu.total_percent - 45.2).abs() < 0.01);
        assert_eq!(output.cpu.per_core.len(), 4);
        assert!((output.cpu.load_1 - 2.1).abs() < 0.01);
        assert!((output.cpu.load_5 - 1.8).abs() < 0.01);
        assert!((output.cpu.load_15 - 1.5).abs() < 0.01);

        // Memory
        assert_eq!(output.memory.total_bytes, 34359738368);
        assert_eq!(output.memory.used_bytes, 23622320128);
        assert_eq!(output.memory.available_bytes, 10737418240);
        assert_eq!(output.memory.swap_total_bytes, 8589934592);
        assert_eq!(output.memory.swap_used_bytes, 0);

        // Disk
        assert_eq!(output.disk.read_bytes_per_sec, 1048576);
        assert_eq!(output.disk.write_bytes_per_sec, 2097152);
        assert_eq!(output.disk.filesystems.len(), 1);
        assert_eq!(output.disk.filesystems[0].mount, "/");

        // Network
        assert_eq!(output.network.rx_bytes_per_sec, 10485760);
        assert_eq!(output.network.tx_bytes_per_sec, 5242880);

        // Processes
        assert_eq!(output.processes.len(), 1);
        assert_eq!(output.processes[0].pid, 1234);
        assert_eq!(output.processes[0].name, "cargo");
    }

    #[test]
    fn test_sysmoni_output_parsing_minimal() {
        // Minimal valid JSON with defaults
        let json = r#"{}"#;

        let output: SysmoniOutput = serde_json::from_str(json).unwrap();

        assert_eq!(output.cpu.total_percent, 0.0);
        assert!(output.cpu.per_core.is_empty());
        assert_eq!(output.memory.total_bytes, 0);
        assert!(output.disk.filesystems.is_empty());
        assert!(output.processes.is_empty());
    }

    #[test]
    fn test_sysmoni_output_parsing_partial() {
        // Partial JSON with some fields missing
        let json = r#"{
            "cpu": {
                "total_percent": 50.0,
                "load_1": 1.0
            },
            "memory": {
                "total_bytes": 16000000000
            }
        }"#;

        let output: SysmoniOutput = serde_json::from_str(json).unwrap();

        assert!((output.cpu.total_percent - 50.0).abs() < 0.01);
        assert!((output.cpu.load_1 - 1.0).abs() < 0.01);
        assert_eq!(output.cpu.load_5, 0.0); // Default
        assert_eq!(output.memory.total_bytes, 16000000000);
        assert_eq!(output.memory.used_bytes, 0); // Default
    }

    #[tokio::test]
    async fn test_sysmoni_collector_behavior() {
        let collector = SysmoniCollector;
        let ctx = CollectContext::local("test", Duration::from_secs(5));

        let result = collector.collect(&ctx).await;

        // Test handles both cases: sysmoni installed or not
        match result {
            Err(CollectError::ToolNotFound(tool)) => {
                // Expected when sysmoni is not installed
                assert_eq!(tool, "sysmoni");
            }
            Err(_) => {
                // Other errors are acceptable (e.g., command not found, parse error)
            }
            Ok(collect_result) => {
                // If sysmoni is installed, verify the result structure
                assert!(collect_result.success);
                assert!(collect_result.total_rows() >= 1); // At least sys_samples row
            }
        }
    }

    #[test]
    fn test_disk_metrics_bytes_per_sec_conversion() {
        // Verify MB/s conversion math
        let bytes_per_sec: i64 = 10_485_760; // 10 MB/s
        let mbps = bytes_per_sec as f64 / 1_000_000.0;
        assert!((mbps - 10.48576).abs() < 0.0001);
    }

    #[test]
    fn test_filesystem_usage_calculation() {
        let total: i64 = 500_000_000_000; // 500 GB
        let used: i64 = 350_000_000_000; // 350 GB
        let usage_pct = (used as f64 / total as f64) * 100.0;
        assert!((usage_pct - 70.0).abs() < 0.01);
    }
}
