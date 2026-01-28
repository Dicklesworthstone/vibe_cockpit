//! Screen implementations for the TUI
//!
//! Each screen module provides:
//! - A render function that draws the screen
//! - State management specific to that screen
//! - Input handling for screen-specific actions

pub mod accounts;
pub mod alerts;
pub mod beads;
pub mod events;
pub mod guardian;
pub mod mail;
pub mod oracle;
pub mod overview;
pub mod rch;
pub mod sessions;

pub use accounts::{AccountSortField, AccountStatus, AccountsData, render_accounts};
pub use alerts::{
    AlertInfo, AlertRuleInfo, AlertStats, AlertViewMode, AlertsData, Severity, render_alerts,
};
pub use beads::{
    BeadsData, BlockerItem, GraphHealthData, QuickRefData, RecommendationItem, render_beads,
};
pub use events::{
    DcgEvent, EventFilter, EventSection, EventSeverity, EventStats, EventsData, PtFinding,
    PtFindingType, RanoEvent, RanoEventType, TimeRange, render_events,
};
pub use guardian::{
    ActiveProtocol, GuardianData, GuardianMode, GuardianRun, GuardianSection, GuardianStatus,
    PendingApproval, ProtocolStatus, RunResult, render_guardian,
};
pub use mail::{MailData, MailPane, MessageInfo, ThreadSummary, render_mail};
pub use oracle::{
    CostTrajectory, FailureRisk, OracleData, OracleSection, RateForecast, ResourceForecast,
    render_oracle,
};
pub use overview::{AlertSummary, MachineStatus, OverviewData, RepoStatus, render_overview};
pub use rch::{
    CacheStatus, CrateStats, RchBuild, RchData, RchSection, WorkerState, WorkerStatus, render_rch,
};
pub use sessions::{SessionGroupBy, SessionInfo, SessionsData, render_sessions};
