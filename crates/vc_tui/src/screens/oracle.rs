//! Oracle screen implementation
//!
//! Displays predictions, forecasts, and risk assessments from vc_oracle.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use crate::theme::Theme;

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
pub fn render_oracle(f: &mut Frame, data: &OracleData, theme: &Theme) {
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

fn render_header(f: &mut Frame, area: Rect, data: &OracleData, theme: &Theme) {
    let refresh_text = if data.refresh_age_secs == 0 {
        "just now".to_string()
    } else if data.refresh_age_secs < 60 {
        format!("{}s ago", data.refresh_age_secs)
    } else {
        format!("{}m ago", data.refresh_age_secs / 60)
    };

    let at_risk_count = data
        .failure_risks
        .iter()
        .filter(|r| r.risk_pct > 50.0)
        .count();
    let rate_warning_count = data
        .rate_forecasts
        .iter()
        .filter(|r| r.usage_pct > 80.0)
        .count();

    let title = Line::from(vec![
        Span::styled(
            "  O R A C L E  ",
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " - Predictions & Forecasts",
            Style::default().fg(theme.muted),
        ),
        Span::raw("  "),
        if rate_warning_count > 0 {
            Span::styled(
                format!("[{} rate warnings]", rate_warning_count),
                Style::default().fg(theme.warning),
            )
        } else {
            Span::raw("")
        },
        if at_risk_count > 0 {
            Span::styled(
                format!("  [{} at risk]", at_risk_count),
                Style::default().fg(theme.critical),
            )
        } else {
            Span::raw("")
        },
        Span::raw("  "),
        Span::styled(
            format!("[Updated: {}]", refresh_text),
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

fn render_main_content(f: &mut Frame, area: Rect, data: &OracleData, theme: &Theme) {
    // Split into 2x2 grid
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let top_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[0]);

    let bottom_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[1]);

    render_rate_forecasts(f, top_cols[0], data, theme);
    render_failure_risks(f, top_cols[1], data, theme);
    render_cost_trajectory(f, bottom_cols[0], data, theme);
    render_resource_forecasts(f, bottom_cols[1], data, theme);
}

fn render_rate_forecasts(f: &mut Frame, area: Rect, data: &OracleData, theme: &Theme) {
    let is_selected = data.selected_section == OracleSection::RateLimits;
    let border_color = if is_selected {
        theme.accent
    } else {
        theme.muted
    };

    if data.rate_forecasts.is_empty() {
        let empty = Paragraph::new(Span::styled(
            "  No rate limit data available",
            Style::default().fg(theme.muted),
        ))
        .block(
            Block::default()
                .title(Span::styled(
                    " RATE LIMIT FORECASTS ",
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
        .rate_forecasts
        .iter()
        .flat_map(|forecast| {
            let usage_color = if forecast.usage_pct >= 90.0 {
                theme.critical
            } else if forecast.usage_pct >= 70.0 {
                theme.warning
            } else {
                theme.healthy
            };

            let status_text = match forecast.minutes_to_limit {
                Some(mins) if mins < 60 => format!("limit in {} min", mins),
                Some(mins) => format!("{} hr headroom", mins / 60),
                None => "plenty of headroom".to_string(),
            };

            let mut lines = vec![
                ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        &forecast.provider,
                        Style::default().fg(theme.provider_color(&forecast.provider)),
                    ),
                    Span::styled(
                        format!(" ({})", forecast.account),
                        Style::default().fg(theme.muted),
                    ),
                ])),
                ListItem::new(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(
                        format!("{:.0}% used, ", forecast.usage_pct),
                        Style::default().fg(usage_color),
                    ),
                    Span::styled(status_text, Style::default().fg(theme.text)),
                ])),
            ];

            if let Some(ref rec) = forecast.recommendation {
                lines.push(ListItem::new(Line::from(vec![
                    Span::raw("    "),
                    Span::styled("→ ", Style::default().fg(theme.info)),
                    Span::styled(rec, Style::default().fg(theme.info)),
                ])));
            }

            lines
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(Span::styled(
                " RATE LIMIT FORECASTS ",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color)),
    );

    f.render_widget(list, area);
}

fn render_failure_risks(f: &mut Frame, area: Rect, data: &OracleData, theme: &Theme) {
    let is_selected = data.selected_section == OracleSection::FailureRisk;
    let border_color = if is_selected {
        theme.accent
    } else {
        theme.muted
    };

    if data.failure_risks.is_empty() {
        let empty = Paragraph::new(Span::styled(
            "  No agents being monitored",
            Style::default().fg(theme.muted),
        ))
        .block(
            Block::default()
                .title(Span::styled(
                    " FAILURE RISK ",
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
        .failure_risks
        .iter()
        .flat_map(|risk| {
            let risk_color = if risk.risk_pct >= 70.0 {
                theme.critical
            } else if risk.risk_pct >= 40.0 {
                theme.warning
            } else {
                theme.healthy
            };

            let status_indicator = if risk.status == "healthy" {
                "●"
            } else {
                "◐"
            };

            let mut lines = vec![ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(status_indicator, Style::default().fg(risk_color)),
                Span::raw(" "),
                Span::styled(&risk.agent_id, Style::default().fg(theme.text)),
                Span::styled(
                    format!(" on {}", risk.machine),
                    Style::default().fg(theme.muted),
                ),
            ]))];

            if risk.risk_pct > 0.0 {
                let risk_text = match risk.minutes_to_failure {
                    Some(mins) => format!("{:.0}% stuck risk in {} min", risk.risk_pct, mins),
                    None => format!("{:.0}% risk", risk.risk_pct),
                };
                lines.push(ListItem::new(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(risk_text, Style::default().fg(risk_color)),
                ])));

                if !risk.indicators.is_empty() {
                    lines.push(ListItem::new(Line::from(vec![
                        Span::raw("    "),
                        Span::styled(
                            format!("Indicators: {}", risk.indicators.join(", ")),
                            Style::default().fg(theme.muted),
                        ),
                    ])));
                }
            } else {
                lines.push(ListItem::new(Line::from(vec![
                    Span::raw("    "),
                    Span::styled("healthy", Style::default().fg(theme.healthy)),
                ])));
            }

            lines
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(Span::styled(
                " FAILURE RISK ",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color)),
    );

    f.render_widget(list, area);
}

fn render_cost_trajectory(f: &mut Frame, area: Rect, data: &OracleData, theme: &Theme) {
    let is_selected = data.selected_section == OracleSection::CostTrajectory;
    let border_color = if is_selected {
        theme.accent
    } else {
        theme.muted
    };

    let cost = &data.cost_trajectory;
    let today_pct = if cost.today_budget > 0.0 {
        (cost.today_spent / cost.today_budget * 100.0).min(100.0)
    } else {
        0.0
    };

    let week_pct = if cost.week_budget > 0.0 {
        (cost.week_spent / cost.week_budget * 100.0).min(100.0)
    } else {
        0.0
    };

    let status_color = if cost.on_track {
        theme.healthy
    } else {
        theme.warning
    };
    let status_text = if cost.on_track {
        "on track"
    } else {
        "over budget"
    };

    let items = vec![
        ListItem::new(Line::from(vec![
            Span::raw("  "),
            Span::styled("Today: ", Style::default().fg(theme.text)),
            Span::styled(
                format!("${:.2}", cost.today_spent),
                Style::default().fg(theme.text),
            ),
            Span::styled(
                format!(" / ${:.2}", cost.today_budget),
                Style::default().fg(theme.muted),
            ),
            Span::styled(
                format!(" ({:.0}%)", today_pct),
                Style::default().fg(status_color),
            ),
        ])),
        ListItem::new(Line::from(vec![
            Span::raw("    "),
            Span::styled(
                format!(
                    "Projection: ${:.2} by EOD ({})",
                    cost.today_projection, status_text
                ),
                Style::default().fg(status_color),
            ),
        ])),
        ListItem::new(Line::from(vec![Span::raw("")])),
        ListItem::new(Line::from(vec![
            Span::raw("  "),
            Span::styled("This week: ", Style::default().fg(theme.text)),
            Span::styled(
                format!("${:.2}", cost.week_spent),
                Style::default().fg(theme.text),
            ),
            Span::styled(
                format!(" / ${:.2}", cost.week_budget),
                Style::default().fg(theme.muted),
            ),
            Span::styled(
                format!(" ({:.0}%)", week_pct),
                Style::default().fg(theme.text),
            ),
        ])),
        if let Some(ref suggestion) = cost.savings_suggestion {
            ListItem::new(Line::from(vec![
                Span::raw("    "),
                Span::styled("→ ", Style::default().fg(theme.info)),
                Span::styled(suggestion, Style::default().fg(theme.info)),
            ]))
        } else {
            ListItem::new(Line::from(vec![Span::raw("")]))
        },
    ];

    let list = List::new(items).block(
        Block::default()
            .title(Span::styled(
                " COST TRAJECTORY ",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color)),
    );

    f.render_widget(list, area);
}

fn render_resource_forecasts(f: &mut Frame, area: Rect, data: &OracleData, theme: &Theme) {
    let is_selected = data.selected_section == OracleSection::Resources;
    let border_color = if is_selected {
        theme.accent
    } else {
        theme.muted
    };

    if data.resource_forecasts.is_empty() {
        let empty = Paragraph::new(Span::styled(
            "  No resource forecasts available",
            Style::default().fg(theme.muted),
        ))
        .block(
            Block::default()
                .title(Span::styled(
                    " RESOURCE FORECASTS ",
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
        .resource_forecasts
        .iter()
        .flat_map(|forecast| {
            let trend_color = if forecast.projected_pct >= 90.0 {
                theme.critical
            } else if forecast.projected_pct >= 75.0 {
                theme.warning
            } else {
                theme.healthy
            };

            let trend_arrow = if forecast.projected_pct > forecast.current_pct {
                "↑"
            } else if forecast.projected_pct < forecast.current_pct {
                "↓"
            } else {
                "→"
            };

            let mut lines = vec![ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(&forecast.machine, Style::default().fg(theme.text)),
                Span::styled(
                    format!(" {}: ", forecast.resource),
                    Style::default().fg(theme.muted),
                ),
                Span::styled(
                    format!(
                        "{:.0}% {} {:.0}% in {} days",
                        forecast.current_pct,
                        trend_arrow,
                        forecast.projected_pct,
                        forecast.projection_days
                    ),
                    Style::default().fg(trend_color),
                ),
            ]))];

            if let Some(ref alert) = forecast.alert {
                lines.push(ListItem::new(Line::from(vec![
                    Span::raw("    "),
                    Span::styled("! ", Style::default().fg(theme.critical)),
                    Span::styled(alert, Style::default().fg(theme.critical)),
                ])));
            }

            lines
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(Span::styled(
                " RESOURCE FORECASTS ",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color)),
    );

    f.render_widget(list, area);
}

fn render_footer(f: &mut Frame, area: Rect, theme: &Theme) {
    let shortcuts = vec![
        ("[Tab]", "Section"),
        ("[r]", "Refresh"),
        ("[Enter]", "Details"),
        ("[a]", "Apply"),
        ("[d]", "Dismiss"),
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(forecast.usage_pct, 0.0);
        assert!(forecast.minutes_to_limit.is_none());
    }

    #[test]
    fn test_failure_risk_default() {
        let risk = FailureRisk::default();
        assert!(risk.agent_id.is_empty());
        assert_eq!(risk.risk_pct, 0.0);
        assert_eq!(risk.status, "healthy");
    }

    #[test]
    fn test_cost_trajectory_default() {
        let cost = CostTrajectory::default();
        assert_eq!(cost.today_spent, 0.0);
        assert!(!cost.on_track);
    }

    #[test]
    fn test_resource_forecast_default() {
        let forecast = ResourceForecast::default();
        assert!(forecast.machine.is_empty());
        assert_eq!(forecast.current_pct, 0.0);
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
        assert_eq!(data.rate_forecasts[0].usage_pct, 92.0);
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
        assert_eq!(data.failure_risks[0].risk_pct, 78.0);
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
}
