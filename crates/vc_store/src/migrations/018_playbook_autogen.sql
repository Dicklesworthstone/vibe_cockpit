-- Playbook auto-generation tables

-- Captured resolutions: operator actions that resolved alerts
CREATE TABLE IF NOT EXISTS resolutions (
    id INTEGER PRIMARY KEY,
    alert_id INTEGER,
    alert_type TEXT NOT NULL,
    trigger_context TEXT,
    actions TEXT NOT NULL,
    outcome TEXT NOT NULL DEFAULT 'unknown',
    captured_at TIMESTAMP DEFAULT current_timestamp,
    machine_id TEXT,
    operator TEXT
);

CREATE INDEX IF NOT EXISTS idx_resolutions_alert_type ON resolutions(alert_type);
CREATE INDEX IF NOT EXISTS idx_resolutions_outcome ON resolutions(outcome);
CREATE INDEX IF NOT EXISTS idx_resolutions_captured_at ON resolutions(captured_at);

-- Playbook drafts: auto-generated playbooks pending review
CREATE TABLE IF NOT EXISTS playbook_drafts (
    draft_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    alert_type TEXT NOT NULL,
    trigger_json TEXT NOT NULL,
    steps_json TEXT NOT NULL,
    confidence DOUBLE NOT NULL DEFAULT 0.0,
    sample_count INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'pending_review',
    approved_by TEXT,
    approved_at TIMESTAMP,
    created_at TIMESTAMP DEFAULT current_timestamp,
    source_pattern_json TEXT
);

CREATE INDEX IF NOT EXISTS idx_playbook_drafts_status ON playbook_drafts(status);
CREATE INDEX IF NOT EXISTS idx_playbook_drafts_alert_type ON playbook_drafts(alert_type);
