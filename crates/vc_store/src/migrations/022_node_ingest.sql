-- Ingest audit log: tracks bundles ingested from vc-node push agents
-- Translated from DuckDB to SQLite-compatible SQL (bd-h6y)
CREATE TABLE IF NOT EXISTS node_ingest_log (
    id INTEGER PRIMARY KEY,
    bundle_id TEXT NOT NULL,
    machine_id TEXT NOT NULL,
    collector TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    row_count INTEGER NOT NULL,
    ingested_at TEXT DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_ingest_content_hash
    ON node_ingest_log(content_hash);
CREATE INDEX IF NOT EXISTS idx_ingest_machine
    ON node_ingest_log(machine_id, collector);
CREATE INDEX IF NOT EXISTS idx_ingest_bundle
    ON node_ingest_log(bundle_id);
