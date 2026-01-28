//! Guardian screen implementation
//!
//! Displays self-healing status, active protocols, pending approvals, and history.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::theme::Theme;

/// Data needed to render the guardian screen
#[derive(Debug, Clone, Default)]
pub struct GuardianData {
    /// Guardian system status
    pub status: GuardianStatus,
    /// Active healing protocols
    pub active_protocols: Vec<ActiveProtocol>,
    /// Pending approvals (for destructive actions)
    pub pending_approvals: Vec<PendingApproval>,
    /// Recent run history
    pub recent_runs: Vec<GuardianRun>,
    /// Currently selected section
    pub selected_section: GuardianSection,
    /// Selected index within section
    pub selected_index: usize,
}

/// Guardian sections for navigation
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum GuardianSection {
    #[default]
    Status,
    Active,
    Pending,
    History,
}

impl GuardianSection {
    pub fn next(&self) -> Self {
        match self {
            Self::Status => Self::Active,
            Self::Active => Self::Pending,
            Self::Pending => Self::History,
            Self::History => Self::Status,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            Self::Status => Self::History,
            Self::Active => Self::Status,
            Self::Pending => Self::Active,
            Self::History => Self::Pending,
        }
    }
}

/// Guardian operating mode
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum GuardianMode {
    /// Guardian is off
    Off,
    /// Suggest actions but don't execute
    #[default]
    Suggest,
    /// Execute safe (allowlisted) actions
    ExecuteSafe,
    /// Execute with approval for destructive actions
    WithApproval,
}

impl GuardianMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Suggest => "suggest-only",
            Self::ExecuteSafe => "execute-safe",
            Self::WithApproval => "with-approval",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Off => "Guardian disabled",
            Self::Suggest => "Shows suggestions, no automatic actions",
            Self::ExecuteSafe => "Executes allowlisted safe commands only",
            Self::WithApproval => "Executes safe + queues destructive for approval",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Self::Off => Self::Suggest,
            Self::Suggest => Self::ExecuteSafe,
            Self::ExecuteSafe => Self::WithApproval,
            Self::WithApproval => Self::Off,
        }
    }
}

/// Guardian system status
#[derive(Debug, Clone, Default)]
pub struct GuardianStatus {
    /// Current operating mode
    pub mode: GuardianMode,
    /// Is guardian enabled
    pub enabled: bool,
    /// Number of active detection patterns
    pub active_patterns: u32,
    /// Last action timestamp (human readable)
    pub last_action: Option<String>,
    /// Success rate over last 7 days (0-100)
    pub success_rate_7d: f64,
    /// Total successful runs
    pub successful_runs: u32,
    /// Total runs
    pub total_runs: u32,
}

/// Active healing protocol
#[derive(Debug, Clone)]
pub struct ActiveProtocol {
    /// Protocol/playbook ID
    pub playbook_id: String,
    /// Protocol name
    pub name: String,
    /// Machine being healed
    pub machine_id: String,
    /// Current step (1-indexed)
    pub current_step: u32,
    /// Total steps
    pub total_steps: u32,
    /// Current step description
    pub step_description: String,
    /// When started (human readable)
    pub started_ago: String,
    /// Status: running, paused, waiting
    pub status: ProtocolStatus,
}

impl Default for ActiveProtocol {
    fn default() -> Self {
        Self {
            playbook_id: String::new(),
            name: String::new(),
            machine_id: String::new(),
            current_step: 0,
            total_steps: 0,
            step_description: String::new(),
            started_ago: String::new(),
            status: ProtocolStatus::default(),
        }
    }
}

/// Protocol execution status
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum ProtocolStatus {
    #[default]
    Running,
    Paused,
    WaitingApproval,
    WaitingCondition,
}

impl ProtocolStatus {
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Running => "▶",
            Self::Paused => "⏸",
            Self::WaitingApproval => "⏳",
            Self::WaitingCondition => "⏱",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Paused => "paused",
            Self::WaitingApproval => "awaiting approval",
            Self::WaitingCondition => "waiting",
        }
    }
}

/// Pending approval for destructive action
#[derive(Debug, Clone)]
pub struct PendingApproval {
    /// Approval ID
    pub id: u64,
    /// Playbook that needs approval
    pub playbook_id: String,
    /// Playbook name
    pub playbook_name: String,
    /// Machine involved
    pub machine_id: String,
    /// What action needs approval
    pub action_description: String,
    /// Why this needs approval
    pub reason: String,
    /// When queued (human readable)
    pub queued_ago: String,
}

impl Default for PendingApproval {
    fn default() -> Self {
        Self {
            id: 0,
            playbook_id: String::new(),
            playbook_name: String::new(),
            machine_id: String::new(),
            action_description: String::new(),
            reason: String::new(),
            queued_ago: String::new(),
        }
    }
}

/// Guardian run history entry
#[derive(Debug, Clone)]
pub struct GuardianRun {
    /// Run ID
    pub id: u64,
    /// Playbook that ran
    pub playbook_id: String,
    /// Playbook name
    pub playbook_name: String,
    /// Machine
    pub machine_id: String,
    /// Run result
    pub result: RunResult,
    /// When completed (human readable)
    pub completed_ago: String,
    /// Summary of what happened
    pub summary: String,
}

impl Default for GuardianRun {
    fn default() -> Self {
        Self {
            id: 0,
            playbook_id: String::new(),
            playbook_name: String::new(),
            machine_id: String::new(),
            result: RunResult::default(),
            completed_ago: String::new(),
            summary: String::new(),
        }
    }
}

/// Run result status
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum RunResult {
    #[default]
    Success,
    Failed,
    Aborted,
    Escalated,
}

impl RunResult {
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Success => "✓",
            Self::Failed => "✗",
            Self::Aborted => "⊘",
            Self::Escalated => "↑",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Success => "OK",
            Self::Failed => "FAIL",
            Self::Aborted => "ABORT",
            Self::Escalated => "ESCALATED",
        }
    }
}

/// Render the guardian screen
pub fn render_guardian(f: &mut Frame, data: &GuardianData, theme: &Theme) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Length(6), // Status
            Constraint::Length(8), // Active protocols
            Constraint::Length(6), // Pending approvals
            Constraint::Min(6),    // History
            Constraint::Length(3), // Footer
        ])
        .split(f.area());

    render_header(f, chunks[0], data, theme);
    render_status(f, chunks[1], data, theme);
    render_active(f, chunks[2], data, theme);
    render_pending(f, chunks[3], data, theme);
    render_history(f, chunks[4], data, theme);
    render_footer(f, chunks[5], data, theme);
}

fn render_header(f: &mut Frame, area: Rect, data: &GuardianData, theme: &Theme) {
    let mode_color = match data.status.mode {
        GuardianMode::Off => theme.critical,
        GuardianMode::Suggest => theme.info,
        GuardianMode::ExecuteSafe => theme.healthy,
        GuardianMode::WithApproval => theme.warning,
    };

    let title = Line::from(vec![
        Span::styled(
            "  G U A R D I A N  ",
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Self-Healing  ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("[{}]", data.status.mode.label()),
            Style::default().fg(mode_color),
        ),
        Span::raw("    "),
        Span::styled("[t]oggle mode", Style::default().fg(theme.muted)),
    ]);

    let header = Paragraph::new(title).block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(theme.border)),
    );

    f.render_widget(header, area);
}

fn render_status(f: &mut Frame, area: Rect, data: &GuardianData, theme: &Theme) {
    let is_selected = data.selected_section == GuardianSection::Status;

    let block = Block::default()
        .title(" Status ")
        .borders(Borders::ALL)
        .border_style(if is_selected {
            Style::default().fg(theme.highlight)
        } else {
            Style::default().fg(theme.border)
        });

    let inner = block.inner(area);
    f.render_widget(block, area);

    let success_color = if data.status.success_rate_7d >= 90.0 {
        theme.healthy
    } else if data.status.success_rate_7d >= 70.0 {
        theme.warning
    } else {
        theme.critical
    };

    let lines = vec![
        Line::from(vec![
            Span::styled("├─ Mode: ", Style::default().fg(theme.muted)),
            Span::styled(
                format!(
                    "{} ({})",
                    data.status.mode.label(),
                    data.status.mode.description()
                ),
                Style::default().fg(theme.text),
            ),
        ]),
        Line::from(vec![
            Span::styled("├─ Patterns detected: ", Style::default().fg(theme.muted)),
            Span::styled(
                format!("{} active", data.status.active_patterns),
                Style::default().fg(theme.text),
            ),
        ]),
        Line::from(vec![
            Span::styled("├─ Last action: ", Style::default().fg(theme.muted)),
            Span::styled(
                data.status.last_action.as_deref().unwrap_or("never"),
                Style::default().fg(theme.text),
            ),
        ]),
        Line::from(vec![
            Span::styled("└─ Success rate: ", Style::default().fg(theme.muted)),
            Span::styled(
                format!(
                    "{:.0}% ({}/{} last week)",
                    data.status.success_rate_7d,
                    data.status.successful_runs,
                    data.status.total_runs
                ),
                Style::default().fg(success_color),
            ),
        ]),
    ];

    let status_para = Paragraph::new(lines);
    f.render_widget(status_para, inner);
}

fn render_active(f: &mut Frame, area: Rect, data: &GuardianData, theme: &Theme) {
    let is_selected = data.selected_section == GuardianSection::Active;

    let block = Block::default()
        .title(format!(
            " Active Protocols ({}) ",
            data.active_protocols.len()
        ))
        .borders(Borders::ALL)
        .border_style(if is_selected {
            Style::default().fg(theme.highlight)
        } else {
            Style::default().fg(theme.border)
        });

    if data.active_protocols.is_empty() {
        let empty = Paragraph::new("  No active protocols")
            .style(Style::default().fg(theme.muted))
            .block(block);
        f.render_widget(empty, area);
        return;
    }

    let inner = block.inner(area);
    f.render_widget(block, area);

    let items: Vec<ListItem> = data
        .active_protocols
        .iter()
        .enumerate()
        .map(|(i, proto)| {
            let style = if is_selected && i == data.selected_index {
                Style::default().fg(theme.highlight)
            } else {
                Style::default().fg(theme.text)
            };

            let status_style = match proto.status {
                ProtocolStatus::Running => Style::default().fg(theme.healthy),
                ProtocolStatus::Paused => Style::default().fg(theme.warning),
                ProtocolStatus::WaitingApproval => Style::default().fg(theme.warning),
                ProtocolStatus::WaitingCondition => Style::default().fg(theme.info),
            };

            let lines = vec![
                Line::from(vec![
                    Span::styled(format!("{} ", proto.status.symbol()), status_style),
                    Span::styled(&proto.name, style),
                    Span::styled(
                        format!(" on {}", proto.machine_id),
                        Style::default().fg(theme.muted),
                    ),
                ]),
                Line::from(vec![
                    Span::raw("   └─ "),
                    Span::styled(
                        format!(
                            "Step {}/{}: {}",
                            proto.current_step, proto.total_steps, proto.step_description
                        ),
                        Style::default().fg(theme.muted),
                    ),
                ]),
                Line::from(vec![
                    Span::raw("   └─ "),
                    Span::styled(
                        format!("Started: {}", proto.started_ago),
                        Style::default().fg(theme.muted),
                    ),
                ]),
            ];

            ListItem::new(lines)
        })
        .collect();

    let list = List::new(items);
    f.render_widget(list, inner);
}

fn render_pending(f: &mut Frame, area: Rect, data: &GuardianData, theme: &Theme) {
    let is_selected = data.selected_section == GuardianSection::Pending;

    let block = Block::default()
        .title(format!(
            " Pending Interventions ({}) ",
            data.pending_approvals.len()
        ))
        .borders(Borders::ALL)
        .border_style(if is_selected {
            Style::default().fg(theme.highlight)
        } else {
            Style::default().fg(theme.border)
        });

    if data.pending_approvals.is_empty() {
        let empty = Paragraph::new("  No pending approvals")
            .style(Style::default().fg(theme.muted))
            .block(block);
        f.render_widget(empty, area);
        return;
    }

    let inner = block.inner(area);
    f.render_widget(block, area);

    let items: Vec<ListItem> = data
        .pending_approvals
        .iter()
        .enumerate()
        .map(|(i, pending)| {
            let style = if is_selected && i == data.selected_index {
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            };

            let lines = vec![
                Line::from(vec![
                    Span::styled("├─ ", Style::default().fg(theme.muted)),
                    Span::styled(&pending.playbook_name, style),
                ]),
                Line::from(vec![
                    Span::raw("│  └─ "),
                    Span::styled("Waiting for: ", Style::default().fg(theme.muted)),
                    Span::styled("manual approval", Style::default().fg(theme.warning)),
                ]),
                Line::from(vec![
                    Span::raw("│  └─ "),
                    Span::styled("Actions: ", Style::default().fg(theme.muted)),
                    Span::styled(&pending.action_description, Style::default().fg(theme.text)),
                ]),
            ];

            ListItem::new(lines)
        })
        .collect();

    let list = List::new(items);
    f.render_widget(list, inner);
}

fn render_history(f: &mut Frame, area: Rect, data: &GuardianData, theme: &Theme) {
    let is_selected = data.selected_section == GuardianSection::History;

    let block = Block::default()
        .title(" History (last 24h) ")
        .borders(Borders::ALL)
        .border_style(if is_selected {
            Style::default().fg(theme.highlight)
        } else {
            Style::default().fg(theme.border)
        });

    if data.recent_runs.is_empty() {
        let empty = Paragraph::new("  No recent runs")
            .style(Style::default().fg(theme.muted))
            .block(block);
        f.render_widget(empty, area);
        return;
    }

    let inner = block.inner(area);
    f.render_widget(block, area);

    let items: Vec<ListItem> = data
        .recent_runs
        .iter()
        .enumerate()
        .map(|(i, run)| {
            let style = if is_selected && i == data.selected_index {
                Style::default().fg(theme.highlight)
            } else {
                Style::default().fg(theme.text)
            };

            let result_style = match run.result {
                RunResult::Success => Style::default().fg(theme.healthy),
                RunResult::Failed => Style::default().fg(theme.critical),
                RunResult::Aborted => Style::default().fg(theme.warning),
                RunResult::Escalated => Style::default().fg(theme.warning),
            };

            let lines = vec![
                Line::from(vec![
                    Span::styled(format!("[{}] ", run.result.label()), result_style),
                    Span::styled(&run.playbook_name, style),
                    Span::styled(
                        format!(" ({}) - {}", run.machine_id, run.completed_ago),
                        Style::default().fg(theme.muted),
                    ),
                ]),
                Line::from(vec![
                    Span::raw("   └─ "),
                    Span::styled(&run.summary, Style::default().fg(theme.muted)),
                ]),
            ];

            ListItem::new(lines)
        })
        .collect();

    let list = List::new(items);
    f.render_widget(list, inner);
}

fn render_footer(f: &mut Frame, area: Rect, data: &GuardianData, theme: &Theme) {
    let help_text = match data.selected_section {
        GuardianSection::Status => "[t]oggle mode  [Tab]section  [p]ause  [r]esume",
        GuardianSection::Active => "[p]ause  [c]ancel  [Tab]section  [Enter]details",
        GuardianSection::Pending => "[y]approve  [n]reject  [Tab]section  [Enter]details",
        GuardianSection::History => "[Enter]details  [Tab]section  [h]full history",
    };

    let content = Line::from(vec![
        Span::styled(" ", Style::default()),
        Span::styled(help_text, Style::default().fg(theme.muted)),
    ]);

    let footer = Paragraph::new(content).block(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(theme.border)),
    );

    f.render_widget(footer, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guardian_mode_labels() {
        assert_eq!(GuardianMode::Off.label(), "off");
        assert_eq!(GuardianMode::Suggest.label(), "suggest-only");
        assert_eq!(GuardianMode::ExecuteSafe.label(), "execute-safe");
        assert_eq!(GuardianMode::WithApproval.label(), "with-approval");
    }

    #[test]
    fn test_guardian_mode_cycling() {
        assert_eq!(GuardianMode::Off.next(), GuardianMode::Suggest);
        assert_eq!(GuardianMode::Suggest.next(), GuardianMode::ExecuteSafe);
        assert_eq!(GuardianMode::ExecuteSafe.next(), GuardianMode::WithApproval);
        assert_eq!(GuardianMode::WithApproval.next(), GuardianMode::Off);
    }

    #[test]
    fn test_guardian_section_navigation() {
        assert_eq!(GuardianSection::Status.next(), GuardianSection::Active);
        assert_eq!(GuardianSection::Active.next(), GuardianSection::Pending);
        assert_eq!(GuardianSection::Pending.next(), GuardianSection::History);
        assert_eq!(GuardianSection::History.next(), GuardianSection::Status);

        assert_eq!(GuardianSection::Status.prev(), GuardianSection::History);
    }

    #[test]
    fn test_protocol_status_symbols() {
        assert_eq!(ProtocolStatus::Running.symbol(), "▶");
        assert_eq!(ProtocolStatus::Paused.symbol(), "⏸");
        assert_eq!(ProtocolStatus::WaitingApproval.symbol(), "⏳");
        assert_eq!(ProtocolStatus::WaitingCondition.symbol(), "⏱");
    }

    #[test]
    fn test_run_result_symbols() {
        assert_eq!(RunResult::Success.symbol(), "✓");
        assert_eq!(RunResult::Failed.symbol(), "✗");
        assert_eq!(RunResult::Aborted.symbol(), "⊘");
        assert_eq!(RunResult::Escalated.symbol(), "↑");
    }

    #[test]
    fn test_run_result_labels() {
        assert_eq!(RunResult::Success.label(), "OK");
        assert_eq!(RunResult::Failed.label(), "FAIL");
        assert_eq!(RunResult::Aborted.label(), "ABORT");
        assert_eq!(RunResult::Escalated.label(), "ESCALATED");
    }

    #[test]
    fn test_default_guardian_data() {
        let data = GuardianData::default();
        assert!(data.active_protocols.is_empty());
        assert!(data.pending_approvals.is_empty());
        assert!(data.recent_runs.is_empty());
        assert_eq!(data.selected_section, GuardianSection::Status);
    }

    #[test]
    fn test_default_guardian_status() {
        let status = GuardianStatus::default();
        assert_eq!(status.mode, GuardianMode::Suggest);
        assert!(!status.enabled);
        assert!(status.last_action.is_none());
    }

    #[test]
    fn test_default_active_protocol() {
        let proto = ActiveProtocol::default();
        assert!(proto.playbook_id.is_empty());
        assert_eq!(proto.current_step, 0);
        assert_eq!(proto.status, ProtocolStatus::Running);
    }

    #[test]
    fn test_default_pending_approval() {
        let pending = PendingApproval::default();
        assert_eq!(pending.id, 0);
        assert!(pending.playbook_id.is_empty());
    }

    #[test]
    fn test_default_guardian_run() {
        let run = GuardianRun::default();
        assert_eq!(run.id, 0);
        assert_eq!(run.result, RunResult::Success);
    }

    #[test]
    fn test_active_protocol_with_data() {
        let proto = ActiveProtocol {
            playbook_id: "rate-limit-switch".to_string(),
            name: "Rate Limit Account Switch".to_string(),
            machine_id: "orko".to_string(),
            current_step: 2,
            total_steps: 4,
            step_description: "Preparing account swap".to_string(),
            started_ago: "45 sec".to_string(),
            status: ProtocolStatus::Running,
        };

        assert_eq!(proto.current_step, 2);
        assert_eq!(proto.status.symbol(), "▶");
    }

    #[test]
    fn test_guardian_run_failed() {
        let run = GuardianRun {
            result: RunResult::Failed,
            summary: "Agent did not recover".to_string(),
            ..Default::default()
        };

        assert_eq!(run.result.symbol(), "✗");
        assert_eq!(run.result.label(), "FAIL");
    }

    #[test]
    fn test_guardian_mode_descriptions() {
        assert!(!GuardianMode::Off.description().is_empty());
        assert!(!GuardianMode::Suggest.description().is_empty());
        assert!(!GuardianMode::ExecuteSafe.description().is_empty());
        assert!(!GuardianMode::WithApproval.description().is_empty());
    }
}
