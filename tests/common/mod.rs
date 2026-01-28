//! Common test utilities for vibe_cockpit integration tests.
//!
//! This module provides:
//! - Tracing initialization for test output
//! - Temporary database path generation
//! - Test configuration builders
//! - Mock data fixtures for collectors

use std::path::PathBuf;
use std::sync::Once;
use std::time::{SystemTime, UNIX_EPOCH};

use tracing_subscriber::{fmt, prelude::*, EnvFilter};

static INIT: Once = Once::new();

/// Initialize tracing once for integration tests.
pub fn init_tracing() {
    INIT.call_once(|| {
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
        tracing_subscriber::registry()
            .with(fmt::layer())
            .with(filter)
            .init();
    });
}

/// Generate a unique temporary DuckDB path for a test.
pub fn temp_db_path(test_name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("vc_{test_name}_{nanos}.duckdb"))
}

/// Build a default config with a test-scoped DB path.
pub fn temp_config(test_name: &str) -> vc_config::VcConfig {
    let mut config = vc_config::VcConfig::default();
    config.global.db_path = temp_db_path(test_name);
    config
}

// =============================================================================
// Mock Data Fixtures for Collectors
// =============================================================================

/// Sample ru list --json output for testing RuCollector
pub const RU_LIST_FIXTURE: &str = r#"{
    "repos": [
        {
            "path": "/data/projects/vibe_cockpit",
            "url": "git@github.com:Dicklesworthstone/vibe_cockpit.git",
            "name": "vibe_cockpit"
        },
        {
            "path": "/data/projects/beads_rust",
            "url": "git@github.com:Dicklesworthstone/beads_rust.git",
            "name": "beads_rust"
        }
    ]
}"#;

/// Sample ru status --no-fetch --json output for testing RuCollector
pub const RU_STATUS_FIXTURE: &str = r#"{
    "repos": [
        {
            "path": "/data/projects/vibe_cockpit",
            "url": "git@github.com:Dicklesworthstone/vibe_cockpit.git",
            "branch": "main",
            "dirty": false,
            "ahead": 0,
            "behind": 0,
            "modified_files": [],
            "untracked_files": []
        },
        {
            "path": "/data/projects/beads_rust",
            "url": "git@github.com:Dicklesworthstone/beads_rust.git",
            "branch": "feature/new-sync",
            "dirty": true,
            "ahead": 3,
            "behind": 1,
            "modified_files": ["src/lib.rs", "Cargo.toml"],
            "untracked_files": ["notes.txt"]
        }
    ]
}"#;

/// Sample sysmoni --json output for testing SysmoniCollector
pub const SYSMONI_FIXTURE: &str = r#"{
    "timestamp": "2026-01-28T00:00:00Z",
    "cpu": {
        "total_percent": 45.2,
        "per_core": [42.1, 48.3, 44.0, 46.4],
        "load_1": 2.1,
        "load_5": 1.8,
        "load_15": 1.5
    },
    "memory": {
        "total_bytes": 34359738368,
        "used_bytes": 23622320128,
        "available_bytes": 10737418240,
        "swap_total_bytes": 8589934592,
        "swap_used_bytes": 1073741824
    },
    "disk": {
        "read_bytes_per_sec": 1048576,
        "write_bytes_per_sec": 2097152,
        "filesystems": [
            {"mount": "/", "total_bytes": 500107862016, "used_bytes": 350075103232},
            {"mount": "/home", "total_bytes": 1000204886016, "used_bytes": 600122931200}
        ]
    },
    "network": {
        "rx_bytes_per_sec": 10485760,
        "tx_bytes_per_sec": 5242880
    },
    "processes": [
        {"pid": 1234, "name": "cargo", "cpu_percent": 45.0, "memory_bytes": 1073741824},
        {"pid": 5678, "name": "rust-analyzer", "cpu_percent": 12.5, "memory_bytes": 536870912}
    ]
}"#;

/// Sample uptime output (Linux format)
pub const UPTIME_LINUX_FIXTURE: &str =
    " 14:32:25 up 5 days, 3:45, 2 users, load average: 0.25, 0.18, 0.12";

/// Sample uptime output (macOS format)
pub const UPTIME_MACOS_FIXTURE: &str =
    "14:32  up 5 days,  3:45, 2 users, load averages: 1.23 0.98 0.67";

/// Sample df -P output
pub const DF_FIXTURE: &str = r#"Filesystem     1024-blocks      Used Available Capacity Mounted on
/dev/sda1       488378368 341064857 122460567      74% /
/dev/sdb1       976754560 585052736 342048256      63% /home
tmpfs            16384000         0  16384000       0% /dev/shm
"#;

/// Sample /proc/meminfo output
pub const PROC_MEMINFO_FIXTURE: &str = r#"MemTotal:       16384000 kB
MemFree:         1234567 kB
MemAvailable:    8000000 kB
Buffers:          500000 kB
Cached:          4000000 kB
SwapTotal:       4194304 kB
SwapFree:        4000000 kB
"#;

/// Sample free -b output
pub const FREE_FIXTURE: &str = r#"              total        used        free      shared  buff/cache   available
Mem:    16777216000  8000000000  2000000000   500000000  6000000000  8000000000
Swap:    4294967296  1000000000  3294967296
"#;
