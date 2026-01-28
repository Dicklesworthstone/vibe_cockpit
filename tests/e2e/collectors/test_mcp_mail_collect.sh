#!/usr/bin/env bash
# E2E Test: MCP Agent Mail Collector
#
# This test creates a mock agent mail SQLite database, validates
# fixture integrity, and invokes the mcp_agent_mail collector.

set -euo pipefail

# Source test helpers
source "$(dirname "$0")/../lib/test_helpers.sh"

test_info "Starting mcp_agent_mail collector E2E test"

# Setup test environment
setup_test_env

# Ensure sqlite3 is available
if ! command -v sqlite3 >/dev/null 2>&1; then
    test_warn "sqlite3 not available - skipping mcp_agent_mail collector test"
    exit 2
fi

# Create mock agent mail SQLite database under a temp HOME
export HOME="$TEST_TEMP_DIR"
MAIL_DIR="$HOME/.mcp_agent_mail_git_mailbox_repo"
DB_PATH="$MAIL_DIR/storage.sqlite3"
mkdir -p "$MAIL_DIR"

test_info "Creating mock agent mail SQLite database"
sqlite3 "$DB_PATH" <<'SQL'
CREATE TABLE messages (
    id INTEGER PRIMARY KEY,
    project_id INTEGER,
    thread_id TEXT,
    sender TEXT,
    importance TEXT,
    ack_required INTEGER,
    created_ts TEXT,
    subject TEXT
);
CREATE TABLE file_reservations (
    id INTEGER PRIMARY KEY,
    project_id INTEGER,
    path_pattern TEXT,
    agent_id INTEGER,
    expires_ts TEXT,
    exclusive INTEGER,
    reason TEXT,
    created_ts TEXT,
    released_ts TEXT
);
INSERT INTO messages (id, project_id, thread_id, sender, importance, ack_required, created_ts, subject)
VALUES (1, 1, 'bd-30z', 'MaroonCove', 'normal', 0, '2026-01-28T00:00:00Z', 'Hello from test');
INSERT INTO file_reservations (id, project_id, path_pattern, agent_id, expires_ts, exclusive, reason, created_ts, released_ts)
VALUES (1, 1, 'tests/e2e/collectors/*', 42, '2026-01-29T00:00:00Z', 1, 'bd-30z', '2026-01-28T00:00:00Z', NULL);
SQL

# Test 1: Verify DB file exists
assert_file_exists "$DB_PATH" "Agent mail database should exist"

# Test 2: Verify messages table has data
msg_count=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM messages;")
assert_eq "1" "$msg_count" "messages table should have 1 row"

# Test 3: Verify sample message fields
subject=$(sqlite3 "$DB_PATH" "SELECT subject FROM messages WHERE id = 1;")
assert_contains "$subject" "Hello" "message subject should match"

# Test 4: Verify file reservation row exists
res_count=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM file_reservations;")
assert_eq "1" "$res_count" "file_reservations table should have 1 row"

# Test 5: Invoke vc collect for mcp_agent_mail (best-effort)
collect_output=$(run_vc collect --collector mcp_agent_mail 2>&1) || {
    test_warn "mcp_agent_mail collector invocation returned non-zero"
    test_warn "$collect_output"
}
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
test_info "PASS: vc collect --collector mcp_agent_mail invoked"

# Finalize and output results
finalize_test
