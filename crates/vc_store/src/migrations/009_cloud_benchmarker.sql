-- Migration 009: cloud_benchmarker collector tables
-- Created: 2026-01-29
-- Collector pattern: HTTP Scrape or SQLite direct read

-- Raw benchmark results from /data/raw/
CREATE TABLE IF NOT EXISTS cloud_bench_raw (
    machine_id TEXT NOT NULL,
    collected_at TIMESTAMP NOT NULL,
    benchmark_type TEXT NOT NULL,    -- cpu, memory, disk, network
    benchmark_name TEXT NOT NULL,    -- specific test name
    value REAL NOT NULL,
    unit TEXT,                       -- ops/sec, MB/s, ms, etc.
    raw_json TEXT,
    PRIMARY KEY (machine_id, collected_at, benchmark_type, benchmark_name)
);

-- Overall scores from /data/overall/
CREATE TABLE IF NOT EXISTS cloud_bench_overall (
    machine_id TEXT NOT NULL,
    collected_at TIMESTAMP NOT NULL,
    overall_score REAL,
    cpu_score REAL,
    memory_score REAL,
    disk_score REAL,
    network_score REAL,
    subscores_json TEXT,             -- JSON object with detailed sub-scores
    raw_json TEXT,
    PRIMARY KEY (machine_id, collected_at)
);

-- Historical trend tracking with baseline comparison
CREATE TABLE IF NOT EXISTS cloud_bench_history (
    machine_id TEXT NOT NULL,
    benchmark_date DATE NOT NULL,
    overall_score REAL,
    baseline_score REAL,             -- First recorded score for this machine
    delta_from_baseline REAL,        -- Percentage change from baseline
    anomaly_detected BOOLEAN DEFAULT FALSE,
    anomaly_threshold REAL,          -- Threshold used for detection
    PRIMARY KEY (machine_id, benchmark_date)
);

-- Index for trend queries
CREATE INDEX IF NOT EXISTS idx_cloud_bench_history_date ON cloud_bench_history(benchmark_date);

-- Index for anomaly detection queries
CREATE INDEX IF NOT EXISTS idx_cloud_bench_history_anomaly ON cloud_bench_history(machine_id, anomaly_detected) WHERE anomaly_detected = TRUE;
