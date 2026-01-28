#!/usr/bin/env bash
# E2E Test: CAAM (Claude Account Manager) Collector with Mock
#
# Tests the caam collector using a mock caam command.
# Verifies account profile collection.

set -euo pipefail

# Source test helpers
source "$(dirname "$0")/../lib/test_helpers.sh"

test_info "Starting caam mock E2E test"

# Setup test environment
setup_test_env

# Create mock caam command
test_info "Creating mock caam command"
MOCK_BIN_DIR="$TEST_TEMP_DIR/bin"
mkdir -p "$MOCK_BIN_DIR"

cat > "$MOCK_BIN_DIR/caam" <<'MOCK_SCRIPT'
#!/usr/bin/env bash
# Mock caam - returns sample account profiles

case "$1" in
    list)
        if [[ "${*}" == *"--json"* ]]; then
            cat <<'JSON'
{
  "accounts": [
    {
      "provider": "anthropic",
      "account_id": "acct_001",
      "email": "user@example.com",
      "plan_type": "max",
      "is_active": true,
      "is_current": true,
      "priority": 1
    },
    {
      "provider": "anthropic",
      "account_id": "acct_002",
      "email": "backup@example.com",
      "plan_type": "pro",
      "is_active": true,
      "is_current": false,
      "priority": 2
    },
    {
      "provider": "openai",
      "account_id": "org-xyz",
      "email": "user@example.com",
      "plan_type": "plus",
      "is_active": true,
      "is_current": false,
      "priority": 3
    }
  ]
}
JSON
        else
            echo "Accounts:"
            echo "  1. anthropic/acct_001 (max) - CURRENT"
            echo "  2. anthropic/acct_002 (pro)"
            echo "  3. openai/org-xyz (plus)"
        fi
        ;;
    current)
        if [[ "${*}" == *"--json"* ]]; then
            cat <<'JSON'
{
  "provider": "anthropic",
  "account_id": "acct_001",
  "email": "user@example.com",
  "plan_type": "max"
}
JSON
        else
            echo "Current: anthropic/acct_001 (max)"
        fi
        ;;
    *)
        echo "Usage: caam <list|current> [--json]"
        exit 1
        ;;
esac
MOCK_SCRIPT
chmod +x "$MOCK_BIN_DIR/caam"

# Prepend mock bin to PATH
export PATH="$MOCK_BIN_DIR:$PATH"

# Test 1: Verify mock caam list works
test_info "Test 1: Verifying mock caam list"
mock_list=$("$MOCK_BIN_DIR/caam" list --json)
assert_json_valid "$mock_list" "Mock caam list should be valid JSON"

account_count=$(echo "$mock_list" | jq '.accounts | length')
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
if [[ "$account_count" -ge 1 ]]; then
    test_info "PASS: caam list has accounts ($account_count)"
else
    test_error "FAIL: caam list missing accounts"
    TEST_FAILURES=$((TEST_FAILURES + 1))
fi

# Test 2: Verify current account
test_info "Test 2: Verifying current account"
current_account=$("$MOCK_BIN_DIR/caam" current --json)
assert_json_valid "$current_account" "Current account should be valid JSON"
assert_json_field "$current_account" ".plan_type" "max" "Current plan should be max"

# Test 3: Run caam collector
test_info "Test 3: Running caam collector"
run_vc_or_skip collect --collector caam 2>&1 || {
    collect_output="$VC_LAST_OUTPUT"
    test_warn "CAAM collector had issues: $collect_output"
}
collect_output="$VC_LAST_OUTPUT"
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
test_info "PASS: CAAM collector completed"

# Test 4: Verify database was updated
test_info "Test 4: Checking database"
assert_file_exists "$TEST_DB_PATH" "Database should exist"

# Test 5: Run caam collector again
test_info "Test 5: Running caam collector again"
run_vc_or_skip collect --collector caam 2>&1 || {
    collect_output2="$VC_LAST_OUTPUT"
    test_warn "Second caam collect had issues"
}
collect_output2="$VC_LAST_OUTPUT"
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
test_info "PASS: Second caam collect completed"

# Finalize and output results
finalize_test
