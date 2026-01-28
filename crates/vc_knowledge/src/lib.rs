//! vc_knowledge - Knowledge base for Vibe Cockpit
//!
//! This crate provides:
//! - Solution mining from agent sessions
//! - Gotcha database
//! - Playbook recommendations
//! - Pattern learning

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Knowledge errors
#[derive(Error, Debug)]
pub enum KnowledgeError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Store error: {0}")]
    StoreError(#[from] vc_store::StoreError),
}

/// A learned solution pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Solution {
    pub solution_id: String,
    pub title: String,
    pub description: String,
    pub problem_pattern: String,
    pub resolution_steps: Vec<String>,
    pub success_rate: f64,
    pub times_used: u32,
    pub source_sessions: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A gotcha/known issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gotcha {
    pub gotcha_id: String,
    pub title: String,
    pub description: String,
    pub symptoms: Vec<String>,
    pub workaround: Option<String>,
    pub severity: String,
    pub affected_tools: Vec<String>,
    pub discovered_at: DateTime<Utc>,
}

/// Knowledge base manager
pub struct KnowledgeBase {
    // Will hold store reference
}

impl KnowledgeBase {
    /// Create a new knowledge base
    pub fn new() -> Self {
        Self {}
    }

    /// Search for relevant solutions
    pub fn search_solutions(&self, _query: &str) -> Result<Vec<Solution>, KnowledgeError> {
        // Placeholder
        Ok(vec![])
    }

    /// Search for relevant gotchas
    pub fn search_gotchas(&self, _query: &str) -> Result<Vec<Gotcha>, KnowledgeError> {
        // Placeholder
        Ok(vec![])
    }

    /// Record a new solution from session analysis
    pub fn record_solution(&self, _solution: Solution) -> Result<(), KnowledgeError> {
        // Placeholder
        Ok(())
    }
}

impl Default for KnowledgeBase {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // KnowledgeBase tests
    #[test]
    fn test_knowledge_base_new() {
        let kb = KnowledgeBase::new();
        let solutions = kb.search_solutions("test").unwrap();
        assert!(solutions.is_empty());
    }

    #[test]
    fn test_knowledge_base_default() {
        let kb = KnowledgeBase::default();
        let gotchas = kb.search_gotchas("test").unwrap();
        assert!(gotchas.is_empty());
    }

    #[test]
    fn test_search_solutions_empty() {
        let kb = KnowledgeBase::new();
        let results = kb.search_solutions("rate limit").unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_gotchas_empty() {
        let kb = KnowledgeBase::new();
        let results = kb.search_gotchas("timeout").unwrap();
        assert!(results.is_empty());
    }

    // Solution tests
    #[test]
    fn test_solution_creation() {
        let now = Utc::now();
        let solution = Solution {
            solution_id: "sol-001".to_string(),
            title: "Rate Limit Recovery".to_string(),
            description: "How to recover from rate limits".to_string(),
            problem_pattern: "rate.*limit.*exceeded".to_string(),
            resolution_steps: vec![
                "Check current usage".to_string(),
                "Switch to backup account".to_string(),
            ],
            success_rate: 0.95,
            times_used: 42,
            source_sessions: vec!["session-1".to_string(), "session-2".to_string()],
            created_at: now,
            updated_at: now,
        };

        assert_eq!(solution.solution_id, "sol-001");
        assert_eq!(solution.resolution_steps.len(), 2);
        assert!(solution.success_rate > 0.9);
        assert_eq!(solution.times_used, 42);
    }

    #[test]
    fn test_solution_serialization() {
        let now = Utc::now();
        let solution = Solution {
            solution_id: "sol-ser".to_string(),
            title: "Test Solution".to_string(),
            description: "For testing".to_string(),
            problem_pattern: "test".to_string(),
            resolution_steps: vec!["step1".to_string()],
            success_rate: 0.8,
            times_used: 5,
            source_sessions: vec![],
            created_at: now,
            updated_at: now,
        };

        let json = serde_json::to_string(&solution).unwrap();
        let parsed: Solution = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.solution_id, solution.solution_id);
        assert_eq!(parsed.title, solution.title);
        assert_eq!(parsed.success_rate, solution.success_rate);
    }

    #[test]
    fn test_solution_empty_steps() {
        let now = Utc::now();
        let solution = Solution {
            solution_id: "empty".to_string(),
            title: "Empty Steps".to_string(),
            description: "No resolution steps".to_string(),
            problem_pattern: "".to_string(),
            resolution_steps: vec![],
            success_rate: 0.0,
            times_used: 0,
            source_sessions: vec![],
            created_at: now,
            updated_at: now,
        };

        assert!(solution.resolution_steps.is_empty());
        assert_eq!(solution.times_used, 0);
    }

    // Gotcha tests
    #[test]
    fn test_gotcha_creation() {
        let gotcha = Gotcha {
            gotcha_id: "gotcha-001".to_string(),
            title: "SSH Key Permission Issue".to_string(),
            description: "SSH keys with wrong permissions fail silently".to_string(),
            symptoms: vec![
                "Connection timeout".to_string(),
                "Permission denied".to_string(),
            ],
            workaround: Some("chmod 600 ~/.ssh/id_rsa".to_string()),
            severity: "medium".to_string(),
            affected_tools: vec!["ssh".to_string(), "rsync".to_string()],
            discovered_at: Utc::now(),
        };

        assert_eq!(gotcha.gotcha_id, "gotcha-001");
        assert_eq!(gotcha.symptoms.len(), 2);
        assert!(gotcha.workaround.is_some());
        assert_eq!(gotcha.affected_tools.len(), 2);
    }

    #[test]
    fn test_gotcha_no_workaround() {
        let gotcha = Gotcha {
            gotcha_id: "gotcha-002".to_string(),
            title: "Unknown Issue".to_string(),
            description: "Undiagnosed problem".to_string(),
            symptoms: vec!["Random failures".to_string()],
            workaround: None,
            severity: "high".to_string(),
            affected_tools: vec![],
            discovered_at: Utc::now(),
        };

        assert!(gotcha.workaround.is_none());
        assert!(gotcha.affected_tools.is_empty());
    }

    #[test]
    fn test_gotcha_serialization() {
        let gotcha = Gotcha {
            gotcha_id: "gotcha-ser".to_string(),
            title: "Serialization Test".to_string(),
            description: "Testing JSON".to_string(),
            symptoms: vec!["symptom1".to_string()],
            workaround: Some("fix it".to_string()),
            severity: "low".to_string(),
            affected_tools: vec!["tool1".to_string()],
            discovered_at: Utc::now(),
        };

        let json = serde_json::to_string(&gotcha).unwrap();
        let parsed: Gotcha = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.gotcha_id, gotcha.gotcha_id);
        assert_eq!(parsed.severity, gotcha.severity);
        assert_eq!(parsed.workaround, gotcha.workaround);
    }

    #[test]
    fn test_gotcha_severity_values() {
        let severities = ["low", "medium", "high", "critical"];

        for severity in severities {
            let gotcha = Gotcha {
                gotcha_id: format!("sev-{}", severity),
                title: "Severity Test".to_string(),
                description: "Testing severity".to_string(),
                symptoms: vec![],
                workaround: None,
                severity: severity.to_string(),
                affected_tools: vec![],
                discovered_at: Utc::now(),
            };
            assert_eq!(gotcha.severity, severity);
        }
    }

    // KnowledgeError tests
    #[test]
    fn test_error_not_found() {
        let err = KnowledgeError::NotFound("solution-123".to_string());
        assert!(err.to_string().contains("Not found"));
        assert!(err.to_string().contains("solution-123"));
    }

    // record_solution tests
    #[test]
    fn test_record_solution() {
        let kb = KnowledgeBase::new();
        let solution = Solution {
            solution_id: "new-sol".to_string(),
            title: "New Solution".to_string(),
            description: "Test".to_string(),
            problem_pattern: "test".to_string(),
            resolution_steps: vec![],
            success_rate: 1.0,
            times_used: 0,
            source_sessions: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        // Should succeed (placeholder returns Ok)
        assert!(kb.record_solution(solution).is_ok());
    }
}
