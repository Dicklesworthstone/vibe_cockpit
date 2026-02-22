//! Query Guardrails - Safety constraints for SQL queries
//!
//! This module provides:
//! - Query validation (read-only enforcement)
//! - Safe query templates with parameter substitution
//! - Runtime and row limits
//! - Query audit logging

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Query validation errors
#[derive(Debug, Clone, Serialize)]
pub enum ValidationError {
    /// Query contains forbidden statement type
    ForbiddenStatement { statement_type: String },
    /// Query exceeds row limit
    RowLimitExceeded { limit: usize, attempted: usize },
    /// Query timeout exceeded
    TimeoutExceeded { limit_ms: u64 },
    /// Unknown template
    UnknownTemplate { name: String },
    /// Missing required parameter
    MissingParameter { param: String },
    /// Invalid parameter value
    InvalidParameter { param: String, reason: String },
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ForbiddenStatement { statement_type } => {
                write!(
                    f,
                    "Forbidden statement type: {statement_type}. Only SELECT is allowed."
                )
            }
            Self::RowLimitExceeded { limit, attempted } => {
                write!(
                    f,
                    "Row limit exceeded: attempted {attempted}, limit is {limit}"
                )
            }
            Self::TimeoutExceeded { limit_ms } => {
                write!(f, "Query timeout exceeded: limit is {limit_ms}ms")
            }
            Self::UnknownTemplate { name } => {
                write!(f, "Unknown query template: {name}")
            }
            Self::MissingParameter { param } => {
                write!(f, "Missing required parameter: {param}")
            }
            Self::InvalidParameter { param, reason } => {
                write!(f, "Invalid parameter '{param}': {reason}")
            }
        }
    }
}

impl std::error::Error for ValidationError {}

/// Query guardrail configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardrailConfig {
    /// Maximum rows to return
    pub max_rows: usize,
    /// Maximum query runtime in milliseconds
    pub max_runtime_ms: u64,
    /// Maximum output size in bytes
    pub max_output_bytes: usize,
    /// Allow raw SQL (if false, only templates allowed)
    pub allow_raw_sql: bool,
}

impl Default for GuardrailConfig {
    fn default() -> Self {
        Self {
            max_rows: 10000,
            max_runtime_ms: 30000,              // 30 seconds
            max_output_bytes: 10 * 1024 * 1024, // 10 MB
            allow_raw_sql: true,
        }
    }
}

/// A safe query template with named parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryTemplate {
    /// Template name
    pub name: String,
    /// Description for help output
    pub description: String,
    /// SQL template with placeholders like `{param_name}`
    pub sql: String,
    /// Required parameters with descriptions
    pub params: Vec<TemplateParam>,
    /// Whether this template is safe for agents
    pub agent_safe: bool,
}

/// Template parameter definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateParam {
    /// Parameter name (used in {name} placeholders)
    pub name: String,
    /// Description for help
    pub description: String,
    /// Default value if not provided
    pub default: Option<String>,
    /// Parameter type for validation
    pub param_type: ParamType,
}

/// Parameter types for validation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamType {
    /// String value (will be quoted)
    String,
    /// Integer value
    Integer,
    /// Float value
    Float,
    /// Boolean value
    Boolean,
    /// Identifier (table/column name, no quoting)
    Identifier,
    /// Timestamp in RFC3339 format
    Timestamp,
}

/// Query validator
pub struct QueryValidator {
    config: GuardrailConfig,
    templates: HashMap<String, QueryTemplate>,
}

impl QueryValidator {
    /// Create a new validator with configuration
    #[must_use]
    pub fn new(config: GuardrailConfig) -> Self {
        let mut validator = Self {
            config,
            templates: HashMap::new(),
        };
        validator.register_default_templates();
        validator
    }

    /// Register default safe templates
    #[allow(clippy::too_many_lines)]
    fn register_default_templates(&mut self) {
        // Machine status template
        self.register_template(QueryTemplate {
            name: "machine_status".to_string(),
            description: "Get status of all machines or a specific machine".to_string(),
            sql: "SELECT machine_id, hostname, enabled, last_seen_at, tags \
                  FROM machines \
                  WHERE ({machine_id} IS NULL OR machine_id = {machine_id}) \
                  ORDER BY hostname \
                  LIMIT {limit}"
                .to_string(),
            params: vec![
                TemplateParam {
                    name: "machine_id".to_string(),
                    description: "Optional machine ID to filter".to_string(),
                    default: Some("NULL".to_string()),
                    param_type: ParamType::String,
                },
                TemplateParam {
                    name: "limit".to_string(),
                    description: "Maximum rows to return".to_string(),
                    default: Some("100".to_string()),
                    param_type: ParamType::Integer,
                },
            ],
            agent_safe: true,
        });

        // Recent alerts template
        self.register_template(QueryTemplate {
            name: "recent_alerts".to_string(),
            description: "Get recent alerts, optionally filtered by severity".to_string(),
            sql: "SELECT id, rule_id, fired_at, severity, title, message, acknowledged_at \
                  FROM alert_history \
                  WHERE ({severity} IS NULL OR severity = {severity}) \
                  ORDER BY fired_at DESC \
                  LIMIT {limit}"
                .to_string(),
            params: vec![
                TemplateParam {
                    name: "severity".to_string(),
                    description: "Filter by severity (critical, high, medium, low)".to_string(),
                    default: Some("NULL".to_string()),
                    param_type: ParamType::String,
                },
                TemplateParam {
                    name: "limit".to_string(),
                    description: "Maximum rows to return".to_string(),
                    default: Some("100".to_string()),
                    param_type: ParamType::Integer,
                },
            ],
            agent_safe: true,
        });

        // Repository status template
        self.register_template(QueryTemplate {
            name: "repo_status".to_string(),
            description: "Get repository status summary".to_string(),
            sql:
                "SELECT repo_id, path, branch, dirty, ahead, behind, modified_count, collected_at \
                  FROM repo_status_snapshots \
                  WHERE collected_at >= TIMESTAMP {since} \
                  ORDER BY collected_at DESC \
                  LIMIT {limit}"
                    .to_string(),
            params: vec![
                TemplateParam {
                    name: "since".to_string(),
                    description: "Only show repos updated since this time (RFC3339)".to_string(),
                    default: Some("'1970-01-01T00:00:00Z'".to_string()),
                    param_type: ParamType::Timestamp,
                },
                TemplateParam {
                    name: "limit".to_string(),
                    description: "Maximum rows to return".to_string(),
                    default: Some("100".to_string()),
                    param_type: ParamType::Integer,
                },
            ],
            agent_safe: true,
        });

        // Collector health template
        self.register_template(QueryTemplate {
            name: "collector_health".to_string(),
            description: "Get collector execution status".to_string(),
            sql: "SELECT machine_id, collector, collected_at, success, duration_ms, \
                  rows_inserted, error_class, freshness_seconds \
                  FROM collector_health \
                  ORDER BY collected_at DESC \
                  LIMIT {limit}"
                .to_string(),
            params: vec![TemplateParam {
                name: "limit".to_string(),
                description: "Maximum rows to return".to_string(),
                default: Some("100".to_string()),
                param_type: ParamType::Integer,
            }],
            agent_safe: true,
        });

        // System metrics template
        self.register_template(QueryTemplate {
            name: "system_metrics".to_string(),
            description: "Get system metrics for a machine".to_string(),
            sql: "SELECT machine_id, collected_at, cpu_total, load1, load5, load15, \
                  mem_used_bytes, mem_total_bytes, mem_available_bytes \
                  FROM sys_samples \
                  WHERE ({machine_id} IS NULL OR machine_id = {machine_id}) \
                  AND collected_at >= TIMESTAMP {since} \
                  ORDER BY collected_at DESC \
                  LIMIT {limit}"
                .to_string(),
            params: vec![
                TemplateParam {
                    name: "machine_id".to_string(),
                    description: "Optional machine ID to filter".to_string(),
                    default: Some("NULL".to_string()),
                    param_type: ParamType::String,
                },
                TemplateParam {
                    name: "since".to_string(),
                    description: "Only show metrics since this time (RFC3339)".to_string(),
                    default: Some("'1970-01-01T00:00:00Z'".to_string()),
                    param_type: ParamType::Timestamp,
                },
                TemplateParam {
                    name: "limit".to_string(),
                    description: "Maximum rows to return".to_string(),
                    default: Some("1000".to_string()),
                    param_type: ParamType::Integer,
                },
            ],
            agent_safe: true,
        });
    }

    /// Register a custom template
    pub fn register_template(&mut self, template: QueryTemplate) {
        self.templates.insert(template.name.clone(), template);
    }

    /// Get all available templates
    #[must_use]
    pub fn templates(&self) -> &HashMap<String, QueryTemplate> {
        &self.templates
    }

    /// Validate a raw SQL query
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError`] when raw SQL is disallowed or the query is not read-only.
    pub fn validate_raw(&self, sql: &str) -> Result<(), ValidationError> {
        if !self.config.allow_raw_sql {
            return Err(ValidationError::ForbiddenStatement {
                statement_type: "raw SQL".to_string(),
            });
        }

        self.validate_readonly(sql)
    }

    /// Check that a query is read-only (SELECT only)
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError`] when a forbidden statement is detected or the query is not
    /// `SELECT`/`WITH`.
    pub fn validate_readonly(&self, sql: &str) -> Result<(), ValidationError> {
        // Replace all whitespace characters with a single space to avoid bypasses
        let normalized: String = sql.chars().map(|c| if c.is_whitespace() { ' ' } else { c }).collect();
        let normalized = normalized.trim().to_uppercase();
        // Pad with spaces to simplify boundary checks (catches keywords at start/end)
        let padded = format!(" {normalized} ");

        // Check for forbidden statement types
        let forbidden = [
            "INSERT",
            "UPDATE",
            "DELETE",
            "DROP",
            "CREATE",
            "ALTER",
            "TRUNCATE",
            "REPLACE",
            "MERGE",
            "UPSERT",
            "GRANT",
            "REVOKE",
            "VACUUM",
            "PRAGMA",
            "ATTACH",
            "DETACH",
            "BEGIN",
            "COMMIT",
            "ROLLBACK",
            "SAVEPOINT",
        ];

        for keyword in forbidden {
            // Check for forbidden keyword surrounded by word boundaries (spaces)
            // The padded string ensures keywords at start/end are also caught
            if padded.contains(&format!(" {keyword} ")) {
                // Allow SELECT after WITH (for CTEs like "WITH x AS (SELECT ...)")
                if keyword == "SELECT" {
                    continue;
                }
                // Avoid false positives for " AS CREATE" type patterns in column aliases
                if padded.contains(&format!(" AS {keyword} ")) {
                    continue;
                }
                return Err(ValidationError::ForbiddenStatement {
                    statement_type: keyword.to_string(),
                });
            }
            // Also check for keyword after semicolon (multi-statement attempts)
            if padded.contains(&format!("; {keyword} ")) || padded.contains(&format!(";{keyword} "))
            {
                return Err(ValidationError::ForbiddenStatement {
                    statement_type: keyword.to_string(),
                });
            }
        }

        // Ensure query is a SELECT or WITH ... SELECT
        if !normalized.starts_with("SELECT") && !normalized.starts_with("WITH") {
            return Err(ValidationError::ForbiddenStatement {
                statement_type: "non-SELECT".to_string(),
            });
        }

        Ok(())
    }

    /// Expand a template with parameters
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError`] when the template is missing, a required parameter is missing,
    /// or a parameter fails validation.
    pub fn expand_template(
        &self,
        template_name: &str,
        params: &HashMap<String, String>,
    ) -> Result<String, ValidationError> {
        let template =
            self.templates
                .get(template_name)
                .ok_or_else(|| ValidationError::UnknownTemplate {
                    name: template_name.to_string(),
                })?;

        let mut sql = template.sql.clone();

        for param_def in &template.params {
            let placeholder = format!("{{{}}}", param_def.name);
            let value = params
                .get(&param_def.name)
                .or(param_def.default.as_ref())
                .ok_or_else(|| ValidationError::MissingParameter {
                    param: param_def.name.clone(),
                })?;

            // Validate parameter value
            let validated_value =
                Self::validate_param_value(value, &param_def.param_type, &param_def.name)?;
            sql = sql.replace(&placeholder, &validated_value);
        }

        Ok(sql)
    }

    /// Validate and sanitize a parameter value
    fn validate_param_value(
        value: &str,
        param_type: &ParamType,
        param_name: &str,
    ) -> Result<String, ValidationError> {
        match param_type {
            ParamType::String => {
                // Escape single quotes and wrap in quotes
                if value == "NULL" {
                    Ok("NULL".to_string())
                } else {
                    let escaped = value.replace('\'', "''");
                    Ok(format!("'{escaped}'"))
                }
            }
            ParamType::Integer => {
                value
                    .parse::<i64>()
                    .map_err(|_| ValidationError::InvalidParameter {
                        param: param_name.to_string(),
                        reason: "Expected integer".to_string(),
                    })?;
                Ok(value.to_string())
            }
            ParamType::Float => {
                value
                    .parse::<f64>()
                    .map_err(|_| ValidationError::InvalidParameter {
                        param: param_name.to_string(),
                        reason: "Expected float".to_string(),
                    })?;
                Ok(value.to_string())
            }
            ParamType::Boolean => {
                let lower = value.to_lowercase();
                if lower == "true" || lower == "1" {
                    Ok("TRUE".to_string())
                } else if lower == "false" || lower == "0" {
                    Ok("FALSE".to_string())
                } else {
                    Err(ValidationError::InvalidParameter {
                        param: param_name.to_string(),
                        reason: "Expected boolean (true/false)".to_string(),
                    })
                }
            }
            ParamType::Identifier => {
                // Validate identifier (alphanumeric + underscore only)
                if value.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    Ok(value.to_string())
                } else {
                    Err(ValidationError::InvalidParameter {
                        param: param_name.to_string(),
                        reason: "Identifier must be alphanumeric".to_string(),
                    })
                }
            }
            ParamType::Timestamp => {
                // Basic validation for timestamp format
                if value == "NULL" || value.starts_with('\'') {
                    Ok(value.to_string())
                } else {
                    // Try to parse as RFC3339
                    chrono::DateTime::parse_from_rfc3339(value).map_err(|_| {
                        ValidationError::InvalidParameter {
                            param: param_name.to_string(),
                            reason: "Expected RFC3339 timestamp".to_string(),
                        }
                    })?;
                    Ok(format!("'{value}'"))
                }
            }
        }
    }

    /// Get the guardrail configuration
    #[must_use]
    pub fn config(&self) -> &GuardrailConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_select() {
        let validator = QueryValidator::new(GuardrailConfig::default());
        assert!(
            validator
                .validate_readonly("SELECT * FROM machines")
                .is_ok()
        );
        assert!(
            validator
                .validate_readonly("select * from machines")
                .is_ok()
        );
        assert!(
            validator
                .validate_readonly("  SELECT * FROM machines  ")
                .is_ok()
        );
    }

    #[test]
    fn test_validate_with_cte() {
        let validator = QueryValidator::new(GuardrailConfig::default());
        let sql = "WITH recent AS (SELECT * FROM machines) SELECT * FROM recent";
        assert!(validator.validate_readonly(sql).is_ok());
    }

    #[test]
    fn test_reject_insert() {
        let validator = QueryValidator::new(GuardrailConfig::default());
        let result = validator.validate_readonly("INSERT INTO machines VALUES (1, 'test')");
        assert!(matches!(
            result,
            Err(ValidationError::ForbiddenStatement { .. })
        ));
    }

    #[test]
    fn test_reject_delete() {
        let validator = QueryValidator::new(GuardrailConfig::default());
        let result = validator.validate_readonly("DELETE FROM machines WHERE id = 1");
        assert!(matches!(
            result,
            Err(ValidationError::ForbiddenStatement { .. })
        ));
    }

    #[test]
    fn test_reject_drop() {
        let validator = QueryValidator::new(GuardrailConfig::default());
        let result = validator.validate_readonly("DROP TABLE machines");
        assert!(matches!(
            result,
            Err(ValidationError::ForbiddenStatement { .. })
        ));
    }

    #[test]
    fn test_expand_template() {
        let validator = QueryValidator::new(GuardrailConfig::default());

        let mut params = HashMap::new();
        params.insert("limit".to_string(), "50".to_string());

        let sql = validator
            .expand_template("machine_status", &params)
            .unwrap();
        assert!(sql.contains("LIMIT 50"));
    }

    #[test]
    fn test_expand_template_with_string_param() {
        let validator = QueryValidator::new(GuardrailConfig::default());

        let mut params = HashMap::new();
        params.insert("machine_id".to_string(), "orko".to_string());
        params.insert("limit".to_string(), "10".to_string());

        let sql = validator
            .expand_template("machine_status", &params)
            .unwrap();
        assert!(sql.contains("'orko'"));
    }

    #[test]
    fn test_unknown_template() {
        let validator = QueryValidator::new(GuardrailConfig::default());
        let params = HashMap::new();
        let result = validator.expand_template("nonexistent", &params);
        assert!(matches!(
            result,
            Err(ValidationError::UnknownTemplate { .. })
        ));
    }

    #[test]
    fn test_invalid_integer_param() {
        let validator = QueryValidator::new(GuardrailConfig::default());
        let mut params = HashMap::new();
        params.insert("limit".to_string(), "not_a_number".to_string());

        let result = validator.expand_template("machine_status", &params);
        assert!(matches!(
            result,
            Err(ValidationError::InvalidParameter { .. })
        ));
    }

    #[test]
    fn test_sql_injection_prevention() {
        let validator = QueryValidator::new(GuardrailConfig::default());
        let mut params = HashMap::new();
        params.insert(
            "machine_id".to_string(),
            "'; DROP TABLE machines; --".to_string(),
        );
        params.insert("limit".to_string(), "10".to_string());

        let sql = validator
            .expand_template("machine_status", &params)
            .unwrap();
        // The injection should be escaped: ' becomes '' inside the string literal
        // The malicious text is now safely inside a string literal
        assert!(sql.contains("''"));
        // The string should be properly quoted - original ' is now '' and whole value is in quotes
        // This means "'; DROP TABLE" becomes "'''; DROP TABLE" (inside single quotes)
        assert!(sql.contains("'''"));
        // Verify the injection is properly escaped inside a string literal, not executable
        // The SQL should look like: ... = '''; DROP TABLE machines; --' ...
        // NOT like: ... = ''; DROP TABLE machines; --
        assert!(!sql.contains("= ''; DROP"));
    }

    #[test]
    fn test_templates_registered() {
        let validator = QueryValidator::new(GuardrailConfig::default());
        assert!(validator.templates().contains_key("machine_status"));
        assert!(validator.templates().contains_key("recent_alerts"));
        assert!(validator.templates().contains_key("repo_status"));
        assert!(validator.templates().contains_key("collector_health"));
        assert!(validator.templates().contains_key("system_metrics"));
    }

    #[test]
    fn test_raw_sql_disabled() {
        let config = GuardrailConfig {
            allow_raw_sql: false,
            ..Default::default()
        };
        let validator = QueryValidator::new(config);
        let result = validator.validate_raw("SELECT * FROM machines");
        assert!(matches!(
            result,
            Err(ValidationError::ForbiddenStatement { .. })
        ));
    }
}
