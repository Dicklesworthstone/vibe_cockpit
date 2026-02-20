-- Track which agent sessions have been mined for solutions
-- Prevents re-mining the same session and stores mining metadata

CREATE TABLE IF NOT EXISTS mined_sessions (
    session_id TEXT PRIMARY KEY,
    machine_id TEXT,
    mined_at TIMESTAMP DEFAULT current_timestamp,
    solutions_extracted INTEGER DEFAULT 0,
    patterns_extracted INTEGER DEFAULT 0,
    quality_avg REAL,
    miner_version TEXT DEFAULT 'v1'
);

CREATE INDEX IF NOT EXISTS idx_mined_sessions_at ON mined_sessions(mined_at);
CREATE INDEX IF NOT EXISTS idx_mined_sessions_machine ON mined_sessions(machine_id);
