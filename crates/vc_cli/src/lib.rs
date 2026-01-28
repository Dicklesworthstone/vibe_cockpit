//! vc_cli - CLI commands for Vibe Cockpit
//!
//! This crate provides:
//! - clap-based command definitions
//! - Robot mode output formatting (JSON envelope)
//! - TOON output support
//! - All subcommands (status, tui, daemon, robot, etc.)

use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use vc_collect::executor::Executor;
use vc_config::VcConfig;
use vc_store::{AuditEventFilter, AuditEventType, VcStore};

pub mod robot;

pub use robot::{HealthData, RobotEnvelope, StatusData, TriageData};

/// CLI errors
#[derive(Error, Debug)]
pub enum CliError {
    #[error("Command failed: {0}")]
    CommandFailed(String),

    #[error("Config error: {0}")]
    ConfigError(#[from] vc_config::ConfigError),

    #[error("Store error: {0}")]
    StoreError(#[from] vc_store::StoreError),

    #[error("Query error: {0}")]
    QueryError(#[from] vc_query::QueryError),

    #[error("Validation error: {0}")]
    ValidationError(#[from] vc_query::ValidationError),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Output format for robot mode
#[derive(Debug, Clone, Copy, ValueEnum, Serialize, Deserialize)]
pub enum OutputFormat {
    /// Standard JSON output
    Json,
    /// Token-optimized output (TOON)
    Toon,
    /// Human-readable text
    Text,
}

/// Main CLI application
#[derive(Parser, Debug)]
#[command(name = "vc")]
#[command(
    author,
    version,
    about = "Vibe Cockpit - Agent fleet monitoring and orchestration"
)]
pub struct Cli {
    /// Configuration file path
    #[arg(short, long, global = true)]
    pub config: Option<std::path::PathBuf>,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Output format for commands
    #[arg(long, global = true, default_value = "text")]
    pub format: OutputFormat,

    #[command(subcommand)]
    pub command: Commands,
}

/// Available commands
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Start the TUI dashboard
    Tui,

    /// Run the daemon (poll loop)
    Daemon {
        /// Run in foreground
        #[arg(short, long)]
        foreground: bool,
    },

    /// Show current status
    Status {
        /// Machine to show status for
        #[arg(short, long)]
        machine: Option<String>,
    },

    /// Robot mode commands for agent consumption
    Robot {
        #[command(subcommand)]
        command: RobotCommands,
    },

    /// Watch for events (streaming mode)
    Watch {
        /// Event types to watch
        #[arg(short, long)]
        events: Option<Vec<String>>,

        /// Only show changes
        #[arg(long)]
        changes_only: bool,
    },

    /// Collector management
    Collect {
        /// Run specific collector
        #[arg(long)]
        collector: Option<String>,

        /// Target machine
        #[arg(short, long)]
        machine: Option<String>,
    },

    /// Alert management
    Alert {
        #[command(subcommand)]
        command: AlertCommands,
    },

    /// Guardian management
    Guardian {
        #[command(subcommand)]
        command: GuardianCommands,
    },

    /// Fleet management
    Fleet {
        #[command(subcommand)]
        command: FleetCommands,
    },

    /// Run vacuum (retention policies)
    Vacuum {
        /// Dry run - show what would be deleted
        #[arg(long)]
        dry_run: bool,

        /// Specific table to vacuum
        #[arg(long)]
        table: Option<String>,
    },

    /// Start web dashboard server
    Web {
        /// Port to listen on
        #[arg(short, long, default_value = "8080")]
        port: u16,

        /// Address to bind to
        #[arg(short, long, default_value = "127.0.0.1")]
        bind: String,
    },

    /// Audit trail queries
    Audit {
        #[command(subcommand)]
        command: AuditCommands,
    },

    /// Machine inventory management
    Machines {
        #[command(subcommand)]
        command: MachineCommands,
    },

    /// Query the database with guardrails
    Query {
        #[command(subcommand)]
        command: QueryCommands,
    },
}

/// Query subcommands
#[derive(Subcommand, Debug)]
pub enum QueryCommands {
    /// Run a raw SQL query (SELECT only)
    Raw {
        /// SQL query to execute
        sql: String,

        /// Maximum rows to return
        #[arg(long, default_value = "1000")]
        limit: usize,
    },

    /// Run a safe template query
    Template {
        /// Template name
        name: String,

        /// Parameters in key=value format
        #[arg(short, long)]
        param: Vec<String>,
    },

    /// List available templates
    Templates,
}

/// Robot mode subcommands
#[derive(Subcommand, Debug)]
pub enum RobotCommands {
    /// Get fleet health status
    Health,

    /// Get triage recommendations
    Triage,

    /// Get comprehensive fleet status (machines, repos, alerts)
    Status,

    /// Get account status
    Accounts,

    /// Get predictions from Oracle
    Oracle,

    /// Get machine status
    Machines,

    /// Get repository status
    Repos,
}

/// Alert subcommands
#[derive(Subcommand, Debug)]
pub enum AlertCommands {
    /// List alerts
    List {
        /// Show only unacknowledged
        #[arg(long)]
        unacked: bool,
    },

    /// Acknowledge an alert
    Ack {
        /// Alert ID
        id: i64,
    },

    /// Show alert rules
    Rules,
}

/// Guardian subcommands
#[derive(Subcommand, Debug)]
pub enum GuardianCommands {
    /// List playbooks
    Playbooks,

    /// Show playbook runs
    Runs,

    /// Trigger a playbook manually
    Trigger {
        /// Playbook ID
        playbook_id: String,
    },

    /// Approve a pending playbook
    Approve {
        /// Run ID
        run_id: i64,
    },
}

/// Fleet subcommands
#[derive(Subcommand, Debug)]
pub enum FleetCommands {
    /// Spawn new agents
    Spawn {
        /// Agent type
        #[arg(long)]
        agent_type: String,

        /// Count to spawn
        #[arg(long, default_value = "1")]
        count: u32,

        /// Target machine
        #[arg(long)]
        machine: String,
    },

    /// Rebalance workload
    Rebalance {
        /// Rebalance strategy
        #[arg(long, default_value = "even-load")]
        strategy: String,
    },

    /// Emergency stop
    EmergencyStop {
        /// Scope (machine:name, all, agent-type:name)
        #[arg(long)]
        scope: String,

        /// Reason for stop
        #[arg(long)]
        reason: String,

        /// Force without confirmation
        #[arg(long)]
        force: bool,
    },

    /// Migrate workload
    Migrate {
        /// Source machine
        #[arg(long)]
        from: String,

        /// Destination machine
        #[arg(long)]
        to: String,

        /// Workload pattern
        #[arg(long)]
        workload: Option<String>,
    },
}

/// Audit trail subcommands
#[derive(Subcommand, Debug)]
pub enum AuditCommands {
    /// List audit events
    List {
        /// Filter by event type
        #[arg(long)]
        event_type: Option<String>,

        /// Filter by machine ID
        #[arg(long)]
        machine: Option<String>,

        /// Filter by RFC3339 timestamp (inclusive)
        #[arg(long)]
        since: Option<String>,

        /// Limit number of events returned
        #[arg(long, default_value = "100")]
        limit: usize,
    },

    /// Show audit event details by ID
    Show {
        /// Audit event ID
        id: i64,
    },
}

/// Machine management subcommands
#[derive(Subcommand, Debug)]
pub enum MachineCommands {
    /// List all registered machines
    List {
        /// Filter by status (online, offline, unknown)
        #[arg(long)]
        status: Option<String>,

        /// Filter by tags (comma-separated)
        #[arg(long)]
        tags: Option<String>,

        /// Show only enabled machines
        #[arg(long)]
        enabled: Option<bool>,
    },

    /// Show details for a specific machine
    Show {
        /// Machine ID
        id: String,
    },

    /// Add a new machine
    Add {
        /// Machine ID
        id: String,

        /// SSH connection string (user@host)
        #[arg(long)]
        ssh: Option<String>,

        /// SSH port
        #[arg(long, default_value = "22")]
        port: u16,

        /// Tags (comma-separated)
        #[arg(long)]
        tags: Option<String>,
    },

    /// Probe a machine for available tools
    Probe {
        /// Machine ID
        id: String,
    },

    /// Update machine status
    Enable {
        /// Machine ID
        id: String,

        /// Enable or disable
        #[arg(long)]
        enabled: bool,
    },
}

impl Cli {
    /// Run the CLI
    pub async fn run(self) -> Result<(), CliError> {
        match self.command {
            Commands::Tui => {
                println!("Starting TUI...");
                // TUI implementation will go here
            }
            Commands::Status { machine } => {
                println!(
                    "Status for {:?}",
                    machine.unwrap_or_else(|| "all".to_string())
                );
                // Status implementation will go here
            }
            Commands::Robot { command } => {
                // Robot mode output - always JSON for robot commands
                match command {
                    RobotCommands::Health => {
                        let output = robot::robot_health();
                        println!("{}", output.to_json_pretty());
                    }
                    RobotCommands::Triage => {
                        let output = robot::robot_triage();
                        println!("{}", output.to_json_pretty());
                    }
                    RobotCommands::Status => {
                        let output = robot::robot_status();
                        println!("{}", output.to_json_pretty());
                    }
                    RobotCommands::Accounts => {
                        let output = robot::RobotEnvelope::new(
                            "vc.robot.accounts.v1",
                            serde_json::json!({ "accounts": [], "warning": "not yet implemented" }),
                        );
                        println!("{}", output.to_json_pretty());
                    }
                    RobotCommands::Oracle => {
                        let output = robot::RobotEnvelope::new(
                            "vc.robot.oracle.v1",
                            serde_json::json!({ "predictions": [], "warning": "not yet implemented" }),
                        );
                        println!("{}", output.to_json_pretty());
                    }
                    RobotCommands::Machines => {
                        let store = Arc::new(open_store(self.config.as_ref())?);
                        let config = match &self.config {
                            Some(path) => VcConfig::load_with_env(path)?,
                            None => VcConfig::discover_with_env()?,
                        };
                        let registry = vc_collect::machine::MachineRegistry::new(store);
                        let _ = registry.load_from_config(&config);
                        let machines = registry.list_machines(None).unwrap_or_default();
                        let output = robot::RobotEnvelope::new(
                            "vc.robot.machines.v1",
                            serde_json::json!({
                                "machines": machines,
                                "total": machines.len(),
                            }),
                        );
                        println!("{}", output.to_json_pretty());
                    }
                    RobotCommands::Repos => {
                        let output = robot::RobotEnvelope::new(
                            "vc.robot.repos.v1",
                            serde_json::json!({ "repos": [], "warning": "not yet implemented" }),
                        );
                        println!("{}", output.to_json_pretty());
                    }
                }
            }
            Commands::Audit { command } => {
                let store = open_store(self.config.as_ref())?;
                match command {
                    AuditCommands::List {
                        event_type,
                        machine,
                        since,
                        limit,
                    } => {
                        let event_type = match event_type {
                            Some(value) => Some(
                                value
                                    .parse::<AuditEventType>()
                                    .map_err(CliError::CommandFailed)?,
                            ),
                            None => None,
                        };

                        let since = match since {
                            Some(value) => Some(parse_rfc3339(&value)?),
                            None => None,
                        };

                        let filter = AuditEventFilter {
                            event_type,
                            machine_id: machine,
                            since,
                            limit,
                        };
                        let rows = store.list_audit_events(&filter)?;
                        print_output(&rows, self.format);
                    }
                    AuditCommands::Show { id } => {
                        let row = store.get_audit_event(id)?;
                        if let Some(row) = row {
                            print_output(&row, self.format);
                        } else {
                            return Err(CliError::CommandFailed(format!(
                                "Audit event not found: {id}"
                            )));
                        }
                    }
                }
            }
            Commands::Machines { command } => {
                let store = Arc::new(open_store(self.config.as_ref())?);
                let config = match &self.config {
                    Some(path) => VcConfig::load_with_env(path)?,
                    None => VcConfig::discover_with_env()?,
                };
                let registry = vc_collect::machine::MachineRegistry::new(store);
                let _ = registry.load_from_config(&config);

                match command {
                    MachineCommands::List {
                        status,
                        tags,
                        enabled,
                    } => {
                        let status_filter =
                            status
                                .as_ref()
                                .and_then(|s| match s.to_lowercase().as_str() {
                                    "online" => Some(vc_collect::machine::MachineStatus::Online),
                                    "offline" => Some(vc_collect::machine::MachineStatus::Offline),
                                    "unknown" => Some(vc_collect::machine::MachineStatus::Unknown),
                                    _ => None,
                                });
                        let tags_filter = tags.as_ref().map(|t| {
                            t.split(',')
                                .filter_map(|s| {
                                    let trimmed = s.trim();
                                    if trimmed.is_empty() {
                                        None
                                    } else {
                                        Some(trimmed.to_string())
                                    }
                                })
                                .collect::<Vec<_>>()
                        });
                        let filter = vc_collect::machine::MachineFilter {
                            status: status_filter,
                            tags: tags_filter,
                            is_local: None,
                            enabled,
                        };
                        let machines = registry.list_machines(Some(filter)).unwrap_or_default();
                        print_output(&machines, self.format);
                    }
                    MachineCommands::Show { id } => match registry.get_machine(&id) {
                        Ok(Some(machine)) => print_output(&machine, self.format),
                        Ok(None) => {
                            return Err(CliError::CommandFailed(format!(
                                "Machine not found: {id}"
                            )));
                        }
                        Err(e) => {
                            return Err(CliError::CommandFailed(format!(
                                "Error fetching machine: {e}"
                            )));
                        }
                    },
                    MachineCommands::Add {
                        id,
                        ssh,
                        port,
                        tags,
                    } => {
                        // Parse SSH string (user@host)
                        let (ssh_user, ssh_host) = if let Some(ssh) = ssh {
                            if let Some((user, host)) = ssh.split_once('@') {
                                (Some(user.to_string()), Some(host.to_string()))
                            } else {
                                (Some("ubuntu".to_string()), Some(ssh))
                            }
                        } else {
                            (None, None)
                        };
                        let tags_vec = tags
                            .map(|t| {
                                t.split(',')
                                    .filter_map(|s| {
                                        let trimmed = s.trim();
                                        if trimmed.is_empty() {
                                            None
                                        } else {
                                            Some(trimmed.to_string())
                                        }
                                    })
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default();
                        let is_local = ssh_host.is_none();

                        let machine = vc_collect::machine::Machine {
                            machine_id: id.clone(),
                            hostname: ssh_host.clone().unwrap_or_else(|| id.clone()),
                            display_name: Some(id.clone()),
                            ssh_host,
                            ssh_user,
                            ssh_key_path: None,
                            ssh_port: port,
                            is_local,
                            os_type: None,
                            arch: None,
                            added_at: Some(chrono::Utc::now().to_rfc3339()),
                            last_seen_at: None,
                            last_probe_at: None,
                            status: vc_collect::machine::MachineStatus::Unknown,
                            tags: tags_vec,
                            metadata: None,
                            enabled: true,
                        };
                        registry.upsert_machine(&machine).map_err(|e| {
                            CliError::CommandFailed(format!("Failed to add machine: {e}"))
                        })?;
                        print_output(&machine, self.format);
                    }
                    MachineCommands::Probe { id } => {
                        let machine = match registry.get_machine(&id) {
                            Ok(Some(machine)) => machine,
                            Ok(None) => {
                                return Err(CliError::CommandFailed(format!(
                                    "Machine not found: {id}"
                                )));
                            }
                            Err(e) => {
                                return Err(CliError::CommandFailed(format!(
                                    "Error fetching machine: {e}"
                                )));
                            }
                        };

                        let executor = match machine.ssh_config() {
                            Some(cfg) => Executor::remote(cfg),
                            None => Executor::local(),
                        };

                        // First, check connectivity with uname
                        let connectivity = executor.run("uname -s", Duration::from_secs(5)).await;
                        let (status, os_detail) = match connectivity {
                            Ok(output) if output.exit_code == 0 => {
                                registry
                                    .update_status(&id, vc_collect::machine::MachineStatus::Online)
                                    .map_err(|e| {
                                        CliError::CommandFailed(format!(
                                            "Status update failed: {e}"
                                        ))
                                    })?;
                                (
                                    vc_collect::machine::MachineStatus::Online,
                                    Some(output.stdout.trim().to_string()),
                                )
                            }
                            Ok(output) => {
                                registry
                                    .update_status(&id, vc_collect::machine::MachineStatus::Offline)
                                    .map_err(|e| {
                                        CliError::CommandFailed(format!(
                                            "Status update failed: {e}"
                                        ))
                                    })?;
                                (
                                    vc_collect::machine::MachineStatus::Offline,
                                    Some(output.stderr),
                                )
                            }
                            Err(err) => {
                                registry
                                    .update_status(&id, vc_collect::machine::MachineStatus::Offline)
                                    .map_err(|e| {
                                        CliError::CommandFailed(format!(
                                            "Status update failed: {e}"
                                        ))
                                    })?;
                                (
                                    vc_collect::machine::MachineStatus::Offline,
                                    Some(err.to_string()),
                                )
                            }
                        };

                        // If online, probe for tools
                        let tools_result = if status == vc_collect::machine::MachineStatus::Online {
                            let prober = vc_collect::ToolProber::new();
                            Some(prober.probe_machine(&id, &executor, &registry).await)
                        } else {
                            None
                        };

                        let payload = serde_json::json!({
                            "machine_id": id,
                            "status": status.as_str(),
                            "os": os_detail,
                            "tools": tools_result.as_ref().map(|r| {
                                r.found_tools.iter().map(|t| serde_json::json!({
                                    "name": t.tool_name,
                                    "path": t.tool_path,
                                    "version": t.tool_version,
                                    "available": t.is_available,
                                })).collect::<Vec<_>>()
                            }),
                            "tools_found": tools_result.as_ref().map(|r| r.tool_count()).unwrap_or(0),
                            "probe_errors": tools_result.as_ref().map(|r| &r.errors),
                        });
                        print_output(&payload, self.format);
                    }
                    MachineCommands::Enable { id, enabled } => {
                        let existing = registry.get_machine(&id).map_err(|e| {
                            CliError::CommandFailed(format!("Error fetching machine: {e}"))
                        })?;
                        if existing.is_none() {
                            return Err(CliError::CommandFailed(format!(
                                "Machine not found: {id}"
                            )));
                        }
                        registry.set_enabled(&id, enabled).map_err(|e| {
                            CliError::CommandFailed(format!("Enable update failed: {e}"))
                        })?;
                        let updated = registry
                            .get_machine(&id)
                            .map_err(|e| {
                                CliError::CommandFailed(format!("Error fetching machine: {e}"))
                            })?
                            .ok_or_else(|| {
                                CliError::CommandFailed(format!("Machine not found: {id}"))
                            })?;
                        print_output(&updated, self.format);
                    }
                }
            }
            Commands::Query { command } => {
                let store = open_store(self.config.as_ref())?;
                let validator = vc_query::QueryValidator::new(vc_query::GuardrailConfig::default());

                match command {
                    QueryCommands::Raw { sql, limit } => {
                        // Validate the query is read-only
                        validator.validate_raw(&sql)?;

                        // Add LIMIT if not present
                        let query = if sql.to_uppercase().contains("LIMIT") {
                            sql
                        } else {
                            format!("{} LIMIT {}", sql.trim_end_matches(';'), limit)
                        };

                        let rows = store.query_json(&query)?;

                        if rows.len() >= limit {
                            eprintln!("Warning: Results may be truncated at {} rows", limit);
                        }

                        print_output(&rows, self.format);
                    }
                    QueryCommands::Template { name, param } => {
                        // Parse parameters
                        let mut params = std::collections::HashMap::new();
                        for p in param {
                            if let Some((key, value)) = p.split_once('=') {
                                params.insert(key.to_string(), value.to_string());
                            } else {
                                return Err(CliError::CommandFailed(format!(
                                    "Invalid parameter format: '{}'. Use key=value",
                                    p
                                )));
                            }
                        }

                        // Expand template
                        let sql = validator.expand_template(&name, &params)?;

                        // Execute query
                        let rows = store.query_json(&sql)?;
                        print_output(&rows, self.format);
                    }
                    QueryCommands::Templates => {
                        let templates: Vec<_> = validator
                            .templates()
                            .iter()
                            .map(|(name, t)| {
                                serde_json::json!({
                                    "name": name,
                                    "description": t.description,
                                    "params": t.params.iter().map(|p| serde_json::json!({
                                        "name": p.name,
                                        "description": p.description,
                                        "default": p.default,
                                    })).collect::<Vec<_>>(),
                                    "agent_safe": t.agent_safe,
                                })
                            })
                            .collect();
                        print_output(&templates, self.format);
                    }
                }
            }
            _ => {
                println!("Command not yet implemented: {:?}", self.command);
            }
        }
        Ok(())
    }
}

fn open_store(config_path: Option<&std::path::PathBuf>) -> Result<VcStore, CliError> {
    let config = match config_path {
        Some(path) => VcConfig::load_with_env(path)?,
        None => VcConfig::discover_with_env()?,
    };
    Ok(VcStore::open(&config.global.db_path)?)
}

fn parse_rfc3339(value: &str) -> Result<DateTime<Utc>, CliError> {
    let parsed = DateTime::parse_from_rfc3339(value)
        .map_err(|err| CliError::CommandFailed(format!("Invalid timestamp: {err}")))?;
    Ok(parsed.with_timezone(&Utc))
}

fn print_output<T: Serialize>(value: &T, format: OutputFormat) {
    let json = match format {
        OutputFormat::Json => serde_json::to_string_pretty(value),
        OutputFormat::Toon => serde_json::to_string(value),
        OutputFormat::Text => serde_json::to_string_pretty(value),
    }
    .unwrap_or_else(|e| format!(r#"{{"error": "serialization failed: {e}"}}"#));
    println!("{json}");
}

#[cfg(test)]
mod tests {
    use super::*;

    // =============================================================================
    // CliError Tests
    // =============================================================================

    #[test]
    fn cli_error_command_failed_display() {
        let err = CliError::CommandFailed("timeout".to_string());
        assert_eq!(err.to_string(), "Command failed: timeout");
    }

    #[test]
    fn cli_error_debug_format() {
        let err = CliError::CommandFailed("test".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("CommandFailed"));
    }

    // =============================================================================
    // OutputFormat Tests
    // =============================================================================

    #[test]
    fn output_format_json_serialize() {
        let format = OutputFormat::Json;
        let json = serde_json::to_string(&format).unwrap();
        assert!(json.contains("Json") || json.contains("json"));
    }

    #[test]
    fn output_format_toon_serialize() {
        let format = OutputFormat::Toon;
        let json = serde_json::to_string(&format).unwrap();
        assert!(json.contains("Toon") || json.contains("toon"));
    }

    #[test]
    fn output_format_text_serialize() {
        let format = OutputFormat::Text;
        let json = serde_json::to_string(&format).unwrap();
        assert!(json.contains("Text") || json.contains("text"));
    }

    #[test]
    fn output_format_roundtrip() {
        for format in [OutputFormat::Json, OutputFormat::Toon, OutputFormat::Text] {
            let json = serde_json::to_string(&format).unwrap();
            let parsed: OutputFormat = serde_json::from_str(&json).unwrap();
            assert!(matches!(
                (&format, &parsed),
                (OutputFormat::Json, OutputFormat::Json)
                    | (OutputFormat::Toon, OutputFormat::Toon)
                    | (OutputFormat::Text, OutputFormat::Text)
            ));
        }
    }

    #[test]
    fn output_format_clone() {
        let format = OutputFormat::Json;
        let cloned = format;
        assert!(matches!(cloned, OutputFormat::Json));
    }

    // =============================================================================
    // Basic CLI Parsing Tests
    // =============================================================================

    #[test]
    fn test_cli_parse() {
        let cli = Cli::parse_from(["vc", "status"]);
        assert!(matches!(cli.command, Commands::Status { .. }));
    }

    #[test]
    fn test_cli_debug() {
        let cli = Cli::parse_from(["vc", "tui"]);
        let debug = format!("{:?}", cli);
        assert!(debug.contains("Cli"));
    }

    #[test]
    fn test_global_format_flag() {
        let cli = Cli::parse_from(["vc", "--format", "json", "status"]);
        assert!(matches!(cli.format, OutputFormat::Json));
    }

    #[test]
    fn test_global_format_toon() {
        let cli = Cli::parse_from(["vc", "--format", "toon", "status"]);
        assert!(matches!(cli.format, OutputFormat::Toon));
    }

    #[test]
    fn test_global_verbose_flag() {
        let cli = Cli::parse_from(["vc", "--verbose", "status"]);
        assert!(cli.verbose);
    }

    #[test]
    fn test_global_config_flag() {
        let cli = Cli::parse_from(["vc", "--config", "/path/to/config.toml", "status"]);
        assert_eq!(
            cli.config,
            Some(std::path::PathBuf::from("/path/to/config.toml"))
        );
    }

    #[test]
    fn test_default_format_is_text() {
        let cli = Cli::parse_from(["vc", "status"]);
        assert!(matches!(cli.format, OutputFormat::Text));
    }

    #[test]
    fn test_default_verbose_is_false() {
        let cli = Cli::parse_from(["vc", "status"]);
        assert!(!cli.verbose);
    }

    // =============================================================================
    // Commands::Tui Tests
    // =============================================================================

    #[test]
    fn test_tui_parse() {
        let cli = Cli::parse_from(["vc", "tui"]);
        assert!(matches!(cli.command, Commands::Tui));
    }

    // =============================================================================
    // Commands::Daemon Tests
    // =============================================================================

    #[test]
    fn test_daemon_parse() {
        let cli = Cli::parse_from(["vc", "daemon"]);
        if let Commands::Daemon { foreground } = cli.command {
            assert!(!foreground);
        } else {
            panic!("Expected Daemon command");
        }
    }

    #[test]
    fn test_daemon_foreground() {
        let cli = Cli::parse_from(["vc", "daemon", "--foreground"]);
        if let Commands::Daemon { foreground } = cli.command {
            assert!(foreground);
        } else {
            panic!("Expected Daemon command");
        }
    }

    #[test]
    fn test_daemon_short_foreground() {
        let cli = Cli::parse_from(["vc", "daemon", "-f"]);
        if let Commands::Daemon { foreground } = cli.command {
            assert!(foreground);
        } else {
            panic!("Expected Daemon command");
        }
    }

    // =============================================================================
    // Commands::Status Tests
    // =============================================================================

    #[test]
    fn test_status_no_machine() {
        let cli = Cli::parse_from(["vc", "status"]);
        if let Commands::Status { machine } = cli.command {
            assert!(machine.is_none());
        } else {
            panic!("Expected Status command");
        }
    }

    #[test]
    fn test_status_with_machine() {
        let cli = Cli::parse_from(["vc", "status", "--machine", "server-1"]);
        if let Commands::Status { machine } = cli.command {
            assert_eq!(machine, Some("server-1".to_string()));
        } else {
            panic!("Expected Status command");
        }
    }

    // =============================================================================
    // Commands::Robot Tests
    // =============================================================================

    #[test]
    fn test_robot_parse() {
        let cli = Cli::parse_from(["vc", "robot", "health"]);
        assert!(matches!(cli.command, Commands::Robot { .. }));
    }

    #[test]
    fn test_robot_health_parse() {
        let cli = Cli::parse_from(["vc", "robot", "health"]);
        if let Commands::Robot { command } = cli.command {
            assert!(matches!(command, RobotCommands::Health));
        } else {
            panic!("Expected Robot command");
        }
    }

    #[test]
    fn test_robot_triage_parse() {
        let cli = Cli::parse_from(["vc", "robot", "triage"]);
        if let Commands::Robot { command } = cli.command {
            assert!(matches!(command, RobotCommands::Triage));
        } else {
            panic!("Expected Robot command");
        }
    }

    #[test]
    fn test_robot_accounts_parse() {
        let cli = Cli::parse_from(["vc", "robot", "accounts"]);
        if let Commands::Robot { command } = cli.command {
            assert!(matches!(command, RobotCommands::Accounts));
        } else {
            panic!("Expected Robot command");
        }
    }

    #[test]
    fn test_robot_oracle_parse() {
        let cli = Cli::parse_from(["vc", "robot", "oracle"]);
        if let Commands::Robot { command } = cli.command {
            assert!(matches!(command, RobotCommands::Oracle));
        } else {
            panic!("Expected Robot command");
        }
    }

    #[test]
    fn test_robot_machines_parse() {
        let cli = Cli::parse_from(["vc", "robot", "machines"]);
        if let Commands::Robot { command } = cli.command {
            assert!(matches!(command, RobotCommands::Machines));
        } else {
            panic!("Expected Robot command");
        }
    }

    #[test]
    fn test_robot_repos_parse() {
        let cli = Cli::parse_from(["vc", "robot", "repos"]);
        if let Commands::Robot { command } = cli.command {
            assert!(matches!(command, RobotCommands::Repos));
        } else {
            panic!("Expected Robot command");
        }
    }

    #[test]
    fn test_robot_status_parse() {
        let cli = Cli::parse_from(["vc", "robot", "status"]);
        if let Commands::Robot { command } = cli.command {
            assert!(matches!(command, RobotCommands::Status));
        } else {
            panic!("Expected Robot command");
        }
    }

    // =============================================================================
    // Commands::Machines Tests
    // =============================================================================

    #[test]
    fn test_machines_list_parse() {
        let cli = Cli::parse_from(["vc", "machines", "list"]);
        if let Commands::Machines { command } = cli.command {
            if let MachineCommands::List {
                status,
                tags,
                enabled,
            } = command
            {
                assert!(status.is_none());
                assert!(tags.is_none());
                assert!(enabled.is_none());
            } else {
                panic!("Expected Machines list command");
            }
        } else {
            panic!("Expected Machines command");
        }
    }

    #[test]
    fn test_machines_list_filters_parse() {
        let cli = Cli::parse_from([
            "vc",
            "machines",
            "list",
            "--status",
            "online",
            "--tags",
            "mini,builder",
            "--enabled",
            "true",
        ]);
        if let Commands::Machines { command } = cli.command {
            if let MachineCommands::List {
                status,
                tags,
                enabled,
            } = command
            {
                assert_eq!(status, Some("online".to_string()));
                assert_eq!(tags, Some("mini,builder".to_string()));
                assert_eq!(enabled, Some(true));
            } else {
                panic!("Expected Machines list command");
            }
        } else {
            panic!("Expected Machines command");
        }
    }

    #[test]
    fn test_machines_show_parse() {
        let cli = Cli::parse_from(["vc", "machines", "show", "mac-mini-1"]);
        if let Commands::Machines { command } = cli.command {
            if let MachineCommands::Show { id } = command {
                assert_eq!(id, "mac-mini-1");
            } else {
                panic!("Expected Machines show command");
            }
        } else {
            panic!("Expected Machines command");
        }
    }

    #[test]
    fn test_machines_add_parse() {
        let cli = Cli::parse_from([
            "vc",
            "machines",
            "add",
            "mac-mini-3",
            "--ssh",
            "ubuntu@192.168.1.102",
            "--port",
            "2222",
            "--tags",
            "mini,builder",
        ]);
        if let Commands::Machines { command } = cli.command {
            if let MachineCommands::Add {
                id,
                ssh,
                port,
                tags,
            } = command
            {
                assert_eq!(id, "mac-mini-3");
                assert_eq!(ssh, Some("ubuntu@192.168.1.102".to_string()));
                assert_eq!(port, 2222);
                assert_eq!(tags, Some("mini,builder".to_string()));
            } else {
                panic!("Expected Machines add command");
            }
        } else {
            panic!("Expected Machines command");
        }
    }

    #[test]
    fn test_machines_probe_parse() {
        let cli = Cli::parse_from(["vc", "machines", "probe", "mac-mini-1"]);
        if let Commands::Machines { command } = cli.command {
            if let MachineCommands::Probe { id } = command {
                assert_eq!(id, "mac-mini-1");
            } else {
                panic!("Expected Machines probe command");
            }
        } else {
            panic!("Expected Machines command");
        }
    }

    #[test]
    fn test_machines_enable_parse() {
        let cli = Cli::parse_from(["vc", "machines", "enable", "mac-mini-1", "--enabled"]);
        if let Commands::Machines { command } = cli.command {
            if let MachineCommands::Enable { id, enabled } = command {
                assert_eq!(id, "mac-mini-1");
                assert!(enabled);
            } else {
                panic!("Expected Machines enable command");
            }
        } else {
            panic!("Expected Machines command");
        }
    }

    // =============================================================================
    // Commands::Watch Tests
    // =============================================================================

    #[test]
    fn test_watch_parse() {
        let cli = Cli::parse_from(["vc", "watch"]);
        if let Commands::Watch {
            events,
            changes_only,
        } = cli.command
        {
            assert!(events.is_none());
            assert!(!changes_only);
        } else {
            panic!("Expected Watch command");
        }
    }

    #[test]
    fn test_watch_changes_only() {
        let cli = Cli::parse_from(["vc", "watch", "--changes-only"]);
        if let Commands::Watch { changes_only, .. } = cli.command {
            assert!(changes_only);
        } else {
            panic!("Expected Watch command");
        }
    }

    // =============================================================================
    // Commands::Collect Tests
    // =============================================================================

    #[test]
    fn test_collect_parse() {
        let cli = Cli::parse_from(["vc", "collect"]);
        if let Commands::Collect { collector, machine } = cli.command {
            assert!(collector.is_none());
            assert!(machine.is_none());
        } else {
            panic!("Expected Collect command");
        }
    }

    #[test]
    fn test_collect_with_collector() {
        let cli = Cli::parse_from(["vc", "collect", "--collector", "sysmoni"]);
        if let Commands::Collect { collector, .. } = cli.command {
            assert_eq!(collector, Some("sysmoni".to_string()));
        } else {
            panic!("Expected Collect command");
        }
    }

    #[test]
    fn test_collect_with_machine() {
        let cli = Cli::parse_from(["vc", "collect", "--machine", "server-2"]);
        if let Commands::Collect { machine, .. } = cli.command {
            assert_eq!(machine, Some("server-2".to_string()));
        } else {
            panic!("Expected Collect command");
        }
    }

    // =============================================================================
    // Commands::Alert Tests
    // =============================================================================

    #[test]
    fn test_alert_list_parse() {
        let cli = Cli::parse_from(["vc", "alert", "list"]);
        if let Commands::Alert { command } = cli.command {
            if let AlertCommands::List { unacked } = command {
                assert!(!unacked);
            } else {
                panic!("Expected List subcommand");
            }
        } else {
            panic!("Expected Alert command");
        }
    }

    #[test]
    fn test_alert_list_unacked() {
        let cli = Cli::parse_from(["vc", "alert", "list", "--unacked"]);
        if let Commands::Alert { command } = cli.command {
            if let AlertCommands::List { unacked } = command {
                assert!(unacked);
            } else {
                panic!("Expected List subcommand");
            }
        } else {
            panic!("Expected Alert command");
        }
    }

    #[test]
    fn test_alert_ack_parse() {
        let cli = Cli::parse_from(["vc", "alert", "ack", "123"]);
        if let Commands::Alert { command } = cli.command {
            if let AlertCommands::Ack { id } = command {
                assert_eq!(id, 123);
            } else {
                panic!("Expected Ack subcommand");
            }
        } else {
            panic!("Expected Alert command");
        }
    }

    #[test]
    fn test_alert_rules_parse() {
        let cli = Cli::parse_from(["vc", "alert", "rules"]);
        if let Commands::Alert { command } = cli.command {
            assert!(matches!(command, AlertCommands::Rules));
        } else {
            panic!("Expected Alert command");
        }
    }

    // =============================================================================
    // Commands::Guardian Tests
    // =============================================================================

    #[test]
    fn test_guardian_playbooks_parse() {
        let cli = Cli::parse_from(["vc", "guardian", "playbooks"]);
        if let Commands::Guardian { command } = cli.command {
            assert!(matches!(command, GuardianCommands::Playbooks));
        } else {
            panic!("Expected Guardian command");
        }
    }

    #[test]
    fn test_guardian_runs_parse() {
        let cli = Cli::parse_from(["vc", "guardian", "runs"]);
        if let Commands::Guardian { command } = cli.command {
            assert!(matches!(command, GuardianCommands::Runs));
        } else {
            panic!("Expected Guardian command");
        }
    }

    #[test]
    fn test_guardian_trigger_parse() {
        let cli = Cli::parse_from(["vc", "guardian", "trigger", "swap-account"]);
        if let Commands::Guardian { command } = cli.command {
            if let GuardianCommands::Trigger { playbook_id } = command {
                assert_eq!(playbook_id, "swap-account");
            } else {
                panic!("Expected Trigger subcommand");
            }
        } else {
            panic!("Expected Guardian command");
        }
    }

    #[test]
    fn test_guardian_approve_parse() {
        let cli = Cli::parse_from(["vc", "guardian", "approve", "456"]);
        if let Commands::Guardian { command } = cli.command {
            if let GuardianCommands::Approve { run_id } = command {
                assert_eq!(run_id, 456);
            } else {
                panic!("Expected Approve subcommand");
            }
        } else {
            panic!("Expected Guardian command");
        }
    }

    // =============================================================================
    // Commands::Fleet Tests
    // =============================================================================

    #[test]
    fn test_fleet_spawn_parse() {
        let cli = Cli::parse_from([
            "vc",
            "fleet",
            "spawn",
            "--agent-type",
            "claude-code",
            "--machine",
            "server-1",
        ]);
        if let Commands::Fleet { command } = cli.command {
            if let FleetCommands::Spawn {
                agent_type,
                count,
                machine,
            } = command
            {
                assert_eq!(agent_type, "claude-code");
                assert_eq!(count, 1); // default
                assert_eq!(machine, "server-1");
            } else {
                panic!("Expected Spawn subcommand");
            }
        } else {
            panic!("Expected Fleet command");
        }
    }

    #[test]
    fn test_fleet_spawn_with_count() {
        let cli = Cli::parse_from([
            "vc",
            "fleet",
            "spawn",
            "--agent-type",
            "codex",
            "--count",
            "5",
            "--machine",
            "server-2",
        ]);
        if let Commands::Fleet { command } = cli.command {
            if let FleetCommands::Spawn { count, .. } = command {
                assert_eq!(count, 5);
            } else {
                panic!("Expected Spawn subcommand");
            }
        } else {
            panic!("Expected Fleet command");
        }
    }

    #[test]
    fn test_fleet_rebalance_parse() {
        let cli = Cli::parse_from(["vc", "fleet", "rebalance"]);
        if let Commands::Fleet { command } = cli.command {
            if let FleetCommands::Rebalance { strategy } = command {
                assert_eq!(strategy, "even-load"); // default
            } else {
                panic!("Expected Rebalance subcommand");
            }
        } else {
            panic!("Expected Fleet command");
        }
    }

    #[test]
    fn test_fleet_rebalance_custom_strategy() {
        let cli = Cli::parse_from(["vc", "fleet", "rebalance", "--strategy", "round-robin"]);
        if let Commands::Fleet { command } = cli.command {
            if let FleetCommands::Rebalance { strategy } = command {
                assert_eq!(strategy, "round-robin");
            } else {
                panic!("Expected Rebalance subcommand");
            }
        } else {
            panic!("Expected Fleet command");
        }
    }

    #[test]
    fn test_fleet_emergency_stop_parse() {
        let cli = Cli::parse_from([
            "vc",
            "fleet",
            "emergency-stop",
            "--scope",
            "all",
            "--reason",
            "testing",
        ]);
        if let Commands::Fleet { command } = cli.command {
            if let FleetCommands::EmergencyStop {
                scope,
                reason,
                force,
            } = command
            {
                assert_eq!(scope, "all");
                assert_eq!(reason, "testing");
                assert!(!force);
            } else {
                panic!("Expected EmergencyStop subcommand");
            }
        } else {
            panic!("Expected Fleet command");
        }
    }

    #[test]
    fn test_fleet_emergency_stop_force() {
        let cli = Cli::parse_from([
            "vc",
            "fleet",
            "emergency-stop",
            "--scope",
            "machine:server-1",
            "--reason",
            "fire",
            "--force",
        ]);
        if let Commands::Fleet { command } = cli.command {
            if let FleetCommands::EmergencyStop { force, .. } = command {
                assert!(force);
            } else {
                panic!("Expected EmergencyStop subcommand");
            }
        } else {
            panic!("Expected Fleet command");
        }
    }

    #[test]
    fn test_fleet_migrate_parse() {
        let cli = Cli::parse_from([
            "vc", "fleet", "migrate", "--from", "server-1", "--to", "server-2",
        ]);
        if let Commands::Fleet { command } = cli.command {
            if let FleetCommands::Migrate { from, to, workload } = command {
                assert_eq!(from, "server-1");
                assert_eq!(to, "server-2");
                assert!(workload.is_none());
            } else {
                panic!("Expected Migrate subcommand");
            }
        } else {
            panic!("Expected Fleet command");
        }
    }

    #[test]
    fn test_fleet_migrate_with_workload() {
        let cli = Cli::parse_from([
            "vc",
            "fleet",
            "migrate",
            "--from",
            "a",
            "--to",
            "b",
            "--workload",
            "claude-*",
        ]);
        if let Commands::Fleet { command } = cli.command {
            if let FleetCommands::Migrate { workload, .. } = command {
                assert_eq!(workload, Some("claude-*".to_string()));
            } else {
                panic!("Expected Migrate subcommand");
            }
        } else {
            panic!("Expected Fleet command");
        }
    }

    // =============================================================================
    // Commands::Vacuum Tests
    // =============================================================================

    #[test]
    fn test_vacuum_parse() {
        let cli = Cli::parse_from(["vc", "vacuum"]);
        if let Commands::Vacuum { dry_run, table } = cli.command {
            assert!(!dry_run);
            assert!(table.is_none());
        } else {
            panic!("Expected Vacuum command");
        }
    }

    #[test]
    fn test_vacuum_dry_run() {
        let cli = Cli::parse_from(["vc", "vacuum", "--dry-run"]);
        if let Commands::Vacuum { dry_run, .. } = cli.command {
            assert!(dry_run);
        } else {
            panic!("Expected Vacuum command");
        }
    }

    #[test]
    fn test_vacuum_specific_table() {
        let cli = Cli::parse_from(["vc", "vacuum", "--table", "metrics"]);
        if let Commands::Vacuum { table, .. } = cli.command {
            assert_eq!(table, Some("metrics".to_string()));
        } else {
            panic!("Expected Vacuum command");
        }
    }

    // =============================================================================
    // Commands::Web Tests
    // =============================================================================

    #[test]
    fn test_web_parse() {
        let cli = Cli::parse_from(["vc", "web"]);
        if let Commands::Web { port, bind } = cli.command {
            assert_eq!(port, 8080); // default
            assert_eq!(bind, "127.0.0.1"); // default
        } else {
            panic!("Expected Web command");
        }
    }

    #[test]
    fn test_web_port() {
        let cli = Cli::parse_from(["vc", "web", "--port", "3000"]);
        if let Commands::Web { port, .. } = cli.command {
            assert_eq!(port, 3000);
        } else {
            panic!("Expected Web command");
        }
    }

    #[test]
    fn test_web_bind() {
        let cli = Cli::parse_from(["vc", "web", "--bind", "0.0.0.0"]);
        if let Commands::Web { bind, .. } = cli.command {
            assert_eq!(bind, "0.0.0.0");
        } else {
            panic!("Expected Web command");
        }
    }

    #[test]
    fn test_web_port_and_bind() {
        let cli = Cli::parse_from(["vc", "web", "-p", "9000", "-b", "192.168.1.1"]);
        if let Commands::Web { port, bind } = cli.command {
            assert_eq!(port, 9000);
            assert_eq!(bind, "192.168.1.1");
        } else {
            panic!("Expected Web command");
        }
    }

    // =============================================================================
    // Commands::Audit Tests
    // =============================================================================

    #[test]
    fn test_audit_list_parse() {
        let cli = Cli::parse_from(["vc", "audit", "list", "--event-type", "collector_run"]);
        if let Commands::Audit { command } = cli.command {
            if let AuditCommands::List { event_type, .. } = command {
                assert_eq!(event_type, Some("collector_run".to_string()));
            } else {
                panic!("Expected Audit list");
            }
        } else {
            panic!("Expected Audit command");
        }
    }

    #[test]
    fn test_audit_show_parse() {
        let cli = Cli::parse_from(["vc", "audit", "show", "42"]);
        if let Commands::Audit { command } = cli.command {
            if let AuditCommands::Show { id } = command {
                assert_eq!(id, 42);
            } else {
                panic!("Expected Audit show");
            }
        } else {
            panic!("Expected Audit command");
        }
    }

    // =============================================================================
    // Cli::run Tests
    // =============================================================================

    #[tokio::test]
    async fn test_cli_run_status() {
        let cli = Cli::parse_from(["vc", "status"]);
        let result = cli.run().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_cli_run_tui() {
        let cli = Cli::parse_from(["vc", "tui"]);
        let result = cli.run().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_cli_run_robot_health() {
        let cli = Cli::parse_from(["vc", "robot", "health"]);
        let result = cli.run().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_cli_run_robot_triage() {
        let cli = Cli::parse_from(["vc", "robot", "triage"]);
        let result = cli.run().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_cli_run_robot_accounts() {
        let cli = Cli::parse_from(["vc", "robot", "accounts"]);
        let result = cli.run().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_cli_run_robot_oracle() {
        let cli = Cli::parse_from(["vc", "robot", "oracle"]);
        let result = cli.run().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_cli_run_robot_machines() {
        let cli = Cli::parse_from(["vc", "robot", "machines"]);
        let result = cli.run().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_cli_run_robot_repos() {
        let cli = Cli::parse_from(["vc", "robot", "repos"]);
        let result = cli.run().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_cli_run_robot_status() {
        let cli = Cli::parse_from(["vc", "robot", "status"]);
        let result = cli.run().await;
        assert!(result.is_ok());
    }
}
