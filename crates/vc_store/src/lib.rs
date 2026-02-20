//! vc_store - DuckDB storage layer for Vibe Cockpit
//!
//! This crate provides:
//! - DuckDB connection management
//! - Schema migrations
//! - Data ingestion helpers
//! - Query utilities

use chrono::{DateTime, Utc};
use duckdb::Connection;
use serde::{Deserialize, Serialize};
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

/// Audit event categories
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventType {
    CollectorRun,
    AutopilotAction,
    UserCommand,
    GuardianAction,
}

impl AuditEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditEventType::CollectorRun => "collector_run",
            AuditEventType::AutopilotAction => "autopilot_action",
            AuditEventType::UserCommand => "user_command",
            AuditEventType::GuardianAction => "guardian_action",
        }
    }
}

impl std::str::FromStr for AuditEventType {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().as_str() {
            "collector_run" => Ok(AuditEventType::CollectorRun),
            "autopilot_action" => Ok(AuditEventType::AutopilotAction),
            "user_command" => Ok(AuditEventType::UserCommand),
            "guardian_action" => Ok(AuditEventType::GuardianAction),
            other => Err(format!("unknown audit event type: {other}")),
        }
    }
}

/// Audit event result
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditResult {
    Success,
    Failure,
    Skipped,
}

impl AuditResult {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditResult::Success => "success",
            AuditResult::Failure => "failure",
            AuditResult::Skipped => "skipped",
        }
    }
}

impl std::str::FromStr for AuditResult {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().as_str() {
            "success" => Ok(AuditResult::Success),
            "failure" => Ok(AuditResult::Failure),
            "skipped" => Ok(AuditResult::Skipped),
            other => Err(format!("unknown audit result: {other}")),
        }
    }
}

/// Audit event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub ts: DateTime<Utc>,
    pub event_type: AuditEventType,
    pub actor: String,
    pub machine_id: Option<String>,
    pub action: String,
    pub result: AuditResult,
    pub details: serde_json::Value,
}

impl AuditEvent {
    pub fn new(
        event_type: AuditEventType,
        actor: impl Into<String>,
        action: impl Into<String>,
        result: AuditResult,
        details: serde_json::Value,
    ) -> Self {
        Self {
            ts: Utc::now(),
            event_type,
            actor: actor.into(),
            machine_id: None,
            action: action.into(),
            result,
            details,
        }
    }

    pub fn with_machine_id(mut self, machine_id: impl Into<String>) -> Self {
        self.machine_id = Some(machine_id.into());
        self
    }
}

/// Filtering options for audit event queries
#[derive(Debug, Clone, Default)]
pub struct AuditEventFilter {
    pub event_type: Option<AuditEventType>,
    pub machine_id: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub limit: usize,
}

/// Trait for types that can produce audit events.
///
/// Implement this for collector runs, guardian actions, autopilot executions,
/// and any other actionable types to enable uniform audit logging.
pub trait Auditable {
    fn to_audit_event(&self) -> AuditEvent;
}

/// Retention policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    pub policy_id: String,
    pub table_name: String,
    pub retention_days: i32,
    pub aggregate_table: Option<String>,
    pub enabled: bool,
    pub last_vacuum_at: Option<String>,
}

/// Result of a vacuum operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VacuumResult {
    pub table_name: String,
    pub rows_deleted: i64,
    pub rows_would_delete: i64,
    pub rows_aggregated: i64,
    pub duration_ms: i64,
    pub dry_run: bool,
    pub error: Option<String>,
}

/// Collector health record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectorHealth {
    pub machine_id: String,
    pub collector: String,
    pub collected_at: String,
    pub success: bool,
    pub duration_ms: Option<i64>,
    pub rows_inserted: i64,
    pub bytes_parsed: i64,
    pub error_class: Option<String>,
    pub freshness_seconds: Option<i64>,
    pub payload_hash: Option<String>,
    pub collector_version: Option<String>,
    pub schema_version: Option<String>,
    pub cursor_json: Option<String>,
}

/// Machine baseline profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineBaseline {
    pub machine_id: String,
    pub baseline_window: String,
    pub computed_at: String,
    pub metrics_json: serde_json::Value,
}

/// Drift severity levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DriftSeverity {
    Info,
    Warning,
    Critical,
}

impl DriftSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            DriftSeverity::Info => "info",
            DriftSeverity::Warning => "warning",
            DriftSeverity::Critical => "critical",
        }
    }

    pub fn from_z_score(z: f64) -> Self {
        let abs_z = z.abs();
        if abs_z >= 4.0 {
            DriftSeverity::Critical
        } else if abs_z >= 3.0 {
            DriftSeverity::Warning
        } else {
            DriftSeverity::Info
        }
    }
}

impl std::str::FromStr for DriftSeverity {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().as_str() {
            "info" => Ok(DriftSeverity::Info),
            "warning" => Ok(DriftSeverity::Warning),
            "critical" => Ok(DriftSeverity::Critical),
            other => Err(format!("unknown drift severity: {other}")),
        }
    }
}

/// Drift event detected when a metric exceeds baseline thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftEvent {
    pub machine_id: String,
    pub detected_at: String,
    pub metric: String,
    pub current_value: f64,
    pub baseline_mean: f64,
    pub baseline_std: f64,
    pub z_score: f64,
    pub severity: DriftSeverity,
    pub evidence_json: Option<serde_json::Value>,
}

/// Freshness summary for a machine/collector pair
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreshnessSummary {
    pub machine_id: String,
    pub collector: String,
    pub last_success_at: Option<String>,
    pub freshness_seconds: i64,
    pub success_rate_24h: f64,
    pub total_runs_24h: i64,
    pub stale: bool,
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

    /// Get access to the underlying connection
    pub fn connection(&self) -> Arc<Mutex<Connection>> {
        Arc::clone(&self.conn)
    }

    /// Execute a query that returns no results
    pub fn execute(&self, sql: &str, params: &[&str]) -> Result<usize, StoreError> {
        let conn = self.conn.lock().unwrap();
        let affected = conn.execute(sql, duckdb::params_from_iter(params.iter()))?;
        Ok(affected)
    }

    /// Execute a query without parameters
    pub fn execute_simple(&self, sql: &str) -> Result<usize, StoreError> {
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

            let params: Vec<Box<dyn duckdb::ToSql>> =
                map.values().map(|v| json_value_to_sql(v)).collect();

            let param_refs: Vec<&dyn duckdb::ToSql> = params.iter().map(|b| b.as_ref()).collect();

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

    /// Insert a single audit event
    pub fn insert_audit_event(&self, event: &AuditEvent) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let details_json = serde_json::to_string(&event.details)?;

        // Get next ID (DuckDB doesn't auto-increment INTEGER PRIMARY KEY like SQLite)
        // COALESCE handles empty table case (returns 0 + 1 = 1)
        let next_id: i64 = conn.query_row(
            "SELECT COALESCE(MAX(id), 0) + 1 FROM audit_events",
            [],
            |row| row.get(0),
        )?;

        conn.execute(
            r#"
            INSERT INTO audit_events (id, ts, event_type, actor, machine_id, action, result, details_json)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            duckdb::params![
                next_id,
                event.ts.to_rfc3339(),
                event.event_type.as_str(),
                event.actor,
                event.machine_id,
                event.action,
                event.result.as_str(),
                details_json
            ],
        )?;
        Ok(())
    }

    /// List audit events with optional filters
    pub fn list_audit_events(
        &self,
        filter: &AuditEventFilter,
    ) -> Result<Vec<serde_json::Value>, StoreError> {
        let mut clauses: Vec<String> = Vec::new();

        if let Some(event_type) = filter.event_type {
            clauses.push(format!(
                "event_type = '{}'",
                escape_sql_literal(event_type.as_str())
            ));
        }

        if let Some(machine_id) = &filter.machine_id {
            clauses.push(format!("machine_id = '{}'", escape_sql_literal(machine_id)));
        }

        if let Some(since) = filter.since {
            clauses.push(format!(
                "ts >= '{}'",
                escape_sql_literal(&since.to_rfc3339())
            ));
        }

        let where_sql = if clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", clauses.join(" AND "))
        };

        let limit = clamp_audit_limit(filter.limit);
        let sql = format!(
            "SELECT id, ts, event_type, actor, machine_id, action, result, details_json \
             FROM audit_events {where_sql} ORDER BY ts DESC LIMIT {limit}"
        );

        self.query_json(&sql)
    }

    /// Fetch a single audit event by ID
    pub fn get_audit_event(&self, id: i64) -> Result<Option<serde_json::Value>, StoreError> {
        let sql = format!(
            "SELECT id, ts, event_type, actor, machine_id, action, result, details_json \
             FROM audit_events WHERE id = {id}"
        );
        let mut rows = self.query_json(&sql)?;
        Ok(rows.pop())
    }

    // =========================================================================
    // Retention Policy Methods
    // =========================================================================

    /// List all retention policies
    pub fn list_retention_policies(&self) -> Result<Vec<RetentionPolicy>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT policy_id, table_name, retention_days, aggregate_table, enabled, last_vacuum_at \
             FROM retention_policies ORDER BY table_name",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(RetentionPolicy {
                policy_id: row.get(0)?,
                table_name: row.get(1)?,
                retention_days: row.get(2)?,
                aggregate_table: row.get(3)?,
                enabled: row.get(4)?,
                last_vacuum_at: row.get(5)?,
            })
        })?;

        let mut policies = Vec::new();
        for row in rows {
            policies.push(row?);
        }
        Ok(policies)
    }

    /// Get a single retention policy by table name
    pub fn get_retention_policy(
        &self,
        table_name: &str,
    ) -> Result<Option<RetentionPolicy>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT policy_id, table_name, retention_days, aggregate_table, enabled, last_vacuum_at \
             FROM retention_policies WHERE table_name = ?",
        )?;

        let result = stmt.query_row([table_name], |row| {
            Ok(RetentionPolicy {
                policy_id: row.get(0)?,
                table_name: row.get(1)?,
                retention_days: row.get(2)?,
                aggregate_table: row.get(3)?,
                enabled: row.get(4)?,
                last_vacuum_at: row.get(5)?,
            })
        });

        match result {
            Ok(policy) => Ok(Some(policy)),
            Err(duckdb::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Set retention policy for a table (upsert)
    pub fn set_retention_policy(
        &self,
        table_name: &str,
        retention_days: i32,
        aggregate_table: Option<&str>,
        enabled: bool,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let policy_id = format!("retention_{}", table_name);

        conn.execute(
            "INSERT OR REPLACE INTO retention_policies (policy_id, table_name, retention_days, aggregate_table, enabled) \
             VALUES (?, ?, ?, ?, ?)",
            duckdb::params![policy_id, table_name, retention_days, aggregate_table, enabled],
        )?;

        Ok(())
    }

    /// Run vacuum for all enabled retention policies (or specific table)
    pub fn run_vacuum(
        &self,
        dry_run: bool,
        specific_table: Option<&str>,
    ) -> Result<Vec<VacuumResult>, StoreError> {
        let policies = self.list_retention_policies()?;
        let mut results = Vec::new();

        for policy in policies {
            // Skip disabled policies
            if !policy.enabled {
                continue;
            }

            // Skip if specific table requested and doesn't match
            if let Some(table) = specific_table {
                if policy.table_name != table {
                    continue;
                }
            }

            let result = self.vacuum_table(&policy, dry_run)?;
            results.push(result);
        }

        Ok(results)
    }

    /// Vacuum a single table based on its retention policy
    fn vacuum_table(
        &self,
        policy: &RetentionPolicy,
        dry_run: bool,
    ) -> Result<VacuumResult, StoreError> {
        let conn = self.conn.lock().unwrap();
        let start = std::time::Instant::now();

        // Calculate cutoff date
        let cutoff = Utc::now() - chrono::Duration::days(policy.retention_days as i64);
        let cutoff_str = cutoff.format("%Y-%m-%d %H:%M:%S").to_string();

        // Count rows that would be deleted
        // Try common timestamp column names
        let ts_column = self.detect_timestamp_column(&conn, &policy.table_name)?;

        let count_sql = format!(
            "SELECT COUNT(*) FROM {} WHERE {} < '{}'",
            policy.table_name, ts_column, cutoff_str
        );

        let rows_to_delete: i64 = conn
            .query_row(&count_sql, [], |row| row.get(0))
            .unwrap_or(0);

        if dry_run {
            // Log dry-run result
            self.log_vacuum_result(
                &conn,
                policy,
                0,
                0,
                start.elapsed().as_millis() as i64,
                true,
                None,
            )?;

            return Ok(VacuumResult {
                table_name: policy.table_name.clone(),
                rows_deleted: 0,
                rows_would_delete: rows_to_delete,
                rows_aggregated: 0,
                duration_ms: start.elapsed().as_millis() as i64,
                dry_run: true,
                error: None,
            });
        }

        // Actually delete old rows
        let delete_sql = format!(
            "DELETE FROM {} WHERE {} < '{}'",
            policy.table_name, ts_column, cutoff_str
        );

        let deleted = match conn.execute(&delete_sql, []) {
            Ok(n) => n as i64,
            Err(e) => {
                let error_msg = e.to_string();
                self.log_vacuum_result(
                    &conn,
                    policy,
                    0,
                    0,
                    start.elapsed().as_millis() as i64,
                    false,
                    Some(&error_msg),
                )?;
                return Ok(VacuumResult {
                    table_name: policy.table_name.clone(),
                    rows_deleted: 0,
                    rows_would_delete: rows_to_delete,
                    rows_aggregated: 0,
                    duration_ms: start.elapsed().as_millis() as i64,
                    dry_run: false,
                    error: Some(error_msg),
                });
            }
        };

        // Update last_vacuum_at
        conn.execute(
            "UPDATE retention_policies SET last_vacuum_at = current_timestamp WHERE policy_id = ?",
            [&policy.policy_id],
        )?;

        // Log success
        self.log_vacuum_result(
            &conn,
            policy,
            deleted,
            0,
            start.elapsed().as_millis() as i64,
            false,
            None,
        )?;

        Ok(VacuumResult {
            table_name: policy.table_name.clone(),
            rows_deleted: deleted,
            rows_would_delete: rows_to_delete,
            rows_aggregated: 0,
            duration_ms: start.elapsed().as_millis() as i64,
            dry_run: false,
            error: None,
        })
    }

    /// Detect the timestamp column for a table
    fn detect_timestamp_column(
        &self,
        conn: &Connection,
        table_name: &str,
    ) -> Result<String, StoreError> {
        // Common timestamp column names in order of preference
        let candidates = ["collected_at", "ts", "created_at", "timestamp", "time"];

        for col in candidates {
            let check_sql = format!(
                "SELECT 1 FROM information_schema.columns WHERE table_name = '{}' AND column_name = '{}' LIMIT 1",
                table_name, col
            );
            if conn.query_row(&check_sql, [], |_| Ok(())).is_ok() {
                return Ok(col.to_string());
            }
        }

        Err(StoreError::QueryError(format!(
            "No timestamp column found in table '{}'. Expected one of: {:?}",
            table_name, candidates
        )))
    }

    /// Log a vacuum operation to retention_log
    fn log_vacuum_result(
        &self,
        conn: &Connection,
        policy: &RetentionPolicy,
        rows_deleted: i64,
        rows_aggregated: i64,
        duration_ms: i64,
        dry_run: bool,
        error_message: Option<&str>,
    ) -> Result<(), StoreError> {
        // Get next ID
        let next_id: i64 = conn.query_row(
            "SELECT COALESCE(MAX(id), 0) + 1 FROM retention_log",
            [],
            |row| row.get(0),
        )?;

        conn.execute(
            "INSERT INTO retention_log (id, policy_id, table_name, rows_deleted, rows_aggregated, duration_ms, dry_run, error_message) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            duckdb::params![next_id, policy.policy_id, policy.table_name, rows_deleted, rows_aggregated, duration_ms, dry_run, error_message],
        )?;

        Ok(())
    }

    /// Get vacuum history
    pub fn list_vacuum_history(&self, limit: usize) -> Result<Vec<serde_json::Value>, StoreError> {
        let limit = limit.min(1000);
        let sql = format!(
            "SELECT id, ts, policy_id, table_name, rows_deleted, rows_aggregated, duration_ms, dry_run, error_message \
             FROM retention_log ORDER BY ts DESC LIMIT {}",
            limit
        );
        self.query_json(&sql)
    }

    // =========================================================================
    // Collector Health Methods
    // =========================================================================

    /// Record a collector health entry
    pub fn insert_collector_health(&self, health: &CollectorHealth) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO collector_health \
             (machine_id, collector, collected_at, success, duration_ms, rows_inserted, \
              bytes_parsed, error_class, freshness_seconds, payload_hash, \
              collector_version, schema_version, cursor_json) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            duckdb::params![
                health.machine_id,
                health.collector,
                health.collected_at,
                health.success,
                health.duration_ms,
                health.rows_inserted,
                health.bytes_parsed,
                health.error_class,
                health.freshness_seconds,
                health.payload_hash,
                health.collector_version,
                health.schema_version,
                health.cursor_json,
            ],
        )?;
        Ok(())
    }

    /// Get freshness summary for all collectors on a machine (or all machines)
    pub fn get_freshness_summaries(
        &self,
        machine_id: Option<&str>,
        stale_threshold_secs: i64,
    ) -> Result<Vec<FreshnessSummary>, StoreError> {
        let conn = self.conn.lock().unwrap();

        let machine_filter = match machine_id {
            Some(id) => format!("WHERE machine_id = '{}'", escape_sql_literal(id)),
            None => String::new(),
        };

        // For each machine/collector pair, get:
        // - last successful collection timestamp
        // - freshness in seconds (now - last success)
        // - success rate over last 24h
        // - total runs over last 24h
        // Cast current_timestamp to TIMESTAMP to match the collected_at column type
        // (DuckDB's current_timestamp returns TIMESTAMP WITH TIME ZONE)
        let sql = format!(
            "SELECT \
                machine_id, \
                collector, \
                CAST(MAX(CASE WHEN success THEN collected_at END) AS TEXT) AS last_success_at, \
                COALESCE(CAST(EXTRACT(EPOCH FROM (CAST(current_timestamp AS TIMESTAMP) - \
                    MAX(CASE WHEN success THEN collected_at END))) AS BIGINT), -1) AS freshness_seconds, \
                COALESCE(AVG(CASE WHEN collected_at > CAST(current_timestamp AS TIMESTAMP) - INTERVAL '24 hours' \
                    THEN CASE WHEN success THEN 1.0 ELSE 0.0 END END), 0.0) AS success_rate_24h, \
                COALESCE(COUNT(CASE WHEN collected_at > CAST(current_timestamp AS TIMESTAMP) - INTERVAL '24 hours' \
                    THEN 1 END), 0) AS total_runs_24h \
             FROM collector_health \
             {machine_filter} \
             GROUP BY machine_id, collector \
             ORDER BY machine_id, collector"
        );

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], |row| {
            let freshness_secs: i64 = row.get(3)?;
            Ok(FreshnessSummary {
                machine_id: row.get(0)?,
                collector: row.get(1)?,
                last_success_at: row.get(2)?,
                freshness_seconds: freshness_secs,
                success_rate_24h: row.get(4)?,
                total_runs_24h: row.get(5)?,
                stale: freshness_secs < 0 || freshness_secs > stale_threshold_secs,
            })
        })?;

        let mut summaries = Vec::new();
        for row in rows {
            summaries.push(row?);
        }
        Ok(summaries)
    }

    /// Get recent collector health entries
    pub fn list_collector_health(
        &self,
        machine_id: Option<&str>,
        collector: Option<&str>,
        limit: usize,
    ) -> Result<Vec<serde_json::Value>, StoreError> {
        let mut clauses: Vec<String> = Vec::new();

        if let Some(id) = machine_id {
            clauses.push(format!("machine_id = '{}'", escape_sql_literal(id)));
        }
        if let Some(c) = collector {
            clauses.push(format!("collector = '{}'", escape_sql_literal(c)));
        }

        let where_sql = if clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", clauses.join(" AND "))
        };

        let limit = limit.min(1000);
        let sql = format!(
            "SELECT machine_id, collector, collected_at, success, duration_ms, \
             rows_inserted, bytes_parsed, error_class, freshness_seconds, payload_hash \
             FROM collector_health {where_sql} \
             ORDER BY collected_at DESC LIMIT {limit}"
        );

        self.query_json(&sql)
    }

    // =========================================================================
    // Machine Baseline Methods
    // =========================================================================

    /// Upsert a machine baseline
    pub fn set_machine_baseline(
        &self,
        machine_id: &str,
        baseline_window: &str,
        metrics_json: &serde_json::Value,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let metrics_str = serde_json::to_string(metrics_json)?;

        conn.execute(
            "INSERT OR REPLACE INTO machine_baselines \
             (machine_id, baseline_window, computed_at, metrics_json) \
             VALUES (?, ?, current_timestamp, ?)",
            duckdb::params![machine_id, baseline_window, metrics_str],
        )?;
        Ok(())
    }

    /// Get a machine baseline
    pub fn get_machine_baseline(
        &self,
        machine_id: &str,
        baseline_window: &str,
    ) -> Result<Option<MachineBaseline>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            "SELECT machine_id, baseline_window, CAST(computed_at AS TEXT), metrics_json \
             FROM machine_baselines WHERE machine_id = ? AND baseline_window = ?",
            duckdb::params![machine_id, baseline_window],
            |row| {
                let metrics_str: String = row.get(3)?;
                Ok(MachineBaseline {
                    machine_id: row.get(0)?,
                    baseline_window: row.get(1)?,
                    computed_at: row.get(2)?,
                    metrics_json: serde_json::from_str(&metrics_str).unwrap_or_default(),
                })
            },
        );

        match result {
            Ok(baseline) => Ok(Some(baseline)),
            Err(duckdb::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// List all baselines for a machine
    pub fn list_machine_baselines(
        &self,
        machine_id: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, StoreError> {
        let where_clause = match machine_id {
            Some(id) => format!("WHERE machine_id = '{}'", escape_sql_literal(id)),
            None => String::new(),
        };
        let sql = format!(
            "SELECT machine_id, baseline_window, computed_at, metrics_json \
             FROM machine_baselines {where_clause} \
             ORDER BY machine_id, baseline_window"
        );
        self.query_json(&sql)
    }

    // =========================================================================
    // Drift Detection Methods
    // =========================================================================

    /// Record a drift event
    pub fn insert_drift_event(&self, event: &DriftEvent) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();

        let next_id: i64 = conn.query_row(
            "SELECT COALESCE(MAX(id), 0) + 1 FROM drift_events",
            [],
            |row| row.get(0),
        )?;

        let evidence_str = event
            .evidence_json
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_default());

        conn.execute(
            "INSERT INTO drift_events \
             (id, machine_id, detected_at, metric, current_value, baseline_mean, \
              baseline_std, z_score, severity, evidence_json) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            duckdb::params![
                next_id,
                event.machine_id,
                event.detected_at,
                event.metric,
                event.current_value,
                event.baseline_mean,
                event.baseline_std,
                event.z_score,
                event.severity.as_str(),
                evidence_str,
            ],
        )?;
        Ok(())
    }

    /// List recent drift events
    pub fn list_drift_events(
        &self,
        machine_id: Option<&str>,
        severity: Option<&str>,
        limit: usize,
    ) -> Result<Vec<serde_json::Value>, StoreError> {
        let mut clauses: Vec<String> = Vec::new();

        if let Some(id) = machine_id {
            clauses.push(format!("machine_id = '{}'", escape_sql_literal(id)));
        }
        if let Some(s) = severity {
            clauses.push(format!("severity = '{}'", escape_sql_literal(s)));
        }

        let where_sql = if clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", clauses.join(" AND "))
        };

        let limit = limit.min(1000);
        let sql = format!(
            "SELECT id, machine_id, detected_at, metric, current_value, baseline_mean, \
             baseline_std, z_score, severity, evidence_json \
             FROM drift_events {where_sql} \
             ORDER BY detected_at DESC LIMIT {limit}"
        );

        self.query_json(&sql)
    }

    /// Detect drift by comparing a current value against a machine baseline.
    /// Returns a DriftEvent if z-score exceeds the threshold.
    pub fn check_drift(
        &self,
        machine_id: &str,
        metric: &str,
        current_value: f64,
        z_threshold: f64,
        baseline_window: &str,
    ) -> Result<Option<DriftEvent>, StoreError> {
        let baseline = self.get_machine_baseline(machine_id, baseline_window)?;

        let Some(baseline) = baseline else {
            return Ok(None);
        };

        // Extract mean and std for the requested metric from the baseline JSON
        let metrics = &baseline.metrics_json;
        let metric_data = &metrics[metric];

        if metric_data.is_null() {
            return Ok(None);
        }

        let mean = metric_data["mean"].as_f64().unwrap_or(0.0);
        let std = metric_data["std"].as_f64().unwrap_or(0.0);

        // Avoid division by zero
        if std < f64::EPSILON {
            return Ok(None);
        }

        let z_score = (current_value - mean) / std;

        if z_score.abs() >= z_threshold {
            let severity = DriftSeverity::from_z_score(z_score);
            let event = DriftEvent {
                machine_id: machine_id.to_string(),
                detected_at: Utc::now().to_rfc3339(),
                metric: metric.to_string(),
                current_value,
                baseline_mean: mean,
                baseline_std: std,
                z_score,
                severity,
                evidence_json: Some(serde_json::json!({
                    "baseline_window": baseline_window,
                    "computed_at": baseline.computed_at,
                    "threshold": z_threshold,
                })),
            };

            // Persist the drift event
            self.insert_drift_event(&event)?;

            Ok(Some(event))
        } else {
            Ok(None)
        }
    }

    // =========================================================================
    // Alert Delivery Log Methods
    // =========================================================================

    /// Log an alert delivery attempt
    pub fn insert_delivery_log(
        &self,
        alert_id: &str,
        channel_type: &str,
        status: &str,
        error_message: Option<&str>,
        duration_ms: Option<i64>,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();

        let next_id: i64 = conn.query_row(
            "SELECT COALESCE(MAX(id), 0) + 1 FROM alert_delivery_log",
            [],
            |row| row.get(0),
        )?;

        conn.execute(
            "INSERT INTO alert_delivery_log (id, alert_id, channel_type, status, error_message, duration_ms)
             VALUES (?, ?, ?, ?, ?, ?)",
            duckdb::params![next_id, alert_id, channel_type, status, error_message, duration_ms],
        )?;
        Ok(())
    }

    /// Update delivery status (e.g., after retry)
    pub fn update_delivery_status(
        &self,
        delivery_id: i64,
        status: &str,
        error_message: Option<&str>,
        retry_count: i32,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE alert_delivery_log SET status = ?, error_message = ?, retry_count = ? WHERE id = ?",
            duckdb::params![status, error_message, retry_count, delivery_id],
        )?;
        Ok(())
    }

    /// List delivery logs for an alert
    pub fn list_delivery_logs(
        &self,
        alert_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<serde_json::Value>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let limit = if limit == 0 { 50 } else { limit.min(1000) };

        let (sql, params): (String, Vec<Box<dyn duckdb::ToSql>>) = if let Some(aid) = alert_id {
            (
                format!(
                    "SELECT id, alert_id, channel_type, CAST(delivered_at AS TEXT) AS delivered_at, \
                     status, error_message, retry_count, duration_ms \
                     FROM alert_delivery_log WHERE alert_id = ? \
                     ORDER BY delivered_at DESC LIMIT {}",
                    limit
                ),
                vec![Box::new(aid.to_string())],
            )
        } else {
            (
                format!(
                    "SELECT id, alert_id, channel_type, CAST(delivered_at AS TEXT) AS delivered_at, \
                     status, error_message, retry_count, duration_ms \
                     FROM alert_delivery_log \
                     ORDER BY delivered_at DESC LIMIT {}",
                    limit
                ),
                vec![],
            )
        };

        let mut stmt = conn.prepare(&sql)?;
        let param_refs: Vec<&dyn duckdb::ToSql> = params.iter().map(|b| b.as_ref()).collect();
        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, i64>(0)?,
                "alert_id": row.get::<_, String>(1)?,
                "channel_type": row.get::<_, String>(2)?,
                "delivered_at": row.get::<_, Option<String>>(3)?,
                "status": row.get::<_, String>(4)?,
                "error_message": row.get::<_, Option<String>>(5)?,
                "retry_count": row.get::<_, i32>(6)?,
                "duration_ms": row.get::<_, Option<i64>>(7)?,
            }))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Get delivery summary stats (total, succeeded, failed per channel)
    pub fn delivery_summary(&self) -> Result<Vec<serde_json::Value>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT channel_type, \
                    COUNT(*) AS total, \
                    SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END) AS succeeded, \
                    SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END) AS failed, \
                    SUM(CASE WHEN status = 'pending' THEN 1 ELSE 0 END) AS pending_count \
             FROM alert_delivery_log \
             GROUP BY channel_type \
             ORDER BY channel_type",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(serde_json::json!({
                "channel_type": row.get::<_, String>(0)?,
                "total": row.get::<_, i64>(1)?,
                "succeeded": row.get::<_, i64>(2)?,
                "failed": row.get::<_, i64>(3)?,
                "pending": row.get::<_, i64>(4)?,
            }))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    // =========================================================================
    // Autopilot Decision Methods
    // =========================================================================

    /// Log an autopilot decision
    pub fn insert_autopilot_decision(
        &self,
        decision_type: &str,
        reason: &str,
        confidence: f64,
        executed: bool,
        details_json: Option<&str>,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();

        let next_id: i64 = conn.query_row(
            "SELECT COALESCE(MAX(id), 0) + 1 FROM autopilot_decisions",
            [],
            |row| row.get(0),
        )?;

        conn.execute(
            "INSERT INTO autopilot_decisions (id, decision_type, reason, confidence, executed, details_json)
             VALUES (?, ?, ?, ?, ?, ?)",
            duckdb::params![next_id, decision_type, reason, confidence, executed, details_json],
        )?;
        Ok(())
    }

    /// List recent autopilot decisions
    pub fn list_autopilot_decisions(
        &self,
        decision_type: Option<&str>,
        limit: usize,
    ) -> Result<Vec<serde_json::Value>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let limit = if limit == 0 { 50 } else { limit.min(1000) };

        let (sql, params): (String, Vec<Box<dyn duckdb::ToSql>>) = if let Some(dt) = decision_type {
            (
                format!(
                    "SELECT id, decision_type, reason, confidence, executed, \
                     CAST(decided_at AS TEXT) AS decided_at, details_json \
                     FROM autopilot_decisions WHERE decision_type = ? \
                     ORDER BY decided_at DESC LIMIT {}",
                    limit
                ),
                vec![Box::new(dt.to_string())],
            )
        } else {
            (
                format!(
                    "SELECT id, decision_type, reason, confidence, executed, \
                     CAST(decided_at AS TEXT) AS decided_at, details_json \
                     FROM autopilot_decisions \
                     ORDER BY decided_at DESC LIMIT {}",
                    limit
                ),
                vec![],
            )
        };

        let mut stmt = conn.prepare(&sql)?;
        let param_refs: Vec<&dyn duckdb::ToSql> = params.iter().map(|b| b.as_ref()).collect();
        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, i64>(0)?,
                "decision_type": row.get::<_, String>(1)?,
                "reason": row.get::<_, String>(2)?,
                "confidence": row.get::<_, f64>(3)?,
                "executed": row.get::<_, bool>(4)?,
                "decided_at": row.get::<_, Option<String>>(5)?,
                "details_json": row.get::<_, Option<String>>(6)?,
            }))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Get autopilot decision summary (counts by type and executed status)
    pub fn autopilot_decision_summary(&self) -> Result<Vec<serde_json::Value>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT decision_type, \
                    COUNT(*) AS total, \
                    SUM(CASE WHEN executed THEN 1 ELSE 0 END) AS executed_count, \
                    SUM(CASE WHEN NOT executed THEN 1 ELSE 0 END) AS suggested_count \
             FROM autopilot_decisions \
             GROUP BY decision_type \
             ORDER BY decision_type",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(serde_json::json!({
                "decision_type": row.get::<_, String>(0)?,
                "total": row.get::<_, i64>(1)?,
                "executed": row.get::<_, i64>(2)?,
                "suggested": row.get::<_, i64>(3)?,
            }))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
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

                let params: Vec<Box<dyn duckdb::ToSql>> =
                    map.values().map(|v| json_value_to_sql(v)).collect();

                let param_refs: Vec<&dyn duckdb::ToSql> =
                    params.iter().map(|b| b.as_ref()).collect();

                stmt.execute(param_refs.as_slice())?;
                count += 1;
            }
        }

        Ok(count)
    }

    // ========================================================================
    // Incident Management
    // ========================================================================

    /// Create a new incident
    pub fn create_incident(
        &self,
        incident_id: &str,
        title: &str,
        severity: &str,
        description: Option<&str>,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO incidents (incident_id, title, description, severity, status, started_at, created_at) \
             VALUES (?, ?, ?, ?, 'open', current_timestamp, current_timestamp)",
            duckdb::params![incident_id, title, description, severity],
        )?;
        Ok(())
    }

    /// Get an incident by ID
    pub fn get_incident(&self, incident_id: &str) -> Result<Option<serde_json::Value>, StoreError> {
        let sql = "SELECT to_json(_row) FROM \
                   (SELECT * FROM incidents WHERE incident_id = ?) AS _row";
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(sql, [incident_id], |row| {
            let json_str: String = row.get(0)?;
            Ok(json_str)
        });

        match result {
            Ok(json_str) => {
                let val: serde_json::Value = serde_json::from_str(&json_str)?;
                Ok(Some(val))
            }
            Err(duckdb::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StoreError::DatabaseError(e)),
        }
    }

    /// List incidents with optional status filter
    pub fn list_incidents(
        &self,
        status: Option<&str>,
        limit: usize,
    ) -> Result<Vec<serde_json::Value>, StoreError> {
        let limit = if limit == 0 { 50 } else { limit.min(1000) };
        let (sql, params): (String, Vec<String>) = if let Some(status) = status {
            (
                format!(
                    "SELECT to_json(_row) FROM \
                     (SELECT * FROM incidents WHERE status = ? ORDER BY created_at DESC LIMIT {limit}) AS _row"
                ),
                vec![status.to_string()],
            )
        } else {
            (
                format!(
                    "SELECT to_json(_row) FROM \
                     (SELECT * FROM incidents ORDER BY created_at DESC LIMIT {limit}) AS _row"
                ),
                vec![],
            )
        };

        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&sql)?;

        let param_refs: Vec<&dyn duckdb::ToSql> = params
            .iter()
            .map(|p| p as &dyn duckdb::ToSql)
            .collect();

        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            let json_str: String = row.get(0)?;
            Ok(json_str)
        })?;

        let mut results = Vec::new();
        for row in rows {
            let json_str = row?;
            let val: serde_json::Value = serde_json::from_str(&json_str)
                .map_err(|e| StoreError::QueryError(format!("JSON parse error: {e}")))?;
            results.push(val);
        }
        Ok(results)
    }

    /// Update incident status
    pub fn update_incident_status(
        &self,
        incident_id: &str,
        status: &str,
        resolution: Option<&str>,
        root_cause: Option<&str>,
    ) -> Result<usize, StoreError> {
        let conn = self.conn.lock().unwrap();

        let mut set_clauses = vec!["status = ?".to_string(), "updated_at = current_timestamp".to_string()];
        let mut params: Vec<Box<dyn duckdb::ToSql>> = vec![Box::new(status.to_string())];

        if let Some(res) = resolution {
            set_clauses.push("resolution = ?".to_string());
            params.push(Box::new(res.to_string()));
        }

        if let Some(cause) = root_cause {
            set_clauses.push("root_cause = ?".to_string());
            params.push(Box::new(cause.to_string()));
        }

        // Add ended_at when closing
        if status == "closed" || status == "mitigated" {
            set_clauses.push("ended_at = current_timestamp".to_string());
        }

        params.push(Box::new(incident_id.to_string()));

        let sql = format!(
            "UPDATE incidents SET {} WHERE incident_id = ?",
            set_clauses.join(", ")
        );

        let param_refs: Vec<&dyn duckdb::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let affected = conn.execute(&sql, param_refs.as_slice())?;
        Ok(affected)
    }

    /// Add a note to an incident
    pub fn add_incident_note(
        &self,
        incident_id: &str,
        author: Option<&str>,
        content: &str,
    ) -> Result<i64, StoreError> {
        let conn = self.conn.lock().unwrap();
        let id: i64 = conn.query_row(
            "INSERT INTO incident_notes (incident_id, author, content, created_at) \
             VALUES (?, ?, ?, current_timestamp) RETURNING id",
            duckdb::params![incident_id, author, content],
            |row| row.get(0),
        )?;
        Ok(id)
    }

    /// Get incident notes
    pub fn get_incident_notes(
        &self,
        incident_id: &str,
    ) -> Result<Vec<serde_json::Value>, StoreError> {
        let sql = "SELECT to_json(_row) FROM \
                   (SELECT * FROM incident_notes WHERE incident_id = ? ORDER BY created_at ASC) AS _row";
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map([incident_id], |row| {
            let json_str: String = row.get(0)?;
            Ok(json_str)
        })?;

        let mut results = Vec::new();
        for row in rows {
            let json_str = row?;
            let val: serde_json::Value = serde_json::from_str(&json_str)
                .map_err(|e| StoreError::QueryError(format!("JSON parse error: {e}")))?;
            results.push(val);
        }
        Ok(results)
    }

    /// Add a timeline event to an incident
    pub fn add_incident_timeline_event(
        &self,
        incident_id: &str,
        event_type: &str,
        source: &str,
        description: &str,
        details_json: Option<&str>,
    ) -> Result<i64, StoreError> {
        let conn = self.conn.lock().unwrap();
        let id: i64 = conn.query_row(
            "INSERT INTO incident_timeline_events (incident_id, ts, event_type, source, description, details_json) \
             VALUES (?, current_timestamp, ?, ?, ?, ?) RETURNING id",
            duckdb::params![incident_id, event_type, source, description, details_json],
            |row| row.get(0),
        )?;
        Ok(id)
    }

    // ========================================================================
    // Fleet Commands
    // ========================================================================

    /// Record a fleet command
    pub fn record_fleet_command(
        &self,
        command_id: &str,
        command_type: &str,
        params_json: &str,
        initiated_by: Option<&str>,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO fleet_commands (command_id, command_type, params_json, status, started_at, initiated_by) \
             VALUES (?, ?, ?, 'pending', current_timestamp, ?)",
            duckdb::params![command_id, command_type, params_json, initiated_by],
        )?;
        Ok(())
    }

    /// Update fleet command status
    pub fn update_fleet_command(
        &self,
        command_id: &str,
        status: &str,
        result_json: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<usize, StoreError> {
        let conn = self.conn.lock().unwrap();
        let affected = conn.execute(
            "UPDATE fleet_commands SET status = ?, completed_at = current_timestamp, \
             result_json = ?, error_message = ? WHERE command_id = ?",
            duckdb::params![status, result_json, error_message, command_id],
        )?;
        Ok(affected)
    }

    /// List fleet commands
    pub fn list_fleet_commands(
        &self,
        command_type: Option<&str>,
        limit: usize,
    ) -> Result<Vec<serde_json::Value>, StoreError> {
        let limit = if limit == 0 { 50 } else { limit.min(1000) };
        let (sql, params): (String, Vec<String>) = if let Some(ct) = command_type {
            (
                format!(
                    "SELECT to_json(_row) FROM \
                     (SELECT * FROM fleet_commands WHERE command_type = ? ORDER BY started_at DESC LIMIT {limit}) AS _row"
                ),
                vec![ct.to_string()],
            )
        } else {
            (
                format!(
                    "SELECT to_json(_row) FROM \
                     (SELECT * FROM fleet_commands ORDER BY started_at DESC LIMIT {limit}) AS _row"
                ),
                vec![],
            )
        };

        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&sql)?;
        let param_refs: Vec<&dyn duckdb::ToSql> = params
            .iter()
            .map(|p| p as &dyn duckdb::ToSql)
            .collect();

        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            let json_str: String = row.get(0)?;
            Ok(json_str)
        })?;

        let mut results = Vec::new();
        for row in rows {
            let json_str = row?;
            let val: serde_json::Value = serde_json::from_str(&json_str)
                .map_err(|e| StoreError::QueryError(format!("JSON parse error: {e}")))?;
            results.push(val);
        }
        Ok(results)
    }

    // =========================================================================
    // Solution Mining: mined_sessions table
    // =========================================================================

    /// Mark a session as mined
    pub fn mark_session_mined(
        &self,
        session_id: &str,
        machine_id: &str,
        solutions: i32,
        patterns: i32,
        quality_avg: Option<f64>,
    ) -> Result<(), StoreError> {
        let sql = "INSERT INTO mined_sessions (session_id, machine_id, solutions_extracted, patterns_extracted, quality_avg) \
                   VALUES (?, ?, ?, ?, ?)";
        let conn = self.conn.lock().unwrap();
        conn.execute(
            sql,
            duckdb::params![session_id, machine_id, solutions, patterns, quality_avg],
        )?;
        Ok(())
    }

    /// Check if a session has already been mined
    pub fn is_session_mined(&self, session_id: &str) -> Result<bool, StoreError> {
        let sql = "SELECT COUNT(*) FROM mined_sessions WHERE session_id = ?";
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(sql, [session_id], |row| row.get(0))?;
        Ok(count > 0)
    }

    /// List unmined successful sessions for mining candidates
    pub fn list_unmined_sessions(
        &self,
        limit: usize,
    ) -> Result<Vec<serde_json::Value>, StoreError> {
        let sql = format!(
            "SELECT to_json(_row) FROM \
            (SELECT s.machine_id, s.session_id, s.program, s.model, s.repo_path, s.started_at, s.ended_at, s.token_count \
             FROM agent_sessions s \
             WHERE s.ended_at IS NOT NULL \
               AND NOT EXISTS (SELECT 1 FROM mined_sessions m WHERE m.session_id = s.session_id) \
             ORDER BY s.ended_at DESC \
             LIMIT {limit}) AS _row"
        );
        self.query_json(&sql)
    }

    /// Get mining statistics
    pub fn mining_stats(&self) -> Result<serde_json::Value, StoreError> {
        let sql = "SELECT to_json(_row) FROM \
                   (SELECT COUNT(*) as total_mined, \
                    COALESCE(SUM(solutions_extracted), 0) as total_solutions, \
                    COALESCE(SUM(patterns_extracted), 0) as total_patterns, \
                    COALESCE(AVG(quality_avg), 0) as avg_quality \
                    FROM mined_sessions) AS _row";
        let results = self.query_json(sql)?;
        Ok(results.into_iter().next().unwrap_or(serde_json::json!({})))
    }

    /// Get incident timeline events
    pub fn get_incident_timeline(
        &self,
        incident_id: &str,
    ) -> Result<Vec<serde_json::Value>, StoreError> {
        let sql = "SELECT to_json(_row) FROM \
                   (SELECT * FROM incident_timeline_events WHERE incident_id = ? ORDER BY ts ASC) AS _row";
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map([incident_id], |row| {
            let json_str: String = row.get(0)?;
            Ok(json_str)
        })?;

        let mut results = Vec::new();
        for row in rows {
            let json_str = row?;
            let val: serde_json::Value = serde_json::from_str(&json_str)
                .map_err(|e| StoreError::QueryError(format!("JSON parse error: {e}")))?;
            results.push(val);
        }
        Ok(results)
    }

    // =========================================================================
    // Incident replay / time-travel methods
    // =========================================================================

    /// Build a point-in-time replay snapshot for an incident
    pub fn build_replay_snapshot(
        &self,
        incident_id: &str,
        at_ts: &str,
    ) -> Result<serde_json::Value, StoreError> {
        // Get the incident itself
        let incident = self.get_incident(incident_id)?;
        let incident = incident.ok_or_else(|| {
            StoreError::QueryError(format!("Incident not found: {incident_id}"))
        })?;

        // Machines state at timestamp
        let machines_sql = format!(
            "SELECT * FROM machines WHERE last_seen <= '{at_ts}' ORDER BY hostname"
        );
        let machines = self.query_json(&machines_sql).unwrap_or_default();

        // Alerts active around the timestamp
        let alerts_sql = format!(
            "SELECT * FROM alerts WHERE fired_at <= '{at_ts}' \
             ORDER BY fired_at DESC LIMIT 50"
        );
        let alerts = self.query_json(&alerts_sql).unwrap_or_default();

        // Audit events around the timestamp (context window: 1 hour before to 1 hour after)
        let audit_sql = format!(
            "SELECT * FROM audit_events \
             WHERE timestamp BETWEEN (TIMESTAMP '{at_ts}' - INTERVAL 1 HOUR) \
             AND (TIMESTAMP '{at_ts}' + INTERVAL 1 HOUR) \
             ORDER BY timestamp ASC LIMIT 100"
        );
        let audit_events = self.query_json(&audit_sql).unwrap_or_default();

        // Collector health at timestamp
        let collector_sql = format!(
            "SELECT * FROM collector_health \
             WHERE checked_at <= '{at_ts}' \
             ORDER BY checked_at DESC LIMIT 20"
        );
        let collectors = self.query_json(&collector_sql).unwrap_or_default();

        // Timeline events for this incident up to timestamp
        let timeline_sql = "SELECT to_json(_row) FROM \
            (SELECT * FROM incident_timeline_events \
             WHERE incident_id = ? AND ts <= ? \
             ORDER BY ts ASC) AS _row";
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(timeline_sql)?;
        let rows = stmt.query_map([incident_id, at_ts], |row| {
            let json_str: String = row.get(0)?;
            Ok(json_str)
        })?;
        let mut timeline = Vec::new();
        for row in rows {
            let json_str = row?;
            let val: serde_json::Value = serde_json::from_str(&json_str)
                .map_err(|e| StoreError::QueryError(format!("JSON parse: {e}")))?;
            timeline.push(val);
        }
        drop(stmt);
        drop(conn);

        // Health scores at timestamp
        let health_sql = format!(
            "SELECT * FROM health_scores WHERE computed_at <= '{at_ts}' \
             ORDER BY computed_at DESC LIMIT 20"
        );
        let health_scores = self.query_json(&health_sql).unwrap_or_default();

        Ok(serde_json::json!({
            "incident": incident,
            "snapshot_at": at_ts,
            "machines": machines,
            "alerts": alerts,
            "audit_events": audit_events,
            "collectors": collectors,
            "timeline": timeline,
            "health_scores": health_scores,
        }))
    }

    /// Cache a replay snapshot for fast retrieval
    pub fn cache_replay_snapshot(
        &self,
        incident_id: &str,
        snapshot_ts: &str,
        snapshot_json: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let next_id: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(id), 0) + 1 FROM incident_replay_snapshots",
                [],
                |row| row.get(0),
            )
            .unwrap_or(1);

        conn.execute(
            "INSERT INTO incident_replay_snapshots (id, incident_id, snapshot_ts, snapshot_json) \
             VALUES (?, ?, ?, ?)",
            duckdb::params![next_id, incident_id, snapshot_ts, snapshot_json],
        )?;
        Ok(())
    }

    /// Get a cached replay snapshot
    pub fn get_cached_replay(
        &self,
        incident_id: &str,
        snapshot_ts: &str,
    ) -> Result<Option<serde_json::Value>, StoreError> {
        let sql = "SELECT snapshot_json FROM incident_replay_snapshots \
                   WHERE incident_id = ? AND snapshot_ts = ? \
                   ORDER BY created_at DESC LIMIT 1";
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(sql, [incident_id, snapshot_ts], |row| {
            let json_str: String = row.get(0)?;
            Ok(json_str)
        });

        match result {
            Ok(json_str) => {
                let val: serde_json::Value = serde_json::from_str(&json_str)
                    .map_err(|e| StoreError::QueryError(format!("JSON parse: {e}")))?;
                Ok(Some(val))
            }
            Err(duckdb::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// List cached replay timestamps for an incident
    pub fn list_replay_snapshots(
        &self,
        incident_id: &str,
    ) -> Result<Vec<serde_json::Value>, StoreError> {
        self.query_json(&format!(
            "SELECT id, incident_id, snapshot_ts, created_at \
             FROM incident_replay_snapshots \
             WHERE incident_id = '{incident_id}' \
             ORDER BY snapshot_ts ASC"
        ))
    }

    /// Get replay with caching: returns cached snapshot if available, otherwise builds and caches
    pub fn get_or_build_replay(
        &self,
        incident_id: &str,
        at_ts: &str,
    ) -> Result<serde_json::Value, StoreError> {
        // Check cache first
        if let Some(cached) = self.get_cached_replay(incident_id, at_ts)? {
            return Ok(cached);
        }

        // Build fresh snapshot
        let snapshot = self.build_replay_snapshot(incident_id, at_ts)?;

        // Cache it
        let snapshot_str = serde_json::to_string(&snapshot)
            .map_err(|e| StoreError::QueryError(format!("JSON serialize: {e}")))?;
        self.cache_replay_snapshot(incident_id, at_ts, &snapshot_str)?;

        Ok(snapshot)
    }

    /// Export incident replay as structured JSON (all snapshots + metadata)
    pub fn export_incident_replay(
        &self,
        incident_id: &str,
    ) -> Result<serde_json::Value, StoreError> {
        let incident = self.get_incident(incident_id)?;
        let incident = incident.ok_or_else(|| {
            StoreError::QueryError(format!("Incident not found: {incident_id}"))
        })?;

        let timeline = self.get_incident_timeline(incident_id)?;
        let notes = self.get_incident_notes(incident_id)?;
        let cached_snapshots = self.list_replay_snapshots(incident_id)?;

        Ok(serde_json::json!({
            "export_version": "1.0",
            "incident": incident,
            "timeline": timeline,
            "notes": notes,
            "snapshots": cached_snapshots,
        }))
    }

    // =========================================================================
    // Adaptive polling methods
    // =========================================================================

    /// Record a poll schedule decision
    pub fn insert_poll_decision(
        &self,
        machine_id: &str,
        collector: &str,
        next_interval_seconds: i32,
        reason_json: Option<&str>,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let next_id: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(id), 0) + 1 FROM poll_schedule_decisions",
                [],
                |row| row.get(0),
            )
            .unwrap_or(1);

        conn.execute(
            "INSERT INTO poll_schedule_decisions (id, machine_id, collector, next_interval_seconds, reason_json) \
             VALUES (?, ?, ?, ?, ?)",
            duckdb::params![next_id, machine_id, collector, next_interval_seconds, reason_json],
        )?;
        Ok(())
    }

    /// Get the latest poll interval for a machine/collector
    pub fn get_latest_poll_interval(
        &self,
        machine_id: &str,
        collector: &str,
    ) -> Result<Option<i32>, StoreError> {
        let sql = "SELECT next_interval_seconds FROM poll_schedule_decisions \
                   WHERE machine_id = ? AND collector = ? \
                   ORDER BY decided_at DESC LIMIT 1";
        let conn = self.conn.lock().unwrap();
        match conn.query_row(sql, [machine_id, collector], |row| row.get::<_, i32>(0)) {
            Ok(val) => Ok(Some(val)),
            Err(duckdb::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// List recent poll decisions
    pub fn list_poll_decisions(
        &self,
        machine_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<serde_json::Value>, StoreError> {
        let sql = if let Some(mid) = machine_id {
            format!(
                "SELECT * FROM poll_schedule_decisions \
                 WHERE machine_id = '{mid}' \
                 ORDER BY decided_at DESC LIMIT {limit}"
            )
        } else {
            format!(
                "SELECT * FROM poll_schedule_decisions \
                 ORDER BY decided_at DESC LIMIT {limit}"
            )
        };
        self.query_json(&sql)
    }

    /// Insert a profiling sample
    pub fn insert_profile_sample(
        &self,
        machine_id: &str,
        profile_id: &str,
        metrics_json: Option<&str>,
        raw_json: Option<&str>,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let next_id: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(id), 0) + 1 FROM sys_profile_samples",
                [],
                |row| row.get(0),
            )
            .unwrap_or(1);

        conn.execute(
            "INSERT INTO sys_profile_samples (id, machine_id, profile_id, metrics_json, raw_json) \
             VALUES (?, ?, ?, ?, ?)",
            duckdb::params![next_id, machine_id, profile_id, metrics_json, raw_json],
        )?;
        Ok(())
    }

    /// List profiling samples, optionally filtered by machine or profile
    pub fn list_profile_samples(
        &self,
        machine_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<serde_json::Value>, StoreError> {
        let sql = if let Some(mid) = machine_id {
            format!(
                "SELECT * FROM sys_profile_samples \
                 WHERE machine_id = '{mid}' \
                 ORDER BY collected_at DESC LIMIT {limit}"
            )
        } else {
            format!(
                "SELECT * FROM sys_profile_samples \
                 ORDER BY collected_at DESC LIMIT {limit}"
            )
        };
        self.query_json(&sql)
    }

    // =========================================================================
    // Digest report methods
    // =========================================================================

    /// Store a generated digest report
    pub fn insert_digest_report(
        &self,
        report_id: &str,
        window_hours: i32,
        summary_json: &str,
        markdown: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let next_id: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(id), 0) + 1 FROM digest_reports",
                [],
                |row| row.get(0),
            )
            .unwrap_or(1);
        conn.execute(
            "INSERT INTO digest_reports (id, report_id, window_hours, summary_json, markdown) \
             VALUES (?, ?, ?, ?, ?)",
            duckdb::params![next_id, report_id, window_hours, summary_json, markdown],
        )?;
        Ok(())
    }

    /// Get a digest report by ID
    pub fn get_digest_report(&self, report_id: &str) -> Result<Option<serde_json::Value>, StoreError> {
        let results = self.query_json(&format!(
            "SELECT * FROM digest_reports WHERE report_id = '{report_id}' LIMIT 1"
        ))?;
        Ok(results.into_iter().next())
    }

    /// List recent digest reports
    pub fn list_digest_reports(&self, limit: usize) -> Result<Vec<serde_json::Value>, StoreError> {
        self.query_json(&format!(
            "SELECT id, report_id, window_hours, generated_at \
             FROM digest_reports ORDER BY generated_at DESC LIMIT {limit}"
        ))
    }

    // =========================================================================
    // Redaction audit methods
    // =========================================================================

    /// Record a redaction event
    pub fn insert_redaction_event(
        &self,
        machine_id: &str,
        collector: &str,
        redacted_fields: i32,
        redacted_bytes: i64,
        rules_version: &str,
        sample_hash: Option<&str>,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let next_id: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(id), 0) + 1 FROM redaction_events",
                [],
                |row| row.get(0),
            )
            .unwrap_or(1);
        conn.execute(
            "INSERT INTO redaction_events (id, machine_id, collector, redacted_fields, redacted_bytes, rules_version, sample_hash) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            duckdb::params![next_id, machine_id, collector, redacted_fields, redacted_bytes, rules_version, sample_hash.unwrap_or("")],
        )?;
        Ok(())
    }

    /// List recent redaction events
    pub fn list_redaction_events(
        &self,
        machine_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<serde_json::Value>, StoreError> {
        let sql = if let Some(mid) = machine_id {
            format!(
                "SELECT * FROM redaction_events \
                 WHERE machine_id = '{mid}' \
                 ORDER BY collected_at DESC LIMIT {limit}"
            )
        } else {
            format!(
                "SELECT * FROM redaction_events \
                 ORDER BY collected_at DESC LIMIT {limit}"
            )
        };
        self.query_json(&sql)
    }

    /// Get redaction summary (total redacted fields/bytes per collector)
    pub fn redaction_summary(&self) -> Result<Vec<serde_json::Value>, StoreError> {
        self.query_json(
            "SELECT collector, \
                    COUNT(*) as event_count, \
                    SUM(redacted_fields) as total_fields, \
                    SUM(redacted_bytes) as total_bytes, \
                    MAX(rules_version) as latest_rules_version \
             FROM redaction_events \
             GROUP BY collector \
             ORDER BY total_fields DESC"
        )
    }

    // =========================================================================
    // Node ingest / deduplication methods
    // =========================================================================

    /// Check if a content hash has already been ingested
    pub fn has_ingest_record(&self, content_hash: &str) -> Result<bool, StoreError> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM node_ingest_log WHERE content_hash = ?",
                [content_hash],
                |row| row.get(0),
            )
            .unwrap_or(0);
        Ok(count > 0)
    }

    /// Record a successful ingest for future dedup
    pub fn record_ingest(
        &self,
        bundle_id: &str,
        machine_id: &str,
        collector: &str,
        content_hash: &str,
        row_count: usize,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let next_id: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(id), 0) + 1 FROM node_ingest_log",
                [],
                |row| row.get(0),
            )
            .unwrap_or(1);
        conn.execute(
            "INSERT INTO node_ingest_log (id, bundle_id, machine_id, collector, content_hash, row_count) \
             VALUES (?, ?, ?, ?, ?, ?)",
            duckdb::params![next_id, bundle_id, machine_id, collector, content_hash, row_count as i64],
        )?;
        Ok(())
    }

    /// List recent ingest records
    pub fn list_ingest_records(
        &self,
        machine_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<serde_json::Value>, StoreError> {
        let sql = if let Some(mid) = machine_id {
            format!(
                "SELECT * FROM node_ingest_log \
                 WHERE machine_id = '{mid}' \
                 ORDER BY ingested_at DESC LIMIT {limit}"
            )
        } else {
            format!(
                "SELECT * FROM node_ingest_log \
                 ORDER BY ingested_at DESC LIMIT {limit}"
            )
        };
        self.query_json(&sql)
    }

    // =========================================================================
    // Data export/backup methods
    // =========================================================================

    /// List all user tables in the database (excludes internal tables)
    pub fn list_tables(&self) -> Result<Vec<String>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT table_name FROM duckdb_tables() \
             WHERE schema_name = 'main' \
             AND table_name NOT LIKE '\\_%' ESCAPE '\\' \
             ORDER BY table_name"
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut tables = Vec::new();
        for row in rows {
            tables.push(row?);
        }
        Ok(tables)
    }

    /// Export a single table as JSONL (one JSON object per line)
    pub fn export_table_jsonl(
        &self,
        table: &str,
        since: Option<&str>,
        until: Option<&str>,
    ) -> Result<Vec<String>, StoreError> {
        // Build query with optional time filtering
        let ts_column = self.guess_timestamp_column(table);

        let mut sql = format!("SELECT * FROM \"{table}\"");
        let mut conditions = Vec::new();

        if let (Some(col), Some(since)) = (&ts_column, since) {
            conditions.push(format!("{col} >= '{since}'"));
        }
        if let (Some(col), Some(until)) = (&ts_column, until) {
            conditions.push(format!("{col} <= '{until}'"));
        }
        if !conditions.is_empty() {
            sql.push_str(&format!(" WHERE {}", conditions.join(" AND ")));
        }

        let rows = self.query_json(&sql)?;
        let lines: Vec<String> = rows
            .iter()
            .map(|r| serde_json::to_string(r).unwrap_or_default())
            .collect();
        Ok(lines)
    }

    /// Guess the timestamp column for a table (for time-window filtering)
    fn guess_timestamp_column(&self, table: &str) -> Option<String> {
        // Common timestamp column names in order of preference
        let candidates = [
            "created_at",
            "timestamp",
            "fired_at",
            "checked_at",
            "computed_at",
            "started_at",
            "routed_at",
            "captured_at",
            "applied_at",
            "snapshot_ts",
            "last_seen",
            "ts",
        ];

        let conn = self.conn.lock().unwrap();
        for col in &candidates {
            let sql = format!(
                "SELECT column_name FROM duckdb_columns() \
                 WHERE table_name = '{table}' AND column_name = '{col}' LIMIT 1"
            );
            if let Ok(name) = conn.query_row(&sql, [], |row| row.get::<_, String>(0)) {
                return Some(name);
            }
        }
        None
    }

    /// Get row count for a table
    pub fn table_row_count(&self, table: &str) -> Result<i64, StoreError> {
        self.query_scalar(&format!("SELECT COUNT(*) FROM \"{table}\""))
    }

    /// Build an export manifest (metadata about the export)
    pub fn build_export_manifest(
        &self,
        tables: &[String],
        since: Option<&str>,
        until: Option<&str>,
    ) -> Result<serde_json::Value, StoreError> {
        let mut table_info = Vec::new();
        for table in tables {
            let count = self.table_row_count(table).unwrap_or(0);
            table_info.push(serde_json::json!({
                "table": table,
                "row_count": count,
            }));
        }

        Ok(serde_json::json!({
            "export_version": "1.0",
            "schema_version": 20,
            "exported_at": chrono::Utc::now().to_rfc3339(),
            "tables": table_info,
            "filter": {
                "since": since,
                "until": until,
            },
        }))
    }

    /// Import JSONL data into a table (append mode)
    pub fn import_table_jsonl(
        &self,
        table: &str,
        lines: &[String],
    ) -> Result<usize, StoreError> {
        let mut imported = 0;
        for line in lines {
            if line.trim().is_empty() {
                continue;
            }
            let value: serde_json::Value = serde_json::from_str(line)
                .map_err(|e| StoreError::QueryError(format!("JSON parse error: {e}")))?;

            match self.insert_json(table, &value) {
                Ok(_) => imported += 1,
                Err(e) => {
                    tracing::warn!(table, error = %e, "Skipping row during import");
                }
            }
        }
        Ok(imported)
    }

    // =========================================================================
    // Alert routing event methods
    // =========================================================================

    /// Record an alert routing decision
    pub fn insert_routing_event(
        &self,
        alert_id: &str,
        rule_id: Option<&str>,
        channel: &str,
        action: &str,
        reason_json: Option<&str>,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let next_id: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(id), 0) + 1 FROM alert_routing_events",
                [],
                |row| row.get(0),
            )
            .unwrap_or(1);

        conn.execute(
            "INSERT INTO alert_routing_events (id, alert_id, rule_id, channel, action, reason_json) \
             VALUES (?, ?, ?, ?, ?, ?)",
            duckdb::params![next_id, alert_id, rule_id, channel, action, reason_json],
        )?;
        Ok(())
    }

    /// List routing events for an alert
    pub fn list_routing_events(
        &self,
        alert_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<serde_json::Value>, StoreError> {
        let sql = if let Some(aid) = alert_id {
            format!(
                "SELECT * FROM alert_routing_events \
                 WHERE alert_id = '{aid}' \
                 ORDER BY routed_at DESC LIMIT {limit}"
            )
        } else {
            format!(
                "SELECT * FROM alert_routing_events \
                 ORDER BY routed_at DESC LIMIT {limit}"
            )
        };
        self.query_json(&sql)
    }

    /// Count routing events by action type
    pub fn routing_event_summary(
        &self,
    ) -> Result<Vec<serde_json::Value>, StoreError> {
        self.query_json(
            "SELECT action, COUNT(*) AS count FROM alert_routing_events \
             GROUP BY action ORDER BY count DESC"
        )
    }

    // =========================================================================
    // Resolution capture methods (playbook auto-generation)
    // =========================================================================

    pub fn insert_resolution(
        &self,
        alert_type: &str,
        actions_json: &str,
        outcome: &str,
        alert_id: Option<i64>,
        machine_id: Option<&str>,
        operator: Option<&str>,
    ) -> Result<i64, StoreError> {
        let conn = self.conn.lock().unwrap();
        // DuckDB doesn't auto-increment INTEGER PRIMARY KEY
        let next_id: i64 = conn.query_row(
            "SELECT COALESCE(MAX(id), 0) + 1 FROM resolutions",
            [],
            |row| row.get(0),
        )?;
        conn.execute(
            "INSERT INTO resolutions (id, alert_type, actions, outcome, alert_id, machine_id, operator) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            duckdb::params![
                next_id,
                alert_type,
                actions_json,
                outcome,
                alert_id,
                machine_id,
                operator,
            ],
        )?;
        Ok(next_id)
    }

    pub fn list_resolutions(
        &self,
        alert_type: Option<&str>,
        outcome: Option<&str>,
        limit: usize,
    ) -> Result<Vec<serde_json::Value>, StoreError> {
        let limit = limit.min(1000);
        let sql = match (alert_type, outcome) {
            (Some(at), Some(oc)) => format!(
                "SELECT * FROM resolutions WHERE alert_type = '{}' AND outcome = '{}' \
                 ORDER BY captured_at DESC LIMIT {}",
                escape_sql_literal(at),
                escape_sql_literal(oc),
                limit
            ),
            (Some(at), None) => format!(
                "SELECT * FROM resolutions WHERE alert_type = '{}' \
                 ORDER BY captured_at DESC LIMIT {}",
                escape_sql_literal(at),
                limit
            ),
            (None, Some(oc)) => format!(
                "SELECT * FROM resolutions WHERE outcome = '{}' \
                 ORDER BY captured_at DESC LIMIT {}",
                escape_sql_literal(oc),
                limit
            ),
            (None, None) => format!(
                "SELECT * FROM resolutions ORDER BY captured_at DESC LIMIT {}",
                limit
            ),
        };
        self.query_json(&sql)
    }

    pub fn count_resolutions_by_type(
        &self,
        alert_type: &str,
        outcome: &str,
    ) -> Result<i64, StoreError> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM resolutions WHERE alert_type = ? AND outcome = ?",
            [alert_type, outcome],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub fn distinct_resolution_alert_types(&self) -> Result<Vec<String>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT DISTINCT alert_type FROM resolutions WHERE outcome = 'success' ORDER BY alert_type",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut types = Vec::new();
        for row in rows {
            types.push(row?);
        }
        Ok(types)
    }

    // =========================================================================
    // Playbook draft methods
    // =========================================================================

    pub fn insert_playbook_draft(
        &self,
        draft_id: &str,
        name: &str,
        description: &str,
        alert_type: &str,
        trigger_json: &str,
        steps_json: &str,
        confidence: f64,
        sample_count: i32,
        source_pattern_json: Option<&str>,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO playbook_drafts \
             (draft_id, name, description, alert_type, trigger_json, steps_json, \
              confidence, sample_count, source_pattern_json) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            duckdb::params![
                draft_id,
                name,
                description,
                alert_type,
                trigger_json,
                steps_json,
                confidence,
                sample_count,
                source_pattern_json,
            ],
        )?;
        Ok(())
    }

    pub fn list_playbook_drafts(
        &self,
        status: Option<&str>,
        limit: usize,
    ) -> Result<Vec<serde_json::Value>, StoreError> {
        let limit = limit.min(1000);
        let sql = match status {
            Some(s) => format!(
                "SELECT * FROM playbook_drafts WHERE status = '{}' \
                 ORDER BY created_at DESC LIMIT {}",
                escape_sql_literal(s),
                limit
            ),
            None => format!(
                "SELECT * FROM playbook_drafts ORDER BY created_at DESC LIMIT {}",
                limit
            ),
        };
        self.query_json(&sql)
    }

    pub fn get_playbook_draft(
        &self,
        draft_id: &str,
    ) -> Result<Option<serde_json::Value>, StoreError> {
        let sql = "SELECT to_json(_row) FROM \
                   (SELECT * FROM playbook_drafts WHERE draft_id = ?) AS _row";
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(sql, [draft_id], |row| {
            let json_str: String = row.get(0)?;
            Ok(json_str)
        });

        match result {
            Ok(json_str) => {
                let val: serde_json::Value = serde_json::from_str(&json_str)?;
                Ok(Some(val))
            }
            Err(duckdb::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StoreError::DatabaseError(e)),
        }
    }

    pub fn approve_playbook_draft(
        &self,
        draft_id: &str,
        approver: &str,
    ) -> Result<usize, StoreError> {
        let conn = self.conn.lock().unwrap();
        let affected = conn.execute(
            "UPDATE playbook_drafts SET status = 'approved', approved_by = ?, \
             approved_at = current_timestamp WHERE draft_id = ? AND status = 'pending_review'",
            [approver, draft_id],
        )?;
        Ok(affected)
    }

    pub fn reject_playbook_draft(
        &self,
        draft_id: &str,
        reason: Option<&str>,
    ) -> Result<usize, StoreError> {
        let conn = self.conn.lock().unwrap();
        let affected = conn.execute(
            "UPDATE playbook_drafts SET status = 'rejected' WHERE draft_id = ? AND status = 'pending_review'",
            [draft_id],
        )?;
        // Record rejection reason as description update if provided
        if affected > 0 {
            if let Some(reason) = reason {
                let desc = format!("[Rejected] {reason}");
                conn.execute(
                    "UPDATE playbook_drafts SET description = ? WHERE draft_id = ?",
                    [&desc, draft_id],
                )?;
            }
        }
        Ok(affected)
    }

    pub fn activate_playbook_from_draft(
        &self,
        draft_id: &str,
    ) -> Result<Option<serde_json::Value>, StoreError> {
        let draft = self.get_playbook_draft(draft_id)?;
        let draft = match draft {
            Some(d) => d,
            None => return Ok(None),
        };

        let status = draft["status"].as_str().unwrap_or("");
        if status != "approved" {
            return Err(StoreError::QueryError(
                "Draft must be approved before activation".to_string(),
            ));
        }

        // Insert into guardian_playbooks
        let playbook_id = draft["draft_id"].as_str().unwrap_or(draft_id);
        let name = draft["name"].as_str().unwrap_or("");
        let description = draft["description"].as_str().unwrap_or("");
        let trigger_json = draft["trigger_json"].as_str().unwrap_or("{}");
        let steps_json = draft["steps_json"].as_str().unwrap_or("[]");

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO guardian_playbooks \
             (playbook_id, name, description, trigger_condition, steps, \
              enabled, requires_approval, max_runs_per_hour) \
             VALUES (?, ?, ?, ?, ?, TRUE, TRUE, 3)",
            duckdb::params![playbook_id, name, description, trigger_json, steps_json],
        )?;

        // Mark draft as activated
        conn.execute(
            "UPDATE playbook_drafts SET status = 'activated' WHERE draft_id = ?",
            [draft_id],
        )?;

        Ok(Some(serde_json::json!({
            "playbook_id": playbook_id,
            "name": name,
            "status": "activated",
        })))
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

pub fn escape_sql_literal(value: &str) -> String {
    value.replace('\'', "''")
}

fn clamp_audit_limit(limit: usize) -> usize {
    let limit = if limit == 0 { 100 } else { limit };
    limit.min(10_000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDuration;

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
            .execute_simple("CREATE TABLE test (id INTEGER, name TEXT)")
            .unwrap();
        store
            .execute_simple("INSERT INTO test VALUES (1, 'hello')")
            .unwrap();

        let results = store.query_json("SELECT * FROM test").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_execute_returns_affected_rows() {
        let store = VcStore::open_memory().unwrap();
        store
            .execute_simple("CREATE TABLE test_affected (id INTEGER, name TEXT)")
            .unwrap();

        // Insert should affect 1 row
        let affected = store
            .execute_simple("INSERT INTO test_affected VALUES (1, 'a')")
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
        store
            .execute_batch(
                r#"
            CREATE TABLE batch_test (id INTEGER, value TEXT);
            INSERT INTO batch_test VALUES (1, 'first');
            INSERT INTO batch_test VALUES (2, 'second');
        "#,
            )
            .unwrap();

        let results = store
            .query_json("SELECT * FROM batch_test ORDER BY id")
            .unwrap();
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
        let cursor = store
            .get_cursor("machine1", "collector_a", "last_ts")
            .unwrap();
        assert!(cursor.is_none());

        // Set cursor
        store
            .set_cursor("machine1", "collector_a", "last_ts", "2026-01-27T12:00:00Z")
            .unwrap();

        // Get cursor
        let cursor = store
            .get_cursor("machine1", "collector_a", "last_ts")
            .unwrap();
        assert_eq!(cursor, Some("2026-01-27T12:00:00Z".to_string()));

        // Update cursor
        store
            .set_cursor("machine1", "collector_a", "last_ts", "2026-01-27T13:00:00Z")
            .unwrap();
        let cursor = store
            .get_cursor("machine1", "collector_a", "last_ts")
            .unwrap();
        assert_eq!(cursor, Some("2026-01-27T13:00:00Z".to_string()));

        // Different source
        let other = store
            .get_cursor("machine1", "collector_b", "last_ts")
            .unwrap();
        assert!(other.is_none());
    }

    #[test]
    fn test_cursor_different_machines() {
        let store = VcStore::open_memory().unwrap();

        store
            .set_cursor("machine1", "src", "key", "value1")
            .unwrap();
        store
            .set_cursor("machine2", "src", "key", "value2")
            .unwrap();

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
            .execute_simple("CREATE TABLE test_json_insert (id INTEGER, name TEXT, active BOOLEAN)")
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
            .execute_simple("CREATE TABLE test_insert (id INTEGER, name TEXT)")
            .unwrap();

        let result = store.insert_json("test_insert", &serde_json::json!(["not", "object"]));
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("insert_json requires a JSON object")
        );
    }

    #[test]
    fn test_insert_json_string_error() {
        let store = VcStore::open_memory().unwrap();
        store
            .execute_simple("CREATE TABLE test_str (id INTEGER)")
            .unwrap();

        let result = store.insert_json("test_str", &serde_json::json!("just a string"));
        assert!(result.is_err());
    }

    #[test]
    fn test_insert_json_null_error() {
        let store = VcStore::open_memory().unwrap();
        store
            .execute_simple("CREATE TABLE test_null (id INTEGER)")
            .unwrap();

        let result = store.insert_json("test_null", &serde_json::Value::Null);
        assert!(result.is_err());
    }

    #[test]
    fn test_insert_json_with_null_value() {
        let store = VcStore::open_memory().unwrap();
        store
            .execute_simple("CREATE TABLE test_nullable (id INTEGER, optional_field TEXT)")
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
            .execute_simple("CREATE TABLE test_nested (id INTEGER, metadata TEXT)")
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

    // =============================================================================
    // Audit Event Tests
    // =============================================================================

    #[test]
    fn test_insert_and_list_audit_event() {
        let store = VcStore::open_memory().unwrap();

        let event = AuditEvent::new(
            AuditEventType::CollectorRun,
            "sysmoni",
            "collect",
            AuditResult::Success,
            serde_json::json!({"rows": 10}),
        )
        .with_machine_id("local");

        store.insert_audit_event(&event).unwrap();

        let filter = AuditEventFilter {
            event_type: None,
            machine_id: None,
            since: None,
            limit: 10,
        };

        let rows = store.list_audit_events(&filter).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["event_type"], "collector_run");
        assert_eq!(rows[0]["result"], "success");
    }

    #[test]
    fn test_audit_event_filters() {
        let store = VcStore::open_memory().unwrap();

        let event_a = AuditEvent::new(
            AuditEventType::CollectorRun,
            "sysmoni",
            "collect",
            AuditResult::Success,
            serde_json::json!({"rows": 5}),
        )
        .with_machine_id("alpha");
        let event_b = AuditEvent::new(
            AuditEventType::UserCommand,
            "user",
            "vc status",
            AuditResult::Success,
            serde_json::json!({"args": ["status"]}),
        )
        .with_machine_id("beta");

        store.insert_audit_event(&event_a).unwrap();
        store.insert_audit_event(&event_b).unwrap();

        let filter = AuditEventFilter {
            event_type: Some(AuditEventType::UserCommand),
            machine_id: None,
            since: None,
            limit: 10,
        };
        let rows = store.list_audit_events(&filter).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["event_type"], "user_command");

        let since = Utc::now() - ChronoDuration::minutes(1);
        let filter = AuditEventFilter {
            event_type: None,
            machine_id: Some("alpha".to_string()),
            since: Some(since),
            limit: 10,
        };
        let rows = store.list_audit_events(&filter).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["machine_id"], "alpha");
    }

    #[test]
    fn test_audit_event_all_types() {
        let store = VcStore::open_memory().unwrap();

        // Insert one event of each type
        let types = [
            (AuditEventType::CollectorRun, "collector_run"),
            (AuditEventType::AutopilotAction, "autopilot_action"),
            (AuditEventType::UserCommand, "user_command"),
            (AuditEventType::GuardianAction, "guardian_action"),
        ];

        for (event_type, _expected_str) in &types {
            let event = AuditEvent::new(
                *event_type,
                "test_actor",
                "test_action",
                AuditResult::Success,
                serde_json::json!({}),
            );
            store.insert_audit_event(&event).unwrap();
        }

        // Verify all 4 events were inserted
        let filter = AuditEventFilter {
            limit: 100,
            ..Default::default()
        };
        let rows = store.list_audit_events(&filter).unwrap();
        assert_eq!(rows.len(), 4);

        // Verify each type can be filtered individually
        for (event_type, expected_str) in &types {
            let filter = AuditEventFilter {
                event_type: Some(*event_type),
                limit: 100,
                ..Default::default()
            };
            let rows = store.list_audit_events(&filter).unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0]["event_type"], *expected_str);
        }
    }

    #[test]
    fn test_audit_event_all_results() {
        let store = VcStore::open_memory().unwrap();

        let results = [
            (AuditResult::Success, "success"),
            (AuditResult::Failure, "failure"),
            (AuditResult::Skipped, "skipped"),
        ];

        for (result, _expected) in &results {
            let event = AuditEvent::new(
                AuditEventType::CollectorRun,
                "test",
                "test",
                *result,
                serde_json::json!({}),
            );
            store.insert_audit_event(&event).unwrap();
        }

        let filter = AuditEventFilter {
            limit: 100,
            ..Default::default()
        };
        let rows = store.list_audit_events(&filter).unwrap();
        assert_eq!(rows.len(), 3);
    }

    #[test]
    fn test_audit_event_get_by_id() {
        let store = VcStore::open_memory().unwrap();

        let event = AuditEvent::new(
            AuditEventType::GuardianAction,
            "guardian",
            "restart_service",
            AuditResult::Success,
            serde_json::json!({"service": "nginx", "playbook": "web-recovery"}),
        )
        .with_machine_id("web-01");

        store.insert_audit_event(&event).unwrap();

        // Get the event by ID (first event = ID 1)
        let fetched = store.get_audit_event(1).unwrap();
        assert!(fetched.is_some());
        let fetched = fetched.unwrap();
        assert_eq!(fetched["event_type"], "guardian_action");
        assert_eq!(fetched["actor"], "guardian");
        assert_eq!(fetched["machine_id"], "web-01");

        // Non-existent ID
        let missing = store.get_audit_event(999).unwrap();
        assert!(missing.is_none());
    }

    #[test]
    fn test_audit_event_limit() {
        let store = VcStore::open_memory().unwrap();

        // Insert 5 events
        for i in 0..5 {
            let event = AuditEvent::new(
                AuditEventType::CollectorRun,
                format!("collector_{i}"),
                "collect",
                AuditResult::Success,
                serde_json::json!({"iteration": i}),
            );
            store.insert_audit_event(&event).unwrap();
        }

        // Limit to 3
        let filter = AuditEventFilter {
            limit: 3,
            ..Default::default()
        };
        let rows = store.list_audit_events(&filter).unwrap();
        assert_eq!(rows.len(), 3);

        // Limit of 0 defaults to 100
        let filter = AuditEventFilter {
            limit: 0,
            ..Default::default()
        };
        let rows = store.list_audit_events(&filter).unwrap();
        assert_eq!(rows.len(), 5);
    }

    #[test]
    fn test_audit_event_type_roundtrip() {
        // Test FromStr and as_str for all event types
        let types = [
            "collector_run",
            "autopilot_action",
            "user_command",
            "guardian_action",
        ];
        for type_str in types {
            let parsed: AuditEventType = type_str.parse().unwrap();
            assert_eq!(parsed.as_str(), type_str);
        }

        // Invalid type
        let err = "invalid_type".parse::<AuditEventType>();
        assert!(err.is_err());
    }

    #[test]
    fn test_audit_result_roundtrip() {
        let results = ["success", "failure", "skipped"];
        for result_str in results {
            let parsed: AuditResult = result_str.parse().unwrap();
            assert_eq!(parsed.as_str(), result_str);
        }

        let err = "invalid".parse::<AuditResult>();
        assert!(err.is_err());
    }

    #[test]
    fn test_auditable_trait() {
        // Test that the Auditable trait works with a custom type
        struct TestCollectorRun {
            collector_name: String,
            machine_id: String,
            rows_inserted: u64,
            success: bool,
        }

        impl Auditable for TestCollectorRun {
            fn to_audit_event(&self) -> AuditEvent {
                AuditEvent::new(
                    AuditEventType::CollectorRun,
                    &self.collector_name,
                    format!("collect {} rows", self.rows_inserted),
                    if self.success {
                        AuditResult::Success
                    } else {
                        AuditResult::Failure
                    },
                    serde_json::json!({"rows_inserted": self.rows_inserted}),
                )
                .with_machine_id(&self.machine_id)
            }
        }

        let run = TestCollectorRun {
            collector_name: "sysmoni".to_string(),
            machine_id: "orko".to_string(),
            rows_inserted: 42,
            success: true,
        };

        let store = VcStore::open_memory().unwrap();
        let event = run.to_audit_event();
        store.insert_audit_event(&event).unwrap();

        let filter = AuditEventFilter {
            limit: 10,
            ..Default::default()
        };
        let rows = store.list_audit_events(&filter).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["actor"], "sysmoni");
        assert_eq!(rows[0]["machine_id"], "orko");
        assert_eq!(rows[0]["event_type"], "collector_run");
    }

    #[test]
    fn test_insert_json_with_array_value() {
        let store = VcStore::open_memory().unwrap();
        store
            .execute_simple("CREATE TABLE test_array (id INTEGER, tags TEXT)")
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
            .execute_simple("CREATE TABLE test_batch (id INTEGER, name TEXT)")
            .unwrap();

        let count = store.insert_json_batch("test_batch", &[]).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_insert_json_batch_multiple() {
        let store = VcStore::open_memory().unwrap();
        store
            .execute_simple("CREATE TABLE test_batch_multi (id INTEGER, name TEXT)")
            .unwrap();

        let rows = vec![
            serde_json::json!({"id": 1, "name": "first"}),
            serde_json::json!({"id": 2, "name": "second"}),
            serde_json::json!({"id": 3, "name": "third"}),
        ];
        let count = store.insert_json_batch("test_batch_multi", &rows).unwrap();
        assert_eq!(count, 3);

        let results = store
            .query_json("SELECT * FROM test_batch_multi ORDER BY id")
            .unwrap();
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
            .execute_simple("CREATE TABLE test_scalar (id INTEGER, value INTEGER)")
            .unwrap();
        store
            .execute_simple("INSERT INTO test_scalar VALUES (1, 42)")
            .unwrap();

        let value: i64 = store
            .query_scalar("SELECT value FROM test_scalar WHERE id = 1")
            .unwrap();
        assert_eq!(value, 42);
    }

    #[test]
    fn test_query_scalar_string() {
        let store = VcStore::open_memory().unwrap();
        store
            .execute_simple("CREATE TABLE test_str_scalar (id INTEGER, name TEXT)")
            .unwrap();
        store
            .execute_simple("INSERT INTO test_str_scalar VALUES (1, 'hello')")
            .unwrap();

        let name: String = store
            .query_scalar("SELECT name FROM test_str_scalar WHERE id = 1")
            .unwrap();
        assert_eq!(name, "hello");
    }

    #[test]
    fn test_query_scalar_float() {
        let store = VcStore::open_memory().unwrap();
        store
            .execute_simple("CREATE TABLE test_float (id INTEGER, val DOUBLE)")
            .unwrap();
        store
            .execute_simple("INSERT INTO test_float VALUES (1, 3.14)")
            .unwrap();

        let val: f64 = store
            .query_scalar("SELECT val FROM test_float WHERE id = 1")
            .unwrap();
        assert!((val - 3.14).abs() < f64::EPSILON);
    }

    #[test]
    fn test_query_scalar_no_rows() {
        let store = VcStore::open_memory().unwrap();
        store
            .execute_simple("CREATE TABLE test_empty (id INTEGER)")
            .unwrap();

        let result: Result<i64, _> = store.query_scalar("SELECT id FROM test_empty");
        assert!(result.is_err());
    }

    #[test]
    fn test_query_json_empty() {
        let store = VcStore::open_memory().unwrap();
        store
            .execute_simple("CREATE TABLE test_empty_json (id INTEGER)")
            .unwrap();

        let results = store.query_json("SELECT * FROM test_empty_json").unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_query_json_multiple_rows() {
        let store = VcStore::open_memory().unwrap();
        store
            .execute_batch(
                r#"
            CREATE TABLE test_multi (id INTEGER, name TEXT);
            INSERT INTO test_multi VALUES (1, 'a');
            INSERT INTO test_multi VALUES (2, 'b');
            INSERT INTO test_multi VALUES (3, 'c');
        "#,
            )
            .unwrap();

        let results = store
            .query_json("SELECT * FROM test_multi ORDER BY id")
            .unwrap();
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
        store
            .execute_batch(
                r#"
            CREATE TABLE test_upsert (
                id TEXT PRIMARY KEY,
                value INTEGER
            );
        "#,
            )
            .unwrap();

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

        let results = store
            .query_json("SELECT * FROM test_upsert ORDER BY id")
            .unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0]["value"], 10); // Updated
        assert_eq!(results[1]["value"], 2); // Unchanged
        assert_eq!(results[2]["value"], 3); // New
    }

    #[test]
    fn test_upsert_json_empty() {
        let store = VcStore::open_memory().unwrap();
        store
            .execute_batch(
                r#"
            CREATE TABLE test_upsert_empty (
                id TEXT PRIMARY KEY,
                value INTEGER
            );
        "#,
            )
            .unwrap();

        let count = store
            .upsert_json("test_upsert_empty", &[], &["id"])
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_upsert_json_skips_non_objects() {
        let store = VcStore::open_memory().unwrap();
        store
            .execute_batch(
                r#"
            CREATE TABLE test_upsert_mixed (
                id TEXT PRIMARY KEY,
                value INTEGER
            );
        "#,
            )
            .unwrap();

        // Mix of valid object and invalid non-object
        let rows = vec![
            serde_json::json!({"id": "valid", "value": 1}),
            serde_json::json!("not an object"),
        ];
        // Only the valid object should be inserted
        let count = store
            .upsert_json("test_upsert_mixed", &rows, &["id"])
            .unwrap();
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
        let result = store.execute_simple("INVALID SQL STATEMENT HERE");
        assert!(result.is_err());
        let err = result.unwrap_err();
        // DatabaseError should format with "Database error: ..."
        let msg = err.to_string();
        assert!(
            msg.contains("Database error") || msg.contains("error"),
            "Error: {}",
            msg
        );
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
        store
            .execute_batch(
                r#"
            CREATE TABLE workflow_test (
                machine_id TEXT PRIMARY KEY,
                hostname TEXT,
                status TEXT,
                cpu_pct DOUBLE
            );
        "#,
            )
            .unwrap();

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
        let results = store
            .query_json("SELECT * FROM workflow_test WHERE status = 'online'")
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["hostname"], "server-01");

        // Update via upsert
        let updates = vec![serde_json::json!({
            "machine_id": "m2",
            "hostname": "server-02",
            "status": "online",
            "cpu_pct": 30.0
        })];
        store
            .upsert_json("workflow_test", &updates, &["machine_id"])
            .unwrap();

        // Verify update
        let online = store
            .query_json("SELECT * FROM workflow_test WHERE status = 'online' ORDER BY machine_id")
            .unwrap();
        assert_eq!(online.len(), 2);

        // Scalar query
        let count: i64 = store
            .query_scalar("SELECT COUNT(*) FROM workflow_test")
            .unwrap();
        assert_eq!(count, 2);

        // Cursor management
        store
            .set_cursor("m1", "collector", "last_poll", "2026-01-28T00:00:00Z")
            .unwrap();
        let cursor = store.get_cursor("m1", "collector", "last_poll").unwrap();
        assert_eq!(cursor, Some("2026-01-28T00:00:00Z".to_string()));
    }

    #[test]
    fn test_db_path_accessor() {
        let store = VcStore::open_memory().unwrap();
        assert_eq!(store.db_path(), ":memory:");
    }

    // =============================================================================
    // Retention Policy Tests
    // =============================================================================

    #[test]
    fn test_retention_policy_crud() {
        let store = VcStore::open_memory().unwrap();

        // Initially no policies
        let policies = store.list_retention_policies().unwrap();
        assert!(policies.is_empty());

        // Set a policy
        store
            .set_retention_policy("sys_samples", 7, None, true)
            .unwrap();

        // List policies
        let policies = store.list_retention_policies().unwrap();
        assert_eq!(policies.len(), 1);
        assert_eq!(policies[0].table_name, "sys_samples");
        assert_eq!(policies[0].retention_days, 7);
        assert!(policies[0].enabled);

        // Get specific policy
        let policy = store.get_retention_policy("sys_samples").unwrap();
        assert!(policy.is_some());
        let policy = policy.unwrap();
        assert_eq!(policy.retention_days, 7);

        // Non-existent policy
        let none = store.get_retention_policy("nonexistent").unwrap();
        assert!(none.is_none());
    }

    #[test]
    fn test_retention_policy_update() {
        let store = VcStore::open_memory().unwrap();

        // Set initial policy
        store
            .set_retention_policy("sys_samples", 7, None, true)
            .unwrap();

        // Update policy
        store
            .set_retention_policy("sys_samples", 30, None, false)
            .unwrap();

        // Verify update
        let policy = store.get_retention_policy("sys_samples").unwrap().unwrap();
        assert_eq!(policy.retention_days, 30);
        assert!(!policy.enabled);
    }

    #[test]
    fn test_vacuum_dry_run_no_policies() {
        let store = VcStore::open_memory().unwrap();

        // Run vacuum with no policies
        let results = store.run_vacuum(true, None).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_vacuum_history() {
        let store = VcStore::open_memory().unwrap();

        // Initially no history
        let history = store.list_vacuum_history(10).unwrap();
        assert!(history.is_empty());
    }

    #[test]
    fn test_vacuum_with_data() {
        let store = VcStore::open_memory().unwrap();

        // Create a test table with a timestamp column
        store
            .execute_simple(
                "CREATE TABLE test_vacuum_data (id INTEGER, collected_at TIMESTAMP, data TEXT)",
            )
            .unwrap();

        // Insert some data - some old, some recent
        store
            .execute_simple(
                "INSERT INTO test_vacuum_data VALUES
                 (1, '2020-01-01 00:00:00', 'old'),
                 (2, '2020-06-01 00:00:00', 'old'),
                 (3, current_timestamp, 'new')",
            )
            .unwrap();

        // Set a retention policy for 30 days
        store
            .set_retention_policy("test_vacuum_data", 30, None, true)
            .unwrap();

        // Run dry-run vacuum
        let results = store.run_vacuum(true, Some("test_vacuum_data")).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].dry_run);
        assert_eq!(results[0].rows_deleted, 0); // dry run doesn't delete
        assert_eq!(results[0].rows_would_delete, 2); // 2 old rows would be deleted
        assert!(results[0].error.is_none());

        // Verify data still exists (dry run)
        let count: i64 = store
            .query_scalar("SELECT COUNT(*) FROM test_vacuum_data")
            .unwrap();
        assert_eq!(count, 3);

        // Now run actual vacuum
        let results = store.run_vacuum(false, Some("test_vacuum_data")).unwrap();
        assert_eq!(results.len(), 1);
        assert!(!results[0].dry_run);
        assert_eq!(results[0].rows_deleted, 2);
        assert!(results[0].error.is_none());

        // Verify only new data remains
        let count: i64 = store
            .query_scalar("SELECT COUNT(*) FROM test_vacuum_data")
            .unwrap();
        assert_eq!(count, 1);

        // Verify history was logged
        let history = store.list_vacuum_history(10).unwrap();
        assert_eq!(history.len(), 2); // dry run + actual run
    }

    #[test]
    fn test_vacuum_disabled_policy() {
        let store = VcStore::open_memory().unwrap();

        // Create a test table
        store
            .execute_simple("CREATE TABLE test_disabled (id INTEGER, ts TIMESTAMP)")
            .unwrap();

        // Set a disabled retention policy
        store
            .set_retention_policy("test_disabled", 7, None, false)
            .unwrap();

        // Run vacuum - should skip disabled policy
        let results = store.run_vacuum(true, None).unwrap();
        assert!(results.is_empty());
    }

    // =============================================================================
    // Collector Health Tests
    // =============================================================================

    #[test]
    fn test_insert_collector_health() {
        let store = VcStore::open_memory().unwrap();

        let health = CollectorHealth {
            machine_id: "m1".to_string(),
            collector: "sysmoni".to_string(),
            collected_at: "2026-01-30 00:00:00".to_string(),
            success: true,
            duration_ms: Some(150),
            rows_inserted: 42,
            bytes_parsed: 8192,
            error_class: None,
            freshness_seconds: Some(120),
            payload_hash: Some("abc123".to_string()),
            collector_version: Some("1.0".to_string()),
            schema_version: Some("v1".to_string()),
            cursor_json: None,
        };

        store.insert_collector_health(&health).unwrap();

        let entries = store
            .list_collector_health(Some("m1"), Some("sysmoni"), 10)
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["machine_id"], "m1");
        assert_eq!(entries[0]["collector"], "sysmoni");
        assert_eq!(entries[0]["success"], true);
    }

    #[test]
    fn test_collector_health_multiple_entries() {
        let store = VcStore::open_memory().unwrap();

        // Insert health for two collectors
        for (collector, ts) in [
            ("sysmoni", "2026-01-30 00:00:00"),
            ("caut", "2026-01-30 00:01:00"),
        ] {
            let health = CollectorHealth {
                machine_id: "m1".to_string(),
                collector: collector.to_string(),
                collected_at: ts.to_string(),
                success: true,
                duration_ms: Some(100),
                rows_inserted: 10,
                bytes_parsed: 1024,
                error_class: None,
                freshness_seconds: Some(60),
                payload_hash: None,
                collector_version: None,
                schema_version: None,
                cursor_json: None,
            };
            store.insert_collector_health(&health).unwrap();
        }

        // List all for machine
        let all = store.list_collector_health(Some("m1"), None, 100).unwrap();
        assert_eq!(all.len(), 2);

        // Filter by collector
        let sysmoni_only = store
            .list_collector_health(Some("m1"), Some("sysmoni"), 100)
            .unwrap();
        assert_eq!(sysmoni_only.len(), 1);
    }

    #[test]
    fn test_collector_health_failure() {
        let store = VcStore::open_memory().unwrap();

        let health = CollectorHealth {
            machine_id: "m1".to_string(),
            collector: "broken".to_string(),
            collected_at: "2026-01-30 00:00:00".to_string(),
            success: false,
            duration_ms: Some(5000),
            rows_inserted: 0,
            bytes_parsed: 0,
            error_class: Some("timeout".to_string()),
            freshness_seconds: None,
            payload_hash: None,
            collector_version: None,
            schema_version: None,
            cursor_json: None,
        };

        store.insert_collector_health(&health).unwrap();

        let entries = store
            .list_collector_health(None, Some("broken"), 10)
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["success"], false);
        assert_eq!(entries[0]["error_class"], "timeout");
    }

    #[test]
    fn test_freshness_summaries() {
        let store = VcStore::open_memory().unwrap();

        // Insert a recent successful collection
        let health = CollectorHealth {
            machine_id: "m1".to_string(),
            collector: "sysmoni".to_string(),
            collected_at: Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            success: true,
            duration_ms: Some(100),
            rows_inserted: 10,
            bytes_parsed: 1024,
            error_class: None,
            freshness_seconds: Some(5),
            payload_hash: None,
            collector_version: None,
            schema_version: None,
            cursor_json: None,
        };
        store.insert_collector_health(&health).unwrap();

        let summaries = store.get_freshness_summaries(Some("m1"), 600).unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].machine_id, "m1");
        assert_eq!(summaries[0].collector, "sysmoni");
        // Recently inserted, freshness should be small (< 10 seconds)
        assert!(summaries[0].freshness_seconds < 60);
        assert!(!summaries[0].stale);
    }

    #[test]
    fn test_freshness_stale_detection() {
        let store = VcStore::open_memory().unwrap();

        // Insert an old collection (1 hour ago)
        let old_ts = (Utc::now() - ChronoDuration::hours(1))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        let health = CollectorHealth {
            machine_id: "m1".to_string(),
            collector: "slow_collector".to_string(),
            collected_at: old_ts,
            success: true,
            duration_ms: Some(100),
            rows_inserted: 5,
            bytes_parsed: 512,
            error_class: None,
            freshness_seconds: Some(3600),
            payload_hash: None,
            collector_version: None,
            schema_version: None,
            cursor_json: None,
        };
        store.insert_collector_health(&health).unwrap();

        // Threshold of 600 seconds (10 min) - should be stale
        let summaries = store.get_freshness_summaries(Some("m1"), 600).unwrap();
        assert_eq!(summaries.len(), 1);
        assert!(summaries[0].stale);
        assert!(summaries[0].freshness_seconds > 600);
    }

    // =============================================================================
    // Machine Baseline Tests
    // =============================================================================

    #[test]
    fn test_machine_baseline_crud() {
        let store = VcStore::open_memory().unwrap();

        let metrics = serde_json::json!({
            "cpu_pct": {"mean": 45.0, "std": 10.0, "p50": 44.0, "p95": 62.0},
            "mem_pct": {"mean": 60.0, "std": 5.0, "p50": 59.0, "p95": 70.0},
        });

        store.set_machine_baseline("m1", "7d", &metrics).unwrap();

        // Retrieve
        let baseline = store.get_machine_baseline("m1", "7d").unwrap();
        assert!(baseline.is_some());
        let baseline = baseline.unwrap();
        assert_eq!(baseline.machine_id, "m1");
        assert_eq!(baseline.baseline_window, "7d");
        assert_eq!(baseline.metrics_json["cpu_pct"]["mean"], 45.0);

        // Non-existent
        let missing = store.get_machine_baseline("m1", "30d").unwrap();
        assert!(missing.is_none());
    }

    #[test]
    fn test_machine_baseline_update() {
        let store = VcStore::open_memory().unwrap();

        let metrics_v1 = serde_json::json!({"cpu_pct": {"mean": 40.0, "std": 8.0}});
        store.set_machine_baseline("m1", "7d", &metrics_v1).unwrap();

        let metrics_v2 = serde_json::json!({"cpu_pct": {"mean": 50.0, "std": 12.0}});
        store.set_machine_baseline("m1", "7d", &metrics_v2).unwrap();

        let baseline = store.get_machine_baseline("m1", "7d").unwrap().unwrap();
        assert_eq!(baseline.metrics_json["cpu_pct"]["mean"], 50.0);
    }

    #[test]
    fn test_list_machine_baselines() {
        let store = VcStore::open_memory().unwrap();

        store
            .set_machine_baseline("m1", "7d", &serde_json::json!({"cpu": 40}))
            .unwrap();
        store
            .set_machine_baseline("m1", "30d", &serde_json::json!({"cpu": 42}))
            .unwrap();
        store
            .set_machine_baseline("m2", "7d", &serde_json::json!({"cpu": 55}))
            .unwrap();

        // All baselines
        let all = store.list_machine_baselines(None).unwrap();
        assert_eq!(all.len(), 3);

        // Filtered by machine
        let m1_only = store.list_machine_baselines(Some("m1")).unwrap();
        assert_eq!(m1_only.len(), 2);
    }

    // =============================================================================
    // Drift Detection Tests
    // =============================================================================

    #[test]
    fn test_drift_event_insert_and_list() {
        let store = VcStore::open_memory().unwrap();

        let event = DriftEvent {
            machine_id: "m1".to_string(),
            detected_at: Utc::now().to_rfc3339(),
            metric: "cpu_pct".to_string(),
            current_value: 95.0,
            baseline_mean: 45.0,
            baseline_std: 10.0,
            z_score: 5.0,
            severity: DriftSeverity::Critical,
            evidence_json: Some(serde_json::json!({"baseline_window": "7d"})),
        };

        store.insert_drift_event(&event).unwrap();

        let events = store.list_drift_events(Some("m1"), None, 100).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["metric"], "cpu_pct");
        assert_eq!(events[0]["severity"], "critical");
    }

    #[test]
    fn test_drift_events_filter_by_severity() {
        let store = VcStore::open_memory().unwrap();

        // Insert events of different severities
        for (sev, z) in [
            (DriftSeverity::Info, 2.0),
            (DriftSeverity::Warning, 3.5),
            (DriftSeverity::Critical, 5.0),
        ] {
            let event = DriftEvent {
                machine_id: "m1".to_string(),
                detected_at: Utc::now().to_rfc3339(),
                metric: "cpu_pct".to_string(),
                current_value: 80.0,
                baseline_mean: 45.0,
                baseline_std: 10.0,
                z_score: z,
                severity: sev,
                evidence_json: None,
            };
            store.insert_drift_event(&event).unwrap();
        }

        let critical = store
            .list_drift_events(None, Some("critical"), 100)
            .unwrap();
        assert_eq!(critical.len(), 1);

        let all = store.list_drift_events(None, None, 100).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_check_drift_triggers() {
        let store = VcStore::open_memory().unwrap();

        // Set up baseline: cpu_pct mean=45, std=10
        let baseline = serde_json::json!({
            "cpu_pct": {"mean": 45.0, "std": 10.0},
        });
        store.set_machine_baseline("m1", "7d", &baseline).unwrap();

        // Check with value that exceeds 3-sigma threshold
        let event = store.check_drift("m1", "cpu_pct", 95.0, 3.0, "7d").unwrap();
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.severity, DriftSeverity::Critical);
        assert!((event.z_score - 5.0).abs() < f64::EPSILON);

        // Check with value within normal range
        let no_event = store.check_drift("m1", "cpu_pct", 50.0, 3.0, "7d").unwrap();
        assert!(no_event.is_none());
    }

    #[test]
    fn test_check_drift_no_baseline() {
        let store = VcStore::open_memory().unwrap();

        // No baseline exists - should return None (not an error)
        let result = store.check_drift("m1", "cpu_pct", 95.0, 3.0, "7d").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_check_drift_unknown_metric() {
        let store = VcStore::open_memory().unwrap();

        let baseline = serde_json::json!({
            "cpu_pct": {"mean": 45.0, "std": 10.0},
        });
        store.set_machine_baseline("m1", "7d", &baseline).unwrap();

        // Unknown metric - should return None
        let result = store.check_drift("m1", "disk_io", 95.0, 3.0, "7d").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_check_drift_zero_std() {
        let store = VcStore::open_memory().unwrap();

        let baseline = serde_json::json!({
            "cpu_pct": {"mean": 45.0, "std": 0.0},
        });
        store.set_machine_baseline("m1", "7d", &baseline).unwrap();

        // Zero std deviation - avoid division by zero, return None
        let result = store.check_drift("m1", "cpu_pct", 95.0, 3.0, "7d").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_drift_severity_from_z_score() {
        assert_eq!(DriftSeverity::from_z_score(2.0), DriftSeverity::Info);
        assert_eq!(DriftSeverity::from_z_score(3.0), DriftSeverity::Warning);
        assert_eq!(DriftSeverity::from_z_score(3.5), DriftSeverity::Warning);
        assert_eq!(DriftSeverity::from_z_score(4.0), DriftSeverity::Critical);
        assert_eq!(DriftSeverity::from_z_score(5.0), DriftSeverity::Critical);
        assert_eq!(DriftSeverity::from_z_score(-4.5), DriftSeverity::Critical);
    }

    #[test]
    fn test_drift_severity_roundtrip() {
        let severities = ["info", "warning", "critical"];
        for s in severities {
            let parsed: DriftSeverity = s.parse().unwrap();
            assert_eq!(parsed.as_str(), s);
        }

        let err = "invalid".parse::<DriftSeverity>();
        assert!(err.is_err());
    }

    // =========================================================================
    // Alert Delivery Log Tests
    // =========================================================================

    #[test]
    fn test_insert_delivery_log() {
        let store = VcStore::open_memory().unwrap();
        store
            .insert_delivery_log("alert-1", "slack", "success", None, Some(150))
            .unwrap();

        let logs = store.list_delivery_logs(Some("alert-1"), 10).unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0]["alert_id"], "alert-1");
        assert_eq!(logs[0]["channel_type"], "slack");
        assert_eq!(logs[0]["status"], "success");
        assert!(logs[0]["error_message"].is_null());
        assert_eq!(logs[0]["duration_ms"], 150);
    }

    #[test]
    fn test_delivery_log_with_error() {
        let store = VcStore::open_memory().unwrap();
        store
            .insert_delivery_log(
                "alert-2",
                "discord",
                "failed",
                Some("Connection refused"),
                Some(5000),
            )
            .unwrap();

        let logs = store.list_delivery_logs(Some("alert-2"), 10).unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0]["status"], "failed");
        assert_eq!(logs[0]["error_message"], "Connection refused");
    }

    #[test]
    fn test_delivery_log_multiple_channels() {
        let store = VcStore::open_memory().unwrap();
        store
            .insert_delivery_log("alert-3", "slack", "success", None, Some(100))
            .unwrap();
        store
            .insert_delivery_log("alert-3", "discord", "success", None, Some(200))
            .unwrap();
        store
            .insert_delivery_log(
                "alert-3",
                "desktop",
                "failed",
                Some("notify-send not found"),
                None,
            )
            .unwrap();

        let logs = store.list_delivery_logs(Some("alert-3"), 10).unwrap();
        assert_eq!(logs.len(), 3);
    }

    #[test]
    fn test_delivery_log_filter_by_alert() {
        let store = VcStore::open_memory().unwrap();
        store
            .insert_delivery_log("alert-a", "slack", "success", None, None)
            .unwrap();
        store
            .insert_delivery_log("alert-b", "slack", "success", None, None)
            .unwrap();

        let logs_a = store.list_delivery_logs(Some("alert-a"), 10).unwrap();
        assert_eq!(logs_a.len(), 1);
        assert_eq!(logs_a[0]["alert_id"], "alert-a");

        let logs_b = store.list_delivery_logs(Some("alert-b"), 10).unwrap();
        assert_eq!(logs_b.len(), 1);
        assert_eq!(logs_b[0]["alert_id"], "alert-b");

        // All logs (no filter)
        let all = store.list_delivery_logs(None, 10).unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_update_delivery_status() {
        let store = VcStore::open_memory().unwrap();
        store
            .insert_delivery_log("alert-retry", "webhook", "pending", None, None)
            .unwrap();

        let logs = store.list_delivery_logs(Some("alert-retry"), 10).unwrap();
        let delivery_id = logs[0]["id"].as_i64().unwrap();

        store
            .update_delivery_status(delivery_id, "success", None, 1)
            .unwrap();

        let logs = store.list_delivery_logs(Some("alert-retry"), 10).unwrap();
        assert_eq!(logs[0]["status"], "success");
        assert_eq!(logs[0]["retry_count"], 1);
    }

    #[test]
    fn test_update_delivery_status_with_error() {
        let store = VcStore::open_memory().unwrap();
        store
            .insert_delivery_log("alert-fail", "slack", "pending", None, None)
            .unwrap();

        let logs = store.list_delivery_logs(Some("alert-fail"), 10).unwrap();
        let delivery_id = logs[0]["id"].as_i64().unwrap();

        store
            .update_delivery_status(delivery_id, "failed", Some("timeout after 30s"), 3)
            .unwrap();

        let logs = store.list_delivery_logs(Some("alert-fail"), 10).unwrap();
        assert_eq!(logs[0]["status"], "failed");
        assert_eq!(logs[0]["error_message"], "timeout after 30s");
        assert_eq!(logs[0]["retry_count"], 3);
    }

    #[test]
    fn test_delivery_summary() {
        let store = VcStore::open_memory().unwrap();

        // Slack: 2 success, 1 failed
        store
            .insert_delivery_log("a1", "slack", "success", None, None)
            .unwrap();
        store
            .insert_delivery_log("a2", "slack", "success", None, None)
            .unwrap();
        store
            .insert_delivery_log("a3", "slack", "failed", Some("err"), None)
            .unwrap();

        // Discord: 1 success
        store
            .insert_delivery_log("a1", "discord", "success", None, None)
            .unwrap();

        // Desktop: 1 pending
        store
            .insert_delivery_log("a1", "desktop", "pending", None, None)
            .unwrap();

        let summary = store.delivery_summary().unwrap();
        assert_eq!(summary.len(), 3);

        let desktop = summary
            .iter()
            .find(|s| s["channel_type"] == "desktop")
            .unwrap();
        assert_eq!(desktop["total"], 1);
        assert_eq!(desktop["pending"], 1);

        let discord = summary
            .iter()
            .find(|s| s["channel_type"] == "discord")
            .unwrap();
        assert_eq!(discord["total"], 1);
        assert_eq!(discord["succeeded"], 1);

        let slack = summary
            .iter()
            .find(|s| s["channel_type"] == "slack")
            .unwrap();
        assert_eq!(slack["total"], 3);
        assert_eq!(slack["succeeded"], 2);
        assert_eq!(slack["failed"], 1);
    }

    #[test]
    fn test_delivery_log_limit() {
        let store = VcStore::open_memory().unwrap();
        for i in 0..10 {
            store
                .insert_delivery_log(&format!("alert-{i}"), "slack", "success", None, None)
                .unwrap();
        }

        let logs = store.list_delivery_logs(None, 5).unwrap();
        assert_eq!(logs.len(), 5);

        // Zero limit defaults to 50
        let logs = store.list_delivery_logs(None, 0).unwrap();
        assert_eq!(logs.len(), 10);
    }

    #[test]
    fn test_delivery_summary_empty() {
        let store = VcStore::open_memory().unwrap();
        let summary = store.delivery_summary().unwrap();
        assert!(summary.is_empty());
    }

    // =========================================================================
    // Autopilot Decision Tests
    // =========================================================================

    #[test]
    fn test_insert_autopilot_decision() {
        let store = VcStore::open_memory().unwrap();
        store
            .insert_autopilot_decision(
                "account_switch",
                "Usage at 80%",
                0.92,
                true,
                Some(r#"{"from":"acc1","to":"acc2"}"#),
            )
            .unwrap();

        let decisions = store.list_autopilot_decisions(None, 10).unwrap();
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0]["decision_type"], "account_switch");
        assert_eq!(decisions[0]["reason"], "Usage at 80%");
        assert_eq!(decisions[0]["confidence"], 0.92);
        assert_eq!(decisions[0]["executed"], true);
    }

    #[test]
    fn test_autopilot_decision_suggested_only() {
        let store = VcStore::open_memory().unwrap();
        store
            .insert_autopilot_decision(
                "cost_optimization",
                "Daily spend exceeds budget",
                0.95,
                false,
                None,
            )
            .unwrap();

        let decisions = store.list_autopilot_decisions(None, 10).unwrap();
        assert_eq!(decisions[0]["executed"], false);
        assert!(decisions[0]["details_json"].is_null());
    }

    #[test]
    fn test_autopilot_decision_filter_by_type() {
        let store = VcStore::open_memory().unwrap();
        store
            .insert_autopilot_decision("account_switch", "r1", 0.9, true, None)
            .unwrap();
        store
            .insert_autopilot_decision("cost_optimization", "r2", 0.8, false, None)
            .unwrap();
        store
            .insert_autopilot_decision("account_switch", "r3", 0.85, true, None)
            .unwrap();

        let switches = store
            .list_autopilot_decisions(Some("account_switch"), 10)
            .unwrap();
        assert_eq!(switches.len(), 2);

        let costs = store
            .list_autopilot_decisions(Some("cost_optimization"), 10)
            .unwrap();
        assert_eq!(costs.len(), 1);

        let all = store.list_autopilot_decisions(None, 10).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_autopilot_decision_limit() {
        let store = VcStore::open_memory().unwrap();
        for i in 0..10 {
            store
                .insert_autopilot_decision(
                    "workload_balance",
                    &format!("reason-{i}"),
                    0.7,
                    false,
                    None,
                )
                .unwrap();
        }

        let decisions = store.list_autopilot_decisions(None, 5).unwrap();
        assert_eq!(decisions.len(), 5);
    }

    #[test]
    fn test_autopilot_decision_summary() {
        let store = VcStore::open_memory().unwrap();
        store
            .insert_autopilot_decision("account_switch", "r1", 0.9, true, None)
            .unwrap();
        store
            .insert_autopilot_decision("account_switch", "r2", 0.85, false, None)
            .unwrap();
        store
            .insert_autopilot_decision("cost_optimization", "r3", 0.8, true, None)
            .unwrap();

        let summary = store.autopilot_decision_summary().unwrap();
        assert_eq!(summary.len(), 2);

        let switch_summary = summary
            .iter()
            .find(|s| s["decision_type"] == "account_switch")
            .unwrap();
        assert_eq!(switch_summary["total"], 2);
        assert_eq!(switch_summary["executed"], 1);
        assert_eq!(switch_summary["suggested"], 1);

        let cost_summary = summary
            .iter()
            .find(|s| s["decision_type"] == "cost_optimization")
            .unwrap();
        assert_eq!(cost_summary["total"], 1);
        assert_eq!(cost_summary["executed"], 1);
    }

    #[test]
    fn test_autopilot_decision_summary_empty() {
        let store = VcStore::open_memory().unwrap();
        let summary = store.autopilot_decision_summary().unwrap();
        assert!(summary.is_empty());
    }

    // =========================================================================
    // Incident replay / time-travel tests
    // =========================================================================

    #[test]
    fn test_build_replay_snapshot() {
        let store = VcStore::open_memory().unwrap();
        store
            .create_incident("inc-replay-1", "Test incident", "critical", Some("A test"))
            .unwrap();

        let snapshot = store
            .build_replay_snapshot("inc-replay-1", "2099-01-01T00:00:00")
            .unwrap();

        assert!(snapshot.get("incident").is_some());
        assert!(snapshot.get("snapshot_at").is_some());
        assert_eq!(snapshot["snapshot_at"], "2099-01-01T00:00:00");
        assert!(snapshot.get("machines").is_some());
        assert!(snapshot.get("alerts").is_some());
        assert!(snapshot.get("audit_events").is_some());
        assert!(snapshot.get("timeline").is_some());
        assert!(snapshot.get("health_scores").is_some());
    }

    #[test]
    fn test_build_replay_snapshot_not_found() {
        let store = VcStore::open_memory().unwrap();
        let result = store.build_replay_snapshot("nonexistent", "2026-01-01T00:00:00");
        assert!(result.is_err());
    }

    #[test]
    fn test_cache_and_get_replay() {
        let store = VcStore::open_memory().unwrap();
        let snapshot = serde_json::json!({"test": "data", "machines": []});
        let snapshot_str = serde_json::to_string(&snapshot).unwrap();

        store
            .cache_replay_snapshot("inc-1", "2026-01-01T12:00:00", &snapshot_str)
            .unwrap();

        let cached = store
            .get_cached_replay("inc-1", "2026-01-01T12:00:00")
            .unwrap();

        assert!(cached.is_some());
        assert_eq!(cached.unwrap()["test"], "data");
    }

    #[test]
    fn test_get_cached_replay_miss() {
        let store = VcStore::open_memory().unwrap();
        let cached = store
            .get_cached_replay("nonexistent", "2026-01-01T00:00:00")
            .unwrap();
        assert!(cached.is_none());
    }

    #[test]
    fn test_list_replay_snapshots() {
        let store = VcStore::open_memory().unwrap();
        store
            .cache_replay_snapshot("inc-2", "2026-01-01T10:00:00", "{}")
            .unwrap();
        store
            .cache_replay_snapshot("inc-2", "2026-01-01T11:00:00", "{}")
            .unwrap();

        let snapshots = store.list_replay_snapshots("inc-2").unwrap();
        assert_eq!(snapshots.len(), 2);
    }

    #[test]
    fn test_list_replay_snapshots_empty() {
        let store = VcStore::open_memory().unwrap();
        let snapshots = store.list_replay_snapshots("nonexistent").unwrap();
        assert!(snapshots.is_empty());
    }

    #[test]
    fn test_get_or_build_replay_caches() {
        let store = VcStore::open_memory().unwrap();
        store
            .create_incident("inc-cache-1", "Cache test", "warning", None)
            .unwrap();

        // First call should build and cache
        let snap1 = store
            .get_or_build_replay("inc-cache-1", "2099-01-01T00:00:00")
            .unwrap();
        assert!(snap1.get("incident").is_some());

        // Second call should return cached version
        let snap2 = store
            .get_or_build_replay("inc-cache-1", "2099-01-01T00:00:00")
            .unwrap();
        assert_eq!(snap1["snapshot_at"], snap2["snapshot_at"]);

        // Verify it's actually cached
        let cached = store
            .get_cached_replay("inc-cache-1", "2099-01-01T00:00:00")
            .unwrap();
        assert!(cached.is_some());
    }

    #[test]
    fn test_export_incident_replay() {
        let store = VcStore::open_memory().unwrap();
        store
            .create_incident("inc-export-1", "Export test", "critical", Some("Test desc"))
            .unwrap();

        let export = store.export_incident_replay("inc-export-1").unwrap();
        assert_eq!(export["export_version"], "1.0");
        assert!(export.get("incident").is_some());
        assert!(export.get("timeline").is_some());
        assert!(export.get("notes").is_some());
        assert!(export.get("snapshots").is_some());
    }

    #[test]
    fn test_export_incident_not_found() {
        let store = VcStore::open_memory().unwrap();
        let result = store.export_incident_replay("nonexistent");
        assert!(result.is_err());
    }

    // =========================================================================
    // Data export/backup tests
    // =========================================================================

    #[test]
    fn test_list_tables() {
        let store = VcStore::open_memory().unwrap();
        let tables = store.list_tables().unwrap();
        assert!(!tables.is_empty());
        assert!(tables.contains(&"machines".to_string()));
        assert!(tables.contains(&"alert_history".to_string()));
    }

    #[test]
    fn test_export_table_jsonl_empty() {
        let store = VcStore::open_memory().unwrap();
        let lines = store.export_table_jsonl("machines", None, None).unwrap();
        assert!(lines.is_empty());
    }

    #[test]
    fn test_export_table_jsonl_with_data() {
        let store = VcStore::open_memory().unwrap();
        store
            .insert_json(
                "machines",
                &serde_json::json!({
                    "machine_id": "m-1",
                    "hostname": "test-host",
                    "status": "online",
                }),
            )
            .unwrap();

        let lines = store.export_table_jsonl("machines", None, None).unwrap();
        assert_eq!(lines.len(), 1);
        let parsed: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(parsed["hostname"], "test-host");
    }

    #[test]
    fn test_table_row_count() {
        let store = VcStore::open_memory().unwrap();
        let count = store.table_row_count("machines").unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_build_export_manifest() {
        let store = VcStore::open_memory().unwrap();
        let manifest = store
            .build_export_manifest(&["machines".to_string()], None, None)
            .unwrap();

        assert_eq!(manifest["export_version"], "1.0");
        assert!(manifest.get("exported_at").is_some());
        let tables = manifest["tables"].as_array().unwrap();
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0]["table"], "machines");
    }

    #[test]
    fn test_build_export_manifest_with_filter() {
        let store = VcStore::open_memory().unwrap();
        let manifest = store
            .build_export_manifest(
                &["machines".to_string()],
                Some("2026-01-01"),
                Some("2026-12-31"),
            )
            .unwrap();

        assert_eq!(manifest["filter"]["since"], "2026-01-01");
        assert_eq!(manifest["filter"]["until"], "2026-12-31");
    }

    #[test]
    fn test_import_table_jsonl() {
        let store = VcStore::open_memory().unwrap();
        let lines = vec![
            r#"{"machine_id": "m-imp-1", "hostname": "import-host", "status": "online"}"#.to_string(),
        ];

        let imported = store.import_table_jsonl("machines", &lines).unwrap();
        assert_eq!(imported, 1);

        let count = store.table_row_count("machines").unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_import_table_jsonl_empty() {
        let store = VcStore::open_memory().unwrap();
        let imported = store.import_table_jsonl("machines", &[]).unwrap();
        assert_eq!(imported, 0);
    }

    #[test]
    fn test_import_table_jsonl_skips_blank_lines() {
        let store = VcStore::open_memory().unwrap();
        let lines = vec![
            r#"{"machine_id": "m-1", "hostname": "h1", "status": "online"}"#.to_string(),
            "".to_string(),
            "  ".to_string(),
        ];

        let imported = store.import_table_jsonl("machines", &lines).unwrap();
        assert_eq!(imported, 1);
    }

    #[test]
    fn test_export_import_roundtrip() {
        let store = VcStore::open_memory().unwrap();

        // Insert some data
        store
            .create_incident("inc-rt-1", "Roundtrip test", "warning", Some("test"))
            .unwrap();

        // Export
        let lines = store
            .export_table_jsonl("incidents", None, None)
            .unwrap();
        assert_eq!(lines.len(), 1);

        // Create a fresh store and import
        let store2 = VcStore::open_memory().unwrap();
        let imported = store2.import_table_jsonl("incidents", &lines).unwrap();
        assert_eq!(imported, 1);

        // Verify data
        let incidents = store2.list_incidents(None, 10).unwrap();
        assert_eq!(incidents.len(), 1);
    }

    // =========================================================================
    // Alert routing event tests
    // =========================================================================

    #[test]
    fn test_insert_routing_event() {
        let store = VcStore::open_memory().unwrap();
        store
            .insert_routing_event("a-1", Some("r-1"), "slack", "sent", None)
            .unwrap();

        let events = store.list_routing_events(Some("a-1"), 10).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["action"].as_str(), Some("sent"));
    }

    #[test]
    fn test_list_routing_events_all() {
        let store = VcStore::open_memory().unwrap();
        store
            .insert_routing_event("a-1", None, "log", "sent", None)
            .unwrap();
        store
            .insert_routing_event("a-2", None, "slack", "escalated", None)
            .unwrap();

        let events = store.list_routing_events(None, 10).unwrap();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_routing_event_summary() {
        let store = VcStore::open_memory().unwrap();
        store
            .insert_routing_event("a-1", None, "log", "sent", None)
            .unwrap();
        store
            .insert_routing_event("a-2", None, "slack", "sent", None)
            .unwrap();
        store
            .insert_routing_event("a-3", None, "log", "suppressed", None)
            .unwrap();

        let summary = store.routing_event_summary().unwrap();
        assert_eq!(summary.len(), 2); // "sent" and "suppressed"
    }
}
