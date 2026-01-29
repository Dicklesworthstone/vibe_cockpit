-- Cost attribution analytics schema
-- Tracks cost attribution to repos, machines, and agent types

-- Main cost attribution snapshot table
CREATE TABLE IF NOT EXISTS cost_attribution_snapshot (
    id INTEGER PRIMARY KEY,
    collected_at TIMESTAMP DEFAULT current_timestamp,
    repo_id VARCHAR,                     -- Repository identifier (from ru)
    repo_path VARCHAR,                   -- Repository path
    machine_id VARCHAR,                  -- Machine identifier
    agent_type VARCHAR,                  -- claude, codex, gemini, etc.
    provider VARCHAR,                    -- anthropic, openai, google, etc.
    estimated_cost_usd DOUBLE,           -- Estimated cost in USD
    tokens_input BIGINT DEFAULT 0,       -- Input tokens used
    tokens_output BIGINT DEFAULT 0,      -- Output tokens used
    tokens_total BIGINT DEFAULT 0,       -- Total tokens (input + output)
    sessions_count INTEGER DEFAULT 0,    -- Number of sessions
    requests_count INTEGER DEFAULT 0,    -- Number of API requests
    confidence DOUBLE,                   -- Confidence score (0.0 to 1.0)
    confidence_factors_json TEXT,        -- JSON breakdown of confidence factors
    raw_json TEXT                        -- Full attribution details
);

-- Daily cost summary for trend analysis
CREATE TABLE IF NOT EXISTS cost_daily_summary (
    id INTEGER PRIMARY KEY,
    date DATE NOT NULL,
    repo_id VARCHAR,
    machine_id VARCHAR,
    agent_type VARCHAR,
    provider VARCHAR,
    total_cost_usd DOUBLE DEFAULT 0,
    total_tokens BIGINT DEFAULT 0,
    sessions_count INTEGER DEFAULT 0,
    avg_confidence DOUBLE,
    UNIQUE (date, repo_id, machine_id, agent_type, provider)
);

-- Cost anomalies for alerting
CREATE TABLE IF NOT EXISTS cost_anomalies (
    id INTEGER PRIMARY KEY,
    detected_at TIMESTAMP DEFAULT current_timestamp,
    anomaly_type VARCHAR NOT NULL,       -- spike, drift, unusual_pattern
    severity VARCHAR NOT NULL,           -- info, warning, critical
    repo_id VARCHAR,
    machine_id VARCHAR,
    provider VARCHAR,
    expected_cost_usd DOUBLE,
    actual_cost_usd DOUBLE,
    deviation_percent DOUBLE,
    details_json TEXT,
    acknowledged_at TIMESTAMP,
    acknowledged_by VARCHAR
);

-- Provider pricing reference table
CREATE TABLE IF NOT EXISTS provider_pricing (
    id INTEGER PRIMARY KEY,
    provider VARCHAR NOT NULL,
    model VARCHAR NOT NULL,
    price_per_1k_input_tokens DOUBLE,    -- USD per 1K input tokens
    price_per_1k_output_tokens DOUBLE,   -- USD per 1K output tokens
    effective_from TIMESTAMP,
    effective_until TIMESTAMP,
    notes TEXT,
    UNIQUE (provider, model, effective_from)
);

-- Insert default pricing (as of Jan 2026)
INSERT INTO provider_pricing (id, provider, model, price_per_1k_input_tokens, price_per_1k_output_tokens, effective_from, notes)
VALUES
    (1, 'anthropic', 'claude-opus-4-5-20251101', 0.015, 0.075, '2025-11-01', 'Claude Opus 4.5'),
    (2, 'anthropic', 'claude-sonnet-4-20250514', 0.003, 0.015, '2025-05-14', 'Claude Sonnet 4'),
    (3, 'anthropic', 'claude-3-5-sonnet-20241022', 0.003, 0.015, '2024-10-22', 'Claude 3.5 Sonnet'),
    (4, 'anthropic', 'claude-3-5-haiku-20241022', 0.001, 0.005, '2024-10-22', 'Claude 3.5 Haiku'),
    (5, 'openai', 'gpt-4o', 0.0025, 0.01, '2024-05-13', 'GPT-4o'),
    (6, 'openai', 'gpt-4-turbo', 0.01, 0.03, '2024-04-09', 'GPT-4 Turbo'),
    (7, 'openai', 'o1', 0.015, 0.06, '2024-12-17', 'o1 reasoning'),
    (8, 'openai', 'o3-mini', 0.0011, 0.0044, '2025-01-31', 'o3-mini'),
    (9, 'google', 'gemini-2.0-flash-exp', 0.0001, 0.0004, '2024-12-11', 'Gemini 2.0 Flash'),
    (10, 'google', 'gemini-1.5-pro', 0.00125, 0.005, '2024-02-15', 'Gemini 1.5 Pro')
ON CONFLICT DO NOTHING;

-- Indexes for efficient queries
CREATE INDEX IF NOT EXISTS idx_cost_attr_collected ON cost_attribution_snapshot(collected_at);
CREATE INDEX IF NOT EXISTS idx_cost_attr_repo ON cost_attribution_snapshot(repo_id);
CREATE INDEX IF NOT EXISTS idx_cost_attr_machine ON cost_attribution_snapshot(machine_id);
CREATE INDEX IF NOT EXISTS idx_cost_attr_provider ON cost_attribution_snapshot(provider);

CREATE INDEX IF NOT EXISTS idx_cost_daily_date ON cost_daily_summary(date);
CREATE INDEX IF NOT EXISTS idx_cost_daily_repo ON cost_daily_summary(repo_id);

CREATE INDEX IF NOT EXISTS idx_cost_anomaly_detected ON cost_anomalies(detected_at);
CREATE INDEX IF NOT EXISTS idx_cost_anomaly_severity ON cost_anomalies(severity);
