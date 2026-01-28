//! vc_store - DuckDB storage layer for Vibe Cockpit
//!
//! This crate provides:
//! - DuckDB connection management
//! - Schema migrations
//! - Data ingestion helpers
//! - Query utilities

use duckdb::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};
use thiserror::Error;
use tracing::{info, instrument};

pub mod migrations;
pub mod schema;

/// Storage errors
#[derive(Error, Debug)]
pub enum StoreError {
    #[error("Database error: {0}")]
    DatabaseError(#[from] duckdb::Error),

    #[error("Migration error: {0}")]
    MigrationError(String),

    #[error("Query error: {0}")]
    QueryError(String),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Main storage handle
pub struct VcStore {
    conn: Arc<Mutex<Connection>>,
    db_path: String,
}

impl VcStore {
    /// Open or create database at path
    #[instrument]
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        info!(path = %path.display(), "Opening DuckDB database");

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)?;

        // Set pragmas for performance
        conn.execute_batch(
            r#"
            PRAGMA threads=4;
            PRAGMA memory_limit='512MB';
        "#,
        )?;

        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
            db_path: path.to_string_lossy().to_string(),
        };

        // Run migrations
        store.run_migrations()?;

        Ok(store)
    }

    /// Open in-memory database (for testing)
    pub fn open_memory() -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory()?;

        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
            db_path: ":memory:".to_string(),
        };

        store.run_migrations()?;

        Ok(store)
    }

    /// Run all pending migrations
    fn run_migrations(&self) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        migrations::run_all(&conn)?;
        Ok(())
    }

    /// Execute a query that returns no results
    pub fn execute(&self, sql: &str) -> Result<usize, StoreError> {
        let conn = self.conn.lock().unwrap();
        let affected = conn.execute(sql, [])?;
        Ok(affected)
    }

    /// Execute a batch of SQL statements
    pub fn execute_batch(&self, sql: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(sql)?;
        Ok(())
    }

    /// Insert a row into a table from JSON
    /// Note: This extracts key-value pairs from the JSON object
    pub fn insert_json(&self, table: &str, json: &serde_json::Value) -> Result<(), StoreError> {
        if let serde_json::Value::Object(map) = json {
            let conn = self.conn.lock().unwrap();

            let columns: Vec<&str> = map.keys().map(|k| k.as_str()).collect();
            let placeholders: Vec<&str> = columns.iter().map(|_| "?").collect();

            let sql = format!(
                "INSERT INTO {} ({}) VALUES ({})",
                table,
                columns.join(", "),
                placeholders.join(", ")
            );

            let mut stmt = conn.prepare(&sql)?;

            let params: Vec<Box<dyn duckdb::ToSql>> = map
                .values()
                .map(|v| json_value_to_sql(v))
                .collect();

            let param_refs: Vec<&dyn duckdb::ToSql> =
                params.iter().map(|b| b.as_ref()).collect();

            stmt.execute(param_refs.as_slice())?;
            Ok(())
        } else {
            Err(StoreError::QueryError(
                "insert_json requires a JSON object".to_string(),
            ))
        }
    }

    /// Insert multiple rows from JSON array
    pub fn insert_json_batch(
        &self,
        table: &str,
        rows: &[serde_json::Value],
    ) -> Result<usize, StoreError> {
        if rows.is_empty() {
            return Ok(0);
        }

        let mut count = 0;
        for row in rows {
            self.insert_json(table, row)?;
            count += 1;
        }
        Ok(count)
    }

    /// Query and return results as JSON
    pub fn query_json(&self, sql: &str) -> Result<Vec<serde_json::Value>, StoreError> {
        let conn = self.conn.lock().unwrap();

        // Wrap query to output each row as JSON using DuckDB's to_json()
        let json_sql = format!("SELECT to_json(_row) FROM ({sql}) AS _row");

        let mut stmt = conn.prepare(&json_sql)?;
        let mut rows = stmt.query([])?;

        let mut results = Vec::new();
        while let Some(row) = rows.next()? {
            let json_str: String = row.get(0)?;
            let value: serde_json::Value = serde_json::from_str(&json_str)?;
            results.push(value);
        }
        Ok(results)
    }

    /// Query for a single scalar value
    pub fn query_scalar<T: duckdb::types::FromSql>(&self, sql: &str) -> Result<T, StoreError> {
        let conn = self.conn.lock().unwrap();
        let value: T = conn.query_row(sql, [], |row| row.get(0))?;
        Ok(value)
    }

    /// Get database path
    pub fn db_path(&self) -> &str {
        &self.db_path
    }

    /// Get cursor for incremental collection
    pub fn get_cursor(
        &self,
        machine_id: &str,
        source: &str,
        key: &str,
    ) -> Result<Option<String>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            "SELECT cursor_value FROM ingestion_cursors WHERE machine_id = ? AND source = ? AND cursor_key = ?",
            duckdb::params![machine_id, source, key],
            |row| row.get(0),
        );

        match result {
            Ok(v) => Ok(Some(v)),
            Err(duckdb::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Update cursor after successful collection
    pub fn set_cursor(
        &self,
        machine_id: &str,
        source: &str,
        key: &str,
        value: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT OR REPLACE INTO ingestion_cursors (machine_id, source, cursor_key, cursor_value, updated_at)
            VALUES (?, ?, ?, ?, current_timestamp)
            "#,
            duckdb::params![machine_id, source, key, value],
        )?;
        Ok(())
    }

    /// Insert or replace rows (handles conflicts via PRIMARY KEY)
    /// Uses INSERT OR REPLACE which replaces the row if a conflict occurs
    pub fn upsert_json(
        &self,
        table: &str,
        rows: &[serde_json::Value],
        _conflict_columns: &[&str],
    ) -> Result<usize, StoreError> {
        if rows.is_empty() {
            return Ok(0);
        }

        let conn = self.conn.lock().unwrap();
        let mut count = 0;

        for row in rows {
            if let serde_json::Value::Object(map) = row {
                let columns: Vec<&str> = map.keys().map(|k| k.as_str()).collect();
                let placeholders: Vec<&str> = columns.iter().map(|_| "?").collect();

                let sql = format!(
                    "INSERT OR REPLACE INTO {} ({}) VALUES ({})",
                    table,
                    columns.join(", "),
                    placeholders.join(", ")
                );

                let mut stmt = conn.prepare(&sql)?;

                let params: Vec<Box<dyn duckdb::ToSql>> = map
                    .values()
                    .map(|v| json_value_to_sql(v))
                    .collect();

                let param_refs: Vec<&dyn duckdb::ToSql> =
                    params.iter().map(|b| b.as_ref()).collect();

                stmt.execute(param_refs.as_slice())?;
                count += 1;
            }
        }

        Ok(count)
    }
}

/// Convert JSON value to a SQL parameter
fn json_value_to_sql(value: &serde_json::Value) -> Box<dyn duckdb::ToSql> {
    match value {
        serde_json::Value::Null => Box::new(None::<String>),
        serde_json::Value::Bool(b) => Box::new(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Box::new(i)
            } else if let Some(f) = n.as_f64() {
                Box::new(f)
            } else {
                Box::new(n.to_string())
            }
        }
        serde_json::Value::String(s) => Box::new(s.clone()),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            Box::new(serde_json::to_string(value).unwrap_or_default())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_memory() {
        let store = VcStore::open_memory().unwrap();
        assert_eq!(store.db_path(), ":memory:");
    }

    #[test]
    fn test_execute() {
        let store = VcStore::open_memory().unwrap();
        store
            .execute("CREATE TABLE test (id INTEGER, name TEXT)")
            .unwrap();
        store
            .execute("INSERT INTO test VALUES (1, 'hello')")
            .unwrap();

        let results = store.query_json("SELECT * FROM test").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_cursor_get_set() {
        let store = VcStore::open_memory().unwrap();

        // Initially no cursor
        let cursor = store.get_cursor("machine1", "collector_a", "last_ts").unwrap();
        assert!(cursor.is_none());

        // Set cursor
        store.set_cursor("machine1", "collector_a", "last_ts", "2026-01-27T12:00:00Z").unwrap();

        // Get cursor
        let cursor = store.get_cursor("machine1", "collector_a", "last_ts").unwrap();
        assert_eq!(cursor, Some("2026-01-27T12:00:00Z".to_string()));

        // Update cursor
        store.set_cursor("machine1", "collector_a", "last_ts", "2026-01-27T13:00:00Z").unwrap();
        let cursor = store.get_cursor("machine1", "collector_a", "last_ts").unwrap();
        assert_eq!(cursor, Some("2026-01-27T13:00:00Z".to_string()));

        // Different source
        let other = store.get_cursor("machine1", "collector_b", "last_ts").unwrap();
        assert!(other.is_none());
    }

    #[test]
    fn test_migrations_idempotent() {
        let store = VcStore::open_memory().unwrap();
        // Run migrations again - should be idempotent
        store.run_migrations().unwrap();
        store.run_migrations().unwrap();
        // No panic = success
    }

    #[test]
    fn test_insert_json_non_object_error() {
        let store = VcStore::open_memory().unwrap();
        store
            .execute("CREATE TABLE test_insert (id INTEGER, name TEXT)")
            .unwrap();

        let result = store.insert_json("test_insert", &serde_json::json!(["not", "object"]));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("insert_json requires a JSON object"));
    }

    #[test]
    fn test_insert_json_batch_empty() {
        let store = VcStore::open_memory().unwrap();
        store
            .execute("CREATE TABLE test_batch (id INTEGER, name TEXT)")
            .unwrap();

        let count = store
            .insert_json_batch("test_batch", &[])
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_query_scalar() {
        let store = VcStore::open_memory().unwrap();
        store
            .execute("CREATE TABLE test_scalar (id INTEGER, value INTEGER)")
            .unwrap();
        store.execute("INSERT INTO test_scalar VALUES (1, 42)").unwrap();

        let value: i64 = store
            .query_scalar("SELECT value FROM test_scalar WHERE id = 1")
            .unwrap();
        assert_eq!(value, 42);
    }

    #[test]
    fn test_upsert_json() {
        let store = VcStore::open_memory().unwrap();

        // Create a test table with primary key
        store.execute_batch(r#"
            CREATE TABLE test_upsert (
                id TEXT PRIMARY KEY,
                value INTEGER
            );
        "#).unwrap();

        // Insert initial data
        let rows = vec![
            serde_json::json!({"id": "a", "value": 1}),
            serde_json::json!({"id": "b", "value": 2}),
        ];
        let count = store.upsert_json("test_upsert", &rows, &["id"]).unwrap();
        assert_eq!(count, 2);

        // Upsert with conflict on id 'a'
        let rows = vec![
            serde_json::json!({"id": "a", "value": 10}), // Update existing
            serde_json::json!({"id": "c", "value": 3}),  // Insert new
        ];
        store.upsert_json("test_upsert", &rows, &["id"]).unwrap();

        let results = store.query_json("SELECT * FROM test_upsert ORDER BY id").unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0]["value"], 10); // Updated
        assert_eq!(results[1]["value"], 2);  // Unchanged
        assert_eq!(results[2]["value"], 3);  // New
    }
}
