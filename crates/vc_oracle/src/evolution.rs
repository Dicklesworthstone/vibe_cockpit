//! Evolutionary Optimization for Agent Configurations
//!
//! This module implements genetic algorithms to automatically discover
//! high-performing agent configurations through evolution.
//!
//! # Algorithm Overview
//!
//! 1. **Initialize**: Create random population of configurations
//! 2. **Evaluate**: Score each configuration on fitness metrics
//! 3. **Select**: Choose top performers via tournament selection
//! 4. **Crossover**: Combine traits from two parents
//! 5. **Mutate**: Introduce random variations
//! 6. **Repeat**: Until convergence or generation limit
//!
//! # Example
//!
//! ```rust,ignore
//! use vc_oracle::evolution::*;
//!
//! // Define the genome structure
//! let genome_template = GenomeTemplate::new()
//!     .add_float("temperature", 0.0, 2.0, 1.0)
//!     .add_int("max_tokens", 100, 8000, 4000)
//!     .add_bool("streaming", true)
//!     .add_choice("model", &["opus-4.5", "sonnet-4"], 0);
//!
//! // Create evolution manager
//! let config = EvolutionConfig {
//!     population_size: 20,
//!     generations: 50,
//!     elite_count: 2,
//!     ..Default::default()
//! };
//!
//! let mut manager = EvolutionManager::new(genome_template, config);
//!
//! // Run evolution with fitness evaluator
//! let best = manager.evolve(|genome| {
//!     // Evaluate this configuration
//!     evaluate_agent_performance(genome)
//! }).await?;
//! ```

use rand::{Rng, RngExt, SeedableRng, rngs::StdRng};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use tracing::{debug, info, instrument, warn};

use crate::OracleError;

/// Errors specific to evolutionary optimization
#[derive(Error, Debug)]
pub enum EvolutionError {
    #[error("Invalid gene: {0}")]
    InvalidGene(String),

    #[error("Empty population")]
    EmptyPopulation,

    #[error("Genome mismatch: expected {expected}, got {got}")]
    GenomeMismatch { expected: usize, got: usize },

    #[error("Evolution failed: {0}")]
    EvolutionFailed(String),

    #[error("No convergence after {0} generations")]
    NoConvergence(usize),
}

impl From<EvolutionError> for OracleError {
    fn from(err: EvolutionError) -> Self {
        OracleError::PredictionFailed(err.to_string())
    }
}

/// Gene types for configuration parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Gene {
    /// Floating point value with range
    Float { value: f64, min: f64, max: f64 },
    /// Integer value with range
    Int { value: i64, min: i64, max: i64 },
    /// Boolean flag
    Bool { value: bool },
    /// Choice from a set of options (stored as index)
    Choice { value: usize, options: Vec<String> },
}

impl Gene {
    /// Create a new float gene
    #[must_use]
    pub fn float(value: f64, min: f64, max: f64) -> Self {
        Gene::Float {
            value: value.clamp(min, max),
            min,
            max,
        }
    }

    /// Create a new int gene
    #[must_use]
    pub fn int(value: i64, min: i64, max: i64) -> Self {
        Gene::Int {
            value: value.clamp(min, max),
            min,
            max,
        }
    }

    /// Create a new bool gene
    #[must_use]
    pub fn bool(value: bool) -> Self {
        Gene::Bool { value }
    }

    /// Create a new choice gene
    #[must_use]
    pub fn choice(value: usize, options: Vec<String>) -> Self {
        Gene::Choice {
            value: value.min(options.len().saturating_sub(1)),
            options,
        }
    }

    /// Get the value as a JSON value
    #[must_use]
    pub fn to_json_value(&self) -> serde_json::Value {
        match self {
            Gene::Float { value, .. } => serde_json::json!(value),
            Gene::Int { value, .. } => serde_json::json!(value),
            Gene::Bool { value } => serde_json::json!(value),
            Gene::Choice { value, options } => {
                serde_json::json!(options.get(*value).cloned().unwrap_or_default())
            }
        }
    }

    /// Mutate the gene with given probability
    pub fn mutate(&mut self, mutation_rate: f64, rng: &mut impl Rng) {
        if rng.random::<f64>() > mutation_rate {
            return;
        }

        match self {
            Gene::Float { value, min, max } => {
                // Gaussian mutation
                let range = *max - *min;
                let mutation = rng.random::<f64>() * range * 0.1 - range * 0.05;
                *value = (*value + mutation).clamp(*min, *max);
            }
            Gene::Int { value, min, max } => {
                // Random walk mutation
                let delta = if rng.random_bool(0.5) { 1 } else { -1 };
                let range = (*max - *min).max(1);
                let step = (range / 10).max(1);
                *value = (*value + delta * step).clamp(*min, *max);
            }
            Gene::Bool { value } => {
                // Flip mutation
                *value = !*value;
            }
            Gene::Choice { value, options } => {
                // Random choice mutation
                if !options.is_empty() {
                    *value = rng.random_range(0..options.len());
                }
            }
        }
    }

    /// Perform crossover with another gene
    #[must_use]
    pub fn crossover(&self, other: &Gene, rng: &mut impl Rng) -> Gene {
        if rng.random_bool(0.5) {
            self.clone()
        } else {
            match (self, other) {
                (
                    Gene::Float {
                        value: v1,
                        min,
                        max,
                    },
                    Gene::Float { value: v2, .. },
                ) => {
                    // Blend crossover for floats
                    let alpha = rng.random::<f64>();
                    let new_value = alpha * v1 + (1.0 - alpha) * v2;
                    Gene::Float {
                        value: new_value.clamp(*min, *max),
                        min: *min,
                        max: *max,
                    }
                }
                (
                    Gene::Int {
                        value: v1,
                        min,
                        max,
                    },
                    Gene::Int { value: v2, .. },
                ) => {
                    // Average for integers
                    let new_value = (v1 + v2) / 2;
                    Gene::Int {
                        value: new_value.clamp(*min, *max),
                        min: *min,
                        max: *max,
                    }
                }
                (Gene::Bool { .. }, Gene::Bool { value }) => Gene::Bool { value: *value },
                (Gene::Choice { options, .. }, Gene::Choice { value, .. }) => Gene::Choice {
                    value: *value,
                    options: options.clone(),
                },
                _ => self.clone(),
            }
        }
    }
}

/// Template for creating genomes
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GenomeTemplate {
    /// Gene definitions keyed by name
    pub genes: HashMap<String, Gene>,
    /// Ordered list of gene names
    pub gene_order: Vec<String>,
}

impl GenomeTemplate {
    /// Create a new empty genome template
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a float gene
    #[must_use]
    pub fn add_float(mut self, name: impl Into<String>, min: f64, max: f64, default: f64) -> Self {
        let name = name.into();
        self.gene_order.push(name.clone());
        self.genes.insert(name, Gene::float(default, min, max));
        self
    }

    /// Add an integer gene
    #[must_use]
    pub fn add_int(mut self, name: impl Into<String>, min: i64, max: i64, default: i64) -> Self {
        let name = name.into();
        self.gene_order.push(name.clone());
        self.genes.insert(name, Gene::int(default, min, max));
        self
    }

    /// Add a boolean gene
    #[must_use]
    pub fn add_bool(mut self, name: impl Into<String>, default: bool) -> Self {
        let name = name.into();
        self.gene_order.push(name.clone());
        self.genes.insert(name, Gene::bool(default));
        self
    }

    /// Add a choice gene
    #[must_use]
    pub fn add_choice(mut self, name: impl Into<String>, options: &[&str], default: usize) -> Self {
        let name = name.into();
        self.gene_order.push(name.clone());
        self.genes.insert(
            name,
            Gene::choice(default, options.iter().map(ToString::to_string).collect()),
        );
        self
    }

    /// Create a random genome from this template
    pub fn create_random(&self, rng: &mut impl Rng) -> Genome {
        let mut genes = HashMap::new();

        for (name, template) in &self.genes {
            let gene = match template {
                Gene::Float { min, max, .. } => {
                    Gene::float(rng.random_range(*min..*max), *min, *max)
                }
                Gene::Int { min, max, .. } => Gene::int(rng.random_range(*min..*max), *min, *max),
                Gene::Bool { .. } => Gene::bool(rng.random_bool(0.5)),
                Gene::Choice { options, .. } => {
                    Gene::choice(rng.random_range(0..options.len()), options.clone())
                }
            };
            genes.insert(name.clone(), gene);
        }

        Genome {
            genes,
            gene_order: self.gene_order.clone(),
        }
    }
}

/// A complete genome (collection of genes)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Genome {
    /// Gene values keyed by name
    pub genes: HashMap<String, Gene>,
    /// Ordered list of gene names
    pub gene_order: Vec<String>,
}

impl Genome {
    /// Get a gene by name
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Gene> {
        self.genes.get(name)
    }

    /// Get a mutable gene by name
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Gene> {
        self.genes.get_mut(name)
    }

    /// Convert genome to JSON configuration
    #[must_use]
    pub fn to_config(&self) -> serde_json::Value {
        let mut config = serde_json::Map::new();
        for name in &self.gene_order {
            if let Some(gene) = self.genes.get(name) {
                config.insert(name.clone(), gene.to_json_value());
            }
        }
        serde_json::Value::Object(config)
    }

    /// Mutate all genes with given probability
    pub fn mutate(&mut self, mutation_rate: f64, rng: &mut impl Rng) {
        for gene in self.genes.values_mut() {
            gene.mutate(mutation_rate, rng);
        }
    }

    /// Perform crossover with another genome
    #[must_use]
    pub fn crossover(&self, other: &Genome, rng: &mut impl Rng) -> Genome {
        let mut genes = HashMap::new();

        for name in &self.gene_order {
            let child_gene = match (self.genes.get(name), other.genes.get(name)) {
                (Some(g1), Some(g2)) => g1.crossover(g2, rng),
                (Some(g1), None) => g1.clone(),
                (None, Some(g2)) => g2.clone(),
                (None, None) => continue,
            };
            genes.insert(name.clone(), child_gene);
        }

        Genome {
            genes,
            gene_order: self.gene_order.clone(),
        }
    }
}

/// An individual in the population (genome + fitness)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Individual {
    /// The genome
    pub genome: Genome,
    /// Evaluated fitness (higher is better)
    pub fitness: Option<f64>,
    /// Generation this individual was created
    pub generation: usize,
    /// Individual ID for tracking
    pub id: String,
}

impl Individual {
    /// Create a new individual
    #[must_use]
    pub fn new(genome: Genome, generation: usize) -> Self {
        let id = format!("ind-{generation}-{}", rand::random::<u32>() % 10_000);
        Self {
            genome,
            fitness: None,
            generation,
            id,
        }
    }

    /// Set fitness score
    #[must_use]
    pub fn with_fitness(mut self, fitness: f64) -> Self {
        self.fitness = Some(fitness);
        self
    }
}

/// Fitness metrics for evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FitnessMetrics {
    pub task_success_rate: f64,
    pub avg_tokens_per_task: f64,
    pub avg_response_time_ms: f64,
    pub error_rate: f64,
    pub cost_per_task: f64,
}

impl FitnessMetrics {
    /// Convert metrics to fitness score using weights
    #[must_use]
    pub fn to_fitness(&self, weights: &FitnessWeights) -> f64 {
        let normalized_tokens = 1.0 - (self.avg_tokens_per_task / 10000.0).min(1.0);
        let normalized_time = 1.0 - (self.avg_response_time_ms / 60000.0).min(1.0);
        let normalized_error = 1.0 - self.error_rate;
        let normalized_cost = 1.0 - (self.cost_per_task / 1.0).min(1.0);

        weights.task_success_rate * self.task_success_rate
            + weights.tokens * normalized_tokens
            + weights.response_time * normalized_time
            + weights.error_rate * normalized_error
            + weights.cost * normalized_cost
    }
}

/// Weights for fitness calculation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FitnessWeights {
    pub task_success_rate: f64,
    pub tokens: f64,
    pub response_time: f64,
    pub error_rate: f64,
    pub cost: f64,
}

impl Default for FitnessWeights {
    fn default() -> Self {
        Self {
            task_success_rate: 0.4,
            tokens: 0.15,
            response_time: 0.15,
            error_rate: 0.2,
            cost: 0.1,
        }
    }
}

/// Configuration for evolution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionConfig {
    /// Number of individuals in population
    pub population_size: usize,
    /// Maximum number of generations
    pub max_generations: usize,
    /// Number of elite individuals preserved each generation
    pub elite_count: usize,
    /// Tournament size for selection
    pub tournament_size: usize,
    /// Probability of mutation per gene
    pub mutation_rate: f64,
    /// Probability of crossover
    pub crossover_rate: f64,
    /// Convergence threshold (stop if best fitness hasn't improved by this much)
    pub convergence_threshold: f64,
    /// Generations to wait for improvement before declaring convergence
    pub convergence_patience: usize,
    /// Fitness weights
    pub fitness_weights: FitnessWeights,
}

impl Default for EvolutionConfig {
    fn default() -> Self {
        Self {
            population_size: 20,
            max_generations: 50,
            elite_count: 2,
            tournament_size: 3,
            mutation_rate: 0.1,
            crossover_rate: 0.8,
            convergence_threshold: 0.001,
            convergence_patience: 10,
            fitness_weights: FitnessWeights::default(),
        }
    }
}

/// Statistics for a generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationStats {
    pub generation: usize,
    pub best_fitness: f64,
    pub avg_fitness: f64,
    pub worst_fitness: f64,
    pub fitness_std_dev: f64,
    pub best_individual_id: String,
}

/// Evolution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionResult {
    /// Best individual found
    pub best_individual: Individual,
    /// Best configuration as JSON
    pub best_config: serde_json::Value,
    /// Final generation reached
    pub final_generation: usize,
    /// Whether evolution converged
    pub converged: bool,
    /// Statistics for each generation
    pub generation_history: Vec<GenerationStats>,
    /// Total individuals evaluated
    pub total_evaluations: usize,
}

/// Evolution manager for running the genetic algorithm
pub struct EvolutionManager {
    /// Genome template for creating individuals
    template: GenomeTemplate,
    /// Evolution configuration
    config: EvolutionConfig,
    /// Current population
    population: Vec<Individual>,
    /// Current generation number
    generation: usize,
    /// History of generation statistics
    history: Vec<GenerationStats>,
    /// Best fitness seen so far
    best_fitness: f64,
    /// Generations since improvement
    stagnation_counter: usize,
    /// Total fitness evaluations performed
    total_evaluations: usize,
    /// Random number generator
    rng: StdRng,
}

impl EvolutionManager {
    /// Create a new evolution manager
    #[must_use]
    pub fn new(template: GenomeTemplate, config: EvolutionConfig) -> Self {
        Self {
            template,
            config,
            population: Vec::new(),
            generation: 0,
            history: Vec::new(),
            best_fitness: f64::NEG_INFINITY,
            stagnation_counter: 0,
            total_evaluations: 0,
            rng: StdRng::from_rng(&mut rand::rng()),
        }
    }

    /// Create with fixed seed for reproducibility
    #[must_use]
    pub fn with_seed(template: GenomeTemplate, config: EvolutionConfig, seed: u64) -> Self {
        Self {
            template,
            config,
            population: Vec::new(),
            generation: 0,
            history: Vec::new(),
            best_fitness: f64::NEG_INFINITY,
            stagnation_counter: 0,
            total_evaluations: 0,
            rng: StdRng::seed_from_u64(seed),
        }
    }

    /// Initialize the population with random individuals
    #[instrument(skip(self))]
    pub fn initialize(&mut self) {
        self.population.clear();
        self.generation = 0;
        self.history.clear();
        self.best_fitness = f64::NEG_INFINITY;
        self.stagnation_counter = 0;
        self.total_evaluations = 0;

        for _ in 0..self.config.population_size {
            let genome = self.template.create_random(&mut self.rng);
            self.population.push(Individual::new(genome, 0));
        }

        info!(
            population_size = self.config.population_size,
            "Initialized population"
        );
    }

    /// Evaluate fitness for all individuals without fitness
    pub fn evaluate<F>(&mut self, mut evaluator: F)
    where
        F: FnMut(&Genome) -> FitnessMetrics,
    {
        for individual in &mut self.population {
            if individual.fitness.is_none() {
                let metrics = evaluator(&individual.genome);
                let fitness = metrics.to_fitness(&self.config.fitness_weights);
                individual.fitness = Some(fitness);
                self.total_evaluations += 1;

                debug!(
                    individual_id = %individual.id,
                    fitness = fitness,
                    "Evaluated individual"
                );
            }
        }
    }

    /// Perform tournament selection
    ///
    /// # Panics
    /// Panics if population is empty or `tournament_size` is 0.
    /// Caller must ensure population is non-empty before calling.
    fn tournament_select(&mut self) -> Individual {
        assert!(
            !self.population.is_empty(),
            "cannot perform tournament selection on empty population"
        );
        assert!(
            self.config.tournament_size > 0,
            "tournament_size must be greater than 0"
        );

        let mut best: Option<&Individual> = None;

        for _ in 0..self.config.tournament_size {
            let idx = self.rng.random_range(0..self.population.len());
            let candidate = &self.population[idx];

            if best.is_none_or(|b| {
                candidate.fitness.unwrap_or(f64::NEG_INFINITY)
                    > b.fitness.unwrap_or(f64::NEG_INFINITY)
            }) {
                best = Some(candidate);
            }
        }

        best.expect("best should be Some after non-zero tournament iterations")
            .clone()
    }

    /// Create the next generation
    #[instrument(skip(self))]
    pub fn evolve_generation(&mut self) {
        // Sort by fitness (descending)
        self.population.sort_by(|a, b| {
            b.fitness
                .unwrap_or(f64::NEG_INFINITY)
                .partial_cmp(&a.fitness.unwrap_or(f64::NEG_INFINITY))
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Record statistics
        let fitnesses: Vec<f64> = self.population.iter().filter_map(|i| i.fitness).collect();

        let best_fitness = fitnesses.first().copied().unwrap_or(0.0);
        let avg_fitness = fitnesses.iter().sum::<f64>() / usize_to_f64(fitnesses.len().max(1));
        let worst_fitness = fitnesses.last().copied().unwrap_or(0.0);
        let (_, std_dev) = crate::dna::mean_stddev(&fitnesses);

        let stats = GenerationStats {
            generation: self.generation,
            best_fitness,
            avg_fitness,
            worst_fitness,
            fitness_std_dev: std_dev,
            best_individual_id: self
                .population
                .first()
                .map(|i| i.id.clone())
                .unwrap_or_default(),
        };
        self.history.push(stats);

        // Check for improvement
        if best_fitness > self.best_fitness + self.config.convergence_threshold {
            self.best_fitness = best_fitness;
            self.stagnation_counter = 0;
        } else {
            self.stagnation_counter += 1;
        }

        info!(
            generation = self.generation,
            best_fitness = best_fitness,
            avg_fitness = avg_fitness,
            stagnation = self.stagnation_counter,
            "Generation complete"
        );

        // Elitism: preserve top individuals
        let mut new_population: Vec<Individual> = self
            .population
            .iter()
            .take(self.config.elite_count)
            .cloned()
            .collect();

        // Create rest of population through selection and crossover
        while new_population.len() < self.config.population_size {
            let parent1 = self.tournament_select();
            let parent2 = self.tournament_select();

            let mut child_genome = if self.rng.random::<f64>() < self.config.crossover_rate {
                parent1.genome.crossover(&parent2.genome, &mut self.rng)
            } else {
                parent1.genome.clone()
            };

            child_genome.mutate(self.config.mutation_rate, &mut self.rng);

            new_population.push(Individual::new(child_genome, self.generation + 1));
        }

        self.population = new_population;
        self.generation += 1;
    }

    /// Check if evolution has converged
    #[must_use]
    pub fn has_converged(&self) -> bool {
        self.stagnation_counter >= self.config.convergence_patience
    }

    /// Check if maximum generations reached
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.generation >= self.config.max_generations
    }

    /// Get the best individual
    #[must_use]
    pub fn best(&self) -> Option<&Individual> {
        self.population.iter().max_by(|a, b| {
            a.fitness
                .unwrap_or(f64::NEG_INFINITY)
                .partial_cmp(&b.fitness.unwrap_or(f64::NEG_INFINITY))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    /// Run the full evolution process
    #[instrument(skip(self, evaluator))]
    pub fn run<F>(&mut self, mut evaluator: F) -> EvolutionResult
    where
        F: FnMut(&Genome) -> FitnessMetrics,
    {
        self.initialize();
        self.evaluate(&mut evaluator);

        while !self.is_complete() && !self.has_converged() {
            self.evolve_generation();
            self.evaluate(&mut evaluator);
        }

        let best = self.best().cloned().unwrap_or_else(|| {
            Individual::new(self.template.create_random(&mut self.rng), self.generation)
        });

        let result = EvolutionResult {
            best_config: best.genome.to_config(),
            best_individual: best,
            final_generation: self.generation,
            converged: self.has_converged(),
            generation_history: self.history.clone(),
            total_evaluations: self.total_evaluations,
        };

        info!(
            final_generation = result.final_generation,
            converged = result.converged,
            best_fitness = result.best_individual.fitness,
            total_evaluations = result.total_evaluations,
            "Evolution complete"
        );

        result
    }

    /// Get current population
    #[must_use]
    pub fn population(&self) -> &[Individual] {
        &self.population
    }

    /// Get current generation
    #[must_use]
    pub fn current_generation(&self) -> usize {
        self.generation
    }

    /// Get evolution history
    #[must_use]
    pub fn history(&self) -> &[GenerationStats] {
        &self.history
    }
}

fn usize_to_f64(value: usize) -> f64 {
    f64::from(u32::try_from(value).unwrap_or(u32::MAX))
}

#[cfg(test)]
#[allow(clippy::cast_precision_loss, clippy::float_cmp)]
mod tests {
    use super::*;

    fn sample_template() -> GenomeTemplate {
        GenomeTemplate::new()
            .add_float("temperature", 0.0, 2.0, 1.0)
            .add_int("max_tokens", 100, 8000, 4000)
            .add_bool("streaming", true)
            .add_choice("model", &["opus", "sonnet"], 0)
    }

    fn mock_evaluator(genome: &Genome) -> FitnessMetrics {
        // Simple mock that prefers temperature around 0.7 and lower tokens
        let temp = match genome.get("temperature") {
            Some(Gene::Float { value, .. }) => *value,
            _ => 1.0,
        };
        let tokens = match genome.get("max_tokens") {
            Some(Gene::Int { value, .. }) => *value as f64,
            _ => 4000.0,
        };

        let temp_score = 1.0 - (temp - 0.7).abs();
        let token_score = 1.0 - tokens / 8000.0;

        FitnessMetrics {
            task_success_rate: 0.5 + temp_score * 0.3 + token_score * 0.2,
            avg_tokens_per_task: tokens,
            avg_response_time_ms: 1000.0 + tokens / 4.0,
            error_rate: 0.1,
            cost_per_task: tokens / 1000.0 * 0.01,
        }
    }

    // =============================================================================
    // Gene Tests
    // =============================================================================

    #[test]
    fn test_gene_float() {
        let gene = Gene::float(1.5, 0.0, 2.0);
        match gene {
            Gene::Float { value, min, max } => {
                assert_eq!(value, 1.5);
                assert_eq!(min, 0.0);
                assert_eq!(max, 2.0);
            }
            _ => panic!("Expected float gene"),
        }
    }

    #[test]
    fn test_gene_float_clamps() {
        let gene = Gene::float(10.0, 0.0, 2.0);
        match gene {
            Gene::Float { value, .. } => {
                assert_eq!(value, 2.0);
            }
            _ => panic!("Expected float gene"),
        }
    }

    #[test]
    fn test_gene_to_json_value() {
        assert_eq!(
            Gene::float(1.5, 0.0, 2.0).to_json_value(),
            serde_json::json!(1.5)
        );
        assert_eq!(
            Gene::int(100, 0, 200).to_json_value(),
            serde_json::json!(100)
        );
        assert_eq!(Gene::bool(true).to_json_value(), serde_json::json!(true));
        assert_eq!(
            Gene::choice(1, vec!["a".to_string(), "b".to_string()]).to_json_value(),
            serde_json::json!("b")
        );
    }

    #[test]
    fn test_gene_mutate() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut gene = Gene::float(1.0, 0.0, 2.0);
        gene.mutate(1.0, &mut rng); // 100% mutation rate

        match gene {
            Gene::Float { value, .. } => {
                assert!((0.0..=2.0).contains(&value));
            }
            _ => panic!("Expected float gene"),
        }
    }

    // =============================================================================
    // GenomeTemplate Tests
    // =============================================================================

    #[test]
    fn test_genome_template_create() {
        let template = sample_template();
        assert_eq!(template.genes.len(), 4);
        assert_eq!(template.gene_order.len(), 4);
    }

    #[test]
    fn test_genome_template_create_random() {
        let template = sample_template();
        let mut rng = StdRng::seed_from_u64(42);
        let genome = template.create_random(&mut rng);

        assert_eq!(genome.genes.len(), 4);
        assert!(genome.get("temperature").is_some());
        assert!(genome.get("max_tokens").is_some());
    }

    // =============================================================================
    // Genome Tests
    // =============================================================================

    #[test]
    fn test_genome_to_config() {
        let template = sample_template();
        let mut rng = StdRng::seed_from_u64(42);
        let genome = template.create_random(&mut rng);
        let config = genome.to_config();

        assert!(config.is_object());
        assert!(config.get("temperature").is_some());
        assert!(config.get("max_tokens").is_some());
    }

    #[test]
    fn test_genome_crossover() {
        let template = sample_template();
        let mut rng = StdRng::seed_from_u64(42);

        let genome1 = template.create_random(&mut rng);
        let genome2 = template.create_random(&mut rng);
        let child = genome1.crossover(&genome2, &mut rng);

        assert_eq!(child.genes.len(), genome1.genes.len());
    }

    // =============================================================================
    // FitnessMetrics Tests
    // =============================================================================

    #[test]
    fn test_fitness_metrics_to_fitness() {
        let metrics = FitnessMetrics {
            task_success_rate: 0.9,
            avg_tokens_per_task: 2000.0,
            avg_response_time_ms: 5000.0,
            error_rate: 0.05,
            cost_per_task: 0.1,
        };

        let weights = FitnessWeights::default();
        let fitness = metrics.to_fitness(&weights);

        assert!(fitness > 0.0);
        assert!(fitness < 1.0);
    }

    // =============================================================================
    // EvolutionManager Tests
    // =============================================================================

    #[test]
    fn test_evolution_manager_initialize() {
        let template = sample_template();
        let config = EvolutionConfig {
            population_size: 10,
            ..Default::default()
        };

        let mut manager = EvolutionManager::with_seed(template, config, 42);
        manager.initialize();

        assert_eq!(manager.population.len(), 10);
        assert_eq!(manager.generation, 0);
    }

    #[test]
    fn test_evolution_manager_evaluate() {
        let template = sample_template();
        let config = EvolutionConfig {
            population_size: 10,
            ..Default::default()
        };

        let mut manager = EvolutionManager::with_seed(template, config, 42);
        manager.initialize();
        manager.evaluate(mock_evaluator);

        for individual in manager.population() {
            assert!(individual.fitness.is_some());
        }
    }

    #[test]
    fn test_evolution_manager_evolve_generation() {
        let template = sample_template();
        let config = EvolutionConfig {
            population_size: 10,
            ..Default::default()
        };

        let mut manager = EvolutionManager::with_seed(template, config, 42);
        manager.initialize();
        manager.evaluate(mock_evaluator);
        manager.evolve_generation();

        assert_eq!(manager.generation, 1);
        assert_eq!(manager.history.len(), 1);
    }

    #[test]
    fn test_evolution_manager_run() {
        let template = sample_template();
        let config = EvolutionConfig {
            population_size: 10,
            max_generations: 5,
            convergence_patience: 3,
            ..Default::default()
        };

        let mut manager = EvolutionManager::with_seed(template, config, 42);
        let result = manager.run(mock_evaluator);

        assert!(result.final_generation > 0);
        assert!(result.best_individual.fitness.is_some());
        assert!(!result.generation_history.is_empty());
    }

    #[test]
    fn test_evolution_improves_fitness() {
        let template = sample_template();
        let config = EvolutionConfig {
            population_size: 20,
            max_generations: 20,
            convergence_patience: 10,
            ..Default::default()
        };

        let mut manager = EvolutionManager::with_seed(template, config, 42);
        let result = manager.run(mock_evaluator);

        // Best fitness at end should be >= first generation
        let first_best = result
            .generation_history
            .first()
            .map_or(0.0, |s| s.best_fitness);
        let final_best = result.best_individual.fitness.unwrap_or(0.0);

        assert!(final_best >= first_best - 0.01); // Allow small tolerance
    }

    // =============================================================================
    // Convergence Tests
    // =============================================================================

    #[test]
    fn test_convergence_detection() {
        let template = sample_template();
        let config = EvolutionConfig {
            population_size: 5,
            max_generations: 100,
            convergence_patience: 3,
            ..Default::default()
        };

        // Use a constant evaluator to force convergence
        let constant_evaluator = |_: &Genome| FitnessMetrics {
            task_success_rate: 0.5,
            avg_tokens_per_task: 1000.0,
            avg_response_time_ms: 1000.0,
            error_rate: 0.1,
            cost_per_task: 0.01,
        };

        let mut manager = EvolutionManager::with_seed(template, config, 42);
        let result = manager.run(constant_evaluator);

        // Should converge before max generations
        assert!(result.converged || result.final_generation < 100);
    }
}
