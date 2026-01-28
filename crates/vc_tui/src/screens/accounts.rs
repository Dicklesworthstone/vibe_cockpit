//! Accounts screen implementation
//!
//! Displays account usage and rate limit status from caut and caam collectors.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Row, Table},
};

use crate::theme::Theme;

/// Data needed to render the accounts screen
#[derive(Debug, Clone, Default)]
pub struct AccountsData {
    /// List of accounts with their status
    pub accounts: Vec<AccountStatus>,
    /// Currently selected index (for highlighting)
    pub selected: usize,
    /// Filter string (empty = show all)
    pub filter: String,
    /// Sort field
    pub sort_by: AccountSortField,
}

/// Sort field for accounts table
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum AccountSortField {
    #[default]
    Program,
    Account,
    Usage,
    Status,
}

/// Individual account status for display
#[derive(Debug, Clone)]
pub struct AccountStatus {
    /// Program name (claude-code, codex-cli, etc.)
    pub program: String,
    /// Account identifier
    pub account: String,
    /// Current usage count
    pub usage: u32,
    /// Limit (if known)
    pub limit: Option<u32>,
    /// Usage percentage (0-100)
    pub usage_pct: Option<f64>,
    /// Rate status: "green", "yellow", "red"
    pub rate_status: String,
    /// Last account switch timestamp
    pub last_switch: Option<String>,
    /// Is this the currently active account?
    pub is_active: bool,
    /// 24h usage trend values for sparkline
    pub usage_trend: Vec<u32>,
}

impl Default for AccountStatus {
    fn default() -> Self {
        Self {
            program: String::new(),
            account: String::new(),
            usage: 0,
            limit: None,
            usage_pct: None,
            rate_status: "green".to_string(),
            last_switch: None,
            is_active: false,
            usage_trend: vec![],
        }
    }
}

impl AccountStatus {
    /// Get a short sparkline representation of usage trend
    pub fn sparkline(&self) -> String {
        if self.usage_trend.is_empty() {
            return "────────".to_string();
        }

        let chars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        let max = *self.usage_trend.iter().max().unwrap_or(&1).max(&1);
        let min = *self.usage_trend.iter().min().unwrap_or(&0);
        let range = (max - min).max(1);

        self.usage_trend
            .iter()
            .map(|&v| {
                let idx = ((v - min) as f64 / range as f64 * 7.0).round() as usize;
                chars[idx.min(7)]
            })
            .collect()
    }
}

/// Render the accounts screen
pub fn render_accounts(f: &mut Frame, data: &AccountsData, theme: &Theme) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Main content
            Constraint::Length(3), // Footer
        ])
        .split(f.area());

    render_header(f, chunks[0], data, theme);
    render_accounts_table(f, chunks[1], data, theme);
    render_footer(f, chunks[2], theme);
}

fn render_header(f: &mut Frame, area: Rect, data: &AccountsData, theme: &Theme) {
    let total_accounts = data.accounts.len();
    let active_count = data.accounts.iter().filter(|a| a.is_active).count();
    let red_count = data
        .accounts
        .iter()
        .filter(|a| a.rate_status == "red")
        .count();

    let title = Line::from(vec![
        Span::styled(
            "  A C C O U N T S  ",
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("[{} accounts]", total_accounts),
            Style::default().fg(theme.muted),
        ),
        Span::raw("  "),
        Span::styled(
            format!("[{} active]", active_count),
            Style::default().fg(theme.healthy),
        ),
        if red_count > 0 {
            Span::styled(
                format!("  [{} rate-limited]", red_count),
                Style::default().fg(theme.critical),
            )
        } else {
            Span::raw("")
        },
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

fn render_accounts_table(f: &mut Frame, area: Rect, data: &AccountsData, theme: &Theme) {
    if data.accounts.is_empty() {
        let empty = Paragraph::new(Span::styled(
            "  No accounts tracked. Run collectors to populate data.",
            Style::default().fg(theme.muted),
        ))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.muted)),
        );
        f.render_widget(empty, area);
        return;
    }

    // Filter accounts
    let filtered: Vec<&AccountStatus> = if data.filter.is_empty() {
        data.accounts.iter().collect()
    } else {
        let filter_lower = data.filter.to_lowercase();
        data.accounts
            .iter()
            .filter(|a| {
                a.program.to_lowercase().contains(&filter_lower)
                    || a.account.to_lowercase().contains(&filter_lower)
            })
            .collect()
    };

    // Create table rows
    let rows: Vec<Row> = filtered
        .iter()
        .enumerate()
        .map(|(i, account)| {
            let is_selected = i == data.selected;
            let row_style = if is_selected {
                Style::default().bg(theme.bg_secondary)
            } else {
                Style::default()
            };

            let active_marker = if account.is_active { "●" } else { " " };
            let active_style = if account.is_active {
                Style::default().fg(theme.healthy)
            } else {
                Style::default().fg(theme.muted)
            };

            let status_color = match account.rate_status.as_str() {
                "green" => theme.healthy,
                "yellow" => theme.warning,
                "red" => theme.critical,
                _ => theme.muted,
            };

            let usage_text = match (account.usage, account.limit) {
                (u, Some(l)) => format!("{}/{}", u, l),
                (u, None) => format!("{}", u),
            };

            let pct_text = account
                .usage_pct
                .map(|p| format!("{:>5.1}%", p))
                .unwrap_or_else(|| "  N/A".to_string());

            let switch_text = account
                .last_switch
                .as_ref()
                .map(|s| s.as_str())
                .unwrap_or("-");

            Row::new(vec![
                Line::from(Span::styled(active_marker, active_style)),
                Line::from(Span::styled(
                    &account.program,
                    Style::default().fg(theme.provider_color(&account.program)),
                )),
                Line::from(Span::styled(
                    &account.account,
                    Style::default().fg(theme.text),
                )),
                Line::from(Span::styled(usage_text, Style::default().fg(theme.text))),
                Line::from(Span::styled(pct_text, Style::default().fg(status_color))),
                Line::from(Span::styled(
                    account.rate_status.to_uppercase(),
                    Style::default().fg(status_color),
                )),
                Line::from(Span::styled(
                    account.sparkline(),
                    Style::default().fg(theme.info),
                )),
                Line::from(Span::styled(switch_text, Style::default().fg(theme.muted))),
            ])
            .style(row_style)
        })
        .collect();

    let header_style = Style::default()
        .fg(theme.accent)
        .add_modifier(Modifier::BOLD);

    let table = Table::new(
        rows,
        [
            Constraint::Length(1),  // Active marker
            Constraint::Length(12), // Program
            Constraint::Length(20), // Account
            Constraint::Length(10), // Usage
            Constraint::Length(7),  // %
            Constraint::Length(7),  // Status
            Constraint::Length(10), // Sparkline
            Constraint::Min(10),    // Last Switch
        ],
    )
    .header(
        Row::new(vec![
            Line::from(Span::styled(" ", header_style)),
            Line::from(Span::styled("Program", header_style)),
            Line::from(Span::styled("Account", header_style)),
            Line::from(Span::styled("Usage", header_style)),
            Line::from(Span::styled("%", header_style)),
            Line::from(Span::styled("Status", header_style)),
            Line::from(Span::styled("24h Trend", header_style)),
            Line::from(Span::styled("Last Switch", header_style)),
        ])
        .bottom_margin(1),
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.muted)),
    );

    f.render_widget(table, area);
}

fn render_footer(f: &mut Frame, area: Rect, theme: &Theme) {
    let shortcuts = vec![
        ("[Tab]", "Overview"),
        ("[j/k]", "Navigate"),
        ("[/]", "Filter"),
        ("[s]", "Sort"),
        ("[Enter]", "Details"),
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
    fn test_accounts_data_default() {
        let data = AccountsData::default();
        assert!(data.accounts.is_empty());
        assert_eq!(data.selected, 0);
        assert!(data.filter.is_empty());
    }

    #[test]
    fn test_account_status_default() {
        let account = AccountStatus::default();
        assert!(account.program.is_empty());
        assert_eq!(account.rate_status, "green");
        assert!(!account.is_active);
    }

    #[test]
    fn test_sparkline_empty() {
        let account = AccountStatus::default();
        assert_eq!(account.sparkline(), "────────");
    }

    #[test]
    fn test_sparkline_with_data() {
        let account = AccountStatus {
            usage_trend: vec![0, 25, 50, 75, 100],
            ..Default::default()
        };
        let spark = account.sparkline();
        assert_eq!(spark.chars().count(), 5);
    }

    #[test]
    fn test_sparkline_constant() {
        let account = AccountStatus {
            usage_trend: vec![50, 50, 50],
            ..Default::default()
        };
        // All same values should produce middle bars
        let spark = account.sparkline();
        assert!(!spark.is_empty());
    }

    #[test]
    fn test_accounts_data_with_items() {
        let data = AccountsData {
            accounts: vec![
                AccountStatus {
                    program: "claude-code".to_string(),
                    account: "max-5".to_string(),
                    usage: 80,
                    limit: Some(100),
                    usage_pct: Some(80.0),
                    rate_status: "yellow".to_string(),
                    is_active: true,
                    ..Default::default()
                },
                AccountStatus {
                    program: "codex-cli".to_string(),
                    account: "pro".to_string(),
                    usage: 150,
                    limit: None,
                    usage_pct: None,
                    rate_status: "green".to_string(),
                    is_active: false,
                    ..Default::default()
                },
            ],
            selected: 0,
            filter: String::new(),
            sort_by: AccountSortField::Program,
        };

        assert_eq!(data.accounts.len(), 2);
        assert!(data.accounts[0].is_active);
        assert_eq!(data.accounts[0].rate_status, "yellow");
    }

    #[test]
    fn test_sort_field_default() {
        assert_eq!(AccountSortField::default(), AccountSortField::Program);
    }
}
