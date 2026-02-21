//! `vc_query` - Query library for Vibe Cockpit
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

pub mod digest;

pub mod nl;
pub use cost::{
    AnomalySeverity, AnomalyType, ConfidenceFactors, CostAnomaly, CostAttribution, CostDriver,
    CostQueryBuilder, CostSummary, CostTrend, MachineCost, ProviderCost, ProviderPricing, RepoCost,
    estimate_cost,
};
pub use nl::{NlEngine, NlQueryResult, QueryIntent};

/// Query errors
#[derive(Error, Debug)]
pub enum QueryError {
    #[error("Store error: {0}")]
    StoreError(#[from] vc_store::StoreError),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

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
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Healthy,
    Info,
    Warning,
    Critical,
}

impl Severity {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Healthy => "healthy",
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Critical => "critical",
        }
    }
}

impl std::str::FromStr for Severity {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "healthy" => Ok(Severity::Healthy),
            "info" => Ok(Severity::Info),
            "warning" => Ok(Severity::Warning),
            "critical" => Ok(Severity::Critical),
            other => Err(format!("unknown severity: {other}")),
        }
    }
}

/// Default factor weights for health score calculation.
/// Each `factor_id` maps to a weight (higher = more important).
pub struct HealthWeights {
    pub sys_cpu: f64,
    pub sys_memory: f64,
    pub sys_disk: f64,
    pub sys_load: f64,
    pub agent_velocity: f64,
    pub rate_limit: f64,
    pub dcg_denies: f64,
    pub network: f64,
    pub rch_queue: f64,
    pub mail_backlog: f64,
    pub repo_cleanliness: f64,
    pub process_health: f64,
    pub data_freshness: f64,
}

impl Default for HealthWeights {
    fn default() -> Self {
        Self {
            sys_cpu: 1.5,
            sys_memory: 1.5,
            sys_disk: 2.0,
            sys_load: 1.0,
            agent_velocity: 1.5,
            rate_limit: 2.0,
            dcg_denies: 1.0,
            network: 0.5,
            rch_queue: 0.5,
            mail_backlog: 0.5,
            repo_cleanliness: 0.5,
            process_health: 1.0,
            data_freshness: 1.0,
        }
    }
}

impl HealthWeights {
    /// Look up weight by `factor_id`.
    #[must_use]
    pub fn weight_for(&self, factor_id: &str) -> f64 {
        match factor_id {
            "sys_cpu" => self.sys_cpu,
            "sys_memory" => self.sys_memory,
            "sys_disk" => self.sys_disk,
            "sys_load" => self.sys_load,
            "agent_velocity" => self.agent_velocity,
            "rate_limit" => self.rate_limit,
            "dcg_denies" => self.dcg_denies,
            "network" => self.network,
            "rch_queue" => self.rch_queue,
            "mail_backlog" => self.mail_backlog,
            "repo_cleanliness" => self.repo_cleanliness,
            "process_health" => self.process_health,
            "data_freshness" => self.data_freshness,
            _ => 1.0,
        }
    }
}

/// Compute an overall health score from a set of factors.
///
/// Algorithm:
/// 1. Compute weighted average of all factor scores
/// 2. Apply penalty for each critical factor (-0.1)
/// 3. Clamp result to [0.0, 1.0]
#[must_use]
pub fn compute_overall_score(factors: &[HealthFactor]) -> f64 {
    if factors.is_empty() {
        return 1.0;
    }

    let total_weight: f64 = factors.iter().map(|f| f.weight).sum();
    if total_weight < f64::EPSILON {
        return 1.0;
    }

    let weighted_sum: f64 = factors.iter().map(|f| f.score * f.weight).sum();
    let mut score = weighted_sum / total_weight;

    // Penalty for each critical factor
    let critical_count = factors
        .iter()
        .filter(|f| f.severity == Severity::Critical)
        .count();
    let critical_count_f64 = f64::from(u32::try_from(critical_count).unwrap_or(u32::MAX));
    score -= critical_count_f64 * 0.1;

    score.clamp(0.0, 1.0)
}

/// Classify a score into a severity level
#[must_use]
pub fn score_to_severity(score: f64) -> Severity {
    if score >= 0.8 {
        Severity::Healthy
    } else if score >= 0.6 {
        Severity::Info
    } else if score >= 0.3 {
        Severity::Warning
    } else {
        Severity::Critical
    }
}

/// Classify a metric value against thresholds into a factor.
/// Returns (score, severity) where score is 0.0=critical to 1.0=healthy.
///
/// - `inverted=false`: higher value is worse (CPU%, memory%, disk%)
/// - `inverted=true`: lower value is worse (free disk%, rate limit time remaining)
#[must_use]
pub fn classify_metric(
    value: f64,
    warning_threshold: f64,
    critical_threshold: f64,
    inverted: bool,
) -> (f64, Severity) {
    if inverted {
        // Lower value is worse. E.g., free disk <20% warning, <10% critical
        if value <= critical_threshold {
            (0.0, Severity::Critical)
        } else if value <= warning_threshold {
            let range = warning_threshold - critical_threshold;
            let score = if range > f64::EPSILON {
                ((value - critical_threshold) / range) * 0.4 + 0.3
            } else {
                0.5
            };
            (score, Severity::Warning)
        } else {
            (1.0, Severity::Healthy)
        }
    } else {
        // Higher value is worse. E.g., CPU >75% warning, >90% critical
        if value >= critical_threshold {
            (0.0, Severity::Critical)
        } else if value >= warning_threshold {
            let range = critical_threshold - warning_threshold;
            let score = if range > f64::EPSILON {
                (1.0 - (value - warning_threshold) / range) * 0.4 + 0.3
            } else {
                0.5
            };
            (score, Severity::Warning)
        } else {
            (1.0, Severity::Healthy)
        }
    }
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
    #[must_use]
    pub fn new(store: &'a VcStore) -> Self {
        Self { store }
    }

    /// Get fleet overview
    ///
    /// # Errors
    ///
    /// Returns [`QueryError`] if retrieval fails.
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

    /// Get health score for a machine by reading the latest stored summary.
    /// Falls back to score 1.0 (healthy) if no health data exists yet.
    ///
    /// # Errors
    ///
    /// Returns [`QueryError`] if underlying store queries fail.
    pub fn machine_health(&self, machine_id: &str) -> Result<HealthScore, QueryError> {
        let sql = format!(
            "SELECT overall_score, worst_factor_id, details_json \
             FROM health_summary \
             WHERE machine_id = '{}' \
             ORDER BY collected_at DESC LIMIT 1",
            vc_store::escape_sql_literal(machine_id)
        );
        let rows = self.store.query_json(&sql)?;
        if rows.is_empty() {
            return Ok(HealthScore {
                machine_id: machine_id.to_string(),
                overall_score: 1.0,
                factors: vec![],
                worst_factor: None,
            });
        }

        let row = &rows[0];
        let overall_score = row["overall_score"].as_f64().unwrap_or(1.0);
        let worst_factor = row["worst_factor_id"].as_str().map(String::from);

        // Load factors from health_factors table
        let factors_sql = format!(
            "SELECT factor_id, severity, score, weight, details_json \
             FROM health_factors \
             WHERE machine_id = '{}' \
             ORDER BY collected_at DESC, factor_id \
             LIMIT 20",
            vc_store::escape_sql_literal(machine_id)
        );
        let factor_rows = self.store.query_json(&factors_sql)?;
        let factors: Vec<HealthFactor> = factor_rows
            .iter()
            .map(|r| {
                let factor_id = r["factor_id"].as_str().unwrap_or("unknown").to_string();
                let severity_str = r["severity"].as_str().unwrap_or("healthy");
                
                let details_json_str = r["details_json"].as_str().unwrap_or("{}");
                let parsed_details: serde_json::Value = serde_json::from_str(details_json_str).unwrap_or_else(|_| serde_json::json!({}));
                let name = parsed_details["name"].as_str().unwrap_or(&factor_id.replace('_', " ")).to_string();
                let details = parsed_details["details"].as_str().unwrap_or("").to_string();

                HealthFactor {
                    name,
                    factor_id,
                    score: r["score"].as_f64().unwrap_or(1.0),
                    weight: r["weight"].as_f64().unwrap_or(1.0),
                    severity: severity_str.parse().unwrap_or(Severity::Healthy),
                    details,
                }
            })
            .collect();

        Ok(HealthScore {
            machine_id: machine_id.to_string(),
            overall_score,
            factors,
            worst_factor,
        })
    }

    /// Compute and persist health factors and summary for a machine.
    /// `factors` are the pre-computed health factors.
    ///
    /// # Errors
    ///
    /// Returns [`QueryError`] if serialization or database writes fail.
    pub fn persist_health_score(
        &self,
        machine_id: &str,
        factors: &[HealthFactor],
    ) -> Result<HealthScore, QueryError> {
        let overall_score = compute_overall_score(factors);

        let worst = factors
            .iter()
            .min_by(|a, b| {
                a.score
                    .partial_cmp(&b.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|f| f.factor_id.clone());

        let critical_count = factors
            .iter()
            .filter(|f| f.severity == Severity::Critical)
            .count();
        let warning_count = factors
            .iter()
            .filter(|f| f.severity == Severity::Warning)
            .count();

        let details = serde_json::json!({
            "factors": factors.iter().map(|f| serde_json::json!({
                "factor_id": f.factor_id,
                "score": f.score,
                "severity": f.severity.as_str(),
            })).collect::<Vec<_>>(),
        });

        // Insert health_factors rows
        for factor in factors {
            let details_json = serde_json::to_string(&serde_json::json!({
                "name": factor.name,
                "details": factor.details,
            }))?;

            let factor_sql = format!(
                "INSERT OR REPLACE INTO health_factors \
                 (machine_id, collected_at, factor_id, severity, score, weight, details_json) \
                 VALUES ('{}', CAST(current_timestamp AS TIMESTAMP), '{}', '{}', {}, {}, '{}')",
                vc_store::escape_sql_literal(machine_id),
                vc_store::escape_sql_literal(&factor.factor_id),
                factor.severity.as_str(),
                factor.score,
                factor.weight,
                vc_store::escape_sql_literal(&details_json),
            );
            self.store.execute_simple(&factor_sql)?;
        }

        // Insert health_summary row
        let details_str = serde_json::to_string(&details)?;
        let summary_sql = format!(
            "INSERT OR REPLACE INTO health_summary \
             (machine_id, collected_at, overall_score, worst_factor_id, \
              factor_count, critical_count, warning_count, details_json) \
             VALUES ('{}', CAST(current_timestamp AS TIMESTAMP), {}, {}, {}, {}, {}, '{}')",
            vc_store::escape_sql_literal(machine_id),
            overall_score,
            worst.as_ref().map_or("NULL".to_string(), |w| format!(
                "'{}'",
                vc_store::escape_sql_literal(w)
            )),
            factors.len(),
            critical_count,
            warning_count,
            vc_store::escape_sql_literal(&details_str),
        );
        self.store.execute_simple(&summary_sql)?;

        Ok(HealthScore {
            machine_id: machine_id.to_string(),
            overall_score,
            factors: factors.to_vec(),
            worst_factor: worst,
        })
    }

    /// List all stored health summaries (latest per machine)
    ///
    /// # Errors
    ///
    /// Returns [`QueryError`] if query execution fails.
    pub fn list_health_summaries(&self) -> Result<Vec<serde_json::Value>, QueryError> {
        let sql = "SELECT hs.machine_id, hs.overall_score, hs.worst_factor_id, \
                   hs.factor_count, hs.critical_count, hs.warning_count, \
                   CAST(hs.collected_at AS TEXT) AS collected_at \
                   FROM health_summary hs \
                   INNER JOIN ( \
                       SELECT machine_id, MAX(collected_at) AS max_ts \
                       FROM health_summary GROUP BY machine_id \
                   ) latest ON hs.machine_id = latest.machine_id AND hs.collected_at = latest.max_ts \
                   ORDER BY hs.overall_score ASC";
        Ok(self.store.query_json(sql)?)
    }

    /// Get recent alerts
    ///
    /// # Errors
    ///
    /// Returns [`QueryError`] if query execution fails.
    pub fn recent_alerts(&self, limit: usize) -> Result<Vec<serde_json::Value>, QueryError> {
        let sql = format!("SELECT * FROM alert_history ORDER BY fired_at DESC LIMIT {limit}");
        Ok(self.store.query_json(&sql)?)
    }

    /// Get machine list with status
    ///
    /// # Errors
    ///
    /// Returns [`QueryError`] if query execution fails.
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
        assert!((parsed.fleet_health_score - overview.fleet_health_score).abs() < f64::EPSILON);
    }

    // QueryBuilder tests (with in-memory store)
    #[test]
    fn test_query_builder_fleet_overview() {
        let store = VcStore::open_memory().unwrap();
        let builder = QueryBuilder::new(&store);

        let overview = builder.fleet_overview().unwrap();
        // Default placeholder returns zeros
        assert_eq!(overview.total_machines, 0);
        assert!((overview.fleet_health_score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_query_builder_machine_health() {
        let store = VcStore::open_memory().unwrap();
        let builder = QueryBuilder::new(&store);

        let health = builder.machine_health("test-machine").unwrap();
        assert_eq!(health.machine_id, "test-machine");
        assert!((health.overall_score - 1.0).abs() < f64::EPSILON);
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
                r"
                INSERT INTO alert_history (id, rule_id, fired_at, severity, title)
                VALUES (1, 'r1', TIMESTAMP '2026-01-01 00:00:00', 'warning', 'First');
                INSERT INTO alert_history (id, rule_id, fired_at, severity, title)
                VALUES (2, 'r2', TIMESTAMP '2026-01-02 00:00:00', 'critical', 'Second');
                ",
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
                r"
                INSERT INTO machines (machine_id, hostname)
                VALUES ('m2', 'zulu');
                INSERT INTO machines (machine_id, hostname)
                VALUES ('m1', 'alpha');
                ",
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

    // =============================================================================
    // Scoring Algorithm Tests
    // =============================================================================

    fn make_factor(id: &str, score: f64, weight: f64, severity: Severity) -> HealthFactor {
        HealthFactor {
            factor_id: id.to_string(),
            name: id.to_string(),
            score,
            weight,
            severity,
            details: String::new(),
        }
    }

    #[test]
    fn test_compute_overall_score_empty() {
        // No factors = fully healthy
        assert!((compute_overall_score(&[]) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_overall_score_single_healthy() {
        let factors = vec![make_factor("cpu", 1.0, 1.0, Severity::Healthy)];
        assert!((compute_overall_score(&factors) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_overall_score_weighted_average() {
        let factors = vec![
            make_factor("cpu", 1.0, 2.0, Severity::Healthy),
            make_factor("disk", 0.5, 2.0, Severity::Warning),
        ];
        // Weighted avg: (1.0*2 + 0.5*2) / 4 = 0.75
        let score = compute_overall_score(&factors);
        assert!((score - 0.75).abs() < 0.001);
    }

    #[test]
    fn test_compute_overall_score_critical_penalty() {
        let factors = vec![
            make_factor("cpu", 0.9, 1.0, Severity::Healthy),
            make_factor("disk", 0.0, 1.0, Severity::Critical),
        ];
        // Weighted avg: (0.9 + 0.0) / 2 = 0.45
        // Critical penalty: -0.1
        // Result: 0.35
        let score = compute_overall_score(&factors);
        assert!((score - 0.35).abs() < 0.001);
    }

    #[test]
    fn test_compute_overall_score_multiple_critical() {
        let factors = vec![
            make_factor("cpu", 0.0, 1.0, Severity::Critical),
            make_factor("disk", 0.0, 1.0, Severity::Critical),
            make_factor("mem", 0.0, 1.0, Severity::Critical),
        ];
        // Weighted avg: 0.0
        // Critical penalty: -0.3
        // Clamped to 0.0
        let score = compute_overall_score(&factors);
        assert!((score - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_overall_score_clamp_high() {
        // Can't exceed 1.0 even with all perfect scores
        let factors = vec![make_factor("cpu", 1.0, 10.0, Severity::Healthy)];
        let score = compute_overall_score(&factors);
        assert!(score <= 1.0);
    }

    // =============================================================================
    // Metric Classification Tests
    // =============================================================================

    #[test]
    fn test_classify_metric_healthy() {
        // CPU at 50%, warning at 75%, critical at 90%
        let (score, severity) = classify_metric(50.0, 75.0, 90.0, false);
        assert!((score - 1.0).abs() < f64::EPSILON);
        assert_eq!(severity, Severity::Healthy);
    }

    #[test]
    fn test_classify_metric_warning() {
        // CPU at 80%, warning at 75%, critical at 90%
        let (score, severity) = classify_metric(80.0, 75.0, 90.0, false);
        assert_eq!(severity, Severity::Warning);
        assert!(score > 0.3 && score < 0.7);
    }

    #[test]
    fn test_classify_metric_critical() {
        // CPU at 95%, warning at 75%, critical at 90%
        let (score, severity) = classify_metric(95.0, 75.0, 90.0, false);
        assert!((score - 0.0).abs() < f64::EPSILON);
        assert_eq!(severity, Severity::Critical);
    }

    #[test]
    fn test_classify_metric_inverted_healthy() {
        // Free disk at 50%, warning at 20%, critical at 10% (inverted: lower is worse)
        let (score, severity) = classify_metric(50.0, 20.0, 10.0, true);
        assert!((score - 1.0).abs() < f64::EPSILON);
        assert_eq!(severity, Severity::Healthy);
    }

    #[test]
    fn test_classify_metric_inverted_warning() {
        // Free disk at 15%, warning at 20%, critical at 10% (inverted)
        let (_score, severity) = classify_metric(15.0, 20.0, 10.0, true);
        assert_eq!(severity, Severity::Warning);
    }

    #[test]
    fn test_classify_metric_inverted_critical() {
        // Free disk at 5%, warning at 20%, critical at 10% (inverted)
        let (score, severity) = classify_metric(5.0, 20.0, 10.0, true);
        assert!((score - 0.0).abs() < f64::EPSILON);
        assert_eq!(severity, Severity::Critical);
    }

    #[test]
    fn test_classify_metric_exact_threshold() {
        // At exactly the critical threshold
        let (_, severity) = classify_metric(90.0, 75.0, 90.0, false);
        assert_eq!(severity, Severity::Critical);

        // At exactly the warning threshold
        let (_, severity) = classify_metric(75.0, 75.0, 90.0, false);
        assert_eq!(severity, Severity::Warning);
    }

    // =============================================================================
    // score_to_severity Tests
    // =============================================================================

    #[test]
    fn test_score_to_severity_healthy() {
        assert_eq!(score_to_severity(1.0), Severity::Healthy);
        assert_eq!(score_to_severity(0.8), Severity::Healthy);
    }

    #[test]
    fn test_score_to_severity_info() {
        assert_eq!(score_to_severity(0.7), Severity::Info);
        assert_eq!(score_to_severity(0.6), Severity::Info);
    }

    #[test]
    fn test_score_to_severity_warning() {
        assert_eq!(score_to_severity(0.5), Severity::Warning);
        assert_eq!(score_to_severity(0.3), Severity::Warning);
    }

    #[test]
    fn test_score_to_severity_critical() {
        assert_eq!(score_to_severity(0.29), Severity::Critical);
        assert_eq!(score_to_severity(0.0), Severity::Critical);
    }

    // =============================================================================
    // Severity Parsing Tests
    // =============================================================================

    #[test]
    fn test_severity_as_str() {
        assert_eq!(Severity::Healthy.as_str(), "healthy");
        assert_eq!(Severity::Info.as_str(), "info");
        assert_eq!(Severity::Warning.as_str(), "warning");
        assert_eq!(Severity::Critical.as_str(), "critical");
    }

    #[test]
    fn test_severity_from_str_roundtrip() {
        for s in ["healthy", "info", "warning", "critical"] {
            let parsed: Severity = s.parse().unwrap();
            assert_eq!(parsed.as_str(), s);
        }
    }

    #[test]
    fn test_severity_from_str_invalid() {
        assert!("invalid".parse::<Severity>().is_err());
    }

    #[test]
    fn test_severity_ord() {
        assert!(Severity::Healthy < Severity::Info);
        assert!(Severity::Info < Severity::Warning);
        assert!(Severity::Warning < Severity::Critical);
    }

    // =============================================================================
    // HealthWeights Tests
    // =============================================================================

    #[test]
    fn test_health_weights_default() {
        let weights = HealthWeights::default();
        assert!(weights.sys_disk > weights.sys_load);
        assert!(weights.rate_limit > weights.mail_backlog);
    }

    #[test]
    fn test_health_weights_lookup() {
        let weights = HealthWeights::default();
        assert!((weights.weight_for("sys_cpu") - 1.5).abs() < f64::EPSILON);
        assert!((weights.weight_for("sys_disk") - 2.0).abs() < f64::EPSILON);
        assert!((weights.weight_for("unknown_factor") - 1.0).abs() < f64::EPSILON);
    }

    // =============================================================================
    // Multi-Factor Scenario Tests
    // =============================================================================

    #[test]
    fn test_scenario_all_healthy() {
        let factors = vec![
            make_factor("sys_cpu", 1.0, 1.5, Severity::Healthy),
            make_factor("sys_memory", 1.0, 1.5, Severity::Healthy),
            make_factor("sys_disk", 1.0, 2.0, Severity::Healthy),
            make_factor("data_freshness", 1.0, 1.0, Severity::Healthy),
        ];
        let score = compute_overall_score(&factors);
        assert!((score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_scenario_disk_critical_mail_backlog() {
        let factors = vec![
            make_factor("sys_cpu", 1.0, 1.5, Severity::Healthy),
            make_factor("sys_memory", 0.8, 1.5, Severity::Healthy),
            make_factor("sys_disk", 0.0, 2.0, Severity::Critical),
            make_factor("mail_backlog", 0.4, 0.5, Severity::Warning),
            make_factor("data_freshness", 1.0, 1.0, Severity::Healthy),
        ];
        let score = compute_overall_score(&factors);
        // Weighted avg + critical penalty
        // (1.0*1.5 + 0.8*1.5 + 0.0*2.0 + 0.4*0.5 + 1.0*1.0) / (1.5+1.5+2.0+0.5+1.0) = 3.9/6.5 ≈ 0.60
        // -0.1 for critical = ~0.50
        assert!(score < 0.6);
        assert!(score > 0.4);
    }

    #[test]
    fn test_scenario_multiple_warnings() {
        let factors = vec![
            make_factor("sys_cpu", 0.5, 1.5, Severity::Warning),
            make_factor("sys_memory", 0.5, 1.5, Severity::Warning),
            make_factor("sys_disk", 0.5, 2.0, Severity::Warning),
        ];
        let score = compute_overall_score(&factors);
        // All 0.5 → weighted avg = 0.5, no critical penalty
        assert!((score - 0.5).abs() < f64::EPSILON);
    }

    // =============================================================================
    // Persist and Retrieve Tests
    // =============================================================================

    #[test]
    fn test_persist_and_retrieve_health_score() {
        let store = VcStore::open_memory().unwrap();
        let qb = QueryBuilder::new(&store);

        let factors = vec![
            make_factor("sys_cpu", 0.9, 1.5, Severity::Healthy),
            make_factor("sys_disk", 0.3, 2.0, Severity::Warning),
        ];

        let result = qb.persist_health_score("m1", &factors).unwrap();
        assert_eq!(result.machine_id, "m1");
        assert!(result.overall_score < 1.0);
        assert_eq!(result.worst_factor, Some("sys_disk".to_string()));

        // Retrieve it back
        let retrieved = qb.machine_health("m1").unwrap();
        assert_eq!(retrieved.machine_id, "m1");
        assert!(retrieved.overall_score < 1.0);
    }

    #[test]
    fn test_list_health_summaries() {
        let store = VcStore::open_memory().unwrap();
        let qb = QueryBuilder::new(&store);

        // Persist scores for two machines
        let factors_m1 = vec![make_factor("sys_cpu", 0.9, 1.0, Severity::Healthy)];
        let factors_m2 = vec![make_factor("sys_cpu", 0.3, 1.0, Severity::Warning)];

        qb.persist_health_score("m1", &factors_m1).unwrap();
        qb.persist_health_score("m2", &factors_m2).unwrap();

        let summaries = qb.list_health_summaries().unwrap();
        assert_eq!(summaries.len(), 2);
        // Ordered by score ASC, so worst first
        assert_eq!(summaries[0]["machine_id"].as_str().unwrap(), "m2");
    }

    #[test]
    fn test_machine_health_no_data() {
        let store = VcStore::open_memory().unwrap();
        let qb = QueryBuilder::new(&store);

        // No data - should return default healthy score
        let health = qb.machine_health("nonexistent").unwrap();
        assert!((health.overall_score - 1.0).abs() < f64::EPSILON);
        assert!(health.factors.is_empty());
    }
}
