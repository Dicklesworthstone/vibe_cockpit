-- Redaction audit trail: tracks PII/secret redaction stats per batch
CREATE TABLE IF NOT EXISTS redaction_events (
    id INTEGER PRIMARY KEY,
    collected_at TIMESTAMP DEFAULT current_timestamp,
    machine_id TEXT NOT NULL,
    collector TEXT NOT NULL,
    redacted_fields INTEGER NOT NULL DEFAULT 0,
    redacted_bytes INTEGER NOT NULL DEFAULT 0,
    rules_version TEXT NOT NULL,
    sample_hash TEXT
);

CREATE INDEX IF NOT EXISTS idx_redaction_machine
    ON redaction_events(machine_id, collector);
CREATE INDEX IF NOT EXISTS idx_redaction_collected
    ON redaction_events(collected_at);
