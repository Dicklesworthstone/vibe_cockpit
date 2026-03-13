#[path = "../common/mod.rs"]
mod common;

use ftui::{Cmd, Frame, GraphemePool, KeyCode, KeyEvent, Model};
use ftui_harness::{MatchMode, assert_buffer_snapshot, buffer_to_text};
use vc_tui::{
    App, AppMessage, Screen,
    screens::{
        accounts, alerts, beads, events, guardian, machines, mail, oracle, overview, rch, sessions,
        settings,
    },
};

const SNAPSHOT_WIDTH: u16 = 120;
const SNAPSHOT_HEIGHT: u16 = 32;
const VC_TUI_SNAPSHOT_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/crates/vc_tui");

#[test]
fn render_all_screens_with_default_state_matches_empty_snapshots() {
    common::init_tracing();

    for screen in Screen::all() {
        let mut app = App::new();
        app.current_screen = *screen;

        let text = render_screen(&app, &format!("{}_empty", screen_slug(*screen)));
        assert!(
            text.chars().any(|ch| !ch.is_whitespace()),
            "screen {screen:?} rendered only whitespace"
        );
    }
}

#[test]
fn render_all_screens_with_mock_data_matches_populated_snapshots() {
    common::init_tracing();

    for screen in Screen::all() {
        let mut app = sample_app();
        app.current_screen = *screen;

        let text = render_screen(&app, &format!("{}_populated", screen_slug(*screen)));
        assert!(
            text.contains(expected_marker(*screen)),
            "screen {screen:?} missing expected marker {:?}",
            expected_marker(*screen)
        );
    }
}

#[test]
fn navigation_shortcuts_visit_each_documented_screen() {
    common::init_tracing();

    let mut app = App::new();
    let sequence = [
        ('1', Screen::Overview),
        ('2', Screen::Machines),
        ('3', Screen::Repos),
        ('4', Screen::Accounts),
        ('5', Screen::Sessions),
        ('6', Screen::Mail),
        ('7', Screen::Alerts),
        ('8', Screen::Guardian),
        ('9', Screen::Oracle),
        ('0', Screen::Events),
        ('b', Screen::Beads),
        ('w', Screen::Rch),
        ('s', Screen::Settings),
        ('?', Screen::Help),
    ];

    for (binding, expected) in sequence {
        dispatch_key(&mut app, KeyEvent::new(KeyCode::Char(binding)));
        assert_eq!(
            app.current_screen, expected,
            "binding {binding:?} should navigate to {expected:?}"
        );
    }

    dispatch_key(&mut app, KeyEvent::new(KeyCode::Escape));
    assert_eq!(app.current_screen, Screen::Overview);
}

fn render_screen(app: &App, snapshot_name: &str) -> String {
    let mut pool = GraphemePool::default();
    let mut frame = Frame::new(SNAPSHOT_WIDTH, SNAPSHOT_HEIGHT, &mut pool);
    app.view(&mut frame);
    assert_buffer_snapshot(
        snapshot_name,
        &frame.buffer,
        VC_TUI_SNAPSHOT_ROOT,
        MatchMode::TrimTrailing,
    );
    buffer_to_text(&frame.buffer)
}

fn dispatch_key(app: &mut App, key: KeyEvent) {
    let cmd = app.update(AppMessage::Key(key));
    apply_cmd(app, cmd);
}

fn apply_cmd(app: &mut App, cmd: Cmd<AppMessage>) {
    match cmd {
        Cmd::None
        | Cmd::Quit
        | Cmd::Tick(_)
        | Cmd::Log(_)
        | Cmd::SaveState
        | Cmd::RestoreState
        | Cmd::SetMouseCapture(_)
        | Cmd::SetTickStrategy(_) => {}
        Cmd::Msg(msg) => {
            let follow_up = app.update(msg);
            apply_cmd(app, follow_up);
        }
        Cmd::Batch(cmds) | Cmd::Sequence(cmds) => {
            for next in cmds {
                apply_cmd(app, next);
            }
        }
        Cmd::Task(_, _) => panic!("unexpected background task in navigation test"),
    }
}

fn expected_marker(screen: Screen) -> &'static str {
    match screen {
        Screen::Overview => "Disk trending high",
        Screen::Machines => "orko",
        Screen::Repos => "Repository status and sync state",
        Screen::Accounts => "max-5",
        Screen::Sessions => "CobaltTurtle",
        Screen::Mail => "BeigeMink",
        Screen::Alerts => "High CPU usage",
        Screen::Guardian => "with-approval",
        Screen::Oracle => "Backup: max-6",
        Screen::Events => "rm -rf target",
        Screen::Beads => "Ready: 9",
        Screen::Rch => "vmi1152480",
        Screen::Settings => "/etc/vibe_cockpit/vibe.toml",
        Screen::Help => "Keyboard shortcuts:",
    }
}

fn screen_slug(screen: Screen) -> &'static str {
    match screen {
        Screen::Overview => "overview",
        Screen::Machines => "machines",
        Screen::Repos => "repos",
        Screen::Accounts => "accounts",
        Screen::Sessions => "sessions",
        Screen::Mail => "mail",
        Screen::Alerts => "alerts",
        Screen::Guardian => "guardian",
        Screen::Oracle => "oracle",
        Screen::Events => "events",
        Screen::Beads => "beads",
        Screen::Rch => "rch",
        Screen::Settings => "settings",
        Screen::Help => "help",
    }
}

fn sample_app() -> App {
    let mut app = App::new();
    app.overview_data = sample_overview_data();
    app.machines_data = sample_machines_data();
    app.accounts_data = sample_accounts_data();
    app.sessions_data = sample_sessions_data();
    app.mail_data = sample_mail_data();
    app.alerts_data = sample_alerts_data();
    app.guardian_data = sample_guardian_data();
    app.oracle_data = sample_oracle_data();
    app.events_data = sample_events_data();
    app.beads_data = sample_beads_data();
    app.rch_data = sample_rch_data();
    app.settings_data = sample_settings_data();
    app.last_error = Some("collector stale: rano".to_string());
    app
}

fn sample_overview_data() -> overview::OverviewData {
    overview::OverviewData {
        fleet_health: 0.82,
        machines: vec![overview::MachineStatus {
            id: "m1".to_string(),
            hostname: "orko".to_string(),
            online: true,
            cpu_pct: Some(45.0),
            mem_pct: Some(63.0),
            health_score: 0.9,
        }],
        alerts: vec![overview::AlertSummary {
            severity: "warning".to_string(),
            title: "Disk trending high".to_string(),
            machine_id: Some("m1".to_string()),
        }],
        repos: vec![overview::RepoStatus {
            name: "vibe_cockpit".to_string(),
            branch: "main".to_string(),
            is_dirty: false,
            ahead: 1,
            behind: 0,
            modified_count: 0,
        }],
        refresh_age_secs: 12,
    }
}

fn sample_machines_data() -> machines::MachinesData {
    machines::MachinesData {
        view_mode: machines::MachinesViewMode::List,
        machines: vec![
            machines::MachineRow {
                machine_id: "m1".to_string(),
                hostname: "orko".to_string(),
                display_name: Some("Orko".to_string()),
                status: machines::MachineOnlineStatus::Online,
                tool_count: 8,
                tags: vec!["local".to_string(), "collector".to_string()],
                is_local: true,
                enabled: true,
                ..machines::MachineRow::default()
            },
            machines::MachineRow {
                machine_id: "m2".to_string(),
                hostname: "gpu-box".to_string(),
                status: machines::MachineOnlineStatus::Offline,
                tool_count: 2,
                tags: vec!["remote".to_string()],
                enabled: true,
                ..machines::MachineRow::default()
            },
        ],
        selected_index: 0,
        selected_detail: None,
        sort_field: machines::MachineSortField::Hostname,
        sort_ascending: true,
        tag_filter: None,
        refresh_age_secs: 10,
    }
}

fn sample_accounts_data() -> accounts::AccountsData {
    accounts::AccountsData {
        accounts: vec![
            accounts::AccountStatus {
                program: "claude".to_string(),
                account: "max-5".to_string(),
                usage: 80,
                limit: Some(100),
                usage_pct: Some(80.0),
                rate_status: "yellow".to_string(),
                last_switch: Some("2m ago".to_string()),
                is_active: true,
                usage_trend: vec![10, 25, 40, 55, 70],
            },
            accounts::AccountStatus {
                program: "codex".to_string(),
                account: "pro".to_string(),
                usage: 15,
                limit: Some(50),
                usage_pct: Some(30.0),
                rate_status: "green".to_string(),
                last_switch: Some("15m ago".to_string()),
                is_active: false,
                usage_trend: vec![5, 10, 15, 20, 25],
            },
        ],
        selected: 0,
        filter: String::new(),
        sort_by: accounts::AccountSortField::Usage,
    }
}

fn sample_sessions_data() -> sessions::SessionsData {
    sessions::SessionsData {
        sessions: vec![
            sessions::SessionInfo {
                id: "sess-1".to_string(),
                project: "/dp/vibe_cockpit".to_string(),
                model: "gpt-5-codex".to_string(),
                agent: "CobaltTurtle".to_string(),
                started_at: "2026-03-13T10:00:00Z".to_string(),
                duration_mins: 95,
                tokens: 125_000,
                cost: 1.42,
                is_active: true,
                last_activity: "30s ago".to_string(),
            },
            sessions::SessionInfo {
                id: "sess-2".to_string(),
                project: "/dp/frankentui".to_string(),
                model: "claude-opus".to_string(),
                agent: "YellowBay".to_string(),
                started_at: "2026-03-13T09:20:00Z".to_string(),
                duration_mins: 40,
                tokens: 48_000,
                cost: 0.58,
                is_active: false,
                last_activity: "9m ago".to_string(),
            },
        ],
        selected: 0,
        group_by: sessions::SessionGroupBy::Project,
        filter: String::new(),
        expanded_groups: vec!["/dp/vibe_cockpit".to_string()],
    }
}

fn sample_mail_data() -> mail::MailData {
    mail::MailData {
        threads: vec![mail::ThreadSummary {
            id: "bd-6q0".to_string(),
            subject: "[bd-6q0] Start: deterministic TUI snapshots".to_string(),
            participant_count: 3,
            participants: vec![
                "CobaltTurtle".to_string(),
                "BeigeMink".to_string(),
                "YellowBay".to_string(),
            ],
            message_count: 2,
            unacked_count: 0,
            last_activity: "just now".to_string(),
            has_urgent: false,
        }],
        selected_thread: 0,
        messages: vec![
            mail::MessageInfo {
                id: 1,
                from: "CobaltTurtle".to_string(),
                to: vec!["BeigeMink".to_string(), "YellowBay".to_string()],
                subject: "[bd-6q0] Start".to_string(),
                body_preview: "Starting snapshot harness wiring.".to_string(),
                timestamp: "11:55".to_string(),
                importance: "normal".to_string(),
                ack_required: false,
                acknowledged: true,
            },
            mail::MessageInfo {
                id: 2,
                from: "BeigeMink".to_string(),
                to: vec!["CobaltTurtle".to_string()],
                subject: "Re: [bd-6q0]".to_string(),
                body_preview: "No overlap from my side.".to_string(),
                timestamp: "11:56".to_string(),
                importance: "normal".to_string(),
                ack_required: false,
                acknowledged: true,
            },
        ],
        selected_message: 0,
        active_pane: mail::MailPane::Threads,
        agent_activity: vec![
            ("CobaltTurtle".to_string(), 4),
            ("BeigeMink".to_string(), 2),
            ("YellowBay".to_string(), 1),
        ],
        filter: String::new(),
    }
}

fn sample_alerts_data() -> alerts::AlertsData {
    alerts::AlertsData {
        active_alerts: vec![alerts::AlertInfo {
            id: 7,
            rule_id: "cpu_high".to_string(),
            title: "High CPU usage".to_string(),
            message: "orko is above 90% CPU".to_string(),
            severity: alerts::Severity::High,
            fired_at: "2026-03-13T11:20:00Z".to_string(),
            age: "5m".to_string(),
            machine_id: Some("orko".to_string()),
            acknowledged: false,
            resolved_at: None,
            context: Some("collector=process_triage".to_string()),
        }],
        recent_alerts: vec![alerts::AlertInfo {
            id: 8,
            rule_id: "mail_backlog".to_string(),
            title: "Mail backlog cleared".to_string(),
            message: "Ack queue returned to normal".to_string(),
            severity: alerts::Severity::Info,
            fired_at: "2026-03-13T11:00:00Z".to_string(),
            age: "25m".to_string(),
            machine_id: None,
            acknowledged: true,
            resolved_at: Some("2m ago".to_string()),
            context: None,
        }],
        rules: vec![alerts::AlertRuleInfo {
            rule_id: "cpu_high".to_string(),
            name: "High CPU".to_string(),
            description: "CPU above 90% for 3m".to_string(),
            severity: alerts::Severity::High,
            enabled: true,
            muted: false,
            check_interval: 60,
            cooldown: 300,
            fired_24h: 4,
        }],
        selected_index: 0,
        view_mode: alerts::AlertViewMode::Active,
        severity_filter: None,
        stats: alerts::AlertStats {
            rules_enabled: 12,
            rules_muted: 1,
            rules_custom: 4,
            alerts_24h: 18,
            critical_active: 1,
        },
    }
}

fn sample_guardian_data() -> guardian::GuardianData {
    guardian::GuardianData {
        status: guardian::GuardianStatus {
            mode: guardian::GuardianMode::WithApproval,
            enabled: true,
            active_patterns: 6,
            last_action: Some("4m ago".to_string()),
            success_rate_7d: 92.0,
            successful_runs: 34,
            total_runs: 37,
        },
        active_protocols: vec![guardian::ActiveProtocol {
            playbook_id: "pb-ssh-recover".to_string(),
            name: "SSH Recovery".to_string(),
            machine_id: "orko".to_string(),
            current_step: 2,
            total_steps: 5,
            step_description: "Restarting agent mux".to_string(),
            started_ago: "3m ago".to_string(),
            status: guardian::ProtocolStatus::Running,
        }],
        pending_approvals: vec![guardian::PendingApproval {
            id: 41,
            playbook_id: "pb-disk-clean".to_string(),
            playbook_name: "Disk Cleanup".to_string(),
            machine_id: "gpu-box".to_string(),
            action_description: "Delete orphaned build cache".to_string(),
            reason: "Disk at 96%".to_string(),
            queued_ago: "1m ago".to_string(),
        }],
        recent_runs: vec![guardian::GuardianRun {
            id: 99,
            playbook_id: "pb-rch-drain".to_string(),
            playbook_name: "Drain unhealthy worker".to_string(),
            machine_id: "vmi1149989".to_string(),
            result: guardian::RunResult::Success,
            completed_ago: "12m ago".to_string(),
            summary: "Worker drained after build failures.".to_string(),
        }],
        selected_section: guardian::GuardianSection::Status,
        selected_index: 0,
    }
}

fn sample_oracle_data() -> oracle::OracleData {
    oracle::OracleData {
        rate_forecasts: vec![oracle::RateForecast {
            provider: "claude".to_string(),
            account: "max-5".to_string(),
            usage_pct: 86.0,
            minutes_to_limit: Some(47),
            recommendation: Some("Switch to backup before lunch".to_string()),
            backup_account: Some("max-6".to_string()),
        }],
        failure_risks: vec![oracle::FailureRisk {
            agent_id: "CobaltTurtle".to_string(),
            machine: "orko".to_string(),
            risk_pct: 18.0,
            minutes_to_failure: Some(180),
            indicators: vec![
                "rate-limit pressure".to_string(),
                "rising retry count".to_string(),
            ],
            past_occurrences: 2,
            status: "watch".to_string(),
        }],
        cost_trajectory: oracle::CostTrajectory {
            today_spent: 34.5,
            today_budget: 50.0,
            today_projection: 41.0,
            week_spent: 190.0,
            week_budget: 250.0,
            savings_suggestion: Some("Pause idle snapshot workers overnight".to_string()),
            on_track: true,
        },
        resource_forecasts: vec![oracle::ResourceForecast {
            machine: "orko".to_string(),
            resource: "disk".to_string(),
            current_pct: 72.0,
            projected_pct: 84.0,
            projection_days: 7,
            alert: Some("Clean up old build artifacts before Friday".to_string()),
        }],
        selected_section: oracle::OracleSection::RateLimits,
        refresh_age_secs: 60,
    }
}

fn sample_events_data() -> events::EventsData {
    events::EventsData {
        dcg_events: vec![events::DcgEvent {
            id: 1,
            machine_id: "orko".to_string(),
            command: "rm -rf target".to_string(),
            reason: "Dangerous command".to_string(),
            severity: events::EventSeverity::Critical,
            timestamp: "2026-03-13T10:00:00Z".to_string(),
            age: "2m".to_string(),
            source: Some("claude-code".to_string()),
        }],
        rano_events: vec![events::RanoEvent {
            id: 2,
            machine_id: "orko".to_string(),
            event_type: events::RanoEventType::AuthLoop,
            remote_host: "api.anthropic.com".to_string(),
            process: "claude-code".to_string(),
            pid: 4_242,
            connection_count: 9,
            timestamp: "2026-03-13T10:01:00Z".to_string(),
            age: "1m".to_string(),
            severity: events::EventSeverity::High,
            details: Some("Repeated login attempts".to_string()),
        }],
        pt_findings: vec![events::PtFinding {
            id: 3,
            machine_id: "orko".to_string(),
            finding_type: events::PtFindingType::Runaway,
            process_name: "cargo test".to_string(),
            pid: 31_337,
            details: "Saturating local CPU".to_string(),
            severity: events::EventSeverity::Medium,
            timestamp: "2026-03-13T10:02:00Z".to_string(),
            age: "30s".to_string(),
            metric_value: Some("CPU 390%".to_string()),
        }],
        selected_section: events::EventSection::Rano,
        selected_index: 0,
        filter: events::EventFilter::default(),
        time_range: events::TimeRange::Hour24,
        stats: events::EventStats {
            dcg_total: 1,
            dcg_critical: 1,
            rano_total: 1,
            pt_total: 1,
            machines_affected: 1,
        },
    }
}

fn sample_beads_data() -> beads::BeadsData {
    beads::BeadsData {
        quick_ref: beads::QuickRefData {
            open_count: 42,
            actionable_count: 9,
            blocked_count: 11,
            in_progress_count: 4,
            epics_with_ready: 3,
            total_epics: 7,
            by_priority: [1, 7, 18, 16],
        },
        recommendations: vec![beads::RecommendationItem {
            id: "bd-6q0".to_string(),
            title: "Create deterministic snapshot tests".to_string(),
            priority: 1,
            score: 0.91,
            unblocks_count: 3,
            status: "in_progress".to_string(),
            reason: "All ftui screen-port beads are complete".to_string(),
        }],
        blockers: vec![beads::BlockerItem {
            id: "bd-bvt".to_string(),
            title: "Port VcStore to fsqlite".to_string(),
            unblocks_count: 5,
            is_actionable: true,
            blocked_by: vec![],
        }],
        graph_health: beads::GraphHealthData {
            node_count: 83,
            edge_count: 112,
            density: 0.033,
            has_cycles: false,
            closed_last_7d: 14,
            closed_last_30d: 39,
            avg_days_to_close: 3.4,
        },
        selected_section: 1,
        selected_recommendation: 0,
        selected_blocker: 0,
        refresh_age_secs: 15,
    }
}

fn sample_rch_data() -> rch::RchData {
    rch::RchData {
        workers: vec![
            rch::WorkerStatus {
                name: "vmi1152480".to_string(),
                state: rch::WorkerState::Building,
                current_crate: Some("vc_tui".to_string()),
                jobs_24h: 42,
                avg_build_time: 18.4,
                last_seen: Some("just now".to_string()),
            },
            rch::WorkerStatus {
                name: "vmi1149989".to_string(),
                state: rch::WorkerState::Offline,
                current_crate: None,
                jobs_24h: 37,
                avg_build_time: 22.0,
                last_seen: Some("drained".to_string()),
            },
        ],
        recent_builds: vec![rch::RchBuild {
            time: "11:52".to_string(),
            crate_name: "vc_tui".to_string(),
            worker: "vmi1152480".to_string(),
            duration_secs: 11.6,
            cache_status: rch::CacheStatus::Hit,
            success: true,
        }],
        slowest_crates: vec![rch::CrateStats {
            name: "duckdb".to_string(),
            avg_time_secs: 95.4,
            build_count: 6,
            bar_pct: 100,
        }],
        cache_hit_rate: 0.82,
        builds_24h: 79,
        selected_section: rch::RchSection::Builds,
        selected_index: 0,
    }
}

fn sample_settings_data() -> settings::SettingsData {
    settings::SettingsData {
        config_source: "/etc/vibe_cockpit/vibe.toml".to_string(),
        config_paths: vec![
            "/etc/vibe_cockpit/vibe.toml".to_string(),
            "/home/ubuntu/.config/vibe_cockpit/vibe.toml".to_string(),
        ],
        db_path: "/var/lib/vibe_cockpit/vibe.duckdb".to_string(),
        runtime: settings::RuntimeSettings {
            poll_interval_secs: 30,
            collector_timeout_secs: 15,
            theme_name: "ember".to_string(),
            inline_mode: false,
            inline_height: 20,
        },
        fleet: settings::FleetSettings {
            enabled_collectors: 14,
            enabled_machines: 3,
            alerting_enabled: true,
            autopilot_enabled: true,
        },
        web: settings::WebSettings {
            enabled: true,
            bind: "0.0.0.0".to_string(),
            port: 8088,
            cors_enabled: true,
        },
        lint: settings::LintSummary {
            errors: 0,
            warnings: 1,
            info: 2,
        },
    }
}
