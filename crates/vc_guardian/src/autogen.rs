//! Automatic playbook generation from observed resolution patterns
//!
//! Pipeline:
//! 1. Capture operator actions that resolve alerts
//! 2. Recognize recurring patterns in successful resolutions
//! 3. Generate playbook drafts from patterns
//! 4. Validate drafts for safety
//! 5. Require approval before activation

use crate::{GuardianError, PlaybookStep, PlaybookTrigger};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use vc_store::VcStore;

// ============================================================================
// Data types
// ============================================================================

/// An action captured during manual alert resolution
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CapturedAction {
    Command {
        cmd: String,
        args: Vec<String>,
        success: bool,
    },
    AccountSwitch {
        from: String,
        to: String,
    },
    ProcessKill {
        pid: u32,
        name: String,
    },
    ConfigChange {
        key: String,
        old: String,
        new: String,
    },
    ServiceRestart {
        name: String,
    },
    Custom {
        description: String,
    },
}

/// Resolution outcome
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ResolutionOutcome {
    Success,
    Partial,
    Failed,
    Unknown,
}

impl ResolutionOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Partial => "partial",
            Self::Failed => "failed",
            Self::Unknown => "unknown",
        }
    }
}

/// A recognized pattern across multiple resolutions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolutionPattern {
    pub alert_type: String,
    pub description: String,
    pub common_steps: Vec<PatternStep>,
    pub confidence: f64,
    pub sample_count: usize,
}

/// A step extracted from resolution patterns
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PatternStep {
    Command { cmd: String, args: Vec<String> },
    AccountSwitch { strategy: String },
    ServiceRestart { name: String },
    Wait { seconds: u64 },
    Notify { message: String },
}

/// A generated playbook draft
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookDraft {
    pub draft_id: String,
    pub name: String,
    pub description: String,
    pub alert_type: String,
    pub trigger: PlaybookTrigger,
    pub steps: Vec<PlaybookStep>,
    pub confidence: f64,
    pub sample_count: usize,
    pub status: DraftStatus,
    pub source_pattern: ResolutionPattern,
}

/// Draft lifecycle status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DraftStatus {
    PendingReview,
    Approved,
    Rejected,
    Activated,
}

/// Validation result for a draft
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub issues: Vec<ValidationIssue>,
}

/// Types of validation issues
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ValidationIssue {
    DangerousCommand { cmd: String, reason: String },
    LowConfidence { confidence: f64, threshold: f64 },
    InsufficientSamples { count: usize, minimum: usize },
    EmptySteps,
}

// ============================================================================
// Action capture
// ============================================================================

/// Captures operator actions that resolve alerts
pub struct ActionCapture {
    store: Arc<VcStore>,
}

impl ActionCapture {
    pub fn new(store: Arc<VcStore>) -> Self {
        Self { store }
    }

    /// Record a resolution (actions taken to resolve an alert)
    pub fn capture(
        &self,
        alert_type: &str,
        actions: &[CapturedAction],
        outcome: ResolutionOutcome,
        alert_id: Option<i64>,
        machine_id: Option<&str>,
        operator: Option<&str>,
    ) -> Result<i64, GuardianError> {
        let actions_json = serde_json::to_string(actions)
            .map_err(|e| GuardianError::ExecutionFailed(format!("JSON serialization error: {e}")))?;

        let id = self
            .store
            .insert_resolution(
                alert_type,
                &actions_json,
                outcome.as_str(),
                alert_id,
                machine_id,
                operator,
            )
            .map_err(GuardianError::StoreError)?;

        Ok(id)
    }

    /// Get count of successful resolutions for an alert type
    pub fn success_count(&self, alert_type: &str) -> Result<i64, GuardianError> {
        self.store
            .count_resolutions_by_type(alert_type, "success")
            .map_err(GuardianError::StoreError)
    }
}

// ============================================================================
// Pattern recognition
// ============================================================================

/// Recognizes patterns in successful resolutions
pub struct PatternRecognizer {
    store: Arc<VcStore>,
    min_samples: usize,
}

impl PatternRecognizer {
    pub fn new(store: Arc<VcStore>) -> Self {
        Self {
            store,
            min_samples: 3,
        }
    }

    pub fn with_min_samples(mut self, min: usize) -> Self {
        self.min_samples = min;
        self
    }

    /// Find patterns for a specific alert type
    pub fn find_patterns(&self, alert_type: &str) -> Result<Vec<ResolutionPattern>, GuardianError> {
        let resolutions = self
            .store
            .list_resolutions(Some(alert_type), Some("success"), 50)
            .map_err(GuardianError::StoreError)?;

        if resolutions.len() < self.min_samples {
            return Ok(vec![]);
        }

        // Extract action sequences from resolutions
        let mut action_sequences: Vec<Vec<CapturedAction>> = Vec::new();
        for res in &resolutions {
            let actions_str = res["actions"].as_str().unwrap_or("[]");
            if let Ok(actions) = serde_json::from_str::<Vec<CapturedAction>>(actions_str) {
                if !actions.is_empty() {
                    action_sequences.push(actions);
                }
            }
        }

        if action_sequences.len() < self.min_samples {
            return Ok(vec![]);
        }

        // Find common action types across sequences
        let common_steps = self.extract_common_steps(&action_sequences);
        if common_steps.is_empty() {
            return Ok(vec![]);
        }

        let confidence = action_sequences.len() as f64 / resolutions.len() as f64;

        Ok(vec![ResolutionPattern {
            alert_type: alert_type.to_string(),
            description: format!(
                "Common resolution for {alert_type} ({} samples)",
                action_sequences.len()
            ),
            common_steps,
            confidence: (confidence * 100.0).round() / 100.0,
            sample_count: action_sequences.len(),
        }])
    }

    /// Find patterns across all alert types
    pub fn find_all_patterns(&self) -> Result<Vec<ResolutionPattern>, GuardianError> {
        let alert_types = self
            .store
            .distinct_resolution_alert_types()
            .map_err(GuardianError::StoreError)?;

        let mut all_patterns = Vec::new();
        for alert_type in &alert_types {
            let patterns = self.find_patterns(alert_type)?;
            all_patterns.extend(patterns);
        }
        Ok(all_patterns)
    }

    /// Extract common steps from multiple action sequences
    fn extract_common_steps(&self, sequences: &[Vec<CapturedAction>]) -> Vec<PatternStep> {
        // Count action type occurrences
        let mut command_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut switch_count: usize = 0;
        let mut restart_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        for sequence in sequences {
            let mut seen_cmds = std::collections::HashSet::new();
            let mut seen_restarts = std::collections::HashSet::new();
            let mut has_switch = false;

            for action in sequence {
                match action {
                    CapturedAction::Command { cmd, .. } => {
                        if seen_cmds.insert(cmd.clone()) {
                            *command_counts.entry(cmd.clone()).or_insert(0) += 1;
                        }
                    }
                    CapturedAction::AccountSwitch { .. } => {
                        if !has_switch {
                            switch_count += 1;
                            has_switch = true;
                        }
                    }
                    CapturedAction::ServiceRestart { name } => {
                        if seen_restarts.insert(name.clone()) {
                            *restart_counts.entry(name.clone()).or_insert(0) += 1;
                        }
                    }
                    _ => {}
                }
            }
        }

        let threshold = sequences.len() / 2; // Must appear in >50% of sequences
        let mut steps = Vec::new();

        // Add common commands
        for (cmd, count) in &command_counts {
            if *count > threshold {
                // Find the most common args for this command
                let args = self.most_common_args(sequences, cmd);
                steps.push(PatternStep::Command {
                    cmd: cmd.clone(),
                    args,
                });
            }
        }

        // Add account switch if common
        if switch_count > threshold {
            steps.push(PatternStep::AccountSwitch {
                strategy: "least_used".to_string(),
            });
        }

        // Add service restarts if common
        for (name, count) in &restart_counts {
            if *count > threshold {
                steps.push(PatternStep::ServiceRestart { name: name.clone() });
            }
        }

        steps
    }

    /// Find most common args for a command across sequences
    fn most_common_args(&self, sequences: &[Vec<CapturedAction>], cmd: &str) -> Vec<String> {
        let mut arg_counts: std::collections::HashMap<Vec<String>, usize> =
            std::collections::HashMap::new();

        for sequence in sequences {
            for action in sequence {
                if let CapturedAction::Command {
                    cmd: c, args, ..
                } = action
                {
                    if c == cmd {
                        *arg_counts.entry(args.clone()).or_insert(0) += 1;
                    }
                }
            }
        }

        arg_counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(args, _)| args)
            .unwrap_or_default()
    }
}

// ============================================================================
// Playbook generation
// ============================================================================

/// Generates playbook drafts from recognized patterns
pub struct PlaybookGenerator {
    store: Arc<VcStore>,
    min_confidence: f64,
    min_samples: usize,
}

impl PlaybookGenerator {
    pub fn new(store: Arc<VcStore>) -> Self {
        Self {
            store,
            min_confidence: 0.5,
            min_samples: 3,
        }
    }

    pub fn with_min_confidence(mut self, confidence: f64) -> Self {
        self.min_confidence = confidence;
        self
    }

    pub fn with_min_samples(mut self, min: usize) -> Self {
        self.min_samples = min;
        self
    }

    /// Generate a playbook draft from a pattern
    pub fn generate_from_pattern(&self, pattern: &ResolutionPattern) -> PlaybookDraft {
        let steps = self.pattern_to_playbook_steps(&pattern.common_steps);
        let draft_id = format!(
            "auto-{}-{}",
            pattern.alert_type,
            &uuid::Uuid::new_v4().to_string()[..8]
        );

        PlaybookDraft {
            draft_id,
            name: format!("Auto: {}", pattern.description),
            description: format!(
                "Auto-generated from {} successful resolutions. Confidence: {:.0}%",
                pattern.sample_count,
                pattern.confidence * 100.0
            ),
            alert_type: pattern.alert_type.clone(),
            trigger: PlaybookTrigger::OnAlert {
                rule_id: pattern.alert_type.clone(),
            },
            steps,
            confidence: pattern.confidence,
            sample_count: pattern.sample_count,
            status: DraftStatus::PendingReview,
            source_pattern: pattern.clone(),
        }
    }

    /// Generate playbook drafts for all patterns that meet thresholds
    pub fn generate_all(
        &self,
        patterns: &[ResolutionPattern],
    ) -> Result<Vec<PlaybookDraft>, GuardianError> {
        let mut drafts = Vec::new();

        for pattern in patterns {
            if pattern.confidence < self.min_confidence {
                continue;
            }
            if pattern.sample_count < self.min_samples {
                continue;
            }

            let draft = self.generate_from_pattern(pattern);

            // Store the draft
            let trigger_json = serde_json::to_string(&draft.trigger).unwrap_or_default();
            let steps_json = serde_json::to_string(&draft.steps).unwrap_or_default();
            let pattern_json = serde_json::to_string(&draft.source_pattern).ok();

            self.store
                .insert_playbook_draft(
                    &draft.draft_id,
                    &draft.name,
                    &draft.description,
                    &draft.alert_type,
                    &trigger_json,
                    &steps_json,
                    draft.confidence,
                    draft.sample_count as i32,
                    pattern_json.as_deref(),
                )
                .map_err(GuardianError::StoreError)?;

            drafts.push(draft);
        }

        Ok(drafts)
    }

    /// Convert pattern steps to playbook steps
    fn pattern_to_playbook_steps(&self, pattern_steps: &[PatternStep]) -> Vec<PlaybookStep> {
        let mut steps = vec![PlaybookStep::Log {
            message: "Auto-generated playbook starting".to_string(),
        }];

        for ps in pattern_steps {
            match ps {
                PatternStep::Command { cmd, args } => {
                    steps.push(PlaybookStep::Command {
                        cmd: cmd.clone(),
                        args: args.clone(),
                        timeout_secs: 30,
                        allow_failure: false,
                    });
                }
                PatternStep::AccountSwitch { strategy } => {
                    steps.push(PlaybookStep::SwitchAccount {
                        program: "auto".to_string(),
                        strategy: strategy.clone(),
                    });
                }
                PatternStep::ServiceRestart { name } => {
                    steps.push(PlaybookStep::Command {
                        cmd: "systemctl".to_string(),
                        args: vec!["restart".to_string(), name.clone()],
                        timeout_secs: 60,
                        allow_failure: false,
                    });
                }
                PatternStep::Wait { seconds } => {
                    steps.push(PlaybookStep::Wait { seconds: *seconds });
                }
                PatternStep::Notify { message } => {
                    steps.push(PlaybookStep::Notify {
                        channel: "tui".to_string(),
                        message: message.clone(),
                    });
                }
            }
        }

        steps.push(PlaybookStep::Notify {
            channel: "tui".to_string(),
            message: "Auto-generated playbook completed".to_string(),
        });

        steps
    }
}

// ============================================================================
// Validation
// ============================================================================

/// Dangerous commands that should be flagged
const DANGEROUS_COMMANDS: &[&str] = &[
    "rm", "dd", "mkfs", "fdisk", "shutdown", "reboot", "init", "halt",
];

/// Dangerous command argument patterns
const DANGEROUS_ARGS: &[&str] = &["-rf", "--force", "--no-preserve-root", "format"];

/// Validate a playbook draft for safety
pub fn validate_draft(draft: &PlaybookDraft) -> ValidationResult {
    let mut issues = Vec::new();

    // Check confidence threshold
    if draft.confidence < 0.5 {
        issues.push(ValidationIssue::LowConfidence {
            confidence: draft.confidence,
            threshold: 0.5,
        });
    }

    // Check sample count
    if draft.sample_count < 3 {
        issues.push(ValidationIssue::InsufficientSamples {
            count: draft.sample_count,
            minimum: 3,
        });
    }

    // Check for empty steps (beyond log + notify wrapper)
    if draft.steps.len() <= 2 {
        issues.push(ValidationIssue::EmptySteps);
    }

    // Check for dangerous commands
    for step in &draft.steps {
        if let PlaybookStep::Command { cmd, args, .. } = step {
            if DANGEROUS_COMMANDS.contains(&cmd.as_str()) {
                issues.push(ValidationIssue::DangerousCommand {
                    cmd: cmd.clone(),
                    reason: format!("Command '{cmd}' is potentially destructive"),
                });
            }
            for arg in args {
                if DANGEROUS_ARGS.contains(&arg.as_str()) {
                    issues.push(ValidationIssue::DangerousCommand {
                        cmd: format!("{cmd} {arg}"),
                        reason: format!("Argument '{arg}' is potentially destructive"),
                    });
                }
            }
        }
    }

    ValidationResult {
        valid: issues.is_empty(),
        issues,
    }
}

/// Check if a command is considered dangerous
pub fn is_dangerous_command(cmd: &str, args: &[String]) -> bool {
    if DANGEROUS_COMMANDS.contains(&cmd) {
        return true;
    }
    args.iter().any(|a| DANGEROUS_ARGS.contains(&a.as_str()))
}

// ============================================================================
// Full pipeline
// ============================================================================

/// Run the full auto-generation pipeline
pub fn run_pipeline(
    store: Arc<VcStore>,
    min_samples: usize,
    min_confidence: f64,
) -> Result<Vec<PlaybookDraft>, GuardianError> {
    let recognizer = PatternRecognizer::new(store.clone()).with_min_samples(min_samples);
    let generator = PlaybookGenerator::new(store)
        .with_min_confidence(min_confidence)
        .with_min_samples(min_samples);

    let patterns = recognizer.find_all_patterns()?;
    let drafts = generator.generate_all(&patterns)?;

    Ok(drafts)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> Arc<VcStore> {
        Arc::new(VcStore::open_memory().unwrap())
    }

    // CapturedAction tests
    #[test]
    fn test_captured_action_command_serialization() {
        let action = CapturedAction::Command {
            cmd: "caam".to_string(),
            args: vec!["switch".to_string()],
            success: true,
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("command"));
        assert!(json.contains("caam"));
        let parsed: CapturedAction = serde_json::from_str(&json).unwrap();
        if let CapturedAction::Command { cmd, .. } = parsed {
            assert_eq!(cmd, "caam");
        } else {
            panic!("Expected Command variant");
        }
    }

    #[test]
    fn test_captured_action_account_switch() {
        let action = CapturedAction::AccountSwitch {
            from: "acct1".to_string(),
            to: "acct2".to_string(),
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("account_switch"));
        assert!(json.contains("acct1"));
    }

    #[test]
    fn test_captured_action_service_restart() {
        let action = CapturedAction::ServiceRestart {
            name: "nginx".to_string(),
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("service_restart"));
        assert!(json.contains("nginx"));
    }

    #[test]
    fn test_captured_action_process_kill() {
        let action = CapturedAction::ProcessKill {
            pid: 1234,
            name: "stuck-agent".to_string(),
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("process_kill"));
        assert!(json.contains("1234"));
    }

    #[test]
    fn test_captured_action_config_change() {
        let action = CapturedAction::ConfigChange {
            key: "max_retries".to_string(),
            old: "3".to_string(),
            new: "5".to_string(),
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("config_change"));
        assert!(json.contains("max_retries"));
    }

    #[test]
    fn test_captured_action_custom() {
        let action = CapturedAction::Custom {
            description: "Manual intervention".to_string(),
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("custom"));
    }

    // ResolutionOutcome tests
    #[test]
    fn test_resolution_outcome_as_str() {
        assert_eq!(ResolutionOutcome::Success.as_str(), "success");
        assert_eq!(ResolutionOutcome::Partial.as_str(), "partial");
        assert_eq!(ResolutionOutcome::Failed.as_str(), "failed");
        assert_eq!(ResolutionOutcome::Unknown.as_str(), "unknown");
    }

    #[test]
    fn test_resolution_outcome_serialization() {
        let outcome = ResolutionOutcome::Success;
        let json = serde_json::to_string(&outcome).unwrap();
        let parsed: ResolutionOutcome = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ResolutionOutcome::Success);
    }

    // ActionCapture tests
    #[test]
    fn test_action_capture_record() {
        let store = test_store();
        let capture = ActionCapture::new(store);

        let actions = vec![
            CapturedAction::Command {
                cmd: "caam".to_string(),
                args: vec!["switch".to_string()],
                success: true,
            },
            CapturedAction::ServiceRestart {
                name: "nginx".to_string(),
            },
        ];

        let id = capture
            .capture(
                "rate-limit-warning",
                &actions,
                ResolutionOutcome::Success,
                None,
                Some("orko"),
                Some("operator1"),
            )
            .unwrap();
        assert!(id > 0);
    }

    #[test]
    fn test_action_capture_success_count() {
        let store = test_store();
        let capture = ActionCapture::new(store);

        let actions = vec![CapturedAction::Command {
            cmd: "test".to_string(),
            args: vec![],
            success: true,
        }];

        capture
            .capture("test-alert", &actions, ResolutionOutcome::Success, None, None, None)
            .unwrap();
        capture
            .capture("test-alert", &actions, ResolutionOutcome::Success, None, None, None)
            .unwrap();
        capture
            .capture("test-alert", &actions, ResolutionOutcome::Failed, None, None, None)
            .unwrap();

        let count = capture.success_count("test-alert").unwrap();
        assert_eq!(count, 2);
    }

    // PatternRecognizer tests
    #[test]
    fn test_pattern_recognizer_insufficient_samples() {
        let store = test_store();
        let recognizer = PatternRecognizer::new(store);

        let patterns = recognizer.find_patterns("nonexistent").unwrap();
        assert!(patterns.is_empty());
    }

    #[test]
    fn test_pattern_recognizer_finds_pattern() {
        let store = test_store();
        let capture = ActionCapture::new(store.clone());

        // Create 4 similar resolutions
        for _ in 0..4 {
            let actions = vec![
                CapturedAction::Command {
                    cmd: "caam".to_string(),
                    args: vec!["switch".to_string(), "--strategy".to_string(), "least_used".to_string()],
                    success: true,
                },
                CapturedAction::ServiceRestart {
                    name: "agent-worker".to_string(),
                },
            ];
            capture
                .capture("rate-limit", &actions, ResolutionOutcome::Success, None, None, None)
                .unwrap();
        }

        // Verify data is retrievable
        let resolutions = store.list_resolutions(Some("rate-limit"), Some("success"), 50).unwrap();
        assert_eq!(resolutions.len(), 4, "Expected 4 resolutions, got {}", resolutions.len());

        // Check that actions field is parseable
        let first = &resolutions[0];
        let actions_str = first["actions"].as_str().unwrap_or("[]");
        let parsed: Vec<CapturedAction> = serde_json::from_str(actions_str).unwrap();
        assert!(!parsed.is_empty(), "Parsed actions empty");

        let recognizer = PatternRecognizer::new(store).with_min_samples(3);
        let patterns = recognizer.find_patterns("rate-limit").unwrap();
        assert!(!patterns.is_empty(), "No patterns found");

        let pattern = &patterns[0];
        assert_eq!(pattern.alert_type, "rate-limit");
        assert_eq!(pattern.sample_count, 4);
        assert!(!pattern.common_steps.is_empty());
    }

    #[test]
    fn test_pattern_recognizer_find_all_patterns() {
        let store = test_store();
        let capture = ActionCapture::new(store.clone());

        // Create resolutions for two alert types
        for _ in 0..3 {
            capture
                .capture(
                    "type-a",
                    &[CapturedAction::Command {
                        cmd: "fix-a".to_string(),
                        args: vec![],
                        success: true,
                    }],
                    ResolutionOutcome::Success,
                    None,
                    None,
                    None,
                )
                .unwrap();
        }

        for _ in 0..3 {
            capture
                .capture(
                    "type-b",
                    &[CapturedAction::ServiceRestart {
                        name: "svc-b".to_string(),
                    }],
                    ResolutionOutcome::Success,
                    None,
                    None,
                    None,
                )
                .unwrap();
        }

        let recognizer = PatternRecognizer::new(store).with_min_samples(3);
        let patterns = recognizer.find_all_patterns().unwrap();
        assert_eq!(patterns.len(), 2);
    }

    // PlaybookGenerator tests
    #[test]
    fn test_generate_from_pattern() {
        let store = test_store();
        let generator = PlaybookGenerator::new(store);

        let pattern = ResolutionPattern {
            alert_type: "rate-limit".to_string(),
            description: "Common rate limit fix".to_string(),
            common_steps: vec![
                PatternStep::Command {
                    cmd: "caam".to_string(),
                    args: vec!["switch".to_string()],
                },
                PatternStep::AccountSwitch {
                    strategy: "least_used".to_string(),
                },
            ],
            confidence: 0.85,
            sample_count: 5,
        };

        let draft = generator.generate_from_pattern(&pattern);
        assert!(draft.draft_id.starts_with("auto-rate-limit-"));
        assert!(draft.name.contains("Auto:"));
        assert_eq!(draft.confidence, 0.85);
        // Log + 2 pattern steps + Notify = 4
        assert_eq!(draft.steps.len(), 4);
        assert_eq!(draft.status, DraftStatus::PendingReview);
    }

    #[test]
    fn test_generate_all_filters_low_confidence() {
        let store = test_store();
        let generator = PlaybookGenerator::new(store).with_min_confidence(0.7);

        let patterns = vec![
            ResolutionPattern {
                alert_type: "good".to_string(),
                description: "High confidence".to_string(),
                common_steps: vec![PatternStep::Command {
                    cmd: "fix".to_string(),
                    args: vec![],
                }],
                confidence: 0.9,
                sample_count: 5,
            },
            ResolutionPattern {
                alert_type: "bad".to_string(),
                description: "Low confidence".to_string(),
                common_steps: vec![PatternStep::Command {
                    cmd: "maybe".to_string(),
                    args: vec![],
                }],
                confidence: 0.3,
                sample_count: 5,
            },
        ];

        let drafts = generator.generate_all(&patterns).unwrap();
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].alert_type, "good");
    }

    #[test]
    fn test_generate_all_filters_low_samples() {
        let store = test_store();
        let generator = PlaybookGenerator::new(store).with_min_samples(5);

        let patterns = vec![ResolutionPattern {
            alert_type: "few".to_string(),
            description: "Too few".to_string(),
            common_steps: vec![PatternStep::Command {
                cmd: "fix".to_string(),
                args: vec![],
            }],
            confidence: 0.9,
            sample_count: 2,
        }];

        let drafts = generator.generate_all(&patterns).unwrap();
        assert!(drafts.is_empty());
    }

    // Validation tests
    #[test]
    fn test_validate_safe_draft() {
        let pattern = ResolutionPattern {
            alert_type: "test".to_string(),
            description: "Safe".to_string(),
            common_steps: vec![],
            confidence: 0.8,
            sample_count: 5,
        };

        let draft = PlaybookDraft {
            draft_id: "test-1".to_string(),
            name: "Test".to_string(),
            description: "Safe playbook".to_string(),
            alert_type: "test".to_string(),
            trigger: PlaybookTrigger::OnAlert {
                rule_id: "test".to_string(),
            },
            steps: vec![
                PlaybookStep::Log {
                    message: "start".to_string(),
                },
                PlaybookStep::Command {
                    cmd: "echo".to_string(),
                    args: vec!["hello".to_string()],
                    timeout_secs: 10,
                    allow_failure: false,
                },
                PlaybookStep::Notify {
                    channel: "tui".to_string(),
                    message: "done".to_string(),
                },
            ],
            confidence: 0.8,
            sample_count: 5,
            status: DraftStatus::PendingReview,
            source_pattern: pattern,
        };

        let result = validate_draft(&draft);
        assert!(result.valid);
        assert!(result.issues.is_empty());
    }

    #[test]
    fn test_validate_dangerous_command() {
        let pattern = ResolutionPattern {
            alert_type: "test".to_string(),
            description: "Dangerous".to_string(),
            common_steps: vec![],
            confidence: 0.8,
            sample_count: 5,
        };

        let draft = PlaybookDraft {
            draft_id: "test-2".to_string(),
            name: "Dangerous".to_string(),
            description: "Has rm".to_string(),
            alert_type: "test".to_string(),
            trigger: PlaybookTrigger::Manual,
            steps: vec![
                PlaybookStep::Log {
                    message: "start".to_string(),
                },
                PlaybookStep::Command {
                    cmd: "rm".to_string(),
                    args: vec!["-rf".to_string(), "/tmp/old".to_string()],
                    timeout_secs: 10,
                    allow_failure: false,
                },
                PlaybookStep::Notify {
                    channel: "tui".to_string(),
                    message: "done".to_string(),
                },
            ],
            confidence: 0.8,
            sample_count: 5,
            status: DraftStatus::PendingReview,
            source_pattern: pattern,
        };

        let result = validate_draft(&draft);
        assert!(!result.valid);
        // Should have dangerous command issues (rm and -rf)
        assert!(result.issues.len() >= 1);
    }

    #[test]
    fn test_validate_low_confidence() {
        let pattern = ResolutionPattern {
            alert_type: "test".to_string(),
            description: "Low".to_string(),
            common_steps: vec![],
            confidence: 0.2,
            sample_count: 1,
        };

        let draft = PlaybookDraft {
            draft_id: "test-3".to_string(),
            name: "Low confidence".to_string(),
            description: "Bad".to_string(),
            alert_type: "test".to_string(),
            trigger: PlaybookTrigger::Manual,
            steps: vec![PlaybookStep::Log {
                message: "only one step".to_string(),
            }],
            confidence: 0.2,
            sample_count: 1,
            status: DraftStatus::PendingReview,
            source_pattern: pattern,
        };

        let result = validate_draft(&draft);
        assert!(!result.valid);
        // Low confidence + insufficient samples + empty steps
        assert!(result.issues.len() >= 3);
    }

    #[test]
    fn test_is_dangerous_command() {
        assert!(is_dangerous_command("rm", &[]));
        assert!(is_dangerous_command("dd", &[]));
        assert!(is_dangerous_command(
            "echo",
            &["-rf".to_string()]
        ));
        assert!(!is_dangerous_command("echo", &["hello".to_string()]));
        assert!(!is_dangerous_command("caam", &["switch".to_string()]));
    }

    // DraftStatus tests
    #[test]
    fn test_draft_status_serialization() {
        let statuses = [
            (DraftStatus::PendingReview, "pending_review"),
            (DraftStatus::Approved, "approved"),
            (DraftStatus::Rejected, "rejected"),
            (DraftStatus::Activated, "activated"),
        ];

        for (status, expected) in statuses {
            let json = serde_json::to_string(&status).unwrap();
            assert!(json.contains(expected), "Expected {expected} in {json}");
        }
    }

    // Store integration tests
    #[test]
    fn test_store_insert_resolution() {
        let store = test_store();
        let id = store
            .insert_resolution(
                "test-alert",
                r#"[{"type":"command","cmd":"fix","args":[],"success":true}]"#,
                "success",
                None,
                Some("machine1"),
                Some("op1"),
            )
            .unwrap();
        assert!(id > 0);
    }

    #[test]
    fn test_store_list_resolutions() {
        let store = test_store();
        store
            .insert_resolution("alert-a", "[]", "success", None, None, None)
            .unwrap();
        store
            .insert_resolution("alert-b", "[]", "failed", None, None, None)
            .unwrap();

        let all = store.list_resolutions(None, None, 100).unwrap();
        assert_eq!(all.len(), 2);

        let typed = store.list_resolutions(Some("alert-a"), None, 100).unwrap();
        assert_eq!(typed.len(), 1);

        let success = store.list_resolutions(None, Some("success"), 100).unwrap();
        assert_eq!(success.len(), 1);
    }

    #[test]
    fn test_store_distinct_alert_types() {
        let store = test_store();
        store
            .insert_resolution("type-x", "[]", "success", None, None, None)
            .unwrap();
        store
            .insert_resolution("type-y", "[]", "success", None, None, None)
            .unwrap();
        store
            .insert_resolution("type-x", "[]", "failed", None, None, None)
            .unwrap();

        let types = store.distinct_resolution_alert_types().unwrap();
        assert_eq!(types.len(), 2);
        assert!(types.contains(&"type-x".to_string()));
        assert!(types.contains(&"type-y".to_string()));
    }

    #[test]
    fn test_store_playbook_draft_lifecycle() {
        let store = test_store();

        store
            .insert_playbook_draft(
                "draft-1",
                "Auto: test",
                "Test draft",
                "test-alert",
                r#"{"type":"on_alert","rule_id":"test-alert"}"#,
                r#"[{"type":"log","message":"hello"}]"#,
                0.85,
                5,
                None,
            )
            .unwrap();

        // List drafts
        let drafts = store.list_playbook_drafts(None, 100).unwrap();
        assert_eq!(drafts.len(), 1);

        // Get by ID
        let draft = store.get_playbook_draft("draft-1").unwrap();
        assert!(draft.is_some());
        let draft = draft.unwrap();
        assert_eq!(draft["name"].as_str().unwrap(), "Auto: test");

        // Approve
        let affected = store.approve_playbook_draft("draft-1", "admin").unwrap();
        assert_eq!(affected, 1);

        // Verify status
        let draft = store.get_playbook_draft("draft-1").unwrap().unwrap();
        assert_eq!(draft["status"].as_str().unwrap(), "approved");
    }

    #[test]
    fn test_store_reject_draft() {
        let store = test_store();

        store
            .insert_playbook_draft(
                "draft-rej",
                "Auto: reject test",
                "Will be rejected",
                "test-alert",
                "{}",
                "[]",
                0.5,
                3,
                None,
            )
            .unwrap();

        let affected = store.reject_playbook_draft("draft-rej", Some("too risky")).unwrap();
        assert_eq!(affected, 1);

        let draft = store.get_playbook_draft("draft-rej").unwrap().unwrap();
        assert_eq!(draft["status"].as_str().unwrap(), "rejected");
    }

    #[test]
    fn test_store_activate_from_draft() {
        let store = test_store();

        store
            .insert_playbook_draft(
                "draft-act",
                "Auto: activate test",
                "Will be activated",
                "test-alert",
                r#"{"type":"on_alert","rule_id":"test-alert"}"#,
                r#"[{"type":"log","message":"hello"}]"#,
                0.9,
                10,
                None,
            )
            .unwrap();

        // Must approve first
        store.approve_playbook_draft("draft-act", "admin").unwrap();

        // Now activate
        let result = store.activate_playbook_from_draft("draft-act").unwrap();
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result["status"].as_str().unwrap(), "activated");
    }

    #[test]
    fn test_store_activate_unapproved_fails() {
        let store = test_store();

        store
            .insert_playbook_draft(
                "draft-unapproved",
                "Auto: test",
                "Not approved",
                "test-alert",
                "{}",
                "[]",
                0.5,
                3,
                None,
            )
            .unwrap();

        let result = store.activate_playbook_from_draft("draft-unapproved");
        assert!(result.is_err());
    }

    // Full pipeline test
    #[test]
    fn test_full_pipeline() {
        let store = test_store();
        let capture = ActionCapture::new(store.clone());

        // Seed enough resolutions
        for _ in 0..5 {
            capture
                .capture(
                    "disk-full",
                    &[
                        CapturedAction::Command {
                            cmd: "journalctl".to_string(),
                            args: vec!["--vacuum-size=100M".to_string()],
                            success: true,
                        },
                        CapturedAction::Command {
                            cmd: "docker".to_string(),
                            args: vec!["system".to_string(), "prune".to_string()],
                            success: true,
                        },
                    ],
                    ResolutionOutcome::Success,
                    None,
                    None,
                    None,
                )
                .unwrap();
        }

        let drafts = run_pipeline(store, 3, 0.5).unwrap();
        assert!(!drafts.is_empty());

        let draft = &drafts[0];
        assert_eq!(draft.alert_type, "disk-full");
        assert!(draft.confidence > 0.0);
        assert!(draft.steps.len() > 2); // Log + at least one action + Notify
    }

    // ValidationResult/ValidationIssue serialization
    #[test]
    fn test_validation_result_serialization() {
        let result = ValidationResult {
            valid: false,
            issues: vec![
                ValidationIssue::DangerousCommand {
                    cmd: "rm".to_string(),
                    reason: "destructive".to_string(),
                },
                ValidationIssue::LowConfidence {
                    confidence: 0.3,
                    threshold: 0.5,
                },
            ],
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("dangerous_command"));
        assert!(json.contains("low_confidence"));
    }

    // PatternStep serialization
    #[test]
    fn test_pattern_step_serialization() {
        let steps = vec![
            PatternStep::Command {
                cmd: "fix".to_string(),
                args: vec!["--fast".to_string()],
            },
            PatternStep::AccountSwitch {
                strategy: "round_robin".to_string(),
            },
            PatternStep::ServiceRestart {
                name: "web".to_string(),
            },
            PatternStep::Wait { seconds: 10 },
            PatternStep::Notify {
                message: "done".to_string(),
            },
        ];

        for step in &steps {
            let json = serde_json::to_string(step).unwrap();
            let parsed: PatternStep = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&parsed).unwrap();
            assert_eq!(json, json2);
        }
    }
}
