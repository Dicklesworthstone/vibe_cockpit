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
// Status Data Structures
// ============================================================================

/// Comprehensive fleet status data for `vc robot status`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusData {
    /// Fleet-level summary
    pub fleet: FleetSummary,

    /// Per-machine status
    pub machines: Vec<MachineStatus>,

    /// Repository status summary
    pub repos: RepoSummary,

    /// Alert counts by severity
    pub alerts: AlertSummary,
}

/// Fleet-level summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetSummary {
    /// Total number of machines
    pub total_machines: u32,

    /// Number of online machines
    pub online: u32,

    /// Number of offline machines
    pub offline: u32,

    /// Overall fleet health score (0.0 to 1.0)
    pub health_score: f64,
}

/// Per-machine status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineStatus {
    /// Machine identifier
    pub id: String,

    /// Status: "online", "offline", "degraded"
    pub status: String,

    /// Last data collection timestamp
    pub last_seen: DateTime<Utc>,

    /// Health score (0.0 to 1.0)
    pub health_score: f64,

    /// Current metrics (None if offline)
    pub metrics: Option<MachineMetrics>,

    /// Top issue affecting this machine
    pub top_issue: Option<String>,
}

/// Machine resource metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineMetrics {
    /// CPU usage percentage (0-100)
    pub cpu_pct: f64,

    /// Memory usage percentage (0-100)
    pub mem_pct: f64,

    /// 5-minute load average
    pub load5: f64,

    /// Available disk percentage (0-100)
    pub disk_free_pct: f64,
}

/// Repository status summary
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepoSummary {
    /// Total tracked repositories
    pub total: u32,

    /// Repositories with uncommitted changes
    pub dirty: u32,

    /// Repositories ahead of remote
    pub ahead: u32,

    /// Repositories behind remote
    pub behind: u32,
}

/// Alert counts by severity level
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AlertSummary {
    /// Critical alerts
    pub critical: u32,

    /// High severity alerts
    pub high: u32,

    /// Medium severity alerts
    pub medium: u32,

    /// Low severity alerts
    pub low: u32,
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

/// Generate comprehensive fleet status
///
/// Returns machine status, repo summary, and alerts for agent consumption.
/// This is the primary command for agents to understand overall system state.
pub fn robot_status() -> RobotEnvelope<StatusData> {
    // Build fleet summary
    let fleet = FleetSummary {
        total_machines: 1,
        online: 1,
        offline: 0,
        health_score: 1.0,
    };

    // Build machine list with placeholder data
    let machines = vec![MachineStatus {
        id: "local".to_string(),
        status: "online".to_string(),
        last_seen: Utc::now(),
        health_score: 1.0,
        metrics: Some(MachineMetrics {
            cpu_pct: 0.0,
            mem_pct: 0.0,
            load5: 0.0,
            disk_free_pct: 100.0,
        }),
        top_issue: None,
    }];

    // Build repo summary (placeholder - will query ru collector data)
    let repos = RepoSummary {
        total: 0,
        dirty: 0,
        ahead: 0,
        behind: 0,
    };

    // Build alert summary (placeholder - will query alert tables)
    let alerts = AlertSummary {
        critical: 0,
        high: 0,
        medium: 0,
        low: 0,
    };

    let data = StatusData {
        fleet,
        machines,
        repos,
        alerts,
    };

    RobotEnvelope::new("vc.robot.status.v1", data)
        .add_warning("No collectors have run yet - data may be incomplete")
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

    // ========================================================================
    // Status Tests
    // ========================================================================

    #[test]
    fn test_robot_status() {
        let envelope = robot_status();

        assert_eq!(envelope.schema_version, "vc.robot.status.v1");
        assert!(envelope.data.fleet.health_score >= 0.0);
        assert!(envelope.data.fleet.health_score <= 1.0);
        assert!(!envelope.data.machines.is_empty());
    }

    #[test]
    fn test_status_data_serialization() {
        let status = StatusData {
            fleet: FleetSummary {
                total_machines: 4,
                online: 3,
                offline: 1,
                health_score: 0.85,
            },
            machines: vec![MachineStatus {
                id: "orko".to_string(),
                status: "online".to_string(),
                last_seen: Utc::now(),
                health_score: 0.91,
                metrics: Some(MachineMetrics {
                    cpu_pct: 45.2,
                    mem_pct: 68.0,
                    load5: 1.8,
                    disk_free_pct: 35.0,
                }),
                top_issue: None,
            }],
            repos: RepoSummary {
                total: 15,
                dirty: 2,
                ahead: 3,
                behind: 1,
            },
            alerts: AlertSummary {
                critical: 0,
                high: 1,
                medium: 2,
                low: 0,
            },
        };

        let envelope = RobotEnvelope::new("vc.robot.status.v1", status);
        let json = envelope.to_json_pretty();

        // Verify it parses back
        let parsed: RobotEnvelope<StatusData> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.data.fleet.total_machines, 4);
        assert_eq!(parsed.data.fleet.online, 3);
        assert_eq!(parsed.data.repos.dirty, 2);
        assert_eq!(parsed.data.alerts.high, 1);
    }

    #[test]
    fn test_fleet_summary_creation() {
        let fleet = FleetSummary {
            total_machines: 5,
            online: 4,
            offline: 1,
            health_score: 0.9,
        };

        assert_eq!(fleet.total_machines, fleet.online + fleet.offline);
    }

    #[test]
    fn test_machine_status_with_metrics() {
        let machine = MachineStatus {
            id: "test".to_string(),
            status: "online".to_string(),
            last_seen: Utc::now(),
            health_score: 0.95,
            metrics: Some(MachineMetrics {
                cpu_pct: 50.0,
                mem_pct: 60.0,
                load5: 1.5,
                disk_free_pct: 40.0,
            }),
            top_issue: None,
        };

        assert!(machine.metrics.is_some());
        let m = machine.metrics.unwrap();
        assert!(m.cpu_pct >= 0.0 && m.cpu_pct <= 100.0);
    }

    #[test]
    fn test_machine_status_offline() {
        let machine = MachineStatus {
            id: "offline-box".to_string(),
            status: "offline".to_string(),
            last_seen: Utc::now(),
            health_score: 0.0,
            metrics: None,
            top_issue: Some("no_response".to_string()),
        };

        assert!(machine.metrics.is_none());
        assert!(machine.top_issue.is_some());
    }

    #[test]
    fn test_repo_summary_defaults() {
        let repos = RepoSummary::default();
        assert_eq!(repos.total, 0);
        assert_eq!(repos.dirty, 0);
    }

    #[test]
    fn test_alert_summary_defaults() {
        let alerts = AlertSummary::default();
        assert_eq!(alerts.critical, 0);
        assert_eq!(alerts.high, 0);
        assert_eq!(alerts.medium, 0);
        assert_eq!(alerts.low, 0);
    }

    #[test]
    fn test_status_json_contains_expected_fields() {
        let envelope = robot_status();
        let json = envelope.to_json();

        assert!(json.contains("\"fleet\""));
        assert!(json.contains("\"machines\""));
        assert!(json.contains("\"repos\""));
        assert!(json.contains("\"alerts\""));
        assert!(json.contains("\"schema_version\""));
        assert!(json.contains("vc.robot.status.v1"));
    }
}
