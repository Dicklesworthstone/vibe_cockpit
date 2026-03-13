//! RCH (Remote Compilation Helper) screen implementation
//!
//! Displays worker status, recent builds, cache metrics, and slowest crates.

use crate::theme::Theme;
use ftui::{
    Frame as FtuiFrame, PackedRgba, Style as FtuiStyle,
    layout::{Constraint as FtuiConstraint, Flex, Rect as FtuiRect},
    text::{Line as FtuiLine, Span as FtuiSpan, Text as FtuiText},
    widgets::{
        Widget as FtuiWidget,
        block::Block as FtuiBlock,
        borders::Borders as FtuiBorders,
        paragraph::Paragraph as FtuiParagraph,
        table::{Row as FtuiRow, Table as FtuiTable},
    },
};

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
    #[must_use]
    pub fn next(&self) -> Self {
        match self {
            Self::Workers => Self::Builds,
            Self::Builds => Self::Crates,
            Self::Crates => Self::Cache,
            Self::Cache => Self::Workers,
        }
    }

    #[must_use]
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
    #[must_use]
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Idle => "🟢",
            Self::Building => "🔵",
            Self::Offline => "🔴",
        }
    }

    #[must_use]
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
    #[must_use]
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
pub fn render_rch_ftui(f: &mut FtuiFrame, data: &RchData, theme: &Theme) {
    let rows = Flex::vertical()
        .constraints([
            FtuiConstraint::Fixed(3),
            FtuiConstraint::Fixed(7),
            FtuiConstraint::Fill,
            FtuiConstraint::Fixed(7),
            FtuiConstraint::Fixed(3),
        ])
        .gap(1)
        .split(ftui_full_area(f));

    if rows.len() < 5 {
        return;
    }

    render_rch_ftui_header(f, rows[0], data, theme);
    render_rch_ftui_workers(f, rows[1], data, theme);
    render_rch_ftui_builds(f, rows[2], data, theme);
    render_rch_ftui_slowest_crates(f, rows[3], data, theme);
    render_rch_ftui_footer(f, rows[4], data, theme);
}

fn render_rch_ftui_header(f: &mut FtuiFrame, area: FtuiRect, data: &RchData, theme: &Theme) {
    let colors = theme.ftui_colors();
    let online_count = data
        .workers
        .iter()
        .filter(|worker| worker.state != WorkerState::Offline)
        .count();
    let building_count = data
        .workers
        .iter()
        .filter(|worker| worker.state == WorkerState::Building)
        .count();
    let spans = vec![
        FtuiSpan::styled("  RCH  ", FtuiStyle::new().fg(packed(colors.text)).bold()),
        FtuiSpan::styled(
            "Remote Compilation",
            FtuiStyle::new().fg(packed(colors.muted)),
        ),
        FtuiSpan::raw(" "),
        FtuiSpan::styled(
            format!("[{online_count}/{} online]", data.workers.len()),
            FtuiStyle::new().fg(packed(if online_count == data.workers.len() {
                colors.healthy
            } else {
                colors.warning
            })),
        ),
        FtuiSpan::raw(" "),
        FtuiSpan::styled(
            format!("[{building_count} building]"),
            FtuiStyle::new().fg(packed(colors.info)),
        ),
        FtuiSpan::raw(" "),
        FtuiSpan::styled(
            format!("[{} builds/24h]", data.builds_24h),
            FtuiStyle::new().fg(packed(colors.muted)),
        ),
    ];

    let header = FtuiParagraph::new(FtuiText::from_spans(spans))
        .style(FtuiStyle::new().bg(packed(colors.bg_secondary)))
        .block(ftui_block(None, colors.muted));
    FtuiWidget::render(&header, area, f);
}

fn render_rch_ftui_workers(f: &mut FtuiFrame, area: FtuiRect, data: &RchData, theme: &Theme) {
    let colors = theme.ftui_colors();
    let border_color = if data.selected_section == RchSection::Workers {
        colors.accent
    } else {
        colors.muted
    };

    if data.workers.is_empty() {
        let empty = FtuiParagraph::new(FtuiText::from_spans([FtuiSpan::styled(
            "No workers configured",
            FtuiStyle::new().fg(packed(colors.muted)),
        )]))
        .block(ftui_block(Some(" Worker Status "), border_color));
        FtuiWidget::render(&empty, area, f);
        return;
    }

    let clamped_selected = data
        .selected_index
        .min(data.workers.len().saturating_sub(1));
    let header = FtuiRow::new([
        FtuiText::from_spans([FtuiSpan::styled("Worker", FtuiStyle::new().bold())]),
        FtuiText::from_spans([FtuiSpan::styled("State", FtuiStyle::new().bold())]),
        FtuiText::from_spans([FtuiSpan::styled("Current", FtuiStyle::new().bold())]),
        FtuiText::from_spans([FtuiSpan::styled("Jobs", FtuiStyle::new().bold())]),
        FtuiText::from_spans([FtuiSpan::styled("Last seen", FtuiStyle::new().bold())]),
    ])
    .style(FtuiStyle::new().fg(packed(colors.muted)))
    .bottom_margin(1);

    let rows: Vec<FtuiRow> = data
        .workers
        .iter()
        .enumerate()
        .map(|(index, worker)| {
            let row_style =
                if data.selected_section == RchSection::Workers && index == clamped_selected {
                    FtuiStyle::new().bg(packed(colors.bg_secondary))
                } else {
                    FtuiStyle::new()
                };

            FtuiRow::new([
                FtuiText::from_spans([FtuiSpan::styled(
                    &worker.name,
                    FtuiStyle::new().fg(packed(colors.text)),
                )]),
                FtuiText::from_spans([FtuiSpan::styled(
                    worker.state.label(),
                    FtuiStyle::new()
                        .fg(packed(worker_state_color(worker.state, theme)))
                        .bold(),
                )]),
                FtuiText::from_spans([FtuiSpan::styled(
                    worker.current_crate.as_deref().unwrap_or("idle"),
                    FtuiStyle::new().fg(packed(colors.info)),
                )]),
                FtuiText::from_spans([FtuiSpan::styled(
                    worker.jobs_24h.to_string(),
                    FtuiStyle::new().fg(packed(colors.text)),
                )]),
                FtuiText::from_spans([FtuiSpan::styled(
                    worker.last_seen.as_deref().unwrap_or("n/a"),
                    FtuiStyle::new().fg(packed(colors.muted)),
                )]),
            ])
            .style(row_style)
        })
        .collect();

    let table = FtuiTable::new(
        rows,
        [
            FtuiConstraint::Fixed(16),
            FtuiConstraint::Fixed(10),
            FtuiConstraint::Fixed(24),
            FtuiConstraint::Fixed(8),
            FtuiConstraint::Min(12),
        ],
    )
    .header(header)
    .column_spacing(1)
    .block(ftui_block(Some(" Worker Status "), border_color));
    FtuiWidget::render(&table, area, f);
}

fn render_rch_ftui_builds(f: &mut FtuiFrame, area: FtuiRect, data: &RchData, theme: &Theme) {
    let colors = theme.ftui_colors();
    let border_color = if data.selected_section == RchSection::Builds {
        colors.accent
    } else {
        colors.muted
    };

    if data.recent_builds.is_empty() {
        let empty = FtuiParagraph::new(FtuiText::from_spans([FtuiSpan::styled(
            "No recent builds",
            FtuiStyle::new().fg(packed(colors.muted)),
        )]))
        .block(ftui_block(Some(" Recent Builds "), border_color));
        FtuiWidget::render(&empty, area, f);
        return;
    }

    let clamped_selected = data
        .selected_index
        .min(data.recent_builds.len().saturating_sub(1));
    let header = FtuiRow::new([
        FtuiText::from_spans([FtuiSpan::styled("Time", FtuiStyle::new().bold())]),
        FtuiText::from_spans([FtuiSpan::styled("Crate", FtuiStyle::new().bold())]),
        FtuiText::from_spans([FtuiSpan::styled("Worker", FtuiStyle::new().bold())]),
        FtuiText::from_spans([FtuiSpan::styled("Duration", FtuiStyle::new().bold())]),
        FtuiText::from_spans([FtuiSpan::styled("Cache", FtuiStyle::new().bold())]),
        FtuiText::from_spans([FtuiSpan::styled("Result", FtuiStyle::new().bold())]),
    ])
    .style(FtuiStyle::new().fg(packed(colors.muted)))
    .bottom_margin(1);

    let rows: Vec<FtuiRow> = data
        .recent_builds
        .iter()
        .enumerate()
        .map(|(index, build)| {
            let row_style =
                if data.selected_section == RchSection::Builds && index == clamped_selected {
                    FtuiStyle::new().bg(packed(colors.bg_secondary))
                } else {
                    FtuiStyle::new()
                };

            FtuiRow::new([
                FtuiText::from_spans([FtuiSpan::styled(
                    &build.time,
                    FtuiStyle::new().fg(packed(colors.text)),
                )]),
                FtuiText::from_spans([FtuiSpan::styled(
                    truncate_chars(&build.crate_name, 26),
                    FtuiStyle::new().fg(packed(colors.text)),
                )]),
                FtuiText::from_spans([FtuiSpan::styled(
                    &build.worker,
                    FtuiStyle::new().fg(packed(colors.info)),
                )]),
                FtuiText::from_spans([FtuiSpan::styled(
                    format!("{:.1}s", build.duration_secs),
                    FtuiStyle::new().fg(packed(colors.warning)),
                )]),
                FtuiText::from_spans([FtuiSpan::styled(
                    build.cache_status.label(),
                    FtuiStyle::new()
                        .fg(packed(cache_status_color(build.cache_status, theme)))
                        .bold(),
                )]),
                FtuiText::from_spans([FtuiSpan::styled(
                    if build.success { "ok" } else { "fail" },
                    FtuiStyle::new().fg(packed(if build.success {
                        colors.healthy
                    } else {
                        colors.critical
                    })),
                )]),
            ])
            .style(row_style)
        })
        .collect();

    let table = FtuiTable::new(
        rows,
        [
            FtuiConstraint::Fixed(8),
            FtuiConstraint::Min(22),
            FtuiConstraint::Fixed(12),
            FtuiConstraint::Fixed(10),
            FtuiConstraint::Fixed(9),
            FtuiConstraint::Fixed(8),
        ],
    )
    .header(header)
    .column_spacing(1)
    .block(ftui_block(Some(" Recent Builds "), border_color));
    FtuiWidget::render(&table, area, f);
}

fn render_rch_ftui_slowest_crates(
    f: &mut FtuiFrame,
    area: FtuiRect,
    data: &RchData,
    theme: &Theme,
) {
    let colors = theme.ftui_colors();
    let border_color = if data.selected_section == RchSection::Crates {
        colors.accent
    } else {
        colors.muted
    };

    if data.slowest_crates.is_empty() {
        let empty = FtuiParagraph::new(FtuiText::from_spans([FtuiSpan::styled(
            "No build data",
            FtuiStyle::new().fg(packed(colors.muted)),
        )]))
        .block(ftui_block(Some(" Slowest Crates "), border_color));
        FtuiWidget::render(&empty, area, f);
        return;
    }

    let clamped_selected = data
        .selected_index
        .min(data.slowest_crates.len().saturating_sub(1));
    let lines: Vec<FtuiLine> = data
        .slowest_crates
        .iter()
        .enumerate()
        .map(|(index, stats)| {
            let bar_width = (usize::from(stats.bar_pct) * 30 / 100).max(1);
            let bar = "█".repeat(bar_width);
            let is_selected =
                data.selected_section == RchSection::Crates && index == clamped_selected;
            let row_bg = if is_selected {
                Some(packed(colors.bg_secondary))
            } else {
                None
            };
            let base_style = match row_bg {
                Some(bg) => FtuiStyle::new().bg(bg),
                None => FtuiStyle::new(),
            };
            let text_style = match row_bg {
                Some(bg) => FtuiStyle::new().fg(packed(colors.text)).bg(bg),
                None => FtuiStyle::new().fg(packed(colors.text)),
            };
            let info_style = match row_bg {
                Some(bg) => FtuiStyle::new().fg(packed(colors.info)).bg(bg),
                None => FtuiStyle::new().fg(packed(colors.info)),
            };
            let warning_style = match row_bg {
                Some(bg) => FtuiStyle::new().fg(packed(colors.warning)).bg(bg),
                None => FtuiStyle::new().fg(packed(colors.warning)),
            };
            let muted_style = match row_bg {
                Some(bg) => FtuiStyle::new().fg(packed(colors.muted)).bg(bg),
                None => FtuiStyle::new().fg(packed(colors.muted)),
            };

            FtuiLine::from_spans([
                FtuiSpan::styled(
                    format!("{:<18}", truncate_chars(&stats.name, 18)),
                    text_style,
                ),
                FtuiSpan::styled(bar, info_style),
                FtuiSpan::styled(" ", base_style),
                FtuiSpan::styled(format!("{:.1}s", stats.avg_time_secs), warning_style),
                FtuiSpan::styled(" ", base_style),
                FtuiSpan::styled(format!("({} builds)", stats.build_count), muted_style),
            ])
        })
        .collect();

    let paragraph = FtuiParagraph::new(FtuiText::from_lines(lines))
        .block(ftui_block(Some(" Slowest Crates "), border_color));
    FtuiWidget::render(&paragraph, area, f);
}

fn render_rch_ftui_footer(f: &mut FtuiFrame, area: FtuiRect, data: &RchData, theme: &Theme) {
    let colors = theme.ftui_colors();
    let filled = cache_bar_fill(data.cache_hit_rate, 20);
    let empty = 20usize.saturating_sub(filled);
    let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
    let footer = FtuiParagraph::new(FtuiText::from_spans([
        FtuiSpan::styled("Cache: ", FtuiStyle::new().fg(packed(colors.muted))),
        FtuiSpan::styled(
            bar,
            FtuiStyle::new().fg(packed(cache_hit_color(data.cache_hit_rate, theme))),
        ),
        FtuiSpan::raw(" "),
        FtuiSpan::styled(
            format!("{}%", cache_percent_label(data.cache_hit_rate)),
            FtuiStyle::new().fg(packed(colors.text)).bold(),
        ),
        FtuiSpan::raw("  "),
        FtuiSpan::styled("Sections:", FtuiStyle::new().fg(packed(colors.muted))),
        FtuiSpan::raw(" workers / builds / crates / cache"),
    ]))
    .style(FtuiStyle::new().bg(packed(colors.bg_secondary)))
    .block(ftui_block(
        None,
        if data.selected_section == RchSection::Cache {
            colors.accent
        } else {
            colors.muted
        },
    ));
    FtuiWidget::render(&footer, area, f);
}

fn cache_percent_label(rate: f64) -> String {
    format!("{:.0}", rate.clamp(0.0, 1.0) * 100.0)
}

fn cache_bar_fill(rate: f64, slots: usize) -> usize {
    match slots {
        20 => {
            const THRESHOLDS: [f64; 20] = [
                0.05, 0.10, 0.15, 0.20, 0.25, 0.30, 0.35, 0.40, 0.45, 0.50, 0.55, 0.60, 0.65, 0.70,
                0.75, 0.80, 0.85, 0.90, 0.95, 1.0,
            ];
            let clamped = rate.clamp(0.0, 1.0);
            THRESHOLDS
                .iter()
                .take_while(|threshold| clamped >= **threshold)
                .count()
        }
        _ => 0,
    }
}

fn worker_state_color(state: WorkerState, theme: &Theme) -> ftui::Color {
    match state {
        WorkerState::Idle => theme.ftui_colors().healthy,
        WorkerState::Building => theme.ftui_colors().info,
        WorkerState::Offline => theme.ftui_colors().critical,
    }
}

fn cache_status_color(status: CacheStatus, theme: &Theme) -> ftui::Color {
    match status {
        CacheStatus::Hit => theme.ftui_colors().healthy,
        CacheStatus::Miss => theme.ftui_colors().warning,
        CacheStatus::Partial => theme.ftui_colors().info,
    }
}

fn cache_hit_color(rate: f64, theme: &Theme) -> ftui::Color {
    if rate >= 0.7 {
        theme.ftui_colors().healthy
    } else if rate >= 0.4 {
        theme.ftui_colors().warning
    } else {
        theme.ftui_colors().critical
    }
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_chars {
        value.to_string()
    } else {
        let truncated: String = value.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{truncated}…")
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
        assert!(data.cache_hit_rate.abs() < f64::EPSILON);
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
        assert!(stats.avg_time_secs.abs() < f64::EPSILON);
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

    #[test]
    fn test_render_rch_ftui_renders_tables() {
        let data = RchData {
            workers: vec![
                WorkerStatus {
                    name: "vmi1149989".to_string(),
                    state: WorkerState::Building,
                    current_crate: Some("vc_tui".to_string()),
                    jobs_24h: 42,
                    avg_build_time: 18.4,
                    last_seen: Some("just now".to_string()),
                },
                WorkerStatus {
                    name: "vmi1150001".to_string(),
                    state: WorkerState::Idle,
                    current_crate: None,
                    jobs_24h: 37,
                    avg_build_time: 12.0,
                    last_seen: Some("30s ago".to_string()),
                },
            ],
            recent_builds: vec![RchBuild {
                time: "10:15".to_string(),
                crate_name: "vc_tui".to_string(),
                worker: "vmi1149989".to_string(),
                duration_secs: 11.6,
                cache_status: CacheStatus::Hit,
                success: true,
            }],
            slowest_crates: vec![CrateStats {
                name: "duckdb".to_string(),
                avg_time_secs: 95.4,
                build_count: 6,
                bar_pct: 100,
            }],
            cache_hit_rate: 0.82,
            builds_24h: 79,
            selected_section: RchSection::Builds,
            selected_index: 0,
        };
        let theme = Theme::default();
        let mut pool = GraphemePool::new();
        let mut frame = FtuiFrame::new(120, 28, &mut pool);

        render_rch_ftui(&mut frame, &data, &theme);

        assert!(buffer_contains(&frame.buffer, 120, 28, "RCH"));
        assert!(buffer_contains(&frame.buffer, 120, 28, "vmi1149989"));
        assert!(buffer_contains(&frame.buffer, 120, 28, "vc_tui"));
        assert!(buffer_contains(&frame.buffer, 120, 28, "duckdb"));
    }

    #[test]
    fn test_render_rch_ftui_renders_empty_state() {
        let data = RchData::default();
        let theme = Theme::default();
        let mut pool = GraphemePool::new();
        let mut frame = FtuiFrame::new(100, 24, &mut pool);

        render_rch_ftui(&mut frame, &data, &theme);

        assert!(buffer_contains(&frame.buffer, 100, 24, "RCH"));
        assert!(buffer_contains(
            &frame.buffer,
            100,
            24,
            "No workers configured"
        ));
    }
}
