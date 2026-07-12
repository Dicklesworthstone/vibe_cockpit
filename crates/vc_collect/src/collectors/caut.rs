//! caut collector - account usage tracking via the caut tool
//!
//! This collector uses the CLI Snapshot ingestion pattern to collect
//! account usage statistics from the `caut` tool.
//!
//! ## Integration Method
//! ```bash
//! caut usage --json
//! ```
//!
//! ## Tables Populated
//! - `account_usage_snapshots`: Usage percentages and reset times per account

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Instant;

use crate::{
    CollectContext, CollectError, CollectOutcome, CollectResult, Collector, Cursor, RowBatch,
    Warning,
};

/// Output schema from `caut usage --json`
#[derive(Debug, Deserialize)]
pub struct CautUsageOutput {
    /// List of account usage data
    #[serde(default)]
    pub accounts: Vec<AccountUsage>,
}

/// Per-account usage data
#[derive(Debug, Deserialize, Serialize)]
pub struct AccountUsage {
    /// Provider name (claude, openai, gemini)
    #[serde(default)]
    pub provider: String,

    /// Account identifier (email or account ID)
    #[serde(default)]
    pub account: String,

    /// Usage window type (`5_hour`, daily, monthly)
    #[serde(default)]
    pub window: String,

    /// Percentage of usage consumed
    #[serde(default)]
    pub used_percent: f64,

    /// Percentage of usage remaining
    #[serde(default)]
    pub remaining_percent: f64,

    /// When the usage window resets
    #[serde(default)]
    pub resets_at: Option<String>,

    /// Credits/dollars remaining (if applicable)
    #[serde(default)]
    pub credits_remaining: Option<f64>,

    /// Usage status (healthy, warning, critical, exhausted)
    #[serde(default)]
    pub status: String,

    /// Tokens used in current window
    #[serde(default)]
    pub tokens_used: Option<i64>,

    /// Token limit for current window
    #[serde(default)]
    pub tokens_limit: Option<i64>,
}

/// caut collector for account usage tracking
///
/// Collects per-account usage percentages, reset times, and status
/// using the `caut` tool's JSON output.
pub struct CautCollector;

impl Default for CautCollector {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl Collector for CautCollector {
    fn name(&self) -> &'static str {
        "caut"
    }

    fn schema_version(&self) -> u32 {
        1
    }

    fn required_tool(&self) -> Option<&'static str> {
        Some("caut")
    }

    fn supports_incremental(&self) -> bool {
        false // Each collection is a point-in-time snapshot
    }

    async fn collect(&self, cx: &asupersync::Cx, ctx: &CollectContext) -> CollectOutcome {
        let start = Instant::now();
        let mut warnings = Vec::new();
        crate::collect_checkpoint!(cx, "collect_start");

        // Check if caut is available
        if !self.check_availability(cx, ctx).await {
            return asupersync::Outcome::Err(CollectError::ToolNotFound("caut".to_string()));
        }

        // Run caut usage --json
        crate::collect_checkpoint!(cx, "pre_caut_usage_command");
        let output = crate::collect_try!(
            ctx.executor
                .run_timeout(cx, "caut usage --json", ctx.timeout)
                .await
        );

        // Parse the JSON output
        crate::collect_checkpoint!(cx, "post_caut_usage_command_pre_parse");
        let data: CautUsageOutput = match serde_json::from_str(&output) {
            Ok(d) => d,
            Err(e) => {
                // Try to continue with empty data if parse fails
                warnings.push(Warning::warn(format!("Failed to parse caut output: {e}")));
                CautUsageOutput { accounts: vec![] }
            }
        };

        // Build rows for account_usage_snapshots table
        let rows: Vec<_> = data
            .accounts
            .iter()
            .map(|a| {
                serde_json::json!({
                    "machine_id": ctx.machine_id,
                    "collected_at": ctx.collected_at.to_rfc3339(),
                    "provider": a.provider,
                    "account_id": a.account,
                    "usage_pct": a.used_percent,
                    "tokens_used": a.tokens_used,
                    "tokens_limit": a.tokens_limit,
                    "resets_at": a.resets_at,
                    "cost_estimate": a.credits_remaining,
                    "raw_json": serde_json::to_string(a).unwrap_or_default(),
                })
            })
            .collect();

        crate::collect_checkpoint!(cx, "post_parse_pre_return");
        let mut result = CollectResult::with_rows(vec![RowBatch {
            table: "account_usage_snapshots".to_string(),
            rows,
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
        let collector = CautCollector;
        assert_eq!(collector.name(), "caut");
    }

    #[test]
    fn test_collector_schema_version() {
        let collector = CautCollector;
        assert_eq!(collector.schema_version(), 1);
    }

    #[test]
    fn test_collector_not_incremental() {
        let collector = CautCollector;
        assert!(!collector.supports_incremental());
    }

    #[test]
    fn test_collector_required_tool() {
        let collector = CautCollector;
        assert_eq!(collector.required_tool(), Some("caut"));
    }

    #[test]
    fn test_default_impl() {
        let collector = CautCollector;
        assert_eq!(collector.name(), "caut");
    }

    #[test]
    fn test_parse_valid_output() {
        let json = r#"{
            "accounts": [
                {
                    "provider": "claude",
                    "account": "jeff@email.com",
                    "window": "5_hour",
                    "used_percent": 45.0,
                    "remaining_percent": 55.0,
                    "resets_at": "2026-01-27T05:00:00Z",
                    "credits_remaining": null,
                    "status": "healthy"
                },
                {
                    "provider": "openai",
                    "account": "dev@company.com",
                    "window": "daily",
                    "used_percent": 78.0,
                    "remaining_percent": 22.0,
                    "resets_at": "2026-01-28T00:00:00Z",
                    "credits_remaining": 85.50,
                    "status": "warning",
                    "tokens_used": 125000,
                    "tokens_limit": 500000
                }
            ]
        }"#;

        let data: CautUsageOutput = serde_json::from_str(json).unwrap();
        assert_eq!(data.accounts.len(), 2);

        let claude = &data.accounts[0];
        assert_eq!(claude.provider, "claude");
        assert_eq!(claude.account, "jeff@email.com");
        assert!((claude.used_percent - 45.0).abs() < f64::EPSILON);
        assert_eq!(claude.status, "healthy");

        let openai = &data.accounts[1];
        assert_eq!(openai.provider, "openai");
        assert!((openai.used_percent - 78.0).abs() < f64::EPSILON);
        assert_eq!(openai.credits_remaining, Some(85.50));
        assert_eq!(openai.tokens_used, Some(125_000));
        assert_eq!(openai.tokens_limit, Some(500_000));
    }

    #[test]
    fn test_parse_empty_accounts() {
        let json = r#"{"accounts": []}"#;
        let data: CautUsageOutput = serde_json::from_str(json).unwrap();
        assert!(data.accounts.is_empty());
    }

    #[test]
    fn test_parse_missing_fields() {
        let json = r#"{
            "accounts": [
                {
                    "provider": "claude",
                    "account": "test@example.com"
                }
            ]
        }"#;

        let data: CautUsageOutput = serde_json::from_str(json).unwrap();
        assert_eq!(data.accounts.len(), 1);

        let account = &data.accounts[0];
        assert_eq!(account.provider, "claude");
        assert!(account.used_percent.abs() < f64::EPSILON); // Default
        assert!(account.tokens_used.is_none());
        assert!(account.resets_at.is_none());
    }

    #[test]
    fn test_parse_no_accounts_key() {
        let json = r"{}";
        let data: CautUsageOutput = serde_json::from_str(json).unwrap();
        assert!(data.accounts.is_empty());
    }
}
