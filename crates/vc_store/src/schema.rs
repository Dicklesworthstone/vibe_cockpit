//! Schema definitions and constants

/// Table names
pub mod tables {
    pub const MACHINES: &str = "machines";
    pub const MACHINE_TOOLS: &str = "machine_tools";
    pub const COLLECTOR_STATUS: &str = "collector_status";
    pub const INGESTION_CURSORS: &str = "ingestion_cursors";
    pub const SYS_FALLBACK_SAMPLES: &str = "sys_fallback_samples";
    pub const SYS_SAMPLES: &str = "sys_samples";
    pub const SYS_TOP_PROCESSES: &str = "sys_top_processes";
    pub const SYS_FILESYSTEMS: &str = "sys_filesystems";
    pub const REPOS: &str = "repos";
    pub const REPO_STATUS_SNAPSHOTS: &str = "repo_status_snapshots";
    pub const ACCOUNT_USAGE_SNAPSHOTS: &str = "account_usage_snapshots";
    pub const ACCOUNT_PROFILE_SNAPSHOTS: &str = "account_profile_snapshots";
    pub const AGENT_SESSIONS: &str = "agent_sessions";
    pub const MAIL_MESSAGES: &str = "mail_messages";
    pub const MAIL_FILE_RESERVATIONS: &str = "mail_file_reservations";
    pub const NTM_SESSIONS_SNAPSHOT: &str = "ntm_sessions_snapshot";
    pub const NTM_ACTIVITY_SNAPSHOT: &str = "ntm_activity_snapshot";
    pub const NTM_AGENT_SNAPSHOT: &str = "ntm_agent_snapshot";
    pub const RCH_METRICS: &str = "rch_metrics";
    pub const RCH_COMPILATIONS: &str = "rch_compilations";
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
    pub const AGENT_DNA: &str = "agent_dna";
    pub const DNA_HISTORY: &str = "dna_history";
    pub const EXPERIMENTS: &str = "experiments";
    pub const EXPERIMENT_VARIANTS: &str = "experiment_variants";
    pub const EXPERIMENT_ASSIGNMENTS: &str = "experiment_assignments";
    pub const EXPERIMENT_OBSERVATIONS: &str = "experiment_observations";
    pub const EXPERIMENT_RESULTS: &str = "experiment_results";
    pub const COST_ATTRIBUTION_SNAPSHOT: &str = "cost_attribution_snapshot";
    pub const COST_DAILY_SUMMARY: &str = "cost_daily_summary";
    pub const COST_ANOMALIES: &str = "cost_anomalies";
    pub const PROVIDER_PRICING: &str = "provider_pricing";
    pub const AFSC_STATUS_SNAPSHOT: &str = "afsc_status_snapshot";
    pub const AFSC_RUN_FACTS: &str = "afsc_run_facts";
    pub const AFSC_EVENT_LOGS: &str = "afsc_event_logs";
    pub const AFSC_ERROR_CLUSTERS: &str = "afsc_error_clusters";
    pub const CLOUD_BENCH_RAW: &str = "cloud_bench_raw";
    pub const CLOUD_BENCH_OVERALL: &str = "cloud_bench_overall";
    pub const CLOUD_BENCH_HISTORY: &str = "cloud_bench_history";
    pub const BEADS_TRIAGE_SNAPSHOTS: &str = "beads_triage_snapshots";
    pub const BEADS_GRAPH_METRICS: &str = "beads_graph_metrics";
    pub const PT_PROCESSES: &str = "pt_processes";
    pub const PT_SNAPSHOTS: &str = "pt_snapshots";
    pub const GH_REPO_ISSUE_PR_SNAPSHOT: &str = "gh_repo_issue_pr_snapshot";
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
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            CollectorStatus::Ok => "ok",
            CollectorStatus::Failed => "failed",
            CollectorStatus::Timeout => "timeout",
            CollectorStatus::Skipped => "skipped",
        }
    }
}
