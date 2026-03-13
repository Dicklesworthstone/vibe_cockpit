//! Agent Mail screen implementation
//!
//! Displays agent communication threads and messages from `mcp_agent_mail` collector.

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
    /// Agent activity heatmap data (`agent_name` -> activity level 0-4)
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
#[derive(Debug, Clone, Default)]
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
            format!("[{total_threads} threads]"),
            Style::default().fg(theme.muted),
        ),
        if total_unacked > 0 {
            Span::styled(
                format!("  [{total_unacked} unacked]"),
                Style::default().fg(theme.warning),
            )
        } else {
            Span::raw("")
        },
        if urgent_count > 0 {
            Span::styled(
                format!("  [{urgent_count} urgent]"),
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
                format!("{trunc}...")
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
                format!("{trunc}...")
            } else {
                msg.from.clone()
            };

            let preview = if msg.body_preview.chars().count() > 30 {
                let trunc: String = msg.body_preview.chars().take(27).collect();
                format!("{trunc}...")
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
                Span::styled(format!("{short_name} "), Style::default().fg(theme.muted)),
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

pub fn render_mail_ftui(f: &mut FtuiFrame, data: &MailData, theme: &Theme) {
    let rows = Flex::vertical()
        .constraints([
            FtuiConstraint::Fixed(3),
            FtuiConstraint::Fill,
            FtuiConstraint::Fixed(4),
            FtuiConstraint::Fixed(3),
        ])
        .split(ftui_full_area(f));

    if rows.len() < 4 {
        return;
    }

    render_mail_ftui_header(f, rows[0], data, theme);
    render_mail_ftui_main_content(f, rows[1], data, theme);
    render_mail_ftui_activity_heatmap(f, rows[2], data, theme);
    render_mail_ftui_footer(f, rows[3], theme);
}

fn render_mail_ftui_header(f: &mut FtuiFrame, area: FtuiRect, data: &MailData, theme: &Theme) {
    let colors = theme.ftui_colors();
    let total_threads = data.threads.len();
    let total_unacked: usize = data.threads.iter().map(|thread| thread.unacked_count).sum();
    let urgent_count = data
        .threads
        .iter()
        .filter(|thread| thread.has_urgent)
        .count();

    let mut spans = vec![
        FtuiSpan::styled(
            "  AGENT MAIL  ",
            FtuiStyle::new().fg(packed(colors.text)).bold(),
        ),
        FtuiSpan::raw(" "),
        FtuiSpan::styled(
            format!("[{total_threads} threads]"),
            FtuiStyle::new().fg(packed(colors.muted)),
        ),
    ];

    if total_unacked > 0 {
        spans.push(FtuiSpan::raw(" "));
        spans.push(FtuiSpan::styled(
            format!("[{total_unacked} unacked]"),
            FtuiStyle::new().fg(packed(colors.warning)),
        ));
    }
    if urgent_count > 0 {
        spans.push(FtuiSpan::raw(" "));
        spans.push(FtuiSpan::styled(
            format!("[{urgent_count} urgent]"),
            FtuiStyle::new().fg(packed(colors.critical)),
        ));
    }
    if !data.filter.is_empty() {
        spans.push(FtuiSpan::raw(" "));
        spans.push(FtuiSpan::styled(
            format!("[Filter: {}]", data.filter),
            FtuiStyle::new().fg(packed(colors.info)),
        ));
    }

    let header = FtuiParagraph::new(FtuiText::from_spans(spans))
        .style(FtuiStyle::new().bg(packed(colors.bg_secondary)))
        .block(ftui_block(None, theme.ftui_colors().muted));

    FtuiWidget::render(&header, area, f);
}

fn render_mail_ftui_main_content(
    f: &mut FtuiFrame,
    area: FtuiRect,
    data: &MailData,
    theme: &Theme,
) {
    let cols = Flex::horizontal()
        .constraints([
            FtuiConstraint::Percentage(40.0),
            FtuiConstraint::Percentage(60.0),
        ])
        .gap(1)
        .split(area);

    if cols.len() < 2 {
        return;
    }

    render_mail_ftui_threads_pane(f, cols[0], data, theme);
    render_mail_ftui_messages_pane(f, cols[1], data, theme);
}

fn render_mail_ftui_threads_pane(
    f: &mut FtuiFrame,
    area: FtuiRect,
    data: &MailData,
    theme: &Theme,
) {
    let colors = theme.ftui_colors();
    let border_color = if data.active_pane == MailPane::Threads {
        colors.accent
    } else {
        colors.muted
    };

    if data.threads.is_empty() {
        let empty = FtuiParagraph::new(FtuiText::from_spans([FtuiSpan::styled(
            "No threads found",
            FtuiStyle::new().fg(packed(colors.muted)),
        )]))
        .block(ftui_block(Some(" Threads "), border_color));
        FtuiWidget::render(&empty, area, f);
        return;
    }

    let filtered: Vec<(usize, &ThreadSummary)> = if data.filter.is_empty() {
        data.threads.iter().enumerate().collect()
    } else {
        let filter = data.filter.to_lowercase();
        data.threads
            .iter()
            .enumerate()
            .filter(|(_, thread)| {
                thread.subject.to_lowercase().contains(&filter)
                    || thread
                        .participants
                        .iter()
                        .any(|participant| participant.to_lowercase().contains(&filter))
            })
            .collect()
    };

    if filtered.is_empty() {
        let empty = FtuiParagraph::new(FtuiText::from_spans([FtuiSpan::styled(
            "No threads match the current filter.",
            FtuiStyle::new().fg(packed(colors.muted)),
        )]))
        .block(ftui_block(Some(" Threads "), border_color));
        FtuiWidget::render(&empty, area, f);
        return;
    }

    let items: Vec<FtuiListItem> = filtered
        .iter()
        .map(|(index, thread)| {
            let row_style =
                if *index == data.selected_thread && data.active_pane == MailPane::Threads {
                    FtuiStyle::new().bg(packed(colors.bg_secondary))
                } else {
                    FtuiStyle::new()
                };
            let unacked_indicator = if thread.unacked_count > 0 {
                FtuiSpan::styled(
                    format!("[{}]", thread.unacked_count),
                    FtuiStyle::new().fg(packed(colors.warning)),
                )
            } else {
                FtuiSpan::styled("   ", FtuiStyle::new())
            };
            let urgent_indicator = if thread.has_urgent {
                FtuiSpan::styled("!", FtuiStyle::new().fg(packed(colors.critical)))
            } else {
                FtuiSpan::styled("·", FtuiStyle::new().fg(packed(colors.muted)))
            };
            let subject = truncate_chars(&thread.subject, 28);
            let meta = format!(
                "{} msg / {} people / {}",
                thread.message_count, thread.participant_count, thread.last_activity
            );

            FtuiListItem::new(FtuiText::from_lines([
                FtuiLine::from_spans([
                    FtuiSpan::raw(" "),
                    urgent_indicator,
                    FtuiSpan::raw(" "),
                    unacked_indicator,
                    FtuiSpan::raw(" "),
                    FtuiSpan::styled(subject, FtuiStyle::new().fg(packed(colors.text))),
                ]),
                FtuiLine::from_spans([
                    FtuiSpan::raw("     "),
                    FtuiSpan::styled(meta, FtuiStyle::new().fg(packed(colors.muted))),
                ]),
            ]))
            .style(row_style)
        })
        .collect();

    let list = FtuiList::new(items).block(ftui_block(Some(" Threads "), border_color));
    FtuiWidget::render(&list, area, f);
}

fn render_mail_ftui_messages_pane(
    f: &mut FtuiFrame,
    area: FtuiRect,
    data: &MailData,
    theme: &Theme,
) {
    let colors = theme.ftui_colors();
    let border_color = if data.active_pane == MailPane::Messages {
        colors.accent
    } else {
        colors.muted
    };

    if data.messages.is_empty() {
        let hint = if data.threads.is_empty() {
            "No threads to display"
        } else {
            "Select a thread to view messages"
        };
        let empty = FtuiParagraph::new(FtuiText::from_spans([FtuiSpan::styled(
            hint,
            FtuiStyle::new().fg(packed(colors.muted)),
        )]))
        .block(ftui_block(Some(" Messages "), border_color));
        FtuiWidget::render(&empty, area, f);
        return;
    }

    let clamped_selected = data
        .selected_message
        .min(data.messages.len().saturating_sub(1));
    let items: Vec<FtuiListItem> = data
        .messages
        .iter()
        .enumerate()
        .map(|(index, message)| {
            let row_style = if index == clamped_selected && data.active_pane == MailPane::Messages {
                FtuiStyle::new().bg(packed(colors.bg_secondary))
            } else {
                FtuiStyle::new()
            };
            let subject = truncate_chars(&message.subject, 26);
            let preview = truncate_chars(&message.body_preview, 40);

            FtuiListItem::new(FtuiText::from_lines([
                FtuiLine::from_spans([
                    FtuiSpan::raw(" "),
                    message_importance_indicator(&message.importance, theme),
                    FtuiSpan::raw(" "),
                    message_ack_indicator(message.ack_required, message.acknowledged, theme),
                    FtuiSpan::raw(" "),
                    FtuiSpan::styled(
                        truncate_chars(&message.from, 12),
                        FtuiStyle::new().fg(packed(colors.info)),
                    ),
                    FtuiSpan::raw(" "),
                    FtuiSpan::styled(
                        &message.timestamp,
                        FtuiStyle::new().fg(packed(colors.muted)),
                    ),
                ]),
                FtuiLine::from_spans([
                    FtuiSpan::raw("     "),
                    FtuiSpan::styled(subject, FtuiStyle::new().fg(packed(colors.text))),
                    FtuiSpan::raw(" "),
                    FtuiSpan::styled(preview, FtuiStyle::new().fg(packed(colors.muted))),
                ]),
            ]))
            .style(row_style)
        })
        .collect();

    let list = FtuiList::new(items).block(ftui_block(Some(" Messages "), border_color));
    FtuiWidget::render(&list, area, f);
}

fn render_mail_ftui_activity_heatmap(
    f: &mut FtuiFrame,
    area: FtuiRect,
    data: &MailData,
    theme: &Theme,
) {
    let colors = theme.ftui_colors();
    let heat_chars = ['░', '▒', '▓', '█', '█'];
    let spans: Vec<FtuiSpan> = if data.agent_activity.is_empty() {
        vec![
            FtuiSpan::styled("Activity: ", FtuiStyle::new().fg(packed(colors.accent))),
            FtuiSpan::styled(
                "No agent activity data",
                FtuiStyle::new().fg(packed(colors.muted)),
            ),
        ]
    } else {
        let mut spans = vec![FtuiSpan::styled(
            "Activity: ",
            FtuiStyle::new().fg(packed(colors.accent)),
        )];
        for (name, level) in data.agent_activity.iter().take(20) {
            let clamped = usize::from((*level).min(4));
            spans.push(FtuiSpan::styled(
                heat_chars[clamped].to_string(),
                FtuiStyle::new().fg(packed(activity_level_color(clamped, theme))),
            ));
            spans.push(FtuiSpan::raw(" "));
            spans.push(FtuiSpan::styled(
                truncate_chars(name, 8),
                FtuiStyle::new().fg(packed(colors.muted)),
            ));
            spans.push(FtuiSpan::raw(" "));
        }
        spans
    };

    let heatmap = FtuiParagraph::new(FtuiText::from_lines([FtuiLine::from_spans(spans)]))
        .style(FtuiStyle::new().bg(packed(colors.bg_secondary)))
        .block(ftui_block(Some(" Agent Activity "), colors.muted));

    FtuiWidget::render(&heatmap, area, f);
}

fn render_mail_ftui_footer(f: &mut FtuiFrame, area: FtuiRect, theme: &Theme) {
    let colors = theme.ftui_colors();
    let footer = FtuiParagraph::new(FtuiText::from_spans([FtuiSpan::styled(
        "Tab Switch Pane  ↑↓ Navigate  Enter Select  a Acknowledge  / Filter  q Back",
        FtuiStyle::new().fg(packed(colors.muted)),
    )]))
    .style(FtuiStyle::new().bg(packed(colors.bg_secondary)))
    .block(ftui_block(None, colors.muted));

    FtuiWidget::render(&footer, area, f);
}

fn message_importance_indicator(importance: &str, theme: &Theme) -> FtuiSpan<'static> {
    match importance.to_ascii_lowercase().as_str() {
        "urgent" | "high" => FtuiSpan::styled(
            "!",
            FtuiStyle::new().fg(packed(theme.ftui_colors().critical)),
        ),
        _ => FtuiSpan::styled("·", FtuiStyle::new().fg(packed(theme.ftui_colors().muted))),
    }
}

fn message_ack_indicator(
    ack_required: bool,
    acknowledged: bool,
    theme: &Theme,
) -> FtuiSpan<'static> {
    if !ack_required {
        return FtuiSpan::styled(" ", FtuiStyle::new());
    }

    if acknowledged {
        FtuiSpan::styled(
            "✓",
            FtuiStyle::new().fg(packed(theme.ftui_colors().healthy)),
        )
    } else {
        FtuiSpan::styled(
            "○",
            FtuiStyle::new().fg(packed(theme.ftui_colors().warning)),
        )
    }
}

fn activity_level_color(level: usize, theme: &Theme) -> ftui::Color {
    match level {
        1 => theme.ftui_colors().info,
        2 => theme.ftui_colors().healthy,
        3 => theme.ftui_colors().warning,
        4 => theme.ftui_colors().critical,
        _ => theme.ftui_colors().muted,
    }
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    let mut chars = input.chars();
    let mut truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        truncated.push_str("...");
    }
    truncated
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

    #[test]
    fn test_render_mail_ftui_renders_thread_and_message_panes() {
        let data = MailData {
            threads: vec![ThreadSummary {
                id: "bd-1l8".to_string(),
                subject: "Accounts, Sessions, and Mail port".to_string(),
                participant_count: 2,
                participants: vec!["CobaltTurtle".to_string(), "YellowBay".to_string()],
                message_count: 3,
                unacked_count: 1,
                last_activity: "2m ago".to_string(),
                has_urgent: true,
            }],
            selected_thread: 0,
            messages: vec![MessageInfo {
                id: 1,
                from: "YellowBay".to_string(),
                to: vec!["CobaltTurtle".to_string()],
                subject: "Re: ftui port".to_string(),
                body_preview: "I am watching the migration progress.".to_string(),
                timestamp: "10:03".to_string(),
                importance: "high".to_string(),
                ack_required: true,
                acknowledged: false,
            }],
            selected_message: 0,
            active_pane: MailPane::Messages,
            agent_activity: vec![("CobaltTurtle".to_string(), 4)],
            filter: String::new(),
        };
        let theme = Theme::default();
        let mut pool = GraphemePool::new();
        let mut frame = FtuiFrame::new(100, 20, &mut pool);

        render_mail_ftui(&mut frame, &data, &theme);

        assert!(buffer_contains(&frame.buffer, 100, 20, "AGENT MAIL"));
        assert!(buffer_contains(
            &frame.buffer,
            100,
            20,
            "Accounts, Sessions"
        ));
        assert!(buffer_contains(&frame.buffer, 100, 20, "YellowBay"));
        assert!(buffer_contains(&frame.buffer, 100, 20, "CobaltTu"));
    }

    #[test]
    fn test_render_mail_ftui_renders_empty_state() {
        let data = MailData::default();
        let theme = Theme::default();
        let mut pool = GraphemePool::new();
        let mut frame = FtuiFrame::new(80, 18, &mut pool);

        render_mail_ftui(&mut frame, &data, &theme);

        assert!(buffer_contains(&frame.buffer, 80, 18, "No threads found"));
        assert!(buffer_contains(
            &frame.buffer,
            80,
            18,
            "No threads to display"
        ));
        assert!(buffer_contains(
            &frame.buffer,
            80,
            18,
            "No agent activity data"
        ));
    }
}
