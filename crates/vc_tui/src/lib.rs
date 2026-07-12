//! `vc_tui` - Terminal UI for Vibe Cockpit
//!
//! This crate provides:
//! - `FrankenTUI` Elm-architecture terminal interface (`Model` trait)
//! - Multiple screens (overview, machines, repos, alerts, etc.)
//! - Real-time updates via tick subscriptions
//! - Keyboard navigation

use serde::{Deserialize, Serialize};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use thiserror::Error;
use vc_config::VcConfig;
use vc_store::VcStore;

pub mod data;
pub mod screens;
pub mod theme;
pub mod widgets;

pub use screens::{
    AccountsData, AlertsData, BeadsData, EventsData, GuardianData, MachinesData, MailData,
    OracleData, OverviewData, RchData, SessionsData, SettingsData,
};
pub use theme::Theme;

/// Default dashboard refresh interval.
const TICK_INTERVAL: Duration = Duration::from_secs(5);

/// TUI errors
#[derive(Error, Debug)]
pub enum TuiError {
    #[error("Terminal error: {0}")]
    TerminalError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Query error: {0}")]
    QueryError(#[from] vc_query::QueryError),

    #[error("Store error: {0}")]
    StoreError(#[from] vc_store::StoreError),
}

/// Handles the TUI needs to fetch live data.
///
/// The store is shared with the background refresh tasks spawned by
/// [`ftui::Cmd::task`], so it is held behind an [`Arc`].
#[derive(Clone)]
pub struct AppContext {
    store: Arc<VcStore>,
    config: Arc<VcConfig>,
    config_source: String,
}

impl AppContext {
    /// Build a context from an already-opened store and the loaded config.
    ///
    /// `config_source` is the provenance string shown on the Settings screen
    /// (for example the config file path, or `"defaults"`).
    #[must_use]
    pub fn new(
        store: Arc<VcStore>,
        config: Arc<VcConfig>,
        config_source: impl Into<String>,
    ) -> Self {
        Self {
            store,
            config,
            config_source: config_source.into(),
        }
    }

    /// The shared store handle.
    #[must_use]
    pub fn store(&self) -> Arc<VcStore> {
        Arc::clone(&self.store)
    }
}

/// Runtime launch options for the `FrankenTUI` entry point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunOptions {
    /// Render below the current cursor instead of taking over the terminal.
    pub inline_mode: bool,
    /// Reserved height for inline mode.
    pub inline_height: u16,
    /// Request mouse support when the backend mode allows it.
    pub mouse_support: bool,
}

impl RunOptions {
    /// Resolve the ftui screen mode from the user-facing launch options.
    #[must_use]
    pub fn screen_mode(self) -> ftui::ScreenMode {
        if self.inline_mode {
            ftui::ScreenMode::Inline {
                ui_height: self.inline_height.max(1),
            }
        } else {
            ftui::ScreenMode::AltScreen
        }
    }
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            inline_mode: false,
            inline_height: 20,
            mouse_support: true,
        }
    }
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
    Rch,
    Settings,
    Help,
}

impl Screen {
    /// Get screen title
    #[must_use]
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
            Screen::Rch => "RCH Workers",
            Screen::Settings => "Settings",
            Screen::Help => "Help",
        }
    }

    /// Get the primary keyboard shortcut shown in UI hints.
    #[must_use]
    pub fn shortcut(&self) -> Option<char> {
        match self {
            Screen::Overview => Some('1'),
            Screen::Machines => Some('2'),
            Screen::Repos => Some('3'),
            Screen::Accounts => Some('4'),
            Screen::Sessions => Some('5'),
            Screen::Mail => Some('6'),
            Screen::Alerts => Some('7'),
            Screen::Guardian => Some('8'),
            Screen::Oracle => Some('9'),
            Screen::Events => Some('0'),
            Screen::Beads => Some('b'),
            Screen::Rch => Some('w'),
            Screen::Settings => Some('s'),
            Screen::Help => Some('?'),
        }
    }

    /// Resolve a screen from either the documented binding or legacy aliases.
    #[must_use]
    pub fn from_binding(input: char) -> Option<Self> {
        match input {
            '!' => Some(Self::Alerts),
            '?' => Some(Self::Help),
            _ => match input.to_ascii_lowercase() {
                '1' | 'o' => Some(Self::Overview),
                '2' | 'm' => Some(Self::Machines),
                '3' | 'r' => Some(Self::Repos),
                '4' | 'a' => Some(Self::Accounts),
                '5' => Some(Self::Sessions),
                '6' | 'l' => Some(Self::Mail),
                '7' => Some(Self::Alerts),
                '8' | 'g' => Some(Self::Guardian),
                '9' | 'p' => Some(Self::Oracle),
                '0' | 'e' => Some(Self::Events),
                'b' => Some(Self::Beads),
                'w' => Some(Self::Rch),
                's' => Some(Self::Settings),
                _ => None,
            },
        }
    }

    /// Whether this screen is backed by a real query against the store/config.
    ///
    /// Screens that return `false` have no collector query wired yet; the app
    /// renders an explicit "no data source yet" state for them instead of an
    /// empty dashboard that would look like real (but zeroed) data.
    #[must_use]
    pub fn has_data_source(self) -> bool {
        match self {
            // Backed by `data::load_*` (store) or `SettingsData::from_config`.
            Screen::Overview
            | Screen::Machines
            | Screen::Alerts
            | Screen::Sessions
            | Screen::Events
            | Screen::Settings
            | Screen::Help => true,
            // No query layer yet — see `Screen::missing_data_source_note`.
            Screen::Repos
            | Screen::Accounts
            | Screen::Mail
            | Screen::Guardian
            | Screen::Oracle
            | Screen::Beads
            | Screen::Rch => false,
        }
    }

    /// Explanation shown on screens that have no backing query yet.
    #[must_use]
    pub fn missing_data_source_note(self) -> &'static str {
        match self {
            Screen::Repos => {
                "Repo status lives in `repo_status_snapshots`, but this screen has no renderer or query yet. (The Overview repo panel does show it.)"
            }
            Screen::Accounts => {
                "No query for `account_usage_snapshots` / `account_profile_snapshots` is wired yet."
            }
            Screen::Mail => "No query for `mail_messages` / `mail_file_reservations` is wired yet.",
            Screen::Guardian => "No query for `guardian_runs` / `guardian_playbooks` is wired yet.",
            Screen::Oracle => "No query for `predictions` / `resolutions` is wired yet.",
            Screen::Beads => "No query for `beads_snapshot` / `beads_graph_metrics` is wired yet.",
            Screen::Rch => "No query for `rch_metrics` / `rch_compilations` is wired yet.",
            Screen::Overview
            | Screen::Machines
            | Screen::Alerts
            | Screen::Sessions
            | Screen::Events
            | Screen::Settings
            | Screen::Help => "",
        }
    }

    /// All screens in order
    #[must_use]
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
            Screen::Rch,
            Screen::Settings,
            Screen::Help,
        ]
    }

    #[must_use]
    fn next(self) -> Self {
        let screens = Self::all();
        let current_idx = screens
            .iter()
            .position(|screen| *screen == self)
            .unwrap_or(0);
        screens[(current_idx + 1) % screens.len()]
    }

    #[must_use]
    fn previous(self) -> Self {
        let screens = Self::all();
        let current_idx = screens
            .iter()
            .position(|screen| *screen == self)
            .unwrap_or(0);
        let previous_idx = current_idx.checked_sub(1).unwrap_or(screens.len() - 1);
        screens[previous_idx]
    }
}

// ==========================================================================
// Elm architecture: AppMessage + Model impl
// ==========================================================================

/// Messages that drive the Elm update loop.
#[derive(Debug)]
pub enum AppMessage {
    /// Terminal key event forwarded from the runtime.
    Key(ftui::KeyEvent),
    /// Periodic tick — triggers data refresh.
    Tick,
    /// Navigate to a specific screen.
    ScreenChanged(Screen),
    /// Fresh data arrived for a screen.
    DataRefreshed(ScreenData),
    /// An error occurred during an operation.
    Error(String),
    /// Quit the application.
    Quit,
}

/// Typed payload for screen data refreshes.
#[derive(Debug)]
pub enum ScreenData {
    Overview(Box<OverviewData>),
    Machines(Box<MachinesData>),
    Accounts(Box<AccountsData>),
    Sessions(Box<SessionsData>),
    Mail(Box<MailData>),
    Alerts(Box<AlertsData>),
    Guardian(Box<GuardianData>),
    Oracle(Box<OracleData>),
    Events(Box<EventsData>),
    Beads(Box<BeadsData>),
    Rch(Box<RchData>),
    Settings(Box<SettingsData>),
}

impl From<ftui::Event> for AppMessage {
    fn from(event: ftui::Event) -> Self {
        match event {
            ftui::Event::Key(k) => AppMessage::Key(k),
            ftui::Event::Tick => AppMessage::Tick,
            _ => {
                // Resize, Mouse, Paste, Focus, Clipboard, Ime → no-op key
                AppMessage::Key(ftui::KeyEvent::new(ftui::KeyCode::Null))
            }
        }
    }
}

/// Application state
pub struct App {
    pub current_screen: Screen,
    pub should_quit: bool,
    pub last_error: Option<String>,
    shutdown_requested: Option<Arc<AtomicBool>>,
    /// Store/config handles used to refresh screen data. `None` means the app
    /// was constructed without a data source (tests, or a caller that has no
    /// store); every data screen then renders the "no data source" state.
    context: Option<AppContext>,
    pub theme: Theme,
    // Screen data — all screens represented
    pub overview_data: OverviewData,
    pub machines_data: MachinesData,
    pub accounts_data: AccountsData,
    pub sessions_data: SessionsData,
    pub mail_data: MailData,
    pub alerts_data: AlertsData,
    pub guardian_data: GuardianData,
    pub oracle_data: OracleData,
    pub events_data: EventsData,
    pub beads_data: BeadsData,
    pub rch_data: RchData,
    pub settings_data: SettingsData,
}

impl App {
    /// Create a new app instance.
    ///
    /// Pass `Some(context)` to give the app a live store/config handle; the
    /// theme and Settings screen are then derived from the config and every
    /// tick refreshes the store-backed screens. Pass `None` for a detached app
    /// with no data source.
    #[must_use]
    pub fn new(context: Option<AppContext>) -> Self {
        let settings_data = context.as_ref().map_or_else(SettingsData::default, |ctx| {
            SettingsData::from_config(&ctx.config, ctx.config_source.clone())
        });
        let theme = context.as_ref().map_or_else(Theme::default, |ctx| {
            match ctx.config.tui.theme.to_ascii_lowercase().as_str() {
                "light" => Theme::from_mode(false),
                "dark" => Theme::from_mode(true),
                _ => Theme::default(),
            }
        });

        Self {
            current_screen: Screen::Overview,
            should_quit: false,
            last_error: None,
            shutdown_requested: None,
            context,
            theme,
            overview_data: OverviewData::default(),
            machines_data: MachinesData::default(),
            accounts_data: AccountsData::default(),
            sessions_data: SessionsData::default(),
            mail_data: MailData::default(),
            alerts_data: AlertsData::default(),
            guardian_data: GuardianData::default(),
            oracle_data: OracleData::default(),
            events_data: EventsData::default(),
            beads_data: BeadsData::default(),
            rch_data: RchData::default(),
            settings_data,
        }
    }

    /// Create a new app instance that can be asked to quit externally.
    #[must_use]
    pub fn with_shutdown_flag(
        context: Option<AppContext>,
        shutdown_requested: Arc<AtomicBool>,
    ) -> Self {
        let mut app = Self::new(context);
        app.shutdown_requested = Some(shutdown_requested);
        app
    }

    /// Build the refresh command executed on every tick.
    ///
    /// Each store-backed screen gets its own background task so a slow or
    /// failing query cannot block the others or the render loop. The trailing
    /// [`ftui::Cmd::tick`] re-arms the refresh interval.
    fn refresh_cmd(&self) -> ftui::Cmd<AppMessage> {
        let Some(context) = self.context.as_ref() else {
            // No data source: nothing to fetch, just keep the clock running.
            return ftui::Cmd::tick(TICK_INTERVAL);
        };

        let mut cmds: Vec<ftui::Cmd<AppMessage>> = Vec::with_capacity(6);

        let store = context.store();
        cmds.push(ftui::Cmd::task_named(
            "vc_tui.load_overview",
            move || match data::load_overview(&store) {
                Ok(overview) => AppMessage::DataRefreshed(ScreenData::Overview(Box::new(overview))),
                Err(err) => AppMessage::Error(format!("overview refresh failed: {err}")),
            },
        ));

        let store = context.store();
        cmds.push(ftui::Cmd::task_named(
            "vc_tui.load_machines",
            move || match data::load_machines(&store) {
                Ok(machines) => AppMessage::DataRefreshed(ScreenData::Machines(Box::new(machines))),
                Err(err) => AppMessage::Error(format!("machines refresh failed: {err}")),
            },
        ));

        let store = context.store();
        cmds.push(ftui::Cmd::task_named(
            "vc_tui.load_alerts",
            move || match data::load_alerts(&store) {
                Ok(alerts) => AppMessage::DataRefreshed(ScreenData::Alerts(Box::new(alerts))),
                Err(err) => AppMessage::Error(format!("alerts refresh failed: {err}")),
            },
        ));

        let store = context.store();
        cmds.push(ftui::Cmd::task_named(
            "vc_tui.load_sessions",
            move || match data::load_sessions(&store) {
                Ok(sessions) => AppMessage::DataRefreshed(ScreenData::Sessions(Box::new(sessions))),
                Err(err) => AppMessage::Error(format!("sessions refresh failed: {err}")),
            },
        ));

        let store = context.store();
        cmds.push(ftui::Cmd::task_named(
            "vc_tui.load_events",
            move || match data::load_events(&store) {
                Ok(events) => AppMessage::DataRefreshed(ScreenData::Events(Box::new(events))),
                Err(err) => AppMessage::Error(format!("events refresh failed: {err}")),
            },
        ));

        cmds.push(ftui::Cmd::tick(TICK_INTERVAL));
        ftui::Cmd::batch(cmds)
    }

    /// Render the explicit "no data source yet" state for an unbacked screen.
    fn render_no_data_source(&self, frame: &mut ftui::Frame, screen: Screen) {
        let title = format!("Vibe Cockpit | {}", screen.title());
        Self::write_line(&mut frame.buffer, 0, &title);
        Self::write_line(&mut frame.buffer, 2, "NO DATA SOURCE YET");
        Self::write_line(&mut frame.buffer, 3, screen.missing_data_source_note());
        Self::write_line(
            &mut frame.buffer,
            4,
            "Nothing is fetched for this screen, so nothing is shown. Live screens: 1 Overview, 2 Machines, 5 Sessions, 7 Alerts, 0 Events, s Settings.",
        );

        if let Some(ref err) = self.last_error {
            let err_line = format!("Error: {err}");
            Self::write_line(&mut frame.buffer, 6, &err_line);
        }

        let nav = "1:Overview 2:Machines 3:Repos 4:Accounts 5:Sessions 6:Mail 7:Alerts 8:Guardian 9:Oracle 0:Events b:Beads w:RCH s:Settings ?:Help q:Quit";
        let bottom_y = frame.height().saturating_sub(1);
        Self::write_line(&mut frame.buffer, bottom_y, nav);
    }

    /// Write a string into an ftui buffer at the given row.
    fn write_line(buf: &mut ftui::Buffer, y: u16, text: &str) {
        for (i, ch) in text.chars().enumerate() {
            let Ok(x) = u16::try_from(i) else {
                break;
            };
            if x < buf.width() && y < buf.height() {
                buf.set(x, y, ftui::Cell::from_char(ch));
            }
        }
    }
}

impl ftui::Model for App {
    type Message = AppMessage;

    fn init(&mut self) -> ftui::Cmd<Self::Message> {
        // Start on Overview, schedule first data load
        self.current_screen = Screen::Overview;
        ftui::Cmd::msg(AppMessage::Tick)
    }

    fn update(&mut self, msg: Self::Message) -> ftui::Cmd<Self::Message> {
        match msg {
            AppMessage::Key(k) => self.handle_ftui_key(k),
            AppMessage::Tick => self.refresh_cmd(),
            AppMessage::ScreenChanged(screen) => {
                self.current_screen = screen;
                ftui::Cmd::none()
            }
            AppMessage::DataRefreshed(data) => {
                // A successful refresh clears any stale failure banner.
                self.last_error = None;
                match data {
                    ScreenData::Overview(d) => self.overview_data = *d,
                    ScreenData::Machines(d) => self.machines_data = *d,
                    ScreenData::Accounts(d) => self.accounts_data = *d,
                    ScreenData::Sessions(d) => self.sessions_data = *d,
                    ScreenData::Mail(d) => self.mail_data = *d,
                    ScreenData::Alerts(d) => self.alerts_data = *d,
                    ScreenData::Guardian(d) => self.guardian_data = *d,
                    ScreenData::Oracle(d) => self.oracle_data = *d,
                    ScreenData::Events(d) => self.events_data = *d,
                    ScreenData::Beads(d) => self.beads_data = *d,
                    ScreenData::Rch(d) => self.rch_data = *d,
                    ScreenData::Settings(d) => self.settings_data = *d,
                }
                ftui::Cmd::none()
            }
            AppMessage::Error(e) => {
                self.last_error = Some(e);
                ftui::Cmd::none()
            }
            AppMessage::Quit => {
                self.should_quit = true;
                ftui::Cmd::quit()
            }
        }
    }

    fn view(&self, frame: &mut ftui::Frame) {
        match self.current_screen {
            Screen::Overview => {
                crate::screens::overview::render_overview_ftui(
                    frame,
                    &self.overview_data,
                    &self.theme,
                );
            }
            Screen::Machines => {
                crate::screens::machines::render_machines_ftui(
                    frame,
                    &self.machines_data,
                    &self.theme,
                );
            }
            Screen::Sessions => {
                crate::screens::sessions::render_sessions_ftui(
                    frame,
                    &self.sessions_data,
                    &self.theme,
                );
            }
            Screen::Alerts => {
                crate::screens::alerts::render_alerts_ftui(frame, &self.alerts_data, &self.theme);
            }
            Screen::Events => {
                crate::screens::events::render_events_ftui(frame, &self.events_data, &self.theme);
            }
            Screen::Settings => {
                crate::screens::settings::render_settings_ftui(
                    frame,
                    &self.settings_data,
                    &self.theme,
                );
            }
            Screen::Help => {
                let title = format!("Vibe Cockpit | {}", self.current_screen.title());
                Self::write_line(&mut frame.buffer, 0, &title);
                Self::write_line(
                    &mut frame.buffer,
                    2,
                    "Keyboard shortcuts: 1-9, 0, b, w, s, ? | Tab / Shift+Tab cycle | Esc returns | q quits",
                );

                if let Some(ref err) = self.last_error {
                    let err_line = format!("Error: {err}");
                    Self::write_line(&mut frame.buffer, 4, &err_line);
                }

                let nav = "1:Overview 2:Machines 3:Repos 4:Accounts 5:Sessions 6:Mail 7:Alerts 8:Guardian 9:Oracle 0:Events b:Beads w:RCH s:Settings ?:Help q:Quit";
                let bottom_y = frame.height().saturating_sub(1);
                Self::write_line(&mut frame.buffer, bottom_y, nav);
            }
            // Screens with no backing query: say so explicitly rather than
            // rendering an empty dashboard that reads as real, zeroed data.
            screen @ (Screen::Repos
            | Screen::Accounts
            | Screen::Mail
            | Screen::Guardian
            | Screen::Oracle
            | Screen::Beads
            | Screen::Rch) => {
                self.render_no_data_source(frame, screen);
            }
        }
    }

    fn subscriptions(&self) -> Vec<Box<dyn ftui::runtime::Subscription<Self::Message>>> {
        let mut subscriptions: Vec<Box<dyn ftui::runtime::Subscription<Self::Message>>> =
            vec![Box::new(ftui::runtime::Every::new(TICK_INTERVAL, || {
                AppMessage::Tick
            }))];

        if let Some(shutdown_requested) = &self.shutdown_requested {
            subscriptions.push(Box::new(ShutdownSubscription {
                shutdown_requested: Arc::clone(shutdown_requested),
            }));
        }

        subscriptions
    }
}

struct ShutdownSubscription {
    shutdown_requested: Arc<AtomicBool>,
}

impl ftui::runtime::Subscription<AppMessage> for ShutdownSubscription {
    fn id(&self) -> ftui::runtime::SubId {
        0x5643_5f53_4855_5444
    }

    fn run(&self, sender: std::sync::mpsc::Sender<AppMessage>, stop: ftui::runtime::StopSignal) {
        while !stop.wait_timeout(Duration::from_millis(50)) {
            if self.shutdown_requested.load(Ordering::Acquire) {
                let _ = sender.send(AppMessage::Quit);
                break;
            }
        }
    }
}

fn run_app_with_options(app: App, options: RunOptions) -> Result<(), TuiError> {
    let screen_mode = options.screen_mode();
    tracing::info!(
        inline_mode = options.inline_mode,
        inline_height = options.inline_height,
        mouse_support = options.mouse_support,
        ?screen_mode,
        "starting vc_tui"
    );

    let builder = ftui::App::new(app).screen_mode(screen_mode);
    let builder = if options.mouse_support {
        builder
    } else {
        builder.with_mouse_enabled(false)
    };

    builder.run().map_err(TuiError::from)
}

/// Run the TUI application with an external shutdown flag.
///
/// `context` carries the store/config handles the dashboard queries on every
/// tick. Passing `None` runs the TUI with no data source.
///
/// # Errors
///
/// Returns [`TuiError`] if terminal setup or the `FrankenTUI` runtime fails.
pub fn run_with_options_and_shutdown_flag(
    options: RunOptions,
    context: Option<AppContext>,
    shutdown_requested: Arc<AtomicBool>,
) -> Result<(), TuiError> {
    run_app_with_options(
        App::with_shutdown_flag(context, shutdown_requested),
        options,
    )
}

/// Run the TUI application with the requested screen mode.
///
/// # Errors
///
/// Returns [`TuiError`] if terminal setup or the `FrankenTUI` runtime fails.
pub fn run_with_options(options: RunOptions, context: Option<AppContext>) -> Result<(), TuiError> {
    run_app_with_options(App::new(context), options)
}
impl App {
    /// Handle an ftui key event (Elm path).
    fn handle_ftui_key(&mut self, key: ftui::KeyEvent) -> ftui::Cmd<AppMessage> {
        // Quit shortcuts
        if key.ctrl() && matches!(key.code, ftui::KeyCode::Char('c' | 'C' | 'q' | 'Q')) {
            return ftui::Cmd::msg(AppMessage::Quit);
        }

        match key.code {
            ftui::KeyCode::Char('q') => ftui::Cmd::msg(AppMessage::Quit),
            ftui::KeyCode::Tab if key.shift() => {
                ftui::Cmd::msg(AppMessage::ScreenChanged(self.current_screen.previous()))
            }
            ftui::KeyCode::Tab => {
                ftui::Cmd::msg(AppMessage::ScreenChanged(self.current_screen.next()))
            }
            ftui::KeyCode::BackTab => {
                ftui::Cmd::msg(AppMessage::ScreenChanged(self.current_screen.previous()))
            }
            ftui::KeyCode::Escape => ftui::Cmd::msg(AppMessage::ScreenChanged(Screen::Overview)),
            ftui::KeyCode::Char(c) => {
                if c == 'q' {
                    return ftui::Cmd::msg(AppMessage::Quit);
                }
                if let Some(screen) = Screen::from_binding(c) {
                    return ftui::Cmd::msg(AppMessage::ScreenChanged(screen));
                }
                ftui::Cmd::none()
            }
            _ => ftui::Cmd::none(),
        }
    }
}

impl Default for App {
    /// A detached app with no data source.
    fn default() -> Self {
        Self::new(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply_cmd(app: &mut App, cmd: ftui::Cmd<AppMessage>) {
        use ftui::Model;

        match cmd {
            ftui::Cmd::None
            | ftui::Cmd::Quit
            | ftui::Cmd::Tick(_)
            | ftui::Cmd::Log(_)
            | ftui::Cmd::SaveState
            | ftui::Cmd::RestoreState
            | ftui::Cmd::SetMouseCapture(_)
            | ftui::Cmd::SetTickStrategy(_) => {}
            ftui::Cmd::Msg(msg) => {
                let follow_up = app.update(msg);
                apply_cmd(app, follow_up);
            }
            ftui::Cmd::Batch(cmds) | ftui::Cmd::Sequence(cmds) => {
                for next in cmds {
                    apply_cmd(app, next);
                }
            }
            ftui::Cmd::Task(_, _) => panic!("unexpected background task in model test"),
        }
    }

    // ==========================================================================
    // Screen Tests
    // ==========================================================================

    #[test]
    fn test_screen_shortcuts() {
        assert_eq!(Screen::Overview.shortcut(), Some('1'));
        assert_eq!(Screen::Machines.shortcut(), Some('2'));
        assert_eq!(Screen::Repos.shortcut(), Some('3'));
        assert_eq!(Screen::Accounts.shortcut(), Some('4'));
        assert_eq!(Screen::Sessions.shortcut(), Some('5'));
        assert_eq!(Screen::Mail.shortcut(), Some('6'));
        assert_eq!(Screen::Alerts.shortcut(), Some('7'));
        assert_eq!(Screen::Guardian.shortcut(), Some('8'));
        assert_eq!(Screen::Oracle.shortcut(), Some('9'));
        assert_eq!(Screen::Events.shortcut(), Some('0'));
        assert_eq!(Screen::Beads.shortcut(), Some('b'));
        assert_eq!(Screen::Rch.shortcut(), Some('w'));
        assert_eq!(Screen::Settings.shortcut(), Some('s'));
        assert_eq!(Screen::Help.shortcut(), Some('?'));
    }

    #[test]
    fn test_screen_from_binding_supports_documented_keys_and_aliases() {
        assert_eq!(Screen::from_binding('1'), Some(Screen::Overview));
        assert_eq!(Screen::from_binding('5'), Some(Screen::Sessions));
        assert_eq!(Screen::from_binding('0'), Some(Screen::Events));
        assert_eq!(Screen::from_binding('b'), Some(Screen::Beads));
        assert_eq!(Screen::from_binding('w'), Some(Screen::Rch));
        assert_eq!(Screen::from_binding('s'), Some(Screen::Settings));
        assert_eq!(Screen::from_binding('o'), Some(Screen::Overview));
        assert_eq!(Screen::from_binding('m'), Some(Screen::Machines));
        assert_eq!(Screen::from_binding('!'), Some(Screen::Alerts));
        assert_eq!(Screen::from_binding('?'), Some(Screen::Help));
        assert_eq!(Screen::from_binding('z'), None);
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
        assert_eq!(Screen::Rch.title(), "RCH Workers");
        assert_eq!(Screen::Settings.title(), "Settings");
        assert_eq!(Screen::Help.title(), "Help");
    }

    #[test]
    fn test_screen_all() {
        let screens = Screen::all();
        assert_eq!(screens.len(), 14);
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
    fn test_screen_rch_serialization() {
        let screen = Screen::Rch;
        let json = serde_json::to_string(&screen).unwrap();
        assert_eq!(json, "\"Rch\"");

        let parsed: Screen = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, Screen::Rch);
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
    // App State Tests
    // ==========================================================================

    #[test]
    fn test_app_new() {
        let app = App::new(None);
        assert_eq!(app.current_screen, Screen::Overview);
        assert!(!app.should_quit);
        assert!(app.last_error.is_none());
    }

    #[test]
    fn test_app_default() {
        let app1 = App::new(None);
        let app2 = App::default();
        assert_eq!(app1.current_screen, app2.current_screen);
        assert_eq!(app1.should_quit, app2.should_quit);
        assert!(
            (app1.overview_data.fleet_health - app2.overview_data.fleet_health).abs()
                < f64::EPSILON
        );
    }

    #[test]
    fn test_app_with_shutdown_flag_registers_shutdown_subscription() {
        use ftui::Model;

        let shutdown_requested = Arc::new(AtomicBool::new(false));
        let app = App::with_shutdown_flag(None, shutdown_requested);
        let subscriptions = app.subscriptions();

        assert_eq!(subscriptions.len(), 2);
    }

    // ==========================================================================
    // Model Tests
    // ==========================================================================

    #[test]
    fn test_model_init_returns_overview() {
        use ftui::Model;
        let mut app = App::new(None);
        let _cmd = app.init();
        assert_eq!(app.current_screen, Screen::Overview);
    }

    #[test]
    fn test_model_init_returns_tick_cmd() {
        use ftui::Model;
        let mut app = App::new(None);
        let cmd = app.init();
        // init() returns Cmd::Msg(Tick) to trigger first data load
        assert!(matches!(cmd, ftui::Cmd::Msg(AppMessage::Tick)));
    }

    #[test]
    fn test_model_update_screen_changed() {
        use ftui::Model;
        let mut app = App::new(None);
        assert_eq!(app.current_screen, Screen::Overview);

        let cmd = app.update(AppMessage::ScreenChanged(Screen::Machines));
        assert!(matches!(cmd, ftui::Cmd::None));
        assert_eq!(app.current_screen, Screen::Machines);
    }

    #[test]
    fn test_model_update_key_tab_cycles() {
        use ftui::Model;
        let mut app = App::new(None);
        assert_eq!(app.current_screen, Screen::Overview);

        let cmd = app.update(AppMessage::Key(ftui::KeyEvent::new(ftui::KeyCode::Tab)));
        // Should produce a ScreenChanged message for Machines
        assert!(matches!(
            cmd,
            ftui::Cmd::Msg(AppMessage::ScreenChanged(Screen::Machines))
        ));
    }

    #[test]
    fn test_model_update_key_shortcut_navigates() {
        use ftui::Model;
        let mut app = App::new(None);

        let cmd = app.update(AppMessage::Key(ftui::KeyEvent::new(ftui::KeyCode::Char(
            '0',
        ))));
        assert!(matches!(
            cmd,
            ftui::Cmd::Msg(AppMessage::ScreenChanged(Screen::Events))
        ));
    }

    #[test]
    fn test_model_update_key_settings_shortcut_navigates() {
        use ftui::Model;
        let mut app = App::new(None);

        let cmd = app.update(AppMessage::Key(ftui::KeyEvent::new(ftui::KeyCode::Char(
            's',
        ))));
        assert!(matches!(
            cmd,
            ftui::Cmd::Msg(AppMessage::ScreenChanged(Screen::Settings))
        ));
    }

    #[test]
    fn test_model_update_tick_returns_tick_cmd() {
        use ftui::Model;
        let mut app = App::new(None);

        let cmd = app.update(AppMessage::Tick);
        assert!(matches!(cmd, ftui::Cmd::Tick(_)));
    }

    fn test_context() -> AppContext {
        let store = Arc::new(VcStore::open_memory().expect("in-memory store"));
        AppContext::new(store, Arc::new(VcConfig::default()), "test")
    }

    #[test]
    fn test_tick_with_context_spawns_refresh_tasks() {
        use ftui::Model;
        let mut app = App::new(Some(test_context()));

        let cmd = app.update(AppMessage::Tick);
        let ftui::Cmd::Batch(cmds) = cmd else {
            panic!("tick with a data source must return a batch of refresh tasks");
        };

        let tasks = cmds
            .iter()
            .filter(|cmd| matches!(cmd, ftui::Cmd::Task(..)))
            .count();
        // overview, machines, alerts, sessions, events
        assert_eq!(tasks, 5);
        // The batch re-arms the refresh interval.
        assert!(matches!(cmds.last(), Some(ftui::Cmd::Tick(_))));
    }

    #[test]
    fn test_tick_without_context_only_reticks() {
        use ftui::Model;
        let mut app = App::new(None);

        let cmd = app.update(AppMessage::Tick);
        assert!(matches!(cmd, ftui::Cmd::Tick(_)));
    }

    #[test]
    fn test_context_populates_settings_screen_from_config() {
        let app = App::new(Some(test_context()));
        assert_eq!(app.settings_data.config_source, "test");
    }

    #[test]
    fn test_backed_screens_are_exactly_the_wired_ones() {
        for screen in Screen::all() {
            let backed = screen.has_data_source();
            let expected = matches!(
                screen,
                Screen::Overview
                    | Screen::Machines
                    | Screen::Alerts
                    | Screen::Sessions
                    | Screen::Events
                    | Screen::Settings
                    | Screen::Help
            );
            assert_eq!(backed, expected, "{screen:?}");
            assert_eq!(
                screen.missing_data_source_note().is_empty(),
                backed,
                "{screen:?} note"
            );
        }
    }

    #[test]
    fn test_unbacked_screen_renders_no_data_source_notice() {
        use ftui::Model;
        let mut pool = ftui::GraphemePool::default();
        let mut app = App::new(None);
        app.current_screen = Screen::Beads;

        let mut frame = ftui::Frame::new(120, 24, &mut pool);
        app.view(&mut frame);

        let row: String = (0..frame.buffer.width())
            .map(|x| {
                frame
                    .buffer
                    .get(x, 2)
                    .and_then(|cell| cell.content.as_char())
                    .unwrap_or(' ')
            })
            .collect();
        assert!(
            row.contains("NO DATA SOURCE YET"),
            "unbacked screen must say so: {row:?}"
        );
    }

    #[test]
    fn test_model_update_data_refreshed() {
        use ftui::Model;
        let mut app = App::new(None);

        let new_overview = OverviewData {
            fleet_health: 95.0,
            ..OverviewData::default()
        };
        let cmd = app.update(AppMessage::DataRefreshed(ScreenData::Overview(Box::new(
            new_overview,
        ))));
        assert!(matches!(cmd, ftui::Cmd::None));
        assert!((app.overview_data.fleet_health - 95.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_model_update_key_tab_applies_screen_change() {
        use ftui::Model;
        let mut app = App::new(None);

        let cmd = app.update(AppMessage::Key(ftui::KeyEvent::new(ftui::KeyCode::Tab)));
        apply_cmd(&mut app, cmd);

        assert_eq!(app.current_screen, Screen::Machines);
    }

    #[test]
    fn test_model_update_key_shortcut_applies_screen_change() {
        use ftui::Model;
        let mut app = App::new(None);

        let cmd = app.update(AppMessage::Key(ftui::KeyEvent::new(ftui::KeyCode::Char(
            '8',
        ))));
        apply_cmd(&mut app, cmd);

        assert_eq!(app.current_screen, Screen::Guardian);
    }

    #[test]
    fn test_model_update_key_escape_applies_screen_change() {
        use ftui::Model;
        let mut app = App::new(None);
        app.current_screen = Screen::Oracle;

        let cmd = app.update(AppMessage::Key(ftui::KeyEvent::new(ftui::KeyCode::Escape)));
        apply_cmd(&mut app, cmd);

        assert_eq!(app.current_screen, Screen::Overview);
    }

    #[test]
    fn test_model_update_key_ctrl_c_applies_quit() {
        use ftui::Model;
        let mut app = App::new(None);

        let key =
            ftui::KeyEvent::new(ftui::KeyCode::Char('c')).with_modifiers(ftui::Modifiers::CTRL);
        let cmd = app.update(AppMessage::Key(key));
        apply_cmd(&mut app, cmd);

        assert!(app.should_quit);
    }

    #[test]
    fn test_model_update_quit() {
        use ftui::Model;
        let mut app = App::new(None);

        let cmd = app.update(AppMessage::Quit);
        assert!(app.should_quit);
        assert!(matches!(cmd, ftui::Cmd::Quit));
    }

    #[test]
    fn test_model_update_error() {
        use ftui::Model;
        let mut app = App::new(None);
        assert!(app.last_error.is_none());

        let _cmd = app.update(AppMessage::Error("test error".to_string()));
        assert_eq!(app.last_error.as_deref(), Some("test error"));
    }

    #[test]
    fn test_model_update_key_q_quits() {
        use ftui::Model;
        let mut app = App::new(None);

        let cmd = app.update(AppMessage::Key(ftui::KeyEvent::new(ftui::KeyCode::Char(
            'q',
        ))));
        assert!(matches!(cmd, ftui::Cmd::Msg(AppMessage::Quit)));
    }

    #[test]
    fn test_model_update_key_ctrl_c_quits() {
        use ftui::Model;
        let mut app = App::new(None);

        let key =
            ftui::KeyEvent::new(ftui::KeyCode::Char('c')).with_modifiers(ftui::Modifiers::CTRL);
        let cmd = app.update(AppMessage::Key(key));
        assert!(matches!(cmd, ftui::Cmd::Msg(AppMessage::Quit)));
    }

    #[test]
    fn test_model_update_key_backtab_cycles_backward() {
        use ftui::Model;
        let mut app = App::new(None);

        let cmd = app.update(AppMessage::Key(ftui::KeyEvent::new(ftui::KeyCode::BackTab)));
        assert!(matches!(
            cmd,
            ftui::Cmd::Msg(AppMessage::ScreenChanged(Screen::Help))
        ));
    }

    #[test]
    fn test_model_update_key_shift_tab_cycles_backward() {
        use ftui::Model;
        let mut app = App::new(None);

        let key = ftui::KeyEvent::new(ftui::KeyCode::Tab).with_modifiers(ftui::Modifiers::SHIFT);
        let cmd = app.update(AppMessage::Key(key));
        assert!(matches!(
            cmd,
            ftui::Cmd::Msg(AppMessage::ScreenChanged(Screen::Help))
        ));
    }

    #[test]
    fn test_model_update_key_escape_returns_overview() {
        use ftui::Model;
        let mut app = App::new(None);
        app.current_screen = Screen::Guardian;

        let cmd = app.update(AppMessage::Key(ftui::KeyEvent::new(ftui::KeyCode::Escape)));
        assert!(matches!(
            cmd,
            ftui::Cmd::Msg(AppMessage::ScreenChanged(Screen::Overview))
        ));
    }

    #[test]
    fn test_model_update_key_unknown_returns_none() {
        use ftui::Model;
        let mut app = App::new(None);

        let cmd = app.update(AppMessage::Key(ftui::KeyEvent::new(ftui::KeyCode::Left)));
        assert!(matches!(cmd, ftui::Cmd::None));
    }

    #[test]
    fn test_model_update_key_enter_is_currently_a_noop() {
        use ftui::Model;
        let mut app = App::new(None);
        app.current_screen = Screen::Mail;

        let cmd = app.update(AppMessage::Key(ftui::KeyEvent::new(ftui::KeyCode::Enter)));
        assert!(matches!(cmd, ftui::Cmd::None));
        assert_eq!(app.current_screen, Screen::Mail);
        assert!(!app.should_quit);
    }

    #[test]
    fn test_model_view_dispatches_all_screens() {
        use ftui::Model;
        let mut pool = ftui::GraphemePool::default();

        for screen in Screen::all() {
            let mut app = App::new(None);
            app.current_screen = *screen;
            let mut frame = ftui::Frame::new(80, 24, &mut pool);
            // Should not panic for any screen
            app.view(&mut frame);
        }
    }

    #[test]
    fn test_model_subscriptions_returns_tick() {
        use ftui::Model;
        let app = App::new(None);
        let subs = app.subscriptions();
        assert_eq!(subs.len(), 1);
    }

    #[test]
    fn test_from_event_key() {
        let event = ftui::Event::Key(ftui::KeyEvent::new(ftui::KeyCode::Char('x')));
        let msg: AppMessage = event.into();
        assert!(matches!(msg, AppMessage::Key(_)));
    }

    #[test]
    fn test_from_event_tick() {
        let event = ftui::Event::Tick;
        let msg: AppMessage = event.into();
        assert!(matches!(msg, AppMessage::Tick));
    }

    #[test]
    fn test_from_event_resize_becomes_key_null() {
        let event = ftui::Event::Resize {
            width: 80,
            height: 24,
        };
        let msg: AppMessage = event.into();
        assert!(matches!(msg, AppMessage::Key(_)));
    }

    // ==========================================================================
    // TuiError Tests
    // ==========================================================================

    #[test]
    fn test_tui_error_display() {
        let err = TuiError::TerminalError("resize failed".to_string());
        assert_eq!(format!("{err}"), "Terminal error: resize failed");
    }

    #[test]
    fn test_tui_error_from_io() {
        let io_err = std::io::Error::other("test");
        let tui_err: TuiError = io_err.into();
        assert!(matches!(tui_err, TuiError::IoError(_)));
    }

    #[test]
    fn test_run_options_default_to_alt_screen() {
        let options = RunOptions::default();
        assert_eq!(options.screen_mode(), ftui::ScreenMode::AltScreen);
    }

    #[test]
    fn test_run_options_inline_mode_uses_configured_height() {
        let options = RunOptions {
            inline_mode: true,
            inline_height: 18,
            mouse_support: true,
        };

        assert_eq!(
            options.screen_mode(),
            ftui::ScreenMode::Inline { ui_height: 18 }
        );
    }

    #[test]
    fn test_run_options_inline_mode_clamps_height() {
        let options = RunOptions {
            inline_mode: true,
            inline_height: 0,
            mouse_support: true,
        };

        assert_eq!(
            options.screen_mode(),
            ftui::ScreenMode::Inline { ui_height: 1 }
        );
    }
}
