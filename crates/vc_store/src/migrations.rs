//! Database migrations for vc_store

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
];

/// Run all pending migrations
pub fn run_all(conn: &Connection) -> Result<(), StoreError> {
    // Create migrations table if not exists
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS _migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TIMESTAMP DEFAULT current_timestamp
        );
    "#,
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
        if migration.version as i64 > current_version {
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
