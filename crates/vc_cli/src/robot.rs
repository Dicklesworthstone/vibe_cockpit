//! Robot mode output for agent consumption
//!
//! This module provides:
//! - Standard envelope format for all robot output
//! - Health status data structures
//! - Triage recommendations
//! - Machine and account status

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Standard envelope for all robot mode output
///
/// Every robot command returns data wrapped in this envelope,
/// providing consistent metadata for agent consumption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RobotEnvelope<T: Serialize> {
    /// Schema version identifier (e.g., "vc.robot.health.v1")
    pub schema_version: String,

    /// When this output was generated
    pub generated_at: DateTime<Utc>,

    /// The actual data payload
    pub data: T,

    /// Data staleness by source (seconds since last collection)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub staleness: HashMap<String, u64>,

    /// Warnings about data quality or collection issues
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

impl<T: Serialize> RobotEnvelope<T> {
    /// Create a new envelope with the given schema and data
    pub fn new(schema_version: impl Into<String>, data: T) -> Self {
        Self {
            schema_version: schema_version.into(),
            generated_at: Utc::now(),
            data,
            staleness: HashMap::new(),
            warnings: Vec::new(),
        }
    }

    /// Add staleness information
    pub fn with_staleness(mut self, staleness: HashMap<String, u64>) -> Self {
        self.staleness = staleness;
        self
    }

    /// Add a single staleness entry
    pub fn add_staleness(mut self, source: impl Into<String>, seconds: u64) -> Self {
        self.staleness.insert(source.into(), seconds);
        self
    }

    /// Add warnings
    pub fn with_warnings(mut self, warnings: Vec<String>) -> Self {
        self.warnings = warnings;
        self
    }

    /// Add a single warning
    pub fn add_warning(mut self, warning: impl Into<String>) -> Self {
        self.warnings.push(warning.into());
        self
    }

    /// Serialize to pretty JSON string
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|e| {
            format!(r#"{{"error": "serialization failed: {}"}}"#, e)
        })
    }

    /// Serialize to compact JSON string
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|e| {
            format!(r#"{{"error": "serialization failed: {}"}}"#, e)
        })
    }
}

// ============================================================================
// Health Data Structures
// ============================================================================

/// Overall fleet health data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthData {
    /// Overall health summary
    pub overall: OverallHealth,

    /// Per-machine health
    pub machines: Vec<MachineHealth>,

    /// Active alert count by severity
    pub alerts_by_severity: AlertCounts,
}

/// Overall health summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverallHealth {
    /// Health score (0.0 to 1.0)
    pub score: f64,

    /// Severity level: "healthy", "warning", "critical"
    pub severity: String,

    /// Total active alerts
    pub active_alerts: u32,

    /// Number of machines monitored
    pub machine_count: u32,

    /// Number of active agents
    pub agent_count: u32,
}

/// Per-machine health
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineHealth {
    /// Machine identifier
    pub id: String,

    /// Display name
    pub name: String,

    /// Health score (0.0 to 1.0)
    pub score: f64,

    /// Status: "online", "degraded", "offline", "unknown"
    pub status: String,

    /// Top issue affecting this machine (if any)
    pub top_issue: Option<String>,

    /// Last data collection timestamp
    pub last_seen: DateTime<Utc>,

    /// Active agent count on this machine
    pub agent_count: u32,

    /// CPU usage percentage (0-100)
    pub cpu_percent: Option<f64>,

    /// Memory usage percentage (0-100)
    pub memory_percent: Option<f64>,
}

/// Alert counts by severity
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AlertCounts {
    pub critical: u32,
    pub warning: u32,
    pub info: u32,
}

// ============================================================================
// Triage Data Structures
// ============================================================================

/// Triage recommendations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriageData {
    /// Prioritized recommendations
    pub recommendations: Vec<Recommendation>,

    /// Suggested commands to run
    pub suggested_commands: Vec<SuggestedCommand>,
}

/// A single triage recommendation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    /// Unique identifier
    pub id: String,

    /// Priority (1 = highest)
    pub priority: u32,

    /// Short title
    pub title: String,

    /// Detailed description
    pub description: String,

    /// Affected scope (machine, collector, etc.)
    pub scope: String,

    /// Suggested action
    pub action: String,
}

/// A suggested command for the agent to run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedCommand {
    /// Command to run
    pub command: String,

    /// Why this is suggested
    pub reason: String,

    /// Confidence level (0.0 to 1.0)
    pub confidence: f64,
}

// ============================================================================
// Health Command Implementation
// ============================================================================

/// Generate health status (stub implementation)
///
/// This returns placeholder data until the store queries are implemented.
pub fn robot_health() -> RobotEnvelope<HealthData> {
    let data = HealthData {
        overall: OverallHealth {
            score: 1.0,
            severity: "healthy".to_string(),
            active_alerts: 0,
            machine_count: 1,
            agent_count: 0,
        },
        machines: vec![MachineHealth {
            id: "local".to_string(),
            name: "Local Machine".to_string(),
            score: 1.0,
            status: "online".to_string(),
            top_issue: None,
            last_seen: Utc::now(),
            agent_count: 0,
            cpu_percent: None,
            memory_percent: None,
        }],
        alerts_by_severity: AlertCounts::default(),
    };

    RobotEnvelope::new("vc.robot.health.v1", data)
        .add_warning("No collectors have run yet - data may be incomplete")
}

/// Generate triage recommendations (stub implementation)
pub fn robot_triage() -> RobotEnvelope<TriageData> {
    let data = TriageData {
        recommendations: vec![],
        suggested_commands: vec![SuggestedCommand {
            command: "vc collect".to_string(),
            reason: "Run initial data collection".to_string(),
            confidence: 0.9,
        }],
    };

    RobotEnvelope::new("vc.robot.triage.v1", data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_robot_envelope_new() {
        let envelope = RobotEnvelope::new("test.v1", "hello");
        assert_eq!(envelope.schema_version, "test.v1");
        assert_eq!(envelope.data, "hello");
        assert!(envelope.staleness.is_empty());
        assert!(envelope.warnings.is_empty());
    }

    #[test]
    fn test_robot_envelope_with_staleness() {
        let mut staleness = HashMap::new();
        staleness.insert("sysmoni".to_string(), 60);

        let envelope = RobotEnvelope::new("test.v1", "data")
            .with_staleness(staleness);

        assert_eq!(envelope.staleness.get("sysmoni"), Some(&60));
    }

    #[test]
    fn test_robot_envelope_with_warnings() {
        let envelope = RobotEnvelope::new("test.v1", "data")
            .add_warning("warning 1")
            .add_warning("warning 2");

        assert_eq!(envelope.warnings.len(), 2);
    }

    #[test]
    fn test_robot_envelope_to_json() {
        let envelope = RobotEnvelope::new("test.v1", serde_json::json!({"key": "value"}));
        let json = envelope.to_json();

        assert!(json.contains("test.v1"));
        assert!(json.contains("key"));
        assert!(json.contains("value"));
    }

    #[test]
    fn test_robot_health() {
        let envelope = robot_health();

        assert_eq!(envelope.schema_version, "vc.robot.health.v1");
        assert_eq!(envelope.data.overall.severity, "healthy");
        assert!(envelope.data.overall.score >= 0.0 && envelope.data.overall.score <= 1.0);
    }

    #[test]
    fn test_robot_triage() {
        let envelope = robot_triage();

        assert_eq!(envelope.schema_version, "vc.robot.triage.v1");
        assert!(!envelope.data.suggested_commands.is_empty());
    }

    #[test]
    fn test_health_data_serialization() {
        let health = HealthData {
            overall: OverallHealth {
                score: 0.85,
                severity: "warning".to_string(),
                active_alerts: 2,
                machine_count: 3,
                agent_count: 5,
            },
            machines: vec![],
            alerts_by_severity: AlertCounts {
                critical: 0,
                warning: 2,
                info: 1,
            },
        };

        let envelope = RobotEnvelope::new("vc.robot.health.v1", health);
        let json = envelope.to_json_pretty();

        // Verify it parses back
        let parsed: RobotEnvelope<HealthData> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.data.overall.score, 0.85);
        assert_eq!(parsed.data.alerts_by_severity.warning, 2);
    }
}
