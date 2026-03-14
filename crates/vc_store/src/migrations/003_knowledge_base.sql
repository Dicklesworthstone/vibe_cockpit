-- Knowledge Base schema
-- Stores solutions, patterns, debug logs, and other learnings from agent sessions
-- Translated from DuckDB to SQLite-compatible SQL (bd-dfl)

-- DuckDB ENUM removed: entry_type enforced via CHECK constraint below

-- Main knowledge entries table
CREATE TABLE IF NOT EXISTS knowledge_entries (
    id INTEGER PRIMARY KEY,
    entry_type TEXT NOT NULL CHECK(entry_type IN ('solution', 'pattern', 'prompt', 'debug_log')),
    title TEXT NOT NULL,
    summary TEXT,
    content TEXT NOT NULL,
    source_session_id TEXT,            -- Link to cass session
    source_file TEXT,
    source_lines TEXT,                 -- "10-25" or NULL
    tags TEXT,                         -- JSON array: '["rust","async"]', query via json_each(tags)
    -- embedding FLOAT[1536],         -- For semantic search (future)
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT,
    usefulness_score REAL DEFAULT 0.0, -- Computed from feedback
    view_count INTEGER DEFAULT 0,
    applied_count INTEGER DEFAULT 0
);

-- Feedback on knowledge entries
CREATE TABLE IF NOT EXISTS knowledge_feedback (
    id INTEGER PRIMARY KEY,
    entry_id INTEGER REFERENCES knowledge_entries(id),
    feedback_type TEXT NOT NULL,       -- helpful, not_helpful, outdated
    session_id TEXT,
    comment TEXT,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_knowledge_entry_type ON knowledge_entries(entry_type);
CREATE INDEX IF NOT EXISTS idx_knowledge_created ON knowledge_entries(created_at);
CREATE INDEX IF NOT EXISTS idx_knowledge_score ON knowledge_entries(usefulness_score DESC);
CREATE INDEX IF NOT EXISTS idx_feedback_entry ON knowledge_feedback(entry_id);
