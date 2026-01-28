//! Overview screen implementation
//!
//! The main dashboard showing fleet status, machines, alerts, and repos.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use crate::theme::Theme;

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

/// Render the overview screen
pub fn render_overview(f: &mut Frame, data: &OverviewData, theme: &Theme) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Main content
            Constraint::Length(3), // Footer
        ])
        .split(f.area());

    render_header(f, chunks[0], data, theme);
    render_main_content(f, chunks[1], data, theme);
    render_footer(f, chunks[2], theme);
}

fn render_header(f: &mut Frame, area: Rect, data: &OverviewData, theme: &Theme) {
    let health_color = theme.health_color(data.fleet_health);
    let health_indicator = theme.health_indicator(data.fleet_health);

    let refresh_text = if data.refresh_age_secs == 0 {
        "just now".to_string()
    } else if data.refresh_age_secs < 60 {
        format!("{}s ago", data.refresh_age_secs)
    } else {
        format!("{}m ago", data.refresh_age_secs / 60)
    };

    let title = Line::from(vec![
        Span::styled(
            "  V I B E   C O C K P I T  ",
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled("[Health: ", Style::default().fg(theme.muted)),
        Span::styled(health_indicator, Style::default().fg(health_color)),
        Span::styled("]", Style::default().fg(theme.muted)),
        Span::raw("  "),
        Span::styled(
            format!("[Refresh: {}]", refresh_text),
            Style::default().fg(theme.muted),
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

fn render_main_content(f: &mut Frame, area: Rect, data: &OverviewData, theme: &Theme) {
    // Split into top row (machines + alerts) and bottom row (repos)
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    // Top row: machines and alerts side by side
    let top_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(rows[0]);

    render_machines_panel(f, top_cols[0], &data.machines, theme);
    render_alerts_panel(f, top_cols[1], &data.alerts, theme);

    // Bottom row: repos
    render_repos_panel(f, rows[1], &data.repos, theme);
}

fn render_machines_panel(f: &mut Frame, area: Rect, machines: &[MachineStatus], theme: &Theme) {
    let items: Vec<ListItem> = if machines.is_empty() {
        vec![ListItem::new(Span::styled(
            "  No machines registered",
            Style::default().fg(theme.muted),
        ))]
    } else {
        machines
            .iter()
            .map(|m| {
                let status_indicator = if m.online {
                    Span::styled("●", Style::default().fg(theme.healthy))
                } else {
                    Span::styled("○", Style::default().fg(theme.critical))
                };

                let metrics = if m.online {
                    match (m.cpu_pct, m.mem_pct) {
                        (Some(cpu), Some(mem)) => {
                            format!("CPU {:>3.0}% MEM {:>3.0}%", cpu, mem)
                        }
                        _ => "metrics pending".to_string(),
                    }
                } else {
                    "[offline]".to_string()
                };

                let health_color = theme.health_color(m.health_score);

                ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    status_indicator,
                    Span::raw(" "),
                    Span::styled(
                        format!("{:<16}", m.hostname),
                        Style::default().fg(if m.online { theme.text } else { theme.muted }),
                    ),
                    Span::styled(metrics, Style::default().fg(health_color)),
                ]))
            })
            .collect()
    };

    let list = List::new(items).block(
        Block::default()
            .title(Span::styled(
                " MACHINES ",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.muted)),
    );

    f.render_widget(list, area);
}

fn render_alerts_panel(f: &mut Frame, area: Rect, alerts: &[AlertSummary], theme: &Theme) {
    let items: Vec<ListItem> = if alerts.is_empty() {
        vec![ListItem::new(Span::styled(
            "  No active alerts",
            Style::default().fg(theme.muted),
        ))]
    } else {
        alerts
            .iter()
            .map(|a| {
                let (indicator, color) = match a.severity.to_lowercase().as_str() {
                    "critical" => ("!", theme.critical),
                    "warning" => ("⚠", theme.warning),
                    _ => ("ℹ", theme.info),
                };

                ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(indicator, Style::default().fg(color)),
                    Span::raw(" "),
                    Span::styled(&a.title, Style::default().fg(theme.text)),
                ]))
            })
            .collect()
    };

    let list = List::new(items).block(
        Block::default()
            .title(Span::styled(
                " ALERTS ",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.muted)),
    );

    f.render_widget(list, area);
}

fn render_repos_panel(f: &mut Frame, area: Rect, repos: &[RepoStatus], theme: &Theme) {
    let items: Vec<ListItem> = if repos.is_empty() {
        vec![ListItem::new(Span::styled(
            "  No repositories tracked",
            Style::default().fg(theme.muted),
        ))]
    } else {
        repos
            .iter()
            .map(|r| {
                let status_indicator = if r.is_dirty {
                    Span::styled("!", Style::default().fg(theme.warning))
                } else {
                    Span::styled("✓", Style::default().fg(theme.healthy))
                };

                let status_text = if r.is_dirty {
                    format!("dirty  {} modified", r.modified_count)
                } else {
                    "clean".to_string()
                };

                let sync_status = format!("{}↑ {}↓", r.ahead, r.behind);

                ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(format!("{:<20}", r.name), Style::default().fg(theme.text)),
                    Span::styled(format!("{:<8}", r.branch), Style::default().fg(theme.muted)),
                    status_indicator,
                    Span::raw(" "),
                    Span::styled(
                        format!("{:<18}", status_text),
                        Style::default().fg(if r.is_dirty {
                            theme.warning
                        } else {
                            theme.healthy
                        }),
                    ),
                    Span::styled(sync_status, Style::default().fg(theme.info)),
                ]))
            })
            .collect()
    };

    let list = List::new(items).block(
        Block::default()
            .title(Span::styled(
                " REPOS ",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.muted)),
    );

    f.render_widget(list, area);
}

fn render_footer(f: &mut Frame, area: Rect, theme: &Theme) {
    let shortcuts = vec![
        ("[?]", "Help"),
        ("[q]", "Quit"),
        ("[r]", "Refresh"),
        ("[m]", "Machines"),
        ("[a]", "Accounts"),
        ("[g]", "Repos"),
        ("[l]", "Mail"),
        ("[b]", "Beads"),
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
    fn test_overview_data_default() {
        let data = OverviewData::default();
        assert_eq!(data.fleet_health, 0.0);
        assert!(data.machines.is_empty());
        assert!(data.alerts.is_empty());
        assert!(data.repos.is_empty());
    }

    #[test]
    fn test_machine_status_default() {
        let status = MachineStatus::default();
        assert!(!status.online);
        assert!(status.cpu_pct.is_none());
        assert_eq!(status.health_score, 1.0);
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
}
