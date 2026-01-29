-- Migration 008: afsc (Automated Flywheel Setup Checker) collector tables
-- Created: 2026-01-29
-- Collector pattern: CLI Incremental Window (time-bounded)

-- Status snapshot: overall health from `afsc status --format json`
CREATE TABLE IF NOT EXISTS afsc_status_snapshot (
    machine_id TEXT NOT NULL,
    collected_at TIMESTAMP NOT NULL,
    overall_health TEXT,           -- healthy, degraded, unhealthy, unknown
    installers_total INTEGER,
    installers_healthy INTEGER,
    installers_failed INTEGER,
    last_run_at TIMESTAMP,
    last_run_status TEXT,
    uptime_seconds BIGINT,
    raw_json TEXT,
    PRIMARY KEY (machine_id, collected_at)
);

-- Run facts: individual run records from `afsc list --format jsonl`
CREATE TABLE IF NOT EXISTS afsc_run_facts (
    machine_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    ts TIMESTAMP NOT NULL,
    status TEXT,                   -- success, failed, partial, skipped
    duration_ms BIGINT,
    error_category TEXT,           -- timeout, dependency, permission, network, etc.
    installer_name TEXT,
    installer_version TEXT,
    exit_code INTEGER,
    error_message TEXT,
    raw_json TEXT,
    PRIMARY KEY (machine_id, run_id)
);

-- Event logs: streaming events from `afsc validate --format jsonl`
CREATE TABLE IF NOT EXISTS afsc_event_logs (
    id INTEGER PRIMARY KEY,
    machine_id TEXT NOT NULL,
    ts TIMESTAMP NOT NULL,
    event_type TEXT,               -- run_start, run_end, validation, error, warning
    severity TEXT,                 -- info, warn, error, fatal
    message TEXT,
    installer_name TEXT,
    component TEXT,
    raw_json TEXT
);

-- Create index for time-based queries on event logs
CREATE INDEX IF NOT EXISTS idx_afsc_event_logs_ts ON afsc_event_logs(machine_id, ts);

-- Error clusters: aggregated error patterns from `afsc classify-error --format jsonl`
CREATE TABLE IF NOT EXISTS afsc_error_clusters (
    machine_id TEXT NOT NULL,
    collected_at TIMESTAMP NOT NULL,
    error_category TEXT NOT NULL,  -- timeout, dependency, permission, network, config, unknown
    occurrence_count INTEGER,
    first_seen TIMESTAMP,
    last_seen TIMESTAMP,
    affected_installers TEXT,      -- JSON array of installer names
    example_errors_json TEXT,      -- JSON array of example error messages
    PRIMARY KEY (machine_id, collected_at, error_category)
);

-- Create index for error trend queries
CREATE INDEX IF NOT EXISTS idx_afsc_error_clusters_category ON afsc_error_clusters(error_category, last_seen);
