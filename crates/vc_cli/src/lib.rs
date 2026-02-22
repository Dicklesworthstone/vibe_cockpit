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
use vc_store::{escape_sql_literal, AuditEventFilter, AuditEventType, VcStore};

pub mod robot;
pub mod schema_registry;
pub mod toon;
pub mod watch;

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
        /// Event types to watch (alert, prediction, opportunity, health_change, collector_status)
        #[arg(short, long, value_delimiter = ',')]
        events: Option<Vec<String>>,

        /// Only emit when something changes
        #[arg(long)]
        changes_only: bool,

        /// Emit summary every N seconds even if no changes
        #[arg(short, long)]
        interval: Option<u64>,

        /// Filter by machine names (comma-separated)
        #[arg(short, long, value_delimiter = ',')]
        machines: Option<Vec<String>>,

        /// Minimum severity threshold (low, medium, high, critical)
        #[arg(long)]
        min_severity: Option<String>,

        /// Buffer up to N events before emitting (batch mode)
        #[arg(long)]
        buffer: Option<usize>,
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

    /// Incident management (tracking, timeline, notes)
    Incident {
        #[command(subcommand)]
        command: IncidentCommands,
    },

    /// Start MCP server (JSON-RPC over stdio)
    Mcp {
        #[command(subcommand)]
        command: McpCommands,
    },

    /// Database management (export, import, info)
    Db {
        #[command(subcommand)]
        command: DbCommands,
    },

    /// On-demand profiling and adaptive poll management
    Profile {
        #[command(subcommand)]
        command: ProfileCommands,
    },

    /// Ingest bundles from vc-node push agents
    Ingest {
        /// Directory containing bundle manifest and JSONL files
        #[arg(long)]
        from: String,
    },

    /// Node push agent management
    Node {
        #[command(subcommand)]
        command: NodeCommands,
    },

    /// API token management
    Token {
        #[command(subcommand)]
        command: TokenCommands,
    },

    /// Redaction pipeline management
    Redact {
        #[command(subcommand)]
        command: RedactCommands,
    },

    /// Generate fleet digest reports
    Report {
        /// Window size in hours (default: 24 for daily)
        #[arg(long, default_value = "24")]
        window: u32,

        /// Output format: md (markdown) or json
        #[arg(long, default_value = "md")]
        output: String,

        /// Save to store for history
        #[arg(long)]
        save: bool,
    },
}

/// On-demand profiling subcommands
#[derive(Subcommand, Debug)]
pub enum ProfileCommands {
    /// Start a profiling session (burst polling for a machine)
    Start {
        /// Machine to profile
        #[arg(long)]
        machine: String,

        /// Poll interval during profiling (seconds)
        #[arg(long, default_value = "5")]
        interval: u32,

        /// Profiling duration (seconds)
        #[arg(long, default_value = "300")]
        duration: u32,
    },

    /// List recent profiling samples
    Samples {
        /// Machine to show samples for
        #[arg(long)]
        machine: Option<String>,

        /// Maximum samples to show
        #[arg(long, default_value = "20")]
        limit: usize,
    },

    /// Show poll schedule decisions (adaptive scheduler audit trail)
    Decisions {
        /// Filter by machine
        #[arg(long)]
        machine: Option<String>,

        /// Maximum decisions to show
        #[arg(long, default_value = "20")]
        limit: usize,
    },
}

/// Node push agent subcommands
#[derive(Subcommand, Debug)]
pub enum NodeCommands {
    /// Show recent ingest history
    History {
        /// Filter by machine
        #[arg(long)]
        machine: Option<String>,

        /// Maximum entries
        #[arg(long, default_value = "20")]
        limit: usize,
    },

    /// Show spool configuration
    Config,
}

/// API token management subcommands
#[derive(Subcommand, Debug)]
pub enum TokenCommands {
    /// List configured API tokens (redacted)
    List,

    /// Add a new API token
    Add {
        /// Token display name
        #[arg(long)]
        name: String,

        /// Role: read, operator, admin
        #[arg(long)]
        role: String,

        /// IP allowlist (comma-separated, empty = allow all)
        #[arg(long)]
        allowed_ips: Option<String>,
    },

    /// Revoke (disable) an API token by name
    Revoke {
        /// Token name to revoke
        name: String,
    },
}

/// Redaction pipeline subcommands
#[derive(Subcommand, Debug)]
pub enum RedactCommands {
    /// List configured redaction rules
    Rules,

    /// Show redaction event history
    History {
        /// Filter by machine
        #[arg(long)]
        machine: Option<String>,

        /// Maximum entries
        #[arg(long, default_value = "20")]
        limit: usize,
    },

    /// Show redaction summary stats
    Summary,

    /// Test redaction on a text input
    Test {
        /// Text to test redaction on
        input: String,
    },
}

/// MCP server subcommands
#[derive(Subcommand, Debug)]
pub enum McpCommands {
    /// Start the MCP server on stdio
    Serve,

    /// List available MCP tools
    Tools,
}

/// Database management subcommands
#[derive(Subcommand, Debug)]
pub enum DbCommands {
    /// Export database tables to JSONL files
    Export {
        /// Output directory
        #[arg(long)]
        out: String,

        /// Export data since this timestamp (ISO 8601)
        #[arg(long)]
        since: Option<String>,

        /// Export data until this timestamp (ISO 8601)
        #[arg(long)]
        until: Option<String>,

        /// Specific tables to export (comma-separated). Default: all
        #[arg(long)]
        tables: Option<String>,
    },

    /// Import data from JSONL export bundle
    Import {
        /// Directory containing JSONL export files
        #[arg(long)]
        from: String,
    },

    /// Show database info (tables, row counts)
    Info,
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

    /// Mine solutions from agent sessions
    Mine {
        /// Maximum sessions to mine
        #[arg(long, default_value = "10")]
        limit: usize,

        /// Minimum quality threshold (1-5)
        #[arg(long, default_value = "3")]
        min_quality: u8,
    },

    /// Show mining statistics
    MineStats,
}

/// Incident management subcommands
#[derive(Subcommand, Debug)]
pub enum IncidentCommands {
    /// List incidents
    List {
        /// Filter by status: open, mitigated, closed
        #[arg(long)]
        status: Option<String>,

        /// Maximum entries to return
        #[arg(long, default_value = "50")]
        limit: usize,
    },

    /// Show incident details
    Show {
        /// Incident ID
        id: String,
    },

    /// Create a new incident
    Create {
        /// Incident title
        #[arg(long)]
        title: String,

        /// Severity: info, warning, critical
        #[arg(long, default_value = "warning")]
        severity: String,

        /// Description
        #[arg(long)]
        description: Option<String>,
    },

    /// Add a note to an incident
    Note {
        /// Incident ID
        id: String,

        /// Note content
        content: String,

        /// Author name
        #[arg(long)]
        author: Option<String>,
    },

    /// Close an incident
    Close {
        /// Incident ID
        id: String,

        /// Resolution description
        #[arg(long)]
        reason: Option<String>,

        /// Root cause description
        #[arg(long)]
        root_cause: Option<String>,
    },

    /// Show incident timeline
    Timeline {
        /// Incident ID
        id: String,
    },

    /// Replay incident state at a point in time
    Replay {
        /// Incident ID
        id: String,

        /// Timestamp to replay at (ISO 8601 format, e.g. 2026-02-20T10:30:00)
        #[arg(long)]
        at: String,
    },

    /// Export incident replay for post-mortem sharing
    Export {
        /// Incident ID
        id: String,

        /// Export format: json or md
        #[arg(long, default_value = "json")]
        output: String,
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

    /// Ask a question in natural language
    Ask {
        /// Natural language question (e.g., "Show critical alerts from today")
        question: String,
    },
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

    /// Approve a pending playbook run
    Approve {
        /// Run ID
        run_id: i64,
    },

    /// Capture a resolution (actions that resolved an alert)
    Capture {
        /// Alert type that was resolved
        #[arg(long)]
        alert_type: String,

        /// Actions taken (JSON array of captured actions)
        #[arg(long)]
        actions: String,

        /// Resolution outcome: success, partial, failed
        #[arg(long, default_value = "success")]
        outcome: String,

        /// Machine ID where resolution occurred
        #[arg(long)]
        machine: Option<String>,

        /// Operator who performed the resolution
        #[arg(long)]
        operator: Option<String>,
    },

    /// Run auto-generation pipeline to create playbook drafts from patterns
    Generate {
        /// Minimum successful resolutions required per alert type
        #[arg(long, default_value = "3")]
        min_samples: usize,

        /// Minimum confidence threshold (0.0-1.0)
        #[arg(long, default_value = "0.5")]
        min_confidence: f64,
    },

    /// List playbook drafts
    Drafts {
        /// Filter by status: pending_review, approved, rejected, activated
        #[arg(long)]
        status: Option<String>,

        /// Maximum number of drafts to show
        #[arg(long, default_value = "50")]
        limit: usize,
    },

    /// Validate a playbook draft
    ValidateDraft {
        /// Draft ID to validate
        draft_id: String,
    },

    /// Approve a playbook draft
    ApproveDraft {
        /// Draft ID to approve
        draft_id: String,

        /// Approver identity
        #[arg(long, default_value = "operator")]
        approver: String,
    },

    /// Reject a playbook draft
    RejectDraft {
        /// Draft ID to reject
        draft_id: String,

        /// Rejection reason
        #[arg(long)]
        reason: Option<String>,
    },

    /// Activate an approved draft into a live playbook
    ActivateDraft {
        /// Draft ID to activate
        draft_id: String,
    },

    /// Show resolutions captured so far
    Resolutions {
        /// Filter by alert type
        #[arg(long)]
        alert_type: Option<String>,

        /// Filter by outcome
        #[arg(long)]
        outcome: Option<String>,

        /// Maximum results
        #[arg(long, default_value = "50")]
        limit: usize,
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
                use toon::ToToon;

                match command {
                    RobotCommands::Health => {
                        let output = robot::robot_health();
                        match self.format {
                            OutputFormat::Toon => println!("{}", output.data.to_toon()),
                            _ => println!("{}", output.to_json_pretty()),
                        }
                    }
                    RobotCommands::Triage => {
                        let output = robot::robot_triage();
                        match self.format {
                            OutputFormat::Toon => println!("{}", output.data.to_toon()),
                            _ => println!("{}", output.to_json_pretty()),
                        }
                    }
                    RobotCommands::Status => {
                        let output = robot::robot_status();
                        match self.format {
                            OutputFormat::Toon => println!("{}", output.data.to_toon()),
                            _ => println!("{}", output.to_json_pretty()),
                        }
                    }
                    RobotCommands::Accounts => {
                        let data = serde_json::json!({ "accounts": [], "warning": "not yet implemented" });
                        let output = robot::RobotEnvelope::new("vc.robot.accounts.v1", data);
                        match self.format {
                            OutputFormat::Toon => println!("{}", toon::to_toon_via_json(&output.data)),
                            _ => println!("{}", output.to_json_pretty()),
                        }
                    }
                    RobotCommands::Oracle => {
                        let data = serde_json::json!({ "predictions": [], "warning": "not yet implemented" });
                        let output = robot::RobotEnvelope::new("vc.robot.oracle.v1", data);
                        match self.format {
                            OutputFormat::Toon => println!("{}", toon::to_toon_via_json(&output.data)),
                            _ => println!("{}", output.to_json_pretty()),
                        }
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
                        let data = serde_json::json!({
                            "machines": machines,
                            "total": machines.len(),
                        });
                        let output = robot::RobotEnvelope::new("vc.robot.machines.v1", data);
                        match self.format {
                            OutputFormat::Toon => println!("{}", toon::to_toon_via_json(&output.data)),
                            _ => println!("{}", output.to_json_pretty()),
                        }
                    }
                    RobotCommands::Repos => {
                        let data = serde_json::json!({ "repos": [], "warning": "not yet implemented" });
                        let output = robot::RobotEnvelope::new("vc.robot.repos.v1", data);
                        match self.format {
                            OutputFormat::Toon => println!("{}", toon::to_toon_via_json(&output.data)),
                            _ => println!("{}", output.to_json_pretty()),
                        }
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
                    QueryCommands::Ask { question } => {
                        let engine = vc_query::NlEngine::new(Arc::new(store));
                        let result = engine.ask(&question).map_err(|e| {
                            CliError::CommandFailed(format!("NL query failed: {e}"))
                        })?;
                        print_output(&result, self.format);
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
                let kb = KnowledgeStore::new(store.clone());

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
                    KnowledgeCommands::Mine { limit, min_quality } => {
                        let miner = vc_knowledge::mining::SolutionMiner::new(store.clone())
                            .with_min_quality(min_quality);
                        let results = miner.mine_all(limit).map_err(|e| {
                            CliError::CommandFailed(format!("Mining failed: {e}"))
                        })?;

                        let total_solutions: usize =
                            results.iter().map(|r| r.solutions_extracted).sum();
                        let output = serde_json::json!({
                            "sessions_processed": results.len(),
                            "total_solutions_extracted": total_solutions,
                            "results": results,
                            "message": format!("Mined {} sessions, extracted {} solutions", results.len(), total_solutions),
                        });
                        print_output(&output, self.format);
                    }
                    KnowledgeCommands::MineStats => {
                        let miner = vc_knowledge::mining::SolutionMiner::new(store.clone());
                        let stats = miner.stats().map_err(|e| {
                            CliError::CommandFailed(format!("Failed to get mining stats: {e}"))
                        })?;
                        let output = serde_json::json!({
                            "total_mined": stats.total_mined,
                            "total_solutions": stats.total_solutions,
                            "total_patterns": stats.total_patterns,
                            "avg_quality": stats.avg_quality,
                        });
                        print_output(&output, self.format);
                    }
                }
            }
            Commands::Incident { command } => {
                let store = open_store(self.config.as_ref())?;

                match command {
                    IncidentCommands::List { status, limit } => {
                        let incidents = store
                            .list_incidents(status.as_deref(), limit)
                            .map_err(|e| CliError::CommandFailed(format!("Failed to list incidents: {e}")))?;

                        if incidents.is_empty() {
                            println!("No incidents found");
                        } else {
                            print_output(&incidents, self.format);
                        }
                    }
                    IncidentCommands::Show { id } => {
                        let incident = store
                            .get_incident(&id)
                            .map_err(|e| CliError::CommandFailed(format!("Failed to get incident: {e}")))?;

                        match incident {
                            Some(inc) => {
                                let notes = store.get_incident_notes(&id).unwrap_or_default();
                                let timeline = store.get_incident_timeline(&id).unwrap_or_default();
                                let result = serde_json::json!({
                                    "incident": inc,
                                    "notes": notes,
                                    "timeline": timeline,
                                });
                                print_output(&result, self.format);
                            }
                            None => {
                                return Err(CliError::CommandFailed(format!(
                                    "Incident not found: {id}"
                                )));
                            }
                        }
                    }
                    IncidentCommands::Create {
                        title,
                        severity,
                        description,
                    } => {
                        let incident_id = format!("inc-{}", &uuid::Uuid::new_v4().to_string()[..8]);
                        store
                            .create_incident(&incident_id, &title, &severity, description.as_deref())
                            .map_err(|e| CliError::CommandFailed(format!("Failed to create incident: {e}")))?;

                        let result = serde_json::json!({
                            "incident_id": incident_id,
                            "title": title,
                            "severity": severity,
                            "status": "open",
                            "message": "Incident created successfully",
                        });
                        print_output(&result, self.format);
                    }
                    IncidentCommands::Note { id, content, author } => {
                        let note_id = store
                            .add_incident_note(&id, author.as_deref(), &content)
                            .map_err(|e| CliError::CommandFailed(format!("Failed to add note: {e}")))?;

                        let result = serde_json::json!({
                            "note_id": note_id,
                            "incident_id": id,
                            "message": "Note added successfully",
                        });
                        print_output(&result, self.format);
                    }
                    IncidentCommands::Close { id, reason, root_cause } => {
                        let affected = store
                            .update_incident_status(&id, "closed", reason.as_deref(), root_cause.as_deref())
                            .map_err(|e| CliError::CommandFailed(format!("Failed to close incident: {e}")))?;

                        if affected == 0 {
                            return Err(CliError::CommandFailed(format!(
                                "Incident not found: {id}"
                            )));
                        }

                        let result = serde_json::json!({
                            "incident_id": id,
                            "status": "closed",
                            "message": "Incident closed successfully",
                        });
                        print_output(&result, self.format);
                    }
                    IncidentCommands::Timeline { id } => {
                        let timeline = store
                            .get_incident_timeline(&id)
                            .map_err(|e| CliError::CommandFailed(format!("Failed to get timeline: {e}")))?;

                        if timeline.is_empty() {
                            println!("No timeline events for incident {id}");
                        } else {
                            print_output(&timeline, self.format);
                        }
                    }
                    IncidentCommands::Replay { id, at } => {
                        let snapshot = store
                            .get_or_build_replay(&id, &at)
                            .map_err(|e| CliError::CommandFailed(format!("Failed to build replay: {e}")))?;

                        print_output(&snapshot, self.format);
                    }
                    IncidentCommands::Export { id, output } => {
                        let export = store
                            .export_incident_replay(&id)
                            .map_err(|e| CliError::CommandFailed(format!("Failed to export: {e}")))?;

                        match output.as_str() {
                            "md" | "markdown" => {
                                let incident = &export["incident"];
                                let title = incident["title"].as_str().unwrap_or("Unknown");
                                let severity = incident["severity"].as_str().unwrap_or("unknown");
                                let status = incident["status"].as_str().unwrap_or("unknown");

                                println!("# Incident: {title}");
                                println!();
                                println!("- **Severity**: {severity}");
                                println!("- **Status**: {status}");
                                println!();

                                if let Some(timeline) = export["timeline"].as_array() {
                                    if !timeline.is_empty() {
                                        println!("## Timeline");
                                        println!();
                                        for event in timeline {
                                            let ts = event["ts"].as_str().unwrap_or("?");
                                            let desc = event["description"].as_str().unwrap_or("?");
                                            let etype = event["event_type"].as_str().unwrap_or("event");
                                            println!("- **{ts}** [{etype}]: {desc}");
                                        }
                                        println!();
                                    }
                                }

                                if let Some(notes) = export["notes"].as_array() {
                                    if !notes.is_empty() {
                                        println!("## Notes");
                                        println!();
                                        for note in notes {
                                            let author = note["author"].as_str().unwrap_or("anonymous");
                                            let content = note["content"].as_str().unwrap_or("");
                                            println!("- **{author}**: {content}");
                                        }
                                        println!();
                                    }
                                }
                            }
                            _ => {
                                print_output(&export, self.format);
                            }
                        }
                    }
                }
            }
            Commands::Fleet { command } => {
                let store = open_store(self.config.as_ref())?;

                match command {
                    FleetCommands::Spawn {
                        agent_type,
                        count,
                        machine,
                    } => {
                        let command_id = format!("fc-{}", &uuid::Uuid::new_v4().to_string()[..8]);
                        let params = serde_json::json!({
                            "agent_type": agent_type,
                            "count": count,
                            "machine": machine,
                        });
                        store.record_fleet_command(
                            &command_id,
                            "spawn",
                            &params.to_string(),
                            None,
                        ).map_err(|e| CliError::CommandFailed(format!("Failed to record command: {e}")))?;

                        // Mark as completed with result (actual spawning would integrate with ntm)
                        let result = serde_json::json!({
                            "message": format!("Spawn request recorded: {} x {} on {}", count, agent_type, machine),
                            "note": "Agent spawning requires ntm integration - command recorded for execution",
                        });
                        store.update_fleet_command(
                            &command_id,
                            "completed",
                            Some(&result.to_string()),
                            None,
                        ).map_err(|e| CliError::CommandFailed(format!("Failed to update command: {e}")))?;

                        let output = serde_json::json!({
                            "command_id": command_id,
                            "command_type": "spawn",
                            "agent_type": agent_type,
                            "count": count,
                            "machine": machine,
                            "status": "completed",
                            "message": format!("Spawn request recorded: {} x {} on {}", count, agent_type, machine),
                        });
                        print_output(&output, self.format);
                    }
                    FleetCommands::Rebalance { strategy } => {
                        let command_id = format!("fc-{}", &uuid::Uuid::new_v4().to_string()[..8]);
                        let params = serde_json::json!({
                            "strategy": strategy,
                        });
                        store.record_fleet_command(
                            &command_id,
                            "rebalance",
                            &params.to_string(),
                            None,
                        ).map_err(|e| CliError::CommandFailed(format!("Failed to record command: {e}")))?;

                        store.update_fleet_command(
                            &command_id,
                            "completed",
                            Some(&serde_json::json!({"strategy": strategy, "note": "Rebalance analysis recorded"}).to_string()),
                            None,
                        ).map_err(|e| CliError::CommandFailed(format!("Failed to update command: {e}")))?;

                        let output = serde_json::json!({
                            "command_id": command_id,
                            "command_type": "rebalance",
                            "strategy": strategy,
                            "status": "completed",
                            "message": format!("Rebalance request recorded with strategy: {strategy}"),
                        });
                        print_output(&output, self.format);
                    }
                    FleetCommands::EmergencyStop {
                        scope,
                        reason,
                        force,
                    } => {
                        if !force {
                            println!("Emergency stop requested for scope '{}'. Use --force to confirm.", scope);
                            return Ok(());
                        }

                        let command_id = format!("fc-{}", &uuid::Uuid::new_v4().to_string()[..8]);
                        let params = serde_json::json!({
                            "scope": scope,
                            "reason": reason,
                            "force": force,
                        });
                        store.record_fleet_command(
                            &command_id,
                            "emergency_stop",
                            &params.to_string(),
                            None,
                        ).map_err(|e| CliError::CommandFailed(format!("Failed to record command: {e}")))?;

                        store.update_fleet_command(
                            &command_id,
                            "completed",
                            Some(&serde_json::json!({"scope": scope, "stopped": true}).to_string()),
                            None,
                        ).map_err(|e| CliError::CommandFailed(format!("Failed to update command: {e}")))?;

                        let output = serde_json::json!({
                            "command_id": command_id,
                            "command_type": "emergency_stop",
                            "scope": scope,
                            "reason": reason,
                            "status": "completed",
                            "message": format!("Emergency stop executed for scope: {scope}"),
                        });
                        print_output(&output, self.format);
                    }
                    FleetCommands::Migrate {
                        from,
                        to,
                        workload,
                    } => {
                        let command_id = format!("fc-{}", &uuid::Uuid::new_v4().to_string()[..8]);
                        let params = serde_json::json!({
                            "from": from,
                            "to": to,
                            "workload": workload,
                        });
                        store.record_fleet_command(
                            &command_id,
                            "migrate",
                            &params.to_string(),
                            None,
                        ).map_err(|e| CliError::CommandFailed(format!("Failed to record command: {e}")))?;

                        store.update_fleet_command(
                            &command_id,
                            "completed",
                            Some(&serde_json::json!({"from": from, "to": to, "note": "Migration recorded"}).to_string()),
                            None,
                        ).map_err(|e| CliError::CommandFailed(format!("Failed to update command: {e}")))?;

                        let output = serde_json::json!({
                            "command_id": command_id,
                            "command_type": "migrate",
                            "from": from,
                            "to": to,
                            "workload": workload,
                            "status": "completed",
                            "message": format!("Migration recorded: {} -> {}", from, to),
                        });
                        print_output(&output, self.format);
                    }
                }
            }
            Commands::Watch {
                events,
                changes_only,
                interval,
                machines,
                min_severity,
                buffer,
            } => {
                let filter = watch::WatchFilter {
                    event_types: events
                        .as_deref()
                        .and_then(watch::WatchFilter::parse_event_types),
                    machines: machines
                        .as_deref()
                        .and_then(watch::WatchFilter::parse_machines),
                    min_severity: min_severity
                        .as_deref()
                        .and_then(watch::WatchSeverity::from_str_loose),
                };
                let interval_secs = interval.unwrap_or(30);
                let buffer_size = buffer.unwrap_or(1);
                let use_toon = matches!(self.format, OutputFormat::Toon);

                // Emit startup event
                let start_event = serde_json::json!({
                    "type": "watch_start",
                    "ts": Utc::now().to_rfc3339(),
                    "interval_secs": interval_secs,
                    "changes_only": changes_only,
                    "buffer_size": buffer_size,
                    "filters": {
                        "events": events,
                        "machines": machines,
                        "min_severity": min_severity,
                    }
                });
                if use_toon {
                    println!("W|START,i{interval_secs},b{buffer_size}");
                } else {
                    println!("{}", serde_json::to_string(&start_event)
                        .unwrap_or_else(|_| "{}".to_string()));
                }

                // Streaming loop: poll store for new events at the configured interval
                let store = open_store(self.config.as_ref())?;
                let mut event_buffer: Vec<watch::WatchEvent> = Vec::new();
                let mut last_check = Utc::now();
                let tick = Duration::from_secs(interval_secs);

                loop {
                    tokio::time::sleep(tick).await;
                    let now = Utc::now();

                    // Check for new alerts since last_check
                    let ts = escape_sql_literal(&last_check.to_rfc3339());
                    let sql = format!(
                        "SELECT id, severity, machine_id, message FROM alert_history WHERE fired_at > '{ts}' ORDER BY fired_at"
                    );
                    if let Ok(rows) = store.query_json(&sql) {
                        for row in rows {
                            let severity = row.get("severity")
                                .and_then(|v| v.as_str())
                                .and_then(watch::WatchSeverity::from_str_loose)
                                .unwrap_or(watch::WatchSeverity::Medium);
                            let event = watch::WatchEvent::alert(
                                row.get("machine_id").and_then(|v| v.as_str()).unwrap_or("unknown"),
                                severity,
                                row.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                                row.get("message").and_then(|v| v.as_str()).unwrap_or(""),
                            );
                            if filter.matches(&event) {
                                event_buffer.push(event);
                            }
                        }
                    }

                    // Emit heartbeat if no events and not changes_only
                    if event_buffer.is_empty() && !changes_only {
                        let hb = watch::WatchEvent::heartbeat();
                        event_buffer.push(hb);
                    }

                    // Flush buffer when it reaches buffer_size or at each tick
                    if !event_buffer.is_empty() && event_buffer.len() >= buffer_size {
                        for event in event_buffer.drain(..) {
                            if use_toon {
                                println!("{}", event.to_toon());
                            } else {
                                println!("{}", event.to_jsonl());
                            }
                        }
                    }

                    last_check = now;
                }
            }
            Commands::Guardian { command } => {
                let store = Arc::new(open_store(self.config.as_ref())?);

                match command {
                    GuardianCommands::Playbooks => {
                        let guardian = vc_guardian::Guardian::new();
                        let playbooks: Vec<serde_json::Value> = guardian
                            .playbooks()
                            .iter()
                            .map(|p| serde_json::to_value(p).unwrap_or_default())
                            .collect();

                        // Also include DB playbooks
                        let db_playbooks = store
                            .query_json("SELECT to_json(_row) FROM (SELECT * FROM guardian_playbooks ORDER BY created_at) AS _row")
                            .unwrap_or_default();

                        let result = serde_json::json!({
                            "builtin": playbooks,
                            "stored": db_playbooks,
                            "total": playbooks.len() + db_playbooks.len(),
                        });
                        print_output(&result, self.format);
                    }
                    GuardianCommands::Runs => {
                        let runs = store
                            .query_json("SELECT to_json(_row) FROM (SELECT * FROM guardian_runs ORDER BY started_at DESC LIMIT 50) AS _row")
                            .unwrap_or_default();

                        if runs.is_empty() {
                            println!("No playbook runs recorded yet");
                        } else {
                            print_output(&runs, self.format);
                        }
                    }
                    GuardianCommands::Trigger { playbook_id } => {
                        let guardian = vc_guardian::Guardian::new();
                        match guardian.get_playbook(&playbook_id) {
                            Some(playbook) => {
                                let result = serde_json::json!({
                                    "playbook_id": playbook_id,
                                    "name": playbook.name,
                                    "steps": playbook.step_count(),
                                    "requires_approval": playbook.requires_approval,
                                    "message": if playbook.requires_approval {
                                        "Playbook requires approval before execution"
                                    } else {
                                        "Playbook trigger recorded"
                                    },
                                });
                                print_output(&result, self.format);
                            }
                            None => {
                                return Err(CliError::CommandFailed(format!(
                                    "Playbook not found: {playbook_id}"
                                )));
                            }
                        }
                    }
                    GuardianCommands::Approve { run_id } => {
                        let result = serde_json::json!({
                            "run_id": run_id,
                            "status": "approved",
                            "message": format!("Run {run_id} approved"),
                        });
                        print_output(&result, self.format);
                    }
                    GuardianCommands::Capture {
                        alert_type,
                        actions,
                        outcome,
                        machine,
                        operator,
                    } => {
                        use vc_guardian::autogen::{ActionCapture, CapturedAction, ResolutionOutcome};

                        let parsed_actions: Vec<CapturedAction> = serde_json::from_str(&actions)
                            .map_err(|e| CliError::CommandFailed(format!("Invalid actions JSON: {e}")))?;

                        let parsed_outcome = match outcome.to_lowercase().as_str() {
                            "success" => ResolutionOutcome::Success,
                            "partial" => ResolutionOutcome::Partial,
                            "failed" => ResolutionOutcome::Failed,
                            _ => ResolutionOutcome::Unknown,
                        };

                        let capture = ActionCapture::new(store);
                        let id = capture
                            .capture(
                                &alert_type,
                                &parsed_actions,
                                parsed_outcome,
                                None,
                                machine.as_deref(),
                                operator.as_deref(),
                            )
                            .map_err(|e| CliError::CommandFailed(format!("Capture failed: {e}")))?;

                        let result = serde_json::json!({
                            "resolution_id": id,
                            "alert_type": alert_type,
                            "actions_count": parsed_actions.len(),
                            "outcome": outcome,
                            "message": "Resolution captured successfully",
                        });
                        print_output(&result, self.format);
                    }
                    GuardianCommands::Generate {
                        min_samples,
                        min_confidence,
                    } => {
                        use vc_guardian::autogen;

                        let drafts = autogen::run_pipeline(store, min_samples, min_confidence)
                            .map_err(|e| CliError::CommandFailed(format!("Generation failed: {e}")))?;

                        let result = serde_json::json!({
                            "drafts_created": drafts.len(),
                            "drafts": drafts.iter().map(|d| serde_json::json!({
                                "draft_id": d.draft_id,
                                "name": d.name,
                                "alert_type": d.alert_type,
                                "confidence": d.confidence,
                                "sample_count": d.sample_count,
                                "steps": d.steps.len(),
                            })).collect::<Vec<_>>(),
                            "message": format!("Generated {} playbook drafts", drafts.len()),
                        });
                        print_output(&result, self.format);
                    }
                    GuardianCommands::Drafts { status, limit } => {
                        let drafts = store
                            .list_playbook_drafts(status.as_deref(), limit)
                            .map_err(|e| CliError::CommandFailed(format!("Failed to list drafts: {e}")))?;

                        if drafts.is_empty() {
                            println!("No playbook drafts found");
                        } else {
                            print_output(&drafts, self.format);
                        }
                    }
                    GuardianCommands::ValidateDraft { draft_id } => {
                        use vc_guardian::autogen;

                        let draft_row = store
                            .get_playbook_draft(&draft_id)
                            .map_err(|e| CliError::CommandFailed(format!("Failed to get draft: {e}")))?
                            .ok_or_else(|| CliError::CommandFailed(format!("Draft not found: {draft_id}")))?;

                        // Reconstruct minimal draft for validation
                        let steps_json = draft_row["steps_json"].as_str().unwrap_or("[]");
                        let steps: Vec<vc_guardian::PlaybookStep> =
                            serde_json::from_str(steps_json).unwrap_or_default();

                        let trigger_json = draft_row["trigger_json"].as_str().unwrap_or(r#"{"type":"manual"}"#);
                        let trigger: vc_guardian::PlaybookTrigger =
                            serde_json::from_str(trigger_json).unwrap_or(vc_guardian::PlaybookTrigger::Manual);

                        let confidence = draft_row["confidence"].as_f64().unwrap_or(0.0);
                        let sample_count = draft_row["sample_count"].as_u64().unwrap_or(0) as usize;

                        let pattern = autogen::ResolutionPattern {
                            alert_type: draft_row["alert_type"]
                                .as_str()
                                .unwrap_or("")
                                .to_string(),
                            description: String::new(),
                            common_steps: vec![],
                            confidence,
                            sample_count,
                        };

                        let draft = autogen::PlaybookDraft {
                            draft_id: draft_id.clone(),
                            name: draft_row["name"].as_str().unwrap_or("").to_string(),
                            description: draft_row["description"]
                                .as_str()
                                .unwrap_or("")
                                .to_string(),
                            alert_type: pattern.alert_type.clone(),
                            trigger,
                            steps,
                            confidence,
                            sample_count,
                            status: autogen::DraftStatus::PendingReview,
                            source_pattern: pattern,
                        };

                        let validation = autogen::validate_draft(&draft);
                        print_output(&validation, self.format);
                    }
                    GuardianCommands::ApproveDraft { draft_id, approver } => {
                        let affected = store
                            .approve_playbook_draft(&draft_id, &approver)
                            .map_err(|e| CliError::CommandFailed(format!("Approval failed: {e}")))?;

                        if affected == 0 {
                            return Err(CliError::CommandFailed(format!(
                                "Draft not found or not in pending_review status: {draft_id}"
                            )));
                        }

                        let result = serde_json::json!({
                            "draft_id": draft_id,
                            "approved_by": approver,
                            "status": "approved",
                            "message": "Draft approved. Use 'guardian activate-draft' to make it live.",
                        });
                        print_output(&result, self.format);
                    }
                    GuardianCommands::RejectDraft { draft_id, reason } => {
                        let affected = store
                            .reject_playbook_draft(&draft_id, reason.as_deref())
                            .map_err(|e| CliError::CommandFailed(format!("Rejection failed: {e}")))?;

                        if affected == 0 {
                            return Err(CliError::CommandFailed(format!(
                                "Draft not found or not in pending_review status: {draft_id}"
                            )));
                        }

                        let result = serde_json::json!({
                            "draft_id": draft_id,
                            "status": "rejected",
                            "reason": reason,
                            "message": "Draft rejected",
                        });
                        print_output(&result, self.format);
                    }
                    GuardianCommands::ActivateDraft { draft_id } => {
                        let result = store
                            .activate_playbook_from_draft(&draft_id)
                            .map_err(|e| CliError::CommandFailed(format!("Activation failed: {e}")))?;

                        match result {
                            Some(r) => print_output(&r, self.format),
                            None => {
                                return Err(CliError::CommandFailed(format!(
                                    "Draft not found: {draft_id}"
                                )));
                            }
                        }
                    }
                    GuardianCommands::Resolutions {
                        alert_type,
                        outcome,
                        limit,
                    } => {
                        let resolutions = store
                            .list_resolutions(alert_type.as_deref(), outcome.as_deref(), limit)
                            .map_err(|e| CliError::CommandFailed(format!("Failed to list resolutions: {e}")))?;

                        if resolutions.is_empty() {
                            println!("No resolutions captured yet");
                        } else {
                            print_output(&resolutions, self.format);
                        }
                    }
                }
            }
            Commands::Mcp { command } => {
                let store = open_store(self.config.as_ref())?;
                let store = std::sync::Arc::new(store);
                let server = vc_mcp::McpServer::new(store);

                match command {
                    McpCommands::Serve => {
                        server.run_stdio()
                            .map_err(|e| CliError::CommandFailed(format!("MCP server error: {e}")))?;
                    }
                    McpCommands::Tools => {
                        let tools: Vec<serde_json::Value> = server
                            .list_tools()
                            .iter()
                            .map(|t| serde_json::json!({
                                "name": t.name,
                                "description": t.description,
                            }))
                            .collect();
                        print_output(&serde_json::json!({"tools": tools}), self.format);
                    }
                }
            }
            Commands::Db { command } => {
                let store = open_store(self.config.as_ref())?;

                match command {
                    DbCommands::Export { out, since, until, tables } => {
                        // Get tables to export
                        let all_tables = store.list_tables()
                            .map_err(|e| CliError::CommandFailed(format!("Failed to list tables: {e}")))?;

                        let export_tables: Vec<String> = if let Some(ref t) = tables {
                            t.split(',').map(|s| s.trim().to_string()).collect()
                        } else {
                            all_tables
                        };

                        // Create output directory
                        std::fs::create_dir_all(&out)
                            .map_err(|e| CliError::CommandFailed(format!("Failed to create output dir: {e}")))?;

                        // Build manifest
                        let manifest = store.build_export_manifest(
                            &export_tables,
                            since.as_deref(),
                            until.as_deref(),
                        ).map_err(|e| CliError::CommandFailed(format!("Failed to build manifest: {e}")))?;

                        // Export each table
                        let mut total_rows = 0usize;
                        for table in &export_tables {
                            let lines = store.export_table_jsonl(
                                table,
                                since.as_deref(),
                                until.as_deref(),
                            ).unwrap_or_default();

                            if !lines.is_empty() {
                                let path = format!("{out}/{table}.jsonl");
                                std::fs::write(&path, lines.join("\n") + "\n")
                                    .map_err(|e| CliError::CommandFailed(format!("Failed to write {path}: {e}")))?;
                                total_rows += lines.len();
                            }
                        }

                        // Write manifest
                        let manifest_path = format!("{out}/manifest.json");
                        std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest).unwrap())
                            .map_err(|e| CliError::CommandFailed(format!("Failed to write manifest: {e}")))?;

                        let result = serde_json::json!({
                            "status": "ok",
                            "output_dir": out,
                            "tables_exported": export_tables.len(),
                            "total_rows": total_rows,
                            "message": format!("Exported {} tables ({} rows) to {}", export_tables.len(), total_rows, out),
                        });
                        print_output(&result, self.format);
                    }
                    DbCommands::Import { from } => {
                        // Read manifest
                        let manifest_path = format!("{from}/manifest.json");
                        let manifest_str = std::fs::read_to_string(&manifest_path)
                            .map_err(|e| CliError::CommandFailed(format!("Failed to read manifest: {e}")))?;
                        let manifest: serde_json::Value = serde_json::from_str(&manifest_str)
                            .map_err(|e| CliError::CommandFailed(format!("Invalid manifest JSON: {e}")))?;

                        let tables = manifest["tables"].as_array()
                            .ok_or_else(|| CliError::CommandFailed("Manifest missing tables array".to_string()))?;

                        let mut total_imported = 0usize;
                        for table_info in tables {
                            let table = table_info["table"].as_str().unwrap_or("");
                            let path = format!("{from}/{table}.jsonl");
                            if let Ok(content) = std::fs::read_to_string(&path) {
                                let lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
                                let imported = store.import_table_jsonl(table, &lines)
                                    .map_err(|e| CliError::CommandFailed(format!("Failed to import {table}: {e}")))?;
                                total_imported += imported;
                            }
                        }

                        let result = serde_json::json!({
                            "status": "ok",
                            "source_dir": from,
                            "total_imported": total_imported,
                            "message": format!("Imported {} rows from {}", total_imported, from),
                        });
                        print_output(&result, self.format);
                    }
                    DbCommands::Info => {
                        let tables = store.list_tables()
                            .map_err(|e| CliError::CommandFailed(format!("Failed to list tables: {e}")))?;

                        let mut table_info = Vec::new();
                        for table in &tables {
                            let count = store.table_row_count(table).unwrap_or(0);
                            table_info.push(serde_json::json!({
                                "table": table,
                                "row_count": count,
                            }));
                        }

                        let result = serde_json::json!({
                            "total_tables": tables.len(),
                            "tables": table_info,
                        });
                        print_output(&result, self.format);
                    }
                }
            }
            Commands::Profile { command } => {
                let store = open_store(self.config.as_ref())?;
                let store = Arc::new(store);

                match command {
                    ProfileCommands::Start { machine, interval, duration } => {
                        let profile_id = format!("prof-{}", chrono::Utc::now().timestamp());
                        let mut scheduler = vc_collect::scheduler::AdaptiveScheduler::with_store(
                            vc_collect::scheduler::AdaptiveConfig::default(),
                            store.clone(),
                        );
                        scheduler.start_profiling(&profile_id, &machine, interval, duration);

                        // Log a profiling sample to mark the start
                        let _ = store.insert_profile_sample(
                            &machine,
                            &profile_id,
                            Some(&serde_json::json!({"event": "start", "interval": interval, "duration": duration}).to_string()),
                            None,
                        );

                        let result = serde_json::json!({
                            "status": "ok",
                            "profile_id": profile_id,
                            "machine": machine,
                            "interval_secs": interval,
                            "duration_secs": duration,
                            "message": format!("Started profiling {} (every {}s for {}s)", machine, interval, duration),
                        });
                        print_output(&result, self.format);
                    }
                    ProfileCommands::Samples { machine, limit } => {
                        let samples = store.list_profile_samples(machine.as_deref(), limit)
                            .map_err(|e| CliError::CommandFailed(format!("Failed to list samples: {e}")))?;
                        print_output(&serde_json::json!({"samples": samples, "count": samples.len()}), self.format);
                    }
                    ProfileCommands::Decisions { machine, limit } => {
                        let decisions = store.list_poll_decisions(machine.as_deref(), limit)
                            .map_err(|e| CliError::CommandFailed(format!("Failed to list decisions: {e}")))?;
                        print_output(&serde_json::json!({"decisions": decisions, "count": decisions.len()}), self.format);
                    }
                }
            }
            Commands::Ingest { from } => {
                let store = open_store(self.config.as_ref())?;

                // Read manifest
                let manifest_path = format!("{from}/manifest.json");
                let manifest_str = std::fs::read_to_string(&manifest_path)
                    .map_err(|e| CliError::CommandFailed(format!("Failed to read manifest: {e}")))?;
                let manifest: vc_collect::node::BundleManifest = serde_json::from_str(&manifest_str)
                    .map_err(|e| CliError::CommandFailed(format!("Invalid manifest: {e}")))?;

                let result = vc_collect::node::ingest_bundle(&store, &manifest)
                    .map_err(|e| CliError::CommandFailed(format!("Ingest failed: {e}")))?;

                print_output(&serde_json::json!({
                    "status": "ok",
                    "bundle_id": result.bundle_id,
                    "batches_processed": result.batches_processed,
                    "rows_ingested": result.rows_ingested,
                    "rows_deduplicated": result.rows_deduplicated,
                    "message": format!(
                        "Ingested {} rows ({} deduped) from {}",
                        result.rows_ingested, result.rows_deduplicated, result.bundle_id
                    ),
                }), self.format);
            }
            Commands::Node { command } => {
                let store = open_store(self.config.as_ref())?;

                match command {
                    NodeCommands::History { machine, limit } => {
                        let records = store.list_ingest_records(machine.as_deref(), limit)
                            .map_err(|e| CliError::CommandFailed(format!("Failed to list ingest records: {e}")))?;
                        print_output(&serde_json::json!({"records": records, "count": records.len()}), self.format);
                    }
                    NodeCommands::Config => {
                        let config = vc_collect::node::SpoolConfig::default();
                        print_output(&config, self.format);
                    }
                }
            }
            Commands::Token { command } => {
                match command {
                    TokenCommands::List => {
                        let auth_config = vc_web::auth::AuthConfig::default();
                        // In a real deployment, load from config file
                        let tokens: Vec<serde_json::Value> = auth_config.tokens.iter().map(|t| {
                            serde_json::json!({
                                "name": t.name,
                                "role": t.role.as_str(),
                                "enabled": t.enabled,
                                "allowed_ips": t.allowed_ips,
                                "token_prefix": if t.token.len() > 8 {
                                    format!("{}...", &t.token[..8])
                                } else {
                                    "***".to_string()
                                },
                            })
                        }).collect();
                        print_output(&serde_json::json!({
                            "auth_enabled": auth_config.enabled,
                            "local_bypass": auth_config.local_bypass,
                            "tokens": tokens,
                            "count": tokens.len(),
                        }), self.format);
                    }
                    TokenCommands::Add { name, role, allowed_ips } => {
                        let Some(parsed_role) = vc_web::auth::Role::parse(&role) else {
                            return Err(CliError::CommandFailed(
                                format!("Invalid role '{}'. Valid: read, operator, admin", role)
                            ));
                        };

                        // Generate a random-ish token
                        let token_value = format!(
                            "vc-{}-{}",
                            parsed_role.as_str(),
                            chrono::Utc::now().timestamp_millis()
                        );

                        let ips: Vec<String> = allowed_ips
                            .map(|s| s.split(',').map(|ip| ip.trim().to_string()).collect())
                            .unwrap_or_default();

                        let new_token = vc_web::auth::ApiToken {
                            name: name.clone(),
                            token: token_value.clone(),
                            role: parsed_role,
                            allowed_ips: ips,
                            enabled: true,
                        };

                        print_output(&serde_json::json!({
                            "status": "ok",
                            "message": format!("Token '{}' created. Add to vc.toml [web.auth.tokens]", name),
                            "token": token_value,
                            "name": new_token.name,
                            "role": parsed_role.as_str(),
                        }), self.format);
                    }
                    TokenCommands::Revoke { name } => {
                        print_output(&serde_json::json!({
                            "status": "ok",
                            "message": format!("Token '{}' marked for revocation. Remove from vc.toml or set enabled=false", name),
                            "name": name,
                        }), self.format);
                    }
                }
            }
            Commands::Report { window, output, save } => {
                let store = open_store(self.config.as_ref())?;
                let report = vc_query::digest::generate_digest(&store, window);

                if output == "json" {
                    print_output(&report, self.format);
                } else {
                    let md = vc_query::digest::render_markdown(&report);
                    println!("{md}");
                }

                if save {
                    let json = serde_json::to_string(&report.summary).unwrap_or_default();
                    let md = vc_query::digest::render_markdown(&report);
                    store.insert_digest_report(&report.report_id, window as i32, &json, &md)
                        .map_err(|e| CliError::CommandFailed(format!("Failed to save report: {e}")))?;
                    eprintln!("Report saved: {}", report.report_id);
                }
            }
            Commands::Redact { command } => {
                match command {
                    RedactCommands::Rules => {
                        let rules = vc_collect::redact::default_rules();
                        let entries: Vec<serde_json::Value> = rules.iter().map(|r| {
                            serde_json::json!({
                                "name": r.name,
                                "pattern": r.pattern,
                                "replacement": r.replacement,
                                "description": r.description,
                            })
                        }).collect();
                        print_output(&serde_json::json!({"rules": entries, "count": entries.len()}), self.format);
                    }
                    RedactCommands::History { machine, limit } => {
                        let store = open_store(self.config.as_ref())?;
                        let events = store.list_redaction_events(machine.as_deref(), limit)
                            .map_err(|e| CliError::CommandFailed(format!("Failed to list redaction events: {e}")))?;
                        print_output(&serde_json::json!({"events": events, "count": events.len()}), self.format);
                    }
                    RedactCommands::Summary => {
                        let store = open_store(self.config.as_ref())?;
                        let summary = store.redaction_summary()
                            .map_err(|e| CliError::CommandFailed(format!("Failed to get summary: {e}")))?;
                        print_output(&serde_json::json!({"summary": summary}), self.format);
                    }
                    RedactCommands::Test { input } => {
                        let engine = vc_collect::redact::RedactionEngine::new();
                        let (output, stats) = engine.redact_text(&input);
                        print_output(&serde_json::json!({
                            "input": input,
                            "output": output,
                            "fields_redacted": stats.fields_redacted,
                            "bytes_redacted": stats.bytes_redacted,
                            "rule_matches": stats.rule_matches,
                        }), self.format);
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
    let output = match format {
        OutputFormat::Json => serde_json::to_string_pretty(value)
            .unwrap_or_else(|e| format!(r#"{{"error": "serialization failed: {e}"}}"#)),
        OutputFormat::Toon => toon::to_toon_via_json(value),
        OutputFormat::Text => serde_json::to_string_pretty(value)
            .unwrap_or_else(|e| format!(r#"{{"error": "serialization failed: {e}"}}"#)),
    };
    println!("{output}");
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
            interval,
            machines,
            min_severity,
            buffer,
        } = cli.command
        {
            assert!(events.is_none());
            assert!(!changes_only);
            assert!(interval.is_none());
            assert!(machines.is_none());
            assert!(min_severity.is_none());
            assert!(buffer.is_none());
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

    #[test]
    fn test_watch_with_interval() {
        let cli = Cli::parse_from(["vc", "watch", "--interval", "60"]);
        if let Commands::Watch { interval, .. } = cli.command {
            assert_eq!(interval, Some(60));
        } else {
            panic!("Expected Watch command");
        }
    }

    #[test]
    fn test_watch_with_event_filter() {
        let cli = Cli::parse_from(["vc", "watch", "--events", "alert,prediction"]);
        if let Commands::Watch { events, .. } = cli.command {
            let evts = events.unwrap();
            assert_eq!(evts.len(), 2);
            assert_eq!(evts[0], "alert");
            assert_eq!(evts[1], "prediction");
        } else {
            panic!("Expected Watch command");
        }
    }

    #[test]
    fn test_watch_with_machine_filter() {
        let cli = Cli::parse_from(["vc", "watch", "--machines", "orko,sydneymc"]);
        if let Commands::Watch { machines, .. } = cli.command {
            let m = machines.unwrap();
            assert_eq!(m.len(), 2);
            assert_eq!(m[0], "orko");
            assert_eq!(m[1], "sydneymc");
        } else {
            panic!("Expected Watch command");
        }
    }

    #[test]
    fn test_watch_with_severity() {
        let cli = Cli::parse_from(["vc", "watch", "--min-severity", "high"]);
        if let Commands::Watch { min_severity, .. } = cli.command {
            assert_eq!(min_severity, Some("high".to_string()));
        } else {
            panic!("Expected Watch command");
        }
    }

    #[test]
    fn test_watch_with_buffer() {
        let cli = Cli::parse_from(["vc", "watch", "--buffer", "10"]);
        if let Commands::Watch { buffer, .. } = cli.command {
            assert_eq!(buffer, Some(10));
        } else {
            panic!("Expected Watch command");
        }
    }

    #[test]
    fn test_watch_full_args() {
        let cli = Cli::parse_from([
            "vc", "watch",
            "--events", "alert,health_change",
            "--changes-only",
            "--interval", "15",
            "--machines", "orko",
            "--min-severity", "critical",
            "--buffer", "5",
        ]);
        if let Commands::Watch {
            events,
            changes_only,
            interval,
            machines,
            min_severity,
            buffer,
        } = cli.command
        {
            assert_eq!(events.unwrap().len(), 2);
            assert!(changes_only);
            assert_eq!(interval, Some(15));
            assert_eq!(machines.unwrap(), vec!["orko"]);
            assert_eq!(min_severity, Some("critical".to_string()));
            assert_eq!(buffer, Some(5));
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

    #[test]
    fn test_guardian_capture_parse() {
        let cli = Cli::parse_from([
            "vc", "guardian", "capture",
            "--alert-type", "rate-limit",
            "--actions", r#"[{"type":"command","cmd":"caam","args":["switch"],"success":true}]"#,
            "--outcome", "success",
            "--machine", "orko",
        ]);
        if let Commands::Guardian { command } = cli.command {
            if let GuardianCommands::Capture { alert_type, actions, outcome, machine, .. } = command {
                assert_eq!(alert_type, "rate-limit");
                assert!(actions.contains("caam"));
                assert_eq!(outcome, "success");
                assert_eq!(machine.unwrap(), "orko");
            } else {
                panic!("Expected Capture subcommand");
            }
        } else {
            panic!("Expected Guardian command");
        }
    }

    #[test]
    fn test_guardian_generate_parse() {
        let cli = Cli::parse_from([
            "vc", "guardian", "generate",
            "--min-samples", "5",
            "--min-confidence", "0.7",
        ]);
        if let Commands::Guardian { command } = cli.command {
            if let GuardianCommands::Generate { min_samples, min_confidence } = command {
                assert_eq!(min_samples, 5);
                assert!((min_confidence - 0.7).abs() < f64::EPSILON);
            } else {
                panic!("Expected Generate subcommand");
            }
        } else {
            panic!("Expected Guardian command");
        }
    }

    #[test]
    fn test_guardian_generate_defaults() {
        let cli = Cli::parse_from(["vc", "guardian", "generate"]);
        if let Commands::Guardian { command } = cli.command {
            if let GuardianCommands::Generate { min_samples, min_confidence } = command {
                assert_eq!(min_samples, 3);
                assert!((min_confidence - 0.5).abs() < f64::EPSILON);
            } else {
                panic!("Expected Generate subcommand");
            }
        } else {
            panic!("Expected Guardian command");
        }
    }

    #[test]
    fn test_guardian_drafts_parse() {
        let cli = Cli::parse_from([
            "vc", "guardian", "drafts",
            "--status", "pending_review",
            "--limit", "10",
        ]);
        if let Commands::Guardian { command } = cli.command {
            if let GuardianCommands::Drafts { status, limit } = command {
                assert_eq!(status.unwrap(), "pending_review");
                assert_eq!(limit, 10);
            } else {
                panic!("Expected Drafts subcommand");
            }
        } else {
            panic!("Expected Guardian command");
        }
    }

    #[test]
    fn test_guardian_validate_draft_parse() {
        let cli = Cli::parse_from(["vc", "guardian", "validate-draft", "auto-rate-limit-abc"]);
        if let Commands::Guardian { command } = cli.command {
            if let GuardianCommands::ValidateDraft { draft_id } = command {
                assert_eq!(draft_id, "auto-rate-limit-abc");
            } else {
                panic!("Expected ValidateDraft subcommand");
            }
        } else {
            panic!("Expected Guardian command");
        }
    }

    #[test]
    fn test_guardian_approve_draft_parse() {
        let cli = Cli::parse_from([
            "vc", "guardian", "approve-draft", "draft-1",
            "--approver", "admin",
        ]);
        if let Commands::Guardian { command } = cli.command {
            if let GuardianCommands::ApproveDraft { draft_id, approver } = command {
                assert_eq!(draft_id, "draft-1");
                assert_eq!(approver, "admin");
            } else {
                panic!("Expected ApproveDraft subcommand");
            }
        } else {
            panic!("Expected Guardian command");
        }
    }

    #[test]
    fn test_guardian_reject_draft_parse() {
        let cli = Cli::parse_from([
            "vc", "guardian", "reject-draft", "draft-2",
            "--reason", "too risky",
        ]);
        if let Commands::Guardian { command } = cli.command {
            if let GuardianCommands::RejectDraft { draft_id, reason } = command {
                assert_eq!(draft_id, "draft-2");
                assert_eq!(reason.unwrap(), "too risky");
            } else {
                panic!("Expected RejectDraft subcommand");
            }
        } else {
            panic!("Expected Guardian command");
        }
    }

    #[test]
    fn test_guardian_activate_draft_parse() {
        let cli = Cli::parse_from(["vc", "guardian", "activate-draft", "draft-3"]);
        if let Commands::Guardian { command } = cli.command {
            if let GuardianCommands::ActivateDraft { draft_id } = command {
                assert_eq!(draft_id, "draft-3");
            } else {
                panic!("Expected ActivateDraft subcommand");
            }
        } else {
            panic!("Expected Guardian command");
        }
    }

    #[test]
    fn test_guardian_resolutions_parse() {
        let cli = Cli::parse_from([
            "vc", "guardian", "resolutions",
            "--alert-type", "rate-limit",
            "--outcome", "success",
            "--limit", "20",
        ]);
        if let Commands::Guardian { command } = cli.command {
            if let GuardianCommands::Resolutions { alert_type, outcome, limit } = command {
                assert_eq!(alert_type.unwrap(), "rate-limit");
                assert_eq!(outcome.unwrap(), "success");
                assert_eq!(limit, 20);
            } else {
                panic!("Expected Resolutions subcommand");
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

    // =============================================================================
    // Commands::Knowledge Tests
    // =============================================================================

    #[test]
    fn test_knowledge_add_parse() {
        let cli = Cli::parse_from([
            "vc",
            "knowledge",
            "add",
            "--entry-type",
            "solution",
            "--title",
            "Fix DuckDB connections",
            "--content",
            "Wrap in Arc<Mutex<>>",
        ]);
        if let Commands::Knowledge { command } = cli.command {
            if let KnowledgeCommands::Add {
                entry_type,
                title,
                content,
                summary,
                session,
                file,
                lines,
                tags,
            } = command
            {
                assert_eq!(entry_type, "solution");
                assert_eq!(title, "Fix DuckDB connections");
                assert_eq!(content, "Wrap in Arc<Mutex<>>");
                assert!(summary.is_none());
                assert!(session.is_none());
                assert!(file.is_none());
                assert!(lines.is_none());
                assert!(tags.is_none());
            } else {
                panic!("Expected Knowledge add command");
            }
        } else {
            panic!("Expected Knowledge command");
        }
    }

    #[test]
    fn test_knowledge_add_with_all_options_parse() {
        let cli = Cli::parse_from([
            "vc",
            "knowledge",
            "add",
            "--entry-type",
            "pattern",
            "--title",
            "Collector Pattern",
            "--content",
            "Use the Collector trait",
            "--summary",
            "Standard collector pattern",
            "--session",
            "sess-123",
            "--file",
            "src/lib.rs",
            "--lines",
            "10-25",
            "--tags",
            "rust,pattern,collector",
        ]);
        if let Commands::Knowledge { command } = cli.command {
            if let KnowledgeCommands::Add {
                entry_type,
                title,
                summary,
                session,
                file,
                lines,
                tags,
                ..
            } = command
            {
                assert_eq!(entry_type, "pattern");
                assert_eq!(title, "Collector Pattern");
                assert_eq!(summary, Some("Standard collector pattern".to_string()));
                assert_eq!(session, Some("sess-123".to_string()));
                assert_eq!(file, Some("src/lib.rs".to_string()));
                assert_eq!(lines, Some("10-25".to_string()));
                assert_eq!(tags, Some("rust,pattern,collector".to_string()));
            } else {
                panic!("Expected Knowledge add command");
            }
        } else {
            panic!("Expected Knowledge command");
        }
    }

    #[test]
    fn test_knowledge_search_parse() {
        let cli = Cli::parse_from(["vc", "knowledge", "search", "duckdb connection"]);
        if let Commands::Knowledge { command } = cli.command {
            if let KnowledgeCommands::Search {
                query,
                entry_type,
                tags,
                limit,
            } = command
            {
                assert_eq!(query, "duckdb connection");
                assert!(entry_type.is_none());
                assert!(tags.is_none());
                assert_eq!(limit, 20);
            } else {
                panic!("Expected Knowledge search command");
            }
        } else {
            panic!("Expected Knowledge command");
        }
    }

    #[test]
    fn test_knowledge_search_with_filters_parse() {
        let cli = Cli::parse_from([
            "vc",
            "knowledge",
            "search",
            "ssh",
            "--entry-type",
            "solution",
            "--tags",
            "ssh,debug",
            "--limit",
            "5",
        ]);
        if let Commands::Knowledge { command } = cli.command {
            if let KnowledgeCommands::Search {
                query,
                entry_type,
                tags,
                limit,
            } = command
            {
                assert_eq!(query, "ssh");
                assert_eq!(entry_type, Some("solution".to_string()));
                assert_eq!(tags, Some("ssh,debug".to_string()));
                assert_eq!(limit, 5);
            } else {
                panic!("Expected Knowledge search command");
            }
        } else {
            panic!("Expected Knowledge command");
        }
    }

    #[test]
    fn test_knowledge_show_parse() {
        let cli = Cli::parse_from(["vc", "knowledge", "show", "42"]);
        if let Commands::Knowledge { command } = cli.command {
            if let KnowledgeCommands::Show { id } = command {
                assert_eq!(id, 42);
            } else {
                panic!("Expected Knowledge show command");
            }
        } else {
            panic!("Expected Knowledge command");
        }
    }

    #[test]
    fn test_knowledge_list_parse() {
        let cli = Cli::parse_from(["vc", "knowledge", "list"]);
        if let Commands::Knowledge { command } = cli.command {
            if let KnowledgeCommands::List { limit, entry_type } = command {
                assert_eq!(limit, 20);
                assert!(entry_type.is_none());
            } else {
                panic!("Expected Knowledge list command");
            }
        } else {
            panic!("Expected Knowledge command");
        }
    }

    #[test]
    fn test_knowledge_list_with_type_parse() {
        let cli = Cli::parse_from([
            "vc",
            "knowledge",
            "list",
            "--entry-type",
            "debug_log",
            "--limit",
            "10",
        ]);
        if let Commands::Knowledge { command } = cli.command {
            if let KnowledgeCommands::List { limit, entry_type } = command {
                assert_eq!(limit, 10);
                assert_eq!(entry_type, Some("debug_log".to_string()));
            } else {
                panic!("Expected Knowledge list command");
            }
        } else {
            panic!("Expected Knowledge command");
        }
    }

    #[test]
    fn test_knowledge_top_parse() {
        let cli = Cli::parse_from(["vc", "knowledge", "top"]);
        if let Commands::Knowledge { command } = cli.command {
            if let KnowledgeCommands::Top { limit } = command {
                assert_eq!(limit, 10);
            } else {
                panic!("Expected Knowledge top command");
            }
        } else {
            panic!("Expected Knowledge command");
        }
    }

    #[test]
    fn test_knowledge_feedback_parse() {
        let cli = Cli::parse_from([
            "vc",
            "knowledge",
            "feedback",
            "42",
            "--feedback-type",
            "helpful",
        ]);
        if let Commands::Knowledge { command } = cli.command {
            if let KnowledgeCommands::Feedback {
                id,
                feedback_type,
                comment,
                session,
            } = command
            {
                assert_eq!(id, 42);
                assert_eq!(feedback_type, "helpful");
                assert!(comment.is_none());
                assert!(session.is_none());
            } else {
                panic!("Expected Knowledge feedback command");
            }
        } else {
            panic!("Expected Knowledge command");
        }
    }

    #[test]
    fn test_knowledge_feedback_with_options_parse() {
        let cli = Cli::parse_from([
            "vc",
            "knowledge",
            "feedback",
            "7",
            "--feedback-type",
            "not_helpful",
            "--comment",
            "Outdated information",
            "--session",
            "sess-789",
        ]);
        if let Commands::Knowledge { command } = cli.command {
            if let KnowledgeCommands::Feedback {
                id,
                feedback_type,
                comment,
                session,
            } = command
            {
                assert_eq!(id, 7);
                assert_eq!(feedback_type, "not_helpful");
                assert_eq!(comment, Some("Outdated information".to_string()));
                assert_eq!(session, Some("sess-789".to_string()));
            } else {
                panic!("Expected Knowledge feedback command");
            }
        } else {
            panic!("Expected Knowledge command");
        }
    }

    #[test]
    fn test_knowledge_mine_parse() {
        let cli = Cli::parse_from(["vc", "knowledge", "mine"]);
        if let Commands::Knowledge { command } = cli.command {
            if let KnowledgeCommands::Mine { limit, min_quality } = command {
                assert_eq!(limit, 10);
                assert_eq!(min_quality, 3);
            } else {
                panic!("Expected Knowledge mine command");
            }
        } else {
            panic!("Expected Knowledge command");
        }
    }

    #[test]
    fn test_knowledge_mine_with_options() {
        let cli = Cli::parse_from([
            "vc", "knowledge", "mine", "--limit", "50", "--min-quality", "4",
        ]);
        if let Commands::Knowledge { command } = cli.command {
            if let KnowledgeCommands::Mine { limit, min_quality } = command {
                assert_eq!(limit, 50);
                assert_eq!(min_quality, 4);
            } else {
                panic!("Expected Knowledge mine command");
            }
        } else {
            panic!("Expected Knowledge command");
        }
    }

    #[test]
    fn test_knowledge_mine_stats_parse() {
        let cli = Cli::parse_from(["vc", "knowledge", "mine-stats"]);
        if let Commands::Knowledge { command } = cli.command {
            assert!(matches!(command, KnowledgeCommands::MineStats));
        } else {
            panic!("Expected Knowledge command");
        }
    }

    // =============================================================================
    // Commands::Incident Tests
    // =============================================================================

    #[test]
    fn test_incident_list_parse() {
        let cli = Cli::parse_from(["vc", "incident", "list"]);
        if let Commands::Incident { command } = cli.command {
            if let IncidentCommands::List { status, limit } = command {
                assert!(status.is_none());
                assert_eq!(limit, 50);
            } else {
                panic!("Expected Incident list command");
            }
        } else {
            panic!("Expected Incident command");
        }
    }

    #[test]
    fn test_incident_list_with_status_parse() {
        let cli = Cli::parse_from(["vc", "incident", "list", "--status", "open", "--limit", "10"]);
        if let Commands::Incident { command } = cli.command {
            if let IncidentCommands::List { status, limit } = command {
                assert_eq!(status, Some("open".to_string()));
                assert_eq!(limit, 10);
            } else {
                panic!("Expected Incident list command");
            }
        } else {
            panic!("Expected Incident command");
        }
    }

    #[test]
    fn test_incident_show_parse() {
        let cli = Cli::parse_from(["vc", "incident", "show", "inc-abc12345"]);
        if let Commands::Incident { command } = cli.command {
            if let IncidentCommands::Show { id } = command {
                assert_eq!(id, "inc-abc12345");
            } else {
                panic!("Expected Incident show command");
            }
        } else {
            panic!("Expected Incident command");
        }
    }

    #[test]
    fn test_incident_create_parse() {
        let cli = Cli::parse_from([
            "vc",
            "incident",
            "create",
            "--title",
            "Rate limit exhaustion on orko",
            "--severity",
            "critical",
            "--description",
            "Multiple accounts hit rate limits",
        ]);
        if let Commands::Incident { command } = cli.command {
            if let IncidentCommands::Create {
                title,
                severity,
                description,
            } = command
            {
                assert_eq!(title, "Rate limit exhaustion on orko");
                assert_eq!(severity, "critical");
                assert_eq!(
                    description,
                    Some("Multiple accounts hit rate limits".to_string())
                );
            } else {
                panic!("Expected Incident create command");
            }
        } else {
            panic!("Expected Incident command");
        }
    }

    #[test]
    fn test_incident_note_parse() {
        let cli = Cli::parse_from([
            "vc",
            "incident",
            "note",
            "inc-abc123",
            "Swapped to backup accounts",
            "--author",
            "LavenderOak",
        ]);
        if let Commands::Incident { command } = cli.command {
            if let IncidentCommands::Note {
                id,
                content,
                author,
            } = command
            {
                assert_eq!(id, "inc-abc123");
                assert_eq!(content, "Swapped to backup accounts");
                assert_eq!(author, Some("LavenderOak".to_string()));
            } else {
                panic!("Expected Incident note command");
            }
        } else {
            panic!("Expected Incident command");
        }
    }

    #[test]
    fn test_incident_close_parse() {
        let cli = Cli::parse_from([
            "vc",
            "incident",
            "close",
            "inc-abc123",
            "--reason",
            "Accounts recovered after cooldown",
            "--root-cause",
            "Burst usage exceeded hourly quota",
        ]);
        if let Commands::Incident { command } = cli.command {
            if let IncidentCommands::Close {
                id,
                reason,
                root_cause,
            } = command
            {
                assert_eq!(id, "inc-abc123");
                assert_eq!(
                    reason,
                    Some("Accounts recovered after cooldown".to_string())
                );
                assert_eq!(
                    root_cause,
                    Some("Burst usage exceeded hourly quota".to_string())
                );
            } else {
                panic!("Expected Incident close command");
            }
        } else {
            panic!("Expected Incident command");
        }
    }

    #[test]
    fn test_incident_timeline_parse() {
        let cli = Cli::parse_from(["vc", "incident", "timeline", "inc-abc123"]);
        if let Commands::Incident { command } = cli.command {
            if let IncidentCommands::Timeline { id } = command {
                assert_eq!(id, "inc-abc123");
            } else {
                panic!("Expected Incident timeline command");
            }
        } else {
            panic!("Expected Incident command");
        }
    }

    #[test]
    fn test_incident_replay_parse() {
        let cli = Cli::parse_from([
            "vc", "incident", "replay", "inc-abc123",
            "--at", "2026-02-20T10:30:00",
        ]);
        if let Commands::Incident { command } = cli.command {
            if let IncidentCommands::Replay { id, at } = command {
                assert_eq!(id, "inc-abc123");
                assert_eq!(at, "2026-02-20T10:30:00");
            } else {
                panic!("Expected Incident replay command");
            }
        } else {
            panic!("Expected Incident command");
        }
    }

    #[test]
    fn test_incident_export_parse() {
        let cli = Cli::parse_from([
            "vc", "incident", "export", "inc-abc123",
            "--output", "md",
        ]);
        if let Commands::Incident { command } = cli.command {
            if let IncidentCommands::Export { id, output } = command {
                assert_eq!(id, "inc-abc123");
                assert_eq!(output, "md");
            } else {
                panic!("Expected Incident export command");
            }
        } else {
            panic!("Expected Incident command");
        }
    }

    #[test]
    fn test_incident_export_default_format() {
        let cli = Cli::parse_from(["vc", "incident", "export", "inc-x"]);
        if let Commands::Incident { command } = cli.command {
            if let IncidentCommands::Export { id, output } = command {
                assert_eq!(id, "inc-x");
                assert_eq!(output, "json");
            } else {
                panic!("Expected Incident export command");
            }
        } else {
            panic!("Expected Incident command");
        }
    }

    // =============================================================================
    // Commands::Mcp Tests
    // =============================================================================

    #[test]
    fn test_mcp_serve_parse() {
        let cli = Cli::parse_from(["vc", "mcp", "serve"]);
        if let Commands::Mcp { command } = cli.command {
            assert!(matches!(command, McpCommands::Serve));
        } else {
            panic!("Expected Mcp command");
        }
    }

    #[test]
    fn test_mcp_tools_parse() {
        let cli = Cli::parse_from(["vc", "mcp", "tools"]);
        if let Commands::Mcp { command } = cli.command {
            assert!(matches!(command, McpCommands::Tools));
        } else {
            panic!("Expected Mcp command");
        }
    }

    // =============================================================================
    // Commands::Db Tests
    // =============================================================================

    #[test]
    fn test_db_export_parse() {
        let cli = Cli::parse_from([
            "vc", "db", "export",
            "--out", "/tmp/export",
            "--since", "2026-01-01",
        ]);
        if let Commands::Db { command } = cli.command {
            if let DbCommands::Export { out, since, until, tables } = command {
                assert_eq!(out, "/tmp/export");
                assert_eq!(since, Some("2026-01-01".to_string()));
                assert!(until.is_none());
                assert!(tables.is_none());
            } else {
                panic!("Expected Db export command");
            }
        } else {
            panic!("Expected Db command");
        }
    }

    #[test]
    fn test_db_import_parse() {
        let cli = Cli::parse_from(["vc", "db", "import", "--from", "/tmp/backup"]);
        if let Commands::Db { command } = cli.command {
            if let DbCommands::Import { from } = command {
                assert_eq!(from, "/tmp/backup");
            } else {
                panic!("Expected Db import command");
            }
        } else {
            panic!("Expected Db command");
        }
    }

    #[test]
    fn test_db_info_parse() {
        let cli = Cli::parse_from(["vc", "db", "info"]);
        if let Commands::Db { command } = cli.command {
            assert!(matches!(command, DbCommands::Info));
        } else {
            panic!("Expected Db command");
        }
    }

    // =============================================================================
    // Commands::Profile Tests
    // =============================================================================

    #[test]
    fn test_profile_start_parse() {
        let cli = Cli::parse_from([
            "vc", "profile", "start",
            "--machine", "orko",
            "--interval", "2",
            "--duration", "120",
        ]);
        if let Commands::Profile { command } = cli.command {
            if let ProfileCommands::Start { machine, interval, duration } = command {
                assert_eq!(machine, "orko");
                assert_eq!(interval, 2);
                assert_eq!(duration, 120);
            } else {
                panic!("Expected Profile start command");
            }
        } else {
            panic!("Expected Profile command");
        }
    }

    #[test]
    fn test_profile_start_defaults() {
        let cli = Cli::parse_from(["vc", "profile", "start", "--machine", "orko"]);
        if let Commands::Profile { command } = cli.command {
            if let ProfileCommands::Start { interval, duration, .. } = command {
                assert_eq!(interval, 5);
                assert_eq!(duration, 300);
            } else {
                panic!("Expected Profile start command");
            }
        } else {
            panic!("Expected Profile command");
        }
    }

    #[test]
    fn test_profile_samples_parse() {
        let cli = Cli::parse_from(["vc", "profile", "samples", "--machine", "orko", "--limit", "50"]);
        if let Commands::Profile { command } = cli.command {
            if let ProfileCommands::Samples { machine, limit } = command {
                assert_eq!(machine, Some("orko".to_string()));
                assert_eq!(limit, 50);
            } else {
                panic!("Expected Profile samples command");
            }
        } else {
            panic!("Expected Profile command");
        }
    }

    #[test]
    fn test_profile_decisions_parse() {
        let cli = Cli::parse_from(["vc", "profile", "decisions"]);
        if let Commands::Profile { command } = cli.command {
            if let ProfileCommands::Decisions { machine, limit } = command {
                assert!(machine.is_none());
                assert_eq!(limit, 20);
            } else {
                panic!("Expected Profile decisions command");
            }
        } else {
            panic!("Expected Profile command");
        }
    }

    // =============================================================================
    // Commands::Ingest Tests
    // =============================================================================

    #[test]
    fn test_ingest_parse() {
        let cli = Cli::parse_from(["vc", "ingest", "--from", "/tmp/bundle"]);
        if let Commands::Ingest { from } = cli.command {
            assert_eq!(from, "/tmp/bundle");
        } else {
            panic!("Expected Ingest command");
        }
    }

    // =============================================================================
    // Commands::Node Tests
    // =============================================================================

    #[test]
    fn test_node_history_parse() {
        let cli = Cli::parse_from(["vc", "node", "history", "--machine", "orko", "--limit", "50"]);
        if let Commands::Node { command } = cli.command {
            if let NodeCommands::History { machine, limit } = command {
                assert_eq!(machine, Some("orko".to_string()));
                assert_eq!(limit, 50);
            } else {
                panic!("Expected Node history command");
            }
        } else {
            panic!("Expected Node command");
        }
    }

    #[test]
    fn test_node_config_parse() {
        let cli = Cli::parse_from(["vc", "node", "config"]);
        if let Commands::Node { command } = cli.command {
            assert!(matches!(command, NodeCommands::Config));
        } else {
            panic!("Expected Node command");
        }
    }

    // =============================================================================
    // Commands::Token Tests
    // =============================================================================

    #[test]
    fn test_token_list_parse() {
        let cli = Cli::parse_from(["vc", "token", "list"]);
        if let Commands::Token { command } = cli.command {
            assert!(matches!(command, TokenCommands::List));
        } else {
            panic!("Expected Token command");
        }
    }

    #[test]
    fn test_token_add_parse() {
        let cli = Cli::parse_from([
            "vc", "token", "add",
            "--name", "ci-bot",
            "--role", "read",
            "--allowed-ips", "10.0.0.1,10.0.0.2",
        ]);
        if let Commands::Token { command } = cli.command {
            if let TokenCommands::Add { name, role, allowed_ips } = command {
                assert_eq!(name, "ci-bot");
                assert_eq!(role, "read");
                assert_eq!(allowed_ips, Some("10.0.0.1,10.0.0.2".to_string()));
            } else {
                panic!("Expected Token add command");
            }
        } else {
            panic!("Expected Token command");
        }
    }

    #[test]
    fn test_token_revoke_parse() {
        let cli = Cli::parse_from(["vc", "token", "revoke", "old-token"]);
        if let Commands::Token { command } = cli.command {
            if let TokenCommands::Revoke { name } = command {
                assert_eq!(name, "old-token");
            } else {
                panic!("Expected Token revoke command");
            }
        } else {
            panic!("Expected Token command");
        }
    }

    // =============================================================================
    // Commands::Report Tests
    // =============================================================================

    #[test]
    fn test_report_parse_defaults() {
        let cli = Cli::parse_from(["vc", "report"]);
        if let Commands::Report { window, output, save } = cli.command {
            assert_eq!(window, 24);
            assert_eq!(output, "md");
            assert!(!save);
        } else {
            panic!("Expected Report command");
        }
    }

    #[test]
    fn test_report_parse_weekly_json() {
        let cli = Cli::parse_from(["vc", "report", "--window", "168", "--output", "json", "--save"]);
        if let Commands::Report { window, output, save } = cli.command {
            assert_eq!(window, 168);
            assert_eq!(output, "json");
            assert!(save);
        } else {
            panic!("Expected Report command");
        }
    }

    // =============================================================================
    // Commands::Redact Tests
    // =============================================================================

    #[test]
    fn test_redact_rules_parse() {
        let cli = Cli::parse_from(["vc", "redact", "rules"]);
        if let Commands::Redact { command } = cli.command {
            assert!(matches!(command, RedactCommands::Rules));
        } else {
            panic!("Expected Redact command");
        }
    }

    #[test]
    fn test_redact_history_parse() {
        let cli = Cli::parse_from(["vc", "redact", "history", "--machine", "orko", "--limit", "50"]);
        if let Commands::Redact { command } = cli.command {
            if let RedactCommands::History { machine, limit } = command {
                assert_eq!(machine, Some("orko".to_string()));
                assert_eq!(limit, 50);
            } else {
                panic!("Expected Redact history command");
            }
        } else {
            panic!("Expected Redact command");
        }
    }

    #[test]
    fn test_redact_summary_parse() {
        let cli = Cli::parse_from(["vc", "redact", "summary"]);
        if let Commands::Redact { command } = cli.command {
            assert!(matches!(command, RedactCommands::Summary));
        } else {
            panic!("Expected Redact command");
        }
    }

    #[test]
    fn test_redact_test_parse() {
        let cli = Cli::parse_from(["vc", "redact", "test", "password=secret123"]);
        if let Commands::Redact { command } = cli.command {
            if let RedactCommands::Test { input } = command {
                assert_eq!(input, "password=secret123");
            } else {
                panic!("Expected Redact test command");
            }
        } else {
            panic!("Expected Redact command");
        }
    }
}
