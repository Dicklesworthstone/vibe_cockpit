//! Screen implementations for the TUI
//!
//! Each screen module provides:
//! - A render function that draws the screen
//! - State management specific to that screen
//! - Input handling for screen-specific actions

pub mod accounts;
pub mod beads;
pub mod mail;
pub mod oracle;
pub mod overview;
pub mod sessions;

pub use accounts::{AccountSortField, AccountStatus, AccountsData, render_accounts};
pub use beads::{
    BeadsData, BlockerItem, GraphHealthData, QuickRefData, RecommendationItem, render_beads,
};
pub use mail::{MailData, MailPane, MessageInfo, ThreadSummary, render_mail};
pub use oracle::{
    CostTrajectory, FailureRisk, OracleData, OracleSection, RateForecast, ResourceForecast,
    render_oracle,
};
pub use overview::{AlertSummary, MachineStatus, OverviewData, RepoStatus, render_overview};
pub use sessions::{SessionGroupBy, SessionInfo, SessionsData, render_sessions};
