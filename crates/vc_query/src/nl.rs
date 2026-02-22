//! Natural language query interface
//!
//! Translates plain-English questions into SQL queries against `DuckDB`.
//! Uses rule-based pattern matching (no LLM required).
//!
//! Pipeline:
//! 1. Normalize input (lowercase, strip punctuation)
//! 2. Classify intent (what type of query?)
//! 3. Extract entities (machines, time ranges, metrics)
//! 4. Generate SQL from intent + entities
//! 5. Execute query with guardrails
//! 6. Format results

use crate::{
    QueryError,
    guardrails::{GuardrailConfig, QueryValidator},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use vc_store::VcStore;

// ============================================================================
// Types
// ============================================================================

/// Classified query intent
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryIntent {
    MachineStatus,
    MachineList,
    AlertList,
    AlertCount,
    HealthScore,
    SessionList,
    SessionCount,
    TokenUsage,
    CostSummary,
    PlaybookList,
    IncidentList,
    CollectorStatus,
    AuditLog,
    KnowledgeSearch,
    FleetOverview,
    Unknown,
}

impl QueryIntent {
    #[must_use]
    pub fn description(&self) -> &'static str {
        match self {
            Self::MachineStatus => "Machine status query",
            Self::MachineList => "List machines",
            Self::AlertList => "List alerts",
            Self::AlertCount => "Count alerts",
            Self::HealthScore => "Health score query",
            Self::SessionList => "List sessions",
            Self::SessionCount => "Count sessions",
            Self::TokenUsage => "Token usage query",
            Self::CostSummary => "Cost summary",
            Self::PlaybookList => "List playbooks",
            Self::IncidentList => "List incidents",
            Self::CollectorStatus => "Collector status",
            Self::AuditLog => "Audit log query",
            Self::KnowledgeSearch => "Knowledge base search",
            Self::FleetOverview => "Fleet overview",
            Self::Unknown => "Unknown query type",
        }
    }
}

/// Extracted entities from the query
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueryEntities {
    pub machine: Option<String>,
    pub time_range: Option<TimeRange>,
    pub severity: Option<String>,
    pub limit: Option<usize>,
    pub search_term: Option<String>,
}

/// Time range specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeRange {
    pub interval: String,
    pub sql_expr: String,
}

/// Result of NL query processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NlQueryResult {
    pub original_question: String,
    pub intent: QueryIntent,
    pub entities: QueryEntities,
    pub generated_sql: String,
    pub explanation: String,
    pub results: Vec<serde_json::Value>,
    pub result_count: usize,
}

// ============================================================================
// Intent classification (rule-based)
// ============================================================================

/// Keyword patterns for intent classification
struct IntentPattern {
    intent: QueryIntent,
    keywords: &'static [&'static str],
    boost_keywords: &'static [&'static str],
}

const INTENT_PATTERNS: &[IntentPattern] = &[
    IntentPattern {
        intent: QueryIntent::FleetOverview,
        keywords: &["fleet", "overview", "dashboard", "summary"],
        boost_keywords: &["overall", "general"],
    },
    IntentPattern {
        intent: QueryIntent::MachineStatus,
        keywords: &["machine", "server", "host", "node"],
        boost_keywords: &["status", "state", "how is", "offline", "online"],
    },
    IntentPattern {
        intent: QueryIntent::MachineList,
        keywords: &["machines", "servers", "hosts", "nodes"],
        boost_keywords: &["list", "all", "show"],
    },
    IntentPattern {
        intent: QueryIntent::AlertList,
        keywords: &["alert", "alerts", "alarm", "warning"],
        boost_keywords: &["active", "recent", "list", "show", "unacked", "critical"],
    },
    IntentPattern {
        intent: QueryIntent::AlertCount,
        keywords: &["alert", "alerts", "how many", "count", "number of", "total"],
        boost_keywords: &["how many", "count", "number of", "total"],
    },
    IntentPattern {
        intent: QueryIntent::HealthScore,
        keywords: &["health", "score", "healthy", "by health"],
        boost_keywords: &["worst", "best", "critical", "top", "lowest", "highest"],
    },
    IntentPattern {
        intent: QueryIntent::SessionList,
        keywords: &["session", "sessions", "agent session"],
        boost_keywords: &["recent", "list", "active", "running"],
    },
    IntentPattern {
        intent: QueryIntent::SessionCount,
        keywords: &["session", "sessions"],
        boost_keywords: &["how many", "count", "number of", "total"],
    },
    IntentPattern {
        intent: QueryIntent::TokenUsage,
        keywords: &["token", "tokens", "usage", "consumption"],
        boost_keywords: &["most", "used", "expensive", "cost", "today"],
    },
    IntentPattern {
        intent: QueryIntent::CostSummary,
        keywords: &["cost", "spend", "spending", "bill", "expense"],
        boost_keywords: &["total", "summary", "breakdown", "today", "month"],
    },
    IntentPattern {
        intent: QueryIntent::PlaybookList,
        keywords: &["playbook", "playbooks", "remediation", "guardian"],
        boost_keywords: &["list", "show", "active", "runs"],
    },
    IntentPattern {
        intent: QueryIntent::IncidentList,
        keywords: &["incident", "incidents", "outage"],
        boost_keywords: &["open", "active", "recent", "list"],
    },
    IntentPattern {
        intent: QueryIntent::CollectorStatus,
        keywords: &["collector", "collectors", "collection"],
        boost_keywords: &["status", "healthy", "failing", "error"],
    },
    IntentPattern {
        intent: QueryIntent::AuditLog,
        keywords: &["audit", "log", "event", "history"],
        boost_keywords: &["recent", "who", "when", "action"],
    },
    IntentPattern {
        intent: QueryIntent::KnowledgeSearch,
        keywords: &["knowledge", "solution", "gotcha", "pattern"],
        boost_keywords: &["search", "find", "about", "how to"],
    },
];

/// Classify the intent of a natural language query
#[must_use]
pub fn classify_intent(question: &str) -> QueryIntent {
    let normalized = question.to_lowercase();
    let mut best_intent = QueryIntent::Unknown;
    let mut best_score = 0i32;

    for pattern in INTENT_PATTERNS {
        let mut score = 0i32;

        for kw in pattern.keywords {
            if normalized.contains(kw) {
                score += 2;
            }
        }

        for bkw in pattern.boost_keywords {
            if normalized.contains(bkw) {
                score += 1;
            }
        }

        if score > best_score {
            best_score = score;
            best_intent = pattern.intent;
        }
    }

    best_intent
}

// ============================================================================
// Entity extraction
// ============================================================================

/// Extract entities from a natural language query
#[must_use]
pub fn extract_entities(question: &str) -> QueryEntities {
    let normalized = question.to_lowercase();
    QueryEntities {
        machine: extract_machine_name(&normalized),
        time_range: extract_time_range(&normalized),
        severity: extract_severity(&normalized),
        limit: extract_limit(&normalized),
        search_term: extract_search_term(&normalized),
    }
}

/// Extract time range from query
fn extract_time_range(text: &str) -> Option<TimeRange> {
    let time_patterns: &[(&str, &str, &str)] = &[
        (
            "today",
            "today",
            "captured_at >= CAST(current_date AS TIMESTAMP)",
        ),
        (
            "yesterday",
            "yesterday",
            "captured_at >= CAST(current_date - INTERVAL 1 DAY AS TIMESTAMP) AND captured_at < CAST(current_date AS TIMESTAMP)",
        ),
        (
            "last hour",
            "1 hour",
            "captured_at >= current_timestamp - INTERVAL 1 HOUR",
        ),
        (
            "past hour",
            "1 hour",
            "captured_at >= current_timestamp - INTERVAL 1 HOUR",
        ),
        (
            "last 24 hours",
            "24 hours",
            "captured_at >= current_timestamp - INTERVAL 24 HOUR",
        ),
        (
            "last day",
            "1 day",
            "captured_at >= current_timestamp - INTERVAL 1 DAY",
        ),
        (
            "last week",
            "1 week",
            "captured_at >= current_timestamp - INTERVAL 7 DAY",
        ),
        (
            "past week",
            "1 week",
            "captured_at >= current_timestamp - INTERVAL 7 DAY",
        ),
        (
            "this week",
            "this week",
            "captured_at >= date_trunc('week', current_date)",
        ),
        (
            "last month",
            "1 month",
            "captured_at >= current_timestamp - INTERVAL 30 DAY",
        ),
        (
            "this month",
            "this month",
            "captured_at >= date_trunc('month', current_date)",
        ),
    ];

    for (pattern, interval, sql) in time_patterns {
        if text.contains(pattern) {
            return Some(TimeRange {
                interval: interval.to_string(),
                sql_expr: sql.to_string(),
            });
        }
    }
    None
}

/// Extract severity from query
fn extract_severity(text: &str) -> Option<String> {
    if text.contains("critical") {
        Some("critical".to_string())
    } else if text.contains("warning") || text.contains("warn") {
        Some("warning".to_string())
    } else if text.contains("info") {
        Some("info".to_string())
    } else {
        None
    }
}

/// Extract limit from query (e.g., "top 5", "last 10")
fn extract_limit(text: &str) -> Option<usize> {
    let limit_patterns = ["top ", "last ", "first ", "limit "];

    for prefix in limit_patterns {
        if let Some(pos) = text.find(prefix) {
            let after = &text[pos + prefix.len()..];
            let num_str: String = after.chars().take_while(char::is_ascii_digit).collect();
            if let Ok(n) = num_str.parse::<usize>()
                && n > 0
                && n <= 1000
            {
                return Some(n);
            }
        }
    }
    None
}

/// Extract machine name from query
fn extract_machine_name(text: &str) -> Option<String> {
    let machine_prefixes = ["machine ", "on ", "for ", "server ", "host "];

    for prefix in machine_prefixes {
        if let Some(pos) = text.find(prefix) {
            let after = &text[pos + prefix.len()..];
            let name: String = after
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                .collect();
            if !name.is_empty() && name.len() > 1 {
                // Filter out common words that aren't machine names
                let common_words = [
                    "the", "is", "are", "was", "any", "all", "most", "least", "it", "my", "our",
                    "this", "that", "has", "have", "not",
                ];
                if !common_words.contains(&name.as_str()) {
                    return Some(name);
                }
            }
        }
    }
    None
}

/// Extract search term for knowledge queries
fn extract_search_term(text: &str) -> Option<String> {
    let search_prefixes = ["about ", "for ", "search ", "find "];

    for prefix in search_prefixes {
        if let Some(pos) = text.find(prefix) {
            let after = &text[pos + prefix.len()..];
            let term: String = after
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_')
                .collect();
            let trimmed = term.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }
    None
}

// ============================================================================
// SQL generation
// ============================================================================

/// Generate SQL from intent and entities
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn generate_sql(intent: QueryIntent, entities: &QueryEntities) -> String {
    let limit = entities.limit.unwrap_or(50).min(1000);

    match intent {
        QueryIntent::FleetOverview => {
            "SELECT \
             (SELECT COUNT(*) FROM machines) AS total_machines, \
             (SELECT COUNT(*) FROM machines WHERE enabled = true) AS online_machines, \
             (SELECT COUNT(*) FROM alert_history WHERE acknowledged = false) AS active_alerts, \
             (SELECT COUNT(*) FROM incidents WHERE status = 'open') AS open_incidents"
                .to_string()
        }
        QueryIntent::MachineStatus => {
            if let Some(machine) = &entities.machine {
                format!(
                    "SELECT * FROM machines WHERE machine_id = '{m}' OR hostname = '{m}'",
                    m = vc_store::escape_sql_literal(machine)
                )
            } else {
                format!("SELECT * FROM machines ORDER BY hostname LIMIT {limit}")
            }
        }
        QueryIntent::MachineList => {
            format!("SELECT machine_id, hostname, enabled, ssh_host, tags FROM machines ORDER BY hostname LIMIT {limit}")
        }
        QueryIntent::AlertList => {
            let mut conditions = Vec::new();
            if let Some(sev) = &entities.severity {
                conditions.push(format!(
                    "severity = '{}'",
                    vc_store::escape_sql_literal(sev)
                ));
            }
            if let Some(tr) = &entities.time_range {
                conditions.push(tr.sql_expr.replace("captured_at", "fired_at"));
            }

            let where_clause = if conditions.is_empty() {
                String::new()
            } else {
                format!(" WHERE {}", conditions.join(" AND "))
            };

            format!(
                "SELECT id, rule_id, severity, title, fired_at FROM alert_history{where_clause} ORDER BY fired_at DESC LIMIT {limit}"
            )
        }
        QueryIntent::AlertCount => {
            let mut conditions = Vec::new();
            if let Some(sev) = &entities.severity {
                conditions.push(format!(
                    "severity = '{}'",
                    vc_store::escape_sql_literal(sev)
                ));
            }
            if let Some(tr) = &entities.time_range {
                conditions.push(tr.sql_expr.replace("captured_at", "fired_at"));
            }

            let where_clause = if conditions.is_empty() {
                String::new()
            } else {
                format!(" WHERE {}", conditions.join(" AND "))
            };

            format!("SELECT COUNT(*) AS alert_count FROM alert_history{where_clause}")
        }
        QueryIntent::HealthScore => {
            if let Some(machine) = &entities.machine {
                format!(
                    "SELECT machine_id, overall_score, worst_factor_id, factor_count, \
                     critical_count, warning_count, CAST(collected_at AS TEXT) AS collected_at \
                     FROM health_summary WHERE machine_id = '{}' \
                     ORDER BY collected_at DESC LIMIT 1",
                    vc_store::escape_sql_literal(machine)
                )
            } else {
                format!(
                    "SELECT machine_id, overall_score, worst_factor_id, \
                     critical_count, warning_count \
                     FROM health_summary \
                     WHERE collected_at = (SELECT MAX(collected_at) FROM health_summary hs2 \
                                           WHERE hs2.machine_id = health_summary.machine_id) \
                     ORDER BY overall_score ASC LIMIT {limit}"
                )
            }
        }
        QueryIntent::SessionList => {
            let mut conditions = Vec::new();
            if let Some(machine) = &entities.machine {
                conditions.push(format!(
                    "machine_id = '{}'",
                    vc_store::escape_sql_literal(machine)
                ));
            }
            if let Some(tr) = &entities.time_range {
                conditions.push(tr.sql_expr.replace("captured_at", "started_at"));
            }

            let where_clause = if conditions.is_empty() {
                String::new()
            } else {
                format!(" WHERE {}", conditions.join(" AND "))
            };

            format!(
                "SELECT session_id, machine_id, program, model, \
                 CAST(started_at AS TEXT) AS started_at, \
                 CAST(ended_at AS TEXT) AS ended_at, \
                 token_count \
                 FROM agent_sessions{where_clause} ORDER BY started_at DESC LIMIT {limit}"
            )
        }
        QueryIntent::SessionCount => {
            let mut conditions = Vec::new();
            if let Some(machine) = &entities.machine {
                conditions.push(format!(
                    "machine_id = '{}'",
                    vc_store::escape_sql_literal(machine)
                ));
            }
            if let Some(tr) = &entities.time_range {
                conditions.push(tr.sql_expr.replace("captured_at", "started_at"));
            }

            let where_clause = if conditions.is_empty() {
                String::new()
            } else {
                format!(" WHERE {}", conditions.join(" AND "))
            };

            format!("SELECT COUNT(*) AS session_count FROM agent_sessions{where_clause}")
        }
        QueryIntent::TokenUsage => {
            let mut conditions = Vec::new();
            if let Some(machine) = &entities.machine {
                conditions.push(format!(
                    "machine_id = '{}'",
                    vc_store::escape_sql_literal(machine)
                ));
            }
            if let Some(tr) = &entities.time_range {
                conditions.push(tr.sql_expr.replace("captured_at", "started_at"));
            }

            let where_clause = if conditions.is_empty() {
                String::new()
            } else {
                format!(" WHERE {}", conditions.join(" AND "))
            };

            format!(
                "SELECT machine_id, model, \
                 SUM(token_count) AS total_tokens, \
                 COUNT(*) AS session_count \
                 FROM agent_sessions{where_clause} \
                 GROUP BY machine_id, model \
                 ORDER BY total_tokens DESC LIMIT {limit}"
            )
        }
        QueryIntent::CostSummary => {
            let mut conditions = Vec::new();
            if let Some(tr) = &entities.time_range {
                conditions.push(tr.sql_expr.replace("captured_at", "ts"));
            }

            let where_clause = if conditions.is_empty() {
                String::new()
            } else {
                format!(" WHERE {}", conditions.join(" AND "))
            };

            format!(
                "SELECT machine_id, provider, \
                 SUM(cost_estimate) AS total_cost, \
                 COUNT(*) AS entries \
                 FROM account_usage_snapshots{where_clause} \
                 GROUP BY machine_id, provider \
                 ORDER BY total_cost DESC LIMIT {limit}"
            )
        }
        QueryIntent::PlaybookList => {
            format!(
                "SELECT playbook_id, name, description, enabled, requires_approval \
                 FROM guardian_playbooks ORDER BY created_at DESC LIMIT {limit}"
            )
        }
        QueryIntent::IncidentList => {
            let status_filter = if entities
                .search_term
                .as_ref()
                .is_some_and(|t| t.contains("closed"))
            {
                "status = 'closed'"
            } else {
                "status = 'open'"
            };

            format!(
                "SELECT incident_id, title, severity, status, \
                 CAST(started_at AS TEXT) AS started_at \
                 FROM incidents WHERE {status_filter} ORDER BY started_at DESC LIMIT {limit}"
            )
        }
        QueryIntent::CollectorStatus => {
            format!(
                "SELECT collector, machine_id, success, \
                 CAST(collected_at AS TEXT) AS collected_at, \
                 rows_inserted, error_class \
                 FROM collector_health ORDER BY collected_at DESC LIMIT {limit}"
            )
        }
        QueryIntent::AuditLog => {
            let mut conditions = Vec::new();
            if let Some(machine) = &entities.machine {
                conditions.push(format!(
                    "machine_id = '{}'",
                    vc_store::escape_sql_literal(machine)
                ));
            }
            if let Some(tr) = &entities.time_range {
                conditions.push(tr.sql_expr.replace("captured_at", "ts"));
            }

            let where_clause = if conditions.is_empty() {
                String::new()
            } else {
                format!(" WHERE {}", conditions.join(" AND "))
            };

            format!(
                "SELECT id, CAST(ts AS TEXT) AS ts, event_type, actor, action, result \
                 FROM audit_events{where_clause} ORDER BY ts DESC LIMIT {limit}"
            )
        }
        QueryIntent::KnowledgeSearch => {
            let search = entities.search_term.as_deref().unwrap_or("");
            if search.is_empty() {
                format!(
                    "SELECT id, entry_type, title, summary, quality_score \
                     FROM knowledge_entries ORDER BY quality_score DESC LIMIT {limit}"
                )
            } else {
                format!(
                    "SELECT id, entry_type, title, summary, quality_score \
                     FROM knowledge_entries \
                     WHERE title ILIKE '%{s}%' OR content ILIKE '%{s}%' OR summary ILIKE '%{s}%' \
                     ORDER BY quality_score DESC LIMIT {limit}",
                    s = vc_store::escape_sql_literal(search)
                )
            }
        }
        QueryIntent::Unknown => {
            "SELECT 'I could not understand your question. Try asking about machines, alerts, sessions, costs, or health scores.' AS message"
                .to_string()
        }
    }
}

/// Generate a human-readable explanation of what the query does
#[must_use]
pub fn explain_query(intent: QueryIntent, entities: &QueryEntities) -> String {
    let mut parts = vec![intent.description().to_string()];

    if let Some(machine) = &entities.machine {
        parts.push(format!("for machine '{machine}'"));
    }

    if let Some(tr) = &entities.time_range {
        parts.push(format!("(time range: {})", tr.interval));
    }

    if let Some(sev) = &entities.severity {
        parts.push(format!("with severity '{sev}'"));
    }

    if let Some(limit) = entities.limit {
        parts.push(format!("(limit: {limit})"));
    }

    parts.join(" ")
}

// ============================================================================
// NL Query Engine
// ============================================================================

/// Natural language query engine
pub struct NlEngine {
    store: Arc<VcStore>,
    validator: QueryValidator,
}

impl NlEngine {
    #[must_use]
    pub fn new(store: Arc<VcStore>) -> Self {
        Self {
            store,
            validator: QueryValidator::new(GuardrailConfig::default()),
        }
    }

    /// Process a natural language question and return results
    ///
    /// # Errors
    ///
    /// Returns [`QueryError`] when query safety checks fail.
    pub fn ask(&self, question: &str) -> Result<NlQueryResult, QueryError> {
        let intent = classify_intent(question);
        let entities = extract_entities(question);
        let sql = generate_sql(intent, &entities);
        let explanation = explain_query(intent, &entities);

        // Validate query safety
        if let Err(e) = self.validator.validate_raw(&sql) {
            return Err(QueryError::InvalidQuery(format!(
                "Query safety check failed: {e}"
            )));
        }

        // Execute query
        let results = self.store.query_json(&sql).unwrap_or_default();
        let result_count = results.len();

        Ok(NlQueryResult {
            original_question: question.to_string(),
            intent,
            entities,
            generated_sql: sql,
            explanation,
            results,
            result_count,
        })
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Intent classification tests
    #[test]
    fn test_classify_machine_status() {
        assert_eq!(
            classify_intent("How is machine orko doing?"),
            QueryIntent::MachineStatus
        );
        assert_eq!(
            classify_intent("Is the server online?"),
            QueryIntent::MachineStatus
        );
    }

    #[test]
    fn test_classify_machine_list() {
        assert_eq!(
            classify_intent("Show all machines"),
            QueryIntent::MachineList
        );
        assert_eq!(classify_intent("List my servers"), QueryIntent::MachineList);
    }

    #[test]
    fn test_classify_alert_list() {
        assert_eq!(
            classify_intent("Show me active alerts"),
            QueryIntent::AlertList
        );
        assert_eq!(
            classify_intent("What alerts are firing?"),
            QueryIntent::AlertList
        );
    }

    #[test]
    fn test_classify_alert_count() {
        assert_eq!(
            classify_intent("How many alerts are there?"),
            QueryIntent::AlertCount
        );
        assert_eq!(
            classify_intent("Count of critical alerts"),
            QueryIntent::AlertCount
        );
    }

    #[test]
    fn test_classify_health() {
        assert_eq!(
            classify_intent("What is the health score?"),
            QueryIntent::HealthScore
        );
        assert_eq!(
            classify_intent("Which machine is the worst health?"),
            QueryIntent::HealthScore
        );
    }

    #[test]
    fn test_classify_sessions() {
        assert_eq!(
            classify_intent("List recent sessions"),
            QueryIntent::SessionList
        );
        assert_eq!(
            classify_intent("How many sessions today?"),
            QueryIntent::SessionCount
        );
    }

    #[test]
    fn test_classify_tokens() {
        assert_eq!(
            classify_intent("Which agent used the most tokens?"),
            QueryIntent::TokenUsage
        );
        assert_eq!(
            classify_intent("Show token usage today"),
            QueryIntent::TokenUsage
        );
    }

    #[test]
    fn test_classify_cost() {
        assert_eq!(
            classify_intent("What is the total cost this month?"),
            QueryIntent::CostSummary
        );
        assert_eq!(
            classify_intent("Show spending breakdown"),
            QueryIntent::CostSummary
        );
    }

    #[test]
    fn test_classify_playbooks() {
        assert_eq!(
            classify_intent("List guardian playbooks"),
            QueryIntent::PlaybookList
        );
    }

    #[test]
    fn test_classify_incidents() {
        assert_eq!(
            classify_intent("Show open incidents"),
            QueryIntent::IncidentList
        );
    }

    #[test]
    fn test_classify_collectors() {
        assert_eq!(
            classify_intent("What is the collector status?"),
            QueryIntent::CollectorStatus
        );
    }

    #[test]
    fn test_classify_audit() {
        assert_eq!(
            classify_intent("Show recent audit events"),
            QueryIntent::AuditLog
        );
    }

    #[test]
    fn test_classify_knowledge() {
        assert_eq!(
            classify_intent("Search knowledge base for rate limits"),
            QueryIntent::KnowledgeSearch
        );
    }

    #[test]
    fn test_classify_fleet() {
        assert_eq!(
            classify_intent("Give me a fleet overview"),
            QueryIntent::FleetOverview
        );
    }

    #[test]
    fn test_classify_unknown() {
        assert_eq!(
            classify_intent("What is the meaning of life?"),
            QueryIntent::Unknown
        );
    }

    // Entity extraction tests
    #[test]
    fn test_extract_time_range_today() {
        let entities = extract_entities("Show alerts from today");
        assert!(entities.time_range.is_some());
        assert_eq!(entities.time_range.unwrap().interval, "today");
    }

    #[test]
    fn test_extract_time_range_last_hour() {
        let entities = extract_entities("What happened in the last hour?");
        assert!(entities.time_range.is_some());
        assert_eq!(entities.time_range.unwrap().interval, "1 hour");
    }

    #[test]
    fn test_extract_time_range_last_week() {
        let entities = extract_entities("Sessions from last week");
        assert!(entities.time_range.is_some());
        assert_eq!(entities.time_range.unwrap().interval, "1 week");
    }

    #[test]
    fn test_extract_time_range_none() {
        let entities = extract_entities("Show all machines");
        assert!(entities.time_range.is_none());
    }

    #[test]
    fn test_extract_severity() {
        let entities = extract_entities("Show critical alerts");
        assert_eq!(entities.severity, Some("critical".to_string()));

        let entities = extract_entities("Warning level alerts");
        assert_eq!(entities.severity, Some("warning".to_string()));

        let entities = extract_entities("Show all alerts");
        assert!(entities.severity.is_none());
    }

    #[test]
    fn test_extract_limit() {
        let entities = extract_entities("Show top 5 machines");
        assert_eq!(entities.limit, Some(5));

        let entities = extract_entities("List last 10 alerts");
        assert_eq!(entities.limit, Some(10));

        let entities = extract_entities("Show machines");
        assert!(entities.limit.is_none());
    }

    #[test]
    fn test_extract_machine_name() {
        let entities = extract_entities("Health of machine orko");
        assert_eq!(entities.machine, Some("orko".to_string()));

        let entities = extract_entities("Status on skeletor");
        assert_eq!(entities.machine, Some("skeletor".to_string()));
    }

    #[test]
    fn test_extract_machine_filters_common_words() {
        let entities = extract_entities("Health for the fleet");
        // "the" should be filtered out
        assert!(entities.machine.is_none());
    }

    #[test]
    fn test_extract_search_term() {
        let entities = extract_entities("Search knowledge about rate limits");
        assert_eq!(entities.search_term, Some("rate limits".to_string()));

        let entities = extract_entities("Find solutions for ssh errors");
        assert_eq!(entities.search_term, Some("ssh errors".to_string()));
    }

    // SQL generation tests
    #[test]
    fn test_generate_sql_fleet_overview() {
        let sql = generate_sql(QueryIntent::FleetOverview, &QueryEntities::default());
        assert!(sql.contains("SELECT"));
        assert!(sql.contains("total_machines"));
    }

    #[test]
    fn test_generate_sql_machine_status_specific() {
        let entities = QueryEntities {
            machine: Some("orko".to_string()),
            ..Default::default()
        };
        let sql = generate_sql(QueryIntent::MachineStatus, &entities);
        assert!(sql.contains("orko"));
        assert!(sql.contains("machines"));
    }

    #[test]
    fn test_generate_sql_alerts_with_severity() {
        let entities = QueryEntities {
            severity: Some("critical".to_string()),
            ..Default::default()
        };
        let sql = generate_sql(QueryIntent::AlertList, &entities);
        assert!(sql.contains("severity = 'critical'"));
    }

    #[test]
    fn test_generate_sql_alerts_with_time_range() {
        let entities = QueryEntities {
            time_range: Some(TimeRange {
                interval: "1 hour".to_string(),
                sql_expr: "captured_at >= current_timestamp - INTERVAL 1 HOUR".to_string(),
            }),
            ..Default::default()
        };
        let sql = generate_sql(QueryIntent::AlertList, &entities);
        assert!(sql.contains("INTERVAL 1 HOUR"));
    }

    #[test]
    fn test_generate_sql_token_usage() {
        let sql = generate_sql(QueryIntent::TokenUsage, &QueryEntities::default());
        assert!(sql.contains("SUM(token_count)"));
        assert!(sql.contains("GROUP BY"));
    }

    #[test]
    fn test_generate_sql_knowledge_search() {
        let entities = QueryEntities {
            search_term: Some("ssh errors".to_string()),
            ..Default::default()
        };
        let sql = generate_sql(QueryIntent::KnowledgeSearch, &entities);
        assert!(sql.contains("ILIKE"));
        assert!(sql.contains("ssh errors"));
    }

    #[test]
    fn test_generate_sql_unknown() {
        let sql = generate_sql(QueryIntent::Unknown, &QueryEntities::default());
        assert!(sql.contains("could not understand"));
    }

    #[test]
    fn test_generate_sql_respects_limit() {
        let entities = QueryEntities {
            limit: Some(10),
            ..Default::default()
        };
        let sql = generate_sql(QueryIntent::MachineList, &entities);
        assert!(sql.contains("LIMIT 10"));
    }

    // Query explanation tests
    #[test]
    fn test_explain_query_basic() {
        let explanation = explain_query(QueryIntent::AlertList, &QueryEntities::default());
        assert!(explanation.contains("List alerts"));
    }

    #[test]
    fn test_explain_query_with_entities() {
        let entities = QueryEntities {
            machine: Some("orko".to_string()),
            severity: Some("critical".to_string()),
            time_range: Some(TimeRange {
                interval: "1 hour".to_string(),
                sql_expr: String::new(),
            }),
            limit: Some(5),
            ..Default::default()
        };
        let explanation = explain_query(QueryIntent::AlertList, &entities);
        assert!(explanation.contains("orko"));
        assert!(explanation.contains("critical"));
        assert!(explanation.contains("1 hour"));
        assert!(explanation.contains('5'));
    }

    // QueryIntent tests
    #[test]
    fn test_query_intent_description() {
        assert!(!QueryIntent::FleetOverview.description().is_empty());
        assert!(!QueryIntent::Unknown.description().is_empty());
    }

    #[test]
    fn test_query_intent_serialization() {
        let intent = QueryIntent::AlertList;
        let json = serde_json::to_string(&intent).unwrap();
        let parsed: QueryIntent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, intent);
    }

    // NlQueryResult serialization
    #[test]
    fn test_nl_query_result_serialization() {
        let result = NlQueryResult {
            original_question: "test".to_string(),
            intent: QueryIntent::MachineList,
            entities: QueryEntities::default(),
            generated_sql: "SELECT 1".to_string(),
            explanation: "test query".to_string(),
            results: vec![serde_json::json!({"test": 1})],
            result_count: 1,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("machine_list"));
        assert!(json.contains("test query"));
    }

    // Full engine test (with in-memory store)
    #[test]
    fn test_nl_engine_ask_unknown() {
        let store = Arc::new(VcStore::open_memory().unwrap());
        let engine = NlEngine::new(store);

        let result = engine.ask("What is the meaning of life?").unwrap();
        assert_eq!(result.intent, QueryIntent::Unknown);
        assert!(!result.results.is_empty());
    }

    #[test]
    fn test_nl_engine_ask_machines() {
        let store = Arc::new(VcStore::open_memory().unwrap());
        let engine = NlEngine::new(store);

        let result = engine.ask("List all machines").unwrap();
        assert_eq!(result.intent, QueryIntent::MachineList);
        assert!(result.generated_sql.contains("machines"));
    }

    #[test]
    fn test_nl_engine_ask_alerts() {
        let store = Arc::new(VcStore::open_memory().unwrap());

        // Insert some test data
        store
            .execute_batch(
                "INSERT INTO alert_history (id, rule_id, fired_at, severity, title) \
                 VALUES (1, 'r1', current_timestamp, 'critical', 'Test alert')",
            )
            .unwrap();

        let engine = NlEngine::new(store);
        let result = engine.ask("Show me critical alerts").unwrap();
        assert_eq!(result.intent, QueryIntent::AlertList);
        assert_eq!(result.result_count, 1);
    }

    #[test]
    fn test_nl_engine_ask_health() {
        let store = Arc::new(VcStore::open_memory().unwrap());
        let engine = NlEngine::new(store);

        let result = engine
            .ask("What is the health score for machine orko?")
            .unwrap();
        assert_eq!(result.intent, QueryIntent::HealthScore);
        assert!(result.entities.machine.is_some());
    }

    // End-to-end intent + entity + SQL tests
    #[test]
    fn test_e2e_critical_alerts_last_hour() {
        let q = "Show critical alerts from the last hour";
        let intent = classify_intent(q);
        let entities = extract_entities(q);
        let sql = generate_sql(intent, &entities);

        assert_eq!(intent, QueryIntent::AlertList);
        assert_eq!(entities.severity, Some("critical".to_string()));
        assert!(entities.time_range.is_some());
        assert!(sql.contains("severity = 'critical'"));
        assert!(sql.contains("INTERVAL 1 HOUR"));
    }

    #[test]
    fn test_e2e_token_usage_today() {
        let q = "Which agent used the most tokens today?";
        let intent = classify_intent(q);
        let entities = extract_entities(q);
        let sql = generate_sql(intent, &entities);

        assert_eq!(intent, QueryIntent::TokenUsage);
        assert!(entities.time_range.is_some());
        assert!(sql.contains("SUM(token_count)"));
        assert!(sql.contains("current_date"));
    }

    #[test]
    fn test_e2e_top_5_machines() {
        let q = "Show top 5 machines by health";
        let intent = classify_intent(q);
        let entities = extract_entities(q);
        let sql = generate_sql(intent, &entities);

        assert_eq!(intent, QueryIntent::HealthScore);
        assert_eq!(entities.limit, Some(5));
        assert!(sql.contains("LIMIT 5"));
    }

    #[test]
    fn test_e2e_sessions_on_machine() {
        let q = "List sessions on orko from last week";
        let intent = classify_intent(q);
        let entities = extract_entities(q);
        let sql = generate_sql(intent, &entities);

        assert_eq!(intent, QueryIntent::SessionList);
        assert_eq!(entities.machine, Some("orko".to_string()));
        assert!(entities.time_range.is_some());
        assert!(sql.contains("machine_id = 'orko'"));
    }
}
