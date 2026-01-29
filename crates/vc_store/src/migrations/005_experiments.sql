-- A/B testing experiments schema
-- Enables controlled experiments with agent configurations

-- Main experiments table
CREATE TABLE IF NOT EXISTS experiments (
    experiment_id VARCHAR PRIMARY KEY,
    name VARCHAR NOT NULL,
    description TEXT,
    hypothesis VARCHAR,
    status VARCHAR NOT NULL,              -- draft, running, paused, completed
    started_at TIMESTAMP,
    ended_at TIMESTAMP,
    target_sample_size INTEGER,
    actual_sample_size INTEGER DEFAULT 0,
    primary_metric VARCHAR NOT NULL,
    secondary_metrics JSON,               -- Array of metric names
    significance_threshold DOUBLE DEFAULT 0.05,
    stop_early_on_significance BOOLEAN DEFAULT FALSE,
    min_runtime_hours INTEGER,
    max_runtime_hours INTEGER,
    created_at TIMESTAMP DEFAULT current_timestamp,
    created_by VARCHAR
);

-- Indexes for experiment queries
CREATE INDEX IF NOT EXISTS idx_experiments_status ON experiments(status);
CREATE INDEX IF NOT EXISTS idx_experiments_created ON experiments(created_at);

-- Experiment variants (control and test groups)
CREATE TABLE IF NOT EXISTS experiment_variants (
    variant_id VARCHAR PRIMARY KEY,
    experiment_id VARCHAR NOT NULL,
    name VARCHAR NOT NULL,
    is_control BOOLEAN DEFAULT FALSE,
    config JSON NOT NULL,
    traffic_weight DOUBLE DEFAULT 1.0,
    sample_count INTEGER DEFAULT 0,

    FOREIGN KEY (experiment_id) REFERENCES experiments(experiment_id)
);

-- Indexes for variant queries
CREATE INDEX IF NOT EXISTS idx_experiment_variants_experiment ON experiment_variants(experiment_id);

-- Session assignments to variants
CREATE TABLE IF NOT EXISTS experiment_assignments (
    id INTEGER PRIMARY KEY,
    experiment_id VARCHAR NOT NULL,
    variant_id VARCHAR NOT NULL,
    session_id VARCHAR NOT NULL,
    assigned_at TIMESTAMP DEFAULT current_timestamp,

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
    experiment_id VARCHAR NOT NULL,
    variant_id VARCHAR NOT NULL,
    session_id VARCHAR NOT NULL,
    metric_name VARCHAR NOT NULL,
    metric_value DOUBLE NOT NULL,
    observed_at TIMESTAMP DEFAULT current_timestamp,

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
    experiment_id VARCHAR PRIMARY KEY,
    computed_at TIMESTAMP,
    winner_variant VARCHAR,
    confidence_level DOUBLE,
    primary_metric_lift DOUBLE,
    is_significant BOOLEAN,
    full_results JSON,

    FOREIGN KEY (experiment_id) REFERENCES experiments(experiment_id)
);
