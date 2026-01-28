-- Knowledge Base schema
-- Stores solutions, patterns, debug logs, and other learnings from agent sessions

-- Knowledge entry types
CREATE TYPE IF NOT EXISTS entry_type AS ENUM ('solution', 'pattern', 'prompt', 'debug_log');

-- Main knowledge entries table
CREATE TABLE IF NOT EXISTS knowledge_entries (
    id INTEGER PRIMARY KEY,
    entry_type VARCHAR NOT NULL,          -- solution, pattern, prompt, debug_log
    title VARCHAR NOT NULL,
    summary TEXT,
    content TEXT NOT NULL,
    source_session_id VARCHAR,            -- Link to cass session
    source_file VARCHAR,
    source_lines VARCHAR,                 -- "10-25" or NULL
    tags VARCHAR[],
    -- embedding FLOAT[1536],             -- For semantic search (future)
    created_at TIMESTAMP DEFAULT now(),
    updated_at TIMESTAMP,
    usefulness_score DOUBLE DEFAULT 0.0,  -- Computed from feedback
    view_count INTEGER DEFAULT 0,
    applied_count INTEGER DEFAULT 0
);

-- Feedback on knowledge entries
CREATE TABLE IF NOT EXISTS knowledge_feedback (
    id INTEGER PRIMARY KEY,
    entry_id INTEGER REFERENCES knowledge_entries(id),
    feedback_type VARCHAR NOT NULL,       -- helpful, not_helpful, outdated
    session_id VARCHAR,
    comment TEXT,
    created_at TIMESTAMP DEFAULT now()
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_knowledge_entry_type ON knowledge_entries(entry_type);
CREATE INDEX IF NOT EXISTS idx_knowledge_created ON knowledge_entries(created_at);
CREATE INDEX IF NOT EXISTS idx_knowledge_score ON knowledge_entries(usefulness_score DESC);
CREATE INDEX IF NOT EXISTS idx_feedback_entry ON knowledge_feedback(entry_id);
