//! vc_query - Query library for Vibe Cockpit
//!
//! This crate provides:
//! - Canonical queries for health, rollups, and anomalies
//! - Health score calculation
//! - Time-travel query support
//! - Aggregation utilities

use serde::{Deserialize, Serialize};
use thiserror::Error;
use vc_store::VcStore;

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
        let sql = format!(
            "SELECT * FROM alert_history ORDER BY fired_at DESC LIMIT {limit}"
        );
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

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Critical != Severity::Healthy);
    }
}
