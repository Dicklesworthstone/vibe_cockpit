//! Guardian screen implementation
//!
//! Displays self-healing status, active protocols, pending approvals, and history.

use crate::theme::Theme;
use ftui::{
    Frame as FtuiFrame, PackedRgba, Style as FtuiStyle,
    layout::{Constraint as FtuiConstraint, Flex, Rect as FtuiRect},
    text::{Line as FtuiLine, Span as FtuiSpan, Text as FtuiText},
    widgets::{
        Widget as FtuiWidget,
        block::Block as FtuiBlock,
        borders::Borders as FtuiBorders,
        list::{List as FtuiList, ListItem as FtuiListItem},
        paragraph::Paragraph as FtuiParagraph,
    },
};

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
    #[must_use]
    pub fn next(&self) -> Self {
        match self {
            Self::Status => Self::Active,
            Self::Active => Self::Pending,
            Self::Pending => Self::History,
            Self::History => Self::Status,
        }
    }

    #[must_use]
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
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Suggest => "suggest-only",
            Self::ExecuteSafe => "execute-safe",
            Self::WithApproval => "with-approval",
        }
    }

    #[must_use]
    pub fn description(&self) -> &'static str {
        match self {
            Self::Off => "Guardian disabled",
            Self::Suggest => "Shows suggestions, no automatic actions",
            Self::ExecuteSafe => "Executes allowlisted safe commands only",
            Self::WithApproval => "Executes safe + queues destructive for approval",
        }
    }

    #[must_use]
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
#[derive(Debug, Clone, Default)]
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
    #[must_use]
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Running => "▶",
            Self::Paused => "⏸",
            Self::WaitingApproval => "⏳",
            Self::WaitingCondition => "⏱",
        }
    }

    #[must_use]
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
#[derive(Debug, Clone, Default)]
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

/// Guardian run history entry
#[derive(Debug, Clone, Default)]
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
    #[must_use]
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Success => "✓",
            Self::Failed => "✗",
            Self::Aborted => "⊘",
            Self::Escalated => "↑",
        }
    }

    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Success => "OK",
            Self::Failed => "FAIL",
            Self::Aborted => "ABORT",
            Self::Escalated => "ESCALATED",
        }
    }
}

pub fn render_guardian_ftui(f: &mut FtuiFrame, data: &GuardianData, theme: &Theme) {
    let rows = Flex::vertical()
        .constraints([
            FtuiConstraint::Fixed(3),
            FtuiConstraint::Fixed(6),
            FtuiConstraint::Fixed(8),
            FtuiConstraint::Fixed(6),
            FtuiConstraint::Fill,
            FtuiConstraint::Fixed(3),
        ])
        .split(ftui_full_area(f));

    if rows.len() < 6 {
        return;
    }

    render_guardian_ftui_header(f, rows[0], data, theme);
    render_guardian_ftui_status(f, rows[1], data, theme);
    render_guardian_ftui_active(f, rows[2], data, theme);
    render_guardian_ftui_pending(f, rows[3], data, theme);
    render_guardian_ftui_history(f, rows[4], data, theme);
    render_guardian_ftui_footer(f, rows[5], data, theme);
}

fn render_guardian_ftui_header(
    f: &mut FtuiFrame,
    area: FtuiRect,
    data: &GuardianData,
    theme: &Theme,
) {
    let colors = theme.ftui_colors();
    let header = FtuiParagraph::new(FtuiText::from_spans(vec![
        FtuiSpan::styled(
            "  GUARDIAN  ",
            FtuiStyle::new().fg(packed(colors.text)).bold(),
        ),
        FtuiSpan::styled("Self-Healing", FtuiStyle::new().fg(packed(colors.muted))),
        FtuiSpan::raw(" "),
        FtuiSpan::styled(
            format!("[{}]", data.status.mode.label()),
            FtuiStyle::new()
                .fg(packed(guardian_mode_color(data.status.mode, theme)))
                .bold(),
        ),
        FtuiSpan::raw(" "),
        FtuiSpan::styled(
            if data.status.enabled {
                "[enabled]"
            } else {
                "[disabled]"
            },
            FtuiStyle::new().fg(packed(if data.status.enabled {
                colors.healthy
            } else {
                colors.warning
            })),
        ),
        FtuiSpan::raw(" "),
        FtuiSpan::styled(
            format!("[{} active]", data.active_protocols.len()),
            FtuiStyle::new().fg(packed(colors.info)),
        ),
        FtuiSpan::raw(" "),
        FtuiSpan::styled(
            format!("[{} pending]", data.pending_approvals.len()),
            FtuiStyle::new().fg(packed(colors.warning)),
        ),
    ]))
    .style(FtuiStyle::new().bg(packed(colors.bg_secondary)))
    .block(ftui_block(None, colors.muted));

    FtuiWidget::render(&header, area, f);
}

fn render_guardian_ftui_status(
    f: &mut FtuiFrame,
    area: FtuiRect,
    data: &GuardianData,
    theme: &Theme,
) {
    let colors = theme.ftui_colors();
    let status = &data.status;
    let lines = vec![
        FtuiLine::from_spans([
            FtuiSpan::styled("Mode: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                format!("{} ({})", status.mode.label(), status.mode.description()),
                FtuiStyle::new().fg(packed(guardian_mode_color(status.mode, theme))),
            ),
        ]),
        FtuiLine::from_spans([
            FtuiSpan::styled("Patterns: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                format!("{} active", status.active_patterns),
                FtuiStyle::new().fg(packed(colors.text)),
            ),
        ]),
        FtuiLine::from_spans([
            FtuiSpan::styled("Last action: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                status.last_action.as_deref().unwrap_or("never"),
                FtuiStyle::new().fg(packed(colors.text)),
            ),
        ]),
        FtuiLine::from_spans([
            FtuiSpan::styled("Success rate: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                format!(
                    "{:.0}% ({}/{} last week)",
                    status.success_rate_7d, status.successful_runs, status.total_runs
                ),
                FtuiStyle::new().fg(packed(success_rate_color(status.success_rate_7d, theme))),
            ),
        ]),
    ];

    let status_panel = FtuiParagraph::new(FtuiText::from_lines(lines)).block(ftui_block(
        Some(" Status "),
        section_border_color(data.selected_section == GuardianSection::Status, theme),
    ));

    FtuiWidget::render(&status_panel, area, f);
}

fn render_guardian_ftui_active(
    f: &mut FtuiFrame,
    area: FtuiRect,
    data: &GuardianData,
    theme: &Theme,
) {
    let colors = theme.ftui_colors();
    if data.active_protocols.is_empty() {
        let empty = FtuiParagraph::new(FtuiText::from_spans([FtuiSpan::styled(
            "No active protocols",
            FtuiStyle::new().fg(packed(colors.muted)),
        )]))
        .block(ftui_block(
            Some(" Active Protocols "),
            section_border_color(data.selected_section == GuardianSection::Active, theme),
        ));
        FtuiWidget::render(&empty, area, f);
        return;
    }

    let is_selected_section = data.selected_section == GuardianSection::Active;
    let clamped_selected = data
        .selected_index
        .min(data.active_protocols.len().saturating_sub(1));
    let items: Vec<FtuiListItem> = data
        .active_protocols
        .iter()
        .enumerate()
        .map(|(index, proto)| {
            let row_style = if is_selected_section && index == clamped_selected {
                FtuiStyle::new().bg(packed(colors.bg_secondary))
            } else {
                FtuiStyle::new()
            };

            FtuiListItem::new(FtuiText::from_lines([
                FtuiLine::from_spans([
                    FtuiSpan::styled(
                        format!("{} ", proto.status.symbol()),
                        FtuiStyle::new()
                            .fg(packed(protocol_status_color(proto.status, theme)))
                            .bold(),
                    ),
                    FtuiSpan::styled(&proto.name, FtuiStyle::new().fg(packed(colors.text))),
                    FtuiSpan::styled(
                        format!(" on {}", proto.machine_id),
                        FtuiStyle::new().fg(packed(colors.muted)),
                    ),
                ]),
                FtuiLine::from_spans([
                    FtuiSpan::raw("  "),
                    FtuiSpan::styled(
                        format!(
                            "Step {}/{}: {}",
                            proto.current_step, proto.total_steps, proto.step_description
                        ),
                        FtuiStyle::new().fg(packed(colors.info)),
                    ),
                ]),
                FtuiLine::from_spans([
                    FtuiSpan::raw("  "),
                    FtuiSpan::styled(
                        format!("Started: {}", proto.started_ago),
                        FtuiStyle::new().fg(packed(colors.muted)),
                    ),
                ]),
            ]))
            .style(row_style)
        })
        .collect();

    let list = FtuiList::new(items).block(ftui_block(
        Some(" Active Protocols "),
        section_border_color(is_selected_section, theme),
    ));
    FtuiWidget::render(&list, area, f);
}

fn render_guardian_ftui_pending(
    f: &mut FtuiFrame,
    area: FtuiRect,
    data: &GuardianData,
    theme: &Theme,
) {
    let colors = theme.ftui_colors();
    if data.pending_approvals.is_empty() {
        let empty = FtuiParagraph::new(FtuiText::from_spans([FtuiSpan::styled(
            "No pending approvals",
            FtuiStyle::new().fg(packed(colors.muted)),
        )]))
        .block(ftui_block(
            Some(" Pending Interventions "),
            section_border_color(data.selected_section == GuardianSection::Pending, theme),
        ));
        FtuiWidget::render(&empty, area, f);
        return;
    }

    let is_selected_section = data.selected_section == GuardianSection::Pending;
    let clamped_selected = data
        .selected_index
        .min(data.pending_approvals.len().saturating_sub(1));
    let items: Vec<FtuiListItem> = data
        .pending_approvals
        .iter()
        .enumerate()
        .map(|(index, pending)| {
            let row_style = if is_selected_section && index == clamped_selected {
                FtuiStyle::new().bg(packed(colors.bg_secondary))
            } else {
                FtuiStyle::new()
            };

            FtuiListItem::new(FtuiText::from_lines([
                FtuiLine::from_spans([
                    FtuiSpan::styled(
                        &pending.playbook_name,
                        FtuiStyle::new().fg(packed(colors.text)),
                    ),
                    FtuiSpan::styled(
                        format!(" ({})", pending.machine_id),
                        FtuiStyle::new().fg(packed(colors.muted)),
                    ),
                ]),
                FtuiLine::from_spans([
                    FtuiSpan::styled("Waiting: ", FtuiStyle::new().fg(packed(colors.muted))),
                    FtuiSpan::styled(
                        "manual approval",
                        FtuiStyle::new().fg(packed(colors.warning)).bold(),
                    ),
                    FtuiSpan::raw(" "),
                    FtuiSpan::styled(
                        &pending.queued_ago,
                        FtuiStyle::new().fg(packed(colors.info)),
                    ),
                ]),
                FtuiLine::from_spans([
                    FtuiSpan::styled("Action: ", FtuiStyle::new().fg(packed(colors.muted))),
                    FtuiSpan::styled(
                        &pending.action_description,
                        FtuiStyle::new().fg(packed(colors.text)),
                    ),
                ]),
            ]))
            .style(row_style)
        })
        .collect();

    let list = FtuiList::new(items).block(ftui_block(
        Some(" Pending Interventions "),
        section_border_color(is_selected_section, theme),
    ));
    FtuiWidget::render(&list, area, f);
}

fn render_guardian_ftui_history(
    f: &mut FtuiFrame,
    area: FtuiRect,
    data: &GuardianData,
    theme: &Theme,
) {
    let colors = theme.ftui_colors();
    if data.recent_runs.is_empty() {
        let empty = FtuiParagraph::new(FtuiText::from_spans([FtuiSpan::styled(
            "No recent runs",
            FtuiStyle::new().fg(packed(colors.muted)),
        )]))
        .block(ftui_block(
            Some(" History (last 24h) "),
            section_border_color(data.selected_section == GuardianSection::History, theme),
        ));
        FtuiWidget::render(&empty, area, f);
        return;
    }

    let is_selected_section = data.selected_section == GuardianSection::History;
    let clamped_selected = data
        .selected_index
        .min(data.recent_runs.len().saturating_sub(1));
    let items: Vec<FtuiListItem> = data
        .recent_runs
        .iter()
        .enumerate()
        .map(|(index, run)| {
            let row_style = if is_selected_section && index == clamped_selected {
                FtuiStyle::new().bg(packed(colors.bg_secondary))
            } else {
                FtuiStyle::new()
            };

            FtuiListItem::new(FtuiText::from_lines([
                FtuiLine::from_spans([
                    FtuiSpan::styled(
                        format!("[{}] ", run.result.label()),
                        FtuiStyle::new()
                            .fg(packed(run_result_color(run.result, theme)))
                            .bold(),
                    ),
                    FtuiSpan::styled(&run.playbook_name, FtuiStyle::new().fg(packed(colors.text))),
                    FtuiSpan::styled(
                        format!(" ({}) - {}", run.machine_id, run.completed_ago),
                        FtuiStyle::new().fg(packed(colors.muted)),
                    ),
                ]),
                FtuiLine::from_spans([FtuiSpan::styled(
                    &run.summary,
                    FtuiStyle::new().fg(packed(colors.info)),
                )]),
            ]))
            .style(row_style)
        })
        .collect();

    let list = FtuiList::new(items).block(ftui_block(
        Some(" History (last 24h) "),
        section_border_color(is_selected_section, theme),
    ));
    FtuiWidget::render(&list, area, f);
}

fn render_guardian_ftui_footer(
    f: &mut FtuiFrame,
    area: FtuiRect,
    data: &GuardianData,
    theme: &Theme,
) {
    let colors = theme.ftui_colors();
    let help_text = match data.selected_section {
        GuardianSection::Status => "[t]oggle mode [Tab]section [p]ause [r]esume",
        GuardianSection::Active => "[p]ause [c]ancel [Tab]section [Enter]details",
        GuardianSection::Pending => "[y]approve [n]reject [Tab]section [Enter]details",
        GuardianSection::History => "[Enter]details [Tab]section [h]history",
    };

    let footer = FtuiParagraph::new(FtuiText::from_spans([FtuiSpan::styled(
        help_text,
        FtuiStyle::new().fg(packed(colors.muted)),
    )]))
    .style(FtuiStyle::new().bg(packed(colors.bg_secondary)))
    .block(ftui_block(None, colors.muted));

    FtuiWidget::render(&footer, area, f);
}

fn guardian_mode_color(mode: GuardianMode, theme: &Theme) -> ftui::Color {
    match mode {
        GuardianMode::Off => theme.ftui_colors().critical,
        GuardianMode::Suggest => theme.ftui_colors().info,
        GuardianMode::ExecuteSafe => theme.ftui_colors().healthy,
        GuardianMode::WithApproval => theme.ftui_colors().warning,
    }
}

fn success_rate_color(success_rate: f64, theme: &Theme) -> ftui::Color {
    if success_rate >= 90.0 {
        theme.ftui_colors().healthy
    } else if success_rate >= 70.0 {
        theme.ftui_colors().warning
    } else {
        theme.ftui_colors().critical
    }
}

fn protocol_status_color(status: ProtocolStatus, theme: &Theme) -> ftui::Color {
    match status {
        ProtocolStatus::Running => theme.ftui_colors().healthy,
        ProtocolStatus::Paused | ProtocolStatus::WaitingApproval => theme.ftui_colors().warning,
        ProtocolStatus::WaitingCondition => theme.ftui_colors().info,
    }
}

fn run_result_color(result: RunResult, theme: &Theme) -> ftui::Color {
    match result {
        RunResult::Success => theme.ftui_colors().healthy,
        RunResult::Failed => theme.ftui_colors().critical,
        RunResult::Aborted | RunResult::Escalated => theme.ftui_colors().warning,
    }
}

fn section_border_color(is_selected: bool, theme: &Theme) -> ftui::Color {
    if is_selected {
        theme.ftui_colors().accent
    } else {
        theme.ftui_colors().muted
    }
}

fn ftui_block(title: Option<&str>, border_color: ftui::Color) -> FtuiBlock<'_> {
    let mut block = FtuiBlock::new()
        .borders(FtuiBorders::ALL)
        .border_style(FtuiStyle::new().fg(packed(border_color)));
    if let Some(title) = title {
        block = block.title(title);
    }
    block
}

fn ftui_full_area(frame: &FtuiFrame) -> FtuiRect {
    FtuiRect::new(0, 0, frame.width(), frame.height())
}

fn packed(color: ftui::Color) -> PackedRgba {
    let rgb = color.to_rgb();
    PackedRgba::rgb(rgb.r, rgb.g, rgb.b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ftui::{Buffer, GraphemePool};

    fn buffer_contains(buffer: &Buffer, width: u16, height: u16, needle: &str) -> bool {
        (0..height).any(|y| {
            let row: String = (0..width)
                .map(|x| {
                    buffer
                        .get(x, y)
                        .and_then(|cell| cell.content.as_char())
                        .unwrap_or(' ')
                })
                .collect();
            row.contains(needle)
        })
    }

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

    #[test]
    fn test_render_guardian_ftui_renders_operational_panels() {
        let data = GuardianData {
            status: GuardianStatus {
                mode: GuardianMode::WithApproval,
                enabled: true,
                active_patterns: 7,
                last_action: Some("2m ago".to_string()),
                success_rate_7d: 92.0,
                successful_runs: 22,
                total_runs: 24,
            },
            active_protocols: vec![ActiveProtocol {
                playbook_id: "pb-1".to_string(),
                name: "Rate Limit Account Switch".to_string(),
                machine_id: "orko".to_string(),
                current_step: 2,
                total_steps: 4,
                step_description: "Preparing backup account".to_string(),
                started_ago: "45s ago".to_string(),
                status: ProtocolStatus::Running,
            }],
            pending_approvals: vec![PendingApproval {
                id: 10,
                playbook_id: "pb-2".to_string(),
                playbook_name: "Kill runaway cargo".to_string(),
                machine_id: "orko".to_string(),
                action_description: "Terminate runaway build tree".to_string(),
                reason: "Load is pinned".to_string(),
                queued_ago: "30s ago".to_string(),
            }],
            recent_runs: vec![GuardianRun {
                id: 1,
                playbook_id: "pb-3".to_string(),
                playbook_name: "Clear stale locks".to_string(),
                machine_id: "sydneymc".to_string(),
                result: RunResult::Success,
                completed_ago: "10m ago".to_string(),
                summary: "Recovered build throughput".to_string(),
            }],
            selected_section: GuardianSection::Active,
            selected_index: 0,
        };
        let theme = Theme::default();
        let mut pool = GraphemePool::new();
        let mut frame = FtuiFrame::new(100, 34, &mut pool);

        render_guardian_ftui(&mut frame, &data, &theme);

        assert!(buffer_contains(&frame.buffer, 100, 34, "GUARDIAN"));
        assert!(buffer_contains(
            &frame.buffer,
            100,
            34,
            "Rate Limit Account Switch"
        ));
        assert!(buffer_contains(
            &frame.buffer,
            100,
            34,
            "Kill runaway cargo"
        ));
        assert!(buffer_contains(&frame.buffer, 100, 34, "Clear stale locks"));
    }

    #[test]
    fn test_render_guardian_ftui_renders_empty_state() {
        let data = GuardianData::default();
        let theme = Theme::default();
        let mut pool = GraphemePool::new();
        let mut frame = FtuiFrame::new(88, 32, &mut pool);

        render_guardian_ftui(&mut frame, &data, &theme);

        assert!(buffer_contains(
            &frame.buffer,
            88,
            32,
            "No active protocols"
        ));
        assert!(buffer_contains(
            &frame.buffer,
            88,
            32,
            "No pending approvals"
        ));
        assert!(buffer_contains(&frame.buffer, 88, 32, "No recent runs"));
    }
}
