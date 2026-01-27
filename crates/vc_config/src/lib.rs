//! vc_config - Configuration parsing and validation for Vibe Cockpit
//!
//! This crate provides:
//! - TOML configuration parsing
//! - Default value handling
//! - Environment variable overrides
//! - Path expansion (~/ to home directory)
//! - Auto-discovery from standard config paths
//! - Machine inventory definitions

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;
use tracing::info;

/// Configuration errors
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    ReadError(#[from] std::io::Error),

    #[error("Failed to parse TOML: {0}")]
    ParseError(#[from] toml::de::Error),

    #[error("Invalid configuration: {0}")]
    ValidationError(String),

    #[error("Missing required field: {0}")]
    MissingField(String),
}

/// Top-level configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VcConfig {
    /// Global settings
    pub global: GlobalConfig,

    /// Machine inventory
    pub machines: HashMap<String, MachineConfig>,

    /// Collector settings
    pub collectors: CollectorConfig,

    /// Alert settings
    pub alerts: AlertConfig,

    /// Autopilot settings
    pub autopilot: AutopilotConfig,

    /// TUI settings
    pub tui: TuiConfig,

    /// Web dashboard settings
    pub web: WebConfig,
}

impl Default for VcConfig {
    fn default() -> Self {
        Self {
            global: GlobalConfig::default(),
            machines: HashMap::new(),
            collectors: CollectorConfig::default(),
            alerts: AlertConfig::default(),
            autopilot: AutopilotConfig::default(),
            tui: TuiConfig::default(),
            web: WebConfig::default(),
        }
    }
}

/// Global configuration settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GlobalConfig {
    /// Path to DuckDB database file
    pub db_path: PathBuf,

    /// Default poll interval in seconds
    pub poll_interval_secs: u64,

    /// Log level (trace, debug, info, warn, error)
    pub log_level: String,

    /// Enable JSON logging
    pub json_logs: bool,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            db_path: default_db_path(),
            poll_interval_secs: 120,
            log_level: "info".to_string(),
            json_logs: false,
        }
    }
}

/// Default database path using XDG directories
fn default_db_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("vc")
        .join("vc.duckdb")
}

/// Expand tilde in path to home directory
pub fn expand_path(path: &Path) -> PathBuf {
    let path_str = path.to_string_lossy();
    if path_str.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(&path_str[2..]);
        }
    } else if path_str == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    path.to_path_buf()
}

/// Expand all paths in GlobalConfig
impl GlobalConfig {
    pub fn expand_paths(&mut self) {
        self.db_path = expand_path(&self.db_path);
    }
}

/// Machine configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineConfig {
    /// Display name
    pub name: String,

    /// SSH hostname (if remote)
    pub ssh_host: Option<String>,

    /// SSH user (if remote)
    pub ssh_user: Option<String>,

    /// SSH key path (if remote)
    pub ssh_key: Option<PathBuf>,

    /// Whether this machine is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Machine-specific collector overrides
    #[serde(default)]
    pub collectors: HashMap<String, bool>,

    /// Tags for filtering
    #[serde(default)]
    pub tags: Vec<String>,
}

fn default_true() -> bool {
    true
}

/// Collector configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CollectorConfig {
    /// Enable sysmoni collector
    pub sysmoni: bool,

    /// Enable ru (repo updater) collector
    pub ru: bool,

    /// Enable caut (usage tracker) collector
    pub caut: bool,

    /// Enable caam (account manager) collector
    pub caam: bool,

    /// Enable cass (session search) collector
    pub cass: bool,

    /// Enable mcp_agent_mail collector
    pub mcp_agent_mail: bool,

    /// Enable ntm collector
    pub ntm: bool,

    /// Enable rch collector
    pub rch: bool,

    /// Enable rano collector
    pub rano: bool,

    /// Enable dcg collector
    pub dcg: bool,

    /// Enable pt collector
    pub pt: bool,

    /// Enable bv_br (beads) collector
    pub bv_br: bool,

    /// Enable afsc collector
    pub afsc: bool,

    /// Enable cloud_benchmarker collector
    pub cloud_benchmarker: bool,

    /// Collector timeout in seconds
    pub timeout_secs: u64,
}

impl Default for CollectorConfig {
    fn default() -> Self {
        Self {
            sysmoni: true,
            ru: true,
            caut: true,
            caam: true,
            cass: true,
            mcp_agent_mail: true,
            ntm: true,
            rch: true,
            rano: true,
            dcg: true,
            pt: true,
            bv_br: true,
            afsc: false,
            cloud_benchmarker: false,
            timeout_secs: 30,
        }
    }
}

/// Alert configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AlertConfig {
    /// Enable alerting
    pub enabled: bool,

    /// Default cooldown between duplicate alerts (seconds)
    pub default_cooldown_secs: u64,

    /// Webhook URL for alerts
    pub webhook_url: Option<String>,

    /// Slack webhook URL
    pub slack_webhook_url: Option<String>,

    /// Discord webhook URL
    pub discord_webhook_url: Option<String>,

    /// Enable desktop notifications
    pub desktop_notifications: bool,
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_cooldown_secs: 300,
            webhook_url: None,
            slack_webhook_url: None,
            discord_webhook_url: None,
            desktop_notifications: false,
        }
    }
}

/// Autopilot configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AutopilotConfig {
    /// Enable autopilot mode
    pub enabled: bool,

    /// Minimum confidence for automatic actions
    pub min_confidence: f64,

    /// Enable automatic account switching
    pub auto_switch_accounts: bool,

    /// Usage threshold for account switching (percentage)
    pub switch_threshold: f64,

    /// Minutes before predicted limit to switch
    pub preemptive_mins: u32,

    /// Enable automatic workload balancing
    pub auto_balance_workload: bool,

    /// CPU threshold for overload detection
    pub cpu_overload_threshold: f64,

    /// Daily cost budget (for alerts)
    pub daily_budget: Option<f64>,
}

impl Default for AutopilotConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            min_confidence: 0.8,
            auto_switch_accounts: false,
            switch_threshold: 0.75,
            preemptive_mins: 15,
            auto_balance_workload: false,
            cpu_overload_threshold: 80.0,
            daily_budget: None,
        }
    }
}

/// TUI configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TuiConfig {
    /// Refresh interval in milliseconds
    pub refresh_ms: u64,

    /// Enable mouse support
    pub mouse_support: bool,

    /// Color theme
    pub theme: String,

    /// Show mini-charts in overview
    pub show_charts: bool,
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            refresh_ms: 1000,
            mouse_support: true,
            theme: "default".to_string(),
            show_charts: true,
        }
    }
}

/// Web dashboard configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WebConfig {
    /// Enable web dashboard
    pub enabled: bool,

    /// Bind address
    pub bind_address: String,

    /// Port
    pub port: u16,

    /// Enable CORS
    pub cors_enabled: bool,

    /// Allowed origins for CORS
    pub cors_origins: Vec<String>,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bind_address: "127.0.0.1".to_string(),
            port: 8080,
            cors_enabled: false,
            cors_origins: vec![],
        }
    }
}

impl VcConfig {
    /// Standard config file paths, in order of precedence
    pub fn config_paths() -> Vec<PathBuf> {
        let mut paths = vec![
            // 1. Current directory (project-local)
            PathBuf::from("vc.toml"),
        ];

        // 2. User config directory (~/.config/vc/vc.toml)
        if let Some(config_dir) = dirs::config_dir() {
            paths.push(config_dir.join("vc").join("vc.toml"));
        }

        // 3. System config
        paths.push(PathBuf::from("/etc/vc/vc.toml"));

        paths
    }

    /// Discover and load configuration from standard paths
    /// Returns defaults if no config file is found
    pub fn discover() -> Result<Self, ConfigError> {
        for path in Self::config_paths() {
            if path.exists() {
                info!(path = %path.display(), "Loading config from");
                return Self::load(&path);
            }
        }

        info!("No config file found, using defaults");
        Ok(Self::default())
    }

    /// Discover config and apply environment variable overrides
    pub fn discover_with_env() -> Result<Self, ConfigError> {
        let mut config = Self::discover()?;
        config.apply_env_overrides();
        config.expand_all_paths();
        Ok(config)
    }

    /// Load configuration from a specific TOML file
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let mut config: VcConfig = toml::from_str(&content)?;
        config.expand_all_paths();
        config.validate()?;
        Ok(config)
    }

    /// Load configuration with environment variable overrides
    pub fn load_with_env(path: &Path) -> Result<Self, ConfigError> {
        let mut config = Self::load(path)?;
        config.apply_env_overrides();
        Ok(config)
    }

    /// Expand all paths in configuration (resolve ~/ to home directory)
    pub fn expand_all_paths(&mut self) {
        self.global.expand_paths();

        // Expand SSH key paths for all machines
        for machine in self.machines.values_mut() {
            if let Some(ref mut key_path) = machine.ssh_key {
                *key_path = expand_path(key_path);
            }
        }
    }

    /// Apply environment variable overrides
    fn apply_env_overrides(&mut self) {
        if let Ok(val) = std::env::var("VC_DB_PATH") {
            self.global.db_path = expand_path(&PathBuf::from(val));
        }
        if let Ok(val) = std::env::var("VC_LOG_LEVEL") {
            self.global.log_level = val;
        }
        if let Ok(val) = std::env::var("VC_POLL_INTERVAL") {
            if let Ok(secs) = val.parse() {
                self.global.poll_interval_secs = secs;
            }
        }
        if let Ok(val) = std::env::var("VC_WEB_PORT") {
            if let Ok(port) = val.parse() {
                self.web.port = port;
            }
        }
        if let Ok(val) = std::env::var("VC_WEB_BIND") {
            self.web.bind_address = val;
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Validate poll interval
        if self.global.poll_interval_secs == 0 {
            return Err(ConfigError::ValidationError(
                "poll_interval_secs must be > 0".to_string(),
            ));
        }

        // Validate collector timeout
        if self.collectors.timeout_secs == 0 {
            return Err(ConfigError::ValidationError(
                "collector timeout_secs must be > 0".to_string(),
            ));
        }

        // Validate log level
        let valid_levels = ["trace", "debug", "info", "warn", "error"];
        if !valid_levels.contains(&self.global.log_level.to_lowercase().as_str()) {
            return Err(ConfigError::ValidationError(format!(
                "Invalid log_level '{}'. Must be one of: {}",
                self.global.log_level,
                valid_levels.join(", ")
            )));
        }

        // Validate autopilot thresholds
        if self.autopilot.min_confidence < 0.0 || self.autopilot.min_confidence > 1.0 {
            return Err(ConfigError::ValidationError(
                "autopilot.min_confidence must be between 0.0 and 1.0".to_string(),
            ));
        }

        // Validate web port
        if self.web.port == 0 {
            return Err(ConfigError::ValidationError(
                "web.port must be > 0".to_string(),
            ));
        }

        // Validate machine configurations
        for (id, machine) in &self.machines {
            if machine.ssh_host.is_some() && machine.ssh_user.is_none() {
                return Err(ConfigError::ValidationError(format!(
                    "Machine '{}' has ssh_host but missing ssh_user",
                    id
                )));
            }
        }

        Ok(())
    }

    /// Get poll interval as Duration
    pub fn poll_interval(&self) -> Duration {
        Duration::from_secs(self.global.poll_interval_secs)
    }

    /// Get collector timeout as Duration
    pub fn collector_timeout(&self) -> Duration {
        Duration::from_secs(self.collectors.timeout_secs)
    }

    /// Check if a machine is local (no SSH required)
    pub fn is_local_machine(&self, machine_id: &str) -> bool {
        self.machines
            .get(machine_id)
            .is_none_or(|m| m.ssh_host.is_none())
    }

    /// Get enabled machines
    pub fn enabled_machines(&self) -> impl Iterator<Item = (&String, &MachineConfig)> {
        self.machines.iter().filter(|(_, m)| m.enabled)
    }

    /// Check if a collector is enabled for a specific machine
    pub fn is_collector_enabled(&self, machine_id: &str, collector_name: &str) -> bool {
        // Check machine-specific override first
        if let Some(machine) = self.machines.get(machine_id) {
            if let Some(&enabled) = machine.collectors.get(collector_name) {
                return enabled;
            }
        }

        // Fall back to global collector config
        match collector_name {
            "sysmoni" => self.collectors.sysmoni,
            "ru" => self.collectors.ru,
            "caut" => self.collectors.caut,
            "caam" => self.collectors.caam,
            "cass" => self.collectors.cass,
            "mcp_agent_mail" => self.collectors.mcp_agent_mail,
            "ntm" => self.collectors.ntm,
            "rch" => self.collectors.rch,
            "rano" => self.collectors.rano,
            "dcg" => self.collectors.dcg,
            "pt" => self.collectors.pt,
            "bv_br" => self.collectors.bv_br,
            "afsc" => self.collectors.afsc,
            "cloud_benchmarker" => self.collectors.cloud_benchmarker,
            _ => false, // Unknown collectors are disabled
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_default_config() {
        let config = VcConfig::default();
        assert!(config.collectors.sysmoni);
        assert_eq!(config.global.poll_interval_secs, 120);
        assert_eq!(config.global.log_level, "info");
    }

    #[test]
    fn test_config_validation_poll_interval() {
        let mut config = VcConfig::default();
        config.global.poll_interval_secs = 0;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("poll_interval_secs"));
    }

    #[test]
    fn test_config_validation_collector_timeout() {
        let mut config = VcConfig::default();
        config.collectors.timeout_secs = 0;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("timeout_secs"));
    }

    #[test]
    fn test_config_validation_log_level() {
        let mut config = VcConfig::default();
        config.global.log_level = "invalid".to_string();
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("log_level"));
    }

    #[test]
    fn test_config_validation_autopilot_confidence() {
        let mut config = VcConfig::default();
        config.autopilot.min_confidence = 1.5;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("min_confidence"));
    }

    #[test]
    fn test_config_validation_machine_ssh() {
        let mut config = VcConfig::default();
        config.machines.insert(
            "test".to_string(),
            MachineConfig {
                name: "Test".to_string(),
                ssh_host: Some("example.com".to_string()),
                ssh_user: None, // Missing user
                ssh_key: None,
                enabled: true,
                collectors: HashMap::new(),
                tags: vec![],
            },
        );
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("ssh_user"));
    }

    #[test]
    fn test_path_expansion_tilde() {
        let path = PathBuf::from("~/test/path");
        let expanded = expand_path(&path);
        if let Some(home) = dirs::home_dir() {
            assert_eq!(expanded, home.join("test/path"));
        }
    }

    #[test]
    fn test_path_expansion_no_tilde() {
        let path = PathBuf::from("/absolute/path");
        let expanded = expand_path(&path);
        assert_eq!(expanded, PathBuf::from("/absolute/path"));
    }

    #[test]
    fn test_path_expansion_just_tilde() {
        let path = PathBuf::from("~");
        let expanded = expand_path(&path);
        if let Some(home) = dirs::home_dir() {
            assert_eq!(expanded, home);
        }
    }

    #[test]
    fn test_load_from_toml() {
        let toml_content = r#"
[global]
db_path = "/tmp/test.duckdb"
poll_interval_secs = 60
log_level = "debug"

[collectors]
sysmoni = false
timeout_secs = 15

[machines.test-machine]
name = "Test"
enabled = true
"#;

        let dir = std::env::temp_dir();
        let path = dir.join("vc_test_config.toml");
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(toml_content.as_bytes()).unwrap();

        let config = VcConfig::load(&path).unwrap();
        assert_eq!(config.global.poll_interval_secs, 60);
        assert_eq!(config.global.log_level, "debug");
        assert!(!config.collectors.sysmoni);
        assert_eq!(config.collectors.timeout_secs, 15);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_is_collector_enabled() {
        let mut config = VcConfig::default();

        // Global enabled
        assert!(config.is_collector_enabled("any-machine", "sysmoni"));

        // Global disabled
        assert!(!config.is_collector_enabled("any-machine", "afsc"));

        // Machine-specific override
        let mut collectors = HashMap::new();
        collectors.insert("sysmoni".to_string(), false);
        collectors.insert("afsc".to_string(), true);

        config.machines.insert(
            "override-machine".to_string(),
            MachineConfig {
                name: "Override".to_string(),
                ssh_host: None,
                ssh_user: None,
                ssh_key: None,
                enabled: true,
                collectors,
                tags: vec![],
            },
        );

        // Override disables global-enabled collector
        assert!(!config.is_collector_enabled("override-machine", "sysmoni"));
        // Override enables global-disabled collector
        assert!(config.is_collector_enabled("override-machine", "afsc"));
    }

    #[test]
    fn test_is_local_machine() {
        let mut config = VcConfig::default();

        // Unknown machine is considered local
        assert!(config.is_local_machine("unknown"));

        // Machine without SSH is local
        config.machines.insert(
            "local".to_string(),
            MachineConfig {
                name: "Local".to_string(),
                ssh_host: None,
                ssh_user: None,
                ssh_key: None,
                enabled: true,
                collectors: HashMap::new(),
                tags: vec![],
            },
        );
        assert!(config.is_local_machine("local"));

        // Machine with SSH is remote
        config.machines.insert(
            "remote".to_string(),
            MachineConfig {
                name: "Remote".to_string(),
                ssh_host: Some("example.com".to_string()),
                ssh_user: Some("user".to_string()),
                ssh_key: None,
                enabled: true,
                collectors: HashMap::new(),
                tags: vec![],
            },
        );
        assert!(!config.is_local_machine("remote"));
    }

    #[test]
    fn test_enabled_machines() {
        let mut config = VcConfig::default();

        config.machines.insert(
            "enabled1".to_string(),
            MachineConfig {
                name: "Enabled 1".to_string(),
                ssh_host: None,
                ssh_user: None,
                ssh_key: None,
                enabled: true,
                collectors: HashMap::new(),
                tags: vec![],
            },
        );

        config.machines.insert(
            "disabled".to_string(),
            MachineConfig {
                name: "Disabled".to_string(),
                ssh_host: None,
                ssh_user: None,
                ssh_key: None,
                enabled: false,
                collectors: HashMap::new(),
                tags: vec![],
            },
        );

        let enabled: Vec<_> = config.enabled_machines().collect();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].0, "enabled1");
    }

    #[test]
    fn test_config_paths() {
        let paths = VcConfig::config_paths();
        assert!(!paths.is_empty());
        assert_eq!(paths[0], PathBuf::from("vc.toml"));
    }

    #[test]
    fn test_durations() {
        let config = VcConfig::default();
        assert_eq!(config.poll_interval(), Duration::from_secs(120));
        assert_eq!(config.collector_timeout(), Duration::from_secs(30));
    }
}
