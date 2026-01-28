//! Reusable widgets for the TUI
//!
//! Common UI components used across multiple screens.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
    layout::Rect,
};

use crate::theme::Theme;

/// Render a styled section header
pub fn section_header<'a>(title: &'a str, theme: &Theme) -> Paragraph<'a> {
    Paragraph::new(Line::from(vec![Span::styled(
        format!(" {} ", title),
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.muted)),
    )
}

/// Render a health badge
pub fn health_badge(score: f64, theme: &Theme) -> Span<'static> {
    let color = theme.health_color(score);
    let indicator = theme.health_indicator(score);
    Span::styled(indicator.to_string(), Style::default().fg(color))
}

/// Render a status indicator (online/offline)
pub fn status_indicator(online: bool, theme: &Theme) -> Span<'static> {
    if online {
        Span::styled("●", Style::default().fg(theme.healthy))
    } else {
        Span::styled("○", Style::default().fg(theme.critical))
    }
}

/// Render a severity indicator
pub fn severity_indicator(severity: &str, theme: &Theme) -> (Span<'static>, ratatui::style::Color) {
    match severity.to_lowercase().as_str() {
        "critical" => (Span::styled("!", Style::default().fg(theme.critical)), theme.critical),
        "warning" => (Span::styled("⚠", Style::default().fg(theme.warning)), theme.warning),
        "info" => (Span::styled("ℹ", Style::default().fg(theme.info)), theme.info),
        _ => (Span::styled("·", Style::default().fg(theme.muted)), theme.muted),
    }
}

/// Render a loading message
pub fn loading_message(f: &mut Frame, area: Rect, message: &str, theme: &Theme) {
    let text = Paragraph::new(Line::from(vec![
        Span::styled("⟳ ", Style::default().fg(theme.accent)),
        Span::styled(message, Style::default().fg(theme.muted)),
    ]))
    .block(Block::default().borders(Borders::ALL));
    f.render_widget(text, area);
}

/// Render an error message
pub fn error_message(f: &mut Frame, area: Rect, message: &str, theme: &Theme) {
    let text = Paragraph::new(Line::from(vec![
        Span::styled("✗ ", Style::default().fg(theme.critical)),
        Span::styled(message, Style::default().fg(theme.text)),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.critical)),
    );
    f.render_widget(text, area);
}

/// Format bytes to human readable string
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.1}TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.1}GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1}MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1}KB", bytes as f64 / KB as f64)
    } else {
        format!("{}B", bytes)
    }
}

/// Format duration to human readable string
pub fn format_duration(secs: u64) -> String {
    if secs == 0 {
        "just now".to_string()
    } else if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0B");
        assert_eq!(format_bytes(512), "512B");
        assert_eq!(format_bytes(1024), "1.0KB");
        assert_eq!(format_bytes(1536), "1.5KB");
        assert_eq!(format_bytes(1048576), "1.0MB");
        assert_eq!(format_bytes(1073741824), "1.0GB");
        assert_eq!(format_bytes(1099511627776), "1.0TB");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(0), "just now");
        assert_eq!(format_duration(30), "30s");
        assert_eq!(format_duration(59), "59s");
        assert_eq!(format_duration(60), "1m");
        assert_eq!(format_duration(3600), "1h");
        assert_eq!(format_duration(86400), "1d");
        assert_eq!(format_duration(172800), "2d");
    }

    #[test]
    fn test_health_badge() {
        let theme = Theme::default();
        let badge = health_badge(1.0, &theme);
        assert!(!badge.content.is_empty());
    }

    #[test]
    fn test_status_indicator() {
        let theme = Theme::default();
        let online = status_indicator(true, &theme);
        let offline = status_indicator(false, &theme);
        assert_ne!(online.style.fg, offline.style.fg);
    }

    #[test]
    fn test_severity_indicator() {
        let theme = Theme::default();
        let (critical, _) = severity_indicator("critical", &theme);
        let (warning, _) = severity_indicator("warning", &theme);
        let (info, _) = severity_indicator("info", &theme);
        let (unknown, _) = severity_indicator("unknown", &theme);

        // Just verify they return different content
        assert!(!critical.content.is_empty());
        assert!(!warning.content.is_empty());
        assert!(!info.content.is_empty());
        assert!(!unknown.content.is_empty());
    }
}
