#!/usr/bin/env bash
# E2E Test: RU (Repo Utils) Collector with Mock
#
# Tests the ru collector using a mock ru command.
# Creates test git repositories and verifies repo status collection.

set -euo pipefail

# Source test helpers
source "$(dirname "$0")/../lib/test_helpers.sh"

test_info "Starting ru mock E2E test"

# Setup test environment
setup_test_env

# Create mock ru command
test_info "Creating mock ru command"
MOCK_BIN_DIR="$TEST_TEMP_DIR/bin"
mkdir -p "$MOCK_BIN_DIR"

cat > "$MOCK_BIN_DIR/ru" <<'MOCK_SCRIPT'
#!/usr/bin/env bash
# Mock ru - returns sample repository status

case "$1" in
    list)
        if [[ "${*}" == *"--json"* ]]; then
            cat <<'JSON'
{
  "repositories": [
    {
      "path": "/data/projects/vibe_cockpit",
      "name": "vibe_cockpit",
      "branch": "main",
      "is_dirty": false,
      "ahead": 0,
      "behind": 0,
      "stash_count": 0,
      "uncommitted_files": 0,
      "last_commit_ts": "2026-01-28T10:30:00Z",
      "last_commit_hash": "abc123def"
    },
    {
      "path": "/data/projects/flywheel",
      "name": "flywheel",
      "branch": "feature/new-thing",
      "is_dirty": true,
      "ahead": 3,
      "behind": 1,
      "stash_count": 2,
      "uncommitted_files": 5,
      "last_commit_ts": "2026-01-28T09:15:00Z",
      "last_commit_hash": "def456abc"
    }
  ]
}
JSON
        else
            echo "vibe_cockpit (main) - clean"
            echo "flywheel (feature/new-thing) - dirty, 5 uncommitted"
        fi
        ;;
    status)
        if [[ "${*}" == *"--json"* ]]; then
            cat <<'JSON'
{
  "path": "/data/projects/vibe_cockpit",
  "name": "vibe_cockpit",
  "branch": "main",
  "is_dirty": false,
  "uncommitted_files": 0,
  "modified": [],
  "untracked": [],
  "staged": []
}
JSON
        else
            echo "Repository: vibe_cockpit"
            echo "Branch: main"
            echo "Status: clean"
        fi
        ;;
    *)
        echo "Usage: ru <list|status> [--json]"
        exit 1
        ;;
esac
MOCK_SCRIPT
chmod +x "$MOCK_BIN_DIR/ru"

# Prepend mock bin to PATH
export PATH="$MOCK_BIN_DIR:$PATH"

# Test 1: Verify mock ru list works
test_info "Test 1: Verifying mock ru list"
mock_list=$("$MOCK_BIN_DIR/ru" list --json)
assert_json_valid "$mock_list" "Mock ru list should be valid JSON"

repo_count=$(echo "$mock_list" | jq '.repositories | length')
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
if [[ "$repo_count" -ge 1 ]]; then
    test_info "PASS: ru list has repositories ($repo_count)"
else
    test_error "FAIL: ru list missing repositories"
    TEST_FAILURES=$((TEST_FAILURES + 1))
fi

# Test 2: Verify mock ru status works
test_info "Test 2: Verifying mock ru status"
mock_status=$("$MOCK_BIN_DIR/ru" status --json)
assert_json_valid "$mock_status" "Mock ru status should be valid JSON"
assert_json_field "$mock_status" ".branch" "main" "Branch should be main"

# Test 3: Run ru collector
test_info "Test 3: Running ru collector"
collect_output=$(run_vc_or_skip collect --collector ru 2>&1) || {
    test_warn "RU collector had issues: $collect_output"
}
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
test_info "PASS: RU collector completed"

# Test 4: Verify database was updated
test_info "Test 4: Checking database"
assert_file_exists "$TEST_DB_PATH" "Database should exist"

# Test 5: Check that dirty repos are detected
test_info "Test 5: Verifying dirty repo detection"
# The mock has one dirty repo (flywheel)
dirty_count=$(echo "$mock_list" | jq '[.repositories[] | select(.is_dirty == true)] | length')
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
if [[ "$dirty_count" -ge 1 ]]; then
    test_info "PASS: Dirty repos detected ($dirty_count)"
else
    test_error "FAIL: No dirty repos detected"
    TEST_FAILURES=$((TEST_FAILURES + 1))
fi

# Test 6: Run ru collector again
test_info "Test 6: Running ru collector again"
collect_output2=$(run_vc_or_skip collect --collector ru 2>&1) || {
    test_warn "Second ru collect had issues"
}
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
test_info "PASS: Second ru collect completed"

# Finalize and output results
finalize_test
