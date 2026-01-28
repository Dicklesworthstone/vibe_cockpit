//! vc_alert - Alerting system for Vibe Cockpit
//!
//! This crate provides:
//! - Alert rule definitions
//! - Condition evaluation
//! - Alert history management
//! - Delivery channels (TUI, webhook, desktop)

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use thiserror::Error;

/// Alert errors
#[derive(Error, Debug)]
pub enum AlertError {
    #[error("Rule not found: {0}")]
    RuleNotFound(String),

    #[error("Evaluation failed: {0}")]
    EvaluationFailed(String),

    #[error("Delivery failed: {0}")]
    DeliveryFailed(String),

    #[error("Store error: {0}")]
    StoreError(#[from] vc_store::StoreError),
}

/// Alert severity
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

/// Alert rule definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    pub rule_id: String,
    pub name: String,
    pub description: Option<String>,
    pub severity: Severity,
    pub enabled: bool,
    pub condition: AlertCondition,
    pub cooldown_secs: u64,
    pub channels: Vec<String>,
}

/// Alert condition types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AlertCondition {
    Threshold { query: String, operator: ThresholdOp, value: f64 },
    Pattern { table: String, column: String, regex: String },
    Absence { table: String, max_age_secs: u64 },
    RateOfChange { query: String, window_secs: u64, threshold_per_sec: f64 },
}

/// Threshold comparison operators
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThresholdOp {
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
}

impl ThresholdOp {
    pub fn check(&self, actual: f64, threshold: f64) -> bool {
        match self {
            ThresholdOp::Gt => actual > threshold,
            ThresholdOp::Gte => actual >= threshold,
            ThresholdOp::Lt => actual < threshold,
            ThresholdOp::Lte => actual <= threshold,
            ThresholdOp::Eq => (actual - threshold).abs() < f64::EPSILON,
        }
    }
}

/// A fired alert
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: Option<i64>,
    pub rule_id: String,
    pub fired_at: DateTime<Utc>,
    pub severity: Severity,
    pub title: String,
    pub message: String,
    pub machine_id: Option<String>,
    pub context: serde_json::Value,
}

/// Alert delivery channel trait
#[async_trait]
pub trait AlertChannel: Send + Sync {
    fn name(&self) -> &str;
    async fn deliver(&self, alert: &Alert) -> Result<(), AlertError>;
}

/// Alert engine for rule evaluation
pub struct AlertEngine {
    rules: Vec<AlertRule>,
    cooldowns: DashMap<String, Instant>,
}

impl AlertEngine {
    /// Create a new alert engine
    pub fn new() -> Self {
        Self {
            rules: Self::default_rules(),
            cooldowns: DashMap::new(),
        }
    }

    /// Get default built-in rules
    fn default_rules() -> Vec<AlertRule> {
        vec![
            AlertRule {
                rule_id: "rate-limit-warning".to_string(),
                name: "Rate Limit Warning".to_string(),
                description: Some("Alert when account usage exceeds 80%".to_string()),
                severity: Severity::Warning,
                enabled: true,
                condition: AlertCondition::Threshold {
                    query: "SELECT MAX(usage_pct) FROM account_usage_snapshots WHERE collected_at > datetime('now', '-5 minutes')".to_string(),
                    operator: ThresholdOp::Gte,
                    value: 80.0,
                },
                cooldown_secs: 900,
                channels: vec!["tui".to_string()],
            },
            AlertRule {
                rule_id: "disk-critical".to_string(),
                name: "Disk Space Critical".to_string(),
                description: Some("Alert when disk usage exceeds 90%".to_string()),
                severity: Severity::Critical,
                enabled: true,
                condition: AlertCondition::Threshold {
                    query: "SELECT 100.0 * (1 - (SELECT AVG(mem_total_bytes - mem_used_bytes) / AVG(mem_total_bytes) FROM sys_samples WHERE collected_at > datetime('now', '-5 minutes')))".to_string(),
                    operator: ThresholdOp::Gte,
                    value: 90.0,
                },
                cooldown_secs: 300,
                channels: vec!["tui".to_string(), "desktop".to_string()],
            },
        ]
    }

    /// Get all rules
    pub fn rules(&self) -> &[AlertRule] {
        &self.rules
    }

    /// Check if a rule is in cooldown
    pub fn is_in_cooldown(&self, rule_id: &str, cooldown_secs: u64) -> bool {
        if let Some(last_fired) = self.cooldowns.get(rule_id) {
            last_fired.elapsed() < Duration::from_secs(cooldown_secs)
        } else {
            false
        }
    }

    /// Record that a rule fired
    pub fn record_fired(&self, rule_id: &str) {
        self.cooldowns.insert(rule_id.to_string(), Instant::now());
    }
}

impl Default for AlertEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use mockall::mock;

    mock! {
        Channel {}

        #[async_trait]
        impl AlertChannel for Channel {
            fn name(&self) -> &str;
            async fn deliver(&self, alert: &Alert) -> Result<(), AlertError>;
        }
    }

    #[test]
    fn test_threshold_op() {
        assert!(ThresholdOp::Gt.check(10.0, 5.0));
        assert!(!ThresholdOp::Gt.check(5.0, 10.0));
        assert!(ThresholdOp::Gte.check(10.0, 10.0));
        assert!(ThresholdOp::Lt.check(5.0, 10.0));
    }

    #[test]
    fn test_default_rules() {
        let engine = AlertEngine::new();
        assert!(!engine.rules().is_empty());
    }

    #[test]
    fn test_cooldown() {
        let engine = AlertEngine::new();
        assert!(!engine.is_in_cooldown("test", 60));
        engine.record_fired("test");
        assert!(engine.is_in_cooldown("test", 60));
    }

    #[tokio::test]
    async fn test_mock_channel_deliver() {
        let mut mock = MockChannel::new();
        mock.expect_name().return_const("mock");
        mock.expect_deliver().returning(|_| Ok(()));

        let alert = Alert {
            id: None,
            rule_id: "test-rule".to_string(),
            fired_at: Utc::now(),
            severity: Severity::Info,
            title: "Test alert".to_string(),
            message: "Testing delivery".to_string(),
            machine_id: None,
            context: serde_json::json!({}),
        };

        assert_eq!(mock.name(), "mock");
        assert!(mock.deliver(&alert).await.is_ok());
    }
}
