//! Solution mining pipeline for extracting knowledge from agent sessions.
//!
//! The mining pipeline:
//! 1. Session Selection - Find unmined successful sessions
//! 2. Transcript Analysis - Identify problem-solution pairs
//! 3. Solution Extraction - Extract reusable patterns
//! 4. Quality Scoring - Rank by usefulness
//! 5. Knowledge Storage - Store in knowledge base
//! 6. Deduplication - Skip entries too similar to existing ones

use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::sync::Arc;
use vc_store::VcStore;

use crate::{EntryType, KnowledgeEntry, KnowledgeError, KnowledgeStore, SearchOptions};

/// A problem-solution pair extracted from a session transcript.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProblemSolutionPair {
    pub problem: String,
    pub solution: String,
    pub insights: Vec<String>,
    pub code_snippets: Vec<String>,
    pub quality: u8,
    pub tags: Vec<String>,
}

/// A candidate session for mining.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCandidate {
    pub session_id: String,
    pub machine_id: String,
    pub program: Option<String>,
    pub model: Option<String>,
    pub repo_path: Option<String>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub token_count: Option<i64>,
}

/// Result of mining a single session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiningResult {
    pub session_id: String,
    pub solutions_extracted: usize,
    pub patterns_extracted: usize,
    pub quality_avg: f64,
    pub entries_created: Vec<i64>,
}

/// Mining statistics summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiningStats {
    pub total_mined: i64,
    pub total_solutions: i64,
    pub total_patterns: i64,
    pub avg_quality: f64,
}

/// The solution miner orchestrates the mining pipeline.
pub struct SolutionMiner {
    store: Arc<VcStore>,
    knowledge: KnowledgeStore,
    min_quality: u8,
}

impl SolutionMiner {
    /// Create a new miner with the given store.
    #[must_use]
    pub fn new(store: Arc<VcStore>) -> Self {
        let knowledge = KnowledgeStore::new(store.clone());
        Self {
            store,
            knowledge,
            min_quality: 3,
        }
    }

    /// Set minimum quality threshold (1-5).
    #[must_use]
    pub fn with_min_quality(mut self, quality: u8) -> Self {
        self.min_quality = quality.clamp(1, 5);
        self
    }

    /// List unmined session candidates.
    ///
    /// # Errors
    ///
    /// Returns an error if querying unmined sessions fails.
    pub fn candidates(&self, limit: usize) -> Result<Vec<SessionCandidate>, KnowledgeError> {
        let rows = self.store.list_unmined_sessions(limit)?;
        let candidates: Vec<SessionCandidate> = rows
            .into_iter()
            .filter_map(|row| {
                Some(SessionCandidate {
                    session_id: row.get("session_id")?.as_str()?.to_string(),
                    machine_id: row.get("machine_id")?.as_str()?.to_string(),
                    program: row
                        .get("program")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    model: row.get("model").and_then(|v| v.as_str()).map(String::from),
                    repo_path: row
                        .get("repo_path")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    started_at: row
                        .get("started_at")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    ended_at: row
                        .get("ended_at")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    token_count: row.get("token_count").and_then(serde_json::Value::as_i64),
                })
            })
            .collect();
        Ok(candidates)
    }

    /// Check if a session has already been mined.
    ///
    /// # Errors
    ///
    /// Returns an error if mined-session lookup fails.
    pub fn is_mined(&self, session_id: &str) -> Result<bool, KnowledgeError> {
        Ok(self.store.is_session_mined(session_id)?)
    }

    /// Analyze a session transcript and extract problem-solution pairs.
    ///
    /// This is a rule-based extractor that identifies patterns in session data.
    /// A future version could use LLM analysis for deeper extraction.
    ///
    /// # Errors
    ///
    /// Returns an error if session analysis requires store access and that access fails.
    pub fn analyze_session(
        &self,
        candidate: &SessionCandidate,
    ) -> Result<Vec<ProblemSolutionPair>, KnowledgeError> {
        // Rule-based extraction from session metadata
        let mut pairs = Vec::new();

        // Extract patterns from the session program and model
        if let Some(ref program) = candidate.program {
            let mut tags = vec![program.clone()];
            if let Some(ref model) = candidate.model {
                tags.push(model.clone());
            }

            // Sessions with large token counts often indicate complex problem solving
            let quality = match candidate.token_count {
                Some(tc) if tc > 50000 => 4,
                Some(tc) if tc > 20000 => 3,
                Some(tc) if tc > 5000 => 2,
                _ => 1,
            };

            if let Some(ref repo) = candidate.repo_path {
                let repo_name = repo.rsplit('/').next().unwrap_or(repo);
                tags.push(repo_name.to_string());

                pairs.push(ProblemSolutionPair {
                    problem: format!("Agent session in {repo_name} on {program}"),
                    solution: format!(
                        "Session completed successfully using {} with {} tokens",
                        candidate.model.as_deref().unwrap_or("unknown model"),
                        candidate.token_count.unwrap_or(0)
                    ),
                    insights: vec![
                        format!("Project: {}", repo_name),
                        format!("Program: {}", program),
                    ],
                    code_snippets: vec![],
                    quality,
                    tags,
                });
            }
        }

        Ok(pairs)
    }

    /// Extract solutions from a session and store them in the knowledge base.
    ///
    /// # Errors
    ///
    /// Returns an error if session mining state updates or store writes fail.
    pub fn extract(&self, candidate: &SessionCandidate) -> Result<MiningResult, KnowledgeError> {
        // Check if already mined
        if self.is_mined(&candidate.session_id)? {
            return Ok(MiningResult {
                session_id: candidate.session_id.clone(),
                solutions_extracted: 0,
                patterns_extracted: 0,
                quality_avg: 0.0,
                entries_created: vec![],
            });
        }

        // Analyze session
        let pairs = self.analyze_session(candidate)?;

        let mut entries_created = Vec::new();
        let mut quality_sum = 0u32;
        let mut solutions = 0_i32;
        let patterns = 0_i32;

        for pair in &pairs {
            if pair.quality < self.min_quality {
                continue;
            }

            // Check for duplicates via title search
            let search_results = self.knowledge.search(
                &pair.problem,
                &SearchOptions {
                    entry_type: Some(EntryType::Solution),
                    limit: 3,
                    ..Default::default()
                },
            )?;

            let is_duplicate = search_results
                .iter()
                .any(|r| r.entry.title == Self::generate_title(pair));

            if is_duplicate {
                continue;
            }

            let content = Self::format_solution(pair);
            let entry =
                KnowledgeEntry::new(EntryType::Solution, Self::generate_title(pair), content)
                    .with_summary(&pair.problem)
                    .with_session(&candidate.session_id)
                    .with_tags(pair.tags.clone());

            match self.knowledge.insert(&entry) {
                Ok(id) => {
                    entries_created.push(id);
                    solutions += 1;
                    quality_sum += u32::from(pair.quality);
                }
                Err(e) => {
                    tracing::warn!("Failed to store solution: {e}");
                }
            }
        }

        let quality_avg = if solutions > 0 {
            f64::from(quality_sum) / f64::from(solutions)
        } else {
            0.0
        };

        // Mark session as mined
        self.store.mark_session_mined(
            &candidate.session_id,
            &candidate.machine_id,
            solutions,
            patterns,
            if solutions > 0 {
                Some(quality_avg)
            } else {
                None
            },
        )?;

        let solutions_extracted = usize::try_from(solutions).unwrap_or_default();
        let patterns_extracted = usize::try_from(patterns).unwrap_or_default();

        Ok(MiningResult {
            session_id: candidate.session_id.clone(),
            solutions_extracted,
            patterns_extracted,
            quality_avg,
            entries_created,
        })
    }

    /// Run mining on all available candidates.
    ///
    /// # Errors
    ///
    /// Returns an error if loading candidate sessions fails.
    pub fn mine_all(&self, max_sessions: usize) -> Result<Vec<MiningResult>, KnowledgeError> {
        let candidates = self.candidates(max_sessions)?;
        let mut results = Vec::new();

        for candidate in &candidates {
            match self.extract(candidate) {
                Ok(result) => results.push(result),
                Err(e) => {
                    tracing::warn!(
                        session_id = %candidate.session_id,
                        "Mining failed: {e}"
                    );
                }
            }
        }

        Ok(results)
    }

    /// Get mining statistics.
    ///
    /// # Errors
    ///
    /// Returns an error if statistics query fails.
    pub fn stats(&self) -> Result<MiningStats, KnowledgeError> {
        let json = self.store.mining_stats()?;

        Ok(MiningStats {
            total_mined: json
                .get("total_mined")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0),
            total_solutions: json
                .get("total_solutions")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0),
            total_patterns: json
                .get("total_patterns")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0),
            avg_quality: json
                .get("avg_quality")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0),
        })
    }

    fn generate_title(pair: &ProblemSolutionPair) -> String {
        let max_len = 80;
        let char_count = pair.problem.chars().count();
        if char_count <= max_len {
            pair.problem.clone()
        } else {
            let truncated: String = pair.problem.chars().take(max_len - 2).collect();
            format!("{}..", truncated)
        }
    }

    fn format_solution(pair: &ProblemSolutionPair) -> String {
        let mut content = String::new();

        content.push_str("## Problem\n");
        content.push_str(&pair.problem);
        content.push_str("\n\n## Solution\n");
        content.push_str(&pair.solution);

        if !pair.insights.is_empty() {
            content.push_str("\n\n## Key Insights\n");
            for insight in &pair.insights {
                let _ = writeln!(content, "- {insight}");
            }
        }

        if !pair.code_snippets.is_empty() {
            content.push_str("\n\n## Code Examples\n");
            for snippet in &pair.code_snippets {
                let _ = writeln!(content, "```\n{snippet}\n```");
            }
        }

        content
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_problem_solution_pair_serde() {
        let pair = ProblemSolutionPair {
            problem: "Build fails".to_string(),
            solution: "Add missing dependency".to_string(),
            insights: vec!["Check Cargo.toml".to_string()],
            code_snippets: vec!["cargo add serde".to_string()],
            quality: 4,
            tags: vec!["rust".to_string()],
        };
        let json = serde_json::to_string(&pair).unwrap();
        let parsed: ProblemSolutionPair = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.problem, "Build fails");
        assert_eq!(parsed.quality, 4);
    }

    #[test]
    fn test_session_candidate_serde() {
        let candidate = SessionCandidate {
            session_id: "sess-123".to_string(),
            machine_id: "orko".to_string(),
            program: Some("claude-code".to_string()),
            model: Some("opus-4.6".to_string()),
            repo_path: Some("/data/projects/myrepo".to_string()),
            started_at: Some("2026-01-01T00:00:00Z".to_string()),
            ended_at: Some("2026-01-01T01:00:00Z".to_string()),
            token_count: Some(25_000),
        };
        let json = serde_json::to_string(&candidate).unwrap();
        assert!(json.contains("sess-123"));
    }

    #[test]
    fn test_mining_result_serde() {
        let result = MiningResult {
            session_id: "sess-1".to_string(),
            solutions_extracted: 3,
            patterns_extracted: 1,
            quality_avg: 3.5,
            entries_created: vec![1, 2, 3],
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: MiningResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.solutions_extracted, 3);
        assert_eq!(parsed.entries_created.len(), 3);
    }

    #[test]
    fn test_mining_stats_defaults() {
        let stats = MiningStats {
            total_mined: 0,
            total_solutions: 0,
            total_patterns: 0,
            avg_quality: 0.0,
        };
        assert_eq!(stats.total_mined, 0);
    }

    #[test]
    fn test_solution_miner_creation() {
        let store = Arc::new(VcStore::open_memory().unwrap());
        let miner = SolutionMiner::new(store);
        assert_eq!(miner.min_quality, 3);
    }

    #[test]
    fn test_solution_miner_quality_threshold() {
        let store = Arc::new(VcStore::open_memory().unwrap());
        let miner = SolutionMiner::new(store).with_min_quality(4);
        assert_eq!(miner.min_quality, 4);
    }

    #[test]
    fn test_solution_miner_quality_clamping() {
        let store = Arc::new(VcStore::open_memory().unwrap());
        let miner = SolutionMiner::new(store.clone()).with_min_quality(0);
        assert_eq!(miner.min_quality, 1);
        let miner = SolutionMiner::new(store).with_min_quality(10);
        assert_eq!(miner.min_quality, 5);
    }

    #[test]
    fn test_analyze_session_basic() {
        let store = Arc::new(VcStore::open_memory().unwrap());
        let miner = SolutionMiner::new(store);
        let candidate = SessionCandidate {
            session_id: "s1".to_string(),
            machine_id: "m1".to_string(),
            program: Some("claude-code".to_string()),
            model: Some("opus-4.6".to_string()),
            repo_path: Some("/data/projects/vibe_cockpit".to_string()),
            started_at: None,
            ended_at: None,
            token_count: Some(30000),
        };
        let pairs = miner.analyze_session(&candidate).unwrap();
        assert!(!pairs.is_empty());
        assert!(pairs[0].tags.contains(&"claude-code".to_string()));
        assert_eq!(pairs[0].quality, 3); // 20000 < 30000 < 50000
    }

    #[test]
    fn test_analyze_session_high_token() {
        let store = Arc::new(VcStore::open_memory().unwrap());
        let miner = SolutionMiner::new(store);
        let candidate = SessionCandidate {
            session_id: "s1".to_string(),
            machine_id: "m1".to_string(),
            program: Some("claude-code".to_string()),
            model: Some("opus-4.6".to_string()),
            repo_path: Some("/data/projects/big_project".to_string()),
            started_at: None,
            ended_at: None,
            token_count: Some(100_000),
        };
        let pairs = miner.analyze_session(&candidate).unwrap();
        assert_eq!(pairs[0].quality, 4);
    }

    #[test]
    fn test_analyze_session_no_program() {
        let store = Arc::new(VcStore::open_memory().unwrap());
        let miner = SolutionMiner::new(store);
        let candidate = SessionCandidate {
            session_id: "s1".to_string(),
            machine_id: "m1".to_string(),
            program: None,
            model: None,
            repo_path: None,
            started_at: None,
            ended_at: None,
            token_count: None,
        };
        let pairs = miner.analyze_session(&candidate).unwrap();
        assert!(pairs.is_empty());
    }

    #[test]
    fn test_format_solution() {
        let pair = ProblemSolutionPair {
            problem: "Build fails".to_string(),
            solution: "Add dependency".to_string(),
            insights: vec!["Check deps".to_string()],
            code_snippets: vec!["cargo add serde".to_string()],
            quality: 4,
            tags: vec![],
        };
        let content = SolutionMiner::format_solution(&pair);
        assert!(content.contains("## Problem"));
        assert!(content.contains("## Solution"));
        assert!(content.contains("## Key Insights"));
        assert!(content.contains("## Code Examples"));
        assert!(content.contains("cargo add serde"));
    }

    #[test]
    fn test_generate_title_short() {
        let pair = ProblemSolutionPair {
            problem: "Short problem".to_string(),
            solution: String::new(),
            insights: vec![],
            code_snippets: vec![],
            quality: 3,
            tags: vec![],
        };
        assert_eq!(SolutionMiner::generate_title(&pair), "Short problem");
    }

    #[test]
    fn test_generate_title_long() {
        let long = "A".repeat(100);
        let pair = ProblemSolutionPair {
            problem: long,
            solution: String::new(),
            insights: vec![],
            code_snippets: vec![],
            quality: 3,
            tags: vec![],
        };
        let title = SolutionMiner::generate_title(&pair);
        assert_eq!(title.len(), 80);
        assert!(title.ends_with(".."));
    }

    #[test]
    fn test_candidates_empty_store() {
        let store = Arc::new(VcStore::open_memory().unwrap());
        let miner = SolutionMiner::new(store);
        let candidates = miner.candidates(10).unwrap();
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_mining_stats_empty() {
        let store = Arc::new(VcStore::open_memory().unwrap());
        let miner = SolutionMiner::new(store);
        let stats = miner.stats().unwrap();
        assert_eq!(stats.total_mined, 0);
    }
}
