//! vc_tui - Terminal UI for Vibe Cockpit
//!
//! This crate provides:
//! - ratatui-based terminal interface
//! - Multiple screens (overview, machines, repos, alerts, etc.)
//! - Real-time updates
//! - Keyboard navigation

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod screens;
pub mod theme;
pub mod widgets;

pub use screens::{render_overview, OverviewData};
pub use theme::Theme;

/// TUI errors
#[derive(Error, Debug)]
pub enum TuiError {
    #[error("Terminal error: {0}")]
    TerminalError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Query error: {0}")]
    QueryError(#[from] vc_query::QueryError),
}

/// Available screens
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Screen {
    Overview,
    Machines,
    Repos,
    Accounts,
    Sessions,
    Mail,
    Alerts,
    Guardian,
    Oracle,
    Events,
    Beads,
    Settings,
    Help,
}

impl Screen {
    /// Get screen title
    pub fn title(&self) -> &'static str {
        match self {
            Screen::Overview => "Overview",
            Screen::Machines => "Machines",
            Screen::Repos => "Repositories",
            Screen::Accounts => "Accounts",
            Screen::Sessions => "Sessions",
            Screen::Mail => "Agent Mail",
            Screen::Alerts => "Alerts",
            Screen::Guardian => "Guardian",
            Screen::Oracle => "Oracle",
            Screen::Events => "Events",
            Screen::Beads => "Beads",
            Screen::Settings => "Settings",
            Screen::Help => "Help",
        }
    }

    /// Get keyboard shortcut
    pub fn shortcut(&self) -> Option<char> {
        match self {
            Screen::Overview => Some('o'),
            Screen::Machines => Some('m'),
            Screen::Repos => Some('r'),
            Screen::Accounts => Some('a'),
            Screen::Sessions => Some('s'),
            Screen::Mail => Some('l'),
            Screen::Alerts => Some('!'),
            Screen::Guardian => Some('g'),
            Screen::Oracle => Some('p'),
            Screen::Events => Some('e'),
            Screen::Beads => Some('b'),
            Screen::Settings => None,
            Screen::Help => Some('?'),
        }
    }

    /// All screens in order
    pub fn all() -> &'static [Screen] {
        &[
            Screen::Overview,
            Screen::Machines,
            Screen::Repos,
            Screen::Accounts,
            Screen::Sessions,
            Screen::Mail,
            Screen::Alerts,
            Screen::Guardian,
            Screen::Oracle,
            Screen::Events,
            Screen::Beads,
            Screen::Settings,
            Screen::Help,
        ]
    }
}

/// Application state
pub struct App {
    pub current_screen: Screen,
    pub should_quit: bool,
    pub last_error: Option<String>,
    pub theme: Theme,
    pub overview_data: OverviewData,
}

impl App {
    /// Create a new app instance
    pub fn new() -> Self {
        Self {
            current_screen: Screen::Overview,
            should_quit: false,
            last_error: None,
            theme: Theme::default(),
            overview_data: OverviewData::default(),
        }
    }

    /// Render the current screen
    pub fn render(&self, f: &mut Frame) {
        match self.current_screen {
            Screen::Overview => {
                render_overview(f, &self.overview_data, &self.theme);
            }
            _ => {
                // Placeholder for other screens - render a simple message
                use ratatui::widgets::{Block, Borders, Paragraph};
                let text = Paragraph::new(format!(
                    "Screen: {} - Press 'o' for Overview",
                    self.current_screen.title()
                ))
                .block(Block::default().title("Vibe Cockpit").borders(Borders::ALL));
                f.render_widget(text, f.area());
            }
        }
    }

    /// Handle keyboard input
    pub fn handle_key(&mut self, key: KeyEvent) {
        // Global shortcuts
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') | KeyCode::Char('q') => {
                    self.should_quit = true;
                    return;
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') => self.current_screen = Screen::Help,
            KeyCode::Char(c) => {
                // Check screen shortcuts
                for screen in Screen::all() {
                    if screen.shortcut() == Some(c) {
                        self.current_screen = *screen;
                        break;
                    }
                }
            }
            KeyCode::Tab => {
                // Cycle to next screen
                let screens = Screen::all();
                let current_idx = screens
                    .iter()
                    .position(|s| *s == self.current_screen)
                    .unwrap_or(0);
                let next_idx = (current_idx + 1) % screens.len();
                self.current_screen = screens[next_idx];
            }
            _ => {}
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================================================
    // Screen Tests
    // ==========================================================================

    #[test]
    fn test_screen_shortcuts() {
        assert_eq!(Screen::Overview.shortcut(), Some('o'));
        assert_eq!(Screen::Machines.shortcut(), Some('m'));
        assert_eq!(Screen::Repos.shortcut(), Some('r'));
        assert_eq!(Screen::Accounts.shortcut(), Some('a'));
        assert_eq!(Screen::Sessions.shortcut(), Some('s'));
        assert_eq!(Screen::Mail.shortcut(), Some('l'));
        assert_eq!(Screen::Alerts.shortcut(), Some('!'));
        assert_eq!(Screen::Guardian.shortcut(), Some('g'));
        assert_eq!(Screen::Oracle.shortcut(), Some('p'));
        assert_eq!(Screen::Events.shortcut(), Some('e'));
        assert_eq!(Screen::Beads.shortcut(), Some('b'));
        assert_eq!(Screen::Settings.shortcut(), None);
        assert_eq!(Screen::Help.shortcut(), Some('?'));
    }

    #[test]
    fn test_screen_titles() {
        assert_eq!(Screen::Overview.title(), "Overview");
        assert_eq!(Screen::Machines.title(), "Machines");
        assert_eq!(Screen::Repos.title(), "Repositories");
        assert_eq!(Screen::Accounts.title(), "Accounts");
        assert_eq!(Screen::Sessions.title(), "Sessions");
        assert_eq!(Screen::Mail.title(), "Agent Mail");
        assert_eq!(Screen::Alerts.title(), "Alerts");
        assert_eq!(Screen::Guardian.title(), "Guardian");
        assert_eq!(Screen::Oracle.title(), "Oracle");
        assert_eq!(Screen::Events.title(), "Events");
        assert_eq!(Screen::Beads.title(), "Beads");
        assert_eq!(Screen::Settings.title(), "Settings");
        assert_eq!(Screen::Help.title(), "Help");
    }

    #[test]
    fn test_screen_all() {
        let screens = Screen::all();
        assert_eq!(screens.len(), 13);
        assert_eq!(screens[0], Screen::Overview);
        assert_eq!(screens[screens.len() - 1], Screen::Help);
    }

    #[test]
    fn test_screen_serialization() {
        let screen = Screen::Overview;
        let json = serde_json::to_string(&screen).unwrap();
        assert_eq!(json, "\"Overview\"");

        let parsed: Screen = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, Screen::Overview);
    }

    #[test]
    fn test_all_screens_serialize_roundtrip() {
        for screen in Screen::all() {
            let json = serde_json::to_string(screen).unwrap();
            let parsed: Screen = serde_json::from_str(&json).unwrap();
            assert_eq!(*screen, parsed);
        }
    }

    // ==========================================================================
    // App Tests
    // ==========================================================================

    #[test]
    fn test_app_new() {
        let app = App::new();
        assert_eq!(app.current_screen, Screen::Overview);
        assert!(!app.should_quit);
        assert!(app.last_error.is_none());
    }

    #[test]
    fn test_app_default() {
        let app1 = App::new();
        let app2 = App::default();
        assert_eq!(app1.current_screen, app2.current_screen);
        assert_eq!(app1.should_quit, app2.should_quit);
        // Theme and overview_data use defaults
        assert_eq!(app1.overview_data.fleet_health, app2.overview_data.fleet_health);
    }

    #[test]
    fn test_app_quit() {
        let mut app = App::new();
        assert!(!app.should_quit);
        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(app.should_quit);
    }

    #[test]
    fn test_app_quit_ctrl_c() {
        let mut app = App::new();
        assert!(!app.should_quit);
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.should_quit);
    }

    #[test]
    fn test_app_quit_ctrl_q() {
        let mut app = App::new();
        assert!(!app.should_quit);
        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL));
        assert!(app.should_quit);
    }

    #[test]
    fn test_app_screen_navigation_shortcuts() {
        let mut app = App::new();

        // Navigate to Machines with 'm'
        app.handle_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
        assert_eq!(app.current_screen, Screen::Machines);

        // Navigate to Repos with 'r'
        app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
        assert_eq!(app.current_screen, Screen::Repos);

        // Navigate to Help with '?'
        app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
        assert_eq!(app.current_screen, Screen::Help);

        // Navigate back to Overview with 'o'
        app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
        assert_eq!(app.current_screen, Screen::Overview);
    }

    #[test]
    fn test_app_tab_navigation() {
        let mut app = App::new();
        assert_eq!(app.current_screen, Screen::Overview);

        // Tab should move to next screen
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.current_screen, Screen::Machines);

        // Tab again
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.current_screen, Screen::Repos);
    }

    #[test]
    fn test_app_tab_wraps_around() {
        let mut app = App::new();
        let screens = Screen::all();

        // Navigate to last screen
        app.current_screen = screens[screens.len() - 1];
        assert_eq!(app.current_screen, Screen::Help);

        // Tab should wrap to first screen
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.current_screen, Screen::Overview);
    }

    #[test]
    fn test_app_unknown_key_no_effect() {
        let mut app = App::new();
        let initial_screen = app.current_screen;

        // Random key should not change state
        app.handle_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE));
        assert_eq!(app.current_screen, initial_screen);
        assert!(!app.should_quit);
    }

    #[test]
    fn test_app_all_screen_shortcuts_work() {
        for screen in Screen::all() {
            if let Some(shortcut) = screen.shortcut() {
                let mut app = App::new();
                app.handle_key(KeyEvent::new(KeyCode::Char(shortcut), KeyModifiers::NONE));
                assert_eq!(
                    app.current_screen, *screen,
                    "Shortcut '{}' should navigate to {:?}",
                    shortcut, screen
                );
            }
        }
    }

    // ==========================================================================
    // TuiError Tests
    // ==========================================================================

    #[test]
    fn test_tui_error_display() {
        let err = TuiError::TerminalError("resize failed".to_string());
        assert_eq!(format!("{}", err), "Terminal error: resize failed");
    }

    #[test]
    fn test_tui_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::Other, "test");
        let tui_err: TuiError = io_err.into();
        assert!(matches!(tui_err, TuiError::IoError(_)));
    }
}
