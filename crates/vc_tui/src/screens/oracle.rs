//! Oracle screen implementation
//!
//! Displays predictions, forecasts, and risk assessments from `vc_oracle`.

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

/// Data needed to render the oracle screen
#[derive(Debug, Clone, Default)]
pub struct OracleData {
    /// Rate limit forecasts per account
    pub rate_forecasts: Vec<RateForecast>,
    /// Failure risk assessments
    pub failure_risks: Vec<FailureRisk>,
    /// Cost trajectory data
    pub cost_trajectory: CostTrajectory,
    /// Resource forecasts
    pub resource_forecasts: Vec<ResourceForecast>,
    /// Currently selected section
    pub selected_section: OracleSection,
    /// Seconds since last refresh
    pub refresh_age_secs: u64,
}

/// Oracle screen sections
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum OracleSection {
    #[default]
    RateLimits,
    FailureRisk,
    CostTrajectory,
    Resources,
}

/// Rate limit forecast for an account
#[derive(Debug, Clone)]
pub struct RateForecast {
    /// Provider name (claude, openai, gemini)
    pub provider: String,
    /// Account identifier
    pub account: String,
    /// Current usage percentage (0-100)
    pub usage_pct: f64,
    /// Minutes until limit at current rate (None = plenty of headroom)
    pub minutes_to_limit: Option<u32>,
    /// Recommended action (if any)
    pub recommendation: Option<String>,
    /// Backup account to swap to
    pub backup_account: Option<String>,
}

impl Default for RateForecast {
    fn default() -> Self {
        Self {
            provider: String::new(),
            account: String::new(),
            usage_pct: 0.0,
            minutes_to_limit: None,
            recommendation: None,
            backup_account: None,
        }
    }
}

/// Failure risk assessment for an agent
#[derive(Debug, Clone)]
pub struct FailureRisk {
    /// Agent identifier
    pub agent_id: String,
    /// Machine hostname
    pub machine: String,
    /// Risk probability (0-100)
    pub risk_pct: f64,
    /// Time until predicted failure (minutes)
    pub minutes_to_failure: Option<u32>,
    /// Risk indicators
    pub indicators: Vec<String>,
    /// Similar past occurrences count
    pub past_occurrences: u32,
    /// Current status
    pub status: String,
}

impl Default for FailureRisk {
    fn default() -> Self {
        Self {
            agent_id: String::new(),
            machine: String::new(),
            risk_pct: 0.0,
            minutes_to_failure: None,
            indicators: vec![],
            past_occurrences: 0,
            status: "healthy".to_string(),
        }
    }
}

/// Cost trajectory information
#[derive(Debug, Clone, Default)]
pub struct CostTrajectory {
    /// Today's spend
    pub today_spent: f64,
    /// Today's budget
    pub today_budget: f64,
    /// Projected end-of-day spend
    pub today_projection: f64,
    /// This week's spend
    pub week_spent: f64,
    /// This week's budget
    pub week_budget: f64,
    /// Savings opportunity suggestion
    pub savings_suggestion: Option<String>,
    /// Is on track?
    pub on_track: bool,
}

/// Resource forecast for a machine
#[derive(Debug, Clone)]
pub struct ResourceForecast {
    /// Machine hostname
    pub machine: String,
    /// Resource type (disk, cpu, memory)
    pub resource: String,
    /// Current percentage
    pub current_pct: f64,
    /// Projected percentage
    pub projected_pct: f64,
    /// Days until projection
    pub projection_days: u32,
    /// Alert message (if critical)
    pub alert: Option<String>,
}

impl Default for ResourceForecast {
    fn default() -> Self {
        Self {
            machine: String::new(),
            resource: String::new(),
            current_pct: 0.0,
            projected_pct: 0.0,
            projection_days: 0,
            alert: None,
        }
    }
}

/// Render the oracle screen
pub fn render_oracle_ftui(f: &mut FtuiFrame, data: &OracleData, theme: &Theme) {
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

    render_oracle_ftui_header(f, rows[0], data, theme);
    render_oracle_ftui_content(f, rows[1], data, theme);
    render_oracle_ftui_footer(f, rows[2], theme);
}

fn render_oracle_ftui_header(f: &mut FtuiFrame, area: FtuiRect, data: &OracleData, theme: &Theme) {
    let colors = theme.ftui_colors();
    let refresh_text = refresh_age_label(data.refresh_age_secs);
    let at_risk_count = data
        .failure_risks
        .iter()
        .filter(|risk| risk.risk_pct > 50.0)
        .count();
    let rate_warning_count = data
        .rate_forecasts
        .iter()
        .filter(|forecast| forecast.usage_pct > 80.0)
        .count();

    let mut spans = vec![
        FtuiSpan::styled(
            "  ORACLE  ",
            FtuiStyle::new().fg(packed(colors.text)).bold(),
        ),
        FtuiSpan::styled(
            "Predictions & Forecasts",
            FtuiStyle::new().fg(packed(colors.muted)),
        ),
    ];

    if rate_warning_count > 0 {
        spans.push(FtuiSpan::raw(" "));
        spans.push(FtuiSpan::styled(
            format!("[{rate_warning_count} rate warnings]"),
            FtuiStyle::new().fg(packed(colors.warning)),
        ));
    }

    if at_risk_count > 0 {
        spans.push(FtuiSpan::raw(" "));
        spans.push(FtuiSpan::styled(
            format!("[{at_risk_count} at risk]"),
            FtuiStyle::new().fg(packed(colors.critical)),
        ));
    }

    spans.push(FtuiSpan::raw(" "));
    spans.push(FtuiSpan::styled(
        format!("[Updated: {refresh_text}]"),
        FtuiStyle::new().fg(packed(colors.info)),
    ));

    let header = FtuiParagraph::new(FtuiText::from_spans(spans))
        .style(FtuiStyle::new().bg(packed(colors.bg_secondary)))
        .block(ftui_block(None, colors.muted));

    FtuiWidget::render(&header, area, f);
}

fn render_oracle_ftui_content(f: &mut FtuiFrame, area: FtuiRect, data: &OracleData, theme: &Theme) {
    if area.width < 96 || area.height < 18 {
        let stacks = Flex::vertical()
            .constraints([
                FtuiConstraint::Fill,
                FtuiConstraint::Fill,
                FtuiConstraint::Fill,
                FtuiConstraint::Fill,
            ])
            .split(area);
        if stacks.len() < 4 {
            return;
        }

        render_oracle_ftui_rate_forecasts(f, stacks[0], data, theme);
        render_oracle_ftui_failure_risks(f, stacks[1], data, theme);
        render_oracle_ftui_cost_trajectory(f, stacks[2], data, theme);
        render_oracle_ftui_resource_forecasts(f, stacks[3], data, theme);
        return;
    }

    let row_chunks = Flex::vertical()
        .constraints([FtuiConstraint::Fill, FtuiConstraint::Fill])
        .split(area);
    if row_chunks.len() < 2 {
        return;
    }

    let top_cols = Flex::horizontal()
        .constraints([FtuiConstraint::Fill, FtuiConstraint::Fill])
        .split(row_chunks[0]);
    let bottom_cols = Flex::horizontal()
        .constraints([FtuiConstraint::Fill, FtuiConstraint::Fill])
        .split(row_chunks[1]);
    if top_cols.len() < 2 || bottom_cols.len() < 2 {
        return;
    }

    render_oracle_ftui_rate_forecasts(f, top_cols[0], data, theme);
    render_oracle_ftui_failure_risks(f, top_cols[1], data, theme);
    render_oracle_ftui_cost_trajectory(f, bottom_cols[0], data, theme);
    render_oracle_ftui_resource_forecasts(f, bottom_cols[1], data, theme);
}

fn render_oracle_ftui_rate_forecasts(
    f: &mut FtuiFrame,
    area: FtuiRect,
    data: &OracleData,
    theme: &Theme,
) {
    let colors = theme.ftui_colors();
    if data.rate_forecasts.is_empty() {
        let empty = FtuiParagraph::new(FtuiText::from_spans([FtuiSpan::styled(
            "No rate limit data available",
            FtuiStyle::new().fg(packed(colors.muted)),
        )]))
        .block(ftui_block(
            Some(" Rate Limit Forecasts "),
            oracle_section_border(data.selected_section == OracleSection::RateLimits, theme),
        ));
        FtuiWidget::render(&empty, area, f);
        return;
    }

    let items: Vec<FtuiListItem> = data
        .rate_forecasts
        .iter()
        .flat_map(|forecast| {
            let usage_color = rate_usage_color(forecast.usage_pct, theme);
            let status_text = match forecast.minutes_to_limit {
                Some(minutes) if minutes < 60 => format!("limit in {minutes} min"),
                Some(minutes) => format!("{} hr headroom", minutes / 60),
                None => "plenty of headroom".to_string(),
            };

            let mut item_lines = vec![
                FtuiListItem::new(FtuiLine::from_spans([
                    FtuiSpan::styled(
                        &forecast.provider,
                        FtuiStyle::new()
                            .fg(packed(theme.provider_color(&forecast.provider)))
                            .bold(),
                    ),
                    FtuiSpan::styled(
                        format!(" ({})", forecast.account),
                        FtuiStyle::new().fg(packed(colors.muted)),
                    ),
                ])),
                FtuiListItem::new(FtuiLine::from_spans([
                    FtuiSpan::styled(
                        format!("{:.0}% used", forecast.usage_pct),
                        FtuiStyle::new().fg(packed(usage_color)),
                    ),
                    FtuiSpan::raw(" "),
                    FtuiSpan::styled(status_text, FtuiStyle::new().fg(packed(colors.text))),
                ])),
            ];

            if let Some(recommendation) = forecast.recommendation.as_deref() {
                item_lines.push(FtuiListItem::new(FtuiLine::from_spans([
                    FtuiSpan::styled("-> ", FtuiStyle::new().fg(packed(colors.info))),
                    FtuiSpan::styled(recommendation, FtuiStyle::new().fg(packed(colors.info))),
                ])));
            }

            if let Some(backup) = forecast.backup_account.as_deref() {
                item_lines.push(FtuiListItem::new(FtuiLine::from_spans([
                    FtuiSpan::styled("Backup: ", FtuiStyle::new().fg(packed(colors.muted))),
                    FtuiSpan::styled(backup, FtuiStyle::new().fg(packed(colors.healthy))),
                ])));
            }

            item_lines
        })
        .collect();

    let list = FtuiList::new(items).block(ftui_block(
        Some(" Rate Limit Forecasts "),
        oracle_section_border(data.selected_section == OracleSection::RateLimits, theme),
    ));
    FtuiWidget::render(&list, area, f);
}

fn render_oracle_ftui_failure_risks(
    f: &mut FtuiFrame,
    area: FtuiRect,
    data: &OracleData,
    theme: &Theme,
) {
    let colors = theme.ftui_colors();
    if data.failure_risks.is_empty() {
        let empty = FtuiParagraph::new(FtuiText::from_spans([FtuiSpan::styled(
            "No agents being monitored",
            FtuiStyle::new().fg(packed(colors.muted)),
        )]))
        .block(ftui_block(
            Some(" Failure Risk "),
            oracle_section_border(data.selected_section == OracleSection::FailureRisk, theme),
        ));
        FtuiWidget::render(&empty, area, f);
        return;
    }

    let items: Vec<FtuiListItem> = data
        .failure_risks
        .iter()
        .flat_map(|risk| {
            let risk_color = failure_risk_color(risk.risk_pct, theme);
            let status_indicator = if risk.status == "healthy" {
                "●"
            } else {
                "◐"
            };
            let mut item_lines = vec![FtuiListItem::new(FtuiLine::from_spans([
                FtuiSpan::styled(
                    status_indicator,
                    FtuiStyle::new().fg(packed(risk_color)).bold(),
                ),
                FtuiSpan::raw(" "),
                FtuiSpan::styled(&risk.agent_id, FtuiStyle::new().fg(packed(colors.text))),
                FtuiSpan::styled(
                    format!(" on {}", risk.machine),
                    FtuiStyle::new().fg(packed(colors.muted)),
                ),
            ]))];

            if risk.risk_pct > 0.0 {
                let risk_text = match risk.minutes_to_failure {
                    Some(minutes) => format!("{:.0}% risk in {} min", risk.risk_pct, minutes),
                    None => format!("{:.0}% risk", risk.risk_pct),
                };
                item_lines.push(FtuiListItem::new(FtuiLine::from_spans([FtuiSpan::styled(
                    risk_text,
                    FtuiStyle::new().fg(packed(risk_color)),
                )])));

                if !risk.indicators.is_empty() {
                    item_lines.push(FtuiListItem::new(FtuiLine::from_spans([
                        FtuiSpan::styled("Indicators: ", FtuiStyle::new().fg(packed(colors.muted))),
                        FtuiSpan::styled(
                            risk.indicators.join(", "),
                            FtuiStyle::new().fg(packed(colors.info)),
                        ),
                    ])));
                }
            } else {
                item_lines.push(FtuiListItem::new(FtuiLine::from_spans([FtuiSpan::styled(
                    "healthy",
                    FtuiStyle::new().fg(packed(colors.healthy)),
                )])));
            }

            item_lines.push(FtuiListItem::new(FtuiLine::from_spans([
                FtuiSpan::styled(
                    "Past occurrences: ",
                    FtuiStyle::new().fg(packed(colors.muted)),
                ),
                FtuiSpan::styled(
                    risk.past_occurrences.to_string(),
                    FtuiStyle::new().fg(packed(colors.warning)),
                ),
            ])));

            item_lines
        })
        .collect();

    let list = FtuiList::new(items).block(ftui_block(
        Some(" Failure Risk "),
        oracle_section_border(data.selected_section == OracleSection::FailureRisk, theme),
    ));
    FtuiWidget::render(&list, area, f);
}

fn render_oracle_ftui_cost_trajectory(
    f: &mut FtuiFrame,
    area: FtuiRect,
    data: &OracleData,
    theme: &Theme,
) {
    let colors = theme.ftui_colors();
    let cost = &data.cost_trajectory;
    let today_pct = budget_pct(cost.today_spent, cost.today_budget);
    let week_pct = budget_pct(cost.week_spent, cost.week_budget);
    let status_color = if cost.on_track {
        colors.healthy
    } else {
        colors.warning
    };
    let status_text = if cost.on_track {
        "on track"
    } else {
        "over budget"
    };

    let mut lines = vec![
        FtuiLine::from_spans([
            FtuiSpan::styled("Today: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                format!("${:.2}", cost.today_spent),
                FtuiStyle::new().fg(packed(colors.text)),
            ),
            FtuiSpan::styled(
                format!(" / ${:.2}", cost.today_budget),
                FtuiStyle::new().fg(packed(colors.info)),
            ),
            FtuiSpan::raw(" "),
            FtuiSpan::styled(
                format!("({today_pct:.0}%)"),
                FtuiStyle::new().fg(packed(status_color)),
            ),
        ]),
        FtuiLine::from_spans([
            FtuiSpan::styled("Projection: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                format!("${:.2} by EOD", cost.today_projection),
                FtuiStyle::new().fg(packed(colors.text)),
            ),
            FtuiSpan::raw(" "),
            FtuiSpan::styled(
                status_text,
                FtuiStyle::new().fg(packed(status_color)).bold(),
            ),
        ]),
        FtuiLine::from_spans([
            FtuiSpan::styled("Week: ", FtuiStyle::new().fg(packed(colors.muted))),
            FtuiSpan::styled(
                format!("${:.2}", cost.week_spent),
                FtuiStyle::new().fg(packed(colors.text)),
            ),
            FtuiSpan::styled(
                format!(" / ${:.2}", cost.week_budget),
                FtuiStyle::new().fg(packed(colors.info)),
            ),
            FtuiSpan::raw(" "),
            FtuiSpan::styled(
                format!("({week_pct:.0}%)"),
                FtuiStyle::new().fg(packed(colors.warning)),
            ),
        ]),
    ];

    if let Some(suggestion) = cost.savings_suggestion.as_deref() {
        lines.push(FtuiLine::from_spans([
            FtuiSpan::styled("-> ", FtuiStyle::new().fg(packed(colors.info))),
            FtuiSpan::styled(suggestion, FtuiStyle::new().fg(packed(colors.info))),
        ]));
    }

    let paragraph = FtuiParagraph::new(FtuiText::from_lines(lines)).block(ftui_block(
        Some(" Cost Trajectory "),
        oracle_section_border(
            data.selected_section == OracleSection::CostTrajectory,
            theme,
        ),
    ));
    FtuiWidget::render(&paragraph, area, f);
}

fn render_oracle_ftui_resource_forecasts(
    f: &mut FtuiFrame,
    area: FtuiRect,
    data: &OracleData,
    theme: &Theme,
) {
    let colors = theme.ftui_colors();
    if data.resource_forecasts.is_empty() {
        let empty = FtuiParagraph::new(FtuiText::from_spans([FtuiSpan::styled(
            "No resource forecasts available",
            FtuiStyle::new().fg(packed(colors.muted)),
        )]))
        .block(ftui_block(
            Some(" Resource Forecasts "),
            oracle_section_border(data.selected_section == OracleSection::Resources, theme),
        ));
        FtuiWidget::render(&empty, area, f);
        return;
    }

    let items: Vec<FtuiListItem> = data
        .resource_forecasts
        .iter()
        .flat_map(|forecast| {
            let trend_color = resource_trend_color(forecast.projected_pct, theme);
            let trend_arrow = if forecast.projected_pct > forecast.current_pct {
                "↑"
            } else if forecast.projected_pct < forecast.current_pct {
                "↓"
            } else {
                "→"
            };

            let mut item_lines = vec![FtuiListItem::new(FtuiLine::from_spans([
                FtuiSpan::styled(&forecast.machine, FtuiStyle::new().fg(packed(colors.text))),
                FtuiSpan::styled(
                    format!(" {}:", forecast.resource),
                    FtuiStyle::new().fg(packed(colors.muted)),
                ),
                FtuiSpan::raw(" "),
                FtuiSpan::styled(
                    format!(
                        "{:.0}% {} {:.0}% in {} days",
                        forecast.current_pct,
                        trend_arrow,
                        forecast.projected_pct,
                        forecast.projection_days
                    ),
                    FtuiStyle::new().fg(packed(trend_color)),
                ),
            ]))];

            if let Some(alert) = forecast.alert.as_deref() {
                item_lines.push(FtuiListItem::new(FtuiLine::from_spans([
                    FtuiSpan::styled("! ", FtuiStyle::new().fg(packed(colors.critical)).bold()),
                    FtuiSpan::styled(alert, FtuiStyle::new().fg(packed(colors.critical))),
                ])));
            }

            item_lines
        })
        .collect();

    let list = FtuiList::new(items).block(ftui_block(
        Some(" Resource Forecasts "),
        oracle_section_border(data.selected_section == OracleSection::Resources, theme),
    ));
    FtuiWidget::render(&list, area, f);
}

fn render_oracle_ftui_footer(f: &mut FtuiFrame, area: FtuiRect, theme: &Theme) {
    let colors = theme.ftui_colors();
    let footer = FtuiParagraph::new(FtuiText::from_spans(vec![
        FtuiSpan::styled("[Tab]", FtuiStyle::new().fg(packed(colors.accent))),
        FtuiSpan::styled(" Section ", FtuiStyle::new().fg(packed(colors.muted))),
        FtuiSpan::styled("[r]", FtuiStyle::new().fg(packed(colors.accent))),
        FtuiSpan::styled(" Refresh ", FtuiStyle::new().fg(packed(colors.muted))),
        FtuiSpan::styled("[Enter]", FtuiStyle::new().fg(packed(colors.accent))),
        FtuiSpan::styled(" Details ", FtuiStyle::new().fg(packed(colors.muted))),
        FtuiSpan::styled("[a]", FtuiStyle::new().fg(packed(colors.accent))),
        FtuiSpan::styled(" Apply ", FtuiStyle::new().fg(packed(colors.muted))),
        FtuiSpan::styled("[d]", FtuiStyle::new().fg(packed(colors.accent))),
        FtuiSpan::styled(" Dismiss ", FtuiStyle::new().fg(packed(colors.muted))),
        FtuiSpan::styled("[q]", FtuiStyle::new().fg(packed(colors.accent))),
        FtuiSpan::styled(" Back", FtuiStyle::new().fg(packed(colors.muted))),
    ]))
    .style(FtuiStyle::new().bg(packed(colors.bg_secondary)))
    .block(ftui_block(None, colors.muted));

    FtuiWidget::render(&footer, area, f);
}

fn refresh_age_label(refresh_age_secs: u64) -> String {
    if refresh_age_secs == 0 {
        "just now".to_string()
    } else if refresh_age_secs < 60 {
        format!("{refresh_age_secs}s ago")
    } else {
        format!("{}m ago", refresh_age_secs / 60)
    }
}

fn oracle_section_border(is_selected: bool, theme: &Theme) -> ftui::Color {
    if is_selected {
        theme.ftui_colors().accent
    } else {
        theme.ftui_colors().muted
    }
}

fn rate_usage_color(usage_pct: f64, theme: &Theme) -> ftui::Color {
    if usage_pct >= 90.0 {
        theme.ftui_colors().critical
    } else if usage_pct >= 70.0 {
        theme.ftui_colors().warning
    } else {
        theme.ftui_colors().healthy
    }
}

fn failure_risk_color(risk_pct: f64, theme: &Theme) -> ftui::Color {
    if risk_pct >= 70.0 {
        theme.ftui_colors().critical
    } else if risk_pct >= 40.0 {
        theme.ftui_colors().warning
    } else {
        theme.ftui_colors().healthy
    }
}

fn resource_trend_color(projected_pct: f64, theme: &Theme) -> ftui::Color {
    if projected_pct >= 90.0 {
        theme.ftui_colors().critical
    } else if projected_pct >= 75.0 {
        theme.ftui_colors().warning
    } else {
        theme.ftui_colors().healthy
    }
}

fn budget_pct(spent: f64, budget: f64) -> f64 {
    if budget > 0.0 {
        (spent / budget * 100.0).min(100.0)
    } else {
        0.0
    }
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

    fn buffer_contains(buffer: &Buffer, width: u16, height: u16, needle: &str) -> bool {
        (0..height).any(|y| {
            let row: String = (0..width)
                .map(|x| {
                    buffer
                        .get(x, y)
                        .and_then(|cell| cell.content.as_char())
                        .unwrap_or(' ')
                })
                .collect();
            row.contains(needle)
        })
    }

    #[test]
    fn test_oracle_data_default() {
        let data = OracleData::default();
        assert!(data.rate_forecasts.is_empty());
        assert!(data.failure_risks.is_empty());
        assert_eq!(data.selected_section, OracleSection::RateLimits);
    }

    #[test]
    fn test_rate_forecast_default() {
        let forecast = RateForecast::default();
        assert!(forecast.provider.is_empty());
        assert!((forecast.usage_pct - 0.0).abs() < f64::EPSILON);
        assert!(forecast.minutes_to_limit.is_none());
    }

    #[test]
    fn test_failure_risk_default() {
        let risk = FailureRisk::default();
        assert!(risk.agent_id.is_empty());
        assert!((risk.risk_pct - 0.0).abs() < f64::EPSILON);
        assert_eq!(risk.status, "healthy");
    }

    #[test]
    fn test_cost_trajectory_default() {
        let cost = CostTrajectory::default();
        assert!((cost.today_spent - 0.0).abs() < f64::EPSILON);
        assert!(!cost.on_track);
    }

    #[test]
    fn test_resource_forecast_default() {
        let forecast = ResourceForecast::default();
        assert!(forecast.machine.is_empty());
        assert!((forecast.current_pct - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_oracle_section_default() {
        assert_eq!(OracleSection::default(), OracleSection::RateLimits);
    }

    #[test]
    fn test_oracle_data_with_rate_forecasts() {
        let data = OracleData {
            rate_forecasts: vec![
                RateForecast {
                    provider: "claude".to_string(),
                    account: "max-5".to_string(),
                    usage_pct: 92.0,
                    minutes_to_limit: Some(8),
                    recommendation: Some("Swap to backup".to_string()),
                    backup_account: Some("backup@email.com".to_string()),
                },
                RateForecast {
                    provider: "openai".to_string(),
                    account: "pro".to_string(),
                    usage_pct: 45.0,
                    minutes_to_limit: Some(240),
                    recommendation: None,
                    backup_account: None,
                },
            ],
            ..Default::default()
        };

        assert_eq!(data.rate_forecasts.len(), 2);
        assert!((data.rate_forecasts[0].usage_pct - 92.0).abs() < f64::EPSILON);
        assert!(data.rate_forecasts[0].recommendation.is_some());
    }

    #[test]
    fn test_oracle_data_with_failure_risks() {
        let data = OracleData {
            failure_risks: vec![
                FailureRisk {
                    agent_id: "cc_5".to_string(),
                    machine: "orko".to_string(),
                    risk_pct: 78.0,
                    minutes_to_failure: Some(15),
                    indicators: vec!["velocity down".to_string(), "context high".to_string()],
                    past_occurrences: 47,
                    status: "at_risk".to_string(),
                },
                FailureRisk {
                    agent_id: "codex_2".to_string(),
                    machine: "sydneymc".to_string(),
                    risk_pct: 0.0,
                    minutes_to_failure: None,
                    indicators: vec![],
                    past_occurrences: 0,
                    status: "healthy".to_string(),
                },
            ],
            ..Default::default()
        };

        assert_eq!(data.failure_risks.len(), 2);
        assert!((data.failure_risks[0].risk_pct - 78.0).abs() < f64::EPSILON);
        assert_eq!(data.failure_risks[1].status, "healthy");
    }

    #[test]
    fn test_cost_trajectory_calculations() {
        let cost = CostTrajectory {
            today_spent: 15.50,
            today_budget: 20.00,
            today_projection: 18.00,
            week_spent: 45.00,
            week_budget: 100.00,
            savings_suggestion: Some("shift to off-peak".to_string()),
            on_track: true,
        };

        let today_pct = cost.today_spent / cost.today_budget * 100.0;
        assert!((today_pct - 77.5).abs() < 0.1);
        assert!(cost.on_track);
    }

    #[test]
    fn test_resource_forecast_with_alert() {
        let forecast = ResourceForecast {
            machine: "orko".to_string(),
            resource: "disk".to_string(),
            current_pct: 89.0,
            projected_pct: 95.0,
            projection_days: 2,
            alert: Some("Will hit critical in 48h".to_string()),
        };

        assert!(forecast.alert.is_some());
        assert!(forecast.projected_pct > forecast.current_pct);
    }

    #[test]
    fn test_render_oracle_ftui_renders_forecasts() {
        let data = OracleData {
            rate_forecasts: vec![RateForecast {
                provider: "claude".to_string(),
                account: "max-5".to_string(),
                usage_pct: 92.0,
                minutes_to_limit: Some(8),
                recommendation: Some("Swap to backup".to_string()),
                backup_account: Some("alt-claude".to_string()),
            }],
            failure_risks: vec![FailureRisk {
                agent_id: "cc_5".to_string(),
                machine: "orko".to_string(),
                risk_pct: 78.0,
                minutes_to_failure: Some(15),
                indicators: vec!["velocity down".to_string(), "context high".to_string()],
                past_occurrences: 47,
                status: "at_risk".to_string(),
            }],
            cost_trajectory: CostTrajectory {
                today_spent: 15.5,
                today_budget: 20.0,
                today_projection: 18.0,
                week_spent: 45.0,
                week_budget: 100.0,
                savings_suggestion: Some("shift to off-peak".to_string()),
                on_track: true,
            },
            resource_forecasts: vec![ResourceForecast {
                machine: "orko".to_string(),
                resource: "disk".to_string(),
                current_pct: 89.0,
                projected_pct: 95.0,
                projection_days: 2,
                alert: Some("Will hit critical in 48h".to_string()),
            }],
            selected_section: OracleSection::RateLimits,
            refresh_age_secs: 30,
        };
        let theme = Theme::default();
        let mut pool = GraphemePool::new();
        let mut frame = FtuiFrame::new(108, 24, &mut pool);

        render_oracle_ftui(&mut frame, &data, &theme);

        assert!(buffer_contains(&frame.buffer, 108, 24, "ORACLE"));
        assert!(buffer_contains(&frame.buffer, 108, 24, "max-5"));
        assert!(buffer_contains(&frame.buffer, 108, 24, "cc_5"));
        assert!(buffer_contains(&frame.buffer, 108, 24, "shift to off-peak"));
        assert!(buffer_contains(&frame.buffer, 108, 24, "Will hit critical"));
    }

    #[test]
    fn test_render_oracle_ftui_renders_small_frame_stack() {
        let data = OracleData {
            rate_forecasts: vec![RateForecast {
                provider: "openai".to_string(),
                account: "pro".to_string(),
                usage_pct: 55.0,
                minutes_to_limit: Some(120),
                recommendation: None,
                backup_account: None,
            }],
            refresh_age_secs: 0,
            ..OracleData::default()
        };
        let theme = Theme::default();
        let mut pool = GraphemePool::new();
        let mut frame = FtuiFrame::new(72, 20, &mut pool);

        render_oracle_ftui(&mut frame, &data, &theme);

        assert!(buffer_contains(
            &frame.buffer,
            72,
            20,
            "Rate Limit Forecasts"
        ));
        assert!(buffer_contains(&frame.buffer, 72, 20, "openai"));
        assert!(buffer_contains(&frame.buffer, 72, 20, "just now"));
    }

    #[test]
    fn test_render_oracle_ftui_renders_empty_states() {
        let data = OracleData::default();
        let theme = Theme::default();
        let mut pool = GraphemePool::new();
        let mut frame = FtuiFrame::new(96, 22, &mut pool);

        render_oracle_ftui(&mut frame, &data, &theme);

        assert!(buffer_contains(
            &frame.buffer,
            96,
            22,
            "No rate limit data available"
        ));
        assert!(buffer_contains(
            &frame.buffer,
            96,
            22,
            "No agents being monitored"
        ));
        assert!(buffer_contains(
            &frame.buffer,
            96,
            22,
            "No resource forecasts available"
        ));
    }
}
