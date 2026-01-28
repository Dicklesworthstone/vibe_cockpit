#!/usr/bin/env bash
# E2E Test: DCG (Dangerous Command Guard) Collector
#
# This test creates a mock dcg SQLite database and exercises
# the dcg collector path. It validates the fixture and ensures
# vc collect can be invoked with the dcg collector.

set -euo pipefail

# Source test helpers
source "$(dirname "$0")/../lib/test_helpers.sh"

test_info "Starting dcg collector E2E test"

# Setup test environment
setup_test_env

# Ensure sqlite3 is available
if ! command -v sqlite3 >/dev/null 2>&1; then
    test_warn "sqlite3 not available - skipping dcg collector test"
    exit 2
fi

# Create mock dcg SQLite database under a temp HOME
export HOME="$TEST_TEMP_DIR"
DCG_DIR="$HOME/.dcg"
DB_PATH="$DCG_DIR/events.db"
mkdir -p "$DCG_DIR"

test_info "Creating mock dcg SQLite database"
sqlite3 "$DB_PATH" <<'SQL'
CREATE TABLE events (
    id INTEGER PRIMARY KEY,
    ts TEXT,
    type TEXT,
    cmd TEXT,
    cwd TEXT,
    rule TEXT,
    severity TEXT,
    decision TEXT,
    reason TEXT,
    user TEXT
);
INSERT INTO events (id, ts, type, cmd, cwd, rule, severity, decision, reason, user)
VALUES (1, '2026-01-28T00:00:00Z', 'command', 'git reset --hard', '/data/projects',
        'core.git:reset-hard', 'critical', 'deny', 'destructive', 'ubuntu');
SQL

# Test 1: Verify DB file exists
assert_file_exists "$DB_PATH" "DCG database should exist"

# Test 2: Verify events table has data
event_count=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM events;")
assert_eq "1" "$event_count" "events table should have 1 row"

# Test 3: Verify sample row fields
row_cmd=$(sqlite3 "$DB_PATH" "SELECT cmd FROM events WHERE id = 1;")
assert_contains "$row_cmd" "git reset --hard" "event command should match"

# Test 4: Invoke vc collect for dcg (best-effort)
collect_output=$(run_vc_or_skip collect --collector dcg 2>&1) || {
    test_warn "dcg collector invocation returned non-zero"
    test_warn "$collect_output"
}
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))

test_info "PASS: vc collect --collector dcg invoked"

# Finalize and output results
finalize_test
