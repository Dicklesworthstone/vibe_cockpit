//! Rate limit forecasting module
//!
//! Provides the core forecasting logic for predicting when accounts
//! will hit rate limits and recommending preemptive actions.

use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::time::Duration;

use crate::{OracleError, RateLimitAction, RateLimitForecast};

/// A usage sample for a single account at a point in time
#[derive(Debug, Clone)]
pub struct UsageSample {
    pub provider: String,
    pub account: String,
    pub used_percent: f64,
    pub collected_at: DateTime<Utc>,
    pub resets_at: Option<DateTime<Utc>>,
}

/// Key for grouping samples by account
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct AccountKey {
    pub provider: String,
    pub account: String,
}

/// Configuration for the rate limit forecaster
#[derive(Debug, Clone)]
pub struct ForecastConfig {
    /// Time threshold (seconds) below which to recommend `SwapNow`
    pub swap_now_threshold_secs: u64,
    /// Time threshold (seconds) below which to recommend `PrepareSwap`
    pub prepare_swap_threshold_secs: u64,
    /// Time threshold (seconds) below which to recommend `SlowDown` (if velocity high)
    pub slow_down_threshold_secs: u64,
    /// Velocity threshold above which to recommend `SlowDown`
    pub high_velocity_threshold: f64,
    /// `SlowDown` target as fraction of current velocity
    pub slow_down_factor: f64,
}

impl Default for ForecastConfig {
    fn default() -> Self {
        Self {
            swap_now_threshold_secs: 300,     // 5 minutes
            prepare_swap_threshold_secs: 600, // 10 minutes
            slow_down_threshold_secs: 1800,   // 30 minutes
            high_velocity_threshold: 1.0,     // 1% per minute
            slow_down_factor: 0.7,            // Target 70% of current velocity
        }
    }
}

/// Rate limit forecaster
pub struct RateLimitForecaster {
    config: ForecastConfig,
}

impl RateLimitForecaster {
    /// Create a new forecaster with default config
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: ForecastConfig::default(),
        }
    }

    /// Create a forecaster with custom config
    #[must_use]
    pub fn with_config(config: ForecastConfig) -> Self {
        Self { config }
    }

    /// Generate forecasts from usage samples
    ///
    /// Takes a list of usage samples and returns forecasts for each account.
    #[must_use]
    pub fn forecast(&self, samples: Vec<UsageSample>) -> Vec<RateLimitForecast> {
        // Group samples by account
        let grouped = Self::group_by_account(samples);

        // Generate forecasts for each account
        let mut forecasts: Vec<_> = grouped
            .into_iter()
            .filter_map(|(key, account_samples)| self.forecast_single(&key, &account_samples).ok())
            .collect();

        // Sort by urgency (smallest time_to_limit first)
        forecasts.sort_by_key(|f| f.time_to_limit);

        forecasts
    }

    /// Group samples by provider+account
    fn group_by_account(samples: Vec<UsageSample>) -> HashMap<AccountKey, Vec<UsageSample>> {
        let mut grouped: HashMap<AccountKey, Vec<UsageSample>> = HashMap::new();

        for sample in samples {
            let key = AccountKey {
                provider: sample.provider.clone(),
                account: sample.account.clone(),
            };
            grouped.entry(key).or_default().push(sample);
        }

        // Sort each account's samples by time
        for samples in grouped.values_mut() {
            samples.sort_by_key(|s| s.collected_at);
        }

        grouped
    }

    /// Generate a forecast for a single account
    fn forecast_single(
        &self,
        key: &AccountKey,
        samples: &[UsageSample],
    ) -> Result<RateLimitForecast, OracleError> {
        if samples.is_empty() {
            return Err(OracleError::InsufficientData);
        }

        // Get current state from most recent sample
        let current = samples.last().unwrap();
        let current_usage = current.used_percent;

        // Calculate velocity (% per minute)
        let velocity = Self::calculate_velocity(samples);

        // Calculate time to limit
        let time_to_limit = Self::calculate_time_to_limit(current_usage, velocity);

        // Calculate confidence
        let confidence = Self::calculate_confidence(samples, velocity);

        // Determine recommended action
        let action = self.determine_action(time_to_limit, velocity);

        // Calculate optimal swap time
        let optimal_swap_time = Self::calculate_optimal_swap_time(time_to_limit, current.resets_at);

        Ok(RateLimitForecast {
            provider: key.provider.clone(),
            account: key.account.clone(),
            current_usage_pct: current_usage,
            current_velocity: velocity,
            time_to_limit,
            confidence,
            recommended_action: action,
            optimal_swap_time,
            alternative_accounts: vec![], // Filled by higher-level code with store access
        })
    }

    /// Calculate velocity (rate of usage increase) from samples
    ///
    /// Returns velocity in percent per minute.
    fn calculate_velocity(samples: &[UsageSample]) -> f64 {
        if samples.len() < 2 {
            return 0.0;
        }

        // Calculate velocity using time-weighted linear regression
        let first = samples.first().unwrap();
        let last = samples.last().unwrap();

        let time_diff = i64_to_f64((last.collected_at - first.collected_at).num_seconds());
        if time_diff < 60.0 {
            // Less than a minute of data - use simple difference
            return last.used_percent - first.used_percent;
        }

        let usage_diff = last.used_percent - first.used_percent;
        let minutes = time_diff / 60.0;

        usage_diff / minutes
    }

    /// Calculate time until 100% usage at current velocity
    fn calculate_time_to_limit(current_usage: f64, velocity: f64) -> Duration {
        if velocity <= 0.0 {
            // Not increasing, effectively infinite time
            return Duration::from_secs(u64::MAX / 2);
        }

        let remaining = 100.0 - current_usage;
        if remaining <= 0.0 {
            // Already at or over limit
            return Duration::ZERO;
        }

        let minutes_to_limit = remaining / velocity;
        Duration::from_secs_f64(minutes_to_limit * 60.0)
    }

    /// Calculate prediction confidence based on data quality
    fn calculate_confidence(samples: &[UsageSample], _velocity: f64) -> f64 {
        // Sample count factor
        let sample_factor = (usize_to_f64(samples.len()) / 10.0).min(1.0);

        // Calculate velocity variance (how stable is the velocity?)
        let velocities: Vec<f64> = samples
            .windows(2)
            .filter_map(|w| {
                let minutes = i64_to_f64((w[1].collected_at - w[0].collected_at).num_seconds()) / 60.0;
                if minutes > 0.0 {
                    Some((w[1].used_percent - w[0].used_percent) / minutes)
                } else {
                    None
                }
            })
            .collect();

        let variance = if velocities.is_empty() {
            1.0
        } else {
            calculate_variance(&velocities)
        };

        let consistency_factor = 1.0 / (1.0 + variance);

        // Recency factor - more recent data is more reliable
        if let Some(last) = samples.last() {
            let age_minutes = i64_to_f64((Utc::now() - last.collected_at).num_minutes());
            let recency_factor = 1.0 / (1.0 + age_minutes / 10.0);

            (sample_factor * consistency_factor * recency_factor).clamp(0.1, 0.99)
        } else {
            0.1
        }
    }

    /// Determine the recommended action based on time to limit and velocity
    fn determine_action(&self, time_to_limit: Duration, velocity: f64) -> RateLimitAction {
        let secs = time_to_limit.as_secs();

        if secs == 0 {
            RateLimitAction::EmergencyPause
        } else if secs <= self.config.swap_now_threshold_secs {
            RateLimitAction::SwapNow {
                to_account: String::new(), // Filled by higher-level code
            }
        } else if secs <= self.config.prepare_swap_threshold_secs {
            RateLimitAction::PrepareSwap {
                in_minutes: u32::try_from(secs / 60).unwrap_or(u32::MAX),
            }
        } else if secs <= self.config.slow_down_threshold_secs
            && velocity > self.config.high_velocity_threshold
        {
            RateLimitAction::SlowDown {
                target_velocity: velocity * self.config.slow_down_factor,
            }
        } else {
            RateLimitAction::Continue
        }
    }

    /// Calculate optimal time to swap accounts
    fn calculate_optimal_swap_time(
        time_to_limit: Duration,
        resets_at: Option<DateTime<Utc>>,
    ) -> Option<DateTime<Utc>> {
        // If we have a reset time coming up before we hit the limit, no need to swap
        if let Some(reset_time) = resets_at {
            let now = Utc::now();
            if reset_time > now {
                let time_to_reset = u64::try_from((reset_time - now).num_seconds()).unwrap_or(0);
                if time_to_reset < time_to_limit.as_secs() {
                    // Reset happens before limit - no swap needed
                    return None;
                }
            }
        }

        // Optimal swap time is when we're at ~80% to give some buffer
        let secs_to_limit = time_to_limit.as_secs();
        if secs_to_limit == 0 || secs_to_limit > u64::MAX / 2 - 1000 {
            return None;
        }

        // Swap at 80% of the way to limit
        let swap_buffer_secs = secs_to_limit / 5; // 20% buffer
        let optimal_secs = secs_to_limit - swap_buffer_secs;
        let optimal_secs = i64::try_from(optimal_secs).ok()?;
        Some(Utc::now() + chrono::Duration::seconds(optimal_secs))
    }
}

impl Default for RateLimitForecaster {
    fn default() -> Self {
        Self::new()
    }
}

/// Calculate variance of a set of values
#[must_use]
fn calculate_variance(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }

    let mean = values.iter().sum::<f64>() / usize_to_f64(values.len());
    let sq_diff_sum: f64 = values.iter().map(|v| (v - mean).powi(2)).sum();
    sq_diff_sum / usize_to_f64(values.len())
}

/// Find alternative accounts with available headroom
///
/// Returns list of (`account_id`, `headroom_percent`) sorted by headroom descending.
#[must_use]
pub fn rank_alternative_accounts(
    samples: &[UsageSample],
    current_provider: &str,
    current_account: &str,
) -> Vec<(String, f64)> {
    let mut accounts: HashMap<String, f64> = HashMap::new();

    for sample in samples {
        // Only consider same provider, different account
        if sample.provider == current_provider && sample.account != current_account {
            // Use most recent sample for each account
            accounts
                .entry(sample.account.clone())
                .and_modify(|v| {
                    // Update if this sample is more recent (higher usage = more recent)
                    if sample.used_percent > *v {
                        *v = sample.used_percent;
                    }
                })
                .or_insert(sample.used_percent);
        }
    }

    // Convert to headroom and sort
    let mut alternatives: Vec<_> = accounts
        .into_iter()
        .map(|(account, usage)| (account, 100.0 - usage))
        .filter(|(_, headroom)| *headroom > 10.0) // Only accounts with >10% headroom
        .collect();

    alternatives.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    alternatives
}

fn usize_to_f64(value: usize) -> f64 {
    f64::from(u32::try_from(value).unwrap_or(u32::MAX))
}

fn i64_to_f64(value: i64) -> f64 {
    f64::from(i32::try_from(value).unwrap_or(if value.is_negative() {
        i32::MIN
    } else {
        i32::MAX
    }))
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    fn make_samples(usages: &[(i64, f64)]) -> Vec<UsageSample> {
        let base = Utc::now() - chrono::Duration::hours(1);
        usages
            .iter()
            .map(|(minutes_offset, usage)| UsageSample {
                provider: "claude".to_string(),
                account: "test@example.com".to_string(),
                used_percent: *usage,
                collected_at: base + chrono::Duration::minutes(*minutes_offset),
                resets_at: None,
            })
            .collect()
    }

    #[test]
    fn test_forecaster_new() {
        let forecaster = RateLimitForecaster::new();
        assert_eq!(forecaster.config.swap_now_threshold_secs, 300);
    }

    #[test]
    fn test_forecaster_default() {
        let forecaster = RateLimitForecaster::default();
        assert_eq!(forecaster.config.prepare_swap_threshold_secs, 600);
    }

    #[test]
    fn test_calculate_velocity_increasing() {
        // 10% increase over 10 minutes = 1% per minute
        let samples = make_samples(&[(0, 50.0), (5, 55.0), (10, 60.0)]);
        let velocity = RateLimitForecaster::calculate_velocity(&samples);
        assert!((velocity - 1.0).abs() < 0.1);
    }

    #[test]
    fn test_calculate_velocity_decreasing() {
        // 20% decrease over 10 minutes = -2% per minute
        let samples = make_samples(&[(0, 80.0), (5, 70.0), (10, 60.0)]);
        let velocity = RateLimitForecaster::calculate_velocity(&samples);
        assert!(velocity < 0.0);
    }

    #[test]
    fn test_calculate_velocity_constant() {
        let samples = make_samples(&[(0, 50.0), (5, 50.0), (10, 50.0)]);
        let velocity = RateLimitForecaster::calculate_velocity(&samples);
        assert!(velocity.abs() < 0.01);
    }

    #[test]
    fn test_calculate_velocity_empty() {
        let velocity = RateLimitForecaster::calculate_velocity(&[]);
        assert_eq!(velocity, 0.0);
    }

    #[test]
    fn test_calculate_velocity_single_sample() {
        let samples = make_samples(&[(0, 50.0)]);
        let velocity = RateLimitForecaster::calculate_velocity(&samples);
        assert_eq!(velocity, 0.0);
    }

    #[test]
    fn test_calculate_time_to_limit_positive_velocity() {
        // At 80%, velocity 2% per minute -> 10 minutes to 100%
        let time = RateLimitForecaster::calculate_time_to_limit(80.0, 2.0);
        assert!((time.as_secs_f64() - 600.0).abs() < 10.0);
    }

    #[test]
    fn test_calculate_time_to_limit_zero_velocity() {
        let time = RateLimitForecaster::calculate_time_to_limit(50.0, 0.0);
        assert!(time.as_secs() > 1_000_000_000);
    }

    #[test]
    fn test_calculate_time_to_limit_negative_velocity() {
        let time = RateLimitForecaster::calculate_time_to_limit(50.0, -1.0);
        assert!(time.as_secs() > 1_000_000_000);
    }

    #[test]
    fn test_calculate_time_to_limit_already_at_limit() {
        let time = RateLimitForecaster::calculate_time_to_limit(100.0, 1.0);
        assert_eq!(time.as_secs(), 0);
    }

    #[test]
    fn test_determine_action_continue() {
        let forecaster = RateLimitForecaster::new();
        let action = forecaster.determine_action(Duration::from_secs(2 * 3600), 0.5);
        assert!(matches!(action, RateLimitAction::Continue));
    }

    #[test]
    fn test_determine_action_slow_down() {
        let forecaster = RateLimitForecaster::new();
        // 20 minutes to limit, high velocity
        let action = forecaster.determine_action(Duration::from_secs(20 * 60), 2.0);
        match action {
            RateLimitAction::SlowDown { target_velocity } => {
                assert!((target_velocity - 1.4).abs() < 0.01); // 2.0 * 0.7
            }
            _ => panic!("Expected SlowDown"),
        }
    }

    #[test]
    fn test_determine_action_prepare_swap() {
        let forecaster = RateLimitForecaster::new();
        // 8 minutes to limit
        let action = forecaster.determine_action(Duration::from_secs(8 * 60), 0.5);
        match action {
            RateLimitAction::PrepareSwap { in_minutes } => {
                assert_eq!(in_minutes, 8);
            }
            _ => panic!("Expected PrepareSwap"),
        }
    }

    #[test]
    fn test_determine_action_swap_now() {
        let forecaster = RateLimitForecaster::new();
        // 3 minutes to limit
        let action = forecaster.determine_action(Duration::from_secs(3 * 60), 0.5);
        match action {
            RateLimitAction::SwapNow { .. } => {}
            _ => panic!("Expected SwapNow"),
        }
    }

    #[test]
    fn test_determine_action_emergency() {
        let forecaster = RateLimitForecaster::new();
        let action = forecaster.determine_action(Duration::ZERO, 5.0);
        assert!(matches!(action, RateLimitAction::EmergencyPause));
    }

    #[test]
    fn test_forecast_single_account() {
        let forecaster = RateLimitForecaster::new();
        let samples = make_samples(&[(0, 50.0), (5, 55.0), (10, 60.0)]);
        let forecasts = forecaster.forecast(samples);

        assert_eq!(forecasts.len(), 1);
        let f = &forecasts[0];
        assert_eq!(f.provider, "claude");
        assert_eq!(f.account, "test@example.com");
        assert!((f.current_usage_pct - 60.0).abs() < 0.1);
        assert!(f.current_velocity > 0.0);
    }

    #[test]
    fn test_forecast_multiple_accounts() {
        let base = Utc::now() - chrono::Duration::hours(1);
        let samples = vec![
            UsageSample {
                provider: "claude".to_string(),
                account: "acc1@example.com".to_string(),
                used_percent: 90.0,
                collected_at: base,
                resets_at: None,
            },
            UsageSample {
                provider: "claude".to_string(),
                account: "acc2@example.com".to_string(),
                used_percent: 20.0,
                collected_at: base,
                resets_at: None,
            },
        ];

        let forecaster = RateLimitForecaster::new();
        let forecasts = forecaster.forecast(samples);

        assert_eq!(forecasts.len(), 2);
    }

    #[test]
    fn test_forecast_empty() {
        let forecaster = RateLimitForecaster::new();
        let forecasts = forecaster.forecast(vec![]);
        assert!(forecasts.is_empty());
    }

    #[test]
    fn test_rank_alternative_accounts() {
        let base = Utc::now();
        let samples = vec![
            UsageSample {
                provider: "claude".to_string(),
                account: "current@example.com".to_string(),
                used_percent: 90.0,
                collected_at: base,
                resets_at: None,
            },
            UsageSample {
                provider: "claude".to_string(),
                account: "alt1@example.com".to_string(),
                used_percent: 20.0, // 80% headroom
                collected_at: base,
                resets_at: None,
            },
            UsageSample {
                provider: "claude".to_string(),
                account: "alt2@example.com".to_string(),
                used_percent: 50.0, // 50% headroom
                collected_at: base,
                resets_at: None,
            },
            UsageSample {
                provider: "openai".to_string(), // Different provider - excluded
                account: "other@example.com".to_string(),
                used_percent: 10.0,
                collected_at: base,
                resets_at: None,
            },
        ];

        let alternatives = rank_alternative_accounts(&samples, "claude", "current@example.com");

        assert_eq!(alternatives.len(), 2);
        assert_eq!(alternatives[0].0, "alt1@example.com");
        assert!((alternatives[0].1 - 80.0).abs() < 0.1);
        assert_eq!(alternatives[1].0, "alt2@example.com");
        assert!((alternatives[1].1 - 50.0).abs() < 0.1);
    }

    #[test]
    fn test_rank_alternative_accounts_excludes_low_headroom() {
        let base = Utc::now();
        let samples = vec![UsageSample {
            provider: "claude".to_string(),
            account: "alt@example.com".to_string(),
            used_percent: 95.0, // Only 5% headroom - excluded
            collected_at: base,
            resets_at: None,
        }];

        let alternatives = rank_alternative_accounts(&samples, "claude", "current@example.com");
        assert!(alternatives.is_empty());
    }

    #[test]
    fn test_calculate_variance() {
        assert_eq!(calculate_variance(&[]), 0.0);
        assert_eq!(calculate_variance(&[5.0]), 0.0);
        assert_eq!(calculate_variance(&[5.0, 5.0, 5.0]), 0.0);

        // [1, 2, 3] -> mean=2, variance = ((1-2)^2 + (2-2)^2 + (3-2)^2) / 3 = 2/3
        let var = calculate_variance(&[1.0, 2.0, 3.0]);
        assert!((var - 2.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn test_calculate_confidence_increases_with_samples() {
        // Use recent samples (relative to now) to avoid recency factor issues
        let now = Utc::now();
        let samples_few = vec![
            UsageSample {
                provider: "claude".to_string(),
                account: "test@example.com".to_string(),
                used_percent: 50.0,
                collected_at: now - chrono::Duration::minutes(5),
                resets_at: None,
            },
            UsageSample {
                provider: "claude".to_string(),
                account: "test@example.com".to_string(),
                used_percent: 55.0,
                collected_at: now,
                resets_at: None,
            },
        ];
        let samples_many = vec![
            UsageSample {
                provider: "claude".to_string(),
                account: "test@example.com".to_string(),
                used_percent: 50.0,
                collected_at: now - chrono::Duration::minutes(10),
                resets_at: None,
            },
            UsageSample {
                provider: "claude".to_string(),
                account: "test@example.com".to_string(),
                used_percent: 52.0,
                collected_at: now - chrono::Duration::minutes(8),
                resets_at: None,
            },
            UsageSample {
                provider: "claude".to_string(),
                account: "test@example.com".to_string(),
                used_percent: 54.0,
                collected_at: now - chrono::Duration::minutes(6),
                resets_at: None,
            },
            UsageSample {
                provider: "claude".to_string(),
                account: "test@example.com".to_string(),
                used_percent: 56.0,
                collected_at: now - chrono::Duration::minutes(4),
                resets_at: None,
            },
            UsageSample {
                provider: "claude".to_string(),
                account: "test@example.com".to_string(),
                used_percent: 58.0,
                collected_at: now - chrono::Duration::minutes(2),
                resets_at: None,
            },
            UsageSample {
                provider: "claude".to_string(),
                account: "test@example.com".to_string(),
                used_percent: 60.0,
                collected_at: now,
                resets_at: None,
            },
        ];

        let conf_few = RateLimitForecaster::calculate_confidence(&samples_few, 1.0);
        let conf_many = RateLimitForecaster::calculate_confidence(&samples_many, 1.0);

        // More samples should give higher confidence (same recency for both)
        assert!(
            conf_many > conf_few,
            "conf_many={conf_many} should be > conf_few={conf_few}"
        );
    }

    #[test]
    fn test_optimal_swap_time_with_upcoming_reset() {
        // Reset happens in 30 minutes, but we'd hit limit in 60 minutes
        // -> No swap needed since reset happens first
        let reset_time = Utc::now() + chrono::Duration::minutes(30);
        let time_to_limit = Duration::from_secs(3600); // 60 minutes

        let optimal =
            RateLimitForecaster::calculate_optimal_swap_time(time_to_limit, Some(reset_time));
        assert!(optimal.is_none());
    }

    #[test]
    fn test_optimal_swap_time_without_reset() {
        let time_to_limit = Duration::from_secs(10 * 60); // 10 minutes

        let optimal = RateLimitForecaster::calculate_optimal_swap_time(time_to_limit, None);
        assert!(optimal.is_some());

        // Should be around 8 minutes from now (80% of 10 minutes)
        let swap_time = optimal.unwrap();
        let diff = (swap_time - Utc::now()).num_seconds();
        assert!(diff > 400 && diff < 600);
    }
}
