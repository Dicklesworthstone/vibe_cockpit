-- Migration 010: retention_log table for tracking vacuum operations
-- Created: 2026-01-29
-- Purpose: Log all retention policy executions for audit and debugging

-- Log of vacuum/retention operations
CREATE TABLE IF NOT EXISTS retention_log (
    id INTEGER PRIMARY KEY,
    ts TIMESTAMP DEFAULT current_timestamp,
    policy_id TEXT,
    table_name TEXT NOT NULL,
    rows_deleted BIGINT DEFAULT 0,
    rows_aggregated BIGINT DEFAULT 0,
    duration_ms BIGINT,
    dry_run BOOLEAN DEFAULT FALSE,
    error_message TEXT
);

-- Index for querying retention history
CREATE INDEX IF NOT EXISTS idx_retention_log_ts ON retention_log(ts);
CREATE INDEX IF NOT EXISTS idx_retention_log_table ON retention_log(table_name);
