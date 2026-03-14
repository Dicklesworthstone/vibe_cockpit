//! Machine inventory and registry
//!
//! Provides CRUD operations for machines and their tool availability.

use std::sync::Arc;

use chrono::Utc;
use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use vc_config::{MachineConfig, VcConfig};
use vc_store::VcStore;

use crate::executor::SshConfig;

#[derive(Error, Debug)]
pub enum RegistryError {
    #[error("Store error: {0}")]
    StoreError(#[from] vc_store::StoreError),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
}

/// Machine status values
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum MachineStatus {
    Online,
    Offline,
    #[default]
    Unknown,
}

impl MachineStatus {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Online => "online",
            Self::Offline => "offline",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Machine {
    pub machine_id: String,
    pub hostname: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub ssh_host: Option<String>,
    #[serde(default)]
    pub ssh_user: Option<String>,
    #[serde(default)]
    pub ssh_key_path: Option<String>,
    #[serde(default = "default_ssh_port")]
    pub ssh_port: u16,
    #[serde(default, deserialize_with = "deserialize_boolish")]
    pub is_local: bool,
    #[serde(default)]
    pub os_type: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
    #[serde(default)]
    pub added_at: Option<String>,
    #[serde(default)]
    pub last_seen_at: Option<String>,
    #[serde(default)]
    pub last_probe_at: Option<String>,
    #[serde(default)]
    pub status: MachineStatus,
    #[serde(default, deserialize_with = "deserialize_tags")]
    pub tags: Vec<String>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    #[serde(default = "default_true", deserialize_with = "deserialize_enabled")]
    pub enabled: bool,
}

impl Machine {
    #[must_use]
    pub fn ssh_config(&self) -> Option<SshConfig> {
        let host = self.ssh_host.as_ref()?;
        let user = self.ssh_user.as_ref()?;
        let mut cfg = SshConfig::new(user.clone(), host.clone()).with_port(self.ssh_port);
        if let Some(path) = &self.ssh_key_path {
            cfg = cfg.with_key(path.clone());
        }
        Some(cfg)
    }

    fn normalize_metadata(mut self) -> Self {
        if let Some(serde_json::Value::String(raw)) = &self.metadata
            && let Ok(value) = serde_json::from_str::<serde_json::Value>(raw)
        {
            self.metadata = Some(value);
        }

        if self.tags.is_empty()
            && let Some(serde_json::Value::Object(map)) = &self.metadata
            && let Some(serde_json::Value::Array(tags)) = map.get("tags")
        {
            let parsed: Vec<String> = tags
                .iter()
                .filter_map(|tag| tag.as_str().map(std::string::ToString::to_string))
                .collect();
            if !parsed.is_empty() {
                self.tags = parsed;
            }
        }
        self
    }

    fn to_row(&self) -> serde_json::Value {
        let metadata = if let Some(value) = &self.metadata {
            value.clone()
        } else if !self.tags.is_empty() {
            serde_json::json!({ "tags": self.tags })
        } else {
            serde_json::Value::Null
        };

        serde_json::json!({
            "machine_id": self.machine_id,
            "hostname": self.hostname,
            "display_name": self.display_name,
            "ssh_host": self.ssh_host,
            "ssh_user": self.ssh_user,
            "ssh_key_path": self.ssh_key_path,
            "ssh_port": self.ssh_port,
            "is_local": self.is_local,
            "os_type": self.os_type,
            "arch": self.arch,
            "added_at": self.added_at,
            "last_seen_at": self.last_seen_at,
            "last_probe_at": self.last_probe_at,
            "status": self.status.as_str(),
            "tags": &self.tags,
            "metadata": if metadata.is_null() { serde_json::Value::Null } else { metadata },
            "enabled": self.enabled,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub tool_name: String,
    pub tool_path: Option<String>,
    pub tool_version: Option<String>,
    pub is_available: bool,
}

impl ToolInfo {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            tool_name: name.into(),
            tool_path: None,
            tool_version: None,
            is_available: true,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MachineFilter {
    pub status: Option<MachineStatus>,
    pub tags: Option<Vec<String>>,
    pub is_local: Option<bool>,
    pub enabled: Option<bool>,
}

impl MachineFilter {
    fn matches(&self, machine: &Machine) -> bool {
        if let Some(status) = self.status
            && machine.status != status
        {
            return false;
        }
        if let Some(is_local) = self.is_local
            && machine.is_local != is_local
        {
            return false;
        }
        if let Some(enabled) = self.enabled
            && machine.enabled != enabled
        {
            return false;
        }
        if let Some(tags) = &self.tags
            && !tags.iter().all(|tag| machine.tags.iter().any(|t| t == tag))
        {
            return false;
        }
        true
    }
}

pub struct MachineRegistry {
    store: Arc<VcStore>,
}

impl MachineRegistry {
    #[must_use]
    pub fn new(store: Arc<VcStore>) -> Self {
        Self { store }
    }

    /// Load machines from config and persist them into the registry table.
    ///
    /// # Errors
    ///
    /// Returns [`RegistryError`] when serialization or database upsert fails.
    pub fn load_from_config(&self, config: &VcConfig) -> Result<usize, RegistryError> {
        let mut machines: Vec<Machine> = Vec::new();
        let mut has_local = false;

        for (id, machine) in &config.machines {
            if id == "local" {
                has_local = true;
            }
            machines.push(machine_from_config(id, machine));
        }

        if !has_local {
            machines.push(local_machine_default());
        }

        let rows: Vec<_> = machines.iter().map(Machine::to_row).collect();
        self.store.upsert_json("machines", &rows, &["machine_id"])?;
        Ok(rows.len())
    }

    /// Insert or update a machine row.
    ///
    /// # Errors
    ///
    /// Returns [`RegistryError`] when persistence fails.
    pub fn upsert_machine(&self, machine: &Machine) -> Result<(), RegistryError> {
        let row = machine.to_row();
        self.store
            .upsert_json("machines", &[row], &["machine_id"])?;
        Ok(())
    }

    /// Fetch a machine by ID.
    ///
    /// # Errors
    ///
    /// Returns [`RegistryError`] when querying or deserialization fails.
    pub fn get_machine(&self, id: &str) -> Result<Option<Machine>, RegistryError> {
        let sql = format!(
            "SELECT machine_id, hostname, display_name, ssh_host, ssh_user, ssh_key_path, ssh_port, \
             is_local, os_type, arch, COALESCE(added_at, created_at) AS added_at, last_seen_at, \
             last_probe_at, status, tags, COALESCE(metadata, metadata_json) AS metadata, enabled \
             FROM machines WHERE machine_id = '{}' LIMIT 1",
            escape_sql_literal(id)
        );

        let mut rows = self.store.query_json(&sql)?;
        if let Some(row) = rows.pop() {
            let machine: Machine = serde_json::from_value(row)?;
            return Ok(Some(machine.normalize_metadata()));
        }
        Ok(None)
    }

    /// List machines, optionally applying a filter.
    ///
    /// # Errors
    ///
    /// Returns [`RegistryError`] when query execution fails.
    pub fn list_machines(
        &self,
        filter: Option<MachineFilter>,
    ) -> Result<Vec<Machine>, RegistryError> {
        let sql = "SELECT machine_id, hostname, display_name, ssh_host, ssh_user, ssh_key_path, ssh_port, \
                   is_local, os_type, arch, COALESCE(added_at, created_at) AS added_at, last_seen_at, \
                   last_probe_at, status, tags, COALESCE(metadata, metadata_json) AS metadata, enabled \
                   FROM machines ORDER BY hostname";
        let rows = self.store.query_json(sql)?;

        let mut machines: Vec<Machine> = rows
            .into_iter()
            .filter_map(|row| serde_json::from_value::<Machine>(row).ok())
            .map(Machine::normalize_metadata)
            .collect();

        if let Some(filter) = filter {
            machines.retain(|m| filter.matches(m));
        }

        Ok(machines)
    }

    /// Update machine status and touch `last_seen_at`.
    ///
    /// # Errors
    ///
    /// Returns [`RegistryError`] when update fails.
    pub fn update_status(&self, id: &str, status: MachineStatus) -> Result<(), RegistryError> {
        let sql = format!(
            "UPDATE machines SET status = '{}', last_seen_at = current_timestamp WHERE machine_id = '{}'",
            status.as_str(),
            escape_sql_literal(id)
        );
        self.store.execute_simple(&sql)?;
        Ok(())
    }

    /// Record or update tool probe information for a machine.
    ///
    /// # Errors
    ///
    /// Returns [`RegistryError`] when upsert/update fails.
    pub fn record_tool(&self, id: &str, tool: &ToolInfo) -> Result<(), RegistryError> {
        let row = serde_json::json!({
            "machine_id": id,
            "tool_name": &tool.tool_name,
            "tool_path": &tool.tool_path,
            "tool_version": &tool.tool_version,
            "is_available": tool.is_available,
            "probed_at": Utc::now().to_rfc3339(),
        });

        self.store
            .upsert_json("machine_tools", &[row], &["machine_id", "tool_name"])?;

        let sql = format!(
            "UPDATE machines SET last_probe_at = current_timestamp WHERE machine_id = '{}'",
            escape_sql_literal(id)
        );
        self.store.execute_simple(&sql)?;
        Ok(())
    }

    /// Enable or disable a machine in the registry.
    ///
    /// # Errors
    ///
    /// Returns [`RegistryError`] when update fails.
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), RegistryError> {
        let sql = format!(
            "UPDATE machines SET enabled = {} WHERE machine_id = '{}'",
            if enabled { "TRUE" } else { "FALSE" },
            escape_sql_literal(id)
        );
        self.store.execute_simple(&sql)?;
        Ok(())
    }
}

fn local_machine_default() -> Machine {
    let hostname = default_hostname();
    Machine {
        machine_id: "local".to_string(),
        hostname: hostname.clone(),
        display_name: Some("Local Machine".to_string()),
        ssh_host: None,
        ssh_user: None,
        ssh_key_path: None,
        ssh_port: default_ssh_port(),
        is_local: true,
        os_type: None,
        arch: None,
        added_at: Some(Utc::now().to_rfc3339()),
        last_seen_at: None,
        last_probe_at: None,
        status: MachineStatus::Unknown,
        tags: Vec::new(),
        metadata: None,
        enabled: true,
    }
}

fn machine_from_config(id: &str, config: &MachineConfig) -> Machine {
    let hostname = config
        .ssh_host
        .clone()
        .unwrap_or_else(|| config.name.clone());
    let ssh_key_path = config
        .ssh_key
        .as_ref()
        .map(|p| p.to_string_lossy().to_string());
    let is_local = config.ssh_host.is_none();

    let metadata = if config.collectors.is_empty() && config.tags.is_empty() {
        None
    } else {
        Some(serde_json::json!({
            "collectors": config.collectors,
            "tags": config.tags,
            "source": "config"
        }))
    };

    Machine {
        machine_id: id.to_string(),
        hostname,
        display_name: Some(config.name.clone()),
        ssh_host: config.ssh_host.clone(),
        ssh_user: config.ssh_user.clone(),
        ssh_key_path,
        ssh_port: config.ssh_port,
        is_local,
        os_type: None,
        arch: None,
        added_at: Some(Utc::now().to_rfc3339()),
        last_seen_at: None,
        last_probe_at: None,
        status: MachineStatus::Unknown,
        tags: config.tags.clone(),
        metadata,
        enabled: config.enabled,
    }
}

fn escape_sql_literal(value: &str) -> String {
    value.replace('\'', "''")
}

fn default_true() -> bool {
    true
}

fn deserialize_boolish<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_boolish_with_default(deserializer, false)
}

fn deserialize_enabled<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_boolish_with_default(deserializer, true)
}

fn deserialize_boolish_with_default<'de, D>(
    deserializer: D,
    default: bool,
) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Bool(value) => Ok(value),
        serde_json::Value::Number(value) => value
            .as_i64()
            .map(|value| value != 0)
            .ok_or_else(|| de::Error::custom("expected integer-compatible boolean")),
        serde_json::Value::String(value) => match value.trim().to_ascii_lowercase().as_str() {
            "true" | "1" => Ok(true),
            "false" | "0" => Ok(false),
            other => Err(de::Error::custom(format!(
                "expected boolean-compatible string, got {other}"
            ))),
        },
        serde_json::Value::Null => Ok(default),
        other => Err(de::Error::custom(format!(
            "expected boolean-compatible value, got {other}"
        ))),
    }
}

fn deserialize_tags<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Array(tags) => tags
            .into_iter()
            .map(|tag| {
                tag.as_str()
                    .map(std::string::ToString::to_string)
                    .ok_or_else(|| de::Error::custom("expected tag array of strings"))
            })
            .collect(),
        serde_json::Value::String(tags) => {
            if tags.trim().is_empty() {
                return Ok(Vec::new());
            }

            serde_json::from_str::<Vec<String>>(&tags).or_else(|_| Ok(vec![tags]))
        }
        serde_json::Value::Null => Ok(Vec::new()),
        other => Err(de::Error::custom(format!(
            "expected tags array or JSON string, got {other}"
        ))),
    }
}

fn default_ssh_port() -> u16 {
    22
}

fn default_hostname() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "local".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_loads_local_default() {
        let store = Arc::new(VcStore::open_memory().unwrap());
        let registry = MachineRegistry::new(store);
        let config = VcConfig::default();

        let count = registry.load_from_config(&config).unwrap();
        assert_eq!(count, 1);

        let machines = registry.list_machines(None).unwrap();
        assert_eq!(machines.len(), 1);
        assert_eq!(machines[0].machine_id, "local");
        assert!(machines[0].is_local);
    }

    #[test]
    fn test_registry_loads_remote_machine() {
        let store = Arc::new(VcStore::open_memory().unwrap());
        let registry = MachineRegistry::new(store);

        let mut config = VcConfig::default();
        config.machines.insert(
            "remote-1".to_string(),
            MachineConfig {
                name: "Remote 1".to_string(),
                ssh_host: Some("example.com".to_string()),
                ssh_user: Some("ubuntu".to_string()),
                ssh_key: None,
                ssh_port: 2222,
                enabled: true,
                collectors: std::collections::HashMap::new(),
                tags: vec!["builder".to_string()],
            },
        );

        registry.load_from_config(&config).unwrap();
        let machine = registry.get_machine("remote-1").unwrap().unwrap();

        assert_eq!(machine.hostname, "example.com");
        assert!(!machine.is_local);
        assert_eq!(machine.ssh_port, 2222);
        assert_eq!(machine.ssh_user.as_deref(), Some("ubuntu"));

        let local = registry.get_machine("local").unwrap().unwrap();
        assert!(local.is_local);
    }

    #[test]
    fn test_registry_set_enabled() {
        let store = Arc::new(VcStore::open_memory().unwrap());
        let registry = MachineRegistry::new(store);
        let config = VcConfig::default();

        registry.load_from_config(&config).unwrap();
        registry.set_enabled("local", false).unwrap();

        let machine = registry.get_machine("local").unwrap().unwrap();
        assert!(!machine.enabled);
    }

    #[test]
    fn test_machine_deserializes_stringified_tags_and_integer_flags() {
        let row = serde_json::json!({
            "machine_id": "remote-1",
            "hostname": "example.com",
            "ssh_port": 22,
            "is_local": 0,
            "status": "unknown",
            "tags": "[\"builder\",\"gpu\"]",
            "enabled": 1,
        });

        let machine: Machine = serde_json::from_value(row).unwrap();
        assert!(!machine.is_local);
        assert!(machine.enabled);
        assert_eq!(machine.tags, vec!["builder", "gpu"]);
    }
}
