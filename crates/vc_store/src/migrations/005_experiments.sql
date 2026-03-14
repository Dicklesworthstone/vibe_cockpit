-- A/B testing experiments schema
-- Enables controlled experiments with agent configurations
-- Translated from DuckDB to SQLite-compatible SQL (bd-dfl)

-- Main experiments table
CREATE TABLE IF NOT EXISTS experiments (
    experiment_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    hypothesis TEXT,
    status TEXT NOT NULL,              -- draft, running, paused, completed
    started_at TEXT,
    ended_at TEXT,
    target_sample_size INTEGER,
    actual_sample_size INTEGER DEFAULT 0,
    primary_metric TEXT NOT NULL,
    secondary_metrics JSON,               -- Array of metric names
    significance_threshold REAL DEFAULT 0.05,
    stop_early_on_significance INTEGER DEFAULT 0,
    min_runtime_hours INTEGER,
    max_runtime_hours INTEGER,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    created_by TEXT
);

-- Indexes for experiment queries
CREATE INDEX IF NOT EXISTS idx_experiments_status ON experiments(status);
CREATE INDEX IF NOT EXISTS idx_experiments_created ON experiments(created_at);

-- Experiment variants (control and test groups)
CREATE TABLE IF NOT EXISTS experiment_variants (
    variant_id TEXT PRIMARY KEY,
    experiment_id TEXT NOT NULL,
    name TEXT NOT NULL,
    is_control INTEGER DEFAULT 0,
    config JSON NOT NULL,
    traffic_weight REAL DEFAULT 1.0,
    sample_count INTEGER DEFAULT 0,

    FOREIGN KEY (experiment_id) REFERENCES experiments(experiment_id)
);

-- Indexes for variant queries
CREATE INDEX IF NOT EXISTS idx_experiment_variants_experiment ON experiment_variants(experiment_id);

-- Session assignments to variants
CREATE TABLE IF NOT EXISTS experiment_assignments (
    id INTEGER PRIMARY KEY,
    experiment_id TEXT NOT NULL,
    variant_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    assigned_at TEXT DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (experiment_id) REFERENCES experiments(experiment_id),
    FOREIGN KEY (variant_id) REFERENCES experiment_variants(variant_id)
);

-- Unique constraint: each session gets one assignment per experiment
CREATE UNIQUE INDEX IF NOT EXISTS idx_experiment_assignments_unique
    ON experiment_assignments(experiment_id, session_id);
CREATE INDEX IF NOT EXISTS idx_experiment_assignments_session ON experiment_assignments(session_id);

-- Metric observations for experiments
CREATE TABLE IF NOT EXISTS experiment_observations (
    id INTEGER PRIMARY KEY,
    experiment_id TEXT NOT NULL,
    variant_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    metric_name TEXT NOT NULL,
    metric_value REAL NOT NULL,
    observed_at TEXT DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (experiment_id) REFERENCES experiments(experiment_id),
    FOREIGN KEY (variant_id) REFERENCES experiment_variants(variant_id)
);

-- Indexes for efficient observation queries
CREATE INDEX IF NOT EXISTS idx_experiment_observations_experiment
    ON experiment_observations(experiment_id, metric_name);
CREATE INDEX IF NOT EXISTS idx_experiment_observations_variant
    ON experiment_observations(variant_id, metric_name);

-- Computed experiment results
CREATE TABLE IF NOT EXISTS experiment_results (
    experiment_id TEXT PRIMARY KEY,
    computed_at TEXT,
    winner_variant TEXT,
    confidence_level REAL,
    primary_metric_lift REAL,
    is_significant INTEGER,
    full_results JSON,

    FOREIGN KEY (experiment_id) REFERENCES experiments(experiment_id)
);
