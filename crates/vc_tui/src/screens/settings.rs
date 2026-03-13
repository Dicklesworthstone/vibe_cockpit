//! Settings screen implementation.
//!
//! Displays a compact view of configuration defaults or an injected config snapshot.

use ftui::{
    Frame as FtuiFrame, PackedRgba, Style as FtuiStyle,
    layout::{Constraint as FtuiConstraint, Flex, Rect as FtuiRect},
    text::{Line as FtuiLine, Span as FtuiSpan, Text as FtuiText},
    widgets::{
        Widget as FtuiWidget, block::Block as FtuiBlock, borders::Borders as FtuiBorders,
        paragraph::Paragraph as FtuiParagraph,
    },
};
use vc_config::VcConfig;

use crate::theme::Theme;

/// Data needed to render the settings screen.
#[derive(Debug, Clone)]
pub struct SettingsData {
    /// Where the current configuration came from.
    pub config_source: String,
    /// Candidate configuration paths in precedence order.
    pub config_paths: Vec<String>,
    /// Database file path.
    pub db_path: String,
    /// Runtime-related settings.
    pub runtime: RuntimeSettings,
    /// Collector and automation settings.
    pub fleet: FleetSettings,
    /// Web dashboard settings.
    pub web: WebSettings,
    /// Config lint summary.
    pub lint: LintSummary,
}

#[derive(Debug, Clone)]
pub struct RuntimeSettings {
    /// Poll interval in seconds.
    pub poll_interval_secs: u64,
    /// Collector timeout in seconds.
    pub collector_timeout_secs: u64,
    /// TUI theme name.
    pub theme_name: String,
    /// Inline mode enabled.
    pub inline_mode: bool,
    /// Inline height in rows.
    pub inline_height: u16,
}

#[derive(Debug, Clone)]
pub struct FleetSettings {
    /// Number of enabled collectors.
    pub enabled_collectors: usize,
    /// Number of enabled machines.
    pub enabled_machines: usize,
    /// Alerting enabled.
    pub alerting_enabled: bool,
    /// Autopilot enabled.
    pub autopilot_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct WebSettings {
    /// Web dashboard enabled.
    pub enabled: bool,
    /// Web bind address.
    pub bind: String,
    /// Web port.
    pub port: u16,
    /// CORS enabled.
    pub cors_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct LintSummary {
    /// Config lint error count.
    pub errors: usize,
    /// Config lint warning count.
    pub warnings: usize,
    /// Config lint info count.
    pub info: usize,
}

impl Default for SettingsData {
    fn default() -> Self {
        Self::from_config(&VcConfig::default(), "defaults")
    }
}

impl SettingsData {
    #[must_use]
    pub fn from_config(config: &VcConfig, config_source: impl Into<String>) -> Self {
        let lint = config.lint();
        Self {
            config_source: config_source.into(),
            config_paths: VcConfig::config_paths()
                .into_iter()
                .map(|path| path.display().to_string())
                .collect(),
            db_path: config.global.db_path.display().to_string(),
            runtime: RuntimeSettings {
                poll_interval_secs: config.global.poll_interval_secs,
                collector_timeout_secs: config.collectors.timeout_secs,
                theme_name: config.tui.theme.clone(),
                inline_mode: config.tui.inline_mode,
                inline_height: config.tui.inline_height,
            },
            fleet: FleetSettings {
                enabled_collectors: enabled_collectors(config),
                enabled_machines: config.enabled_machines().count(),
                alerting_enabled: config.alerts.enabled,
                autopilot_enabled: config.autopilot.enabled,
            },
            web: WebSettings {
                enabled: config.web.enabled,
                bind: config.web.bind_address.clone(),
                port: config.web.port,
                cors_enabled: config.web.cors_enabled,
            },
            lint: LintSummary {
                errors: lint.error_count,
                warnings: lint.warning_count,
                info: lint.info_count,
            },
        }
    }
}

pub fn render_settings_ftui(f: &mut FtuiFrame, data: &SettingsData, theme: &Theme) {
    let rows = Flex::vertical()
        .constraints([
            FtuiConstraint::Fixed(3),
            FtuiConstraint::Fill,
            FtuiConstraint::Fixed(3),
        ])
        .gap(1)
        .split(ftui_full_area(f));

    if rows.len() < 3 {
        return;
    }

    render_settings_ftui_header(f, rows[0], data, theme);
    render_settings_ftui_body(f, rows[1], data, theme);
    render_settings_ftui_footer(f, rows[2], theme);
}

fn render_settings_ftui_header(
    f: &mut FtuiFrame,
    area: FtuiRect,
    data: &SettingsData,
    theme: &Theme,
) {
    let colors = theme.ftui_colors();
    let header = FtuiParagraph::new(FtuiText::from_spans([
        FtuiSpan::styled(
            "  SETTINGS  ",
            FtuiStyle::new().fg(packed(colors.text)).bold(),
        ),
        FtuiSpan::styled("Config snapshot", FtuiStyle::new().fg(packed(colors.muted))),
        FtuiSpan::raw(" "),
        FtuiSpan::styled(
            format!("[source: {}]", data.config_source),
            FtuiStyle::new().fg(packed(colors.info)),
        ),
        FtuiSpan::raw(" "),
        FtuiSpan::styled(
            format!("[{} collectors]", data.fleet.enabled_collectors),
            FtuiStyle::new().fg(packed(colors.warning)),
        ),
        FtuiSpan::raw(" "),
        FtuiSpan::styled(
            format!("[{} machines]", data.fleet.enabled_machines),
            FtuiStyle::new().fg(packed(colors.muted)),
        ),
    ]))
    .style(FtuiStyle::new().bg(packed(colors.bg_secondary)))
    .block(ftui_block(None, colors.muted));
    FtuiWidget::render(&header, area, f);
}

fn render_settings_ftui_body(
    f: &mut FtuiFrame,
    area: FtuiRect,
    data: &SettingsData,
    theme: &Theme,
) {
    let rows = Flex::vertical()
        .constraints([
            FtuiConstraint::Percentage(50.0),
            FtuiConstraint::Percentage(50.0),
        ])
        .gap(1)
        .split(area);

    if rows.len() < 2 {
        return;
    }

    let top = Flex::horizontal()
        .constraints([
            FtuiConstraint::Percentage(50.0),
            FtuiConstraint::Percentage(50.0),
        ])
        .gap(1)
        .split(rows[0]);
    let bottom = Flex::horizontal()
        .constraints([
            FtuiConstraint::Percentage(50.0),
            FtuiConstraint::Percentage(50.0),
        ])
        .gap(1)
        .split(rows[1]);

    if top.len() < 2 || bottom.len() < 2 {
        return;
    }

    render_settings_ftui_paths(f, top[0], data, theme);
    render_settings_ftui_runtime(f, top[1], data, theme);
    render_settings_ftui_collectors(f, bottom[0], data, theme);
    render_settings_ftui_web(f, bottom[1], data, theme);
}

fn render_settings_ftui_paths(
    f: &mut FtuiFrame,
    area: FtuiRect,
    data: &SettingsData,
    theme: &Theme,
) {
    let colors = theme.ftui_colors();
    let mut lines = vec![FtuiLine::from_spans([
        FtuiSpan::styled("DB: ", FtuiStyle::new().fg(packed(colors.muted))),
        FtuiSpan::styled(&data.db_path, FtuiStyle::new().fg(packed(colors.text))),
    ])];
    lines.extend(
        data.config_paths
            .iter()
            .take(3)
            .enumerate()
            .map(|(index, path)| {
                FtuiLine::from_spans([
                    FtuiSpan::styled(
                        format!("Path {}: ", index + 1),
                        FtuiStyle::new().fg(packed(colors.muted)),
                    ),
                    FtuiSpan::styled(path, FtuiStyle::new().fg(packed(colors.info))),
                ])
            }),
    );

    let panel = FtuiParagraph::new(FtuiText::from_lines(lines))
        .block(ftui_block(Some(" Paths "), colors.muted));
    FtuiWidget::render(&panel, area, f);
}

fn render_settings_ftui_runtime(
    f: &mut FtuiFrame,
    area: FtuiRect,
    data: &SettingsData,
    theme: &Theme,
) {
    let colors = theme.ftui_colors();
    let lines = vec![
        FtuiLine::from_spans([
            FtuiSpan::styled("Poll: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                format!("{}s", data.runtime.poll_interval_secs),
                FtuiStyle::new().fg(packed(colors.text)),
            ),
            FtuiSpan::raw("  "),
            FtuiSpan::styled("Timeout: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                format!("{}s", data.runtime.collector_timeout_secs),
                FtuiStyle::new().fg(packed(colors.warning)),
            ),
        ]),
        FtuiLine::from_spans([
            FtuiSpan::styled("Theme: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                &data.runtime.theme_name,
                FtuiStyle::new().fg(packed(colors.text)),
            ),
        ]),
        FtuiLine::from_spans([
            FtuiSpan::styled("Inline: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                if data.runtime.inline_mode {
                    "enabled"
                } else {
                    "disabled"
                },
                FtuiStyle::new().fg(packed(if data.runtime.inline_mode {
                    colors.info
                } else {
                    colors.muted
                })),
            ),
            FtuiSpan::raw("  "),
            FtuiSpan::styled("Height: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                data.runtime.inline_height.to_string(),
                FtuiStyle::new().fg(packed(colors.text)),
            ),
        ]),
    ];

    let panel = FtuiParagraph::new(FtuiText::from_lines(lines))
        .block(ftui_block(Some(" Runtime "), colors.muted));
    FtuiWidget::render(&panel, area, f);
}

fn render_settings_ftui_collectors(
    f: &mut FtuiFrame,
    area: FtuiRect,
    data: &SettingsData,
    theme: &Theme,
) {
    let colors = theme.ftui_colors();
    let lines = vec![
        FtuiLine::from_spans([
            FtuiSpan::styled("Collectors: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                data.fleet.enabled_collectors.to_string(),
                FtuiStyle::new().fg(packed(colors.text)),
            ),
        ]),
        FtuiLine::from_spans([
            FtuiSpan::styled("Machines: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                data.fleet.enabled_machines.to_string(),
                FtuiStyle::new().fg(packed(colors.info)),
            ),
        ]),
        FtuiLine::from_spans([
            FtuiSpan::styled("Alerting: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                if data.fleet.alerting_enabled {
                    "on"
                } else {
                    "off"
                },
                FtuiStyle::new().fg(packed(if data.fleet.alerting_enabled {
                    colors.healthy
                } else {
                    colors.muted
                })),
            ),
            FtuiSpan::raw("  "),
            FtuiSpan::styled("Autopilot: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                if data.fleet.autopilot_enabled {
                    "on"
                } else {
                    "off"
                },
                FtuiStyle::new().fg(packed(if data.fleet.autopilot_enabled {
                    colors.warning
                } else {
                    colors.muted
                })),
            ),
        ]),
    ];

    let panel = FtuiParagraph::new(FtuiText::from_lines(lines))
        .block(ftui_block(Some(" Collectors "), colors.muted));
    FtuiWidget::render(&panel, area, f);
}

fn render_settings_ftui_web(f: &mut FtuiFrame, area: FtuiRect, data: &SettingsData, theme: &Theme) {
    let colors = theme.ftui_colors();
    let lint_color = if data.lint.errors > 0 {
        colors.critical
    } else if data.lint.warnings > 0 {
        colors.warning
    } else {
        colors.healthy
    };
    let lines = vec![
        FtuiLine::from_spans([
            FtuiSpan::styled("Web: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                if data.web.enabled {
                    "enabled"
                } else {
                    "disabled"
                },
                FtuiStyle::new().fg(packed(if data.web.enabled {
                    colors.healthy
                } else {
                    colors.muted
                })),
            ),
            FtuiSpan::raw("  "),
            FtuiSpan::styled("CORS: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                if data.web.cors_enabled { "on" } else { "off" },
                FtuiStyle::new().fg(packed(if data.web.cors_enabled {
                    colors.warning
                } else {
                    colors.muted
                })),
            ),
        ]),
        FtuiLine::from_spans([
            FtuiSpan::styled("Bind: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                format!("{}:{}", data.web.bind, data.web.port),
                FtuiStyle::new().fg(packed(colors.info)),
            ),
        ]),
        FtuiLine::from_spans([
            FtuiSpan::styled("Lint: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                format!(
                    "{} error / {} warning / {} info",
                    data.lint.errors, data.lint.warnings, data.lint.info
                ),
                FtuiStyle::new().fg(packed(lint_color)),
            ),
        ]),
    ];

    let panel = FtuiParagraph::new(FtuiText::from_lines(lines))
        .block(ftui_block(Some(" Web + Lint "), colors.muted));
    FtuiWidget::render(&panel, area, f);
}

fn render_settings_ftui_footer(f: &mut FtuiFrame, area: FtuiRect, theme: &Theme) {
    let colors = theme.ftui_colors();
    let footer = FtuiParagraph::new(FtuiText::from_spans([
        FtuiSpan::styled(
            "Read-only snapshot",
            FtuiStyle::new().fg(packed(colors.muted)),
        ),
        FtuiSpan::raw("  "),
        FtuiSpan::styled(
            "Future runtime wiring can replace defaults with discovered config",
            FtuiStyle::new().fg(packed(colors.info)),
        ),
    ]))
    .style(FtuiStyle::new().bg(packed(colors.bg_secondary)))
    .block(ftui_block(None, colors.muted));
    FtuiWidget::render(&footer, area, f);
}

fn enabled_collectors(config: &VcConfig) -> usize {
    [
        config.collectors.sysmoni,
        config.collectors.ru,
        config.collectors.caut,
        config.collectors.caam,
        config.collectors.cass,
        config.collectors.mcp_agent_mail,
        config.collectors.ntm,
        config.collectors.rch,
        config.collectors.rano,
        config.collectors.dcg,
        config.collectors.pt,
        config.collectors.bv_br,
        config.collectors.afsc,
        config.collectors.cloud_benchmarker,
    ]
    .into_iter()
    .filter(|enabled| *enabled)
    .count()
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
    fn test_settings_data_from_config() {
        let mut config = VcConfig::default();
        config.global.poll_interval_secs = 45;
        config.web.enabled = true;
        config.web.port = 9090;
        config.autopilot.enabled = true;

        let data = SettingsData::from_config(&config, "test");

        assert_eq!(data.config_source, "test");
        assert_eq!(data.runtime.poll_interval_secs, 45);
        assert!(data.web.enabled);
        assert_eq!(data.web.port, 9090);
        assert!(data.fleet.autopilot_enabled);
        assert!(!data.config_paths.is_empty());
    }

    #[test]
    fn test_render_settings_ftui_renders_config_values() {
        let mut config = VcConfig::default();
        config.global.poll_interval_secs = 30;
        config.tui.theme = "ember".to_string();
        config.web.enabled = true;
        config.web.bind_address = "0.0.0.0".to_string();
        config.web.port = 8088;
        let data = SettingsData::from_config(&config, "/tmp/vc.toml");
        let theme = Theme::default();
        let mut pool = GraphemePool::new();
        let mut frame = FtuiFrame::new(120, 28, &mut pool);

        render_settings_ftui(&mut frame, &data, &theme);

        assert!(buffer_contains(&frame.buffer, 120, 28, "SETTINGS"));
        assert!(buffer_contains(&frame.buffer, 120, 28, "/tmp/vc.toml"));
        assert!(buffer_contains(&frame.buffer, 120, 28, "0.0.0.0:8088"));
        assert!(buffer_contains(&frame.buffer, 120, 28, "ember"));
    }

    #[test]
    fn test_render_settings_ftui_renders_defaults() {
        let data = SettingsData::default();
        let theme = Theme::default();
        let mut pool = GraphemePool::new();
        let mut frame = FtuiFrame::new(100, 24, &mut pool);

        render_settings_ftui(&mut frame, &data, &theme);

        assert!(buffer_contains(&frame.buffer, 100, 24, "SETTINGS"));
        assert!(buffer_contains(&frame.buffer, 100, 24, "defaults"));
        assert!(buffer_contains(
            &frame.buffer,
            100,
            24,
            "Read-only snapshot"
        ));
    }
}
