//! Reusable widget helpers for the TUI.
//!
//! These helpers target `ftui` primitives directly so screen modules can share
//! a single native widget vocabulary.

use ftui::{
    Frame, PackedRgba, Style,
    core::geometry::Rect,
    text::{Span, Text},
    widgets::{
        Badge, StatefulWidget, Widget,
        block::Block,
        borders::Borders,
        paragraph::Paragraph,
        spinner::{Spinner, SpinnerState},
    },
};

use crate::theme::Theme;

/// Render a styled section header.
#[must_use]
pub fn section_header(title: &str, theme: &Theme) -> Paragraph<'static> {
    Paragraph::new(Text::from_spans([Span::styled(
        format!(" {title} "),
        Style::new().fg(packed(theme.ftui_colors().accent)).bold(),
    )]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(packed(theme.ftui_colors().muted))),
    )
}

/// Render a health badge.
#[must_use]
pub fn health_badge(score: f64, theme: &Theme) -> Badge<'static> {
    Badge::new(theme.health_indicator(score)).with_style(
        Style::new()
            .fg(packed(theme.ftui_colors().text))
            .bg(packed(theme.health_color(score))),
    )
}

/// Render a status indicator (online/offline).
#[must_use]
pub fn status_indicator(online: bool, theme: &Theme) -> Span<'static> {
    if online {
        Span::styled("●", Style::new().fg(packed(theme.ftui_colors().healthy)))
    } else {
        Span::styled("○", Style::new().fg(packed(theme.ftui_colors().critical)))
    }
}

/// Render a severity indicator.
#[must_use]
pub fn severity_indicator(severity: &str, theme: &Theme) -> (Span<'static>, ftui::Color) {
    match severity.to_lowercase().as_str() {
        "critical" => (
            Span::styled("!", Style::new().fg(packed(theme.ftui_colors().critical))),
            theme.ftui_colors().critical,
        ),
        "warning" => (
            Span::styled("⚠", Style::new().fg(packed(theme.ftui_colors().warning))),
            theme.ftui_colors().warning,
        ),
        "info" => (
            Span::styled("ℹ", Style::new().fg(packed(theme.ftui_colors().info))),
            theme.ftui_colors().info,
        ),
        _ => (
            Span::styled("·", Style::new().fg(packed(theme.ftui_colors().muted))),
            theme.ftui_colors().muted,
        ),
    }
}

/// Render a loading message.
pub fn loading_message(f: &mut Frame, area: Rect, message: &str, theme: &Theme) {
    let spinner = Spinner::new()
        .style(Style::new().fg(packed(theme.ftui_colors().accent)))
        .label(message)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::new().fg(packed(theme.ftui_colors().muted))),
        );
    let mut state = SpinnerState::default();
    StatefulWidget::render(&spinner, area, f, &mut state);
}

/// Render an error message.
pub fn error_message(f: &mut Frame, area: Rect, message: &str, theme: &Theme) {
    let text = Paragraph::new(Text::from_spans([
        Span::styled("✗ ", Style::new().fg(packed(theme.ftui_colors().critical))),
        Span::styled(
            message.to_owned(),
            Style::new().fg(packed(theme.ftui_colors().text)),
        ),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(packed(theme.ftui_colors().critical))),
    );
    Widget::render(&text, area, f);
}

/// Format bytes to a human-readable string.
#[must_use]
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1_024;
    const MB: u64 = KB * 1_024;
    const GB: u64 = MB * 1_024;
    const TB: u64 = GB * 1_024;

    if bytes >= TB {
        format_bytes_unit(bytes, TB, "TB")
    } else if bytes >= GB {
        format_bytes_unit(bytes, GB, "GB")
    } else if bytes >= MB {
        format_bytes_unit(bytes, MB, "MB")
    } else if bytes >= KB {
        format_bytes_unit(bytes, KB, "KB")
    } else {
        format!("{bytes} B")
    }
}

/// Format a duration to a human-readable string.
#[must_use]
pub fn format_duration(secs: u64) -> String {
    if secs == 0 {
        return "just now".to_string();
    }

    let days = secs / 86_400;
    if days > 0 {
        return format!("{days}d");
    }

    let hours = secs / 3_600;
    let minutes = (secs % 3_600) / 60;
    let seconds = secs % 60;

    if hours > 0 {
        let mut parts = vec![format!("{hours}h")];
        if minutes > 0 {
            parts.push(format!("{minutes}m"));
        }
        if seconds > 0 {
            parts.push(format!("{seconds}s"));
        }
        return parts.join(" ");
    }

    if minutes > 0 {
        if seconds > 0 {
            format!("{minutes}m {seconds}s")
        } else {
            format!("{minutes}m")
        }
    } else {
        format!("{seconds}s")
    }
}

fn packed(color: ftui::Color) -> PackedRgba {
    let rgb = color.to_rgb();
    PackedRgba::rgb(rgb.r, rgb.g, rgb.b)
}

fn format_bytes_unit(bytes: u64, unit: u64, suffix: &str) -> String {
    let whole = bytes / unit;
    let remainder = bytes % unit;
    let mut tenths = ((remainder * 10) + (unit / 2)) / unit;
    let mut whole = whole;
    if tenths == 10 {
        whole += 1;
        tenths = 0;
    }
    format!("{whole}.{tenths} {suffix}")
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

    fn find_char(buffer: &Buffer, y: u16, width: u16, needle: char) -> Option<u16> {
        (0..width).find(|x| {
            buffer
                .get(*x, y)
                .and_then(|cell| cell.content.as_char())
                .is_some_and(|ch| ch == needle)
        })
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1_024), "1.0 KB");
        assert_eq!(format_bytes(1_536), "1.5 KB");
        assert_eq!(format_bytes(1_048_576), "1.0 MB");
        assert_eq!(format_bytes(1_073_741_824), "1.0 GB");
        assert_eq!(format_bytes(1_099_511_627_776), "1.0 TB");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(0), "just now");
        assert_eq!(format_duration(30), "30s");
        assert_eq!(format_duration(59), "59s");
        assert_eq!(format_duration(60), "1m");
        assert_eq!(format_duration(61), "1m 1s");
        assert_eq!(format_duration(3_600), "1h");
        assert_eq!(format_duration(3_661), "1h 1m 1s");
        assert_eq!(format_duration(86_400), "1d");
        assert_eq!(format_duration(172_800), "2d");
    }

    #[test]
    fn test_section_header_renders_title_with_accent_color() {
        let theme = Theme::default();
        let widget = section_header("Fleet", &theme);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(18, 3, &mut pool);

        widget.render(Rect::new(0, 0, 18, 3), &mut frame);

        assert!(row_string(&frame.buffer, 1, 18).contains(" Fleet "));
        let x = find_char(&frame.buffer, 1, 18, 'F').expect("title should render");
        let cell = frame.buffer.get(x, 1).expect("title cell should exist");
        assert_eq!(cell.fg, packed(theme.ftui_colors().accent));
    }

    #[test]
    fn test_health_badge_healthy_renders_green_badge() {
        let theme = Theme::default();
        let badge = health_badge(0.95, &theme);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(5, 1, &mut pool);

        badge.render(Rect::new(0, 0, 5, 1), &mut frame);

        let cell = frame
            .buffer
            .get(1, 0)
            .expect("badge center cell should exist");
        assert_eq!(cell.content.as_char(), Some('●'));
        assert_eq!(cell.bg, packed(theme.ftui_colors().healthy));
    }

    #[test]
    fn test_health_badge_warning_renders_yellow_badge() {
        let theme = Theme::default();
        let badge = health_badge(0.65, &theme);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(5, 1, &mut pool);

        badge.render(Rect::new(0, 0, 5, 1), &mut frame);

        let cell = frame
            .buffer
            .get(1, 0)
            .expect("badge center cell should exist");
        assert_eq!(cell.content.as_char(), Some('◐'));
        assert_eq!(cell.bg, packed(theme.ftui_colors().warning));
    }

    #[test]
    fn test_health_badge_critical_renders_red_badge() {
        let theme = Theme::default();
        let badge = health_badge(0.30, &theme);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(5, 1, &mut pool);

        badge.render(Rect::new(0, 0, 5, 1), &mut frame);

        let cell = frame
            .buffer
            .get(1, 0)
            .expect("badge center cell should exist");
        assert_eq!(cell.content.as_char(), Some('○'));
        assert_eq!(cell.bg, packed(theme.ftui_colors().critical));
    }

    #[test]
    fn test_status_indicator() {
        let theme = Theme::default();
        let online = status_indicator(true, &theme);
        let offline = status_indicator(false, &theme);

        assert_eq!(online.content.as_ref(), "●");
        assert_eq!(offline.content.as_ref(), "○");
        assert_eq!(
            online.style.and_then(|style| style.fg),
            Some(packed(theme.ftui_colors().healthy))
        );
        assert_eq!(
            offline.style.and_then(|style| style.fg),
            Some(packed(theme.ftui_colors().critical))
        );
    }

    #[test]
    fn test_severity_indicator() {
        let theme = Theme::default();
        let (critical, critical_color) = severity_indicator("critical", &theme);
        let (warning, warning_color) = severity_indicator("warning", &theme);
        let (info, info_color) = severity_indicator("info", &theme);
        let (unknown, unknown_color) = severity_indicator("unknown", &theme);

        assert_eq!(critical.content.as_ref(), "!");
        assert_eq!(warning.content.as_ref(), "⚠");
        assert_eq!(info.content.as_ref(), "ℹ");
        assert_eq!(unknown.content.as_ref(), "·");
        assert_eq!(critical_color, theme.ftui_colors().critical);
        assert_eq!(warning_color, theme.ftui_colors().warning);
        assert_eq!(info_color, theme.ftui_colors().info);
        assert_eq!(unknown_color, theme.ftui_colors().muted);
    }

    #[test]
    fn test_loading_message_renders_spinner_and_text() {
        let theme = Theme::default();
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(24, 3, &mut pool);

        loading_message(
            &mut frame,
            Rect::new(0, 0, 24, 3),
            "Collecting data",
            &theme,
        );

        assert_eq!(
            frame
                .buffer
                .get(1, 1)
                .and_then(|cell| cell.content.as_char()),
            Some('⠋')
        );
        assert!(row_string(&frame.buffer, 1, 24).contains("Collecting data"));
    }

    #[test]
    fn test_error_message_renders_icon_and_text() {
        let theme = Theme::default();
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(24, 3, &mut pool);

        error_message(&mut frame, Rect::new(0, 0, 24, 3), "boom", &theme);

        assert_eq!(
            frame
                .buffer
                .get(1, 1)
                .and_then(|cell| cell.content.as_char()),
            Some('✗')
        );
        assert!(row_string(&frame.buffer, 1, 24).contains("boom"));
    }
}
