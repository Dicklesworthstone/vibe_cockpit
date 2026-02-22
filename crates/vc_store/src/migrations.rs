//! Database migrations for `vc_store`

use crate::StoreError;
use duckdb::Connection;
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
pub fn run_all(conn: &Connection) -> Result<(), StoreError> {
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
