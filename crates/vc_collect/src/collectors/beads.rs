//! Beads (bv/br) Collector - Task tracking and productivity metrics
//!
//! This collector captures:
//! - Triage snapshots from `bv --robot-triage`
//! - Issue data from `br list --json`
//! - Graph metrics (pagerank, betweenness, critical path)

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;

use crate::{CollectContext, CollectError, CollectResult, Collector, RowBatch, Warning};

// =============================================================================
// JSON Structures for bv --robot-triage
// =============================================================================

/// Top-level output from `bv --robot-triage`
#[derive(Debug, Deserialize, Serialize)]
pub struct BvTriageOutput {
    pub generated_at: String,
    pub data_hash: String,
    pub triage: TriageData,
    #[serde(default)]
    pub usage_hints: Vec<String>,
}

/// Triage data section
#[derive(Debug, Deserialize, Serialize)]
pub struct TriageData {
    pub meta: TriageMeta,
    pub quick_ref: QuickRef,
    pub recommendations: Vec<Recommendation>,
    #[serde(default)]
    pub quick_wins: Vec<QuickWin>,
    #[serde(default)]
    pub blockers_to_clear: Vec<BlockerToClear>,
    pub project_health: ProjectHealth,
}

/// Triage metadata
#[derive(Debug, Deserialize, Serialize)]
pub struct TriageMeta {
    pub version: String,
    pub generated_at: String,
    #[serde(default)]
    pub phase2_ready: bool,
    pub issue_count: u32,
    #[serde(default)]
    pub compute_time_ms: u64,
}

/// Quick reference summary
#[derive(Debug, Deserialize, Serialize)]
pub struct QuickRef {
    pub open_count: u32,
    pub actionable_count: u32,
    pub blocked_count: u32,
    pub in_progress_count: u32,
    #[serde(default)]
    pub top_picks: Vec<TopPick>,
}

/// A top pick from triage
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TopPick {
    pub id: String,
    pub title: String,
    pub score: f64,
    #[serde(default)]
    pub reasons: Vec<String>,
    #[serde(default)]
    pub unblocks: u32,
}

/// A recommendation from triage
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Recommendation {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub issue_type: Option<String>,
    pub status: Option<String>,
    pub priority: Option<u32>,
    pub score: f64,
    #[serde(default)]
    pub reasons: Vec<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub unblocks_ids: Vec<String>,
    #[serde(default)]
    pub blocked_by: Vec<String>,
}

/// Quick win item
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QuickWin {
    pub id: String,
    pub title: String,
    pub score: f64,
    pub reason: String,
    #[serde(default)]
    pub unblocks_ids: Vec<String>,
}

/// Blocker to clear
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BlockerToClear {
    pub id: String,
    pub title: String,
    pub unblocks_count: u32,
    #[serde(default)]
    pub unblocks_ids: Vec<String>,
    #[serde(default)]
    pub actionable: bool,
    #[serde(default)]
    pub blocked_by: Vec<String>,
}

/// Project health metrics
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProjectHealth {
    pub counts: HealthCounts,
    pub graph: GraphHealth,
    #[serde(default)]
    pub velocity: Option<VelocityMetrics>,
}

/// Issue counts
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HealthCounts {
    pub total: u32,
    pub open: u32,
    pub closed: u32,
    pub blocked: u32,
    pub actionable: u32,
    #[serde(default)]
    pub by_status: HashMap<String, u32>,
    #[serde(default)]
    pub by_type: HashMap<String, u32>,
    #[serde(default)]
    pub by_priority: HashMap<String, u32>,
}

/// Graph health metrics
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GraphHealth {
    pub node_count: u32,
    pub edge_count: u32,
    pub density: f64,
    #[serde(default)]
    pub has_cycles: bool,
    #[serde(default)]
    pub phase2_ready: bool,
}

/// Velocity metrics
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VelocityMetrics {
    pub closed_last_7_days: u32,
    pub closed_last_30_days: u32,
    #[serde(default)]
    pub avg_days_to_close: f64,
    #[serde(default)]
    pub weekly: Vec<WeeklyCount>,
}

/// Weekly closed count
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WeeklyCount {
    pub week_start: String,
    pub closed: u32,
}

// =============================================================================
// JSON Structures for br list --json
// =============================================================================

/// Output from `br list --json`
#[derive(Debug, Deserialize, Serialize)]
pub struct BrListOutput {
    #[serde(default)]
    pub issues: Vec<BeadIssue>,
}

/// A single bead/issue
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BeadIssue {
    pub id: String,
    pub title: String,
    pub status: String,
    #[serde(default)]
    pub priority: Option<u32>,
    #[serde(rename = "type")]
    pub issue_type: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub blocked_by: Vec<String>,
    #[serde(default)]
    pub blocks: Vec<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

// =============================================================================
// BeadsCollector Implementation
// =============================================================================

/// Collector for bv (Beads Viewer) and br (Beads Rust)
///
/// Captures task tracking and productivity metrics from the beads system.
pub struct BeadsCollector;

#[async_trait]
impl Collector for BeadsCollector {
    fn name(&self) -> &'static str {
        "beads"
    }

    fn schema_version(&self) -> u32 {
        1
    }

    fn required_tool(&self) -> Option<&'static str> {
        Some("bv")
    }

    fn supports_incremental(&self) -> bool {
        false // Stateless snapshot
    }

    async fn collect(&self, ctx: &CollectContext) -> Result<CollectResult, CollectError> {
        let start = Instant::now();
        let mut rows = vec![];
        let mut warnings = vec![];

        // Generate a repo ID based on the current working directory
        let repo_id = format!("repo_{:x}", hash_string(&ctx.machine_id));

        // 1. Run bv --robot-triage for comprehensive triage data
        let triage_result = ctx
            .executor
            .run_timeout("bv --robot-triage", ctx.timeout)
            .await;

        match triage_result {
            Ok(output) => match serde_json::from_str::<BvTriageOutput>(&output) {
                Ok(triage) => {
                    // Store triage snapshot
                    let triage_row = serde_json::json!({
                        "machine_id": &ctx.machine_id,
                        "collected_at": ctx.collected_at.to_rfc3339(),
                        "repo_id": &repo_id,
                        "quick_ref_json": serde_json::to_string(&triage.triage.quick_ref).ok(),
                        "recommendations_json": serde_json::to_string(&triage.triage.recommendations).ok(),
                        "project_health_json": serde_json::to_string(&triage.triage.project_health).ok(),
                        "raw_json": &output,
                    });

                    rows.push(RowBatch {
                        table: "beads_triage_snapshots".to_string(),
                        rows: vec![triage_row],
                    });

                    // Store graph metrics
                    let health = &triage.triage.project_health;
                    let graph_row = serde_json::json!({
                        "repo_id": &repo_id,
                        "collected_at": ctx.collected_at.to_rfc3339(),
                        "pagerank_json": null, // Would need separate command
                        "betweenness_json": null,
                        "critical_path_json": null,
                        "node_count": health.graph.node_count,
                        "edge_count": health.graph.edge_count,
                        "density": health.graph.density,
                        "has_cycles": health.graph.has_cycles,
                    });

                    rows.push(RowBatch {
                        table: "beads_graph_metrics".to_string(),
                        rows: vec![graph_row],
                    });
                }
                Err(e) => {
                    warnings.push(Warning::error(format!("Failed to parse bv triage: {e}"))
                        .with_context(output));
                }
            },
            Err(e) => {
                warnings.push(Warning::warn(format!("Failed to run bv --robot-triage: {e}")));
            }
        }

        // 2. Run br list --json for issue details
        let list_result = ctx
            .executor
            .run_timeout("br list --format json", ctx.timeout)
            .await;

        match list_result {
            Ok(output) => match serde_json::from_str::<BrListOutput>(&output) {
                Ok(list) => {
                    let issue_rows: Vec<serde_json::Value> = list
                        .issues
                        .iter()
                        .map(|issue| {
                            serde_json::json!({
                                "repo_id": &repo_id,
                                "issue_id": &issue.id,
                                "status": &issue.status,
                                "priority": issue.priority,
                                "type": &issue.issue_type,
                                "title": &issue.title,
                                "labels_json": serde_json::to_string(&issue.labels).ok(),
                                "deps_json": serde_json::to_string(&issue.blocked_by).ok(),
                                "updated_at": &issue.updated_at,
                                "raw_json": serde_json::to_string(&issue).ok(),
                            })
                        })
                        .collect();

                    if !issue_rows.is_empty() {
                        rows.push(RowBatch {
                            table: "beads_issues".to_string(),
                            rows: issue_rows,
                        });
                    }
                }
                Err(e) => {
                    // br list might not support --format json, try alternative
                    warnings.push(Warning::warn(format!("Failed to parse br list: {e}")));
                }
            },
            Err(e) => {
                warnings.push(Warning::warn(format!("Failed to run br list: {e}")));
            }
        }

        let success = rows.iter().any(|batch| !batch.rows.is_empty());

        Ok(CollectResult {
            rows,
            new_cursor: None,
            raw_artifacts: vec![],
            warnings,
            duration: start.elapsed(),
            success,
            error: if success {
                None
            } else {
                Some("Failed to collect beads data".to_string())
            },
        })
    }
}

/// Simple string hash for generating stable IDs
fn hash_string(s: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_beads_collector_name() {
        let collector = BeadsCollector;
        assert_eq!(collector.name(), "beads");
        assert_eq!(collector.required_tool(), Some("bv"));
        assert!(!collector.supports_incremental());
    }

    #[test]
    fn test_parse_triage_output() {
        let json = r#"{
            "generated_at": "2026-01-28T00:00:00Z",
            "data_hash": "abc123",
            "triage": {
                "meta": {
                    "version": "1.0.0",
                    "generated_at": "2026-01-28T00:00:00Z",
                    "issue_count": 81
                },
                "quick_ref": {
                    "open_count": 64,
                    "actionable_count": 10,
                    "blocked_count": 54,
                    "in_progress_count": 3,
                    "top_picks": []
                },
                "recommendations": [],
                "project_health": {
                    "counts": {
                        "total": 81,
                        "open": 64,
                        "closed": 17,
                        "blocked": 54,
                        "actionable": 10
                    },
                    "graph": {
                        "node_count": 81,
                        "edge_count": 290,
                        "density": 0.045
                    }
                }
            }
        }"#;

        let output: BvTriageOutput = serde_json::from_str(json).unwrap();
        assert_eq!(output.triage.meta.issue_count, 81);
        assert_eq!(output.triage.quick_ref.open_count, 64);
        assert_eq!(output.triage.project_health.graph.node_count, 81);
    }

    #[test]
    fn test_parse_triage_with_recommendations() {
        let json = r#"{
            "generated_at": "2026-01-28T00:00:00Z",
            "data_hash": "abc123",
            "triage": {
                "meta": {"version": "1.0.0", "generated_at": "2026-01-28T00:00:00Z", "issue_count": 10},
                "quick_ref": {"open_count": 5, "actionable_count": 3, "blocked_count": 2, "in_progress_count": 1},
                "recommendations": [
                    {
                        "id": "bd-30z",
                        "title": "Create E2E tests",
                        "score": 0.52,
                        "reasons": ["Unblocks 10 items"],
                        "action": "Continue work"
                    }
                ],
                "project_health": {
                    "counts": {"total": 10, "open": 5, "closed": 5, "blocked": 2, "actionable": 3},
                    "graph": {"node_count": 10, "edge_count": 15, "density": 0.15}
                }
            }
        }"#;

        let output: BvTriageOutput = serde_json::from_str(json).unwrap();
        assert_eq!(output.triage.recommendations.len(), 1);
        assert_eq!(output.triage.recommendations[0].id, "bd-30z");
        assert!((output.triage.recommendations[0].score - 0.52).abs() < 0.01);
    }

    #[test]
    fn test_parse_bead_issue() {
        let json = r#"{
            "issues": [
                {
                    "id": "bd-rf7",
                    "title": "Implement beads collector",
                    "status": "in_progress",
                    "priority": 1,
                    "type": "task",
                    "labels": ["collector"],
                    "blocked_by": ["bd-30z"],
                    "blocks": ["bd-15a"]
                }
            ]
        }"#;

        let output: BrListOutput = serde_json::from_str(json).unwrap();
        assert_eq!(output.issues.len(), 1);
        assert_eq!(output.issues[0].id, "bd-rf7");
        assert_eq!(output.issues[0].status, "in_progress");
        assert_eq!(output.issues[0].blocked_by, vec!["bd-30z"]);
    }

    #[test]
    fn test_parse_minimal_issue() {
        let json = r#"{
            "issues": [
                {
                    "id": "test-1",
                    "title": "Test issue",
                    "status": "open"
                }
            ]
        }"#;

        let output: BrListOutput = serde_json::from_str(json).unwrap();
        assert_eq!(output.issues.len(), 1);
        assert!(output.issues[0].priority.is_none());
        assert!(output.issues[0].labels.is_empty());
    }

    #[test]
    fn test_hash_stability() {
        let hash1 = hash_string("test-repo");
        let hash2 = hash_string("test-repo");
        assert_eq!(hash1, hash2);

        let hash3 = hash_string("other-repo");
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_parse_project_health_with_velocity() {
        let json = r#"{
            "generated_at": "2026-01-28T00:00:00Z",
            "data_hash": "test",
            "triage": {
                "meta": {"version": "1.0.0", "generated_at": "2026-01-28T00:00:00Z", "issue_count": 5},
                "quick_ref": {"open_count": 3, "actionable_count": 2, "blocked_count": 1, "in_progress_count": 1},
                "recommendations": [],
                "project_health": {
                    "counts": {"total": 5, "open": 3, "closed": 2, "blocked": 1, "actionable": 2},
                    "graph": {"node_count": 5, "edge_count": 8, "density": 0.32},
                    "velocity": {
                        "closed_last_7_days": 17,
                        "closed_last_30_days": 17,
                        "avg_days_to_close": 0.47
                    }
                }
            }
        }"#;

        let output: BvTriageOutput = serde_json::from_str(json).unwrap();
        let velocity = output.triage.project_health.velocity.unwrap();
        assert_eq!(velocity.closed_last_7_days, 17);
        assert!((velocity.avg_days_to_close - 0.47).abs() < 0.01);
    }
}
