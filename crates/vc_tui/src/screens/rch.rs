//! RCH (Remote Compilation Helper) screen implementation
//!
//! Displays worker status, recent builds, cache metrics, and slowest crates.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table},
    Frame,
};

use crate::theme::Theme;

/// Data needed to render the RCH screen
#[derive(Debug, Clone, Default)]
pub struct RchData {
    /// Worker status list
    pub workers: Vec<WorkerStatus>,
    /// Recent builds
    pub recent_builds: Vec<RchBuild>,
    /// Slowest crates for visualization
    pub slowest_crates: Vec<CrateStats>,
    /// Cache hit rate (0.0 - 1.0)
    pub cache_hit_rate: f64,
    /// Total builds in last 24h
    pub builds_24h: u32,
    /// Selected section for navigation
    pub selected_section: RchSection,
    /// Selected index within section
    pub selected_index: usize,
}

/// RCH screen sections for navigation
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum RchSection {
    #[default]
    Workers,
    Builds,
    Crates,
    Cache,
}

impl RchSection {
    pub fn next(&self) -> Self {
        match self {
            Self::Workers => Self::Builds,
            Self::Builds => Self::Crates,
            Self::Crates => Self::Cache,
            Self::Cache => Self::Workers,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            Self::Workers => Self::Cache,
            Self::Builds => Self::Workers,
            Self::Crates => Self::Builds,
            Self::Cache => Self::Crates,
        }
    }
}

/// Worker status for display
#[derive(Debug, Clone)]
pub struct WorkerStatus {
    /// Worker name/hostname
    pub name: String,
    /// Current state: idle, building, offline
    pub state: WorkerState,
    /// Current crate being built (if any)
    pub current_crate: Option<String>,
    /// Jobs completed in last 24h
    pub jobs_24h: u32,
    /// Average build time in seconds
    pub avg_build_time: f64,
    /// Last seen timestamp
    pub last_seen: Option<String>,
}

/// Worker state enum
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum WorkerState {
    #[default]
    Idle,
    Building,
    Offline,
}

impl WorkerState {
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Idle => "🟢",
            Self::Building => "🔵",
            Self::Offline => "🔴",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Building => "building",
            Self::Offline => "offline",
        }
    }
}

impl Default for WorkerStatus {
    fn default() -> Self {
        Self {
            name: String::new(),
            state: WorkerState::default(),
            current_crate: None,
            jobs_24h: 0,
            avg_build_time: 0.0,
            last_seen: None,
        }
    }
}

/// Recent build information
#[derive(Debug, Clone)]
pub struct RchBuild {
    /// Build timestamp
    pub time: String,
    /// Crate name
    pub crate_name: String,
    /// Worker that built it
    pub worker: String,
    /// Build duration in seconds
    pub duration_secs: f64,
    /// Cache status: HIT, MISS, PARTIAL
    pub cache_status: CacheStatus,
    /// Build succeeded or failed
    pub success: bool,
}

impl Default for RchBuild {
    fn default() -> Self {
        Self {
            time: String::new(),
            crate_name: String::new(),
            worker: String::new(),
            duration_secs: 0.0,
            cache_status: CacheStatus::default(),
            success: true,
        }
    }
}

/// Cache status for a build
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum CacheStatus {
    Hit,
    #[default]
    Miss,
    Partial,
}

impl CacheStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Hit => "HIT",
            Self::Miss => "MISS",
            Self::Partial => "PARTIAL",
        }
    }
}

/// Crate statistics for slowest crates display
#[derive(Debug, Clone, Default)]
pub struct CrateStats {
    /// Crate name
    pub name: String,
    /// Average build time in seconds
    pub avg_time_secs: f64,
    /// Build count
    pub build_count: u32,
    /// Bar width for visualization (0-100)
    pub bar_pct: u8,
}

/// Render the RCH screen
pub fn render_rch(f: &mut Frame, data: &RchData, theme: &Theme) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Length(6), // Workers
            Constraint::Min(8),    // Recent builds
            Constraint::Length(8), // Slowest crates
            Constraint::Length(3), // Cache rate + footer
        ])
        .split(f.area());

    render_header(f, chunks[0], data, theme);
    render_workers(f, chunks[1], data, theme);
    render_builds(f, chunks[2], data, theme);
    render_slowest_crates(f, chunks[3], data, theme);
    render_footer(f, chunks[4], data, theme);
}

fn render_header(f: &mut Frame, area: Rect, data: &RchData, theme: &Theme) {
    let online_count = data
        .workers
        .iter()
        .filter(|w| w.state != WorkerState::Offline)
        .count();
    let building_count = data
        .workers
        .iter()
        .filter(|w| w.state == WorkerState::Building)
        .count();

    let title = Line::from(vec![
        Span::styled(
            "  R C H  ",
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Remote Compilation  ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("[{}/{} workers online]", online_count, data.workers.len()),
            if online_count == data.workers.len() {
                Style::default().fg(theme.healthy)
            } else {
                Style::default().fg(theme.warning)
            },
        ),
        Span::raw("  "),
        if building_count > 0 {
            Span::styled(
                format!("[{} building]", building_count),
                Style::default().fg(theme.info),
            )
        } else {
            Span::styled("[idle]", Style::default().fg(theme.muted))
        },
    ]);

    let header = Paragraph::new(title).block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(theme.border)),
    );

    f.render_widget(header, area);
}

fn render_workers(f: &mut Frame, area: Rect, data: &RchData, theme: &Theme) {
    let is_selected = data.selected_section == RchSection::Workers;

    let block = Block::default()
        .title(" Worker Status ")
        .borders(Borders::ALL)
        .border_style(if is_selected {
            Style::default().fg(theme.highlight)
        } else {
            Style::default().fg(theme.border)
        });

    if data.workers.is_empty() {
        let empty = Paragraph::new("No workers configured")
            .style(Style::default().fg(theme.muted))
            .block(block);
        f.render_widget(empty, area);
        return;
    }

    // Create 2x2 grid of workers
    let inner = block.inner(area);
    f.render_widget(block, area);

    let worker_lines: Vec<Line> = data
        .workers
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let style = if is_selected && i == data.selected_index {
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            };

            let status_span = Span::styled(format!("{} ", w.state.symbol()), style);

            let name_span = Span::styled(format!("{:<12}", w.name), style);

            let state_span = match w.state {
                WorkerState::Building => Span::styled(
                    format!("building {}", w.current_crate.as_deref().unwrap_or("...")),
                    Style::default().fg(theme.info),
                ),
                WorkerState::Idle => Span::styled("idle", Style::default().fg(theme.healthy)),
                WorkerState::Offline => {
                    Span::styled("offline", Style::default().fg(theme.critical))
                }
            };

            Line::from(vec![status_span, name_span, state_span])
        })
        .collect();

    let workers_para = Paragraph::new(worker_lines);
    f.render_widget(workers_para, inner);
}

fn render_builds(f: &mut Frame, area: Rect, data: &RchData, theme: &Theme) {
    let is_selected = data.selected_section == RchSection::Builds;

    let block = Block::default()
        .title(" Recent Builds ")
        .borders(Borders::ALL)
        .border_style(if is_selected {
            Style::default().fg(theme.highlight)
        } else {
            Style::default().fg(theme.border)
        });

    if data.recent_builds.is_empty() {
        let empty = Paragraph::new("No recent builds")
            .style(Style::default().fg(theme.muted))
            .block(block);
        f.render_widget(empty, area);
        return;
    }

    let header = Row::new(vec!["Time", "Crate", "Worker", "Duration", "Cache"]).style(
        Style::default()
            .fg(theme.muted)
            .add_modifier(Modifier::BOLD),
    );

    let rows: Vec<Row> = data
        .recent_builds
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let style = if is_selected && i == data.selected_index {
                Style::default().fg(theme.highlight)
            } else {
                Style::default().fg(theme.text)
            };

            let cache_style = match b.cache_status {
                CacheStatus::Hit => Style::default().fg(theme.healthy),
                CacheStatus::Miss => Style::default().fg(theme.warning),
                CacheStatus::Partial => Style::default().fg(theme.info),
            };

            Row::new(vec![
                b.time.clone(),
                b.crate_name.clone(),
                b.worker.clone(),
                format!("{:.1}s", b.duration_secs),
                b.cache_status.label().to_string(),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(8),
            Constraint::Min(20),
            Constraint::Length(12),
            Constraint::Length(10),
            Constraint::Length(8),
        ],
    )
    .header(header)
    .block(block);

    f.render_widget(table, area);
}

fn render_slowest_crates(f: &mut Frame, area: Rect, data: &RchData, theme: &Theme) {
    let is_selected = data.selected_section == RchSection::Crates;

    let block = Block::default()
        .title(" Slowest Crates (24h) ")
        .borders(Borders::ALL)
        .border_style(if is_selected {
            Style::default().fg(theme.highlight)
        } else {
            Style::default().fg(theme.border)
        });

    if data.slowest_crates.is_empty() {
        let empty = Paragraph::new("No build data")
            .style(Style::default().fg(theme.muted))
            .block(block);
        f.render_widget(empty, area);
        return;
    }

    let inner = block.inner(area);
    f.render_widget(block, area);

    let lines: Vec<Line> = data
        .slowest_crates
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let style = if is_selected && i == data.selected_index {
                Style::default().fg(theme.highlight)
            } else {
                Style::default().fg(theme.text)
            };

            // Create a bar chart
            let bar_width = (c.bar_pct as usize * 30 / 100).max(1);
            let bar: String = "█".repeat(bar_width);

            Line::from(vec![
                Span::styled(format!("{:<18}", c.name), style),
                Span::styled(bar, Style::default().fg(theme.info)),
                Span::styled(
                    format!(" {:.1}s", c.avg_time_secs),
                    Style::default().fg(theme.muted),
                ),
            ])
        })
        .collect();

    let crates_para = Paragraph::new(lines);
    f.render_widget(crates_para, inner);
}

fn render_footer(f: &mut Frame, area: Rect, data: &RchData, theme: &Theme) {
    let is_selected = data.selected_section == RchSection::Cache;

    // Cache hit rate bar
    let cache_pct = (data.cache_hit_rate * 100.0).round() as u8;
    let filled = (cache_pct as usize * 20 / 100).max(0);
    let empty = 20 - filled;
    let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty),);

    let cache_color = if cache_pct >= 70 {
        theme.healthy
    } else if cache_pct >= 40 {
        theme.warning
    } else {
        theme.critical
    };

    let content = Line::from(vec![
        Span::styled(" Cache Hit Rate: ", Style::default().fg(theme.muted)),
        Span::styled(
            bar,
            if is_selected {
                Style::default().fg(theme.highlight)
            } else {
                Style::default().fg(cache_color)
            },
        ),
        Span::styled(format!(" {}%", cache_pct), Style::default().fg(theme.text)),
        Span::raw("    "),
        Span::styled(
            format!("[{} builds/24h]", data.builds_24h),
            Style::default().fg(theme.muted),
        ),
        Span::raw("    "),
        Span::styled(
            "[w]orkers [b]uilds [c]rates [r]efresh",
            Style::default().fg(theme.muted),
        ),
    ]);

    let footer = Paragraph::new(content).block(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(theme.border)),
    );

    f.render_widget(footer, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_state_symbols() {
        assert_eq!(WorkerState::Idle.symbol(), "🟢");
        assert_eq!(WorkerState::Building.symbol(), "🔵");
        assert_eq!(WorkerState::Offline.symbol(), "🔴");
    }

    #[test]
    fn test_worker_state_labels() {
        assert_eq!(WorkerState::Idle.label(), "idle");
        assert_eq!(WorkerState::Building.label(), "building");
        assert_eq!(WorkerState::Offline.label(), "offline");
    }

    #[test]
    fn test_cache_status_labels() {
        assert_eq!(CacheStatus::Hit.label(), "HIT");
        assert_eq!(CacheStatus::Miss.label(), "MISS");
        assert_eq!(CacheStatus::Partial.label(), "PARTIAL");
    }

    #[test]
    fn test_rch_section_navigation() {
        assert_eq!(RchSection::Workers.next(), RchSection::Builds);
        assert_eq!(RchSection::Builds.next(), RchSection::Crates);
        assert_eq!(RchSection::Crates.next(), RchSection::Cache);
        assert_eq!(RchSection::Cache.next(), RchSection::Workers);

        assert_eq!(RchSection::Workers.prev(), RchSection::Cache);
        assert_eq!(RchSection::Cache.prev(), RchSection::Crates);
    }

    #[test]
    fn test_default_rch_data() {
        let data = RchData::default();
        assert!(data.workers.is_empty());
        assert!(data.recent_builds.is_empty());
        assert!(data.slowest_crates.is_empty());
        assert_eq!(data.cache_hit_rate, 0.0);
        assert_eq!(data.selected_section, RchSection::Workers);
    }

    #[test]
    fn test_default_worker_status() {
        let worker = WorkerStatus::default();
        assert!(worker.name.is_empty());
        assert_eq!(worker.state, WorkerState::Idle);
        assert!(worker.current_crate.is_none());
    }

    #[test]
    fn test_default_rch_build() {
        let build = RchBuild::default();
        assert!(build.crate_name.is_empty());
        assert_eq!(build.cache_status, CacheStatus::Miss);
        assert!(build.success);
    }

    #[test]
    fn test_crate_stats_default() {
        let stats = CrateStats::default();
        assert!(stats.name.is_empty());
        assert_eq!(stats.avg_time_secs, 0.0);
        assert_eq!(stats.bar_pct, 0);
    }

    #[test]
    fn test_worker_with_crate() {
        let worker = WorkerStatus {
            name: "mini-1".to_string(),
            state: WorkerState::Building,
            current_crate: Some("serde".to_string()),
            jobs_24h: 50,
            avg_build_time: 12.5,
            last_seen: Some("2026-01-28T10:00:00Z".to_string()),
        };

        assert_eq!(worker.state.symbol(), "🔵");
        assert_eq!(worker.current_crate.as_deref(), Some("serde"));
    }

    #[test]
    fn test_build_with_cache_hit() {
        let build = RchBuild {
            time: "10:05".to_string(),
            crate_name: "tokio".to_string(),
            worker: "mini-1".to_string(),
            duration_secs: 8.7,
            cache_status: CacheStatus::Hit,
            success: true,
        };

        assert_eq!(build.cache_status.label(), "HIT");
    }

    #[test]
    fn test_crate_stats_with_bar() {
        let stats = CrateStats {
            name: "rustc_codegen".to_string(),
            avg_time_secs: 45.2,
            build_count: 10,
            bar_pct: 100,
        };

        assert_eq!(stats.bar_pct, 100);
    }
}
