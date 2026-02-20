//! `vc_config` - Configuration parsing and validation for Vibe Cockpit
//!
//! This crate provides:
//! - TOML configuration parsing
//! - Default value handling
//! - Environment variable overrides
//! - Path expansion (`~/` to home directory)
//! - Auto-discovery from standard config paths
//! - Machine inventory definitions
//! - Configuration linting with actionable suggestions
//! - Configuration wizard for generating new configs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;
use tracing::info;

/// Valid log level strings (trace, debug, info, warn, error)
const VALID_LOG_LEVELS: &[&str] = &["trace", "debug", "info", "warn", "error"];

// =============================================================================
// Lint Types
// =============================================================================

/// Severity level for lint issues
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LintSeverity {
    /// Critical issue that will prevent vc from working
    Error,
    /// Issue that may cause problems or suboptimal behavior
    Warning,
    /// Informational suggestion for improvement
    Info,
}

impl std::fmt::Display for LintSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LintSeverity::Error => write!(f, "error"),
            LintSeverity::Warning => write!(f, "warning"),
            LintSeverity::Info => write!(f, "info"),
        }
    }
}

/// A suggestion for fixing a lint issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintSuggestion {
    /// Human-readable description of the fix
    pub description: String,
    /// The config path to modify (e.g., `"machines.orko.ssh_user"`)
    pub path: String,
    /// Suggested new value (as TOML string)
    pub suggested_value: Option<String>,
}

/// A single lint issue found in the configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintIssue {
    /// Severity of the issue
    pub severity: LintSeverity,
    /// Config path where the issue was found (e.g., `"machines.orko.ssh_host"`)
    pub path: String,
    /// Human-readable message describing the issue
    pub message: String,
    /// Optional suggestion for fixing the issue
    pub suggestion: Option<LintSuggestion>,
}

impl LintIssue {
    /// Create an error-level lint issue
    pub fn error(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: LintSeverity::Error,
            path: path.into(),
            message: message.into(),
            suggestion: None,
        }
    }

    /// Create a warning-level lint issue
    pub fn warning(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: LintSeverity::Warning,
            path: path.into(),
            message: message.into(),
            suggestion: None,
        }
    }

    /// Create an info-level lint issue
    pub fn info(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: LintSeverity::Info,
            path: path.into(),
            message: message.into(),
            suggestion: None,
        }
    }

    /// Add a suggestion to this issue
    #[must_use]
    pub fn with_suggestion(mut self, suggestion: LintSuggestion) -> Self {
        self.suggestion = Some(suggestion);
        self
    }
}

/// Result of linting a configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LintResult {
    /// All issues found
    pub issues: Vec<LintIssue>,
    /// Number of errors
    pub error_count: usize,
    /// Number of warnings
    pub warning_count: usize,
    /// Number of info messages
    pub info_count: usize,
}

impl LintResult {
    /// Create an empty lint result
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an issue to the result
    pub fn add(&mut self, issue: LintIssue) {
        match issue.severity {
            LintSeverity::Error => self.error_count += 1,
            LintSeverity::Warning => self.warning_count += 1,
            LintSeverity::Info => self.info_count += 1,
        }
        self.issues.push(issue);
    }

    /// Check if there are any errors
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.error_count > 0
    }

    /// Check if there are any issues (errors or warnings)
    #[must_use]
    pub fn has_issues(&self) -> bool {
        !self.issues.is_empty()
    }

    /// Get issues filtered by severity
    pub fn by_severity(&self, severity: LintSeverity) -> impl Iterator<Item = &LintIssue> {
        self.issues.iter().filter(move |i| i.severity == severity)
    }
}

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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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

/// Global configuration settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GlobalConfig {
    /// Path to `DuckDB` database file
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
#[must_use]
pub fn expand_path(path: &Path) -> PathBuf {
    let path_str = path.to_string_lossy();
    if let Some(stripped) = path_str.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    } else if path_str == "~"
        && let Some(home) = dirs::home_dir()
    {
        return home;
    }
    path.to_path_buf()
}

/// Expand all paths in `GlobalConfig`
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

    /// SSH port (if remote)
    #[serde(default = "default_ssh_port")]
    pub ssh_port: u16,

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

fn default_ssh_port() -> u16 {
    22
}

/// Collector configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[allow(clippy::struct_excessive_bools)]
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

    /// Enable `mcp_agent_mail` collector
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

    /// Enable `bv_br` (beads) collector
    pub bv_br: bool,

    /// Enable afsc collector
    pub afsc: bool,

    /// Enable `cloud_benchmarker` collector
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
    #[must_use]
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

    /// Discover and load configuration from standard paths.
    ///
    /// Returns defaults if no config file is found.
    ///
    /// # Errors
    /// Returns a [`ConfigError`] if a discovered config file cannot be loaded.
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

    /// Discover config and apply environment variable overrides.
    ///
    /// # Errors
    /// Returns a [`ConfigError`] if config discovery or validation fails.
    pub fn discover_with_env() -> Result<Self, ConfigError> {
        let mut config = Self::discover()?;
        config.apply_env_overrides();
        config.expand_all_paths();
        Ok(config)
    }

    /// Load configuration from a specific TOML file.
    ///
    /// # Errors
    /// Returns a [`ConfigError`] if the file cannot be read, parsed, or validated.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let mut config: VcConfig = toml::from_str(&content)?;
        config.expand_all_paths();
        config.validate()?;
        Ok(config)
    }

    /// Load configuration with environment variable overrides.
    ///
    /// # Errors
    /// Returns a [`ConfigError`] if the file cannot be read, parsed, or validated.
    pub fn load_with_env(path: &Path) -> Result<Self, ConfigError> {
        let mut config = Self::load(path)?;
        config.apply_env_overrides();
        Ok(config)
    }

    /// Expand all paths in configuration (resolve `~/` to home directory)
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
        if let Ok(val) = std::env::var("VC_POLL_INTERVAL")
            && let Ok(secs) = val.parse()
        {
            self.global.poll_interval_secs = secs;
        }
        if let Ok(val) = std::env::var("VC_WEB_PORT")
            && let Ok(port) = val.parse()
        {
            self.web.port = port;
        }
        if let Ok(val) = std::env::var("VC_WEB_BIND") {
            self.web.bind_address = val;
        }
    }

    /// Validate configuration.
    ///
    /// # Errors
    /// Returns a [`ConfigError`] when validation rules are violated.
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
        if !VALID_LOG_LEVELS.contains(&self.global.log_level.to_lowercase().as_str()) {
            return Err(ConfigError::ValidationError(format!(
                "Invalid log_level '{}'. Must be one of: {}",
                self.global.log_level,
                VALID_LOG_LEVELS.join(", ")
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
                    "Machine '{id}' has ssh_host but missing ssh_user"
                )));
            }
        }

        Ok(())
    }

    /// Get poll interval as Duration
    #[must_use]
    pub fn poll_interval(&self) -> Duration {
        Duration::from_secs(self.global.poll_interval_secs)
    }

    /// Get collector timeout as Duration
    #[must_use]
    pub fn collector_timeout(&self) -> Duration {
        Duration::from_secs(self.collectors.timeout_secs)
    }

    /// Check if a machine is local (no SSH required)
    #[must_use]
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
    #[must_use]
    pub fn is_collector_enabled(&self, machine_id: &str, collector_name: &str) -> bool {
        // Check machine-specific override first
        if let Some(machine) = self.machines.get(machine_id)
            && let Some(&enabled) = machine.collectors.get(collector_name)
        {
            return enabled;
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

    /// Lint the configuration and return all issues found.
    ///
    /// This performs more extensive checks than `validate()`, including:
    /// - Validation errors (also caught by `validate()`)
    /// - Warnings about suboptimal configurations
    /// - Suggestions for improvements
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn lint(&self) -> LintResult {
        let mut result = LintResult::new();

        // === ERRORS ===

        // Poll interval must be positive
        if self.global.poll_interval_secs == 0 {
            result.add(
                LintIssue::error(
                    "global.poll_interval_secs",
                    "Poll interval must be greater than 0",
                )
                .with_suggestion(LintSuggestion {
                    description: "Set a reasonable poll interval (e.g., 120 seconds)".to_string(),
                    path: "global.poll_interval_secs".to_string(),
                    suggested_value: Some("120".to_string()),
                }),
            );
        }

        // Collector timeout must be positive
        if self.collectors.timeout_secs == 0 {
            result.add(
                LintIssue::error(
                    "collectors.timeout_secs",
                    "Collector timeout must be greater than 0",
                )
                .with_suggestion(LintSuggestion {
                    description: "Set a reasonable timeout (e.g., 30 seconds)".to_string(),
                    path: "collectors.timeout_secs".to_string(),
                    suggested_value: Some("30".to_string()),
                }),
            );
        }

        // Validate log level
        if !VALID_LOG_LEVELS.contains(&self.global.log_level.to_lowercase().as_str()) {
            result.add(
                LintIssue::error(
                    "global.log_level",
                    format!(
                        "Invalid log level '{}'. Must be one of: {}",
                        self.global.log_level,
                        VALID_LOG_LEVELS.join(", ")
                    ),
                )
                .with_suggestion(LintSuggestion {
                    description: "Use a valid log level".to_string(),
                    path: "global.log_level".to_string(),
                    suggested_value: Some("\"info\"".to_string()),
                }),
            );
        }

        // Autopilot confidence must be in [0, 1]
        if self.autopilot.min_confidence < 0.0 || self.autopilot.min_confidence > 1.0 {
            result.add(
                LintIssue::error(
                    "autopilot.min_confidence",
                    "Min confidence must be between 0.0 and 1.0",
                )
                .with_suggestion(LintSuggestion {
                    description: "Set confidence to a value between 0 and 1".to_string(),
                    path: "autopilot.min_confidence".to_string(),
                    suggested_value: Some("0.8".to_string()),
                }),
            );
        }

        // Web port must be positive
        if self.web.port == 0 {
            result.add(
                LintIssue::error("web.port", "Web port must be greater than 0").with_suggestion(
                    LintSuggestion {
                        description: "Set a valid port number".to_string(),
                        path: "web.port".to_string(),
                        suggested_value: Some("8080".to_string()),
                    },
                ),
            );
        }

        // Machine SSH validation
        for (id, machine) in &self.machines {
            let path_prefix = format!("machines.{id}");

            // SSH host without user
            if machine.ssh_host.is_some() && machine.ssh_user.is_none() {
                result.add(
                    LintIssue::error(
                        format!("{path_prefix}.ssh_user"),
                        format!("Machine '{id}' has ssh_host but missing ssh_user"),
                    )
                    .with_suggestion(LintSuggestion {
                        description: "Add SSH user for remote machine".to_string(),
                        path: format!("{path_prefix}.ssh_user"),
                        suggested_value: Some("\"ubuntu\"".to_string()),
                    }),
                );
            }

            // SSH key path doesn't exist
            if let Some(ref key_path) = machine.ssh_key {
                let expanded = expand_path(key_path);
                if !expanded.exists() {
                    result.add(LintIssue::error(
                        format!("{path_prefix}.ssh_key"),
                        format!("SSH key file does not exist: {}", expanded.display()),
                    ));
                }
            }
        }

        // === WARNINGS ===

        // Very short poll interval
        if self.global.poll_interval_secs > 0 && self.global.poll_interval_secs < 30 {
            result.add(LintIssue::warning(
                "global.poll_interval_secs",
                format!(
                    "Poll interval of {} seconds is very short and may cause high load",
                    self.global.poll_interval_secs
                ),
            ));
        }

        // Very long poll interval
        if self.global.poll_interval_secs > 600 {
            result.add(LintIssue::warning(
                "global.poll_interval_secs",
                format!(
                    "Poll interval of {} seconds is long; data may become stale",
                    self.global.poll_interval_secs
                ),
            ));
        }

        // Autopilot enabled with low confidence
        if self.autopilot.enabled && self.autopilot.min_confidence < 0.5 {
            result.add(LintIssue::warning(
                "autopilot.min_confidence",
                "Autopilot enabled with low confidence threshold; may take risky actions",
            ));
        }

        // Autopilot enabled but account switching disabled
        if self.autopilot.enabled && !self.autopilot.auto_switch_accounts {
            result.add(LintIssue::info(
                "autopilot.auto_switch_accounts",
                "Autopilot is enabled but account switching is disabled",
            ));
        }

        // Web enabled without CORS in production
        if self.web.enabled && !self.web.cors_enabled && self.web.bind_address != "127.0.0.1" {
            result.add(LintIssue::warning(
                "web.cors_enabled",
                "Web dashboard bound to non-localhost without CORS configuration",
            ));
        }

        // Alerts enabled but no delivery channels configured
        if self.alerts.enabled
            && self.alerts.webhook_url.is_none()
            && self.alerts.slack_webhook_url.is_none()
            && self.alerts.discord_webhook_url.is_none()
            && !self.alerts.desktop_notifications
        {
            result.add(LintIssue::warning(
                "alerts",
                "Alerts are enabled but no delivery channels are configured",
            ));
        }

        // === INFO ===

        // No machines defined
        if self.machines.is_empty() {
            result.add(LintIssue::info(
                "machines",
                "No machines defined; vc will only monitor local machine",
            ));
        }

        // All collectors disabled
        if !self.collectors.sysmoni
            && !self.collectors.ru
            && !self.collectors.caut
            && !self.collectors.caam
        {
            result.add(LintIssue::warning(
                "collectors",
                "All primary collectors are disabled; vc will collect minimal data",
            ));
        }

        // Debug logging in production
        if self.global.log_level.to_lowercase() == "trace"
            || self.global.log_level.to_lowercase() == "debug"
        {
            result.add(LintIssue::info(
                "global.log_level",
                "Debug/trace logging enabled; logs may grow large",
            ));
        }

        // Machine with no tags
        for (id, machine) in &self.machines {
            if machine.tags.is_empty() {
                result.add(LintIssue::info(
                    format!("machines.{id}.tags"),
                    format!("Machine '{id}' has no tags; consider adding tags for filtering"),
                ));
            }
        }

        result
    }

    /// Generate a minimal default configuration as TOML string.
    #[must_use]
    pub fn generate_default_toml() -> String {
        r#"# Vibe Cockpit Configuration
# Generated by vc config wizard

[global]
# Path to DuckDB database (default: ~/.local/share/vc/vc.duckdb)
# db_path = "~/.local/share/vc/vc.duckdb"

# Poll interval in seconds (default: 120)
poll_interval_secs = 120

# Log level: trace, debug, info, warn, error (default: info)
log_level = "info"

[collectors]
# Enable/disable individual collectors
sysmoni = true          # System metrics (CPU, memory, disk, network)
ru = true               # Repo updater status
caut = true             # Claude usage tracking
caam = true             # Claude account management
cass = true             # Claude session search
mcp_agent_mail = true   # MCP Agent Mail
ntm = true              # Named Tmux Manager
rch = true              # Remote Compilation Helper
rano = true             # Network observer
dcg = true              # Dangerous command guard
pt = true               # Process tracker
bv_br = true            # Beads (issue tracker)

# Collector timeout in seconds
timeout_secs = 30

[alerts]
enabled = true
default_cooldown_secs = 300
# webhook_url = "https://example.com/webhook"
# slack_webhook_url = "https://hooks.slack.com/services/..."
desktop_notifications = false

[autopilot]
enabled = false
min_confidence = 0.8
auto_switch_accounts = false
switch_threshold = 0.75
preemptive_mins = 15

[tui]
refresh_ms = 1000
mouse_support = true
theme = "default"
show_charts = true

[web]
enabled = false
bind_address = "127.0.0.1"
port = 8080

# Machine inventory (uncomment and customize for remote monitoring)
# [machines.local]
# name = "Local Machine"
# enabled = true
# tags = ["primary"]

# [machines.remote-server]
# name = "Remote Server"
# ssh_host = "192.168.1.100"
# ssh_user = "ubuntu"
# ssh_port = 22
# # ssh_key = "~/.ssh/id_ed25519"
# enabled = true
# tags = ["worker", "builder"]
"#
        .to_string()
    }

    /// Generate a TOML representation of this config.
    ///
    /// # Errors
    /// Returns an error if serialization fails.
    pub fn to_toml(&self) -> Result<String, ConfigError> {
        toml::to_string_pretty(self)
            .map_err(|e| ConfigError::ValidationError(format!("Failed to serialize config: {e}")))
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
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("poll_interval_secs")
        );
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
                ssh_port: 22,
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
                ssh_port: 22,
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
                ssh_port: 22,
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
                ssh_port: 22,
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
                ssh_port: 22,
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
                ssh_port: 22,
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
        assert_eq!(config.poll_interval(), Duration::from_mins(2));
        assert_eq!(config.collector_timeout(), Duration::from_secs(30));
    }

    // =============================================================================
    // Lint Tests
    // =============================================================================

    #[test]
    fn test_lint_valid_config() {
        let config = VcConfig::default();
        let result = config.lint();
        // Default config should have no errors
        assert!(!result.has_errors());
        // But may have info (no machines defined)
        assert!(
            result
                .issues
                .iter()
                .any(|i| i.severity == LintSeverity::Info)
        );
    }

    #[test]
    fn test_lint_poll_interval_zero() {
        let mut config = VcConfig::default();
        config.global.poll_interval_secs = 0;
        let result = config.lint();
        assert!(result.has_errors());
        assert!(result.error_count == 1);
        assert!(
            result
                .issues
                .iter()
                .any(|i| i.path == "global.poll_interval_secs")
        );
    }

    #[test]
    fn test_lint_poll_interval_too_short() {
        let mut config = VcConfig::default();
        config.global.poll_interval_secs = 10;
        let result = config.lint();
        assert!(!result.has_errors());
        assert!(result.warning_count >= 1);
        assert!(
            result
                .issues
                .iter()
                .any(|i| i.path == "global.poll_interval_secs"
                    && i.severity == LintSeverity::Warning)
        );
    }

    #[test]
    fn test_lint_invalid_log_level() {
        let mut config = VcConfig::default();
        config.global.log_level = "invalid".to_string();
        let result = config.lint();
        assert!(result.has_errors());
        assert!(result.issues.iter().any(|i| i.path == "global.log_level"));
    }

    #[test]
    fn test_lint_machine_ssh_missing_user() {
        let mut config = VcConfig::default();
        config.machines.insert(
            "broken".to_string(),
            MachineConfig {
                name: "Broken".to_string(),
                ssh_host: Some("example.com".to_string()),
                ssh_user: None,
                ssh_key: None,
                ssh_port: 22,
                enabled: true,
                collectors: HashMap::new(),
                tags: vec![],
            },
        );
        let result = config.lint();
        assert!(result.has_errors());
        assert!(result.issues.iter().any(|i| i.path.contains("ssh_user")));
    }

    #[test]
    fn test_lint_alerts_no_channels() {
        let mut config = VcConfig::default();
        config.alerts.enabled = true;
        config.alerts.webhook_url = None;
        config.alerts.slack_webhook_url = None;
        config.alerts.discord_webhook_url = None;
        config.alerts.desktop_notifications = false;
        let result = config.lint();
        // Should warn about no delivery channels
        assert!(
            result
                .issues
                .iter()
                .any(|i| i.path == "alerts" && i.severity == LintSeverity::Warning)
        );
    }

    #[test]
    fn test_lint_autopilot_low_confidence() {
        let mut config = VcConfig::default();
        config.autopilot.enabled = true;
        config.autopilot.min_confidence = 0.3;
        let result = config.lint();
        // Should warn about low confidence
        assert!(
            result.issues.iter().any(
                |i| i.path == "autopilot.min_confidence" && i.severity == LintSeverity::Warning
            )
        );
    }

    #[test]
    fn test_lint_suggestion() {
        let mut config = VcConfig::default();
        config.global.poll_interval_secs = 0;
        let result = config.lint();
        let issue = result
            .issues
            .iter()
            .find(|i| i.path == "global.poll_interval_secs")
            .expect("should find poll interval issue");
        assert!(issue.suggestion.is_some());
        let suggestion = issue.suggestion.as_ref().unwrap();
        assert_eq!(suggestion.suggested_value, Some("120".to_string()));
    }

    #[test]
    fn test_lint_result_counts() {
        let mut result = LintResult::new();
        result.add(LintIssue::error("path1", "error"));
        result.add(LintIssue::warning("path2", "warning"));
        result.add(LintIssue::info("path3", "info"));
        result.add(LintIssue::error("path4", "error2"));

        assert_eq!(result.error_count, 2);
        assert_eq!(result.warning_count, 1);
        assert_eq!(result.info_count, 1);
        assert_eq!(result.issues.len(), 4);
        assert!(result.has_errors());
        assert!(result.has_issues());
    }

    #[test]
    fn test_generate_default_toml() {
        let toml = VcConfig::generate_default_toml();
        assert!(toml.contains("[global]"));
        assert!(toml.contains("[collectors]"));
        assert!(toml.contains("[alerts]"));
        assert!(toml.contains("[autopilot]"));
        assert!(toml.contains("[tui]"));
        assert!(toml.contains("[web]"));
        assert!(toml.contains("poll_interval_secs"));
    }

    #[test]
    fn test_config_to_toml() {
        let config = VcConfig::default();
        let toml = config.to_toml().expect("should serialize");
        assert!(toml.contains("[global]"));
        assert!(toml.contains("poll_interval_secs = 120"));
    }
}
