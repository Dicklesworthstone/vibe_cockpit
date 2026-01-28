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

    // =============================================================================
    // VcStore Basic Tests
    // =============================================================================

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
    fn test_execute_returns_affected_rows() {
        let store = VcStore::open_memory().unwrap();
        store
            .execute("CREATE TABLE test_affected (id INTEGER, name TEXT)")
            .unwrap();

        // Insert should affect 1 row
        let affected = store
            .execute("INSERT INTO test_affected VALUES (1, 'a')")
            .unwrap();
        assert_eq!(affected, 1);

        // Insert multiple via execute_batch and verify with query
        store.execute_batch(
            "INSERT INTO test_affected VALUES (2, 'b'); INSERT INTO test_affected VALUES (3, 'c');"
        ).unwrap();

        let results = store.query_json("SELECT * FROM test_affected").unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_execute_batch() {
        let store = VcStore::open_memory().unwrap();
        store.execute_batch(r#"
            CREATE TABLE batch_test (id INTEGER, value TEXT);
            INSERT INTO batch_test VALUES (1, 'first');
            INSERT INTO batch_test VALUES (2, 'second');
        "#).unwrap();

        let results = store.query_json("SELECT * FROM batch_test ORDER BY id").unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0]["value"], "first");
        assert_eq!(results[1]["value"], "second");
    }

    // =============================================================================
    // Cursor Tests
    // =============================================================================

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
    fn test_cursor_different_machines() {
        let store = VcStore::open_memory().unwrap();

        store.set_cursor("machine1", "src", "key", "value1").unwrap();
        store.set_cursor("machine2", "src", "key", "value2").unwrap();

        let c1 = store.get_cursor("machine1", "src", "key").unwrap();
        let c2 = store.get_cursor("machine2", "src", "key").unwrap();

        assert_eq!(c1, Some("value1".to_string()));
        assert_eq!(c2, Some("value2".to_string()));
    }

    #[test]
    fn test_cursor_different_keys() {
        let store = VcStore::open_memory().unwrap();

        store.set_cursor("m1", "src", "key_a", "a").unwrap();
        store.set_cursor("m1", "src", "key_b", "b").unwrap();

        let ca = store.get_cursor("m1", "src", "key_a").unwrap();
        let cb = store.get_cursor("m1", "src", "key_b").unwrap();

        assert_eq!(ca, Some("a".to_string()));
        assert_eq!(cb, Some("b".to_string()));
    }

    // =============================================================================
    // Migration Tests
    // =============================================================================

    #[test]
    fn test_migrations_idempotent() {
        let store = VcStore::open_memory().unwrap();
        // Run migrations again - should be idempotent
        store.run_migrations().unwrap();
        store.run_migrations().unwrap();
        // No panic = success
    }

    // =============================================================================
    // JSON Insert Tests
    // =============================================================================

    #[test]
    fn test_insert_json_valid_object() {
        let store = VcStore::open_memory().unwrap();
        store
            .execute("CREATE TABLE test_json_insert (id INTEGER, name TEXT, active BOOLEAN)")
            .unwrap();

        let row = serde_json::json!({
            "id": 1,
            "name": "test",
            "active": true
        });
        store.insert_json("test_json_insert", &row).unwrap();

        let results = store.query_json("SELECT * FROM test_json_insert").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["id"], 1);
        assert_eq!(results[0]["name"], "test");
        assert_eq!(results[0]["active"], true);
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
    fn test_insert_json_string_error() {
        let store = VcStore::open_memory().unwrap();
        store
            .execute("CREATE TABLE test_str (id INTEGER)")
            .unwrap();

        let result = store.insert_json("test_str", &serde_json::json!("just a string"));
        assert!(result.is_err());
    }

    #[test]
    fn test_insert_json_null_error() {
        let store = VcStore::open_memory().unwrap();
        store
            .execute("CREATE TABLE test_null (id INTEGER)")
            .unwrap();

        let result = store.insert_json("test_null", &serde_json::Value::Null);
        assert!(result.is_err());
    }

    #[test]
    fn test_insert_json_with_null_value() {
        let store = VcStore::open_memory().unwrap();
        store
            .execute("CREATE TABLE test_nullable (id INTEGER, optional_field TEXT)")
            .unwrap();

        let row = serde_json::json!({
            "id": 1,
            "optional_field": null
        });
        store.insert_json("test_nullable", &row).unwrap();

        let results = store.query_json("SELECT * FROM test_nullable").unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0]["optional_field"].is_null());
    }

    #[test]
    fn test_insert_json_with_nested_object() {
        let store = VcStore::open_memory().unwrap();
        store
            .execute("CREATE TABLE test_nested (id INTEGER, metadata TEXT)")
            .unwrap();

        let row = serde_json::json!({
            "id": 1,
            "metadata": {"nested": "value", "count": 42}
        });
        store.insert_json("test_nested", &row).unwrap();

        let results = store.query_json("SELECT * FROM test_nested").unwrap();
        assert_eq!(results.len(), 1);
        // Nested object should be serialized as JSON string
        let metadata_str = results[0]["metadata"].as_str().unwrap();
        assert!(metadata_str.contains("nested"));
    }

    #[test]
    fn test_insert_json_with_array_value() {
        let store = VcStore::open_memory().unwrap();
        store
            .execute("CREATE TABLE test_array (id INTEGER, tags TEXT)")
            .unwrap();

        let row = serde_json::json!({
            "id": 1,
            "tags": ["a", "b", "c"]
        });
        store.insert_json("test_array", &row).unwrap();

        let results = store.query_json("SELECT * FROM test_array").unwrap();
        assert_eq!(results.len(), 1);
        // Array should be serialized as JSON string
        let tags_str = results[0]["tags"].as_str().unwrap();
        assert!(tags_str.contains("a"));
    }

    // =============================================================================
    // JSON Batch Insert Tests
    // =============================================================================

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
    fn test_insert_json_batch_multiple() {
        let store = VcStore::open_memory().unwrap();
        store
            .execute("CREATE TABLE test_batch_multi (id INTEGER, name TEXT)")
            .unwrap();

        let rows = vec![
            serde_json::json!({"id": 1, "name": "first"}),
            serde_json::json!({"id": 2, "name": "second"}),
            serde_json::json!({"id": 3, "name": "third"}),
        ];
        let count = store.insert_json_batch("test_batch_multi", &rows).unwrap();
        assert_eq!(count, 3);

        let results = store.query_json("SELECT * FROM test_batch_multi ORDER BY id").unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0]["name"], "first");
        assert_eq!(results[1]["name"], "second");
        assert_eq!(results[2]["name"], "third");
    }

    // =============================================================================
    // Query Tests
    // =============================================================================

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
    fn test_query_scalar_string() {
        let store = VcStore::open_memory().unwrap();
        store
            .execute("CREATE TABLE test_str_scalar (id INTEGER, name TEXT)")
            .unwrap();
        store.execute("INSERT INTO test_str_scalar VALUES (1, 'hello')").unwrap();

        let name: String = store
            .query_scalar("SELECT name FROM test_str_scalar WHERE id = 1")
            .unwrap();
        assert_eq!(name, "hello");
    }

    #[test]
    fn test_query_scalar_float() {
        let store = VcStore::open_memory().unwrap();
        store
            .execute("CREATE TABLE test_float (id INTEGER, val DOUBLE)")
            .unwrap();
        store.execute("INSERT INTO test_float VALUES (1, 3.14)").unwrap();

        let val: f64 = store
            .query_scalar("SELECT val FROM test_float WHERE id = 1")
            .unwrap();
        assert!((val - 3.14).abs() < f64::EPSILON);
    }

    #[test]
    fn test_query_scalar_no_rows() {
        let store = VcStore::open_memory().unwrap();
        store.execute("CREATE TABLE test_empty (id INTEGER)").unwrap();

        let result: Result<i64, _> = store.query_scalar("SELECT id FROM test_empty");
        assert!(result.is_err());
    }

    #[test]
    fn test_query_json_empty() {
        let store = VcStore::open_memory().unwrap();
        store.execute("CREATE TABLE test_empty_json (id INTEGER)").unwrap();

        let results = store.query_json("SELECT * FROM test_empty_json").unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_query_json_multiple_rows() {
        let store = VcStore::open_memory().unwrap();
        store.execute_batch(r#"
            CREATE TABLE test_multi (id INTEGER, name TEXT);
            INSERT INTO test_multi VALUES (1, 'a');
            INSERT INTO test_multi VALUES (2, 'b');
            INSERT INTO test_multi VALUES (3, 'c');
        "#).unwrap();

        let results = store.query_json("SELECT * FROM test_multi ORDER BY id").unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0]["id"], 1);
        assert_eq!(results[1]["id"], 2);
        assert_eq!(results[2]["id"], 3);
    }

    // =============================================================================
    // Upsert Tests
    // =============================================================================

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

    #[test]
    fn test_upsert_json_empty() {
        let store = VcStore::open_memory().unwrap();
        store.execute_batch(r#"
            CREATE TABLE test_upsert_empty (
                id TEXT PRIMARY KEY,
                value INTEGER
            );
        "#).unwrap();

        let count = store.upsert_json("test_upsert_empty", &[], &["id"]).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_upsert_json_skips_non_objects() {
        let store = VcStore::open_memory().unwrap();
        store.execute_batch(r#"
            CREATE TABLE test_upsert_mixed (
                id TEXT PRIMARY KEY,
                value INTEGER
            );
        "#).unwrap();

        // Mix of valid object and invalid non-object
        let rows = vec![
            serde_json::json!({"id": "valid", "value": 1}),
            serde_json::json!("not an object"),
        ];
        // Only the valid object should be inserted
        let count = store.upsert_json("test_upsert_mixed", &rows, &["id"]).unwrap();
        assert_eq!(count, 1);

        let results = store.query_json("SELECT * FROM test_upsert_mixed").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["id"], "valid");
    }

    // =============================================================================
    // StoreError Tests
    // =============================================================================

    #[test]
    fn test_store_error_database_display() {
        // Create a database error by using invalid SQL
        let store = VcStore::open_memory().unwrap();
        let result = store.execute("INVALID SQL STATEMENT HERE");
        assert!(result.is_err());
        let err = result.unwrap_err();
        // DatabaseError should format with "Database error: ..."
        let msg = err.to_string();
        assert!(msg.contains("Database error") || msg.contains("error"), "Error: {}", msg);
    }

    #[test]
    fn test_store_error_migration_display() {
        let err = StoreError::MigrationError("failed to apply v3".to_string());
        assert_eq!(err.to_string(), "Migration error: failed to apply v3");
    }

    #[test]
    fn test_store_error_query_display() {
        let err = StoreError::QueryError("invalid query syntax".to_string());
        assert_eq!(err.to_string(), "Query error: invalid query syntax");
    }

    #[test]
    fn test_store_error_serialization_from() {
        // Invalid JSON should create SerializationError
        let bad_json = "{ invalid json }";
        let serde_err = serde_json::from_str::<serde_json::Value>(bad_json).unwrap_err();
        let store_err: StoreError = serde_err.into();
        assert!(store_err.to_string().contains("Serialization error"));
    }

    #[test]
    fn test_store_error_debug() {
        let err = StoreError::QueryError("test".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("QueryError"));
        assert!(debug.contains("test"));
    }

    // =============================================================================
    // json_value_to_sql Tests
    // =============================================================================

    #[test]
    fn test_json_value_to_sql_null() {
        let val = serde_json::Value::Null;
        let _boxed = json_value_to_sql(&val);
        // Just verify it doesn't panic and returns something
    }

    #[test]
    fn test_json_value_to_sql_bool() {
        let val_true = serde_json::json!(true);
        let val_false = serde_json::json!(false);
        let _boxed_true = json_value_to_sql(&val_true);
        let _boxed_false = json_value_to_sql(&val_false);
    }

    #[test]
    fn test_json_value_to_sql_integer() {
        let val = serde_json::json!(42);
        let _boxed = json_value_to_sql(&val);
    }

    #[test]
    fn test_json_value_to_sql_float() {
        let val = serde_json::json!(3.14);
        let _boxed = json_value_to_sql(&val);
    }

    #[test]
    fn test_json_value_to_sql_string() {
        let val = serde_json::json!("hello world");
        let _boxed = json_value_to_sql(&val);
    }

    #[test]
    fn test_json_value_to_sql_array() {
        let val = serde_json::json!([1, 2, 3]);
        let _boxed = json_value_to_sql(&val);
    }

    #[test]
    fn test_json_value_to_sql_object() {
        let val = serde_json::json!({"key": "value"});
        let _boxed = json_value_to_sql(&val);
    }

    #[test]
    fn test_json_value_to_sql_large_integer() {
        let val = serde_json::json!(9_223_372_036_854_775_807i64); // i64::MAX
        let _boxed = json_value_to_sql(&val);
    }

    #[test]
    fn test_json_value_to_sql_negative_integer() {
        let val = serde_json::json!(-42);
        let _boxed = json_value_to_sql(&val);
    }

    // =============================================================================
    // Integration Tests
    // =============================================================================

    #[test]
    fn test_full_workflow() {
        let store = VcStore::open_memory().unwrap();

        // Create table
        store.execute_batch(r#"
            CREATE TABLE workflow_test (
                machine_id TEXT PRIMARY KEY,
                hostname TEXT,
                status TEXT,
                cpu_pct DOUBLE
            );
        "#).unwrap();

        // Insert data
        let machines = vec![
            serde_json::json!({
                "machine_id": "m1",
                "hostname": "server-01",
                "status": "online",
                "cpu_pct": 45.5
            }),
            serde_json::json!({
                "machine_id": "m2",
                "hostname": "server-02",
                "status": "offline",
                "cpu_pct": 0.0
            }),
        ];
        store.insert_json_batch("workflow_test", &machines).unwrap();

        // Query data
        let results = store.query_json("SELECT * FROM workflow_test WHERE status = 'online'").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["hostname"], "server-01");

        // Update via upsert
        let updates = vec![serde_json::json!({
            "machine_id": "m2",
            "hostname": "server-02",
            "status": "online",
            "cpu_pct": 30.0
        })];
        store.upsert_json("workflow_test", &updates, &["machine_id"]).unwrap();

        // Verify update
        let online = store.query_json("SELECT * FROM workflow_test WHERE status = 'online' ORDER BY machine_id").unwrap();
        assert_eq!(online.len(), 2);

        // Scalar query
        let count: i64 = store.query_scalar("SELECT COUNT(*) FROM workflow_test").unwrap();
        assert_eq!(count, 2);

        // Cursor management
        store.set_cursor("m1", "collector", "last_poll", "2026-01-28T00:00:00Z").unwrap();
        let cursor = store.get_cursor("m1", "collector", "last_poll").unwrap();
        assert_eq!(cursor, Some("2026-01-28T00:00:00Z".to_string()));
    }

    #[test]
    fn test_db_path_accessor() {
        let store = VcStore::open_memory().unwrap();
        assert_eq!(store.db_path(), ":memory:");
    }
}
