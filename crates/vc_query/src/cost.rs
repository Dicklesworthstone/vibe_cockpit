//! Cost attribution analytics
//!
//! This module provides:
//! - Cost estimation based on token usage and provider pricing
//! - Attribution to repos, machines, and agent types
//! - Confidence scoring for attribution quality
//! - Cost anomaly detection

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use vc_store::{escape_sql_literal, VcStore};

use crate::QueryError;

/// Cost attribution for a single entity (repo/machine/agent)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostAttribution {
    pub repo_id: Option<String>,
    pub repo_path: Option<String>,
    pub machine_id: Option<String>,
    pub agent_type: Option<String>,
    pub provider: String,
    pub estimated_cost_usd: f64,
    pub tokens_input: i64,
    pub tokens_output: i64,
    pub tokens_total: i64,
    pub sessions_count: i32,
    pub requests_count: i32,
    pub confidence: f64,
    pub confidence_factors: ConfidenceFactors,
}

/// Breakdown of confidence score factors
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConfidenceFactors {
    /// Session to repo mapping quality (0.0 to 1.0)
    pub session_mapping: f64,
    /// Time window overlap quality (0.0 to 1.0)
    pub time_window_match: f64,
    /// Data completeness (0.0 to 1.0)
    pub data_completeness: f64,
    /// Notes about confidence calculation
    pub notes: Vec<String>,
}

impl ConfidenceFactors {
    /// Calculate overall confidence score as weighted average
    #[must_use]
    pub fn overall(&self) -> f64 {
        // Weights: session_mapping is most important
        let weights = [0.5, 0.3, 0.2];
        let values = [
            self.session_mapping,
            self.time_window_match,
            self.data_completeness,
        ];

        let weighted_sum: f64 = values.iter().zip(weights.iter()).map(|(v, w)| v * w).sum();
        weighted_sum.clamp(0.0, 1.0)
    }
}

/// Provider pricing information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderPricing {
    pub provider: String,
    pub model: String,
    pub price_per_1k_input_tokens: f64,
    pub price_per_1k_output_tokens: f64,
}

impl ProviderPricing {
    /// Calculate cost for given token counts
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn calculate_cost(&self, input_tokens: i64, output_tokens: i64) -> f64 {
        let input_cost = (input_tokens as f64 / 1000.0) * self.price_per_1k_input_tokens;
        let output_cost = (output_tokens as f64 / 1000.0) * self.price_per_1k_output_tokens;
        input_cost + output_cost
    }
}

/// Cost summary for a time period
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostSummary {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub total_cost_usd: f64,
    pub total_tokens: i64,
    pub by_provider: Vec<ProviderCost>,
    pub by_repo: Vec<RepoCost>,
    pub by_machine: Vec<MachineCost>,
    pub top_cost_drivers: Vec<CostDriver>,
    pub avg_confidence: f64,
}

/// Cost breakdown by provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCost {
    pub provider: String,
    pub cost_usd: f64,
    pub tokens: i64,
    pub percentage: f64,
}

/// Cost breakdown by repository
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoCost {
    pub repo_id: String,
    pub repo_path: Option<String>,
    pub cost_usd: f64,
    pub tokens: i64,
    pub confidence: f64,
}

/// Cost breakdown by machine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineCost {
    pub machine_id: String,
    pub cost_usd: f64,
    pub tokens: i64,
}

/// Top cost driver (entity causing significant spend)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostDriver {
    pub driver_type: String, // "repo", "machine", "agent_type", "provider"
    pub driver_id: String,
    pub cost_usd: f64,
    pub percentage_of_total: f64,
    pub trend: CostTrend,
}

/// Cost trend indicator
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CostTrend {
    Increasing,
    Stable,
    Decreasing,
    Unknown,
}

/// Cost anomaly
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostAnomaly {
    pub anomaly_type: AnomalyType,
    pub severity: AnomalySeverity,
    pub repo_id: Option<String>,
    pub machine_id: Option<String>,
    pub provider: Option<String>,
    pub expected_cost_usd: f64,
    pub actual_cost_usd: f64,
    pub deviation_percent: f64,
    pub details: String,
}

/// Types of cost anomalies
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnomalyType {
    Spike,
    Drift,
    UnusualPattern,
}

/// Anomaly severity levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AnomalySeverity {
    Info,
    Warning,
    Critical,
}

/// Cost attribution query builder
pub struct CostQueryBuilder<'a> {
    store: &'a VcStore,
}

impl<'a> CostQueryBuilder<'a> {
    #[must_use]
    pub fn new(store: &'a VcStore) -> Self {
        Self { store }
    }

    /// Get pricing for a specific provider/model
    ///
    /// # Errors
    ///
    /// Returns [`QueryError`] if querying the pricing table fails.
    pub fn get_pricing(
        &self,
        provider: &str,
        model: &str,
    ) -> Result<Option<ProviderPricing>, QueryError> {
        let sql = format!(
            "SELECT provider, model, price_per_1k_input_tokens, price_per_1k_output_tokens \
             FROM provider_pricing \
             WHERE provider = '{}' AND model = '{}' \
             AND (effective_until IS NULL OR effective_until > current_timestamp) \
             ORDER BY effective_from DESC LIMIT 1",
            escape_sql_literal(provider),
            escape_sql_literal(model)
        );

        let rows = self.store.query_json(&sql)?;
        if let Some(row) = rows.into_iter().next() {
            Ok(Some(ProviderPricing {
                provider: row["provider"].as_str().unwrap_or_default().to_string(),
                model: row["model"].as_str().unwrap_or_default().to_string(),
                price_per_1k_input_tokens: row["price_per_1k_input_tokens"].as_f64().unwrap_or(0.0),
                price_per_1k_output_tokens: row["price_per_1k_output_tokens"]
                    .as_f64()
                    .unwrap_or(0.0),
            }))
        } else {
            Ok(None)
        }
    }

    /// Get all available pricing models
    ///
    /// # Errors
    ///
    /// Returns [`QueryError`] if querying pricing rows fails.
    pub fn list_pricing(&self) -> Result<Vec<ProviderPricing>, QueryError> {
        let sql = "SELECT DISTINCT ON (provider, model) provider, model, \
                   price_per_1k_input_tokens, price_per_1k_output_tokens \
                   FROM provider_pricing \
                   WHERE effective_until IS NULL OR effective_until > current_timestamp \
                   ORDER BY provider, model, effective_from DESC";

        let rows = self.store.query_json(sql)?;
        Ok(rows
            .into_iter()
            .map(|row| ProviderPricing {
                provider: row["provider"].as_str().unwrap_or_default().to_string(),
                model: row["model"].as_str().unwrap_or_default().to_string(),
                price_per_1k_input_tokens: row["price_per_1k_input_tokens"].as_f64().unwrap_or(0.0),
                price_per_1k_output_tokens: row["price_per_1k_output_tokens"]
                    .as_f64()
                    .unwrap_or(0.0),
            })
            .collect())
    }

    /// Get cost summary for a time period
    ///
    /// # Errors
    ///
    /// Returns [`QueryError`] if any underlying summary/breakdown query fails.
    pub fn cost_summary(
        &self,
        since: DateTime<Utc>,
        until: Option<DateTime<Utc>>,
    ) -> Result<CostSummary, QueryError> {
        let until = until.unwrap_or_else(Utc::now);

        // Query cost attribution snapshots
        let sql = format!(
            "SELECT \
                SUM(estimated_cost_usd) as total_cost, \
                SUM(tokens_total) as total_tokens, \
                AVG(confidence) as avg_confidence, \
                COUNT(*) as row_count \
             FROM cost_attribution_snapshot \
             WHERE collected_at >= '{}' AND collected_at <= '{}'",
            since.to_rfc3339(),
            until.to_rfc3339()
        );

        let summary_rows = self.store.query_json(&sql)?;
        let summary = summary_rows.into_iter().next().unwrap_or_default();

        let total_cost = summary["total_cost"].as_f64().unwrap_or(0.0);
        let total_tokens = summary["total_tokens"].as_i64().unwrap_or(0);
        let avg_confidence = summary["avg_confidence"].as_f64().unwrap_or(0.0);

        // Get breakdown by provider
        let by_provider = self.cost_by_provider(since, until)?;

        // Get breakdown by repo
        let by_repo = self.cost_by_repo(since, until)?;

        // Get breakdown by machine
        let by_machine = self.cost_by_machine(since, until)?;

        // Calculate top cost drivers
        let top_cost_drivers =
            Self::calculate_top_drivers(&by_provider, &by_repo, &by_machine, total_cost);

        Ok(CostSummary {
            period_start: since,
            period_end: until,
            total_cost_usd: total_cost,
            total_tokens,
            by_provider,
            by_repo,
            by_machine,
            top_cost_drivers,
            avg_confidence,
        })
    }

    /// Get cost breakdown by provider
    fn cost_by_provider(
        &self,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> Result<Vec<ProviderCost>, QueryError> {
        let sql = format!(
            "SELECT \
                provider, \
                SUM(estimated_cost_usd) as cost_usd, \
                SUM(tokens_total) as tokens \
             FROM cost_attribution_snapshot \
             WHERE collected_at >= '{}' AND collected_at <= '{}' \
             GROUP BY provider \
             ORDER BY cost_usd DESC",
            since.to_rfc3339(),
            until.to_rfc3339()
        );

        let rows = self.store.query_json(&sql)?;
        let total: f64 = rows
            .iter()
            .map(|r| r["cost_usd"].as_f64().unwrap_or(0.0))
            .sum();

        Ok(rows
            .into_iter()
            .map(|row| {
                let cost = row["cost_usd"].as_f64().unwrap_or(0.0);
                ProviderCost {
                    provider: row["provider"].as_str().unwrap_or("unknown").to_string(),
                    cost_usd: cost,
                    tokens: row["tokens"].as_i64().unwrap_or(0),
                    percentage: if total > 0.0 {
                        (cost / total) * 100.0
                    } else {
                        0.0
                    },
                }
            })
            .collect())
    }

    /// Get cost breakdown by repository
    fn cost_by_repo(
        &self,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> Result<Vec<RepoCost>, QueryError> {
        let sql = format!(
            "SELECT \
                repo_id, \
                repo_path, \
                SUM(estimated_cost_usd) as cost_usd, \
                SUM(tokens_total) as tokens, \
                AVG(confidence) as confidence \
             FROM cost_attribution_snapshot \
             WHERE collected_at >= '{}' AND collected_at <= '{}' \
               AND repo_id IS NOT NULL \
             GROUP BY repo_id, repo_path \
             ORDER BY cost_usd DESC \
             LIMIT 20",
            since.to_rfc3339(),
            until.to_rfc3339()
        );

        let rows = self.store.query_json(&sql)?;

        Ok(rows
            .into_iter()
            .map(|row| RepoCost {
                repo_id: row["repo_id"].as_str().unwrap_or("").to_string(),
                repo_path: row["repo_path"].as_str().map(String::from),
                cost_usd: row["cost_usd"].as_f64().unwrap_or(0.0),
                tokens: row["tokens"].as_i64().unwrap_or(0),
                confidence: row["confidence"].as_f64().unwrap_or(0.0),
            })
            .collect())
    }

    /// Get cost breakdown by machine
    fn cost_by_machine(
        &self,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> Result<Vec<MachineCost>, QueryError> {
        let sql = format!(
            "SELECT \
                machine_id, \
                SUM(estimated_cost_usd) as cost_usd, \
                SUM(tokens_total) as tokens \
             FROM cost_attribution_snapshot \
             WHERE collected_at >= '{}' AND collected_at <= '{}' \
               AND machine_id IS NOT NULL \
             GROUP BY machine_id \
             ORDER BY cost_usd DESC \
             LIMIT 20",
            since.to_rfc3339(),
            until.to_rfc3339()
        );

        let rows = self.store.query_json(&sql)?;

        Ok(rows
            .into_iter()
            .map(|row| MachineCost {
                machine_id: row["machine_id"].as_str().unwrap_or("").to_string(),
                cost_usd: row["cost_usd"].as_f64().unwrap_or(0.0),
                tokens: row["tokens"].as_i64().unwrap_or(0),
            })
            .collect())
    }

    /// Calculate top cost drivers from breakdown data
    fn calculate_top_drivers(
        by_provider: &[ProviderCost],
        by_repo: &[RepoCost],
        by_machine: &[MachineCost],
        total_cost: f64,
    ) -> Vec<CostDriver> {
        let mut drivers = Vec::new();

        // Top provider
        if let Some(top) = by_provider.first()
            && top.cost_usd > 0.0
        {
            drivers.push(CostDriver {
                driver_type: "provider".to_string(),
                driver_id: top.provider.clone(),
                cost_usd: top.cost_usd,
                percentage_of_total: top.percentage,
                trend: CostTrend::Unknown, // Would need historical data
            });
        }

        // Top repo
        if let Some(top) = by_repo.first()
            && top.cost_usd > 0.0
        {
            let percentage = if total_cost > 0.0 {
                (top.cost_usd / total_cost) * 100.0
            } else {
                0.0
            };
            drivers.push(CostDriver {
                driver_type: "repo".to_string(),
                driver_id: top.repo_id.clone(),
                cost_usd: top.cost_usd,
                percentage_of_total: percentage,
                trend: CostTrend::Unknown,
            });
        }

        // Top machine
        if let Some(top) = by_machine.first()
            && top.cost_usd > 0.0
        {
            let percentage = if total_cost > 0.0 {
                (top.cost_usd / total_cost) * 100.0
            } else {
                0.0
            };
            drivers.push(CostDriver {
                driver_type: "machine".to_string(),
                driver_id: top.machine_id.clone(),
                cost_usd: top.cost_usd,
                percentage_of_total: percentage,
                trend: CostTrend::Unknown,
            });
        }

        // Sort by cost descending
        drivers.sort_by(|a, b| {
            b.cost_usd
                .partial_cmp(&a.cost_usd)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        drivers.truncate(5);

        drivers
    }

    /// Detect cost anomalies
    ///
    /// # Errors
    ///
    /// Returns [`QueryError`] if anomaly baseline query execution fails.
    pub fn detect_anomalies(&self, threshold_percent: f64) -> Result<Vec<CostAnomaly>, QueryError> {
        // Compare recent costs to historical baseline
        let sql = "SELECT \
                provider, \
                machine_id, \
                repo_id, \
                SUM(CASE WHEN collected_at >= current_timestamp - INTERVAL '1 day' \
                    THEN estimated_cost_usd ELSE 0 END) as recent_cost, \
                AVG(CASE WHEN collected_at < current_timestamp - INTERVAL '1 day' \
                    THEN estimated_cost_usd END) as baseline_cost \
             FROM cost_attribution_snapshot \
             WHERE collected_at >= current_timestamp - INTERVAL '30 days' \
             GROUP BY provider, machine_id, repo_id \
             HAVING recent_cost > 0 AND baseline_cost > 0";

        let rows = self.store.query_json(sql)?;
        let mut anomalies = Vec::new();

        for row in rows {
            let recent = row["recent_cost"].as_f64().unwrap_or(0.0);
            let baseline = row["baseline_cost"].as_f64().unwrap_or(0.0);

            if baseline > 0.0 {
                let deviation = ((recent - baseline) / baseline) * 100.0;

                if deviation.abs() >= threshold_percent {
                    let severity = if deviation.abs() >= 100.0 {
                        AnomalySeverity::Critical
                    } else if deviation.abs() >= 50.0 {
                        AnomalySeverity::Warning
                    } else {
                        AnomalySeverity::Info
                    };

                    let anomaly_type = if deviation > 0.0 {
                        AnomalyType::Spike
                    } else {
                        AnomalyType::Drift
                    };

                    anomalies.push(CostAnomaly {
                        anomaly_type,
                        severity,
                        repo_id: row["repo_id"].as_str().map(String::from),
                        machine_id: row["machine_id"].as_str().map(String::from),
                        provider: row["provider"].as_str().map(String::from),
                        expected_cost_usd: baseline,
                        actual_cost_usd: recent,
                        deviation_percent: deviation,
                        details: format!(
                            "Cost changed by {deviation:.1}% from baseline ${baseline:.2} to ${recent:.2}"
                        ),
                    });
                }
            }
        }

        // Sort by deviation magnitude
        anomalies.sort_by(|a, b| {
            b.deviation_percent
                .abs()
                .partial_cmp(&a.deviation_percent.abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(anomalies)
    }

    /// Insert a cost attribution record
    ///
    /// # Errors
    ///
    /// Returns [`QueryError`] if insertion into snapshot storage fails.
    pub fn insert_attribution(&self, attribution: &CostAttribution) -> Result<(), QueryError> {
        let row = serde_json::json!({
            "repo_id": attribution.repo_id,
            "repo_path": attribution.repo_path,
            "machine_id": attribution.machine_id,
            "agent_type": attribution.agent_type,
            "provider": attribution.provider,
            "estimated_cost_usd": attribution.estimated_cost_usd,
            "tokens_input": attribution.tokens_input,
            "tokens_output": attribution.tokens_output,
            "tokens_total": attribution.tokens_total,
            "sessions_count": attribution.sessions_count,
            "requests_count": attribution.requests_count,
            "confidence": attribution.confidence,
            "confidence_factors_json": serde_json::to_string(&attribution.confidence_factors).unwrap_or_else(|_| "{}".to_string()),
            "raw_json": serde_json::to_string(&attribution).unwrap_or_else(|_| "{}".to_string()),
        });

        self.store.insert_json("cost_attribution_snapshot", &row)?;
        Ok(())
    }
}

/// Estimate cost from token usage using default pricing
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn estimate_cost(provider: &str, model: &str, input_tokens: i64, output_tokens: i64) -> f64 {
    // Default pricing fallback (if not in database)
    let (input_price, output_price) = match (provider, model) {
        ("anthropic", m) if m.contains("opus") => (0.015, 0.075),
        ("anthropic", m) if m.contains("sonnet") => (0.003, 0.015),
        ("anthropic", m) if m.contains("haiku") => (0.001, 0.005),
        ("openai", m) if m.contains("gpt-4o") => (0.0025, 0.01),
        ("openai", m) if m.contains("o1") => (0.015, 0.06),
        ("openai", m) if m.contains("o3") => (0.0011, 0.0044),
        ("google", m) if m.contains("gemini") => (0.00125, 0.005),
        _ => (0.003, 0.015), // Default to sonnet-like pricing
    };

    let input_cost = (input_tokens as f64 / 1000.0) * input_price;
    let output_cost = (output_tokens as f64 / 1000.0) * output_price;
    input_cost + output_cost
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_confidence_factors_overall() {
        let factors = ConfidenceFactors {
            session_mapping: 0.8,
            time_window_match: 0.6,
            data_completeness: 1.0,
            notes: vec![],
        };

        // 0.8 * 0.5 + 0.6 * 0.3 + 1.0 * 0.2 = 0.4 + 0.18 + 0.2 = 0.78
        let overall = factors.overall();
        assert!((overall - 0.78).abs() < 0.001);
    }

    #[test]
    fn test_confidence_factors_clamping() {
        let factors = ConfidenceFactors {
            session_mapping: 2.0, // Invalid but should clamp
            time_window_match: 1.0,
            data_completeness: 1.0,
            notes: vec![],
        };

        let overall = factors.overall();
        assert!(overall <= 1.0);
    }

    #[test]
    fn test_provider_pricing_calculation() {
        let pricing = ProviderPricing {
            provider: "anthropic".to_string(),
            model: "claude-opus-4-5".to_string(),
            price_per_1k_input_tokens: 0.015,
            price_per_1k_output_tokens: 0.075,
        };

        // 1000 input tokens = $0.015, 500 output tokens = $0.0375
        let cost = pricing.calculate_cost(1000, 500);
        assert!((cost - 0.0525).abs() < 0.0001);
    }

    #[test]
    fn test_estimate_cost_anthropic_opus() {
        let cost = estimate_cost("anthropic", "claude-opus-4-5-20251101", 10000, 5000);
        // 10000 * 0.015/1000 + 5000 * 0.075/1000 = 0.15 + 0.375 = 0.525
        assert!((cost - 0.525).abs() < 0.001);
    }

    #[test]
    fn test_estimate_cost_anthropic_sonnet() {
        let cost = estimate_cost("anthropic", "claude-sonnet-4", 10000, 5000);
        // 10000 * 0.003/1000 + 5000 * 0.015/1000 = 0.03 + 0.075 = 0.105
        assert!((cost - 0.105).abs() < 0.001);
    }

    #[test]
    fn test_estimate_cost_openai_gpt4o() {
        let cost = estimate_cost("openai", "gpt-4o", 10000, 5000);
        // 10000 * 0.0025/1000 + 5000 * 0.01/1000 = 0.025 + 0.05 = 0.075
        assert!((cost - 0.075).abs() < 0.001);
    }

    #[test]
    fn test_estimate_cost_unknown_provider() {
        let cost = estimate_cost("unknown", "unknown-model", 1000, 1000);
        // Should use default sonnet-like pricing
        // 1000 * 0.003/1000 + 1000 * 0.015/1000 = 0.003 + 0.015 = 0.018
        assert!((cost - 0.018).abs() < 0.001);
    }

    #[test]
    fn test_cost_attribution_serialization() {
        let attribution = CostAttribution {
            repo_id: Some("my-repo".to_string()),
            repo_path: Some("/path/to/repo".to_string()),
            machine_id: Some("machine-1".to_string()),
            agent_type: Some("claude".to_string()),
            provider: "anthropic".to_string(),
            estimated_cost_usd: 1.50,
            tokens_input: 10000,
            tokens_output: 5000,
            tokens_total: 15000,
            sessions_count: 5,
            requests_count: 20,
            confidence: 0.85,
            confidence_factors: ConfidenceFactors::default(),
        };

        let json = serde_json::to_string(&attribution).unwrap();
        assert!(json.contains("my-repo"));
        assert!(json.contains("anthropic"));
        assert!(json.contains("1.5"));
    }

    #[test]
    fn test_anomaly_type_serialization() {
        let spike = AnomalyType::Spike;
        let json = serde_json::to_string(&spike).unwrap();
        assert_eq!(json, "\"spike\"");

        let drift = AnomalyType::Drift;
        let json = serde_json::to_string(&drift).unwrap();
        assert_eq!(json, "\"drift\"");
    }

    #[test]
    fn test_cost_trend_serialization() {
        let increasing = CostTrend::Increasing;
        let json = serde_json::to_string(&increasing).unwrap();
        assert_eq!(json, "\"increasing\"");
    }

    #[test]
    fn test_cost_summary_with_in_memory_store() {
        let store = VcStore::open_memory().unwrap();
        let builder = CostQueryBuilder::new(&store);

        let summary = builder
            .cost_summary(Utc::now() - chrono::Duration::days(7), None)
            .unwrap();

        // Empty store should return zero values
        assert!((summary.total_cost_usd - 0.0).abs() < f64::EPSILON);
        assert_eq!(summary.total_tokens, 0);
        assert!(summary.by_provider.is_empty());
    }

    #[test]
    fn test_list_pricing_with_in_memory_store() {
        let store = VcStore::open_memory().unwrap();
        let builder = CostQueryBuilder::new(&store);

        let pricing = builder.list_pricing().unwrap();
        // Should have default pricing from migration
        assert!(!pricing.is_empty());
    }
}
