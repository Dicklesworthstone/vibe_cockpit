//! Overview screen implementation
//!
//! The main dashboard showing fleet status, machines, alerts, and repos.

use crate::theme::Theme;
use crate::widgets::{severity_indicator, status_indicator};
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

/// Data needed to render the overview screen
#[derive(Debug, Clone, Default)]
pub struct OverviewData {
    /// Overall fleet health score (0.0 to 1.0)
    pub fleet_health: f64,
    /// List of machines with their status
    pub machines: Vec<MachineStatus>,
    /// Recent alerts
    pub alerts: Vec<AlertSummary>,
    /// Repository status summaries
    pub repos: Vec<RepoStatus>,
    /// Seconds since last data refresh
    pub refresh_age_secs: u64,
}

/// Machine status for display
#[derive(Debug, Clone)]
pub struct MachineStatus {
    pub id: String,
    pub hostname: String,
    pub online: bool,
    pub cpu_pct: Option<f64>,
    pub mem_pct: Option<f64>,
    pub health_score: f64,
}

impl Default for MachineStatus {
    fn default() -> Self {
        Self {
            id: String::new(),
            hostname: String::new(),
            online: false,
            cpu_pct: None,
            mem_pct: None,
            health_score: 1.0,
        }
    }
}

/// Alert summary for display
#[derive(Debug, Clone)]
pub struct AlertSummary {
    pub severity: String,
    pub title: String,
    pub machine_id: Option<String>,
}

impl Default for AlertSummary {
    fn default() -> Self {
        Self {
            severity: "info".to_string(),
            title: String::new(),
            machine_id: None,
        }
    }
}

/// Repository status for display
#[derive(Debug, Clone)]
pub struct RepoStatus {
    pub name: String,
    pub branch: String,
    pub is_dirty: bool,
    pub ahead: u32,
    pub behind: u32,
    pub modified_count: u32,
}

impl Default for RepoStatus {
    fn default() -> Self {
        Self {
            name: String::new(),
            branch: "main".to_string(),
            is_dirty: false,
            ahead: 0,
            behind: 0,
            modified_count: 0,
        }
    }
}

/// Render the overview screen using `ftui` widgets.
pub fn render_overview_ftui(f: &mut FtuiFrame, data: &OverviewData, theme: &Theme) {
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

    render_ftui_header(f, rows[0], data, theme);
    render_ftui_main_content(f, rows[1], data, theme);
    render_ftui_footer(f, rows[2], theme);
}

fn render_ftui_header(f: &mut FtuiFrame, area: FtuiRect, data: &OverviewData, theme: &Theme) {
    let colors = theme.ftui_colors();
    let refresh_text = refresh_label(data.refresh_age_secs);
    let header = FtuiParagraph::new(FtuiText::from_spans([
        FtuiSpan::styled(
            "  V I B E   C O C K P I T  ",
            FtuiStyle::new().fg(packed(colors.text)).bold(),
        ),
        FtuiSpan::styled("[Health: ", FtuiStyle::new().fg(packed(colors.muted))),
        FtuiSpan::styled(
            theme.health_indicator(data.fleet_health),
            FtuiStyle::new().fg(packed(theme.health_color(data.fleet_health))),
        ),
        FtuiSpan::styled("]", FtuiStyle::new().fg(packed(colors.muted))),
        FtuiSpan::raw("  "),
        FtuiSpan::styled(
            format!("[Refresh: {refresh_text}]"),
            FtuiStyle::new().fg(packed(colors.muted)),
        ),
    ]))
    .style(FtuiStyle::new().bg(packed(colors.bg_secondary)))
    .block(ftui_block(None, theme));

    FtuiWidget::render(&header, area, f);
}

fn render_ftui_main_content(f: &mut FtuiFrame, area: FtuiRect, data: &OverviewData, theme: &Theme) {
    let rows = Flex::vertical()
        .constraints([
            FtuiConstraint::Percentage(60.0),
            FtuiConstraint::Percentage(40.0),
        ])
        .gap(1)
        .split(area);

    if rows.len() < 2 {
        return;
    }

    let top_cols = Flex::horizontal()
        .constraints([
            FtuiConstraint::Percentage(60.0),
            FtuiConstraint::Percentage(40.0),
        ])
        .gap(1)
        .split(rows[0]);

    if top_cols.len() >= 2 {
        render_ftui_machines_panel(f, top_cols[0], &data.machines, theme);
        render_ftui_alerts_panel(f, top_cols[1], &data.alerts, theme);
    }

    render_ftui_repos_panel(f, rows[1], &data.repos, theme);
}

fn render_ftui_machines_panel(
    f: &mut FtuiFrame,
    area: FtuiRect,
    machines: &[MachineStatus],
    theme: &Theme,
) {
    let colors = theme.ftui_colors();
    let items: Vec<FtuiListItem> = if machines.is_empty() {
        vec![FtuiListItem::new(FtuiText::from_spans([FtuiSpan::styled(
            "No machines registered",
            FtuiStyle::new().fg(packed(colors.muted)),
        )]))]
    } else {
        machines
            .iter()
            .map(|machine| {
                let metrics = if machine.online {
                    match (machine.cpu_pct, machine.mem_pct) {
                        (Some(cpu), Some(mem)) => format!("CPU {cpu:>3.0}% MEM {mem:>3.0}%"),
                        _ => "metrics pending".to_string(),
                    }
                } else {
                    "[offline]".to_string()
                };

                FtuiListItem::new(FtuiText::from_lines([FtuiLine::from_spans([
                    status_indicator(machine.online, theme),
                    FtuiSpan::raw(" "),
                    FtuiSpan::styled(
                        format!("{:<16}", machine.hostname),
                        FtuiStyle::new().fg(packed(if machine.online {
                            colors.text
                        } else {
                            colors.muted
                        })),
                    ),
                    FtuiSpan::styled(
                        metrics,
                        FtuiStyle::new().fg(packed(theme.health_color(machine.health_score))),
                    ),
                ])]))
            })
            .collect()
    };

    let list = FtuiList::new(items)
        .style(FtuiStyle::new().bg(packed(colors.bg_primary)))
        .block(ftui_block(Some(" MACHINES "), theme));

    FtuiWidget::render(&list, area, f);
}

fn render_ftui_alerts_panel(
    f: &mut FtuiFrame,
    area: FtuiRect,
    alerts: &[AlertSummary],
    theme: &Theme,
) {
    let colors = theme.ftui_colors();
    let items: Vec<FtuiListItem> = if alerts.is_empty() {
        vec![FtuiListItem::new(FtuiText::from_spans([FtuiSpan::styled(
            "No active alerts",
            FtuiStyle::new().fg(packed(colors.muted)),
        )]))]
    } else {
        alerts
            .iter()
            .map(|alert| {
                let (indicator, severity_color) = severity_indicator(&alert.severity, theme);
                let machine_suffix = alert
                    .machine_id
                    .as_deref()
                    .map_or_else(String::new, |id| format!(" [{id}]"));

                FtuiListItem::new(FtuiText::from_lines([FtuiLine::from_spans([
                    indicator,
                    FtuiSpan::raw(" "),
                    FtuiSpan::styled(
                        format!("{}{}", alert.title, machine_suffix),
                        FtuiStyle::new().fg(packed(colors.text)),
                    ),
                    FtuiSpan::raw(" "),
                    FtuiSpan::styled(
                        alert.severity.to_uppercase(),
                        FtuiStyle::new().fg(packed(severity_color)),
                    ),
                ])]))
            })
            .collect()
    };

    let list = FtuiList::new(items)
        .style(FtuiStyle::new().bg(packed(colors.bg_primary)))
        .block(ftui_block(Some(" ALERTS "), theme));

    FtuiWidget::render(&list, area, f);
}

fn render_ftui_repos_panel(f: &mut FtuiFrame, area: FtuiRect, repos: &[RepoStatus], theme: &Theme) {
    let colors = theme.ftui_colors();
    let items: Vec<FtuiListItem> = if repos.is_empty() {
        vec![FtuiListItem::new(FtuiText::from_spans([FtuiSpan::styled(
            "No repositories tracked",
            FtuiStyle::new().fg(packed(colors.muted)),
        )]))]
    } else {
        repos
            .iter()
            .map(|repo| {
                let status_indicator = if repo.is_dirty {
                    FtuiSpan::styled("!", FtuiStyle::new().fg(packed(colors.warning)))
                } else {
                    FtuiSpan::styled("✓", FtuiStyle::new().fg(packed(colors.healthy)))
                };

                let status_text = if repo.is_dirty {
                    format!("dirty  {} modified", repo.modified_count)
                } else {
                    "clean".to_string()
                };

                FtuiListItem::new(FtuiText::from_lines([FtuiLine::from_spans([
                    FtuiSpan::styled(
                        format!("{:<20}", repo.name),
                        FtuiStyle::new().fg(packed(colors.text)),
                    ),
                    FtuiSpan::styled(
                        format!("{:<8}", repo.branch),
                        FtuiStyle::new().fg(packed(colors.muted)),
                    ),
                    status_indicator,
                    FtuiSpan::raw(" "),
                    FtuiSpan::styled(
                        format!("{status_text:<18}"),
                        FtuiStyle::new().fg(packed(if repo.is_dirty {
                            colors.warning
                        } else {
                            colors.healthy
                        })),
                    ),
                    FtuiSpan::styled(
                        format!("{}↑ {}↓", repo.ahead, repo.behind),
                        FtuiStyle::new().fg(packed(colors.info)),
                    ),
                ])]))
            })
            .collect()
    };

    let list = FtuiList::new(items)
        .style(FtuiStyle::new().bg(packed(colors.bg_primary)))
        .block(ftui_block(Some(" REPOS "), theme));

    FtuiWidget::render(&list, area, f);
}

fn render_ftui_footer(f: &mut FtuiFrame, area: FtuiRect, theme: &Theme) {
    let colors = theme.ftui_colors();
    let shortcuts = [
        ("[?]", "Help"),
        ("[q]", "Quit"),
        ("[r]", "Refresh"),
        ("[m]", "Machines"),
        ("[a]", "Accounts"),
        ("[g]", "Repos"),
        ("[l]", "Mail"),
        ("[b]", "Beads"),
    ];

    let spans: Vec<FtuiSpan> = shortcuts
        .into_iter()
        .flat_map(|(key, action)| {
            [
                FtuiSpan::styled(key, FtuiStyle::new().fg(packed(colors.accent))),
                FtuiSpan::styled(action, FtuiStyle::new().fg(packed(colors.muted))),
                FtuiSpan::raw(" "),
            ]
        })
        .collect();

    let footer = FtuiParagraph::new(FtuiText::from_lines([FtuiLine::from_spans(spans)]))
        .style(FtuiStyle::new().bg(packed(colors.bg_secondary)))
        .block(ftui_block(None, theme));

    FtuiWidget::render(&footer, area, f);
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

fn refresh_label(refresh_age_secs: u64) -> String {
    if refresh_age_secs == 0 {
        "just now".to_string()
    } else if refresh_age_secs < 60 {
        format!("{refresh_age_secs}s ago")
    } else {
        format!("{}m ago", refresh_age_secs / 60)
    }
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
    fn test_overview_data_default() {
        let data = OverviewData::default();
        assert!(data.fleet_health.abs() < f64::EPSILON);
        assert!(data.machines.is_empty());
        assert!(data.alerts.is_empty());
        assert!(data.repos.is_empty());
    }

    #[test]
    fn test_machine_status_default() {
        let status = MachineStatus::default();
        assert!(!status.online);
        assert!(status.cpu_pct.is_none());
        assert!((status.health_score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_alert_summary_default() {
        let alert = AlertSummary::default();
        assert_eq!(alert.severity, "info");
        assert!(alert.title.is_empty());
    }

    #[test]
    fn test_repo_status_default() {
        let repo = RepoStatus::default();
        assert_eq!(repo.branch, "main");
        assert!(!repo.is_dirty);
        assert_eq!(repo.ahead, 0);
    }

    #[test]
    fn test_overview_data_with_machines() {
        let data = OverviewData {
            fleet_health: 0.9,
            machines: vec![
                MachineStatus {
                    id: "m1".to_string(),
                    hostname: "orko".to_string(),
                    online: true,
                    cpu_pct: Some(45.0),
                    mem_pct: Some(68.0),
                    health_score: 0.85,
                },
                MachineStatus {
                    id: "m2".to_string(),
                    hostname: "gpu-box".to_string(),
                    online: false,
                    cpu_pct: None,
                    mem_pct: None,
                    health_score: 0.0,
                },
            ],
            alerts: vec![],
            repos: vec![],
            refresh_age_secs: 30,
        };

        assert_eq!(data.machines.len(), 2);
        assert!(data.machines[0].online);
        assert!(!data.machines[1].online);
    }

    #[test]
    fn test_overview_data_with_alerts() {
        let data = OverviewData {
            fleet_health: 0.5,
            machines: vec![],
            alerts: vec![
                AlertSummary {
                    severity: "critical".to_string(),
                    title: "High CPU usage on orko".to_string(),
                    machine_id: Some("m1".to_string()),
                },
                AlertSummary {
                    severity: "warning".to_string(),
                    title: "Disk space low".to_string(),
                    machine_id: None,
                },
            ],
            repos: vec![],
            refresh_age_secs: 0,
        };

        assert_eq!(data.alerts.len(), 2);
        assert_eq!(data.alerts[0].severity, "critical");
    }

    #[test]
    fn test_overview_data_with_repos() {
        let data = OverviewData {
            fleet_health: 1.0,
            machines: vec![],
            alerts: vec![],
            repos: vec![
                RepoStatus {
                    name: "vibe_cockpit".to_string(),
                    branch: "main".to_string(),
                    is_dirty: false,
                    ahead: 0,
                    behind: 0,
                    modified_count: 0,
                },
                RepoStatus {
                    name: "dcg".to_string(),
                    branch: "main".to_string(),
                    is_dirty: true,
                    ahead: 0,
                    behind: 0,
                    modified_count: 3,
                },
            ],
            refresh_age_secs: 120,
        };

        assert_eq!(data.repos.len(), 2);
        assert!(!data.repos[0].is_dirty);
        assert!(data.repos[1].is_dirty);
    }

    #[test]
    fn test_render_overview_ftui_renders_data_panels() {
        let theme = Theme::default();
        let data = OverviewData {
            fleet_health: 0.82,
            machines: vec![MachineStatus {
                id: "m1".to_string(),
                hostname: "orko".to_string(),
                online: true,
                cpu_pct: Some(45.0),
                mem_pct: Some(63.0),
                health_score: 0.9,
            }],
            alerts: vec![AlertSummary {
                severity: "warning".to_string(),
                title: "Disk trending high".to_string(),
                machine_id: Some("m1".to_string()),
            }],
            repos: vec![RepoStatus {
                name: "vibe_cockpit".to_string(),
                branch: "main".to_string(),
                is_dirty: false,
                ahead: 1,
                behind: 0,
                modified_count: 0,
            }],
            refresh_age_secs: 12,
        };
        let mut pool = GraphemePool::new();
        let mut frame = FtuiFrame::new(100, 24, &mut pool);

        render_overview_ftui(&mut frame, &data, &theme);

        assert!(buffer_contains(
            &frame.buffer,
            100,
            24,
            "V I B E   C O C K P I T"
        ));
        assert!(buffer_contains(&frame.buffer, 100, 24, "orko"));
        assert!(buffer_contains(
            &frame.buffer,
            100,
            24,
            "Disk trending high"
        ));
        assert!(buffer_contains(&frame.buffer, 100, 24, "vibe_cockpit"));
    }

    #[test]
    fn test_render_overview_ftui_renders_empty_states() {
        let theme = Theme::default();
        let data = OverviewData::default();
        let mut pool = GraphemePool::new();
        let mut frame = FtuiFrame::new(100, 24, &mut pool);

        render_overview_ftui(&mut frame, &data, &theme);

        assert!(buffer_contains(
            &frame.buffer,
            100,
            24,
            "No machines registered"
        ));
        assert!(buffer_contains(&frame.buffer, 100, 24, "No active alerts"));
        assert!(buffer_contains(
            &frame.buffer,
            100,
            24,
            "No repositories tracked"
        ));
    }
}
