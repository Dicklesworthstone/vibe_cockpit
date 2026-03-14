-- Incident replay snapshots: cached point-in-time fleet state for incident replay
-- Translated from DuckDB to SQLite-compatible SQL (bd-h6y)
CREATE TABLE IF NOT EXISTS incident_replay_snapshots (
    id INTEGER PRIMARY KEY,
    incident_id TEXT NOT NULL,
    snapshot_ts TEXT NOT NULL,
    snapshot_json TEXT NOT NULL,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_replay_snapshot_incident
    ON incident_replay_snapshots(incident_id, snapshot_ts);
