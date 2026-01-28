//! Alerts screen implementation
//!
//! Displays active alerts, history, and rule management.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Row, Table},
    Frame,
};

use crate::theme::Theme;

/// Data needed to render the alerts screen
#[derive(Debug, Clone, Default)]
pub struct AlertsData {
    /// Active (unresolved) alerts
    pub active_alerts: Vec<AlertInfo>,
    /// Recently resolved alerts
    pub recent_alerts: Vec<AlertInfo>,
    /// Alert rules
    pub rules: Vec<AlertRuleInfo>,
    /// Selected index within current view
    pub selected_index: usize,
    /// Current view mode
    pub view_mode: AlertViewMode,
    /// Filter for severity
    pub severity_filter: Option<Severity>,
    /// Statistics
    pub stats: AlertStats,
}

/// View mode for alerts screen
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum AlertViewMode {
    #[default]
    Active,
    History,
    Rules,
}

impl AlertViewMode {
    pub fn next(&self) -> Self {
        match self {
            Self::Active => Self::History,
            Self::History => Self::Rules,
            Self::Rules => Self::Active,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Active => "Active",
            Self::History => "History",
            Self::Rules => "Rules",
        }
    }
}

/// Alert severity levels
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Severity {
    Critical,
    High,
    #[default]
    Warning,
    Info,
    Low,
}

impl Severity {
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Critical => "🔴",
            Self::High => "🟠",
            Self::Warning => "🟡",
            Self::Info => "🔵",
            Self::Low => "⚪",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Critical => "CRITICAL",
            Self::High => "HIGH",
            Self::Warning => "WARNING",
            Self::Info => "INFO",
            Self::Low => "LOW",
        }
    }
}

/// Individual alert information
#[derive(Debug, Clone)]
pub struct AlertInfo {
    /// Alert ID
    pub id: u64,
    /// Rule that triggered this alert
    pub rule_id: String,
    /// Alert title
    pub title: String,
    /// Alert message/description
    pub message: String,
    /// Severity level
    pub severity: Severity,
    /// When the alert fired
    pub fired_at: String,
    /// How long ago (human readable)
    pub age: String,
    /// Machine that triggered it
    pub machine_id: Option<String>,
    /// Whether it's been acknowledged
    pub acknowledged: bool,
    /// When it was resolved (if any)
    pub resolved_at: Option<String>,
    /// Additional context
    pub context: Option<String>,
}

impl Default for AlertInfo {
    fn default() -> Self {
        Self {
            id: 0,
            rule_id: String::new(),
            title: String::new(),
            message: String::new(),
            severity: Severity::default(),
            fired_at: String::new(),
            age: String::new(),
            machine_id: None,
            acknowledged: false,
            resolved_at: None,
            context: None,
        }
    }
}

/// Alert rule information
#[derive(Debug, Clone)]
pub struct AlertRuleInfo {
    /// Rule ID
    pub rule_id: String,
    /// Rule name
    pub name: String,
    /// Rule description
    pub description: String,
    /// Severity when triggered
    pub severity: Severity,
    /// Is rule enabled?
    pub enabled: bool,
    /// Is rule muted?
    pub muted: bool,
    /// Check interval in seconds
    pub check_interval: u32,
    /// Cooldown in seconds
    pub cooldown: u32,
    /// Times fired in last 24h
    pub fired_24h: u32,
}

impl Default for AlertRuleInfo {
    fn default() -> Self {
        Self {
            rule_id: String::new(),
            name: String::new(),
            description: String::new(),
            severity: Severity::default(),
            enabled: true,
            muted: false,
            check_interval: 60,
            cooldown: 300,
            fired_24h: 0,
        }
    }
}

/// Alert statistics
#[derive(Debug, Clone, Default)]
pub struct AlertStats {
    /// Total enabled rules
    pub rules_enabled: u32,
    /// Muted rules
    pub rules_muted: u32,
    /// Custom rules
    pub rules_custom: u32,
    /// Alerts in last 24h
    pub alerts_24h: u32,
    /// Critical alerts active
    pub critical_active: u32,
}

/// Render the alerts screen
pub fn render_alerts(f: &mut Frame, data: &AlertsData, theme: &Theme) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header with tabs
            Constraint::Min(10),   // Main content
            Constraint::Length(3), // Footer
        ])
        .split(f.area());

    render_header(f, chunks[0], data, theme);
    render_content(f, chunks[1], data, theme);
    render_footer(f, chunks[2], data, theme);
}

fn render_header(f: &mut Frame, area: Rect, data: &AlertsData, theme: &Theme) {
    let active_count = data.active_alerts.len();
    let critical_count = data
        .active_alerts
        .iter()
        .filter(|a| a.severity == Severity::Critical)
        .count();

    let title = Line::from(vec![
        Span::styled(
            "  A L E R T S  ",
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        // Tab indicators
        Span::styled(
            if data.view_mode == AlertViewMode::Active {
                " [Active] "
            } else {
                " Active "
            },
            if data.view_mode == AlertViewMode::Active {
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.muted)
            },
        ),
        Span::styled(
            if data.view_mode == AlertViewMode::History {
                " [History] "
            } else {
                " History "
            },
            if data.view_mode == AlertViewMode::History {
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.muted)
            },
        ),
        Span::styled(
            if data.view_mode == AlertViewMode::Rules {
                " [Rules] "
            } else {
                " Rules "
            },
            if data.view_mode == AlertViewMode::Rules {
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.muted)
            },
        ),
        Span::raw("  "),
        if active_count > 0 {
            Span::styled(
                format!("[{} active]", active_count),
                Style::default().fg(theme.warning),
            )
        } else {
            Span::styled("[no active alerts]", Style::default().fg(theme.healthy))
        },
        if critical_count > 0 {
            Span::styled(
                format!("  [{} critical]", critical_count),
                Style::default().fg(theme.critical),
            )
        } else {
            Span::raw("")
        },
    ]);

    let header = Paragraph::new(title).block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(theme.border)),
    );

    f.render_widget(header, area);
}

fn render_content(f: &mut Frame, area: Rect, data: &AlertsData, theme: &Theme) {
    match data.view_mode {
        AlertViewMode::Active => render_active_alerts(f, area, data, theme),
        AlertViewMode::History => render_history(f, area, data, theme),
        AlertViewMode::Rules => render_rules(f, area, data, theme),
    }
}

fn render_active_alerts(f: &mut Frame, area: Rect, data: &AlertsData, theme: &Theme) {
    let block = Block::default()
        .title(format!(" Active Alerts ({}) ", data.active_alerts.len()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border));

    if data.active_alerts.is_empty() {
        let empty = Paragraph::new("  ✓ No active alerts - all systems nominal")
            .style(Style::default().fg(theme.healthy))
            .block(block);
        f.render_widget(empty, area);
        return;
    }

    let inner = block.inner(area);
    f.render_widget(block, area);

    let items: Vec<ListItem> = data
        .active_alerts
        .iter()
        .enumerate()
        .map(|(i, alert)| {
            let style = if i == data.selected_index {
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            };

            let severity_style = match alert.severity {
                Severity::Critical => Style::default().fg(theme.critical),
                Severity::High => Style::default().fg(theme.warning),
                Severity::Warning => Style::default().fg(theme.warning),
                Severity::Info => Style::default().fg(theme.info),
                Severity::Low => Style::default().fg(theme.muted),
            };

            let lines = vec![
                Line::from(vec![
                    Span::styled(format!("{} ", alert.severity.symbol()), severity_style),
                    Span::styled(format!("{:<10}", alert.severity.label()), severity_style),
                    Span::styled(
                        format!("{:>8} ago  ", alert.age),
                        Style::default().fg(theme.muted),
                    ),
                    Span::styled(&alert.title, style),
                ]),
                Line::from(vec![
                    Span::raw("   └─ "),
                    Span::styled(
                        format!(
                            "Machine: {} | Rule: {}",
                            alert.machine_id.as_deref().unwrap_or("unknown"),
                            alert.rule_id
                        ),
                        Style::default().fg(theme.muted),
                    ),
                    if alert.acknowledged {
                        Span::styled(" [ack]", Style::default().fg(theme.healthy))
                    } else {
                        Span::raw("")
                    },
                ]),
            ];

            ListItem::new(lines)
        })
        .collect();

    let list = List::new(items);
    f.render_widget(list, inner);
}

fn render_history(f: &mut Frame, area: Rect, data: &AlertsData, theme: &Theme) {
    let block = Block::default()
        .title(" Recent (Resolved) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border));

    if data.recent_alerts.is_empty() {
        let empty = Paragraph::new("  No recent alerts")
            .style(Style::default().fg(theme.muted))
            .block(block);
        f.render_widget(empty, area);
        return;
    }

    let inner = block.inner(area);
    f.render_widget(block, area);

    let items: Vec<ListItem> = data
        .recent_alerts
        .iter()
        .enumerate()
        .map(|(i, alert)| {
            let style = if i == data.selected_index {
                Style::default().fg(theme.highlight)
            } else {
                Style::default().fg(theme.text)
            };

            let line = Line::from(vec![
                Span::styled("✓ ", Style::default().fg(theme.healthy)),
                Span::styled(
                    format!("{} ago  ", alert.age),
                    Style::default().fg(theme.muted),
                ),
                Span::styled(&alert.title, style),
                if let Some(ref ctx) = alert.context {
                    Span::styled(format!(" - {}", ctx), Style::default().fg(theme.muted))
                } else {
                    Span::raw("")
                },
            ]);

            ListItem::new(line)
        })
        .collect();

    let list = List::new(items);
    f.render_widget(list, inner);
}

fn render_rules(f: &mut Frame, area: Rect, data: &AlertsData, theme: &Theme) {
    let block = Block::default()
        .title(format!(
            " Alert Rules ({} enabled | {} muted | {} custom) ",
            data.stats.rules_enabled, data.stats.rules_muted, data.stats.rules_custom
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border));

    if data.rules.is_empty() {
        let empty = Paragraph::new("  No alert rules configured")
            .style(Style::default().fg(theme.muted))
            .block(block);
        f.render_widget(empty, area);
        return;
    }

    let header = Row::new(vec!["", "Rule", "Severity", "Interval", "24h Fires"]).style(
        Style::default()
            .fg(theme.muted)
            .add_modifier(Modifier::BOLD),
    );

    let rows: Vec<Row> = data
        .rules
        .iter()
        .enumerate()
        .map(|(i, rule)| {
            let style = if i == data.selected_index {
                Style::default().fg(theme.highlight)
            } else if !rule.enabled {
                Style::default().fg(theme.muted)
            } else {
                Style::default().fg(theme.text)
            };

            let status = if rule.muted {
                "🔇"
            } else if rule.enabled {
                "✓"
            } else {
                "✗"
            };

            Row::new(vec![
                status.to_string(),
                rule.name.clone(),
                rule.severity.label().to_string(),
                format!("{}s", rule.check_interval),
                rule.fired_24h.to_string(),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(3),
            Constraint::Min(25),
            Constraint::Length(12),
            Constraint::Length(10),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .block(block);

    f.render_widget(table, area);
}

fn render_footer(f: &mut Frame, area: Rect, data: &AlertsData, theme: &Theme) {
    let help_text = match data.view_mode {
        AlertViewMode::Active => "[a]ck  [m]ute  [d]ismiss  [Tab]view  [/]search",
        AlertViewMode::History => "[Enter]details  [Tab]view  [/]search  [t]ime range",
        AlertViewMode::Rules => "[e]nable  [m]ute  [Enter]edit  [Tab]view",
    };

    let content = Line::from(vec![
        Span::styled(
            format!(" Rules: {} enabled ", data.stats.rules_enabled),
            Style::default().fg(theme.muted),
        ),
        Span::styled("│", Style::default().fg(theme.border)),
        Span::styled(
            format!(" {} muted ", data.stats.rules_muted),
            Style::default().fg(theme.muted),
        ),
        Span::styled("│", Style::default().fg(theme.border)),
        Span::styled(
            format!(" {} custom  ", data.stats.rules_custom),
            Style::default().fg(theme.muted),
        ),
        Span::raw("    "),
        Span::styled(help_text, Style::default().fg(theme.muted)),
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
    fn test_severity_symbols() {
        assert_eq!(Severity::Critical.symbol(), "🔴");
        assert_eq!(Severity::High.symbol(), "🟠");
        assert_eq!(Severity::Warning.symbol(), "🟡");
        assert_eq!(Severity::Info.symbol(), "🔵");
        assert_eq!(Severity::Low.symbol(), "⚪");
    }

    #[test]
    fn test_severity_labels() {
        assert_eq!(Severity::Critical.label(), "CRITICAL");
        assert_eq!(Severity::High.label(), "HIGH");
        assert_eq!(Severity::Warning.label(), "WARNING");
        assert_eq!(Severity::Info.label(), "INFO");
        assert_eq!(Severity::Low.label(), "LOW");
    }

    #[test]
    fn test_view_mode_navigation() {
        assert_eq!(AlertViewMode::Active.next(), AlertViewMode::History);
        assert_eq!(AlertViewMode::History.next(), AlertViewMode::Rules);
        assert_eq!(AlertViewMode::Rules.next(), AlertViewMode::Active);
    }

    #[test]
    fn test_view_mode_labels() {
        assert_eq!(AlertViewMode::Active.label(), "Active");
        assert_eq!(AlertViewMode::History.label(), "History");
        assert_eq!(AlertViewMode::Rules.label(), "Rules");
    }

    #[test]
    fn test_default_alerts_data() {
        let data = AlertsData::default();
        assert!(data.active_alerts.is_empty());
        assert!(data.recent_alerts.is_empty());
        assert!(data.rules.is_empty());
        assert_eq!(data.view_mode, AlertViewMode::Active);
    }

    #[test]
    fn test_default_alert_info() {
        let alert = AlertInfo::default();
        assert_eq!(alert.id, 0);
        assert!(alert.title.is_empty());
        assert_eq!(alert.severity, Severity::Warning);
        assert!(!alert.acknowledged);
    }

    #[test]
    fn test_default_alert_rule_info() {
        let rule = AlertRuleInfo::default();
        assert!(rule.rule_id.is_empty());
        assert!(rule.enabled);
        assert!(!rule.muted);
        assert_eq!(rule.check_interval, 60);
        assert_eq!(rule.cooldown, 300);
    }

    #[test]
    fn test_alert_with_machine() {
        let alert = AlertInfo {
            id: 1,
            rule_id: "rate-limit-warning".to_string(),
            title: "Rate limit at 85%".to_string(),
            message: "Claude Max usage at 85%".to_string(),
            severity: Severity::Warning,
            fired_at: "2026-01-28T10:00:00Z".to_string(),
            age: "5m".to_string(),
            machine_id: Some("orko".to_string()),
            acknowledged: false,
            resolved_at: None,
            context: None,
        };

        assert_eq!(alert.machine_id.as_deref(), Some("orko"));
        assert_eq!(alert.severity.symbol(), "🟡");
    }

    #[test]
    fn test_acknowledged_alert() {
        let alert = AlertInfo {
            acknowledged: true,
            ..Default::default()
        };

        assert!(alert.acknowledged);
    }

    #[test]
    fn test_muted_rule() {
        let rule = AlertRuleInfo {
            rule_id: "test-rule".to_string(),
            name: "Test Rule".to_string(),
            enabled: true,
            muted: true,
            ..Default::default()
        };

        assert!(rule.muted);
        assert!(rule.enabled);
    }

    #[test]
    fn test_critical_alert() {
        let alert = AlertInfo {
            severity: Severity::Critical,
            ..Default::default()
        };

        assert_eq!(alert.severity, Severity::Critical);
        assert_eq!(alert.severity.symbol(), "🔴");
    }

    #[test]
    fn test_alert_stats_default() {
        let stats = AlertStats::default();
        assert_eq!(stats.rules_enabled, 0);
        assert_eq!(stats.rules_muted, 0);
        assert_eq!(stats.critical_active, 0);
    }
}
