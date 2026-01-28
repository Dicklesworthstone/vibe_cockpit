//! vc_cli - CLI commands for Vibe Cockpit
//!
//! This crate provides:
//! - clap-based command definitions
//! - Robot mode output formatting (JSON envelope)
//! - TOON output support
//! - All subcommands (status, tui, daemon, robot, etc.)

use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod robot;

pub use robot::{RobotEnvelope, HealthData, TriageData};

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
#[command(author, version, about = "Vibe Cockpit - Agent fleet monitoring and orchestration")]
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
        #[arg(short, long)]
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
}

/// Robot mode subcommands
#[derive(Subcommand, Debug)]
pub enum RobotCommands {
    /// Get fleet health status
    Health,

    /// Get triage recommendations
    Triage,

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

impl Cli {
    /// Run the CLI
    pub async fn run(self) -> Result<(), CliError> {
        match self.command {
            Commands::Tui => {
                println!("Starting TUI...");
                // TUI implementation will go here
            }
            Commands::Status { machine } => {
                println!("Status for {:?}", machine.unwrap_or_else(|| "all".to_string()));
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
                        let output = robot::RobotEnvelope::new(
                            "vc.robot.machines.v1",
                            serde_json::json!({ "machines": [], "warning": "not yet implemented" }),
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
            _ => {
                println!("Command not yet implemented: {:?}", self.command);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parse() {
        let cli = Cli::parse_from(["vc", "status"]);
        assert!(matches!(cli.command, Commands::Status { .. }));
    }

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
    fn test_global_format_flag() {
        let cli = Cli::parse_from(["vc", "--format", "json", "status"]);
        assert!(matches!(cli.format, OutputFormat::Json));
    }

    #[test]
    fn test_global_verbose_flag() {
        let cli = Cli::parse_from(["vc", "--verbose", "status"]);
        assert!(cli.verbose);
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
    fn test_web_port() {
        let cli = Cli::parse_from(["vc", "web", "--port", "3000"]);
        if let Commands::Web { port, .. } = cli.command {
            assert_eq!(port, 3000);
        } else {
            panic!("Expected Web command");
        }
    }
}
