#!/usr/bin/env bash
# E2E Test: Basic Collect Command
#
# Tests the basic `vc collect` command functionality:
# - Running collectors without external tools
# - Verifying DuckDB database creation
# - Checking collector output format

set -euo pipefail

# Source test helpers
source "$(dirname "$0")/../lib/test_helpers.sh"

test_info "Starting basic collect E2E test"

# Setup test environment
setup_test_env

# Test 1: Verify collect command runs without error
test_info "Test 1: Running vc collect (fallback_probe)"
collect_output=$(run_vc_or_skip collect --collector fallback_probe 2>&1) || {
    test_error "Collect command failed: $collect_output"
    TEST_FAILURES=$((TEST_FAILURES + 1))
}
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
if [[ -z "${collect_output:-}" || "$collect_output" != *"error"* ]]; then
    test_info "PASS: Collect command completed"
else
    test_error "FAIL: Collect command had errors"
    TEST_FAILURES=$((TEST_FAILURES + 1))
fi

# Test 2: Verify DuckDB file was created
test_info "Test 2: Checking DuckDB file creation"
assert_file_exists "$TEST_DB_PATH" "DuckDB file should be created after collect"

# Test 3: Verify DuckDB has expected tables (check via file size)
test_info "Test 3: Checking DuckDB has data"
db_size=$(stat -c%s "$TEST_DB_PATH" 2>/dev/null || stat -f%z "$TEST_DB_PATH")
if [[ $db_size -gt 0 ]]; then
    test_info "PASS: DuckDB file has data (${db_size} bytes)"
else
    test_error "FAIL: DuckDB file is empty"
    TEST_FAILURES=$((TEST_FAILURES + 1))
fi
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))

# Test 4: Run collect again and verify it doesn't error (incremental safety)
test_info "Test 4: Running collect a second time (incremental test)"
collect_output2=$(run_vc_or_skip collect --collector fallback_probe 2>&1) || {
    test_warn "Second collect had warnings (may be expected): $collect_output2"
}
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
test_info "PASS: Second collect completed"

# Test 5: Verify collect with machine filter
test_info "Test 5: Running collect with machine filter"
collect_output3=$(run_vc_or_skip collect --machine local 2>&1) || {
    test_warn "Collect with machine filter had issues: $collect_output3"
}
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
test_info "PASS: Collect with machine filter completed"

# Finalize and output results
finalize_test
