//! vc_tui - Terminal UI for Vibe Cockpit
//!
//! This crate provides:
//! - ratatui-based terminal interface
//! - Multiple screens (overview, machines, repos, alerts, etc.)
//! - Real-time updates
//! - Keyboard navigation

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::prelude::*;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod screens;
pub mod widgets;

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
}

impl App {
    /// Create a new app instance
    pub fn new() -> Self {
        Self {
            current_screen: Screen::Overview,
            should_quit: false,
            last_error: None,
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

    #[test]
    fn test_screen_shortcuts() {
        assert_eq!(Screen::Overview.shortcut(), Some('o'));
        assert_eq!(Screen::Settings.shortcut(), None);
    }

    #[test]
    fn test_app_quit() {
        let mut app = App::new();
        assert!(!app.should_quit);
        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(app.should_quit);
    }
}
