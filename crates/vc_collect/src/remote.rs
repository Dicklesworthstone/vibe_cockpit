//! Remote collector execution infrastructure
//!
//! This module provides the `RemoteCollector` wrapper and `MultiMachineCollector`
//! for executing collectors on remote machines via SSH.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────┐
//! │                 MultiMachineCollector               │
//! ├─────────────────────────────────────────────────────┤
//! │  For each (collector, machine) pair:                │
//! │    1. Check if tool available on machine            │
//! │    2. Get cursor from local store                   │
//! │    3. Execute collector remotely                    │
//! │    4. Parse and store results locally               │
//! └─────────────────────────────────────────────────────┘
//!           │
//!           ▼
//! ┌─────────────────┐     ┌─────────────────┐
//! │ RemoteCollector │────▶│    SshRunner    │
//! ├─────────────────┤     └─────────────────┘
//! │ exec_collect()  │
//! │ parse_output()  │
//! └─────────────────┘
//! ```
//!
//! # Data Tagging
//!
//! All data collected remotely is tagged with `machine_id` to identify its source.
//! This enables fleet-wide queries while maintaining data provenance.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::stream::{self, StreamExt};
use thiserror::Error;
use tracing::{debug, info, instrument, warn};

use crate::machine::{Machine, MachineFilter, MachineRegistry};
use crate::ssh::{SshError, SshRunner};
use crate::{CollectContext, CollectError, CollectResult, Collector, Cursor, RowBatch, Warning};

/// Errors specific to remote collection
#[derive(Error, Debug)]
pub enum RemoteCollectError {
    #[error("Tool '{tool}' not found on machine '{machine}'")]
    ToolNotFound { tool: String, machine: String },

    #[error("Remote command failed on {machine}: {stderr}")]
    RemoteCommandFailed {
        machine: String,
        cmd: String,
        exit_code: u32,
        stderr: String,
    },

    #[error("Failed to parse remote output: {0}")]
    ParseError(String),

    #[error("Machine '{0}' is offline")]
    MachineOffline(String),

    #[error("No SSH configuration for machine '{0}'")]
    NoSshConfig(String),

    #[error("Timeout after {0:?} on machine '{1}'")]
    Timeout(Duration, String),

    #[error("SSH error: {0}")]
    SshError(#[from] SshError),

    #[error("Collection error: {0}")]
    CollectError(#[from] CollectError),

    #[error("JSON parse error: {0}")]
    JsonError(#[from] serde_json::Error),
}

/// Result of collecting from a single machine
#[derive(Debug)]
pub struct MachineCollectResult {
    /// Machine ID
    pub machine_id: String,
    /// Collection result (if successful)
    pub result: Result<CollectResult, RemoteCollectError>,
    /// Duration of the collection
    pub duration: Duration,
    /// Whether the machine was online
    pub was_online: bool,
}

impl MachineCollectResult {
    /// Check if collection succeeded
    pub fn success(&self) -> bool {
        self.result.as_ref().is_ok_and(|r| r.success)
    }

    /// Get total rows collected (0 if failed)
    pub fn total_rows(&self) -> usize {
        self.result.as_ref().map(|r| r.total_rows()).unwrap_or(0)
    }
}

/// Summary of multi-machine collection
#[derive(Debug, Default)]
pub struct CollectionSummary {
    /// Total machines attempted
    pub machines_attempted: usize,
    /// Machines that succeeded
    pub machines_succeeded: usize,
    /// Machines that failed
    pub machines_failed: usize,
    /// Machines that were offline
    pub machines_offline: usize,
    /// Total rows collected
    pub total_rows: usize,
    /// Total collection duration
    pub total_duration: Duration,
    /// Per-machine results
    pub results: Vec<MachineCollectResult>,
}

impl CollectionSummary {
    /// Create a new empty summary
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a machine result to the summary
    pub fn add_result(&mut self, result: MachineCollectResult) {
        self.machines_attempted += 1;
        if result.success() {
            self.machines_succeeded += 1;
            self.total_rows += result.total_rows();
        } else {
            self.machines_failed += 1;
        }
        if !result.was_online {
            self.machines_offline += 1;
        }
        self.total_duration += result.duration;
        self.results.push(result);
    }

    /// Get success rate as a percentage
    pub fn success_rate(&self) -> f64 {
        if self.machines_attempted == 0 {
            0.0
        } else {
            (self.machines_succeeded as f64 / self.machines_attempted as f64) * 100.0
        }
    }
}

/// Configuration for remote collection
#[derive(Debug, Clone)]
pub struct RemoteCollectorConfig {
    /// Command timeout
    pub timeout: Duration,
    /// Maximum concurrent machines
    pub max_concurrent: usize,
    /// Whether to skip offline machines
    pub skip_offline: bool,
    /// Whether to check tool availability before collecting
    pub check_tools: bool,
    /// Poll window for incremental collectors
    pub poll_window: Duration,
}

impl Default for RemoteCollectorConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(60),
            max_concurrent: 4,
            skip_offline: true,
            check_tools: true,
            poll_window: Duration::from_secs(600),
        }
    }
}

/// Wrapper that executes a collector on a remote machine via SSH
///
/// This wrapper:
/// - Builds the appropriate command for the collector
/// - Executes it over SSH
/// - Parses the JSON output
/// - Tags all results with the machine_id
pub struct RemoteCollector<C: Collector> {
    inner: C,
    ssh: Arc<SshRunner>,
    config: RemoteCollectorConfig,
}

impl<C: Collector> RemoteCollector<C> {
    /// Create a new remote collector wrapper
    pub fn new(inner: C, ssh: Arc<SshRunner>) -> Self {
        Self {
            inner,
            ssh,
            config: RemoteCollectorConfig::default(),
        }
    }

    /// Create with custom configuration
    pub fn with_config(inner: C, ssh: Arc<SshRunner>, config: RemoteCollectorConfig) -> Self {
        Self { inner, ssh, config }
    }

    /// Get the inner collector
    pub fn inner(&self) -> &C {
        &self.inner
    }

    /// Execute collection on a remote machine
    #[instrument(skip(self, machine, cursor), fields(
        collector = %self.inner.name(),
        machine_id = %machine.machine_id
    ))]
    pub async fn collect_remote(
        &self,
        machine: &Machine,
        cursor: Option<&Cursor>,
    ) -> Result<CollectResult, RemoteCollectError> {
        let start = Instant::now();

        // Verify SSH config exists
        if machine.ssh_config().is_none() && !machine.is_local {
            return Err(RemoteCollectError::NoSshConfig(machine.machine_id.clone()));
        }

        // Build the remote command
        let cmd = self.build_command(cursor);
        debug!(cmd = %cmd, "Executing remote command");

        // Execute the command
        let output = self
            .ssh
            .exec_timeout(machine, &cmd, self.config.timeout)
            .await?;

        if output.exit_code != 0 {
            return Err(RemoteCollectError::RemoteCommandFailed {
                machine: machine.machine_id.clone(),
                cmd,
                exit_code: output.exit_code,
                stderr: output.stderr,
            });
        }

        // Parse the JSON output
        let mut result: CollectResult = serde_json::from_str(&output.stdout).map_err(|e| {
            RemoteCollectError::ParseError(format!(
                "Failed to parse collector output: {e}. Output was: {}",
                output.stdout.chars().take(200).collect::<String>()
            ))
        })?;

        // Tag all rows with machine_id
        Self::tag_rows_with_machine(&mut result, &machine.machine_id);

        // Update duration
        result.duration = start.elapsed();

        Ok(result)
    }

    /// Build the command to execute remotely
    fn build_command(&self, cursor: Option<&Cursor>) -> String {
        let tool = self.inner.required_tool().unwrap_or(self.inner.name());

        let mut cmd = format!("{} --robot --json", tool);

        // Add cursor argument if present
        if let Some(cursor) = cursor {
            match cursor {
                Cursor::Timestamp(ts) => {
                    cmd.push_str(&format!(" --since '{}'", ts.to_rfc3339()));
                }
                Cursor::PrimaryKey(pk) => {
                    cmd.push_str(&format!(" --since-id {}", pk));
                }
                Cursor::FileOffset { offset, .. } => {
                    cmd.push_str(&format!(" --offset {}", offset));
                }
                Cursor::Opaque(s) => {
                    cmd.push_str(&format!(" --cursor '{}'", s.replace('\'', "'\\''")));
                }
            }
        }

        cmd
    }

    /// Tag all rows with machine_id
    fn tag_rows_with_machine(result: &mut CollectResult, machine_id: &str) {
        for batch in &mut result.rows {
            for row in &mut batch.rows {
                if let serde_json::Value::Object(map) = row {
                    map.insert(
                        "machine_id".to_string(),
                        serde_json::Value::String(machine_id.to_string()),
                    );
                }
            }
        }
    }
}

/// Multi-machine collector for parallel collection across a fleet
///
/// This collector:
/// - Discovers machines that have the required tool
/// - Collects from all machines in parallel (bounded concurrency)
/// - Aggregates results with machine_id tagging
/// - Handles failures gracefully (continues with other machines)
pub struct MultiMachineCollector {
    ssh: Arc<SshRunner>,
    registry: Arc<MachineRegistry>,
    config: RemoteCollectorConfig,
    /// Cursor state per (collector, machine) pair
    cursors: HashMap<(String, String), Cursor>,
}

impl MultiMachineCollector {
    /// Create a new multi-machine collector
    pub fn new(ssh: Arc<SshRunner>, registry: Arc<MachineRegistry>) -> Self {
        Self {
            ssh,
            registry,
            config: RemoteCollectorConfig::default(),
            cursors: HashMap::new(),
        }
    }

    /// Create with custom configuration
    pub fn with_config(
        ssh: Arc<SshRunner>,
        registry: Arc<MachineRegistry>,
        config: RemoteCollectorConfig,
    ) -> Self {
        Self {
            ssh,
            registry,
            config,
            cursors: HashMap::new(),
        }
    }

    /// Set cursor for a specific (collector, machine) pair
    pub fn set_cursor(&mut self, collector: &str, machine_id: &str, cursor: Cursor) {
        self.cursors
            .insert((collector.to_string(), machine_id.to_string()), cursor);
    }

    /// Get cursor for a specific (collector, machine) pair
    pub fn get_cursor(&self, collector: &str, machine_id: &str) -> Option<&Cursor> {
        self.cursors
            .get(&(collector.to_string(), machine_id.to_string()))
    }

    /// Collect from all machines that have the required tool
    #[instrument(skip(self, collector), fields(collector = %collector.name()))]
    pub async fn collect_all<C: Collector + Clone + 'static>(
        &self,
        collector: C,
    ) -> CollectionSummary {
        let start = Instant::now();
        let collector_name = collector.name().to_string();

        // Get machines that should be collected from
        let filter = MachineFilter {
            enabled: Some(true),
            ..Default::default()
        };

        let machines = match self.registry.list_machines(Some(filter)) {
            Ok(m) => m,
            Err(e) => {
                warn!(error = %e, "Failed to list machines");
                return CollectionSummary::new();
            }
        };

        info!(
            collector = %collector_name,
            machine_count = machines.len(),
            "Starting multi-machine collection"
        );

        // Collect from all machines in parallel with bounded concurrency
        let results: Vec<MachineCollectResult> = stream::iter(machines)
            .map(|machine| {
                let collector = collector.clone();
                let ssh = self.ssh.clone();
                let config = self.config.clone();
                let cursor = self
                    .get_cursor(&collector_name, &machine.machine_id)
                    .cloned();

                async move {
                    let machine_id = machine.machine_id.clone();
                    let machine_start = Instant::now();

                    // Check if machine is local
                    if machine.is_local {
                        return self
                            .collect_local(&collector, &machine, cursor.as_ref())
                            .await;
                    }

                    // Check if SSH config exists
                    if machine.ssh_config().is_none() {
                        return MachineCollectResult {
                            machine_id,
                            result: Err(RemoteCollectError::NoSshConfig(
                                machine.machine_id.clone(),
                            )),
                            duration: machine_start.elapsed(),
                            was_online: false,
                        };
                    }

                    // Create remote collector wrapper
                    let remote = RemoteCollector::with_config(collector, ssh, config);

                    // Execute collection
                    let result = remote.collect_remote(&machine, cursor.as_ref()).await;

                    MachineCollectResult {
                        machine_id,
                        result,
                        duration: machine_start.elapsed(),
                        was_online: true,
                    }
                }
            })
            .buffer_unordered(self.config.max_concurrent)
            .collect()
            .await;

        // Build summary
        let mut summary = CollectionSummary::new();
        for result in results {
            summary.add_result(result);
        }
        summary.total_duration = start.elapsed();

        info!(
            collector = %collector_name,
            machines_succeeded = summary.machines_succeeded,
            machines_failed = summary.machines_failed,
            total_rows = summary.total_rows,
            duration_ms = summary.total_duration.as_millis(),
            "Multi-machine collection complete"
        );

        summary
    }

    /// Collect from a local machine
    async fn collect_local<C: Collector>(
        &self,
        collector: &C,
        machine: &Machine,
        cursor: Option<&Cursor>,
    ) -> MachineCollectResult {
        let start = Instant::now();
        let machine_id = machine.machine_id.clone();

        let ctx = CollectContext::local(&machine_id, self.config.timeout)
            .with_poll_window(self.config.poll_window);

        let ctx = if let Some(c) = cursor {
            ctx.with_cursor(c.clone())
        } else {
            ctx
        };

        let result = collector.collect(&ctx).await;

        MachineCollectResult {
            machine_id,
            result: result.map_err(RemoteCollectError::CollectError),
            duration: start.elapsed(),
            was_online: true,
        }
    }

    /// Collect from specific machines only
    #[instrument(skip(self, collector, machine_ids), fields(collector = %collector.name()))]
    pub async fn collect_from<C: Collector + Clone + 'static>(
        &self,
        collector: C,
        machine_ids: &[String],
    ) -> CollectionSummary {
        let start = Instant::now();
        let collector_name = collector.name().to_string();

        // Get specific machines
        let mut machines = Vec::new();
        for id in machine_ids {
            match self.registry.get_machine(id) {
                Ok(Some(m)) if m.enabled => machines.push(m),
                Ok(Some(_)) => {
                    debug!(machine_id = %id, "Machine is disabled, skipping");
                }
                Ok(None) => {
                    warn!(machine_id = %id, "Machine not found");
                }
                Err(e) => {
                    warn!(machine_id = %id, error = %e, "Failed to get machine");
                }
            }
        }

        info!(
            collector = %collector_name,
            requested = machine_ids.len(),
            found = machines.len(),
            "Starting targeted collection"
        );

        // Collect from machines in parallel
        let results: Vec<MachineCollectResult> = stream::iter(machines)
            .map(|machine| {
                let collector = collector.clone();
                let ssh = self.ssh.clone();
                let config = self.config.clone();
                let cursor = self
                    .get_cursor(&collector_name, &machine.machine_id)
                    .cloned();

                async move {
                    let machine_id = machine.machine_id.clone();
                    let machine_start = Instant::now();

                    if machine.is_local {
                        return self
                            .collect_local(&collector, &machine, cursor.as_ref())
                            .await;
                    }

                    if machine.ssh_config().is_none() {
                        return MachineCollectResult {
                            machine_id,
                            result: Err(RemoteCollectError::NoSshConfig(
                                machine.machine_id.clone(),
                            )),
                            duration: machine_start.elapsed(),
                            was_online: false,
                        };
                    }

                    let remote = RemoteCollector::with_config(collector, ssh, config);
                    let result = remote.collect_remote(&machine, cursor.as_ref()).await;

                    MachineCollectResult {
                        machine_id,
                        result,
                        duration: machine_start.elapsed(),
                        was_online: true,
                    }
                }
            })
            .buffer_unordered(self.config.max_concurrent)
            .collect()
            .await;

        let mut summary = CollectionSummary::new();
        for result in results {
            summary.add_result(result);
        }
        summary.total_duration = start.elapsed();

        summary
    }

    /// Aggregate results from multiple machines into a single CollectResult
    ///
    /// This merges all row batches and combines warnings/cursors.
    pub fn aggregate_results(results: &[MachineCollectResult]) -> CollectResult {
        let mut all_rows: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
        let mut all_warnings: Vec<Warning> = Vec::new();
        let mut total_duration = Duration::ZERO;
        let mut any_success = false;
        let mut errors: Vec<String> = Vec::new();

        for mcr in results {
            total_duration += mcr.duration;

            match &mcr.result {
                Ok(result) => {
                    any_success = true;

                    // Merge rows by table
                    for batch in &result.rows {
                        all_rows
                            .entry(batch.table.clone())
                            .or_default()
                            .extend(batch.rows.clone());
                    }

                    // Collect warnings
                    all_warnings.extend(result.warnings.clone());
                }
                Err(e) => {
                    errors.push(format!("{}: {}", mcr.machine_id, e));
                    all_warnings.push(Warning::error(format!(
                        "Collection failed on {}: {}",
                        mcr.machine_id, e
                    )));
                }
            }
        }

        // Convert to RowBatch vec
        let rows: Vec<RowBatch> = all_rows
            .into_iter()
            .map(|(table, rows)| RowBatch { table, rows })
            .collect();

        CollectResult {
            rows,
            new_cursor: None, // Multi-machine doesn't have a single cursor
            raw_artifacts: vec![],
            warnings: all_warnings,
            duration: total_duration,
            success: any_success,
            error: if errors.is_empty() {
                None
            } else {
                Some(errors.join("; "))
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collectors::DummyCollector;
    use chrono::Utc;
    use vc_store::VcStore;

    #[test]
    fn test_remote_collector_config_default() {
        let config = RemoteCollectorConfig::default();
        assert_eq!(config.timeout, Duration::from_secs(60));
        assert_eq!(config.max_concurrent, 4);
        assert!(config.skip_offline);
        assert!(config.check_tools);
    }

    #[test]
    fn test_collection_summary() {
        let mut summary = CollectionSummary::new();

        // Add successful result
        summary.add_result(MachineCollectResult {
            machine_id: "machine1".to_string(),
            result: Ok(CollectResult::with_rows(vec![RowBatch {
                table: "test".to_string(),
                rows: vec![serde_json::json!({"key": "value"})],
            }])),
            duration: Duration::from_millis(100),
            was_online: true,
        });

        // Add failed result
        summary.add_result(MachineCollectResult {
            machine_id: "machine2".to_string(),
            result: Err(RemoteCollectError::MachineOffline("machine2".to_string())),
            duration: Duration::from_millis(50),
            was_online: false,
        });

        assert_eq!(summary.machines_attempted, 2);
        assert_eq!(summary.machines_succeeded, 1);
        assert_eq!(summary.machines_failed, 1);
        assert_eq!(summary.machines_offline, 1);
        assert_eq!(summary.total_rows, 1);
        assert_eq!(summary.success_rate(), 50.0);
    }

    #[test]
    fn test_machine_collect_result_success() {
        let result = MachineCollectResult {
            machine_id: "test".to_string(),
            result: Ok(CollectResult::with_rows(vec![RowBatch {
                table: "test".to_string(),
                rows: vec![serde_json::json!({"a": 1}), serde_json::json!({"b": 2})],
            }])),
            duration: Duration::from_millis(100),
            was_online: true,
        };

        assert!(result.success());
        assert_eq!(result.total_rows(), 2);
    }

    #[test]
    fn test_machine_collect_result_failure() {
        let result = MachineCollectResult {
            machine_id: "test".to_string(),
            result: Err(RemoteCollectError::MachineOffline("test".to_string())),
            duration: Duration::from_millis(50),
            was_online: false,
        };

        assert!(!result.success());
        assert_eq!(result.total_rows(), 0);
    }

    #[test]
    fn test_tag_rows_with_machine() {
        let mut result = CollectResult::with_rows(vec![RowBatch {
            table: "test".to_string(),
            rows: vec![
                serde_json::json!({"key": "value1"}),
                serde_json::json!({"key": "value2"}),
            ],
        }]);

        RemoteCollector::<DummyCollector>::tag_rows_with_machine(&mut result, "orko");

        for batch in &result.rows {
            for row in &batch.rows {
                assert_eq!(row["machine_id"], "orko");
            }
        }
    }

    #[test]
    fn test_aggregate_results_success() {
        let results = vec![
            MachineCollectResult {
                machine_id: "m1".to_string(),
                result: Ok(CollectResult::with_rows(vec![RowBatch {
                    table: "test".to_string(),
                    rows: vec![serde_json::json!({"id": 1})],
                }])),
                duration: Duration::from_millis(100),
                was_online: true,
            },
            MachineCollectResult {
                machine_id: "m2".to_string(),
                result: Ok(CollectResult::with_rows(vec![RowBatch {
                    table: "test".to_string(),
                    rows: vec![serde_json::json!({"id": 2})],
                }])),
                duration: Duration::from_millis(150),
                was_online: true,
            },
        ];

        let aggregated = MultiMachineCollector::aggregate_results(&results);

        assert!(aggregated.success);
        assert_eq!(aggregated.total_rows(), 2);
        assert_eq!(aggregated.duration, Duration::from_millis(250));
        assert!(aggregated.error.is_none());
    }

    #[test]
    fn test_aggregate_results_partial_failure() {
        let results = vec![
            MachineCollectResult {
                machine_id: "m1".to_string(),
                result: Ok(CollectResult::with_rows(vec![RowBatch {
                    table: "test".to_string(),
                    rows: vec![serde_json::json!({"id": 1})],
                }])),
                duration: Duration::from_millis(100),
                was_online: true,
            },
            MachineCollectResult {
                machine_id: "m2".to_string(),
                result: Err(RemoteCollectError::MachineOffline("m2".to_string())),
                duration: Duration::from_millis(50),
                was_online: false,
            },
        ];

        let aggregated = MultiMachineCollector::aggregate_results(&results);

        assert!(aggregated.success); // At least one succeeded
        assert_eq!(aggregated.total_rows(), 1);
        assert!(!aggregated.warnings.is_empty());
        assert!(aggregated.error.is_some());
    }

    #[test]
    fn test_build_command_no_cursor() {
        let collector = DummyCollector;
        let ssh = Arc::new(SshRunner::new());
        let remote = RemoteCollector::new(collector, ssh);

        let cmd = remote.build_command(None);
        assert_eq!(cmd, "dummy --robot --json");
    }

    #[test]
    fn test_build_command_with_timestamp_cursor() {
        let collector = DummyCollector;
        let ssh = Arc::new(SshRunner::new());
        let remote = RemoteCollector::new(collector, ssh);

        let ts = Utc::now();
        let cursor = Cursor::Timestamp(ts);
        let cmd = remote.build_command(Some(&cursor));

        assert!(cmd.contains("--since"));
        assert!(cmd.contains(&ts.to_rfc3339()));
    }

    #[test]
    fn test_build_command_with_pk_cursor() {
        let collector = DummyCollector;
        let ssh = Arc::new(SshRunner::new());
        let remote = RemoteCollector::new(collector, ssh);

        let cursor = Cursor::primary_key(12345);
        let cmd = remote.build_command(Some(&cursor));

        assert!(cmd.contains("--since-id 12345"));
    }

    #[test]
    fn test_remote_collect_error_display() {
        let err = RemoteCollectError::ToolNotFound {
            tool: "caut".to_string(),
            machine: "orko".to_string(),
        };
        assert!(err.to_string().contains("caut"));
        assert!(err.to_string().contains("orko"));

        let err = RemoteCollectError::RemoteCommandFailed {
            machine: "orko".to_string(),
            cmd: "test cmd".to_string(),
            exit_code: 1,
            stderr: "error output".to_string(),
        };
        assert!(err.to_string().contains("orko"));
        assert!(err.to_string().contains("error output"));
    }

    #[tokio::test]
    async fn test_multi_machine_collector_creation() {
        let store = Arc::new(VcStore::open_memory().unwrap());
        let registry = Arc::new(MachineRegistry::new(store));
        let ssh = Arc::new(SshRunner::new());

        let mmc = MultiMachineCollector::new(ssh, registry);
        assert_eq!(mmc.config.max_concurrent, 4);
    }

    #[test]
    fn test_multi_machine_collector_cursor_management() {
        let store = Arc::new(VcStore::open_memory().unwrap());
        let registry = Arc::new(MachineRegistry::new(store));
        let ssh = Arc::new(SshRunner::new());

        let mut mmc = MultiMachineCollector::new(ssh, registry);

        // Set cursor
        mmc.set_cursor("sysmoni", "orko", Cursor::primary_key(100));

        // Get cursor
        let cursor = mmc.get_cursor("sysmoni", "orko");
        assert!(cursor.is_some());
        assert_eq!(cursor.unwrap(), &Cursor::primary_key(100));

        // Non-existent cursor
        let none = mmc.get_cursor("sysmoni", "other");
        assert!(none.is_none());
    }
}
