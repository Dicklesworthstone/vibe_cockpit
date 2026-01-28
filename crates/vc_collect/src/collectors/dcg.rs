//! DCG (Dangerous Command Guard) collector - security command auditing
//!
//! This collector uses the SQLite Incremental ingestion pattern to collect
//! blocked/allowed dangerous command events from the dcg SQLite database.
//!
//! ## Integration Method
//! Direct SQLite queries on `~/.dcg/events.db`
//!
//! ## Tables Populated
//! - `dcg_events`: Command execution events with decisions

use async_trait::async_trait;
use std::time::Instant;

use crate::{CollectContext, CollectError, CollectResult, Collector, Cursor, RowBatch, Warning};

/// Default path to the dcg SQLite database
pub const DEFAULT_DB_PATH: &str = "~/.dcg/events.db";

/// DCG collector for dangerous command guard events
///
/// Collects command execution audit events from the dcg
/// database using incremental primary key pattern.
pub struct DcgCollector {
    /// Path to the SQLite database (with ~ expansion)
    db_path: String,
}

impl DcgCollector {
    /// Create a new collector with the default database path
    pub fn new() -> Self {
        Self {
            db_path: DEFAULT_DB_PATH.to_string(),
        }
    }

    /// Create a collector with a custom database path
    pub fn with_path(path: impl Into<String>) -> Self {
        Self {
            db_path: path.into(),
        }
    }

    /// Expand ~ to home directory in the path
    fn expand_path(&self) -> String {
        if self.db_path.starts_with("~/") {
            if let Ok(home) = std::env::var("HOME") {
                return self.db_path.replacen("~", &home, 1);
            }
        }
        self.db_path.clone()
    }
}

impl Default for DcgCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Collector for DcgCollector {
    fn name(&self) -> &'static str {
        "dcg"
    }

    fn schema_version(&self) -> u32 {
        1
    }

    fn required_tool(&self) -> Option<&'static str> {
        Some("sqlite3")
    }

    fn supports_incremental(&self) -> bool {
        true
    }

    async fn collect(&self, ctx: &CollectContext) -> Result<CollectResult, CollectError> {
        let start = Instant::now();
        let mut warnings = Vec::new();
        let db_path = self.expand_path();

        // Check if sqlite3 is available
        if !self.check_availability(ctx).await {
            return Err(CollectError::ToolNotFound("sqlite3".to_string()));
        }

        // Check if the database file exists
        if !ctx.executor.file_exists(&db_path, ctx.timeout).await? {
            // Database doesn't exist yet - this is not an error, just no data
            warnings.push(Warning::info(format!(
                "DCG database not found: {}",
                db_path
            )));
            return Ok(CollectResult::empty()
                .with_warning(Warning::info("Database file not found"))
                .with_duration(start.elapsed()));
        }

        // Get the last seen event ID from cursor
        let last_id = ctx.primary_key_cursor().unwrap_or(0);
        let mut max_id = last_id;

        // Collect events incrementally
        // DCG stores events with: id, ts, type, cmd, cwd, rule, severity, decision, reason, user
        let events_query = format!(
            r#"
            SELECT
                id,
                ts,
                type,
                cmd,
                cwd,
                rule,
                severity,
                decision,
                reason,
                user
            FROM events
            WHERE id > {}
            ORDER BY id
            LIMIT {}
            "#,
            last_id, ctx.max_rows
        );

        let events = ctx
            .executor
            .sqlite_query(&db_path, &events_query, ctx.timeout)
            .await?;

        let mut event_rows = Vec::with_capacity(events.len());
        for event in &events {
            let id = event["id"].as_i64().unwrap_or(0);
            max_id = max_id.max(id);

            event_rows.push(serde_json::json!({
                "machine_id": ctx.machine_id,
                "collected_at": ctx.collected_at.to_rfc3339(),
                "ts": event["ts"],
                "command": event["cmd"],
                "severity": event["severity"],
                "decision": event["decision"],
                "reason": event["reason"],
                "user": event["user"],
                "pwd": event["cwd"],
                "raw_json": serde_json::to_string(event).unwrap_or_default(),
            }));
        }

        // Build the result
        let mut batches = Vec::new();

        if !event_rows.is_empty() {
            batches.push(RowBatch {
                table: "dcg_events".to_string(),
                rows: event_rows,
            });
        }

        let mut result = CollectResult::with_rows(batches).with_duration(start.elapsed());

        // Only update cursor if we got new events
        if max_id > last_id {
            result = result.with_cursor(Cursor::primary_key(max_id));
        } else if let Some(cursor) = &ctx.cursor {
            // Preserve existing cursor if no new events
            result = result.with_cursor(cursor.clone());
        }

        // Add any warnings
        for warning in warnings {
            result = result.with_warning(warning);
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collector_name() {
        let collector = DcgCollector::new();
        assert_eq!(collector.name(), "dcg");
    }

    #[test]
    fn test_collector_schema_version() {
        let collector = DcgCollector::new();
        assert_eq!(collector.schema_version(), 1);
    }

    #[test]
    fn test_collector_supports_incremental() {
        let collector = DcgCollector::new();
        assert!(collector.supports_incremental());
    }

    #[test]
    fn test_collector_required_tool() {
        let collector = DcgCollector::new();
        assert_eq!(collector.required_tool(), Some("sqlite3"));
    }

    #[test]
    fn test_default_db_path() {
        let collector = DcgCollector::new();
        assert_eq!(collector.db_path, DEFAULT_DB_PATH);
    }

    #[test]
    fn test_custom_db_path() {
        let collector = DcgCollector::with_path("/custom/path/events.db");
        assert_eq!(collector.db_path, "/custom/path/events.db");
    }

    #[test]
    fn test_path_expansion() {
        let collector = DcgCollector::new();
        let expanded = collector.expand_path();

        // Should expand ~ to home directory if HOME is set
        if std::env::var("HOME").is_ok() {
            assert!(!expanded.starts_with("~/"));
            assert!(expanded.contains(".dcg"));
        }
    }

    #[test]
    fn test_path_expansion_no_tilde() {
        let collector = DcgCollector::with_path("/absolute/path/events.db");
        let expanded = collector.expand_path();
        assert_eq!(expanded, "/absolute/path/events.db");
    }

    #[test]
    fn test_default_impl() {
        let collector = DcgCollector::default();
        assert_eq!(collector.db_path, DEFAULT_DB_PATH);
    }

    // Integration tests require actual database - mark as ignored
    #[tokio::test]
    #[ignore = "requires actual dcg database"]
    async fn test_collect_missing_database() {
        use std::time::Duration;

        let collector = DcgCollector::with_path("/nonexistent/path/events.db");
        let ctx = CollectContext::local("test-machine", Duration::from_secs(10));

        let result = collector.collect(&ctx).await;
        // Should succeed with empty result and warning
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.has_warnings());
    }
}
