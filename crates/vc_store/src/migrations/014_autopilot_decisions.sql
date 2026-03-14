-- Migration 014: Autopilot decisions log
-- Created: 2026-01-30
-- Purpose: Track autopilot decisions for auditing and review
-- Translated from DuckDB to SQLite-compatible SQL (bd-phr)

CREATE TABLE IF NOT EXISTS autopilot_decisions (
    id INTEGER PRIMARY KEY,
    decision_type TEXT NOT NULL,
    reason TEXT NOT NULL,
    confidence REAL NOT NULL,
    executed INTEGER NOT NULL DEFAULT 0,
    decided_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    details_json TEXT
);
