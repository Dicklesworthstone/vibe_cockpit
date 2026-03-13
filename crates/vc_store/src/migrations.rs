//! Database migrations for `vc_store`
//!
//! # DuckDB → FrankenSQLite Translation Map (bd-axj audit, 2026-03-12)
//!
//! Single source of truth for every DuckDB-specific construct found in
//! migration SQL files and Rust query code. Downstream beads (bd-dfl,
//! bd-phr, bd-h6y, bd-zut, bd-s8lm) reference this table.
//!
//! ## Type Translations (migrations)
//!
//! | DuckDB          | SQLite        | Count | Notes                                    |
//! |-----------------|---------------|-------|------------------------------------------|
//! | TIMESTAMP       | TEXT          |    81 | Store ISO-8601, query with datetime()    |
//! | BOOLEAN         | INTEGER       |    29 | 0 = false, 1 = true; DEFAULT FALSE → 0   |
//! | BIGINT          | INTEGER       |    48 | SQLite integers are already 64-bit        |
//! | DOUBLE          | REAL          |    35 | Direct equivalent                         |
//! | TEXT[]           | TEXT          |     3 | JSON array string, query via json_each()  |
//! | DOUBLE[]         | TEXT          |     1 | JSON array of floats (004 dna_embedding)  |
//! | CREATE TYPE ENUM | TEXT + CHECK  |     1 | CHECK(col IN ('v1','v2',...)) (003)        |
//! | VARCHAR          | TEXT          |     — | Drop size limit (SQLite ignores it)       |
//!
//! ## Function Translations (Rust SQL strings)
//!
//! | DuckDB                              | SQLite                                                   | Count | Locations                               |
//! |-------------------------------------|----------------------------------------------------------|-------|-----------------------------------------|
//! | `to_json(_row)`                     | Rust-side `serde_json` row construction                  |    12 | vc_store/lib.rs (518..3285)             |
//! | `current_timestamp`                 | `datetime('now')`                                        |    33 | vc_store, vc_query, migrations          |
//! | `current_timestamp - INTERVAL 'Xu'` | `datetime('now', '-X unit')`                             |    19 | vc_store (4), vc_query/nl (11), cost (3), digest (1) |
//! | `CAST(x AS TIMESTAMP)`              | Remove — TEXT already stores ISO-8601                    |     6 | vc_store (3), vc_query/nl (2), vc_web (1) |
//! | `date_trunc('week', ts)`            | `strftime('%Y-%m-%d', ts, 'weekday 0', '-6 days')`      |     1 | vc_query/nl.rs:284                      |
//! | `date_trunc('month', ts)`           | `strftime('%Y-%m-01', ts)`                               |     1 | vc_query/nl.rs:294                      |
//! | `DISTINCT ON (cols)`                | `ROW_NUMBER() OVER (PARTITION BY cols ORDER BY …) = 1`   |     1 | vc_query/cost.rs:228                    |
//! | `ILIKE`                             | `LIKE` (SQLite LIKE is case-insensitive for ASCII)       |     3 | vc_query/nl.rs:640, vc_knowledge:471    |
//! | `list_contains(arr, val)`           | `EXISTS (SELECT 1 FROM json_each(arr) WHERE value=val)`  |     1 | vc_knowledge/lib.rs:530                 |
//! | `EXTRACT(EPOCH FROM ts)`            | `CAST(strftime('%s', ts) AS INTEGER)`                    |     2 | vc_store:1084, vc_web:567               |
//! | `COUNT(*) FILTER (WHERE cond)`      | `SUM(CASE WHEN cond THEN 1 ELSE 0 END)`                 |     1 | vc_web/lib.rs:595                       |
//! | `now()`                             | `datetime('now')`                                        |     2 | migrations 003 only                     |
//! | `DEFAULT current_timestamp`         | `DEFAULT (datetime('now'))`                              |   ~30 | Migration DEFAULT clauses               |
//!
//! ## Rust API Translations (DuckDB crate → fsqlite)
//!
//! | DuckDB crate                     | FrankenSQLite                             | Count | Notes                     |
//! |----------------------------------|-------------------------------------------|-------|---------------------------|
//! | `duckdb::params![…]`             | `fsqlite::params![…]`                     |     2 | vc_knowledge:329,407      |
//! | `duckdb::Connection`             | `fsqlite::Connection`                     |     — | All crate entry points    |
//! | `QueryReturnedNoRows`            | `FrankenError::QueryReturnedNoRows`       |     — | See bd-bvt for audit      |
//! | `Box<dyn duckdb::ToSql>`         | `Box<dyn fsqlite::ToSql>` (if needed)     |     — | See bd-bvt for audit      |
//!
//! ## Syntax Notes
//!
//! - `ON CONFLICT DO UPDATE`  — supported in SQLite (same syntax)
//! - `RETURNING` clause       — supported since SQLite 3.35
//! - `PRAGMA threads=N`       — not found; remove if encountered
//! - `PRAGMA memory_limit`    — use `PRAGMA cache_size=-N` (kilobytes)
//! - json1 extension          — enabled by default in FrankenSQLite
//!
//! ## Migration-Specific Risks
//!
//! - **003_knowledge_base.sql**: Only file with `CREATE TYPE … AS ENUM` — must
//!   convert to `TEXT + CHECK(col IN (…))` constraint.
//! - **003_knowledge_base.sql**: Uses `now()` (DuckDB alias) instead of
//!   `current_timestamp` — translate to `datetime('now')`.
//! - **001_initial_schema.sql**: 3 `TEXT[]` columns (tags, recipients, channels)
//!   — convert to `TEXT` and query via `json_each()`.
//! - **004_agent_dna.sql**: 1 `DOUBLE[]` column (dna_embedding) — convert to
//!   `TEXT` storing JSON float array.

#![allow(clippy::doc_markdown)]

use crate::{StoreConnectionGuard, StoreError};
use tracing::{debug, info};

/// Migration definition
struct Migration {
    version: u32,
    name: &'static str,
    sql: &'static str,
}

/// All migrations in order
const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "initial_schema",
        sql: include_str!("migrations/001_initial_schema.sql"),
    },
    Migration {
        version: 2,
        name: "machine_registry",
        sql: include_str!("migrations/002_machine_registry.sql"),
    },
    Migration {
        version: 3,
        name: "knowledge_base",
        sql: include_str!("migrations/003_knowledge_base.sql"),
    },
    Migration {
        version: 4,
        name: "agent_dna",
        sql: include_str!("migrations/004_agent_dna.sql"),
    },
    Migration {
        version: 5,
        name: "experiments",
        sql: include_str!("migrations/005_experiments.sql"),
    },
    Migration {
        version: 6,
        name: "ntm_collector",
        sql: include_str!("migrations/006_ntm_collector.sql"),
    },
    Migration {
        version: 7,
        name: "cost_attribution",
        sql: include_str!("migrations/007_cost_attribution.sql"),
    },
    Migration {
        version: 8,
        name: "afsc_collector",
        sql: include_str!("migrations/008_afsc_collector.sql"),
    },
    Migration {
        version: 9,
        name: "cloud_benchmarker",
        sql: include_str!("migrations/009_cloud_benchmarker.sql"),
    },
    Migration {
        version: 10,
        name: "retention_log",
        sql: include_str!("migrations/010_retention_log.sql"),
    },
    Migration {
        version: 11,
        name: "collector_health",
        sql: include_str!("migrations/011_collector_health.sql"),
    },
    Migration {
        version: 12,
        name: "health_scores",
        sql: include_str!("migrations/012_health_scores.sql"),
    },
    Migration {
        version: 13,
        name: "alert_delivery_log",
        sql: include_str!("migrations/013_alert_delivery_log.sql"),
    },
    Migration {
        version: 14,
        name: "autopilot_decisions",
        sql: include_str!("migrations/014_autopilot_decisions.sql"),
    },
    Migration {
        version: 15,
        name: "incident_notes",
        sql: include_str!("migrations/015_incident_notes.sql"),
    },
    Migration {
        version: 16,
        name: "fleet_commands",
        sql: include_str!("migrations/016_fleet_commands.sql"),
    },
    Migration {
        version: 17,
        name: "mined_sessions",
        sql: include_str!("migrations/017_mined_sessions.sql"),
    },
    Migration {
        version: 18,
        name: "playbook_autogen",
        sql: include_str!("migrations/018_playbook_autogen.sql"),
    },
    Migration {
        version: 19,
        name: "incident_replay",
        sql: include_str!("migrations/019_incident_replay.sql"),
    },
    Migration {
        version: 20,
        name: "alert_routing",
        sql: include_str!("migrations/020_alert_routing.sql"),
    },
    Migration {
        version: 21,
        name: "adaptive_polling",
        sql: include_str!("migrations/021_adaptive_polling.sql"),
    },
    Migration {
        version: 22,
        name: "node_ingest",
        sql: include_str!("migrations/022_node_ingest.sql"),
    },
    Migration {
        version: 23,
        name: "redaction_events",
        sql: include_str!("migrations/023_redaction_events.sql"),
    },
    Migration {
        version: 24,
        name: "digest_reports",
        sql: include_str!("migrations/024_digest_reports.sql"),
    },
    Migration {
        version: 25,
        name: "collector_tables",
        sql: include_str!("migrations/025_collector_tables.sql"),
    },
    Migration {
        version: 26,
        name: "machine_status",
        sql: include_str!("migrations/026_machine_status.sql"),
    },
];

/// Run all pending migrations
///
/// # Errors
///
/// Returns [`StoreError`] if migration bookkeeping or any migration SQL fails.
pub(crate) fn run_all(conn: &StoreConnectionGuard<'_>) -> Result<(), StoreError> {
    // Create migrations table if not exists
    conn.execute_batch(
        r"
        CREATE TABLE IF NOT EXISTS _migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TIMESTAMP DEFAULT current_timestamp
        );
    ",
    )?;

    // Get current version
    let current_version: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM _migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    info!(current_version = current_version, "Checking migrations");

    // Apply pending migrations
    for migration in MIGRATIONS {
        if i64::from(migration.version) > current_version {
            info!(
                version = migration.version,
                name = migration.name,
                "Applying migration"
            );

            conn.execute_batch(migration.sql).map_err(|e| {
                StoreError::MigrationError(format!(
                    "Failed to apply migration {}: {}",
                    migration.name, e
                ))
            })?;

            conn.execute(
                "INSERT INTO _migrations (version, name) VALUES (?, ?)",
                [&migration.version.to_string(), &migration.name.to_string()],
            )?;

            debug!(version = migration.version, "Migration applied");
        }
    }

    Ok(())
}
