//! GitHub Collector - Issues, PRs, and repository metadata
//!
//! This collector captures:
//! - Issue and PR counts from `gh repo view --json` and `gh issue list --json`
//! - PR status from `gh pr list --json`
//! - Label breakdown for triage
//! - Correlated with ru repo_id for cross-referencing

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;

use crate::{CollectContext, CollectError, CollectResult, Collector, RowBatch, Warning};

// =============================================================================
// JSON Structures for gh CLI output
// =============================================================================

/// Output from `gh repo view --json`
#[derive(Debug, Deserialize)]
pub struct GhRepoView {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub owner: Option<GhOwner>,
    #[serde(default, rename = "nameWithOwner")]
    pub name_with_owner: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default, rename = "defaultBranchRef")]
    pub default_branch_ref: Option<GhBranchRef>,
    #[serde(default, rename = "isPrivate")]
    pub is_private: bool,
    #[serde(default, rename = "isArchived")]
    pub is_archived: bool,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, rename = "stargazerCount")]
    pub stargazer_count: u32,
    #[serde(default, rename = "forkCount")]
    pub fork_count: u32,
    #[serde(default, rename = "openIssueCount")]
    pub open_issue_count: Option<u32>,
    #[serde(default, rename = "openPullRequestCount")]
    pub open_pull_request_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct GhOwner {
    pub login: String,
}

#[derive(Debug, Deserialize)]
pub struct GhBranchRef {
    pub name: String,
}

/// Single issue from `gh issue list --json`
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GhIssue {
    pub number: u32,
    pub title: String,
    pub state: String,
    #[serde(default)]
    pub labels: Vec<GhLabel>,
    #[serde(default)]
    pub assignees: Vec<GhAssignee>,
    #[serde(default, rename = "createdAt")]
    pub created_at: Option<String>,
    #[serde(default, rename = "updatedAt")]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub author: Option<GhAuthor>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GhLabel {
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GhAssignee {
    pub login: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GhAuthor {
    pub login: String,
}

/// Single PR from `gh pr list --json`
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GhPullRequest {
    pub number: u32,
    pub title: String,
    pub state: String,
    #[serde(default)]
    pub labels: Vec<GhLabel>,
    #[serde(default)]
    pub assignees: Vec<GhAssignee>,
    #[serde(default, rename = "createdAt")]
    pub created_at: Option<String>,
    #[serde(default, rename = "updatedAt")]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub author: Option<GhAuthor>,
    #[serde(default, rename = "isDraft")]
    pub is_draft: bool,
    #[serde(default, rename = "mergeable")]
    pub mergeable: Option<String>,
    #[serde(default, rename = "reviewDecision")]
    pub review_decision: Option<String>,
}

/// Label breakdown for triage
#[derive(Debug, Clone, Serialize)]
pub struct LabelBreakdown {
    pub label: String,
    pub issue_count: u32,
    pub pr_count: u32,
}

/// Triage summary
#[derive(Debug, Clone, Serialize)]
pub struct TriageSummary {
    pub open_issues: u32,
    pub open_prs: u32,
    pub draft_prs: u32,
    pub needs_review: u32,
    pub approved: u32,
    pub changes_requested: u32,
    pub stale_issues_30d: u32,
    pub stale_prs_7d: u32,
}

// =============================================================================
// GhCollector Implementation
// =============================================================================

/// Collector for GitHub issues, PRs, and repository metadata
///
/// Uses the `gh` CLI to fetch data from GitHub API, respecting rate limits.
pub struct GhCollector;

impl GhCollector {
    /// Generate a stable repo ID from the repo URL or path
    fn hash_repo(identifier: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        identifier.hash(&mut hasher);
        format!("repo_{:016x}", hasher.finish())
    }

    /// Count issues by label
    fn count_labels(issues: &[GhIssue], prs: &[GhPullRequest]) -> Vec<LabelBreakdown> {
        let mut label_counts: HashMap<String, (u32, u32)> = HashMap::new();

        for issue in issues {
            for label in &issue.labels {
                let entry = label_counts.entry(label.name.clone()).or_insert((0, 0));
                entry.0 += 1;
            }
        }

        for pr in prs {
            for label in &pr.labels {
                let entry = label_counts.entry(label.name.clone()).or_insert((0, 0));
                entry.1 += 1;
            }
        }

        let mut result: Vec<_> = label_counts
            .into_iter()
            .map(|(label, (issue_count, pr_count))| LabelBreakdown {
                label,
                issue_count,
                pr_count,
            })
            .collect();

        // Sort by total count descending
        result.sort_by(|a, b| {
            (b.issue_count + b.pr_count).cmp(&(a.issue_count + a.pr_count))
        });

        result
    }

    /// Create triage summary
    fn create_triage_summary(issues: &[GhIssue], prs: &[GhPullRequest]) -> TriageSummary {
        let open_issues = issues.iter().filter(|i| i.state == "OPEN").count() as u32;
        let open_prs = prs.iter().filter(|p| p.state == "OPEN").count() as u32;
        let draft_prs = prs.iter().filter(|p| p.is_draft).count() as u32;

        let needs_review = prs
            .iter()
            .filter(|p| p.review_decision.as_deref() == Some("REVIEW_REQUIRED"))
            .count() as u32;

        let approved = prs
            .iter()
            .filter(|p| p.review_decision.as_deref() == Some("APPROVED"))
            .count() as u32;

        let changes_requested = prs
            .iter()
            .filter(|p| p.review_decision.as_deref() == Some("CHANGES_REQUESTED"))
            .count() as u32;

        // Simplified stale detection (would need proper date parsing in production)
        let stale_issues_30d = 0; // TODO: implement date comparison
        let stale_prs_7d = 0; // TODO: implement date comparison

        TriageSummary {
            open_issues,
            open_prs,
            draft_prs,
            needs_review,
            approved,
            changes_requested,
            stale_issues_30d,
            stale_prs_7d,
        }
    }
}

#[async_trait]
impl Collector for GhCollector {
    fn name(&self) -> &'static str {
        "github"
    }

    fn schema_version(&self) -> u32 {
        1
    }

    fn required_tool(&self) -> Option<&'static str> {
        Some("gh")
    }

    fn supports_incremental(&self) -> bool {
        false // Stateless snapshot
    }

    async fn collect(&self, ctx: &CollectContext) -> Result<CollectResult, CollectError> {
        let start = Instant::now();
        let mut rows = vec![];
        let mut warnings = vec![];

        // Try to detect repo from current directory
        let repo_view_result = ctx
            .executor
            .run_timeout(
                "gh repo view --json name,owner,nameWithOwner,url,defaultBranchRef,isPrivate,isArchived,description,stargazerCount,forkCount",
                ctx.timeout,
            )
            .await;

        let repo_id = match &repo_view_result {
            Ok(output) => {
                if let Ok(view) = serde_json::from_str::<GhRepoView>(output) {
                    if let Some(name_with_owner) = &view.name_with_owner {
                        Self::hash_repo(name_with_owner)
                    } else if let Some(url) = &view.url {
                        Self::hash_repo(url)
                    } else {
                        Self::hash_repo(&ctx.machine_id)
                    }
                } else {
                    Self::hash_repo(&ctx.machine_id)
                }
            }
            Err(_) => Self::hash_repo(&ctx.machine_id),
        };

        // Get issues (limit to 100 for performance)
        let issues_result = ctx
            .executor
            .run_timeout(
                "gh issue list --state all --limit 100 --json number,title,state,labels,assignees,createdAt,updatedAt,author",
                ctx.timeout,
            )
            .await;

        let issues: Vec<GhIssue> = match issues_result {
            Ok(output) => match serde_json::from_str(&output) {
                Ok(issues) => issues,
                Err(e) => {
                    warnings.push(Warning::warn(format!("Failed to parse issues: {e}")));
                    vec![]
                }
            },
            Err(e) => {
                warnings.push(Warning::warn(format!("Failed to list issues: {e}")));
                vec![]
            }
        };

        // Get PRs (limit to 100 for performance)
        let prs_result = ctx
            .executor
            .run_timeout(
                "gh pr list --state all --limit 100 --json number,title,state,labels,assignees,createdAt,updatedAt,author,isDraft,mergeable,reviewDecision",
                ctx.timeout,
            )
            .await;

        let prs: Vec<GhPullRequest> = match prs_result {
            Ok(output) => match serde_json::from_str(&output) {
                Ok(prs) => prs,
                Err(e) => {
                    warnings.push(Warning::warn(format!("Failed to parse PRs: {e}")));
                    vec![]
                }
            },
            Err(e) => {
                warnings.push(Warning::warn(format!("Failed to list PRs: {e}")));
                vec![]
            }
        };

        // Create triage summary and label breakdown
        let triage = Self::create_triage_summary(&issues, &prs);
        let labels = Self::count_labels(&issues, &prs);

        // Build snapshot row
        let snapshot_row = serde_json::json!({
            "repo_id": &repo_id,
            "collected_at": ctx.collected_at.to_rfc3339(),
            "open_issues": triage.open_issues,
            "open_prs": triage.open_prs,
            "triage_json": serde_json::to_string(&triage).ok(),
            "label_breakdown_json": serde_json::to_string(&labels).ok(),
            "raw_json": serde_json::json!({
                "issues_count": issues.len(),
                "prs_count": prs.len(),
            }).to_string(),
        });

        rows.push(RowBatch {
            table: "gh_repo_issue_pr_snapshot".to_string(),
            rows: vec![snapshot_row],
        });

        let success = !issues.is_empty() || !prs.is_empty();

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
                Some("No GitHub data collected (repo may not be a git repo or gh not authenticated)".to_string())
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gh_collector_name() {
        let collector = GhCollector;
        assert_eq!(collector.name(), "github");
        assert_eq!(collector.required_tool(), Some("gh"));
        assert!(!collector.supports_incremental());
    }

    #[test]
    fn test_parse_repo_view() {
        let json = r#"{
            "name": "vibe_cockpit",
            "owner": {"login": "Dicklesworthstone"},
            "nameWithOwner": "Dicklesworthstone/vibe_cockpit",
            "url": "https://github.com/Dicklesworthstone/vibe_cockpit",
            "defaultBranchRef": {"name": "main"},
            "isPrivate": false,
            "isArchived": false,
            "description": "AI agent fleet monitoring",
            "stargazerCount": 10,
            "forkCount": 2
        }"#;

        let view: GhRepoView = serde_json::from_str(json).unwrap();
        assert_eq!(view.name, Some("vibe_cockpit".to_string()));
        assert_eq!(view.name_with_owner, Some("Dicklesworthstone/vibe_cockpit".to_string()));
        assert!(!view.is_private);
        assert!(!view.is_archived);
        assert_eq!(view.stargazer_count, 10);
    }

    #[test]
    fn test_parse_issues() {
        let json = r#"[
            {
                "number": 1,
                "title": "Fix bug",
                "state": "OPEN",
                "labels": [{"name": "bug", "color": "d73a4a"}],
                "assignees": [{"login": "user1"}],
                "author": {"login": "author1"}
            },
            {
                "number": 2,
                "title": "Add feature",
                "state": "CLOSED",
                "labels": [{"name": "enhancement"}],
                "assignees": []
            }
        ]"#;

        let issues: Vec<GhIssue> = serde_json::from_str(json).unwrap();
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].number, 1);
        assert_eq!(issues[0].state, "OPEN");
        assert_eq!(issues[0].labels.len(), 1);
        assert_eq!(issues[0].labels[0].name, "bug");
    }

    #[test]
    fn test_parse_prs() {
        let json = r#"[
            {
                "number": 10,
                "title": "Implement feature X",
                "state": "OPEN",
                "labels": [{"name": "feature"}],
                "assignees": [],
                "isDraft": false,
                "mergeable": "MERGEABLE",
                "reviewDecision": "APPROVED"
            },
            {
                "number": 11,
                "title": "WIP: Draft PR",
                "state": "OPEN",
                "labels": [],
                "assignees": [],
                "isDraft": true,
                "reviewDecision": "REVIEW_REQUIRED"
            }
        ]"#;

        let prs: Vec<GhPullRequest> = serde_json::from_str(json).unwrap();
        assert_eq!(prs.len(), 2);
        assert!(!prs[0].is_draft);
        assert_eq!(prs[0].review_decision, Some("APPROVED".to_string()));
        assert!(prs[1].is_draft);
    }

    #[test]
    fn test_triage_summary() {
        let issues = vec![
            GhIssue {
                number: 1,
                title: "Bug".to_string(),
                state: "OPEN".to_string(),
                labels: vec![],
                assignees: vec![],
                created_at: None,
                updated_at: None,
                author: None,
            },
            GhIssue {
                number: 2,
                title: "Closed".to_string(),
                state: "CLOSED".to_string(),
                labels: vec![],
                assignees: vec![],
                created_at: None,
                updated_at: None,
                author: None,
            },
        ];

        let prs = vec![
            GhPullRequest {
                number: 10,
                title: "PR".to_string(),
                state: "OPEN".to_string(),
                labels: vec![],
                assignees: vec![],
                created_at: None,
                updated_at: None,
                author: None,
                is_draft: false,
                mergeable: None,
                review_decision: Some("APPROVED".to_string()),
            },
            GhPullRequest {
                number: 11,
                title: "Draft".to_string(),
                state: "OPEN".to_string(),
                labels: vec![],
                assignees: vec![],
                created_at: None,
                updated_at: None,
                author: None,
                is_draft: true,
                mergeable: None,
                review_decision: Some("REVIEW_REQUIRED".to_string()),
            },
        ];

        let triage = GhCollector::create_triage_summary(&issues, &prs);
        assert_eq!(triage.open_issues, 1);
        assert_eq!(triage.open_prs, 2);
        assert_eq!(triage.draft_prs, 1);
        assert_eq!(triage.approved, 1);
        assert_eq!(triage.needs_review, 1);
    }

    #[test]
    fn test_label_breakdown() {
        let issues = vec![
            GhIssue {
                number: 1,
                title: "Bug".to_string(),
                state: "OPEN".to_string(),
                labels: vec![
                    GhLabel { name: "bug".to_string(), color: None },
                    GhLabel { name: "p1".to_string(), color: None },
                ],
                assignees: vec![],
                created_at: None,
                updated_at: None,
                author: None,
            },
        ];

        let prs = vec![
            GhPullRequest {
                number: 10,
                title: "Fix".to_string(),
                state: "OPEN".to_string(),
                labels: vec![
                    GhLabel { name: "bug".to_string(), color: None },
                ],
                assignees: vec![],
                created_at: None,
                updated_at: None,
                author: None,
                is_draft: false,
                mergeable: None,
                review_decision: None,
            },
        ];

        let breakdown = GhCollector::count_labels(&issues, &prs);

        // bug should have 1 issue + 1 PR = highest
        let bug_label = breakdown.iter().find(|l| l.label == "bug").unwrap();
        assert_eq!(bug_label.issue_count, 1);
        assert_eq!(bug_label.pr_count, 1);

        // p1 should have 1 issue + 0 PR
        let p1_label = breakdown.iter().find(|l| l.label == "p1").unwrap();
        assert_eq!(p1_label.issue_count, 1);
        assert_eq!(p1_label.pr_count, 0);
    }

    #[test]
    fn test_hash_stability() {
        let hash1 = GhCollector::hash_repo("Dicklesworthstone/vibe_cockpit");
        let hash2 = GhCollector::hash_repo("Dicklesworthstone/vibe_cockpit");
        assert_eq!(hash1, hash2);

        let hash3 = GhCollector::hash_repo("other/repo");
        assert_ne!(hash1, hash3);

        assert!(hash1.starts_with("repo_"));
    }
}
