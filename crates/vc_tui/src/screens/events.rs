//! Events screen implementation
//!
//! Displays DCG denies, RANO network anomalies, and PT process issues.

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
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use crate::theme::Theme;

/// Data needed to render the events screen
#[derive(Debug, Clone, Default)]
pub struct EventsData {
    /// DCG (dangerous command guard) events
    pub dcg_events: Vec<DcgEvent>,
    /// RANO (network observer) events
    pub rano_events: Vec<RanoEvent>,
    /// PT (process tracker) findings
    pub pt_findings: Vec<PtFinding>,
    /// Currently selected section
    pub selected_section: EventSection,
    /// Selected index within section
    pub selected_index: usize,
    /// Event filter
    pub filter: EventFilter,
    /// Time range for events
    pub time_range: TimeRange,
    /// Statistics
    pub stats: EventStats,
}

/// Event sections for navigation
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum EventSection {
    #[default]
    Dcg,
    Rano,
    Pt,
}

impl EventSection {
    #[must_use]
    pub fn next(&self) -> Self {
        match self {
            Self::Dcg => Self::Rano,
            Self::Rano => Self::Pt,
            Self::Pt => Self::Dcg,
        }
    }

    #[must_use]
    pub fn prev(&self) -> Self {
        match self {
            Self::Dcg => Self::Pt,
            Self::Rano => Self::Dcg,
            Self::Pt => Self::Rano,
        }
    }

    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Dcg => "DCG",
            Self::Rano => "Network",
            Self::Pt => "Processes",
        }
    }
}

/// Event filter options
#[derive(Debug, Clone, Default)]
pub struct EventFilter {
    /// Filter by machine
    pub machine_id: Option<String>,
    /// Filter by severity
    pub min_severity: Option<EventSeverity>,
    /// Search text
    pub search: Option<String>,
}

/// Time range for events
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum TimeRange {
    Hour1,
    Hour6,
    #[default]
    Hour24,
    Days7,
}

impl TimeRange {
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Hour1 => "1h",
            Self::Hour6 => "6h",
            Self::Hour24 => "24h",
            Self::Days7 => "7d",
        }
    }

    #[must_use]
    pub fn next(&self) -> Self {
        match self {
            Self::Hour1 => Self::Hour6,
            Self::Hour6 => Self::Hour24,
            Self::Hour24 => Self::Days7,
            Self::Days7 => Self::Hour1,
        }
    }
}

/// Event severity
#[derive(Debug, Clone, Copy, Default, PartialEq, Ord, PartialOrd, Eq)]
pub enum EventSeverity {
    Critical,
    High,
    #[default]
    Medium,
    Low,
    Info,
}

impl EventSeverity {
    #[must_use]
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Critical => "🔴",
            Self::High => "🟠",
            Self::Medium => "🟡",
            Self::Low => "🔵",
            Self::Info => "⚪",
        }
    }

    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
            Self::Info => "info",
        }
    }
}

/// DCG (dangerous command guard) event
#[derive(Debug, Clone, Default)]
pub struct DcgEvent {
    /// Event ID
    pub id: u64,
    /// Machine where blocked
    pub machine_id: String,
    /// Command that was blocked
    pub command: String,
    /// Why it was blocked
    pub reason: String,
    /// Severity
    pub severity: EventSeverity,
    /// When blocked
    pub timestamp: String,
    /// How long ago (human readable)
    pub age: String,
    /// Process/session that attempted it
    pub source: Option<String>,
}

/// RANO (network observer) event
#[derive(Debug, Clone, Default)]
pub struct RanoEvent {
    /// Event ID
    pub id: u64,
    /// Machine
    pub machine_id: String,
    /// Event type
    pub event_type: RanoEventType,
    /// Remote host/domain
    pub remote_host: String,
    /// Process making the connection
    pub process: String,
    /// Process PID
    pub pid: u32,
    /// Connection count
    pub connection_count: u32,
    /// Timestamp
    pub timestamp: String,
    /// Age (human readable)
    pub age: String,
    /// Severity
    pub severity: EventSeverity,
    /// Additional details
    pub details: Option<String>,
}

/// RANO event types
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum RanoEventType {
    /// Unknown/suspicious provider
    #[default]
    UnknownProvider,
    /// Authentication loop detected
    AuthLoop,
    /// High connection volume
    HighVolume,
    /// Unusual port
    UnusualPort,
    /// Blocked by policy
    Blocked,
}

impl RanoEventType {
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::UnknownProvider => "Unknown provider",
            Self::AuthLoop => "Auth loop detected",
            Self::HighVolume => "High volume",
            Self::UnusualPort => "Unusual port",
            Self::Blocked => "Blocked",
        }
    }
}

/// PT (process tracker) finding
#[derive(Debug, Clone, Default)]
pub struct PtFinding {
    /// Finding ID
    pub id: u64,
    /// Machine
    pub machine_id: String,
    /// Finding type
    pub finding_type: PtFindingType,
    /// Process name
    pub process_name: String,
    /// Process PID
    pub pid: u32,
    /// Finding details
    pub details: String,
    /// Severity
    pub severity: EventSeverity,
    /// Timestamp
    pub timestamp: String,
    /// Age (human readable)
    pub age: String,
    /// Metric value (e.g., CPU %, memory, etc.)
    pub metric_value: Option<String>,
}

/// PT finding types
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum PtFindingType {
    /// Zombie process
    #[default]
    Zombie,
    /// Stuck agent (no velocity)
    StuckAgent,
    /// Runaway process (high CPU)
    Runaway,
    /// Memory hog
    MemoryHog,
    /// Long-running build
    LongBuild,
    /// Orphaned process
    Orphaned,
}

impl PtFindingType {
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Zombie => "Zombie process",
            Self::StuckAgent => "Stuck agent",
            Self::Runaway => "Runaway process",
            Self::MemoryHog => "Memory hog",
            Self::LongBuild => "Long build",
            Self::Orphaned => "Orphaned",
        }
    }

    #[must_use]
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Zombie => "💀",
            Self::StuckAgent => "🔒",
            Self::Runaway => "🔥",
            Self::MemoryHog => "🐘",
            Self::LongBuild => "⏰",
            Self::Orphaned => "👻",
        }
    }
}

/// Event statistics
#[derive(Debug, Clone, Default)]
pub struct EventStats {
    /// Total DCG denies in time range
    pub dcg_total: u32,
    /// Critical DCG denies
    pub dcg_critical: u32,
    /// Total RANO events
    pub rano_total: u32,
    /// Total PT findings
    pub pt_total: u32,
    /// Machines with events
    pub machines_affected: u32,
}

/// Render the events screen
pub fn render_events(f: &mut Frame, data: &EventsData, theme: &Theme) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),      // Header
            Constraint::Percentage(40), // DCG
            Constraint::Percentage(30), // RANO
            Constraint::Percentage(30), // PT
            Constraint::Length(3),      // Footer
        ])
        .split(f.area());

    render_header(f, chunks[0], data, theme);
    render_dcg(f, chunks[1], data, theme);
    render_rano(f, chunks[2], data, theme);
    render_pt(f, chunks[3], data, theme);
    render_footer(f, chunks[4], data, theme);
}

fn render_header(f: &mut Frame, area: Rect, data: &EventsData, theme: &Theme) {
    let title = Line::from(vec![
        Span::styled(
            "  E V E N T S  ",
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        // Section tabs
        Span::styled(
            if data.selected_section == EventSection::Dcg {
                " [DCG] "
            } else {
                " DCG "
            },
            if data.selected_section == EventSection::Dcg {
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.muted)
            },
        ),
        Span::styled(
            if data.selected_section == EventSection::Rano {
                " [Network] "
            } else {
                " Network "
            },
            if data.selected_section == EventSection::Rano {
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.muted)
            },
        ),
        Span::styled(
            if data.selected_section == EventSection::Pt {
                " [Processes] "
            } else {
                " Processes "
            },
            if data.selected_section == EventSection::Pt {
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.muted)
            },
        ),
        Span::raw("  "),
        Span::styled(
            format!("[{}]", data.time_range.label()),
            Style::default().fg(theme.info),
        ),
        Span::raw("  "),
        Span::styled("[f]ilter [/]search", Style::default().fg(theme.muted)),
    ]);

    let header = Paragraph::new(title).block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(theme.border)),
    );

    f.render_widget(header, area);
}

fn render_dcg(f: &mut Frame, area: Rect, data: &EventsData, theme: &Theme) {
    let is_selected = data.selected_section == EventSection::Dcg;

    let critical_count = data
        .dcg_events
        .iter()
        .filter(|e| e.severity == EventSeverity::Critical)
        .count();

    let title = format!(
        " DCG Denies ({}) {}",
        data.dcg_events.len(),
        if critical_count > 0 {
            format!("[{critical_count} critical]")
        } else {
            String::new()
        }
    );

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(if is_selected {
            Style::default().fg(theme.highlight)
        } else {
            Style::default().fg(theme.border)
        });

    if data.dcg_events.is_empty() {
        let empty = Paragraph::new("  ✓ No blocked commands")
            .style(Style::default().fg(theme.healthy))
            .block(block);
        f.render_widget(empty, area);
        return;
    }

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Group by machine
    let mut by_machine: std::collections::HashMap<&str, Vec<&DcgEvent>> =
        std::collections::HashMap::new();
    for event in &data.dcg_events {
        by_machine.entry(&event.machine_id).or_default().push(event);
    }

    let mut items: Vec<ListItem> = Vec::new();
    // Track a flattened event index across all machine groups for correct selection
    let mut flat_event_index: usize = 0;
    for (machine, events) in by_machine {
        let critical = events
            .iter()
            .filter(|e| e.severity == EventSeverity::Critical)
            .count();

        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                format!("├─ {}: {} denies", machine, events.len()),
                Style::default().fg(theme.text),
            ),
            if critical > 0 {
                Span::styled(
                    format!(" ({critical} critical)"),
                    Style::default().fg(theme.critical),
                )
            } else {
                Span::raw("")
            },
        ])));

        // Show top events
        for event in events.iter().take(3) {
            let is_event_selected = is_selected && data.selected_index == flat_event_index;
            let style = if is_event_selected {
                Style::default().fg(theme.highlight)
            } else {
                Style::default().fg(theme.muted)
            };

            let severity_style = match event.severity {
                EventSeverity::Critical => Style::default().fg(theme.critical),
                EventSeverity::High => Style::default().fg(theme.warning),
                _ => Style::default().fg(theme.muted),
            };

            items.push(ListItem::new(Line::from(vec![
                Span::raw("│   └─ "),
                Span::styled(&event.command, style),
                Span::styled(format!(" ({})", event.severity.label()), severity_style),
                Span::styled(
                    format!(" [{}]", event.age),
                    Style::default().fg(theme.muted),
                ),
            ])));
            flat_event_index += 1;
        }
    }

    let list = List::new(items);
    f.render_widget(list, inner);
}

fn render_rano(f: &mut Frame, area: Rect, data: &EventsData, theme: &Theme) {
    let is_selected = data.selected_section == EventSection::Rano;

    let block = Block::default()
        .title(format!(" Network Anomalies ({}) ", data.rano_events.len()))
        .borders(Borders::ALL)
        .border_style(if is_selected {
            Style::default().fg(theme.highlight)
        } else {
            Style::default().fg(theme.border)
        });

    if data.rano_events.is_empty() {
        let empty = Paragraph::new("  ✓ No network anomalies")
            .style(Style::default().fg(theme.healthy))
            .block(block);
        f.render_widget(empty, area);
        return;
    }

    let inner = block.inner(area);
    f.render_widget(block, area);

    let items: Vec<ListItem> = data
        .rano_events
        .iter()
        .enumerate()
        .map(|(i, event)| {
            let style = if is_selected && i == data.selected_index {
                Style::default().fg(theme.highlight)
            } else {
                Style::default().fg(theme.text)
            };

            let lines = vec![
                Line::from(vec![
                    Span::styled("├─ ", Style::default().fg(theme.muted)),
                    Span::styled(event.event_type.label(), style),
                    Span::styled(
                        format!(": {}", event.remote_host),
                        Style::default().fg(theme.text),
                    ),
                ]),
                Line::from(vec![
                    Span::raw("│   └─ "),
                    Span::styled(
                        format!(
                            "PID {} ({}) -> {} connections",
                            event.pid, event.process, event.connection_count
                        ),
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

fn render_pt(f: &mut Frame, area: Rect, data: &EventsData, theme: &Theme) {
    let is_selected = data.selected_section == EventSection::Pt;

    let block = Block::default()
        .title(format!(" Process Issues ({}) ", data.pt_findings.len()))
        .borders(Borders::ALL)
        .border_style(if is_selected {
            Style::default().fg(theme.highlight)
        } else {
            Style::default().fg(theme.border)
        });

    if data.pt_findings.is_empty() {
        let empty = Paragraph::new("  ✓ No process issues")
            .style(Style::default().fg(theme.healthy))
            .block(block);
        f.render_widget(empty, area);
        return;
    }

    let inner = block.inner(area);
    f.render_widget(block, area);

    let items: Vec<ListItem> = data
        .pt_findings
        .iter()
        .enumerate()
        .map(|(i, finding)| {
            let style = if is_selected && i == data.selected_index {
                Style::default().fg(theme.highlight)
            } else {
                Style::default().fg(theme.text)
            };

            let severity_style = match finding.severity {
                EventSeverity::Critical => Style::default().fg(theme.critical),
                EventSeverity::High => Style::default().fg(theme.warning),
                _ => Style::default().fg(theme.text),
            };

            let line = Line::from(vec![
                Span::styled(format!("{} ", finding.finding_type.symbol()), style),
                Span::styled(finding.finding_type.label(), severity_style),
                Span::styled(
                    format!(": {} on {}", finding.process_name, finding.machine_id),
                    Style::default().fg(theme.text),
                ),
                if let Some(ref metric) = finding.metric_value {
                    Span::styled(format!(" ({metric})"), Style::default().fg(theme.muted))
                } else {
                    Span::raw("")
                },
            ]);

            ListItem::new(line)
        })
        .collect();

    let list = List::new(items);
    f.render_widget(list, inner);
}

fn render_footer(f: &mut Frame, area: Rect, data: &EventsData, theme: &Theme) {
    let help_text = "[Tab]section  [t]ime range  [f]ilter  [Enter]details  [j/k]navigate";

    let content = Line::from(vec![
        Span::styled(
            format!(
                " {} events ",
                data.stats.dcg_total + data.stats.rano_total + data.stats.pt_total
            ),
            Style::default().fg(theme.muted),
        ),
        Span::styled("│", Style::default().fg(theme.border)),
        Span::styled(
            format!(" {} machines ", data.stats.machines_affected),
            Style::default().fg(theme.muted),
        ),
        Span::raw("    "),
        Span::styled(help_text, Style::default().fg(theme.muted)),
    ]);

    let footer = Paragraph::new(content).block(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(theme.border)),
    );

    f.render_widget(footer, area);
}

pub fn render_events_ftui(f: &mut FtuiFrame, data: &EventsData, theme: &Theme) {
    let rows = Flex::vertical()
        .constraints([
            FtuiConstraint::Fixed(3),
            FtuiConstraint::Fill,
            FtuiConstraint::Fixed(3),
        ])
        .gap(1)
        .split(ftui_full_area(f));

    if rows.len() < 3 {
        return;
    }

    render_events_ftui_header(f, rows[0], data, theme);
    render_events_ftui_content(f, rows[1], data, theme);
    render_events_ftui_footer(f, rows[2], data, theme);
}

fn render_events_ftui_header(f: &mut FtuiFrame, area: FtuiRect, data: &EventsData, theme: &Theme) {
    let colors = theme.ftui_colors();
    let filter_summary = filter_summary(&data.filter);
    let spans = vec![
        FtuiSpan::styled(
            "  EVENTS  ",
            FtuiStyle::new().fg(packed(colors.text)).bold(),
        ),
        FtuiSpan::styled(
            format!("[{}]", data.selected_section.label()),
            FtuiStyle::new()
                .fg(packed(event_section_color(data.selected_section, theme)))
                .bold(),
        ),
        FtuiSpan::raw(" "),
        FtuiSpan::styled(
            format!("[scope: {}]", data.time_range.label()),
            FtuiStyle::new().fg(packed(colors.info)),
        ),
        FtuiSpan::raw(" "),
        FtuiSpan::styled(
            format!("[{} machines]", data.stats.machines_affected),
            FtuiStyle::new().fg(packed(colors.muted)),
        ),
        FtuiSpan::raw(" "),
        FtuiSpan::styled(
            format!(
                "[{} total]",
                data.stats.dcg_total + data.stats.rano_total + data.stats.pt_total
            ),
            FtuiStyle::new().fg(packed(colors.warning)),
        ),
        FtuiSpan::raw(" "),
        FtuiSpan::styled(filter_summary, FtuiStyle::new().fg(packed(colors.muted))),
    ];

    let header = FtuiParagraph::new(FtuiText::from_spans(spans))
        .style(FtuiStyle::new().bg(packed(colors.bg_secondary)))
        .block(ftui_block(None, colors.muted));
    FtuiWidget::render(&header, area, f);
}

fn render_events_ftui_content(f: &mut FtuiFrame, area: FtuiRect, data: &EventsData, theme: &Theme) {
    if area.height < 18 {
        match data.selected_section {
            EventSection::Dcg => render_events_ftui_dcg(f, area, data, theme),
            EventSection::Rano => render_events_ftui_rano(f, area, data, theme),
            EventSection::Pt => render_events_ftui_pt(f, area, data, theme),
        }
        return;
    }

    let rows = Flex::vertical()
        .constraints([
            FtuiConstraint::Percentage(34.0),
            FtuiConstraint::Percentage(33.0),
            FtuiConstraint::Percentage(33.0),
        ])
        .gap(1)
        .split(area);

    if rows.len() < 3 {
        return;
    }

    render_events_ftui_dcg(f, rows[0], data, theme);
    render_events_ftui_rano(f, rows[1], data, theme);
    render_events_ftui_pt(f, rows[2], data, theme);
}

fn render_events_ftui_dcg(f: &mut FtuiFrame, area: FtuiRect, data: &EventsData, theme: &Theme) {
    let colors = theme.ftui_colors();
    let items = filtered_dcg_events(data);
    let border_color = if data.selected_section == EventSection::Dcg {
        colors.accent
    } else {
        colors.muted
    };

    if items.is_empty() {
        let empty = FtuiParagraph::new(FtuiText::from_spans([FtuiSpan::styled(
            "No DCG events in scope",
            FtuiStyle::new().fg(packed(colors.muted)),
        )]))
        .block(ftui_block(Some(" DCG Denies "), border_color));
        FtuiWidget::render(&empty, area, f);
        return;
    }

    let clamped_selected = data.selected_index.min(items.len().saturating_sub(1));
    let list_items: Vec<FtuiListItem> = items
        .iter()
        .enumerate()
        .map(|(index, event)| {
            let row_style =
                if data.selected_section == EventSection::Dcg && index == clamped_selected {
                    FtuiStyle::new().bg(packed(colors.bg_secondary))
                } else {
                    FtuiStyle::new()
                };
            let severity_color = severity_color(event.severity, theme);
            let source = event.source.as_deref().unwrap_or("unknown source");

            FtuiListItem::new(FtuiText::from_lines([
                FtuiLine::from_spans([
                    FtuiSpan::styled(
                        format!("{} ", event.severity.symbol()),
                        FtuiStyle::new().fg(packed(severity_color)).bold(),
                    ),
                    FtuiSpan::styled(
                        event.severity.label().to_ascii_uppercase(),
                        FtuiStyle::new().fg(packed(severity_color)).bold(),
                    ),
                    FtuiSpan::raw(" "),
                    FtuiSpan::styled(&event.machine_id, FtuiStyle::new().fg(packed(colors.info))),
                    FtuiSpan::raw(" "),
                    FtuiSpan::styled(
                        truncate_chars(&event.command, 54),
                        FtuiStyle::new().fg(packed(colors.text)),
                    ),
                ]),
                FtuiLine::from_spans([
                    FtuiSpan::raw("    "),
                    FtuiSpan::styled(
                        format!("{} | {}", event.age, truncate_chars(&event.reason, 58)),
                        FtuiStyle::new().fg(packed(colors.muted)),
                    ),
                ]),
                FtuiLine::from_spans([
                    FtuiSpan::raw("    "),
                    FtuiSpan::styled(
                        format!("source {source}"),
                        FtuiStyle::new().fg(packed(colors.warning)),
                    ),
                ]),
            ]))
            .style(row_style)
        })
        .collect();

    let list = FtuiList::new(list_items).block(ftui_block(Some(" DCG Denies "), border_color));
    FtuiWidget::render(&list, area, f);
}

fn render_events_ftui_rano(f: &mut FtuiFrame, area: FtuiRect, data: &EventsData, theme: &Theme) {
    let colors = theme.ftui_colors();
    let items = filtered_rano_events(data);
    let border_color = if data.selected_section == EventSection::Rano {
        colors.accent
    } else {
        colors.muted
    };

    if items.is_empty() {
        let empty = FtuiParagraph::new(FtuiText::from_spans([FtuiSpan::styled(
            "No RANO events in scope",
            FtuiStyle::new().fg(packed(colors.muted)),
        )]))
        .block(ftui_block(Some(" Network Anomalies "), border_color));
        FtuiWidget::render(&empty, area, f);
        return;
    }

    let clamped_selected = data.selected_index.min(items.len().saturating_sub(1));
    let list_items: Vec<FtuiListItem> = items
        .iter()
        .enumerate()
        .map(|(index, event)| {
            let row_style =
                if data.selected_section == EventSection::Rano && index == clamped_selected {
                    FtuiStyle::new().bg(packed(colors.bg_secondary))
                } else {
                    FtuiStyle::new()
                };
            let severity_color = severity_color(event.severity, theme);

            FtuiListItem::new(FtuiText::from_lines([
                FtuiLine::from_spans([
                    FtuiSpan::styled(
                        format!("{} ", event.severity.symbol()),
                        FtuiStyle::new().fg(packed(severity_color)).bold(),
                    ),
                    FtuiSpan::styled(
                        event.event_type.label(),
                        FtuiStyle::new().fg(packed(severity_color)).bold(),
                    ),
                    FtuiSpan::raw(" "),
                    FtuiSpan::styled(
                        truncate_chars(&event.remote_host, 34),
                        FtuiStyle::new().fg(packed(colors.text)),
                    ),
                ]),
                FtuiLine::from_spans([
                    FtuiSpan::raw("    "),
                    FtuiSpan::styled(
                        format!(
                            "{} | {} pid {} | {} conns | {}",
                            event.machine_id,
                            truncate_chars(&event.process, 18),
                            event.pid,
                            event.connection_count,
                            event.age
                        ),
                        FtuiStyle::new().fg(packed(colors.muted)),
                    ),
                ]),
                FtuiLine::from_spans([
                    FtuiSpan::raw("    "),
                    FtuiSpan::styled(
                        event.details.as_deref().unwrap_or("No additional details"),
                        FtuiStyle::new().fg(packed(colors.info)),
                    ),
                ]),
            ]))
            .style(row_style)
        })
        .collect();

    let list =
        FtuiList::new(list_items).block(ftui_block(Some(" Network Anomalies "), border_color));
    FtuiWidget::render(&list, area, f);
}

fn render_events_ftui_pt(f: &mut FtuiFrame, area: FtuiRect, data: &EventsData, theme: &Theme) {
    let colors = theme.ftui_colors();
    let items = filtered_pt_findings(data);
    let border_color = if data.selected_section == EventSection::Pt {
        colors.accent
    } else {
        colors.muted
    };

    if items.is_empty() {
        let empty = FtuiParagraph::new(FtuiText::from_spans([FtuiSpan::styled(
            "No process findings in scope",
            FtuiStyle::new().fg(packed(colors.muted)),
        )]))
        .block(ftui_block(Some(" Process Issues "), border_color));
        FtuiWidget::render(&empty, area, f);
        return;
    }

    let clamped_selected = data.selected_index.min(items.len().saturating_sub(1));
    let list_items: Vec<FtuiListItem> = items
        .iter()
        .enumerate()
        .map(|(index, finding)| {
            let row_style =
                if data.selected_section == EventSection::Pt && index == clamped_selected {
                    FtuiStyle::new().bg(packed(colors.bg_secondary))
                } else {
                    FtuiStyle::new()
                };
            let severity_color = severity_color(finding.severity, theme);

            FtuiListItem::new(FtuiText::from_lines([
                FtuiLine::from_spans([
                    FtuiSpan::styled(
                        format!("{} ", finding.finding_type.symbol()),
                        FtuiStyle::new().fg(packed(severity_color)).bold(),
                    ),
                    FtuiSpan::styled(
                        finding.finding_type.label(),
                        FtuiStyle::new().fg(packed(severity_color)).bold(),
                    ),
                    FtuiSpan::raw(" "),
                    FtuiSpan::styled(
                        truncate_chars(&finding.process_name, 26),
                        FtuiStyle::new().fg(packed(colors.text)),
                    ),
                ]),
                FtuiLine::from_spans([
                    FtuiSpan::raw("    "),
                    FtuiSpan::styled(
                        format!(
                            "{} pid {} | {}",
                            finding.machine_id, finding.pid, finding.age
                        ),
                        FtuiStyle::new().fg(packed(colors.muted)),
                    ),
                ]),
                FtuiLine::from_spans([
                    FtuiSpan::raw("    "),
                    FtuiSpan::styled(
                        finding
                            .metric_value
                            .as_deref()
                            .unwrap_or("No metric recorded"),
                        FtuiStyle::new().fg(packed(colors.warning)),
                    ),
                ]),
            ]))
            .style(row_style)
        })
        .collect();

    let list = FtuiList::new(list_items).block(ftui_block(Some(" Process Issues "), border_color));
    FtuiWidget::render(&list, area, f);
}

fn render_events_ftui_footer(f: &mut FtuiFrame, area: FtuiRect, data: &EventsData, theme: &Theme) {
    let colors = theme.ftui_colors();
    let footer = FtuiParagraph::new(FtuiText::from_spans([
        FtuiSpan::styled("Focus:", FtuiStyle::new().fg(packed(colors.muted))),
        FtuiSpan::raw(" "),
        FtuiSpan::styled(
            "DCG",
            FtuiStyle::new().fg(packed(event_section_color(EventSection::Dcg, theme))),
        ),
        FtuiSpan::raw(" / "),
        FtuiSpan::styled(
            "Network",
            FtuiStyle::new().fg(packed(event_section_color(EventSection::Rano, theme))),
        ),
        FtuiSpan::raw(" / "),
        FtuiSpan::styled(
            "Processes",
            FtuiStyle::new().fg(packed(event_section_color(EventSection::Pt, theme))),
        ),
        FtuiSpan::raw("  "),
        FtuiSpan::styled(
            format!("scope {}", data.time_range.label()),
            FtuiStyle::new().fg(packed(colors.info)),
        ),
        FtuiSpan::raw("  "),
        FtuiSpan::styled(
            filter_summary(&data.filter),
            FtuiStyle::new().fg(packed(colors.muted)),
        ),
    ]))
    .style(FtuiStyle::new().bg(packed(colors.bg_secondary)))
    .block(ftui_block(None, colors.muted));
    FtuiWidget::render(&footer, area, f);
}

fn filtered_dcg_events(data: &EventsData) -> Vec<&DcgEvent> {
    data.dcg_events
        .iter()
        .filter(|event| {
            matches_filter(
                &event.machine_id,
                event.severity,
                &[
                    &event.command,
                    &event.reason,
                    event.source.as_deref().unwrap_or(""),
                ],
                &data.filter,
            )
        })
        .collect()
}

fn filtered_rano_events(data: &EventsData) -> Vec<&RanoEvent> {
    data.rano_events
        .iter()
        .filter(|event| {
            matches_filter(
                &event.machine_id,
                event.severity,
                &[
                    event.event_type.label(),
                    &event.remote_host,
                    &event.process,
                    event.details.as_deref().unwrap_or(""),
                ],
                &data.filter,
            )
        })
        .collect()
}

fn filtered_pt_findings(data: &EventsData) -> Vec<&PtFinding> {
    data.pt_findings
        .iter()
        .filter(|finding| {
            matches_filter(
                &finding.machine_id,
                finding.severity,
                &[
                    finding.finding_type.label(),
                    &finding.process_name,
                    finding.metric_value.as_deref().unwrap_or(""),
                ],
                &data.filter,
            )
        })
        .collect()
}

fn matches_filter(
    machine_id: &str,
    severity: EventSeverity,
    fields: &[&str],
    filter: &EventFilter,
) -> bool {
    if let Some(target_machine) = filter.machine_id.as_deref()
        && !machine_id.eq_ignore_ascii_case(target_machine)
    {
        return false;
    }

    if let Some(min_severity) = filter.min_severity
        && severity > min_severity
    {
        return false;
    }

    if let Some(search) = filter.search.as_deref() {
        let needle = search.to_lowercase();
        if !fields
            .iter()
            .any(|field| field.to_lowercase().contains(&needle))
            && !machine_id.to_lowercase().contains(&needle)
        {
            return false;
        }
    }

    true
}

fn filter_summary(filter: &EventFilter) -> String {
    let mut parts = Vec::new();
    if let Some(machine_id) = filter.machine_id.as_deref() {
        parts.push(format!("machine {machine_id}"));
    }
    if let Some(min_severity) = filter.min_severity {
        parts.push(format!("min {}", min_severity.label()));
    }
    if let Some(search) = filter.search.as_deref() {
        parts.push(format!("search {search}"));
    }

    if parts.is_empty() {
        "[no filter]".to_string()
    } else {
        format!("[{}]", parts.join(" | "))
    }
}

fn event_section_color(section: EventSection, theme: &Theme) -> ftui::Color {
    match section {
        EventSection::Dcg => theme.ftui_colors().critical,
        EventSection::Rano => theme.ftui_colors().info,
        EventSection::Pt => theme.ftui_colors().warning,
    }
}

fn severity_color(severity: EventSeverity, theme: &Theme) -> ftui::Color {
    match severity {
        EventSeverity::Critical => theme.ftui_colors().critical,
        EventSeverity::High => theme.ftui_colors().warning,
        EventSeverity::Medium => theme.ftui_colors().info,
        EventSeverity::Low | EventSeverity::Info => theme.ftui_colors().muted,
    }
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_chars {
        value.to_string()
    } else {
        let truncated: String = value.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{truncated}…")
    }
}

fn ftui_block(title: Option<&str>, border_color: ftui::Color) -> FtuiBlock<'_> {
    let block = FtuiBlock::default()
        .borders(FtuiBorders::ALL)
        .border_style(FtuiStyle::new().fg(packed(border_color)));
    if let Some(title) = title {
        block.title(title)
    } else {
        block
    }
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
        let mut rows = Vec::with_capacity(usize::from(height));
        for y in 0..height {
            let row: String = (0..width)
                .map(|x| {
                    buffer
                        .get(x, y)
                        .and_then(|cell| cell.content.as_char())
                        .unwrap_or(' ')
                })
                .collect();
            rows.push(row);
        }
        rows.join("\n").contains(needle)
    }

    #[test]
    fn test_event_section_navigation() {
        assert_eq!(EventSection::Dcg.next(), EventSection::Rano);
        assert_eq!(EventSection::Rano.next(), EventSection::Pt);
        assert_eq!(EventSection::Pt.next(), EventSection::Dcg);

        assert_eq!(EventSection::Dcg.prev(), EventSection::Pt);
    }

    #[test]
    fn test_event_section_labels() {
        assert_eq!(EventSection::Dcg.label(), "DCG");
        assert_eq!(EventSection::Rano.label(), "Network");
        assert_eq!(EventSection::Pt.label(), "Processes");
    }

    #[test]
    fn test_time_range_cycling() {
        assert_eq!(TimeRange::Hour1.next(), TimeRange::Hour6);
        assert_eq!(TimeRange::Hour6.next(), TimeRange::Hour24);
        assert_eq!(TimeRange::Hour24.next(), TimeRange::Days7);
        assert_eq!(TimeRange::Days7.next(), TimeRange::Hour1);
    }

    #[test]
    fn test_time_range_labels() {
        assert_eq!(TimeRange::Hour1.label(), "1h");
        assert_eq!(TimeRange::Hour24.label(), "24h");
        assert_eq!(TimeRange::Days7.label(), "7d");
    }

    #[test]
    fn test_event_severity_symbols() {
        assert_eq!(EventSeverity::Critical.symbol(), "🔴");
        assert_eq!(EventSeverity::High.symbol(), "🟠");
        assert_eq!(EventSeverity::Medium.symbol(), "🟡");
        assert_eq!(EventSeverity::Low.symbol(), "🔵");
        assert_eq!(EventSeverity::Info.symbol(), "⚪");
    }

    #[test]
    fn test_event_severity_ordering() {
        assert!(EventSeverity::Critical < EventSeverity::High);
        assert!(EventSeverity::High < EventSeverity::Medium);
    }

    #[test]
    fn test_rano_event_type_labels() {
        assert_eq!(RanoEventType::UnknownProvider.label(), "Unknown provider");
        assert_eq!(RanoEventType::AuthLoop.label(), "Auth loop detected");
        assert_eq!(RanoEventType::HighVolume.label(), "High volume");
    }

    #[test]
    fn test_pt_finding_type_symbols() {
        assert_eq!(PtFindingType::Zombie.symbol(), "💀");
        assert_eq!(PtFindingType::StuckAgent.symbol(), "🔒");
        assert_eq!(PtFindingType::Runaway.symbol(), "🔥");
        assert_eq!(PtFindingType::MemoryHog.symbol(), "🐘");
    }

    #[test]
    fn test_pt_finding_type_labels() {
        assert_eq!(PtFindingType::Zombie.label(), "Zombie process");
        assert_eq!(PtFindingType::StuckAgent.label(), "Stuck agent");
        assert_eq!(PtFindingType::Runaway.label(), "Runaway process");
    }

    #[test]
    fn test_default_events_data() {
        let data = EventsData::default();
        assert!(data.dcg_events.is_empty());
        assert!(data.rano_events.is_empty());
        assert!(data.pt_findings.is_empty());
        assert_eq!(data.selected_section, EventSection::Dcg);
        assert_eq!(data.time_range, TimeRange::Hour24);
    }

    #[test]
    fn test_default_dcg_event() {
        let event = DcgEvent::default();
        assert_eq!(event.id, 0);
        assert!(event.command.is_empty());
        assert_eq!(event.severity, EventSeverity::Medium);
    }

    #[test]
    fn test_default_rano_event() {
        let event = RanoEvent::default();
        assert_eq!(event.id, 0);
        assert_eq!(event.pid, 0);
        assert_eq!(event.event_type, RanoEventType::UnknownProvider);
    }

    #[test]
    fn test_default_pt_finding() {
        let finding = PtFinding::default();
        assert_eq!(finding.id, 0);
        assert_eq!(finding.finding_type, PtFindingType::Zombie);
    }

    #[test]
    fn test_dcg_event_with_data() {
        let event = DcgEvent {
            id: 1,
            machine_id: "orko".to_string(),
            command: "rm -rf /".to_string(),
            reason: "Dangerous command".to_string(),
            severity: EventSeverity::Critical,
            timestamp: "2026-01-28T12:34:00Z".to_string(),
            age: "5m".to_string(),
            source: Some("claude-code".to_string()),
        };

        assert_eq!(event.severity.symbol(), "🔴");
        assert_eq!(event.source.as_deref(), Some("claude-code"));
    }

    #[test]
    fn test_rano_auth_loop() {
        let event = RanoEvent {
            event_type: RanoEventType::AuthLoop,
            remote_host: "api.anthropic.com".to_string(),
            ..Default::default()
        };

        assert_eq!(event.event_type.label(), "Auth loop detected");
    }

    #[test]
    fn test_pt_stuck_agent() {
        let finding = PtFinding {
            finding_type: PtFindingType::StuckAgent,
            process_name: "claude-code".to_string(),
            metric_value: Some("0 velocity for 15min".to_string()),
            ..Default::default()
        };

        assert_eq!(finding.finding_type.symbol(), "🔒");
        assert!(finding.metric_value.is_some());
    }

    #[test]
    fn test_event_stats_default() {
        let stats = EventStats::default();
        assert_eq!(stats.dcg_total, 0);
        assert_eq!(stats.rano_total, 0);
        assert_eq!(stats.pt_total, 0);
    }

    #[test]
    fn test_event_filter_default() {
        let filter = EventFilter::default();
        assert!(filter.machine_id.is_none());
        assert!(filter.min_severity.is_none());
        assert!(filter.search.is_none());
    }

    #[test]
    fn test_render_events_ftui_renders_selected_section() {
        let data = EventsData {
            dcg_events: vec![DcgEvent {
                id: 1,
                machine_id: "orko".to_string(),
                command: "rm -rf target".to_string(),
                reason: "Dangerous command".to_string(),
                severity: EventSeverity::Critical,
                timestamp: "2026-03-13T10:00:00Z".to_string(),
                age: "2m".to_string(),
                source: Some("claude-code".to_string()),
            }],
            rano_events: vec![RanoEvent {
                id: 2,
                machine_id: "orko".to_string(),
                event_type: RanoEventType::AuthLoop,
                remote_host: "api.anthropic.com".to_string(),
                process: "claude-code".to_string(),
                pid: 4242,
                connection_count: 9,
                timestamp: "2026-03-13T10:01:00Z".to_string(),
                age: "1m".to_string(),
                severity: EventSeverity::High,
                details: Some("Repeated login attempts".to_string()),
            }],
            pt_findings: vec![PtFinding {
                id: 3,
                machine_id: "orko".to_string(),
                finding_type: PtFindingType::Runaway,
                process_name: "cargo test".to_string(),
                pid: 31337,
                timestamp: "2026-03-13T10:02:00Z".to_string(),
                age: "30s".to_string(),
                severity: EventSeverity::Medium,
                metric_value: Some("CPU 390%".to_string()),
                details: String::new(),
            }],
            selected_section: EventSection::Rano,
            selected_index: 0,
            filter: EventFilter::default(),
            time_range: TimeRange::Hour24,
            stats: EventStats {
                dcg_total: 1,
                dcg_critical: 1,
                rano_total: 1,
                pt_total: 1,
                machines_affected: 1,
            },
        };
        let theme = Theme::default();
        let mut pool = GraphemePool::new();
        let mut frame = FtuiFrame::new(120, 28, &mut pool);

        render_events_ftui(&mut frame, &data, &theme);

        assert!(buffer_contains(&frame.buffer, 120, 28, "EVENTS"));
        assert!(buffer_contains(
            &frame.buffer,
            120,
            28,
            "Auth loop detected"
        ));
    }

    #[test]
    fn test_render_events_ftui_renders_empty_filtered_state() {
        let data = EventsData {
            filter: EventFilter {
                machine_id: Some("other".to_string()),
                min_severity: None,
                search: None,
            },
            ..EventsData::default()
        };
        let theme = Theme::default();
        let mut pool = GraphemePool::new();
        let mut frame = FtuiFrame::new(100, 24, &mut pool);

        render_events_ftui(&mut frame, &data, &theme);

        assert!(buffer_contains(&frame.buffer, 100, 24, "EVENTS"));
        assert!(buffer_contains(
            &frame.buffer,
            100,
            24,
            "No DCG events in scope"
        ));
    }
}
