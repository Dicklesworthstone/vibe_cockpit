//! vc_knowledge - Knowledge base for Vibe Cockpit
//!
//! This crate provides:
//! - Knowledge entry storage (solutions, patterns, prompts, debug_logs)
//! - Feedback tracking for usefulness scoring
//! - Search capabilities (keyword-based)
//! - Integration with agent sessions

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use vc_store::VcStore;

/// Knowledge errors
#[derive(Error, Debug)]
pub enum KnowledgeError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Store error: {0}")]
    StoreError(#[from] vc_store::StoreError),

    #[error("Database error: {0}")]
    DatabaseError(#[from] duckdb::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Invalid entry type: {0}")]
    InvalidEntryType(String),

    #[error("Validation error: {0}")]
    ValidationError(String),
}

/// Entry type for knowledge items
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EntryType {
    /// A solution to a specific problem
    Solution,
    /// A reusable code or workflow pattern
    Pattern,
    /// An effective prompt template
    Prompt,
    /// A debug investigation log
    DebugLog,
}

impl EntryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EntryType::Solution => "solution",
            EntryType::Pattern => "pattern",
            EntryType::Prompt => "prompt",
            EntryType::DebugLog => "debug_log",
        }
    }
}

impl std::str::FromStr for EntryType {
    type Err = KnowledgeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "solution" => Ok(EntryType::Solution),
            "pattern" => Ok(EntryType::Pattern),
            "prompt" => Ok(EntryType::Prompt),
            "debug_log" | "debuglog" => Ok(EntryType::DebugLog),
            other => Err(KnowledgeError::InvalidEntryType(other.to_string())),
        }
    }
}

impl std::fmt::Display for EntryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Feedback type for knowledge entries
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackType {
    Helpful,
    NotHelpful,
    Outdated,
}

impl FeedbackType {
    pub fn as_str(&self) -> &'static str {
        match self {
            FeedbackType::Helpful => "helpful",
            FeedbackType::NotHelpful => "not_helpful",
            FeedbackType::Outdated => "outdated",
        }
    }
}

impl std::str::FromStr for FeedbackType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "helpful" => Ok(FeedbackType::Helpful),
            "not_helpful" | "nothelpful" => Ok(FeedbackType::NotHelpful),
            "outdated" => Ok(FeedbackType::Outdated),
            other => Err(format!("unknown feedback type: {other}")),
        }
    }
}

/// A knowledge entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeEntry {
    pub id: Option<i64>,
    pub entry_type: EntryType,
    pub title: String,
    pub summary: Option<String>,
    pub content: String,
    pub source_session_id: Option<String>,
    pub source_file: Option<String>,
    pub source_lines: Option<String>,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub usefulness_score: f64,
    pub view_count: i32,
    pub applied_count: i32,
}

impl KnowledgeEntry {
    /// Create a new knowledge entry
    pub fn new(entry_type: EntryType, title: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            id: None,
            entry_type,
            title: title.into(),
            summary: None,
            content: content.into(),
            source_session_id: None,
            source_file: None,
            source_lines: None,
            tags: vec![],
            created_at: Utc::now(),
            updated_at: None,
            usefulness_score: 0.0,
            view_count: 0,
            applied_count: 0,
        }
    }

    /// Set summary
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    /// Set source session
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.source_session_id = Some(session_id.into());
        self
    }

    /// Set source file and optional line range
    pub fn with_source(mut self, file: impl Into<String>, lines: Option<String>) -> Self {
        self.source_file = Some(file.into());
        self.source_lines = lines;
        self
    }

    /// Set tags
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Validate the entry
    pub fn validate(&self) -> Result<(), KnowledgeError> {
        if self.title.trim().is_empty() {
            return Err(KnowledgeError::ValidationError(
                "title cannot be empty".to_string(),
            ));
        }
        if self.content.trim().is_empty() {
            return Err(KnowledgeError::ValidationError(
                "content cannot be empty".to_string(),
            ));
        }
        Ok(())
    }
}

/// Feedback on a knowledge entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeFeedback {
    pub id: Option<i64>,
    pub entry_id: i64,
    pub feedback_type: FeedbackType,
    pub session_id: Option<String>,
    pub comment: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl KnowledgeFeedback {
    pub fn new(entry_id: i64, feedback_type: FeedbackType) -> Self {
        Self {
            id: None,
            entry_id,
            feedback_type,
            session_id: None,
            comment: None,
            created_at: Utc::now(),
        }
    }

    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    pub fn with_comment(mut self, comment: impl Into<String>) -> Self {
        self.comment = Some(comment.into());
        self
    }
}

/// Search results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub entry: KnowledgeEntry,
    pub score: f64,
}

/// Search options
#[derive(Debug, Clone, Default)]
pub struct SearchOptions {
    pub entry_type: Option<EntryType>,
    pub tags: Vec<String>,
    pub min_score: Option<f64>,
    pub limit: usize,
}

impl SearchOptions {
    pub fn new() -> Self {
        Self {
            limit: 20,
            ..Default::default()
        }
    }

    pub fn with_type(mut self, entry_type: EntryType) -> Self {
        self.entry_type = Some(entry_type);
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }
}

/// Knowledge store for database operations
pub struct KnowledgeStore {
    store: Arc<VcStore>,
}

impl KnowledgeStore {
    /// Create a new knowledge store
    pub fn new(store: Arc<VcStore>) -> Self {
        Self { store }
    }

    /// Insert a new knowledge entry
    pub fn insert(&self, entry: &KnowledgeEntry) -> Result<i64, KnowledgeError> {
        entry.validate()?;

        let tags_json = serde_json::to_string(&entry.tags)?;
        let sql = r#"
            INSERT INTO knowledge_entries
            (entry_type, title, summary, content, source_session_id, source_file, source_lines, tags, created_at, usefulness_score, view_count, applied_count)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            RETURNING id
        "#;

        let conn = self.store.connection();
        let conn_guard = conn.lock().map_err(|e| {
            KnowledgeError::StoreError(vc_store::StoreError::QueryError(format!(
                "lock error: {e}"
            )))
        })?;

        let id: i64 = conn_guard.query_row(
            sql,
            duckdb::params![
                entry.entry_type.as_str(),
                &entry.title,
                &entry.summary,
                &entry.content,
                &entry.source_session_id,
                &entry.source_file,
                &entry.source_lines,
                &tags_json,
                entry.created_at.to_rfc3339(),
                entry.usefulness_score,
                entry.view_count,
                entry.applied_count,
            ],
            |row| row.get(0),
        )?;

        Ok(id)
    }

    /// Get an entry by ID
    pub fn get(&self, id: i64) -> Result<KnowledgeEntry, KnowledgeError> {
        let sql = "SELECT * FROM knowledge_entries WHERE id = ?";
        let conn = self.store.connection();
        let conn_guard = conn.lock().map_err(|e| {
            KnowledgeError::StoreError(vc_store::StoreError::QueryError(format!(
                "lock error: {e}"
            )))
        })?;

        conn_guard
            .query_row(sql, [id], |row| self.row_to_entry(row))
            .map_err(|_| KnowledgeError::NotFound(format!("entry with id {id}")))
    }

    /// Increment view count
    pub fn record_view(&self, id: i64) -> Result<(), KnowledgeError> {
        let sql = "UPDATE knowledge_entries SET view_count = view_count + 1 WHERE id = ?";
        self.store.execute(sql, &[&id.to_string()])?;
        Ok(())
    }

    /// Increment applied count
    pub fn record_applied(&self, id: i64) -> Result<(), KnowledgeError> {
        let sql = "UPDATE knowledge_entries SET applied_count = applied_count + 1 WHERE id = ?";
        self.store.execute(sql, &[&id.to_string()])?;
        Ok(())
    }

    /// Add feedback to an entry
    pub fn add_feedback(&self, feedback: &KnowledgeFeedback) -> Result<i64, KnowledgeError> {
        let sql = r#"
            INSERT INTO knowledge_feedback (entry_id, feedback_type, session_id, comment, created_at)
            VALUES (?, ?, ?, ?, ?)
            RETURNING id
        "#;

        let conn = self.store.connection();
        let conn_guard = conn.lock().map_err(|e| {
            KnowledgeError::StoreError(vc_store::StoreError::QueryError(format!(
                "lock error: {e}"
            )))
        })?;

        let id: i64 = conn_guard.query_row(
            sql,
            duckdb::params![
                feedback.entry_id,
                feedback.feedback_type.as_str(),
                &feedback.session_id,
                &feedback.comment,
                feedback.created_at.to_rfc3339(),
            ],
            |row| row.get(0),
        )?;

        // Update usefulness score based on feedback
        self.recalculate_score(feedback.entry_id)?;

        Ok(id)
    }

    /// Recalculate usefulness score based on feedback
    fn recalculate_score(&self, entry_id: i64) -> Result<(), KnowledgeError> {
        // Simple scoring: helpful = +1, not_helpful = -1, outdated = -0.5
        let sql = r#"
            UPDATE knowledge_entries
            SET usefulness_score = (
                SELECT COALESCE(SUM(
                    CASE feedback_type
                        WHEN 'helpful' THEN 1.0
                        WHEN 'not_helpful' THEN -1.0
                        WHEN 'outdated' THEN -0.5
                        ELSE 0
                    END
                ), 0)
                FROM knowledge_feedback
                WHERE entry_id = ?
            ),
            updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
        "#;
        self.store.execute(sql, &[&entry_id.to_string(), &entry_id.to_string()])?;
        Ok(())
    }

    /// Search for entries by keyword
    pub fn search(&self, query: &str, options: &SearchOptions) -> Result<Vec<SearchResult>, KnowledgeError> {
        let mut conditions = vec!["1=1".to_string()];
        let mut params: Vec<String> = vec![];

        // Filter by entry type
        if let Some(entry_type) = &options.entry_type {
            conditions.push(format!("entry_type = ${}", params.len() + 1));
            params.push(entry_type.as_str().to_string());
        }

        // Keyword search in title and content
        if !query.trim().is_empty() {
            let pattern = format!("%{}%", query.replace('%', "\\%").replace('_', "\\_"));
            conditions.push(format!(
                "(title ILIKE ${} OR content ILIKE ${} OR summary ILIKE ${})",
                params.len() + 1,
                params.len() + 1,
                params.len() + 1
            ));
            params.push(pattern);
        }

        // Build query
        let limit = if options.limit == 0 { 20 } else { options.limit };
        let sql = format!(
            r#"
            SELECT *,
                   (usefulness_score * 0.5 + view_count * 0.1 + applied_count * 0.3) as score
            FROM knowledge_entries
            WHERE {}
            ORDER BY score DESC, created_at DESC
            LIMIT {}
            "#,
            conditions.join(" AND "),
            limit
        );

        let results = self.store.query_json(&sql)?;

        results
            .into_iter()
            .map(|row| {
                let entry = serde_json::from_value::<KnowledgeEntry>(row.clone())
                    .map_err(|e| KnowledgeError::StoreError(vc_store::StoreError::SerializationError(e)))?;
                let score = row["score"].as_f64().unwrap_or(0.0);
                Ok(SearchResult { entry, score })
            })
            .collect()
    }

    /// Get entries by tag
    pub fn get_by_tags(&self, tags: &[String], limit: usize) -> Result<Vec<KnowledgeEntry>, KnowledgeError> {
        if tags.is_empty() {
            return Ok(vec![]);
        }

        // Check if any tag matches
        let tag_conditions: Vec<String> = tags
            .iter()
            .enumerate()
            .map(|(i, _)| format!("list_contains(tags, ${})", i + 1))
            .collect();

        let sql = format!(
            r#"
            SELECT * FROM knowledge_entries
            WHERE ({})
            ORDER BY usefulness_score DESC
            LIMIT {}
            "#,
            tag_conditions.join(" OR "),
            limit
        );

        let results = self.store.query_json(&sql)?;

        results
            .into_iter()
            .map(|row| {
                serde_json::from_value::<KnowledgeEntry>(row)
                    .map_err(|e| KnowledgeError::StoreError(vc_store::StoreError::SerializationError(e)))
            })
            .collect()
    }

    /// Get recent entries
    pub fn recent(&self, limit: usize) -> Result<Vec<KnowledgeEntry>, KnowledgeError> {
        let sql = format!(
            "SELECT * FROM knowledge_entries ORDER BY created_at DESC LIMIT {limit}"
        );
        let results = self.store.query_json(&sql)?;

        results
            .into_iter()
            .map(|row| {
                serde_json::from_value::<KnowledgeEntry>(row)
                    .map_err(|e| KnowledgeError::StoreError(vc_store::StoreError::SerializationError(e)))
            })
            .collect()
    }

    /// Get top-rated entries
    pub fn top_rated(&self, limit: usize) -> Result<Vec<KnowledgeEntry>, KnowledgeError> {
        let sql = format!(
            "SELECT * FROM knowledge_entries WHERE usefulness_score > 0 ORDER BY usefulness_score DESC LIMIT {limit}"
        );
        let results = self.store.query_json(&sql)?;

        results
            .into_iter()
            .map(|row| {
                serde_json::from_value::<KnowledgeEntry>(row)
                    .map_err(|e| KnowledgeError::StoreError(vc_store::StoreError::SerializationError(e)))
            })
            .collect()
    }

    /// Delete an entry
    pub fn delete(&self, id: i64) -> Result<(), KnowledgeError> {
        self.store.execute("DELETE FROM knowledge_feedback WHERE entry_id = ?", &[&id.to_string()])?;
        self.store.execute("DELETE FROM knowledge_entries WHERE id = ?", &[&id.to_string()])?;
        Ok(())
    }

    /// Helper to convert a row to KnowledgeEntry
    fn row_to_entry(&self, row: &duckdb::Row<'_>) -> Result<KnowledgeEntry, duckdb::Error> {
        let entry_type_str: String = row.get("entry_type")?;
        let entry_type = entry_type_str.parse::<EntryType>().unwrap_or(EntryType::Solution);

        let tags_str: Option<String> = row.get("tags")?;
        let tags: Vec<String> = tags_str
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        let created_str: String = row.get("created_at")?;
        let created_at = DateTime::parse_from_rfc3339(&created_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        let updated_str: Option<String> = row.get("updated_at")?;
        let updated_at = updated_str.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&Utc))
                .ok()
        });

        Ok(KnowledgeEntry {
            id: Some(row.get("id")?),
            entry_type,
            title: row.get("title")?,
            summary: row.get("summary")?,
            content: row.get("content")?,
            source_session_id: row.get("source_session_id")?,
            source_file: row.get("source_file")?,
            source_lines: row.get("source_lines")?,
            tags,
            created_at,
            updated_at,
            usefulness_score: row.get("usefulness_score")?,
            view_count: row.get("view_count")?,
            applied_count: row.get("applied_count")?,
        })
    }
}

// ============================================================================
// Legacy types for backward compatibility
// ============================================================================

/// A learned solution pattern (legacy)
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

/// A gotcha/known issue (legacy)
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

/// Knowledge base manager (legacy interface)
pub struct KnowledgeBase {
    store: Option<Arc<VcStore>>,
}

impl KnowledgeBase {
    /// Create a new knowledge base
    pub fn new() -> Self {
        Self { store: None }
    }

    /// Create with store
    pub fn with_store(store: Arc<VcStore>) -> Self {
        Self { store: Some(store) }
    }

    /// Search for relevant solutions
    pub fn search_solutions(&self, _query: &str) -> Result<Vec<Solution>, KnowledgeError> {
        Ok(vec![])
    }

    /// Search for relevant gotchas
    pub fn search_gotchas(&self, _query: &str) -> Result<Vec<Gotcha>, KnowledgeError> {
        Ok(vec![])
    }

    /// Record a new solution from session analysis
    pub fn record_solution(&self, _solution: Solution) -> Result<(), KnowledgeError> {
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

    // EntryType tests
    #[test]
    fn test_entry_type_as_str() {
        assert_eq!(EntryType::Solution.as_str(), "solution");
        assert_eq!(EntryType::Pattern.as_str(), "pattern");
        assert_eq!(EntryType::Prompt.as_str(), "prompt");
        assert_eq!(EntryType::DebugLog.as_str(), "debug_log");
    }

    #[test]
    fn test_entry_type_from_str() {
        assert_eq!("solution".parse::<EntryType>().unwrap(), EntryType::Solution);
        assert_eq!("pattern".parse::<EntryType>().unwrap(), EntryType::Pattern);
        assert_eq!("prompt".parse::<EntryType>().unwrap(), EntryType::Prompt);
        assert_eq!("debug_log".parse::<EntryType>().unwrap(), EntryType::DebugLog);
        assert_eq!("debuglog".parse::<EntryType>().unwrap(), EntryType::DebugLog);
    }

    #[test]
    fn test_entry_type_case_insensitive() {
        assert_eq!("SOLUTION".parse::<EntryType>().unwrap(), EntryType::Solution);
        assert_eq!("Pattern".parse::<EntryType>().unwrap(), EntryType::Pattern);
    }

    #[test]
    fn test_entry_type_invalid() {
        assert!("invalid".parse::<EntryType>().is_err());
    }

    #[test]
    fn test_entry_type_display() {
        assert_eq!(format!("{}", EntryType::Solution), "solution");
        assert_eq!(format!("{}", EntryType::DebugLog), "debug_log");
    }

    #[test]
    fn test_entry_type_serialization() {
        let entry_type = EntryType::Solution;
        let json = serde_json::to_string(&entry_type).unwrap();
        assert_eq!(json, "\"solution\"");

        let parsed: EntryType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, EntryType::Solution);
    }

    // FeedbackType tests
    #[test]
    fn test_feedback_type_as_str() {
        assert_eq!(FeedbackType::Helpful.as_str(), "helpful");
        assert_eq!(FeedbackType::NotHelpful.as_str(), "not_helpful");
        assert_eq!(FeedbackType::Outdated.as_str(), "outdated");
    }

    #[test]
    fn test_feedback_type_from_str() {
        assert_eq!("helpful".parse::<FeedbackType>().unwrap(), FeedbackType::Helpful);
        assert_eq!("not_helpful".parse::<FeedbackType>().unwrap(), FeedbackType::NotHelpful);
        assert_eq!("outdated".parse::<FeedbackType>().unwrap(), FeedbackType::Outdated);
    }

    // KnowledgeEntry tests
    #[test]
    fn test_knowledge_entry_new() {
        let entry = KnowledgeEntry::new(EntryType::Solution, "Test Title", "Test content");
        assert_eq!(entry.title, "Test Title");
        assert_eq!(entry.content, "Test content");
        assert_eq!(entry.entry_type, EntryType::Solution);
        assert!(entry.id.is_none());
        assert!(entry.tags.is_empty());
    }

    #[test]
    fn test_knowledge_entry_builders() {
        let entry = KnowledgeEntry::new(EntryType::Pattern, "Pattern", "Content")
            .with_summary("A summary")
            .with_session("sess-123")
            .with_source("file.rs", Some("10-20".to_string()))
            .with_tags(vec!["rust".to_string(), "pattern".to_string()]);

        assert_eq!(entry.summary, Some("A summary".to_string()));
        assert_eq!(entry.source_session_id, Some("sess-123".to_string()));
        assert_eq!(entry.source_file, Some("file.rs".to_string()));
        assert_eq!(entry.source_lines, Some("10-20".to_string()));
        assert_eq!(entry.tags.len(), 2);
    }

    #[test]
    fn test_knowledge_entry_validation() {
        let valid = KnowledgeEntry::new(EntryType::Solution, "Title", "Content");
        assert!(valid.validate().is_ok());

        let empty_title = KnowledgeEntry::new(EntryType::Solution, "", "Content");
        assert!(empty_title.validate().is_err());

        let whitespace_title = KnowledgeEntry::new(EntryType::Solution, "   ", "Content");
        assert!(whitespace_title.validate().is_err());

        let empty_content = KnowledgeEntry::new(EntryType::Solution, "Title", "");
        assert!(empty_content.validate().is_err());
    }

    #[test]
    fn test_knowledge_entry_serialization() {
        let entry = KnowledgeEntry::new(EntryType::Solution, "Test", "Content")
            .with_tags(vec!["tag1".to_string()]);

        let json = serde_json::to_string(&entry).unwrap();
        let parsed: KnowledgeEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.title, entry.title);
        assert_eq!(parsed.content, entry.content);
        assert_eq!(parsed.entry_type, entry.entry_type);
        assert_eq!(parsed.tags, entry.tags);
    }

    // KnowledgeFeedback tests
    #[test]
    fn test_knowledge_feedback_new() {
        let feedback = KnowledgeFeedback::new(42, FeedbackType::Helpful);
        assert_eq!(feedback.entry_id, 42);
        assert_eq!(feedback.feedback_type, FeedbackType::Helpful);
        assert!(feedback.id.is_none());
    }

    #[test]
    fn test_knowledge_feedback_builders() {
        let feedback = KnowledgeFeedback::new(1, FeedbackType::NotHelpful)
            .with_session("sess-456")
            .with_comment("Not relevant");

        assert_eq!(feedback.session_id, Some("sess-456".to_string()));
        assert_eq!(feedback.comment, Some("Not relevant".to_string()));
    }

    // SearchOptions tests
    #[test]
    fn test_search_options_default() {
        let opts = SearchOptions::default();
        assert!(opts.entry_type.is_none());
        assert!(opts.tags.is_empty());
        assert_eq!(opts.limit, 0);
    }

    #[test]
    fn test_search_options_builders() {
        let opts = SearchOptions::new()
            .with_type(EntryType::Solution)
            .with_tags(vec!["rust".to_string()])
            .with_limit(50);

        assert_eq!(opts.entry_type, Some(EntryType::Solution));
        assert_eq!(opts.tags, vec!["rust".to_string()]);
        assert_eq!(opts.limit, 50);
    }

    // KnowledgeBase legacy tests
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

    // Solution legacy tests
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
    }

    // Gotcha legacy tests
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
    }

    // KnowledgeError tests
    #[test]
    fn test_error_not_found() {
        let err = KnowledgeError::NotFound("solution-123".to_string());
        assert!(err.to_string().contains("Not found"));
        assert!(err.to_string().contains("solution-123"));
    }

    #[test]
    fn test_error_invalid_entry_type() {
        let err = KnowledgeError::InvalidEntryType("unknown".to_string());
        assert!(err.to_string().contains("Invalid entry type"));
    }

    #[test]
    fn test_error_validation() {
        let err = KnowledgeError::ValidationError("title empty".to_string());
        assert!(err.to_string().contains("Validation error"));
    }
}
