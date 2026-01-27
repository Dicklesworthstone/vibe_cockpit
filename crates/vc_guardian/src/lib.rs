//! vc_guardian - Self-healing protocols for Vibe Cockpit
//!
//! This crate provides:
//! - Playbook definitions and execution
//! - Automated remediation
//! - Fleet orchestration commands
//! - Approval workflow

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

/// Guardian errors
#[derive(Error, Debug)]
pub enum GuardianError {
    #[error("Playbook not found: {0}")]
    PlaybookNotFound(String),

    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Rate limited: max {0} runs per hour")]
    RateLimited(u32),

    #[error("Approval required")]
    ApprovalRequired,

    #[error("Store error: {0}")]
    StoreError(#[from] vc_store::StoreError),
}

/// Playbook definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playbook {
    pub playbook_id: String,
    pub name: String,
    pub description: String,
    pub trigger: PlaybookTrigger,
    pub steps: Vec<PlaybookStep>,
    pub requires_approval: bool,
    pub max_runs_per_hour: u32,
    pub enabled: bool,
}

/// Playbook trigger conditions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PlaybookTrigger {
    OnAlert { rule_id: String },
    OnThreshold { query: String, operator: String, value: f64 },
    Manual,
}

/// Playbook execution step
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PlaybookStep {
    Log { message: String },
    Command { cmd: String, args: Vec<String>, timeout_secs: u64, allow_failure: bool },
    SwitchAccount { program: String, strategy: String },
    Notify { channel: String, message: String },
    Wait { seconds: u64 },
}

/// Playbook run status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookRun {
    pub id: i64,
    pub playbook_id: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub status: RunStatus,
    pub steps_completed: usize,
    pub steps_total: usize,
    pub error_message: Option<String>,
}

/// Run status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    Running,
    Success,
    Failed,
    Aborted,
    PendingApproval,
}

/// The Guardian executor
pub struct Guardian {
    playbooks: Vec<Playbook>,
}

impl Guardian {
    /// Create a new Guardian with default playbooks
    pub fn new() -> Self {
        Self {
            playbooks: Self::default_playbooks(),
        }
    }

    /// Get default built-in playbooks
    fn default_playbooks() -> Vec<Playbook> {
        vec![
            Playbook {
                playbook_id: "rate-limit-switch".to_string(),
                name: "Rate Limit Account Switch".to_string(),
                description: "Switch to backup account when rate limit approaches".to_string(),
                trigger: PlaybookTrigger::OnAlert {
                    rule_id: "rate-limit-warning".to_string(),
                },
                steps: vec![
                    PlaybookStep::Log {
                        message: "Rate limit warning detected, switching account".to_string(),
                    },
                    PlaybookStep::SwitchAccount {
                        program: "claude-code".to_string(),
                        strategy: "least_used".to_string(),
                    },
                    PlaybookStep::Notify {
                        channel: "tui".to_string(),
                        message: "Switched to backup account due to rate limit".to_string(),
                    },
                ],
                requires_approval: false,
                max_runs_per_hour: 3,
                enabled: true,
            },
        ]
    }

    /// Get all playbooks
    pub fn playbooks(&self) -> &[Playbook] {
        &self.playbooks
    }

    /// Find playbook by ID
    pub fn get_playbook(&self, id: &str) -> Option<&Playbook> {
        self.playbooks.iter().find(|p| p.playbook_id == id)
    }
}

impl Default for Guardian {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_playbooks() {
        let guardian = Guardian::new();
        assert!(!guardian.playbooks().is_empty());
    }

    #[test]
    fn test_get_playbook() {
        let guardian = Guardian::new();
        let playbook = guardian.get_playbook("rate-limit-switch");
        assert!(playbook.is_some());
    }
}
