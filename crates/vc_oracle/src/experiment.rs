//! A/B Testing Framework for Agent Configurations
//!
//! This module provides controlled experimentation capabilities:
//! - Create experiments with control and variant groups
//! - Weighted random assignment to variants
//! - Metric collection and observation storage
//! - Statistical analysis with significance testing
//! - Experiment lifecycle management
//!
//! # Example
//!
//! ```rust,ignore
//! use vc_oracle::experiment::*;
//!
//! // Create an experiment
//! let config = ExperimentConfig {
//!     name: "new_prompt_v2".to_string(),
//!     primary_metric: Metric::TaskCompletionRate,
//!     variants: vec![
//!         VariantConfig::control("control", agent_config_a),
//!         VariantConfig::treatment("new_prompt", agent_config_b),
//!     ],
//!     target_sample_size: 100,
//!     significance_threshold: 0.05,
//!     ..Default::default()
//! };
//!
//! let experiment = manager.create(config).await?;
//! manager.start(&experiment.experiment_id).await?;
//!
//! // Assign sessions
//! let variant = manager.assign(&experiment.experiment_id, "session_123").await?;
//!
//! // Record metrics
//! collector.record("session_123", Metric::TaskCompletionRate, 1.0).await?;
//!
//! // Analyze results
//! let results = analyzer.analyze(&experiment.experiment_id).await?;
//! ```

use chrono::{DateTime, Duration, Utc};
use rand::RngExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use tracing::{debug, info, instrument, warn};

use crate::OracleError;

/// Errors specific to experimentation
#[derive(Error, Debug)]
pub enum ExperimentError {
    #[error("Experiment not found: {0}")]
    NotFound(String),

    #[error("Invalid experiment state: {0}")]
    InvalidState(String),

    #[error("No control variant defined")]
    NoControl,

    #[error("Experiment not running: {0}")]
    NotRunning(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Insufficient samples for analysis: need {needed}, have {have}")]
    InsufficientSamples { needed: usize, have: usize },

    #[error("Store error: {0}")]
    StoreError(String),
}

impl From<ExperimentError> for OracleError {
    fn from(err: ExperimentError) -> Self {
        OracleError::PredictionFailed(err.to_string())
    }
}

/// Experiment status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExperimentStatus {
    #[default]
    Draft,
    Running,
    Paused,
    Completed,
}

impl ExperimentStatus {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            ExperimentStatus::Draft => "draft",
            ExperimentStatus::Running => "running",
            ExperimentStatus::Paused => "paused",
            ExperimentStatus::Completed => "completed",
        }
    }
}

impl std::str::FromStr for ExperimentStatus {
    type Err = ExperimentError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "draft" => Ok(ExperimentStatus::Draft),
            "running" => Ok(ExperimentStatus::Running),
            "paused" => Ok(ExperimentStatus::Paused),
            "completed" => Ok(ExperimentStatus::Completed),
            _ => Err(ExperimentError::InvalidState(format!(
                "Unknown status: {s}"
            ))),
        }
    }
}

/// Metrics that can be tracked in experiments
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Metric {
    TaskCompletionRate,
    AverageTokensPerTask,
    ErrorRate,
    AverageResponseTime,
    CostPerTask,
    RecoveryRate,
    ToolSuccessRate,
    SessionDuration,
    Custom { name: String, description: String },
}

impl Metric {
    /// Get the name of the metric
    #[must_use]
    pub fn name(&self) -> String {
        match self {
            Metric::TaskCompletionRate => "task_completion_rate".to_string(),
            Metric::AverageTokensPerTask => "avg_tokens_per_task".to_string(),
            Metric::ErrorRate => "error_rate".to_string(),
            Metric::AverageResponseTime => "avg_response_time".to_string(),
            Metric::CostPerTask => "cost_per_task".to_string(),
            Metric::RecoveryRate => "recovery_rate".to_string(),
            Metric::ToolSuccessRate => "tool_success_rate".to_string(),
            Metric::SessionDuration => "session_duration".to_string(),
            Metric::Custom { name, .. } => name.clone(),
        }
    }

    /// Check if a higher value is better for this metric
    #[must_use]
    pub fn higher_is_better(&self) -> bool {
        match self {
            Metric::AverageTokensPerTask
            | Metric::ErrorRate
            | Metric::AverageResponseTime
            | Metric::CostPerTask
            | Metric::SessionDuration => false,
            Metric::TaskCompletionRate
            | Metric::RecoveryRate
            | Metric::ToolSuccessRate
            | Metric::Custom { .. } => true,
        }
    }
}

/// Experiment configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentConfig {
    /// Human-readable name
    pub name: String,
    /// Detailed description
    pub description: Option<String>,
    /// What we expect to happen
    pub hypothesis: Option<String>,
    /// Variant configurations
    pub variants: Vec<VariantConfig>,
    /// Primary metric to optimize
    pub primary_metric: Metric,
    /// Additional metrics to track
    pub secondary_metrics: Vec<Metric>,
    /// Target number of samples per variant
    pub target_sample_size: u32,
    /// Minimum runtime before declaring results
    pub min_runtime_hours: Option<u32>,
    /// Maximum runtime before auto-stopping
    pub max_runtime_hours: Option<u32>,
    /// Stop early if significance is reached
    pub stop_early_on_significance: bool,
    /// P-value threshold for significance
    pub significance_threshold: f64,
    /// Who created this experiment
    pub created_by: Option<String>,
}

impl Default for ExperimentConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: None,
            hypothesis: None,
            variants: Vec::new(),
            primary_metric: Metric::TaskCompletionRate,
            secondary_metrics: Vec::new(),
            target_sample_size: 100,
            min_runtime_hours: Some(24),
            max_runtime_hours: Some(168), // 1 week
            stop_early_on_significance: true,
            significance_threshold: 0.05,
            created_by: None,
        }
    }
}

/// Variant configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantConfig {
    /// Variant name
    pub name: String,
    /// Whether this is the control group
    pub is_control: bool,
    /// Configuration to apply for this variant
    pub config: serde_json::Value,
    /// Traffic allocation weight (relative)
    pub traffic_weight: f64,
}

impl VariantConfig {
    /// Create a control variant
    #[must_use]
    pub fn control(name: impl Into<String>, config: serde_json::Value) -> Self {
        Self {
            name: name.into(),
            is_control: true,
            config,
            traffic_weight: 1.0,
        }
    }

    /// Create a treatment variant
    #[must_use]
    pub fn treatment(name: impl Into<String>, config: serde_json::Value) -> Self {
        Self {
            name: name.into(),
            is_control: false,
            config,
            traffic_weight: 1.0,
        }
    }

    /// Set traffic weight
    #[must_use]
    pub fn with_weight(mut self, weight: f64) -> Self {
        self.traffic_weight = weight;
        self
    }
}

/// Experiment definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experiment {
    pub experiment_id: String,
    pub name: String,
    pub description: Option<String>,
    pub hypothesis: Option<String>,
    pub status: ExperimentStatus,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub target_sample_size: u32,
    pub actual_sample_size: u32,
    pub primary_metric: Metric,
    pub secondary_metrics: Vec<Metric>,
    pub significance_threshold: f64,
    pub stop_early_on_significance: bool,
    pub min_runtime_hours: Option<u32>,
    pub max_runtime_hours: Option<u32>,
    pub created_at: DateTime<Utc>,
    pub created_by: Option<String>,
}

impl Experiment {
    /// Create from configuration
    #[must_use]
    pub fn from_config(config: ExperimentConfig) -> Self {
        let experiment_id = format!(
            "exp-{}-{}",
            config.name.replace(' ', "_").to_lowercase(),
            Utc::now().timestamp_millis() % 100_000
        );

        Self {
            experiment_id,
            name: config.name,
            description: config.description,
            hypothesis: config.hypothesis,
            status: ExperimentStatus::Draft,
            started_at: None,
            ended_at: None,
            target_sample_size: config.target_sample_size,
            actual_sample_size: 0,
            primary_metric: config.primary_metric,
            secondary_metrics: config.secondary_metrics,
            significance_threshold: config.significance_threshold,
            stop_early_on_significance: config.stop_early_on_significance,
            min_runtime_hours: config.min_runtime_hours,
            max_runtime_hours: config.max_runtime_hours,
            created_at: Utc::now(),
            created_by: config.created_by,
        }
    }

    /// Check if the experiment has reached minimum runtime
    #[must_use]
    pub fn has_min_runtime(&self) -> bool {
        match (self.started_at, self.min_runtime_hours) {
            (Some(start), Some(hours)) => Utc::now() - start >= Duration::hours(i64::from(hours)),
            (_, None) => true, // No minimum
            (None, _) => false,
        }
    }

    /// Check if the experiment has reached maximum runtime
    #[must_use]
    pub fn has_max_runtime(&self) -> bool {
        match (self.started_at, self.max_runtime_hours) {
            (Some(start), Some(hours)) => Utc::now() - start >= Duration::hours(i64::from(hours)),
            _ => false,
        }
    }

    /// Check if target sample size is reached
    #[must_use]
    pub fn has_target_samples(&self) -> bool {
        self.actual_sample_size >= self.target_sample_size
    }
}

/// Variant definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variant {
    pub variant_id: String,
    pub experiment_id: String,
    pub name: String,
    pub is_control: bool,
    pub config: serde_json::Value,
    pub traffic_weight: f64,
    pub sample_count: u32,
}

impl Variant {
    /// Create from config with experiment ID
    #[must_use]
    pub fn from_config(experiment_id: &str, config: &VariantConfig) -> Self {
        let variant_id = format!(
            "{}-{}",
            experiment_id,
            config.name.replace(' ', "_").to_lowercase()
        );

        Self {
            variant_id,
            experiment_id: experiment_id.to_string(),
            name: config.name.clone(),
            is_control: config.is_control,
            config: config.config.clone(),
            traffic_weight: config.traffic_weight,
            sample_count: 0,
        }
    }
}

/// Assignment of a session to a variant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assignment {
    pub id: Option<i64>,
    pub experiment_id: String,
    pub variant_id: String,
    pub session_id: String,
    pub assigned_at: DateTime<Utc>,
}

/// Metric observation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub id: Option<i64>,
    pub experiment_id: String,
    pub variant_id: String,
    pub session_id: String,
    pub metric_name: String,
    pub metric_value: f64,
    pub observed_at: DateTime<Utc>,
}

/// Statistics for a variant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantStats {
    pub variant_id: String,
    pub variant_name: String,
    pub is_control: bool,
    pub sample_size: usize,
    pub mean: f64,
    pub std_dev: f64,
    pub min: f64,
    pub max: f64,
    pub confidence_interval: (f64, f64),
}

/// Comparison result between two variants
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantComparison {
    pub treatment_id: String,
    pub control_id: String,
    pub t_statistic: f64,
    pub p_value: f64,
    pub lift: f64, // Percentage improvement
    pub is_significant: bool,
    pub direction: String, // "better", "worse", "neutral"
}

/// Experiment results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentResults {
    pub experiment_id: String,
    pub computed_at: DateTime<Utc>,
    pub variant_stats: Vec<VariantStats>,
    pub comparisons: Vec<VariantComparison>,
    pub winner_variant: Option<String>,
    pub confidence_level: f64,
    pub primary_metric_lift: f64,
    pub is_significant: bool,
    pub recommendation: String,
}

/// Experiment manager for lifecycle operations
#[derive(Debug)]
pub struct ExperimentManager {
    /// In-memory storage for demonstration (would use `VcStore` in production)
    experiments: HashMap<String, Experiment>,
    variants: HashMap<String, Vec<Variant>>,
    assignments: HashMap<String, Vec<Assignment>>,
}

#[allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::needless_pass_by_value
)]
impl ExperimentManager {
    /// Create a new experiment manager
    #[must_use]
    pub fn new() -> Self {
        Self {
            experiments: HashMap::new(),
            variants: HashMap::new(),
            assignments: HashMap::new(),
        }
    }

    /// Create a new experiment
    #[instrument(skip(self, config), fields(name = %config.name))]
    pub fn create(&mut self, config: ExperimentConfig) -> Result<Experiment, ExperimentError> {
        // Validate configuration
        Self::validate_config(&config)?;

        // Create experiment
        let experiment = Experiment::from_config(config.clone());
        let experiment_id = experiment.experiment_id.clone();

        // Create variants
        let variants: Vec<Variant> = config
            .variants
            .iter()
            .map(|v| Variant::from_config(&experiment_id, v))
            .collect();

        // Store
        self.experiments
            .insert(experiment_id.clone(), experiment.clone());
        self.variants.insert(experiment_id, variants);

        info!(
            experiment_id = %experiment.experiment_id,
            "Created experiment"
        );

        Ok(experiment)
    }

    /// Validate experiment configuration
    fn validate_config(config: &ExperimentConfig) -> Result<(), ExperimentError> {
        if config.name.is_empty() {
            return Err(ExperimentError::InvalidConfig(
                "Name is required".to_string(),
            ));
        }

        if config.variants.is_empty() {
            return Err(ExperimentError::InvalidConfig(
                "At least one variant required".to_string(),
            ));
        }

        let has_control = config.variants.iter().any(|v| v.is_control);
        if !has_control {
            return Err(ExperimentError::NoControl);
        }

        if config.target_sample_size == 0 {
            return Err(ExperimentError::InvalidConfig(
                "Target sample size must be > 0".to_string(),
            ));
        }

        if config.significance_threshold <= 0.0 || config.significance_threshold >= 1.0 {
            return Err(ExperimentError::InvalidConfig(
                "Significance threshold must be between 0 and 1".to_string(),
            ));
        }

        Ok(())
    }

    /// Start an experiment
    #[instrument(skip(self), fields(experiment_id = %experiment_id))]
    pub fn start(&mut self, experiment_id: &str) -> Result<(), ExperimentError> {
        let experiment = self
            .experiments
            .get_mut(experiment_id)
            .ok_or_else(|| ExperimentError::NotFound(experiment_id.to_string()))?;

        if experiment.status != ExperimentStatus::Draft {
            return Err(ExperimentError::InvalidState(
                "Can only start draft experiments".to_string(),
            ));
        }

        experiment.status = ExperimentStatus::Running;
        experiment.started_at = Some(Utc::now());

        info!(experiment_id = %experiment_id, "Started experiment");
        Ok(())
    }

    /// Pause an experiment
    #[instrument(skip(self), fields(experiment_id = %experiment_id))]
    pub fn pause(&mut self, experiment_id: &str) -> Result<(), ExperimentError> {
        let experiment = self
            .experiments
            .get_mut(experiment_id)
            .ok_or_else(|| ExperimentError::NotFound(experiment_id.to_string()))?;

        if experiment.status != ExperimentStatus::Running {
            return Err(ExperimentError::InvalidState(
                "Can only pause running experiments".to_string(),
            ));
        }

        experiment.status = ExperimentStatus::Paused;

        info!(experiment_id = %experiment_id, "Paused experiment");
        Ok(())
    }

    /// Complete an experiment
    #[instrument(skip(self), fields(experiment_id = %experiment_id))]
    pub fn complete(&mut self, experiment_id: &str) -> Result<(), ExperimentError> {
        let experiment = self
            .experiments
            .get_mut(experiment_id)
            .ok_or_else(|| ExperimentError::NotFound(experiment_id.to_string()))?;

        experiment.status = ExperimentStatus::Completed;
        experiment.ended_at = Some(Utc::now());

        info!(experiment_id = %experiment_id, "Completed experiment");
        Ok(())
    }

    /// Get an experiment
    #[must_use]
    pub fn get(&self, experiment_id: &str) -> Option<&Experiment> {
        self.experiments.get(experiment_id)
    }

    /// Get variants for an experiment
    #[must_use]
    pub fn get_variants(&self, experiment_id: &str) -> Option<&Vec<Variant>> {
        self.variants.get(experiment_id)
    }

    /// Assign a session to an experiment variant
    #[instrument(skip(self), fields(experiment_id = %experiment_id, session_id = %session_id))]
    pub fn assign(
        &mut self,
        experiment_id: &str,
        session_id: &str,
    ) -> Result<Variant, ExperimentError> {
        // Check experiment status first (immutable borrow)
        {
            let experiment = self
                .experiments
                .get(experiment_id)
                .ok_or_else(|| ExperimentError::NotFound(experiment_id.to_string()))?;

            if experiment.status != ExperimentStatus::Running {
                return Err(ExperimentError::NotRunning(experiment_id.to_string()));
            }
        }

        // Check if already assigned
        let existing_variant_id = self
            .assignments
            .get(experiment_id)
            .and_then(|a| a.iter().find(|a| a.session_id == session_id))
            .map(|a| a.variant_id.clone());

        if let Some(variant_id) = existing_variant_id {
            let variants = self.variants.get(experiment_id).unwrap();
            let variant = variants
                .iter()
                .find(|v| v.variant_id == variant_id)
                .unwrap();
            return Ok(variant.clone());
        }

        // Select variant (immutable borrow for selection)
        let variant_id = {
            let variants = self.variants.get(experiment_id).unwrap();
            Self::select_variant(variants)?.variant_id.clone()
        };

        // Create assignment
        let assignment = Assignment {
            id: None,
            experiment_id: experiment_id.to_string(),
            variant_id: variant_id.clone(),
            session_id: session_id.to_string(),
            assigned_at: Utc::now(),
        };

        self.assignments
            .entry(experiment_id.to_string())
            .or_default()
            .push(assignment);

        // Update counts (mutable borrow)
        let variants = self.variants.get_mut(experiment_id).unwrap();
        for v in variants.iter_mut() {
            if v.variant_id == variant_id {
                v.sample_count += 1;
            }
        }

        let experiment = self.experiments.get_mut(experiment_id).unwrap();
        experiment.actual_sample_size += 1;

        let selected = self
            .variants
            .get(experiment_id)
            .unwrap()
            .iter()
            .find(|v| v.variant_id == variant_id)
            .unwrap()
            .clone();

        debug!(
            experiment_id = %experiment_id,
            variant = %selected.name,
            "Assigned session to variant"
        );

        Ok(selected)
    }

    /// Select a variant using weighted random selection
    fn select_variant(variants: &[Variant]) -> Result<&Variant, ExperimentError> {
        let total_weight: f64 = variants.iter().map(|v| v.traffic_weight).sum();

        if total_weight <= 0.0 {
            return Err(ExperimentError::InvalidConfig(
                "Total traffic weight must be > 0".to_string(),
            ));
        }

        let mut rng = rand::rng();
        let roll: f64 = rng.random::<f64>() * total_weight;

        let mut cumulative = 0.0;
        for variant in variants {
            cumulative += variant.traffic_weight;
            if roll <= cumulative {
                return Ok(variant);
            }
        }

        // Fallback to last variant (shouldn't happen with valid weights)
        Ok(variants.last().unwrap())
    }

    /// List all experiments
    #[must_use]
    pub fn list(&self, status: Option<ExperimentStatus>) -> Vec<&Experiment> {
        self.experiments
            .values()
            .filter(|e| status.is_none_or(|s| e.status == s))
            .collect()
    }
}

impl Default for ExperimentManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Metric collector for recording observations
pub struct MetricCollector {
    observations: HashMap<String, Vec<Observation>>,
}

impl MetricCollector {
    /// Create a new metric collector
    #[must_use]
    pub fn new() -> Self {
        Self {
            observations: HashMap::new(),
        }
    }

    /// Record a metric observation
    #[instrument(skip(self), fields(session_id = %session_id, metric = %metric.name()))]
    pub fn record(
        &mut self,
        manager: &ExperimentManager,
        session_id: &str,
        metric: &Metric,
        value: f64,
    ) {
        // Find experiments this session is assigned to
        for (experiment_id, assignments) in &manager.assignments {
            if let Some(assignment) = assignments.iter().find(|a| a.session_id == session_id)
                && let Some(experiment) = manager.get(experiment_id)
            {
                // Check if this metric is relevant
                let is_relevant = experiment.primary_metric.name() == metric.name()
                    || experiment
                        .secondary_metrics
                        .iter()
                        .any(|m| m.name() == metric.name());

                if is_relevant {
                    let observation = Observation {
                        id: None,
                        experiment_id: experiment_id.clone(),
                        variant_id: assignment.variant_id.clone(),
                        session_id: session_id.to_string(),
                        metric_name: metric.name(),
                        metric_value: value,
                        observed_at: Utc::now(),
                    };

                    self.observations
                        .entry(experiment_id.clone())
                        .or_default()
                        .push(observation);

                    debug!(
                        experiment_id = %experiment_id,
                        metric = %metric.name(),
                        value = value,
                        "Recorded observation"
                    );
                }
            }
        }
    }

    /// Get observations for an experiment and metric
    #[must_use]
    pub fn get_observations(
        &self,
        experiment_id: &str,
        variant_id: Option<&str>,
        metric_name: &str,
    ) -> Vec<&Observation> {
        self.observations
            .get(experiment_id)
            .map(|obs| {
                obs.iter()
                    .filter(|o| {
                        o.metric_name == metric_name && variant_id.is_none_or(|v| o.variant_id == v)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl Default for MetricCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Experiment analyzer for statistical analysis
pub struct ExperimentAnalyzer;

impl ExperimentAnalyzer {
    /// Analyze experiment results
    #[allow(
        clippy::missing_errors_doc,
        clippy::missing_panics_doc,
        clippy::too_many_lines
    )]
    #[instrument(skip(manager, collector), fields(experiment_id = %experiment_id))]
    pub fn analyze(
        manager: &ExperimentManager,
        collector: &MetricCollector,
        experiment_id: &str,
    ) -> Result<ExperimentResults, ExperimentError> {
        let experiment = manager
            .get(experiment_id)
            .ok_or_else(|| ExperimentError::NotFound(experiment_id.to_string()))?;

        let variants = manager
            .get_variants(experiment_id)
            .ok_or_else(|| ExperimentError::NotFound(experiment_id.to_string()))?;

        let control = variants
            .iter()
            .find(|v| v.is_control)
            .ok_or(ExperimentError::NoControl)?;

        let metric_name = experiment.primary_metric.name();
        let higher_is_better = experiment.primary_metric.higher_is_better();

        // Compute stats for each variant
        let mut variant_stats = Vec::new();

        for variant in variants {
            let observations =
                collector.get_observations(experiment_id, Some(&variant.variant_id), &metric_name);

            let values: Vec<f64> = observations.iter().map(|o| o.metric_value).collect();

            let (mean, std_dev) = if values.is_empty() {
                (0.0, 0.0)
            } else {
                crate::dna::mean_stddev(&values)
            };

            let min = values.iter().copied().fold(f64::INFINITY, f64::min);
            let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);

            let ci = confidence_interval_95(&values);

            variant_stats.push(VariantStats {
                variant_id: variant.variant_id.clone(),
                variant_name: variant.name.clone(),
                is_control: variant.is_control,
                sample_size: values.len(),
                mean,
                std_dev,
                min: if min.is_infinite() { 0.0 } else { min },
                max: if max.is_infinite() { 0.0 } else { max },
                confidence_interval: ci,
            });
        }

        // Find control stats
        let control_stats = variant_stats
            .iter()
            .find(|s| s.variant_id == control.variant_id)
            .unwrap();

        // Compare each treatment to control
        let mut comparisons = Vec::new();
        let mut best_treatment: Option<(String, f64, f64)> = None;

        for stats in &variant_stats {
            if !stats.is_control {
                let (t_stat, p_value) = two_sample_t_test(control_stats, stats);
                let lift = if control_stats.mean > 0.0 {
                    (stats.mean - control_stats.mean) / control_stats.mean * 100.0
                } else {
                    0.0
                };

                let is_significant = p_value < experiment.significance_threshold;
                let is_better = if higher_is_better {
                    stats.mean > control_stats.mean
                } else {
                    stats.mean < control_stats.mean
                };

                let direction = if is_significant {
                    if is_better { "better" } else { "worse" }
                } else {
                    "neutral"
                }
                .to_string();

                comparisons.push(VariantComparison {
                    treatment_id: stats.variant_id.clone(),
                    control_id: control.variant_id.clone(),
                    t_statistic: t_stat,
                    p_value,
                    lift,
                    is_significant,
                    direction: direction.clone(),
                });

                if is_significant && is_better {
                    let lift_abs = lift.abs();
                    if best_treatment
                        .as_ref()
                        .is_none_or(|(_, _, l)| lift_abs > *l)
                    {
                        best_treatment = Some((stats.variant_id.clone(), 1.0 - p_value, lift_abs));
                    }
                }
            }
        }

        // Determine winner and recommendation
        let (winner_variant, confidence_level, primary_metric_lift, is_significant) =
            if let Some((winner, conf, lift)) = best_treatment {
                (Some(winner), conf, lift, true)
            } else {
                (None, 0.0, 0.0, false)
            };

        let recommendation = if is_significant {
            format!(
                "Implement variant '{}' - shows {:.1}% improvement with {:.1}% confidence",
                winner_variant.as_ref().unwrap_or(&"unknown".to_string()),
                primary_metric_lift,
                confidence_level * 100.0
            )
        } else if variant_stats
            .iter()
            .all(|s| s.sample_size >= experiment.target_sample_size as usize)
        {
            "No significant difference found. Consider keeping current configuration.".to_string()
        } else {
            format!(
                "Insufficient data. Continue experiment until {} samples per variant.",
                experiment.target_sample_size
            )
        };

        let results = ExperimentResults {
            experiment_id: experiment_id.to_string(),
            computed_at: Utc::now(),
            variant_stats,
            comparisons,
            winner_variant,
            confidence_level,
            primary_metric_lift,
            is_significant,
            recommendation,
        };

        info!(
            experiment_id = %experiment_id,
            is_significant = is_significant,
            winner = ?results.winner_variant,
            "Analyzed experiment"
        );

        Ok(results)
    }
}

/// Compute 95% confidence interval for a sample
fn confidence_interval_95(values: &[f64]) -> (f64, f64) {
    if values.is_empty() {
        return (0.0, 0.0);
    }

    let (mean, std_dev) = crate::dna::mean_stddev(values);
    let n = usize_to_f64(values.len());

    // Use t-distribution critical value for 95% CI
    // For large samples, ~1.96. For smaller, use approximation.
    let t_critical = if n > 30.0 { 1.96 } else { 2.0 };

    let margin = t_critical * std_dev / n.sqrt();

    (mean - margin, mean + margin)
}

/// Two-sample t-test (Welch's t-test)
fn two_sample_t_test(a: &VariantStats, b: &VariantStats) -> (f64, f64) {
    if a.sample_size < 2 || b.sample_size < 2 {
        return (0.0, 1.0); // Cannot compute
    }

    let n1 = usize_to_f64(a.sample_size);
    let n2 = usize_to_f64(b.sample_size);

    let mean_diff = b.mean - a.mean;
    let var1 = a.std_dev * a.std_dev;
    let var2 = b.std_dev * b.std_dev;

    let se = (var1 / n1 + var2 / n2).sqrt();

    if se < f64::EPSILON {
        return (0.0, 1.0);
    }

    let t_stat = mean_diff / se;

    // Welch-Satterthwaite degrees of freedom approximation
    let df_num = (var1 / n1 + var2 / n2).powi(2);
    let df_denom = (var1 / n1).powi(2) / (n1 - 1.0) + (var2 / n2).powi(2) / (n2 - 1.0);
    let df = df_num / df_denom;

    // Approximate p-value using Student's t-distribution
    let p_value = t_distribution_p_value(t_stat.abs(), df);

    (t_stat, p_value)
}

/// Approximate p-value from t-distribution (two-tailed)
fn t_distribution_p_value(t: f64, df: f64) -> f64 {
    // Simple approximation using normal distribution for large df
    // For small df, this underestimates p-values slightly
    if df > 30.0 {
        // Use normal approximation
        2.0 * (1.0 - normal_cdf(t))
    } else {
        // Rough approximation for smaller df
        let correction = 1.0 + 0.5 / df;
        2.0 * (1.0 - normal_cdf(t / correction))
    }
}

/// Standard normal CDF approximation
fn normal_cdf(x: f64) -> f64 {
    // Abramowitz and Stegun approximation
    let a1 = 0.254_829_592;
    let a2 = -0.284_496_736;
    let a3 = 1.421_413_741;
    let a4 = -1.453_152_027;
    let a5 = 1.061_405_429;
    let p = 0.327_591_1;

    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();

    let t = 1.0 / (1.0 + p * x);
    let y = 1.0 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * (-x * x / 2.0).exp();

    0.5 * (1.0 + sign * y)
}

fn usize_to_f64(value: usize) -> f64 {
    f64::from(u32::try_from(value).unwrap_or(u32::MAX))
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    fn sample_config() -> ExperimentConfig {
        ExperimentConfig {
            name: "test_experiment".to_string(),
            description: Some("A test experiment".to_string()),
            hypothesis: Some("Variant B will be better".to_string()),
            variants: vec![
                VariantConfig::control("control", serde_json::json!({"version": "v1"})),
                VariantConfig::treatment("variant_b", serde_json::json!({"version": "v2"})),
            ],
            primary_metric: Metric::TaskCompletionRate,
            secondary_metrics: vec![],
            target_sample_size: 50,
            min_runtime_hours: None,
            max_runtime_hours: None,
            stop_early_on_significance: true,
            significance_threshold: 0.05,
            created_by: Some("test".to_string()),
        }
    }

    // =============================================================================
    // ExperimentStatus Tests
    // =============================================================================

    #[test]
    fn test_experiment_status_from_str() {
        assert_eq!(
            "draft".parse::<ExperimentStatus>().unwrap(),
            ExperimentStatus::Draft
        );
        assert_eq!(
            "running".parse::<ExperimentStatus>().unwrap(),
            ExperimentStatus::Running
        );
        assert!("invalid".parse::<ExperimentStatus>().is_err());
    }

    #[test]
    fn test_experiment_status_as_str() {
        assert_eq!(ExperimentStatus::Draft.as_str(), "draft");
        assert_eq!(ExperimentStatus::Completed.as_str(), "completed");
    }

    // =============================================================================
    // Metric Tests
    // =============================================================================

    #[test]
    fn test_metric_name() {
        assert_eq!(Metric::TaskCompletionRate.name(), "task_completion_rate");
        assert_eq!(Metric::ErrorRate.name(), "error_rate");
        assert_eq!(
            Metric::Custom {
                name: "custom".to_string(),
                description: "test".to_string()
            }
            .name(),
            "custom"
        );
    }

    #[test]
    fn test_metric_higher_is_better() {
        assert!(Metric::TaskCompletionRate.higher_is_better());
        assert!(!Metric::ErrorRate.higher_is_better());
        assert!(!Metric::CostPerTask.higher_is_better());
    }

    // =============================================================================
    // VariantConfig Tests
    // =============================================================================

    #[test]
    fn test_variant_config_control() {
        let config = VariantConfig::control("control", serde_json::json!({}));
        assert!(config.is_control);
        assert_eq!(config.name, "control");
        assert_eq!(config.traffic_weight, 1.0);
    }

    #[test]
    fn test_variant_config_treatment() {
        let config = VariantConfig::treatment("treatment", serde_json::json!({})).with_weight(2.0);
        assert!(!config.is_control);
        assert_eq!(config.traffic_weight, 2.0);
    }

    // =============================================================================
    // ExperimentManager Tests
    // =============================================================================

    #[test]
    fn test_create_experiment() {
        let mut manager = ExperimentManager::new();
        let config = sample_config();

        let experiment = manager.create(config).unwrap();
        assert_eq!(experiment.status, ExperimentStatus::Draft);
        assert!(experiment.experiment_id.starts_with("exp-"));
    }

    #[test]
    fn test_create_experiment_no_control() {
        let mut manager = ExperimentManager::new();
        let config = ExperimentConfig {
            name: "test".to_string(),
            variants: vec![VariantConfig::treatment("treatment", serde_json::json!({}))],
            ..Default::default()
        };

        assert!(matches!(
            manager.create(config),
            Err(ExperimentError::NoControl)
        ));
    }

    #[test]
    fn test_start_experiment() {
        let mut manager = ExperimentManager::new();
        let experiment = manager.create(sample_config()).unwrap();
        let id = experiment.experiment_id.clone();

        manager.start(&id).unwrap();

        let updated = manager.get(&id).unwrap();
        assert_eq!(updated.status, ExperimentStatus::Running);
        assert!(updated.started_at.is_some());
    }

    #[test]
    fn test_assign_session() {
        let mut manager = ExperimentManager::new();
        let experiment = manager.create(sample_config()).unwrap();
        let id = experiment.experiment_id.clone();
        manager.start(&id).unwrap();

        let variant = manager.assign(&id, "session-1").unwrap();
        assert!(!variant.variant_id.is_empty());

        // Same session should get same variant
        let variant2 = manager.assign(&id, "session-1").unwrap();
        assert_eq!(variant.variant_id, variant2.variant_id);
    }

    #[test]
    fn test_assign_not_running() {
        let mut manager = ExperimentManager::new();
        let experiment = manager.create(sample_config()).unwrap();
        let id = experiment.experiment_id.clone();

        assert!(matches!(
            manager.assign(&id, "session-1"),
            Err(ExperimentError::NotRunning(_))
        ));
    }

    // =============================================================================
    // MetricCollector Tests
    // =============================================================================

    #[test]
    fn test_record_observation() {
        let mut manager = ExperimentManager::new();
        let experiment = manager.create(sample_config()).unwrap();
        let id = experiment.experiment_id.clone();
        manager.start(&id).unwrap();
        manager.assign(&id, "session-1").unwrap();

        let mut collector = MetricCollector::new();
        collector.record(&manager, "session-1", &Metric::TaskCompletionRate, 1.0);

        let observations = collector.get_observations(&id, None, "task_completion_rate");
        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0].metric_value, 1.0);
    }

    // =============================================================================
    // Statistical Tests
    // =============================================================================

    #[test]
    fn test_confidence_interval_95() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let (low, high) = confidence_interval_95(&values);
        assert!(low < 3.0);
        assert!(high > 3.0);
    }

    #[test]
    fn test_confidence_interval_empty() {
        let (low, high) = confidence_interval_95(&[]);
        assert_eq!(low, 0.0);
        assert_eq!(high, 0.0);
    }

    #[test]
    fn test_two_sample_t_test() {
        let control = VariantStats {
            variant_id: "control".to_string(),
            variant_name: "Control".to_string(),
            is_control: true,
            sample_size: 30,
            mean: 0.5,
            std_dev: 0.1,
            min: 0.3,
            max: 0.7,
            confidence_interval: (0.45, 0.55),
        };

        let treatment = VariantStats {
            variant_id: "treatment".to_string(),
            variant_name: "Treatment".to_string(),
            is_control: false,
            sample_size: 30,
            mean: 0.6,
            std_dev: 0.1,
            min: 0.4,
            max: 0.8,
            confidence_interval: (0.55, 0.65),
        };

        let (t_stat, p_value) = two_sample_t_test(&control, &treatment);
        assert!(t_stat > 0.0); // Treatment mean is higher
        assert!(p_value < 0.05); // Should be significant
    }

    #[test]
    fn test_normal_cdf() {
        // Standard normal at 0 should be 0.5
        assert!((normal_cdf(0.0) - 0.5).abs() < 0.001);
        // At 1.96, should be ~0.975
        assert!((normal_cdf(1.96) - 0.975).abs() < 0.01);
    }

    // =============================================================================
    // ExperimentAnalyzer Tests
    // =============================================================================

    #[test]
    fn test_analyze_experiment() {
        let mut manager = ExperimentManager::new();
        let experiment = manager.create(sample_config()).unwrap();
        let id = experiment.experiment_id.clone();
        manager.start(&id).unwrap();

        let mut collector = MetricCollector::new();

        // Assign and record for multiple sessions
        for i in 0..50 {
            let session_id = format!("session-{i}");
            let variant = manager.assign(&id, &session_id).unwrap();

            // Control gets ~0.5, treatment gets ~0.7
            let value = if variant.is_control {
                0.5 + rand::random::<f64>() * 0.1 - 0.05
            } else {
                0.7 + rand::random::<f64>() * 0.1 - 0.05
            };
            collector.record(&manager, &session_id, &Metric::TaskCompletionRate, value);
        }

        let results = ExperimentAnalyzer::analyze(&manager, &collector, &id).unwrap();

        assert_eq!(results.variant_stats.len(), 2);
        assert!(!results.recommendation.is_empty());
    }
}
