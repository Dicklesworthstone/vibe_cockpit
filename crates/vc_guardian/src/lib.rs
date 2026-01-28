//! vc_guardian - Self-healing protocols for Vibe Cockpit
//!
//! This crate provides:
//! - Playbook definitions and execution
//! - Automated remediation
//! - Fleet orchestration commands
//! - Approval workflow

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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
    OnAlert {
        rule_id: String,
    },
    OnThreshold {
        query: String,
        operator: String,
        value: f64,
    },
    Manual,
}

/// Playbook execution step
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PlaybookStep {
    Log {
        message: String,
    },
    Command {
        cmd: String,
        args: Vec<String>,
        timeout_secs: u64,
        allow_failure: bool,
    },
    SwitchAccount {
        program: String,
        strategy: String,
    },
    Notify {
        channel: String,
        message: String,
    },
    Wait {
        seconds: u64,
    },
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
            Playbook {
                playbook_id: "stuck-agent-restart".to_string(),
                name: "Restart Stuck Agent".to_string(),
                description: "Restart agent that appears stuck (no activity for 10 minutes)"
                    .to_string(),
                trigger: PlaybookTrigger::OnAlert {
                    rule_id: "agent-stuck".to_string(),
                },
                steps: vec![
                    PlaybookStep::Log {
                        message: "Agent appears stuck, attempting restart".to_string(),
                    },
                    PlaybookStep::Command {
                        cmd: "pkill".to_string(),
                        args: vec!["-f".to_string(), "claude-code".to_string()],
                        timeout_secs: 10,
                        allow_failure: true,
                    },
                    PlaybookStep::Wait { seconds: 5 },
                    PlaybookStep::Notify {
                        channel: "tui".to_string(),
                        message: "Stuck agent terminated, ready for restart".to_string(),
                    },
                ],
                requires_approval: true, // Destructive action
                max_runs_per_hour: 2,
                enabled: true,
            },
            Playbook {
                playbook_id: "memory-cleanup".to_string(),
                name: "Memory Pressure Cleanup".to_string(),
                description: "Free memory when usage exceeds critical threshold".to_string(),
                trigger: PlaybookTrigger::OnAlert {
                    rule_id: "memory-critical".to_string(),
                },
                steps: vec![
                    PlaybookStep::Log {
                        message: "Memory critical, initiating cleanup".to_string(),
                    },
                    PlaybookStep::Command {
                        cmd: "sync".to_string(),
                        args: vec![],
                        timeout_secs: 30,
                        allow_failure: true,
                    },
                    PlaybookStep::Command {
                        cmd: "sudo".to_string(),
                        args: vec![
                            "sh".to_string(),
                            "-c".to_string(),
                            "echo 3 > /proc/sys/vm/drop_caches".to_string(),
                        ],
                        timeout_secs: 10,
                        allow_failure: true,
                    },
                    PlaybookStep::Notify {
                        channel: "tui".to_string(),
                        message: "Memory cleanup attempted".to_string(),
                    },
                ],
                requires_approval: true,
                max_runs_per_hour: 1,
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

    /// Find playbooks that trigger on a specific alert
    pub fn playbooks_for_alert(&self, alert_rule_id: &str) -> Vec<&Playbook> {
        self.playbooks
            .iter()
            .filter(|p| {
                p.enabled
                    && matches!(&p.trigger, PlaybookTrigger::OnAlert { rule_id } if rule_id == alert_rule_id)
            })
            .collect()
    }

    /// Get enabled playbooks only
    pub fn enabled_playbooks(&self) -> Vec<&Playbook> {
        self.playbooks.iter().filter(|p| p.enabled).collect()
    }

    /// Check if playbook should be triggered by an alert
    pub fn should_trigger(&self, playbook: &Playbook, alert_rule_id: &str) -> bool {
        if !playbook.enabled {
            return false;
        }
        match &playbook.trigger {
            PlaybookTrigger::OnAlert { rule_id } => rule_id == alert_rule_id,
            _ => false,
        }
    }
}

impl Playbook {
    /// Get total number of steps
    pub fn step_count(&self) -> usize {
        self.steps.len()
    }

    /// Check if this playbook is destructive (requires approval)
    pub fn is_destructive(&self) -> bool {
        self.requires_approval
    }
}

impl PlaybookStep {
    /// Check if this step allows failure
    pub fn allows_failure(&self) -> bool {
        match self {
            PlaybookStep::Command { allow_failure, .. } => *allow_failure,
            _ => false,
        }
    }

    /// Get step type name
    pub fn type_name(&self) -> &'static str {
        match self {
            PlaybookStep::Log { .. } => "log",
            PlaybookStep::Command { .. } => "command",
            PlaybookStep::SwitchAccount { .. } => "switch_account",
            PlaybookStep::Notify { .. } => "notify",
            PlaybookStep::Wait { .. } => "wait",
        }
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
    use proptest::prelude::*;

    // Guardian tests
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

    #[test]
    fn test_get_playbook_not_found() {
        let guardian = Guardian::new();
        let playbook = guardian.get_playbook("nonexistent");
        assert!(playbook.is_none());
    }

    #[test]
    fn test_guardian_default() {
        let guardian = Guardian::default();
        assert!(!guardian.playbooks().is_empty());
    }

    // Playbook tests
    #[test]
    fn test_playbook_creation() {
        let playbook = Playbook {
            playbook_id: "test-playbook".to_string(),
            name: "Test Playbook".to_string(),
            description: "A test playbook".to_string(),
            trigger: PlaybookTrigger::Manual,
            steps: vec![],
            requires_approval: false,
            max_runs_per_hour: 10,
            enabled: true,
        };
        assert_eq!(playbook.playbook_id, "test-playbook");
        assert!(playbook.enabled);
        assert!(playbook.steps.is_empty());
    }

    #[test]
    fn test_playbook_with_steps() {
        let playbook = Playbook {
            playbook_id: "multi-step".to_string(),
            name: "Multi-Step".to_string(),
            description: "Has multiple steps".to_string(),
            trigger: PlaybookTrigger::Manual,
            steps: vec![
                PlaybookStep::Log {
                    message: "Starting".to_string(),
                },
                PlaybookStep::Wait { seconds: 5 },
                PlaybookStep::Log {
                    message: "Done".to_string(),
                },
            ],
            requires_approval: true,
            max_runs_per_hour: 5,
            enabled: true,
        };
        assert_eq!(playbook.steps.len(), 3);
        assert!(playbook.requires_approval);
    }

    #[test]
    fn test_playbook_serialization() {
        let playbook = Playbook {
            playbook_id: "serialize-test".to_string(),
            name: "Serialize Test".to_string(),
            description: "Test serialization".to_string(),
            trigger: PlaybookTrigger::Manual,
            steps: vec![PlaybookStep::Log {
                message: "hello".to_string(),
            }],
            requires_approval: false,
            max_runs_per_hour: 1,
            enabled: true,
        };

        let json = serde_json::to_string(&playbook).unwrap();
        let parsed: Playbook = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.playbook_id, playbook.playbook_id);
        assert_eq!(parsed.steps.len(), 1);
    }

    // PlaybookTrigger tests
    #[test]
    fn test_trigger_manual() {
        let trigger = PlaybookTrigger::Manual;
        let json = serde_json::to_string(&trigger).unwrap();
        assert!(json.contains("manual"));
    }

    #[test]
    fn test_trigger_on_alert() {
        let trigger = PlaybookTrigger::OnAlert {
            rule_id: "test-rule".to_string(),
        };
        let json = serde_json::to_string(&trigger).unwrap();
        assert!(json.contains("on_alert"));
        assert!(json.contains("test-rule"));
    }

    #[test]
    fn test_trigger_on_threshold() {
        let trigger = PlaybookTrigger::OnThreshold {
            query: "SELECT 1".to_string(),
            operator: "gte".to_string(),
            value: 90.0,
        };
        let json = serde_json::to_string(&trigger).unwrap();
        assert!(json.contains("on_threshold"));
        assert!(json.contains("SELECT 1"));
    }

    // PlaybookStep tests
    #[test]
    fn test_step_log() {
        let step = PlaybookStep::Log {
            message: "Test message".to_string(),
        };
        let json = serde_json::to_string(&step).unwrap();
        assert!(json.contains("log"));
        assert!(json.contains("Test message"));
    }

    #[test]
    fn test_step_command() {
        let step = PlaybookStep::Command {
            cmd: "caam".to_string(),
            args: vec![
                "switch".to_string(),
                "--strategy".to_string(),
                "least_used".to_string(),
            ],
            timeout_secs: 30,
            allow_failure: false,
        };
        let json = serde_json::to_string(&step).unwrap();
        assert!(json.contains("command"));
        assert!(json.contains("caam"));
    }

    #[test]
    fn test_step_switch_account() {
        let step = PlaybookStep::SwitchAccount {
            program: "claude-code".to_string(),
            strategy: "round_robin".to_string(),
        };
        let json = serde_json::to_string(&step).unwrap();
        assert!(json.contains("switch_account"));
        assert!(json.contains("claude-code"));
    }

    #[test]
    fn test_step_notify() {
        let step = PlaybookStep::Notify {
            channel: "slack".to_string(),
            message: "Alert triggered".to_string(),
        };
        let json = serde_json::to_string(&step).unwrap();
        assert!(json.contains("notify"));
        assert!(json.contains("slack"));
    }

    #[test]
    fn test_step_wait() {
        let step = PlaybookStep::Wait { seconds: 60 };
        let json = serde_json::to_string(&step).unwrap();
        assert!(json.contains("wait"));
        assert!(json.contains("60"));
    }

    proptest! {
        #[test]
        fn test_trigger_roundtrip(rule_id in "[a-zA-Z0-9_-]{1,32}") {
            let trigger = PlaybookTrigger::OnAlert { rule_id };
            let json = serde_json::to_string(&trigger).unwrap();
            let parsed: PlaybookTrigger = serde_json::from_str(&json).unwrap();

            match parsed {
                PlaybookTrigger::OnAlert { rule_id: parsed_id } => {
                    prop_assert_eq!(parsed_id, match trigger {
                        PlaybookTrigger::OnAlert { rule_id } => rule_id,
                        _ => unreachable!(),
                    });
                }
                _ => prop_assert!(false, "Expected OnAlert variant"),
            }
        }
    }

    proptest! {
        #[test]
        fn test_step_roundtrip(message in ".{1,64}", seconds in 0u64..3600u64) {
            let step = PlaybookStep::Log { message };
            let json = serde_json::to_string(&step).unwrap();
            let parsed: PlaybookStep = serde_json::from_str(&json).unwrap();

            match parsed {
                PlaybookStep::Log { message: parsed_msg } => {
                    prop_assert_eq!(parsed_msg, match step {
                        PlaybookStep::Log { message } => message,
                        _ => unreachable!(),
                    });
                }
                _ => prop_assert!(false, "Expected Log variant"),
            }

            let step = PlaybookStep::Wait { seconds };
            let json = serde_json::to_string(&step).unwrap();
            let parsed: PlaybookStep = serde_json::from_str(&json).unwrap();

            match parsed {
                PlaybookStep::Wait { seconds: parsed_secs } => {
                    prop_assert_eq!(parsed_secs, match step {
                        PlaybookStep::Wait { seconds } => seconds,
                        _ => unreachable!(),
                    });
                }
                _ => prop_assert!(false, "Expected Wait variant"),
            }
        }
    }

    // RunStatus tests
    #[test]
    fn test_run_status_variants() {
        assert_ne!(RunStatus::Running, RunStatus::Success);
        assert_ne!(RunStatus::Failed, RunStatus::Aborted);
        assert_ne!(RunStatus::PendingApproval, RunStatus::Running);
    }

    #[test]
    fn test_run_status_serialization() {
        let statuses = [
            (RunStatus::Running, "running"),
            (RunStatus::Success, "success"),
            (RunStatus::Failed, "failed"),
            (RunStatus::Aborted, "aborted"),
            (RunStatus::PendingApproval, "pendingapproval"),
        ];

        for (status, expected) in statuses {
            let json = serde_json::to_string(&status).unwrap();
            assert!(
                json.to_lowercase().contains(expected),
                "Expected {} in {}",
                expected,
                json
            );
        }
    }

    // PlaybookRun tests
    #[test]
    fn test_playbook_run_creation() {
        let run = PlaybookRun {
            id: 1,
            playbook_id: "test".to_string(),
            started_at: Utc::now(),
            completed_at: None,
            status: RunStatus::Running,
            steps_completed: 0,
            steps_total: 3,
            error_message: None,
        };
        assert_eq!(run.id, 1);
        assert_eq!(run.status, RunStatus::Running);
        assert!(run.completed_at.is_none());
    }

    #[test]
    fn test_playbook_run_completed() {
        let now = Utc::now();
        let run = PlaybookRun {
            id: 2,
            playbook_id: "test".to_string(),
            started_at: now,
            completed_at: Some(now),
            status: RunStatus::Success,
            steps_completed: 3,
            steps_total: 3,
            error_message: None,
        };
        assert!(run.completed_at.is_some());
        assert_eq!(run.steps_completed, run.steps_total);
    }

    #[test]
    fn test_playbook_run_failed() {
        let run = PlaybookRun {
            id: 3,
            playbook_id: "test".to_string(),
            started_at: Utc::now(),
            completed_at: Some(Utc::now()),
            status: RunStatus::Failed,
            steps_completed: 1,
            steps_total: 3,
            error_message: Some("Command timed out".to_string()),
        };
        assert_eq!(run.status, RunStatus::Failed);
        assert!(run.error_message.is_some());
        assert!(run.error_message.unwrap().contains("timed out"));
    }

    #[test]
    fn test_playbook_run_serialization() {
        let run = PlaybookRun {
            id: 4,
            playbook_id: "serialize-test".to_string(),
            started_at: Utc::now(),
            completed_at: None,
            status: RunStatus::PendingApproval,
            steps_completed: 0,
            steps_total: 2,
            error_message: None,
        };

        let json = serde_json::to_string(&run).unwrap();
        let parsed: PlaybookRun = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, run.id);
        assert_eq!(parsed.playbook_id, run.playbook_id);
        assert_eq!(parsed.status, run.status);
    }

    // GuardianError tests
    #[test]
    fn test_error_playbook_not_found() {
        let err = GuardianError::PlaybookNotFound("missing".to_string());
        assert!(err.to_string().contains("Playbook not found"));
        assert!(err.to_string().contains("missing"));
    }

    #[test]
    fn test_error_execution_failed() {
        let err = GuardianError::ExecutionFailed("timeout".to_string());
        assert!(err.to_string().contains("Execution failed"));
    }

    #[test]
    fn test_error_rate_limited() {
        let err = GuardianError::RateLimited(5);
        assert!(err.to_string().contains("Rate limited"));
        assert!(err.to_string().contains("5"));
    }

    #[test]
    fn test_error_approval_required() {
        let err = GuardianError::ApprovalRequired;
        assert!(err.to_string().contains("Approval required"));
    }

    // Additional Guardian tests
    #[test]
    fn test_default_playbooks_count() {
        let guardian = Guardian::new();
        assert_eq!(guardian.playbooks().len(), 3);
    }

    #[test]
    fn test_stuck_agent_playbook() {
        let guardian = Guardian::new();
        let playbook = guardian.get_playbook("stuck-agent-restart");
        assert!(playbook.is_some());
        let playbook = playbook.unwrap();
        assert!(playbook.requires_approval);
        assert_eq!(playbook.max_runs_per_hour, 2);
    }

    #[test]
    fn test_memory_cleanup_playbook() {
        let guardian = Guardian::new();
        let playbook = guardian.get_playbook("memory-cleanup");
        assert!(playbook.is_some());
        let playbook = playbook.unwrap();
        assert!(playbook.requires_approval);
        assert_eq!(playbook.max_runs_per_hour, 1);
    }

    #[test]
    fn test_playbooks_for_alert() {
        let guardian = Guardian::new();
        let playbooks = guardian.playbooks_for_alert("rate-limit-warning");
        assert_eq!(playbooks.len(), 1);
        assert_eq!(playbooks[0].playbook_id, "rate-limit-switch");
    }

    #[test]
    fn test_playbooks_for_alert_not_found() {
        let guardian = Guardian::new();
        let playbooks = guardian.playbooks_for_alert("nonexistent-alert");
        assert!(playbooks.is_empty());
    }

    #[test]
    fn test_enabled_playbooks() {
        let guardian = Guardian::new();
        let enabled = guardian.enabled_playbooks();
        assert_eq!(enabled.len(), 3);
        for p in enabled {
            assert!(p.enabled);
        }
    }

    #[test]
    fn test_should_trigger() {
        let guardian = Guardian::new();
        let playbook = guardian.get_playbook("rate-limit-switch").unwrap();
        assert!(guardian.should_trigger(playbook, "rate-limit-warning"));
        assert!(!guardian.should_trigger(playbook, "other-alert"));
    }

    // Playbook helper tests
    #[test]
    fn test_playbook_step_count() {
        let guardian = Guardian::new();
        let playbook = guardian.get_playbook("rate-limit-switch").unwrap();
        assert_eq!(playbook.step_count(), 3);
    }

    #[test]
    fn test_playbook_is_destructive() {
        let guardian = Guardian::new();

        let rate_limit = guardian.get_playbook("rate-limit-switch").unwrap();
        assert!(!rate_limit.is_destructive());

        let stuck_agent = guardian.get_playbook("stuck-agent-restart").unwrap();
        assert!(stuck_agent.is_destructive());
    }

    // PlaybookStep helper tests
    #[test]
    fn test_step_allows_failure() {
        let step_ok = PlaybookStep::Command {
            cmd: "test".to_string(),
            args: vec![],
            timeout_secs: 10,
            allow_failure: true,
        };
        assert!(step_ok.allows_failure());

        let step_fail = PlaybookStep::Command {
            cmd: "test".to_string(),
            args: vec![],
            timeout_secs: 10,
            allow_failure: false,
        };
        assert!(!step_fail.allows_failure());

        let step_log = PlaybookStep::Log {
            message: "test".to_string(),
        };
        assert!(!step_log.allows_failure());
    }

    #[test]
    fn test_step_type_name() {
        assert_eq!(
            PlaybookStep::Log {
                message: "test".to_string()
            }
            .type_name(),
            "log"
        );
        assert_eq!(
            PlaybookStep::Command {
                cmd: "test".to_string(),
                args: vec![],
                timeout_secs: 10,
                allow_failure: false
            }
            .type_name(),
            "command"
        );
        assert_eq!(
            PlaybookStep::SwitchAccount {
                program: "test".to_string(),
                strategy: "test".to_string()
            }
            .type_name(),
            "switch_account"
        );
        assert_eq!(
            PlaybookStep::Notify {
                channel: "test".to_string(),
                message: "test".to_string()
            }
            .type_name(),
            "notify"
        );
        assert_eq!(PlaybookStep::Wait { seconds: 5 }.type_name(), "wait");
    }
}
