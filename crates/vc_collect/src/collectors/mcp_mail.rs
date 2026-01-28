//! MCP Agent Mail collector - agent coordination data
//!
//! This collector uses the SQLite Incremental ingestion pattern to collect
//! messages and file reservations from the mcp_agent_mail SQLite database.
//!
//! ## Integration Method
//! Direct SQLite queries on `~/.mcp_agent_mail_git_mailbox_repo/storage.sqlite3`
//!
//! ## Tables Populated
//! - `mail_messages`: Messages between agents (incremental)
//! - `mail_file_reservations`: Active file reservations (snapshot)

use async_trait::async_trait;
use std::time::Instant;

use crate::{CollectContext, CollectError, CollectResult, Collector, Cursor, RowBatch, Warning};

/// Default path to the agent mail SQLite database
pub const DEFAULT_DB_PATH: &str = "~/.mcp_agent_mail_git_mailbox_repo/storage.sqlite3";

/// MCP Agent Mail collector
///
/// Collects messages and file reservations from the mcp_agent_mail
/// coordination system's SQLite database using incremental primary key
/// pattern for messages and snapshot pattern for file reservations.
pub struct AgentMailCollector {
    /// Path to the SQLite database (with ~ expansion)
    db_path: String,
}

impl AgentMailCollector {
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

impl Default for AgentMailCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Collector for AgentMailCollector {
    fn name(&self) -> &'static str {
        "mcp_agent_mail"
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
                "Agent mail database not found: {}",
                db_path
            )));
            return Ok(CollectResult::empty()
                .with_warning(Warning::info("Database file not found"))
                .with_duration(start.elapsed()));
        }

        // Get the last seen message ID from cursor
        let last_id = ctx.primary_key_cursor().unwrap_or(0);
        let mut max_id = last_id;

        // Collect messages incrementally
        let messages_query = format!(
            r"
            SELECT
                id,
                project_id,
                thread_id,
                sender,
                importance,
                ack_required,
                created_ts,
                subject
            FROM messages
            WHERE id > {}
            ORDER BY id
            LIMIT {}
            ",
            last_id, ctx.max_rows
        );

        let messages = ctx
            .executor
            .sqlite_query(&db_path, &messages_query, ctx.timeout)
            .await?;

        let mut message_rows = Vec::with_capacity(messages.len());
        for msg in &messages {
            let id = msg["id"].as_i64().unwrap_or(0);
            max_id = max_id.max(id);

            message_rows.push(serde_json::json!({
                "machine_id": ctx.machine_id,
                "collected_at": ctx.collected_at.to_rfc3339(),
                "message_id": id,
                "thread_id": msg["thread_id"],
                "subject": msg["subject"],
                "sender": msg["sender"],
                "importance": msg["importance"],
                "ack_required": msg["ack_required"],
                "created_at": msg["created_ts"],
                "raw_json": serde_json::to_string(msg).unwrap_or_default(),
            }));
        }

        // Collect file reservations as snapshot (all currently active)
        let reservations_query = r"
            SELECT
                id,
                project_id,
                path_pattern,
                agent_id,
                expires_ts,
                exclusive,
                reason,
                created_ts
            FROM file_reservations
            WHERE released_ts IS NULL
              AND (expires_ts IS NULL OR expires_ts > datetime('now'))
        ";

        let reservations = ctx
            .executor
            .sqlite_query(&db_path, reservations_query, ctx.timeout)
            .await
            .unwrap_or_else(|e| {
                // File reservations table might not exist in older versions
                warnings.push(Warning::warn(format!(
                    "Could not query file_reservations: {}",
                    e
                )));
                vec![]
            });

        let reservation_rows: Vec<_> = reservations
            .iter()
            .map(|r| {
                serde_json::json!({
                    "machine_id": ctx.machine_id,
                    "collected_at": ctx.collected_at.to_rfc3339(),
                    "reservation_id": r["id"],
                    "project_id": r["project_id"],
                    "path_pattern": r["path_pattern"],
                    "holder": r["agent_id"],
                    "expires_ts": r["expires_ts"],
                    "exclusive": r["exclusive"],
                    "reason": r["reason"],
                    "raw_json": serde_json::to_string(r).unwrap_or_default(),
                })
            })
            .collect();

        // Build the result with both batches
        let mut batches = Vec::new();

        if !message_rows.is_empty() {
            batches.push(RowBatch {
                table: "mail_messages".to_string(),
                rows: message_rows,
            });
        }

        if !reservation_rows.is_empty() {
            batches.push(RowBatch {
                table: "mail_file_reservations".to_string(),
                rows: reservation_rows,
            });
        }

        let mut result = CollectResult::with_rows(batches).with_duration(start.elapsed());

        // Only update cursor if we got new messages
        if max_id > last_id {
            result = result.with_cursor(Cursor::primary_key(max_id));
        } else if let Some(cursor) = &ctx.cursor {
            // Preserve existing cursor if no new messages
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
        let collector = AgentMailCollector::new();
        assert_eq!(collector.name(), "mcp_agent_mail");
    }

    #[test]
    fn test_collector_schema_version() {
        let collector = AgentMailCollector::new();
        assert_eq!(collector.schema_version(), 1);
    }

    #[test]
    fn test_collector_supports_incremental() {
        let collector = AgentMailCollector::new();
        assert!(collector.supports_incremental());
    }

    #[test]
    fn test_collector_required_tool() {
        let collector = AgentMailCollector::new();
        assert_eq!(collector.required_tool(), Some("sqlite3"));
    }

    #[test]
    fn test_default_db_path() {
        let collector = AgentMailCollector::new();
        assert_eq!(collector.db_path, DEFAULT_DB_PATH);
    }

    #[test]
    fn test_custom_db_path() {
        let collector = AgentMailCollector::with_path("/custom/path/storage.sqlite3");
        assert_eq!(collector.db_path, "/custom/path/storage.sqlite3");
    }

    #[test]
    fn test_path_expansion() {
        let collector = AgentMailCollector::new();
        let expanded = collector.expand_path();

        // Should expand ~ to home directory if HOME is set
        if std::env::var("HOME").is_ok() {
            assert!(!expanded.starts_with("~/"));
            assert!(expanded.contains("mcp_agent_mail_git_mailbox_repo"));
        }
    }

    #[test]
    fn test_path_expansion_no_tilde() {
        let collector = AgentMailCollector::with_path("/absolute/path/db.sqlite3");
        let expanded = collector.expand_path();
        assert_eq!(expanded, "/absolute/path/db.sqlite3");
    }

    #[test]
    fn test_default_impl() {
        let collector = AgentMailCollector::default();
        assert_eq!(collector.db_path, DEFAULT_DB_PATH);
    }

    // Integration tests require actual database - mark as ignored
    #[tokio::test]
    #[ignore = "requires actual mcp_agent_mail database"]
    async fn test_collect_missing_database() {
        use std::time::Duration;

        let collector = AgentMailCollector::with_path("/nonexistent/path/db.sqlite3");
        let ctx = CollectContext::local("test-machine", Duration::from_secs(10));

        let result = collector.collect(&ctx).await;
        // Should succeed with empty result and warning
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.has_warnings());
    }
}
