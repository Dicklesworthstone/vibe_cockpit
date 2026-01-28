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
            "vc",
            "fleet",
            "migrate",
            "--from",
            "server-1",
            "--to",
            "server-2",
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
}
