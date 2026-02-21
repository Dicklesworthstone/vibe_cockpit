//! Autopilot mode - autonomous fleet management
//!
//! Coordinates Oracle predictions, Guardian playbooks, and telemetry
//! to make autonomous decisions about account switching, workload
//! balancing, and cost optimization.

use serde::{Deserialize, Serialize};

/// Autopilot operating mode
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AutopilotMode {
    /// Autopilot is disabled
    Off,
    /// Autopilot suggests actions but does not execute them
    Suggest,
    /// Autopilot executes safe, non-destructive actions automatically
    Execute,
}

impl AutopilotMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            AutopilotMode::Off => "off",
            AutopilotMode::Suggest => "suggest",
            AutopilotMode::Execute => "execute",
        }
    }

    pub fn is_active(&self) -> bool {
        !matches!(self, AutopilotMode::Off)
    }

    pub fn can_execute(&self) -> bool {
        matches!(self, AutopilotMode::Execute)
    }
}

impl std::str::FromStr for AutopilotMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "off" => Ok(AutopilotMode::Off),
            "suggest" => Ok(AutopilotMode::Suggest),
            "execute" => Ok(AutopilotMode::Execute),
            other => Err(format!("unknown autopilot mode: {other}")),
        }
    }
}

impl std::fmt::Display for AutopilotMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Type of autopilot decision
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecisionType {
    /// Switch to a different API account
    AccountSwitch,
    /// Balance workload across machines
    WorkloadBalance,
    /// Cost optimization recommendation
    CostOptimization,
    /// Trigger a guardian playbook
    PlaybookTrigger,
}

impl DecisionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            DecisionType::AccountSwitch => "account_switch",
            DecisionType::WorkloadBalance => "workload_balance",
            DecisionType::CostOptimization => "cost_optimization",
            DecisionType::PlaybookTrigger => "playbook_trigger",
        }
    }
}

impl std::str::FromStr for DecisionType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "account_switch" => Ok(DecisionType::AccountSwitch),
            "workload_balance" => Ok(DecisionType::WorkloadBalance),
            "cost_optimization" => Ok(DecisionType::CostOptimization),
            "playbook_trigger" => Ok(DecisionType::PlaybookTrigger),
            other => Err(format!("unknown decision type: {other}")),
        }
    }
}

/// An autopilot decision record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutopilotDecision {
    pub decision_type: DecisionType,
    pub reason: String,
    pub confidence: f64,
    pub executed: bool,
    pub decided_at: String,
    pub details_json: Option<serde_json::Value>,
}

/// Account switch recommendation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchRecommendation {
    pub from_account: String,
    pub to_account: String,
    pub provider: String,
    pub reason: String,
    pub confidence: f64,
    pub current_usage_pct: f64,
    pub target_usage_pct: f64,
}

/// Workload balance action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceAction {
    pub machine_id: String,
    pub action: BalanceActionType,
    pub reason: String,
}

/// Types of balance actions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BalanceActionType {
    ScaleDown,
    ScaleUp,
    Migrate { to_machine: String },
}

/// Cost analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostAnalysis {
    pub daily_cost: f64,
    pub projected_monthly: f64,
    pub budget_remaining: Option<f64>,
    pub recommendations: Vec<CostRecommendation>,
}

/// Cost optimization recommendation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostRecommendation {
    pub title: String,
    pub potential_savings: f64,
    pub severity: String,
}

/// Autopilot status snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutopilotStatus {
    pub mode: AutopilotMode,
    pub decisions_today: u64,
    pub last_decision_at: Option<String>,
    pub account_switches: u64,
    pub cost_alerts: u64,
}

// =============================================================================
// Evaluation Functions (pure logic, no side effects)
// =============================================================================

/// Evaluate whether an account switch is needed based on forecast data.
///
/// Returns a SwitchRecommendation if switching is advisable.
pub fn evaluate_account_switch(
    current_usage_pct: f64,
    velocity_pct_per_min: f64,
    switch_threshold: f64,
    preemptive_mins: u32,
    min_confidence: f64,
    alternative_accounts: &[(String, f64)], // (account_id, usage_pct)
) -> Option<SwitchRecommendation> {
    // Check if we're already above or approaching the threshold
    let minutes_to_threshold = if velocity_pct_per_min > 0.0 {
        let remaining = (switch_threshold * 100.0) - current_usage_pct;
        if remaining <= 0.0 {
            0.0
        } else {
            remaining / velocity_pct_per_min
        }
    } else {
        f64::MAX
    };

    let should_switch = current_usage_pct >= switch_threshold * 100.0
        || (velocity_pct_per_min > 0.0 && minutes_to_threshold <= preemptive_mins as f64);

    if !should_switch {
        return None;
    }

    // Find the best alternative account (lowest usage)
    let best = alternative_accounts
        .iter()
        .filter(|(_, usage)| *usage < switch_threshold * 100.0)
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))?;

    // Calculate confidence based on data quality
    let confidence = if current_usage_pct >= switch_threshold * 100.0 {
        0.95 // High confidence when already above threshold
    } else {
        // Confidence decreases with time distance
        let urgency = 1.0 - (minutes_to_threshold / (preemptive_mins as f64 * 2.0)).min(1.0);
        (0.6 + urgency * 0.35).min(0.99)
    };

    if confidence < min_confidence {
        return None;
    }

    Some(SwitchRecommendation {
        from_account: String::new(), // Filled by caller
        to_account: best.0.clone(),
        provider: String::new(), // Filled by caller
        reason: if current_usage_pct >= switch_threshold * 100.0 {
            format!(
                "Usage at {:.1}% exceeds {:.0}% threshold",
                current_usage_pct,
                switch_threshold * 100.0
            )
        } else {
            format!(
                "Predicted to reach {:.0}% threshold in {:.0} minutes",
                switch_threshold * 100.0,
                minutes_to_threshold
            )
        },
        confidence,
        current_usage_pct,
        target_usage_pct: best.1,
    })
}

/// Evaluate workload balance across machines.
///
/// Returns balance actions for overloaded or underutilized machines.
pub fn evaluate_workload_balance(
    machine_loads: &[(String, f64, usize)], // (machine_id, avg_cpu, agent_count)
    cpu_overload_threshold: f64,
    cpu_underutil_threshold: f64,
) -> Vec<BalanceAction> {
    let mut actions = Vec::new();

    let overloaded: Vec<_> = machine_loads
        .iter()
        .filter(|(_, cpu, _)| *cpu > cpu_overload_threshold)
        .collect();

    let underutilized: Vec<_> = machine_loads
        .iter()
        .filter(|(_, cpu, _)| *cpu < cpu_underutil_threshold)
        .collect();

    let mut underutilized_iter = underutilized.iter().cycle();

    // Suggest scaling down overloaded machines
    for (machine_id, cpu, agent_count) in &overloaded {
        if let Some((target_id, _, _)) = underutilized_iter.next() {
            actions.push(BalanceAction {
                machine_id: machine_id.clone(),
                action: BalanceActionType::Migrate {
                    to_machine: target_id.clone(),
                },
                reason: format!(
                    "CPU at {cpu:.1}% with {agent_count} agents; migrate to {}",
                    target_id
                ),
            });
        } else {
            actions.push(BalanceAction {
                machine_id: machine_id.clone(),
                action: BalanceActionType::ScaleDown,
                reason: format!(
                    "CPU at {cpu:.1}% with {agent_count} agents; no underutilized target available"
                ),
            });
        }
    }

    actions
}

/// Evaluate cost spending against budget.
///
/// Returns a CostAnalysis with recommendations.
pub fn evaluate_costs(
    daily_cost: f64,
    daily_budget: Option<f64>,
    model_costs: &[(String, f64, u64)], // (model, cost, request_count)
) -> CostAnalysis {
    let projected_monthly = daily_cost * 30.0;
    let budget_remaining = daily_budget.map(|b| b - daily_cost);
    let mut recommendations = Vec::new();

    // Check if over budget
    if let Some(budget) = daily_budget {
        if daily_cost > budget {
            recommendations.push(CostRecommendation {
                title: format!(
                    "Daily spend ${:.2} exceeds budget ${:.2}",
                    daily_cost, budget
                ),
                potential_savings: daily_cost - budget,
                severity: "critical".to_string(),
            });
        } else if daily_cost > budget * 0.8 {
            recommendations.push(CostRecommendation {
                title: format!(
                    "Daily spend ${:.2} at {:.0}% of budget",
                    daily_cost,
                    (daily_cost / budget) * 100.0
                ),
                potential_savings: 0.0,
                severity: "warning".to_string(),
            });
        }
    }

    // Check for expensive model usage
    for (model, cost, _count) in model_costs {
        if model.contains("opus") && *cost > 20.0 {
            recommendations.push(CostRecommendation {
                title: format!(
                    "High Opus usage: ${:.2}/day - consider Sonnet for routine tasks",
                    cost
                ),
                potential_savings: cost * 0.5,
                severity: "info".to_string(),
            });
        }
    }

    CostAnalysis {
        daily_cost,
        projected_monthly,
        budget_remaining,
        recommendations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // AutopilotMode Tests
    // =========================================================================

    #[test]
    fn test_mode_as_str() {
        assert_eq!(AutopilotMode::Off.as_str(), "off");
        assert_eq!(AutopilotMode::Suggest.as_str(), "suggest");
        assert_eq!(AutopilotMode::Execute.as_str(), "execute");
    }

    #[test]
    fn test_mode_is_active() {
        assert!(!AutopilotMode::Off.is_active());
        assert!(AutopilotMode::Suggest.is_active());
        assert!(AutopilotMode::Execute.is_active());
    }

    #[test]
    fn test_mode_can_execute() {
        assert!(!AutopilotMode::Off.can_execute());
        assert!(!AutopilotMode::Suggest.can_execute());
        assert!(AutopilotMode::Execute.can_execute());
    }

    #[test]
    fn test_mode_parse() {
        assert_eq!("off".parse::<AutopilotMode>().unwrap(), AutopilotMode::Off);
        assert_eq!(
            "suggest".parse::<AutopilotMode>().unwrap(),
            AutopilotMode::Suggest
        );
        assert_eq!(
            "execute".parse::<AutopilotMode>().unwrap(),
            AutopilotMode::Execute
        );
        assert_eq!(
            "EXECUTE".parse::<AutopilotMode>().unwrap(),
            AutopilotMode::Execute
        );
        assert!("invalid".parse::<AutopilotMode>().is_err());
    }

    #[test]
    fn test_mode_display() {
        assert_eq!(format!("{}", AutopilotMode::Off), "off");
        assert_eq!(format!("{}", AutopilotMode::Suggest), "suggest");
        assert_eq!(format!("{}", AutopilotMode::Execute), "execute");
    }

    #[test]
    fn test_mode_serialization() {
        let json = serde_json::to_string(&AutopilotMode::Execute).unwrap();
        assert_eq!(json, "\"execute\"");
        let parsed: AutopilotMode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, AutopilotMode::Execute);
    }

    // =========================================================================
    // DecisionType Tests
    // =========================================================================

    #[test]
    fn test_decision_type_as_str() {
        assert_eq!(DecisionType::AccountSwitch.as_str(), "account_switch");
        assert_eq!(DecisionType::WorkloadBalance.as_str(), "workload_balance");
        assert_eq!(DecisionType::CostOptimization.as_str(), "cost_optimization");
        assert_eq!(DecisionType::PlaybookTrigger.as_str(), "playbook_trigger");
    }

    #[test]
    fn test_decision_type_parse() {
        assert_eq!(
            "account_switch".parse::<DecisionType>().unwrap(),
            DecisionType::AccountSwitch
        );
        assert_eq!(
            "workload_balance".parse::<DecisionType>().unwrap(),
            DecisionType::WorkloadBalance
        );
        assert!("invalid".parse::<DecisionType>().is_err());
    }

    #[test]
    fn test_decision_type_serialization() {
        let json = serde_json::to_string(&DecisionType::AccountSwitch).unwrap();
        assert_eq!(json, "\"account_switch\"");
        let parsed: DecisionType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, DecisionType::AccountSwitch);
    }

    // =========================================================================
    // Account Switch Evaluation Tests
    // =========================================================================

    #[test]
    fn test_evaluate_no_switch_low_usage() {
        let result = evaluate_account_switch(
            30.0, // current usage 30%
            0.5,  // velocity 0.5% per min
            0.75, // threshold 75%
            15,   // preemptive mins
            0.8,  // min confidence
            &[("alt1".to_string(), 20.0)],
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_evaluate_switch_above_threshold() {
        let result = evaluate_account_switch(
            80.0, // current usage 80% - above 75%
            1.0,
            0.75,
            15,
            0.8,
            &[("alt1".to_string(), 20.0)],
        );
        assert!(result.is_some());
        let rec = result.unwrap();
        assert_eq!(rec.to_account, "alt1");
        assert!(rec.confidence >= 0.8);
        assert!(rec.reason.contains("exceeds"));
    }

    #[test]
    fn test_evaluate_switch_preemptive() {
        // At 60%, gaining 2%/min -> reaches 75% in 7.5 minutes (< 15 min preemptive)
        let result =
            evaluate_account_switch(60.0, 2.0, 0.75, 15, 0.6, &[("alt1".to_string(), 10.0)]);
        assert!(result.is_some());
        let rec = result.unwrap();
        assert!(rec.reason.contains("Predicted"));
    }

    #[test]
    fn test_evaluate_no_switch_zero_velocity() {
        let result = evaluate_account_switch(
            50.0,
            0.0, // no growth
            0.75,
            15,
            0.8,
            &[("alt1".to_string(), 20.0)],
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_evaluate_no_switch_negative_velocity() {
        let result = evaluate_account_switch(
            70.0,
            -0.5, // usage decreasing
            0.75,
            15,
            0.8,
            &[("alt1".to_string(), 20.0)],
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_evaluate_no_switch_no_alternatives() {
        let result = evaluate_account_switch(
            80.0,
            1.0,
            0.75,
            15,
            0.8,
            &[], // no alternative accounts
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_evaluate_no_switch_alternatives_also_high() {
        let result = evaluate_account_switch(
            80.0,
            1.0,
            0.75,
            15,
            0.8,
            &[("alt1".to_string(), 90.0)], // alternative also above threshold
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_evaluate_picks_lowest_usage_alt() {
        let result = evaluate_account_switch(
            80.0,
            1.0,
            0.75,
            15,
            0.8,
            &[
                ("alt1".to_string(), 50.0),
                ("alt2".to_string(), 10.0), // lowest
                ("alt3".to_string(), 30.0),
            ],
        );
        assert!(result.is_some());
        assert_eq!(result.unwrap().to_account, "alt2");
    }

    // =========================================================================
    // Workload Balance Tests
    // =========================================================================

    #[test]
    fn test_evaluate_workload_balanced() {
        let machines = vec![("m1".to_string(), 50.0, 3), ("m2".to_string(), 45.0, 2)];
        let actions = evaluate_workload_balance(&machines, 80.0, 20.0);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_evaluate_workload_overloaded_with_target() {
        let machines = vec![
            ("m1".to_string(), 90.0, 5), // overloaded
            ("m2".to_string(), 15.0, 1), // underutilized
        ];
        let actions = evaluate_workload_balance(&machines, 80.0, 20.0);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].machine_id, "m1");
        assert!(matches!(
            actions[0].action,
            BalanceActionType::Migrate { ref to_machine } if to_machine == "m2"
        ));
    }

    #[test]
    fn test_evaluate_workload_overloaded_no_target() {
        let machines = vec![
            ("m1".to_string(), 90.0, 5), // overloaded
            ("m2".to_string(), 85.0, 4), // also loaded
        ];
        let actions = evaluate_workload_balance(&machines, 80.0, 20.0);
        assert_eq!(actions.len(), 2);
        assert!(matches!(actions[0].action, BalanceActionType::ScaleDown));
    }

    #[test]
    fn test_evaluate_workload_empty() {
        let actions = evaluate_workload_balance(&[], 80.0, 20.0);
        assert!(actions.is_empty());
    }

    // =========================================================================
    // Cost Evaluation Tests
    // =========================================================================

    #[test]
    fn test_evaluate_costs_under_budget() {
        let analysis = evaluate_costs(10.0, Some(50.0), &[]);
        assert_eq!(analysis.daily_cost, 10.0);
        assert_eq!(analysis.projected_monthly, 300.0);
        assert_eq!(analysis.budget_remaining, Some(40.0));
        assert!(analysis.recommendations.is_empty());
    }

    #[test]
    fn test_evaluate_costs_over_budget() {
        let analysis = evaluate_costs(60.0, Some(50.0), &[]);
        assert_eq!(analysis.recommendations.len(), 1);
        assert_eq!(analysis.recommendations[0].severity, "critical");
        assert!(analysis.recommendations[0].title.contains("exceeds"));
    }

    #[test]
    fn test_evaluate_costs_near_budget() {
        let analysis = evaluate_costs(42.0, Some(50.0), &[]); // 84% of budget
        assert_eq!(analysis.recommendations.len(), 1);
        assert_eq!(analysis.recommendations[0].severity, "warning");
    }

    #[test]
    fn test_evaluate_costs_no_budget() {
        let analysis = evaluate_costs(100.0, None, &[]);
        assert!(analysis.budget_remaining.is_none());
        assert!(analysis.recommendations.is_empty());
    }

    #[test]
    fn test_evaluate_costs_high_opus_usage() {
        let model_costs = vec![
            ("claude-opus".to_string(), 25.0, 100),
            ("claude-sonnet".to_string(), 5.0, 200),
        ];
        let analysis = evaluate_costs(30.0, Some(50.0), &model_costs);
        let opus_rec = analysis
            .recommendations
            .iter()
            .find(|r| r.title.contains("Opus"));
        assert!(opus_rec.is_some());
        assert_eq!(opus_rec.unwrap().severity, "info");
    }

    #[test]
    fn test_evaluate_costs_low_opus_usage() {
        let model_costs = vec![("claude-opus".to_string(), 5.0, 10)];
        let analysis = evaluate_costs(5.0, Some(50.0), &model_costs);
        let opus_rec = analysis
            .recommendations
            .iter()
            .find(|r| r.title.contains("Opus"));
        assert!(opus_rec.is_none()); // Below $20 threshold
    }

    // =========================================================================
    // Struct Tests
    // =========================================================================

    #[test]
    fn test_autopilot_decision_serialization() {
        let decision = AutopilotDecision {
            decision_type: DecisionType::AccountSwitch,
            reason: "Rate limit approaching".to_string(),
            confidence: 0.92,
            executed: true,
            decided_at: chrono::Utc::now().to_rfc3339(),
            details_json: Some(serde_json::json!({"from": "acc1", "to": "acc2"})),
        };

        let json = serde_json::to_string(&decision).unwrap();
        let parsed: AutopilotDecision = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.decision_type, DecisionType::AccountSwitch);
        assert!(parsed.executed);
        assert!((parsed.confidence - 0.92).abs() < f64::EPSILON);
    }

    #[test]
    fn test_switch_recommendation_serialization() {
        let rec = SwitchRecommendation {
            from_account: "acc1".to_string(),
            to_account: "acc2".to_string(),
            provider: "claude".to_string(),
            reason: "Usage high".to_string(),
            confidence: 0.85,
            current_usage_pct: 80.0,
            target_usage_pct: 20.0,
        };

        let json = serde_json::to_string(&rec).unwrap();
        let parsed: SwitchRecommendation = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.to_account, "acc2");
    }

    #[test]
    fn test_balance_action_serialization() {
        let action = BalanceAction {
            machine_id: "m1".to_string(),
            action: BalanceActionType::Migrate {
                to_machine: "m2".to_string(),
            },
            reason: "CPU overload".to_string(),
        };

        let json = serde_json::to_string(&action).unwrap();
        let parsed: BalanceAction = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.machine_id, "m1");
    }

    #[test]
    fn test_cost_analysis_serialization() {
        let analysis = CostAnalysis {
            daily_cost: 25.0,
            projected_monthly: 750.0,
            budget_remaining: Some(25.0),
            recommendations: vec![CostRecommendation {
                title: "Near budget".to_string(),
                potential_savings: 0.0,
                severity: "warning".to_string(),
            }],
        };

        let json = serde_json::to_string(&analysis).unwrap();
        let parsed: CostAnalysis = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.daily_cost, 25.0);
        assert_eq!(parsed.recommendations.len(), 1);
    }

    #[test]
    fn test_autopilot_status_serialization() {
        let status = AutopilotStatus {
            mode: AutopilotMode::Suggest,
            decisions_today: 5,
            last_decision_at: Some(chrono::Utc::now().to_rfc3339()),
            account_switches: 2,
            cost_alerts: 1,
        };

        let json = serde_json::to_string(&status).unwrap();
        let parsed: AutopilotStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.mode, AutopilotMode::Suggest);
        assert_eq!(parsed.decisions_today, 5);
    }
}
