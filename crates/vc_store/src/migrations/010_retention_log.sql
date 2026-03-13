-- Migration 010: retention_log table for tracking vacuum operations
-- Created: 2026-01-29
-- Purpose: Log all retention policy executions for audit and debugging
-- Translated from DuckDB to SQLite-compatible SQL (bd-phr)

-- Log of vacuum/retention operations
CREATE TABLE IF NOT EXISTS retention_log (
    id INTEGER PRIMARY KEY,
    ts TEXT DEFAULT (datetime('now')),
    policy_id TEXT,
    table_name TEXT NOT NULL,
    rows_deleted INTEGER DEFAULT 0,
    rows_aggregated INTEGER DEFAULT 0,
    duration_ms INTEGER,
    dry_run INTEGER DEFAULT 0,
    error_message TEXT
);

-- Index for querying retention history
CREATE INDEX IF NOT EXISTS idx_retention_log_ts ON retention_log(ts);
CREATE INDEX IF NOT EXISTS idx_retention_log_table ON retention_log(table_name);
