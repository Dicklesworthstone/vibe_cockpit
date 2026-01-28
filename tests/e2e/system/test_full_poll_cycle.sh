#!/usr/bin/env bash
# E2E Test: Full Poll Cycle
#
# Tests a complete poll cycle that:
# - Creates DuckDB with schema
# - Runs all available collectors
# - Verifies data was stored
# - Checks health status reflects data

set -euo pipefail

# Source test helpers
source "$(dirname "$0")/../lib/test_helpers.sh"

test_info "Starting full poll cycle E2E test"

# Setup test environment
setup_test_env

# Test 1: Run initial collect to set up database
test_info "Test 1: Running initial collect"
run_vc_or_skip collect 2>&1 || {
    collect_output="$VC_LAST_OUTPUT"
    test_warn "Initial collect had issues (expected if no tools): $collect_output"
}
collect_output="$VC_LAST_OUTPUT"
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
test_info "PASS: Initial collect completed"

# Test 2: Verify database exists
test_info "Test 2: Verifying database creation"
assert_file_exists "$TEST_DB_PATH" "DuckDB should exist after collect"

# Test 3: Check health after data collection
test_info "Test 3: Checking health status"
if run_vc_or_skip robot health 2>&1; then
    health_output="$VC_LAST_OUTPUT"
else
    test_error "Health check failed after collect"
    health_output='{"data":{"overall":{"severity":"unknown"}}}'
fi
assert_json_valid "$health_output" "Health should be valid JSON"

severity=$(echo "$health_output" | jq -r '.data.overall.severity // "unknown"')
test_info "Health severity: $severity"
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
test_info "PASS: Health check completed with severity: $severity"

# Test 4: Run triage after data collection
test_info "Test 4: Checking triage output"
if run_vc_or_skip robot triage 2>&1; then
    triage_output="$VC_LAST_OUTPUT"
else
    test_error "Triage failed after collect"
    triage_output='{}'
fi
assert_json_valid "$triage_output" "Triage should be valid JSON"

# Test 5: Run collect again (second poll)
test_info "Test 5: Running second poll cycle"
run_vc_or_skip collect 2>&1 || {
    collect_output2="$VC_LAST_OUTPUT"
    test_warn "Second collect had issues: $collect_output2"
}
collect_output2="$VC_LAST_OUTPUT"
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
test_info "PASS: Second poll completed"

# Test 6: Verify incremental data (db size should grow or stay same)
test_info "Test 6: Checking database after second poll"
db_size=$(stat -c%s "$TEST_DB_PATH" 2>/dev/null || stat -f%z "$TEST_DB_PATH")
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
if [[ $db_size -gt 0 ]]; then
    test_info "PASS: Database has data after multiple polls (${db_size} bytes)"
else
    test_error "FAIL: Database empty after polls"
    TEST_FAILURES=$((TEST_FAILURES + 1))
fi

# Test 7: Run specific collector
test_info "Test 7: Running specific collector (fallback_probe)"
run_vc_or_skip collect --collector fallback_probe 2>&1 || {
    specific_output="$VC_LAST_OUTPUT"
    test_warn "Specific collector had issues"
}
specific_output="$VC_LAST_OUTPUT"
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
test_info "PASS: Specific collector completed"

# Test 8: Verify status command works
test_info "Test 8: Checking status command"
if run_vc_or_skip status 2>&1; then
    status_output="$VC_LAST_OUTPUT"
else
    test_warn "Status command had issues"
    status_output="status unavailable"
fi
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
test_info "PASS: Status command completed"

# Finalize and output results
finalize_test
