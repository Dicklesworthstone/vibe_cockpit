-- Migration 011: collector_health, machine_baselines, drift_events
-- Created: 2026-01-30
-- Purpose: Track per-collector freshness, machine baselines, and metric drift
-- Translated from DuckDB to SQLite-compatible SQL (bd-phr)

-- Per-collector health tracking
CREATE TABLE IF NOT EXISTS collector_health (
    machine_id TEXT NOT NULL,
    collector TEXT NOT NULL,
    collected_at TEXT NOT NULL,
    success INTEGER NOT NULL DEFAULT 1,
    duration_ms INTEGER,
    rows_inserted INTEGER DEFAULT 0,
    bytes_parsed INTEGER DEFAULT 0,
    error_class TEXT,
    freshness_seconds INTEGER,
    payload_hash TEXT,
    collector_version TEXT,
    schema_version TEXT,
    cursor_json TEXT,
    PRIMARY KEY (machine_id, collector, collected_at)
);

CREATE INDEX IF NOT EXISTS idx_collector_health_machine
    ON collector_health(machine_id, collector);
CREATE INDEX IF NOT EXISTS idx_collector_health_ts
    ON collector_health(collected_at);

-- Machine baseline profiles (rolling stats)
CREATE TABLE IF NOT EXISTS machine_baselines (
    machine_id TEXT NOT NULL,
    baseline_window TEXT NOT NULL,
    computed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    metrics_json TEXT NOT NULL,
    PRIMARY KEY (machine_id, baseline_window)
);

-- Drift events detected by z-score or percentile divergence
CREATE TABLE IF NOT EXISTS drift_events (
    id INTEGER PRIMARY KEY,
    machine_id TEXT NOT NULL,
    detected_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    metric TEXT NOT NULL,
    current_value REAL NOT NULL,
    baseline_mean REAL NOT NULL,
    baseline_std REAL NOT NULL,
    z_score REAL NOT NULL,
    severity TEXT NOT NULL DEFAULT 'info',
    evidence_json TEXT
);

CREATE INDEX IF NOT EXISTS idx_drift_events_machine
    ON drift_events(machine_id);
CREATE INDEX IF NOT EXISTS idx_drift_events_ts
    ON drift_events(detected_at);
CREATE INDEX IF NOT EXISTS idx_drift_events_severity
    ON drift_events(severity);
