//! Agent Mail screen implementation
//!
//! Displays agent communication threads and messages from mcp_agent_mail collector.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use crate::theme::Theme;

/// Data needed to render the mail screen
#[derive(Debug, Clone, Default)]
pub struct MailData {
    /// List of threads
    pub threads: Vec<ThreadSummary>,
    /// Currently selected thread index
    pub selected_thread: usize,
    /// Messages in the selected thread
    pub messages: Vec<MessageInfo>,
    /// Currently selected message index
    pub selected_message: usize,
    /// Active pane (Threads or Messages)
    pub active_pane: MailPane,
    /// Agent activity heatmap data (agent_name -> activity level 0-4)
    pub agent_activity: Vec<(String, u8)>,
    /// Filter string
    pub filter: String,
}

/// Which pane is currently active
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum MailPane {
    #[default]
    Threads,
    Messages,
}

/// Thread summary for display
#[derive(Debug, Clone)]
pub struct ThreadSummary {
    /// Thread ID
    pub id: String,
    /// Thread subject
    pub subject: String,
    /// Number of participants
    pub participant_count: usize,
    /// Participant names
    pub participants: Vec<String>,
    /// Total message count
    pub message_count: usize,
    /// Unacknowledged message count
    pub unacked_count: usize,
    /// Most recent activity timestamp
    pub last_activity: String,
    /// Has urgent/high importance messages
    pub has_urgent: bool,
}

impl Default for ThreadSummary {
    fn default() -> Self {
        Self {
            id: String::new(),
            subject: String::new(),
            participant_count: 0,
            participants: vec![],
            message_count: 0,
            unacked_count: 0,
            last_activity: String::new(),
            has_urgent: false,
        }
    }
}

/// Individual message information
#[derive(Debug, Clone)]
pub struct MessageInfo {
    /// Message ID
    pub id: u64,
    /// Sender agent name
    pub from: String,
    /// Recipients
    pub to: Vec<String>,
    /// Subject
    pub subject: String,
    /// Message body preview
    pub body_preview: String,
    /// Timestamp
    pub timestamp: String,
    /// Importance level
    pub importance: String,
    /// Is acknowledgement required?
    pub ack_required: bool,
    /// Has been acknowledged?
    pub acknowledged: bool,
}

impl Default for MessageInfo {
    fn default() -> Self {
        Self {
            id: 0,
            from: String::new(),
            to: vec![],
            subject: String::new(),
            body_preview: String::new(),
            timestamp: String::new(),
            importance: "normal".to_string(),
            ack_required: false,
            acknowledged: false,
        }
    }
}

/// Render the mail screen
pub fn render_mail(f: &mut Frame, data: &MailData, theme: &Theme) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Main content (threads + messages)
            Constraint::Length(4), // Activity heatmap
            Constraint::Length(3), // Footer
        ])
        .split(f.area());

    render_header(f, chunks[0], data, theme);
    render_main_content(f, chunks[1], data, theme);
    render_activity_heatmap(f, chunks[2], data, theme);
    render_footer(f, chunks[3], theme);
}

fn render_header(f: &mut Frame, area: Rect, data: &MailData, theme: &Theme) {
    let total_threads = data.threads.len();
    let total_unacked: usize = data.threads.iter().map(|t| t.unacked_count).sum();
    let urgent_count = data.threads.iter().filter(|t| t.has_urgent).count();

    let title = Line::from(vec![
        Span::styled(
            "  A G E N T   M A I L  ",
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("[{} threads]", total_threads),
            Style::default().fg(theme.muted),
        ),
        if total_unacked > 0 {
            Span::styled(
                format!("  [{} unacked]", total_unacked),
                Style::default().fg(theme.warning),
            )
        } else {
            Span::raw("")
        },
        if urgent_count > 0 {
            Span::styled(
                format!("  [{} urgent]", urgent_count),
                Style::default().fg(theme.critical),
            )
        } else {
            Span::raw("")
        },
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

fn render_main_content(f: &mut Frame, area: Rect, data: &MailData, theme: &Theme) {
    // Split into two panes: threads (left) and messages (right)
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    render_threads_pane(f, panes[0], data, theme);
    render_messages_pane(f, panes[1], data, theme);
}

fn render_threads_pane(f: &mut Frame, area: Rect, data: &MailData, theme: &Theme) {
    let is_active = data.active_pane == MailPane::Threads;
    let border_color = if is_active { theme.accent } else { theme.muted };

    if data.threads.is_empty() {
        let empty = Paragraph::new(Span::styled(
            "  No threads found",
            Style::default().fg(theme.muted),
        ))
        .block(
            Block::default()
                .title(Span::styled(
                    " THREADS ",
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color)),
        );
        f.render_widget(empty, area);
        return;
    }

    // Filter threads
    let filtered: Vec<(usize, &ThreadSummary)> = if data.filter.is_empty() {
        data.threads.iter().enumerate().collect()
    } else {
        let filter_lower = data.filter.to_lowercase();
        data.threads
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                t.subject.to_lowercase().contains(&filter_lower)
                    || t.participants
                        .iter()
                        .any(|p| p.to_lowercase().contains(&filter_lower))
            })
            .collect()
    };

    let items: Vec<ListItem> = filtered
        .iter()
        .map(|(i, thread)| {
            let is_selected = *i == data.selected_thread && is_active;
            let row_style = if is_selected {
                Style::default().bg(theme.bg_secondary)
            } else {
                Style::default()
            };

            let unacked_indicator = if thread.unacked_count > 0 {
                Span::styled(
                    format!("[{}] ", thread.unacked_count),
                    Style::default().fg(theme.warning),
                )
            } else {
                Span::raw("    ")
            };

            let urgent_indicator = if thread.has_urgent {
                Span::styled("!", Style::default().fg(theme.critical))
            } else {
                Span::raw(" ")
            };

            // Truncate subject
            let subject_display = if thread.subject.chars().count() > 25 {
                let trunc: String = thread.subject.chars().take(22).collect();
                format!("{}...", trunc)
            } else {
                thread.subject.clone()
            };

            ListItem::new(Line::from(vec![
                Span::raw("  "),
                urgent_indicator,
                unacked_indicator,
                Span::styled(subject_display, Style::default().fg(theme.text)),
            ]))
            .style(row_style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(Span::styled(
                " THREADS ",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color)),
    );

    f.render_widget(list, area);
}

fn render_messages_pane(f: &mut Frame, area: Rect, data: &MailData, theme: &Theme) {
    let is_active = data.active_pane == MailPane::Messages;
    let border_color = if is_active { theme.accent } else { theme.muted };

    if data.messages.is_empty() {
        let hint = if data.threads.is_empty() {
            "No threads to display"
        } else {
            "Select a thread to view messages"
        };
        let empty = Paragraph::new(Span::styled(hint, Style::default().fg(theme.muted))).block(
            Block::default()
                .title(Span::styled(
                    " MESSAGES ",
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color)),
        );
        f.render_widget(empty, area);
        return;
    }

    let items: Vec<ListItem> = data
        .messages
        .iter()
        .enumerate()
        .map(|(i, msg)| {
            let is_selected = i == data.selected_message && is_active;
            let row_style = if is_selected {
                Style::default().bg(theme.bg_secondary)
            } else {
                Style::default()
            };

            let importance_indicator = match msg.importance.as_str() {
                "urgent" | "high" => Span::styled("!", Style::default().fg(theme.critical)),
                "normal" => Span::raw(" "),
                _ => Span::styled("·", Style::default().fg(theme.muted)),
            };

            let ack_indicator = if msg.ack_required {
                if msg.acknowledged {
                    Span::styled("✓", Style::default().fg(theme.healthy))
                } else {
                    Span::styled("○", Style::default().fg(theme.warning))
                }
            } else {
                Span::raw(" ")
            };

            // Build message line
            let from_display = if msg.from.chars().count() > 12 {
                let trunc: String = msg.from.chars().take(9).collect();
                format!("{}...", trunc)
            } else {
                msg.from.clone()
            };

            let preview = if msg.body_preview.chars().count() > 30 {
                let trunc: String = msg.body_preview.chars().take(27).collect();
                format!("{}...", trunc)
            } else {
                msg.body_preview.clone()
            };

            ListItem::new(vec![
                Line::from(vec![
                    Span::raw("  "),
                    importance_indicator,
                    ack_indicator,
                    Span::raw(" "),
                    Span::styled(from_display, Style::default().fg(theme.info)),
                    Span::raw("  "),
                    Span::styled(msg.timestamp.clone(), Style::default().fg(theme.muted)),
                ]),
                Line::from(vec![
                    Span::raw("     "),
                    Span::styled(preview, Style::default().fg(theme.text)),
                ]),
            ])
            .style(row_style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(Span::styled(
                " MESSAGES ",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color)),
    );

    f.render_widget(list, area);
}

fn render_activity_heatmap(f: &mut Frame, area: Rect, data: &MailData, theme: &Theme) {
    let heatmap_chars = ['░', '▒', '▓', '█', '█'];

    let items: Vec<Span> = data
        .agent_activity
        .iter()
        .take(20) // Limit to fit in one line
        .flat_map(|(name, level)| {
            let level = (*level as usize).min(4);
            let color = match level {
                0 => theme.muted,
                1 => theme.info,
                2 => theme.healthy,
                3 => theme.warning,
                4 => theme.critical,
                _ => theme.muted,
            };
            let short_name = if name.chars().count() > 8 {
                name.chars().take(8).collect::<String>()
            } else {
                name.clone()
            };
            vec![
                Span::styled(heatmap_chars[level].to_string(), Style::default().fg(color)),
                Span::styled(
                    format!("{} ", short_name),
                    Style::default().fg(theme.muted),
                ),
            ]
        })
        .collect();

    let label = vec![Span::styled(
        "  Activity: ",
        Style::default().fg(theme.accent),
    )];

    let content = if items.is_empty() {
        Line::from(vec![
            Span::styled("  Activity: ", Style::default().fg(theme.accent)),
            Span::styled("No agent activity data", Style::default().fg(theme.muted)),
        ])
    } else {
        Line::from([label, items].concat())
    };

    let heatmap = Paragraph::new(content)
        .block(
            Block::default()
                .title(Span::styled(
                    " AGENT ACTIVITY ",
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.muted)),
        )
        .style(Style::default().bg(theme.bg_secondary));

    f.render_widget(heatmap, area);
}

fn render_footer(f: &mut Frame, area: Rect, theme: &Theme) {
    let shortcuts = vec![
        ("[Tab]", "Switch pane"),
        ("[j/k]", "Navigate"),
        ("[Enter]", "Select"),
        ("[a]", "Acknowledge"),
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
    fn test_mail_data_default() {
        let data = MailData::default();
        assert!(data.threads.is_empty());
        assert!(data.messages.is_empty());
        assert_eq!(data.active_pane, MailPane::Threads);
    }

    #[test]
    fn test_thread_summary_default() {
        let thread = ThreadSummary::default();
        assert!(thread.id.is_empty());
        assert_eq!(thread.unacked_count, 0);
        assert!(!thread.has_urgent);
    }

    #[test]
    fn test_message_info_default() {
        let msg = MessageInfo::default();
        assert_eq!(msg.id, 0);
        assert_eq!(msg.importance, "normal");
        assert!(!msg.ack_required);
    }

    #[test]
    fn test_mail_pane_default() {
        assert_eq!(MailPane::default(), MailPane::Threads);
    }

    #[test]
    fn test_mail_data_with_threads() {
        let data = MailData {
            threads: vec![
                ThreadSummary {
                    id: "t1".to_string(),
                    subject: "bd-30z discussion".to_string(),
                    participant_count: 3,
                    participants: vec!["AgentA".to_string(), "AgentB".to_string()],
                    message_count: 5,
                    unacked_count: 2,
                    last_activity: "2 min ago".to_string(),
                    has_urgent: true,
                },
                ThreadSummary {
                    id: "t2".to_string(),
                    subject: "Build status".to_string(),
                    participant_count: 2,
                    participants: vec!["AgentC".to_string()],
                    message_count: 3,
                    unacked_count: 0,
                    last_activity: "1 hour ago".to_string(),
                    has_urgent: false,
                },
            ],
            ..Default::default()
        };

        assert_eq!(data.threads.len(), 2);
        assert!(data.threads[0].has_urgent);
        assert_eq!(data.threads[0].unacked_count, 2);
    }

    #[test]
    fn test_mail_data_with_messages() {
        let data = MailData {
            messages: vec![
                MessageInfo {
                    id: 1,
                    from: "BlueLake".to_string(),
                    to: vec!["GreenCastle".to_string()],
                    subject: "Re: Build plan".to_string(),
                    body_preview: "I've reviewed the approach and it looks good...".to_string(),
                    timestamp: "14:32".to_string(),
                    importance: "normal".to_string(),
                    ack_required: true,
                    acknowledged: false,
                },
                MessageInfo {
                    id: 2,
                    from: "GreenCastle".to_string(),
                    to: vec!["BlueLake".to_string()],
                    subject: "Re: Build plan".to_string(),
                    body_preview: "Thanks, starting implementation now".to_string(),
                    timestamp: "14:35".to_string(),
                    importance: "high".to_string(),
                    ack_required: false,
                    acknowledged: false,
                },
            ],
            ..Default::default()
        };

        assert_eq!(data.messages.len(), 2);
        assert!(data.messages[0].ack_required);
        assert_eq!(data.messages[1].importance, "high");
    }

    #[test]
    fn test_agent_activity() {
        let data = MailData {
            agent_activity: vec![
                ("AgentA".to_string(), 4),
                ("AgentB".to_string(), 2),
                ("AgentC".to_string(), 0),
            ],
            ..Default::default()
        };

        assert_eq!(data.agent_activity.len(), 3);
        assert_eq!(data.agent_activity[0].1, 4);
    }
}
