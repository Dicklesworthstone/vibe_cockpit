//! Accounts screen implementation
//!
//! Displays account usage and rate limit status from caut and caam collectors.

use crate::theme::Theme;
use ftui::{
    Frame as FtuiFrame, PackedRgba, Style as FtuiStyle,
    layout::{Constraint as FtuiConstraint, Flex, Rect as FtuiRect},
    text::{Span as FtuiSpan, Text as FtuiText},
    widgets::{
        Widget as FtuiWidget,
        block::Block as FtuiBlock,
        borders::Borders as FtuiBorders,
        paragraph::Paragraph as FtuiParagraph,
        table::{Row as FtuiRow, Table as FtuiTable},
    },
};

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
    #[must_use]
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
                let numerator = u64::from(v - min) * 7 + (u64::from(range) / 2);
                let idx = usize::try_from(numerator / u64::from(range)).unwrap_or(7);
                chars[idx.min(7)]
            })
            .collect()
    }
}

pub fn render_accounts_ftui(f: &mut FtuiFrame, data: &AccountsData, theme: &Theme) {
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

    render_accounts_ftui_header(f, rows[0], data, theme);
    render_accounts_ftui_table(f, rows[1], data, theme);
    render_accounts_ftui_footer(f, rows[2], theme);
}

fn render_accounts_ftui_header(
    f: &mut FtuiFrame,
    area: FtuiRect,
    data: &AccountsData,
    theme: &Theme,
) {
    let colors = theme.ftui_colors();
    let total_accounts = data.accounts.len();
    let active_count = data
        .accounts
        .iter()
        .filter(|account| account.is_active)
        .count();
    let limited_count = data
        .accounts
        .iter()
        .filter(|account| account.rate_status.eq_ignore_ascii_case("red"))
        .count();

    let mut spans = vec![
        FtuiSpan::styled(
            "  ACCOUNTS  ",
            FtuiStyle::new().fg(packed(colors.text)).bold(),
        ),
        FtuiSpan::raw(" "),
        FtuiSpan::styled(
            format!("[Sort: {}]", accounts_sort_label(data.sort_by)),
            FtuiStyle::new().fg(packed(colors.accent)),
        ),
        FtuiSpan::raw(" "),
        FtuiSpan::styled(
            format!("[{total_accounts} accounts]"),
            FtuiStyle::new().fg(packed(colors.muted)),
        ),
        FtuiSpan::raw(" "),
        FtuiSpan::styled(
            format!("[{active_count} active]"),
            FtuiStyle::new().fg(packed(colors.healthy)),
        ),
    ];

    if limited_count > 0 {
        spans.push(FtuiSpan::raw(" "));
        spans.push(FtuiSpan::styled(
            format!("[{limited_count} rate-limited]"),
            FtuiStyle::new().fg(packed(colors.critical)),
        ));
    }

    if !data.filter.is_empty() {
        spans.push(FtuiSpan::raw(" "));
        spans.push(FtuiSpan::styled(
            format!("[Filter: {}]", data.filter),
            FtuiStyle::new().fg(packed(colors.info)),
        ));
    }

    let header = FtuiParagraph::new(FtuiText::from_spans(spans))
        .style(FtuiStyle::new().bg(packed(colors.bg_secondary)))
        .block(ftui_block(None, theme));

    FtuiWidget::render(&header, area, f);
}

fn render_accounts_ftui_table(
    f: &mut FtuiFrame,
    area: FtuiRect,
    data: &AccountsData,
    theme: &Theme,
) {
    let colors = theme.ftui_colors();

    if data.accounts.is_empty() {
        let empty = FtuiParagraph::new(FtuiText::from_spans([FtuiSpan::styled(
            "No accounts tracked. Run collectors to populate data.",
            FtuiStyle::new().fg(packed(colors.muted)),
        )]))
        .block(ftui_block(Some(" Account Inventory "), theme));
        FtuiWidget::render(&empty, area, f);
        return;
    }

    let filtered = filtered_accounts(data);
    if filtered.is_empty() {
        let empty = FtuiParagraph::new(FtuiText::from_spans([FtuiSpan::styled(
            "No accounts match the current filter.",
            FtuiStyle::new().fg(packed(colors.muted)),
        )]))
        .block(ftui_block(Some(" Account Inventory "), theme));
        FtuiWidget::render(&empty, area, f);
        return;
    }

    let clamped_selected = data.selected.min(filtered.len().saturating_sub(1));
    let header = FtuiRow::new([
        FtuiText::from_spans([FtuiSpan::styled("", FtuiStyle::new())]),
        FtuiText::from_spans([FtuiSpan::styled("Program", FtuiStyle::new().bold())]),
        FtuiText::from_spans([FtuiSpan::styled("Account", FtuiStyle::new().bold())]),
        FtuiText::from_spans([FtuiSpan::styled("Usage", FtuiStyle::new().bold())]),
        FtuiText::from_spans([FtuiSpan::styled("%", FtuiStyle::new().bold())]),
        FtuiText::from_spans([FtuiSpan::styled("Status", FtuiStyle::new().bold())]),
        FtuiText::from_spans([FtuiSpan::styled("24h Trend", FtuiStyle::new().bold())]),
        FtuiText::from_spans([FtuiSpan::styled("Last Switch", FtuiStyle::new().bold())]),
    ])
    .style(FtuiStyle::new().fg(packed(colors.muted)))
    .bottom_margin(1);

    let rows: Vec<FtuiRow> = filtered
        .iter()
        .enumerate()
        .map(|(index, account)| render_account_ftui_row(account, index == clamped_selected, theme))
        .collect();

    let table = FtuiTable::new(
        rows,
        [
            FtuiConstraint::Fixed(2),
            FtuiConstraint::Fixed(14),
            FtuiConstraint::Fixed(20),
            FtuiConstraint::Fixed(10),
            FtuiConstraint::Fixed(7),
            FtuiConstraint::Fixed(8),
            FtuiConstraint::Fixed(10),
            FtuiConstraint::Min(12),
        ],
    )
    .header(header)
    .column_spacing(1)
    .block(ftui_block(Some(" Account Inventory "), theme));

    FtuiWidget::render(&table, area, f);
}

fn render_accounts_ftui_footer(f: &mut FtuiFrame, area: FtuiRect, theme: &Theme) {
    let colors = theme.ftui_colors();
    let footer = FtuiParagraph::new(FtuiText::from_spans([FtuiSpan::styled(
        "↑↓ Navigate  / Filter  s Sort  Enter Details  q Back",
        FtuiStyle::new().fg(packed(colors.muted)),
    )]))
    .style(FtuiStyle::new().bg(packed(colors.bg_secondary)))
    .block(ftui_block(None, theme));

    FtuiWidget::render(&footer, area, f);
}

fn filtered_accounts(data: &AccountsData) -> Vec<&AccountStatus> {
    if data.filter.is_empty() {
        return data.accounts.iter().collect();
    }

    let filter = data.filter.to_lowercase();
    data.accounts
        .iter()
        .filter(|account| {
            account.program.to_lowercase().contains(&filter)
                || account.account.to_lowercase().contains(&filter)
        })
        .collect()
}

fn account_rate_color(rate_status: &str, theme: &Theme) -> ftui::Color {
    match rate_status.to_ascii_lowercase().as_str() {
        "green" => theme.ftui_colors().healthy,
        "yellow" => theme.ftui_colors().warning,
        "red" => theme.ftui_colors().critical,
        _ => theme.ftui_colors().muted,
    }
}

fn accounts_sort_label(sort_by: AccountSortField) -> &'static str {
    match sort_by {
        AccountSortField::Program => "Program",
        AccountSortField::Account => "Account",
        AccountSortField::Usage => "Usage",
        AccountSortField::Status => "Status",
    }
}

fn render_account_ftui_row(account: &AccountStatus, is_selected: bool, theme: &Theme) -> FtuiRow {
    let colors = theme.ftui_colors();
    let rate_color = account_rate_color(&account.rate_status, theme);
    let active_marker = if account.is_active { "●" } else { "○" };
    let active_color = if account.is_active {
        colors.healthy
    } else {
        colors.muted
    };
    let row_style = if is_selected {
        FtuiStyle::new().bg(packed(colors.bg_secondary))
    } else {
        FtuiStyle::new()
    };

    FtuiRow::new([
        FtuiText::from_spans([FtuiSpan::styled(
            active_marker,
            FtuiStyle::new().fg(packed(active_color)),
        )]),
        FtuiText::from_spans([FtuiSpan::styled(
            &account.program,
            FtuiStyle::new().fg(packed(theme.provider_color(&account.program))),
        )]),
        FtuiText::from_spans([FtuiSpan::styled(
            &account.account,
            FtuiStyle::new().fg(packed(colors.text)),
        )]),
        FtuiText::from_spans([FtuiSpan::styled(
            account_usage_text(account),
            FtuiStyle::new().fg(packed(colors.text)),
        )]),
        FtuiText::from_spans([FtuiSpan::styled(
            account_pct_text(account),
            FtuiStyle::new().fg(packed(rate_color)),
        )]),
        FtuiText::from_spans([FtuiSpan::styled(
            account.rate_status.to_uppercase(),
            FtuiStyle::new().fg(packed(rate_color)),
        )]),
        FtuiText::from_spans([FtuiSpan::styled(
            account.sparkline(),
            FtuiStyle::new().fg(packed(colors.info)),
        )]),
        FtuiText::from_spans([FtuiSpan::styled(
            account_last_switch_text(account),
            FtuiStyle::new().fg(packed(colors.muted)),
        )]),
    ])
    .style(row_style)
}

fn account_usage_text(account: &AccountStatus) -> String {
    match (account.usage, account.limit) {
        (usage, Some(limit)) => format!("{usage}/{limit}"),
        (usage, None) => usage.to_string(),
    }
}

fn account_pct_text(account: &AccountStatus) -> String {
    account
        .usage_pct
        .map_or_else(|| "  N/A".to_string(), |pct| format!("{pct:>5.1}%"))
}

fn account_last_switch_text(account: &AccountStatus) -> &str {
    account.last_switch.as_deref().unwrap_or("-")
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
    fn test_render_accounts_ftui_renders_rows() {
        let data = AccountsData {
            accounts: vec![
                AccountStatus {
                    program: "claude".to_string(),
                    account: "max-5".to_string(),
                    usage: 80,
                    limit: Some(100),
                    usage_pct: Some(80.0),
                    rate_status: "yellow".to_string(),
                    last_switch: Some("2m ago".to_string()),
                    is_active: true,
                    usage_trend: vec![10, 25, 40, 55, 70],
                },
                AccountStatus {
                    program: "codex".to_string(),
                    account: "pro".to_string(),
                    usage: 15,
                    limit: Some(50),
                    usage_pct: Some(30.0),
                    rate_status: "green".to_string(),
                    last_switch: Some("15m ago".to_string()),
                    is_active: false,
                    usage_trend: vec![5, 10, 15, 20, 25],
                },
            ],
            selected: 0,
            filter: String::new(),
            sort_by: AccountSortField::Usage,
        };
        let theme = Theme::default();
        let mut pool = GraphemePool::new();
        let mut frame = FtuiFrame::new(96, 18, &mut pool);

        render_accounts_ftui(&mut frame, &data, &theme);

        assert!(buffer_contains(&frame.buffer, 96, 18, "ACCOUNTS"));
        assert!(buffer_contains(&frame.buffer, 96, 18, "claude"));
        assert!(buffer_contains(&frame.buffer, 96, 18, "max-5"));
        assert!(buffer_contains(&frame.buffer, 96, 18, "YELLOW"));
        assert!(buffer_contains(&frame.buffer, 96, 18, "codex"));
    }

    #[test]
    fn test_render_accounts_ftui_renders_empty_state() {
        let data = AccountsData::default();
        let theme = Theme::default();
        let mut pool = GraphemePool::new();
        let mut frame = FtuiFrame::new(72, 14, &mut pool);

        render_accounts_ftui(&mut frame, &data, &theme);

        assert!(buffer_contains(
            &frame.buffer,
            72,
            14,
            "No accounts tracked"
        ));
    }

    #[test]
    fn test_sort_field_default() {
        assert_eq!(AccountSortField::default(), AccountSortField::Program);
    }
}
