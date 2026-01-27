# COMPREHENSIVE PLAN TO MAKE vibe_cockpit (vc)

Date: 2026-01-27

This document turns `PROMPT_TO_MAKE_VC.md` into a concrete, buildable plan for a new Rust program: **vibe_cockpit** (**vc**).

vc is:
- A **Rust daemon + TUI dashboard** for monitoring local + remote Linux machines running large fleets of coding agents.
- A **web dashboard** (Next.js UI) served by the Rust program as an alternative to the TUI.
- A **robot/agent mode CLI** returning **stable JSON or concise Markdown** so agents can operate without the TUI.
- A **data platform**: periodic collection from many sources + machines, normalized into **DuckDB** for querying, aggregation, anomaly detection, and alerting.

This plan assumes your existing tools remain the authoritative sources of data, and vc primarily **integrates** them.

---

## 0) Ground Rules / Constraints (Current Repo Reality)

`AGENTS.md` in this repo is copied from another project (dcg). It contains safety and process guidance that still applies as general discipline (especially "no deletion" and "no destructive commands"), but **it is not a spec for vc**. We will revise it later.

For now, in vc:
- Prefer **read-only integration** with existing tools (shell out to their JSON/robot modes, or read their SQLite/JSONL stores).
- Avoid tight coupling (do not vendor entire repos).
- Bias toward **stable, versioned schemas** (JSON schema + SQL schema).

---

## 1) Product Definition

### 1.1 Primary user
You (human) supervising dozens/hundreds of agent sessions across multiple machines, providers, and repos.

### 1.2 Core jobs-to-be-done (JTBD)
1) "At a glance, tell me if the system is healthy right now."
2) "If something is unhealthy, show me why (root cause) quickly."
3) "If action is needed, guide me to safe remediation (or do it automatically in controlled 'autopilot')."
4) "Let agents query the same state in robot mode and do triage on a sleep/wake cycle."

### 1.3 Scope boundaries (first principles)
vc is primarily **observability + triage + orchestration suggestions**, not a replacement for:
- ntm (tmux orchestration)
- rch (remote compilation)
- caut/caam (account usage/management)
- cass (session search/index)
- mcp_agent_mail (agent messaging)
- dcg (command safety)
- br/bv (task tracking)
- pt/sysmoni/rano (process + system + network monitoring)

vc's job is to unify these into a single cockpit, with a coherent model and fast navigation.

### 1.4 North Star: From dashboards to orchestration intelligence
To succeed in real-world practice, vc must be more than "a page of metrics." It should behave like an operations brain for the agent fleet:

- Perceive: continuously ingest signals from machines, repos, agents, accounts, and tools.
- Think: correlate signals, detect anomalies, and forecast near-future failures (rate limits, disk pressure, queue saturation).
- Act (gated): produce ranked, explainable remediation steps; optionally execute a tightly allowlisted set of actions with audit logs.
- Remember: preserve history so you can answer "what changed?" and replay incidents ("time machine"), and accumulate playbooks/gotchas over time.

The key output is not raw data; it's the answer to: "What should we do next, and why?"

---

## 2) High-Level Architecture (Data Plane + Control Plane)

### 2.1 Data flow (pull-first, push-optional)

Initial (MVP) architecture is **pull**:

```
vc (main machine)
  |-- local collectors (read files / run local CLIs)
  |-- remote collectors via SSH (run CLIs remotely OR fetch exported files)
  `-- DuckDB (single DB file)
       |-- raw tables (append-only events/snapshots)
       |-- derived views (rollups, latest-by-key, anomalies)
       `-- exports (JSON/CSV/Parquet as needed)
```

Later (scale) add **push**:

```
remote machine runs vc-node (tiny agent)
  |-- collects locally (low overhead)
  |-- writes local duckdb/parquet/jsonl
  `-- pushes snapshots/events to vc-hub (HTTPS / SSH upload)
```

Start with pull because it requires zero deployment footprint on remotes beyond SSH access and whatever tools are already installed there.

### 2.2 Component model inside vc (Rust crates/modules)
Recommend a single Cargo workspace with a few internal crates (or modules if you want to keep it simpler at first):

- `vc_config`: config parsing + validation, secrets/paths.
- `vc_collect`: collectors (one per upstream tool + system collectors).
- `vc_store`: DuckDB schema migrations + ingestion helpers + query library.
- `vc_query`: canonical queries (health, rollups, leaderboards, anomalies).
- `vc_alert`: rule engine + notifications + autopilot hooks.
- `vc_tui`: ratatui UI + navigation + charts.
- `vc_web`: axum server + embedded static web assets + JSON API.
- `vc_cli`: clap commands; robot mode output formatting.

### 2.3 Threading/concurrency model
- Use `tokio` runtime.
- Each "poll cycle" spawns **bounded concurrency** tasks:
  - Bound by machine count (e.g., max 8 machines in flight).
  - Bound by collectors per machine (e.g., max 4 collectors in flight).
- Each collector enforces:
  - Timeout.
  - Size limits for outputs.
  - A stable "collector version" + "schema version" tag on inserted rows.

### 2.4 Layered mental model (Perception -> Cognition -> Memory -> Action)
This layered model keeps the system understandable and prevents "feature soup":

- Perception: collectors ingest signals from machines/tools (raw facts).
- Cognition: correlations, anomalies, forecasts, and root-cause hypotheses.
- Memory: DuckDB as the primary store (append-only facts + snapshots).
  - Optional later: vector embeddings for semantic search, time-series store for high-frequency metrics.
- Action: playbooks and remediation suggestions (gated execution).
- Interface: TUI, web, and robot CLI outputs.
- Integration: MCP server, Slack, Prometheus export, GitHub webhooks (later phase).

Start with DuckDB only. Add extra stores only if a real bottleneck emerges.

---

## 3) Source Systems: Integration Strategy (What to Pull, How to Pull)

You listed the sources to integrate; below is a pragmatic plan based on quick surveys of those repos under `/dp/`.

General principle:
- Prefer **official CLI robot/JSON output** first.
- If CLI is too slow or requires auth/UI, read **local caches/SQLite/JSONL** next.
- Avoid re-implementing upstream logic unless necessary.

### 3.1 ntm (Named Tmux Manager)
Goal: show running sessions, agent activity, health, and orchestration context.

Likely integration:
- Shell out to `ntm` robot commands for "summary now".
- Optionally parse `events.jsonl` if ntm provides it (low overhead, good history).

vc ingestion:
- `ntm_sessions_snapshot` (current sessions per machine)
- `ntm_activity_snapshot` (counts by state/type)
- `ntm_events` (append-only)

UI value:
- "Agent fleet map" by machine/repo/session.

### 3.2 caut (coding_agent_usage_tracker)
Goal: show per-provider and per-account usage, remaining quota, reset times, status/outage awareness.

Likely integration:
- `caut usage --json` (robot mode).
- Optionally tail local `usage-history.sqlite` for historical charts.

vc ingestion:
- `account_usage_snapshot` (provider, account label, window, used%, resets_at, remaining credits)
- `account_status_events` (provider outages, auth failures)

### 3.3 caam (coding_agent_account_manager)
Goal: show profile mapping, "best account to use", limit forecasting.

Likely integration:
- `caam limits --format json` (and `caam status --json`).
- Use this to power autopilot suggestions like "swap to account X".

vc ingestion:
- `account_profile_snapshot` (tool -> active profile, health)
- `account_limits_snapshot` (provider/profile/window utilization and resets)

### 3.4 cass (coding_agent_session_search)
Goal: global session volume metrics, token usage proxies, compaction counts, time per session, agent-type breakdown.

Likely integration:
- `cass ... --robot/--json` for:
  - index status/freshness
  - aggregate stats
  - timeline summaries

vc ingestion:
- `session_index_status_snapshot`
- `session_stats_snapshot` (counts, durations, per-agent breakdown)
- `session_timeline_buckets` (time-bucketed counts)

### 3.5 remote_compilation_helper (rch)
Goal: show master/worker activity, queue depth, latency, failures, transfer stats.

Likely integration options:
1) Scrape Prometheus `/metrics` endpoint (best for dashboards).
2) `rch status --json` (best for simple pull without Prometheus).

vc ingestion:
- `rch_status_snapshot` (daemon/workers/jobs)
- `rch_metric_samples` (if scraping prometheus: metric_name, labels, value, ts)

### 3.6 repo_updater (ru)
Goal: repo fleet overview per machine: dirty repos, ahead/behind, recent changes, issues/PR counts (optional).

Likely integration:
- `ru list --json` to enumerate repo paths.
- `ru status --no-fetch --json` to avoid network in tight loops.
- Add optional "deep scan" mode (less frequent) for:
  - `git rev-list --count HEAD`
  - LoC via `tokei` or `git ls-files | wc -l` (decide later).

vc ingestion:
- `repo_status_snapshot` (dirty, branch, ahead/behind)
- `repo_commit_stats_snapshot` (optional)
- `repo_loc_snapshot` (optional)

### 3.7 mcp_agent_mail
Goal: message counts, ack-required backlog, urgent messages, inter-agent communication heatmaps, "concerning words" surfacing.

Likely integration:
- Read from its SQLite database (`storage.sqlite3`) and/or its git-backed archive.
- Optionally call its HTTP endpoints if exposed (but DB read is simplest).

vc ingestion:
- `mail_message_facts` (messages as facts; dedupe by message_id)
- `mail_recipient_facts` (read/ack timestamps)
- `mail_file_reservation_snapshot` (locks/leases)

### 3.8 process_triage (pt)
Goal: detect stuck/zombie/runaway processes and provide safe remediation suggestions.

Likely integration:
- Use its "robot plan" outputs for "what's wrong".
- Use it as a controlled remediation executor (only after explicit user config).

vc ingestion:
- `process_triage_plan_snapshot` (findings + recommended actions)
- `process_triage_findings` (normalized)

### 3.9 destructive_command_guard (dcg)
Goal: blocked command counts over time by machine/repo, severity breakdown, top rule IDs.

Likely integration:
- Tail `~/.config/dcg/history.jsonl` (and other jsonl logs).
- Or `dcg stats --json` if it exists/works for your desired views.

vc ingestion:
- `dcg_events` (append-only: denied/allowed, rule_id, pack_id, severity, cwd, cmd hash, ts)

### 3.10 system_resource_protection_script / sysmoni (SRPS)
Goal: CPU/mem/io/net/gpu, top processes, throttling, inotify exhaustion, temps, etc.

Likely integration:
- Run `sysmoni --json` locally and via SSH remotely.
- Optionally run `sysmoni --json-stream` (NDJSON) and ingest continuously.

vc ingestion:
- `sys_sample` (structured snapshot)
- `sys_process_top` (top-N processes per sample)
- `sys_throttle_events` / `sys_resource_pressure` (derived)

### 3.11 rano
Goal: network activity observability per provider/process; detect abnormal spikes or broken authentication loops.

Likely integration:
- `rano export --format jsonl --since <duration>`
- Or read its SQLite tables/views directly.

vc ingestion:
- `net_events` (connect/close, provider, process, remote domain/ip/port)
- `net_session_summary` (derived)

### 3.12 beads_viewer (bv) + beads_rust (br)
Goal: productivity metrics and project health via tasks graph; show "what to work on next" and blockers.

Likely integration:
- `bv --robot-triage` and friends (JSON).
- `br sync --flush-only` and `br ready --json` for underlying issue data.

vc ingestion:
- `beads_triage_snapshot` (bv output)
- `beads_issue_facts` (from `.beads/issues.jsonl` across repos)
- `beads_graph_metrics_snapshot` (bv insights)

### 3.13 automated_flywheel_setup_checker (afsc)
Goal: ensure flywheel setup health; surface installer/run summaries and failure clusters.

Likely integration:
- `automated_flywheel_setup_checker status --format json`
- `... list/validate/classify-error --format jsonl`

vc ingestion:
- `afsc_run_facts`
- `afsc_event_logs`

### 3.14 cloud_benchmarker
Goal: baseline + drift of VPS machine performance, scores, subscores.

Likely integration:
- Pull from its API endpoints (`/data/raw/`, `/data/overall/`) or read SQLite.

vc ingestion:
- `cloud_bench_raw_subscores`
- `cloud_bench_overall_scores`

### 3.15 Standard collector contracts (vc-side)
Treat each upstream integration as a `Collector` with a strict contract so vc remains debuggable and future-proof.

Collector design goals:
- **Idempotent inserts**: the same source payload should not create duplicates.
- **Incremental by default**: collectors should avoid rescanning large histories every poll.
- **Versioned outputs**: every collector has `collector_version` + `schema_version`.
- **Fail-soft**: a broken collector should degrade the cockpit (show "stale data") not crash the daemon.

Proposed trait shape (conceptually):
- `fn name() -> &'static str`
- `fn schema_version() -> u32`
- `async fn collect(ctx: &CollectContext) -> CollectResult`

Where `CollectContext` includes:
- machine_id
- local/remote execution handle
- last successful cursor (if any)
- poll window (e.g., "since 10 minutes ago")
- hard limits (timeout, max bytes, max rows)

And `CollectResult` returns:
- `rows: Vec<RowBatch>` (already normalized)
- `new_cursor: Option<Cursor>`
- `raw_artifacts: Vec<ArtifactRef>` (optional: store raw JSON for debugging)
- `warnings: Vec<Warning>` (surface in UI)

### 3.16 Incremental ingestion patterns (how vc avoids rescans)
Most sources fit into one of these patterns; bake them in early:

1) **CLI snapshot** (stateless)
   - Run command, parse JSON, insert snapshot rows tagged with `collected_at`.
   - Example: `caut usage --json`, `ru status --json`.

2) **CLI incremental window** (time-bounded)
   - Run command with `--since` or similar; store a cursor as last-seen timestamp.
   - Example: `rano export --since 10m --format jsonl` (store last ts).

3) **JSONL tail** (file offset cursor)
   - Maintain `(path, inode, offset)` per machine/source.
   - On rotation/inode change: fall back to "last N minutes" scan or reset with explicit marker.
   - Example: `~/.config/dcg/history.jsonl`.

4) **SQLite incremental** (primary key cursor)
   - Keep "last seen message_id/created_ts".
   - Query `WHERE created_ts > ?` (or `id > ?`) each poll.
   - Example: mcp_agent_mail `messages` table.

5) **Prometheus scrape** (metric samples)
   - Scrape `/metrics` text; parse into `(metric, labels, value, ts)`.
   - Downsample aggressively; store only what you chart/alert on.
   - Example: rch metrics endpoint.

---

## 4) Machine Model (Local + Remote)

### 4.1 Machine inventory
vc needs a durable inventory of machines:
- machine_id (stable, human-readable)
- ssh target (host/user/port, optional ProxyJump)
- roles (main, worker, storage, etc.)
- tags (gpu, fast-net, low-cost, etc.)
- installed tool availability (auto-detected and cached)

Define in `vc.toml`:

```toml
[vc]
db_path = "~/.local/share/vc/vc.duckdb"
data_dir = "~/.local/share/vc"

[polling]
default_interval_seconds = 120
max_machines_in_flight = 8
max_collectors_in_flight_per_machine = 4

[[machines]]
id = "main"
ssh = "local" # special-case local execution
tags = ["primary"]

[[machines]]
id = "worker-01"
ssh = "ubuntu@10.0.0.12:22"
tags = ["worker","rch"]
```

### 4.2 Remote execution mechanism (MVP choice)
MVP: run commands over SSH with a small wrapper that:
- uses `ssh` binary (simple, leverages existing SSH config), OR
- uses an SSH library (more control, but more complexity).

Recommendation:
- Start by invoking `ssh` binary with strict timeouts and `BatchMode=yes`.
- Later upgrade to a Rust SSH client if needed.

Remote "tool detection":
- first connect, run `command -v <tool>` for each tool.
- cache results in DuckDB `machine_tool_capabilities` table with TTL (e.g., 24h).

### 4.3 Remote acquisition patterns (MVP)
vc needs predictable, low-risk ways to pull data from remote machines.

Use a small set of patterns:

1) **Run remote command, read stdout JSON**
   - `ssh <target> "<cmd>"`
   - Parse JSON directly into vc.
   - Best when the upstream tool already provides robot/JSON output.

2) **Read remote file (JSONL/SQLite)**
   - `ssh <target> "cat <path>"` for small JSONL segments.
   - For larger files, prefer ranged reads:
     - `tail -n <N>` for JSONL
     - or `dd if=<file> bs=1 skip=<offset> count=<bytes>` (careful with performance)
   - Store file cursor state in DuckDB.

3) **Fetch remote export artifact**
   - Trigger remote export (e.g., `rano export`) into a temp file, then `scp` it back.
   - Useful when stdout is too large or needs batching.

4) **Fallback "basic system probe"**
   - Even if no tools are installed remotely, vc can collect baseline health:
     - `uptime`, `df -P`, `free -b`, `/proc/loadavg`, `/proc/meminfo`.
   - Keep this collector separate and always enabled.

### 4.4 Push mode (future scale option)
When pull becomes expensive (many machines, many collectors), add a tiny `vc-node` on each machine:
- Collect locally and write to Parquet/JSONL.
- Push to hub on interval or on-change.
- Allows "store-and-forward" when a machine is briefly offline.

---

## 5) DuckDB: Storage Design (Schemas, Retention, Queries)

### 5.1 Table taxonomy
Use two styles:
- **append-only event tables** (facts)
- **point-in-time snapshot tables** (facts with `collected_at`)

Always include:
- `collected_at` (timestamp)
- `machine_id`
- `source` (collector name)
- `source_version` (semantic version string)
- `schema_version` (integer or string)
- `payload_json` (optional raw JSON for forward compatibility)

Then add normalized columns for queries.

### 5.2 Suggested core tables (MVP)

Machines:
- `machines(machine_id, tags_json, ssh_target, first_seen_at, last_seen_at)`
- `machine_tool_capabilities(machine_id, tool_name, available, version, checked_at)`

System:
- `sys_samples(machine_id, collected_at, cpu_total, load1, load5, load15, mem_used_bytes, mem_total_bytes, swap_used_bytes, disk_read_mbps, disk_write_mbps, net_rx_mbps, net_tx_mbps, raw_json)`
- `sys_top_processes(machine_id, collected_at, pid, comm, cpu_pct, mem_bytes, fd_count, io_read_bytes, io_write_bytes, raw_json)`

Repos:
- `repos(machine_id, repo_id, path, url, name)` (repo_id = stable hash of url or path)
- `repo_status_snapshots(machine_id, collected_at, repo_id, branch, dirty, ahead, behind, raw_json)`

Accounts:
- `account_usage_snapshots(machine_id, collected_at, provider, account, window, used_percent, resets_at, credits_remaining, status, raw_json)`
- `account_profile_snapshots(machine_id, collected_at, tool, active_profile, health_expires_at, raw_json)`

Sessions (cass):
- `cass_index_status(machine_id, collected_at, state, total_sessions, last_index_at, raw_json)`
- `cass_stats_snapshots(machine_id, collected_at, metric_name, metric_value, dimensions_json, raw_json)`

Agent mail:
- `mail_messages(collected_at, project_id, message_id, thread_id, sender, importance, ack_required, created_ts, subject, raw_json)`
- `mail_recipients(collected_at, message_id, recipient, read_ts, ack_ts, raw_json)`
- `mail_file_reservations(collected_at, project_id, reservation_id, path_pattern, holder, expires_ts, exclusive, raw_json)`

dcg:
- `dcg_events(machine_id, ts, decision, rule_id, pack_id, severity, cwd, command_hash, raw_json)`

rch:
- `rch_status_snapshots(machine_id, collected_at, daemon_state, workers_total, workers_available, builds_active, queue_depth, raw_json)`
- (optional) `rch_metric_samples(machine_id, ts, metric_name, labels_json, value)`

rano:
- `rano_events(machine_id, ts, event, provider, pid, comm, remote_ip, remote_port, domain, duration_ms, raw_json)`

beads:
- `beads_triage_snapshots(machine_id, collected_at, repo_id, quick_ref_json, recommendations_json, raw_json)`
- `beads_issues(repo_id, issue_id, status, priority, labels_json, deps_json, updated_at, raw_json)`

### 5.3 Retention policy
DuckDB is fast, but you'll collect a lot of data. Plan retention early.

Default retention suggestions:
- high-frequency system samples: keep 7-30 days at full resolution, then downsample.
- event logs (dcg, rano): keep 90 days raw; optionally keep aggregates forever.
- derived rollups: keep forever (small).

Implement as:
- a periodic `vc vacuum` / retention job that:
  - deletes old raw rows (only if you explicitly enable it)
  - or compacts to Parquet partitions and then truncates raw tables

Note: Given the "no deletion" discipline, make retention opt-in and transparent.

### 5.4 Migrations, schema evolution, and ingestion cursors
vc will evolve quickly; bake in safe evolution mechanisms immediately.

DuckDB migrations:
- Maintain a `schema_migrations` table:
  - `version` (int), `applied_at`, `description`, `checksum`.
- Apply migrations at startup; refuse to start if a migration checksum mismatch occurs (unless `--force`).

Ingestion cursors/state:
- Maintain an `ingestion_cursors` table keyed by `(machine_id, source, cursor_key)`:
  - examples:
    - `("main","dcg","history.jsonl") -> {inode, offset, last_ts}`
    - `("worker-01","mcp_agent_mail","messages") -> {last_message_id}`
    - `("main","rano","export") -> {last_ts}`
- This lets collectors be incremental and restart-safe.

Raw payload retention:
- For forward compatibility, store a `raw_json` column for most tables.
- Keep normalized columns for the queries you care about (health + dashboards).

### 5.5 Time machine: snapshots, incidents, and replay
Beyond raw tables, vc should support "what happened?" questions without needing ad-hoc archaeology.

Add two concepts:

1) Fleet state snapshots (complete, queryable summaries)
- Periodically materialize a "whole fleet" snapshot that is cheap to render and easy to diff.
- Store both normalized summaries and a compact JSON blob for forward compatibility.

Suggested tables:
- `fleet_state_snapshots(collected_at, hash, fleet_health_score, risk_level, summary_json)`
- `machine_state_snapshots(machine_id, collected_at, health_score, summary_json)`

2) Incidents (correlated multi-signal failures)
- An incident is a bounded time range with a root cause hypothesis and a reconstructed timeline.

Suggested tables:
- `incidents(incident_id, opened_at, closed_at, severity, title, status, primary_machine_id, root_cause_json)`
- `incident_timeline_events(incident_id, ts, source, event_type, summary, evidence_json)`
- `incident_notes(incident_id, ts, author, note_md)`

Time travel queries (DuckDB-friendly pattern):
- "As of time T" is typically: pick the latest snapshot with `collected_at <= T`, then fetch context in a window.

Example SQL sketch:

```sql
-- State at a point in time (latest snapshot at or before T)
SELECT *
FROM fleet_state_snapshots
WHERE collected_at <= TIMESTAMP '2026-01-25 03:00:00'
ORDER BY collected_at DESC
LIMIT 1;

-- Context window around an incident
SELECT *
FROM dcg_events
WHERE ts BETWEEN TIMESTAMP '2026-01-25 02:45:00' AND TIMESTAMP '2026-01-25 03:45:00'
ORDER BY ts;
```

This is the foundation for an "incident replay" UI: a timeline + correlated graphs + the actions taken.

---

## 6) Derived Views: "Health" and "What Matters"

vc should not just store data; it should answer the question:
"What should I look at *right now*?"

### 6.1 Canonical "health score" (per machine + global)
Define a health model that combines:
- system resource pressure (CPU load, mem pressure, swap churn, disk space, temps)
- agent productivity signals (beads throughput, session rate)
- agent friction signals (dcg blocked spikes, repeated network errors, mcp mail urgent/unacked)
- infra bottlenecks (rch queue depth, failed builds)

Represent as:
- `health_factors(machine_id, collected_at, factor_id, severity, score, details_json)`
- `health_summary(machine_id, collected_at, overall_score, worst_factor_id, details_json)`

Make severity and factors explainable, not a black box.

### 6.2 Anomaly detection (MVP vs later)
MVP: deterministic thresholds + rate-of-change checks:
- load spikes
- swap non-zero
- disk < X%
- dcg denies > N per hour
- mcp urgent unread > N
- rch queue depth > N
- rano provider domains unusual / too many connections

Later: statistical models (DuckDB SQL + offline analysis):
- robust z-scores on time series
- seasonal decomposition for daily cycles
- forecasting for "when will credits reset / run out"

### 6.3 Forecasting and proactive recommendations (rate limits, failures, costs)
Add a light-weight "oracle" that produces **explainable** predictions before heavy ML:

- Rate limit forecasting (from caut/caam):
  - Estimate time-to-limit by provider/account.
  - Recommend swap windows and candidate accounts.
- Failure risk forecasting (from system + agent signals):
  - Detect rising load, swap churn, rch queue growth, repeated dcg denies.
  - Flag "probable failure within N minutes" with reasons.
- Cost forecasting:
  - Daily/weekly cost projections by provider and by project.

Store predictions in:
- `predictions(machine_id, generated_at, prediction_type, horizon_minutes, confidence, details_json)`

### 6.4 Knowledge base: solutions, gotchas, and playbooks (later phase)
Use the existing session history (cass) + agent mail to extract:

- **Solutions**: "when X fails, do Y" (successful remediation patterns).
- **Gotchas**: subtle failure modes tied to specific repos/tools.
- **Playbooks**: step-by-step remediation with safety gates.

This can start as curated Markdown/JSON in vc, later upgraded to a search-able knowledge store.

---

## 7) User Interfaces

### 7.1 Rust TUI (primary)
Use `ratatui` + `crossterm`.

Navigation principles:
- single keypress to jump between core dashboards
- persistent filter bar (machine / repo / provider / time window)
- "drill-down" from global -> machine -> repo -> event detail
- integrated search (delegates to cass for content search; local search for vc facts)

Core screens (MVP):
1) Overview (global health, top alerts, machines list)
2) Machine detail (system charts + processes + rch + agent activity)
3) Repos (dirty, ahead/behind, recent changes)
4) Accounts (caut/caam usage + "recommend swap")
5) Agents/messages (mcp mail: urgent, ack required, file reservations)
6) Events (dcg denies, rano anomalies, pt findings)
7) Beads (bv triage, blockers, next picks)

### 7.2 Web dashboard (Next.js served by Rust)
Recommendation: **static Next.js build + Rust JSON API**, at least for MVP.

Why:
- simplest "served by Rust" story: Rust serves static assets + API.
- avoids running Node in production and reduces moving parts.

Plan:
- `web/` directory contains Next.js app.
- Next.js configured for static output (where possible).
- Rust `vc_web` serves:
  - `GET /api/...` (JSON endpoints backed by DuckDB queries)
  - `GET /` static files (embedded or on-disk)

If you later need SSR:
- run a Node "standalone" server as a managed child process and reverse proxy via Rust.

### 7.3 CLI robot mode (agent-friendly)
Design goals:
- stable schemas
- no color noise
- stdout=data, stderr=diagnostics
- low token output variants (TOON-like where helpful)

Example commands (proposed):
- `vc robot health --json`
- `vc robot triage --json` (top issues + suggested next commands)
- `vc robot machine <id> --json`
- `vc robot accounts --json`
- `vc robot repos --json`
- `vc robot alerts --json`
- `vc query "<sql>" --format json|md` (guarded; read-only by default)

Provide `vc robot-docs`:
- `vc robot-docs schemas`
- `vc robot-docs examples`

### 7.4 TUI interaction spec (MVP)
Keep keybindings simple and consistent:

- Global:
  - `?` help overlay
  - `/` search (routes to cass for content; vc local search for facts)
  - `f` filter bar (machine/repo/provider/time window)
  - `r` refresh now
  - `Esc` back / close modal
  - `q` quit

- Overview screen:
  - `Enter` drill into selected machine
  - `a` accounts
  - `g` git/repos
  - `m` mail
  - `b` beads
  - `e` events

Display guidelines:
- The top row is always: "global health + poll freshness + active alerts".
- The left pane is always: "selectable list (machines/repos/alerts)".
- The right pane is always: "details for selection".

### 7.5 Web API and auth (MVP)
Keep the web UI read-only at first.

Endpoints (proposed):
- `GET /api/health` -> global + per-machine health summary
- `GET /api/machines` -> machine inventory + tool availability + last_seen
- `GET /api/machines/:id/overview` -> system + key alerts + top processes
- `GET /api/repos` -> repo status (filterable by machine)
- `GET /api/accounts` -> caut/caam merged view + "recommendations"
- `GET /api/alerts` -> open/acked/closed
- `GET /api/events/dcg` -> recent denies breakdown
- `GET /api/events/rano` -> recent anomalies/network summary
- `GET /api/beads/:repo_id/triage` -> bv outputs

Auth (MVP):
- local-only binding by default (`127.0.0.1`).
- optional `VC_WEB_TOKEN` header for remote access.
- no cookies/session auth until you need multi-user.

### 7.6 Robot output schemas (example)
Every robot command should emit a top-level envelope:
- `schema_version`
- `generated_at`
- `data`
- `staleness` (per component, seconds since last update)
- `warnings`

Example (sketch):

```json
{
  "schema_version": "vc.robot.health.v1",
  "generated_at": "2026-01-27T00:00:00Z",
  "data": {
    "overall": { "score": 0.82, "severity": "medium" },
    "machines": [
      { "id": "main", "score": 0.91, "top_issue": "dcg_denies_spike" }
    ]
  },
  "staleness": { "sysmoni": 40, "ru": 120, "caut": 1800 },
  "warnings": []
}
```

### 7.7 Streaming watch mode (agent loop friendly)
Add a `vc watch` mode that emits **JSONL** events at a fixed interval or on change:

Example:
- `vc watch --format jsonl --interval 30`

Event types:
- `alert`: new or escalated alerts
- `prediction`: forecast risk crossing a threshold
- `opportunity`: optimization hints (cost saving, account swap)

This enables a guardian agent to monitor without polling full snapshots.

### 7.8 Integration surfaces (later phase)
Add integrations only when core data + UI are stable:

- **MCP server mode**: expose vc as a tool provider for agents.
- **Prometheus export**: allow Grafana dashboards (simple metrics endpoint).
- **Slack/Discord bot**: alert delivery and quick status queries.
- **GitHub correlation**: link commits/PRs to fleet activity (optional).

These can be implemented as separate modules to avoid bloating the MVP.

### 7.9 Natural language queries (optional, later)
If you want conversational queries:
- Implement a minimal "NL -> query" layer that maps intents to predefined queries.
- Avoid free-form SQL generation until strict guardrails exist.
- Keep it opt-in and read-only.

---

## 8) Alerting + Autopilot ("sleep/wake cycle")

### 8.1 Alerts
An "alert" is a persisted entity:
- id, machine_id, type, severity, first_seen_at, last_seen_at, state(open/ack/closed)
- evidence_json (links to underlying rows)
- suggested_actions (commands or playbooks)

Alert delivery channels (phased):
MVP:
- TUI highlight + web highlight
- write to `alerts.jsonl` file
Later:
- Agent Mail message (thread per alert)
- webhook/Discord/Slack
- desktop notifications

### 8.2 Autopilot modes (strict safety)
Autopilot is explicitly configured and gated:
- `mode = off | suggest | execute-safe | execute-with-approval`

Suggest mode:
- vc prints "Recommended actions" but does not run them.

Execute-safe mode:
- only runs a small allowlisted set of commands (e.g., "start new tmux session", "switch caam profile", "restart a known safe service").

Execute-with-approval mode:
- creates a human confirmation step in TUI/web before executing.

Autopilot should also be "agent-friendly":
- `vc robot triage` returns the same recommendations with deterministic next commands.

### 8.3 Initial alert rules (concrete MVP set)
Start with a small set that catches the common "empire is on fire" cases:

System:
- Disk free < 10% (per filesystem)
- Load5 > (cpu_cores * 1.5) for > 10 minutes
- Swap used > 0 and rising
- OOM kills detected (if accessible)
- Inotify near limit (SRPS/sysmoni)

Agents + tooling:
- `dcg` denies > N/hour OR new "critical" rule IDs appear
- `rano` connections to unknown provider domains spike
- `mcp_agent_mail` urgent-unread > N OR ack-overdue > N
- `rch` queue depth > N OR workers_available == 0 while builds_active > 0
- `cass` index stale > 24h (or health failing)

Repos:
- dirty repos > N on a machine (indicates stuck workflows)
- a repo that was "hot" recently stops getting commits entirely (optional later)

### 8.4 Autopilot playbooks (suggested, gated)
These should be *suggestions first*; execution requires explicit config:

- "Swap account":
  - if active profile near limit, suggest `caam recommend` / `caam limits` based swap.
- "Unstick machine":
  - if load high + suspicious processes, suggest `pt robot plan` and safe actions.
- "Free disk space":
  - suggest largest dirs, docker prune (only as suggestion), log rotation checks.
- "Fix rch bottleneck":
  - suggest `rch status --workers`, worker restart, or remove flaky worker from pool.
- "Investigate dcg spike":
  - show top rule IDs + cwd correlation; suggest reviewing the agent behavior.

### 8.5 Playbook protocol (make actions safe and auditable)
Define a structured playbook format so every automated action is explainable:

Each playbook should include:
- Trigger: when it runs (thresholds + conditions).
- Diagnosis: what checks it performs before acting.
- Treatment: actions to take (in order).
- Verification: how it decides the action succeeded.
- Rollback: how to revert or neutralize side effects.
- Escalation: when to stop and alert a human.

Store playbooks as versioned JSON/YAML with a stable schema and keep them in git.

### 8.6 Guardian mode (optional, later)
Run a long-lived "guardian" agent on a sleep/wake cycle:
- Subscribes to `vc watch --format jsonl`.
- Acts only on `execute-safe` actions by policy.
- Logs every action into `audit_events`.

This preserves safety while enabling near real-time remediation.

---

## 9) Security, Secrets, and Access Control

### 9.1 Threat model (practical)
vc will see and sometimes execute sensitive operations across machines. Assume:
- local attackers could read your home directory if permissions are lax
- remote machines could be compromised and return malicious payloads
- dashboards might get exposed accidentally if bound to 0.0.0.0

### 9.2 Secrets handling
- Do not store provider credentials in vc; defer to upstream tools (caut/caam).
- vc stores only:
  - machine inventory (ssh targets)
  - optional web token
  - optional alert webhooks
- Prefer OS keychains when available; otherwise store secrets in a dedicated file with `0600`.

### 9.3 Remote execution safety
- Default remote execution is read-only commands.
- Any command execution beyond read-only must be:
  - explicitly enabled per-machine and per-command category
  - logged in an audit table (`audit_events`)
  - visible in UI with "who/when/what"

### 9.4 Audit logs
- Persist "vc actions" (even read-only) for debugging:
  - collector start/end, duration, bytes parsed, rows inserted, errors
- Persist "autopilot executions" separately:
  - exact command run, machine_id, user confirmation state, result exit code

---

## 10) Implementation Phases (Milestones)

### Milestone 0: Repo bootstrap (1-2 days)
- Create Cargo workspace, `vc` binary, `vc.toml` config loader.
- Create DuckDB file + migration framework.
- Implement a minimal collector runner with one "dummy collector".
- Implement `vc robot health` returning stub JSON.

### Milestone 1: System + repo basics (3-7 days)
- Collect system samples (local only) via sysmoni or direct `/proc` parsing.
- Integrate `ru` locally (`ru list --json`, `ru status --json`).
- Store snapshots in DuckDB.
- TUI: overview screen + machine list + repo list.

### Milestone 2: Accounts + sessions + mail (1-2 weeks)
- Integrate `caut usage --json`.
- Integrate `caam limits --format json` + `caam status --json`.
- Integrate cass high-level stats (`cass health`, `cass stats`, `cass timeline`).
- Integrate mcp_agent_mail SQLite read for urgent/unacked/file reservations.
- UI screens: Accounts, Sessions, Mail.

### Milestone 3: Remote machines via SSH (1-2 weeks)
- Add remote machine inventory in `vc.toml`.
- SSH runner with timeouts and tool availability probing.
- Remote collectors for sysmoni + ru + selected CLIs.
- UI: per-machine drill-down.

### Milestone 4: rch + rano + dcg + pt (1-2 weeks)
- rch integration (status JSON first; optional prometheus later).
- rano integration (export jsonl).
- dcg integration (history.jsonl tail or dcg stats).
- process_triage integration (robot plan).
- Alerts MVP: threshold rules + persistence.

### Milestone 5: Web dashboard + autopilot (2-4 weeks)
- Next.js UI build + Rust server to host it.
- JSON API endpoints with auth (at least local-only token).
- Autopilot: suggest mode first; optional safe execution.

### Milestone 6: Integration + intelligence (optional, later)
- `vc watch` streaming JSONL and guardian loop.
- MCP server mode for agent integrations.
- Prometheus export + Slack/Discord alerts.
- Initial forecasting (rate limit + cost) and playbook knowledge base.

---

## 11) Success Metrics and SLOs (track value, not just features)

Operational excellence:
- Mean time to detect critical anomalies: target < 60s (stretch < 10s).
- Mean time to remediate common incidents: target < 5 min (stretch < 1 min).
- Rate limit incidents per week: target < 2 (stretch 0).
- Unplanned downtime per week: target < 15 min (stretch 0).

Productivity:
- Fleet utilization: target > 80% (stretch > 90%).
- Cost per completed task: target -30% from baseline.
- Tasks completed per day: target 2x baseline within 8-12 weeks.

Knowledge accumulation:
- New playbooks/gotchas captured per week.
- "Incident replay" coverage (% of incidents with timeline + root cause).

These metrics ensure vc improves outcomes, not just visibility.

---

## 12) Testing, Quality, and Performance

### 12.1 Collector testing strategy
- Unit tests for parsing upstream JSON into internal structs.
- Fixture-based tests: store sample outputs from each upstream tool under `fixtures/` and assert stable ingestion.
- "Schema version" tests: reject unknown schema versions unless raw_json fallback is enabled.

### 12.2 DuckDB query testing
- For canonical queries (health, top issues, leaderboards), create golden snapshots of small DuckDB DBs and assert outputs.

### 12.3 Performance budgets
- Poll cycle should be bounded (e.g., < 3s local; < 30s with remotes) with timeouts.
- Avoid large JSON decoding in tight loops; prefer incremental export windows (e.g., rano "since 10m").
- Use prepared statements / batched inserts into DuckDB.

---

## 13) Key Design Decisions (Make Early)

1) Pull-only vs push-enabled from day 1
   - Recommendation: pull-only MVP, add push later.

2) How to serve Next.js
   - Recommendation: static build served by Rust + JSON API; SSR later if needed.

3) Where to do "heavy analytics"
   - Recommendation: keep all raw data in DuckDB, define derived views, and only export to other systems when needed.

4) What vc is allowed to execute
   - Recommendation: default "observe + suggest"; execution requires explicit config.

---

## 14) Immediate Next Steps (Actionable)

If you want to proceed after this plan:
1) Decide whether vc should be a single crate or a workspace with sub-crates.
2) Confirm the "served by Rust" requirement for Next.js means "static assets hosted by Rust" (recommended) vs "SSR inside Rust".
3) Provide a short list of your current machines (IDs + ssh targets) to seed `vc.toml`.
4) Pick the MVP collectors you want first (I'd start with sysmoni + ru + caut).

---

## Appendix A: Quick Integration Command Cheatsheet (From Existing Tools)

These are examples of the kinds of stable, machine-readable surfaces vc can depend on:

System:
- `sysmoni --json` (snapshot)
- `sysmoni --json-stream` (NDJSON stream)

Repos:
- `ru list --json`
- `ru status --no-fetch --json`

Accounts:
- `caut usage --json`
- `caam limits --format json`
- `caam status --json`

Sessions:
- `cass health --json || cass index --full`
- `cass search "<q>" --robot --limit 5 --fields minimal`

Beads:
- `bv --robot-triage`
- `bv --robot-plan`
- `br ready --json`
- `br sync --flush-only --json`

Remote compilation:
- `rch status --json` (or scrape `/metrics`)

Network observer:
- `rano export --format jsonl --since 24h`

Command safety:
- read `~/.config/dcg/history.jsonl` (or `dcg stats --json`)

Process triage:
- `pt robot plan --format json`

## Appendix B: Source Survey Notes (paths + storage)

These are the kinds of paths vc can read directly when CLI invocation is not ideal. Treat them as configurable, not hard-coded.

caut (coding_agent_usage_tracker):
- Config: `~/.config/caut/config.toml` (XDG)
- Token accounts: `~/.config/caut/token-accounts.json`
- Cache examples: `~/.cache/caut/*`
- History DB: `~/.local/share/caut/usage-history.sqlite`

caam (coding_agent_account_manager):
- Config: `~/.config/caam/config.json` (and `~/.caam/config.yaml` for some UI configs)
- Data DB: `~/.caam/data/caam.db`
- Vault: `~/.local/share/caam/vault/<tool>/<profile>/...`

cass (coding_agent_session_search):
- Data dir default: `~/.local/share/coding-agent-search/`
- SQLite DB: `agent_search.db`
- Tantivy index: `tantivy_index/`
- Vector index: `vector_index/index-...`
- Remotes mirror: `remotes/<source>/...`

mcp_agent_mail:
- Git mailbox repo (default): `~/.mcp_agent_mail_git_mailbox_repo/`
- SQLite: `storage.sqlite3` (via `DATABASE_URL`)
- Canonical messages: `projects/<slug>/messages/YYYY/MM/<id>.md`
- Per-agent inbox/outbox: `projects/<slug>/agents/<name>/inbox/...` and `.../outbox/...`
- File reservations: `projects/<slug>/file_reservations/*.json`

dcg (destructive_command_guard):
- History/events: `~/.config/dcg/history.jsonl`
- Pending exceptions: `~/.config/dcg/pending_exceptions.jsonl`
- Allow-once: `~/.config/dcg/allow_once.jsonl`

rano:
- SQLite (often): `observer.sqlite` (check repo/config)
- Export is preferred: `rano export --format jsonl --since ...`

rch:
- Prometheus endpoint configured in rch config (e.g., `0.0.0.0:9090/metrics`)

sysmoni / SRPS:
- sysmoni supports JSON snapshots and NDJSON streams; VC should prefer JSON output to avoid scraping human text.
