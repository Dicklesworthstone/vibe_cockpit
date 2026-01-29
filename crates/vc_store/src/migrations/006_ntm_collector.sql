-- NTM (Named Tmux Manager) collector schema
-- Stores tmux session state, agent information, and activity snapshots

-- Session snapshots table - captures state of each ntm session
CREATE TABLE IF NOT EXISTS ntm_sessions_snapshot (
    machine_id VARCHAR NOT NULL,
    collected_at TIMESTAMP NOT NULL,
    session_name VARCHAR NOT NULL,
    exists BOOLEAN DEFAULT TRUE,
    attached BOOLEAN DEFAULT FALSE,
    windows INTEGER DEFAULT 0,
    panes INTEGER DEFAULT 0,
    agent_count INTEGER DEFAULT 0,
    agents_json TEXT,                    -- JSON array of agent details
    raw_json TEXT,
    PRIMARY KEY (machine_id, collected_at, session_name)
);

-- Activity snapshot table - aggregated agent activity stats
CREATE TABLE IF NOT EXISTS ntm_activity_snapshot (
    machine_id VARCHAR NOT NULL,
    collected_at TIMESTAMP NOT NULL,
    total_sessions INTEGER DEFAULT 0,
    total_agents INTEGER DEFAULT 0,
    attached_count INTEGER DEFAULT 0,
    claude_count INTEGER DEFAULT 0,
    codex_count INTEGER DEFAULT 0,
    gemini_count INTEGER DEFAULT 0,
    idle_count INTEGER DEFAULT 0,
    busy_count INTEGER DEFAULT 0,
    error_count INTEGER DEFAULT 0,
    by_type_json TEXT,                   -- JSON breakdown by agent type
    by_state_json TEXT,                  -- JSON breakdown by agent state
    raw_json TEXT,
    PRIMARY KEY (machine_id, collected_at)
);

-- Agent detail snapshots - per-agent metrics over time
CREATE TABLE IF NOT EXISTS ntm_agent_snapshot (
    machine_id VARCHAR NOT NULL,
    collected_at TIMESTAMP NOT NULL,
    session_name VARCHAR NOT NULL,
    pane_id VARCHAR NOT NULL,
    agent_type VARCHAR,                  -- claude, codex, gemini, unknown
    window_idx INTEGER DEFAULT 0,
    pane_idx INTEGER DEFAULT 0,
    is_active BOOLEAN DEFAULT FALSE,
    pid INTEGER,
    process_state VARCHAR,               -- S (sleeping), R (running), etc.
    process_state_name VARCHAR,
    memory_mb INTEGER,
    context_tokens INTEGER,
    context_limit INTEGER,
    context_percent DOUBLE,
    context_model VARCHAR,
    last_output_ts TIMESTAMP,
    output_lines_since_last INTEGER DEFAULT 0,
    raw_json TEXT,
    PRIMARY KEY (machine_id, collected_at, session_name, pane_id)
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_ntm_sessions_machine ON ntm_sessions_snapshot(machine_id);
CREATE INDEX IF NOT EXISTS idx_ntm_sessions_collected ON ntm_sessions_snapshot(collected_at);
CREATE INDEX IF NOT EXISTS idx_ntm_sessions_name ON ntm_sessions_snapshot(session_name);

CREATE INDEX IF NOT EXISTS idx_ntm_activity_machine ON ntm_activity_snapshot(machine_id);
CREATE INDEX IF NOT EXISTS idx_ntm_activity_collected ON ntm_activity_snapshot(collected_at);

CREATE INDEX IF NOT EXISTS idx_ntm_agent_machine ON ntm_agent_snapshot(machine_id);
CREATE INDEX IF NOT EXISTS idx_ntm_agent_collected ON ntm_agent_snapshot(collected_at);
CREATE INDEX IF NOT EXISTS idx_ntm_agent_type ON ntm_agent_snapshot(agent_type);
CREATE INDEX IF NOT EXISTS idx_ntm_agent_session ON ntm_agent_snapshot(session_name);
