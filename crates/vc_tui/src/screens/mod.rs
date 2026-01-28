//! Screen implementations for the TUI
//!
//! Each screen module provides:
//! - A render function that draws the screen
//! - State management specific to that screen
//! - Input handling for screen-specific actions

pub mod overview;

pub use overview::{render_overview, OverviewData};
