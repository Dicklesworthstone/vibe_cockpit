//! Theme and color definitions for the TUI.
//!
//! `ftui` is the canonical source of truth for palette and resolved theme data.

use ftui::{
    AdaptiveColor, Color as FtuiColor, ResolvedTheme as FtuiResolvedTheme, Theme as FtuiTheme,
    ThemeBuilder,
};

/// `ftui-style` palette values used by `vc_tui`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeColors {
    /// Primary background color.
    pub bg_primary: FtuiColor,
    /// Secondary background color.
    pub bg_secondary: FtuiColor,
    /// Healthy/good status color.
    pub healthy: FtuiColor,
    /// Warning status color.
    pub warning: FtuiColor,
    /// Critical/error status color.
    pub critical: FtuiColor,
    /// Info status color.
    pub info: FtuiColor,
    /// Muted/dim text color.
    pub muted: FtuiColor,
    /// Text color.
    pub text: FtuiColor,
    /// Accent color for highlights.
    pub accent: FtuiColor,
    /// Highlight color for selected items.
    pub highlight: FtuiColor,
    /// Border color.
    pub border: FtuiColor,
    /// Claude provider color.
    pub claude: FtuiColor,
    /// Codex provider color.
    pub codex: FtuiColor,
    /// Gemini provider color.
    pub gemini: FtuiColor,
}

/// TUI color theme.
#[derive(Debug, Clone)]
pub struct Theme {
    style: FtuiTheme,
    resolved: FtuiResolvedTheme,
    ftui_colors: ThemeColors,
    is_dark_mode: bool,
}

impl Default for Theme {
    fn default() -> Self {
        Self::from_mode(true)
    }
}

impl Theme {
    /// Build the theme for a specific light or dark mode.
    #[must_use]
    pub fn from_mode(is_dark_mode: bool) -> Self {
        let style = build_style_theme();
        let resolved = style.resolve(is_dark_mode);
        let ftui_colors = ThemeColors {
            bg_primary: resolved.background,
            bg_secondary: resolved.surface,
            healthy: resolved.success,
            warning: resolved.warning,
            critical: resolved.error,
            info: resolved.info,
            muted: resolved.text_muted,
            text: resolved.text,
            accent: resolved.primary,
            highlight: resolved.border_focused,
            border: resolved.border,
            claude: provider_claude(),
            codex: provider_codex(),
            gemini: provider_gemini(),
        };

        Self {
            style,
            resolved,
            ftui_colors,
            is_dark_mode,
        }
    }

    /// Build the theme using the terminal's detected preferred mode.
    #[must_use]
    pub fn detected() -> Self {
        Self::from_mode(FtuiTheme::detect_dark_mode())
    }

    /// Return the canonical `ftui-style` theme definition.
    #[must_use]
    pub fn style_theme(&self) -> &FtuiTheme {
        &self.style
    }

    /// Return the resolved `ftui-style` theme for the active mode.
    #[must_use]
    pub fn resolved_theme(&self) -> &FtuiResolvedTheme {
        &self.resolved
    }

    /// Return the resolved `ftui-style` color slots used by `vc_tui`.
    #[must_use]
    pub fn ftui_colors(&self) -> ThemeColors {
        self.ftui_colors
    }

    /// Return whether this theme was resolved in dark mode.
    #[must_use]
    pub fn is_dark_mode(&self) -> bool {
        self.is_dark_mode
    }

    /// Get the `ftui-style` color for a health score.
    #[must_use]
    pub fn health_color(&self, score: f64) -> FtuiColor {
        if score >= 0.8 {
            self.ftui_colors.healthy
        } else if score >= 0.5 {
            self.ftui_colors.warning
        } else {
            self.ftui_colors.critical
        }
    }

    /// Get health indicator character for a score.
    #[must_use]
    pub fn health_indicator(&self, score: f64) -> &'static str {
        if score >= 0.8 {
            "●"
        } else if score >= 0.5 {
            "◐"
        } else {
            "○"
        }
    }

    /// Get the `ftui-style` color for a provider name.
    #[must_use]
    pub fn provider_color(&self, provider: &str) -> FtuiColor {
        if provider.eq_ignore_ascii_case("claude") {
            self.ftui_colors.claude
        } else if provider.eq_ignore_ascii_case("codex") || provider.eq_ignore_ascii_case("openai")
        {
            self.ftui_colors.codex
        } else if provider.eq_ignore_ascii_case("gemini") || provider.eq_ignore_ascii_case("google")
        {
            self.ftui_colors.gemini
        } else {
            self.ftui_colors.muted
        }
    }
}

fn build_style_theme() -> FtuiTheme {
    ThemeBuilder::new()
        .primary(adaptive(rgb(130, 80, 223), rgb(136, 87, 229)))
        .secondary(adaptive(rgb(9, 105, 218), rgb(88, 166, 255)))
        .accent(adaptive(rgb(188, 92, 60), rgb(217, 119, 87)))
        .background(adaptive(rgb(255, 255, 255), rgb(13, 17, 23)))
        .surface(adaptive(rgb(246, 248, 250), rgb(22, 27, 34)))
        .overlay(adaptive(rgb(255, 255, 255), rgb(48, 54, 61)))
        .text(adaptive(rgb(31, 35, 40), rgb(230, 237, 243)))
        .text_muted(adaptive(rgb(87, 96, 106), rgb(139, 148, 158)))
        .text_subtle(adaptive(rgb(140, 149, 159), rgb(110, 118, 129)))
        .success(adaptive(rgb(26, 127, 55), rgb(63, 185, 80)))
        .warning(adaptive(rgb(158, 106, 3), rgb(210, 153, 34)))
        .error(adaptive(rgb(207, 34, 46), rgb(248, 81, 73)))
        .info(adaptive(rgb(9, 105, 218), rgb(88, 166, 255)))
        .border(adaptive(rgb(208, 215, 222), rgb(48, 54, 61)))
        .border_focused(adaptive(rgb(9, 105, 218), rgb(88, 166, 255)))
        .selection_bg(adaptive(rgb(221, 244, 255), rgb(56, 139, 253)))
        .selection_fg(adaptive(rgb(31, 35, 40), rgb(230, 237, 243)))
        .scrollbar_track(adaptive(rgb(246, 248, 250), rgb(22, 27, 34)))
        .scrollbar_thumb(adaptive(rgb(175, 184, 193), rgb(72, 79, 88)))
        .build()
}

const fn rgb(r: u8, g: u8, b: u8) -> FtuiColor {
    FtuiColor::rgb(r, g, b)
}

const fn adaptive(light: FtuiColor, dark: FtuiColor) -> AdaptiveColor {
    AdaptiveColor::adaptive(light, dark)
}

const fn provider_claude() -> FtuiColor {
    rgb(217, 119, 87)
}

const fn provider_codex() -> FtuiColor {
    rgb(16, 163, 127)
}

const fn provider_gemini() -> FtuiColor {
    rgb(66, 133, 244)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_ftui_rgb(color: FtuiColor, r: u8, g: u8, b: u8) {
        assert_eq!(color.to_rgb().r, r);
        assert_eq!(color.to_rgb().g, g);
        assert_eq!(color.to_rgb().b, b);
    }

    #[test]
    fn test_theme_default_uses_dark_mode() {
        let theme = Theme::default();

        assert!(theme.is_dark_mode());
        assert!(theme.style_theme().background.is_adaptive());
        assert_ftui_rgb(theme.resolved_theme().background, 13, 17, 23);
        assert_ftui_rgb(theme.resolved_theme().text, 230, 237, 243);
        assert_ftui_rgb(theme.resolved_theme().border, 48, 54, 61);
    }

    #[test]
    fn test_theme_ftui_colors_match_dark_defaults() {
        let theme = Theme::default();
        let colors = theme.ftui_colors();

        assert_ftui_rgb(colors.bg_primary, 13, 17, 23);
        assert_ftui_rgb(colors.bg_secondary, 22, 27, 34);
        assert_ftui_rgb(colors.healthy, 63, 185, 80);
        assert_ftui_rgb(colors.warning, 210, 153, 34);
        assert_ftui_rgb(colors.critical, 248, 81, 73);
        assert_ftui_rgb(colors.info, 88, 166, 255);
        assert_ftui_rgb(colors.muted, 139, 148, 158);
        assert_ftui_rgb(colors.text, 230, 237, 243);
        assert_ftui_rgb(colors.accent, 136, 87, 229);
        assert_ftui_rgb(colors.highlight, 88, 166, 255);
        assert_ftui_rgb(colors.border, 48, 54, 61);
        assert_ftui_rgb(colors.claude, 217, 119, 87);
        assert_ftui_rgb(colors.codex, 16, 163, 127);
        assert_ftui_rgb(colors.gemini, 66, 133, 244);
    }

    #[test]
    fn test_theme_light_mode_resolves_adaptive_slots() {
        let theme = Theme::from_mode(false);
        let colors = theme.ftui_colors();

        assert!(!theme.is_dark_mode());
        assert_ftui_rgb(colors.bg_primary, 255, 255, 255);
        assert_ftui_rgb(colors.bg_secondary, 246, 248, 250);
        assert_ftui_rgb(colors.text, 31, 35, 40);
        assert_ftui_rgb(colors.border, 208, 215, 222);
        assert_ftui_rgb(colors.accent, 130, 80, 223);
    }

    #[test]
    fn test_health_color_healthy() {
        let theme = Theme::default();

        assert_eq!(theme.health_color(1.0), theme.ftui_colors().healthy);
        assert_eq!(theme.health_color(0.9), theme.ftui_colors().healthy);
        assert_eq!(theme.health_color(0.8), theme.ftui_colors().healthy);
    }

    #[test]
    fn test_health_color_warning() {
        let theme = Theme::default();

        assert_eq!(theme.health_color(0.79), theme.ftui_colors().warning);
        assert_eq!(theme.health_color(0.5), theme.ftui_colors().warning);
    }

    #[test]
    fn test_health_color_critical() {
        let theme = Theme::default();

        assert_eq!(theme.health_color(0.49), theme.ftui_colors().critical);
        assert_eq!(theme.health_color(0.0), theme.ftui_colors().critical);
    }

    #[test]
    fn test_health_indicator() {
        let theme = Theme::default();

        assert_eq!(theme.health_indicator(1.0), "●");
        assert_eq!(theme.health_indicator(0.8), "●");
        assert_eq!(theme.health_indicator(0.6), "◐");
        assert_eq!(theme.health_indicator(0.5), "◐");
        assert_eq!(theme.health_indicator(0.3), "○");
    }

    #[test]
    fn test_provider_color() {
        let theme = Theme::default();

        assert_eq!(theme.provider_color("claude"), theme.ftui_colors().claude);
        assert_eq!(theme.provider_color("Claude"), theme.ftui_colors().claude);
        assert_eq!(theme.provider_color("codex"), theme.ftui_colors().codex);
        assert_eq!(theme.provider_color("openai"), theme.ftui_colors().codex);
        assert_eq!(theme.provider_color("gemini"), theme.ftui_colors().gemini);
        assert_eq!(theme.provider_color("google"), theme.ftui_colors().gemini);
        assert_eq!(theme.provider_color("unknown"), theme.ftui_colors().muted);
    }
}
