-- Initial schema for Vibe Cockpit
-- Migration 001: Core tables

-- Machine inventory
CREATE TABLE IF NOT EXISTS machines (
    machine_id TEXT PRIMARY KEY,
    hostname TEXT NOT NULL,
    is_local BOOLEAN DEFAULT FALSE,
    ssh_host TEXT,
    ssh_user TEXT,
    enabled BOOLEAN DEFAULT TRUE,
    last_seen_at TIMESTAMP,
    tags TEXT[], -- Array of tags
    metadata_json TEXT,
    created_at TIMESTAMP DEFAULT current_timestamp,
    updated_at TIMESTAMP DEFAULT current_timestamp
);

-- Collector status tracking
CREATE TABLE IF NOT EXISTS collector_status (
    machine_id TEXT,
    collector_name TEXT,
    last_run_at TIMESTAMP,
    last_success_at TIMESTAMP,
    status TEXT, -- ok, failed, timeout, skipped
    rows_collected INTEGER,
    duration_ms BIGINT,
    error_message TEXT,
    cursor_state TEXT,
    PRIMARY KEY (machine_id, collector_name)
);

-- Fallback system probe samples (always-on baseline)
CREATE TABLE IF NOT EXISTS sys_fallback_samples (
    machine_id TEXT,
    collected_at TIMESTAMP,
    uptime_seconds BIGINT,
    load1 REAL,
    load5 REAL,
    load15 REAL,
    mem_total_bytes BIGINT,
    mem_available_bytes BIGINT,
    mem_used_bytes BIGINT,
    swap_total_bytes BIGINT,
    swap_used_bytes BIGINT,
    disk_usage_json TEXT, -- [{mount, total, used, avail, pct}]
    raw_output TEXT,
    PRIMARY KEY (machine_id, collected_at)
);

-- System metrics samples
CREATE TABLE IF NOT EXISTS sys_samples (
    machine_id TEXT,
    collected_at TIMESTAMP,
    cpu_total REAL,
    load1 REAL,
    load5 REAL,
    load15 REAL,
    mem_used_bytes BIGINT,
    mem_total_bytes BIGINT,
    mem_available_bytes BIGINT,
    swap_used_bytes BIGINT,
    swap_total_bytes BIGINT,
    disk_read_mbps REAL,
    disk_write_mbps REAL,
    net_rx_mbps REAL,
    net_tx_mbps REAL,
    core_count INTEGER,
    raw_json TEXT,
    PRIMARY KEY (machine_id, collected_at)
);

-- Top processes snapshot
CREATE TABLE IF NOT EXISTS sys_top_processes (
    machine_id TEXT,
    collected_at TIMESTAMP,
    pid INTEGER,
    comm TEXT,
    cpu_pct REAL,
    mem_bytes BIGINT,
    fd_count INTEGER,
    io_read_bytes BIGINT,
    io_write_bytes BIGINT,
    raw_json TEXT
);

-- Filesystems snapshot
CREATE TABLE IF NOT EXISTS sys_filesystems (
    machine_id TEXT,
    collected_at TIMESTAMP,
    mount TEXT,
    total_bytes BIGINT,
    used_bytes BIGINT,
    usage_pct REAL,
    PRIMARY KEY (machine_id, collected_at, mount)
);

-- Repositories inventory
CREATE TABLE IF NOT EXISTS repos (
    machine_id TEXT,
    repo_id TEXT,
    path TEXT,
    url TEXT,
    name TEXT,
    PRIMARY KEY (machine_id, repo_id)
);

-- Repository status snapshots (from ru)
CREATE TABLE IF NOT EXISTS repo_status_snapshots (
    machine_id TEXT,
    collected_at TIMESTAMP,
    repo_id TEXT,
    branch TEXT,
    dirty BOOLEAN,
    ahead INTEGER,
    behind INTEGER,
    modified_count INTEGER,
    untracked_count INTEGER,
    raw_json TEXT,
    PRIMARY KEY (machine_id, collected_at, repo_id)
);

-- Account usage snapshots (from caut)
CREATE TABLE IF NOT EXISTS account_usage_snapshots (
    machine_id TEXT,
    collected_at TIMESTAMP,
    provider TEXT,
    account_id TEXT,
    usage_pct REAL,
    tokens_used BIGINT,
    tokens_limit BIGINT,
    resets_at TIMESTAMP,
    cost_estimate REAL,
    raw_json TEXT,
    PRIMARY KEY (machine_id, collected_at, provider, account_id)
);

-- Account profile snapshots (from caam)
CREATE TABLE IF NOT EXISTS account_profile_snapshots (
    machine_id TEXT,
    collected_at TIMESTAMP,
    provider TEXT,
    account_id TEXT,
    email TEXT,
    plan_type TEXT,
    is_active BOOLEAN,
    is_current BOOLEAN,
    priority INTEGER,
    raw_json TEXT,
    PRIMARY KEY (machine_id, collected_at, provider, account_id)
);

-- Agent sessions (from cass)
CREATE TABLE IF NOT EXISTS agent_sessions (
    machine_id TEXT,
    collected_at TIMESTAMP,
    session_id TEXT,
    program TEXT,
    model TEXT,
    repo_path TEXT,
    started_at TIMESTAMP,
    ended_at TIMESTAMP,
    turn_count INTEGER,
    token_count BIGINT,
    cost_estimate REAL,
    raw_json TEXT,
    PRIMARY KEY (machine_id, session_id)
);

-- CASS index status (from cass health)
CREATE TABLE IF NOT EXISTS cass_index_status (
    machine_id TEXT,
    collected_at TIMESTAMP,
    state TEXT,
    total_sessions INTEGER,
    last_index_at TIMESTAMP,
    index_size_bytes BIGINT,
    freshness_seconds BIGINT,
    raw_json TEXT,
    PRIMARY KEY (machine_id, collected_at)
);

-- CASS statistics snapshots (from cass stats)
CREATE TABLE IF NOT EXISTS cass_stats_snapshots (
    machine_id TEXT,
    collected_at TIMESTAMP,
    metric_name TEXT,
    metric_value REAL,
    dimensions_json TEXT,
    raw_json TEXT
);

CREATE INDEX IF NOT EXISTS idx_cass_stats_ts ON cass_stats_snapshots(collected_at);
CREATE INDEX IF NOT EXISTS idx_cass_stats_metric ON cass_stats_snapshots(metric_name);

-- Mail messages (from mcp_agent_mail)
CREATE TABLE IF NOT EXISTS mail_messages (
    machine_id TEXT,
    collected_at TIMESTAMP,
    message_id INTEGER,
    thread_id TEXT,
    subject TEXT,
    sender TEXT,
    recipients TEXT[],
    importance TEXT,
    ack_required BOOLEAN,
    read_at TIMESTAMP,
    acked_at TIMESTAMP,
    created_at TIMESTAMP,
    raw_json TEXT,
    PRIMARY KEY (machine_id, message_id)
);

-- Mail file reservations (from mcp_agent_mail)
CREATE TABLE IF NOT EXISTS mail_file_reservations (
    machine_id TEXT,
    collected_at TIMESTAMP,
    reservation_id INTEGER,
    project_id TEXT,
    path_pattern TEXT,
    holder TEXT,
    expires_ts TIMESTAMP,
    exclusive BOOLEAN,
    reason TEXT,
    raw_json TEXT,
    PRIMARY KEY (machine_id, collected_at, reservation_id)
);

-- NTM sessions snapshot
CREATE TABLE IF NOT EXISTS ntm_sessions_snapshot (
    machine_id TEXT,
    collected_at TIMESTAMP,
    session_name TEXT,
    work_dir TEXT,
    git_branch TEXT,
    agent_counts_json TEXT,
    panes_json TEXT,
    raw_json TEXT,
    PRIMARY KEY (machine_id, collected_at, session_name)
);

-- RCH metrics (remote compilation helper)
CREATE TABLE IF NOT EXISTS rch_metrics (
    machine_id TEXT,
    collected_at TIMESTAMP,
    queue_depth INTEGER,
    workers_active INTEGER,
    workers_total INTEGER,
    jobs_completed BIGINT,
    jobs_failed BIGINT,
    avg_job_duration_ms BIGINT,
    raw_json TEXT,
    PRIMARY KEY (machine_id, collected_at)
);

-- RCH compilations (individual compilation records)
CREATE TABLE IF NOT EXISTS rch_compilations (
    machine_id TEXT,
    collected_at TIMESTAMP,
    worker_host TEXT,
    crate_name TEXT NOT NULL,
    crate_version TEXT,
    profile TEXT,
    target_triple TEXT,
    started_at TIMESTAMP,
    duration_ms INTEGER,
    cache_hit BOOLEAN DEFAULT FALSE,
    cache_key TEXT,
    exit_code INTEGER,
    error_msg TEXT,
    cpu_time_ms INTEGER,
    peak_memory_mb INTEGER,
    raw_json TEXT
);

CREATE INDEX IF NOT EXISTS idx_rch_compilations_crate ON rch_compilations(crate_name);
CREATE INDEX IF NOT EXISTS idx_rch_compilations_ts ON rch_compilations(started_at);
CREATE INDEX IF NOT EXISTS idx_rch_compilations_worker ON rch_compilations(worker_host);

-- Network events (from rano)
CREATE TABLE IF NOT EXISTS net_events (
    machine_id TEXT,
    ts TIMESTAMP,
    event_type TEXT,
    direction TEXT,
    remote_ip TEXT,
    remote_port INTEGER,
    local_port INTEGER,
    protocol TEXT,
    provider TEXT,
    is_known BOOLEAN,
    raw_json TEXT
);

-- DCG events (dangerous command guard)
CREATE TABLE IF NOT EXISTS dcg_events (
    machine_id TEXT,
    ts TIMESTAMP,
    command TEXT,
    severity TEXT,
    decision TEXT,
    reason TEXT,
    user TEXT,
    pwd TEXT,
    raw_json TEXT
);

-- Process triage (from pt)
CREATE TABLE IF NOT EXISTS process_triage (
    machine_id TEXT,
    collected_at TIMESTAMP,
    pid INTEGER,
    comm TEXT,
    category TEXT,
    cpu_pct REAL,
    mem_bytes BIGINT,
    started_at TIMESTAMP,
    ended_at TIMESTAMP,
    parent_pid INTEGER,
    raw_json TEXT
);

-- Beads snapshot (from bv/br)
CREATE TABLE IF NOT EXISTS beads_snapshot (
    machine_id TEXT,
    collected_at TIMESTAMP,
    project_path TEXT,
    total_count INTEGER,
    open_count INTEGER,
    blocked_count INTEGER,
    actionable_count INTEGER,
    by_priority_json TEXT,
    top_picks_json TEXT,
    raw_json TEXT,
    PRIMARY KEY (machine_id, collected_at, project_path)
);

-- Alert rules
CREATE TABLE IF NOT EXISTS alert_rules (
    rule_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    severity TEXT NOT NULL,
    enabled BOOLEAN DEFAULT TRUE,
    check_interval_secs INTEGER DEFAULT 60,
    condition_type TEXT NOT NULL,
    condition_config TEXT NOT NULL,
    cooldown_secs INTEGER DEFAULT 300,
    channels TEXT[],
    created_at TIMESTAMP DEFAULT current_timestamp,
    updated_at TIMESTAMP
);

-- Alert history
CREATE TABLE IF NOT EXISTS alert_history (
    id INTEGER PRIMARY KEY,
    rule_id TEXT,
    fired_at TIMESTAMP NOT NULL,
    resolved_at TIMESTAMP,
    severity TEXT NOT NULL,
    title TEXT NOT NULL,
    message TEXT,
    context_json TEXT,
    machine_id TEXT,
    acknowledged BOOLEAN DEFAULT FALSE,
    acknowledged_by TEXT,
    acknowledged_at TIMESTAMP
);

-- Health factors
CREATE TABLE IF NOT EXISTS health_factors (
    machine_id TEXT,
    collected_at TIMESTAMP,
    factor_id TEXT,
    severity TEXT,
    score REAL,
    details_json TEXT,
    PRIMARY KEY (machine_id, collected_at, factor_id)
);

-- Health summary
CREATE TABLE IF NOT EXISTS health_summary (
    machine_id TEXT,
    collected_at TIMESTAMP,
    overall_score REAL,
    worst_factor_id TEXT,
    details_json TEXT,
    PRIMARY KEY (machine_id, collected_at)
);

-- Audit events
CREATE TABLE IF NOT EXISTS audit_events (
    id INTEGER PRIMARY KEY,
    ts TIMESTAMP DEFAULT current_timestamp,
    event_type TEXT,
    actor TEXT,
    machine_id TEXT,
    action TEXT,
    result TEXT,
    details_json TEXT
);

-- Predictions (from oracle)
CREATE TABLE IF NOT EXISTS predictions (
    id INTEGER PRIMARY KEY,
    machine_id TEXT,
    generated_at TIMESTAMP DEFAULT current_timestamp,
    prediction_type TEXT,
    horizon_mins INTEGER,
    confidence REAL,
    details_json TEXT
);

-- Incidents
CREATE TABLE IF NOT EXISTS incidents (
    incident_id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    description TEXT,
    severity TEXT NOT NULL,
    status TEXT DEFAULT 'open',
    started_at TIMESTAMP NOT NULL,
    ended_at TIMESTAMP,
    root_cause TEXT,
    resolution TEXT,
    created_at TIMESTAMP DEFAULT current_timestamp,
    updated_at TIMESTAMP
);

-- Incident timeline events
CREATE TABLE IF NOT EXISTS incident_timeline_events (
    id INTEGER PRIMARY KEY,
    incident_id TEXT,
    ts TIMESTAMP NOT NULL,
    event_type TEXT,
    source TEXT,
    description TEXT,
    details_json TEXT
);

-- Guardian playbooks
CREATE TABLE IF NOT EXISTS guardian_playbooks (
    playbook_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    trigger_condition TEXT NOT NULL,
    steps TEXT NOT NULL,
    enabled BOOLEAN DEFAULT TRUE,
    requires_approval BOOLEAN DEFAULT FALSE,
    max_runs_per_hour INTEGER DEFAULT 3,
    created_at TIMESTAMP DEFAULT current_timestamp
);

-- Guardian runs
CREATE TABLE IF NOT EXISTS guardian_runs (
    id INTEGER PRIMARY KEY,
    playbook_id TEXT,
    started_at TIMESTAMP NOT NULL,
    completed_at TIMESTAMP,
    status TEXT NOT NULL,
    trigger_context TEXT,
    steps_completed INTEGER DEFAULT 0,
    steps_total INTEGER,
    error_message TEXT,
    rollback_performed BOOLEAN DEFAULT FALSE
);

-- Retention policies
CREATE TABLE IF NOT EXISTS retention_policies (
    policy_id TEXT PRIMARY KEY,
    table_name TEXT NOT NULL,
    retention_days INTEGER NOT NULL,
    aggregate_table TEXT,
    enabled BOOLEAN DEFAULT TRUE,
    last_vacuum_at TIMESTAMP
);

-- Ingestion cursors for incremental collection
CREATE TABLE IF NOT EXISTS ingestion_cursors (
    machine_id TEXT,
    source TEXT,
    cursor_key TEXT,
    cursor_value TEXT,
    updated_at TIMESTAMP DEFAULT current_timestamp,
    PRIMARY KEY (machine_id, source, cursor_key)
);

-- Create indexes for common queries
CREATE INDEX IF NOT EXISTS idx_sys_samples_ts ON sys_samples(collected_at);
CREATE INDEX IF NOT EXISTS idx_sys_samples_machine ON sys_samples(machine_id);
CREATE INDEX IF NOT EXISTS idx_repo_status_ts ON repo_status_snapshots(collected_at);
CREATE INDEX IF NOT EXISTS idx_account_usage_ts ON account_usage_snapshots(collected_at);
CREATE INDEX IF NOT EXISTS idx_alert_history_fired ON alert_history(fired_at);
CREATE INDEX IF NOT EXISTS idx_health_summary_ts ON health_summary(collected_at);
CREATE INDEX IF NOT EXISTS idx_audit_events_ts ON audit_events(ts);
CREATE INDEX IF NOT EXISTS idx_audit_events_type ON audit_events(event_type);
CREATE INDEX IF NOT EXISTS idx_audit_events_machine ON audit_events(machine_id);
CREATE INDEX IF NOT EXISTS idx_dcg_events_ts ON dcg_events(ts);
CREATE INDEX IF NOT EXISTS idx_net_events_ts ON net_events(ts);
