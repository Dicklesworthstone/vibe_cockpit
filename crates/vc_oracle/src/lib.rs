//! vc_oracle - Prediction engine for Vibe Cockpit
//!
//! This crate provides:
//! - Rate limit forecasting
//! - Pattern recognition
//! - Anomaly detection
//! - Predictive recommendations

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

/// Oracle errors
#[derive(Error, Debug)]
pub enum OracleError {
    #[error("Insufficient data for prediction")]
    InsufficientData,

    #[error("Query error: {0}")]
    QueryError(#[from] vc_query::QueryError),

    #[error("Prediction failed: {0}")]
    PredictionFailed(String),
}

/// Rate limit forecast
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitForecast {
    pub provider: String,
    pub account: String,
    pub current_usage_pct: f64,
    pub current_velocity: f64,
    pub time_to_limit: Duration,
    pub confidence: f64,
    pub recommended_action: RateLimitAction,
    pub optimal_swap_time: Option<DateTime<Utc>>,
    pub alternative_accounts: Vec<(String, f64)>,
}

/// Recommended action for rate limit
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RateLimitAction {
    Continue,
    SlowDown { target_velocity: f64 },
    PrepareSwap { in_minutes: u32 },
    SwapNow { to_account: String },
    EmergencyPause,
}

/// The Oracle prediction engine
pub struct Oracle {
    // Will hold store reference and configuration
}

impl Oracle {
    /// Create a new Oracle instance
    pub fn new() -> Self {
        Self {}
    }

    /// Forecast rate limits for all accounts
    pub async fn forecast_rate_limits(&self) -> Result<Vec<RateLimitForecast>, OracleError> {
        // Placeholder implementation
        Ok(vec![])
    }

    /// Calculate velocity (rate of usage increase) from samples
    pub fn calculate_velocity(samples: &[(DateTime<Utc>, f64)]) -> f64 {
        if samples.len() < 2 {
            return 0.0;
        }

        // Linear regression
        let n = samples.len() as f64;
        let sum_x: f64 = samples.iter().enumerate().map(|(i, _)| i as f64).sum();
        let sum_y: f64 = samples.iter().map(|(_, y)| y).sum();
        let sum_xy: f64 = samples
            .iter()
            .enumerate()
            .map(|(i, (_, y))| i as f64 * y)
            .sum();
        let sum_xx: f64 = samples
            .iter()
            .enumerate()
            .map(|(i, _)| (i * i) as f64)
            .sum();

        let denominator = n * sum_xx - sum_x * sum_x;
        if denominator.abs() < f64::EPSILON {
            return 0.0;
        }

        (n * sum_xy - sum_x * sum_y) / denominator
    }

    /// Calculate prediction confidence based on data quality
    pub fn calculate_confidence(sample_count: usize, velocity_variance: f64) -> f64 {
        let sample_factor = (sample_count as f64 / 10.0).min(1.0);
        let consistency_factor = 1.0 / (1.0 + velocity_variance);
        (sample_factor * consistency_factor).clamp(0.1, 0.99)
    }
}

impl Default for Oracle {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_velocity_empty() {
        assert_eq!(Oracle::calculate_velocity(&[]), 0.0);
    }

    #[test]
    fn test_calculate_velocity_single() {
        let samples = vec![(Utc::now(), 50.0)];
        assert_eq!(Oracle::calculate_velocity(&samples), 0.0);
    }

    #[test]
    fn test_calculate_confidence() {
        let conf = Oracle::calculate_confidence(10, 0.0);
        assert!(conf > 0.9);

        let conf_low = Oracle::calculate_confidence(2, 1.0);
        assert!(conf_low < conf);
    }
}
