//! caam collector - account manager profile and limits via the caam tool
//!
//! This collector uses the CLI Snapshot ingestion pattern to collect
//! account profiles and limits from the `caam` tool.
//!
//! ## Integration Method
//! ```bash
//! caam limits --format json    # Detailed limit info per profile
//! caam status --json           # Active profile and health status
//! ```
//!
//! ## Tables Populated
//! - `account_profile_snapshots`: Profile status and active selections

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Instant;

use crate::{
    CollectContext, CollectError, CollectOutcome, CollectResult, Collector, Cursor, RowBatch,
    Warning,
};

/// Output schema from `caam limits --format json`
#[derive(Debug, Deserialize)]
pub struct CaamLimitsOutput {
    /// List of providers with their profiles
    #[serde(default)]
    pub providers: Vec<ProviderLimits>,
}

/// Per-provider limits data
#[derive(Debug, Deserialize)]
pub struct ProviderLimits {
    /// Provider name (claude, openai, gemini)
    #[serde(default)]
    pub provider: String,

    /// Profiles for this provider
    #[serde(default)]
    pub profiles: Vec<ProfileLimits>,
}

/// Per-profile limits data
#[derive(Debug, Deserialize, Serialize)]
pub struct ProfileLimits {
    /// Profile name (usually email)
    #[serde(default)]
    pub name: String,

    /// Usage window type
    #[serde(default)]
    pub window: String,

    /// Current utilization percentage
    #[serde(default)]
    pub utilization_percent: f64,

    /// When the usage window resets
    #[serde(default)]
    pub resets_at: Option<String>,

    /// Whether this profile is currently active
    #[serde(default)]
    pub is_active: bool,

    /// Priority for automatic selection
    #[serde(default)]
    pub priority: Option<i32>,
}

/// Output schema from `caam status --json`
#[derive(Debug, Deserialize)]
pub struct CaamStatusOutput {
    /// List of tools with their active profiles
    #[serde(default)]
    pub tools: Vec<ToolStatus>,
}

/// Per-tool status data
#[derive(Debug, Deserialize, Serialize)]
pub struct ToolStatus {
    /// Tool name (claude-code, codex, etc.)
    #[serde(default)]
    pub tool: String,

    /// Currently active profile for this tool
    #[serde(default)]
    pub active_profile: Option<String>,

    /// Health score (0.0 - 1.0)
    #[serde(default)]
    pub health_score: f64,

    /// When the health score expires
    #[serde(default)]
    pub health_expires_at: Option<String>,

    /// Provider for the active profile
    #[serde(default)]
    pub provider: Option<String>,
}

/// caam collector for account profile management
///
/// Collects account profiles, limits, and active selections using
/// the `caam` tool's JSON output from multiple commands.
pub struct CaamCollector;

impl Default for CaamCollector {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl Collector for CaamCollector {
    fn name(&self) -> &'static str {
        "caam"
    }

    fn schema_version(&self) -> u32 {
        1
    }

    fn required_tool(&self) -> Option<&'static str> {
        Some("caam")
    }

    fn supports_incremental(&self) -> bool {
        false // Each collection is a point-in-time snapshot
    }

    async fn collect(&self, cx: &asupersync::Cx, ctx: &CollectContext) -> CollectOutcome {
        let start = Instant::now();
        let mut warnings = Vec::new();
        let mut profile_rows = Vec::new();
        crate::collect_checkpoint!(cx, "collect_start");

        // Check if caam is available
        if !self.check_availability(cx, ctx).await {
            return asupersync::Outcome::Err(CollectError::ToolNotFound("caam".to_string()));
        }

        // Run caam limits --format json
        crate::collect_checkpoint!(cx, "pre_caam_limits_command");
        match ctx
            .executor
            .run_timeout(cx, "caam limits --format json", ctx.timeout)
            .await
        {
            Ok(output) => {
                crate::collect_checkpoint!(cx, "post_caam_limits_command_pre_parse");
                match serde_json::from_str::<CaamLimitsOutput>(&output) {
                    Ok(limits) => {
                        for provider in &limits.providers {
                            for profile in &provider.profiles {
                                profile_rows.push(serde_json::json!({
                                    "machine_id": ctx.machine_id,
                                    "collected_at": ctx.collected_at.to_rfc3339(),
                                    "provider": provider.provider,
                                    "account_id": profile.name,
                                    "email": profile.name, // Usually same as account_id
                                    "plan_type": profile.window,
                                    "is_active": profile.is_active,
                                    "is_current": profile.is_active,
                                    "priority": profile.priority,
                                    "raw_json": serde_json::to_string(profile).unwrap_or_default(),
                                }));
                            }
                        }
                    }
                    Err(e) => {
                        warnings.push(Warning::warn(format!(
                            "Failed to parse caam limits output: {e}",
                        )));
                    }
                }
            }
            Err(e) => {
                warnings.push(Warning::warn(format!("Failed to run caam limits: {e}")));
            }
        }

        // Run caam status --json
        crate::collect_checkpoint!(cx, "pre_caam_status_command");
        match ctx
            .executor
            .run_timeout(cx, "caam status --json", ctx.timeout)
            .await
        {
            Ok(output) => {
                crate::collect_checkpoint!(cx, "post_caam_status_command_pre_parse");
                match serde_json::from_str::<CaamStatusOutput>(&output) {
                    Ok(status) => {
                        for tool in &status.tools {
                            // Only add if we have an active profile
                            if tool.active_profile.is_some() {
                                profile_rows.push(serde_json::json!({
                                    "machine_id": ctx.machine_id,
                                    "collected_at": ctx.collected_at.to_rfc3339(),
                                    "provider": tool.provider.as_deref().unwrap_or("unknown"),
                                    "account_id": tool.active_profile,
                                    "email": tool.active_profile,
                                    "plan_type": tool.tool, // Tool as context
                                    "is_active": true,
                                    "is_current": true,
                                    "priority": None::<i32>,
                                    "raw_json": serde_json::to_string(tool).unwrap_or_default(),
                                }));
                            }
                        }
                    }
                    Err(e) => {
                        warnings.push(Warning::warn(format!(
                            "Failed to parse caam status output: {e}",
                        )));
                    }
                }
            }
            Err(e) => {
                warnings.push(Warning::warn(format!("Failed to run caam status: {e}")));
            }
        }

        crate::collect_checkpoint!(cx, "post_parse_pre_return");
        let mut result = CollectResult::with_rows(vec![RowBatch {
            table: "account_profile_snapshots".to_string(),
            rows: profile_rows,
        }])
        .with_cursor(Cursor::now())
        .with_duration(start.elapsed());

        // Add any warnings
        for warning in warnings {
            result = result.with_warning(warning);
        }

        crate::collect_checkpoint!(cx, "collect_complete");
        asupersync::Outcome::Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collector_name() {
        let collector = CaamCollector;
        assert_eq!(collector.name(), "caam");
    }

    #[test]
    fn test_collector_schema_version() {
        let collector = CaamCollector;
        assert_eq!(collector.schema_version(), 1);
    }

    #[test]
    fn test_collector_not_incremental() {
        let collector = CaamCollector;
        assert!(!collector.supports_incremental());
    }

    #[test]
    fn test_collector_required_tool() {
        let collector = CaamCollector;
        assert_eq!(collector.required_tool(), Some("caam"));
    }

    #[test]
    fn test_default_impl() {
        let collector = CaamCollector;
        assert_eq!(collector.name(), "caam");
    }

    #[test]
    fn test_parse_limits_output() {
        let json = r#"{
            "providers": [
                {
                    "provider": "claude",
                    "profiles": [
                        {
                            "name": "jeff@email.com",
                            "window": "5_hour",
                            "utilization_percent": 45.0,
                            "resets_at": "2026-01-27T05:00:00Z",
                            "is_active": true,
                            "priority": 1
                        },
                        {
                            "name": "work@company.com",
                            "window": "5_hour",
                            "utilization_percent": 10.0,
                            "resets_at": "2026-01-27T05:00:00Z",
                            "is_active": false,
                            "priority": 2
                        }
                    ]
                },
                {
                    "provider": "openai",
                    "profiles": [
                        {
                            "name": "dev@company.com",
                            "window": "daily",
                            "utilization_percent": 78.0,
                            "resets_at": "2026-01-28T00:00:00Z",
                            "is_active": true
                        }
                    ]
                }
            ]
        }"#;

        let data: CaamLimitsOutput = serde_json::from_str(json).unwrap();
        assert_eq!(data.providers.len(), 2);

        let claude = &data.providers[0];
        assert_eq!(claude.provider, "claude");
        assert_eq!(claude.profiles.len(), 2);
        assert_eq!(claude.profiles[0].name, "jeff@email.com");
        assert!(claude.profiles[0].is_active);
        assert_eq!(claude.profiles[0].priority, Some(1));

        let openai = &data.providers[1];
        assert_eq!(openai.provider, "openai");
        assert_eq!(openai.profiles.len(), 1);
    }

    #[test]
    fn test_parse_status_output() {
        let json = r#"{
            "tools": [
                {
                    "tool": "claude-code",
                    "active_profile": "jeff@email.com",
                    "health_score": 0.85,
                    "health_expires_at": "2026-01-27T01:00:00Z",
                    "provider": "claude"
                },
                {
                    "tool": "codex",
                    "active_profile": "dev@company.com",
                    "health_score": 0.60,
                    "health_expires_at": "2026-01-27T02:00:00Z",
                    "provider": "openai"
                }
            ]
        }"#;

        let data: CaamStatusOutput = serde_json::from_str(json).unwrap();
        assert_eq!(data.tools.len(), 2);

        let cc = &data.tools[0];
        assert_eq!(cc.tool, "claude-code");
        assert_eq!(cc.active_profile, Some("jeff@email.com".to_string()));
        assert!((cc.health_score - 0.85).abs() < f64::EPSILON);
        assert_eq!(cc.provider, Some("claude".to_string()));

        let codex = &data.tools[1];
        assert_eq!(codex.tool, "codex");
        assert!((codex.health_score - 0.60).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_empty_providers() {
        let json = r#"{"providers": []}"#;
        let data: CaamLimitsOutput = serde_json::from_str(json).unwrap();
        assert!(data.providers.is_empty());
    }

    #[test]
    fn test_parse_empty_tools() {
        let json = r#"{"tools": []}"#;
        let data: CaamStatusOutput = serde_json::from_str(json).unwrap();
        assert!(data.tools.is_empty());
    }

    #[test]
    fn test_parse_missing_optional_fields() {
        let json = r#"{
            "providers": [
                {
                    "provider": "claude",
                    "profiles": [
                        {
                            "name": "test@example.com"
                        }
                    ]
                }
            ]
        }"#;

        let data: CaamLimitsOutput = serde_json::from_str(json).unwrap();
        let profile = &data.providers[0].profiles[0];
        assert_eq!(profile.name, "test@example.com");
        assert!(profile.utilization_percent.abs() < f64::EPSILON); // Default
        assert!(!profile.is_active); // Default false
        assert!(profile.resets_at.is_none());
        assert!(profile.priority.is_none());
    }

    #[test]
    fn test_parse_status_no_active_profile() {
        let json = r#"{
            "tools": [
                {
                    "tool": "unused-tool",
                    "health_score": 0.0
                }
            ]
        }"#;

        let data: CaamStatusOutput = serde_json::from_str(json).unwrap();
        let tool = &data.tools[0];
        assert!(tool.active_profile.is_none());
        assert!(tool.provider.is_none());
    }
}
