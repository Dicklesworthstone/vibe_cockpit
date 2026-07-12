//! Robot mode output for agent consumption
//!
//! This module provides:
//! - Standard envelope format for all robot output
//! - Health status data structures
//! - Triage recommendations
//! - Machine, account, repo and forecast status
//!
//! Every payload here is derived from the store. Values that genuinely cannot be
//! known (a machine that has never been collected from, an account with no usage
//! snapshot) are reported as `null` rather than filled with a plausible-looking
//! default — a `null` is information, an invented number is a lie.
//!
//! # `DuckDB` timestamp handling
//!
//! Timestamp columns are stored as `TEXT`. Comparing them against another
//! timestamp (or `current_timestamp`) raises a Binder Error, so every comparison
//! and `MAX()` used for "latest row per key" wraps the column in
//! `CAST(col AS TIMESTAMP)`, and every timestamp we read back out is projected as
//! `CAST(col AS TEXT)`.

use crate::CliError;
use chrono::{DateTime, NaiveDateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use vc_oracle::rate_limit::{RateLimitForecaster, UsageSample};
use vc_query::QueryBuilder;
use vc_store::VcStore;

/// Standard envelope for all robot mode output
///
/// Every robot command returns data wrapped in this envelope,
/// providing consistent metadata for agent consumption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RobotEnvelope<T: Serialize> {
    /// Schema version identifier (e.g., "vc.robot.health.v1")
    pub schema_version: String,

    /// When this output was generated
    pub generated_at: DateTime<Utc>,

    /// The actual data payload
    pub data: T,

    /// Data staleness by source (seconds since last collection)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub staleness: HashMap<String, u64>,

    /// Warnings about data quality or collection issues
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

impl<T: Serialize> RobotEnvelope<T> {
    /// Create a new envelope with the given schema and data
    pub fn new(schema_version: impl Into<String>, data: T) -> Self {
        Self {
            schema_version: schema_version.into(),
            generated_at: Utc::now(),
            data,
            staleness: HashMap::new(),
            warnings: Vec::new(),
        }
    }

    /// Add staleness information
    #[must_use]
    pub fn with_staleness(mut self, staleness: HashMap<String, u64>) -> Self {
        self.staleness = staleness;
        self
    }

    /// Add a single staleness entry
    #[must_use]
    pub fn add_staleness(mut self, source: impl Into<String>, seconds: u64) -> Self {
        self.staleness.insert(source.into(), seconds);
        self
    }

    /// Add warnings
    #[must_use]
    pub fn with_warnings(mut self, warnings: Vec<String>) -> Self {
        self.warnings = warnings;
        self
    }

    /// Add a single warning
    #[must_use]
    pub fn add_warning(mut self, warning: impl Into<String>) -> Self {
        self.warnings.push(warning.into());
        self
    }

    /// Serialize to pretty JSON string
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self)
            .unwrap_or_else(|e| format!(r#"{{"error": "serialization failed: {e}"}}"#))
    }

    /// Serialize to compact JSON string
    pub fn to_json(&self) -> String {
        serde_json::to_string(self)
            .unwrap_or_else(|e| format!(r#"{{"error": "serialization failed: {e}"}}"#))
    }
}

// ============================================================================
// Health Data Structures
// ============================================================================

/// Overall fleet health data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthData {
    /// Overall health summary
    pub overall: OverallHealth,

    /// Per-machine health
    pub machines: Vec<MachineHealth>,

    /// Active alert count by severity
    pub alerts_by_severity: AlertCounts,
}

/// Overall health summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverallHealth {
    /// Health score (0.0 to 1.0)
    pub score: f64,

    /// Severity level: "healthy", "warning", "critical"
    pub severity: String,

    /// Total active alerts
    pub active_alerts: u32,

    /// Number of machines monitored
    pub machine_count: u32,

    /// Number of active agents
    pub agent_count: u32,
}

/// Per-machine health
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineHealth {
    /// Machine identifier
    pub id: String,

    /// Display name
    pub name: String,

    /// Health score (0.0 to 1.0), `None` when no health summary has been persisted
    pub score: Option<f64>,

    /// Status: "online", "degraded", "offline", "unknown"
    pub status: String,

    /// Top issue affecting this machine (if any)
    pub top_issue: Option<String>,

    /// Last data collection timestamp, `None` if never collected from
    pub last_seen: Option<DateTime<Utc>>,

    /// Active agent count on this machine
    pub agent_count: u32,

    /// CPU usage percentage (0-100)
    pub cpu_percent: Option<f64>,

    /// Memory usage percentage (0-100)
    pub memory_percent: Option<f64>,
}

/// Alert counts by severity
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AlertCounts {
    pub critical: u32,
    pub warning: u32,
    pub info: u32,
}

// ============================================================================
// Triage Data Structures
// ============================================================================

/// Triage recommendations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriageData {
    /// Prioritized recommendations
    pub recommendations: Vec<Recommendation>,

    /// Suggested commands to run
    pub suggested_commands: Vec<SuggestedCommand>,
}

/// A single triage recommendation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    /// Unique identifier
    pub id: String,

    /// Priority (1 = highest)
    pub priority: u32,

    /// Short title
    pub title: String,

    /// Detailed description
    pub description: String,

    /// Affected scope (machine, collector, etc.)
    pub scope: String,

    /// Suggested action
    pub action: String,
}

/// A suggested command for the agent to run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedCommand {
    /// Command to run
    pub command: String,

    /// Why this is suggested
    pub reason: String,

    /// Confidence level (0.0 to 1.0)
    pub confidence: f64,
}

// ============================================================================
// Status Data Structures
// ============================================================================

/// Comprehensive fleet status data for `vc robot status`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusData {
    /// Fleet-level summary
    pub fleet: FleetSummary,

    /// Per-machine status
    pub machines: Vec<MachineStatus>,

    /// Repository status summary
    pub repos: RepoSummary,

    /// Alert counts by severity
    pub alerts: AlertSummary,
}

/// Fleet-level summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetSummary {
    /// Total number of machines
    pub total_machines: u32,

    /// Number of online machines
    pub online: u32,

    /// Number of offline machines
    pub offline: u32,

    /// Overall fleet health score (0.0 to 1.0)
    pub health_score: f64,
}

/// Per-machine status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineStatus {
    /// Machine identifier
    pub id: String,

    /// Status: "online", "offline", "degraded", "unknown"
    pub status: String,

    /// Last data collection timestamp, `None` if never collected from
    pub last_seen: Option<DateTime<Utc>>,

    /// Health score (0.0 to 1.0), `None` when no health summary has been persisted
    pub health_score: Option<f64>,

    /// Latest resource metrics (`None` when no sample has ever been collected)
    pub metrics: Option<MachineMetrics>,

    /// Top issue affecting this machine
    pub top_issue: Option<String>,
}

/// Machine resource metrics
///
/// Each field is independently optional: a machine may have a load sample from
/// the fallback probe but no CPU total, or system samples but no filesystem
/// snapshot. Missing components are `null`, never zero-filled.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MachineMetrics {
    /// CPU usage percentage (0-100)
    pub cpu_pct: Option<f64>,

    /// Memory usage percentage (0-100)
    pub mem_pct: Option<f64>,

    /// 5-minute load average
    pub load5: Option<f64>,

    /// Available disk percentage (0-100)
    pub disk_free_pct: Option<f64>,
}

impl MachineMetrics {
    /// True when every component is missing — such a struct is reported as `null`.
    fn is_empty(&self) -> bool {
        self.cpu_pct.is_none()
            && self.mem_pct.is_none()
            && self.load5.is_none()
            && self.disk_free_pct.is_none()
    }
}

/// Repository status summary
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepoSummary {
    /// Total tracked repositories
    pub total: u32,

    /// Repositories with uncommitted changes
    pub dirty: u32,

    /// Repositories ahead of remote
    pub ahead: u32,

    /// Repositories behind remote
    pub behind: u32,
}

/// Unresolved alert counts by severity level
///
/// The severity vocabulary is the one `vc_alert` actually writes into
/// `alert_history`: `critical`, `warning`, `info`. The previous
/// critical/high/medium/low shape had no producer and was always zero.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AlertSummary {
    /// Critical alerts
    pub critical: u32,

    /// Warning alerts
    pub warning: u32,

    /// Informational alerts
    pub info: u32,
}

// ============================================================================
// Accounts / Repos / Oracle Data Structures
// ============================================================================

/// Account status payload for `vc robot accounts`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccountsData {
    /// One entry per (machine, provider, account) known to the store
    pub accounts: Vec<AccountInfo>,

    /// Number of accounts returned
    pub total: u32,
}

/// A single provider account, joining the latest usage and profile snapshots
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    /// Machine the snapshot was collected on
    pub machine_id: String,

    /// Provider (e.g. "claude", "openai")
    pub provider: String,

    /// Provider-side account identifier
    pub account_id: String,

    /// Account email (from the caam profile collector), `None` if unknown
    pub email: Option<String>,

    /// Plan type (from the caam profile collector), `None` if unknown
    pub plan_type: Option<String>,

    /// Whether this is the account currently switched in, `None` if unknown
    pub is_current: Option<bool>,

    /// Whether the account is enabled for rotation, `None` if unknown
    pub is_active: Option<bool>,

    /// Usage percentage (0-100), `None` if no usage snapshot exists
    pub usage_pct: Option<f64>,

    /// Tokens consumed in the current window
    pub tokens_used: Option<i64>,

    /// Token limit for the current window
    pub tokens_limit: Option<i64>,

    /// When the current window resets
    pub resets_at: Option<DateTime<Utc>>,

    /// When the usage snapshot was taken
    pub collected_at: Option<DateTime<Utc>>,
}

/// Repository payload for `vc robot repos`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReposData {
    /// One entry per tracked repository
    pub repos: Vec<RepoInfo>,

    /// Roll-up of the same repositories
    pub summary: RepoSummary,
}

/// A single repository and its latest git status snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoInfo {
    /// Machine the repository lives on
    pub machine_id: String,

    /// Repository identifier used by the `ru` collector
    pub repo_id: String,

    /// Repository name, `None` if the inventory row is missing
    pub name: Option<String>,

    /// Absolute path on the machine
    pub path: Option<String>,

    /// Remote URL
    pub url: Option<String>,

    /// Checked-out branch, `None` if no status snapshot exists yet
    pub branch: Option<String>,

    /// Whether the working tree has uncommitted changes
    pub dirty: Option<bool>,

    /// Commits ahead of the tracking branch
    pub ahead: Option<u32>,

    /// Commits behind the tracking branch
    pub behind: Option<u32>,

    /// Modified file count
    pub modified: Option<u32>,

    /// Untracked file count
    pub untracked: Option<u32>,

    /// When the status snapshot was taken
    pub collected_at: Option<DateTime<Utc>>,
}

/// Oracle payload for `vc robot oracle`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OracleData {
    /// Rate-limit forecasts, most urgent first
    pub forecasts: Vec<ForecastInfo>,

    /// Number of usage samples fed to the forecaster
    pub sample_count: u32,

    /// Size of the history window the samples were drawn from
    pub lookback_hours: u32,
}

/// A single rate-limit forecast produced by `vc_oracle`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForecastInfo {
    /// Provider the forecast applies to
    pub provider: String,

    /// Account the forecast applies to
    pub account: String,

    /// Most recent observed usage percentage
    pub current_usage_pct: f64,

    /// Observed burn rate, in usage-percent per minute
    pub velocity_pct_per_min: f64,

    /// Seconds until the account is projected to hit 100%.
    /// `None` when usage is flat or falling, i.e. the limit is never reached at
    /// the observed velocity.
    pub time_to_limit_secs: Option<u64>,

    /// Forecast confidence (0.0 to 1.0), driven by sample count and fit
    pub confidence: f64,

    /// Recommended action, tagged: `continue`, `slow_down`, `prepare_swap`,
    /// `swap_now`, `emergency_pause`
    pub recommended_action: serde_json::Value,

    /// Best moment to swap accounts, if a swap is recommended
    pub optimal_swap_time: Option<DateTime<Utc>>,

    /// Alternative accounts ranked by remaining headroom
    pub alternative_accounts: Vec<AlternativeAccount>,
}

/// An alternative account the Oracle could swap to
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlternativeAccount {
    /// Account identifier
    pub account: String,

    /// Remaining headroom as a usage percentage (100 - `usage_pct`)
    pub headroom_pct: f64,
}

// ============================================================================
// Store Access Helpers
// ============================================================================

/// Lookback window used when feeding the Oracle from usage history.
const ORACLE_LOOKBACK_HOURS: i64 = 24;

/// Usage percentage at or above which an account is worth triaging.
const ACCOUNT_PRESSURE_PCT: f64 = 80.0;

/// Parse a timestamp column. `DuckDB` hands these back as strings in either
/// RFC 3339 (what the collectors write) or `CURRENT_TIMESTAMP`'s
/// `YYYY-MM-DD HH:MM:SS` form (what column defaults write).
fn parse_ts(raw: &str) -> Option<DateTime<Utc>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if let Ok(parsed) = DateTime::parse_from_rfc3339(raw) {
        return Some(parsed.with_timezone(&Utc));
    }
    for format in ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S%.f"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(raw, format) {
            return Some(naive.and_utc());
        }
    }
    None
}

/// Read a timestamp out of a JSON row field.
fn row_ts(row: &serde_json::Value, key: &str) -> Option<DateTime<Utc>> {
    row.get(key)?.as_str().and_then(parse_ts)
}

/// Read a string out of a JSON row field, treating empty strings as absent.
fn row_str(row: &serde_json::Value, key: &str) -> Option<String> {
    let value = row.get(key)?.as_str()?.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

/// Read an f64 out of a JSON row field.
fn row_f64(row: &serde_json::Value, key: &str) -> Option<f64> {
    row.get(key)?.as_f64()
}

/// Read an i64 out of a JSON row field.
fn row_i64(row: &serde_json::Value, key: &str) -> Option<i64> {
    row.get(key)?.as_i64()
}

/// Read an unsigned count out of a JSON row field, saturating at zero.
fn row_u32(row: &serde_json::Value, key: &str) -> Option<u32> {
    let value = row.get(key)?.as_i64()?;
    Some(u32::try_from(value.max(0)).unwrap_or(u32::MAX))
}

/// Read a boolean out of a JSON row field. `DuckDB` may hand back either a JSON
/// bool or the 0/1 integer the SQLite-flavoured schema declares.
fn row_bool(row: &serde_json::Value, key: &str) -> Option<bool> {
    match row.get(key)? {
        serde_json::Value::Bool(value) => Some(*value),
        serde_json::Value::Number(value) => Some(value.as_i64().unwrap_or(0) != 0),
        _ => None,
    }
}

/// Seconds elapsed since `ts`, clamped at zero for clock skew.
fn seconds_since(ts: DateTime<Utc>) -> u64 {
    u64::try_from((Utc::now() - ts).num_seconds().max(0)).unwrap_or(0)
}

/// Latest `collected_at` in a table, or `None` when the table has no rows.
fn latest_collected_at(store: &VcStore, table: &str) -> Option<DateTime<Utc>> {
    let sql =
        format!("SELECT CAST(MAX(CAST(collected_at AS TIMESTAMP)) AS TEXT) AS max_ts FROM {table}");
    let rows = store.query_json(&sql).ok()?;
    row_ts(rows.first()?, "max_ts")
}

/// Build the envelope staleness map: seconds since the newest row in each
/// collector-backed table this command reads. Tables that have never been
/// written are simply absent from the map.
fn staleness_for(store: &VcStore, tables: &[&str]) -> HashMap<String, u64> {
    let mut staleness = HashMap::new();
    for table in tables {
        if let Some(ts) = latest_collected_at(store, table) {
            staleness.insert((*table).to_string(), seconds_since(ts));
        }
    }
    staleness
}

/// Map a health score onto the severity vocabulary the robot schemas allow.
fn severity_for_score(score: f64) -> &'static str {
    if score >= 0.8 {
        "healthy"
    } else if score >= 0.5 {
        "warning"
    } else {
        "critical"
    }
}

/// A machine as the registry knows it.
struct MachineRow {
    id: String,
    name: String,
    status: String,
    last_seen: Option<DateTime<Utc>>,
}

/// Registry inventory. `last_seen` is `None` for machines that have been
/// declared but never collected from — we do not substitute "now".
fn load_machines(store: &VcStore) -> Result<Vec<MachineRow>, CliError> {
    let sql = "SELECT machine_id, hostname, display_name, status, \
               CAST(last_seen_at AS TEXT) AS last_seen_at \
               FROM machines ORDER BY hostname";
    let rows = store.query_json(sql)?;

    Ok(rows
        .iter()
        .filter_map(|row| {
            let id = row_str(row, "machine_id")?;
            let name = row_str(row, "display_name")
                .or_else(|| row_str(row, "hostname"))
                .unwrap_or_else(|| id.clone());
            Some(MachineRow {
                id,
                name,
                status: row_str(row, "status").unwrap_or_else(|| "unknown".to_string()),
                last_seen: row_ts(row, "last_seen_at"),
            })
        })
        .collect())
}

/// Latest persisted health summary per machine: score plus worst factor.
fn load_health_scores(store: &VcStore) -> Result<HashMap<String, (f64, Option<String>)>, CliError> {
    let summaries = QueryBuilder::new(store).list_health_summaries()?;

    Ok(summaries
        .iter()
        .filter_map(|row| {
            let machine_id = row_str(row, "machine_id")?;
            let overall = row_f64(row, "overall_score")?;
            Some((machine_id, (overall, row_str(row, "worst_factor_id"))))
        })
        .collect())
}

/// Active (unfinished) agent sessions per machine.
fn load_agent_counts(store: &VcStore) -> Result<HashMap<String, u32>, CliError> {
    let sql = "SELECT machine_id, COUNT(*) AS active_agents FROM agent_sessions \
               WHERE ended_at IS NULL GROUP BY machine_id";
    let rows = store.query_json(sql)?;

    Ok(rows
        .iter()
        .filter_map(|row| {
            let machine_id = row_str(row, "machine_id")?;
            Some((machine_id, row_u32(row, "active_agents").unwrap_or(0)))
        })
        .collect())
}

/// Latest resource metrics per machine.
///
/// Prefers `sys_samples` (full sysmoni sample) and falls back to
/// `sys_fallback_samples` (the always-on baseline probe, which has no CPU
/// total). Disk headroom comes from the newest `sys_filesystems` snapshot.
/// Anything we do not have stays `None`.
fn load_latest_metrics(store: &VcStore) -> Result<HashMap<String, MachineMetrics>, CliError> {
    let mut metrics: HashMap<String, MachineMetrics> = HashMap::new();

    let fallback_sql = "SELECT f.machine_id, f.load5, f.mem_used_bytes, f.mem_total_bytes \
                        FROM sys_fallback_samples f \
                        INNER JOIN ( \
                            SELECT machine_id, MAX(CAST(collected_at AS TIMESTAMP)) AS max_ts \
                            FROM sys_fallback_samples GROUP BY machine_id \
                        ) latest ON f.machine_id = latest.machine_id \
                            AND CAST(f.collected_at AS TIMESTAMP) = latest.max_ts";
    let sys_sql = "SELECT s.machine_id, s.cpu_total, s.load5, s.mem_used_bytes, s.mem_total_bytes \
                   FROM sys_samples s \
                   INNER JOIN ( \
                       SELECT machine_id, MAX(CAST(collected_at AS TIMESTAMP)) AS max_ts \
                       FROM sys_samples GROUP BY machine_id \
                   ) latest ON s.machine_id = latest.machine_id \
                       AND CAST(s.collected_at AS TIMESTAMP) = latest.max_ts";

    // Fallback first so the richer sysmoni sample overwrites it.
    for sql in [fallback_sql, sys_sql] {
        for row in store.query_json(sql)? {
            let Some(machine_id) = row_str(&row, "machine_id") else {
                continue;
            };
            let mem_pct = match (
                row_f64(&row, "mem_used_bytes"),
                row_f64(&row, "mem_total_bytes"),
            ) {
                (Some(used), Some(total)) if total > 0.0 => Some((used / total) * 100.0),
                _ => None,
            };
            let entry = metrics.entry(machine_id).or_default();
            entry.cpu_pct = row_f64(&row, "cpu_total").or(entry.cpu_pct);
            entry.load5 = row_f64(&row, "load5").or(entry.load5);
            entry.mem_pct = mem_pct.or(entry.mem_pct);
        }
    }

    let disk_sql = "SELECT f.machine_id, MAX(f.usage_pct) AS max_usage_pct \
                    FROM sys_filesystems f \
                    INNER JOIN ( \
                        SELECT machine_id, MAX(CAST(collected_at AS TIMESTAMP)) AS max_ts \
                        FROM sys_filesystems GROUP BY machine_id \
                    ) latest ON f.machine_id = latest.machine_id \
                        AND CAST(f.collected_at AS TIMESTAMP) = latest.max_ts \
                    GROUP BY f.machine_id";
    for row in store.query_json(disk_sql)? {
        let Some(machine_id) = row_str(&row, "machine_id") else {
            continue;
        };
        // Fullest filesystem drives headroom: the tightest mount is what hurts.
        if let Some(used_pct) = row_f64(&row, "max_usage_pct") {
            metrics.entry(machine_id).or_default().disk_free_pct =
                Some((100.0 - used_pct).clamp(0.0, 100.0));
        }
    }

    Ok(metrics)
}

/// Unresolved alerts grouped by the severity vocabulary `vc_alert` writes.
fn load_alert_counts(store: &VcStore) -> Result<AlertCounts, CliError> {
    let sql = "SELECT LOWER(severity) AS severity, COUNT(*) AS alert_count \
               FROM alert_history WHERE resolved_at IS NULL GROUP BY LOWER(severity)";
    let rows = store.query_json(sql)?;

    let mut counts = AlertCounts::default();
    for row in &rows {
        let count = row_u32(row, "alert_count").unwrap_or(0);
        match row_str(row, "severity").as_deref() {
            Some("critical") => counts.critical += count,
            Some("warning") => counts.warning += count,
            Some("info") => counts.info += count,
            _ => {}
        }
    }
    Ok(counts)
}

/// Latest git status per repository, joined onto the repo inventory.
///
/// A `FULL OUTER JOIN` because the two sides can drift: `repos` may list a
/// repository the status collector has not reached yet, and a status snapshot
/// can outlive an inventory row.
fn load_repos(store: &VcStore) -> Result<Vec<RepoInfo>, CliError> {
    let sql = "SELECT \
                   COALESCE(r.machine_id, s.machine_id) AS machine_id, \
                   COALESCE(r.repo_id, s.repo_id) AS repo_id, \
                   r.name AS name, \
                   r.path AS path, \
                   r.url AS url, \
                   s.branch AS branch, \
                   s.dirty AS dirty, \
                   s.ahead AS ahead, \
                   s.behind AS behind, \
                   s.modified_count AS modified_count, \
                   s.untracked_count AS untracked_count, \
                   CAST(s.collected_at AS TEXT) AS collected_at \
               FROM repos r \
               FULL OUTER JOIN ( \
                   SELECT rs.machine_id, rs.repo_id, rs.branch, rs.dirty, rs.ahead, rs.behind, \
                          rs.modified_count, rs.untracked_count, rs.collected_at \
                   FROM repo_status_snapshots rs \
                   INNER JOIN ( \
                       SELECT machine_id, repo_id, MAX(CAST(collected_at AS TIMESTAMP)) AS max_ts \
                       FROM repo_status_snapshots GROUP BY machine_id, repo_id \
                   ) latest ON rs.machine_id = latest.machine_id \
                       AND rs.repo_id = latest.repo_id \
                       AND CAST(rs.collected_at AS TIMESTAMP) = latest.max_ts \
               ) s ON r.machine_id = s.machine_id AND r.repo_id = s.repo_id \
               ORDER BY 1, 2";
    let rows = store.query_json(sql)?;

    Ok(rows
        .iter()
        .filter_map(|row| {
            Some(RepoInfo {
                machine_id: row_str(row, "machine_id")?,
                repo_id: row_str(row, "repo_id")?,
                name: row_str(row, "name"),
                path: row_str(row, "path"),
                url: row_str(row, "url"),
                branch: row_str(row, "branch"),
                dirty: row_bool(row, "dirty"),
                ahead: row_u32(row, "ahead"),
                behind: row_u32(row, "behind"),
                modified: row_u32(row, "modified_count"),
                untracked: row_u32(row, "untracked_count"),
                collected_at: row_ts(row, "collected_at"),
            })
        })
        .collect())
}

/// Roll repositories up into the summary carried by `vc robot status`.
fn summarize_repos(repos: &[RepoInfo]) -> RepoSummary {
    let count = |predicate: fn(&RepoInfo) -> bool| {
        u32::try_from(repos.iter().filter(|repo| predicate(repo)).count()).unwrap_or(u32::MAX)
    };

    RepoSummary {
        total: u32::try_from(repos.len()).unwrap_or(u32::MAX),
        dirty: count(|repo| repo.dirty == Some(true)),
        ahead: count(|repo| repo.ahead.is_some_and(|value| value > 0)),
        behind: count(|repo| repo.behind.is_some_and(|value| value > 0)),
    }
}

/// Latest usage snapshot per account, joined with the latest profile snapshot.
///
/// A `FULL OUTER JOIN` because caut (usage) and caam (profiles) are separate
/// collectors: a configured account may have a profile but no usage yet, and a
/// usage row can exist for an account caam has not profiled.
fn load_accounts(store: &VcStore) -> Result<Vec<AccountInfo>, CliError> {
    let sql = "SELECT \
                   COALESCE(u.machine_id, p.machine_id) AS machine_id, \
                   COALESCE(u.provider, p.provider) AS provider, \
                   COALESCE(u.account_id, p.account_id) AS account_id, \
                   u.usage_pct AS usage_pct, \
                   u.tokens_used AS tokens_used, \
                   u.tokens_limit AS tokens_limit, \
                   CAST(u.resets_at AS TEXT) AS resets_at, \
                   CAST(u.collected_at AS TEXT) AS collected_at, \
                   p.email AS email, \
                   p.plan_type AS plan_type, \
                   p.is_current AS is_current, \
                   p.is_active AS is_active \
               FROM ( \
                   SELECT au.machine_id, au.provider, au.account_id, au.usage_pct, \
                          au.tokens_used, au.tokens_limit, au.resets_at, au.collected_at \
                   FROM account_usage_snapshots au \
                   INNER JOIN ( \
                       SELECT machine_id, provider, account_id, \
                              MAX(CAST(collected_at AS TIMESTAMP)) AS max_ts \
                       FROM account_usage_snapshots \
                       GROUP BY machine_id, provider, account_id \
                   ) latest ON au.machine_id = latest.machine_id \
                       AND au.provider = latest.provider \
                       AND au.account_id = latest.account_id \
                       AND CAST(au.collected_at AS TIMESTAMP) = latest.max_ts \
               ) u \
               FULL OUTER JOIN ( \
                   SELECT ap.machine_id, ap.provider, ap.account_id, ap.email, ap.plan_type, \
                          ap.is_current, ap.is_active \
                   FROM account_profile_snapshots ap \
                   INNER JOIN ( \
                       SELECT machine_id, provider, account_id, \
                              MAX(CAST(collected_at AS TIMESTAMP)) AS max_ts \
                       FROM account_profile_snapshots \
                       GROUP BY machine_id, provider, account_id \
                   ) latest ON ap.machine_id = latest.machine_id \
                       AND ap.provider = latest.provider \
                       AND ap.account_id = latest.account_id \
                       AND CAST(ap.collected_at AS TIMESTAMP) = latest.max_ts \
               ) p ON u.machine_id = p.machine_id \
                   AND u.provider = p.provider \
                   AND u.account_id = p.account_id \
               ORDER BY 4 DESC NULLS LAST, 2, 3";
    let rows = store.query_json(sql)?;

    Ok(rows
        .iter()
        .filter_map(|row| {
            Some(AccountInfo {
                machine_id: row_str(row, "machine_id")?,
                provider: row_str(row, "provider")?,
                account_id: row_str(row, "account_id")?,
                email: row_str(row, "email"),
                plan_type: row_str(row, "plan_type"),
                is_current: row_bool(row, "is_current"),
                is_active: row_bool(row, "is_active"),
                usage_pct: row_f64(row, "usage_pct"),
                tokens_used: row_i64(row, "tokens_used"),
                tokens_limit: row_i64(row, "tokens_limit"),
                resets_at: row_ts(row, "resets_at"),
                collected_at: row_ts(row, "collected_at"),
            })
        })
        .collect())
}

/// Load account usage history for the Oracle.
///
/// The `predictions` table is never written by anything, so forecasts are
/// computed live: this pulls the raw usage series and `vc_oracle` turns it into
/// velocity, time-to-limit and a recommended action.
fn load_usage_samples(store: &VcStore) -> Result<Vec<UsageSample>, CliError> {
    let cutoff = (Utc::now() - TimeDelta::hours(ORACLE_LOOKBACK_HOURS))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let sql = format!(
        "SELECT provider, account_id, usage_pct, \
                CAST(collected_at AS TEXT) AS collected_at, \
                CAST(resets_at AS TEXT) AS resets_at \
         FROM account_usage_snapshots \
         WHERE usage_pct IS NOT NULL \
           AND CAST(collected_at AS TIMESTAMP) >= CAST('{cutoff}' AS TIMESTAMP) \
         ORDER BY CAST(collected_at AS TIMESTAMP) ASC"
    );
    let rows = store.query_json(&sql)?;

    Ok(rows
        .iter()
        .filter_map(|row| {
            Some(UsageSample {
                provider: row_str(row, "provider")?,
                account: row_str(row, "account_id")?,
                used_percent: row_f64(row, "usage_pct")?,
                collected_at: row_ts(row, "collected_at")?,
                resets_at: row_ts(row, "resets_at"),
            })
        })
        .collect())
}

// ============================================================================
// Health Command Implementation
// ============================================================================

/// Generate fleet health from the store.
///
/// # Errors
///
/// Returns [`CliError`] if any store query fails.
pub fn robot_health(store: &VcStore) -> Result<RobotEnvelope<HealthData>, CliError> {
    let overview = QueryBuilder::new(store).fleet_overview()?;
    let machines = load_machines(store)?;
    let health_scores = load_health_scores(store)?;
    let agent_counts = load_agent_counts(store)?;
    let metrics = load_latest_metrics(store)?;
    let alerts_by_severity = load_alert_counts(store)?;

    let mut warnings = Vec::new();
    if machines.is_empty() {
        warnings.push(
            "machine registry is empty - run `vc machine add` or `vc collect` to populate it"
                .to_string(),
        );
    }
    if health_scores.is_empty() {
        warnings.push(
            "no health summaries persisted - per-machine scores are null and the fleet score \
             falls back to 1.0"
                .to_string(),
        );
    }

    let machine_health: Vec<MachineHealth> = machines
        .iter()
        .map(|machine| {
            let scored = health_scores.get(&machine.id);
            let overall = scored.map(|(value, _)| *value);
            let top_issue = scored.and_then(|(_, worst)| worst.clone());
            // A machine the registry calls online but whose score has fallen is
            // degraded, not healthy.
            let status = match (machine.status.as_str(), overall) {
                ("online", Some(value)) if value < 0.8 => "degraded".to_string(),
                _ => machine.status.clone(),
            };
            let machine_metrics = metrics.get(&machine.id);

            MachineHealth {
                id: machine.id.clone(),
                name: machine.name.clone(),
                score: overall,
                status,
                top_issue,
                last_seen: machine.last_seen,
                agent_count: agent_counts.get(&machine.id).copied().unwrap_or(0),
                cpu_percent: machine_metrics.and_then(|m| m.cpu_pct),
                memory_percent: machine_metrics.and_then(|m| m.mem_pct),
            }
        })
        .collect();

    let data = HealthData {
        overall: OverallHealth {
            score: overview.fleet_health_score,
            severity: severity_for_score(overview.fleet_health_score).to_string(),
            active_alerts: u32::try_from(overview.active_alerts).unwrap_or(u32::MAX),
            machine_count: u32::try_from(overview.total_machines).unwrap_or(u32::MAX),
            agent_count: u32::try_from(overview.active_agents).unwrap_or(u32::MAX),
        },
        machines: machine_health,
        alerts_by_severity,
    };

    Ok(RobotEnvelope::new("vc.robot.health.v1", data)
        .with_staleness(staleness_for(
            store,
            &["sys_samples", "sys_fallback_samples", "health_summary"],
        ))
        .with_warnings(warnings))
}

/// Generate triage recommendations from the store.
///
/// Every recommendation is derived from a row that exists: an unresolved alert,
/// an offline machine, a failing collector, an account under pressure, or a
/// repository that has drifted from its remote.
///
/// # Errors
///
/// Returns [`CliError`] if any store query fails.
pub fn robot_triage(store: &VcStore) -> Result<RobotEnvelope<TriageData>, CliError> {
    let overview = QueryBuilder::new(store).fleet_overview()?;
    let machines = load_machines(store)?;
    let health_scores = load_health_scores(store)?;
    let accounts = load_accounts(store)?;
    let repos = load_repos(store)?;

    let mut recommendations: Vec<Recommendation> = Vec::new();
    let mut suggested_commands: Vec<SuggestedCommand> = Vec::new();
    let mut warnings = Vec::new();

    // 1. Unresolved alerts, worst first.
    let alert_sql = "SELECT id, rule_id, LOWER(severity) AS severity, title, message, machine_id, \
                     acknowledged, CAST(fired_at AS TEXT) AS fired_at \
                     FROM alert_history WHERE resolved_at IS NULL \
                     ORDER BY CASE LOWER(severity) \
                         WHEN 'critical' THEN 0 WHEN 'warning' THEN 1 ELSE 2 END, \
                         CAST(fired_at AS TIMESTAMP) DESC \
                     LIMIT 10";
    for row in store.query_json(alert_sql)? {
        let severity = row_str(&row, "severity").unwrap_or_else(|| "info".to_string());
        let title = row_str(&row, "title").unwrap_or_else(|| "Unresolved alert".to_string());
        let id = row_i64(&row, "id").unwrap_or(-1);
        let priority = match severity.as_str() {
            "critical" => 1,
            "warning" => 2,
            _ => 3,
        };
        recommendations.push(Recommendation {
            id: format!("alert-{id}"),
            priority,
            title,
            description: row_str(&row, "message").unwrap_or_else(|| {
                format!(
                    "Alert from rule {} is unresolved",
                    row_str(&row, "rule_id").unwrap_or_else(|| "unknown".to_string())
                )
            }),
            scope: row_str(&row, "machine_id").unwrap_or_else(|| "fleet".to_string()),
            action: if row_bool(&row, "acknowledged") == Some(true) {
                "Resolve the underlying condition; the alert is acknowledged but still firing"
                    .to_string()
            } else {
                format!("Acknowledge with `vc alert ack {id}` once you have triaged it")
            },
        });
    }
    if !recommendations.is_empty() {
        suggested_commands.push(SuggestedCommand {
            command: "vc alert list --unacked".to_string(),
            reason: format!(
                "{} unresolved alert(s) in alert_history",
                recommendations.len()
            ),
            confidence: 0.95,
        });
    }

    // 2. Machines that are offline or scoring badly.
    for machine in &machines {
        let overall = health_scores.get(&machine.id).map(|(value, _)| *value);
        let worst = health_scores
            .get(&machine.id)
            .and_then(|(_, worst)| worst.clone());

        if machine.status == "offline" {
            recommendations.push(Recommendation {
                id: format!("machine-offline-{}", machine.id),
                priority: 1,
                title: format!("Machine {} is offline", machine.name),
                description: match machine.last_seen {
                    Some(ts) => format!("Last seen {} seconds ago", seconds_since(ts)),
                    None => "This machine has never been collected from".to_string(),
                },
                scope: machine.id.clone(),
                action: format!("Probe it with `vc machine probe {}`", machine.id),
            });
            suggested_commands.push(SuggestedCommand {
                command: format!("vc machine probe {}", machine.id),
                reason: "Machine is marked offline in the registry".to_string(),
                confidence: 0.8,
            });
        } else if let Some(value) = overall
            && value < 0.8
        {
            recommendations.push(Recommendation {
                id: format!("machine-degraded-{}", machine.id),
                priority: if value < 0.5 { 1 } else { 2 },
                title: format!("Machine {} health is {:.2}", machine.name, value),
                description: match worst {
                    Some(ref factor) => format!("Worst health factor: {factor}"),
                    None => "Health summary is below the healthy threshold".to_string(),
                },
                scope: machine.id.clone(),
                action: format!("Inspect with `vc status --machine {}`", machine.id),
            });
        }
    }

    // 3. Accounts approaching their rate limit.
    for account in &accounts {
        let Some(usage) = account.usage_pct else {
            continue;
        };
        if usage < ACCOUNT_PRESSURE_PCT {
            continue;
        }
        let label = account
            .email
            .clone()
            .unwrap_or_else(|| account.account_id.clone());
        recommendations.push(Recommendation {
            id: format!("account-usage-{}-{}", account.provider, account.account_id),
            priority: if usage >= 95.0 { 1 } else { 2 },
            title: format!("{} account {label} at {usage:.0}%", account.provider),
            description: match account.resets_at {
                Some(reset) => format!("Window resets at {}", reset.to_rfc3339()),
                None => "No reset time reported by the collector".to_string(),
            },
            scope: format!("{}:{}", account.provider, account.account_id),
            action: "Swap accounts (caam) or slow down before the limit lands".to_string(),
        });
    }
    if accounts.iter().any(|account| {
        account
            .usage_pct
            .is_some_and(|usage| usage >= ACCOUNT_PRESSURE_PCT)
    }) {
        suggested_commands.push(SuggestedCommand {
            command: "vc robot oracle".to_string(),
            reason: "At least one account is above 80% - get a time-to-limit forecast".to_string(),
            confidence: 0.85,
        });
    }

    // 4. Collectors that are failing or have never succeeded.
    let collector_sql = "SELECT machine_id, collector_name, status, error_message, \
                         CAST(last_success_at AS TEXT) AS last_success_at \
                         FROM collector_status \
                         WHERE status IS NOT NULL AND LOWER(status) <> 'ok' \
                         ORDER BY machine_id, collector_name LIMIT 10";
    for row in store.query_json(collector_sql)? {
        let Some(collector) = row_str(&row, "collector_name") else {
            continue;
        };
        let machine_id = row_str(&row, "machine_id").unwrap_or_else(|| "local".to_string());
        let status = row_str(&row, "status").unwrap_or_else(|| "unknown".to_string());
        recommendations.push(Recommendation {
            id: format!("collector-{machine_id}-{collector}"),
            priority: 2,
            title: format!("Collector {collector} is {status}"),
            description: row_str(&row, "error_message").unwrap_or_else(|| {
                match row_ts(&row, "last_success_at") {
                    Some(ts) => format!("Last succeeded {} seconds ago", seconds_since(ts)),
                    None => "This collector has never succeeded".to_string(),
                }
            }),
            scope: machine_id,
            action: format!("Re-run with `vc collect --collector {collector}`"),
        });
    }

    // 5. Repositories that have drifted from their remotes.
    let repo_summary = summarize_repos(&repos);
    if repo_summary.behind > 0 || repo_summary.dirty > 0 {
        recommendations.push(Recommendation {
            id: "repos-drift".to_string(),
            priority: 3,
            title: format!(
                "{} dirty, {} behind of {} repos",
                repo_summary.dirty, repo_summary.behind, repo_summary.total
            ),
            description: "Working trees have uncommitted changes or are behind their remotes"
                .to_string(),
            scope: "repos".to_string(),
            action: "Review with `vc robot repos`".to_string(),
        });
        suggested_commands.push(SuggestedCommand {
            command: "vc robot repos".to_string(),
            reason: "Repositories have drifted from their remotes".to_string(),
            confidence: 0.7,
        });
    }

    // 6. Guardian runs waiting on a human.
    if overview.pending_approvals > 0 {
        recommendations.push(Recommendation {
            id: "guardian-pending-approvals".to_string(),
            priority: 2,
            title: format!(
                "{} playbook run(s) awaiting approval",
                overview.pending_approvals
            ),
            description: "Guardian will not act until these are approved".to_string(),
            scope: "guardian".to_string(),
            action: "Review with `vc guardian runs` and approve or reject".to_string(),
        });
        suggested_commands.push(SuggestedCommand {
            command: "vc guardian runs".to_string(),
            reason: "Playbook runs are blocked on approval".to_string(),
            confidence: 0.9,
        });
    }

    // Nothing to triage because nothing has been collected is a different
    // finding from nothing to triage because everything is fine. Say which.
    let store_is_empty = machines.is_empty() && accounts.is_empty() && repos.is_empty();
    if store_is_empty {
        warnings.push(
            "store has no machines, accounts or repos - nothing has been collected yet".to_string(),
        );
        suggested_commands.push(SuggestedCommand {
            command: "vc collect".to_string(),
            reason: "The store is empty - run an initial collection".to_string(),
            confidence: 0.9,
        });
    }

    recommendations.sort_by_key(|recommendation| recommendation.priority);

    let data = TriageData {
        recommendations,
        suggested_commands,
    };

    Ok(RobotEnvelope::new("vc.robot.triage.v1", data)
        .with_staleness(staleness_for(
            store,
            &[
                "account_usage_snapshots",
                "repo_status_snapshots",
                "sys_samples",
            ],
        ))
        .with_warnings(warnings))
}

/// Generate comprehensive fleet status from the store.
///
/// Returns machine status, repo summary, and unresolved alert counts. This is
/// the primary command for agents to understand overall system state.
///
/// # Errors
///
/// Returns [`CliError`] if any store query fails.
pub fn robot_status(store: &VcStore) -> Result<RobotEnvelope<StatusData>, CliError> {
    let overview = QueryBuilder::new(store).fleet_overview()?;
    let machines = load_machines(store)?;
    let health_scores = load_health_scores(store)?;
    let metrics = load_latest_metrics(store)?;
    let repos = load_repos(store)?;
    let alert_counts = load_alert_counts(store)?;

    let mut warnings = Vec::new();
    if machines.is_empty() {
        warnings.push(
            "machine registry is empty - run `vc machine add` or `vc collect` to populate it"
                .to_string(),
        );
    }
    if health_scores.is_empty() {
        warnings.push(
            "no health summaries persisted - per-machine scores are null and the fleet score \
             falls back to 1.0"
                .to_string(),
        );
    }

    let machine_status: Vec<MachineStatus> = machines
        .iter()
        .map(|machine| {
            let scored = health_scores.get(&machine.id);
            let health_score = scored.map(|(value, _)| *value);
            let status = match (machine.status.as_str(), health_score) {
                ("online", Some(value)) if value < 0.8 => "degraded".to_string(),
                _ => machine.status.clone(),
            };

            MachineStatus {
                id: machine.id.clone(),
                status,
                last_seen: machine.last_seen,
                health_score,
                metrics: metrics.get(&machine.id).filter(|m| !m.is_empty()).cloned(),
                top_issue: scored.and_then(|(_, worst)| worst.clone()),
            }
        })
        .collect();

    let data = StatusData {
        fleet: FleetSummary {
            total_machines: u32::try_from(overview.total_machines).unwrap_or(u32::MAX),
            online: u32::try_from(overview.online_machines).unwrap_or(u32::MAX),
            offline: u32::try_from(overview.offline_machines).unwrap_or(u32::MAX),
            health_score: overview.fleet_health_score,
        },
        machines: machine_status,
        repos: summarize_repos(&repos),
        alerts: AlertSummary {
            critical: alert_counts.critical,
            warning: alert_counts.warning,
            info: alert_counts.info,
        },
    };

    Ok(RobotEnvelope::new("vc.robot.status.v1", data)
        .with_staleness(staleness_for(
            store,
            &[
                "sys_samples",
                "sys_fallback_samples",
                "repo_status_snapshots",
                "health_summary",
            ],
        ))
        .with_warnings(warnings))
}

// ============================================================================
// Accounts / Repos / Oracle Command Implementations
// ============================================================================

/// Account status for `vc robot accounts`, from the caut and caam collectors.
///
/// # Errors
///
/// Returns [`CliError`] if any store query fails.
pub fn robot_accounts(store: &VcStore) -> Result<RobotEnvelope<AccountsData>, CliError> {
    let accounts = load_accounts(store)?;

    let mut warnings = Vec::new();
    if accounts.is_empty() {
        warnings.push(
            "no account snapshots in the store - run `vc collect` with the caut/caam collectors"
                .to_string(),
        );
    } else if accounts.iter().all(|account| account.usage_pct.is_none()) {
        warnings.push(
            "accounts are known from profiles (caam) but no usage snapshot (caut) exists yet"
                .to_string(),
        );
    }

    let data = AccountsData {
        total: u32::try_from(accounts.len()).unwrap_or(u32::MAX),
        accounts,
    };

    Ok(RobotEnvelope::new("vc.robot.accounts.v1", data)
        .with_staleness(staleness_for(
            store,
            &["account_usage_snapshots", "account_profile_snapshots"],
        ))
        .with_warnings(warnings))
}

/// Repository status for `vc robot repos`, from the ru collector.
///
/// # Errors
///
/// Returns [`CliError`] if any store query fails.
pub fn robot_repos(store: &VcStore) -> Result<RobotEnvelope<ReposData>, CliError> {
    let repos = load_repos(store)?;

    let mut warnings = Vec::new();
    if repos.is_empty() {
        warnings.push(
            "no repositories in the store - run `vc collect` with the ru collector".to_string(),
        );
    } else if repos.iter().all(|repo| repo.branch.is_none()) {
        warnings
            .push("repositories are inventoried but no git status snapshot exists yet".to_string());
    }

    let data = ReposData {
        summary: summarize_repos(&repos),
        repos,
    };

    Ok(RobotEnvelope::new("vc.robot.repos.v1", data)
        .with_staleness(staleness_for(store, &["repo_status_snapshots"]))
        .with_warnings(warnings))
}

/// Rate-limit forecasts for `vc robot oracle`.
///
/// Forecasts are computed live from the `account_usage_snapshots` history via
/// [`vc_oracle::RateLimitForecaster`]. Nothing writes the `predictions` table, so
/// nothing reads it: a stored prediction here would be a fabrication.
///
/// # Errors
///
/// Returns [`CliError`] if any store query fails.
pub fn robot_oracle(store: &VcStore) -> Result<RobotEnvelope<OracleData>, CliError> {
    let samples = load_usage_samples(store)?;
    let sample_count = u32::try_from(samples.len()).unwrap_or(u32::MAX);

    let forecasts = RateLimitForecaster::new().forecast(samples.clone());

    let mut warnings = Vec::new();
    if samples.is_empty() {
        warnings.push(
            format!("no account usage samples in the last {ORACLE_LOOKBACK_HOURS}h - run `vc collect` with the caut collector"),
        );
    } else if forecasts.is_empty() {
        warnings
            .push("usage samples exist but no account had enough history to forecast".to_string());
    }

    let forecast_info: Vec<ForecastInfo> = forecasts
        .into_iter()
        .map(|forecast| {
            let alternatives = vc_oracle::rate_limit::rank_alternative_accounts(
                &samples,
                &forecast.provider,
                &forecast.account,
            );

            // The forecaster leaves `to_account` empty because it has no store
            // access; we do, so fill it with the account with the most headroom.
            let mut action = serde_json::to_value(&forecast.recommended_action)
                .unwrap_or_else(|_| serde_json::json!({"type": "continue"}));
            if action.get("type").and_then(serde_json::Value::as_str) == Some("swap_now") {
                match alternatives.first() {
                    Some((account, _)) => {
                        action["to_account"] = serde_json::Value::String(account.clone());
                    }
                    None => action["to_account"] = serde_json::Value::Null,
                }
            }

            // The forecaster encodes "never" as u64::MAX / 2. Report that as null
            // rather than as a 292-billion-year countdown.
            let time_to_limit_secs = forecast.time_to_limit.as_secs();
            let time_to_limit_secs = if time_to_limit_secs >= u64::MAX / 2 {
                None
            } else {
                Some(time_to_limit_secs)
            };

            ForecastInfo {
                provider: forecast.provider,
                account: forecast.account,
                current_usage_pct: forecast.current_usage_pct,
                velocity_pct_per_min: forecast.current_velocity,
                time_to_limit_secs,
                confidence: forecast.confidence,
                recommended_action: action,
                optimal_swap_time: forecast.optimal_swap_time,
                alternative_accounts: alternatives
                    .into_iter()
                    .map(|(account, headroom_pct)| AlternativeAccount {
                        account,
                        headroom_pct,
                    })
                    .collect(),
            }
        })
        .collect();

    let data = OracleData {
        forecasts: forecast_info,
        sample_count,
        lookback_hours: u32::try_from(ORACLE_LOOKBACK_HOURS).unwrap_or(u32::MAX),
    };

    Ok(RobotEnvelope::new("vc.robot.oracle.v1", data)
        .with_staleness(staleness_for(store, &["account_usage_snapshots"]))
        .with_warnings(warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_robot_envelope_new() {
        let envelope = RobotEnvelope::new("test.v1", "hello");
        assert_eq!(envelope.schema_version, "test.v1");
        assert_eq!(envelope.data, "hello");
        assert!(envelope.staleness.is_empty());
        assert!(envelope.warnings.is_empty());
    }

    #[test]
    fn test_robot_envelope_with_staleness() {
        let mut staleness = HashMap::new();
        staleness.insert("sysmoni".to_string(), 60);

        let envelope = RobotEnvelope::new("test.v1", "data").with_staleness(staleness);

        assert_eq!(envelope.staleness.get("sysmoni"), Some(&60));
    }

    #[test]
    fn test_robot_envelope_with_warnings() {
        let envelope = RobotEnvelope::new("test.v1", "data")
            .add_warning("warning 1")
            .add_warning("warning 2");

        assert_eq!(envelope.warnings.len(), 2);
    }

    #[test]
    fn test_robot_envelope_to_json() {
        let envelope = RobotEnvelope::new("test.v1", serde_json::json!({"key": "value"}));
        let json = envelope.to_json();

        assert!(json.contains("test.v1"));
        assert!(json.contains("key"));
        assert!(json.contains("value"));
    }

    /// Store with one machine, one repo (dirty, behind), one account at 92%,
    /// one unresolved critical alert and a health summary of 0.4.
    fn populated_store() -> VcStore {
        let store = VcStore::open_memory().expect("open store");
        let now = Utc::now().to_rfc3339();

        store
            .execute_batch(&format!(
                "INSERT INTO machines (machine_id, hostname, display_name, status, last_seen_at) \
                 VALUES ('orko', 'orko.local', 'Orko', 'online', '{now}'); \
                 INSERT INTO machines (machine_id, hostname, status) \
                 VALUES ('ghost', 'ghost.local', 'offline'); \
                 INSERT INTO health_summary (machine_id, collected_at, overall_score, \
                     worst_factor_id, factor_count, critical_count, warning_count, details_json) \
                 VALUES ('orko', '{now}', 0.4, 'disk_pressure', 1, 1, 0, '{{}}'); \
                 INSERT INTO sys_samples (machine_id, collected_at, cpu_total, load5, \
                     mem_used_bytes, mem_total_bytes) \
                 VALUES ('orko', '{now}', 42.5, 1.5, 8000000000, 16000000000); \
                 INSERT INTO sys_filesystems (machine_id, collected_at, mount, total_bytes, \
                     used_bytes, usage_pct) \
                 VALUES ('orko', '{now}', '/', 100, 91, 91.0); \
                 INSERT INTO repos (machine_id, repo_id, path, url, name) \
                 VALUES ('orko', 'vibe_cockpit', '/src/vibe_cockpit', 'git@x:vc.git', 'vibe_cockpit'); \
                 INSERT INTO repo_status_snapshots (machine_id, collected_at, repo_id, branch, \
                     dirty, ahead, behind, modified_count, untracked_count) \
                 VALUES ('orko', '{now}', 'vibe_cockpit', 'main', 1, 0, 3, 2, 1); \
                 INSERT INTO account_usage_snapshots (machine_id, collected_at, provider, \
                     account_id, usage_pct, tokens_used, tokens_limit) \
                 VALUES ('orko', '{now}', 'claude', 'acct-1', 92.0, 920, 1000); \
                 INSERT INTO account_profile_snapshots (machine_id, collected_at, provider, \
                     account_id, email, plan_type, is_active, is_current) \
                 VALUES ('orko', '{now}', 'claude', 'acct-1', 'a@b.c', 'max', 1, 1); \
                 INSERT INTO alert_history (id, rule_id, fired_at, severity, title, message) \
                 VALUES (1, 'disk-critical', '{now}', 'critical', 'Disk full', 'root at 91%');"
            ))
            .expect("seed store");

        store
    }

    #[test]
    fn test_robot_health_empty_store_reports_nothing_rather_than_inventing_a_machine() {
        let store = VcStore::open_memory().unwrap();
        let envelope = robot_health(&store).unwrap();

        assert_eq!(envelope.schema_version, "vc.robot.health.v1");
        // The old stub fabricated a machine called "local" with score 1.0.
        assert!(envelope.data.machines.is_empty());
        assert_eq!(envelope.data.overall.machine_count, 0);
        assert!(!envelope.warnings.is_empty());
    }

    #[test]
    fn test_robot_health_reads_the_store() {
        let store = populated_store();
        let envelope = robot_health(&store).unwrap();

        assert_eq!(envelope.data.machines.len(), 2);
        let orko = envelope
            .data
            .machines
            .iter()
            .find(|m| m.id == "orko")
            .expect("orko present");
        assert_eq!(orko.name, "Orko");
        assert_eq!(orko.score, Some(0.4));
        // Registry says online, but the health summary says 0.4 - that is degraded.
        assert_eq!(orko.status, "degraded");
        assert_eq!(orko.top_issue.as_deref(), Some("disk_pressure"));
        assert!(orko.last_seen.is_some());
        assert_eq!(orko.cpu_percent, Some(42.5));
        assert_eq!(orko.memory_percent, Some(50.0));

        // A machine that has never been collected from has a null last_seen and
        // a null score - not Utc::now() and not 1.0.
        let ghost = envelope
            .data
            .machines
            .iter()
            .find(|m| m.id == "ghost")
            .expect("ghost present");
        assert!(ghost.last_seen.is_none());
        assert!(ghost.score.is_none());
        assert_eq!(ghost.status, "offline");

        assert_eq!(envelope.data.alerts_by_severity.critical, 1);
        assert_eq!(envelope.data.overall.active_alerts, 1);
    }

    #[test]
    fn test_robot_triage_derives_recommendations_from_rows() {
        let store = populated_store();
        let envelope = robot_triage(&store).unwrap();

        assert_eq!(envelope.schema_version, "vc.robot.triage.v1");
        let ids: Vec<&str> = envelope
            .data
            .recommendations
            .iter()
            .map(|r| r.id.as_str())
            .collect();

        assert!(ids.contains(&"alert-1"), "unresolved alert: {ids:?}");
        assert!(ids.contains(&"machine-offline-ghost"), "offline: {ids:?}");
        assert!(ids.contains(&"machine-degraded-orko"), "degraded: {ids:?}");
        assert!(
            ids.contains(&"account-usage-claude-acct-1"),
            "account at 92%: {ids:?}"
        );
        assert!(ids.contains(&"repos-drift"), "dirty/behind repo: {ids:?}");

        // Sorted worst-first.
        assert_eq!(envelope.data.recommendations[0].priority, 1);
    }

    #[test]
    fn test_robot_triage_empty_store_suggests_collection() {
        let store = VcStore::open_memory().unwrap();
        let envelope = robot_triage(&store).unwrap();

        assert!(envelope.data.recommendations.is_empty());
        assert!(
            envelope
                .data
                .suggested_commands
                .iter()
                .any(|c| c.command == "vc collect")
        );
    }

    #[test]
    fn test_robot_status_reads_the_store() {
        let store = populated_store();
        let envelope = robot_status(&store).unwrap();

        assert_eq!(envelope.schema_version, "vc.robot.status.v1");
        assert_eq!(envelope.data.fleet.total_machines, 2);
        assert_eq!(envelope.data.fleet.online, 1);
        assert_eq!(envelope.data.fleet.offline, 1);

        assert_eq!(envelope.data.repos.total, 1);
        assert_eq!(envelope.data.repos.dirty, 1);
        assert_eq!(envelope.data.repos.behind, 1);
        assert_eq!(envelope.data.repos.ahead, 0);

        assert_eq!(envelope.data.alerts.critical, 1);

        let orko = envelope
            .data
            .machines
            .iter()
            .find(|m| m.id == "orko")
            .expect("orko present");
        let metrics = orko.metrics.as_ref().expect("orko has metrics");
        assert_eq!(metrics.cpu_pct, Some(42.5));
        assert_eq!(metrics.load5, Some(1.5));
        assert_eq!(metrics.disk_free_pct, Some(9.0));

        // Never collected from: no metrics, no last_seen, no score.
        let ghost = envelope
            .data
            .machines
            .iter()
            .find(|m| m.id == "ghost")
            .expect("ghost present");
        assert!(ghost.metrics.is_none());
        assert!(ghost.last_seen.is_none());
        assert!(ghost.health_score.is_none());
    }

    #[test]
    fn test_robot_accounts_reads_usage_and_profile() {
        let store = populated_store();
        let envelope = robot_accounts(&store).unwrap();

        assert_eq!(envelope.schema_version, "vc.robot.accounts.v1");
        assert_eq!(envelope.data.total, 1);

        let account = &envelope.data.accounts[0];
        assert_eq!(account.provider, "claude");
        assert_eq!(account.account_id, "acct-1");
        assert_eq!(account.usage_pct, Some(92.0));
        assert_eq!(account.tokens_used, Some(920));
        assert_eq!(account.email.as_deref(), Some("a@b.c"));
        assert_eq!(account.plan_type.as_deref(), Some("max"));
        assert_eq!(account.is_current, Some(true));
        assert!(envelope.warnings.is_empty());
    }

    #[test]
    fn test_robot_accounts_empty_store_warns() {
        let store = VcStore::open_memory().unwrap();
        let envelope = robot_accounts(&store).unwrap();

        assert_eq!(envelope.data.total, 0);
        assert!(envelope.data.accounts.is_empty());
        assert!(!envelope.warnings.is_empty());
    }

    #[test]
    fn test_robot_repos_reads_status_snapshots() {
        let store = populated_store();
        let envelope = robot_repos(&store).unwrap();

        assert_eq!(envelope.schema_version, "vc.robot.repos.v1");
        assert_eq!(envelope.data.repos.len(), 1);

        let repo = &envelope.data.repos[0];
        assert_eq!(repo.repo_id, "vibe_cockpit");
        assert_eq!(repo.branch.as_deref(), Some("main"));
        assert_eq!(repo.dirty, Some(true));
        assert_eq!(repo.behind, Some(3));
        assert_eq!(repo.modified, Some(2));
        assert_eq!(repo.path.as_deref(), Some("/src/vibe_cockpit"));

        assert_eq!(envelope.data.summary.total, 1);
        assert_eq!(envelope.data.summary.dirty, 1);
    }

    #[test]
    fn test_robot_oracle_forecasts_from_usage_history() {
        let store = VcStore::open_memory().unwrap();
        // Two samples an hour apart: 40% -> 70%, i.e. 0.5%/min, 60 minutes to 100%.
        let earlier = (Utc::now() - TimeDelta::hours(1)).to_rfc3339();
        let now = Utc::now().to_rfc3339();
        store
            .execute_batch(&format!(
                "INSERT INTO account_usage_snapshots (machine_id, collected_at, provider, \
                     account_id, usage_pct) VALUES ('orko', '{earlier}', 'claude', 'acct-1', 40.0); \
                 INSERT INTO account_usage_snapshots (machine_id, collected_at, provider, \
                     account_id, usage_pct) VALUES ('orko', '{now}', 'claude', 'acct-1', 70.0);"
            ))
            .unwrap();

        let envelope = robot_oracle(&store).unwrap();

        assert_eq!(envelope.schema_version, "vc.robot.oracle.v1");
        assert_eq!(envelope.data.sample_count, 2);
        assert_eq!(envelope.data.forecasts.len(), 1);

        let forecast = &envelope.data.forecasts[0];
        assert_eq!(forecast.provider, "claude");
        assert_eq!(forecast.current_usage_pct, 70.0);
        assert!((forecast.velocity_pct_per_min - 0.5).abs() < 0.05);
        let secs = forecast
            .time_to_limit_secs
            .expect("rising usage hits a limit");
        assert!((3000..=4200).contains(&secs), "time_to_limit_secs={secs}");
    }

    #[test]
    fn test_robot_oracle_empty_store_warns_and_forecasts_nothing() {
        let store = VcStore::open_memory().unwrap();
        let envelope = robot_oracle(&store).unwrap();

        assert_eq!(envelope.data.sample_count, 0);
        assert!(envelope.data.forecasts.is_empty());
        assert!(!envelope.warnings.is_empty());
    }

    #[test]
    fn test_null_last_seen_survives_serialization() {
        let store = VcStore::open_memory().unwrap();
        store
            .execute_batch(
                "INSERT INTO machines (machine_id, hostname, status) \
                 VALUES ('ghost', 'ghost.local', 'unknown')",
            )
            .unwrap();

        let json = robot_health(&store).unwrap().to_json();
        assert!(json.contains("\"last_seen\":null"), "{json}");
        assert!(json.contains("\"score\":null"), "{json}");
    }

    #[test]
    fn test_health_data_serialization() {
        let health = HealthData {
            overall: OverallHealth {
                score: 0.85,
                severity: "warning".to_string(),
                active_alerts: 2,
                machine_count: 3,
                agent_count: 5,
            },
            machines: vec![],
            alerts_by_severity: AlertCounts {
                critical: 0,
                warning: 2,
                info: 1,
            },
        };

        let envelope = RobotEnvelope::new("vc.robot.health.v1", health);
        let json = envelope.to_json_pretty();

        // Verify it parses back
        let parsed: RobotEnvelope<HealthData> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.data.overall.score, 0.85);
        assert_eq!(parsed.data.alerts_by_severity.warning, 2);
    }

    // ========================================================================
    // Status Tests
    // ========================================================================

    #[test]
    fn test_robot_status_empty_store() {
        let store = VcStore::open_memory().unwrap();
        let envelope = robot_status(&store).unwrap();

        assert_eq!(envelope.schema_version, "vc.robot.status.v1");
        assert!(envelope.data.fleet.health_score >= 0.0);
        assert!(envelope.data.fleet.health_score <= 1.0);
        // The old stub always produced a machine named "local". An empty store
        // has no machines.
        assert!(envelope.data.machines.is_empty());
        assert_eq!(envelope.data.fleet.total_machines, 0);
    }

    #[test]
    fn test_status_data_serialization() {
        let status = StatusData {
            fleet: FleetSummary {
                total_machines: 4,
                online: 3,
                offline: 1,
                health_score: 0.85,
            },
            machines: vec![MachineStatus {
                id: "orko".to_string(),
                status: "online".to_string(),
                last_seen: Some(Utc::now()),
                health_score: Some(0.91),
                metrics: Some(MachineMetrics {
                    cpu_pct: Some(45.2),
                    mem_pct: Some(68.0),
                    load5: Some(1.8),
                    disk_free_pct: Some(35.0),
                }),
                top_issue: None,
            }],
            repos: RepoSummary {
                total: 15,
                dirty: 2,
                ahead: 3,
                behind: 1,
            },
            alerts: AlertSummary {
                critical: 0,
                warning: 1,
                info: 2,
            },
        };

        let envelope = RobotEnvelope::new("vc.robot.status.v1", status);
        let json = envelope.to_json_pretty();

        // Verify it parses back
        let parsed: RobotEnvelope<StatusData> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.data.fleet.total_machines, 4);
        assert_eq!(parsed.data.fleet.online, 3);
        assert_eq!(parsed.data.repos.dirty, 2);
        assert_eq!(parsed.data.alerts.warning, 1);
    }

    #[test]
    fn test_fleet_summary_creation() {
        let fleet = FleetSummary {
            total_machines: 5,
            online: 4,
            offline: 1,
            health_score: 0.9,
        };

        assert_eq!(fleet.total_machines, fleet.online + fleet.offline);
    }

    #[test]
    fn test_machine_status_with_metrics() {
        let machine = MachineStatus {
            id: "test".to_string(),
            status: "online".to_string(),
            last_seen: Some(Utc::now()),
            health_score: Some(0.95),
            metrics: Some(MachineMetrics {
                cpu_pct: Some(50.0),
                mem_pct: Some(60.0),
                load5: Some(1.5),
                disk_free_pct: Some(40.0),
            }),
            top_issue: None,
        };

        assert!(machine.metrics.is_some());
        let m = machine.metrics.unwrap();
        assert_eq!(m.cpu_pct, Some(50.0));
    }

    #[test]
    fn test_machine_status_never_collected_from() {
        let machine = MachineStatus {
            id: "offline-box".to_string(),
            status: "offline".to_string(),
            last_seen: None,
            health_score: None,
            metrics: None,
            top_issue: Some("no_response".to_string()),
        };

        assert!(machine.metrics.is_none());
        assert!(machine.last_seen.is_none());
        assert!(machine.top_issue.is_some());
    }

    #[test]
    fn test_repo_summary_defaults() {
        let repos = RepoSummary::default();
        assert_eq!(repos.total, 0);
        assert_eq!(repos.dirty, 0);
    }

    #[test]
    fn test_alert_summary_defaults() {
        let alerts = AlertSummary::default();
        assert_eq!(alerts.critical, 0);
        assert_eq!(alerts.warning, 0);
        assert_eq!(alerts.info, 0);
    }

    #[test]
    fn test_machine_metrics_is_empty() {
        assert!(MachineMetrics::default().is_empty());
        assert!(
            !MachineMetrics {
                load5: Some(0.1),
                ..MachineMetrics::default()
            }
            .is_empty()
        );
    }

    #[test]
    fn test_parse_ts_accepts_both_stored_formats() {
        // What the collectors write.
        assert!(parse_ts("2026-07-11T12:00:00.123456+00:00").is_some());
        // What CURRENT_TIMESTAMP column defaults write.
        assert!(parse_ts("2026-07-11 12:00:00").is_some());
        assert!(parse_ts("").is_none());
        assert!(parse_ts("not a timestamp").is_none());
    }

    #[test]
    fn test_severity_for_score() {
        assert_eq!(severity_for_score(1.0), "healthy");
        assert_eq!(severity_for_score(0.8), "healthy");
        assert_eq!(severity_for_score(0.79), "warning");
        assert_eq!(severity_for_score(0.49), "critical");
    }

    #[test]
    fn test_status_json_contains_expected_fields() {
        let store = VcStore::open_memory().unwrap();
        let envelope = robot_status(&store).unwrap();
        let json = envelope.to_json();

        assert!(json.contains("\"fleet\""));
        assert!(json.contains("\"machines\""));
        assert!(json.contains("\"repos\""));
        assert!(json.contains("\"alerts\""));
        assert!(json.contains("\"schema_version\""));
        assert!(json.contains("vc.robot.status.v1"));
    }
}
