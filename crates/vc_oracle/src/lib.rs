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
    use proptest::prelude::*;

    // =============================================================================
    // OracleError Tests
    // =============================================================================

    #[test]
    fn oracle_error_insufficient_data_display() {
        let err = OracleError::InsufficientData;
        assert_eq!(err.to_string(), "Insufficient data for prediction");
    }

    #[test]
    fn oracle_error_prediction_failed_display() {
        let err = OracleError::PredictionFailed("model diverged".to_string());
        assert_eq!(err.to_string(), "Prediction failed: model diverged");
    }

    #[test]
    fn oracle_error_debug_format() {
        let err = OracleError::InsufficientData;
        let debug = format!("{:?}", err);
        assert!(debug.contains("InsufficientData"));
    }

    // =============================================================================
    // RateLimitAction Tests
    // =============================================================================

    #[test]
    fn rate_limit_action_continue_serialize() {
        let action = RateLimitAction::Continue;
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("\"type\":\"continue\""));

        let parsed: RateLimitAction = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, RateLimitAction::Continue));
    }

    #[test]
    fn rate_limit_action_slow_down_serialize() {
        let action = RateLimitAction::SlowDown {
            target_velocity: 0.5,
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("\"type\":\"slow_down\""));
        assert!(json.contains("\"target_velocity\":0.5"));

        let parsed: RateLimitAction = serde_json::from_str(&json).unwrap();
        match parsed {
            RateLimitAction::SlowDown { target_velocity } => {
                assert!((target_velocity - 0.5).abs() < f64::EPSILON);
            }
            _ => panic!("Expected SlowDown variant"),
        }
    }

    #[test]
    fn rate_limit_action_prepare_swap_serialize() {
        let action = RateLimitAction::PrepareSwap { in_minutes: 15 };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("\"type\":\"prepare_swap\""));
        assert!(json.contains("\"in_minutes\":15"));

        let parsed: RateLimitAction = serde_json::from_str(&json).unwrap();
        match parsed {
            RateLimitAction::PrepareSwap { in_minutes } => {
                assert_eq!(in_minutes, 15);
            }
            _ => panic!("Expected PrepareSwap variant"),
        }
    }

    #[test]
    fn rate_limit_action_swap_now_serialize() {
        let action = RateLimitAction::SwapNow {
            to_account: "backup-1".to_string(),
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("\"type\":\"swap_now\""));
        assert!(json.contains("\"to_account\":\"backup-1\""));

        let parsed: RateLimitAction = serde_json::from_str(&json).unwrap();
        match parsed {
            RateLimitAction::SwapNow { to_account } => {
                assert_eq!(to_account, "backup-1");
            }
            _ => panic!("Expected SwapNow variant"),
        }
    }

    #[test]
    fn rate_limit_action_emergency_pause_serialize() {
        let action = RateLimitAction::EmergencyPause;
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("\"type\":\"emergency_pause\""));

        let parsed: RateLimitAction = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, RateLimitAction::EmergencyPause));
    }

    #[test]
    fn rate_limit_action_clone() {
        let action = RateLimitAction::SwapNow {
            to_account: "acc-2".to_string(),
        };
        let cloned = action.clone();
        match cloned {
            RateLimitAction::SwapNow { to_account } => {
                assert_eq!(to_account, "acc-2");
            }
            _ => panic!("Clone should preserve variant"),
        }
    }

    // =============================================================================
    // RateLimitForecast Tests
    // =============================================================================

    #[test]
    fn rate_limit_forecast_serialize_roundtrip() {
        let forecast = RateLimitForecast {
            provider: "openai".to_string(),
            account: "acc-123".to_string(),
            current_usage_pct: 75.5,
            current_velocity: 2.5,
            time_to_limit: Duration::from_secs(3600),
            confidence: 0.85,
            recommended_action: RateLimitAction::PrepareSwap { in_minutes: 30 },
            optimal_swap_time: Some(Utc::now()),
            alternative_accounts: vec![("backup-1".to_string(), 0.2), ("backup-2".to_string(), 0.1)],
        };

        let json = serde_json::to_string(&forecast).unwrap();
        let parsed: RateLimitForecast = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.provider, "openai");
        assert_eq!(parsed.account, "acc-123");
        assert!((parsed.current_usage_pct - 75.5).abs() < f64::EPSILON);
        assert!((parsed.current_velocity - 2.5).abs() < f64::EPSILON);
        assert_eq!(parsed.time_to_limit.as_secs(), 3600);
        assert!((parsed.confidence - 0.85).abs() < f64::EPSILON);
        assert!(matches!(
            parsed.recommended_action,
            RateLimitAction::PrepareSwap { in_minutes: 30 }
        ));
        assert!(parsed.optimal_swap_time.is_some());
        assert_eq!(parsed.alternative_accounts.len(), 2);
    }

    #[test]
    fn rate_limit_forecast_no_alternatives() {
        let forecast = RateLimitForecast {
            provider: "anthropic".to_string(),
            account: "single-acc".to_string(),
            current_usage_pct: 10.0,
            current_velocity: 0.1,
            time_to_limit: Duration::from_secs(86400),
            confidence: 0.95,
            recommended_action: RateLimitAction::Continue,
            optimal_swap_time: None,
            alternative_accounts: vec![],
        };

        let json = serde_json::to_string(&forecast).unwrap();
        let parsed: RateLimitForecast = serde_json::from_str(&json).unwrap();

        assert!(parsed.optimal_swap_time.is_none());
        assert!(parsed.alternative_accounts.is_empty());
    }

    #[test]
    fn rate_limit_forecast_clone() {
        let forecast = RateLimitForecast {
            provider: "test".to_string(),
            account: "test-acc".to_string(),
            current_usage_pct: 50.0,
            current_velocity: 1.0,
            time_to_limit: Duration::from_secs(1800),
            confidence: 0.7,
            recommended_action: RateLimitAction::Continue,
            optimal_swap_time: None,
            alternative_accounts: vec![],
        };

        let cloned = forecast.clone();
        assert_eq!(cloned.provider, forecast.provider);
        assert_eq!(cloned.account, forecast.account);
    }

    #[test]
    fn rate_limit_forecast_debug() {
        let forecast = RateLimitForecast {
            provider: "debug-test".to_string(),
            account: "acc".to_string(),
            current_usage_pct: 0.0,
            current_velocity: 0.0,
            time_to_limit: Duration::ZERO,
            confidence: 0.5,
            recommended_action: RateLimitAction::Continue,
            optimal_swap_time: None,
            alternative_accounts: vec![],
        };

        let debug = format!("{:?}", forecast);
        assert!(debug.contains("debug-test"));
        assert!(debug.contains("RateLimitForecast"));
    }

    // =============================================================================
    // Oracle Tests
    // =============================================================================

    #[test]
    fn oracle_new() {
        let _oracle = Oracle::new();
    }

    #[test]
    fn oracle_default() {
        let oracle = Oracle::default();
        // Verify default creates same as new()
        let _also_oracle = Oracle::new();
        std::mem::drop(oracle);
    }

    #[tokio::test]
    async fn oracle_forecast_rate_limits_returns_empty() {
        let oracle = Oracle::new();
        let forecasts = oracle.forecast_rate_limits().await.unwrap();
        assert!(forecasts.is_empty());
    }

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
    fn test_calculate_velocity_constant() {
        let now = Utc::now();
        let samples = vec![
            (now, 50.0),
            (now + chrono::Duration::minutes(1), 50.0),
            (now + chrono::Duration::minutes(2), 50.0),
        ];
        let velocity = Oracle::calculate_velocity(&samples);
        assert!(velocity.abs() < f64::EPSILON, "Constant data should have zero velocity");
    }

    #[test]
    fn test_calculate_velocity_increasing() {
        let now = Utc::now();
        let samples = vec![
            (now, 10.0),
            (now + chrono::Duration::minutes(1), 20.0),
            (now + chrono::Duration::minutes(2), 30.0),
            (now + chrono::Duration::minutes(3), 40.0),
        ];
        let velocity = Oracle::calculate_velocity(&samples);
        assert!(velocity > 0.0, "Increasing data should have positive velocity");
        // Linear increase of 10 per step
        assert!((velocity - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_calculate_velocity_decreasing() {
        let now = Utc::now();
        let samples = vec![
            (now, 100.0),
            (now + chrono::Duration::minutes(1), 80.0),
            (now + chrono::Duration::minutes(2), 60.0),
        ];
        let velocity = Oracle::calculate_velocity(&samples);
        assert!(velocity < 0.0, "Decreasing data should have negative velocity");
    }

    #[test]
    fn test_calculate_velocity_two_samples() {
        let now = Utc::now();
        let samples = vec![(now, 0.0), (now + chrono::Duration::minutes(1), 100.0)];
        let velocity = Oracle::calculate_velocity(&samples);
        // With two samples, linear regression gives exact slope
        assert!((velocity - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_calculate_confidence() {
        let conf = Oracle::calculate_confidence(10, 0.0);
        assert!(conf > 0.9);

        let conf_low = Oracle::calculate_confidence(2, 1.0);
        assert!(conf_low < conf);
    }

    #[test]
    fn test_calculate_confidence_zero_samples() {
        let conf = Oracle::calculate_confidence(0, 0.0);
        // sample_factor = 0/10 = 0, but clamped to 0.1 minimum
        assert!((conf - 0.1).abs() < 0.01);
    }

    #[test]
    fn test_calculate_confidence_high_variance() {
        let conf = Oracle::calculate_confidence(100, 1000.0);
        // Even with many samples, high variance reduces confidence
        assert!(conf < 0.5);
    }

    #[test]
    fn test_calculate_confidence_max_samples() {
        let conf = Oracle::calculate_confidence(1000, 0.0);
        // sample_factor capped at 1.0 (10/10), consistency = 1.0, product clamped to 0.99
        assert!((conf - 0.99).abs() < f64::EPSILON);
    }

    // =============================================================================
    // Proptest Property-Based Tests
    // =============================================================================

    proptest! {
        #[test]
        fn confidence_is_clamped(sample_count in 0usize..1000, variance in 0.0f64..1000.0) {
            let conf = Oracle::calculate_confidence(sample_count, variance);
            prop_assert!((0.1..=0.99).contains(&conf));
        }

        #[test]
        fn velocity_zero_for_constant_data(value in -1000.0f64..1000.0) {
            let now = Utc::now();
            let samples: Vec<_> = (0..5)
                .map(|i| (now + chrono::Duration::minutes(i), value))
                .collect();
            let velocity = Oracle::calculate_velocity(&samples);
            prop_assert!(velocity.abs() < 1e-10, "Constant data velocity should be ~0");
        }

        #[test]
        fn rate_limit_action_roundtrip_continue(_dummy in 0..1) {
            let action = RateLimitAction::Continue;
            let json = serde_json::to_string(&action).unwrap();
            let parsed: RateLimitAction = serde_json::from_str(&json).unwrap();
            prop_assert!(matches!(parsed, RateLimitAction::Continue));
        }

        #[test]
        fn rate_limit_action_roundtrip_slow_down(velocity in 0.0f64..100.0) {
            let action = RateLimitAction::SlowDown { target_velocity: velocity };
            let json = serde_json::to_string(&action).unwrap();
            let parsed: RateLimitAction = serde_json::from_str(&json).unwrap();
            match parsed {
                RateLimitAction::SlowDown { target_velocity } => {
                    prop_assert!((target_velocity - velocity).abs() < 1e-10);
                }
                _ => prop_assert!(false, "Expected SlowDown"),
            }
        }

        #[test]
        fn rate_limit_action_roundtrip_prepare_swap(minutes in 1u32..1000) {
            let action = RateLimitAction::PrepareSwap { in_minutes: minutes };
            let json = serde_json::to_string(&action).unwrap();
            let parsed: RateLimitAction = serde_json::from_str(&json).unwrap();
            match parsed {
                RateLimitAction::PrepareSwap { in_minutes } => {
                    prop_assert_eq!(in_minutes, minutes);
                }
                _ => prop_assert!(false, "Expected PrepareSwap"),
            }
        }

        #[test]
        fn forecast_confidence_clamped(usage in 0.0f64..100.0, velocity in -10.0f64..10.0, conf in 0.0f64..1.0) {
            let clamped = conf.clamp(0.1, 0.99);
            let forecast = RateLimitForecast {
                provider: "test".to_string(),
                account: "acc".to_string(),
                current_usage_pct: usage,
                current_velocity: velocity,
                time_to_limit: Duration::from_secs(3600),
                confidence: clamped,
                recommended_action: RateLimitAction::Continue,
                optimal_swap_time: None,
                alternative_accounts: vec![],
            };
            let json = serde_json::to_string(&forecast).unwrap();
            let parsed: RateLimitForecast = serde_json::from_str(&json).unwrap();
            prop_assert!((parsed.confidence - clamped).abs() < 1e-10);
        }

        #[test]
        fn velocity_sign_matches_trend(delta in 1.0f64..100.0) {
            let now = Utc::now();
            // Increasing data
            let increasing: Vec<_> = (0..5)
                .map(|i| (now + chrono::Duration::minutes(i as i64), i as f64 * delta))
                .collect();
            let inc_velocity = Oracle::calculate_velocity(&increasing);
            prop_assert!(inc_velocity > 0.0, "Increasing data should have positive velocity");

            // Decreasing data
            let decreasing: Vec<_> = (0..5)
                .map(|i| (now + chrono::Duration::minutes(i as i64), 1000.0 - i as f64 * delta))
                .collect();
            let dec_velocity = Oracle::calculate_velocity(&decreasing);
            prop_assert!(dec_velocity < 0.0, "Decreasing data should have negative velocity");
        }
    }
}
