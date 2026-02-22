-- Migration 025: Add missing collector output tables
--
-- Tables needed by the beads, process_triage (pt), and GitHub collectors
-- that were referenced in collector code but never created in schema.

-- Beads triage snapshots (from bv --robot-triage)
CREATE TABLE IF NOT EXISTS beads_triage_snapshots (
    machine_id TEXT NOT NULL,
    collected_at TIMESTAMP NOT NULL,
    repo_id TEXT NOT NULL,
    quick_ref_json TEXT,
    recommendations_json TEXT,
    project_health_json TEXT,
    raw_json TEXT,
    PRIMARY KEY (machine_id, collected_at, repo_id)
);

-- Beads graph metrics (from bv graph analysis)
CREATE TABLE IF NOT EXISTS beads_graph_metrics (
    repo_id TEXT NOT NULL,
    collected_at TIMESTAMP NOT NULL,
    pagerank_json TEXT,
    betweenness_json TEXT,
    critical_path_json TEXT,
    node_count INTEGER,
    edge_count INTEGER,
    density REAL,
    has_cycles BOOLEAN,
    PRIMARY KEY (repo_id, collected_at)
);

-- Process triage: individual process inventory
CREATE TABLE IF NOT EXISTS pt_processes (
    machine_id TEXT NOT NULL,
    collected_at TIMESTAMP NOT NULL,
    pid INTEGER NOT NULL,
    name TEXT,
    cmdline TEXT,
    "user" TEXT,
    started_at TEXT,
    ended_at TEXT,
    exit_code INTEGER,
    status TEXT,
    category TEXT,
    session_id TEXT,
    cpu_percent REAL,
    memory_mb REAL,
    PRIMARY KEY (machine_id, collected_at, pid)
);

-- Process triage: resource usage snapshots
CREATE TABLE IF NOT EXISTS pt_snapshots (
    machine_id TEXT NOT NULL,
    collected_at TIMESTAMP NOT NULL,
    pid INTEGER NOT NULL,
    snapshot_at TIMESTAMP,
    cpu_percent REAL,
    memory_mb REAL,
    memory_percent REAL,
    threads INTEGER,
    open_files INTEGER,
    io_read_bytes BIGINT,
    io_write_bytes BIGINT,
    PRIMARY KEY (machine_id, collected_at, pid)
);

-- GitHub repo issue/PR snapshots
CREATE TABLE IF NOT EXISTS gh_repo_issue_pr_snapshot (
    repo_id TEXT NOT NULL,
    collected_at TIMESTAMP NOT NULL,
    open_issues INTEGER DEFAULT 0,
    open_prs INTEGER DEFAULT 0,
    triage_json TEXT,
    label_breakdown_json TEXT,
    raw_json TEXT,
    PRIMARY KEY (repo_id, collected_at)
);

CREATE INDEX IF NOT EXISTS idx_beads_triage_machine ON beads_triage_snapshots(machine_id);
CREATE INDEX IF NOT EXISTS idx_beads_triage_collected ON beads_triage_snapshots(collected_at);
CREATE INDEX IF NOT EXISTS idx_pt_processes_machine ON pt_processes(machine_id);
CREATE INDEX IF NOT EXISTS idx_pt_snapshots_machine ON pt_snapshots(machine_id);
CREATE INDEX IF NOT EXISTS idx_gh_snapshot_repo ON gh_repo_issue_pr_snapshot(repo_id);
