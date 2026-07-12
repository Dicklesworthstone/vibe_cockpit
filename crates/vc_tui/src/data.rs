//! Store-backed data loaders for the TUI screens.
//!
//! Every loader in this module runs on a background thread (via `ftui::Cmd::task`)
//! and returns a fully-populated screen struct built from rows that are actually
//! present in the DuckDB store. Nothing here invents values: when a column is
//! absent or NULL the corresponding field is left empty/`None`.
//!
//! Screens without a loader here have no backing query in `vc_query`/`vc_store`
//! yet, and the app renders an explicit "no data source yet" state for them
//! rather than showing an empty dashboard that looks like real (but zeroed) data.

use chrono::{DateTime, Utc};
use vc_query::QueryBuilder;
use vc_store::VcStore;

use crate::TuiError;
use crate::screens::{
    AlertInfo, AlertRuleInfo, AlertStats, AlertSummary, AlertsData, DcgEvent, EventSeverity,
    EventStats, EventsData, MachineOnlineStatus, MachineRow, MachineStatus, MachinesData,
    OverviewData, PtFinding, PtFindingType, RanoEvent, RanoEventType, RepoStatus, SessionInfo,
    SessionsData, Severity,
};

/// Row limits for the list-shaped screens.
const ALERT_HISTORY_LIMIT: usize = 200;
const SESSION_LIMIT: usize = 200;
const EVENT_LIMIT: usize = 100;
const OVERVIEW_ALERT_LIMIT: usize = 5;

// ==========================================================================
// JSON row helpers
// ==========================================================================

fn str_field(row: &serde_json::Value, key: &str) -> Option<String> {
    row.get(key).and_then(|v| match v {
        serde_json::Value::String(s) if !s.is_empty() => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    })
}

fn string_or_default(row: &serde_json::Value, key: &str) -> String {
    str_field(row, key).unwrap_or_default()
}

fn f64_field(row: &serde_json::Value, key: &str) -> Option<f64> {
    row.get(key).and_then(|v| match v {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.parse().ok(),
        _ => None,
    })
}

fn i64_field(row: &serde_json::Value, key: &str) -> Option<i64> {
    row.get(key).and_then(|v| match v {
        serde_json::Value::Number(n) => n.as_i64(),
        serde_json::Value::Bool(b) => Some(i64::from(*b)),
        serde_json::Value::String(s) => s.parse().ok(),
        _ => None,
    })
}

fn u64_field(row: &serde_json::Value, key: &str) -> u64 {
    i64_field(row, key).map_or(0, |v| u64::try_from(v).unwrap_or(0))
}

fn u32_field(row: &serde_json::Value, key: &str) -> u32 {
    i64_field(row, key).map_or(0, |v| u32::try_from(v).unwrap_or(u32::MAX))
}

fn bool_field(row: &serde_json::Value, key: &str) -> bool {
    match row.get(key) {
        Some(serde_json::Value::Bool(b)) => *b,
        Some(serde_json::Value::Number(n)) => n.as_i64().is_some_and(|v| v != 0),
        Some(serde_json::Value::String(s)) => {
            matches!(s.as_str(), "1" | "true" | "TRUE" | "True" | "t")
        }
        _ => false,
    }
}

/// Parse an RFC3339 / SQL-ish timestamp string into UTC.
fn parse_ts(value: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(value) {
        return Some(dt.with_timezone(&Utc));
    }
    for format in ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S%.f"] {
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(value, format) {
            return Some(DateTime::from_naive_utc_and_offset(naive, Utc));
        }
    }
    None
}

fn ts_field(row: &serde_json::Value, key: &str) -> Option<DateTime<Utc>> {
    str_field(row, key).as_deref().and_then(parse_ts)
}

/// Human-readable age ("3m", "2h", "5d") for a timestamp, relative to `now`.
fn age_string(ts: Option<DateTime<Utc>>, now: DateTime<Utc>) -> String {
    let Some(ts) = ts else {
        return "-".to_string();
    };
    let secs = (now - ts).num_seconds();
    if secs < 0 {
        return "now".to_string();
    }
    if secs < 60 {
        return format!("{secs}s");
    }
    let minutes = secs / 60;
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h");
    }
    format!("{}d", hours / 24)
}

fn parse_alert_severity(raw: &str) -> Severity {
    match raw.to_ascii_lowercase().as_str() {
        "critical" | "crit" | "fatal" => Severity::Critical,
        "high" | "error" => Severity::High,
        "info" | "notice" => Severity::Info,
        "low" | "debug" => Severity::Low,
        _ => Severity::Warning,
    }
}

fn parse_event_severity(raw: &str) -> EventSeverity {
    match raw.to_ascii_lowercase().as_str() {
        "critical" | "crit" | "fatal" | "block" | "blocked" | "deny" | "denied" => {
            EventSeverity::Critical
        }
        "high" | "error" | "warn" | "warning" => EventSeverity::High,
        "low" => EventSeverity::Low,
        "info" | "allow" | "allowed" => EventSeverity::Info,
        _ => EventSeverity::Medium,
    }
}

/// Parse the `tags` column, stored as a JSON array string.
fn parse_tags(row: &serde_json::Value) -> Vec<String> {
    let Some(raw) = str_field(row, "tags") else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<String>>(&raw).unwrap_or_default()
}

// ==========================================================================
// Overview
// ==========================================================================

/// Load the overview screen from the store.
///
/// Fleet counts and health come from [`vc_query::QueryBuilder::fleet_overview`];
/// per-machine CPU/memory come from the newest `sys_samples` row per machine;
/// alerts come from `alert_history`; repos from `repo_status_snapshots`.
///
/// # Errors
///
/// Returns [`TuiError`] if any underlying query fails.
pub fn load_overview(store: &VcStore) -> Result<OverviewData, TuiError> {
    let query = QueryBuilder::new(store);
    let fleet = query.fleet_overview()?;

    // Latest health score per machine (worst first).
    let health = query.list_health_summaries()?;

    // Newest system sample per machine.
    let samples_sql = "SELECT s.machine_id, s.cpu_total, s.mem_used_bytes, s.mem_total_bytes \
         FROM sys_samples s \
         INNER JOIN ( \
             SELECT machine_id, MAX(collected_at) AS max_ts \
             FROM sys_samples GROUP BY machine_id \
         ) latest ON s.machine_id = latest.machine_id AND s.collected_at = latest.max_ts";
    let samples = store.query_json(samples_sql)?;

    let machines = query.machines()?;
    let machine_rows: Vec<MachineStatus> = machines
        .iter()
        .map(|row| {
            let id = string_or_default(row, "machine_id");
            let sample = samples
                .iter()
                .find(|s| str_field(s, "machine_id").as_deref() == Some(id.as_str()));
            let cpu_pct = sample.and_then(|s| f64_field(s, "cpu_total"));
            let mem_pct = sample.and_then(|s| {
                let used = f64_field(s, "mem_used_bytes")?;
                let total = f64_field(s, "mem_total_bytes")?;
                if total > 0.0 {
                    Some(used / total * 100.0)
                } else {
                    None
                }
            });
            let health_score = health
                .iter()
                .find(|h| str_field(h, "machine_id").as_deref() == Some(id.as_str()))
                .and_then(|h| f64_field(h, "overall_score"))
                .unwrap_or(1.0);

            MachineStatus {
                online: str_field(row, "status").as_deref() == Some("online"),
                hostname: str_field(row, "hostname").unwrap_or_else(|| id.clone()),
                id,
                cpu_pct,
                mem_pct,
                health_score,
            }
        })
        .collect();

    let alerts = query
        .recent_alerts(OVERVIEW_ALERT_LIMIT)?
        .iter()
        .map(|row| AlertSummary {
            severity: str_field(row, "severity").unwrap_or_else(|| "warning".to_string()),
            title: string_or_default(row, "title"),
            machine_id: str_field(row, "machine_id"),
        })
        .collect();

    Ok(OverviewData {
        // `fleet_health` and `health_score` are 0.0..=1.0 scores, matching
        // `Theme::health_color` thresholds.
        fleet_health: fleet.fleet_health_score,
        machines: machine_rows,
        alerts,
        repos: load_repo_status(store)?,
        refresh_age_secs: 0,
    })
}

/// Latest git status snapshot per repo, joined to the repo registry for names.
fn load_repo_status(store: &VcStore) -> Result<Vec<RepoStatus>, TuiError> {
    let sql = "SELECT COALESCE(r.name, s.repo_id) AS name, s.branch, s.dirty, \
               s.ahead, s.behind, s.modified_count \
               FROM repo_status_snapshots s \
               INNER JOIN ( \
                   SELECT machine_id, repo_id, MAX(collected_at) AS max_ts \
                   FROM repo_status_snapshots GROUP BY machine_id, repo_id \
               ) latest ON s.machine_id = latest.machine_id \
                       AND s.repo_id = latest.repo_id \
                       AND s.collected_at = latest.max_ts \
               LEFT JOIN repos r ON r.machine_id = s.machine_id AND r.repo_id = s.repo_id \
               ORDER BY name";
    let rows = store.query_json(sql)?;
    Ok(rows
        .iter()
        .map(|row| RepoStatus {
            name: string_or_default(row, "name"),
            branch: str_field(row, "branch").unwrap_or_else(|| "-".to_string()),
            is_dirty: bool_field(row, "dirty"),
            ahead: u32_field(row, "ahead"),
            behind: u32_field(row, "behind"),
            modified_count: u32_field(row, "modified_count"),
        })
        .collect())
}

// ==========================================================================
// Machines
// ==========================================================================

/// Load the machines screen from the `machines` registry plus `machine_tools` counts.
///
/// # Errors
///
/// Returns [`TuiError`] if any underlying query fails.
pub fn load_machines(store: &VcStore) -> Result<MachinesData, TuiError> {
    let sql = "SELECT m.machine_id, m.hostname, m.display_name, m.status, m.tags, \
               m.is_local, m.enabled, \
               CAST(m.last_seen_at AS TEXT) AS last_seen_at, \
               CAST(m.last_probe_at AS TEXT) AS last_probe_at, \
               (SELECT COUNT(*) FROM machine_tools t \
                WHERE t.machine_id = m.machine_id AND t.is_available = 1) AS tool_count \
               FROM machines m ORDER BY m.hostname";
    let rows = store.query_json(sql)?;

    let machines: Vec<MachineRow> = rows
        .iter()
        .map(|row| MachineRow {
            machine_id: string_or_default(row, "machine_id"),
            hostname: string_or_default(row, "hostname"),
            display_name: str_field(row, "display_name"),
            status: match str_field(row, "status").as_deref() {
                Some("online") => MachineOnlineStatus::Online,
                Some("offline") => MachineOnlineStatus::Offline,
                _ => MachineOnlineStatus::Unknown,
            },
            tool_count: usize::try_from(i64_field(row, "tool_count").unwrap_or(0)).unwrap_or(0),
            last_seen: ts_field(row, "last_seen_at"),
            last_probe: ts_field(row, "last_probe_at"),
            tags: parse_tags(row),
            is_local: bool_field(row, "is_local"),
            enabled: bool_field(row, "enabled"),
        })
        .collect();

    Ok(MachinesData {
        machines,
        // Machine detail (tools, system stats, collection history) is only
        // fetched when a row is selected; no selection handling exists yet.
        selected_detail: None,
        refresh_age_secs: 0,
        ..MachinesData::default()
    })
}

// ==========================================================================
// Alerts
// ==========================================================================

/// Load the alerts screen from `alert_history` and `alert_rules`.
///
/// # Errors
///
/// Returns [`TuiError`] if any underlying query fails.
pub fn load_alerts(store: &VcStore) -> Result<AlertsData, TuiError> {
    let now = Utc::now();

    let history_sql = format!(
        "SELECT id, rule_id, severity, title, message, machine_id, acknowledged, context_json, \
         CAST(fired_at AS TEXT) AS fired_at, CAST(resolved_at AS TEXT) AS resolved_at \
         FROM alert_history ORDER BY fired_at DESC LIMIT {ALERT_HISTORY_LIMIT}"
    );
    let history = store.query_json(&history_sql)?;

    let mut active_alerts = Vec::new();
    let mut recent_alerts = Vec::new();
    let mut alerts_24h = 0_u32;
    let mut critical_active = 0_u32;

    for row in &history {
        let fired_at = ts_field(row, "fired_at");
        let severity = parse_alert_severity(
            str_field(row, "severity")
                .unwrap_or_else(|| "warning".to_string())
                .as_str(),
        );
        let resolved_at = str_field(row, "resolved_at");
        let info = AlertInfo {
            id: u64_field(row, "id"),
            rule_id: string_or_default(row, "rule_id"),
            title: string_or_default(row, "title"),
            message: string_or_default(row, "message"),
            severity,
            fired_at: str_field(row, "fired_at").unwrap_or_default(),
            age: age_string(fired_at, now),
            machine_id: str_field(row, "machine_id"),
            acknowledged: bool_field(row, "acknowledged"),
            resolved_at: resolved_at.clone(),
            context: str_field(row, "context_json"),
        };

        if fired_at.is_some_and(|ts| (now - ts).num_hours() < 24) {
            alerts_24h = alerts_24h.saturating_add(1);
        }

        if resolved_at.is_none() {
            if severity == Severity::Critical {
                critical_active = critical_active.saturating_add(1);
            }
            active_alerts.push(info);
        } else {
            recent_alerts.push(info);
        }
    }

    let rules_sql = "SELECT rule_id, name, description, severity, enabled, \
                     check_interval_secs, cooldown_secs \
                     FROM alert_rules ORDER BY rule_id";
    let rule_rows = store.query_json(rules_sql)?;

    let mut rules_enabled = 0_u32;
    let rules: Vec<AlertRuleInfo> = rule_rows
        .iter()
        .map(|row| {
            let enabled = bool_field(row, "enabled");
            if enabled {
                rules_enabled = rules_enabled.saturating_add(1);
            }
            let rule_id = string_or_default(row, "rule_id");
            let fired_24h = history
                .iter()
                .filter(|alert| {
                    str_field(alert, "rule_id").as_deref() == Some(rule_id.as_str())
                        && ts_field(alert, "fired_at").is_some_and(|ts| (now - ts).num_hours() < 24)
                })
                .count();

            AlertRuleInfo {
                name: str_field(row, "name").unwrap_or_else(|| rule_id.clone()),
                rule_id,
                description: string_or_default(row, "description"),
                severity: parse_alert_severity(
                    str_field(row, "severity")
                        .unwrap_or_else(|| "warning".to_string())
                        .as_str(),
                ),
                enabled,
                // `alert_rules` has no mute column; mutes are not persisted yet.
                muted: false,
                check_interval: u32_field(row, "check_interval_secs"),
                cooldown: u32_field(row, "cooldown_secs"),
                fired_24h: u32::try_from(fired_24h).unwrap_or(u32::MAX),
            }
        })
        .collect();

    let stats = AlertStats {
        rules_enabled,
        // No mute or provenance columns exist in `alert_rules` yet.
        rules_muted: 0,
        rules_custom: 0,
        alerts_24h,
        critical_active,
    };

    Ok(AlertsData {
        active_alerts,
        recent_alerts,
        rules,
        stats,
        ..AlertsData::default()
    })
}

// ==========================================================================
// Sessions
// ==========================================================================

/// Load the sessions screen from `agent_sessions`.
///
/// # Errors
///
/// Returns [`TuiError`] if the query fails.
pub fn load_sessions(store: &VcStore) -> Result<SessionsData, TuiError> {
    let now = Utc::now();
    let sql = format!(
        "SELECT session_id, program, model, repo_path, turn_count, token_count, cost_estimate, \
         CAST(started_at AS TEXT) AS started_at, \
         CAST(ended_at AS TEXT) AS ended_at, \
         CAST(collected_at AS TEXT) AS collected_at \
         FROM agent_sessions \
         ORDER BY started_at DESC LIMIT {SESSION_LIMIT}"
    );
    let rows = store.query_json(&sql)?;

    let sessions: Vec<SessionInfo> = rows
        .iter()
        .map(|row| {
            let started = ts_field(row, "started_at");
            let ended = ts_field(row, "ended_at");
            let is_active = ended.is_none();
            let duration_mins = started.map_or(0, |start| {
                let end = ended.unwrap_or(now);
                u32::try_from((end - start).num_minutes().max(0)).unwrap_or(u32::MAX)
            });
            let last_activity_ts = ended.or_else(|| ts_field(row, "collected_at")).or(started);

            SessionInfo {
                id: string_or_default(row, "session_id"),
                project: str_field(row, "repo_path").unwrap_or_else(|| "-".to_string()),
                model: str_field(row, "model").unwrap_or_else(|| "-".to_string()),
                agent: str_field(row, "program").unwrap_or_else(|| "-".to_string()),
                started_at: str_field(row, "started_at").unwrap_or_default(),
                duration_mins,
                tokens: u64_field(row, "token_count"),
                cost: f64_field(row, "cost_estimate").unwrap_or(0.0),
                is_active,
                last_activity: age_string(last_activity_ts, now),
            }
        })
        .collect();

    Ok(SessionsData {
        sessions,
        ..SessionsData::default()
    })
}

// ==========================================================================
// Events
// ==========================================================================

/// Load the events screen: DCG denials, network (RANO) events, process findings.
///
/// # Errors
///
/// Returns [`TuiError`] if any underlying query fails.
pub fn load_events(store: &VcStore) -> Result<EventsData, TuiError> {
    let now = Utc::now();

    let dcg_sql = format!(
        "SELECT machine_id, command, severity, decision, reason, \"user\", pwd, \
         CAST(ts AS TEXT) AS ts \
         FROM dcg_events ORDER BY ts DESC LIMIT {EVENT_LIMIT}"
    );
    let dcg_rows = store.query_json(&dcg_sql)?;
    let dcg_events: Vec<DcgEvent> = dcg_rows
        .iter()
        .enumerate()
        .map(|(idx, row)| {
            let ts = ts_field(row, "ts");
            let severity = parse_event_severity(
                str_field(row, "severity")
                    .or_else(|| str_field(row, "decision"))
                    .unwrap_or_default()
                    .as_str(),
            );
            DcgEvent {
                id: u64::try_from(idx).unwrap_or(0),
                machine_id: string_or_default(row, "machine_id"),
                command: string_or_default(row, "command"),
                reason: str_field(row, "reason")
                    .or_else(|| str_field(row, "decision"))
                    .unwrap_or_default(),
                severity,
                timestamp: str_field(row, "ts").unwrap_or_default(),
                age: age_string(ts, now),
                source: str_field(row, "user"),
            }
        })
        .collect();

    let net_sql = format!(
        "SELECT machine_id, event_type, direction, remote_ip, remote_port, protocol, \
         provider, is_known, CAST(ts AS TEXT) AS ts \
         FROM net_events ORDER BY ts DESC LIMIT {EVENT_LIMIT}"
    );
    let net_rows = store.query_json(&net_sql)?;
    let rano_events: Vec<RanoEvent> = net_rows
        .iter()
        .enumerate()
        .map(|(idx, row)| {
            let ts = ts_field(row, "ts");
            let is_known = bool_field(row, "is_known");
            let event_type = match str_field(row, "event_type").as_deref() {
                Some("auth_loop") => RanoEventType::AuthLoop,
                Some("high_volume") => RanoEventType::HighVolume,
                Some("unusual_port") => RanoEventType::UnusualPort,
                Some("blocked") => RanoEventType::Blocked,
                _ => RanoEventType::UnknownProvider,
            };
            let remote_host = match (str_field(row, "remote_ip"), i64_field(row, "remote_port")) {
                (Some(ip), Some(port)) => format!("{ip}:{port}"),
                (Some(ip), None) => ip,
                _ => "-".to_string(),
            };
            RanoEvent {
                id: u64::try_from(idx).unwrap_or(0),
                machine_id: string_or_default(row, "machine_id"),
                event_type,
                remote_host,
                // `net_events` records the connection, not the owning process.
                process: str_field(row, "provider").unwrap_or_else(|| "-".to_string()),
                pid: 0,
                connection_count: 1,
                timestamp: str_field(row, "ts").unwrap_or_default(),
                age: age_string(ts, now),
                severity: if is_known {
                    EventSeverity::Info
                } else {
                    EventSeverity::Medium
                },
                details: str_field(row, "protocol").map(|proto| {
                    let direction = str_field(row, "direction").unwrap_or_else(|| "-".to_string());
                    format!("{proto} {direction}")
                }),
            }
        })
        .collect();

    // Newest process-triage snapshot per machine; findings are the processes the
    // triage collector already categorised (`category` column), not re-derived here.
    let pt_sql = format!(
        "SELECT p.machine_id, p.pid, p.comm, p.category, p.cpu_pct, p.mem_bytes, \
         CAST(p.collected_at AS TEXT) AS collected_at \
         FROM process_triage p \
         INNER JOIN ( \
             SELECT machine_id, MAX(collected_at) AS max_ts \
             FROM process_triage GROUP BY machine_id \
         ) latest ON p.machine_id = latest.machine_id AND p.collected_at = latest.max_ts \
         WHERE p.category IS NOT NULL AND p.category <> 'normal' \
         ORDER BY p.cpu_pct DESC LIMIT {EVENT_LIMIT}"
    );
    let pt_rows = store.query_json(&pt_sql)?;
    let pt_findings: Vec<PtFinding> = pt_rows
        .iter()
        .enumerate()
        .map(|(idx, row)| {
            let ts = ts_field(row, "collected_at");
            let category = str_field(row, "category").unwrap_or_default();
            let finding_type = match category.as_str() {
                "stuck" | "stuck_agent" => PtFindingType::StuckAgent,
                "runaway" | "high_cpu" => PtFindingType::Runaway,
                "memory_hog" | "high_memory" => PtFindingType::MemoryHog,
                "long_build" | "build" => PtFindingType::LongBuild,
                "orphan" | "orphaned" => PtFindingType::Orphaned,
                _ => PtFindingType::Zombie,
            };
            let cpu = f64_field(row, "cpu_pct");
            let mem_mb = f64_field(row, "mem_bytes").map(|bytes| bytes / 1_048_576.0);
            PtFinding {
                id: u64::try_from(idx).unwrap_or(0),
                machine_id: string_or_default(row, "machine_id"),
                finding_type,
                process_name: str_field(row, "comm").unwrap_or_else(|| "-".to_string()),
                pid: u32_field(row, "pid"),
                details: category,
                severity: match finding_type {
                    PtFindingType::Runaway | PtFindingType::Zombie => EventSeverity::High,
                    PtFindingType::MemoryHog | PtFindingType::StuckAgent => EventSeverity::Medium,
                    PtFindingType::LongBuild | PtFindingType::Orphaned => EventSeverity::Low,
                },
                timestamp: str_field(row, "collected_at").unwrap_or_default(),
                age: age_string(ts, now),
                metric_value: match (cpu, mem_mb) {
                    (Some(cpu), Some(mem)) => Some(format!("{cpu:.1}% cpu / {mem:.0} MB")),
                    (Some(cpu), None) => Some(format!("{cpu:.1}% cpu")),
                    (None, Some(mem)) => Some(format!("{mem:.0} MB")),
                    (None, None) => None,
                },
            }
        })
        .collect();

    let dcg_critical = u32::try_from(
        dcg_events
            .iter()
            .filter(|e| e.severity == EventSeverity::Critical)
            .count(),
    )
    .unwrap_or(u32::MAX);

    let mut machines_affected: Vec<&str> = dcg_events
        .iter()
        .map(|e| e.machine_id.as_str())
        .chain(rano_events.iter().map(|e| e.machine_id.as_str()))
        .chain(pt_findings.iter().map(|e| e.machine_id.as_str()))
        .filter(|id| !id.is_empty())
        .collect();
    machines_affected.sort_unstable();
    machines_affected.dedup();

    let stats = EventStats {
        dcg_total: u32::try_from(dcg_events.len()).unwrap_or(u32::MAX),
        dcg_critical,
        rano_total: u32::try_from(rano_events.len()).unwrap_or(u32::MAX),
        pt_total: u32::try_from(pt_findings.len()).unwrap_or(u32::MAX),
        machines_affected: u32::try_from(machines_affected.len()).unwrap_or(u32::MAX),
    };

    Ok(EventsData {
        dcg_events,
        rano_events,
        pt_findings,
        stats,
        ..EventsData::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> VcStore {
        VcStore::open_memory().expect("in-memory store")
    }

    #[test]
    fn parse_ts_accepts_rfc3339_and_sql_timestamps() {
        assert!(parse_ts("2026-07-11T10:00:00Z").is_some());
        assert!(parse_ts("2026-07-11 10:00:00").is_some());
        assert!(parse_ts("not a timestamp").is_none());
    }

    #[test]
    fn age_string_formats_buckets() {
        let now = Utc::now();
        assert_eq!(age_string(None, now), "-");
        assert_eq!(
            age_string(Some(now - chrono::Duration::seconds(5)), now),
            "5s"
        );
        assert_eq!(
            age_string(Some(now - chrono::Duration::minutes(5)), now),
            "5m"
        );
        assert_eq!(
            age_string(Some(now - chrono::Duration::hours(5)), now),
            "5h"
        );
        assert_eq!(age_string(Some(now - chrono::Duration::days(5)), now), "5d");
    }

    #[test]
    fn loaders_run_against_empty_store() {
        let store = store();
        let overview = load_overview(&store).expect("overview");
        assert!(overview.machines.is_empty());
        assert!(load_machines(&store).expect("machines").machines.is_empty());
        assert!(
            load_alerts(&store)
                .expect("alerts")
                .active_alerts
                .is_empty()
        );
        assert!(load_sessions(&store).expect("sessions").sessions.is_empty());
        assert!(load_events(&store).expect("events").dcg_events.is_empty());
    }

    #[test]
    fn load_machines_reads_registry_rows() {
        let store = store();
        store
            .execute_simple(
                "INSERT INTO machines (machine_id, hostname, status, is_local, enabled, tags) \
                 VALUES ('m1', 'alpha', 'online', 1, 1, '[\"prod\"]')",
            )
            .expect("insert machine");

        let data = load_machines(&store).expect("machines");
        assert_eq!(data.machines.len(), 1);
        let row = &data.machines[0];
        assert_eq!(row.machine_id, "m1");
        assert_eq!(row.hostname, "alpha");
        assert_eq!(row.status, MachineOnlineStatus::Online);
        assert_eq!(row.tags, vec!["prod".to_string()]);
        assert!(row.is_local);
        assert!(row.enabled);
    }

    #[test]
    fn load_alerts_splits_active_and_resolved() {
        let store = store();
        store
            .execute_simple(
                "INSERT INTO alert_history (id, rule_id, fired_at, resolved_at, severity, title, message, machine_id, acknowledged) \
                 VALUES (1, 'cpu', '2026-07-11T10:00:00Z', NULL, 'critical', 'CPU high', 'cpu over 95%', 'm1', 0), \
                        (2, 'cpu', '2026-07-10T10:00:00Z', '2026-07-10T11:00:00Z', 'warning', 'CPU warn', 'cpu over 80%', 'm1', 1)",
            )
            .expect("insert alerts");

        let data = load_alerts(&store).expect("alerts");
        assert_eq!(data.active_alerts.len(), 1);
        assert_eq!(data.recent_alerts.len(), 1);
        assert_eq!(data.active_alerts[0].severity, Severity::Critical);
        assert_eq!(data.stats.critical_active, 1);
    }

    #[test]
    fn load_sessions_marks_active_sessions() {
        let store = store();
        store
            .execute_simple(
                "INSERT INTO agent_sessions (machine_id, collected_at, session_id, program, model, repo_path, started_at, ended_at, turn_count, token_count, cost_estimate) \
                 VALUES ('m1', '2026-07-11T10:00:00Z', 's1', 'claude-code', 'opus', '/repo/a', '2026-07-11T09:00:00Z', NULL, 3, 1500, 0.25)",
            )
            .expect("insert session");

        let data = load_sessions(&store).expect("sessions");
        assert_eq!(data.sessions.len(), 1);
        let session = &data.sessions[0];
        assert_eq!(session.id, "s1");
        assert_eq!(session.agent, "claude-code");
        assert_eq!(session.project, "/repo/a");
        assert_eq!(session.tokens, 1500);
        assert!(session.is_active);
    }

    #[test]
    fn load_events_reads_dcg_rows() {
        let store = store();
        store
            .execute_simple(
                "INSERT INTO dcg_events (machine_id, ts, command, severity, decision, reason, \"user\", pwd) \
                 VALUES ('m1', '2026-07-11T10:00:00Z', 'rm -rf /', 'critical', 'blocked', 'destructive', 'root', '/')",
            )
            .expect("insert dcg event");

        let data = load_events(&store).expect("events");
        assert_eq!(data.dcg_events.len(), 1);
        assert_eq!(data.dcg_events[0].command, "rm -rf /");
        assert_eq!(data.dcg_events[0].severity, EventSeverity::Critical);
        assert_eq!(data.stats.dcg_total, 1);
        assert_eq!(data.stats.machines_affected, 1);
    }
}
