//! `Agent DNA` - Behavioral fingerprints for AI coding agents
//!
//! Agent DNA captures performance patterns and preferences to enable:
//! - Comparison between different agents and configurations
//! - Anomaly detection for unusual behavior
//! - Optimization recommendations based on historical data
//! - A/B testing support for agent configurations
//!
//! # Architecture
//!
//! ```text
//! ┌───────────────────────────────────────────────────────┐
//! │                    DnaComputer                        │
//! ├───────────────────────────────────────────────────────┤
//! │  1. Query usage/session data from store               │
//! │  2. Compute metrics (tokens, errors, tools, timing)   │
//! │  3. Generate embedding for similarity search          │
//! │  4. Detect anomalies against historical baseline      │
//! └───────────────────────────────────────────────────────┘
//!           │
//!           ▼
//! ┌───────────────────┐     ┌───────────────────┐
//! │     AgentDna      │────▶│    DnaHistory     │
//! └───────────────────┘     └───────────────────┘
//! ```

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use tracing::{info, instrument, warn};

use crate::OracleError;

/// Errors specific to DNA computation
#[derive(Error, Debug)]
pub enum DnaError {
    #[error("Insufficient data for DNA computation: need at least {needed} samples, have {have}")]
    InsufficientData { needed: usize, have: usize },

    #[error("Agent not found: {0}")]
    AgentNotFound(String),

    #[error("Query error: {0}")]
    QueryError(String),

    #[error("Embedding computation failed: {0}")]
    EmbeddingError(String),
}

impl From<DnaError> for OracleError {
    fn from(err: DnaError) -> Self {
        match err {
            DnaError::InsufficientData { .. } => OracleError::InsufficientData,
            _ => OracleError::PredictionFailed(err.to_string()),
        }
    }
}

/// Time range for DNA computation
#[derive(Debug, Clone, Copy)]
pub struct TimeRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl TimeRange {
    /// Create a range for the last N days
    #[must_use]
    pub fn last_days(days: i64) -> Self {
        let end = Utc::now();
        let start = end - Duration::days(days);
        Self { start, end }
    }

    /// Create a range for the last N hours
    #[must_use]
    pub fn last_hours(hours: i64) -> Self {
        let end = Utc::now();
        let start = end - Duration::hours(hours);
        Self { start, end }
    }

    /// Duration of the time range
    #[must_use]
    pub fn duration(&self) -> Duration {
        self.end - self.start
    }
}

impl Default for TimeRange {
    fn default() -> Self {
        Self::last_days(30)
    }
}

/// Agent DNA - behavioral fingerprint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDna {
    /// Unique identifier: `"{program}:{model}:{config_hash}"`
    pub dna_id: String,

    /// Agent program (claude-code, codex, gemini)
    pub agent_program: String,

    /// Agent model (opus-4.5, gpt4, etc.)
    pub agent_model: String,

    /// Hash of relevant configuration (None = default config)
    pub configuration_hash: Option<String>,

    /// When this DNA was computed
    pub computed_at: DateTime<Utc>,

    // Token patterns
    /// Average tokens per turn
    pub avg_tokens_per_turn: f64,
    /// Average input/output token ratio
    pub avg_input_output_ratio: f64,
    /// Token usage variance (standard deviation)
    pub token_variance: f64,

    // Error patterns
    /// Overall error rate (0.0 - 1.0)
    pub error_rate: f64,
    /// Common error types and their frequencies
    pub common_error_types: HashMap<String, f64>,
    /// Recovery rate after errors (0.0 - 1.0)
    pub recovery_rate: f64,

    // Tool usage patterns
    /// Tool preferences (tool -> usage frequency)
    pub tool_preferences: HashMap<String, f64>,
    /// Per-tool success rates
    pub tool_success_rates: HashMap<String, f64>,
    /// Average tools used per task
    pub avg_tools_per_task: f64,

    // Timing patterns
    /// Average response time in milliseconds
    pub avg_response_time_ms: f64,
    /// 95th percentile response time
    pub p95_response_time_ms: f64,
    /// Hourly activity distribution (0-23 -> activity level)
    pub time_of_day_distribution: HashMap<u8, f64>,

    // Task patterns
    /// Average task completion time in minutes
    pub avg_task_completion_time_mins: f64,
    /// Task success rate (0.0 - 1.0)
    pub task_success_rate: f64,
    /// Success rate by complexity level (low/medium/high)
    pub complexity_handling: HashMap<String, f64>,

    // Session patterns
    /// Average session duration in minutes
    pub avg_session_duration_mins: f64,
    /// Average turns per session
    pub avg_turns_per_session: f64,
    /// Session abandonment rate (0.0 - 1.0)
    pub session_abandonment_rate: f64,

    /// 128-dimensional embedding for similarity search
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub dna_embedding: Vec<f64>,
}

impl AgentDna {
    /// Create a new DNA with minimal data
    #[must_use]
    pub fn new(program: impl Into<String>, model: impl Into<String>) -> Self {
        let program = program.into();
        let model = model.into();
        Self {
            dna_id: format!("{program}:{model}:default"),
            agent_program: program,
            agent_model: model,
            configuration_hash: None,
            computed_at: Utc::now(),
            avg_tokens_per_turn: 0.0,
            avg_input_output_ratio: 0.0,
            token_variance: 0.0,
            error_rate: 0.0,
            common_error_types: HashMap::new(),
            recovery_rate: 0.0,
            tool_preferences: HashMap::new(),
            tool_success_rates: HashMap::new(),
            avg_tools_per_task: 0.0,
            avg_response_time_ms: 0.0,
            p95_response_time_ms: 0.0,
            time_of_day_distribution: HashMap::new(),
            avg_task_completion_time_mins: 0.0,
            task_success_rate: 0.0,
            complexity_handling: HashMap::new(),
            avg_session_duration_mins: 0.0,
            avg_turns_per_session: 0.0,
            session_abandonment_rate: 0.0,
            dna_embedding: Vec::new(),
        }
    }

    /// Set configuration hash and update DNA ID
    #[must_use]
    pub fn with_config_hash(mut self, hash: impl Into<String>) -> Self {
        let hash = hash.into();
        self.dna_id = format!("{}:{}:{}", self.agent_program, self.agent_model, hash);
        self.configuration_hash = Some(hash);
        self
    }

    /// Convert DNA metrics to a normalized vector for embedding
    #[must_use]
    pub fn to_feature_vector(&self) -> Vec<f64> {
        vec![
            self.avg_tokens_per_turn / 10000.0,           // Normalize to ~0-1
            self.avg_input_output_ratio.min(10.0) / 10.0, // Cap and normalize
            self.token_variance.min(10000.0) / 10000.0,
            self.error_rate,
            self.recovery_rate,
            self.avg_tools_per_task / 20.0,
            self.avg_response_time_ms.min(60000.0) / 60000.0,
            self.p95_response_time_ms.min(120_000.0) / 120_000.0,
            self.avg_task_completion_time_mins.min(120.0) / 120.0,
            self.task_success_rate,
            self.avg_session_duration_mins.min(480.0) / 480.0,
            self.avg_turns_per_session.min(100.0) / 100.0,
            self.session_abandonment_rate,
            // Tool preferences (top 5)
            *self.tool_preferences.get("Read").unwrap_or(&0.0),
            *self.tool_preferences.get("Edit").unwrap_or(&0.0),
            *self.tool_preferences.get("Write").unwrap_or(&0.0),
            *self.tool_preferences.get("Bash").unwrap_or(&0.0),
            *self.tool_preferences.get("Grep").unwrap_or(&0.0),
        ]
    }

    /// Generate an embedding from the DNA metrics
    pub fn compute_embedding(&mut self) {
        let features = self.to_feature_vector();

        // Simple embedding: pad features to 128 dimensions with hash-based mixing
        let mut embedding = vec![0.0; 128];

        // Copy raw features
        for (i, &f) in features.iter().enumerate() {
            if i < 128 {
                embedding[i] = f;
            }
        }

        // Add derived features (interactions)
        for i in 0..features.len().min(32) {
            for j in (i + 1)..features.len().min(32) {
                let idx = 32 + (i * 10 + j) % 96;
                embedding[idx] += features[i] * features[j] * 0.1;
            }
        }

        // Normalize to unit length
        let magnitude: f64 = embedding.iter().map(|x| x * x).sum::<f64>().sqrt();
        if magnitude > f64::EPSILON {
            for v in &mut embedding {
                *v /= magnitude;
            }
        }

        self.dna_embedding = embedding;
    }
}

/// Historical DNA snapshot for drift detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnaHistory {
    /// Auto-incrementing ID
    pub id: Option<i64>,

    /// Reference to the DNA
    pub dna_id: String,

    /// When this snapshot was taken
    pub computed_at: DateTime<Utc>,

    /// Full metrics snapshot (JSON)
    pub metrics: AgentDna,

    /// Summary of what changed from previous snapshot
    pub change_summary: Option<String>,
}

impl DnaHistory {
    /// Create a new history entry from DNA
    #[must_use]
    pub fn from_dna(dna: &AgentDna, change_summary: Option<String>) -> Self {
        Self {
            id: None,
            dna_id: dna.dna_id.clone(),
            computed_at: dna.computed_at,
            metrics: dna.clone(),
            change_summary,
        }
    }
}

/// Difference between two DNA metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Difference {
    pub metric: String,
    pub value_a: f64,
    pub value_b: f64,
    pub delta: f64,
    pub delta_pct: f64,
}

impl Difference {
    /// Create a new difference
    #[must_use]
    pub fn new(metric: impl Into<String>, a: f64, b: f64) -> Self {
        let delta = b - a;
        let delta_pct = if a.abs() > f64::EPSILON {
            (delta / a) * 100.0
        } else {
            0.0
        };
        Self {
            metric: metric.into(),
            value_a: a,
            value_b: b,
            delta,
            delta_pct,
        }
    }

    /// Check if the difference is significant (>10% change or >0.05 absolute)
    #[must_use]
    pub fn is_significant(&self) -> bool {
        self.delta.abs() > 0.05 || self.delta_pct.abs() > 10.0
    }
}

/// Comparison between two Agent DNAs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnaComparison {
    /// DNA A identifier
    pub dna_a_id: String,
    /// DNA B identifier
    pub dna_b_id: String,
    /// Cosine similarity (0.0 - 1.0)
    pub similarity: f64,
    /// Significant differences
    pub differences: Vec<Difference>,
    /// Overall assessment
    pub assessment: String,
}

impl DnaComparison {
    /// Create a comparison between two DNAs
    #[must_use]
    pub fn compare(a: &AgentDna, b: &AgentDna) -> Self {
        let similarity = cosine_similarity(&a.dna_embedding, &b.dna_embedding);

        let differences: Vec<_> = vec![
            Difference::new("error_rate", a.error_rate, b.error_rate),
            Difference::new("recovery_rate", a.recovery_rate, b.recovery_rate),
            Difference::new(
                "avg_tokens_per_turn",
                a.avg_tokens_per_turn,
                b.avg_tokens_per_turn,
            ),
            Difference::new(
                "task_success_rate",
                a.task_success_rate,
                b.task_success_rate,
            ),
            Difference::new(
                "avg_response_time_ms",
                a.avg_response_time_ms,
                b.avg_response_time_ms,
            ),
            Difference::new(
                "avg_task_completion_time_mins",
                a.avg_task_completion_time_mins,
                b.avg_task_completion_time_mins,
            ),
            Difference::new(
                "avg_tools_per_task",
                a.avg_tools_per_task,
                b.avg_tools_per_task,
            ),
            Difference::new(
                "session_abandonment_rate",
                a.session_abandonment_rate,
                b.session_abandonment_rate,
            ),
        ]
        .into_iter()
        .filter(Difference::is_significant)
        .collect();

        let assessment = if similarity > 0.95 {
            "Nearly identical behavior patterns".to_string()
        } else if similarity > 0.80 {
            format!(
                "Similar behavior with {} notable differences",
                differences.len()
            )
        } else if similarity > 0.60 {
            format!(
                "Moderate behavioral differences ({} significant)",
                differences.len()
            )
        } else {
            format!(
                "Substantially different behavior patterns ({} differences)",
                differences.len()
            )
        };

        Self {
            dna_a_id: a.dna_id.clone(),
            dna_b_id: b.dna_id.clone(),
            similarity,
            differences,
            assessment,
        }
    }
}

/// Anomaly detected in agent behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Anomaly {
    /// Metric that is anomalous
    pub metric: String,
    /// Current value
    pub current: f64,
    /// Expected (baseline) value
    pub expected: f64,
    /// Standard deviations from expected
    pub deviation_sigmas: f64,
    /// Severity: "low", "medium", "high", "critical"
    pub severity: String,
    /// Human-readable description
    pub description: String,
}

impl Anomaly {
    /// Create an anomaly from a metric deviation
    #[must_use]
    pub fn new(metric: impl Into<String>, current: f64, expected: f64, stddev: f64) -> Self {
        let metric = metric.into();
        let deviation_sigmas = if stddev > f64::EPSILON {
            (current - expected) / stddev
        } else {
            0.0
        };

        let severity = if deviation_sigmas.abs() >= 4.0 {
            "critical"
        } else if deviation_sigmas.abs() >= 3.0 {
            "high"
        } else if deviation_sigmas.abs() >= 2.0 {
            "medium"
        } else {
            "low"
        }
        .to_string();

        let direction = if current > expected { "above" } else { "below" };
        let description = format!(
            "{} is {:.1}σ {} expected ({:.2} vs {:.2})",
            metric,
            deviation_sigmas.abs(),
            direction,
            current,
            expected
        );

        Self {
            metric,
            current,
            expected,
            deviation_sigmas,
            severity,
            description,
        }
    }

    /// Check if this anomaly is significant (>2σ)
    #[must_use]
    pub fn is_significant(&self) -> bool {
        self.deviation_sigmas.abs() > 2.0
    }
}

/// Configuration for DNA computation
#[derive(Debug, Clone)]
pub struct DnaComputeConfig {
    /// Minimum samples required for computation
    pub min_samples: usize,
    /// Time range for data collection
    pub time_range: TimeRange,
    /// Whether to compute embedding
    pub compute_embedding: bool,
    /// Anomaly detection threshold (sigma)
    pub anomaly_threshold: f64,
}

impl Default for DnaComputeConfig {
    fn default() -> Self {
        Self {
            min_samples: 10,
            time_range: TimeRange::last_days(30),
            compute_embedding: true,
            anomaly_threshold: 2.0,
        }
    }
}

/// DNA computer for generating and analyzing agent DNA
pub struct DnaComputer {
    config: DnaComputeConfig,
}

impl DnaComputer {
    /// Create a new DNA computer with default configuration
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: DnaComputeConfig::default(),
        }
    }

    /// Create with custom configuration
    #[must_use]
    pub fn with_config(config: DnaComputeConfig) -> Self {
        Self { config }
    }

    /// Compute DNA from raw metrics
    ///
    /// This method takes pre-computed statistics and creates an `AgentDna`.
    /// For actual data collection from store, use `compute_from_store`.
    ///
    /// # Errors
    ///
    /// Returns [`DnaError::InsufficientData`] when the provided sample count is
    /// below the configured minimum threshold.
    #[instrument(skip(self), fields(program = %program, model = %model))]
    pub fn compute_dna(
        &self,
        program: &str,
        model: &str,
        config_hash: Option<&str>,
        stats: DnaStats,
    ) -> Result<AgentDna, DnaError> {
        if stats.sample_count < self.config.min_samples {
            return Err(DnaError::InsufficientData {
                needed: self.config.min_samples,
                have: stats.sample_count,
            });
        }

        let mut dna = AgentDna::new(program, model);

        if let Some(hash) = config_hash {
            dna = dna.with_config_hash(hash);
        }

        // Apply statistics
        dna.avg_tokens_per_turn = stats.avg_tokens_per_turn;
        dna.avg_input_output_ratio = stats.avg_input_output_ratio;
        dna.token_variance = stats.token_variance;
        dna.error_rate = stats.error_rate;
        dna.common_error_types = stats.common_error_types;
        dna.recovery_rate = stats.recovery_rate;
        dna.tool_preferences = stats.tool_preferences;
        dna.tool_success_rates = stats.tool_success_rates;
        dna.avg_tools_per_task = stats.avg_tools_per_task;
        dna.avg_response_time_ms = stats.avg_response_time_ms;
        dna.p95_response_time_ms = stats.p95_response_time_ms;
        dna.time_of_day_distribution = stats.time_of_day_distribution;
        dna.avg_task_completion_time_mins = stats.avg_task_completion_time_mins;
        dna.task_success_rate = stats.task_success_rate;
        dna.complexity_handling = stats.complexity_handling;
        dna.avg_session_duration_mins = stats.avg_session_duration_mins;
        dna.avg_turns_per_session = stats.avg_turns_per_session;
        dna.session_abandonment_rate = stats.session_abandonment_rate;

        // Compute embedding if configured
        if self.config.compute_embedding {
            dna.compute_embedding();
        }

        info!(
            dna_id = %dna.dna_id,
            samples = stats.sample_count,
            "Computed agent DNA"
        );

        Ok(dna)
    }

    /// Compare two DNA profiles
    #[must_use]
    pub fn compare(&self, a: &AgentDna, b: &AgentDna) -> DnaComparison {
        DnaComparison::compare(a, b)
    }

    /// Detect anomalies by comparing current DNA against historical baseline
    #[instrument(skip(self, current, history))]
    pub fn detect_anomalies(&self, current: &AgentDna, history: &[AgentDna]) -> Vec<Anomaly> {
        if history.is_empty() {
            return vec![];
        }

        let mut anomalies = Vec::new();

        // Check each key metric against historical baseline.
        let metrics = [
            MetricDef {
                name: "error_rate",
                getter: metric_error_rate,
            },
            MetricDef {
                name: "recovery_rate",
                getter: metric_recovery_rate,
            },
            MetricDef {
                name: "avg_tokens_per_turn",
                getter: metric_avg_tokens_per_turn,
            },
            MetricDef {
                name: "task_success_rate",
                getter: metric_task_success_rate,
            },
            MetricDef {
                name: "avg_response_time_ms",
                getter: metric_avg_response_time_ms,
            },
            MetricDef {
                name: "session_abandonment_rate",
                getter: metric_session_abandonment_rate,
            },
        ];

        for metric in metrics {
            let current_val = (metric.getter)(current);
            let historical: Vec<f64> = history.iter().map(metric.getter).collect();
            let (mean, stddev) = mean_stddev(&historical);

            if stddev > f64::EPSILON {
                let deviation = (current_val - mean).abs() / stddev;
                if deviation > self.config.anomaly_threshold {
                    anomalies.push(Anomaly::new(metric.name, current_val, mean, stddev));
                }
            }
        }

        if !anomalies.is_empty() {
            warn!(
                dna_id = %current.dna_id,
                anomaly_count = anomalies.len(),
                "Detected behavioral anomalies"
            );
        }

        anomalies
    }

    /// Find similar agents by embedding similarity
    #[must_use]
    pub fn find_similar<'a>(
        &self,
        target: &AgentDna,
        candidates: &'a [AgentDna],
        limit: usize,
    ) -> Vec<(&'a AgentDna, f64)> {
        let mut scored: Vec<_> = candidates
            .iter()
            .filter(|c| c.dna_id != target.dna_id)
            .map(|c| {
                (
                    c,
                    cosine_similarity(&target.dna_embedding, &c.dna_embedding),
                )
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);
        scored
    }
}

impl Default for DnaComputer {
    fn default() -> Self {
        Self::new()
    }
}

/// Raw statistics for DNA computation
#[derive(Debug, Clone, Default)]
pub struct DnaStats {
    pub sample_count: usize,
    pub avg_tokens_per_turn: f64,
    pub avg_input_output_ratio: f64,
    pub token_variance: f64,
    pub error_rate: f64,
    pub common_error_types: HashMap<String, f64>,
    pub recovery_rate: f64,
    pub tool_preferences: HashMap<String, f64>,
    pub tool_success_rates: HashMap<String, f64>,
    pub avg_tools_per_task: f64,
    pub avg_response_time_ms: f64,
    pub p95_response_time_ms: f64,
    pub time_of_day_distribution: HashMap<u8, f64>,
    pub avg_task_completion_time_mins: f64,
    pub task_success_rate: f64,
    pub complexity_handling: HashMap<String, f64>,
    pub avg_session_duration_mins: f64,
    pub avg_turns_per_session: f64,
    pub session_abandonment_rate: f64,
}

/// Compute cosine similarity between two vectors
#[must_use]
pub fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0;
    }

    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let mag_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let mag_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();

    if mag_a > f64::EPSILON && mag_b > f64::EPSILON {
        (dot / (mag_a * mag_b)).clamp(-1.0, 1.0)
    } else if mag_a <= f64::EPSILON && mag_b <= f64::EPSILON {
        // Both vectors are zero/near-zero - consider them identical
        1.0
    } else {
        // One is zero and the other is not - no similarity
        0.0
    }
}

/// Compute mean and standard deviation
#[must_use]
pub fn mean_stddev(values: &[f64]) -> (f64, f64) {
    if values.is_empty() {
        return (0.0, 0.0);
    }

    let n = usize_to_f64(values.len());
    let mean = values.iter().sum::<f64>() / n;

    if values.len() < 2 {
        return (mean, 0.0);
    }

    let variance = values.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0);
    (mean, variance.sqrt())
}

fn usize_to_f64(value: usize) -> f64 {
    f64::from(u32::try_from(value).unwrap_or(u32::MAX))
}

struct MetricDef {
    name: &'static str,
    getter: fn(&AgentDna) -> f64,
}

fn metric_error_rate(dna: &AgentDna) -> f64 {
    dna.error_rate
}

fn metric_recovery_rate(dna: &AgentDna) -> f64 {
    dna.recovery_rate
}

fn metric_avg_tokens_per_turn(dna: &AgentDna) -> f64 {
    dna.avg_tokens_per_turn
}

fn metric_task_success_rate(dna: &AgentDna) -> f64 {
    dna.task_success_rate
}

fn metric_avg_response_time_ms(dna: &AgentDna) -> f64 {
    dna.avg_response_time_ms
}

fn metric_session_abandonment_rate(dna: &AgentDna) -> f64 {
    dna.session_abandonment_rate
}

#[cfg(test)]
#[allow(clippy::cast_lossless, clippy::cast_precision_loss, clippy::float_cmp)]
mod tests {
    use super::*;

    // =============================================================================
    // TimeRange Tests
    // =============================================================================

    #[test]
    fn test_time_range_last_days() {
        let range = TimeRange::last_days(7);
        let duration = range.duration();
        assert!(duration >= Duration::days(6));
        assert!(duration <= Duration::days(8));
    }

    #[test]
    fn test_time_range_last_hours() {
        let range = TimeRange::last_hours(24);
        let duration = range.duration();
        assert!(duration >= Duration::hours(23));
        assert!(duration <= Duration::hours(25));
    }

    #[test]
    fn test_time_range_default() {
        let range = TimeRange::default();
        let duration = range.duration();
        assert!(duration >= Duration::days(29));
    }

    // =============================================================================
    // AgentDna Tests
    // =============================================================================

    #[test]
    fn test_agent_dna_new() {
        let dna = AgentDna::new("claude-code", "opus-4.5");
        assert_eq!(dna.agent_program, "claude-code");
        assert_eq!(dna.agent_model, "opus-4.5");
        assert_eq!(dna.dna_id, "claude-code:opus-4.5:default");
        assert!(dna.configuration_hash.is_none());
    }

    #[test]
    fn test_agent_dna_with_config_hash() {
        let dna = AgentDna::new("codex", "gpt5").with_config_hash("abc123");
        assert_eq!(dna.dna_id, "codex:gpt5:abc123");
        assert_eq!(dna.configuration_hash, Some("abc123".to_string()));
    }

    #[test]
    fn test_agent_dna_to_feature_vector() {
        let mut dna = AgentDna::new("test", "model");
        dna.avg_tokens_per_turn = 5000.0;
        dna.error_rate = 0.1;
        dna.task_success_rate = 0.9;

        let features = dna.to_feature_vector();
        assert!(!features.is_empty());
        assert!(features.iter().all(|&f| f.is_finite()));
    }

    #[test]
    fn test_agent_dna_compute_embedding() {
        let mut dna = AgentDna::new("test", "model");
        dna.avg_tokens_per_turn = 1000.0;
        dna.error_rate = 0.05;
        dna.compute_embedding();

        assert_eq!(dna.dna_embedding.len(), 128);

        // Check normalization (unit length)
        let magnitude: f64 = dna.dna_embedding.iter().map(|x| x * x).sum::<f64>().sqrt();
        assert!((magnitude - 1.0).abs() < 0.01 || magnitude < 0.01);
    }

    #[test]
    fn test_agent_dna_serialization() {
        let dna = AgentDna::new("claude-code", "opus-4.5");
        let json = serde_json::to_string(&dna).unwrap();
        let parsed: AgentDna = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.dna_id, dna.dna_id);
    }

    // =============================================================================
    // DnaHistory Tests
    // =============================================================================

    #[test]
    fn test_dna_history_from_dna() {
        let dna = AgentDna::new("test", "model");
        let history = DnaHistory::from_dna(&dna, Some("Initial".to_string()));

        assert_eq!(history.dna_id, dna.dna_id);
        assert_eq!(history.change_summary, Some("Initial".to_string()));
        assert!(history.id.is_none());
    }

    // =============================================================================
    // Difference Tests
    // =============================================================================

    #[test]
    fn test_difference_new() {
        let diff = Difference::new("error_rate", 0.1, 0.2);
        assert_eq!(diff.metric, "error_rate");
        assert!((diff.delta - 0.1).abs() < f64::EPSILON);
        assert!((diff.delta_pct - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_difference_is_significant() {
        let small = Difference::new("rate", 0.5, 0.51);
        assert!(!small.is_significant());

        let large = Difference::new("rate", 0.5, 0.7);
        assert!(large.is_significant());
    }

    #[test]
    fn test_difference_zero_base() {
        let diff = Difference::new("metric", 0.0, 0.1);
        assert!((diff.delta_pct - 0.0).abs() < f64::EPSILON);
    }

    // =============================================================================
    // DnaComparison Tests
    // =============================================================================

    #[test]
    fn test_dna_comparison_identical() {
        let mut dna1 = AgentDna::new("claude", "opus");
        let mut dna2 = dna1.clone();
        dna1.compute_embedding();
        dna2.compute_embedding();

        let comparison = DnaComparison::compare(&dna1, &dna2);
        assert!(comparison.similarity > 0.99);
        assert!(comparison.assessment.contains("identical"));
    }

    #[test]
    fn test_dna_comparison_different() {
        let mut dna1 = AgentDna::new("claude", "opus");
        let mut dna2 = AgentDna::new("codex", "gpt5");
        dna1.error_rate = 0.1;
        dna2.error_rate = 0.5;
        dna1.compute_embedding();
        dna2.compute_embedding();

        let comparison = DnaComparison::compare(&dna1, &dna2);
        assert!(!comparison.differences.is_empty());
    }

    // =============================================================================
    // Anomaly Tests
    // =============================================================================

    #[test]
    fn test_anomaly_new() {
        // Use 0.31 to avoid floating point boundary issues: (0.31 - 0.1) / 0.05 = 4.2
        let anomaly = Anomaly::new("error_rate", 0.31, 0.1, 0.05);
        assert_eq!(anomaly.metric, "error_rate");
        assert!(anomaly.deviation_sigmas > 4.0);
        assert_eq!(anomaly.severity, "critical");
    }

    #[test]
    fn test_anomaly_is_significant() {
        let sig = Anomaly::new("rate", 0.3, 0.1, 0.05);
        assert!(sig.is_significant());

        let not_sig = Anomaly::new("rate", 0.11, 0.1, 0.05);
        assert!(!not_sig.is_significant());
    }

    #[test]
    fn test_anomaly_severity_levels() {
        // Use values that clearly exceed thresholds to avoid floating point boundary issues
        // critical: >= 4.0 sigmas -> (0.31 - 0.1) / 0.05 = 4.2 sigmas
        let critical = Anomaly::new("m", 0.31, 0.1, 0.05);
        assert_eq!(critical.severity, "critical");

        // high: >= 3.0 but < 4.0 sigmas -> (0.26 - 0.1) / 0.05 = 3.2 sigmas
        let high = Anomaly::new("m", 0.26, 0.1, 0.05);
        assert_eq!(high.severity, "high");

        // medium: >= 2.0 but < 3.0 sigmas -> (0.21 - 0.1) / 0.05 = 2.2 sigmas
        let medium = Anomaly::new("m", 0.21, 0.1, 0.05);
        assert_eq!(medium.severity, "medium");

        // low: < 2.0 sigmas -> (0.18 - 0.1) / 0.05 = 1.6 sigmas
        let low = Anomaly::new("m", 0.18, 0.1, 0.05);
        assert_eq!(low.severity, "low");
    }

    // =============================================================================
    // DnaComputer Tests
    // =============================================================================

    #[test]
    fn test_dna_computer_new() {
        let computer = DnaComputer::new();
        assert_eq!(computer.config.min_samples, 10);
    }

    #[test]
    fn test_dna_computer_compute_dna_insufficient_data() {
        let computer = DnaComputer::new();
        let stats = DnaStats {
            sample_count: 5,
            ..Default::default()
        };

        let result = computer.compute_dna("test", "model", None, stats);
        assert!(matches!(result, Err(DnaError::InsufficientData { .. })));
    }

    #[test]
    fn test_dna_computer_compute_dna_success() {
        let computer = DnaComputer::new();
        let stats = DnaStats {
            sample_count: 100,
            avg_tokens_per_turn: 2500.0,
            error_rate: 0.05,
            task_success_rate: 0.95,
            ..Default::default()
        };

        let result = computer.compute_dna("claude-code", "opus-4.5", None, stats);
        assert!(result.is_ok());

        let dna = result.unwrap();
        assert_eq!(dna.agent_program, "claude-code");
        assert!((dna.error_rate - 0.05).abs() < f64::EPSILON);
        assert!(!dna.dna_embedding.is_empty());
    }

    #[test]
    fn test_dna_computer_detect_anomalies() {
        let computer = DnaComputer::new();

        let mut current = AgentDna::new("test", "model");
        current.error_rate = 0.5; // Anomalously high

        // History needs variation for stddev > 0
        let history: Vec<AgentDna> = (0..10)
            .map(|i| {
                let mut h = AgentDna::new("test", "model");
                // Add small variation: 0.04, 0.05, 0.06, 0.05, 0.04, ...
                h.error_rate = 0.05 + (f64::from(i) % 3.0 - 1.0) * 0.01;
                h
            })
            .collect();

        let anomalies = computer.detect_anomalies(&current, &history);
        assert!(!anomalies.is_empty());
        assert!(anomalies.iter().any(|a| a.metric == "error_rate"));
    }

    #[test]
    fn test_dna_computer_find_similar() {
        let computer = DnaComputer::new();

        let mut target = AgentDna::new("claude", "opus");
        target.error_rate = 0.1;
        target.compute_embedding();

        let mut similar = AgentDna::new("claude", "sonnet");
        similar.error_rate = 0.12;
        similar.compute_embedding();

        let mut different = AgentDna::new("codex", "gpt5");
        different.error_rate = 0.5;
        different.compute_embedding();

        let candidates = vec![similar.clone(), different];
        let results = computer.find_similar(&target, &candidates, 2);

        assert_eq!(results.len(), 2);
        // Similar should be ranked higher
        assert_eq!(results[0].0.agent_model, "sonnet");
    }

    // =============================================================================
    // Utility Function Tests
    // =============================================================================

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 2.0];
        let b = vec![-1.0, -2.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - (-1.0)).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_empty() {
        assert_eq!(cosine_similarity(&[], &[]), 0.0);
        assert_eq!(cosine_similarity(&[1.0], &[]), 0.0);
    }

    #[test]
    fn test_mean_stddev_empty() {
        let (mean, stddev) = mean_stddev(&[]);
        assert_eq!(mean, 0.0);
        assert_eq!(stddev, 0.0);
    }

    #[test]
    fn test_mean_stddev_single() {
        let (mean, stddev) = mean_stddev(&[5.0]);
        assert_eq!(mean, 5.0);
        assert_eq!(stddev, 0.0);
    }

    #[test]
    fn test_mean_stddev_uniform() {
        let values = vec![2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        let (mean, stddev) = mean_stddev(&values);
        assert!((mean - 5.0).abs() < 0.001);
        // Sample stddev with n-1: sqrt(32/7) ≈ 2.138
        assert!((stddev - 2.138).abs() < 0.01);
    }
}
