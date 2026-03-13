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
pub mod machines;
pub mod mail;
pub mod oracle;
pub mod overview;
pub mod rch;
pub mod sessions;
pub mod settings;

pub use accounts::{AccountSortField, AccountStatus, AccountsData};
pub use alerts::{AlertInfo, AlertRuleInfo, AlertStats, AlertViewMode, AlertsData, Severity};
pub use beads::{BeadsData, BlockerItem, GraphHealthData, QuickRefData, RecommendationItem};
pub use events::{
    DcgEvent, EventFilter, EventSection, EventSeverity, EventStats, EventsData, PtFinding,
    PtFindingType, RanoEvent, RanoEventType, TimeRange,
};
pub use guardian::{
    ActiveProtocol, GuardianData, GuardianMode, GuardianRun, GuardianSection, GuardianStatus,
    PendingApproval, ProtocolStatus, RunResult,
};
pub use machines::{
    CollectionEvent, MachineDetail, MachineOnlineStatus, MachineRow, MachineSortField,
    MachinesData, MachinesViewMode, SystemStats, ToolInfoRow,
};
pub use mail::{MailData, MailPane, MessageInfo, ThreadSummary};
pub use oracle::{
    CostTrajectory, FailureRisk, OracleData, OracleSection, RateForecast, ResourceForecast,
};
pub use overview::{AlertSummary, MachineStatus, OverviewData, RepoStatus};
pub use rch::{CacheStatus, CrateStats, RchBuild, RchData, RchSection, WorkerState, WorkerStatus};
pub use sessions::{SessionGroupBy, SessionInfo, SessionsData};
pub use settings::SettingsData;
