-- Poll schedule decisions: audit trail for adaptive polling decisions
CREATE TABLE IF NOT EXISTS poll_schedule_decisions (
    id INTEGER PRIMARY KEY,
    machine_id TEXT NOT NULL,
    collector TEXT NOT NULL,
    decided_at TIMESTAMP DEFAULT current_timestamp,
    next_interval_seconds INTEGER NOT NULL,
    reason_json TEXT
);

CREATE INDEX IF NOT EXISTS idx_poll_schedule_machine
    ON poll_schedule_decisions(machine_id, collector);

-- On-demand profiling samples
CREATE TABLE IF NOT EXISTS sys_profile_samples (
    id INTEGER PRIMARY KEY,
    machine_id TEXT NOT NULL,
    collected_at TIMESTAMP DEFAULT current_timestamp,
    profile_id TEXT NOT NULL,
    metrics_json TEXT,
    raw_json TEXT
);

CREATE INDEX IF NOT EXISTS idx_profile_samples_machine
    ON sys_profile_samples(machine_id, profile_id);
CREATE INDEX IF NOT EXISTS idx_profile_samples_collected
    ON sys_profile_samples(collected_at);
