-- Incident notes for annotations during investigation
-- Complements the incidents and incident_timeline_events tables from 001_initial_schema.sql

CREATE TABLE IF NOT EXISTS incident_notes (
    id INTEGER PRIMARY KEY,
    incident_id TEXT NOT NULL,
    author TEXT,                       -- agent name or user
    content TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT current_timestamp
);

CREATE INDEX IF NOT EXISTS idx_incident_notes_incident ON incident_notes(incident_id);
CREATE INDEX IF NOT EXISTS idx_incidents_status ON incidents(status);
CREATE INDEX IF NOT EXISTS idx_incidents_severity ON incidents(severity);
CREATE INDEX IF NOT EXISTS idx_incident_timeline_incident ON incident_timeline_events(incident_id);
