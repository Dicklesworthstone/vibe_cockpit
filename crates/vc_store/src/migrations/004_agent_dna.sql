-- Agent DNA fingerprinting tables
-- Stores behavioral patterns and metrics for AI coding agents

-- Main DNA table - current fingerprint for each agent configuration
CREATE TABLE IF NOT EXISTS agent_dna (
    dna_id VARCHAR PRIMARY KEY,
    agent_program VARCHAR NOT NULL,
    agent_model VARCHAR NOT NULL,
    configuration_hash VARCHAR,
    computed_at TIMESTAMP DEFAULT current_timestamp,

    -- Token patterns
    avg_tokens_per_turn DOUBLE,
    avg_input_output_ratio DOUBLE,
    token_variance DOUBLE,

    -- Error patterns
    error_rate DOUBLE,
    common_error_types JSON,
    recovery_rate DOUBLE,

    -- Tool usage
    tool_preferences JSON,
    tool_success_rates JSON,
    avg_tools_per_task DOUBLE,

    -- Timing patterns
    avg_response_time_ms DOUBLE,
    p95_response_time_ms DOUBLE,
    time_of_day_distribution JSON,

    -- Task patterns
    avg_task_completion_time_mins DOUBLE,
    task_success_rate DOUBLE,
    complexity_handling JSON,

    -- Session patterns
    avg_session_duration_mins DOUBLE,
    avg_turns_per_session DOUBLE,
    session_abandonment_rate DOUBLE,

    -- 128-dimensional embedding for similarity search
    dna_embedding DOUBLE[]
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_agent_dna_program ON agent_dna(agent_program);
CREATE INDEX IF NOT EXISTS idx_agent_dna_model ON agent_dna(agent_model);
CREATE INDEX IF NOT EXISTS idx_agent_dna_computed ON agent_dna(computed_at);

-- Historical DNA snapshots for drift detection
CREATE TABLE IF NOT EXISTS dna_history (
    id INTEGER PRIMARY KEY,
    dna_id VARCHAR NOT NULL,
    computed_at TIMESTAMP NOT NULL,
    metrics JSON NOT NULL,
    change_summary VARCHAR,

    FOREIGN KEY (dna_id) REFERENCES agent_dna(dna_id)
);

-- Indexes for history queries
CREATE INDEX IF NOT EXISTS idx_dna_history_dna_id ON dna_history(dna_id);
CREATE INDEX IF NOT EXISTS idx_dna_history_computed ON dna_history(computed_at);
