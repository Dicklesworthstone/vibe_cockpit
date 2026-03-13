//! Beads TUI screen implementation
//!
//! Shows bv triage output, blockers, and recommended next picks.
//! Data is sourced from `beads_triage_snapshots`, `beads_issues`, and `beads_graph_metrics` tables.

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
    /// Currently selected section (`0=quick_ref`, 1=recommendations, 2=blockers, 3=graph)
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
    /// Status (open, `in_progress`)
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
#[derive(Debug, Clone, Default)]
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

/// Graph health metrics
#[derive(Debug, Clone, Default)]
pub struct GraphHealthData {
    /// Total nodes in dependency graph
    pub node_count: u32,
    /// Total edges in dependency graph
    pub edge_count: u32,
    /// Graph density (edges / `max_possible_edges`)
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
/// Truncate a string to a maximum number of characters (not bytes)
fn truncate(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{truncated}…")
    }
}

pub fn render_beads_ftui(f: &mut FtuiFrame, data: &BeadsData, theme: &Theme) {
    let rows = Flex::vertical()
        .constraints([
            FtuiConstraint::Fixed(3),
            FtuiConstraint::Fixed(6),
            FtuiConstraint::Fill,
            FtuiConstraint::Fixed(5),
            FtuiConstraint::Fixed(3),
        ])
        .gap(1)
        .split(ftui_full_area(f));

    if rows.len() < 5 {
        return;
    }

    render_beads_ftui_header(f, rows[0], data, theme);
    render_beads_ftui_quick_ref(f, rows[1], data, theme);
    render_beads_ftui_main(f, rows[2], data, theme);
    render_beads_ftui_graph(f, rows[3], data, theme);
    render_beads_ftui_footer(f, rows[4], data, theme);
}

fn render_beads_ftui_header(f: &mut FtuiFrame, area: FtuiRect, data: &BeadsData, theme: &Theme) {
    let colors = theme.ftui_colors();
    let spans = vec![
        FtuiSpan::styled(
            "  BEADS TRIAGE  ",
            FtuiStyle::new().fg(packed(colors.text)).bold(),
        ),
        FtuiSpan::styled(
            format!("[refresh: {}]", refresh_age_label(data.refresh_age_secs)),
            FtuiStyle::new().fg(packed(colors.muted)),
        ),
        FtuiSpan::raw(" "),
        FtuiSpan::styled(
            format!("[{} recs]", data.recommendations.len()),
            FtuiStyle::new().fg(packed(colors.info)),
        ),
        FtuiSpan::raw(" "),
        FtuiSpan::styled(
            format!("[{} blockers]", data.blockers.len()),
            FtuiStyle::new().fg(packed(colors.warning)),
        ),
        FtuiSpan::raw(" "),
        FtuiSpan::styled(
            format!("[{} open]", data.quick_ref.open_count),
            FtuiStyle::new().fg(packed(colors.muted)),
        ),
    ];

    let header = FtuiParagraph::new(FtuiText::from_spans(spans))
        .style(FtuiStyle::new().bg(packed(colors.bg_secondary)))
        .block(ftui_block(None, colors.muted));
    FtuiWidget::render(&header, area, f);
}

fn render_beads_ftui_quick_ref(f: &mut FtuiFrame, area: FtuiRect, data: &BeadsData, theme: &Theme) {
    let colors = theme.ftui_colors();
    let quick_ref = &data.quick_ref;
    let lines = vec![
        FtuiLine::from_spans([
            FtuiSpan::styled("Ready: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                quick_ref.actionable_count.to_string(),
                FtuiStyle::new().fg(packed(colors.healthy)).bold(),
            ),
            FtuiSpan::raw("  "),
            FtuiSpan::styled("Blocked: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                quick_ref.blocked_count.to_string(),
                FtuiStyle::new().fg(packed(colors.warning)).bold(),
            ),
            FtuiSpan::raw("  "),
            FtuiSpan::styled("In progress: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                quick_ref.in_progress_count.to_string(),
                FtuiStyle::new().fg(packed(colors.info)).bold(),
            ),
            FtuiSpan::raw("  "),
            FtuiSpan::styled("Open: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                quick_ref.open_count.to_string(),
                FtuiStyle::new().fg(packed(colors.text)).bold(),
            ),
        ]),
        FtuiLine::from_spans([
            FtuiSpan::styled("Priority mix: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                format!("P0 {}", quick_ref.by_priority[0]),
                FtuiStyle::new().fg(packed(colors.critical)),
            ),
            FtuiSpan::raw(" "),
            FtuiSpan::styled(
                format!("P1 {}", quick_ref.by_priority[1]),
                FtuiStyle::new().fg(packed(colors.warning)),
            ),
            FtuiSpan::raw(" "),
            FtuiSpan::styled(
                format!("P2 {}", quick_ref.by_priority[2]),
                FtuiStyle::new().fg(packed(colors.info)),
            ),
            FtuiSpan::raw(" "),
            FtuiSpan::styled(
                format!("P3 {}", quick_ref.by_priority[3]),
                FtuiStyle::new().fg(packed(colors.muted)),
            ),
            FtuiSpan::raw("  "),
            FtuiSpan::styled("Epics: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                format!(
                    "{}/{} ready",
                    quick_ref.epics_with_ready, quick_ref.total_epics
                ),
                FtuiStyle::new().fg(packed(colors.text)),
            ),
        ]),
    ];

    let paragraph = FtuiParagraph::new(FtuiText::from_lines(lines)).block(ftui_block(
        Some(" Quick Reference "),
        if data.selected_section == 0 {
            colors.accent
        } else {
            colors.muted
        },
    ));
    FtuiWidget::render(&paragraph, area, f);
}

fn render_beads_ftui_main(f: &mut FtuiFrame, area: FtuiRect, data: &BeadsData, theme: &Theme) {
    let cols = Flex::horizontal()
        .constraints([
            FtuiConstraint::Percentage(62.0),
            FtuiConstraint::Percentage(38.0),
        ])
        .gap(1)
        .split(area);

    if cols.len() < 2 {
        return;
    }

    render_beads_ftui_recommendations(f, cols[0], data, theme);
    render_beads_ftui_blockers(f, cols[1], data, theme);
}

fn render_beads_ftui_recommendations(
    f: &mut FtuiFrame,
    area: FtuiRect,
    data: &BeadsData,
    theme: &Theme,
) {
    let colors = theme.ftui_colors();
    let border_color = if data.selected_section == 1 {
        colors.accent
    } else {
        colors.muted
    };

    if data.recommendations.is_empty() {
        let empty = FtuiParagraph::new(FtuiText::from_spans([FtuiSpan::styled(
            "No recommendations available",
            FtuiStyle::new().fg(packed(colors.muted)),
        )]))
        .block(ftui_block(Some(" Recommended Next "), border_color));
        FtuiWidget::render(&empty, area, f);
        return;
    }

    let clamped_selected = data
        .selected_recommendation
        .min(data.recommendations.len().saturating_sub(1));
    let items: Vec<FtuiListItem> = data
        .recommendations
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let row_style = if data.selected_section == 1 && index == clamped_selected {
                FtuiStyle::new().bg(packed(colors.bg_secondary))
            } else {
                FtuiStyle::new()
            };
            let status_indicator = if item.status == "in_progress" {
                "◐"
            } else {
                "○"
            };
            let priority_color = priority_color_ftui(item.priority, theme);
            let title = truncate(&item.title, 52);

            FtuiListItem::new(FtuiText::from_lines([
                FtuiLine::from_spans([
                    FtuiSpan::styled(
                        format!("{status_indicator} "),
                        FtuiStyle::new().fg(packed(priority_color)).bold(),
                    ),
                    FtuiSpan::styled(
                        format!("[P{}]", item.priority),
                        FtuiStyle::new().fg(packed(priority_color)).bold(),
                    ),
                    FtuiSpan::raw(" "),
                    FtuiSpan::styled(&item.id, FtuiStyle::new().fg(packed(colors.accent))),
                    FtuiSpan::raw(" "),
                    FtuiSpan::styled(title, FtuiStyle::new().fg(packed(colors.text))),
                ]),
                FtuiLine::from_spans([
                    FtuiSpan::raw("    "),
                    FtuiSpan::styled(
                        format!("score {:.2}", item.score),
                        FtuiStyle::new().fg(packed(colors.info)),
                    ),
                    FtuiSpan::raw("  "),
                    FtuiSpan::styled(
                        format!("unblocks {}", item.unblocks_count),
                        FtuiStyle::new().fg(packed(colors.warning)),
                    ),
                ]),
                FtuiLine::from_spans([
                    FtuiSpan::raw("    "),
                    FtuiSpan::styled(
                        truncate(&item.reason, 68),
                        FtuiStyle::new().fg(packed(colors.muted)),
                    ),
                ]),
            ]))
            .style(row_style)
        })
        .collect();

    let list = FtuiList::new(items).block(ftui_block(Some(" Recommended Next "), border_color));
    FtuiWidget::render(&list, area, f);
}

fn render_beads_ftui_blockers(f: &mut FtuiFrame, area: FtuiRect, data: &BeadsData, theme: &Theme) {
    let colors = theme.ftui_colors();
    let border_color = if data.selected_section == 2 {
        colors.accent
    } else {
        colors.muted
    };

    if data.blockers.is_empty() {
        let empty = FtuiParagraph::new(FtuiText::from_spans([FtuiSpan::styled(
            "No blockers to clear",
            FtuiStyle::new().fg(packed(colors.muted)),
        )]))
        .block(ftui_block(Some(" Blockers to Clear "), border_color));
        FtuiWidget::render(&empty, area, f);
        return;
    }

    let clamped_selected = data
        .selected_blocker
        .min(data.blockers.len().saturating_sub(1));
    let items: Vec<FtuiListItem> = data
        .blockers
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let row_style = if data.selected_section == 2 && index == clamped_selected {
                FtuiStyle::new().bg(packed(colors.bg_secondary))
            } else {
                FtuiStyle::new()
            };
            let actionable_color = if item.is_actionable {
                colors.healthy
            } else {
                colors.warning
            };
            let actionable_label = if item.is_actionable {
                "ready"
            } else {
                "blocked"
            };
            let blocked_by = if item.blocked_by.is_empty() {
                "No upstream blockers".to_string()
            } else {
                format!("Waiting on {}", item.blocked_by.join(", "))
            };

            FtuiListItem::new(FtuiText::from_lines([
                FtuiLine::from_spans([
                    FtuiSpan::styled(
                        if item.is_actionable { "✓ " } else { "⏳ " },
                        FtuiStyle::new().fg(packed(actionable_color)).bold(),
                    ),
                    FtuiSpan::styled(&item.id, FtuiStyle::new().fg(packed(colors.accent))),
                    FtuiSpan::raw(" "),
                    FtuiSpan::styled(
                        truncate(&item.title, 32),
                        FtuiStyle::new().fg(packed(colors.text)),
                    ),
                ]),
                FtuiLine::from_spans([
                    FtuiSpan::raw("   "),
                    FtuiSpan::styled(
                        format!("{actionable_label} | unblocks {}", item.unblocks_count),
                        FtuiStyle::new().fg(packed(actionable_color)),
                    ),
                ]),
                FtuiLine::from_spans([
                    FtuiSpan::raw("   "),
                    FtuiSpan::styled(blocked_by, FtuiStyle::new().fg(packed(colors.muted))),
                ]),
            ]))
            .style(row_style)
        })
        .collect();

    let list = FtuiList::new(items).block(ftui_block(Some(" Blockers to Clear "), border_color));
    FtuiWidget::render(&list, area, f);
}

fn render_beads_ftui_graph(f: &mut FtuiFrame, area: FtuiRect, data: &BeadsData, theme: &Theme) {
    let colors = theme.ftui_colors();
    let graph = &data.graph_health;
    let cycle_color = if graph.has_cycles {
        colors.critical
    } else {
        colors.healthy
    };
    let lines = vec![
        FtuiLine::from_spans([
            FtuiSpan::styled("Nodes: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                graph.node_count.to_string(),
                FtuiStyle::new().fg(packed(colors.text)),
            ),
            FtuiSpan::raw("  "),
            FtuiSpan::styled("Edges: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                graph.edge_count.to_string(),
                FtuiStyle::new().fg(packed(colors.text)),
            ),
            FtuiSpan::raw("  "),
            FtuiSpan::styled("Density: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                format!("{:.1}%", graph.density * 100.0),
                FtuiStyle::new().fg(packed(colors.info)),
            ),
        ]),
        FtuiLine::from_spans([
            FtuiSpan::styled("Cycles: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                if graph.has_cycles {
                    "detected"
                } else {
                    "clean"
                },
                FtuiStyle::new().fg(packed(cycle_color)).bold(),
            ),
            FtuiSpan::raw("  "),
            FtuiSpan::styled("Velocity: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                format!(
                    "{} / 7d, {} / 30d",
                    graph.closed_last_7d, graph.closed_last_30d
                ),
                FtuiStyle::new().fg(packed(colors.text)),
            ),
            FtuiSpan::raw("  "),
            FtuiSpan::styled("Avg close: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                format!("{:.1}d", graph.avg_days_to_close),
                FtuiStyle::new().fg(packed(colors.warning)),
            ),
        ]),
    ];

    let panel = FtuiParagraph::new(FtuiText::from_lines(lines)).block(ftui_block(
        Some(" Graph Health "),
        if data.selected_section == 3 {
            colors.accent
        } else {
            colors.muted
        },
    ));
    FtuiWidget::render(&panel, area, f);
}

fn render_beads_ftui_footer(f: &mut FtuiFrame, area: FtuiRect, data: &BeadsData, theme: &Theme) {
    let colors = theme.ftui_colors();
    let footer = FtuiParagraph::new(FtuiText::from_spans([
        FtuiSpan::styled("Sections:", FtuiStyle::new().fg(packed(colors.muted))),
        FtuiSpan::raw(" "),
        FtuiSpan::styled("Quick", FtuiStyle::new().fg(packed(colors.accent))),
        FtuiSpan::raw(" / "),
        FtuiSpan::styled(
            "Recommendations",
            FtuiStyle::new().fg(packed(colors.accent)),
        ),
        FtuiSpan::raw(" / "),
        FtuiSpan::styled("Blockers", FtuiStyle::new().fg(packed(colors.accent))),
        FtuiSpan::raw(" / "),
        FtuiSpan::styled("Graph", FtuiStyle::new().fg(packed(colors.accent))),
        FtuiSpan::raw("  "),
        FtuiSpan::styled(
            format!("last refresh {}", refresh_age_label(data.refresh_age_secs)),
            FtuiStyle::new().fg(packed(colors.muted)),
        ),
    ]))
    .style(FtuiStyle::new().bg(packed(colors.bg_secondary)))
    .block(ftui_block(None, colors.muted));
    FtuiWidget::render(&footer, area, f);
}

fn refresh_age_label(refresh_age_secs: u64) -> String {
    if refresh_age_secs < 60 {
        format!("{refresh_age_secs}s ago")
    } else if refresh_age_secs < 3_600 {
        format!("{}m ago", refresh_age_secs / 60)
    } else {
        format!("{}h ago", refresh_age_secs / 3_600)
    }
}

fn priority_color_ftui(priority: u32, theme: &Theme) -> ftui::Color {
    match priority {
        0 => theme.ftui_colors().critical,
        1 => theme.ftui_colors().warning,
        2 => theme.ftui_colors().info,
        _ => theme.ftui_colors().muted,
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
        assert!(rec.score.abs() < f64::EPSILON);
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
        assert!(health.density.abs() < f64::EPSILON);
    }

    #[test]
    fn test_truncate_short() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_long() {
        let result = truncate("hello world this is a long string", 10);
        assert!(result.chars().count() <= 10);
        assert!(result.ends_with('…'));
    }

    #[test]
    fn test_priority_color_ftui_p0() {
        let theme = Theme::default();
        assert_eq!(priority_color_ftui(0, &theme), theme.ftui_colors().critical);
    }

    #[test]
    fn test_priority_color_ftui_p1() {
        let theme = Theme::default();
        assert_eq!(priority_color_ftui(1, &theme), theme.ftui_colors().warning);
    }

    #[test]
    fn test_priority_color_ftui_p2() {
        let theme = Theme::default();
        assert_eq!(priority_color_ftui(2, &theme), theme.ftui_colors().info);
    }

    #[test]
    fn test_priority_color_ftui_p3() {
        let theme = Theme::default();
        assert_eq!(priority_color_ftui(3, &theme), theme.ftui_colors().muted);
    }

    #[test]
    fn test_render_beads_ftui_renders_lists() {
        let data = BeadsData {
            quick_ref: QuickRefData {
                open_count: 12,
                actionable_count: 4,
                blocked_count: 3,
                in_progress_count: 2,
                epics_with_ready: 1,
                total_epics: 3,
                by_priority: [1, 5, 4, 2],
            },
            recommendations: vec![RecommendationItem {
                id: "bd-1l1".to_string(),
                title: "Port Events, Beads, RCH, and Settings screens to ftui".to_string(),
                priority: 1,
                score: 0.71,
                unblocks_count: 2,
                status: "in_progress".to_string(),
                reason: "Completes the remaining ftui migration surface.".to_string(),
            }],
            blockers: vec![BlockerItem {
                id: "bd-bvt".to_string(),
                title: "Port VcStore struct from duckdb::Connection to fsqlite::Connection"
                    .to_string(),
                unblocks_count: 5,
                is_actionable: false,
                blocked_by: vec!["bd-kft".to_string()],
            }],
            graph_health: GraphHealthData {
                node_count: 64,
                edge_count: 73,
                density: 0.18,
                has_cycles: false,
                closed_last_7d: 8,
                closed_last_30d: 24,
                avg_days_to_close: 3.6,
            },
            selected_section: 1,
            selected_recommendation: 0,
            selected_blocker: 0,
            refresh_age_secs: 42,
        };
        let theme = Theme::default();
        let mut pool = GraphemePool::new();
        let mut frame = FtuiFrame::new(120, 28, &mut pool);

        render_beads_ftui(&mut frame, &data, &theme);

        assert!(buffer_contains(&frame.buffer, 120, 28, "BEADS TRIAGE"));
        assert!(buffer_contains(&frame.buffer, 120, 28, "bd-1l1"));
        assert!(buffer_contains(&frame.buffer, 120, 28, "bd-bvt"));
        assert!(buffer_contains(&frame.buffer, 120, 28, "Graph Health"));
    }

    #[test]
    fn test_render_beads_ftui_renders_empty_state() {
        let data = BeadsData::default();
        let theme = Theme::default();
        let mut pool = GraphemePool::new();
        let mut frame = FtuiFrame::new(100, 24, &mut pool);

        render_beads_ftui(&mut frame, &data, &theme);

        assert!(buffer_contains(&frame.buffer, 100, 24, "BEADS TRIAGE"));
        assert!(buffer_contains(
            &frame.buffer,
            100,
            24,
            "No recommendations available"
        ));
        assert!(buffer_contains(
            &frame.buffer,
            100,
            24,
            "No blockers to clear"
        ));
    }
}
