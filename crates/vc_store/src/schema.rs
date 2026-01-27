//! Schema definitions and constants

/// Table names
pub mod tables {
    pub const MACHINES: &str = "machines";
    pub const COLLECTOR_STATUS: &str = "collector_status";
    pub const INGESTION_CURSORS: &str = "ingestion_cursors";
    pub const SYS_SAMPLES: &str = "sys_samples";
    pub const SYS_TOP_PROCESSES: &str = "sys_top_processes";
    pub const REPO_STATUS_SNAPSHOTS: &str = "repo_status_snapshots";
    pub const ACCOUNT_USAGE_SNAPSHOTS: &str = "account_usage_snapshots";
    pub const ACCOUNT_PROFILE_SNAPSHOTS: &str = "account_profile_snapshots";
    pub const AGENT_SESSIONS: &str = "agent_sessions";
    pub const MAIL_MESSAGES: &str = "mail_messages";
    pub const NTM_SESSIONS_SNAPSHOT: &str = "ntm_sessions_snapshot";
    pub const RCH_METRICS: &str = "rch_metrics";
    pub const NET_EVENTS: &str = "net_events";
    pub const DCG_EVENTS: &str = "dcg_events";
    pub const PROCESS_TRIAGE: &str = "process_triage";
    pub const BEADS_SNAPSHOT: &str = "beads_snapshot";
    pub const ALERT_RULES: &str = "alert_rules";
    pub const ALERT_HISTORY: &str = "alert_history";
    pub const HEALTH_FACTORS: &str = "health_factors";
    pub const HEALTH_SUMMARY: &str = "health_summary";
    pub const AUDIT_EVENTS: &str = "audit_events";
    pub const PREDICTIONS: &str = "predictions";
    pub const INCIDENTS: &str = "incidents";
    pub const INCIDENT_TIMELINE_EVENTS: &str = "incident_timeline_events";
    pub const GUARDIAN_PLAYBOOKS: &str = "guardian_playbooks";
    pub const GUARDIAN_RUNS: &str = "guardian_runs";
    pub const RETENTION_POLICIES: &str = "retention_policies";
}

/// Common column names
pub mod columns {
    pub const MACHINE_ID: &str = "machine_id";
    pub const COLLECTED_AT: &str = "collected_at";
    pub const CREATED_AT: &str = "created_at";
    pub const UPDATED_AT: &str = "updated_at";
    pub const RAW_JSON: &str = "raw_json";
}

/// Collector status values
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectorStatus {
    Ok,
    Failed,
    Timeout,
    Skipped,
}

impl CollectorStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            CollectorStatus::Ok => "ok",
            CollectorStatus::Failed => "failed",
            CollectorStatus::Timeout => "timeout",
            CollectorStatus::Skipped => "skipped",
        }
    }
}
