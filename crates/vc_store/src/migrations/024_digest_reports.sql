-- Digest reports: daily/weekly summary snapshots
CREATE TABLE IF NOT EXISTS digest_reports (
    id INTEGER PRIMARY KEY,
    report_id TEXT NOT NULL,
    window_hours INTEGER NOT NULL,
    generated_at TIMESTAMP DEFAULT current_timestamp,
    summary_json TEXT,
    markdown TEXT
);

CREATE INDEX IF NOT EXISTS idx_digest_report_id
    ON digest_reports(report_id);
CREATE INDEX IF NOT EXISTS idx_digest_generated
    ON digest_reports(generated_at);
