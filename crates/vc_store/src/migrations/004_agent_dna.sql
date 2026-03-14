-- Agent DNA fingerprinting tables
-- Stores behavioral patterns and metrics for AI coding agents
-- Translated from DuckDB to SQLite-compatible SQL (bd-dfl)

-- Main DNA table - current fingerprint for each agent configuration
CREATE TABLE IF NOT EXISTS agent_dna (
    dna_id TEXT PRIMARY KEY,
    agent_program TEXT NOT NULL,
    agent_model TEXT NOT NULL,
    configuration_hash TEXT,
    computed_at TEXT DEFAULT CURRENT_TIMESTAMP,

    -- Token patterns
    avg_tokens_per_turn REAL,
    avg_input_output_ratio REAL,
    token_variance REAL,

    -- Error patterns
    error_rate REAL,
    common_error_types JSON,
    recovery_rate REAL,

    -- Tool usage
    tool_preferences JSON,
    tool_success_rates JSON,
    avg_tools_per_task REAL,

    -- Timing patterns
    avg_response_time_ms REAL,
    p95_response_time_ms REAL,
    time_of_day_distribution JSON,

    -- Task patterns
    avg_task_completion_time_mins REAL,
    task_success_rate REAL,
    complexity_handling JSON,

    -- Session patterns
    avg_session_duration_mins REAL,
    avg_turns_per_session REAL,
    session_abandonment_rate REAL,

    -- 128-dimensional embedding for similarity search
    dna_embedding TEXT -- JSON array of floats: '[0.1, 0.2, ...]', query via json_each()
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_agent_dna_program ON agent_dna(agent_program);
CREATE INDEX IF NOT EXISTS idx_agent_dna_model ON agent_dna(agent_model);
CREATE INDEX IF NOT EXISTS idx_agent_dna_computed ON agent_dna(computed_at);

-- Historical DNA snapshots for drift detection
CREATE TABLE IF NOT EXISTS dna_history (
    id INTEGER PRIMARY KEY,
    dna_id TEXT NOT NULL,
    computed_at TEXT NOT NULL,
    metrics JSON NOT NULL,
    change_summary TEXT,

    FOREIGN KEY (dna_id) REFERENCES agent_dna(dna_id)
);

-- Indexes for history queries
CREATE INDEX IF NOT EXISTS idx_dna_history_dna_id ON dna_history(dna_id);
CREATE INDEX IF NOT EXISTS idx_dna_history_computed ON dna_history(computed_at);
