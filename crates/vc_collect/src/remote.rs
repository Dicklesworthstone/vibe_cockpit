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
use std::fmt::Write as _;
use std::sync::Arc;
use std::time::{Duration, Instant};

use asupersync::sync::{AcquireError, Semaphore};
use dashmap::DashMap;
use futures::stream::{self, StreamExt};
use thiserror::Error;
use tracing::{debug, info, instrument, warn};
use vc_config::CollectorConfig as VcCollectorConfig;

use crate::machine::{Machine, MachineFilter, MachineRegistry};
use crate::ssh::{SshError, SshRunner};
use crate::{CollectContext, CollectError, CollectResult, Collector, Cursor, RowBatch, Warning};

type RemoteCollectOutcome = asupersync::Outcome<CollectResult, RemoteCollectError>;

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
    pub result: RemoteCollectOutcome,
    /// Duration of the collection
    pub duration: Duration,
    /// Whether the machine was online
    pub was_online: bool,
}

impl MachineCollectResult {
    /// Check if collection succeeded
    #[must_use]
    pub fn success(&self) -> bool {
        matches!(&self.result, asupersync::Outcome::Ok(result) if result.success)
    }

    /// Get total rows collected (0 if failed)
    #[must_use]
    pub fn total_rows(&self) -> usize {
        match &self.result {
            asupersync::Outcome::Ok(result) => result.total_rows(),
            _ => 0,
        }
    }

    /// Check if collection was cancelled.
    #[must_use]
    pub fn cancelled(&self) -> bool {
        self.result.is_cancelled()
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
    /// Machines that were cancelled
    pub machines_cancelled: usize,
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
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a machine result to the summary
    pub fn add_result(&mut self, result: MachineCollectResult) {
        self.machines_attempted += 1;
        if result.success() {
            self.machines_succeeded += 1;
            self.total_rows += result.total_rows();
        } else if result.cancelled() {
            self.machines_cancelled += 1;
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
    #[must_use]
    pub fn success_rate(&self) -> f64 {
        if self.machines_attempted == 0 {
            0.0
        } else {
            let succeeded = u32::try_from(self.machines_succeeded).unwrap_or(u32::MAX);
            let attempted = u32::try_from(self.machines_attempted).unwrap_or(u32::MAX);
            (f64::from(succeeded) / f64::from(attempted)) * 100.0
        }
    }
}

/// Configuration for remote collection
#[derive(Debug, Clone)]
pub struct RemoteCollectorConfig {
    /// Command timeout
    pub timeout: Duration,
    /// Maximum concurrent collector operations across the fleet
    pub max_concurrent_collectors: usize,
    /// Maximum concurrent collector operations against one machine
    pub max_concurrent_per_machine: usize,
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
            timeout: Duration::from_mins(1),
            max_concurrent_collectors: 8,
            max_concurrent_per_machine: 4,
            skip_offline: true,
            check_tools: true,
            poll_window: Duration::from_mins(10),
        }
    }
}

impl From<&VcCollectorConfig> for RemoteCollectorConfig {
    fn from(config: &VcCollectorConfig) -> Self {
        Self {
            timeout: Duration::from_secs(config.timeout_secs),
            max_concurrent_collectors: usize::try_from(config.max_concurrent_collectors)
                .unwrap_or(usize::MAX),
            max_concurrent_per_machine: usize::try_from(config.max_concurrent_per_machine)
                .unwrap_or(usize::MAX),
            skip_offline: true,
            check_tools: true,
            poll_window: Duration::from_mins(10),
        }
    }
}

fn backpressure_outcome(
    cx: &asupersync::Cx,
    scope: &'static str,
    error: AcquireError,
) -> RemoteCollectOutcome {
    match error {
        AcquireError::Cancelled => asupersync::Outcome::Cancelled(
            cx.cancel_reason()
                .unwrap_or_else(asupersync::CancelReason::parent_cancelled),
        ),
        other => asupersync::Outcome::Err(RemoteCollectError::CollectError(CollectError::Other(
            format!("failed to acquire {scope} collector permit: {other}"),
        ))),
    }
}

/// Wrapper that executes a collector on a remote machine via SSH
///
/// This wrapper:
/// - Builds the appropriate command for the collector
/// - Executes it over SSH
/// - Parses the JSON output
/// - Tags all results with the `machine_id`
pub struct RemoteCollector<C: Collector> {
    inner: C,
    ssh: Arc<SshRunner>,
    config: RemoteCollectorConfig,
}

impl<C: Collector> RemoteCollector<C> {
    /// Create a new remote collector wrapper
    #[must_use]
    pub fn new(inner: C, ssh: Arc<SshRunner>) -> Self {
        Self {
            inner,
            ssh,
            config: RemoteCollectorConfig::default(),
        }
    }

    /// Create with custom configuration
    #[must_use]
    pub fn with_config(inner: C, ssh: Arc<SshRunner>, config: RemoteCollectorConfig) -> Self {
        Self { inner, ssh, config }
    }

    /// Get the inner collector
    pub fn inner(&self) -> &C {
        &self.inner
    }

    /// Execute collection on a remote machine
    ///
    /// # Errors
    ///
    /// Returns [`RemoteCollectError`] when SSH configuration is missing, remote execution fails,
    /// or the collector output cannot be parsed.
    #[instrument(skip(self, machine, cursor), fields(
        collector = %self.inner.name(),
        machine_id = %machine.machine_id
    ))]
    pub async fn collect_remote(
        &self,
        cx: &asupersync::Cx,
        machine: &Machine,
        cursor: Option<&Cursor>,
    ) -> RemoteCollectOutcome {
        let start = Instant::now();

        // Verify SSH config exists
        if machine.ssh_config().is_none() && !machine.is_local {
            return asupersync::Outcome::Err(RemoteCollectError::NoSshConfig(
                machine.machine_id.clone(),
            ));
        }

        // Build the remote command
        let cmd = self.build_command(cursor);
        debug!(cmd = %cmd, "Executing remote command");

        // Execute the command
        let output = match self
            .ssh
            .exec_timeout_with_cx(cx, machine, &cmd, self.config.timeout)
            .await
        {
            Ok(output) => output,
            Err(SshError::Cancelled(reason)) => return asupersync::Outcome::Cancelled(reason),
            Err(error) => return asupersync::Outcome::Err(error.into()),
        };

        crate::collect_checkpoint!(cx, "post_ssh_command_pre_parse");

        if output.exit_code != 0 {
            return asupersync::Outcome::Err(RemoteCollectError::RemoteCommandFailed {
                machine: machine.machine_id.clone(),
                cmd,
                exit_code: output.exit_code,
                stderr: output.stderr,
            });
        }

        // Parse the JSON output
        let mut result: CollectResult = match serde_json::from_str(&output.stdout) {
            Ok(result) => result,
            Err(e) => {
                return asupersync::Outcome::Err(RemoteCollectError::ParseError(format!(
                    "Failed to parse collector output: {e}. Output was: {}",
                    output.stdout.chars().take(200).collect::<String>()
                )));
            }
        };

        crate::collect_checkpoint!(cx, "post_parse_pre_return");

        // Tag all rows with machine_id
        Self::tag_rows_with_machine(&mut result, &machine.machine_id);

        // Update duration
        result.duration = start.elapsed();

        crate::collect_checkpoint!(cx, "collect_complete");
        asupersync::Outcome::Ok(result)
    }

    /// Build the command to execute remotely
    fn build_command(&self, cursor: Option<&Cursor>) -> String {
        let tool = self.inner.required_tool().unwrap_or(self.inner.name());

        let mut cmd = format!("{tool} --robot --json");

        // Add cursor argument if present
        if let Some(cursor) = cursor {
            match cursor {
                Cursor::Timestamp(ts) => {
                    write!(cmd, " --since '{}'", ts.to_rfc3339())
                        .expect("writing to String cannot fail");
                }
                Cursor::PrimaryKey(pk) => {
                    write!(cmd, " --since-id {pk}").expect("writing to String cannot fail");
                }
                Cursor::FileOffset { offset, .. } => {
                    write!(cmd, " --offset {offset}").expect("writing to String cannot fail");
                }
                Cursor::Opaque(s) => {
                    write!(cmd, " --cursor '{}'", s.replace('\'', "'\\''"))
                        .expect("writing to String cannot fail");
                }
            }
        }

        cmd
    }

    /// Tag all rows with `machine_id`.
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
/// - Aggregates results with `machine_id` tagging
/// - Handles failures gracefully (continues with other machines)
pub struct MultiMachineCollector {
    ssh: Arc<SshRunner>,
    registry: Arc<MachineRegistry>,
    config: RemoteCollectorConfig,
    global_limiter: Arc<Semaphore>,
    machine_limiters: DashMap<String, Arc<Semaphore>>,
    /// Cursor state per (collector, machine) pair
    cursors: HashMap<(String, String), Cursor>,
}

impl MultiMachineCollector {
    /// Create a new multi-machine collector
    #[must_use]
    pub fn new(ssh: Arc<SshRunner>, registry: Arc<MachineRegistry>) -> Self {
        let config = RemoteCollectorConfig::default();
        Self {
            ssh,
            registry,
            global_limiter: Arc::new(Semaphore::new(config.max_concurrent_collectors)),
            machine_limiters: DashMap::new(),
            config,
            cursors: HashMap::new(),
        }
    }

    /// Create with custom configuration
    #[must_use]
    pub fn with_config(
        ssh: Arc<SshRunner>,
        registry: Arc<MachineRegistry>,
        config: RemoteCollectorConfig,
    ) -> Self {
        let global_limiter = Arc::new(Semaphore::new(config.max_concurrent_collectors));
        Self {
            ssh,
            registry,
            global_limiter,
            machine_limiters: DashMap::new(),
            config,
            cursors: HashMap::new(),
        }
    }

    fn machine_limiter(&self, machine_id: &str) -> Arc<Semaphore> {
        self.machine_limiters
            .entry(machine_id.to_string())
            .or_insert_with(|| Arc::new(Semaphore::new(self.config.max_concurrent_per_machine)))
            .clone()
    }

    /// Set cursor for a specific (collector, machine) pair
    pub fn set_cursor(&mut self, collector: &str, machine_id: &str, cursor: Cursor) {
        self.cursors
            .insert((collector.to_string(), machine_id.to_string()), cursor);
    }

    /// Get cursor for a specific (collector, machine) pair
    #[must_use]
    pub fn get_cursor(&self, collector: &str, machine_id: &str) -> Option<&Cursor> {
        self.cursors
            .get(&(collector.to_string(), machine_id.to_string()))
    }

    /// Collect from all machines that have the required tool
    ///
    /// Each collector task receives a cloned `Cx` for structured cancellation
    /// and checkpoint support.
    #[instrument(skip(self, cx, collector), fields(collector = %collector.name()))]
    pub async fn collect_all<C: Collector + Clone + 'static>(
        &self,
        cx: &asupersync::Cx,
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

        let machine_count = machines.len();
        info!(
            collector = %collector_name,
            machine_count,
            max_concurrent_collectors = self.config.max_concurrent_collectors,
            max_concurrent_per_machine = self.config.max_concurrent_per_machine,
            cancel_requested = cx.is_cancel_requested(),
            "Collection region: spawning collector tasks"
        );

        // Collect from all machines in parallel with bounded concurrency.
        // Each task gets a cloned Cx for structured cancellation support.
        // TODO(bd-qdp): Wrap in asupersync region with budget deadline for
        // graceful drain when the high-level region API is available.
        let this = self;
        let results: Vec<MachineCollectResult> = stream::iter(machines)
            .map(|machine| {
                let cx = cx.clone();
                let collector = collector.clone();
                let cursor = this
                    .get_cursor(&collector_name, &machine.machine_id)
                    .cloned();

                async move { this.collect_machine(&cx, collector, machine, cursor).await }
            })
            .buffer_unordered(self.config.max_concurrent_collectors)
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
            machine_count,
            machines_succeeded = summary.machines_succeeded,
            machines_failed = summary.machines_failed,
            machines_cancelled = summary.machines_cancelled,
            total_rows = summary.total_rows,
            duration_ms = summary.total_duration.as_millis(),
            "Collection region: all tasks drained"
        );

        summary
    }

    /// Collect from a local machine
    async fn collect_local<C: Collector>(
        &self,
        cx: &asupersync::Cx,
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

        let result = collector.collect(cx, &ctx).await;

        MachineCollectResult {
            machine_id,
            result: result.map_err(RemoteCollectError::CollectError),
            duration: start.elapsed(),
            was_online: true,
        }
    }

    async fn collect_machine<C: Collector + 'static>(
        &self,
        cx: &asupersync::Cx,
        collector: C,
        machine: Machine,
        cursor: Option<Cursor>,
    ) -> MachineCollectResult {
        let machine_id = machine.machine_id.clone();
        let machine_start = Instant::now();

        if !machine.is_local && machine.ssh_config().is_none() {
            return MachineCollectResult {
                machine_id,
                result: asupersync::Outcome::Err(RemoteCollectError::NoSshConfig(
                    machine.machine_id.clone(),
                )),
                duration: machine_start.elapsed(),
                was_online: false,
            };
        }

        let machine_limiter = self.machine_limiter(&machine.machine_id);
        let machine_wait_start = Instant::now();
        let _machine_permit = match machine_limiter.acquire(cx, 1).await {
            Ok(permit) => {
                debug!(
                    collector = %collector.name(),
                    machine_id = %machine.machine_id,
                    wait_ms = machine_wait_start.elapsed().as_millis(),
                    "Acquired per-machine collector permit"
                );
                permit
            }
            Err(error) => {
                return MachineCollectResult {
                    machine_id,
                    result: backpressure_outcome(cx, "per-machine", error),
                    duration: machine_start.elapsed(),
                    was_online: true,
                };
            }
        };

        let global_wait_start = Instant::now();
        let _global_permit = match self.global_limiter.acquire(cx, 1).await {
            Ok(permit) => {
                debug!(
                    collector = %collector.name(),
                    machine_id = %machine.machine_id,
                    wait_ms = global_wait_start.elapsed().as_millis(),
                    "Acquired global collector permit"
                );
                permit
            }
            Err(error) => {
                return MachineCollectResult {
                    machine_id,
                    result: backpressure_outcome(cx, "global", error),
                    duration: machine_start.elapsed(),
                    was_online: true,
                };
            }
        };

        if machine.is_local {
            return self
                .collect_local(cx, &collector, &machine, cursor.as_ref())
                .await;
        }

        let remote = RemoteCollector::with_config(collector, self.ssh.clone(), self.config.clone());
        let result = remote.collect_remote(cx, &machine, cursor.as_ref()).await;

        MachineCollectResult {
            machine_id,
            result,
            duration: machine_start.elapsed(),
            was_online: true,
        }
    }

    /// Collect from specific machines only
    #[instrument(skip(self, cx, collector, machine_ids), fields(collector = %collector.name()))]
    pub async fn collect_from<C: Collector + Clone + 'static>(
        &self,
        cx: &asupersync::Cx,
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
        let this = self;
        let results: Vec<MachineCollectResult> = stream::iter(machines)
            .map(|machine| {
                let cx = cx.clone();
                let collector = collector.clone();
                let cursor = this
                    .get_cursor(&collector_name, &machine.machine_id)
                    .cloned();

                async move { this.collect_machine(&cx, collector, machine, cursor).await }
            })
            .buffer_unordered(self.config.max_concurrent_collectors)
            .collect()
            .await;

        let mut summary = CollectionSummary::new();
        for result in results {
            summary.add_result(result);
        }
        summary.total_duration = start.elapsed();

        summary
    }

    /// Aggregate results from multiple machines into a single `CollectResult`.
    ///
    /// This merges all row batches and combines warnings/cursors.
    #[must_use]
    pub fn aggregate_results(results: &[MachineCollectResult]) -> CollectResult {
        let mut all_rows: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
        let mut all_warnings: Vec<Warning> = Vec::new();
        let mut total_duration = Duration::ZERO;
        let mut any_success = false;
        let mut errors: Vec<String> = Vec::new();

        for mcr in results {
            total_duration += mcr.duration;

            match &mcr.result {
                asupersync::Outcome::Ok(result) => {
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
                asupersync::Outcome::Err(e) => {
                    errors.push(format!("{}: {e}", mcr.machine_id));
                    all_warnings.push(Warning::error(format!(
                        "Collection failed on {}: {e}",
                        mcr.machine_id
                    )));
                }
                asupersync::Outcome::Cancelled(reason) => {
                    all_warnings.push(Warning::info(format!(
                        "Collection cancelled on {}: {reason:?}",
                        mcr.machine_id
                    )));
                }
                asupersync::Outcome::Panicked(payload) => {
                    errors.push(format!("{}: {payload}", mcr.machine_id));
                    all_warnings.push(Warning::error(format!(
                        "Collection panicked on {}: {payload}",
                        mcr.machine_id
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
    use crate::{CollectOutcome, collect_checkpoint};
    use chrono::Utc;
    use std::collections::HashMap as StdHashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use vc_store::VcStore;

    #[test]
    fn test_remote_collector_config_default() {
        let config = RemoteCollectorConfig::default();
        assert_eq!(config.timeout, Duration::from_mins(1));
        assert_eq!(config.max_concurrent_collectors, 8);
        assert_eq!(config.max_concurrent_per_machine, 4);
        assert!(config.skip_offline);
        assert!(config.check_tools);
    }

    #[test]
    fn test_remote_collector_config_from_vc_config() {
        let collector_config = VcCollectorConfig {
            timeout_secs: 45,
            max_concurrent_collectors: 6,
            max_concurrent_per_machine: 2,
            ..VcCollectorConfig::default()
        };

        let remote_config = RemoteCollectorConfig::from(&collector_config);

        assert_eq!(remote_config.timeout, Duration::from_secs(45));
        assert_eq!(remote_config.max_concurrent_collectors, 6);
        assert_eq!(remote_config.max_concurrent_per_machine, 2);
        assert!(remote_config.skip_offline);
        assert!(remote_config.check_tools);
    }

    #[test]
    fn test_collection_summary() {
        let mut summary = CollectionSummary::new();

        // Add successful result
        summary.add_result(MachineCollectResult {
            machine_id: "machine1".to_string(),
            result: asupersync::Outcome::Ok(CollectResult::with_rows(vec![RowBatch {
                table: "test".to_string(),
                rows: vec![serde_json::json!({"key": "value"})],
            }])),
            duration: Duration::from_millis(100),
            was_online: true,
        });

        // Add failed result
        summary.add_result(MachineCollectResult {
            machine_id: "machine2".to_string(),
            result: asupersync::Outcome::Err(RemoteCollectError::MachineOffline(
                "machine2".to_string(),
            )),
            duration: Duration::from_millis(50),
            was_online: false,
        });

        assert_eq!(summary.machines_attempted, 2);
        assert_eq!(summary.machines_succeeded, 1);
        assert_eq!(summary.machines_failed, 1);
        assert_eq!(summary.machines_cancelled, 0);
        assert_eq!(summary.machines_offline, 1);
        assert_eq!(summary.total_rows, 1);
        assert!((summary.success_rate() - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_machine_collect_result_success() {
        let result = MachineCollectResult {
            machine_id: "test".to_string(),
            result: asupersync::Outcome::Ok(CollectResult::with_rows(vec![RowBatch {
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
            result: asupersync::Outcome::Err(RemoteCollectError::MachineOffline(
                "test".to_string(),
            )),
            duration: Duration::from_millis(50),
            was_online: false,
        };

        assert!(!result.success());
        assert_eq!(result.total_rows(), 0);
    }

    #[test]
    fn test_machine_collect_result_cancelled() {
        let result = MachineCollectResult {
            machine_id: "test".to_string(),
            result: asupersync::Outcome::Cancelled(asupersync::CancelReason::user("cancelled")),
            duration: Duration::from_millis(10),
            was_online: true,
        };

        assert!(!result.success());
        assert!(result.cancelled());
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
                result: asupersync::Outcome::Ok(CollectResult::with_rows(vec![RowBatch {
                    table: "test".to_string(),
                    rows: vec![serde_json::json!({"id": 1})],
                }])),
                duration: Duration::from_millis(100),
                was_online: true,
            },
            MachineCollectResult {
                machine_id: "m2".to_string(),
                result: asupersync::Outcome::Ok(CollectResult::with_rows(vec![RowBatch {
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
                result: asupersync::Outcome::Ok(CollectResult::with_rows(vec![RowBatch {
                    table: "test".to_string(),
                    rows: vec![serde_json::json!({"id": 1})],
                }])),
                duration: Duration::from_millis(100),
                was_online: true,
            },
            MachineCollectResult {
                machine_id: "m2".to_string(),
                result: asupersync::Outcome::Err(RemoteCollectError::MachineOffline(
                    "m2".to_string(),
                )),
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
    fn test_aggregate_results_cancelled_is_not_error() {
        let results = vec![MachineCollectResult {
            machine_id: "m1".to_string(),
            result: asupersync::Outcome::Cancelled(asupersync::CancelReason::user(
                "cancelled after remote command",
            )),
            duration: Duration::from_millis(25),
            was_online: true,
        }];

        let aggregated = MultiMachineCollector::aggregate_results(&results);

        assert!(!aggregated.success);
        assert!(aggregated.error.is_none());
        assert_eq!(aggregated.warnings.len(), 1);
        assert_eq!(aggregated.warnings[0].level, crate::WarningLevel::Info);
        assert!(
            aggregated.warnings[0]
                .message
                .contains("Collection cancelled on m1")
        );
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

    #[test]
    fn test_multi_machine_collector_creation() {
        let store = Arc::new(VcStore::open_memory().unwrap());
        let registry = Arc::new(MachineRegistry::new(store));
        let ssh = Arc::new(SshRunner::new());

        let mmc = MultiMachineCollector::new(ssh, registry);
        assert_eq!(mmc.config.max_concurrent_collectors, 8);
        assert_eq!(mmc.config.max_concurrent_per_machine, 4);
        assert_eq!(mmc.global_limiter.available_permits(), 8);
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

    #[derive(Clone)]
    struct BlockingCollector {
        name: &'static str,
        active: Arc<AtomicUsize>,
        max_active: Arc<AtomicUsize>,
        sleep: Duration,
    }

    impl BlockingCollector {
        fn new(name: &'static str, active: Arc<AtomicUsize>, max_active: Arc<AtomicUsize>) -> Self {
            Self::with_sleep(name, active, max_active, Duration::from_millis(25))
        }

        fn with_sleep(
            name: &'static str,
            active: Arc<AtomicUsize>,
            max_active: Arc<AtomicUsize>,
            sleep: Duration,
        ) -> Self {
            Self {
                name,
                active,
                max_active,
                sleep,
            }
        }
    }

    #[async_trait::async_trait]
    impl Collector for BlockingCollector {
        fn name(&self) -> &'static str {
            self.name
        }

        async fn collect(&self, cx: &asupersync::Cx, ctx: &CollectContext) -> CollectOutcome {
            collect_checkpoint!(cx, "blocking_collector:start");

            let current = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            let _ = self.max_active.fetch_max(current, Ordering::SeqCst);

            asupersync::time::sleep(asupersync::time::wall_now(), self.sleep).await;

            self.active.fetch_sub(1, Ordering::SeqCst);
            asupersync::Outcome::Ok(CollectResult::with_rows(vec![RowBatch {
                table: "collector_test".to_string(),
                rows: vec![serde_json::json!({
                    "machine_id": &ctx.machine_id,
                    "collector": self.name,
                })],
            }]))
        }
    }

    fn machine_config(name: &str) -> vc_config::MachineConfig {
        vc_config::MachineConfig {
            name: name.to_string(),
            ssh_host: None,
            ssh_user: None,
            ssh_key: None,
            ssh_port: 22,
            enabled: true,
            collectors: StdHashMap::new(),
            tags: vec![],
        }
    }

    fn registry_with_machines(machine_ids: &[&str]) -> Arc<MachineRegistry> {
        let store = Arc::new(VcStore::open_memory().unwrap());
        let registry = Arc::new(MachineRegistry::new(store));
        let mut config = vc_config::VcConfig::default();

        for machine_id in machine_ids {
            config
                .machines
                .insert((*machine_id).to_string(), machine_config(machine_id));
        }

        registry.load_from_config(&config).unwrap();
        registry
    }

    #[test]
    fn test_global_backpressure_serializes_collectors() {
        crate::run_async_test(async {
            let registry = registry_with_machines(&["machine-a", "machine-b"]);
            let ssh = Arc::new(SshRunner::new());
            let mmc = MultiMachineCollector::with_config(
                ssh,
                registry,
                RemoteCollectorConfig {
                    max_concurrent_collectors: 1,
                    max_concurrent_per_machine: 4,
                    ..RemoteCollectorConfig::default()
                },
            );

            let active = Arc::new(AtomicUsize::new(0));
            let max_active = Arc::new(AtomicUsize::new(0));
            let collector = BlockingCollector::new("global-limit", active, max_active.clone());
            let cx = asupersync::Cx::for_testing();

            let machine_a = vec!["machine-a".to_string()];
            let machine_b = vec!["machine-b".to_string()];

            let (_left, _right) = futures::future::join(
                mmc.collect_from(&cx, collector.clone(), &machine_a),
                mmc.collect_from(&cx, collector, &machine_b),
            )
            .await;

            assert_eq!(max_active.load(Ordering::SeqCst), 1);
        });
    }

    #[test]
    fn test_per_machine_backpressure_serializes_same_machine() {
        crate::run_async_test(async {
            let registry = registry_with_machines(&["machine-a"]);
            let ssh = Arc::new(SshRunner::new());
            let mmc = MultiMachineCollector::with_config(
                ssh,
                registry,
                RemoteCollectorConfig {
                    max_concurrent_collectors: 4,
                    max_concurrent_per_machine: 1,
                    ..RemoteCollectorConfig::default()
                },
            );

            let active = Arc::new(AtomicUsize::new(0));
            let max_active = Arc::new(AtomicUsize::new(0));
            let collector = BlockingCollector::new("machine-limit", active, max_active.clone());
            let cx = asupersync::Cx::for_testing();
            let machine = vec!["machine-a".to_string()];

            let (_left, _right) = futures::future::join(
                mmc.collect_from(&cx, collector.clone(), &machine),
                mmc.collect_from(&cx, collector, &machine),
            )
            .await;

            assert_eq!(max_active.load(Ordering::SeqCst), 1);
        });
    }

    #[test]
    fn test_global_backpressure_cancel_is_leak_free() {
        crate::run_async_test(async {
            let registry = registry_with_machines(&["machine-a"]);
            let ssh = Arc::new(SshRunner::new());
            let mmc = MultiMachineCollector::with_config(
                ssh,
                registry,
                RemoteCollectorConfig {
                    max_concurrent_collectors: 1,
                    max_concurrent_per_machine: 1,
                    ..RemoteCollectorConfig::default()
                },
            );

            let holder_cx = asupersync::Cx::for_testing();
            let held = mmc
                .global_limiter
                .acquire(&holder_cx, 1)
                .await
                .expect("holder should acquire permit");

            let active = Arc::new(AtomicUsize::new(0));
            let max_active = Arc::new(AtomicUsize::new(0));
            let collector = BlockingCollector::new("cancelled-limit", active, max_active.clone());
            let cancel_cx = asupersync::Cx::for_testing();
            let cancel_trigger = cancel_cx.clone();
            let machine = vec!["machine-a".to_string()];

            let (summary, ()) = futures::future::join(
                mmc.collect_from(&cancel_cx, collector, &machine),
                async move {
                    asupersync::time::sleep(
                        asupersync::time::wall_now(),
                        Duration::from_millis(10),
                    )
                    .await;
                    cancel_trigger.set_cancel_requested(true);
                    asupersync::time::sleep(
                        asupersync::time::wall_now(),
                        Duration::from_millis(10),
                    )
                    .await;
                    drop(held);
                },
            )
            .await;

            assert_eq!(summary.machines_cancelled, 1);
            assert_eq!(summary.machines_failed, 0);
            assert_eq!(summary.results.len(), 1);
            assert!(summary.results[0].cancelled());

            assert_eq!(mmc.global_limiter.available_permits(), 1);
            assert_eq!(mmc.machine_limiter("machine-a").available_permits(), 1);
            assert_eq!(max_active.load(Ordering::SeqCst), 0);
        });
    }

    #[test]
    fn test_per_machine_waiters_do_not_consume_global_capacity() {
        crate::run_async_test(async {
            let registry = registry_with_machines(&["machine-a", "machine-b"]);
            let ssh = Arc::new(SshRunner::new());
            let mmc = MultiMachineCollector::with_config(
                ssh,
                registry,
                RemoteCollectorConfig {
                    max_concurrent_collectors: 2,
                    max_concurrent_per_machine: 1,
                    ..RemoteCollectorConfig::default()
                },
            );

            let active = Arc::new(AtomicUsize::new(0));
            let max_active = Arc::new(AtomicUsize::new(0));
            let collector = BlockingCollector::with_sleep(
                "global-capacity",
                active,
                max_active.clone(),
                Duration::from_millis(50),
            );
            let cx = asupersync::Cx::for_testing();
            let machine_a = vec!["machine-a".to_string()];
            let machine_b = vec!["machine-b".to_string()];

            let _results = futures::future::join3(
                mmc.collect_from(&cx, collector.clone(), &machine_a),
                mmc.collect_from(&cx, collector.clone(), &machine_a),
                mmc.collect_from(&cx, collector, &machine_b),
            )
            .await;

            assert_eq!(max_active.load(Ordering::SeqCst), 2);
            assert_eq!(mmc.global_limiter.available_permits(), 2);
            assert_eq!(mmc.machine_limiter("machine-a").available_permits(), 1);
            assert_eq!(mmc.machine_limiter("machine-b").available_permits(), 1);
        });
    }

    #[test]
    fn test_global_backpressure_caps_large_workload_and_returns_permits() {
        crate::run_async_test(async {
            let machine_ids = [
                "local",
                "machine-00",
                "machine-01",
                "machine-02",
                "machine-03",
                "machine-04",
                "machine-05",
                "machine-06",
                "machine-07",
                "machine-08",
                "machine-09",
                "machine-10",
                "machine-11",
                "machine-12",
            ];
            let registry = registry_with_machines(&machine_ids);
            let ssh = Arc::new(SshRunner::new());
            let mmc = MultiMachineCollector::with_config(
                ssh,
                registry,
                RemoteCollectorConfig {
                    max_concurrent_collectors: 8,
                    max_concurrent_per_machine: 4,
                    ..RemoteCollectorConfig::default()
                },
            );

            let active = Arc::new(AtomicUsize::new(0));
            let max_active = Arc::new(AtomicUsize::new(0));
            let collector = BlockingCollector::with_sleep(
                "large-workload",
                active,
                max_active.clone(),
                Duration::from_millis(50),
            );
            let cx = asupersync::Cx::for_testing();

            let summary = mmc.collect_all(&cx, collector).await;

            assert_eq!(summary.machines_attempted, machine_ids.len());
            assert_eq!(summary.machines_succeeded, machine_ids.len());
            assert_eq!(max_active.load(Ordering::SeqCst), 8);
            assert_eq!(mmc.global_limiter.available_permits(), 8);

            for machine_id in machine_ids {
                assert_eq!(mmc.machine_limiter(machine_id).available_permits(), 4);
            }
        });
    }

    #[test]
    fn test_per_machine_backpressure_respects_configured_limit_and_returns_permits() {
        crate::run_async_test(async {
            let registry = registry_with_machines(&["machine-a"]);
            let ssh = Arc::new(SshRunner::new());
            let mmc = MultiMachineCollector::with_config(
                ssh,
                registry,
                RemoteCollectorConfig {
                    max_concurrent_collectors: 8,
                    max_concurrent_per_machine: 4,
                    ..RemoteCollectorConfig::default()
                },
            );

            let active = Arc::new(AtomicUsize::new(0));
            let max_active = Arc::new(AtomicUsize::new(0));
            let collector = BlockingCollector::with_sleep(
                "per-machine-cap",
                active,
                max_active.clone(),
                Duration::from_millis(50),
            );
            let cx = asupersync::Cx::for_testing();
            let machine = vec!["machine-a".to_string()];

            let results = futures::future::join_all(
                (0..6).map(|_| mmc.collect_from(&cx, collector.clone(), &machine)),
            )
            .await;

            assert!(
                results
                    .iter()
                    .all(|summary| summary.machines_succeeded == 1)
            );
            assert_eq!(max_active.load(Ordering::SeqCst), 4);
            assert_eq!(mmc.global_limiter.available_permits(), 8);
            assert_eq!(mmc.machine_limiter("machine-a").available_permits(), 4);
        });
    }
}
