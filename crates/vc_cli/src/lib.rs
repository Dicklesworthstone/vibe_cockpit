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
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use vc_collect::executor::Executor;
use vc_config::VcConfig;
use vc_knowledge::{EntryType, FeedbackType, KnowledgeEntry, KnowledgeFeedback, KnowledgeStore, SearchOptions};
use vc_store::{AuditEventFilter, AuditEventType, VcStore};

pub mod robot;
pub mod schema_registry;

pub use robot::{HealthData, RobotEnvelope, StatusData, TriageData};
pub use schema_registry::{SchemaEntry, SchemaIndex, SchemaRegistry};

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

    #[error("Knowledge error: {0}")]
    KnowledgeError(#[from] vc_knowledge::KnowledgeError),
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

    /// Autopilot management
    Autopilot {
        #[command(subcommand)]
        command: AutopilotCommands,
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

    /// Configuration management
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },

    /// Data retention policy management
    Retention {
        #[command(subcommand)]
        command: RetentionCommands,
    },

    /// Data quality: collector health, freshness, and drift detection
    Health {
        #[command(subcommand)]
        command: HealthCommands,
    },

    /// Knowledge base management (solutions, patterns, prompts, debug logs)
    Knowledge {
        #[command(subcommand)]
        command: KnowledgeCommands,
    },
}

/// Retention policy subcommands
#[derive(Subcommand, Debug)]
pub enum RetentionCommands {
    /// List all retention policies
    List,

    /// Set retention policy for a table
    Set {
        /// Table name
        #[arg(long)]
        table: String,

        /// Retention period in days
        #[arg(long)]
        days: i32,

        /// Disable the policy (default: enabled)
        #[arg(long)]
        disabled: bool,
    },

    /// Show vacuum operation history
    History {
        /// Number of entries to show
        #[arg(long, default_value = "20")]
        limit: usize,
    },
}

/// Data quality subcommands
#[derive(Subcommand, Debug)]
pub enum HealthCommands {
    /// Show freshness summary per collector/machine
    Freshness {
        /// Filter by machine ID
        #[arg(long)]
        machine: Option<String>,

        /// Staleness threshold in seconds (default: 600 = 10 min)
        #[arg(long, default_value = "600")]
        stale_threshold: i64,
    },

    /// Show recent collector health entries
    Collectors {
        /// Filter by machine ID
        #[arg(long)]
        machine: Option<String>,

        /// Filter by collector name
        #[arg(long)]
        collector: Option<String>,

        /// Number of entries to show
        #[arg(long, default_value = "20")]
        limit: usize,
    },

    /// Show recent drift events
    Drift {
        /// Filter by machine ID
        #[arg(long)]
        machine: Option<String>,

        /// Filter by severity (info, warning, critical)
        #[arg(long)]
        severity: Option<String>,

        /// Number of entries to show
        #[arg(long, default_value = "50")]
        limit: usize,
    },

    /// Show machine baselines
    Baselines {
        /// Filter by machine ID
        #[arg(long)]
        machine: Option<String>,
    },

    /// Show health scores (latest summary per machine)
    Score {
        /// Show score for a specific machine
        #[arg(long)]
        machine: Option<String>,
    },
}

/// Knowledge base subcommands
#[derive(Subcommand, Debug)]
pub enum KnowledgeCommands {
    /// Add a new knowledge entry
    Add {
        /// Entry type: solution, pattern, prompt, debug_log
        #[arg(long)]
        entry_type: String,

        /// Title for the entry
        #[arg(long)]
        title: String,

        /// Content (use - for stdin)
        #[arg(long)]
        content: String,

        /// Summary (auto-truncated from content if omitted)
        #[arg(long)]
        summary: Option<String>,

        /// Source session ID
        #[arg(long)]
        session: Option<String>,

        /// Source file path
        #[arg(long)]
        file: Option<String>,

        /// Source line range (e.g. "10-25")
        #[arg(long)]
        lines: Option<String>,

        /// Tags (comma-separated)
        #[arg(long)]
        tags: Option<String>,
    },

    /// Search knowledge entries
    Search {
        /// Search query
        query: String,

        /// Filter by entry type: solution, pattern, prompt, debug_log
        #[arg(long)]
        entry_type: Option<String>,

        /// Filter by tags (comma-separated)
        #[arg(long)]
        tags: Option<String>,

        /// Maximum results to return
        #[arg(long, default_value = "20")]
        limit: usize,
    },

    /// Show a specific knowledge entry
    Show {
        /// Entry ID
        id: i64,
    },

    /// List recent knowledge entries
    List {
        /// Maximum entries to return
        #[arg(long, default_value = "20")]
        limit: usize,

        /// Filter by entry type
        #[arg(long)]
        entry_type: Option<String>,
    },

    /// Show top-rated entries
    Top {
        /// Maximum entries to return
        #[arg(long, default_value = "10")]
        limit: usize,
    },

    /// Add feedback to a knowledge entry
    Feedback {
        /// Entry ID
        id: i64,

        /// Feedback type: helpful, not_helpful, outdated
        #[arg(long)]
        feedback_type: String,

        /// Optional comment
        #[arg(long)]
        comment: Option<String>,

        /// Session ID for tracking
        #[arg(long)]
        session: Option<String>,
    },
}

/// Configuration subcommands
#[derive(Subcommand, Debug)]
pub enum ConfigCommands {
    /// Lint configuration file for errors and warnings
    Lint {
        /// Path to config file (uses auto-discovery if not specified)
        #[arg(short, long)]
        file: Option<PathBuf>,

        /// Show only errors (no warnings or info)
        #[arg(long)]
        errors_only: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Generate a new configuration file interactively
    Wizard {
        /// Output file path (default: vc.toml in current directory)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Overwrite existing file without prompting
        #[arg(long)]
        overwrite: bool,

        /// Generate minimal config (skip optional sections)
        #[arg(long)]
        minimal: bool,
    },

    /// Show the current configuration
    Show {
        /// Path to config file (uses auto-discovery if not specified)
        #[arg(short, long)]
        file: Option<PathBuf>,

        /// Output as JSON instead of TOML
        #[arg(long)]
        json: bool,
    },

    /// Show config file search paths
    Paths,
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

/// Autopilot subcommands
#[derive(Subcommand, Debug)]
pub enum AutopilotCommands {
    /// Show autopilot status
    Status,

    /// List recent autopilot decisions
    Decisions {
        /// Filter by decision type (account_switch, workload_balance, cost_optimization)
        #[arg(long)]
        decision_type: Option<String>,

        /// Maximum number of decisions to show
        #[arg(long, default_value = "20")]
        limit: usize,
    },

    /// Show decision summary statistics
    Summary,
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
            Commands::Config { command } => {
                use vc_config::LintSeverity;

                match command {
                    ConfigCommands::Lint {
                        file,
                        errors_only,
                        json,
                    } => {
                        // Load config from specified file or discover
                        let config = match file {
                            Some(path) => VcConfig::load(&path)?,
                            None => VcConfig::discover()?,
                        };

                        // Run lint
                        let result = config.lint();

                        if json {
                            // JSON output
                            print_output(&result, OutputFormat::Json);
                        } else {
                            // Human-readable output
                            if result.issues.is_empty() {
                                println!("✓ Configuration is valid with no issues");
                            } else {
                                for issue in &result.issues {
                                    if errors_only && issue.severity != LintSeverity::Error {
                                        continue;
                                    }

                                    let severity_icon = match issue.severity {
                                        LintSeverity::Error => "✗",
                                        LintSeverity::Warning => "⚠",
                                        LintSeverity::Info => "ℹ",
                                    };

                                    println!(
                                        "{} [{}] {}: {}",
                                        severity_icon, issue.severity, issue.path, issue.message
                                    );

                                    if let Some(ref suggestion) = issue.suggestion {
                                        println!("  → Fix: {}", suggestion.description);
                                        if let Some(ref val) = suggestion.suggested_value {
                                            println!("    {} = {}", suggestion.path, val);
                                        }
                                    }
                                }

                                println!();
                                println!(
                                    "Summary: {} error(s), {} warning(s), {} info",
                                    result.error_count, result.warning_count, result.info_count
                                );
                            }
                        }

                        // Exit with error if there are errors
                        if result.has_errors() {
                            return Err(CliError::CommandFailed(
                                "Configuration has errors".to_string(),
                            ));
                        }
                    }
                    ConfigCommands::Wizard {
                        output,
                        overwrite,
                        minimal: _,
                    } => {
                        let output_path = output.unwrap_or_else(|| PathBuf::from("vc.toml"));

                        // Check if file exists
                        if output_path.exists() && !overwrite {
                            return Err(CliError::CommandFailed(format!(
                                "File already exists: {}. Use --overwrite to replace.",
                                output_path.display()
                            )));
                        }

                        // Generate default config
                        let content = VcConfig::generate_default_toml();

                        // Write to file
                        std::fs::write(&output_path, &content).map_err(|e| {
                            CliError::CommandFailed(format!("Failed to write config: {}", e))
                        })?;

                        println!("✓ Generated configuration: {}", output_path.display());
                        println!();
                        println!("Next steps:");
                        println!("  1. Edit {} to customize settings", output_path.display());
                        println!("  2. Run 'vc config lint' to validate");
                        println!("  3. Run 'vc daemon' to start monitoring");
                    }
                    ConfigCommands::Show { file, json } => {
                        let config = match file {
                            Some(path) => VcConfig::load(&path)?,
                            None => VcConfig::discover()?,
                        };

                        if json {
                            print_output(&config, OutputFormat::Json);
                        } else {
                            let toml = config.to_toml()?;
                            println!("{toml}");
                        }
                    }
                    ConfigCommands::Paths => {
                        let paths = VcConfig::config_paths();
                        println!("Config file search paths (in order of precedence):");
                        for (i, path) in paths.iter().enumerate() {
                            let exists = path.exists();
                            let marker = if exists { "✓" } else { " " };
                            println!("  {} {}. {}", marker, i + 1, path.display());
                        }

                        // Show which one is currently loaded
                        for path in &paths {
                            if path.exists() {
                                println!();
                                println!("Currently using: {}", path.display());
                                break;
                            }
                        }
                    }
                }
            }
            Commands::Vacuum { dry_run, table } => {
                let store = open_store(self.config.as_ref())?;

                let results = store
                    .run_vacuum(dry_run, table.as_deref())
                    .map_err(|e| CliError::CommandFailed(format!("Vacuum failed: {e}")))?;

                if results.is_empty() {
                    if table.is_some() {
                        println!("No retention policy found for specified table");
                    } else {
                        println!("No enabled retention policies found");
                    }
                } else {
                    let summary = serde_json::json!({
                        "dry_run": dry_run,
                        "tables_processed": results.len(),
                        "total_rows_deleted": results.iter().map(|r| r.rows_deleted).sum::<i64>(),
                        "total_rows_would_delete": results.iter().map(|r| r.rows_would_delete).sum::<i64>(),
                        "results": results,
                    });
                    print_output(&summary, self.format);
                }
            }
            Commands::Retention { command } => {
                let store = open_store(self.config.as_ref())?;

                match command {
                    RetentionCommands::List => {
                        let policies = store.list_retention_policies().map_err(|e| {
                            CliError::CommandFailed(format!("Failed to list policies: {e}"))
                        })?;

                        if policies.is_empty() {
                            println!("No retention policies configured");
                            println!();
                            println!("To add a policy, use:");
                            println!("  vc retention set --table <table_name> --days <days>");
                        } else {
                            print_output(&policies, self.format);
                        }
                    }
                    RetentionCommands::Set {
                        table,
                        days,
                        disabled,
                    } => {
                        let enabled = !disabled;
                        store
                            .set_retention_policy(&table, days, None, enabled)
                            .map_err(|e| {
                                CliError::CommandFailed(format!("Failed to set policy: {e}"))
                            })?;

                        let policy = store.get_retention_policy(&table).map_err(|e| {
                            CliError::CommandFailed(format!("Failed to fetch policy: {e}"))
                        })?;

                        if let Some(policy) = policy {
                            print_output(&policy, self.format);
                        }
                    }
                    RetentionCommands::History { limit } => {
                        let history = store.list_vacuum_history(limit).map_err(|e| {
                            CliError::CommandFailed(format!("Failed to fetch history: {e}"))
                        })?;

                        if history.is_empty() {
                            println!("No vacuum operations recorded yet");
                        } else {
                            print_output(&history, self.format);
                        }
                    }
                }
            }
            Commands::Health { command } => {
                let store = open_store(self.config.as_ref())?;

                match command {
                    HealthCommands::Freshness {
                        machine,
                        stale_threshold,
                    } => {
                        let summaries = store
                            .get_freshness_summaries(machine.as_deref(), stale_threshold)
                            .map_err(|e| {
                                CliError::CommandFailed(format!("Failed to get freshness: {e}"))
                            })?;

                        if summaries.is_empty() {
                            println!("No collector health data recorded yet");
                        } else {
                            print_output(&summaries, self.format);
                        }
                    }
                    HealthCommands::Collectors {
                        machine,
                        collector,
                        limit,
                    } => {
                        let entries = store
                            .list_collector_health(machine.as_deref(), collector.as_deref(), limit)
                            .map_err(|e| {
                                CliError::CommandFailed(format!(
                                    "Failed to list collector health: {e}"
                                ))
                            })?;

                        if entries.is_empty() {
                            println!("No collector health entries found");
                        } else {
                            print_output(&entries, self.format);
                        }
                    }
                    HealthCommands::Drift {
                        machine,
                        severity,
                        limit,
                    } => {
                        let events = store
                            .list_drift_events(machine.as_deref(), severity.as_deref(), limit)
                            .map_err(|e| {
                                CliError::CommandFailed(format!("Failed to list drift events: {e}"))
                            })?;

                        if events.is_empty() {
                            println!("No drift events detected");
                        } else {
                            print_output(&events, self.format);
                        }
                    }
                    HealthCommands::Baselines { machine } => {
                        let baselines =
                            store
                                .list_machine_baselines(machine.as_deref())
                                .map_err(|e| {
                                    CliError::CommandFailed(format!(
                                        "Failed to list baselines: {e}"
                                    ))
                                })?;

                        if baselines.is_empty() {
                            println!("No machine baselines computed yet");
                        } else {
                            print_output(&baselines, self.format);
                        }
                    }
                    HealthCommands::Score { machine } => {
                        let qb = vc_query::QueryBuilder::new(&store);

                        if let Some(machine_id) = &machine {
                            let score = qb.machine_health(machine_id).map_err(|e| {
                                CliError::CommandFailed(format!("Failed to get health score: {e}"))
                            })?;
                            print_output(&score, self.format);
                        } else {
                            let summaries = qb.list_health_summaries().map_err(|e| {
                                CliError::CommandFailed(format!(
                                    "Failed to list health summaries: {e}"
                                ))
                            })?;

                            if summaries.is_empty() {
                                println!("No health scores computed yet");
                            } else {
                                print_output(&summaries, self.format);
                            }
                        }
                    }
                }
            }
            Commands::Autopilot { command } => {
                let store = open_store(self.config.as_ref())?;

                match command {
                    AutopilotCommands::Status => {
                        use vc_guardian::autopilot::AutopilotStatus;

                        let config = match &self.config {
                            Some(path) => VcConfig::load_with_env(path)?,
                            None => VcConfig::discover_with_env()?,
                        };

                        let mode = if config.autopilot.enabled {
                            vc_guardian::autopilot::AutopilotMode::Suggest
                        } else {
                            vc_guardian::autopilot::AutopilotMode::Off
                        };

                        let decisions = store.list_autopilot_decisions(None, 1).unwrap_or_default();
                        let last_decision_at = decisions
                            .first()
                            .and_then(|d| d["decided_at"].as_str().map(String::from));

                        let summary = store.autopilot_decision_summary().unwrap_or_default();
                        let account_switches = summary
                            .iter()
                            .find(|s| s["decision_type"] == "account_switch")
                            .and_then(|s| s["total"].as_u64())
                            .unwrap_or(0);
                        let cost_alerts = summary
                            .iter()
                            .find(|s| s["decision_type"] == "cost_optimization")
                            .and_then(|s| s["total"].as_u64())
                            .unwrap_or(0);
                        let decisions_today = summary
                            .iter()
                            .filter_map(|s| s["total"].as_u64())
                            .sum::<u64>();

                        let status = AutopilotStatus {
                            mode,
                            decisions_today,
                            last_decision_at,
                            account_switches,
                            cost_alerts,
                        };
                        print_output(&status, self.format);
                    }
                    AutopilotCommands::Decisions {
                        decision_type,
                        limit,
                    } => {
                        let decisions = store
                            .list_autopilot_decisions(decision_type.as_deref(), limit)
                            .map_err(|e| {
                                CliError::CommandFailed(format!("Failed to list decisions: {e}"))
                            })?;

                        if decisions.is_empty() {
                            println!("No autopilot decisions recorded yet");
                        } else {
                            print_output(&decisions, self.format);
                        }
                    }
                    AutopilotCommands::Summary => {
                        let summary = store.autopilot_decision_summary().map_err(|e| {
                            CliError::CommandFailed(format!("Failed to get decision summary: {e}"))
                        })?;

                        if summary.is_empty() {
                            println!("No autopilot decisions recorded yet");
                        } else {
                            print_output(&summary, self.format);
                        }
                    }
                }
            }
            Commands::Knowledge { command } => {
                let store = Arc::new(open_store(self.config.as_ref())?);
                let kb = KnowledgeStore::new(store);

                match command {
                    KnowledgeCommands::Add {
                        entry_type,
                        title,
                        content,
                        summary,
                        session,
                        file,
                        lines,
                        tags,
                    } => {
                        let et: EntryType = entry_type.parse().map_err(|e: vc_knowledge::KnowledgeError| {
                            CliError::CommandFailed(e.to_string())
                        })?;

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

                        let mut entry = KnowledgeEntry::new(et, &title, &content)
                            .with_tags(tags_vec);

                        if let Some(summary) = summary {
                            entry = entry.with_summary(summary);
                        }

                        if let Some(session) = session {
                            entry = entry.with_session(session);
                        }

                        if let Some(file) = file {
                            entry = entry.with_source(file, lines);
                        }

                        let id = kb.insert(&entry)?;
                        let result = serde_json::json!({
                            "id": id,
                            "title": title,
                            "entry_type": et.as_str(),
                            "message": "Knowledge entry created successfully",
                        });
                        print_output(&result, self.format);
                    }
                    KnowledgeCommands::Search {
                        query,
                        entry_type,
                        tags,
                        limit,
                    } => {
                        let mut opts = SearchOptions::new().with_limit(limit);

                        if let Some(et_str) = entry_type {
                            let et: EntryType = et_str.parse().map_err(|e: vc_knowledge::KnowledgeError| {
                                CliError::CommandFailed(e.to_string())
                            })?;
                            opts = opts.with_type(et);
                        }

                        if let Some(tags_str) = tags {
                            let tags_vec: Vec<String> = tags_str
                                .split(',')
                                .filter_map(|s| {
                                    let trimmed = s.trim();
                                    if trimmed.is_empty() {
                                        None
                                    } else {
                                        Some(trimmed.to_string())
                                    }
                                })
                                .collect();
                            opts = opts.with_tags(tags_vec);
                        }

                        let results = kb.search(&query, &opts)?;
                        print_output(&results, self.format);
                    }
                    KnowledgeCommands::Show { id } => {
                        let entry = kb.get(id)?;
                        kb.record_view(id).ok(); // best-effort view count
                        print_output(&entry, self.format);
                    }
                    KnowledgeCommands::List { limit, entry_type } => {
                        if let Some(et_str) = entry_type {
                            let et: EntryType = et_str.parse().map_err(|e: vc_knowledge::KnowledgeError| {
                                CliError::CommandFailed(e.to_string())
                            })?;
                            let opts = SearchOptions::new().with_type(et).with_limit(limit);
                            let results = kb.search("", &opts)?;
                            print_output(&results, self.format);
                        } else {
                            let entries = kb.recent(limit)?;
                            print_output(&entries, self.format);
                        }
                    }
                    KnowledgeCommands::Top { limit } => {
                        let entries = kb.top_rated(limit)?;
                        if entries.is_empty() {
                            println!("No rated knowledge entries yet");
                        } else {
                            print_output(&entries, self.format);
                        }
                    }
                    KnowledgeCommands::Feedback {
                        id,
                        feedback_type,
                        comment,
                        session,
                    } => {
                        let ft: FeedbackType = feedback_type.parse().map_err(|e: String| {
                            CliError::CommandFailed(format!("Invalid feedback type: {e}"))
                        })?;

                        let mut feedback = KnowledgeFeedback::new(id, ft);

                        if let Some(comment) = comment {
                            feedback = feedback.with_comment(comment);
                        }

                        if let Some(session) = session {
                            feedback = feedback.with_session(session);
                        }

                        let feedback_id = kb.add_feedback(&feedback)?;
                        let result = serde_json::json!({
                            "feedback_id": feedback_id,
                            "entry_id": id,
                            "feedback_type": ft.as_str(),
                            "message": "Feedback recorded successfully",
                        });
                        print_output(&result, self.format);
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
    // Commands::Autopilot Tests
    // =============================================================================

    #[test]
    fn test_autopilot_status_parse() {
        let cli = Cli::parse_from(["vc", "autopilot", "status"]);
        if let Commands::Autopilot { command } = cli.command {
            assert!(matches!(command, AutopilotCommands::Status));
        } else {
            panic!("Expected Autopilot command");
        }
    }

    #[test]
    fn test_autopilot_decisions_parse() {
        let cli = Cli::parse_from(["vc", "autopilot", "decisions"]);
        if let Commands::Autopilot { command } = cli.command {
            if let AutopilotCommands::Decisions {
                decision_type,
                limit,
            } = command
            {
                assert!(decision_type.is_none());
                assert_eq!(limit, 20);
            } else {
                panic!("Expected Decisions subcommand");
            }
        } else {
            panic!("Expected Autopilot command");
        }
    }

    #[test]
    fn test_autopilot_decisions_with_filter_parse() {
        let cli = Cli::parse_from([
            "vc",
            "autopilot",
            "decisions",
            "--decision-type",
            "account_switch",
            "--limit",
            "5",
        ]);
        if let Commands::Autopilot { command } = cli.command {
            if let AutopilotCommands::Decisions {
                decision_type,
                limit,
            } = command
            {
                assert_eq!(decision_type.as_deref(), Some("account_switch"));
                assert_eq!(limit, 5);
            } else {
                panic!("Expected Decisions subcommand");
            }
        } else {
            panic!("Expected Autopilot command");
        }
    }

    #[test]
    fn test_autopilot_summary_parse() {
        let cli = Cli::parse_from(["vc", "autopilot", "summary"]);
        if let Commands::Autopilot { command } = cli.command {
            assert!(matches!(command, AutopilotCommands::Summary));
        } else {
            panic!("Expected Autopilot command");
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
    // Commands::Retention Tests
    // =============================================================================

    #[test]
    fn test_retention_list_parse() {
        let cli = Cli::parse_from(["vc", "retention", "list"]);
        if let Commands::Retention { command } = cli.command {
            assert!(matches!(command, RetentionCommands::List));
        } else {
            panic!("Expected Retention command");
        }
    }

    #[test]
    fn test_retention_set_parse() {
        let cli = Cli::parse_from([
            "vc",
            "retention",
            "set",
            "--table",
            "sys_samples",
            "--days",
            "30",
        ]);
        if let Commands::Retention { command } = cli.command {
            if let RetentionCommands::Set {
                table,
                days,
                disabled,
            } = command
            {
                assert_eq!(table, "sys_samples");
                assert_eq!(days, 30);
                assert!(!disabled); // default is not disabled (i.e., enabled)
            } else {
                panic!("Expected Retention set");
            }
        } else {
            panic!("Expected Retention command");
        }
    }

    #[test]
    fn test_retention_set_disabled() {
        let cli = Cli::parse_from([
            "vc",
            "retention",
            "set",
            "--table",
            "test",
            "--days",
            "7",
            "--disabled",
        ]);
        if let Commands::Retention { command } = cli.command {
            if let RetentionCommands::Set { disabled, .. } = command {
                assert!(disabled);
            } else {
                panic!("Expected Retention set");
            }
        } else {
            panic!("Expected Retention command");
        }
    }

    #[test]
    fn test_retention_history_parse() {
        let cli = Cli::parse_from(["vc", "retention", "history", "--limit", "50"]);
        if let Commands::Retention { command } = cli.command {
            if let RetentionCommands::History { limit } = command {
                assert_eq!(limit, 50);
            } else {
                panic!("Expected Retention history");
            }
        } else {
            panic!("Expected Retention command");
        }
    }

    // =============================================================================
    // Commands::Health Tests
    // =============================================================================

    #[test]
    fn test_health_freshness_parse() {
        let cli = Cli::parse_from(["vc", "health", "freshness"]);
        if let Commands::Health { command } = cli.command {
            if let HealthCommands::Freshness {
                machine,
                stale_threshold,
            } = command
            {
                assert!(machine.is_none());
                assert_eq!(stale_threshold, 600);
            } else {
                panic!("Expected Health::Freshness");
            }
        } else {
            panic!("Expected Health command");
        }
    }

    #[test]
    fn test_health_freshness_with_options() {
        let cli = Cli::parse_from([
            "vc",
            "health",
            "freshness",
            "--machine",
            "m1",
            "--stale-threshold",
            "300",
        ]);
        if let Commands::Health { command } = cli.command {
            if let HealthCommands::Freshness {
                machine,
                stale_threshold,
            } = command
            {
                assert_eq!(machine.as_deref(), Some("m1"));
                assert_eq!(stale_threshold, 300);
            } else {
                panic!("Expected Health::Freshness");
            }
        } else {
            panic!("Expected Health command");
        }
    }

    #[test]
    fn test_health_collectors_parse() {
        let cli = Cli::parse_from([
            "vc",
            "health",
            "collectors",
            "--machine",
            "m1",
            "--collector",
            "sysmoni",
            "--limit",
            "5",
        ]);
        if let Commands::Health { command } = cli.command {
            if let HealthCommands::Collectors {
                machine,
                collector,
                limit,
            } = command
            {
                assert_eq!(machine.as_deref(), Some("m1"));
                assert_eq!(collector.as_deref(), Some("sysmoni"));
                assert_eq!(limit, 5);
            } else {
                panic!("Expected Health::Collectors");
            }
        } else {
            panic!("Expected Health command");
        }
    }

    #[test]
    fn test_health_drift_parse() {
        let cli = Cli::parse_from([
            "vc",
            "health",
            "drift",
            "--severity",
            "critical",
            "--limit",
            "10",
        ]);
        if let Commands::Health { command } = cli.command {
            if let HealthCommands::Drift {
                machine,
                severity,
                limit,
            } = command
            {
                assert!(machine.is_none());
                assert_eq!(severity.as_deref(), Some("critical"));
                assert_eq!(limit, 10);
            } else {
                panic!("Expected Health::Drift");
            }
        } else {
            panic!("Expected Health command");
        }
    }

    #[test]
    fn test_health_baselines_parse() {
        let cli = Cli::parse_from(["vc", "health", "baselines", "--machine", "m1"]);
        if let Commands::Health { command } = cli.command {
            if let HealthCommands::Baselines { machine } = command {
                assert_eq!(machine.as_deref(), Some("m1"));
            } else {
                panic!("Expected Health::Baselines");
            }
        } else {
            panic!("Expected Health command");
        }
    }

    #[test]
    fn test_health_score_parse() {
        let cli = Cli::parse_from(["vc", "health", "score"]);
        if let Commands::Health { command } = cli.command {
            if let HealthCommands::Score { machine } = command {
                assert!(machine.is_none());
            } else {
                panic!("Expected Health::Score");
            }
        } else {
            panic!("Expected Health command");
        }
    }

    #[test]
    fn test_health_score_with_machine() {
        let cli = Cli::parse_from(["vc", "health", "score", "--machine", "m1"]);
        if let Commands::Health { command } = cli.command {
            if let HealthCommands::Score { machine } = command {
                assert_eq!(machine.as_deref(), Some("m1"));
            } else {
                panic!("Expected Health::Score");
            }
        } else {
            panic!("Expected Health command");
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
