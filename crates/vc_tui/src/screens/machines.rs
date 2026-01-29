//! Machines screen implementation
//!
//! TUI screens for machine inventory, individual machine details,
//! and fleet management.

use chrono::{DateTime, Utc};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table},
    Frame,
};

use crate::theme::Theme;

/// View mode for the machines screen
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MachinesViewMode {
    /// List view showing all machines
    #[default]
    List,
    /// Detail view for a single machine
    Detail,
    /// Comparison view for multiple machines
    Compare,
}

/// Sort field for machines list
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MachineSortField {
    #[default]
    Id,
    Hostname,
    Status,
    ToolCount,
    LastSeen,
}

/// Machine status values
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MachineOnlineStatus {
    Online,
    Offline,
    #[default]
    Unknown,
}

impl MachineOnlineStatus {
    /// Get display indicator
    pub fn indicator(&self) -> &'static str {
        match self {
            Self::Online => "●",
            Self::Offline => "○",
            Self::Unknown => "◌",
        }
    }
}

/// Data needed to render the machines screen
#[derive(Debug, Clone, Default)]
pub struct MachinesData {
    /// View mode
    pub view_mode: MachinesViewMode,
    /// List of machines
    pub machines: Vec<MachineRow>,
    /// Currently selected machine index
    pub selected_index: usize,
    /// Selected machine detail (when in detail mode)
    pub selected_detail: Option<MachineDetail>,
    /// Sort field
    pub sort_field: MachineSortField,
    /// Sort ascending
    pub sort_ascending: bool,
    /// Tag filter (empty = show all)
    pub tag_filter: Option<String>,
    /// Seconds since last refresh
    pub refresh_age_secs: u64,
}

/// Machine row for list display
#[derive(Debug, Clone, Default)]
pub struct MachineRow {
    /// Machine ID
    pub machine_id: String,
    /// Hostname
    pub hostname: String,
    /// Display name (optional)
    pub display_name: Option<String>,
    /// Online status
    pub status: MachineOnlineStatus,
    /// Number of available tools
    pub tool_count: usize,
    /// Last seen timestamp
    pub last_seen: Option<DateTime<Utc>>,
    /// Last probe timestamp
    pub last_probe: Option<DateTime<Utc>>,
    /// Tags
    pub tags: Vec<String>,
    /// Is local machine
    pub is_local: bool,
    /// Enabled flag
    pub enabled: bool,
}

/// Detailed machine information
#[derive(Debug, Clone, Default)]
pub struct MachineDetail {
    /// Base machine info
    pub machine: MachineRow,
    /// SSH connection string
    pub ssh_target: Option<String>,
    /// Available tools
    pub tools: Vec<ToolInfoRow>,
    /// System stats (if available)
    pub system_stats: Option<SystemStats>,
    /// Recent collection events
    pub recent_collections: Vec<CollectionEvent>,
}

/// Tool information for display
#[derive(Debug, Clone, Default)]
pub struct ToolInfoRow {
    /// Tool name
    pub name: String,
    /// Tool path on machine
    pub path: Option<String>,
    /// Tool version
    pub version: Option<String>,
    /// Is available
    pub available: bool,
}

/// System stats from sysmoni
#[derive(Debug, Clone, Default)]
pub struct SystemStats {
    /// CPU usage percentage
    pub cpu_pct: f64,
    /// Memory usage percentage
    pub mem_pct: f64,
    /// Load average (1 min)
    pub load1: f64,
    /// Disk usage percentage (root)
    pub disk_pct: f64,
    /// Uptime in seconds
    pub uptime_secs: i64,
}

/// Recent collection event
#[derive(Debug, Clone, Default)]
pub struct CollectionEvent {
    /// Collector name
    pub collector: String,
    /// When collected
    pub collected_at: DateTime<Utc>,
    /// Number of records
    pub record_count: usize,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Success status
    pub success: bool,
}

/// Render the machines screen
pub fn render_machines(f: &mut Frame, data: &MachinesData, theme: &Theme) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Main content
            Constraint::Length(3), // Footer/help
        ])
        .split(f.area());

    render_header(f, chunks[0], data, theme);

    match data.view_mode {
        MachinesViewMode::List => render_list_view(f, chunks[1], data, theme),
        MachinesViewMode::Detail => render_detail_view(f, chunks[1], data, theme),
        MachinesViewMode::Compare => render_compare_view(f, chunks[1], data, theme),
    }

    render_footer(f, chunks[2], data, theme);
}

fn render_header(f: &mut Frame, area: Rect, data: &MachinesData, theme: &Theme) {
    let online_count = data
        .machines
        .iter()
        .filter(|m| m.status == MachineOnlineStatus::Online)
        .count();
    let total_count = data.machines.len();

    let refresh_text = if data.refresh_age_secs == 0 {
        "just now".to_string()
    } else if data.refresh_age_secs < 60 {
        format!("{}s ago", data.refresh_age_secs)
    } else {
        format!("{}m ago", data.refresh_age_secs / 60)
    };

    let title = Line::from(vec![
        Span::styled(
            "  MACHINES  ",
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("[{}/{} online]", online_count, total_count),
            Style::default().fg(if online_count == total_count {
                theme.healthy
            } else {
                theme.warning
            }),
        ),
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

fn render_list_view(f: &mut Frame, area: Rect, data: &MachinesData, theme: &Theme) {
    let header_cells = ["", "ID", "Hostname", "Status", "Tools", "Last Seen", "Tags"]
        .into_iter()
        .map(|h| {
            Cell::from(h).style(
                Style::default()
                    .fg(theme.muted)
                    .add_modifier(Modifier::BOLD),
            )
        });
    let header = Row::new(header_cells).height(1).bottom_margin(1);

    let rows: Vec<Row> = data
        .machines
        .iter()
        .enumerate()
        .map(|(idx, m)| {
            let selected = idx == data.selected_index;
            let style = if selected {
                Style::default().fg(theme.text).bg(theme.bg_secondary)
            } else {
                Style::default().fg(theme.text)
            };

            let status_color = match m.status {
                MachineOnlineStatus::Online => theme.healthy,
                MachineOnlineStatus::Offline => theme.critical,
                MachineOnlineStatus::Unknown => theme.muted,
            };

            let last_seen = m
                .last_seen
                .map(|ts| format_relative_time(ts))
                .unwrap_or_else(|| "never".to_string());

            let tags = if m.tags.is_empty() {
                "-".to_string()
            } else {
                m.tags
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            };

            let local_indicator = if m.is_local { "◆" } else { " " };

            Row::new(vec![
                Cell::from(local_indicator).style(Style::default().fg(theme.accent)),
                Cell::from(m.machine_id.clone()),
                Cell::from(m.hostname.clone()),
                Cell::from(m.status.indicator()).style(Style::default().fg(status_color)),
                Cell::from(format!("{}", m.tool_count)),
                Cell::from(last_seen),
                Cell::from(tags).style(Style::default().fg(theme.muted)),
            ])
            .style(style)
        })
        .collect();

    let widths = [
        Constraint::Length(2),
        Constraint::Length(15),
        Constraint::Min(20),
        Constraint::Length(6),
        Constraint::Length(5),
        Constraint::Length(10),
        Constraint::Min(15),
    ];

    let table = Table::new(rows, widths).header(header).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.muted))
            .title(Span::styled(
                " Machine Inventory ",
                Style::default().fg(theme.text),
            )),
    );

    f.render_widget(table, area);
}

fn render_detail_view(f: &mut Frame, area: Rect, data: &MachinesData, theme: &Theme) {
    let detail = match &data.selected_detail {
        Some(d) => d,
        None => {
            let msg = Paragraph::new("No machine selected")
                .style(Style::default().fg(theme.muted))
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(msg, area);
            return;
        }
    };

    // Split into left (info + tools) and right (stats + collections)
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Left column: machine info and tools
    let left_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(8), Constraint::Min(5)])
        .split(cols[0]);

    render_machine_info(f, left_rows[0], detail, theme);
    render_tools_panel(f, left_rows[1], &detail.tools, theme);

    // Right column: stats and collections
    let right_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(8), Constraint::Min(5)])
        .split(cols[1]);

    render_system_stats(f, right_rows[0], &detail.system_stats, theme);
    render_recent_collections(f, right_rows[1], &detail.recent_collections, theme);
}

fn render_machine_info(f: &mut Frame, area: Rect, detail: &MachineDetail, theme: &Theme) {
    let m = &detail.machine;
    let status_color = match m.status {
        MachineOnlineStatus::Online => theme.healthy,
        MachineOnlineStatus::Offline => theme.critical,
        MachineOnlineStatus::Unknown => theme.muted,
    };

    let ssh_text = detail
        .ssh_target
        .as_deref()
        .unwrap_or(if m.is_local { "local" } else { "-" });

    let tags_text = if m.tags.is_empty() {
        "-".to_string()
    } else {
        format!("[{}]", m.tags.join(", "))
    };

    let lines = vec![
        Line::from(vec![
            Span::styled("ID:       ", Style::default().fg(theme.muted)),
            Span::styled(&m.machine_id, Style::default().fg(theme.text)),
        ]),
        Line::from(vec![
            Span::styled("Hostname: ", Style::default().fg(theme.muted)),
            Span::styled(&m.hostname, Style::default().fg(theme.text)),
        ]),
        Line::from(vec![
            Span::styled("Status:   ", Style::default().fg(theme.muted)),
            Span::styled(m.status.indicator(), Style::default().fg(status_color)),
            Span::raw(" "),
            Span::styled(format!("{:?}", m.status), Style::default().fg(status_color)),
        ]),
        Line::from(vec![
            Span::styled("SSH:      ", Style::default().fg(theme.muted)),
            Span::styled(ssh_text, Style::default().fg(theme.text)),
        ]),
        Line::from(vec![
            Span::styled("Tags:     ", Style::default().fg(theme.muted)),
            Span::styled(tags_text, Style::default().fg(theme.accent)),
        ]),
    ];

    let info = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.muted))
            .title(Span::styled(
                format!(" {} ", m.display_name.as_deref().unwrap_or(&m.machine_id)),
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
            )),
    );

    f.render_widget(info, area);
}

fn render_tools_panel(f: &mut Frame, area: Rect, tools: &[ToolInfoRow], theme: &Theme) {
    let items: Vec<ListItem> = if tools.is_empty() {
        vec![ListItem::new(Span::styled(
            "  No tools probed",
            Style::default().fg(theme.muted),
        ))]
    } else {
        tools
            .iter()
            .map(|t| {
                let color = if t.available {
                    theme.healthy
                } else {
                    theme.muted
                };
                let version = t.version.as_deref().unwrap_or("-");
                let indicator = if t.available { "✓" } else { "✗" };

                ListItem::new(Line::from(vec![
                    Span::styled(format!(" {} ", indicator), Style::default().fg(color)),
                    Span::styled(format!("{:<12}", t.name), Style::default().fg(theme.text)),
                    Span::styled(format!("v{}", version), Style::default().fg(theme.muted)),
                ]))
            })
            .collect()
    };

    let available_count = tools.iter().filter(|t| t.available).count();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.muted))
            .title(Span::styled(
                format!(" Tools ({}/{}) ", available_count, tools.len()),
                Style::default().fg(theme.text),
            )),
    );

    f.render_widget(list, area);
}

fn render_system_stats(f: &mut Frame, area: Rect, stats: &Option<SystemStats>, theme: &Theme) {
    let lines = match stats {
        Some(s) => {
            let cpu_bar = render_bar(s.cpu_pct, 10, theme);
            let mem_bar = render_bar(s.mem_pct, 10, theme);
            let disk_bar = render_bar(s.disk_pct, 10, theme);

            vec![
                Line::from(vec![
                    Span::styled("CPU:  ", Style::default().fg(theme.muted)),
                    cpu_bar,
                    Span::styled(
                        format!(" {:>5.1}%", s.cpu_pct),
                        Style::default().fg(theme.text),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("MEM:  ", Style::default().fg(theme.muted)),
                    mem_bar,
                    Span::styled(
                        format!(" {:>5.1}%", s.mem_pct),
                        Style::default().fg(theme.text),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("DISK: ", Style::default().fg(theme.muted)),
                    disk_bar,
                    Span::styled(
                        format!(" {:>5.1}%", s.disk_pct),
                        Style::default().fg(theme.text),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Load: ", Style::default().fg(theme.muted)),
                    Span::styled(format!("{:.2}", s.load1), Style::default().fg(theme.text)),
                ]),
                Line::from(vec![
                    Span::styled("Up:   ", Style::default().fg(theme.muted)),
                    Span::styled(
                        format_uptime(s.uptime_secs),
                        Style::default().fg(theme.text),
                    ),
                ]),
            ]
        }
        None => vec![Line::from(Span::styled(
            "  System stats unavailable",
            Style::default().fg(theme.muted),
        ))],
    };

    let panel = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.muted))
            .title(Span::styled(
                " System Stats ",
                Style::default().fg(theme.text),
            )),
    );

    f.render_widget(panel, area);
}

fn render_recent_collections(f: &mut Frame, area: Rect, events: &[CollectionEvent], theme: &Theme) {
    let items: Vec<ListItem> = if events.is_empty() {
        vec![ListItem::new(Span::styled(
            "  No recent collections",
            Style::default().fg(theme.muted),
        ))]
    } else {
        events
            .iter()
            .take(10)
            .map(|e| {
                let color = if e.success {
                    theme.healthy
                } else {
                    theme.critical
                };
                let indicator = if e.success { "✓" } else { "✗" };
                let time_ago = format_relative_time(e.collected_at);

                ListItem::new(Line::from(vec![
                    Span::styled(format!(" {} ", indicator), Style::default().fg(color)),
                    Span::styled(
                        format!("{:<10}", e.collector),
                        Style::default().fg(theme.text),
                    ),
                    Span::styled(format!("{:<8}", time_ago), Style::default().fg(theme.muted)),
                    Span::styled(
                        format!("{:>5} rows", e.record_count),
                        Style::default().fg(theme.text),
                    ),
                    Span::styled(
                        format!(" {:>5}ms", e.duration_ms),
                        Style::default().fg(theme.muted),
                    ),
                ]))
            })
            .collect()
    };

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.muted))
            .title(Span::styled(
                " Recent Collections ",
                Style::default().fg(theme.text),
            )),
    );

    f.render_widget(list, area);
}

fn render_compare_view(f: &mut Frame, area: Rect, _data: &MachinesData, theme: &Theme) {
    // Placeholder for comparison view
    let msg = Paragraph::new(vec![
        Line::from("Cross-machine comparison view"),
        Line::from(""),
        Line::from("Select multiple machines with Space"),
        Line::from("Press Enter to compare"),
    ])
    .style(Style::default().fg(theme.muted))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.muted))
            .title(Span::styled(
                " Compare Machines ",
                Style::default().fg(theme.text),
            )),
    );

    f.render_widget(msg, area);
}

fn render_footer(f: &mut Frame, area: Rect, data: &MachinesData, theme: &Theme) {
    let help_text = match data.view_mode {
        MachinesViewMode::List => "↑↓ Navigate  Enter Detail  p Probe  t Filter Tags  q Back",
        MachinesViewMode::Detail => "Esc Back  p Probe  r Refresh  c Compare",
        MachinesViewMode::Compare => "Space Select  Enter Compare  Esc Back",
    };

    let footer = Paragraph::new(Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(help_text, Style::default().fg(theme.muted)),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.muted)),
    );

    f.render_widget(footer, area);
}

/// Render a progress bar
fn render_bar(pct: f64, width: usize, theme: &Theme) -> Span<'static> {
    let filled = ((pct / 100.0) * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);

    let color = if pct >= 90.0 {
        theme.critical
    } else if pct >= 70.0 {
        theme.warning
    } else {
        theme.healthy
    };

    let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
    Span::styled(bar, Style::default().fg(color))
}

/// Format relative time
fn format_relative_time(ts: DateTime<Utc>) -> String {
    let now = Utc::now();
    let diff = now.signed_duration_since(ts);

    if diff.num_seconds() < 60 {
        "just now".to_string()
    } else if diff.num_minutes() < 60 {
        format!("{}m ago", diff.num_minutes())
    } else if diff.num_hours() < 24 {
        format!("{}h ago", diff.num_hours())
    } else {
        format!("{}d ago", diff.num_days())
    }
}

/// Format uptime duration
fn format_uptime(secs: i64) -> String {
    if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d {}h", secs / 86400, (secs % 86400) / 3600)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_machines_data_default() {
        let data = MachinesData::default();
        assert_eq!(data.view_mode, MachinesViewMode::List);
        assert!(data.machines.is_empty());
        assert_eq!(data.selected_index, 0);
    }

    #[test]
    fn test_machine_online_status_indicator() {
        assert_eq!(MachineOnlineStatus::Online.indicator(), "●");
        assert_eq!(MachineOnlineStatus::Offline.indicator(), "○");
        assert_eq!(MachineOnlineStatus::Unknown.indicator(), "◌");
    }

    #[test]
    fn test_format_relative_time() {
        let now = Utc::now();
        assert_eq!(format_relative_time(now), "just now");

        let five_min_ago = now - chrono::Duration::minutes(5);
        assert_eq!(format_relative_time(five_min_ago), "5m ago");

        let two_hours_ago = now - chrono::Duration::hours(2);
        assert_eq!(format_relative_time(two_hours_ago), "2h ago");

        let three_days_ago = now - chrono::Duration::days(3);
        assert_eq!(format_relative_time(three_days_ago), "3d ago");
    }

    #[test]
    fn test_format_uptime() {
        assert_eq!(format_uptime(300), "5m");
        assert_eq!(format_uptime(7200), "2h 0m");
        assert_eq!(format_uptime(90000), "1d 1h");
    }

    #[test]
    fn test_machine_row_default() {
        let row = MachineRow::default();
        assert!(row.machine_id.is_empty());
        assert_eq!(row.status, MachineOnlineStatus::Unknown);
        assert_eq!(row.tool_count, 0);
        assert!(!row.is_local);
    }

    #[test]
    fn test_tool_info_row() {
        let tool = ToolInfoRow {
            name: "caut".to_string(),
            path: Some("/usr/local/bin/caut".to_string()),
            version: Some("0.3.2".to_string()),
            available: true,
        };

        assert!(tool.available);
        assert_eq!(tool.version, Some("0.3.2".to_string()));
    }

    #[test]
    fn test_system_stats_default() {
        let stats = SystemStats::default();
        assert_eq!(stats.cpu_pct, 0.0);
        assert_eq!(stats.mem_pct, 0.0);
        assert_eq!(stats.uptime_secs, 0);
    }
}
