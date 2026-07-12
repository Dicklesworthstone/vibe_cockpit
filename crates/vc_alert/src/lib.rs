//! `vc_alert` - Alerting system for Vibe Cockpit
//!
//! This crate provides:
//! - Alert rule definitions
//! - Condition evaluation
//! - Alert history management
//! - Delivery channels (TUI, webhook, desktop)
//! - Alert routing, escalation, and suppression

pub mod routing;

use asupersync::Cx;
use asupersync::channel::mpsc;
use asupersync::process::Command as ProcessCommand;
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
    Threshold {
        query: String,
        operator: ThresholdOp,
        value: f64,
    },
    Pattern {
        table: String,
        column: String,
        regex: String,
    },
    Absence {
        table: String,
        max_age_secs: u64,
    },
    RateOfChange {
        query: String,
        window_secs: u64,
        threshold_per_sec: f64,
    },
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
    #[must_use]
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
    fn name(&self) -> &'static str;
    async fn deliver(&self, cx: &Cx, alert: &Alert) -> Result<(), AlertError>;
}

/// Alert engine for rule evaluation
pub struct AlertEngine {
    rules: Vec<AlertRule>,
    cooldowns: DashMap<String, Instant>,
}

impl AlertEngine {
    /// Create a new alert engine
    #[must_use]
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
                    query: "SELECT MAX(usage_pct) FROM account_usage_snapshots WHERE CAST(collected_at AS TIMESTAMP) > current_timestamp - INTERVAL '5 minutes'".to_string(),
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
                    // Extract max disk usage pct from the first mount point in disk_usage_json
                    // JSON format: [{mount, total, used, avail, pct}, ...]
                    query: "SELECT COALESCE(CAST(json_extract(disk_usage_json, '$[0].pct') AS DOUBLE), 0.0) FROM sys_fallback_samples WHERE CAST(collected_at AS TIMESTAMP) > current_timestamp - INTERVAL '5 minutes' AND disk_usage_json IS NOT NULL ORDER BY collected_at DESC LIMIT 1".to_string(),
                    operator: ThresholdOp::Gte,
                    value: 90.0,
                },
                cooldown_secs: 300,
                channels: vec!["tui".to_string(), "desktop".to_string()],
            },
            AlertRule {
                rule_id: "dcg-critical-block".to_string(),
                name: "Critical Command Blocked".to_string(),
                description: Some("Alert when dcg blocks a critical severity command".to_string()),
                severity: Severity::Critical,
                enabled: true,
                condition: AlertCondition::Pattern {
                    table: "dcg_events".to_string(),
                    column: "severity".to_string(),
                    regex: "critical".to_string(),
                },
                cooldown_secs: 60,
                channels: vec!["tui".to_string()],
            },
            AlertRule {
                rule_id: "agent-stuck".to_string(),
                name: "Agent Appears Stuck".to_string(),
                description: Some("Alert when no agent activity for 10 minutes".to_string()),
                severity: Severity::Warning,
                enabled: true,
                condition: AlertCondition::Absence {
                    table: "caut_snapshots".to_string(),
                    max_age_secs: 600,
                },
                cooldown_secs: 600,
                channels: vec!["tui".to_string()],
            },
            AlertRule {
                rule_id: "rch-queue-pressure".to_string(),
                name: "Remote Compilation Queue Pressure".to_string(),
                description: Some("Alert when rch queue depth exceeds threshold".to_string()),
                severity: Severity::Warning,
                enabled: true,
                condition: AlertCondition::Threshold {
                    query: "SELECT queue_depth FROM rch_metrics ORDER BY collected_at DESC LIMIT 1".to_string(),
                    operator: ThresholdOp::Gte,
                    value: 10.0,
                },
                cooldown_secs: 300,
                channels: vec!["tui".to_string()],
            },
            AlertRule {
                rule_id: "memory-critical".to_string(),
                name: "Memory Usage Critical".to_string(),
                description: Some("Alert when memory usage exceeds 95%".to_string()),
                severity: Severity::Critical,
                enabled: true,
                condition: AlertCondition::Threshold {
                    query: "SELECT 100.0 * (1 - CAST(mem_available_bytes AS DOUBLE) / CAST(mem_total_bytes AS DOUBLE)) FROM sys_fallback_samples WHERE CAST(collected_at AS TIMESTAMP) > current_timestamp - INTERVAL '5 minutes' AND mem_total_bytes > 0 ORDER BY collected_at DESC LIMIT 1".to_string(),
                    operator: ThresholdOp::Gte,
                    value: 95.0,
                },
                cooldown_secs: 120,
                channels: vec!["tui".to_string(), "desktop".to_string()],
            },
        ]
    }

    /// Get all rules
    #[must_use]
    pub fn rules(&self) -> &[AlertRule] {
        &self.rules
    }

    /// Check if a rule is in cooldown
    #[must_use]
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

// =============================================================================
// Channel Implementations
// =============================================================================

/// TUI channel - sends alerts to the terminal UI via mpsc
pub struct TuiChannel {
    tx: mpsc::Sender<Alert>,
}

impl TuiChannel {
    /// Create a new TUI channel with the given sender
    #[must_use]
    pub fn new(tx: mpsc::Sender<Alert>) -> Self {
        Self { tx }
    }
}

#[async_trait]
impl AlertChannel for TuiChannel {
    fn name(&self) -> &'static str {
        "tui"
    }

    async fn deliver(&self, cx: &Cx, alert: &Alert) -> Result<(), AlertError> {
        self.tx
            .send(cx, alert.clone())
            .await
            .map_err(|e| AlertError::DeliveryFailed(format!("TUI channel send failed: {e}")))
    }
}

/// Webhook channel - sends alerts via HTTP POST
pub struct WebhookChannel {
    url: String,
    client: reqwest::Client,
}

impl WebhookChannel {
    /// Create a new webhook channel
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            client: reqwest::Client::new(),
        }
    }

    /// Create with a custom client
    pub fn with_client(url: impl Into<String>, client: reqwest::Client) -> Self {
        Self {
            url: url.into(),
            client,
        }
    }
}

#[async_trait]
impl AlertChannel for WebhookChannel {
    fn name(&self) -> &'static str {
        "webhook"
    }

    async fn deliver(&self, _cx: &Cx, alert: &Alert) -> Result<(), AlertError> {
        let response = self
            .client
            .post(&self.url)
            .json(alert)
            .send()
            .await
            .map_err(|e| AlertError::DeliveryFailed(format!("Webhook request failed: {e}")))?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(AlertError::DeliveryFailed(format!(
                "Webhook returned error status: {}",
                response.status()
            )))
        }
    }
}

/// Log channel - writes alerts to tracing logs (useful for debugging/testing)
pub struct LogChannel {
    level: tracing::Level,
}

impl LogChannel {
    /// Create a log channel at the specified level
    #[must_use]
    pub fn new(level: tracing::Level) -> Self {
        Self { level }
    }

    /// Create a warning-level log channel
    #[must_use]
    pub fn warning() -> Self {
        Self::new(tracing::Level::WARN)
    }

    /// Create an info-level log channel
    #[must_use]
    pub fn info() -> Self {
        Self::new(tracing::Level::INFO)
    }
}

impl Default for LogChannel {
    fn default() -> Self {
        Self::warning()
    }
}

#[async_trait]
impl AlertChannel for LogChannel {
    fn name(&self) -> &'static str {
        "log"
    }

    async fn deliver(&self, _cx: &Cx, alert: &Alert) -> Result<(), AlertError> {
        match self.level {
            tracing::Level::ERROR => {
                tracing::error!(
                    rule_id = %alert.rule_id,
                    severity = ?alert.severity,
                    title = %alert.title,
                    message = %alert.message,
                    "Alert fired"
                );
            }
            tracing::Level::WARN => {
                tracing::warn!(
                    rule_id = %alert.rule_id,
                    severity = ?alert.severity,
                    title = %alert.title,
                    message = %alert.message,
                    "Alert fired"
                );
            }
            _ => {
                tracing::info!(
                    rule_id = %alert.rule_id,
                    severity = ?alert.severity,
                    title = %alert.title,
                    message = %alert.message,
                    "Alert fired"
                );
            }
        }
        Ok(())
    }
}

/// In-memory channel for testing - stores alerts in a Vec
#[derive(Default)]
pub struct MemoryChannel {
    alerts: std::sync::Arc<std::sync::Mutex<Vec<Alert>>>,
}

impl MemoryChannel {
    /// Create a new memory channel
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Get all alerts that have been delivered
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned.
    #[must_use]
    pub fn alerts(&self) -> Vec<Alert> {
        self.alerts.lock().unwrap().clone()
    }

    /// Clear all stored alerts
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned.
    pub fn clear(&self) {
        self.alerts.lock().unwrap().clear();
    }

    /// Get count of stored alerts
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned.
    #[must_use]
    pub fn count(&self) -> usize {
        self.alerts.lock().unwrap().len()
    }
}

#[async_trait]
impl AlertChannel for MemoryChannel {
    fn name(&self) -> &'static str {
        "memory"
    }

    async fn deliver(&self, _cx: &Cx, alert: &Alert) -> Result<(), AlertError> {
        self.alerts.lock().unwrap().push(alert.clone());
        Ok(())
    }
}

// =============================================================================
// Slack Channel
// =============================================================================

/// Slack channel - sends alerts as formatted Slack messages via incoming webhook
pub struct SlackChannel {
    webhook_url: String,
    client: reqwest::Client,
    min_severity: Severity,
}

impl SlackChannel {
    pub fn new(webhook_url: impl Into<String>, min_severity: Severity) -> Self {
        Self {
            webhook_url: webhook_url.into(),
            client: reqwest::Client::new(),
            min_severity,
        }
    }

    fn severity_color(severity: Severity) -> &'static str {
        match severity {
            Severity::Info => "#36a64f",
            Severity::Warning => "#daa038",
            Severity::Critical => "#d50000",
        }
    }

    fn severity_emoji(severity: Severity) -> &'static str {
        match severity {
            Severity::Info => ":information_source:",
            Severity::Warning => ":warning:",
            Severity::Critical => ":rotating_light:",
        }
    }
}

#[async_trait]
impl AlertChannel for SlackChannel {
    fn name(&self) -> &'static str {
        "slack"
    }

    async fn deliver(&self, _cx: &Cx, alert: &Alert) -> Result<(), AlertError> {
        if alert.severity < self.min_severity {
            return Ok(());
        }

        let payload = serde_json::json!({
            "attachments": [{
                "color": Self::severity_color(alert.severity),
                "blocks": [
                    {
                        "type": "header",
                        "text": {
                            "type": "plain_text",
                            "text": format!("{} {}", Self::severity_emoji(alert.severity), alert.title),
                        }
                    },
                    {
                        "type": "section",
                        "text": {
                            "type": "mrkdwn",
                            "text": alert.message,
                        }
                    },
                    {
                        "type": "context",
                        "elements": [
                            {
                                "type": "mrkdwn",
                                "text": format!("*Severity:* {:?} | *Rule:* {} | *Time:* {}", alert.severity, alert.rule_id, alert.fired_at.format("%Y-%m-%d %H:%M:%S UTC")),
                            }
                        ]
                    }
                ]
            }]
        });

        let response = self
            .client
            .post(&self.webhook_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| AlertError::DeliveryFailed(format!("Slack webhook failed: {e}")))?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(AlertError::DeliveryFailed(format!(
                "Slack returned status: {}",
                response.status()
            )))
        }
    }
}

// =============================================================================
// Discord Channel
// =============================================================================

/// Discord channel - sends alerts as rich embeds via Discord webhook
pub struct DiscordChannel {
    webhook_url: String,
    client: reqwest::Client,
    min_severity: Severity,
}

impl DiscordChannel {
    pub fn new(webhook_url: impl Into<String>, min_severity: Severity) -> Self {
        Self {
            webhook_url: webhook_url.into(),
            client: reqwest::Client::new(),
            min_severity,
        }
    }

    fn severity_color_int(severity: Severity) -> u32 {
        match severity {
            Severity::Info => 0x36_a6_4f,
            Severity::Warning => 0xda_a0_38,
            Severity::Critical => 0xd5_00_00,
        }
    }
}

#[async_trait]
impl AlertChannel for DiscordChannel {
    fn name(&self) -> &'static str {
        "discord"
    }

    async fn deliver(&self, _cx: &Cx, alert: &Alert) -> Result<(), AlertError> {
        if alert.severity < self.min_severity {
            return Ok(());
        }

        let payload = serde_json::json!({
            "embeds": [{
                "title": alert.title,
                "description": alert.message,
                "color": Self::severity_color_int(alert.severity),
                "fields": [
                    {"name": "Severity", "value": format!("{:?}", alert.severity), "inline": true},
                    {"name": "Rule", "value": &alert.rule_id, "inline": true},
                ],
                "timestamp": alert.fired_at.to_rfc3339(),
                "footer": {"text": "Vibe Cockpit Alert"},
            }]
        });

        let response = self
            .client
            .post(&self.webhook_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| AlertError::DeliveryFailed(format!("Discord webhook failed: {e}")))?;

        if response.status().is_success() || response.status().as_u16() == 204 {
            Ok(())
        } else {
            Err(AlertError::DeliveryFailed(format!(
                "Discord returned status: {}",
                response.status()
            )))
        }
    }
}

// =============================================================================
// Desktop Channel
// =============================================================================

/// Desktop notification channel - uses OS-native notification APIs
pub struct DesktopChannel {
    min_severity: Severity,
}

impl DesktopChannel {
    #[must_use]
    pub fn new(min_severity: Severity) -> Self {
        Self { min_severity }
    }
}

#[async_trait]
impl AlertChannel for DesktopChannel {
    fn name(&self) -> &'static str {
        "desktop"
    }

    async fn deliver(&self, cx: &Cx, alert: &Alert) -> Result<(), AlertError> {
        if alert.severity < self.min_severity {
            return Ok(());
        }

        let title = format!("VC Alert: {}", alert.title);
        let body = &alert.message;

        #[cfg(target_os = "macos")]
        {
            let script = format!(
                r#"display notification "{}" with title "{}""#,
                body.replace('"', r#"\""#),
                title.replace('"', r#"\""#),
            );

            ProcessCommand::new("osascript")
                .arg("-e")
                .arg(&script)
                .output_async(cx)
                .await
                .map_err(|e| AlertError::DeliveryFailed(format!("osascript failed: {e}")))?;
        }

        #[cfg(target_os = "linux")]
        {
            ProcessCommand::new("notify-send")
                .arg("--urgency")
                .arg(match alert.severity {
                    Severity::Critical => "critical",
                    Severity::Warning => "normal",
                    Severity::Info => "low",
                })
                .arg(&title)
                .arg(body)
                .output_async(cx)
                .await
                .map_err(|e| AlertError::DeliveryFailed(format!("notify-send failed: {e}")))?;
        }

        let _ = cx;
        Ok(())
    }
}

// =============================================================================
// Channel Manager
// =============================================================================

/// Result of a single channel delivery attempt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryResult {
    pub channel: String,
    pub success: bool,
    pub error: Option<String>,
}

/// Manages multiple alert delivery channels and dispatches alerts
pub struct ChannelManager {
    channels: Vec<Box<dyn AlertChannel>>,
}

impl ChannelManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            channels: Vec::new(),
        }
    }

    pub fn add_channel(&mut self, channel: Box<dyn AlertChannel>) {
        self.channels.push(channel);
    }

    /// Deliver an alert to all registered channels
    pub async fn deliver_all(&self, cx: &Cx, alert: &Alert) -> Vec<DeliveryResult> {
        let mut results = Vec::with_capacity(self.channels.len());

        for channel in &self.channels {
            let result = channel.deliver(cx, alert).await;
            results.push(DeliveryResult {
                channel: channel.name().to_string(),
                success: result.is_ok(),
                error: result.err().map(|e| e.to_string()),
            });
        }

        results
    }

    /// Number of registered channels
    #[must_use]
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }
}

impl Default for ChannelManager {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Alert Builder
// =============================================================================

impl Alert {
    /// Create an alert from a fired rule
    pub fn from_rule(rule: &AlertRule, message: impl Into<String>) -> Self {
        Self {
            id: None,
            rule_id: rule.rule_id.clone(),
            fired_at: Utc::now(),
            severity: rule.severity,
            title: rule.name.clone(),
            message: message.into(),
            machine_id: None,
            context: serde_json::json!({}),
        }
    }

    /// Set the machine ID
    #[must_use]
    pub fn with_machine_id(mut self, machine_id: impl Into<String>) -> Self {
        self.machine_id = Some(machine_id.into());
        self
    }

    /// Set the context
    #[must_use]
    pub fn with_context(mut self, context: serde_json::Value) -> Self {
        self.context = context;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asupersync::lab::{LabConfig, assert_deterministic};
    use asupersync::types::Budget;
    use async_trait::async_trait;
    use mockall::mock;
    use std::future::Future;
    use std::sync::{Arc, Mutex};

    mock! {
        Channel {}

        #[async_trait]
        impl AlertChannel for Channel {
            fn name(&self) -> &'static str;
            async fn deliver(&self, cx: &Cx, alert: &Alert) -> Result<(), AlertError>;
        }
    }

    fn run_async<F: Future<Output = ()>>(future: F) {
        futures::executor::block_on(future);
    }

    fn test_cx() -> Cx {
        Cx::for_testing()
    }

    // ==========================================================================
    // Severity Tests
    // ==========================================================================

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Info < Severity::Warning);
        assert!(Severity::Warning < Severity::Critical);
        assert!(Severity::Info < Severity::Critical);
    }

    #[test]
    fn test_severity_serialization() {
        assert_eq!(serde_json::to_string(&Severity::Info).unwrap(), "\"info\"");
        assert_eq!(
            serde_json::to_string(&Severity::Warning).unwrap(),
            "\"warning\""
        );
        assert_eq!(
            serde_json::to_string(&Severity::Critical).unwrap(),
            "\"critical\""
        );
    }

    #[test]
    fn test_severity_deserialization() {
        assert_eq!(
            serde_json::from_str::<Severity>("\"info\"").unwrap(),
            Severity::Info
        );
        assert_eq!(
            serde_json::from_str::<Severity>("\"warning\"").unwrap(),
            Severity::Warning
        );
        assert_eq!(
            serde_json::from_str::<Severity>("\"critical\"").unwrap(),
            Severity::Critical
        );
    }

    // ==========================================================================
    // ThresholdOp Tests
    // ==========================================================================

    #[test]
    fn test_threshold_op() {
        assert!(ThresholdOp::Gt.check(10.0, 5.0));
        assert!(!ThresholdOp::Gt.check(5.0, 10.0));
        assert!(ThresholdOp::Gte.check(10.0, 10.0));
        assert!(ThresholdOp::Lt.check(5.0, 10.0));
    }

    #[test]
    fn test_threshold_op_all_operators() {
        // Gt (greater than)
        assert!(ThresholdOp::Gt.check(10.0, 5.0));
        assert!(!ThresholdOp::Gt.check(5.0, 5.0)); // equal should fail
        assert!(!ThresholdOp::Gt.check(4.0, 5.0));

        // Gte (greater than or equal)
        assert!(ThresholdOp::Gte.check(10.0, 5.0));
        assert!(ThresholdOp::Gte.check(5.0, 5.0)); // equal should pass
        assert!(!ThresholdOp::Gte.check(4.0, 5.0));

        // Lt (less than)
        assert!(ThresholdOp::Lt.check(4.0, 5.0));
        assert!(!ThresholdOp::Lt.check(5.0, 5.0)); // equal should fail
        assert!(!ThresholdOp::Lt.check(6.0, 5.0));

        // Lte (less than or equal)
        assert!(ThresholdOp::Lte.check(4.0, 5.0));
        assert!(ThresholdOp::Lte.check(5.0, 5.0)); // equal should pass
        assert!(!ThresholdOp::Lte.check(6.0, 5.0));

        // Eq (equal)
        assert!(ThresholdOp::Eq.check(5.0, 5.0));
        assert!(!ThresholdOp::Eq.check(5.1, 5.0));
    }

    #[test]
    fn test_threshold_op_edge_cases() {
        // Very small differences
        assert!(!ThresholdOp::Eq.check(0.000_000_1, 0.0));

        // Negative numbers
        assert!(ThresholdOp::Lt.check(-10.0, -5.0));
        assert!(ThresholdOp::Gt.check(-5.0, -10.0));

        // Zero
        assert!(ThresholdOp::Eq.check(0.0, 0.0));
        assert!(ThresholdOp::Gte.check(0.0, 0.0));
        assert!(ThresholdOp::Lte.check(0.0, 0.0));
    }

    #[test]
    fn test_threshold_op_serialization() {
        assert_eq!(serde_json::to_string(&ThresholdOp::Gt).unwrap(), "\"gt\"");
        assert_eq!(serde_json::to_string(&ThresholdOp::Gte).unwrap(), "\"gte\"");
        assert_eq!(serde_json::to_string(&ThresholdOp::Lt).unwrap(), "\"lt\"");
        assert_eq!(serde_json::to_string(&ThresholdOp::Lte).unwrap(), "\"lte\"");
        assert_eq!(serde_json::to_string(&ThresholdOp::Eq).unwrap(), "\"eq\"");
    }

    // ==========================================================================
    // AlertCondition Tests
    // ==========================================================================

    #[test]
    fn test_alert_condition_threshold_serialization() {
        let condition = AlertCondition::Threshold {
            query: "SELECT MAX(usage) FROM table".to_string(),
            operator: ThresholdOp::Gte,
            value: 80.0,
        };

        let json = serde_json::to_string(&condition).unwrap();
        assert!(json.contains("\"type\":\"threshold\""));
        assert!(json.contains("\"query\""));
        assert!(json.contains("\"operator\""));
        assert!(json.contains("\"value\""));

        // Round-trip
        let parsed: AlertCondition = serde_json::from_str(&json).unwrap();
        if let AlertCondition::Threshold {
            query,
            operator,
            value,
        } = parsed
        {
            assert_eq!(query, "SELECT MAX(usage) FROM table");
            assert!(matches!(operator, ThresholdOp::Gte));
            assert!((value - 80.0).abs() < f64::EPSILON);
        } else {
            panic!("Expected Threshold condition");
        }
    }

    #[test]
    fn test_alert_condition_pattern_serialization() {
        let condition = AlertCondition::Pattern {
            table: "logs".to_string(),
            column: "message".to_string(),
            regex: "error.*critical".to_string(),
        };

        let json = serde_json::to_string(&condition).unwrap();
        assert!(json.contains("\"type\":\"pattern\""));

        let parsed: AlertCondition = serde_json::from_str(&json).unwrap();
        if let AlertCondition::Pattern {
            table,
            column,
            regex,
        } = parsed
        {
            assert_eq!(table, "logs");
            assert_eq!(column, "message");
            assert_eq!(regex, "error.*critical");
        } else {
            panic!("Expected Pattern condition");
        }
    }

    #[test]
    fn test_alert_condition_absence_serialization() {
        let condition = AlertCondition::Absence {
            table: "heartbeats".to_string(),
            max_age_secs: 300,
        };

        let json = serde_json::to_string(&condition).unwrap();
        assert!(json.contains("\"type\":\"absence\""));

        let parsed: AlertCondition = serde_json::from_str(&json).unwrap();
        if let AlertCondition::Absence {
            table,
            max_age_secs,
        } = parsed
        {
            assert_eq!(table, "heartbeats");
            assert_eq!(max_age_secs, 300);
        } else {
            panic!("Expected Absence condition");
        }
    }

    #[test]
    fn test_alert_condition_rate_of_change_serialization() {
        let condition = AlertCondition::RateOfChange {
            query: "SELECT usage FROM metrics".to_string(),
            window_secs: 60,
            threshold_per_sec: 1.5,
        };

        let json = serde_json::to_string(&condition).unwrap();
        assert!(json.contains("\"type\":\"rate_of_change\""));

        let parsed: AlertCondition = serde_json::from_str(&json).unwrap();
        if let AlertCondition::RateOfChange {
            query,
            window_secs,
            threshold_per_sec,
        } = parsed
        {
            assert_eq!(query, "SELECT usage FROM metrics");
            assert_eq!(window_secs, 60);
            assert!((threshold_per_sec - 1.5).abs() < f64::EPSILON);
        } else {
            panic!("Expected RateOfChange condition");
        }
    }

    // ==========================================================================
    // AlertRule Tests
    // ==========================================================================

    #[test]
    fn test_alert_rule_serialization() {
        let rule = AlertRule {
            rule_id: "test-rule".to_string(),
            name: "Test Rule".to_string(),
            description: Some("A test rule".to_string()),
            severity: Severity::Warning,
            enabled: true,
            condition: AlertCondition::Threshold {
                query: "SELECT 1".to_string(),
                operator: ThresholdOp::Gt,
                value: 0.0,
            },
            cooldown_secs: 300,
            channels: vec!["tui".to_string(), "webhook".to_string()],
        };

        let json = serde_json::to_string(&rule).unwrap();
        let parsed: AlertRule = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.rule_id, "test-rule");
        assert_eq!(parsed.name, "Test Rule");
        assert_eq!(parsed.description, Some("A test rule".to_string()));
        assert_eq!(parsed.severity, Severity::Warning);
        assert!(parsed.enabled);
        assert_eq!(parsed.cooldown_secs, 300);
        assert_eq!(parsed.channels.len(), 2);
    }

    // ==========================================================================
    // Alert Tests
    // ==========================================================================

    #[test]
    fn test_alert_serialization() {
        let alert = Alert {
            id: Some(42),
            rule_id: "disk-critical".to_string(),
            fired_at: Utc::now(),
            severity: Severity::Critical,
            title: "Disk Full".to_string(),
            message: "Disk usage at 95%".to_string(),
            machine_id: Some("server-1".to_string()),
            context: serde_json::json!({"disk": "/dev/sda1", "usage_pct": 95.0}),
        };

        let json = serde_json::to_string(&alert).unwrap();
        let parsed: Alert = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, Some(42));
        assert_eq!(parsed.rule_id, "disk-critical");
        assert_eq!(parsed.severity, Severity::Critical);
        assert_eq!(parsed.title, "Disk Full");
        assert_eq!(parsed.machine_id, Some("server-1".to_string()));
    }

    #[test]
    fn test_alert_minimal() {
        let alert = Alert {
            id: None,
            rule_id: "test".to_string(),
            fired_at: Utc::now(),
            severity: Severity::Info,
            title: "Test".to_string(),
            message: "Test message".to_string(),
            machine_id: None,
            context: serde_json::json!({}),
        };

        let json = serde_json::to_string(&alert).unwrap();
        assert!(json.contains("\"rule_id\":\"test\""));
    }

    // ==========================================================================
    // AlertError Tests
    // ==========================================================================

    #[test]
    fn test_alert_error_display() {
        let err = AlertError::RuleNotFound("missing-rule".to_string());
        assert_eq!(format!("{err}"), "Rule not found: missing-rule");

        let err = AlertError::EvaluationFailed("query timeout".to_string());
        assert_eq!(format!("{err}"), "Evaluation failed: query timeout");

        let err = AlertError::DeliveryFailed("webhook unreachable".to_string());
        assert_eq!(format!("{err}"), "Delivery failed: webhook unreachable");
    }

    // ==========================================================================
    // AlertEngine Tests
    // ==========================================================================

    #[test]
    fn test_default_rules() {
        let engine = AlertEngine::new();
        assert!(!engine.rules().is_empty());
    }

    #[test]
    fn test_default_rules_content() {
        let engine = AlertEngine::new();
        let rules = engine.rules();

        // Should have rate-limit-warning rule
        let rate_rule = rules.iter().find(|r| r.rule_id == "rate-limit-warning");
        assert!(rate_rule.is_some());
        let rate_rule = rate_rule.unwrap();
        assert_eq!(rate_rule.severity, Severity::Warning);
        assert!(rate_rule.enabled);

        // Should have disk-critical rule
        let disk_rule = rules.iter().find(|r| r.rule_id == "disk-critical");
        assert!(disk_rule.is_some());
        let disk_rule = disk_rule.unwrap();
        assert_eq!(disk_rule.severity, Severity::Critical);
    }

    #[test]
    fn test_cooldown() {
        let engine = AlertEngine::new();
        assert!(!engine.is_in_cooldown("test", 60));
        engine.record_fired("test");
        assert!(engine.is_in_cooldown("test", 60));
    }

    #[test]
    fn test_cooldown_multiple_rules() {
        let engine = AlertEngine::new();

        // Fire rule A
        engine.record_fired("rule-a");
        assert!(engine.is_in_cooldown("rule-a", 300));
        assert!(!engine.is_in_cooldown("rule-b", 300));

        // Fire rule B
        engine.record_fired("rule-b");
        assert!(engine.is_in_cooldown("rule-a", 300));
        assert!(engine.is_in_cooldown("rule-b", 300));
    }

    #[test]
    fn test_alert_engine_default() {
        let engine1 = AlertEngine::new();
        let engine2 = AlertEngine::default();
        assert_eq!(engine1.rules().len(), engine2.rules().len());
    }

    // ==========================================================================
    // Mock Channel Tests
    // ==========================================================================

    #[test]
    fn test_mock_channel_deliver() {
        run_async(async {
            let cx = test_cx();
            let mut mock = MockChannel::new();
            mock.expect_name().return_const("mock");
            mock.expect_deliver().returning(|_, _| Ok(()));

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
            assert!(mock.deliver(&cx, &alert).await.is_ok());
        });
    }

    #[test]
    fn test_mock_channel_delivery_failure() {
        run_async(async {
            let cx = test_cx();
            let mut mock = MockChannel::new();
            mock.expect_name().return_const("failing-channel");
            mock.expect_deliver().returning(|_, _| {
                Err(AlertError::DeliveryFailed("connection refused".to_string()))
            });

            let alert = Alert {
                id: None,
                rule_id: "test-rule".to_string(),
                fired_at: Utc::now(),
                severity: Severity::Warning,
                title: "Test".to_string(),
                message: "Testing".to_string(),
                machine_id: None,
                context: serde_json::json!({}),
            };

            let result = mock.deliver(&cx, &alert).await;
            assert!(result.is_err());
            assert!(matches!(result, Err(AlertError::DeliveryFailed(_))));
        });
    }

    // ==========================================================================
    // Additional Default Rules Tests
    // ==========================================================================

    #[test]
    fn test_default_rules_dcg_critical() {
        let engine = AlertEngine::new();
        let rules = engine.rules();

        let dcg_rule = rules.iter().find(|r| r.rule_id == "dcg-critical-block");
        assert!(dcg_rule.is_some());
        let dcg_rule = dcg_rule.unwrap();
        assert_eq!(dcg_rule.severity, Severity::Critical);
        assert!(matches!(dcg_rule.condition, AlertCondition::Pattern { .. }));
    }

    #[test]
    fn test_default_rules_agent_stuck() {
        let engine = AlertEngine::new();
        let rules = engine.rules();

        let agent_rule = rules.iter().find(|r| r.rule_id == "agent-stuck");
        assert!(agent_rule.is_some());
        let agent_rule = agent_rule.unwrap();
        assert_eq!(agent_rule.severity, Severity::Warning);
        assert!(matches!(
            agent_rule.condition,
            AlertCondition::Absence { .. }
        ));
    }

    #[test]
    fn test_default_rules_rch_queue() {
        let engine = AlertEngine::new();
        let rules = engine.rules();

        let rch_rule = rules.iter().find(|r| r.rule_id == "rch-queue-pressure");
        assert!(rch_rule.is_some());
        let rch_rule = rch_rule.unwrap();
        assert_eq!(rch_rule.severity, Severity::Warning);
        assert!(matches!(
            rch_rule.condition,
            AlertCondition::Threshold { .. }
        ));
    }

    #[test]
    fn test_default_rules_memory_critical() {
        let engine = AlertEngine::new();
        let rules = engine.rules();

        let mem_rule = rules.iter().find(|r| r.rule_id == "memory-critical");
        assert!(mem_rule.is_some());
        let mem_rule = mem_rule.unwrap();
        assert_eq!(mem_rule.severity, Severity::Critical);
    }

    #[test]
    fn test_default_rules_count() {
        let engine = AlertEngine::new();
        // We now have 6 default rules
        assert_eq!(engine.rules().len(), 6);
    }

    // ==========================================================================
    // Memory Channel Tests
    // ==========================================================================

    #[test]
    fn test_memory_channel() {
        run_async(async {
            let cx = test_cx();
            let channel = MemoryChannel::new();
            assert_eq!(channel.count(), 0);

            let alert = Alert {
                id: None,
                rule_id: "test".to_string(),
                fired_at: Utc::now(),
                severity: Severity::Info,
                title: "Test".to_string(),
                message: "Test message".to_string(),
                machine_id: None,
                context: serde_json::json!({}),
            };

            channel.deliver(&cx, &alert).await.unwrap();
            assert_eq!(channel.count(), 1);

            channel.deliver(&cx, &alert).await.unwrap();
            assert_eq!(channel.count(), 2);

            let alerts = channel.alerts();
            assert_eq!(alerts.len(), 2);
            assert_eq!(alerts[0].rule_id, "test");
        });
    }

    #[test]
    fn test_memory_channel_clear() {
        run_async(async {
            let cx = test_cx();
            let channel = MemoryChannel::new();
            let alert = Alert {
                id: None,
                rule_id: "test".to_string(),
                fired_at: Utc::now(),
                severity: Severity::Info,
                title: "Test".to_string(),
                message: "Test".to_string(),
                machine_id: None,
                context: serde_json::json!({}),
            };

            channel.deliver(&cx, &alert).await.unwrap();
            assert_eq!(channel.count(), 1);

            channel.clear();
            assert_eq!(channel.count(), 0);
        });
    }

    #[test]
    fn test_memory_channel_name() {
        let channel = MemoryChannel::new();
        assert_eq!(channel.name(), "memory");
    }

    // ==========================================================================
    // Log Channel Tests
    // ==========================================================================

    #[test]
    fn test_log_channel_levels() {
        let warn_channel = LogChannel::warning();
        assert_eq!(warn_channel.level, tracing::Level::WARN);

        let info_channel = LogChannel::info();
        assert_eq!(info_channel.level, tracing::Level::INFO);

        let custom_channel = LogChannel::new(tracing::Level::ERROR);
        assert_eq!(custom_channel.level, tracing::Level::ERROR);
    }

    #[test]
    fn test_log_channel_default() {
        let channel = LogChannel::default();
        assert_eq!(channel.level, tracing::Level::WARN);
    }

    #[test]
    fn test_log_channel_name() {
        let channel = LogChannel::new(tracing::Level::INFO);
        assert_eq!(channel.name(), "log");
    }

    #[test]
    fn test_log_channel_deliver() {
        run_async(async {
            let cx = test_cx();
            let channel = LogChannel::info();
            let alert = Alert {
                id: None,
                rule_id: "test".to_string(),
                fired_at: Utc::now(),
                severity: Severity::Info,
                title: "Test".to_string(),
                message: "Test".to_string(),
                machine_id: None,
                context: serde_json::json!({}),
            };

            assert!(channel.deliver(&cx, &alert).await.is_ok());
        });
    }

    // ==========================================================================
    // TUI Channel Tests
    // ==========================================================================

    #[test]
    fn test_tui_channel() {
        run_async(async {
            let cx = test_cx();
            let (tx, mut rx) = mpsc::channel(10);
            let channel = TuiChannel::new(tx);

            assert_eq!(channel.name(), "tui");

            let alert = Alert {
                id: None,
                rule_id: "test".to_string(),
                fired_at: Utc::now(),
                severity: Severity::Warning,
                title: "Test Alert".to_string(),
                message: "This is a test".to_string(),
                machine_id: Some("test-machine".to_string()),
                context: serde_json::json!({"key": "value"}),
            };

            channel.deliver(&cx, &alert).await.unwrap();

            let received = rx.try_recv().unwrap();
            assert_eq!(received.rule_id, "test");
            assert_eq!(received.title, "Test Alert");
        });
    }

    #[test]
    fn test_tui_channel_delivery_is_deterministic_in_lab() {
        let first_alert = Alert {
            id: None,
            rule_id: "first".to_string(),
            fired_at: DateTime::<Utc>::from_timestamp(1_700_000_000, 0)
                .expect("fixed timestamp for deterministic lab test"),
            severity: Severity::Warning,
            title: "First Alert".to_string(),
            message: "First deterministic delivery".to_string(),
            machine_id: Some("lab-machine".to_string()),
            context: serde_json::json!({ "seq": 1 }),
        };
        let second_alert = Alert {
            id: None,
            rule_id: "second".to_string(),
            fired_at: DateTime::<Utc>::from_timestamp(1_700_000_001, 0)
                .expect("fixed timestamp for deterministic lab test"),
            severity: Severity::Critical,
            title: "Second Alert".to_string(),
            message: "Second deterministic delivery".to_string(),
            machine_id: Some("lab-machine".to_string()),
            context: serde_json::json!({ "seq": 2 }),
        };

        assert_deterministic(
            LabConfig::new(0xA11E).worker_count(2).max_steps(10_000),
            |runtime| {
                let root = runtime.state.create_root_region(Budget::INFINITE);
                let send_cx = Cx::for_testing();
                let recv_cx = send_cx.clone();
                let (tx, mut rx) = mpsc::channel(2);
                let channel = TuiChannel::new(tx);
                let received = Arc::new(Mutex::new(Vec::<String>::new()));

                let received_for_rx = Arc::clone(&received);
                let (receiver_task, _) = runtime
                    .state
                    .create_task(root, Budget::INFINITE, async move {
                        let first = rx.recv(&recv_cx).await.expect("receive first alert");
                        let second = rx.recv(&recv_cx).await.expect("receive second alert");
                        let mut trace = received_for_rx.lock().unwrap();
                        trace.push(first.rule_id);
                        trace.push(second.rule_id);
                    })
                    .expect("create receiver task");

                let alert_one = first_alert.clone();
                let alert_two = second_alert.clone();
                let (sender_task, _) = runtime
                    .state
                    .create_task(root, Budget::INFINITE, async move {
                        channel
                            .deliver(&send_cx, &alert_one)
                            .await
                            .expect("deliver first alert");
                        channel
                            .deliver(&send_cx, &alert_two)
                            .await
                            .expect("deliver second alert");
                    })
                    .expect("create sender task");

                runtime.scheduler.lock().schedule(sender_task, 0);
                runtime.scheduler.lock().schedule(receiver_task, 1);
                runtime.run_until_quiescent();

                let trace = received.lock().unwrap().clone();
                assert_eq!(trace, vec!["first".to_string(), "second".to_string()]);
            },
        );
    }

    // ==========================================================================
    // Webhook Channel Tests
    // ==========================================================================

    #[test]
    fn test_webhook_channel_creation() {
        let channel = WebhookChannel::new("https://example.com/webhook");
        assert_eq!(channel.name(), "webhook");
        assert_eq!(channel.url, "https://example.com/webhook");
    }

    #[test]
    fn test_webhook_channel_with_custom_client() {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();

        let channel = WebhookChannel::with_client("https://example.com/webhook", client);
        assert_eq!(channel.url, "https://example.com/webhook");
    }

    // ==========================================================================
    // Alert Builder Tests
    // ==========================================================================

    #[test]
    fn test_alert_from_rule() {
        let rule = AlertRule {
            rule_id: "test-rule".to_string(),
            name: "Test Rule".to_string(),
            description: None,
            severity: Severity::Warning,
            enabled: true,
            condition: AlertCondition::Threshold {
                query: "SELECT 1".to_string(),
                operator: ThresholdOp::Gt,
                value: 0.0,
            },
            cooldown_secs: 60,
            channels: vec!["tui".to_string()],
        };

        let alert = Alert::from_rule(&rule, "Value exceeded threshold");
        assert_eq!(alert.rule_id, "test-rule");
        assert_eq!(alert.title, "Test Rule");
        assert_eq!(alert.severity, Severity::Warning);
        assert_eq!(alert.message, "Value exceeded threshold");
        assert!(alert.id.is_none());
        assert!(alert.machine_id.is_none());
    }

    #[test]
    fn test_alert_builder_chain() {
        let rule = AlertRule {
            rule_id: "test".to_string(),
            name: "Test".to_string(),
            description: None,
            severity: Severity::Critical,
            enabled: true,
            condition: AlertCondition::Threshold {
                query: "SELECT 1".to_string(),
                operator: ThresholdOp::Gt,
                value: 0.0,
            },
            cooldown_secs: 60,
            channels: vec![],
        };

        let alert = Alert::from_rule(&rule, "Test message")
            .with_machine_id("server-1")
            .with_context(serde_json::json!({"metric": 95.5, "threshold": 90.0}));

        assert_eq!(alert.machine_id, Some("server-1".to_string()));
        assert_eq!(alert.context["metric"], 95.5);
        assert_eq!(alert.context["threshold"], 90.0);
    }

    // ==========================================================================
    // Slack Channel Tests
    // ==========================================================================

    #[test]
    fn test_slack_channel_name() {
        let channel = SlackChannel::new("https://hooks.slack.com/test", Severity::Info);
        assert_eq!(channel.name(), "slack");
    }

    #[test]
    fn test_slack_severity_color() {
        assert_eq!(SlackChannel::severity_color(Severity::Info), "#36a64f");
        assert_eq!(SlackChannel::severity_color(Severity::Warning), "#daa038");
        assert_eq!(SlackChannel::severity_color(Severity::Critical), "#d50000");
    }

    #[test]
    fn test_slack_severity_emoji() {
        assert_eq!(
            SlackChannel::severity_emoji(Severity::Info),
            ":information_source:"
        );
        assert_eq!(SlackChannel::severity_emoji(Severity::Warning), ":warning:");
        assert_eq!(
            SlackChannel::severity_emoji(Severity::Critical),
            ":rotating_light:"
        );
    }

    #[test]
    fn test_slack_channel_filters_below_min_severity() {
        run_async(async {
            let cx = test_cx();
            let channel = SlackChannel::new("https://hooks.slack.com/test", Severity::Critical);

            let alert = Alert {
                id: None,
                rule_id: "test".to_string(),
                fired_at: Utc::now(),
                severity: Severity::Info,
                title: "Test".to_string(),
                message: "Test".to_string(),
                machine_id: None,
                context: serde_json::json!({}),
            };

            assert!(channel.deliver(&cx, &alert).await.is_ok());
        });
    }

    #[test]
    fn test_slack_channel_filters_warning_below_critical() {
        run_async(async {
            let cx = test_cx();
            let channel = SlackChannel::new("https://hooks.slack.com/test", Severity::Critical);

            let alert = Alert {
                id: None,
                rule_id: "test".to_string(),
                fired_at: Utc::now(),
                severity: Severity::Warning,
                title: "Test".to_string(),
                message: "Test".to_string(),
                machine_id: None,
                context: serde_json::json!({}),
            };

            assert!(channel.deliver(&cx, &alert).await.is_ok());
        });
    }

    // ==========================================================================
    // Discord Channel Tests
    // ==========================================================================

    #[test]
    fn test_discord_channel_name() {
        let channel = DiscordChannel::new("https://discord.com/api/webhooks/test", Severity::Info);
        assert_eq!(channel.name(), "discord");
    }

    #[test]
    fn test_discord_severity_color_int() {
        assert_eq!(
            DiscordChannel::severity_color_int(Severity::Info),
            0x0036_a64f
        );
        assert_eq!(
            DiscordChannel::severity_color_int(Severity::Warning),
            0x00da_a038
        );
        assert_eq!(
            DiscordChannel::severity_color_int(Severity::Critical),
            0x00d5_0000
        );
    }

    #[test]
    fn test_discord_channel_filters_below_min_severity() {
        run_async(async {
            let cx = test_cx();
            let channel =
                DiscordChannel::new("https://discord.com/api/webhooks/test", Severity::Warning);

            let alert = Alert {
                id: None,
                rule_id: "test".to_string(),
                fired_at: Utc::now(),
                severity: Severity::Info,
                title: "Test".to_string(),
                message: "Test".to_string(),
                machine_id: None,
                context: serde_json::json!({}),
            };

            assert!(channel.deliver(&cx, &alert).await.is_ok());
        });
    }

    // ==========================================================================
    // Desktop Channel Tests
    // ==========================================================================

    #[test]
    fn test_desktop_channel_name() {
        let channel = DesktopChannel::new(Severity::Info);
        assert_eq!(channel.name(), "desktop");
    }

    #[test]
    fn test_desktop_channel_filters_below_min_severity() {
        run_async(async {
            let cx = test_cx();
            let channel = DesktopChannel::new(Severity::Critical);

            let alert = Alert {
                id: None,
                rule_id: "test".to_string(),
                fired_at: Utc::now(),
                severity: Severity::Warning,
                title: "Test".to_string(),
                message: "Test".to_string(),
                machine_id: None,
                context: serde_json::json!({}),
            };

            assert!(channel.deliver(&cx, &alert).await.is_ok());
        });
    }

    // ==========================================================================
    // DeliveryResult Tests
    // ==========================================================================

    #[test]
    fn test_delivery_result_success() {
        let result = DeliveryResult {
            channel: "slack".to_string(),
            success: true,
            error: None,
        };
        assert!(result.success);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_delivery_result_failure() {
        let result = DeliveryResult {
            channel: "webhook".to_string(),
            success: false,
            error: Some("Connection refused".to_string()),
        };
        assert!(!result.success);
        assert_eq!(result.error.as_deref(), Some("Connection refused"));
    }

    #[test]
    fn test_delivery_result_serialization() {
        let result = DeliveryResult {
            channel: "slack".to_string(),
            success: true,
            error: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"channel\":\"slack\""));
        assert!(json.contains("\"success\":true"));

        let parsed: DeliveryResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.channel, "slack");
        assert!(parsed.success);
    }

    // ==========================================================================
    // ChannelManager Tests
    // ==========================================================================

    #[test]
    fn test_channel_manager_empty() {
        let manager = ChannelManager::new();
        assert_eq!(manager.channel_count(), 0);
    }

    #[test]
    fn test_channel_manager_default() {
        let manager = ChannelManager::default();
        assert_eq!(manager.channel_count(), 0);
    }

    #[test]
    fn test_channel_manager_add_channel() {
        let mut manager = ChannelManager::new();
        manager.add_channel(Box::new(MemoryChannel::new()));
        assert_eq!(manager.channel_count(), 1);

        manager.add_channel(Box::new(LogChannel::info()));
        assert_eq!(manager.channel_count(), 2);
    }

    #[test]
    fn test_channel_manager_deliver_all() {
        run_async(async {
            let cx = test_cx();
            let mut manager = ChannelManager::new();
            manager.add_channel(Box::new(MemoryChannel::new()));
            manager.add_channel(Box::new(LogChannel::info()));

            let alert = Alert {
                id: None,
                rule_id: "test".to_string(),
                fired_at: Utc::now(),
                severity: Severity::Warning,
                title: "Test".to_string(),
                message: "Test message".to_string(),
                machine_id: None,
                context: serde_json::json!({}),
            };

            let results = manager.deliver_all(&cx, &alert).await;
            assert_eq!(results.len(), 2);
            assert!(results.iter().all(|r| r.success));
        });
    }

    #[test]
    fn test_channel_manager_partial_failure() {
        run_async(async {
            let cx = test_cx();
            let mut manager = ChannelManager::new();
            manager.add_channel(Box::new(MemoryChannel::new()));

            let mut mock = MockChannel::new();
            mock.expect_name().return_const("failing");
            mock.expect_deliver()
                .returning(|_, _| Err(AlertError::DeliveryFailed("network error".to_string())));
            manager.add_channel(Box::new(mock));

            let alert = Alert {
                id: None,
                rule_id: "test".to_string(),
                fired_at: Utc::now(),
                severity: Severity::Critical,
                title: "Critical".to_string(),
                message: "Critical alert".to_string(),
                machine_id: None,
                context: serde_json::json!({}),
            };

            let results = manager.deliver_all(&cx, &alert).await;
            assert_eq!(results.len(), 2);
            assert!(results[0].success);
            assert_eq!(results[0].channel, "memory");
            assert!(!results[1].success);
            assert_eq!(results[1].channel, "failing");
            assert!(results[1].error.is_some());
        });
    }

    #[test]
    fn test_channel_manager_empty_deliver() {
        run_async(async {
            let cx = test_cx();
            let manager = ChannelManager::new();

            let alert = Alert {
                id: None,
                rule_id: "test".to_string(),
                fired_at: Utc::now(),
                severity: Severity::Info,
                title: "Test".to_string(),
                message: "Test".to_string(),
                machine_id: None,
                context: serde_json::json!({}),
            };

            let results = manager.deliver_all(&cx, &alert).await;
            assert!(results.is_empty());
        });
    }
}
