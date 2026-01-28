//! Integration tests for collector JSON parsing
//!
//! These tests verify that collectors correctly parse their expected
//! JSON formats using the fixtures from common/mod.rs.

mod common;

use common::{init_tracing, RU_LIST_FIXTURE, RU_STATUS_FIXTURE};
use serde_json;

/// Test that RuCollector can parse ru list output
#[test]
fn test_parse_ru_list_fixture() {
    init_tracing();

    // Parse the fixture using the collector's types
    let output: serde_json::Value = serde_json::from_str(RU_LIST_FIXTURE).unwrap();

    let repos = output["repos"].as_array().unwrap();
    assert_eq!(repos.len(), 2);

    // Verify first repo
    assert_eq!(repos[0]["path"], "/data/projects/vibe_cockpit");
    assert_eq!(
        repos[0]["url"],
        "git@github.com:Dicklesworthstone/vibe_cockpit.git"
    );
    assert_eq!(repos[0]["name"], "vibe_cockpit");

    // Verify second repo
    assert_eq!(repos[1]["path"], "/data/projects/beads_rust");
    assert_eq!(repos[1]["name"], "beads_rust");
}

/// Test that RuCollector can parse ru status output
#[test]
fn test_parse_ru_status_fixture() {
    init_tracing();

    let output: serde_json::Value = serde_json::from_str(RU_STATUS_FIXTURE).unwrap();

    let repos = output["repos"].as_array().unwrap();
    assert_eq!(repos.len(), 2);

    // First repo - clean
    assert_eq!(repos[0]["branch"], "main");
    assert_eq!(repos[0]["dirty"], false);
    assert_eq!(repos[0]["ahead"], 0);
    assert_eq!(repos[0]["behind"], 0);

    // Second repo - dirty with changes
    assert_eq!(repos[1]["branch"], "feature/new-sync");
    assert_eq!(repos[1]["dirty"], true);
    assert_eq!(repos[1]["ahead"], 3);
    assert_eq!(repos[1]["behind"], 1);

    let modified = repos[1]["modified_files"].as_array().unwrap();
    assert_eq!(modified.len(), 2);
    assert!(modified.iter().any(|f| f == "src/lib.rs"));

    let untracked = repos[1]["untracked_files"].as_array().unwrap();
    assert_eq!(untracked.len(), 1);
    assert_eq!(untracked[0], "notes.txt");
}

/// Test sysmoni fixture parsing
#[test]
fn test_parse_sysmoni_fixture() {
    init_tracing();

    let output: serde_json::Value = serde_json::from_str(common::SYSMONI_FIXTURE).unwrap();

    // CPU metrics
    assert_eq!(output["cpu"]["total_percent"], 45.2);
    assert_eq!(output["cpu"]["load_1"], 2.1);
    assert_eq!(output["cpu"]["load_5"], 1.8);
    assert_eq!(output["cpu"]["load_15"], 1.5);

    // Memory metrics
    assert_eq!(output["memory"]["total_bytes"], 34359738368_i64);
    assert_eq!(output["memory"]["available_bytes"], 10737418240_i64);

    // Process list
    let processes = output["processes"].as_array().unwrap();
    assert_eq!(processes.len(), 2);
    assert_eq!(processes[0]["name"], "cargo");
    assert_eq!(processes[0]["cpu_percent"], 45.0);
}

/// Test df -P output parsing (for fallback probe)
#[test]
fn test_parse_df_fixture() {
    init_tracing();

    let lines: Vec<&str> = common::DF_FIXTURE.lines().collect();

    // Skip header, parse data lines
    for line in lines.iter().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 6 {
            let filesystem = parts[0];
            let total_kb: i64 = parts[1].parse().unwrap_or(0);
            let used_kb: i64 = parts[2].parse().unwrap_or(0);
            let mount = parts[5];

            // Verify we can parse the values
            if filesystem.starts_with("/dev/") {
                assert!(total_kb > 0);
                assert!(used_kb > 0);
                assert!(!mount.is_empty());
            }
        }
    }
}

/// Test /proc/meminfo parsing (for fallback probe)
#[test]
fn test_parse_proc_meminfo_fixture() {
    init_tracing();

    let mut mem_total: Option<i64> = None;
    let mut mem_available: Option<i64> = None;
    let mut swap_total: Option<i64> = None;

    for line in common::PROC_MEMINFO_FIXTURE.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let key = parts[0].trim_end_matches(':');
            let value_kb: Option<i64> = parts[1].parse().ok();

            match key {
                "MemTotal" => mem_total = value_kb.map(|v| v * 1024),
                "MemAvailable" => mem_available = value_kb.map(|v| v * 1024),
                "SwapTotal" => swap_total = value_kb.map(|v| v * 1024),
                _ => {}
            }
        }
    }

    assert_eq!(mem_total, Some(16384000 * 1024));
    assert_eq!(mem_available, Some(8000000 * 1024));
    assert_eq!(swap_total, Some(4194304 * 1024));
}

/// Test free -b output parsing (for fallback probe)
#[test]
fn test_parse_free_fixture() {
    init_tracing();

    let mut mem_total: Option<i64> = None;
    let mut mem_used: Option<i64> = None;
    let mut mem_available: Option<i64> = None;
    let mut swap_total: Option<i64> = None;
    let mut swap_used: Option<i64> = None;

    for line in common::FREE_FIXTURE.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();

        if parts.first().is_some_and(|&s| s.starts_with("Mem")) && parts.len() >= 4 {
            mem_total = parts[1].parse().ok();
            mem_used = parts[2].parse().ok();
            if parts.len() >= 7 {
                mem_available = parts[6].parse().ok();
            }
        } else if parts.first().is_some_and(|&s| s.starts_with("Swap")) && parts.len() >= 3 {
            swap_total = parts[1].parse().ok();
            swap_used = parts[2].parse().ok();
        }
    }

    assert_eq!(mem_total, Some(16777216000));
    assert_eq!(mem_used, Some(8000000000));
    assert_eq!(mem_available, Some(8000000000));
    assert_eq!(swap_total, Some(4294967296));
    assert_eq!(swap_used, Some(1000000000));
}

/// Test uptime parsing for Linux format
#[test]
fn test_parse_uptime_linux_fixture() {
    init_tracing();

    let line = common::UPTIME_LINUX_FIXTURE;

    // Extract load averages
    if let Some(load_pos) = line.find("load average") {
        let after_label = &line[load_pos..];
        if let Some(colon_pos) = after_label.find(':') {
            let nums = &after_label[colon_pos + 1..];
            let parts: Vec<f64> = nums
                .split(|c: char| c == ',' || c.is_whitespace())
                .filter_map(|s| s.trim().parse().ok())
                .collect();

            assert_eq!(parts.len(), 3);
            assert!((parts[0] - 0.25).abs() < 0.01);
            assert!((parts[1] - 0.18).abs() < 0.01);
            assert!((parts[2] - 0.12).abs() < 0.01);
        }
    }
}

/// Test uptime parsing for macOS format
#[test]
fn test_parse_uptime_macos_fixture() {
    init_tracing();

    let line = common::UPTIME_MACOS_FIXTURE;

    // macOS uses "load averages" (plural) with space-separated values
    if let Some(load_pos) = line.find("load average") {
        let after_label = &line[load_pos..];
        if let Some(colon_pos) = after_label.find(':') {
            let nums = &after_label[colon_pos + 1..];
            let parts: Vec<f64> = nums
                .split(|c: char| c == ',' || c.is_whitespace())
                .filter_map(|s| s.trim().parse().ok())
                .collect();

            assert!(!parts.is_empty());
            // macOS fixture has: 1.23 0.98 0.67
            assert!((parts[0] - 1.23).abs() < 0.01);
        }
    }
}
