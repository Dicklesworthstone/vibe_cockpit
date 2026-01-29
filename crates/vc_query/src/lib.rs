//! vc_query - Query library for Vibe Cockpit
//!
//! This crate provides:
//! - Canonical queries for health, rollups, and anomalies
//! - Health score calculation
//! - Time-travel query support
//! - Aggregation utilities
//! - Query guardrails and safe templates

use serde::{Deserialize, Serialize};
use thiserror::Error;
use vc_store::VcStore;

pub mod guardrails;
pub use guardrails::{GuardrailConfig, QueryTemplate, QueryValidator, ValidationError};

pub mod cost;
pub use cost::{
    AnomalySeverity, AnomalyType, ConfidenceFactors, CostAnomaly, CostAttribution,
    CostDriver, CostQueryBuilder, CostSummary, CostTrend, MachineCost, ProviderCost,
    ProviderPricing, RepoCost, estimate_cost,
};

/// Query errors
#[derive(Error, Debug)]
pub enum QueryError {
    #[error("Store error: {0}")]
    StoreError(#[from] vc_store::StoreError),

    #[error("Invalid query: {0}")]
    InvalidQuery(String),
}

/// Health score for a machine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthScore {
    pub machine_id: String,
    pub overall_score: f64,
    pub factors: Vec<HealthFactor>,
    pub worst_factor: Option<String>,
}

/// Individual health factor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthFactor {
    pub factor_id: String,
    pub name: String,
    pub score: f64,
    pub weight: f64,
    pub severity: Severity,
    pub details: String,
}

/// Severity levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Healthy,
    Info,
    Warning,
    Critical,
}

/// Fleet overview summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetOverview {
    pub total_machines: usize,
    pub online_machines: usize,
    pub offline_machines: usize,
    pub total_agents: usize,
    pub active_agents: usize,
    pub fleet_health_score: f64,
    pub worst_machine: Option<String>,
    pub active_alerts: usize,
    pub pending_approvals: usize,
}

/// Query builder for common operations
pub struct QueryBuilder<'a> {
    store: &'a VcStore,
}

impl<'a> QueryBuilder<'a> {
    pub fn new(store: &'a VcStore) -> Self {
        Self { store }
    }

    /// Get fleet overview
    pub fn fleet_overview(&self) -> Result<FleetOverview, QueryError> {
        // Placeholder implementation
        Ok(FleetOverview {
            total_machines: 0,
            online_machines: 0,
            offline_machines: 0,
            total_agents: 0,
            active_agents: 0,
            fleet_health_score: 1.0,
            worst_machine: None,
            active_alerts: 0,
            pending_approvals: 0,
        })
    }

    /// Get health score for a machine
    pub fn machine_health(&self, machine_id: &str) -> Result<HealthScore, QueryError> {
        // Placeholder implementation
        Ok(HealthScore {
            machine_id: machine_id.to_string(),
            overall_score: 1.0,
            factors: vec![],
            worst_factor: None,
        })
    }

    /// Get recent alerts
    pub fn recent_alerts(&self, limit: usize) -> Result<Vec<serde_json::Value>, QueryError> {
        let sql = format!("SELECT * FROM alert_history ORDER BY fired_at DESC LIMIT {limit}");
        Ok(self.store.query_json(&sql)?)
    }

    /// Get machine list with status
    pub fn machines(&self) -> Result<Vec<serde_json::Value>, QueryError> {
        let sql = "SELECT * FROM machines ORDER BY hostname";
        Ok(self.store.query_json(sql)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Severity tests
    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Critical != Severity::Healthy);
    }

    #[test]
    fn test_severity_variants() {
        assert_ne!(Severity::Healthy, Severity::Info);
        assert_ne!(Severity::Info, Severity::Warning);
        assert_ne!(Severity::Warning, Severity::Critical);
    }

    #[test]
    fn test_severity_serialization() {
        let healthy = Severity::Healthy;
        let json = serde_json::to_string(&healthy).unwrap();
        assert_eq!(json, "\"healthy\"");

        let critical = Severity::Critical;
        let json = serde_json::to_string(&critical).unwrap();
        assert_eq!(json, "\"critical\"");
    }

    #[test]
    fn test_severity_deserialization() {
        let healthy: Severity = serde_json::from_str("\"healthy\"").unwrap();
        assert_eq!(healthy, Severity::Healthy);

        let warning: Severity = serde_json::from_str("\"warning\"").unwrap();
        assert_eq!(warning, Severity::Warning);
    }

    // HealthScore tests
    #[test]
    fn test_health_score_creation() {
        let score = HealthScore {
            machine_id: "test-machine".to_string(),
            overall_score: 0.85,
            factors: vec![],
            worst_factor: None,
        };
        assert_eq!(score.machine_id, "test-machine");
        assert!((score.overall_score - 0.85).abs() < f64::EPSILON);
    }

    #[test]
    fn test_health_score_with_factors() {
        let factors = vec![
            HealthFactor {
                factor_id: "cpu".to_string(),
                name: "CPU Usage".to_string(),
                score: 0.9,
                weight: 1.0,
                severity: Severity::Healthy,
                details: "CPU is fine".to_string(),
            },
            HealthFactor {
                factor_id: "mem".to_string(),
                name: "Memory Usage".to_string(),
                score: 0.6,
                weight: 1.0,
                severity: Severity::Warning,
                details: "Memory usage high".to_string(),
            },
        ];

        let score = HealthScore {
            machine_id: "machine1".to_string(),
            overall_score: 0.75,
            factors: factors.clone(),
            worst_factor: Some("mem".to_string()),
        };

        assert_eq!(score.factors.len(), 2);
        assert_eq!(score.worst_factor, Some("mem".to_string()));
    }

    #[test]
    fn test_health_score_serialization() {
        let score = HealthScore {
            machine_id: "m1".to_string(),
            overall_score: 1.0,
            factors: vec![],
            worst_factor: None,
        };

        let json = serde_json::to_string(&score).unwrap();
        assert!(json.contains("\"machine_id\":\"m1\""));
        assert!(json.contains("\"overall_score\":1.0"));
    }

    // HealthFactor tests
    #[test]
    fn test_health_factor_creation() {
        let factor = HealthFactor {
            factor_id: "disk".to_string(),
            name: "Disk Space".to_string(),
            score: 0.5,
            weight: 2.0,
            severity: Severity::Critical,
            details: "Disk almost full".to_string(),
        };

        assert_eq!(factor.factor_id, "disk");
        assert_eq!(factor.severity, Severity::Critical);
        assert!((factor.weight - 2.0).abs() < f64::EPSILON);
    }

    // FleetOverview tests
    #[test]
    fn test_fleet_overview_defaults() {
        let overview = FleetOverview {
            total_machines: 5,
            online_machines: 4,
            offline_machines: 1,
            total_agents: 20,
            active_agents: 18,
            fleet_health_score: 0.9,
            worst_machine: Some("machine3".to_string()),
            active_alerts: 2,
            pending_approvals: 0,
        };

        assert_eq!(overview.total_machines, 5);
        assert_eq!(
            overview.online_machines + overview.offline_machines,
            overview.total_machines
        );
        assert!(overview.active_agents <= overview.total_agents);
    }

    #[test]
    fn test_fleet_overview_serialization() {
        let overview = FleetOverview {
            total_machines: 1,
            online_machines: 1,
            offline_machines: 0,
            total_agents: 5,
            active_agents: 5,
            fleet_health_score: 1.0,
            worst_machine: None,
            active_alerts: 0,
            pending_approvals: 0,
        };

        let json = serde_json::to_string(&overview).unwrap();
        let parsed: FleetOverview = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.total_machines, overview.total_machines);
        assert_eq!(parsed.fleet_health_score, overview.fleet_health_score);
    }

    // QueryBuilder tests (with in-memory store)
    #[test]
    fn test_query_builder_fleet_overview() {
        let store = VcStore::open_memory().unwrap();
        let builder = QueryBuilder::new(&store);

        let overview = builder.fleet_overview().unwrap();
        // Default placeholder returns zeros
        assert_eq!(overview.total_machines, 0);
        assert_eq!(overview.fleet_health_score, 1.0);
    }

    #[test]
    fn test_query_builder_machine_health() {
        let store = VcStore::open_memory().unwrap();
        let builder = QueryBuilder::new(&store);

        let health = builder.machine_health("test-machine").unwrap();
        assert_eq!(health.machine_id, "test-machine");
        assert_eq!(health.overall_score, 1.0);
    }

    #[test]
    fn test_query_builder_recent_alerts_empty() {
        let store = VcStore::open_memory().unwrap();
        let builder = QueryBuilder::new(&store);

        let alerts = builder.recent_alerts(10).unwrap();
        assert!(alerts.is_empty());
    }

    #[test]
    fn test_query_builder_recent_alerts_ordering() {
        let store = VcStore::open_memory().unwrap();
        store
            .execute_batch(
                r#"
                INSERT INTO alert_history (id, rule_id, fired_at, severity, title)
                VALUES (1, 'r1', TIMESTAMP '2026-01-01 00:00:00', 'warning', 'First');
                INSERT INTO alert_history (id, rule_id, fired_at, severity, title)
                VALUES (2, 'r2', TIMESTAMP '2026-01-02 00:00:00', 'critical', 'Second');
                "#,
            )
            .unwrap();

        let builder = QueryBuilder::new(&store);
        let alerts = builder.recent_alerts(1).unwrap();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0]["title"].as_str().unwrap(), "Second");
    }

    #[test]
    fn test_query_builder_machines_ordering() {
        let store = VcStore::open_memory().unwrap();
        store
            .execute_batch(
                r#"
                INSERT INTO machines (machine_id, hostname)
                VALUES ('m2', 'zulu');
                INSERT INTO machines (machine_id, hostname)
                VALUES ('m1', 'alpha');
                "#,
            )
            .unwrap();

        let builder = QueryBuilder::new(&store);
        let machines = builder.machines().unwrap();
        assert_eq!(machines.len(), 2);
        assert_eq!(machines[0]["hostname"].as_str().unwrap(), "alpha");
    }

    #[test]
    fn test_query_error_display() {
        let err = QueryError::InvalidQuery("bad sql".to_string());
        assert!(err.to_string().contains("Invalid query"));
        assert!(err.to_string().contains("bad sql"));
    }
}
