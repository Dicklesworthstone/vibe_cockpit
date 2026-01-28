//! rano collector - Network activity observer for coding agents
//!
//! This collector captures connections, domains, processes, and bandwidth
//! from the `rano` network observer tool.
//!
//! ## Integration Method
//! ```bash
//! rano export --format jsonl --since <duration>
//! ```
//!
//! ## Tables Populated
//! - `net_events`: Individual network events

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Instant;

use crate::{CollectContext, CollectError, CollectResult, Collector, Cursor, RowBatch, Warning};

/// A network event from rano JSONL export
#[derive(Debug, Deserialize, Serialize)]
pub struct RanoEvent {
    /// Event timestamp
    #[serde(default)]
    pub ts: Option<String>,

    /// Event ID (for deduplication)
    #[serde(default)]
    pub id: Option<i64>,

    /// Provider name (anthropic, openai, github, etc.)
    #[serde(default)]
    pub provider: Option<String>,

    /// Process name that made the connection
    #[serde(default)]
    pub process: Option<String>,

    /// Process ID
    #[serde(default)]
    pub pid: Option<i32>,

    /// Direction (inbound/outbound)
    #[serde(default)]
    pub direction: Option<String>,

    /// Protocol (tcp, udp, https)
    #[serde(default)]
    pub protocol: Option<String>,

    /// Remote hostname
    #[serde(default)]
    pub remote_host: Option<String>,

    /// Remote IP address
    #[serde(default)]
    pub remote_ip: Option<String>,

    /// Remote port
    #[serde(default)]
    pub remote_port: Option<i32>,

    /// Local port
    #[serde(default)]
    pub local_port: Option<i32>,

    /// Bytes sent
    #[serde(default)]
    pub bytes_sent: Option<i64>,

    /// Bytes received
    #[serde(default)]
    pub bytes_received: Option<i64>,

    /// Whether this is a known/expected provider
    #[serde(default)]
    pub is_known: Option<bool>,

    /// Tags for the event
    #[serde(default)]
    pub tags: Option<Vec<String>>,

    /// Event type (connection, dns, etc.)
    #[serde(default)]
    pub event_type: Option<String>,
}

/// rano collector for network activity
///
/// Collects network events from rano's JSONL export using a
/// timestamp-based cursor for incremental collection.
pub struct RanoCollector {
    /// Default duration for export window (e.g., "10m")
    export_window: String,
}

impl RanoCollector {
    /// Create a new collector with default 10-minute window
    pub fn new() -> Self {
        Self {
            export_window: "10m".to_string(),
        }
    }

    /// Create a collector with a custom export window
    pub fn with_window(window: impl Into<String>) -> Self {
        Self {
            export_window: window.into(),
        }
    }

    /// Normalize provider name for consistent aggregation
    fn normalize_provider(host: Option<&str>) -> Option<String> {
        let host = host?;
        let host_lower = host.to_lowercase();

        // Map known hosts to provider names
        if host_lower.contains("anthropic") || host_lower.contains("claude") {
            Some("anthropic".to_string())
        } else if host_lower.contains("openai") {
            Some("openai".to_string())
        } else if host_lower.contains("github") {
            Some("github".to_string())
        } else if host_lower.contains("google") || host_lower.contains("googleapis") {
            Some("google".to_string())
        } else if host_lower.contains("microsoft") || host_lower.contains("azure") {
            Some("microsoft".to_string())
        } else if host_lower.contains("aws") || host_lower.contains("amazonaws") {
            Some("aws".to_string())
        } else {
            // Return as-is for unknown providers
            Some(host.to_string())
        }
    }
}

impl Default for RanoCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Collector for RanoCollector {
    fn name(&self) -> &'static str {
        "rano"
    }

    fn schema_version(&self) -> u32 {
        1
    }

    fn required_tool(&self) -> Option<&'static str> {
        Some("rano")
    }

    fn supports_incremental(&self) -> bool {
        true
    }

    async fn collect(&self, ctx: &CollectContext) -> Result<CollectResult, CollectError> {
        let start = Instant::now();
        let mut warnings = Vec::new();

        // Check if rano is available
        if !self.check_availability(ctx).await {
            return Err(CollectError::ToolNotFound("rano".to_string()));
        }

        // Get last timestamp from cursor for incremental collection
        let since_opt = ctx.timestamp_cursor();

        // Build the export command
        let cmd = if let Some(since) = since_opt {
            // Use specific timestamp for incremental
            format!(
                "rano export --format jsonl --since {}",
                since.format("%Y-%m-%dT%H:%M:%SZ")
            )
        } else {
            // Use window-based export for initial collection
            format!(
                "rano export --format jsonl --since {}",
                self.export_window
            )
        };

        // Run the export command
        let output = match ctx.executor.run_timeout(&cmd, ctx.timeout).await {
            Ok(out) => out,
            Err(e) => {
                warnings.push(Warning::warn(format!("rano export failed: {}", e)));
                return Ok(CollectResult::empty()
                    .with_warning(Warning::warn(format!("rano export failed: {}", e)))
                    .with_duration(start.elapsed()));
            }
        };

        // Parse JSONL lines
        let mut event_rows = Vec::new();
        let mut max_ts: Option<DateTime<Utc>> = since_opt;

        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            match serde_json::from_str::<RanoEvent>(line) {
                Ok(event) => {
                    // Track max timestamp for cursor
                    if let Some(ts_str) = &event.ts {
                        if let Ok(ts) = DateTime::parse_from_rfc3339(ts_str) {
                            let ts_utc = ts.with_timezone(&Utc);
                            if max_ts.is_none() || Some(ts_utc) > max_ts {
                                max_ts = Some(ts_utc);
                            }
                        }
                    }

                    // Normalize provider from remote_host if not explicitly set
                    let provider = event
                        .provider
                        .clone()
                        .or_else(|| Self::normalize_provider(event.remote_host.as_deref()));

                    event_rows.push(serde_json::json!({
                        "machine_id": ctx.machine_id,
                        "collected_at": ctx.collected_at.to_rfc3339(),
                        "ts": event.ts,
                        "event_type": event.event_type,
                        "direction": event.direction,
                        "remote_ip": event.remote_ip,
                        "remote_port": event.remote_port,
                        "local_port": event.local_port,
                        "protocol": event.protocol,
                        "provider": provider,
                        "is_known": event.is_known,
                        "raw_json": line,
                    }));
                }
                Err(e) => {
                    warnings.push(Warning::warn(format!(
                        "Failed to parse rano event: {}",
                        e
                    )));
                }
            }

            // Limit rows per collection
            if event_rows.len() >= ctx.max_rows {
                break;
            }
        }

        // Build result
        let mut batches = Vec::new();
        if !event_rows.is_empty() {
            batches.push(RowBatch {
                table: "net_events".to_string(),
                rows: event_rows,
            });
        }

        let mut result = CollectResult::with_rows(batches).with_duration(start.elapsed());

        // Update cursor if we have a new max timestamp
        if let Some(ts) = max_ts {
            result = result.with_cursor(Cursor::Timestamp(ts));
        } else if let Some(cursor) = &ctx.cursor {
            // Preserve existing cursor
            result = result.with_cursor(cursor.clone());
        }

        // Add warnings
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
        let collector = RanoCollector::new();
        assert_eq!(collector.name(), "rano");
    }

    #[test]
    fn test_collector_schema_version() {
        let collector = RanoCollector::new();
        assert_eq!(collector.schema_version(), 1);
    }

    #[test]
    fn test_collector_supports_incremental() {
        let collector = RanoCollector::new();
        assert!(collector.supports_incremental());
    }

    #[test]
    fn test_collector_required_tool() {
        let collector = RanoCollector::new();
        assert_eq!(collector.required_tool(), Some("rano"));
    }

    #[test]
    fn test_default_export_window() {
        let collector = RanoCollector::new();
        assert_eq!(collector.export_window, "10m");
    }

    #[test]
    fn test_custom_export_window() {
        let collector = RanoCollector::with_window("1h");
        assert_eq!(collector.export_window, "1h");
    }

    #[test]
    fn test_default_impl() {
        let collector = RanoCollector::default();
        assert_eq!(collector.export_window, "10m");
    }

    #[test]
    fn test_parse_event_full() {
        let json = r#"{
            "ts": "2026-01-27T10:00:00Z",
            "id": 12345,
            "provider": "anthropic",
            "process": "claude-code",
            "pid": 1234,
            "direction": "outbound",
            "protocol": "https",
            "remote_host": "api.anthropic.com",
            "remote_ip": "1.2.3.4",
            "remote_port": 443,
            "local_port": 54321,
            "bytes_sent": 1024,
            "bytes_received": 4096,
            "is_known": true,
            "tags": ["ai", "api"],
            "event_type": "connection"
        }"#;

        let event: RanoEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.ts, Some("2026-01-27T10:00:00Z".to_string()));
        assert_eq!(event.id, Some(12345));
        assert_eq!(event.provider, Some("anthropic".to_string()));
        assert_eq!(event.process, Some("claude-code".to_string()));
        assert_eq!(event.pid, Some(1234));
        assert_eq!(event.direction, Some("outbound".to_string()));
        assert_eq!(event.remote_host, Some("api.anthropic.com".to_string()));
        assert_eq!(event.remote_port, Some(443));
        assert_eq!(event.bytes_sent, Some(1024));
        assert_eq!(event.bytes_received, Some(4096));
        assert_eq!(event.is_known, Some(true));
    }

    #[test]
    fn test_parse_event_minimal() {
        let json = r#"{
            "ts": "2026-01-27T10:00:00Z",
            "remote_ip": "1.2.3.4"
        }"#;

        let event: RanoEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.ts, Some("2026-01-27T10:00:00Z".to_string()));
        assert_eq!(event.remote_ip, Some("1.2.3.4".to_string()));
        assert!(event.provider.is_none());
        assert!(event.process.is_none());
        assert!(event.bytes_sent.is_none());
    }

    #[test]
    fn test_parse_event_empty() {
        let json = r#"{}"#;

        let event: RanoEvent = serde_json::from_str(json).unwrap();
        assert!(event.ts.is_none());
        assert!(event.provider.is_none());
        assert!(event.remote_host.is_none());
    }

    #[test]
    fn test_normalize_provider_anthropic() {
        assert_eq!(
            RanoCollector::normalize_provider(Some("api.anthropic.com")),
            Some("anthropic".to_string())
        );
        assert_eq!(
            RanoCollector::normalize_provider(Some("claude.ai")),
            Some("anthropic".to_string())
        );
    }

    #[test]
    fn test_normalize_provider_openai() {
        assert_eq!(
            RanoCollector::normalize_provider(Some("api.openai.com")),
            Some("openai".to_string())
        );
    }

    #[test]
    fn test_normalize_provider_github() {
        assert_eq!(
            RanoCollector::normalize_provider(Some("github.com")),
            Some("github".to_string())
        );
        assert_eq!(
            RanoCollector::normalize_provider(Some("api.github.com")),
            Some("github".to_string())
        );
    }

    #[test]
    fn test_normalize_provider_google() {
        assert_eq!(
            RanoCollector::normalize_provider(Some("googleapis.com")),
            Some("google".to_string())
        );
        assert_eq!(
            RanoCollector::normalize_provider(Some("www.google.com")),
            Some("google".to_string())
        );
    }

    #[test]
    fn test_normalize_provider_aws() {
        assert_eq!(
            RanoCollector::normalize_provider(Some("s3.amazonaws.com")),
            Some("aws".to_string())
        );
    }

    #[test]
    fn test_normalize_provider_microsoft() {
        assert_eq!(
            RanoCollector::normalize_provider(Some("azure.microsoft.com")),
            Some("microsoft".to_string())
        );
    }

    #[test]
    fn test_normalize_provider_unknown() {
        assert_eq!(
            RanoCollector::normalize_provider(Some("example.com")),
            Some("example.com".to_string())
        );
    }

    #[test]
    fn test_normalize_provider_none() {
        assert_eq!(RanoCollector::normalize_provider(None), None);
    }

    #[test]
    fn test_event_serialize() {
        let event = RanoEvent {
            ts: Some("2026-01-27T10:00:00Z".to_string()),
            id: Some(1),
            provider: Some("test".to_string()),
            process: None,
            pid: None,
            direction: Some("outbound".to_string()),
            protocol: Some("https".to_string()),
            remote_host: Some("example.com".to_string()),
            remote_ip: Some("1.2.3.4".to_string()),
            remote_port: Some(443),
            local_port: None,
            bytes_sent: Some(100),
            bytes_received: Some(200),
            is_known: Some(true),
            tags: None,
            event_type: Some("connection".to_string()),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"provider\":\"test\""));
        assert!(json.contains("\"bytes_sent\":100"));
    }
}
