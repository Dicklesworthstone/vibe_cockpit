//! JSON Schema registry and validation for robot outputs
//!
//! This module provides:
//! - Schema loading from docs/schemas/
//! - Validation helpers for robot output
//! - Schema listing for documentation

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Schema registry entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaEntry {
    /// Schema identifier (e.g., "vc.robot.health.v1")
    pub id: String,
    /// Filename in docs/schemas/
    pub file: String,
    /// Human-readable title
    pub title: String,
    /// Description
    pub description: String,
    /// CLI command that produces this output
    pub command: String,
}

/// Schema registry index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaIndex {
    /// Registry version
    pub version: String,
    /// Available schemas
    pub schemas: Vec<SchemaEntry>,
}

impl Default for SchemaIndex {
    fn default() -> Self {
        Self {
            version: "1.0.0".to_string(),
            schemas: vec![
                SchemaEntry {
                    id: "robot-envelope".to_string(),
                    file: "robot-envelope.json".to_string(),
                    title: "RobotEnvelope".to_string(),
                    description: "Standard envelope for all robot mode output".to_string(),
                    command: "(base schema)".to_string(),
                },
                SchemaEntry {
                    id: "vc.robot.health.v1".to_string(),
                    file: "robot-health.json".to_string(),
                    title: "Health Data".to_string(),
                    description: "Overall fleet health data".to_string(),
                    command: "vc robot health".to_string(),
                },
                SchemaEntry {
                    id: "vc.robot.status.v1".to_string(),
                    file: "robot-status.json".to_string(),
                    title: "Status Data".to_string(),
                    description: "Comprehensive fleet status data".to_string(),
                    command: "vc robot status".to_string(),
                },
                SchemaEntry {
                    id: "vc.robot.triage.v1".to_string(),
                    file: "robot-triage.json".to_string(),
                    title: "Triage Data".to_string(),
                    description: "Triage recommendations".to_string(),
                    command: "vc robot triage".to_string(),
                },
            ],
        }
    }
}

/// Schema registry for managing JSON schemas
pub struct SchemaRegistry {
    /// Base path to schemas directory
    schemas_dir: PathBuf,
    /// Loaded schema content (JSON strings)
    schemas: HashMap<String, String>,
    /// Index of available schemas
    index: SchemaIndex,
}

impl SchemaRegistry {
    /// Create a new schema registry from the docs/schemas directory
    pub fn new(project_root: impl AsRef<Path>) -> Self {
        let schemas_dir = project_root.as_ref().join("docs/schemas");
        Self {
            schemas_dir,
            schemas: HashMap::new(),
            index: SchemaIndex::default(),
        }
    }

    /// Load all schemas from the schemas directory
    pub fn load_all(&mut self) -> Result<(), std::io::Error> {
        for entry in &self.index.schemas {
            let path = self.schemas_dir.join(&entry.file);
            if path.exists() {
                let content = std::fs::read_to_string(&path)?;
                self.schemas.insert(entry.id.clone(), content);
            }
        }
        Ok(())
    }

    /// Get schema content by ID
    pub fn get_schema(&self, schema_id: &str) -> Option<&str> {
        self.schemas.get(schema_id).map(|s| s.as_str())
    }

    /// Get the schema index
    pub fn index(&self) -> &SchemaIndex {
        &self.index
    }

    /// List all available schema IDs
    pub fn list_schemas(&self) -> Vec<&str> {
        self.index.schemas.iter().map(|e| e.id.as_str()).collect()
    }

    /// Find schema entry by ID
    pub fn find_entry(&self, schema_id: &str) -> Option<&SchemaEntry> {
        self.index.schemas.iter().find(|e| e.id == schema_id)
    }

    /// Get schema for a given schema_version from robot output
    pub fn get_schema_for_version(&self, schema_version: &str) -> Option<&str> {
        self.get_schema(schema_version)
    }
}

/// Output for `vc robot-docs schemas` command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemasOutput {
    /// Registry version
    pub version: String,
    /// Available schemas with metadata
    pub schemas: Vec<SchemaEntry>,
    /// Path to schemas directory
    pub schemas_dir: String,
}

/// Generate schemas documentation output
pub fn robot_docs_schemas(project_root: impl AsRef<Path>) -> SchemasOutput {
    let schemas_dir = project_root.as_ref().join("docs/schemas");
    let index = SchemaIndex::default();

    SchemasOutput {
        version: index.version,
        schemas: index.schemas,
        schemas_dir: schemas_dir.display().to_string(),
    }
}

/// Validate that JSON output matches the expected schema_version format
pub fn validate_schema_version(json: &str) -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("Invalid JSON: {e}"))?;

    let schema_version = value
        .get("schema_version")
        .and_then(|v| v.as_str())
        .ok_or("Missing schema_version field")?;

    // Validate format: vc.robot.<name>.v<N>
    if !schema_version.starts_with("vc.robot.") {
        return Err(format!(
            "Invalid schema_version format: {schema_version} (expected vc.robot.<name>.v<N>)"
        ));
    }

    Ok(schema_version.to_string())
}

/// Check if JSON has required envelope fields
pub fn validate_envelope_fields(json: &str) -> Result<(), Vec<String>> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| vec![e.to_string()])?;

    let mut errors = Vec::new();

    if value.get("schema_version").is_none() {
        errors.push("Missing required field: schema_version".to_string());
    }

    if value.get("generated_at").is_none() {
        errors.push("Missing required field: generated_at".to_string());
    }

    if value.get("data").is_none() {
        errors.push("Missing required field: data".to_string());
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_index_default() {
        let index = SchemaIndex::default();
        assert_eq!(index.version, "1.0.0");
        assert!(!index.schemas.is_empty());
    }

    #[test]
    fn test_schema_registry_list() {
        let registry = SchemaRegistry::new("/tmp");
        let schemas = registry.list_schemas();
        assert!(schemas.contains(&"vc.robot.health.v1"));
        assert!(schemas.contains(&"vc.robot.status.v1"));
        assert!(schemas.contains(&"vc.robot.triage.v1"));
    }

    #[test]
    fn test_find_entry() {
        let registry = SchemaRegistry::new("/tmp");
        let entry = registry.find_entry("vc.robot.health.v1");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().command, "vc robot health");
    }

    #[test]
    fn test_validate_schema_version_valid() {
        let json = r#"{"schema_version": "vc.robot.health.v1", "generated_at": "2026-01-29T00:00:00Z", "data": {}}"#;
        let result = validate_schema_version(json);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "vc.robot.health.v1");
    }

    #[test]
    fn test_validate_schema_version_invalid() {
        let json = r#"{"schema_version": "invalid.format", "data": {}}"#;
        let result = validate_schema_version(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_schema_version_missing() {
        let json = r#"{"data": {}}"#;
        let result = validate_schema_version(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_envelope_fields_valid() {
        let json = r#"{"schema_version": "vc.robot.health.v1", "generated_at": "2026-01-29T00:00:00Z", "data": {}}"#;
        let result = validate_envelope_fields(json);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_envelope_fields_missing() {
        let json = r#"{"schema_version": "vc.robot.health.v1"}"#;
        let result = validate_envelope_fields(json);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("generated_at")));
        assert!(errors.iter().any(|e| e.contains("data")));
    }

    #[test]
    fn test_robot_docs_schemas() {
        let output = robot_docs_schemas("/tmp/project");
        assert_eq!(output.version, "1.0.0");
        assert!(!output.schemas.is_empty());
    }
}
