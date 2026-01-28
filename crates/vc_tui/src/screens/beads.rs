//! Beads TUI screen implementation
//!
//! Shows bv triage output, blockers, and recommended next picks.
//! Data is sourced from beads_triage_snapshots, beads_issues, and beads_graph_metrics tables.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph},
};

use crate::theme::Theme;

/// Data needed to render the beads screen
#[derive(Debug, Clone, Default)]
pub struct BeadsData {
    /// Quick reference summary
    pub quick_ref: QuickRefData,
    /// Recommended tasks to work on
    pub recommendations: Vec<RecommendationItem>,
    /// High-impact blockers to clear
    pub blockers: Vec<BlockerItem>,
    /// Graph health metrics
    pub graph_health: GraphHealthData,
    /// Currently selected section (0=quick_ref, 1=recommendations, 2=blockers, 3=graph)
    pub selected_section: usize,
    /// Selected item index within recommendations list
    pub selected_recommendation: usize,
    /// Selected item index within blockers list
    pub selected_blocker: usize,
    /// Seconds since last data refresh
    pub refresh_age_secs: u64,
}

/// Quick reference counts
#[derive(Debug, Clone, Default)]
pub struct QuickRefData {
    /// Total open issues
    pub open_count: u32,
    /// Ready to work on (no blockers)
    pub actionable_count: u32,
    /// Blocked by other issues
    pub blocked_count: u32,
    /// Currently in progress
    pub in_progress_count: u32,
    /// Number of epics with ready work
    pub epics_with_ready: u32,
    /// Total epics
    pub total_epics: u32,
    /// Counts by priority (P0, P1, P2, P3)
    pub by_priority: [u32; 4],
}

/// A recommendation item from bv triage
#[derive(Debug, Clone)]
pub struct RecommendationItem {
    /// Issue ID (e.g., "bd-30z")
    pub id: String,
    /// Issue title
    pub title: String,
    /// Priority (0-3)
    pub priority: u32,
    /// Triage score
    pub score: f64,
    /// Number of issues this unblocks
    pub unblocks_count: u32,
    /// Status (open, in_progress)
    pub status: String,
    /// Top reason for recommendation
    pub reason: String,
}

impl Default for RecommendationItem {
    fn default() -> Self {
        Self {
            id: String::new(),
            title: String::new(),
            priority: 2,
            score: 0.0,
            unblocks_count: 0,
            status: "open".to_string(),
            reason: String::new(),
        }
    }
}

/// A blocker item to clear
#[derive(Debug, Clone)]
pub struct BlockerItem {
    /// Issue ID
    pub id: String,
    /// Issue title
    pub title: String,
    /// Number of downstream issues blocked
    pub unblocks_count: u32,
    /// Whether this blocker is actionable
    pub is_actionable: bool,
    /// What's blocking this blocker (if not actionable)
    pub blocked_by: Vec<String>,
}

impl Default for BlockerItem {
    fn default() -> Self {
        Self {
            id: String::new(),
            title: String::new(),
            unblocks_count: 0,
            is_actionable: false,
            blocked_by: vec![],
        }
    }
}

/// Graph health metrics
#[derive(Debug, Clone, Default)]
pub struct GraphHealthData {
    /// Total nodes in dependency graph
    pub node_count: u32,
    /// Total edges in dependency graph
    pub edge_count: u32,
    /// Graph density (edges / max_possible_edges)
    pub density: f64,
    /// Whether graph has cycles
    pub has_cycles: bool,
    /// Velocity: closed last 7 days
    pub closed_last_7d: u32,
    /// Velocity: closed last 30 days
    pub closed_last_30d: u32,
    /// Average days to close
    pub avg_days_to_close: f64,
}

/// Render the beads screen
pub fn render_beads(f: &mut Frame, data: &BeadsData, theme: &Theme) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Length(7), // Quick reference
            Constraint::Min(10),   // Recommendations + Blockers (split horizontal)
            Constraint::Length(5), // Graph health
        ])
        .split(f.area());

    render_header(f, chunks[0], data, theme);
    render_quick_ref(
        f,
        chunks[1],
        &data.quick_ref,
        data.selected_section == 0,
        theme,
    );

    // Split middle section into recommendations and blockers
    let middle_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(chunks[2]);

    render_recommendations(
        f,
        middle_chunks[0],
        &data.recommendations,
        data.selected_section == 1,
        data.selected_recommendation,
        theme,
    );
    render_blockers(
        f,
        middle_chunks[1],
        &data.blockers,
        data.selected_section == 2,
        data.selected_blocker,
        theme,
    );

    render_graph_health(
        f,
        chunks[3],
        &data.graph_health,
        data.selected_section == 3,
        theme,
    );
}

fn render_header(f: &mut Frame, area: Rect, data: &BeadsData, theme: &Theme) {
    let refresh_text = if data.refresh_age_secs < 60 {
        format!("{}s ago", data.refresh_age_secs)
    } else {
        format!("{}m ago", data.refresh_age_secs / 60)
    };

    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            "BEADS TRIAGE",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("[{}]", refresh_text),
            Style::default().fg(theme.muted),
        ),
        Span::raw("  "),
        Span::styled(
            "[Tab] switch section  [j/k] navigate  [Enter] details  [r] refresh",
            Style::default().fg(theme.muted),
        ),
    ]))
    .block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(theme.muted)),
    );

    f.render_widget(header, area);
}

fn render_quick_ref(f: &mut Frame, area: Rect, data: &QuickRefData, selected: bool, theme: &Theme) {
    let border_color = if selected { theme.accent } else { theme.muted };

    let block = Block::default()
        .title(Span::styled(
            " Quick Reference ",
            Style::default().fg(theme.text),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Split into two rows
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Length(2)])
        .split(inner);

    // First row: counts
    let counts = Paragraph::new(Line::from(vec![
        Span::styled(
            format!(" Ready: {} ", data.actionable_count),
            Style::default().fg(theme.healthy),
        ),
        Span::raw("│"),
        Span::styled(
            format!(" Blocked: {} ", data.blocked_count),
            Style::default().fg(theme.warning),
        ),
        Span::raw("│"),
        Span::styled(
            format!(" In Progress: {} ", data.in_progress_count),
            Style::default().fg(theme.info),
        ),
        Span::raw("│"),
        Span::styled(
            format!(" Open: {} ", data.open_count),
            Style::default().fg(theme.text),
        ),
    ]));
    f.render_widget(counts, rows[0]);

    // Second row: priority breakdown and epics
    let priority_text = format!(
        " P0:{} P1:{} P2:{} P3:{} │ Epics: {}/{} with ready work",
        data.by_priority[0],
        data.by_priority[1],
        data.by_priority[2],
        data.by_priority[3],
        data.epics_with_ready,
        data.total_epics
    );
    let priority = Paragraph::new(Line::from(vec![
        Span::styled(
            format!(" P0:{}", data.by_priority[0]),
            Style::default().fg(theme.critical),
        ),
        Span::raw(" "),
        Span::styled(
            format!("P1:{}", data.by_priority[1]),
            Style::default().fg(theme.warning),
        ),
        Span::raw(" "),
        Span::styled(
            format!("P2:{}", data.by_priority[2]),
            Style::default().fg(theme.info),
        ),
        Span::raw(" "),
        Span::styled(
            format!("P3:{}", data.by_priority[3]),
            Style::default().fg(theme.muted),
        ),
        Span::raw(" │ "),
        Span::styled(
            format!(
                "Epics: {}/{} with ready work",
                data.epics_with_ready, data.total_epics
            ),
            Style::default().fg(theme.text),
        ),
    ]));
    f.render_widget(priority, rows[1]);
}

fn render_recommendations(
    f: &mut Frame,
    area: Rect,
    items: &[RecommendationItem],
    selected: bool,
    selected_idx: usize,
    theme: &Theme,
) {
    let border_color = if selected { theme.accent } else { theme.muted };

    let block = Block::default()
        .title(Span::styled(
            " Recommended Next ",
            Style::default().fg(theme.text),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let list_items: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let priority_style = priority_style(item.priority, theme);
            let status_indicator = if item.status == "in_progress" {
                "◐"
            } else {
                "○"
            };

            let content = Line::from(vec![
                Span::styled(status_indicator, priority_style),
                Span::raw(" "),
                Span::styled(format!("[P{}]", item.priority), priority_style),
                Span::raw(" "),
                Span::styled(&item.id, Style::default().fg(theme.accent)),
                Span::raw(": "),
                Span::styled(truncate(&item.title, 40), Style::default().fg(theme.text)),
            ]);

            let style = if selected && i == selected_idx {
                Style::default()
                    .bg(theme.bg_secondary)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            ListItem::new(content).style(style)
        })
        .collect();

    let list = List::new(list_items).block(block);
    f.render_widget(list, area);
}

fn render_blockers(
    f: &mut Frame,
    area: Rect,
    items: &[BlockerItem],
    selected: bool,
    selected_idx: usize,
    theme: &Theme,
) {
    let border_color = if selected { theme.accent } else { theme.muted };

    let block = Block::default()
        .title(Span::styled(
            " Blockers to Clear ",
            Style::default().fg(theme.text),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let list_items: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let actionable_indicator = if item.is_actionable { "✓" } else { "⏳" };
            let actionable_color = if item.is_actionable {
                theme.healthy
            } else {
                theme.warning
            };

            let content = Line::from(vec![
                Span::styled(actionable_indicator, Style::default().fg(actionable_color)),
                Span::raw(" "),
                Span::styled(&item.id, Style::default().fg(theme.accent)),
                Span::raw(" "),
                Span::styled(
                    format!("(unblocks {})", item.unblocks_count),
                    Style::default().fg(theme.info),
                ),
            ]);

            let style = if selected && i == selected_idx {
                Style::default()
                    .bg(theme.bg_secondary)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            ListItem::new(content).style(style)
        })
        .collect();

    let list = List::new(list_items).block(block);
    f.render_widget(list, area);
}

fn render_graph_health(
    f: &mut Frame,
    area: Rect,
    data: &GraphHealthData,
    selected: bool,
    theme: &Theme,
) {
    let border_color = if selected { theme.accent } else { theme.muted };

    let block = Block::default()
        .title(Span::styled(
            " Graph Health ",
            Style::default().fg(theme.text),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let cycle_indicator = if data.has_cycles {
        "⚠ Cycles detected"
    } else {
        "✓ No cycles"
    };
    let cycle_color = if data.has_cycles {
        theme.critical
    } else {
        theme.healthy
    };

    let content = Paragraph::new(Line::from(vec![
        Span::styled(
            format!("Nodes: {} ", data.node_count),
            Style::default().fg(theme.text),
        ),
        Span::raw("│ "),
        Span::styled(
            format!("Edges: {} ", data.edge_count),
            Style::default().fg(theme.text),
        ),
        Span::raw("│ "),
        Span::styled(
            format!("Density: {:.1}% ", data.density * 100.0),
            Style::default().fg(theme.text),
        ),
        Span::raw("│ "),
        Span::styled(cycle_indicator, Style::default().fg(cycle_color)),
        Span::raw(" │ "),
        Span::styled(
            format!(
                "Velocity: {} (7d) {} (30d)",
                data.closed_last_7d, data.closed_last_30d
            ),
            Style::default().fg(theme.info),
        ),
    ]));

    f.render_widget(content, inner);
}

/// Get style for a priority level
fn priority_style(priority: u32, theme: &Theme) -> Style {
    match priority {
        0 => Style::default().fg(theme.critical),
        1 => Style::default().fg(theme.warning),
        2 => Style::default().fg(theme.info),
        _ => Style::default().fg(theme.muted),
    }
}

/// Truncate a string to a maximum length
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len - 1])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_beads_data_default() {
        let data = BeadsData::default();
        assert_eq!(data.selected_section, 0);
        assert_eq!(data.selected_recommendation, 0);
        assert!(data.recommendations.is_empty());
    }

    #[test]
    fn test_quick_ref_default() {
        let quick_ref = QuickRefData::default();
        assert_eq!(quick_ref.open_count, 0);
        assert_eq!(quick_ref.actionable_count, 0);
        assert_eq!(quick_ref.by_priority, [0, 0, 0, 0]);
    }

    #[test]
    fn test_recommendation_default() {
        let rec = RecommendationItem::default();
        assert_eq!(rec.priority, 2);
        assert_eq!(rec.score, 0.0);
        assert_eq!(rec.status, "open");
    }

    #[test]
    fn test_blocker_default() {
        let blocker = BlockerItem::default();
        assert_eq!(blocker.unblocks_count, 0);
        assert!(!blocker.is_actionable);
        assert!(blocker.blocked_by.is_empty());
    }

    #[test]
    fn test_graph_health_default() {
        let health = GraphHealthData::default();
        assert_eq!(health.node_count, 0);
        assert!(!health.has_cycles);
        assert_eq!(health.density, 0.0);
    }

    #[test]
    fn test_truncate_short() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_long() {
        let result = truncate("hello world this is a long string", 10);
        assert!(result.len() <= 10);
        assert!(result.ends_with('…'));
    }

    #[test]
    fn test_priority_style_p0() {
        let theme = Theme::default();
        let style = priority_style(0, &theme);
        // P0 should use critical color
        assert_eq!(style.fg, Some(theme.critical));
    }

    #[test]
    fn test_priority_style_p1() {
        let theme = Theme::default();
        let style = priority_style(1, &theme);
        assert_eq!(style.fg, Some(theme.warning));
    }

    #[test]
    fn test_priority_style_p2() {
        let theme = Theme::default();
        let style = priority_style(2, &theme);
        assert_eq!(style.fg, Some(theme.info));
    }

    #[test]
    fn test_priority_style_p3() {
        let theme = Theme::default();
        let style = priority_style(3, &theme);
        assert_eq!(style.fg, Some(theme.muted));
    }
}
