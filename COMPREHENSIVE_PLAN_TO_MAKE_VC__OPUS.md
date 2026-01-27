# VIBE COCKPIT: The Autonomous Agent Empire Command Center

> *"Any sufficiently advanced monitoring system is indistinguishable from a sentient overseer."*

---

## The Vision: Beyond Dashboards

**vibe_cockpit is not a dashboard. It's the BRAIN of your agent empire.**

Imagine a system that doesn't just show you what's happening—it UNDERSTANDS what's happening, PREDICTS what will happen, and ACTS to optimize outcomes before you even know there's a problem.

You have:
- 20+ Claude Max accounts
- 10+ GPT Pro accounts
- 5+ Gemini Ultra accounts
- Multiple Linux machines
- Hundreds of concurrent agent instances
- Dozens of active projects

This is not a monitoring problem. This is an **orchestration intelligence** problem.

---

## Part I: Foundations & Constraints

### 0.1 Ground Rules (Current Repo Reality)

`AGENTS.md` in this repo is copied from dcg. It contains safety and process guidance that still applies (especially "no deletion" and "no destructive commands"), but **it is not a spec for vc**. We will create a vc-specific AGENTS.md later.

For vc:
- Prefer **read-only integration** with existing tools (shell out to their JSON/robot modes, or read their SQLite/JSONL stores)
- Avoid tight coupling (do not vendor entire repos)
- Bias toward **stable, versioned schemas** (JSON schema + SQL schema)
- **No premature complexity** — start pull-only, add push later

### 0.2 Product Definition

**Primary User:** You (human) supervising dozens/hundreds of agent sessions across multiple machines, providers, and repos.

**Core Jobs-to-be-Done (JTBD):**
1. "At a glance, tell me if the system is healthy right now."
2. "If something is unhealthy, show me why (root cause) quickly."
3. "If action is needed, guide me to safe remediation (or do it automatically in controlled 'autopilot')."
4. "Let agents query the same state in robot mode and do triage on a sleep/wake cycle."

**What vc IS:**
- **Observability** — unified view of all tools, machines, agents
- **Triage** — intelligent prioritization of what needs attention
- **Orchestration Intelligence** — recommendations and (optionally) automated actions
- **Institutional Memory** — accumulated knowledge, patterns, gotchas

**What vc is NOT (scope boundaries):**
- Not a replacement for ntm (tmux orchestration)
- Not a replacement for rch (remote compilation)
- Not a replacement for caut/caam (account usage/management)
- Not a replacement for cass (session search/index)
- Not a replacement for mcp_agent_mail (agent messaging)
- Not a replacement for dcg (command safety)
- Not a replacement for br/bv (task tracking)
- Not a replacement for pt/sysmoni/rano (process + system + network monitoring)

**vc's job is to unify these into a single cockpit with a coherent model and fast navigation.**

### 0.3 North Star

To succeed in real-world practice, vc must be more than "a page of metrics." It should behave like an **operations brain** for the agent fleet:

- **Perceive:** continuously ingest signals from machines, repos, agents, accounts, and tools
- **Think:** correlate signals, detect anomalies, and forecast near-future failures
- **Act (gated):** produce ranked, explainable remediation steps; optionally execute tightly allowlisted actions with audit logs
- **Remember:** preserve history for "what changed?" queries, replay incidents, accumulate playbooks/gotchas

**The key output is not raw data; it's the answer to: "What should we do next, and why?"**

---

## Part II: The Grand Architecture

```
                              ╔═══════════════════════════════════════╗
                              ║     VIBE COCKPIT NEURAL CORE          ║
                              ║   "The All-Seeing Eye of the Swarm"   ║
                              ╚═══════════════════════════════════════╝
                                              │
                    ┌───────────────────────────────────────────────┐
                    │                                               │
         ┌──────────▼──────────┐                      ┌─────────────▼─────────────┐
         │   PERCEPTION LAYER  │                      │    COGNITION LAYER        │
         │   ═══════════════   │                      │    ════════════════       │
         │                     │                      │                           │
         │  • 15 Data Streams  │                      │  • Pattern Recognition    │
         │  • Real-time Fusion │         ┌───────────│  • Anomaly Detection      │
         │  • State Synthesis  │         │           │  • Causal Inference       │
         │  • Event Correlation│         │           │  • Predictive Models      │
         └─────────────────────┘         │           │  • Optimization Engine    │
                    │                    │           └─────────────────────────────┘
                    │                    │                         │
                    ▼                    ▼                         ▼
         ┌─────────────────────────────────────────────────────────────────────┐
         │                         MEMORY LAYER                                 │
         │   ══════════════════════════════════════════════════════════════    │
         │                                                                      │
         │   ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌───────────┐  │
         │   │  DuckDB     │  │  Vector DB  │  │  Time-Series│  │  Event    │  │
         │   │  Analytics  │  │  Embeddings │  │  (optional) │  │  Sourcing │  │
         │   └─────────────┘  └─────────────┘  └─────────────┘  └───────────┘  │
         │                                                                      │
         │   Full State History • Semantic Search • Forecasting • Replay        │
         └─────────────────────────────────────────────────────────────────────┘
                                        │
         ┌──────────────────────────────┼──────────────────────────────┐
         │                              │                              │
         ▼                              ▼                              ▼
┌─────────────────┐          ┌─────────────────┐          ┌─────────────────────┐
│  ACTION LAYER   │          │ INTERFACE LAYER │          │  INTEGRATION LAYER  │
│  ═════════════  │          │ ═══════════════ │          │  ════════════════   │
│                 │          │                 │          │                     │
│ • Auto-Healing  │          │ • Neural TUI    │          │ • Slack Bot         │
│ • Orchestration │          │ • Web Dashboard │          │ • GitHub Webhooks   │
│ • Load Balancing│          │   (Next.js)     │          │ • MCP Server        │
│ • Account Swap  │          │ • Natural Lang  │          │ • Prometheus Export │
│ • Emergency Stop│          │ • Voice Control │          │ • Mobile Push       │
└─────────────────┘          └─────────────────┘          └─────────────────────┘
```

### 2.1 Layered Mental Model

This layered model keeps the system understandable and prevents "feature soup":

| Layer | Purpose | Examples |
|-------|---------|----------|
| **Perception** | Ingest signals from machines/tools (raw facts) | Collectors, polling, event streams |
| **Cognition** | Correlations, anomalies, forecasts, root-cause hypotheses | Oracle, pattern detection |
| **Memory** | DuckDB primary store (append-only facts + snapshots) | Time travel, incident replay |
| **Action** | Playbooks, remediation suggestions, gated execution | Guardian, autopilot |
| **Interface** | TUI, web, CLI robot outputs | User-facing surfaces |
| **Integration** | MCP server, Slack, Prometheus export, webhooks | External system connections |

Start with DuckDB only. Add extra stores only if a real bottleneck emerges.

### 2.2 Data Flow (Pull-First, Push-Optional)

**Initial (MVP) architecture is PULL:**

```
vc (main machine)
  |-- local collectors (read files / run local CLIs)
  |-- remote collectors via SSH (run CLIs remotely OR fetch exported files)
  `-- DuckDB (single DB file)
       |-- raw tables (append-only events/snapshots)
       |-- derived views (rollups, latest-by-key, anomalies)
       `-- exports (JSON/CSV/Parquet as needed)
```

**Later (scale) add PUSH:**

```
remote machine runs vc-node (tiny agent)
  |-- collects locally (low overhead)
  |-- writes local duckdb/parquet/jsonl
  `-- pushes snapshots/events to vc-hub (HTTPS / SSH upload)
```

Start with pull because it requires zero deployment footprint on remotes beyond SSH access.

### 2.3 Component Model (Rust Crates/Modules)

Single Cargo workspace with internal crates:

| Crate | Purpose |
|-------|---------|
| `vc_config` | Config parsing + validation, secrets/paths |
| `vc_collect` | Collectors (one per upstream tool + system collectors) |
| `vc_store` | DuckDB schema migrations + ingestion helpers + query library |
| `vc_query` | Canonical queries (health, rollups, leaderboards, anomalies) |
| `vc_oracle` | Prediction engine, pattern recognition, forecasting |
| `vc_guardian` | Self-healing protocols, fleet orchestration |
| `vc_knowledge` | Solution mining, gotcha database, playbooks |
| `vc_alert` | Rule engine + notifications + autopilot hooks |
| `vc_tui` | ratatui UI + navigation + charts |
| `vc_web` | axum server + embedded static web assets + JSON API |
| `vc_cli` | clap commands; robot mode output formatting |
| `vc_mcp` | MCP server mode for agent queries |

### 2.4 Threading/Concurrency Model

- Use `tokio` runtime
- Each "poll cycle" spawns **bounded concurrency** tasks:
  - Bound by machine count (e.g., max 8 machines in flight)
  - Bound by collectors per machine (e.g., max 4 collectors in flight)
- Each collector enforces:
  - Timeout (configurable, default 30s)
  - Size limits for outputs
  - A stable "collector version" + "schema version" tag on inserted rows

---

## Part III: Source System Integration (What to Pull, How to Pull)

### 3.0 General Principles

- Prefer **official CLI robot/JSON output** first
- If CLI is too slow or requires auth/UI, read **local caches/SQLite/JSONL** next
- Avoid re-implementing upstream logic unless necessary
- Every collector implements the `Collector` trait (see 3.16)

### 3.1 NTM (Named Tmux Manager) — "Nerve Center"

**Goal:** Show running sessions, agent activity, health, orchestration context.

**Integration:**
- Shell out to `ntm --robot-status` and `ntm --robot-list` for JSON output
- Optionally parse `events.jsonl` if ntm provides it

**vc Ingestion Tables:**
```sql
ntm_sessions_snapshot(machine_id, collected_at, session_name, work_dir, git_branch,
                       agent_counts_json, panes_json, raw_json)
ntm_activity_snapshot(machine_id, collected_at, total_agents, by_type_json,
                       by_state_json, raw_json)
ntm_events(machine_id, ts, event_type, session_name, agent_id, details_json)
```

**UI Value:** "Agent fleet map" by machine/repo/session

### 3.2 CAUT (Coding Agent Usage Tracker)

**Goal:** Per-provider and per-account usage, remaining quota, reset times, status/outage awareness.

**Integration:**
- `caut usage --json` (robot mode)
- Optionally tail `~/.local/share/caut/usage-history.sqlite` for historical charts

**vc Ingestion Tables:**
```sql
account_usage_snapshot(machine_id, collected_at, provider, account_label,
                        window_name, used_pct, remaining_pct, resets_at,
                        credits_remaining, status, raw_json)
account_status_events(machine_id, ts, provider, event_type, details_json)
```

### 3.3 CAAM (Coding Agent Account Manager)

**Goal:** Profile mapping, "best account to use", limit forecasting.

**Integration:**
- `caam limits --format json` and `caam status --json`
- Use to power autopilot suggestions like "swap to account X"

**vc Ingestion Tables:**
```sql
account_profile_snapshot(machine_id, collected_at, tool, active_profile,
                          health_score, health_expires_at, raw_json)
account_limits_snapshot(machine_id, collected_at, provider, profile,
                         window, utilization_pct, resets_at, raw_json)
```

### 3.4 CASS (Coding Agent Session Search)

**Goal:** Global session volume metrics, token usage proxies, compaction counts, agent-type breakdown.

**Integration:**
- `cass stats --json` for aggregate stats
- `cass timeline --json` for time-bucketed data
- `cass health --json` for index status

**vc Ingestion Tables:**
```sql
session_index_status(machine_id, collected_at, state, total_sessions,
                      last_index_at, freshness_seconds, raw_json)
session_stats_snapshot(machine_id, collected_at, total_conversations,
                        total_messages, by_agent_json, by_workspace_json, raw_json)
session_timeline_buckets(machine_id, collected_at, bucket_start, bucket_end,
                          conversation_count, message_count, raw_json)
```

### 3.5 RCH (Remote Compilation Helper)

**Goal:** Master/worker activity, queue depth, latency, failures, transfer stats.

**Integration Options:**
1. Scrape Prometheus `/metrics` endpoint (best for dashboards)
2. `rch status --json` (best for simple pull without Prometheus)

**vc Ingestion Tables:**
```sql
rch_status_snapshot(machine_id, collected_at, daemon_state, workers_total,
                     workers_available, builds_active, queue_depth,
                     avg_latency_ms, raw_json)
rch_metric_samples(machine_id, ts, metric_name, labels_json, value)
```

### 3.6 RU (Repo Updater)

**Goal:** Repo fleet overview: dirty repos, ahead/behind, recent changes.

**Integration:**
- `ru list --json` to enumerate repo paths
- `ru status --no-fetch --json` (avoid network in tight loops)
- Optional deep scan (less frequent) for LoC, commit counts

**vc Ingestion Tables:**
```sql
repos(machine_id, repo_id, path, url, name)  -- repo_id = stable hash
repo_status_snapshot(machine_id, collected_at, repo_id, branch, dirty,
                      ahead, behind, last_commit_at, raw_json)
repo_commit_stats(machine_id, collected_at, repo_id, commits_7d, commits_30d)
```

### 3.7 MCP Agent Mail

**Goal:** Message counts, ack-required backlog, urgent messages, inter-agent heatmaps.

**Integration:**
- Read from SQLite database (`storage.sqlite3`)
- Use incremental cursor on `messages.id`

**vc Ingestion Tables:**
```sql
mail_messages(collected_at, project_id, message_id, thread_id, sender,
               to_recipients_json, importance, ack_required, created_ts,
               subject, raw_json)
mail_recipients(collected_at, message_id, recipient, read_ts, ack_ts)
mail_file_reservations(collected_at, project_id, reservation_id, path_pattern,
                        holder, expires_ts, exclusive, raw_json)
```

### 3.8 PT (Process Triage)

**Goal:** Detect stuck/zombie/runaway processes, provide safe remediation suggestions.

**Integration:**
- `pt robot plan --format json` for findings
- Use as controlled remediation executor (only after explicit config)

**vc Ingestion Tables:**
```sql
process_triage_snapshot(machine_id, collected_at, findings_json,
                         recommendations_json, raw_json)
process_triage_findings(machine_id, collected_at, pid, comm, classification,
                         confidence, evidence_json)
```

### 3.9 DCG (Destructive Command Guard)

**Goal:** Blocked command counts over time by machine/repo, severity breakdown, top rule IDs.

**Integration:**
- Tail `~/.config/dcg/history.jsonl` (JSONL tail pattern)
- Or `dcg stats --json` if available

**vc Ingestion Tables:**
```sql
dcg_events(machine_id, ts, decision, rule_id, pack_id, severity,
            confidence, cwd, command_hash, raw_json)
```

### 3.10 SRPS/Sysmoni (System Resource Protection)

**Goal:** CPU/mem/io/net/gpu, top processes, throttling, temps.

**Integration:**
- `sysmoni --json` (one-shot snapshot)
- `sysmoni --json-stream` (NDJSON continuous) for high-frequency

**vc Ingestion Tables:**
```sql
sys_samples(machine_id, collected_at, cpu_pct, load1, load5, load15,
             mem_used_bytes, mem_total_bytes, swap_used_bytes,
             disk_read_mbps, disk_write_mbps, net_rx_mbps, net_tx_mbps,
             gpu_utilization, temp_cpu, raw_json)
sys_top_processes(machine_id, collected_at, pid, comm, cpu_pct, mem_bytes,
                   fd_count, io_read_bytes, io_write_bytes)
```

### 3.11 RANO (Network Observer)

**Goal:** Network activity per provider/process; detect abnormal spikes or auth loops.

**Integration:**
- `rano export --format jsonl --since <duration>`
- Or read SQLite tables/views directly

**vc Ingestion Tables:**
```sql
net_events(machine_id, ts, event_type, provider, pid, comm,
            remote_ip, remote_port, domain, duration_ms, raw_json)
net_session_summary(machine_id, collected_at, provider, connection_count,
                     bytes_transferred, avg_duration_ms)
```

### 3.12 BV + BR (Beads Viewer + Beads Rust)

**Goal:** Productivity metrics and project health via task graph; show "what to work on next."

**Integration:**
- `bv --robot-triage` (JSON mega-command)
- `br ready --json` for actionable work
- `br sync --flush-only` and read `.beads/issues.jsonl`

**vc Ingestion Tables:**
```sql
beads_triage_snapshot(machine_id, collected_at, repo_id, quick_ref_json,
                       recommendations_json, project_health_json, raw_json)
beads_issues(repo_id, issue_id, status, priority, type, labels_json,
              deps_json, updated_at, raw_json)
beads_graph_metrics(repo_id, collected_at, pagerank_json, betweenness_json,
                     critical_path_json, cycles_json)
```

### 3.13 AFSC (Automated Flywheel Setup Checker)

**Goal:** Flywheel setup health; surface installer/run summaries and failure clusters.

**Integration:**
- `automated_flywheel_setup_checker status --format json`
- `... list/validate/classify-error --format jsonl`

**vc Ingestion Tables:**
```sql
afsc_run_facts(machine_id, run_id, ts, status, duration_ms, error_category, raw_json)
afsc_event_logs(machine_id, ts, event_type, severity, message, raw_json)
```

### 3.14 Cloud Benchmarker

**Goal:** Baseline + drift of VPS machine performance.

**Integration:**
- `GET /data/raw/` and `GET /data/overall/` from its FastAPI server
- Or read SQLite directly

**vc Ingestion Tables:**
```sql
cloud_bench_raw(machine_id, collected_at, benchmark_type, value, raw_json)
cloud_bench_overall(machine_id, collected_at, overall_score, subscores_json)
```

### 3.15 Fallback System Probe

Even if no tools are installed remotely, vc can collect baseline health:

```bash
uptime
df -P
free -b
cat /proc/loadavg
cat /proc/meminfo
```

This collector is **always enabled** as a fallback.

### 3.16 Collector Contract (The `Collector` Trait)

Every upstream integration implements a strict contract for debuggability and evolution:

```rust
pub trait Collector: Send + Sync {
    /// Unique identifier for this collector
    fn name(&self) -> &'static str;

    /// Schema version for output normalization
    fn schema_version(&self) -> u32;

    /// Perform collection with context and constraints
    async fn collect(&self, ctx: &CollectContext) -> CollectResult;
}

pub struct CollectContext {
    pub machine_id: MachineId,
    pub executor: RemoteExecutor,      // local or SSH
    pub last_cursor: Option<Cursor>,   // for incremental collection
    pub poll_window: Duration,         // e.g., "since 10 minutes ago"
    pub timeout: Duration,             // hard timeout for this collection
    pub max_bytes: usize,              // max output size
    pub max_rows: usize,               // max rows to insert
}

pub struct CollectResult {
    pub rows: Vec<RowBatch>,           // already normalized for DuckDB
    pub new_cursor: Option<Cursor>,    // updated cursor for next poll
    pub raw_artifacts: Vec<ArtifactRef>, // optional: store raw JSON for debugging
    pub warnings: Vec<Warning>,        // surface in UI
    pub duration: Duration,            // how long collection took
}

pub enum Cursor {
    Timestamp(DateTime<Utc>),          // for time-bounded queries
    FileOffset { inode: u64, offset: u64 }, // for JSONL tail
    PrimaryKey(i64),                   // for SQLite incremental
    Opaque(String),                    // for custom cursors
}
```

**Collector Design Goals:**
- **Idempotent inserts:** Same source payload should not create duplicates
- **Incremental by default:** Avoid rescanning large histories every poll
- **Versioned outputs:** Every collector has `collector_version` + `schema_version`
- **Fail-soft:** Broken collector degrades cockpit (shows "stale data"), not crash

### 3.17 Incremental Ingestion Patterns

Most sources fit one of these patterns—bake them in early:

**1) CLI Snapshot (stateless)**
- Run command, parse JSON, insert snapshot rows tagged with `collected_at`
- No cursor needed; each poll is a fresh snapshot
- Example: `caut usage --json`, `ru status --json`

```rust
// Pseudo-implementation
async fn collect(&self, ctx: &CollectContext) -> CollectResult {
    let output = ctx.executor.run("caut usage --json").await?;
    let usage: CautUsage = serde_json::from_str(&output)?;
    CollectResult {
        rows: vec![usage.into_row_batch()],
        new_cursor: None,  // stateless
        ..Default::default()
    }
}
```

**2) CLI Incremental Window (time-bounded)**
- Run command with `--since` or similar; store cursor as last-seen timestamp
- Example: `rano export --since 10m --format jsonl`

```rust
async fn collect(&self, ctx: &CollectContext) -> CollectResult {
    let since = match &ctx.last_cursor {
        Some(Cursor::Timestamp(ts)) => ts.to_rfc3339(),
        _ => "10m".to_string(),
    };
    let output = ctx.executor.run(&format!("rano export --since {} --format jsonl", since)).await?;
    let events: Vec<RanoEvent> = parse_jsonl(&output);
    let latest_ts = events.iter().map(|e| e.ts).max();
    CollectResult {
        rows: events.into_iter().map(|e| e.into_row()).collect(),
        new_cursor: latest_ts.map(Cursor::Timestamp),
        ..Default::default()
    }
}
```

**3) JSONL Tail (file offset cursor)**
- Maintain `(path, inode, offset)` per machine/source
- On rotation/inode change: fall back to "last N minutes" scan or reset
- Example: `~/.config/dcg/history.jsonl`

```rust
async fn collect(&self, ctx: &CollectContext) -> CollectResult {
    let path = "~/.config/dcg/history.jsonl";
    let (current_inode, current_size) = ctx.executor.stat(path).await?;

    let (start_offset, rotated) = match &ctx.last_cursor {
        Some(Cursor::FileOffset { inode, offset }) if *inode == current_inode => {
            (*offset, false)
        }
        _ => (0, true),  // file rotated or first run
    };

    if start_offset >= current_size {
        return CollectResult::empty();  // no new data
    }

    let new_bytes = ctx.executor.read_range(path, start_offset, current_size).await?;
    let events: Vec<DcgEvent> = parse_jsonl(&new_bytes);

    CollectResult {
        rows: events.into_iter().map(|e| e.into_row()).collect(),
        new_cursor: Some(Cursor::FileOffset { inode: current_inode, offset: current_size }),
        warnings: if rotated { vec![Warning::FileRotated] } else { vec![] },
        ..Default::default()
    }
}
```

**4) SQLite Incremental (primary key cursor)**
- Keep "last seen message_id/created_ts"
- Query `WHERE created_ts > ?` or `id > ?` each poll
- Example: mcp_agent_mail `messages` table

```rust
async fn collect(&self, ctx: &CollectContext) -> CollectResult {
    let last_id = match &ctx.last_cursor {
        Some(Cursor::PrimaryKey(id)) => *id,
        _ => 0,
    };

    let query = format!(
        "SELECT * FROM messages WHERE id > {} ORDER BY id LIMIT {}",
        last_id, ctx.max_rows
    );
    let rows = ctx.executor.sqlite_query("storage.sqlite3", &query).await?;
    let max_id = rows.iter().map(|r| r.id).max().unwrap_or(last_id);

    CollectResult {
        rows: rows.into_iter().map(|r| r.into_row_batch()).collect(),
        new_cursor: Some(Cursor::PrimaryKey(max_id)),
        ..Default::default()
    }
}
```

**5) Prometheus Scrape (metric samples)**
- Scrape `/metrics` text; parse into `(metric, labels, value, ts)`
- Downsample aggressively; store only what you chart/alert on
- Example: rch metrics endpoint

```rust
async fn collect(&self, ctx: &CollectContext) -> CollectResult {
    let metrics_text = ctx.executor.http_get("http://localhost:9100/metrics").await?;
    let samples = prometheus_parse(&metrics_text)?;

    // Filter to metrics we care about
    let relevant: Vec<_> = samples.into_iter()
        .filter(|s| TRACKED_METRICS.contains(&s.name))
        .collect();

    CollectResult {
        rows: relevant.into_iter().map(|s| s.into_row()).collect(),
        new_cursor: None,  // prometheus is stateless
        ..Default::default()
    }
}
```

---

## Part IV: Machine Model (Local + Remote)

### 4.1 Machine Inventory

vc needs a durable inventory of machines:

```toml
# vc.toml
[vc]
db_path = "~/.local/share/vc/vc.duckdb"
data_dir = "~/.local/share/vc"

[polling]
default_interval_seconds = 120
max_machines_in_flight = 8
max_collectors_in_flight_per_machine = 4

[[machines]]
id = "orko"
ssh = "local"  # special-case local execution
tags = ["primary", "claude", "codex"]

[[machines]]
id = "sydneymc"
ssh = "ubuntu@sydneymc.internal:22"
tags = ["worker", "rch", "claude"]

[[machines]]
id = "mac-mini"
ssh = "admin@mac-mini.local:22"
tags = ["gemini", "backup"]

[[machines]]
id = "gpu-box"
ssh = "ubuntu@10.0.0.50:22"
tags = ["gpu", "rch-worker", "codex"]
```

**Machine Properties:**
- `machine_id` (stable, human-readable)
- ssh target (host/user/port, optional ProxyJump)
- roles (main, worker, storage, etc.)
- tags (gpu, fast-net, low-cost, etc.)
- installed tool availability (auto-detected and cached)

### 4.2 Remote Execution Mechanism (MVP)

**MVP:** Run commands over SSH with a small wrapper:
- Use `ssh` binary with strict timeouts and `BatchMode=yes`
- Later upgrade to Rust SSH client if needed (e.g., `russh`)

**Tool Detection:**
- First connect, run `command -v <tool>` for each tool
- Cache results in DuckDB `machine_tool_capabilities` table with TTL (24h)

```sql
CREATE TABLE machine_tool_capabilities(
    machine_id TEXT,
    tool_name TEXT,
    available BOOLEAN,
    version TEXT,
    checked_at TIMESTAMP,
    PRIMARY KEY (machine_id, tool_name)
);
```

### 4.3 Remote Acquisition Patterns

Four patterns for predictable, low-risk remote data collection:

**1) Run Remote Command, Read stdout JSON**
```bash
ssh <target> "<cmd> --json"
```
Best when upstream tool provides robot/JSON output.

**2) Read Remote File (JSONL/SQLite)**
```bash
ssh <target> "tail -n <N> <path>"  # JSONL
ssh <target> "sqlite3 <db> '<query>'"  # SQLite
```
Store file cursor state in DuckDB.

**3) Fetch Remote Export Artifact**
- Trigger remote export into temp file, then `scp` it back
- Useful when stdout is too large

**4) Fallback Basic System Probe**
- Even without tools: `uptime`, `df -P`, `free -b`, `/proc/loadavg`
- Always enabled as baseline health check

---

## Part V: DuckDB Storage Design

### 5.1 Table Taxonomy

Two styles:
- **Append-only event tables** (facts) — immutable log
- **Point-in-time snapshot tables** (facts with `collected_at`) — latest state

**Always include:**
- `collected_at` (timestamp)
- `machine_id`
- `source` (collector name)
- `source_version` (semantic version string)
- `schema_version` (integer)
- `raw_json` (optional, for forward compatibility)

### 5.2 Core Tables (MVP)

```sql
-- ═══════════════════════════════════════════════════════════════════════════
-- MACHINES
-- ═══════════════════════════════════════════════════════════════════════════

CREATE TABLE machines(
    machine_id TEXT PRIMARY KEY,
    tags_json TEXT,
    ssh_target TEXT,
    first_seen_at TIMESTAMP,
    last_seen_at TIMESTAMP
);

CREATE TABLE machine_tool_capabilities(
    machine_id TEXT,
    tool_name TEXT,
    available BOOLEAN,
    version TEXT,
    checked_at TIMESTAMP,
    PRIMARY KEY (machine_id, tool_name)
);

-- ═══════════════════════════════════════════════════════════════════════════
-- SYSTEM METRICS
-- ═══════════════════════════════════════════════════════════════════════════

CREATE TABLE sys_samples(
    machine_id TEXT,
    collected_at TIMESTAMP,
    cpu_total REAL,
    load1 REAL,
    load5 REAL,
    load15 REAL,
    mem_used_bytes BIGINT,
    mem_total_bytes BIGINT,
    swap_used_bytes BIGINT,
    disk_read_mbps REAL,
    disk_write_mbps REAL,
    net_rx_mbps REAL,
    net_tx_mbps REAL,
    raw_json TEXT,
    PRIMARY KEY (machine_id, collected_at)
);

CREATE TABLE sys_top_processes(
    machine_id TEXT,
    collected_at TIMESTAMP,
    pid INTEGER,
    comm TEXT,
    cpu_pct REAL,
    mem_bytes BIGINT,
    fd_count INTEGER,
    io_read_bytes BIGINT,
    io_write_bytes BIGINT,
    raw_json TEXT
);

-- ═══════════════════════════════════════════════════════════════════════════
-- REPOS
-- ═══════════════════════════════════════════════════════════════════════════

CREATE TABLE repos(
    machine_id TEXT,
    repo_id TEXT,  -- stable hash of url or path
    path TEXT,
    url TEXT,
    name TEXT,
    PRIMARY KEY (machine_id, repo_id)
);

CREATE TABLE repo_status_snapshots(
    machine_id TEXT,
    collected_at TIMESTAMP,
    repo_id TEXT,
    branch TEXT,
    dirty BOOLEAN,
    ahead INTEGER,
    behind INTEGER,
    raw_json TEXT
);

-- ═══════════════════════════════════════════════════════════════════════════
-- ACCOUNTS
-- ═══════════════════════════════════════════════════════════════════════════

CREATE TABLE account_usage_snapshots(
    machine_id TEXT,
    collected_at TIMESTAMP,
    provider TEXT,
    account TEXT,
    window TEXT,
    used_percent REAL,
    resets_at TIMESTAMP,
    credits_remaining REAL,
    status TEXT,
    raw_json TEXT
);

CREATE TABLE account_profile_snapshots(
    machine_id TEXT,
    collected_at TIMESTAMP,
    tool TEXT,
    active_profile TEXT,
    health_expires_at TIMESTAMP,
    raw_json TEXT
);

-- ═══════════════════════════════════════════════════════════════════════════
-- SESSIONS (CASS)
-- ═══════════════════════════════════════════════════════════════════════════

CREATE TABLE cass_index_status(
    machine_id TEXT,
    collected_at TIMESTAMP,
    state TEXT,
    total_sessions INTEGER,
    last_index_at TIMESTAMP,
    raw_json TEXT,
    PRIMARY KEY (machine_id, collected_at)
);

CREATE TABLE cass_stats_snapshots(
    machine_id TEXT,
    collected_at TIMESTAMP,
    metric_name TEXT,
    metric_value REAL,
    dimensions_json TEXT,
    raw_json TEXT
);

-- ═══════════════════════════════════════════════════════════════════════════
-- AGENT MAIL
-- ═══════════════════════════════════════════════════════════════════════════

CREATE TABLE mail_messages(
    collected_at TIMESTAMP,
    project_id TEXT,
    message_id INTEGER,
    thread_id TEXT,
    sender TEXT,
    importance TEXT,
    ack_required BOOLEAN,
    created_ts TIMESTAMP,
    subject TEXT,
    raw_json TEXT,
    PRIMARY KEY (project_id, message_id)
);

CREATE TABLE mail_recipients(
    collected_at TIMESTAMP,
    message_id INTEGER,
    recipient TEXT,
    read_ts TIMESTAMP,
    ack_ts TIMESTAMP,
    raw_json TEXT
);

CREATE TABLE mail_file_reservations(
    collected_at TIMESTAMP,
    project_id TEXT,
    reservation_id INTEGER,
    path_pattern TEXT,
    holder TEXT,
    expires_ts TIMESTAMP,
    exclusive BOOLEAN,
    raw_json TEXT
);

-- ═══════════════════════════════════════════════════════════════════════════
-- DCG (Destructive Command Guard)
-- ═══════════════════════════════════════════════════════════════════════════

CREATE TABLE dcg_events(
    machine_id TEXT,
    ts TIMESTAMP,
    decision TEXT,
    rule_id TEXT,
    pack_id TEXT,
    severity TEXT,
    cwd TEXT,
    command_hash TEXT,
    raw_json TEXT
);

-- ═══════════════════════════════════════════════════════════════════════════
-- RCH (Remote Compilation Helper)
-- ═══════════════════════════════════════════════════════════════════════════

CREATE TABLE rch_status_snapshots(
    machine_id TEXT,
    collected_at TIMESTAMP,
    daemon_state TEXT,
    workers_total INTEGER,
    workers_available INTEGER,
    builds_active INTEGER,
    queue_depth INTEGER,
    raw_json TEXT,
    PRIMARY KEY (machine_id, collected_at)
);

CREATE TABLE rch_metric_samples(
    machine_id TEXT,
    ts TIMESTAMP,
    metric_name TEXT,
    labels_json TEXT,
    value REAL
);

-- ═══════════════════════════════════════════════════════════════════════════
-- RANO (Network Observer)
-- ═══════════════════════════════════════════════════════════════════════════

CREATE TABLE rano_events(
    machine_id TEXT,
    ts TIMESTAMP,
    event TEXT,
    provider TEXT,
    pid INTEGER,
    comm TEXT,
    remote_ip TEXT,
    remote_port INTEGER,
    domain TEXT,
    duration_ms BIGINT,
    raw_json TEXT
);

-- ═══════════════════════════════════════════════════════════════════════════
-- BEADS
-- ═══════════════════════════════════════════════════════════════════════════

CREATE TABLE beads_triage_snapshots(
    machine_id TEXT,
    collected_at TIMESTAMP,
    repo_id TEXT,
    quick_ref_json TEXT,
    recommendations_json TEXT,
    raw_json TEXT
);

CREATE TABLE beads_issues(
    repo_id TEXT,
    issue_id TEXT,
    status TEXT,
    priority TEXT,
    labels_json TEXT,
    deps_json TEXT,
    updated_at TIMESTAMP,
    raw_json TEXT,
    PRIMARY KEY (repo_id, issue_id)
);

-- ═══════════════════════════════════════════════════════════════════════════
-- FLEET STATE (for time travel)
-- ═══════════════════════════════════════════════════════════════════════════

CREATE TABLE fleet_state_snapshots(
    collected_at TIMESTAMP PRIMARY KEY,
    hash TEXT,
    fleet_health_score REAL,
    risk_level TEXT,
    summary_json TEXT
);

CREATE TABLE machine_state_snapshots(
    machine_id TEXT,
    collected_at TIMESTAMP,
    health_score REAL,
    summary_json TEXT,
    PRIMARY KEY (machine_id, collected_at)
);

-- ═══════════════════════════════════════════════════════════════════════════
-- INCIDENTS
-- ═══════════════════════════════════════════════════════════════════════════

CREATE TABLE incidents(
    incident_id TEXT PRIMARY KEY,
    opened_at TIMESTAMP,
    closed_at TIMESTAMP,
    severity TEXT,
    title TEXT,
    status TEXT,
    primary_machine_id TEXT,
    root_cause_json TEXT
);

CREATE TABLE incident_timeline_events(
    incident_id TEXT,
    ts TIMESTAMP,
    source TEXT,
    event_type TEXT,
    summary TEXT,
    evidence_json TEXT
);

CREATE TABLE incident_notes(
    incident_id TEXT,
    ts TIMESTAMP,
    author TEXT,
    note_md TEXT
);

-- ═══════════════════════════════════════════════════════════════════════════
-- HEALTH & ALERTS
-- ═══════════════════════════════════════════════════════════════════════════

CREATE TABLE health_factors(
    machine_id TEXT,
    collected_at TIMESTAMP,
    factor_id TEXT,
    severity TEXT,
    score REAL,
    details_json TEXT
);

CREATE TABLE health_summary(
    machine_id TEXT,
    collected_at TIMESTAMP,
    overall_score REAL,
    worst_factor_id TEXT,
    details_json TEXT,
    PRIMARY KEY (machine_id, collected_at)
);

CREATE TABLE alerts(
    alert_id TEXT PRIMARY KEY,
    machine_id TEXT,
    alert_type TEXT,
    severity TEXT,
    first_seen_at TIMESTAMP,
    last_seen_at TIMESTAMP,
    state TEXT,  -- open, acked, closed
    evidence_json TEXT,
    suggested_actions_json TEXT
);

-- ═══════════════════════════════════════════════════════════════════════════
-- PREDICTIONS (Oracle)
-- ═══════════════════════════════════════════════════════════════════════════

CREATE TABLE predictions(
    machine_id TEXT,
    generated_at TIMESTAMP,
    prediction_type TEXT,
    horizon_minutes INTEGER,
    confidence REAL,
    details_json TEXT
);

-- ═══════════════════════════════════════════════════════════════════════════
-- KNOWLEDGE BASE
-- ═══════════════════════════════════════════════════════════════════════════

CREATE TABLE gotchas(
    gotcha_id TEXT PRIMARY KEY,
    title TEXT,
    description TEXT,
    triggers_json TEXT,
    failure_pattern TEXT,
    symptoms_json TEXT,
    prevention TEXT,
    fix TEXT,
    discovered_at TIMESTAMP,
    discovered_by TEXT,
    occurrences INTEGER,
    last_occurrence TIMESTAMP
);

CREATE TABLE playbooks(
    playbook_id TEXT PRIMARY KEY,
    name TEXT,
    description TEXT,
    applicable_when_json TEXT,
    steps_json TEXT,
    success_criteria_json TEXT,
    executions INTEGER,
    success_rate REAL,
    avg_duration_seconds REAL
);

CREATE TABLE mined_solutions(
    solution_id TEXT PRIMARY KEY,
    problem_pattern TEXT,
    solution_steps_json TEXT,
    success_rate REAL,
    applicable_contexts_json TEXT,
    source_sessions_json TEXT,
    confidence REAL,
    created_at TIMESTAMP
);

-- ═══════════════════════════════════════════════════════════════════════════
-- INGESTION STATE
-- ═══════════════════════════════════════════════════════════════════════════

CREATE TABLE ingestion_cursors(
    machine_id TEXT,
    source TEXT,
    cursor_key TEXT,
    cursor_value TEXT,
    updated_at TIMESTAMP,
    PRIMARY KEY (machine_id, source, cursor_key)
);

-- ═══════════════════════════════════════════════════════════════════════════
-- SCHEMA MIGRATIONS
-- ═══════════════════════════════════════════════════════════════════════════

CREATE TABLE schema_migrations(
    version INTEGER PRIMARY KEY,
    applied_at TIMESTAMP,
    description TEXT,
    checksum TEXT
);

-- ═══════════════════════════════════════════════════════════════════════════
-- AUDIT LOG
-- ═══════════════════════════════════════════════════════════════════════════

CREATE TABLE audit_events(
    ts TIMESTAMP,
    event_type TEXT,  -- collector_run, autopilot_action, user_command
    actor TEXT,       -- collector name, autopilot, user
    machine_id TEXT,
    action TEXT,
    result TEXT,
    details_json TEXT
);
```

### 5.3 Retention Policy

DuckDB is fast, but plan retention early:

| Data Type | Full Resolution | Aggregates |
|-----------|-----------------|------------|
| System samples | 7-30 days | Forever |
| Event logs (dcg, rano) | 90 days raw | Forever |
| Snapshots | 30 days | Forever |
| Derived rollups | N/A | Forever |

**Implementation:**
- Periodic `vc vacuum` job with opt-in deletion
- Compact to Parquet partitions, then truncate raw tables
- Given "no deletion" discipline, make retention **opt-in and transparent**

### 5.4 Time Machine: Snapshots & Incident Replay

**Fleet State Snapshots:**
- Periodically materialize "whole fleet" snapshot
- Cheap to render and easy to diff
- Stored in `fleet_state_snapshots` with `collected_at` as key

**Time Travel Queries:**
```sql
-- State at a point in time (latest snapshot at or before T)
SELECT * FROM fleet_state_snapshots
WHERE collected_at <= TIMESTAMP '2026-01-25 03:00:00'
ORDER BY collected_at DESC
LIMIT 1;

-- Context window around an incident
SELECT * FROM dcg_events
WHERE ts BETWEEN TIMESTAMP '2026-01-25 02:45:00'
         AND TIMESTAMP '2026-01-25 03:45:00'
ORDER BY ts;
```

**Incident Reconstruction:**
An incident is a bounded time range with a root cause hypothesis and a reconstructed timeline. Use `incident_timeline_events` to correlate signals from multiple sources.

---

## Part VI: Revolutionary Capabilities

### 6.1 THE SWARM INTELLIGENCE ENGINE

This isn't just collecting metrics—it's building a **cognitive model** of your entire agent fleet.

#### Agent DNA Profiling

Every agent instance develops a unique "DNA profile" based on observed behavior:

```rust
pub struct AgentDNA {
    // Performance genetics
    pub avg_velocity: f64,              // tokens/min baseline
    pub velocity_variance: f64,         // consistency score
    pub context_efficiency: f64,        // useful work per context %
    pub error_rate: f64,                // failures per 100 tasks
    pub recovery_speed: f64,            // time to recover from errors

    // Behavioral traits
    pub task_affinity: HashMap<TaskType, f64>,  // what it's good at
    pub time_of_day_performance: [f64; 24],     // circadian patterns
    pub collaboration_score: f64,               // works well with others?
    pub autonomy_level: f64,                    // needs hand-holding?

    // Environmental preferences
    pub optimal_context_range: (f64, f64),      // sweet spot
    pub preferred_repos: Vec<String>,           // where it shines
    pub resource_footprint: ResourceProfile,    // CPU/mem/net needs

    // Genealogy
    pub parent_config: Option<ConfigHash>,      // what spawned it
    pub mutation_history: Vec<ConfigDelta>,     // how it evolved
    pub generation: u32,                        // evolutionary age
}
```

**Why this matters:** You can ask "Which agent configuration is best for complex refactoring tasks in Rust codebases?" and get a data-driven answer.

#### Emergent Behavior Detection

The system watches for patterns that emerge from agent interactions:

```rust
pub enum EmergentPattern {
    // Positive emergence
    SynergyCluster {
        agents: Vec<AgentId>,
        performance_boost: f64,  // "These 3 agents together are 40% faster"
    },

    // Negative emergence
    ResourceContention {
        agents: Vec<AgentId>,
        bottleneck: Resource,
        degradation: f64,
    },

    // Interesting emergence
    SpecializationDrift {
        agent: AgentId,
        original_role: TaskType,
        evolved_role: TaskType,
        efficiency_delta: f64,
    },

    // Concerning emergence
    CascadeRisk {
        trigger_agent: AgentId,
        affected_agents: Vec<AgentId>,
        failure_probability: f64,
    },
}
```

### 6.2 THE PREDICTIVE ORACLE

Don't react to problems—**prevent them**.

#### Rate Limit Forecasting

```rust
pub struct RateLimitForecast {
    pub provider: Provider,
    pub account: AccountId,

    // Current state
    pub current_usage_pct: f64,
    pub current_velocity: f64,  // consumption rate

    // Predictions
    pub time_to_limit: Duration,
    pub confidence: f64,
    pub recommended_action: RateLimitAction,

    // Optimization
    pub optimal_swap_time: Option<Instant>,  // "Swap accounts in 12 minutes"
    pub alternative_accounts: Vec<(AccountId, f64)>,  // ranked alternatives
}

pub enum RateLimitAction {
    Continue,                           // All good
    SlowDown { target_velocity: f64 },  // Pace yourself
    PrepareSwap { in_minutes: u32 },    // Get ready
    SwapNow { to_account: AccountId },  // Do it now
    EmergencyPause,                     // Stop everything
}
```

#### Failure Prophecy

Models that predict failures before they happen:

```rust
pub struct FailureProphecy {
    pub agent: AgentId,
    pub predicted_failure: FailureType,
    pub probability: f64,
    pub time_horizon: Duration,
    pub early_indicators: Vec<Indicator>,
    pub preventive_actions: Vec<Action>,
    pub confidence_factors: Vec<(String, f64)>,
}

// Example prophecy:
// "Agent cc_3 on orko has 78% chance of context overflow in next 15 minutes
//  based on: velocity acceleration (+23%), similar past patterns (n=47),
//  current task complexity (high). Recommended: preemptive compaction or
//  task handoff to cc_4 which has 34% context headroom."
```

#### Cost Oracle

```rust
pub struct CostForecast {
    pub timeframe: Duration,

    // Predictions
    pub projected_cost: Money,
    pub cost_by_provider: HashMap<Provider, Money>,
    pub cost_by_project: HashMap<ProjectId, Money>,
    pub cost_by_task_type: HashMap<TaskType, Money>,

    // Optimization opportunities
    pub savings_opportunities: Vec<SavingsOpportunity>,
    pub roi_analysis: ROIAnalysis,

    // Recommendations
    pub optimal_account_allocation: AccountAllocation,
    pub suggested_throttling: Option<ThrottlingPlan>,
}
```

### 6.3 THE AUTONOMOUS GUARDIAN

The system doesn't just alert—it **acts**.

#### Self-Healing Protocols

Every automated action follows a structured playbook protocol:

```rust
pub struct HealingProtocol {
    pub name: &'static str,

    // When to activate
    pub trigger: Condition,

    // What to check before acting
    pub diagnosis: Vec<DiagnosisStep>,

    // Actions to take (in order)
    pub treatment: Vec<TreatmentStep>,

    // How to verify success
    pub verification: VerificationStep,

    // How to revert if something goes wrong
    pub rollback: Option<RollbackPlan>,

    // When to stop and alert a human
    pub escalation: EscalationPath,
}

// Example protocols:

// Rate Limit Healing
HealingProtocol {
    name: "rate_limit_recovery",
    trigger: |state| state.usage_pct > 85,
    diagnosis: vec![
        check_current_velocity(),
        forecast_time_to_limit(),
        find_alternative_accounts(),
    ],
    treatment: vec![
        gradual_throttle(0.7),
        prepare_account_swap(),
        if time_to_limit < 10.minutes() { execute_swap() },
    ],
    verification: confirm_new_account_healthy(),
    rollback: Some(revert_to_previous_account()),
    escalation: after_3_failures(alert_human()),
}

// Stuck Agent Healing
HealingProtocol {
    name: "stuck_agent_recovery",
    trigger: |state| state.velocity < 10 && state.context_usage > 50,
    diagnosis: vec![
        check_for_error_loops(),
        analyze_recent_outputs(),
        compare_to_healthy_agents(),
    ],
    treatment: vec![
        send_interrupt_signal(),
        wait(30.seconds()),
        if still_stuck { force_compaction() },
        inject_recovery_prompt(),
    ],
    verification: velocity_above(100).within(2.minutes()),
    rollback: Some(restart_agent_from_checkpoint()),
    escalation: create_incident_report(),
}

// Disk Space Healing
HealingProtocol {
    name: "disk_space_recovery",
    trigger: |state| state.disk_usage > 90,
    diagnosis: vec![analyze_disk_consumers()],
    treatment: vec![
        clear_agent_caches(),
        rotate_old_logs(),
        compress_session_data(),
        if still_critical { pause_low_priority_agents() },
    ],
    verification: disk_usage_below(80),
    rollback: None,  // Can't undo deletions
    escalation: immediate_if(disk_usage > 95),
}
```

#### Fleet Orchestration Commands

```rust
pub enum FleetCommand {
    // Scaling
    SpawnAgents { agent_type: AgentType, count: u32, machine: MachineId, config: AgentConfig },

    // Load balancing
    RebalanceFleet { strategy: RebalanceStrategy, constraints: Vec<Constraint> },

    // Emergency
    EmergencyStop { scope: StopScope, reason: String },

    // Migration
    MigrateWorkload { from: MachineId, to: MachineId, workload: WorkloadSpec },

    // Experiments
    CanaryDeploy { new_config: AgentConfig, percentage: f64, success_criteria: Criteria },
}

// CLI examples:
// vc fleet spawn --type claude --count 5 --machine gpu-box --model opus
// vc fleet rebalance --strategy even-load --preserve-locality
// vc fleet emergency-stop --scope machine:orko --reason "maintenance"
// vc fleet migrate --from orko --to sydneymc --workload "repo:smartedgar*"
// vc fleet canary --config new_prompt_v2.toml --percentage 10 --auto-rollback
```

#### Intelligent Work Distribution

```rust
pub struct WorkDistributor {
    pub fn assign_task(&self, task: &Task) -> Assignment {
        let candidates = self.find_capable_agents(task);

        // Score each candidate on:
        // - Agent DNA affinity for task type
        // - Current load and headroom
        // - Historical performance on similar tasks
        // - Geographic locality (same machine as repo?)
        // - Account rate limit status
        // - Collaboration requirements
        // - Cost optimization

        let scored = candidates.iter()
            .map(|agent| (agent, self.score_assignment(agent, task)))
            .collect();

        self.select_optimal(scored, task.constraints)
    }
}

pub struct Assignment {
    pub agent: AgentId,
    pub confidence: f64,
    pub reasoning: Vec<String>,
    pub alternatives: Vec<(AgentId, f64)>,
    pub estimated_completion: Duration,
    pub estimated_cost: Money,
}
```

### 6.4 THE KNOWLEDGE CRYSTALLIZER

Transform ephemeral agent work into permanent institutional knowledge.

#### Solution Mining

```rust
pub struct SolutionMiner {
    pub async fn mine_solutions(&self) -> Vec<MinedSolution> {
        let successful_sessions = self.find_successful_sessions();

        for session in successful_sessions {
            // Extract key decision points
            let decisions = self.extract_decisions(session);

            // Identify what made this approach work
            let success_factors = self.analyze_success_factors(session);

            // Generalize into reusable patterns
            let pattern = self.generalize_pattern(decisions, success_factors);

            // Store for future reference
            self.knowledge_base.store(pattern);
        }
    }
}

pub struct MinedSolution {
    pub problem_pattern: ProblemSignature,
    pub solution_steps: Vec<Step>,
    pub success_rate: f64,
    pub applicable_contexts: Vec<Context>,
    pub source_sessions: Vec<SessionId>,
    pub confidence: f64,
}
```

#### Gotcha Database

```rust
pub struct Gotcha {
    pub id: GotchaId,
    pub title: String,
    pub description: String,

    // When to surface this gotcha
    pub triggers: Vec<GotchaTrigger>,

    // What went wrong
    pub failure_pattern: FailurePattern,
    pub symptoms: Vec<Symptom>,

    // How to avoid/fix
    pub prevention: String,
    pub fix: String,

    // Provenance
    pub discovered_at: DateTime<Utc>,
    pub discovered_by: AgentId,
    pub occurrences: u32,
    pub last_occurrence: DateTime<Utc>,
}

// Example gotchas:
//
// "beads_rust sync --flush-only doesn't actually flush the WAL"
//   Trigger: Agent runs br sync --flush-only before git commit
//   Fix: Run PRAGMA wal_checkpoint(TRUNCATE) first
//   Discovered: 2026-01-15 by cc_3, occurred 12 times
//
// "Codex loses context when switching between Rust and TypeScript"
//   Trigger: Task involves both languages in same session
//   Fix: Split into separate focused sessions
//   Discovered: 2026-01-08 by codex_2, occurred 8 times
```

#### Performance Playbooks

```rust
pub struct Playbook {
    pub name: String,
    pub description: String,

    // When to use
    pub applicable_when: Vec<Condition>,

    // Steps with the standard protocol
    pub trigger: PlaybookTrigger,
    pub diagnosis: Vec<DiagnosisStep>,
    pub treatment: Vec<TreatmentStep>,
    pub verification: VerificationStep,
    pub rollback: Option<RollbackPlan>,
    pub escalation: EscalationPath,

    // Expected outcomes
    pub success_criteria: Vec<Criterion>,
    pub expected_duration: Duration,
    pub expected_cost: Money,

    // Track record
    pub executions: u32,
    pub success_rate: f64,
    pub avg_actual_duration: Duration,
}

// Example playbooks:
//
// "New Project Onboarding"
//   When: Starting work on unfamiliar codebase
//   Diagnosis: Check if codebase map exists, assess complexity
//   Treatment:
//     1. Run cass search for similar patterns
//     2. Generate codebase map with bv
//     3. Assign 1 agent for exploration
//     4. Create initial beads from findings
//     5. Begin implementation with 3 agents
//   Verification: Initial beads created, agents productive
//   Rollback: None needed (exploration is safe)
//   Escalation: If exploration agent stuck > 30 min
//   Success: 94% (32/34 executions)
```

### 6.5 THE EXPERIMENTATION ENGINE

Continuous improvement through controlled experiments.

#### A/B Testing Framework

```rust
pub struct Experiment {
    pub id: ExperimentId,
    pub name: String,
    pub hypothesis: String,

    // What we're testing
    pub control: ExperimentArm,
    pub treatment: ExperimentArm,

    // How we're measuring
    pub metrics: Vec<MetricDefinition>,
    pub primary_metric: MetricId,

    // Guardrails
    pub duration: Duration,
    pub min_sample_size: u32,
    pub stop_conditions: Vec<StopCondition>,
    pub auto_rollback: bool,

    // Results
    pub status: ExperimentStatus,
    pub results: Option<ExperimentResults>,
}

// Example experiment:
// "System Prompt v2 for Rust Tasks"
//   Hypothesis: New system prompt increases velocity by 15%
//   Control: Current prompt
//   Treatment: New prompt with explicit type inference hints
//   Status: RUNNING (day 3, n=47)
//   Preliminary: +22% velocity, -8% errors (p=0.03)
```

#### Evolutionary Optimization

```rust
pub struct EvolutionaryOptimizer {
    pub population: Vec<AgentConfig>,
    pub fitness: Box<dyn Fn(&AgentConfig, &PerformanceData) -> f64>,

    pub mutation_rate: f64,
    pub crossover_rate: f64,
    pub selection_pressure: f64,

    pub fn evolve(&mut self) -> Vec<AgentConfig> {
        let selected = self.tournament_select();
        let offspring = self.crossover(selected);
        let mutated = self.mutate(offspring);
        let evaluated = self.evaluate_fitness(mutated);
        self.population = evaluated;

        // Return top performers for deployment
        self.population.iter()
            .sorted_by_fitness()
            .take(3)
            .cloned()
            .collect()
    }
}

// After 50 generations:
// "Evolved optimal config for Rust refactoring:
//  - Context window: 75% max (not 90%)
//  - Temperature: 0.3 (lower than default)
//  - System prompt: variant_47 (emphasizes types)
//  - Best on: morning tasks (6am-noon)
//  - Fitness: 0.94 (vs 0.71 baseline)"
```

---

## Part VII: User Interfaces

### 7.1 Neural TUI (Primary)

Use `ratatui` + `crossterm`.

**Navigation Principles:**
- Single keypress to jump between core dashboards
- Persistent filter bar (machine / repo / provider / time window)
- "Drill-down" from global -> machine -> repo -> event detail
- Integrated search (delegates to cass for content; local for vc facts)

**TUI Keybindings:**

```
Global:
  ?     Help overlay
  /     Search (cass for content, vc for facts)
  f     Filter bar (machine/repo/provider/time)
  r     Refresh now
  Esc   Back / close modal
  q     Quit

Overview screen:
  Enter Drill into selected machine
  a     Accounts tab
  g     Git/repos tab
  m     Mail tab
  b     Beads tab
  e     Events tab
  o     Oracle tab (predictions)
  G     Guardian tab (healing status)
```

**Display Guidelines:**
- Top row: global health + poll freshness + active alerts
- Left pane: selectable list (machines/repos/alerts)
- Right pane: details for selection

**Core Screens (MVP):**
1. Overview (global health, top alerts, machines list)
2. Machine detail (system charts + processes + rch + agent activity)
3. Repos (dirty, ahead/behind, recent changes)
4. Accounts (caut/caam usage + "recommend swap")
5. Agents/messages (mcp mail: urgent, ack required, file reservations)
6. Events (dcg denies, rano anomalies, pt findings)
7. Beads (bv triage, blockers, next picks)
8. Oracle (predictions, forecasts, risk assessment)
9. Guardian (healing status, active protocols, history)

**TUI Mockup:**

```
╔══════════════════════════════════════════════════════════════════════════════╗
║  V I B E   C O C K P I T                              ◉ NEURAL MODE ACTIVE   ║
║  ════════════════════════════════════════════════════════════════════════════║
║                                                                              ║
║  ┌─ SWARM TOPOLOGY ──────────────────────┐  ┌─ PREDICTIONS ───────────────┐ ║
║  │                                        │  │                             │ ║
║  │     ┌───┐         ┌───┐               │  │  ⚡ Rate Limit Forecast     │ ║
║  │     │ O │────────▶│ S │               │  │     claude-1: 23min to swap │ ║
║  │     │ r │    ╱    │ y │               │  │     openai-2: 4hr headroom  │ ║
║  │     │ k │───╱     │ d │               │  │                             │ ║
║  │     │ o │  ╱      │ n │               │  │  🔮 Failure Risk            │ ║
║  │     └───┘ ╱       │ e │               │  │     cc_5: 34% stuck (watch) │ ║
║  │       ▲  ╱        │ y │               │  │     codex_2: healthy        │ ║
║  │       │ ╱         └───┘               │  │                             │ ║
║  │     ┌───┐           │                 │  │  💰 Cost Trajectory         │ ║
║  │     │ M │◀──────────┘                 │  │     Today: $47 (on track)   │ ║
║  │     │ a │                             │  │     Week: $312 (▼8% budget) │ ║
║  │     │ c │────────▶ ┌───┐              │  │                             │ ║
║  │     └───┘          │GPU│              │  └─────────────────────────────┘ ║
║  │                    └───┘              │                                   ║
║  │  ●=healthy ◐=warning ○=critical       │  ┌─ GUARDIAN STATUS ───────────┐ ║
║  │  ─=data  ═=commands  ▶=offload       │  │                             │ ║
║  └────────────────────────────────────────┘  │  🛡️  Auto-Healing: ACTIVE   │ ║
║                                              │  📊 Patterns detected: 3    │ ║
║  ┌─ AGENT CONSTELLATION ──────────────────┐  │  🔄 Last action: 2min ago   │ ║
║  │                                        │  │     "Swapped claude acct"  │ ║
║  │   orko:  ●●●●●●◐●●●●●●●○              │  │                             │ ║
║  │          cc    codex  gem             │  │  Pending interventions: 1   │ ║
║  │                                        │  │  └─ cc_5 recovery (queued) │ ║
║  │   sydneymc: ●●●●●●●●●●●●              │  │                             │ ║
║  │             cc   codex                 │  └─────────────────────────────┘ ║
║  │                                        │                                   ║
║  │   mac-mini: ●●●●●●●●                  │                                   ║
║  │             gemini                     │                                   ║
║  └────────────────────────────────────────┘                                   ║
║                                                                              ║
║  ┌─ VELOCITY WAVES ──────────────────────────────────────────────────────┐  ║
║  │ 2000│      ╭─╮                      ╭──╮                               │  ║
║  │     │     ╱  ╲    ╭─╮              ╱    ╲         ╭╮                   │  ║
║  │ 1500│────╱────╲──╱──╲────────────╱──────╲───────╱╲────────────────   │  ║
║  │ 1000│──╱────────────────╱────╲╱──────────────╱────╲──────────────   │  ║
║  │  500│╱                ╱        ╲            ╱        ╲                 │  ║
║  │     └─────────────────────────────────────────────────────────────▶   │  ║
║  │       6:00    9:00    12:00    15:00    18:00    21:00    now         │  ║
║  └────────────────────────────────────────────────────────────────────────┘  ║
╚══════════════════════════════════════════════════════════════════════════════╝
 [F1]Fleet [F2]Agents [F3]Usage [F4]Tasks [F5]Alerts [F6]Oracle [F7]Guardian
```

### 7.2 Web Dashboard (Next.js served by Rust)

**Architecture:** Static Next.js build + Rust JSON API

Why:
- Simplest "served by Rust" story: Rust serves static assets + API
- Avoids running Node in production
- Reduces moving parts

**Plan:**
- `web/` directory contains Next.js app
- Next.js configured for static output
- Rust `vc_web` serves:
  - `GET /api/...` (JSON endpoints backed by DuckDB)
  - `GET /` static files (embedded or on-disk)

If SSR is needed later:
- Run a Node "standalone" server as a managed child process and reverse proxy via Rust

**API Endpoints:**

| Endpoint | Purpose |
|----------|---------|
| `GET /api/health` | Global + per-machine health summary |
| `GET /api/machines` | Machine inventory + tools + last_seen |
| `GET /api/machines/:id/overview` | System + alerts + top processes |
| `GET /api/repos` | Repo status (filterable by machine) |
| `GET /api/accounts` | caut/caam merged view + recommendations |
| `GET /api/alerts` | Open/acked/closed alerts |
| `GET /api/events/dcg` | Recent denies breakdown |
| `GET /api/events/rano` | Network summary |
| `GET /api/beads/:repo_id/triage` | bv outputs |
| `GET /api/oracle/forecasts` | Rate limit + failure predictions |
| `GET /api/guardian/status` | Healing protocol status |

**Auth (MVP):**
- Local-only binding by default (`127.0.0.1`)
- Optional `VC_WEB_TOKEN` header for remote access
- No cookies/sessions until multi-user needed

### 7.3 CLI Robot Mode (Agent-Friendly)

**Design Goals:**
- Stable schemas (version in envelope)
- No color noise
- stdout=data, stderr=diagnostics
- Low token output variants (TOON)

**Commands:**

```bash
vc robot health --json
vc robot triage --json          # Top issues + suggested commands
vc robot machine <id> --json
vc robot accounts --json
vc robot repos --json
vc robot alerts --json
vc robot oracle --json          # Predictions
vc robot guardian --json        # Healing status
vc query "<sql>" --format json  # Read-only SQL

vc robot-docs schemas           # Schema documentation
vc robot-docs examples          # Example outputs
```

**Robot Output Envelope:**

```json
{
  "schema_version": "vc.robot.health.v1",
  "generated_at": "2026-01-27T00:00:00Z",
  "data": {
    "overall": { "score": 0.82, "severity": "medium" },
    "machines": [
      { "id": "orko", "score": 0.91, "top_issue": "dcg_denies_spike" }
    ]
  },
  "staleness": { "sysmoni": 40, "ru": 120, "caut": 1800 },
  "warnings": []
}
```

**TOON (Token-Optimized Object Notation):**

For extremely token-constrained agents, provide a compact output format:

```
# Traditional JSON: 847 tokens
{"fleet":{"machines":[{"hostname":"orko","status":"online","agents":{"total":15...

# TOON: 312 tokens (63% reduction)
F:4on1off|orko:on,15ag(14h),cpu45,mem68|sydneymc:on,12ag,cpu78|mac:on,8ag|gpu:on,10ag|bak:off
AL:3[!ratelim@orko:95%,!zombie@syd:2,!disk@mac:89%]
PR:[ratelim:swap@23m,fail:cc5@34%,cost:+$12@eod]
```

### 7.4 Streaming Watch Mode

Add a `vc watch` mode that emits **JSONL** events at a fixed interval or on change:

```bash
vc watch --format jsonl --interval 30
```

Event types:
- `alert`: new or escalated alerts
- `prediction`: forecast risk crossing a threshold
- `opportunity`: optimization hints (cost saving, account swap)

This enables a guardian agent to monitor without polling full snapshots.

### 7.5 Natural Language Interface

```
You: "Why is orko slow right now?"

VC: "orko is experiencing elevated load due to 3 factors:

     1. CPU at 87% (normally 45%)
        → 4 Rust compilations running simultaneously
        → Recommendation: Offload 2 to gpu-box via rch

     2. Claude account jeff@email.com at 92% rate limit
        → Will hit limit in ~8 minutes at current velocity
        → Recommendation: Swap to backup@email.com now

     3. Agent cc_5 in error loop (velocity: 12 tokens/min)
        → Stuck on type inference in smartedgar_mcp
        → Recommendation: Inject recovery prompt or restart

     Would you like me to execute these recommendations?"
```

Implementation:
- Map natural language intents to predefined queries
- Avoid free-form SQL generation until strict guardrails exist
- Keep it opt-in and read-only

### 7.6 MCP Server Mode

vibe_cockpit itself becomes an MCP server that agents can query:

```rust
// MCP Tools exposed by vc

vc_fleet_status {
    description: "Get current status of the agent fleet",
    params: { scope: Option<Scope> },
    returns: FleetStatus,
}

vc_find_capacity {
    description: "Find agents with available capacity for work",
    params: { task_type: TaskType, estimated_tokens: u64 },
    returns: Vec<AvailableAgent>,
}

vc_request_resources {
    description: "Request computational resources for a task",
    params: { resource_type: ResourceType, amount: u64 },
    returns: ResourceAllocation,
}

vc_report_completion {
    description: "Report task completion with metrics",
    params: { task_id: TaskId, metrics: TaskMetrics },
    returns: Acknowledgment,
}

vc_get_recommendations {
    description: "Get AI-powered recommendations for current work",
    params: { context: WorkContext },
    returns: Vec<Recommendation>,
}
```

---

## Part VIII: Alerting & Autopilot

### 8.1 Alert Model

```rust
pub struct Alert {
    pub id: AlertId,
    pub machine_id: MachineId,
    pub alert_type: AlertType,
    pub severity: Severity,  // critical, high, medium, low
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub state: AlertState,  // open, acked, closed
    pub evidence: Evidence,
    pub suggested_actions: Vec<SuggestedAction>,
}
```

**Alert Delivery Channels:**

MVP:
- TUI highlight + web highlight
- Write to `alerts.jsonl` file

Later:
- Agent Mail message (thread per alert)
- Webhook/Discord/Slack
- Desktop notifications
- Mobile push

### 8.2 Concrete MVP Alert Rules

**System:**
| Rule | Threshold | Condition |
|------|-----------|-----------|
| Disk critical | < 10% | Per filesystem |
| Load high | Load5 > (cpu_cores × 1.5) | For > 10 minutes |
| Swap pressure | > 0 | And rising |
| OOM kills | Any | If accessible via dmesg |
| Inotify exhaustion | > 90% of max | Per sysmoni |

**Agents + Tooling:**
| Rule | Threshold | Condition |
|------|-----------|-----------|
| DCG denies spike | > 10/hour | OR new "critical" rule IDs |
| Unknown network | N/A | rano connections to unknown provider domains |
| Mail backlog | > 5 | mcp_agent_mail urgent-unread |
| Mail overdue | > 3 | ack_required messages unacked > 1 hour |
| RCH queue saturated | > 10 | queue_depth while workers_available == 0 |
| CASS stale | > 24 hours | Index not refreshed |

**Repos:**
| Rule | Threshold | Condition |
|------|-----------|-----------|
| Dirty repos spike | > 10 | Per machine (indicates stuck workflows) |

### 8.3 Autopilot Modes

```toml
[autopilot]
mode = "suggest"  # off | suggest | execute-safe | execute-with-approval

[autopilot.safe_commands]
# Only these can be executed in execute-safe mode
allow = [
    "caam recommend",
    "caam limits",
    "pt robot plan",
    "vc account swap *",
]
```

**Modes:**
- **off:** No automation
- **suggest:** Print recommendations but don't run them
- **execute-safe:** Run allowlisted commands automatically
- **execute-with-approval:** Create confirmation step in TUI/web before executing

### 8.4 Autopilot Playbooks

Suggestions first; execution requires explicit config:

| Scenario | Diagnosis | Treatment | Verification |
|----------|-----------|-----------|--------------|
| Account near limit | Check velocity, forecast time-to-limit | Gradual throttle, prepare swap, execute if < 10 min | New account healthy |
| Machine load high | Identify top processes, check for stuck agents | Use pt robot plan, suggest safe actions | Load returns to normal |
| Disk space critical | Analyze consumers | Clear caches, rotate logs, compress old data | Disk < 80% |
| RCH bottleneck | Check worker status | Suggest restart or removal of flaky worker | Queue draining |
| DCG spike | Correlate rule IDs with cwds | Show correlation, suggest behavior review | Denies decrease |

---

## Part IX: Security, Secrets, and Access Control

### 9.1 Threat Model

vc will see and sometimes execute sensitive operations across machines. Assume:
- Local attackers could read your home directory if permissions are lax
- Remote machines could be compromised and return malicious payloads
- Dashboards might get exposed accidentally if bound to 0.0.0.0

### 9.2 Secrets Handling

- **Do not store provider credentials in vc** — defer to upstream tools (caut/caam)
- vc stores only:
  - Machine inventory (ssh targets)
  - Optional web token
  - Optional alert webhooks
- Prefer OS keychains when available; otherwise store secrets in a dedicated file with `0600`

### 9.3 Remote Execution Safety

- Default remote execution is **read-only commands**
- Any command execution beyond read-only must be:
  - Explicitly enabled per-machine and per-command category
  - Logged in `audit_events` table
  - Visible in UI with "who/when/what"

```toml
[[machines]]
id = "worker-01"
ssh = "ubuntu@10.0.0.12:22"
allow_write_commands = false  # default
allow_autopilot = false       # default

[[machines]]
id = "orko"
ssh = "local"
allow_write_commands = true   # trusted local machine
allow_autopilot = true
```

### 9.4 Audit Logs

Persist all vc actions for debugging and accountability:

**Collector runs:**
- Collector name, machine_id, start/end time, duration
- Bytes parsed, rows inserted, errors/warnings

**Autopilot executions:**
- Exact command run
- Machine_id
- User confirmation state (if applicable)
- Result exit code
- Before/after state summary

**User commands:**
- All `vc` CLI invocations
- TUI/web interactions that trigger actions

---

## Part X: Implementation Phases

### Phase 0: Foundation

- [ ] Create Cargo workspace, `vc` binary, `vc.toml` config loader
- [ ] Create DuckDB file + migration framework
- [ ] Implement `Collector` trait with dummy collector
- [ ] Implement `vc robot health` returning stub JSON
- [ ] Basic CLI structure with clap

### Phase 1: System + Repo Basics

- [ ] `sysmoni` collector (local only)
- [ ] `ru` collector (`ru list --json`, `ru status --json`)
- [ ] DuckDB tables: `sys_samples`, `repo_status_snapshot`
- [ ] TUI: Overview screen + machine list + repo list
- [ ] `vc robot status` command

### Phase 2: Accounts + Sessions + Mail

- [ ] `caut` collector (`caut usage --json`)
- [ ] `caam` collector (`caam limits --format json`, `caam status --json`)
- [ ] `cass` collector (stats, timeline, health)
- [ ] `mcp_agent_mail` SQLite collector
- [ ] TUI screens: Accounts, Sessions, Mail
- [ ] Rate limit forecasting (Oracle MVP)

### Phase 3: Remote Machines

- [ ] Machine inventory in `vc.toml`
- [ ] SSH runner with timeouts and tool probing
- [ ] Remote collectors: sysmoni, ru, caut, caam
- [ ] TUI: Per-machine drill-down
- [ ] `machine_tool_capabilities` caching

### Phase 4: rch + rano + dcg + pt

- [ ] `rch` collector (status JSON or prometheus)
- [ ] `rano` collector (export jsonl)
- [ ] `dcg` collector (history.jsonl tail)
- [ ] `pt` collector (robot plan)
- [ ] Alerts MVP: threshold rules + persistence
- [ ] Guardian MVP: healing protocols

### Phase 5: Web Dashboard + Autopilot

- [ ] Next.js UI build + Rust server
- [ ] JSON API endpoints with auth
- [ ] Autopilot: suggest mode first
- [ ] Knowledge base: gotcha storage
- [ ] Agent DNA profiling MVP

### Phase 6: Intelligence

- [ ] Solution mining from cass sessions
- [ ] Playbook generation
- [ ] A/B testing framework
- [ ] Evolutionary optimization
- [ ] Natural language query interface
- [ ] MCP server mode

---

## Part XI: Success Metrics

### Operational Excellence

| Metric | Current | Target | Stretch |
|--------|---------|--------|---------|
| Mean time to detect anomaly | ~5 min | <30 sec | <10 sec |
| Mean time to remediate | ~15 min | <2 min | <30 sec |
| Rate limit incidents/week | ~10 | <2 | 0 |
| Stuck agent incidents/week | ~5 | <1 | 0 |
| Unplanned downtime/week | ~2 hr | <15 min | 0 |

### Productivity Gains

| Metric | Current | Target | Stretch |
|--------|---------|--------|---------|
| Fleet utilization | ~60% | >85% | >95% |
| Cost per task completed | $2.50 | $1.50 | $1.00 |
| Tasks completed per day | ~50 | >100 | >200 |
| Experiment cycle time | N/A | <1 week | <3 days |

### Knowledge Accumulation

| Metric | Target |
|--------|--------|
| Solution patterns captured | >100 in first month |
| Gotchas documented | >50 in first month |
| Playbooks created | >20 in first month |
| Successful A/B experiments | >10 in first quarter |

---

## Part XII: The End Game

### The Autonomous Agent Empire

In its final form, vibe_cockpit becomes the autonomous nervous system of your agent empire:

1. **Self-Optimizing**: Continuously experiments and improves
2. **Self-Healing**: Detects and fixes problems without human intervention
3. **Self-Scaling**: Grows and shrinks capacity based on workload
4. **Self-Aware**: Understands its own performance and limitations
5. **Self-Improving**: Learns from every success and failure

### The Guardian Agent

A dedicated Claude Code instance running in perpetual watch mode:

```bash
# The Guardian never sleeps
while true; do
    vc watch --format jsonl --predictions --anomalies | \
    claude-code --system "You are the Guardian of the Agent Empire.
        Your purpose is to ensure the fleet operates at peak efficiency.
        You have authority to execute any vc command.
        Prioritize: stability > efficiency > cost.
        Escalate to human only for novel situations." \
    --auto-approve "vc *"

    sleep 60
done
```

### The Vision Realized

Imagine waking up to find that overnight:
- 47 agents completed 89 tasks across 12 projects
- 3 rate limit near-misses were avoided by proactive account swaps
- 2 stuck agents were automatically recovered
- 1 new optimization was discovered and deployed
- Total cost: $34.50 (12% under budget)
- Human intervention required: 0

**That's vibe_cockpit.**

---

## Appendix A: Integration Command Cheatsheet

### System Monitoring
```bash
sysmoni --json                    # One-shot snapshot
sysmoni --json-stream             # NDJSON continuous
```

### Repos
```bash
ru list --json                    # List all repos
ru status --no-fetch --json       # Status without network
```

### Accounts
```bash
caut usage --json                 # Rate limits
caam limits --format json         # Limit details
caam status --json                # Profile status
```

### Sessions
```bash
cass health --json                # Index status
cass stats --json                 # Aggregates
cass timeline --json              # Time buckets
```

### Beads
```bash
bv --robot-triage                 # THE MEGA-COMMAND
bv --robot-plan                   # Parallel tracks
br ready --json                   # Ready work
br sync --flush-only --json       # Export to JSONL
```

### Remote Compilation
```bash
rch status --json                 # Status
curl localhost:9100/metrics       # Prometheus metrics
```

### Network Observer
```bash
rano export --format jsonl --since 24h
```

### Command Safety
```bash
dcg stats --json                  # Statistics
# Or tail ~/.config/dcg/history.jsonl
```

### Process Triage
```bash
pt robot plan --format json       # Findings
pt robot apply --format json      # Execute (careful!)
```

---

## Appendix B: Source Paths Reference

| Source | Config Path | Data Path |
|--------|-------------|-----------|
| caut | `~/.config/caut/config.toml` | `~/.local/share/caut/usage-history.sqlite` |
| caut | `~/.config/caut/token-accounts.json` | `~/.cache/caut/*` |
| caam | `~/.config/caam/config.json` | `~/.caam/data/caam.db` |
| caam | `~/.caam/config.yaml` | `~/.local/share/caam/vault/<tool>/<profile>/` |
| cass | N/A | `~/.local/share/coding-agent-search/agent_search.db` |
| cass | N/A | `~/.local/share/coding-agent-search/tantivy_index/` |
| cass | N/A | `~/.local/share/coding-agent-search/vector_index/` |
| mcp_agent_mail | `DATABASE_URL` env | `~/.mcp_agent_mail_git_mailbox_repo/storage.sqlite3` |
| mcp_agent_mail | N/A | `projects/<slug>/messages/YYYY/MM/<id>.md` |
| mcp_agent_mail | N/A | `projects/<slug>/agents/<name>/inbox/` |
| mcp_agent_mail | N/A | `projects/<slug>/file_reservations/*.json` |
| dcg | `~/.config/dcg/config.toml` | `~/.config/dcg/history.jsonl` |
| dcg | N/A | `~/.config/dcg/pending_exceptions.jsonl` |
| dcg | N/A | `~/.config/dcg/allow_once.jsonl` |
| rano | N/A | `observer.sqlite` (check repo/config) |
| rch | rch config | Prometheus endpoint (e.g., `0.0.0.0:9090/metrics`) |
| beads | `.beads/config.yaml` | `.beads/beads.db` |
| beads | N/A | `.beads/issues.jsonl` |

---

## Appendix C: Color Scheme

```rust
pub struct NeuralTheme {
    // Background
    pub bg_primary: Color::Rgb(13, 17, 23),      // Deep space
    pub bg_secondary: Color::Rgb(22, 27, 34),    // Nebula

    // Status
    pub healthy: Color::Rgb(63, 185, 80),        // Emerald
    pub warning: Color::Rgb(210, 153, 34),       // Solar flare
    pub critical: Color::Rgb(248, 81, 73),       // Red alert

    // Providers
    pub claude: Color::Rgb(217, 119, 87),        // Anthropic orange
    pub codex: Color::Rgb(16, 163, 127),         // OpenAI green
    pub gemini: Color::Rgb(66, 133, 244),        // Google blue
}
```

---

## Appendix D: Key Design Decisions

These decisions should be made early and consistently applied:

| Decision | Recommendation | Rationale |
|----------|----------------|-----------|
| Pull vs Push | Pull-only MVP, push later | Zero deployment footprint on remotes |
| Next.js serving | Static build + Rust JSON API | Simplest, avoids running Node in prod |
| Analytics location | All in DuckDB | Single source of truth, export when needed |
| Execution authority | Observe + suggest default | Safety first; explicit opt-in for actions |
| Collector failure mode | Degrade gracefully | Show "stale data" badge, don't crash |
| Schema evolution | Migrations + raw_json fallback | Forward compatibility |

---

*This plan synthesizes pragmatic foundations with ambitious vision. It is designed to be built incrementally while maintaining sight of the revolutionary end state.*

**This isn't just a dashboard. This is the command center for the future of software development.**
