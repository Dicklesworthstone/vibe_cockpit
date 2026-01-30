-- Migration 014: Autopilot decisions log
-- Created: 2026-01-30
-- Purpose: Track autopilot decisions for auditing and review

CREATE TABLE IF NOT EXISTS autopilot_decisions (
    id INTEGER PRIMARY KEY,
    decision_type TEXT NOT NULL,
    reason TEXT NOT NULL,
    confidence DOUBLE NOT NULL,
    executed BOOLEAN NOT NULL DEFAULT FALSE,
    decided_at TIMESTAMP NOT NULL DEFAULT current_timestamp,
    details_json TEXT
);
