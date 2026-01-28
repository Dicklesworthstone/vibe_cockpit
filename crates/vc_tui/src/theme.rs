//! Theme and color definitions for the TUI
//!
//! Provides a consistent color scheme across all screens.

use ratatui::style::Color;

/// TUI color theme
#[derive(Debug, Clone)]
pub struct Theme {
    /// Primary background color
    pub bg_primary: Color,
    /// Secondary background color
    pub bg_secondary: Color,
    /// Healthy/good status color
    pub healthy: Color,
    /// Warning status color
    pub warning: Color,
    /// Critical/error status color
    pub critical: Color,
    /// Info status color
    pub info: Color,
    /// Muted/dim text color
    pub muted: Color,
    /// Text color
    pub text: Color,
    /// Accent color for highlights
    pub accent: Color,
    /// Claude provider color
    pub claude: Color,
    /// Codex provider color
    pub codex: Color,
    /// Gemini provider color
    pub gemini: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            bg_primary: Color::Rgb(13, 17, 23),
            bg_secondary: Color::Rgb(22, 27, 34),
            healthy: Color::Rgb(63, 185, 80),
            warning: Color::Rgb(210, 153, 34),
            critical: Color::Rgb(248, 81, 73),
            info: Color::Rgb(88, 166, 255),
            muted: Color::Rgb(139, 148, 158),
            text: Color::Rgb(230, 237, 243),
            accent: Color::Rgb(136, 87, 229),
            claude: Color::Rgb(217, 119, 87),
            codex: Color::Rgb(16, 163, 127),
            gemini: Color::Rgb(66, 133, 244),
        }
    }
}

impl Theme {
    /// Get color for a health score (0.0 to 1.0)
    pub fn health_color(&self, score: f64) -> Color {
        if score >= 0.8 {
            self.healthy
        } else if score >= 0.5 {
            self.warning
        } else {
            self.critical
        }
    }

    /// Get health indicator character for a score
    pub fn health_indicator(&self, score: f64) -> &'static str {
        if score >= 0.8 {
            "●"
        } else if score >= 0.5 {
            "◐"
        } else {
            "○"
        }
    }

    /// Get color for provider name
    pub fn provider_color(&self, provider: &str) -> Color {
        match provider.to_lowercase().as_str() {
            "claude" => self.claude,
            "codex" | "openai" => self.codex,
            "gemini" | "google" => self.gemini,
            _ => self.muted,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_default() {
        let theme = Theme::default();
        // Just verify it creates without panicking
        assert_eq!(theme.healthy, Color::Rgb(63, 185, 80));
    }

    #[test]
    fn test_health_color_healthy() {
        let theme = Theme::default();
        assert_eq!(theme.health_color(1.0), theme.healthy);
        assert_eq!(theme.health_color(0.9), theme.healthy);
        assert_eq!(theme.health_color(0.8), theme.healthy);
    }

    #[test]
    fn test_health_color_warning() {
        let theme = Theme::default();
        assert_eq!(theme.health_color(0.79), theme.warning);
        assert_eq!(theme.health_color(0.5), theme.warning);
    }

    #[test]
    fn test_health_color_critical() {
        let theme = Theme::default();
        assert_eq!(theme.health_color(0.49), theme.critical);
        assert_eq!(theme.health_color(0.0), theme.critical);
    }

    #[test]
    fn test_health_indicator() {
        let theme = Theme::default();
        assert_eq!(theme.health_indicator(1.0), "●");
        assert_eq!(theme.health_indicator(0.6), "◐");
        assert_eq!(theme.health_indicator(0.3), "○");
    }

    #[test]
    fn test_provider_color() {
        let theme = Theme::default();
        assert_eq!(theme.provider_color("claude"), theme.claude);
        assert_eq!(theme.provider_color("Claude"), theme.claude);
        assert_eq!(theme.provider_color("codex"), theme.codex);
        assert_eq!(theme.provider_color("openai"), theme.codex);
        assert_eq!(theme.provider_color("gemini"), theme.gemini);
        assert_eq!(theme.provider_color("unknown"), theme.muted);
    }
}
