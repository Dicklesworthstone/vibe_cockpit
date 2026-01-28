//! Built-in collectors for Vibe Cockpit
//!
//! This module contains all the collector implementations for various
//! upstream tools and data sources.

use async_trait::async_trait;
use chrono::Utc;
use std::time::Instant;

use crate::{CollectContext, CollectError, CollectResult, Collector, Cursor, RowBatch};

use serde::Deserialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::Warning;

// Re-export all collectors at the module level
pub mod sysmoni;
pub use sysmoni::SysmoniCollector;

pub mod mcp_mail;
pub use mcp_mail::AgentMailCollector;

pub mod caut;
pub use caut::CautCollector;

pub mod cass;
pub use cass::CassCollector;

pub mod caam;
pub use caam::CaamCollector;

pub mod rch;
pub use rch::RchCollector;

pub mod rano;
pub use rano::RanoCollector;

pub mod dcg;
pub use dcg::DcgCollector;

pub mod beads;
pub use beads::BeadsCollector;

// Future collectors will be added here as submodules:
// pub mod pt;
// pub mod bv_br;
// pub mod afsc;
// pub mod cloud_benchmarker;
// pub mod ntm;

/// Dummy collector for testing the collector infrastructure
///
/// This collector returns synthetic data to verify that the
/// collector trait, registry, and execution pipeline work correctly.
pub struct DummyCollector;

#[async_trait]
impl Collector for DummyCollector {
    fn name(&self) -> &'static str {
        "dummy"
    }

    fn schema_version(&self) -> u32 {
        1
    }

    fn required_tool(&self) -> Option<&'static str> {
        None
    }

    fn supports_incremental(&self) -> bool {
        false
    }

    async fn collect(&self, ctx: &CollectContext) -> Result<CollectResult, CollectError> {
        let start = Instant::now();

        // Generate a test row
        let row = serde_json::json!({
            "machine_id": ctx.machine_id,
            "collected_at": ctx.collected_at.to_rfc3339(),
            "is_local": ctx.is_local,
            "test_value": 42,
            "message": "Hello from DummyCollector",
        });

        let batch = RowBatch {
            table: "dummy_snapshots".to_string(),
            rows: vec![row],
        };

        Ok(CollectResult::with_rows(vec![batch])
            .with_cursor(Cursor::now())
            .with_duration(start.elapsed()))
    }
}

/// Collector that generates incrementing values for testing incremental collection
pub struct IncrementalDummyCollector;

#[async_trait]
impl Collector for IncrementalDummyCollector {
    fn name(&self) -> &'static str {
        "incremental_dummy"
    }

    fn schema_version(&self) -> u32 {
        1
    }

    fn supports_incremental(&self) -> bool {
        true
    }

    async fn collect(&self, ctx: &CollectContext) -> Result<CollectResult, CollectError> {
        let start = Instant::now();

        // Get the last primary key or start from 0
        let last_pk = ctx.primary_key_cursor().unwrap_or(0);
        let next_pk = last_pk + 1;

        let row = serde_json::json!({
            "id": next_pk,
            "machine_id": ctx.machine_id,
            "collected_at": ctx.collected_at.to_rfc3339(),
            "previous_pk": last_pk,
        });

        let batch = RowBatch {
            table: "incremental_dummy".to_string(),
            rows: vec![row],
        };

        Ok(CollectResult::with_rows(vec![batch])
            .with_cursor(Cursor::primary_key(next_pk))
            .with_duration(start.elapsed()))
    }
}

/// Collector that simulates failures for testing error handling
pub struct FailingDummyCollector {
    /// Whether to fail
    pub should_fail: bool,
    /// Error message when failing
    pub error_message: String,
}

impl FailingDummyCollector {
    /// Create a collector that always fails
    pub fn always_fails(message: impl Into<String>) -> Self {
        Self {
            should_fail: true,
            error_message: message.into(),
        }
    }

    /// Create a collector that always succeeds
    pub fn always_succeeds() -> Self {
        Self {
            should_fail: false,
            error_message: String::new(),
        }
    }
}

#[async_trait]
impl Collector for FailingDummyCollector {
    fn name(&self) -> &'static str {
        "failing_dummy"
    }

    fn schema_version(&self) -> u32 {
        1
    }

    async fn collect(&self, _ctx: &CollectContext) -> Result<CollectResult, CollectError> {
        if self.should_fail {
            Err(CollectError::Other(self.error_message.clone()))
        } else {
            Ok(CollectResult::empty())
        }
    }
}

/// Collector that requires a specific tool (for testing tool availability)
pub struct ToolRequiringDummyCollector {
    /// The required tool name
    pub required: &'static str,
}

impl ToolRequiringDummyCollector {
    /// Create a collector requiring a specific tool
    pub fn new(tool: &'static str) -> Self {
        Self { required: tool }
    }
}

#[async_trait]
impl Collector for ToolRequiringDummyCollector {
    fn name(&self) -> &'static str {
        "tool_requiring_dummy"
    }

    fn schema_version(&self) -> u32 {
        1
    }

    fn required_tool(&self) -> Option<&'static str> {
        Some(self.required)
    }

    async fn collect(&self, ctx: &CollectContext) -> Result<CollectResult, CollectError> {
        // Check tool availability first
        if !self.check_availability(ctx).await {
            return Err(CollectError::ToolNotFound(self.required.to_string()));
        }

        let row = serde_json::json!({
            "machine_id": ctx.machine_id,
            "tool": self.required,
            "available": true,
        });

        Ok(CollectResult::with_rows(vec![RowBatch {
            table: "tool_checks".to_string(),
            rows: vec![row],
        }]))
    }
}

/// Collector that generates multiple rows for testing batch handling
pub struct BatchDummyCollector {
    /// Number of rows to generate
    pub row_count: usize,
}

impl BatchDummyCollector {
    pub fn new(count: usize) -> Self {
        Self { row_count: count }
    }
}

#[async_trait]
impl Collector for BatchDummyCollector {
    fn name(&self) -> &'static str {
        "batch_dummy"
    }

    fn schema_version(&self) -> u32 {
        1
    }

    async fn collect(&self, ctx: &CollectContext) -> Result<CollectResult, CollectError> {
        let start = Instant::now();

        // Respect max_rows limit
        let count = self.row_count.min(ctx.max_rows);

        let rows: Vec<serde_json::Value> = (0..count)
            .map(|i| {
                serde_json::json!({
                    "id": i,
                    "machine_id": &ctx.machine_id,
                    "collected_at": Utc::now().to_rfc3339(),
                })
            })
            .collect();

        Ok(CollectResult::with_rows(vec![RowBatch {
            table: "batch_dummy".to_string(),
            rows,
        }])
        .with_duration(start.elapsed()))
    }
}

// =============================================================================
// RU (Repo Updater) Collector
// =============================================================================

/// Output from `ru list --json`
#[derive(Debug, Deserialize)]
pub struct RuListOutput {
    pub repos: Vec<RuRepo>,
}

/// A single repo from ru list
#[derive(Debug, Deserialize)]
pub struct RuRepo {
    pub path: String,
    pub url: Option<String>,
    pub name: Option<String>,
}

/// Output from `ru status --no-fetch --json`
#[derive(Debug, Deserialize)]
pub struct RuStatusOutput {
    pub repos: Vec<RuRepoStatus>,
}

/// Status of a single repo
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct RuRepoStatus {
    pub path: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub dirty: bool,
    #[serde(default)]
    pub ahead: i32,
    #[serde(default)]
    pub behind: i32,
    #[serde(default)]
    pub modified_files: Vec<String>,
    #[serde(default)]
    pub untracked_files: Vec<String>,
}

/// Collector for repository status via the `ru` tool
///
/// This collector uses the CLI Snapshot pattern with two commands:
/// - `ru list --json` to enumerate tracked repositories
/// - `ru status --no-fetch --json` to get status without network calls
pub struct RuCollector;

impl RuCollector {
    /// Generate a stable hash for a repo identifier
    fn hash_repo(identifier: &str) -> String {
        let mut hasher = DefaultHasher::new();
        identifier.hash(&mut hasher);
        format!("repo_{:016x}", hasher.finish())
    }
}

#[async_trait]
impl Collector for RuCollector {
    fn name(&self) -> &'static str {
        "ru"
    }

    fn schema_version(&self) -> u32 {
        1
    }

    fn required_tool(&self) -> Option<&'static str> {
        Some("ru")
    }

    fn supports_incremental(&self) -> bool {
        false // Stateless - each poll is a fresh snapshot
    }

    async fn collect(&self, ctx: &CollectContext) -> Result<CollectResult, CollectError> {
        let start = Instant::now();
        let mut rows = vec![];
        let mut warnings = vec![];

        // First, get repo list (for populating the repos table)
        let list_result = ctx
            .executor
            .run_timeout("ru list --json", ctx.timeout)
            .await;

        if let Ok(output) = list_result {
            match serde_json::from_str::<RuListOutput>(&output) {
                Ok(list) => {
                    let repo_rows: Vec<serde_json::Value> = list
                        .repos
                        .iter()
                        .map(|repo| {
                            let identifier = repo.url.as_ref().unwrap_or(&repo.path);
                            serde_json::json!({
                                "machine_id": &ctx.machine_id,
                                "repo_id": Self::hash_repo(identifier),
                                "path": &repo.path,
                                "url": &repo.url,
                                "name": &repo.name,
                            })
                        })
                        .collect();

                    if !repo_rows.is_empty() {
                        rows.push(RowBatch {
                            table: "repos".to_string(),
                            rows: repo_rows,
                        });
                    }
                }
                Err(e) => {
                    warnings.push(Warning::warn(format!("Failed to parse ru list: {e}")));
                }
            }
        } else if let Err(e) = list_result {
            warnings.push(Warning::warn(format!("Failed to run ru list: {e}")));
        }

        // Then, get status (no-fetch to avoid network)
        let status_result = ctx
            .executor
            .run_timeout("ru status --no-fetch --json", ctx.timeout)
            .await;

        match status_result {
            Ok(output) => match serde_json::from_str::<RuStatusOutput>(&output) {
                Ok(status) => {
                    let status_rows: Vec<serde_json::Value> = status
                        .repos
                        .iter()
                        .map(|r| {
                            let identifier = r.url.as_ref().unwrap_or(&r.path);
                            serde_json::json!({
                                "machine_id": &ctx.machine_id,
                                "collected_at": ctx.collected_at.to_rfc3339(),
                                "repo_id": Self::hash_repo(identifier),
                                "branch": &r.branch,
                                "dirty": r.dirty,
                                "ahead": r.ahead,
                                "behind": r.behind,
                                "modified_count": r.modified_files.len(),
                                "untracked_count": r.untracked_files.len(),
                                "raw_json": serde_json::to_string(&r).unwrap_or_default(),
                            })
                        })
                        .collect();

                    if !status_rows.is_empty() {
                        rows.push(RowBatch {
                            table: "repo_status_snapshots".to_string(),
                            rows: status_rows,
                        });
                    }
                }
                Err(e) => {
                    warnings.push(
                        Warning::error(format!("Failed to parse ru status: {e}"))
                            .with_context(output),
                    );
                }
            },
            Err(e) => {
                warnings.push(Warning::error(format!("Failed to run ru status: {e}")));
            }
        }

        // Determine success: at least one command must have worked
        let success = rows.iter().any(|batch| !batch.rows.is_empty());

        Ok(CollectResult {
            rows,
            new_cursor: None, // Stateless
            raw_artifacts: vec![],
            warnings,
            duration: start.elapsed(),
            success,
            error: if success {
                None
            } else {
                Some("Failed to collect repository data".to_string())
            },
        })
    }
}

// =============================================================================
// Fallback System Probe Collector
// =============================================================================

/// Fallback system probe collector - ALWAYS ENABLED baseline health check
///
/// This collector works on ANY Linux/macOS system using only standard shell
/// commands. It provides baseline health data when other collectors are not
/// available.
///
/// Key design principle: NEVER FAIL. If individual commands fail, store
/// partial data with warnings.
pub struct FallbackProbeCollector;

/// Parsed uptime data
#[derive(Default)]
struct UptimeData {
    uptime_seconds: Option<i64>,
    load1: Option<f64>,
    load5: Option<f64>,
    load15: Option<f64>,
}

/// Parsed memory data
#[derive(Default)]
struct MemoryData {
    mem_total_bytes: Option<i64>,
    mem_available_bytes: Option<i64>,
    mem_used_bytes: Option<i64>,
    swap_total_bytes: Option<i64>,
    swap_used_bytes: Option<i64>,
}

/// Single disk mount point
#[derive(serde::Serialize)]
struct DiskUsage {
    mount: String,
    total: i64,
    used: i64,
    avail: i64,
    pct: f64,
}

#[async_trait]
impl Collector for FallbackProbeCollector {
    fn name(&self) -> &'static str {
        "fallback_probe"
    }

    fn schema_version(&self) -> u32 {
        1
    }

    fn required_tool(&self) -> Option<&'static str> {
        None // No external tools required - uses only basic shell commands
    }

    fn supports_incremental(&self) -> bool {
        false // Each collection is a point-in-time snapshot
    }

    async fn collect(&self, ctx: &CollectContext) -> Result<CollectResult, CollectError> {
        let start = Instant::now();
        let mut warnings = Vec::new();
        let mut raw_outputs = Vec::new();

        // Detect platform
        let platform = Self::detect_platform(ctx).await;

        // Collect uptime and load averages
        let uptime_data =
            Self::collect_uptime(ctx, &platform, &mut warnings, &mut raw_outputs).await;

        // Collect memory stats
        let memory_data =
            Self::collect_memory(ctx, &platform, &mut warnings, &mut raw_outputs).await;

        // Collect disk usage
        let disk_usage = Self::collect_disk_usage(ctx, &mut warnings, &mut raw_outputs).await;

        // Build the row
        let row = serde_json::json!({
            "machine_id": ctx.machine_id,
            "collected_at": ctx.collected_at.to_rfc3339(),
            "uptime_seconds": uptime_data.uptime_seconds,
            "load1": uptime_data.load1,
            "load5": uptime_data.load5,
            "load15": uptime_data.load15,
            "mem_total_bytes": memory_data.mem_total_bytes,
            "mem_available_bytes": memory_data.mem_available_bytes,
            "mem_used_bytes": memory_data.mem_used_bytes,
            "swap_total_bytes": memory_data.swap_total_bytes,
            "swap_used_bytes": memory_data.swap_used_bytes,
            "disk_usage_json": serde_json::to_string(&disk_usage).ok(),
            "raw_output": raw_outputs.join("\n---\n"),
        });

        let batch = RowBatch {
            table: "sys_fallback_samples".to_string(),
            rows: vec![row],
        };

        let mut result = CollectResult::with_rows(vec![batch])
            .with_cursor(Cursor::now())
            .with_duration(start.elapsed());

        // Add warnings to result
        for warning in warnings {
            result = result.with_warning(Warning::warn(warning));
        }

        Ok(result)
    }
}

impl FallbackProbeCollector {
    /// Detect platform (linux or macos)
    async fn detect_platform(ctx: &CollectContext) -> String {
        let result = ctx.executor.run("uname -s", ctx.timeout).await;
        match result {
            Ok(output) if output.exit_code == 0 => {
                let os = output.stdout.trim().to_lowercase();
                if os.contains("darwin") {
                    "macos".to_string()
                } else {
                    "linux".to_string()
                }
            }
            _ => "linux".to_string(), // Default to Linux
        }
    }

    /// Collect uptime and load averages
    async fn collect_uptime(
        ctx: &CollectContext,
        platform: &str,
        warnings: &mut Vec<String>,
        raw_outputs: &mut Vec<String>,
    ) -> UptimeData {
        let mut data = UptimeData::default();

        // Try /proc/loadavg on Linux first (most reliable)
        if platform == "linux" {
            if let Ok(output) = ctx.executor.run("cat /proc/loadavg", ctx.timeout).await {
                if output.exit_code == 0 {
                    raw_outputs.push(format!("/proc/loadavg:\n{}", output.stdout));
                    // Format: "0.25 0.18 0.12 1/234 5678"
                    let parts: Vec<&str> = output.stdout.trim().split_whitespace().collect();
                    if parts.len() >= 3 {
                        data.load1 = parts[0].parse().ok();
                        data.load5 = parts[1].parse().ok();
                        data.load15 = parts[2].parse().ok();
                    }
                }
            }

            // Try /proc/uptime for uptime seconds
            if let Ok(output) = ctx.executor.run("cat /proc/uptime", ctx.timeout).await {
                if output.exit_code == 0 {
                    raw_outputs.push(format!("/proc/uptime:\n{}", output.stdout));
                    // Format: "12345.67 23456.78" (uptime idle_time)
                    if let Some(uptime_str) = output.stdout.trim().split_whitespace().next() {
                        if let Ok(uptime_float) = uptime_str.parse::<f64>() {
                            data.uptime_seconds = Some(uptime_float as i64);
                        }
                    }
                }
            }
        }

        // Fallback to uptime command (works on both platforms)
        if data.load1.is_none() {
            if let Ok(output) = ctx.executor.run("uptime", ctx.timeout).await {
                if output.exit_code == 0 {
                    raw_outputs.push(format!("uptime:\n{}", output.stdout));
                    Self::parse_uptime_output(&output.stdout, &mut data);
                } else {
                    warnings.push("uptime command failed".to_string());
                }
            } else {
                warnings.push("Could not run uptime command".to_string());
            }
        }

        data
    }

    /// Parse the uptime command output
    fn parse_uptime_output(output: &str, data: &mut UptimeData) {
        let line = output.trim();

        // Find "load average" or "load averages"
        if let Some(load_pos) = line.find("load average") {
            let after_label = &line[load_pos..];
            if let Some(colon_pos) = after_label.find(':') {
                let nums = &after_label[colon_pos + 1..];
                let parts: Vec<f64> = nums
                    .split(|c: char| c == ',' || c.is_whitespace())
                    .filter_map(|s| s.trim().parse().ok())
                    .collect();

                if !parts.is_empty() {
                    data.load1 = Some(parts[0]);
                }
                if parts.len() >= 2 {
                    data.load5 = Some(parts[1]);
                }
                if parts.len() >= 3 {
                    data.load15 = Some(parts[2]);
                }
            }
        }

        // Parse uptime duration: "up X days, Y:Z" or "up X:Y"
        if let Some(up_pos) = line.find(" up ") {
            let after_up = &line[up_pos + 4..];
            if let Some(end_pos) = after_up.find(" user").or_else(|| after_up.find(',')) {
                let duration_str = after_up[..end_pos].trim();
                if let Some(seconds) = Self::parse_uptime_duration(duration_str) {
                    data.uptime_seconds = Some(seconds);
                }
            }
        }
    }

    /// Parse uptime duration string to seconds
    fn parse_uptime_duration(duration: &str) -> Option<i64> {
        let mut total_seconds: i64 = 0;
        let parts: Vec<&str> = duration.split(',').map(|s| s.trim()).collect();

        for part in parts {
            if part.contains("day") {
                if let Some(days_str) = part.split_whitespace().next() {
                    if let Ok(days) = days_str.parse::<i64>() {
                        total_seconds += days * 86400;
                    }
                }
            } else if part.contains(':') {
                let time_parts: Vec<&str> = part.split(':').collect();
                if time_parts.len() == 2 {
                    if let (Ok(hours), Ok(mins)) = (
                        time_parts[0].trim().parse::<i64>(),
                        time_parts[1].trim().parse::<i64>(),
                    ) {
                        total_seconds += hours * 3600 + mins * 60;
                    }
                } else if time_parts.len() == 3 {
                    if let (Ok(hours), Ok(mins), Ok(secs)) = (
                        time_parts[0].trim().parse::<i64>(),
                        time_parts[1].trim().parse::<i64>(),
                        time_parts[2].trim().parse::<i64>(),
                    ) {
                        total_seconds += hours * 3600 + mins * 60 + secs;
                    }
                }
            } else if part.contains("min") {
                if let Some(mins_str) = part.split_whitespace().next() {
                    if let Ok(mins) = mins_str.parse::<i64>() {
                        total_seconds += mins * 60;
                    }
                }
            }
        }

        if total_seconds > 0 {
            Some(total_seconds)
        } else {
            None
        }
    }

    /// Collect memory statistics
    async fn collect_memory(
        ctx: &CollectContext,
        platform: &str,
        warnings: &mut Vec<String>,
        raw_outputs: &mut Vec<String>,
    ) -> MemoryData {
        let mut data = MemoryData::default();

        if platform == "linux" {
            if let Ok(output) = ctx.executor.run("cat /proc/meminfo", ctx.timeout).await {
                if output.exit_code == 0 {
                    raw_outputs.push(format!("/proc/meminfo:\n{}", output.stdout));
                    Self::parse_proc_meminfo(&output.stdout, &mut data);
                }
            }

            if data.mem_total_bytes.is_none() {
                if let Ok(output) = ctx.executor.run("free -b", ctx.timeout).await {
                    if output.exit_code == 0 {
                        raw_outputs.push(format!("free -b:\n{}", output.stdout));
                        Self::parse_free_output(&output.stdout, &mut data);
                    } else {
                        warnings.push("free command failed".to_string());
                    }
                }
            }
        } else {
            // macOS
            if let Ok(output) = ctx.executor.run("sysctl hw.memsize", ctx.timeout).await {
                if output.exit_code == 0 {
                    raw_outputs.push(format!("sysctl hw.memsize:\n{}", output.stdout));
                    if let Some(val) = output.stdout.split(':').nth(1) {
                        data.mem_total_bytes = val.trim().parse().ok();
                    }
                }
            }

            if let Ok(output) = ctx.executor.run("vm_stat", ctx.timeout).await {
                if output.exit_code == 0 {
                    raw_outputs.push(format!("vm_stat:\n{}", output.stdout));
                    Self::parse_vm_stat(&output.stdout, &mut data);
                } else {
                    warnings.push("vm_stat command failed".to_string());
                }
            }
        }

        if let (Some(total), Some(avail)) = (data.mem_total_bytes, data.mem_available_bytes) {
            data.mem_used_bytes = Some(total - avail);
        }

        data
    }

    /// Parse /proc/meminfo output
    fn parse_proc_meminfo(output: &str, data: &mut MemoryData) {
        for line in output.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let key = parts[0].trim_end_matches(':');
                let value_kb: Option<i64> = parts[1].parse().ok();

                match key {
                    "MemTotal" => {
                        data.mem_total_bytes = value_kb.map(|v| v * 1024);
                    }
                    "MemAvailable" => {
                        data.mem_available_bytes = value_kb.map(|v| v * 1024);
                    }
                    "SwapTotal" => {
                        data.swap_total_bytes = value_kb.map(|v| v * 1024);
                    }
                    "SwapFree" => {
                        if let (Some(total), Some(free_kb)) = (data.swap_total_bytes, value_kb) {
                            data.swap_used_bytes = Some(total - free_kb * 1024);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    /// Parse free -b output
    fn parse_free_output(output: &str, data: &mut MemoryData) {
        for line in output.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.first().is_some_and(|&s| s.starts_with("Mem")) && parts.len() >= 4 {
                data.mem_total_bytes = parts[1].parse().ok();
                data.mem_used_bytes = parts[2].parse().ok();
                if parts.len() >= 7 {
                    data.mem_available_bytes = parts[6].parse().ok();
                }
            } else if parts.first().is_some_and(|&s| s.starts_with("Swap")) && parts.len() >= 3 {
                data.swap_total_bytes = parts[1].parse().ok();
                data.swap_used_bytes = parts[2].parse().ok();
            }
        }
    }

    /// Parse vm_stat output (macOS)
    fn parse_vm_stat(output: &str, data: &mut MemoryData) {
        let page_size: i64 = 4096;
        let mut pages_free: i64 = 0;
        let mut pages_inactive: i64 = 0;
        let mut pages_speculative: i64 = 0;
        let mut pages_purgeable: i64 = 0;

        for line in output.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() == 2 {
                let key = parts[0].trim().to_lowercase();
                let value: Option<i64> = parts[1].trim().trim_end_matches('.').parse().ok();

                if let Some(v) = value {
                    if key.contains("pages free") {
                        pages_free = v;
                    } else if key.contains("pages inactive") {
                        pages_inactive = v;
                    } else if key.contains("pages speculative") {
                        pages_speculative = v;
                    } else if key.contains("pages purgeable") {
                        pages_purgeable = v;
                    }
                }
            }
        }

        let available_pages = pages_free + pages_inactive + pages_speculative + pages_purgeable;
        data.mem_available_bytes = Some(available_pages * page_size);
    }

    /// Collect disk usage
    async fn collect_disk_usage(
        ctx: &CollectContext,
        warnings: &mut Vec<String>,
        raw_outputs: &mut Vec<String>,
    ) -> Vec<DiskUsage> {
        let mut disks = Vec::new();

        if let Ok(output) = ctx.executor.run("df -P", ctx.timeout).await {
            if output.exit_code == 0 {
                raw_outputs.push(format!("df -P:\n{}", output.stdout));

                for line in output.stdout.lines().skip(1) {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 6 {
                        let filesystem = parts[0];
                        if filesystem.starts_with("tmpfs")
                            || filesystem.starts_with("devtmpfs")
                            || filesystem == "none"
                            || filesystem.starts_with("overlay")
                        {
                            continue;
                        }

                        let total_kb: i64 = parts[1].parse().unwrap_or(0);
                        let used_kb: i64 = parts[2].parse().unwrap_or(0);
                        let avail_kb: i64 = parts[3].parse().unwrap_or(0);
                        let mount = parts[5].to_string();

                        if total_kb == 0 {
                            continue;
                        }

                        let use_percent = (used_kb as f64 / total_kb as f64) * 100.0;

                        disks.push(DiskUsage {
                            mount,
                            total: total_kb * 1024,
                            used: used_kb * 1024,
                            avail: avail_kb * 1024,
                            pct: use_percent,
                        });
                    }
                }
            } else {
                warnings.push("df command failed".to_string());
            }
        } else {
            warnings.push("Could not run df command".to_string());
        }

        disks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_dummy_collector() {
        let collector = DummyCollector;
        let ctx = CollectContext::local("test", Duration::from_secs(30));

        let result = collector.collect(&ctx).await.unwrap();
        assert!(result.success);
        assert_eq!(result.total_rows(), 1);
        assert!(result.new_cursor.is_some());
    }

    #[tokio::test]
    async fn test_incremental_dummy_collector() {
        let collector = IncrementalDummyCollector;
        let ctx = CollectContext::local("test", Duration::from_secs(30));

        // First collection
        let result1 = collector.collect(&ctx).await.unwrap();
        assert_eq!(result1.new_cursor, Some(Cursor::primary_key(1)));

        // Second collection with cursor
        let ctx2 = ctx.clone().with_cursor(Cursor::primary_key(1));
        let result2 = collector.collect(&ctx2).await.unwrap();
        assert_eq!(result2.new_cursor, Some(Cursor::primary_key(2)));
    }

    #[tokio::test]
    async fn test_failing_dummy_collector() {
        let collector = FailingDummyCollector::always_fails("test error");
        let ctx = CollectContext::local("test", Duration::from_secs(30));

        let result = collector.collect(&ctx).await;
        assert!(result.is_err());
        assert!(matches!(result, Err(CollectError::Other(_))));
    }

    #[tokio::test]
    async fn test_batch_dummy_collector() {
        let collector = BatchDummyCollector::new(100);
        let ctx = CollectContext::local("test", Duration::from_secs(30));

        let result = collector.collect(&ctx).await.unwrap();
        assert_eq!(result.total_rows(), 100);
    }

    #[tokio::test]
    async fn test_batch_dummy_respects_max_rows() {
        let collector = BatchDummyCollector::new(1000);
        let ctx = CollectContext::local("test", Duration::from_secs(30)).with_max_rows(50);

        let result = collector.collect(&ctx).await.unwrap();
        assert_eq!(result.total_rows(), 50);
    }

    #[tokio::test]
    async fn test_tool_requiring_collector_with_available_tool() {
        let collector = ToolRequiringDummyCollector::new("sh");
        let ctx = CollectContext::local("test", Duration::from_secs(30));

        // sh should be available on all Unix systems
        let result = collector.collect(&ctx).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_tool_requiring_collector_with_missing_tool() {
        let collector = ToolRequiringDummyCollector::new("nonexistent_tool_xyz_12345");
        let ctx = CollectContext::local("test", Duration::from_secs(30));

        let result = collector.collect(&ctx).await;
        assert!(result.is_err());
        assert!(matches!(result, Err(CollectError::ToolNotFound(_))));
    }

    // =============================================================================
    // RU Collector Tests
    // =============================================================================

    #[test]
    fn test_ru_collector_name() {
        let collector = RuCollector;
        assert_eq!(collector.name(), "ru");
        assert_eq!(collector.required_tool(), Some("ru"));
        assert!(!collector.supports_incremental());
    }

    #[test]
    fn test_ru_hash_repo_stability() {
        // Same input should produce same hash
        let hash1 = RuCollector::hash_repo("https://github.com/user/repo.git");
        let hash2 = RuCollector::hash_repo("https://github.com/user/repo.git");
        assert_eq!(hash1, hash2);

        // Different input should produce different hash
        let hash3 = RuCollector::hash_repo("https://github.com/user/other.git");
        assert_ne!(hash1, hash3);

        // Hash should start with "repo_"
        assert!(hash1.starts_with("repo_"));
    }

    #[test]
    fn test_ru_list_output_parsing() {
        let json = r#"{
            "repos": [
                {
                    "path": "/data/projects/vibe_cockpit",
                    "url": "git@github.com:Dicklesworthstone/vibe_cockpit.git",
                    "name": "vibe_cockpit"
                },
                {
                    "path": "/data/projects/another",
                    "url": null,
                    "name": null
                }
            ]
        }"#;

        let output: RuListOutput = serde_json::from_str(json).unwrap();
        assert_eq!(output.repos.len(), 2);
        assert_eq!(output.repos[0].path, "/data/projects/vibe_cockpit");
        assert_eq!(
            output.repos[0].url,
            Some("git@github.com:Dicklesworthstone/vibe_cockpit.git".to_string())
        );
        assert_eq!(output.repos[1].url, None);
    }

    #[test]
    fn test_ru_status_output_parsing() {
        let json = r#"{
            "repos": [
                {
                    "path": "/data/projects/vibe_cockpit",
                    "url": "git@github.com:Dicklesworthstone/vibe_cockpit.git",
                    "branch": "main",
                    "dirty": true,
                    "ahead": 2,
                    "behind": 0,
                    "modified_files": ["src/main.rs"],
                    "untracked_files": ["new_file.txt", "temp/"]
                }
            ]
        }"#;

        let output: RuStatusOutput = serde_json::from_str(json).unwrap();
        assert_eq!(output.repos.len(), 1);

        let repo = &output.repos[0];
        assert_eq!(repo.branch, Some("main".to_string()));
        assert!(repo.dirty);
        assert_eq!(repo.ahead, 2);
        assert_eq!(repo.behind, 0);
        assert_eq!(repo.modified_files.len(), 1);
        assert_eq!(repo.untracked_files.len(), 2);
    }

    #[test]
    fn test_ru_status_output_with_defaults() {
        // Minimal JSON with missing optional fields
        let json = r#"{
            "repos": [
                {
                    "path": "/data/projects/test"
                }
            ]
        }"#;

        let output: RuStatusOutput = serde_json::from_str(json).unwrap();
        let repo = &output.repos[0];

        // Defaults should be applied
        assert_eq!(repo.branch, None);
        assert!(!repo.dirty);
        assert_eq!(repo.ahead, 0);
        assert_eq!(repo.behind, 0);
        assert!(repo.modified_files.is_empty());
        assert!(repo.untracked_files.is_empty());
    }

    #[tokio::test]
    async fn test_ru_collector_without_tool() {
        // Test that collector gracefully handles missing ru tool
        let collector = RuCollector;
        let ctx = CollectContext::local("test", Duration::from_secs(5));

        // This should not panic, but will likely fail due to missing tool
        let result = collector.collect(&ctx).await.unwrap();

        // Without ru installed, we expect warnings but no crash
        // The result may have empty rows and warnings
        assert!(result.warnings.len() > 0 || result.rows.is_empty());
    }

    // =============================================================================
    // Fallback Probe Collector Tests
    // =============================================================================

    #[test]
    fn test_fallback_probe_collector_name() {
        let collector = FallbackProbeCollector;
        assert_eq!(collector.name(), "fallback_probe");
        assert_eq!(collector.required_tool(), None); // No external tools required
        assert!(!collector.supports_incremental());
    }

    #[tokio::test]
    async fn test_fallback_probe_collector_local() {
        let collector = FallbackProbeCollector;
        let ctx = CollectContext::local("test-machine", Duration::from_secs(30));

        // This should always succeed - that's the whole point of the fallback collector
        let result = collector.collect(&ctx).await.unwrap();
        assert!(result.success);
        assert_eq!(result.total_rows(), 1);
        assert!(result.new_cursor.is_some());

        // Verify the row has expected fields
        let row = &result.rows[0].rows[0];
        assert_eq!(row["machine_id"], "test-machine");
        assert!(row["collected_at"].is_string());
        // On Linux, we should have load averages
        if cfg!(target_os = "linux") {
            assert!(row["load1"].is_number() || row["load1"].is_null());
        }
    }

    #[test]
    fn test_parse_uptime_output_linux() {
        let mut data = UptimeData::default();

        // Linux uptime format
        let output = " 14:32:25 up 5 days, 3:45, 2 users, load average: 0.25, 0.18, 0.12";
        FallbackProbeCollector::parse_uptime_output(output, &mut data);

        assert_eq!(data.load1, Some(0.25));
        assert_eq!(data.load5, Some(0.18));
        assert_eq!(data.load15, Some(0.12));
    }

    #[test]
    fn test_parse_uptime_output_macos() {
        let mut data = UptimeData::default();

        // macOS uptime format (note: "load averages" plural)
        let output = "14:32  up 5 days,  3:45, 2 users, load averages: 1.23 0.98 0.67";
        FallbackProbeCollector::parse_uptime_output(output, &mut data);

        // Should still parse correctly
        assert!(data.load1.is_some());
    }

    #[test]
    fn test_parse_uptime_duration() {
        // "5 days"
        assert_eq!(
            FallbackProbeCollector::parse_uptime_duration("5 days"),
            Some(5 * 86400)
        );

        // "3:45" (hours:minutes)
        assert_eq!(
            FallbackProbeCollector::parse_uptime_duration("3:45"),
            Some(3 * 3600 + 45 * 60)
        );

        // "5 days, 3:45"
        assert_eq!(
            FallbackProbeCollector::parse_uptime_duration("5 days, 3:45"),
            Some(5 * 86400 + 3 * 3600 + 45 * 60)
        );

        // "30 min"
        assert_eq!(
            FallbackProbeCollector::parse_uptime_duration("30 min"),
            Some(30 * 60)
        );
    }

    #[test]
    fn test_parse_proc_meminfo() {
        let mut data = MemoryData::default();

        let output = r#"MemTotal:       16384000 kB
MemFree:         1234567 kB
MemAvailable:    8000000 kB
Buffers:          500000 kB
Cached:          4000000 kB
SwapTotal:       4194304 kB
SwapFree:        4000000 kB
"#;

        FallbackProbeCollector::parse_proc_meminfo(output, &mut data);

        assert_eq!(data.mem_total_bytes, Some(16384000 * 1024));
        assert_eq!(data.mem_available_bytes, Some(8000000 * 1024));
        assert_eq!(data.swap_total_bytes, Some(4194304 * 1024));
        // swap_used = total - free = 4194304 - 4000000 = 194304 kB
        assert_eq!(data.swap_used_bytes, Some(4194304 * 1024 - 4000000 * 1024));
    }

    #[test]
    fn test_parse_free_output() {
        let mut data = MemoryData::default();

        let output = r#"              total        used        free      shared  buff/cache   available
Mem:    16777216000  8000000000  2000000000   500000000  6000000000  8000000000
Swap:    4294967296  1000000000  3294967296
"#;

        FallbackProbeCollector::parse_free_output(output, &mut data);

        assert_eq!(data.mem_total_bytes, Some(16777216000));
        assert_eq!(data.mem_used_bytes, Some(8000000000));
        assert_eq!(data.mem_available_bytes, Some(8000000000));
        assert_eq!(data.swap_total_bytes, Some(4294967296));
        assert_eq!(data.swap_used_bytes, Some(1000000000));
    }
}
