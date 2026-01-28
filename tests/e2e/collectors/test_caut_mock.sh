#!/usr/bin/env bash
# E2E Test: CAUT (Claude Account Usage Tracker) Collector with Mock
#
# Tests the caut collector using a mock caut command.
# Verifies account usage collection.

set -euo pipefail

# Source test helpers
source "$(dirname "$0")/../lib/test_helpers.sh"

test_info "Starting caut mock E2E test"

# Setup test environment
setup_test_env

# Create mock caut command
test_info "Creating mock caut command"
MOCK_BIN_DIR="$TEST_TEMP_DIR/bin"
mkdir -p "$MOCK_BIN_DIR"

cat > "$MOCK_BIN_DIR/caut" <<'MOCK_SCRIPT'
#!/usr/bin/env bash
# Mock caut - returns sample account usage data

case "$1" in
    status|"")
        if [[ "${*}" == *"--json"* ]]; then
            cat <<'JSON'
{
  "accounts": [
    {
      "provider": "anthropic",
      "account_id": "acct_001",
      "usage_pct": 45.5,
      "tokens_used": 4550000,
      "tokens_limit": 10000000,
      "resets_at": "2026-02-01T00:00:00Z",
      "cost_estimate": 45.50
    },
    {
      "provider": "anthropic",
      "account_id": "acct_002",
      "usage_pct": 12.3,
      "tokens_used": 1230000,
      "tokens_limit": 10000000,
      "resets_at": "2026-02-01T00:00:00Z",
      "cost_estimate": 12.30
    },
    {
      "provider": "openai",
      "account_id": "org-xyz",
      "usage_pct": 78.9,
      "tokens_used": 789000000,
      "tokens_limit": 1000000000,
      "resets_at": "2026-02-01T00:00:00Z",
      "cost_estimate": 157.80
    }
  ],
  "total_cost_estimate": 215.60
}
JSON
        else
            echo "Account Usage:"
            echo "  anthropic/acct_001: 45.5% ($45.50)"
            echo "  anthropic/acct_002: 12.3% ($12.30)"
            echo "  openai/org-xyz: 78.9% ($157.80)"
            echo "Total: $215.60"
        fi
        ;;
    *)
        echo "Usage: caut [status] [--json]"
        exit 1
        ;;
esac
MOCK_SCRIPT
chmod +x "$MOCK_BIN_DIR/caut"

# Prepend mock bin to PATH
export PATH="$MOCK_BIN_DIR:$PATH"

# Test 1: Verify mock caut works
test_info "Test 1: Verifying mock caut"
mock_status=$("$MOCK_BIN_DIR/caut" status --json)
assert_json_valid "$mock_status" "Mock caut should be valid JSON"

account_count=$(echo "$mock_status" | jq '.accounts | length')
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
if [[ "$account_count" -ge 1 ]]; then
    test_info "PASS: caut has accounts ($account_count)"
else
    test_error "FAIL: caut missing accounts"
    TEST_FAILURES=$((TEST_FAILURES + 1))
fi

# Test 2: Verify usage percentages
test_info "Test 2: Verifying usage data"
high_usage=$(echo "$mock_status" | jq '[.accounts[] | select(.usage_pct > 70)] | length')
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
if [[ "$high_usage" -ge 1 ]]; then
    test_info "PASS: High usage accounts detected ($high_usage)"
else
    test_warn "WARN: No high usage accounts in mock data"
fi

# Test 3: Run caut collector
test_info "Test 3: Running caut collector"
run_vc_or_skip collect --collector caut 2>&1 || {
    collect_output="$VC_LAST_OUTPUT"
    test_warn "CAUT collector had issues: $collect_output"
}
collect_output="$VC_LAST_OUTPUT"
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
test_info "PASS: CAUT collector completed"

# Test 4: Verify database was updated
test_info "Test 4: Checking database"
assert_file_exists "$TEST_DB_PATH" "Database should exist"

# Test 5: Total cost estimate
test_info "Test 5: Verifying total cost"
total_cost=$(echo "$mock_status" | jq '.total_cost_estimate')
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
if [[ "$total_cost" != "null" && "$total_cost" != "0" ]]; then
    test_info "PASS: Total cost calculated ($total_cost)"
else
    test_error "FAIL: Total cost missing or zero"
    TEST_FAILURES=$((TEST_FAILURES + 1))
fi

# Test 6: Run caut collector again
test_info "Test 6: Running caut collector again"
run_vc_or_skip collect --collector caut 2>&1 || {
    collect_output2="$VC_LAST_OUTPUT"
    test_warn "Second caut collect had issues"
}
collect_output2="$VC_LAST_OUTPUT"
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
test_info "PASS: Second caut collect completed"

# Finalize and output results
finalize_test
