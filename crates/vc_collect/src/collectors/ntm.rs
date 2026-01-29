//! ntm collector - Named Tmux Manager metrics
//!
//! This collector uses the CLI Snapshot pattern to collect
//! tmux session and agent state from ntm (Named Tmux Manager).
//!
//! ## Integration Method
//! Shell out to `ntm --robot-status` for JSON output
//!
//! ## Tables Populated
//! - `ntm_sessions_snapshot`: Per-session state (windows, panes, agents)
//! - `ntm_activity_snapshot`: Aggregated activity metrics
//! - `ntm_agent_snapshot`: Per-agent details and metrics

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Instant;

use crate::{CollectContext, CollectError, CollectResult, Collector, Cursor, RowBatch, Warning};

/// System info from ntm status
#[derive(Debug, Deserialize)]
pub struct NtmSystem {
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub commit: Option<String>,
    #[serde(default)]
    pub build_date: Option<String>,
    #[serde(default)]
    pub go_version: Option<String>,
    #[serde(default)]
    pub os: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
    #[serde(default)]
    pub tmux_available: bool,
}

/// Agent info within a session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NtmAgent {
    #[serde(rename = "type", default)]
    pub agent_type: String,
    #[serde(default)]
    pub pane: String,
    #[serde(default)]
    pub window: i32,
    #[serde(default)]
    pub pane_idx: i32,
    #[serde(default)]
    pub is_active: bool,
    #[serde(default)]
    pub pid: Option<i64>,
    #[serde(default)]
    pub last_output_ts: Option<String>,
    #[serde(default)]
    pub process_state: Option<String>,
    #[serde(default)]
    pub process_state_name: Option<String>,
    #[serde(default)]
    pub memory_mb: Option<i64>,
    #[serde(default)]
    pub output_lines_since_last: i32,
    #[serde(default)]
    pub context_tokens: Option<i64>,
    #[serde(default)]
    pub context_limit: Option<i64>,
    #[serde(default)]
    pub context_percent: Option<f64>,
    #[serde(default)]
    pub context_model: Option<String>,
}

/// Session info from ntm status
#[derive(Debug, Serialize, Deserialize)]
pub struct NtmSession {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub exists: bool,
    #[serde(default)]
    pub attached: bool,
    #[serde(default)]
    pub windows: i32,
    #[serde(default)]
    pub panes: i32,
    #[serde(default)]
    pub agents: Vec<NtmAgent>,
}

/// Summary stats from ntm status
#[derive(Debug, Default, Deserialize)]
pub struct NtmSummary {
    #[serde(default)]
    pub total_sessions: i32,
    #[serde(default)]
    pub total_agents: i32,
    #[serde(default)]
    pub attached_count: i32,
    #[serde(default)]
    pub claude_count: i32,
    #[serde(default)]
    pub codex_count: i32,
    #[serde(default)]
    pub gemini_count: i32,
    #[serde(default)]
    pub idle_count: i32,
    #[serde(default)]
    pub busy_count: i32,
    #[serde(default)]
    pub error_count: i32,
}

/// Full output from `ntm --robot-status`
#[derive(Debug, Deserialize)]
pub struct NtmStatusOutput {
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub timestamp: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub output_format: Option<String>,
    #[serde(default)]
    pub generated_at: Option<String>,
    #[serde(default)]
    pub system: Option<NtmSystem>,
    #[serde(default)]
    pub sessions: Vec<NtmSession>,
    #[serde(default)]
    pub summary: Option<NtmSummary>,
}

/// ntm collector for tmux session and agent state
///
/// Collects session and agent metrics from ntm using the
/// CLI Snapshot pattern (stateless snapshot each poll).
pub struct NtmCollector;

impl NtmCollector {
    /// Create a new ntm collector
    pub fn new() -> Self {
        Self
    }
}

impl Default for NtmCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Collector for NtmCollector {
    fn name(&self) -> &'static str {
        "ntm"
    }

    fn schema_version(&self) -> u32 {
        1
    }

    fn required_tool(&self) -> Option<&'static str> {
        Some("ntm")
    }

    fn supports_incremental(&self) -> bool {
        false // Stateless - each poll is a fresh snapshot
    }

    async fn collect(&self, ctx: &CollectContext) -> Result<CollectResult, CollectError> {
        let start = Instant::now();
        let mut warnings = Vec::new();

        // Check if ntm is available
        if !self.check_availability(ctx).await {
            return Err(CollectError::ToolNotFound("ntm".to_string()));
        }

        // Run ntm --robot-status to get session state
        let status_result = ctx
            .executor
            .run_timeout("ntm --robot-status", ctx.timeout)
            .await;

        let output = match status_result {
            Ok(out) => out,
            Err(e) => {
                warnings.push(Warning::error(format!(
                    "Failed to run ntm --robot-status: {}",
                    e
                )));
                return Ok(CollectResult::empty()
                    .with_warning(Warning::error(format!("ntm command failed: {}", e)))
                    .with_duration(start.elapsed()));
            }
        };

        // Parse JSON output
        let status: NtmStatusOutput = match serde_json::from_str(&output) {
            Ok(s) => s,
            Err(e) => {
                warnings.push(
                    Warning::error(format!("Failed to parse ntm output: {}", e))
                        .with_context(output.chars().take(500).collect::<String>()),
                );
                return Ok(CollectResult::empty()
                    .with_warning(Warning::error(format!("JSON parse error: {}", e)))
                    .with_duration(start.elapsed()));
            }
        };

        // Check if ntm reported success
        if !status.success {
            warnings.push(Warning::warn("ntm --robot-status reported success=false"));
        }

        let mut batches = Vec::new();

        // Build session snapshot rows
        let session_rows: Vec<serde_json::Value> = status
            .sessions
            .iter()
            .map(|session| {
                serde_json::json!({
                    "machine_id": ctx.machine_id,
                    "collected_at": ctx.collected_at.to_rfc3339(),
                    "session_name": session.name,
                    "exists": session.exists,
                    "attached": session.attached,
                    "windows": session.windows,
                    "panes": session.panes,
                    "agent_count": session.agents.len(),
                    "agents_json": serde_json::to_string(&session.agents).unwrap_or_default(),
                    "raw_json": serde_json::to_string(&session).unwrap_or_default(),
                })
            })
            .collect();

        if !session_rows.is_empty() {
            batches.push(RowBatch {
                table: "ntm_sessions_snapshot".to_string(),
                rows: session_rows,
            });
        }

        // Build agent detail snapshot rows
        let mut agent_rows = Vec::new();
        for session in &status.sessions {
            for agent in &session.agents {
                agent_rows.push(serde_json::json!({
                    "machine_id": ctx.machine_id,
                    "collected_at": ctx.collected_at.to_rfc3339(),
                    "session_name": session.name,
                    "pane_id": agent.pane,
                    "agent_type": agent.agent_type,
                    "window_idx": agent.window,
                    "pane_idx": agent.pane_idx,
                    "is_active": agent.is_active,
                    "pid": agent.pid,
                    "process_state": agent.process_state,
                    "process_state_name": agent.process_state_name,
                    "memory_mb": agent.memory_mb,
                    "context_tokens": agent.context_tokens,
                    "context_limit": agent.context_limit,
                    "context_percent": agent.context_percent,
                    "context_model": agent.context_model,
                    "last_output_ts": agent.last_output_ts,
                    "output_lines_since_last": agent.output_lines_since_last,
                    "raw_json": serde_json::to_string(&agent).unwrap_or_default(),
                }));
            }
        }

        if !agent_rows.is_empty() {
            batches.push(RowBatch {
                table: "ntm_agent_snapshot".to_string(),
                rows: agent_rows,
            });
        }

        // Build activity summary snapshot
        let summary = status.summary.unwrap_or_default();
        let by_type = serde_json::json!({
            "claude": summary.claude_count,
            "codex": summary.codex_count,
            "gemini": summary.gemini_count,
        });
        let by_state = serde_json::json!({
            "idle": summary.idle_count,
            "busy": summary.busy_count,
            "error": summary.error_count,
        });

        let activity_row = serde_json::json!({
            "machine_id": ctx.machine_id,
            "collected_at": ctx.collected_at.to_rfc3339(),
            "total_sessions": summary.total_sessions,
            "total_agents": summary.total_agents,
            "attached_count": summary.attached_count,
            "claude_count": summary.claude_count,
            "codex_count": summary.codex_count,
            "gemini_count": summary.gemini_count,
            "idle_count": summary.idle_count,
            "busy_count": summary.busy_count,
            "error_count": summary.error_count,
            "by_type_json": serde_json::to_string(&by_type).unwrap_or_default(),
            "by_state_json": serde_json::to_string(&by_state).unwrap_or_default(),
            "raw_json": output,
        });

        batches.push(RowBatch {
            table: "ntm_activity_snapshot".to_string(),
            rows: vec![activity_row],
        });

        // Build result
        let mut result = CollectResult::with_rows(batches)
            .with_cursor(Cursor::now())
            .with_duration(start.elapsed());

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
        let collector = NtmCollector::new();
        assert_eq!(collector.name(), "ntm");
    }

    #[test]
    fn test_collector_schema_version() {
        let collector = NtmCollector::new();
        assert_eq!(collector.schema_version(), 1);
    }

    #[test]
    fn test_collector_supports_incremental() {
        let collector = NtmCollector::new();
        assert!(!collector.supports_incremental());
    }

    #[test]
    fn test_collector_required_tool() {
        let collector = NtmCollector::new();
        assert_eq!(collector.required_tool(), Some("ntm"));
    }

    #[test]
    fn test_default_impl() {
        let collector = NtmCollector::default();
        assert_eq!(collector.name(), "ntm");
    }

    #[test]
    fn test_parse_status_full() {
        let json = r#"{
            "success": true,
            "timestamp": "2026-01-29T06:58:30Z",
            "version": "1.0.0",
            "output_format": "json",
            "generated_at": "2026-01-29T06:58:30.93163411Z",
            "system": {
                "version": "dev",
                "commit": "none",
                "build_date": "unknown",
                "go_version": "go1.25.0",
                "os": "linux",
                "arch": "amd64",
                "tmux_available": true
            },
            "sessions": [
                {
                    "name": "test-session",
                    "exists": true,
                    "attached": true,
                    "windows": 1,
                    "panes": 2,
                    "agents": [
                        {
                            "type": "claude",
                            "pane": "%23",
                            "window": 0,
                            "pane_idx": 1,
                            "is_active": false,
                            "pid": 12345,
                            "last_output_ts": "2026-01-29T01:58:32.398930306-05:00",
                            "process_state": "S",
                            "process_state_name": "sleeping",
                            "memory_mb": 12,
                            "output_lines_since_last": 2,
                            "context_tokens": 122,
                            "context_limit": 200000,
                            "context_percent": 0.061,
                            "context_model": "claude-opus-4-5-20251101"
                        }
                    ]
                }
            ],
            "summary": {
                "total_sessions": 1,
                "total_agents": 1,
                "attached_count": 1,
                "claude_count": 1,
                "codex_count": 0,
                "gemini_count": 0,
                "idle_count": 0,
                "busy_count": 1,
                "error_count": 0
            }
        }"#;

        let status: NtmStatusOutput = serde_json::from_str(json).unwrap();
        assert!(status.success);
        assert_eq!(status.sessions.len(), 1);
        assert_eq!(status.sessions[0].name, "test-session");
        assert!(status.sessions[0].attached);
        assert_eq!(status.sessions[0].agents.len(), 1);

        let agent = &status.sessions[0].agents[0];
        assert_eq!(agent.agent_type, "claude");
        assert_eq!(agent.context_tokens, Some(122));
        assert_eq!(agent.context_limit, Some(200000));

        let summary = status.summary.unwrap();
        assert_eq!(summary.total_sessions, 1);
        assert_eq!(summary.claude_count, 1);
    }

    #[test]
    fn test_parse_status_minimal() {
        let json = r#"{
            "success": true,
            "sessions": [],
            "summary": {
                "total_sessions": 0,
                "total_agents": 0
            }
        }"#;

        let status: NtmStatusOutput = serde_json::from_str(json).unwrap();
        assert!(status.success);
        assert!(status.sessions.is_empty());
    }

    #[test]
    fn test_parse_status_with_defaults() {
        let json = r#"{
            "sessions": [
                {
                    "name": "minimal",
                    "agents": []
                }
            ]
        }"#;

        let status: NtmStatusOutput = serde_json::from_str(json).unwrap();
        // success defaults to false
        assert!(!status.success);
        assert_eq!(status.sessions.len(), 1);
        assert_eq!(status.sessions[0].name, "minimal");
        // exists defaults to false
        assert!(!status.sessions[0].exists);
    }

    #[test]
    fn test_parse_agent_minimal() {
        let json = r#"{
            "type": "unknown",
            "pane": "%100"
        }"#;

        let agent: NtmAgent = serde_json::from_str(json).unwrap();
        assert_eq!(agent.agent_type, "unknown");
        assert_eq!(agent.pane, "%100");
        assert!(!agent.is_active);
        assert_eq!(agent.window, 0);
        assert_eq!(agent.pane_idx, 0);
        assert!(agent.pid.is_none());
    }

    #[test]
    fn test_parse_agent_with_context() {
        let json = r#"{
            "type": "claude",
            "pane": "%50",
            "window": 0,
            "pane_idx": 2,
            "is_active": true,
            "pid": 99999,
            "context_tokens": 50000,
            "context_limit": 200000,
            "context_percent": 25.0,
            "context_model": "claude-sonnet-4-20250514"
        }"#;

        let agent: NtmAgent = serde_json::from_str(json).unwrap();
        assert_eq!(agent.agent_type, "claude");
        assert!(agent.is_active);
        assert_eq!(agent.pid, Some(99999));
        assert_eq!(agent.context_tokens, Some(50000));
        assert_eq!(agent.context_percent, Some(25.0));
        assert_eq!(
            agent.context_model,
            Some("claude-sonnet-4-20250514".to_string())
        );
    }

    #[test]
    fn test_parse_system_info() {
        let json = r#"{
            "version": "1.2.3",
            "commit": "abc123",
            "build_date": "2026-01-15",
            "go_version": "go1.25.0",
            "os": "darwin",
            "arch": "arm64",
            "tmux_available": true
        }"#;

        let system: NtmSystem = serde_json::from_str(json).unwrap();
        assert_eq!(system.version, Some("1.2.3".to_string()));
        assert_eq!(system.os, Some("darwin".to_string()));
        assert_eq!(system.arch, Some("arm64".to_string()));
        assert!(system.tmux_available);
    }

    #[test]
    fn test_parse_summary() {
        let json = r#"{
            "total_sessions": 5,
            "total_agents": 12,
            "attached_count": 2,
            "claude_count": 6,
            "codex_count": 4,
            "gemini_count": 2,
            "idle_count": 3,
            "busy_count": 8,
            "error_count": 1
        }"#;

        let summary: NtmSummary = serde_json::from_str(json).unwrap();
        assert_eq!(summary.total_sessions, 5);
        assert_eq!(summary.total_agents, 12);
        assert_eq!(summary.claude_count, 6);
        assert_eq!(summary.codex_count, 4);
        assert_eq!(summary.gemini_count, 2);
        assert_eq!(summary.busy_count, 8);
    }

    #[test]
    fn test_agent_clone() {
        let agent = NtmAgent {
            agent_type: "claude".to_string(),
            pane: "%1".to_string(),
            window: 0,
            pane_idx: 1,
            is_active: true,
            pid: Some(12345),
            last_output_ts: None,
            process_state: Some("S".to_string()),
            process_state_name: Some("sleeping".to_string()),
            memory_mb: Some(100),
            output_lines_since_last: 5,
            context_tokens: Some(1000),
            context_limit: Some(200000),
            context_percent: Some(0.5),
            context_model: Some("claude-opus-4-5".to_string()),
        };

        let cloned = agent.clone();
        assert_eq!(cloned.agent_type, agent.agent_type);
        assert_eq!(cloned.pane, agent.pane);
        assert_eq!(cloned.pid, agent.pid);
    }
}
