//! Sessions screen implementation
//!
//! Displays active coding sessions from cass (session search) collector.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Row, Table},
};

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
    pub fn duration_str(&self) -> String {
        if self.duration_mins < 60 {
            format!("{}m", self.duration_mins)
        } else {
            let hours = self.duration_mins / 60;
            let mins = self.duration_mins % 60;
            format!("{}h{}m", hours, mins)
        }
    }

    /// Format tokens as human-readable string
    pub fn tokens_str(&self) -> String {
        if self.tokens >= 1_000_000 {
            format!("{:.1}M", self.tokens as f64 / 1_000_000.0)
        } else if self.tokens >= 1_000 {
            format!("{:.1}K", self.tokens as f64 / 1_000.0)
        } else {
            format!("{}", self.tokens)
        }
    }

    /// Format cost as string
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
            format!("[{} sessions]", total_sessions),
            Style::default().fg(theme.muted),
        ),
        Span::raw("  "),
        Span::styled(
            format!("[{} active]", active_count),
            Style::default().fg(theme.healthy),
        ),
        Span::raw("  "),
        Span::styled(
            format!("[{}]", group_label),
            Style::default().fg(theme.info),
        ),
        Span::raw("  "),
        Span::styled(
            format!(
                "[{} tokens / ${:.2}]",
                if total_tokens >= 1_000_000 {
                    format!("{:.1}M", total_tokens as f64 / 1_000_000.0)
                } else {
                    format!("{:.1}K", total_tokens as f64 / 1_000.0)
                },
                total_cost
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
    // Filter sessions
    let filtered: Vec<&SessionInfo> = if data.filter.is_empty() {
        data.sessions.iter().collect()
    } else {
        let filter_lower = data.filter.to_lowercase();
        data.sessions
            .iter()
            .filter(|s| {
                s.project.to_lowercase().contains(&filter_lower)
                    || s.model.to_lowercase().contains(&filter_lower)
                    || s.agent.to_lowercase().contains(&filter_lower)
            })
            .collect()
    };

    let rows: Vec<Row> = filtered
        .iter()
        .enumerate()
        .map(|(i, session)| {
            let is_selected = i == data.selected;
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

            // Truncate project path to show just the last component
            let project_short = session
                .project
                .rsplit('/')
                .next()
                .unwrap_or(&session.project);

            Row::new(vec![
                Line::from(Span::styled(
                    active_marker,
                    Style::default().fg(active_color),
                )),
                Line::from(Span::styled(project_short, Style::default().fg(theme.text))),
                Line::from(Span::styled(
                    &session.model,
                    Style::default().fg(theme.provider_color(&session.model)),
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
        })
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
    let mut groups: std::collections::HashMap<String, Vec<&SessionInfo>> =
        std::collections::HashMap::new();

    for session in &data.sessions {
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

    for (group_name, sessions) in groups.iter() {
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
                    format!(", {} active", active_count),
                    Style::default().fg(theme.healthy),
                )
            } else {
                Span::raw("")
            },
            Span::styled(
                format!(
                    ", {} / ${:.2})",
                    if total_tokens >= 1_000_000 {
                        format!("{:.1}M", total_tokens as f64 / 1_000_000.0)
                    } else {
                        format!("{:.1}K", total_tokens as f64 / 1_000.0)
                    },
                    total_cost
                ),
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(session.cost, 0.0);
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
    fn test_group_by_default() {
        assert_eq!(SessionGroupBy::default(), SessionGroupBy::None);
    }
}
