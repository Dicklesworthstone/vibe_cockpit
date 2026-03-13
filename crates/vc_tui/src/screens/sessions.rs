//! Sessions screen implementation
//!
//! Displays active coding sessions from cass (session search) collector.

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
        table::{Row as FtuiRow, Table as FtuiTable},
    },
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Row, Table},
};
use std::collections::BTreeMap;

use crate::theme::Theme;

/// Data needed to render the sessions screen
#[derive(Debug, Clone, Default)]
pub struct SessionsData {
    /// List of sessions
    pub sessions: Vec<SessionInfo>,
    /// Currently selected index
    pub selected: usize,
    /// Grouping mode
    pub group_by: SessionGroupBy,
    /// Filter string
    pub filter: String,
    /// Currently expanded groups (for tree view)
    pub expanded_groups: Vec<String>,
}

/// Session grouping options
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum SessionGroupBy {
    #[default]
    None,
    Project,
    Model,
    Agent,
}

/// Individual session information
#[derive(Debug, Clone)]
pub struct SessionInfo {
    /// Session ID
    pub id: String,
    /// Project/workspace path
    pub project: String,
    /// Model being used
    pub model: String,
    /// Agent name
    pub agent: String,
    /// Session start time
    pub started_at: String,
    /// Duration in minutes
    pub duration_mins: u32,
    /// Total tokens used
    pub tokens: u64,
    /// Estimated cost
    pub cost: f64,
    /// Is session currently active?
    pub is_active: bool,
    /// Last activity timestamp
    pub last_activity: String,
}

impl Default for SessionInfo {
    fn default() -> Self {
        Self {
            id: String::new(),
            project: String::new(),
            model: String::new(),
            agent: String::new(),
            started_at: String::new(),
            duration_mins: 0,
            tokens: 0,
            cost: 0.0,
            is_active: false,
            last_activity: String::new(),
        }
    }
}

impl SessionInfo {
    /// Format duration as human-readable string
    #[must_use]
    pub fn duration_str(&self) -> String {
        if self.duration_mins < 60 {
            format!("{}m", self.duration_mins)
        } else {
            let hours = self.duration_mins / 60;
            let mins = self.duration_mins % 60;
            format!("{hours}h{mins}m")
        }
    }

    /// Format tokens as human-readable string
    #[must_use]
    pub fn tokens_str(&self) -> String {
        if self.tokens >= 1_000 {
            format_token_count(self.tokens)
        } else {
            self.tokens.to_string()
        }
    }

    /// Format cost as string
    #[must_use]
    pub fn cost_str(&self) -> String {
        if self.cost >= 1.0 {
            format!("${:.2}", self.cost)
        } else {
            format!("${:.3}", self.cost)
        }
    }
}

/// Render the sessions screen
pub fn render_sessions(f: &mut Frame, data: &SessionsData, theme: &Theme) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Main content
            Constraint::Length(3), // Footer
        ])
        .split(f.area());

    render_header(f, chunks[0], data, theme);
    render_sessions_content(f, chunks[1], data, theme);
    render_footer(f, chunks[2], theme);
}

fn render_header(f: &mut Frame, area: Rect, data: &SessionsData, theme: &Theme) {
    let total_sessions = data.sessions.len();
    let active_count = data.sessions.iter().filter(|s| s.is_active).count();
    let total_tokens: u64 = data.sessions.iter().map(|s| s.tokens).sum();
    let total_cost: f64 = data.sessions.iter().map(|s| s.cost).sum();

    let group_label = match data.group_by {
        SessionGroupBy::None => "ungrouped",
        SessionGroupBy::Project => "by project",
        SessionGroupBy::Model => "by model",
        SessionGroupBy::Agent => "by agent",
    };

    let title = Line::from(vec![
        Span::styled(
            "  S E S S I O N S  ",
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("[{total_sessions} sessions]"),
            Style::default().fg(theme.muted),
        ),
        Span::raw("  "),
        Span::styled(
            format!("[{active_count} active]"),
            Style::default().fg(theme.healthy),
        ),
        Span::raw("  "),
        Span::styled(format!("[{group_label}]"), Style::default().fg(theme.info)),
        Span::raw("  "),
        Span::styled(
            format!(
                "[{} tokens / ${total_cost:.2}]",
                format_token_count(total_tokens)
            ),
            Style::default().fg(theme.accent),
        ),
    ]);

    let header = Paragraph::new(title)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.muted)),
        )
        .style(Style::default().bg(theme.bg_secondary));

    f.render_widget(header, area);
}

fn render_sessions_content(f: &mut Frame, area: Rect, data: &SessionsData, theme: &Theme) {
    if data.sessions.is_empty() {
        let empty = Paragraph::new(Span::styled(
            "  No sessions tracked. Run cass collector to populate data.",
            Style::default().fg(theme.muted),
        ))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.muted)),
        );
        f.render_widget(empty, area);
        return;
    }

    match data.group_by {
        SessionGroupBy::None => render_sessions_table(f, area, data, theme),
        _ => render_sessions_grouped(f, area, data, theme),
    }
}

fn render_sessions_table(f: &mut Frame, area: Rect, data: &SessionsData, theme: &Theme) {
    let filtered = filtered_sessions(data);
    if filtered.is_empty() {
        let empty = Paragraph::new(Span::styled(
            "  No sessions match the current filter.",
            Style::default().fg(theme.muted),
        ))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.muted)),
        );
        f.render_widget(empty, area);
        return;
    }

    // Clamp selection to filtered list bounds to prevent index mismatch
    let clamped_selected = data.selected.min(filtered.len().saturating_sub(1));

    let rows: Vec<Row> = filtered
        .iter()
        .enumerate()
        .map(|(index, session)| render_session_row(session, index == clamped_selected, theme))
        .collect();

    let header_style = Style::default()
        .fg(theme.accent)
        .add_modifier(Modifier::BOLD);

    let table = Table::new(
        rows,
        [
            Constraint::Length(1),  // Active marker
            Constraint::Length(16), // Project
            Constraint::Length(14), // Model
            Constraint::Length(14), // Agent
            Constraint::Length(6),  // Duration
            Constraint::Length(8),  // Tokens
            Constraint::Length(8),  // Cost
            Constraint::Min(10),    // Last Activity
        ],
    )
    .header(
        Row::new(vec![
            Line::from(Span::styled(" ", header_style)),
            Line::from(Span::styled("Project", header_style)),
            Line::from(Span::styled("Model", header_style)),
            Line::from(Span::styled("Agent", header_style)),
            Line::from(Span::styled("Time", header_style)),
            Line::from(Span::styled("Tokens", header_style)),
            Line::from(Span::styled("Cost", header_style)),
            Line::from(Span::styled("Last Active", header_style)),
        ])
        .bottom_margin(1),
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.muted)),
    );

    f.render_widget(table, area);
}

fn render_sessions_grouped(f: &mut Frame, area: Rect, data: &SessionsData, theme: &Theme) {
    // Group sessions by the selected field
    let mut groups: BTreeMap<String, Vec<&SessionInfo>> = BTreeMap::new();

    for session in filtered_sessions(data) {
        let key = match data.group_by {
            SessionGroupBy::Project => session.project.clone(),
            SessionGroupBy::Model => session.model.clone(),
            SessionGroupBy::Agent => session.agent.clone(),
            SessionGroupBy::None => unreachable!(),
        };
        groups.entry(key).or_default().push(session);
    }

    // Build tree items
    let mut items: Vec<ListItem> = Vec::new();

    for (group_name, sessions) in &groups {
        let is_expanded = data.expanded_groups.contains(group_name);
        let expand_marker = if is_expanded { "▼" } else { "▶" };

        let active_count = sessions.iter().filter(|s| s.is_active).count();
        let total_tokens: u64 = sessions.iter().map(|s| s.tokens).sum();
        let total_cost: f64 = sessions.iter().map(|s| s.cost).sum();

        // Group header
        items.push(ListItem::new(Line::from(vec![
            Span::styled(expand_marker, Style::default().fg(theme.accent)),
            Span::raw(" "),
            Span::styled(group_name, Style::default().fg(theme.text)),
            Span::styled(
                format!(" ({} sessions", sessions.len()),
                Style::default().fg(theme.muted),
            ),
            if active_count > 0 {
                Span::styled(
                    format!(", {active_count} active"),
                    Style::default().fg(theme.healthy),
                )
            } else {
                Span::raw("")
            },
            Span::styled(
                format!(", {} / ${total_cost:.2})", format_token_count(total_tokens)),
                Style::default().fg(theme.muted),
            ),
        ])));

        // Child sessions if expanded
        if is_expanded {
            for session in sessions {
                let active_marker = if session.is_active { "●" } else { "○" };
                let active_color = if session.is_active {
                    theme.healthy
                } else {
                    theme.muted
                };

                items.push(ListItem::new(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(active_marker, Style::default().fg(active_color)),
                    Span::raw(" "),
                    Span::styled(&session.agent, Style::default().fg(theme.info)),
                    Span::raw(" "),
                    Span::styled(
                        format!(
                            "{} / {} / {}",
                            session.duration_str(),
                            session.tokens_str(),
                            session.cost_str()
                        ),
                        Style::default().fg(theme.muted),
                    ),
                ])));
            }
        }
    }

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.muted)),
    );

    f.render_widget(list, area);
}

fn render_footer(f: &mut Frame, area: Rect, theme: &Theme) {
    let shortcuts = vec![
        ("[Tab]", "Overview"),
        ("[j/k]", "Navigate"),
        ("[g]", "Group"),
        ("[Enter]", "Expand"),
        ("[/]", "Filter"),
        ("[q]", "Back"),
    ];

    let spans: Vec<Span> = shortcuts
        .into_iter()
        .flat_map(|(key, action)| {
            vec![
                Span::styled(key, Style::default().fg(theme.accent)),
                Span::styled(action, Style::default().fg(theme.muted)),
                Span::raw(" "),
            ]
        })
        .collect();

    let footer = Paragraph::new(Line::from(spans))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.muted)),
        )
        .style(Style::default().bg(theme.bg_secondary));

    f.render_widget(footer, area);
}

pub fn render_sessions_ftui(f: &mut FtuiFrame, data: &SessionsData, theme: &Theme) {
    let rows = Flex::vertical()
        .constraints([
            FtuiConstraint::Fixed(3),
            FtuiConstraint::Fill,
            FtuiConstraint::Fixed(3),
        ])
        .split(ftui_full_area(f));

    if rows.len() < 3 {
        return;
    }

    render_sessions_ftui_header(f, rows[0], data, theme);
    render_sessions_ftui_content(f, rows[1], data, theme);
    render_sessions_ftui_footer(f, rows[2], data, theme);
}

fn render_sessions_ftui_header(
    f: &mut FtuiFrame,
    area: FtuiRect,
    data: &SessionsData,
    theme: &Theme,
) {
    let colors = theme.ftui_colors();
    let total_sessions = data.sessions.len();
    let active_count = data
        .sessions
        .iter()
        .filter(|session| session.is_active)
        .count();
    let total_tokens: u64 = data.sessions.iter().map(|session| session.tokens).sum();
    let total_cost: f64 = data.sessions.iter().map(|session| session.cost).sum();

    let mut spans = vec![
        FtuiSpan::styled(
            "  SESSIONS  ",
            FtuiStyle::new().fg(packed(colors.text)).bold(),
        ),
        FtuiSpan::raw(" "),
        FtuiSpan::styled(
            format!("[Mode: {}]", sessions_group_label(data.group_by)),
            FtuiStyle::new().fg(packed(colors.accent)),
        ),
        FtuiSpan::raw(" "),
        FtuiSpan::styled(
            format!("[{total_sessions} sessions]"),
            FtuiStyle::new().fg(packed(colors.muted)),
        ),
        FtuiSpan::raw(" "),
        FtuiSpan::styled(
            format!("[{active_count} active]"),
            FtuiStyle::new().fg(packed(colors.healthy)),
        ),
        FtuiSpan::raw(" "),
        FtuiSpan::styled(
            format!("[{} / ${total_cost:.2}]", format_token_count(total_tokens)),
            FtuiStyle::new().fg(packed(colors.warning)),
        ),
    ];

    if !data.filter.is_empty() {
        spans.push(FtuiSpan::raw(" "));
        spans.push(FtuiSpan::styled(
            format!("[Filter: {}]", data.filter),
            FtuiStyle::new().fg(packed(colors.info)),
        ));
    }

    let header = FtuiParagraph::new(FtuiText::from_spans(spans))
        .style(FtuiStyle::new().bg(packed(colors.bg_secondary)))
        .block(ftui_block(None, theme));

    FtuiWidget::render(&header, area, f);
}

fn render_sessions_ftui_content(
    f: &mut FtuiFrame,
    area: FtuiRect,
    data: &SessionsData,
    theme: &Theme,
) {
    if data.sessions.is_empty() {
        let empty = FtuiParagraph::new(FtuiText::from_spans([FtuiSpan::styled(
            "No sessions tracked. Run cass collector to populate data.",
            FtuiStyle::new().fg(packed(theme.ftui_colors().muted)),
        )]))
        .block(ftui_block(Some(" Session Inventory "), theme));
        FtuiWidget::render(&empty, area, f);
        return;
    }

    match data.group_by {
        SessionGroupBy::None => render_sessions_ftui_table(f, area, data, theme),
        SessionGroupBy::Project | SessionGroupBy::Model | SessionGroupBy::Agent => {
            render_sessions_ftui_grouped(f, area, data, theme);
        }
    }
}

fn render_sessions_ftui_table(
    f: &mut FtuiFrame,
    area: FtuiRect,
    data: &SessionsData,
    theme: &Theme,
) {
    let colors = theme.ftui_colors();
    let filtered = filtered_sessions(data);
    if filtered.is_empty() {
        let empty = FtuiParagraph::new(FtuiText::from_spans([FtuiSpan::styled(
            "No sessions match the current filter.",
            FtuiStyle::new().fg(packed(colors.muted)),
        )]))
        .block(ftui_block(Some(" Session Inventory "), theme));
        FtuiWidget::render(&empty, area, f);
        return;
    }

    let clamped_selected = data.selected.min(filtered.len().saturating_sub(1));
    let header = FtuiRow::new([
        FtuiText::from_spans([FtuiSpan::styled("", FtuiStyle::new())]),
        FtuiText::from_spans([FtuiSpan::styled("Project", FtuiStyle::new().bold())]),
        FtuiText::from_spans([FtuiSpan::styled("Model", FtuiStyle::new().bold())]),
        FtuiText::from_spans([FtuiSpan::styled("Agent", FtuiStyle::new().bold())]),
        FtuiText::from_spans([FtuiSpan::styled("Time", FtuiStyle::new().bold())]),
        FtuiText::from_spans([FtuiSpan::styled("Tokens", FtuiStyle::new().bold())]),
        FtuiText::from_spans([FtuiSpan::styled("Cost", FtuiStyle::new().bold())]),
        FtuiText::from_spans([FtuiSpan::styled("Last Active", FtuiStyle::new().bold())]),
    ])
    .style(FtuiStyle::new().fg(packed(colors.muted)))
    .bottom_margin(1);

    let rows: Vec<FtuiRow> = filtered
        .iter()
        .enumerate()
        .map(|(idx, session)| {
            let row_style = if idx == clamped_selected {
                FtuiStyle::new().bg(packed(colors.bg_secondary))
            } else {
                FtuiStyle::new()
            };

            FtuiRow::new([
                FtuiText::from_spans([session_activity_indicator(session.is_active, theme)]),
                FtuiText::from_spans([FtuiSpan::styled(
                    project_label(&session.project),
                    FtuiStyle::new().fg(packed(colors.text)),
                )]),
                FtuiText::from_spans([FtuiSpan::styled(
                    &session.model,
                    FtuiStyle::new().fg(packed(theme.provider_color(&session.model))),
                )]),
                FtuiText::from_spans([FtuiSpan::styled(
                    &session.agent,
                    FtuiStyle::new().fg(packed(colors.info)),
                )]),
                FtuiText::from_spans([FtuiSpan::styled(
                    session.duration_str(),
                    FtuiStyle::new().fg(packed(colors.text)),
                )]),
                FtuiText::from_spans([FtuiSpan::styled(
                    session.tokens_str(),
                    FtuiStyle::new().fg(packed(colors.text)),
                )]),
                FtuiText::from_spans([FtuiSpan::styled(
                    session.cost_str(),
                    FtuiStyle::new().fg(packed(colors.warning)),
                )]),
                FtuiText::from_spans([FtuiSpan::styled(
                    &session.last_activity,
                    FtuiStyle::new().fg(packed(colors.muted)),
                )]),
            ])
            .style(row_style)
        })
        .collect();

    let table = FtuiTable::new(
        rows,
        [
            FtuiConstraint::Fixed(2),
            FtuiConstraint::Fixed(16),
            FtuiConstraint::Fixed(14),
            FtuiConstraint::Fixed(14),
            FtuiConstraint::Fixed(6),
            FtuiConstraint::Fixed(8),
            FtuiConstraint::Fixed(8),
            FtuiConstraint::Min(12),
        ],
    )
    .header(header)
    .column_spacing(1)
    .block(ftui_block(Some(" Session Inventory "), theme));

    FtuiWidget::render(&table, area, f);
}

fn render_sessions_ftui_grouped(
    f: &mut FtuiFrame,
    area: FtuiRect,
    data: &SessionsData,
    theme: &Theme,
) {
    let colors = theme.ftui_colors();
    let filtered = filtered_sessions(data);
    if filtered.is_empty() {
        let empty = FtuiParagraph::new(FtuiText::from_spans([FtuiSpan::styled(
            "No sessions match the current filter.",
            FtuiStyle::new().fg(packed(colors.muted)),
        )]))
        .block(ftui_block(Some(" Session Groups "), theme));
        FtuiWidget::render(&empty, area, f);
        return;
    }

    let mut groups: BTreeMap<String, Vec<&SessionInfo>> = BTreeMap::new();
    for session in filtered {
        let key = match data.group_by {
            SessionGroupBy::Project => session.project.clone(),
            SessionGroupBy::Model => session.model.clone(),
            SessionGroupBy::Agent => session.agent.clone(),
            SessionGroupBy::None => unreachable!(),
        };
        groups.entry(key).or_default().push(session);
    }

    let mut items = Vec::new();
    for (group_name, sessions) in groups {
        let is_expanded = data
            .expanded_groups
            .iter()
            .any(|group| group == &group_name);
        let active_count = sessions.iter().filter(|session| session.is_active).count();
        let total_tokens: u64 = sessions.iter().map(|session| session.tokens).sum();
        let total_cost: f64 = sessions.iter().map(|session| session.cost).sum();

        items.push(FtuiListItem::new(FtuiText::from_lines([
            FtuiLine::from_spans([
                FtuiSpan::styled(
                    if is_expanded { "▼" } else { "▶" },
                    FtuiStyle::new().fg(packed(colors.accent)),
                ),
                FtuiSpan::raw(" "),
                FtuiSpan::styled(group_name.clone(), FtuiStyle::new().fg(packed(colors.text))),
                FtuiSpan::raw(" "),
                FtuiSpan::styled(
                    format!("({} sessions", sessions.len()),
                    FtuiStyle::new().fg(packed(colors.muted)),
                ),
                if active_count > 0 {
                    FtuiSpan::styled(
                        format!(", {active_count} active"),
                        FtuiStyle::new().fg(packed(colors.healthy)),
                    )
                } else {
                    FtuiSpan::raw("")
                },
                FtuiSpan::styled(
                    format!(", {} / ${total_cost:.2})", format_token_count(total_tokens)),
                    FtuiStyle::new().fg(packed(colors.muted)),
                ),
            ]),
        ])));

        if is_expanded {
            for session in sessions {
                items.push(FtuiListItem::new(FtuiText::from_lines([
                    FtuiLine::from_spans([
                        FtuiSpan::raw("    "),
                        session_activity_indicator(session.is_active, theme),
                        FtuiSpan::raw(" "),
                        FtuiSpan::styled(&session.agent, FtuiStyle::new().fg(packed(colors.info))),
                        FtuiSpan::raw(" "),
                        FtuiSpan::styled(
                            format!(
                                "{} / {} / {}",
                                session.duration_str(),
                                session.tokens_str(),
                                session.cost_str()
                            ),
                            FtuiStyle::new().fg(packed(colors.muted)),
                        ),
                    ]),
                ])));
            }
        }
    }

    let list = FtuiList::new(items).block(ftui_block(Some(" Session Groups "), theme));
    FtuiWidget::render(&list, area, f);
}

fn render_sessions_ftui_footer(
    f: &mut FtuiFrame,
    area: FtuiRect,
    data: &SessionsData,
    theme: &Theme,
) {
    let colors = theme.ftui_colors();
    let help = match data.group_by {
        SessionGroupBy::None => "↑↓ Navigate  g Group  / Filter  Enter Details  q Back",
        SessionGroupBy::Project | SessionGroupBy::Model | SessionGroupBy::Agent => {
            "↑↓ Navigate  Enter Expand  g Cycle Group  / Filter  q Back"
        }
    };
    let footer = FtuiParagraph::new(FtuiText::from_spans([FtuiSpan::styled(
        help,
        FtuiStyle::new().fg(packed(colors.muted)),
    )]))
    .style(FtuiStyle::new().bg(packed(colors.bg_secondary)))
    .block(ftui_block(None, theme));

    FtuiWidget::render(&footer, area, f);
}

fn filtered_sessions(data: &SessionsData) -> Vec<&SessionInfo> {
    if data.filter.is_empty() {
        return data.sessions.iter().collect();
    }

    let filter = data.filter.to_lowercase();
    data.sessions
        .iter()
        .filter(|session| {
            session.project.to_lowercase().contains(&filter)
                || session.model.to_lowercase().contains(&filter)
                || session.agent.to_lowercase().contains(&filter)
        })
        .collect()
}

fn render_session_row<'a>(session: &'a SessionInfo, is_selected: bool, theme: &Theme) -> Row<'a> {
    let row_style = if is_selected {
        Style::default().bg(theme.bg_secondary)
    } else {
        Style::default()
    };
    let active_marker = if session.is_active { "●" } else { "○" };
    let active_color = if session.is_active {
        theme.healthy
    } else {
        theme.muted
    };

    Row::new(vec![
        Line::from(Span::styled(
            active_marker,
            Style::default().fg(active_color),
        )),
        Line::from(Span::styled(
            project_label(&session.project),
            Style::default().fg(theme.text),
        )),
        Line::from(Span::styled(
            &session.model,
            Style::default().fg(theme.provider_color_ratatui(&session.model)),
        )),
        Line::from(Span::styled(
            &session.agent,
            Style::default().fg(theme.info),
        )),
        Line::from(Span::styled(
            session.duration_str(),
            Style::default().fg(theme.text),
        )),
        Line::from(Span::styled(
            session.tokens_str(),
            Style::default().fg(theme.text),
        )),
        Line::from(Span::styled(
            session.cost_str(),
            Style::default().fg(theme.warning),
        )),
        Line::from(Span::styled(
            &session.last_activity,
            Style::default().fg(theme.muted),
        )),
    ])
    .style(row_style)
}

fn project_label(project: &str) -> &str {
    project.rsplit('/').next().unwrap_or(project)
}

fn sessions_group_label(group_by: SessionGroupBy) -> &'static str {
    match group_by {
        SessionGroupBy::None => "Ungrouped",
        SessionGroupBy::Project => "By Project",
        SessionGroupBy::Model => "By Model",
        SessionGroupBy::Agent => "By Agent",
    }
}

fn format_token_count(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        return format_scaled(tokens, 1_000_000, "M");
    }
    if tokens >= 1_000 {
        return format_scaled(tokens, 1_000, "K");
    }
    tokens.to_string()
}

fn format_scaled(value: u64, unit: u64, suffix: &str) -> String {
    let whole = value / unit;
    let fractional = ((value % unit) * 10 + (unit / 2)) / unit;
    if fractional >= 10 {
        format!("{}.0{suffix}", whole + 1)
    } else {
        format!("{whole}.{fractional}{suffix}")
    }
}

fn session_activity_indicator(is_active: bool, theme: &Theme) -> FtuiSpan<'static> {
    if is_active {
        FtuiSpan::styled(
            "●",
            FtuiStyle::new().fg(packed(theme.ftui_colors().healthy)),
        )
    } else {
        FtuiSpan::styled("○", FtuiStyle::new().fg(packed(theme.ftui_colors().muted)))
    }
}

fn ftui_block<'a>(title: Option<&'a str>, theme: &Theme) -> FtuiBlock<'a> {
    let mut block = FtuiBlock::new()
        .borders(FtuiBorders::ALL)
        .border_style(FtuiStyle::new().fg(packed(theme.ftui_colors().muted)));
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

    fn row_string(buffer: &Buffer, y: u16, width: u16) -> String {
        (0..width)
            .map(|x| {
                buffer
                    .get(x, y)
                    .and_then(|cell| cell.content.as_char())
                    .unwrap_or(' ')
            })
            .collect()
    }

    fn buffer_contains(buffer: &Buffer, width: u16, height: u16, needle: &str) -> bool {
        (0..height).any(|y| row_string(buffer, y, width).contains(needle))
    }

    #[test]
    fn test_sessions_data_default() {
        let data = SessionsData::default();
        assert!(data.sessions.is_empty());
        assert_eq!(data.group_by, SessionGroupBy::None);
    }

    #[test]
    fn test_session_info_default() {
        let session = SessionInfo::default();
        assert!(session.id.is_empty());
        assert!(!session.is_active);
        assert!(session.cost.abs() < f64::EPSILON);
    }

    #[test]
    fn test_duration_str_minutes() {
        let session = SessionInfo {
            duration_mins: 45,
            ..Default::default()
        };
        assert_eq!(session.duration_str(), "45m");
    }

    #[test]
    fn test_duration_str_hours() {
        let session = SessionInfo {
            duration_mins: 125,
            ..Default::default()
        };
        assert_eq!(session.duration_str(), "2h5m");
    }

    #[test]
    fn test_tokens_str_small() {
        let session = SessionInfo {
            tokens: 500,
            ..Default::default()
        };
        assert_eq!(session.tokens_str(), "500");
    }

    #[test]
    fn test_tokens_str_thousands() {
        let session = SessionInfo {
            tokens: 15_000,
            ..Default::default()
        };
        assert_eq!(session.tokens_str(), "15.0K");
    }

    #[test]
    fn test_tokens_str_millions() {
        let session = SessionInfo {
            tokens: 2_500_000,
            ..Default::default()
        };
        assert_eq!(session.tokens_str(), "2.5M");
    }

    #[test]
    fn test_cost_str_small() {
        let session = SessionInfo {
            cost: 0.125,
            ..Default::default()
        };
        assert_eq!(session.cost_str(), "$0.125");
    }

    #[test]
    fn test_cost_str_large() {
        let session = SessionInfo {
            cost: 5.50,
            ..Default::default()
        };
        assert_eq!(session.cost_str(), "$5.50");
    }

    #[test]
    fn test_render_sessions_ftui_renders_table_rows() {
        let data = SessionsData {
            sessions: vec![
                SessionInfo {
                    id: "s1".to_string(),
                    project: "/tmp/vibe_cockpit".to_string(),
                    model: "claude".to_string(),
                    agent: "CobaltTurtle".to_string(),
                    started_at: "2026-03-13T09:00:00Z".to_string(),
                    duration_mins: 45,
                    tokens: 15_000,
                    cost: 1.25,
                    is_active: true,
                    last_activity: "2m ago".to_string(),
                },
                SessionInfo {
                    id: "s2".to_string(),
                    project: "/tmp/other".to_string(),
                    model: "codex".to_string(),
                    agent: "YellowBay".to_string(),
                    started_at: "2026-03-13T08:00:00Z".to_string(),
                    duration_mins: 10,
                    tokens: 500,
                    cost: 0.05,
                    is_active: false,
                    last_activity: "10m ago".to_string(),
                },
            ],
            selected: 0,
            group_by: SessionGroupBy::None,
            filter: String::new(),
            expanded_groups: Vec::new(),
        };
        let theme = Theme::default();
        let mut pool = GraphemePool::new();
        let mut frame = FtuiFrame::new(96, 18, &mut pool);

        render_sessions_ftui(&mut frame, &data, &theme);

        assert!(buffer_contains(&frame.buffer, 96, 18, "SESSIONS"));
        assert!(buffer_contains(&frame.buffer, 96, 18, "vibe_cockpit"));
        assert!(buffer_contains(&frame.buffer, 96, 18, "CobaltTurtle"));
        assert!(buffer_contains(&frame.buffer, 96, 18, "15.0K"));
    }

    #[test]
    fn test_render_sessions_ftui_renders_grouped_view() {
        let data = SessionsData {
            sessions: vec![SessionInfo {
                id: "s1".to_string(),
                project: "/tmp/vibe_cockpit".to_string(),
                model: "claude".to_string(),
                agent: "CobaltTurtle".to_string(),
                started_at: "2026-03-13T09:00:00Z".to_string(),
                duration_mins: 45,
                tokens: 15_000,
                cost: 1.25,
                is_active: true,
                last_activity: "2m ago".to_string(),
            }],
            selected: 0,
            group_by: SessionGroupBy::Project,
            filter: String::new(),
            expanded_groups: vec!["/tmp/vibe_cockpit".to_string()],
        };
        let theme = Theme::default();
        let mut pool = GraphemePool::new();
        let mut frame = FtuiFrame::new(96, 18, &mut pool);

        render_sessions_ftui(&mut frame, &data, &theme);

        assert!(buffer_contains(&frame.buffer, 96, 18, "/tmp/vibe_cockpit"));
        assert!(buffer_contains(&frame.buffer, 96, 18, "CobaltTurtle"));
    }

    #[test]
    fn test_render_sessions_ftui_renders_empty_state() {
        let data = SessionsData::default();
        let theme = Theme::default();
        let mut pool = GraphemePool::new();
        let mut frame = FtuiFrame::new(72, 14, &mut pool);

        render_sessions_ftui(&mut frame, &data, &theme);

        assert!(buffer_contains(
            &frame.buffer,
            72,
            14,
            "No sessions tracked"
        ));
    }

    #[test]
    fn test_group_by_default() {
        assert_eq!(SessionGroupBy::default(), SessionGroupBy::None);
    }
}
