# Vibe Cockpit (`vc`)

<div align="center">

![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20macOS-blue.svg)
![Rust](https://img.shields.io/badge/rust-2024%20edition-000000.svg)
![License](https://img.shields.io/badge/License-MIT%2BOpenAI%2FAnthropic%20Rider-blue.svg)

</div>

Vibe Cockpit is a fleet console for AI coding agents. It collects telemetry from the
tools you already run — `ntm`, `caut`, `cass`, `caam`, `dcg`, `br`/`bv`, `ru`, `rano`,
`pt`, `rch`, MCP Agent Mail — across local and SSH-reachable machines, lands it in one
DuckDB store, scores it, and serves it back as a TUI, a read-only web API, an MCP
server, and a JSON CLI that agents can drive.

<div align="center">

```bash
curl -fsSL "https://raw.githubusercontent.com/Dicklesworthstone/vibe_cockpit/main/install.sh" | bash
```

</div>

## TL;DR

### The Problem

Running a dozen agents across a few boxes means a dozen sources of truth. Which account
is about to hit a rate limit? Which machine is 3% from a full disk? Which repo has been
dirty for six hours? Every one of those answers lives in a different tool, on a different
host, behind a different flag.

### The Solution

One collector framework, one store, one set of queries. `vc daemon` polls each tool on a
schedule, writes the rows, scores every machine, and raises alerts. Everything else —
TUI, web, MCP, robot JSON — is a view over that store.

### Why It's Different

- **Agent-first.** Every read surface has a machine-readable form. `vc --format json`
  and `vc robot <cmd>` emit a versioned envelope; there is an MCP server so an agent can
  ask about the fleet without shelling out at all.
- **It tells you when it doesn't know.** A screen with no backing query says
  `NO DATA SOURCE YET` and names the table it would need. It does not invent a number.
- **Cancel-correct.** Built on [Asupersync](https://github.com/Dicklesworthstone/asupersync);
  every collector takes a capability context, so a SIGTERM mid-tick unwinds cleanly
  instead of orphaning an `ssh` child.

## Install

```bash
curl -fsSL "https://raw.githubusercontent.com/Dicklesworthstone/vibe_cockpit/main/install.sh" | bash
vc --version
```

Prebuilt binaries for Linux (x86-64, arm64) and macOS (Intel, Apple Silicon).

> **Note on building from source.** `vc` cannot be `cargo install`ed today. It has path
> dependencies on two sibling checkouts, [`frankentui`](https://github.com/Dicklesworthstone/frankentui)
> and [`frankensqlite`](https://github.com/Dicklesworthstone/frankensqlite), so a source
> build needs them cloned alongside this repo. `make siblings` checks your layout and
> tells you what is missing.

## Quick Start

```bash
# Write a starter config and show where it lives
vc config wizard
vc config paths

# One collection pass, then look at the result
vc collect
vc status

# Run it continuously: collect -> score -> alert, every interval
vc daemon

# The interactive console
vc tui

# Machine-readable, for agents
vc robot triage
vc --format json health score
```

## What It Collects

16 collectors, each shelling out to a tool you already have and parsing its JSON (or
reading its SQLite / JSONL directly). A collector whose tool is absent is skipped, not
fatal.

| Collector | Source | What lands in the store |
|:--|:--|:--|
| `fallback_probe` | `/proc`, `uname`, `df` | Always-on baseline: CPU, load, memory, disk |
| `sysmoni` | `sysmoni --json` | Richer system metrics when available |
| `caut` | `caut usage --json` | Remaining quota across 16 AI providers |
| `caam` | `caam limits/status --json` | Account limits and current account |
| `cass` | `cass health/stats --json` | Session-history index health |
| `ntm` | `ntm --robot-status` | Agent sessions, panes, tmux state |
| `dcg` | `~/.dcg/events.db` (SQLite) | Blocked destructive commands |
| `agent_mail` | Agent Mail SQLite archive | Messages and file reservations |
| `beads` | `bv --robot-triage`, `br list` | Issue graph and ready work |
| `ru` | `ru list/status --json` | Repo sync state: branch, dirty, ahead/behind |
| `github` | `gh repo/issue/pr` | Issues and PRs |
| `rch` | `~/.rch/compilations.jsonl` | Remote compilation queue |
| `rano` | `rano export --jsonl` | Outbound connections by provider |
| `pt` | `pt list --robot` | Zombie/abandoned processes |
| `afsc` | `afsc status/list` | Flywheel setup checks |
| `cloud_benchmarker` | local HTTP | Instance benchmark scores |

Remote machines are collected over SSH; add them with `vc machines add` and probe with
`vc machines probe`.

## Core Workflows

### Watch the fleet

```bash
vc daemon                  # collect -> score -> alert on an interval
vc tui                     # 12-screen console, refreshes every 5s
vc web                     # read-only HTTP API + /metrics + /ws
```

### Ask it things

```bash
vc status                  # fleet summary
vc health score            # per-machine health, worst factor first
vc health freshness        # which collectors are stale
vc alert list              # what has fired
vc query ask "which machines are low on disk?"
```

### Drive it from an agent

```bash
vc robot triage            # versioned JSON envelope
vc robot health
vc mcp serve               # MCP server over stdio: 9 tools
vc mcp tools               # list them
```

The robot envelope is `{schema_version, data, warnings}` and is JSON-Schema'd under
`docs/schemas/`. `vc --format toon` emits a token-efficient encoding for prompt context.

## How Health Is Scored

Each machine gets an overall score in `[0, 1]` from weighted factors: `sys_cpu`,
`sys_memory`, `sys_load` (load1 per core), `sys_disk`, `rate_limit`, `process_health`
(collector success rate over the last hour), and `data_freshness`.

`data_freshness` is always emitted. That is deliberate: a machine you have never
collected from scores near zero rather than looking perfectly healthy because there is
nothing to complain about.

Scores land in `health_summary` / `health_factors` on every daemon tick, which is what
the fleet overview, the TUI, and `vc robot health` all read.

## Status: what is real, and what is not

This is not a finished product, and the parts that aren't finished say so rather than
faking it.

**Working end to end:** the collector framework (16 collectors, incremental cursors,
redaction, scheduler), the DuckDB store (27 migrations), health scoring, threshold-based
alerting, the MCP server, the read-only web API, the query layer, and `vc migrate-db`
(a DuckDB → FrankenSQLite exporter with per-table row/NULL/sample verification).

**TUI:** 6 of 12 screens are store-backed and live — overview, machines, alerts,
sessions, events, settings. The other 6 (repos, accounts, mail, guardian, oracle, beads,
rch) render `NO DATA SOURCE YET` and name the table that exists but is not yet queried.

**Alerting:** only `Threshold` rules are evaluated. `Pattern`, `Absence` and
`RateOfChange` conditions parse and are stored, but nothing raises them yet.

**Storage:** DuckDB. The FrankenSQLite migration is a one-way exporter with a type map;
nothing reads the exported file back yet.

**Known bug:** the DuckDB → FrankenSQLite exporter does not round-trip `LIST` / `STRUCT`
columns byte-identically in all cases. `tests/e2e/migration_integrity.rs` covers it.

## Architecture

Twelve crates. Collection is the only thing that reaches outside the process; everything
else reads the store.

```
vc_collect  ──▶ vc_store (DuckDB, 27 migrations)
   │                 ▲
   │                 │
   └── executor      ├── vc_query   ── health scoring, NL→SQL, cost attribution
       (sh / ssh)    ├── vc_alert   ── rules, severities, channels
                     ├── vc_guardian── playbooks, autopilot
                     ├── vc_knowledge
                     └── vc_oracle  ── rate-limit forecasting
                             │
        ┌────────────────────┼────────────────────┐
        ▼                    ▼                    ▼
     vc_tui               vc_web               vc_mcp
   (FrankenTUI)      (axum, read-only)     (stdio JSON-RPC)
                             ▲
                          vc_cli  ── 29 subcommands, robot JSON, TOON
```

## Configuration

```toml
# ~/.config/vc/config.toml
[global]
db_path = "~/.local/share/vc/vc.duckdb"

[collectors]
fallback_probe = true    # always-on baseline
caut = true
cass = true
beads = true             # `bv_br` is accepted as a legacy alias
github = false           # needs a token
afsc = false
cloud_benchmarker = false

[[machines]]
id = "build-box"
host = "build.internal"
user = "ubuntu"
```

`vc config lint` will tell you what is wrong with it.

## Development

```bash
make help
make siblings      # check frankentui / frankensqlite are checked out next door
make check         # fmt + clippy + check
make test
```

## License

MIT with an OpenAI/Anthropic rider. See [LICENSE](LICENSE).
