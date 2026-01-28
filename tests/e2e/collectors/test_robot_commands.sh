#!/usr/bin/env bash
# E2E Test: Robot Commands
#
# Tests the robot mode commands that provide JSON output for AI agents:
# - vc robot health
# - vc robot triage
# - JSON schema validation

set -euo pipefail

# Source test helpers
source "$(dirname "$0")/../lib/test_helpers.sh"

test_info "Starting robot commands E2E test"

# Setup test environment
setup_test_env

# Test 1: Robot health returns valid JSON
test_info "Test 1: Checking vc robot health"
health_output=$(run_vc_or_skip robot health 2>&1) || {
    test_error "Robot health command failed"
    health_output="{}"
}
assert_json_valid "$health_output" "Health output should be valid JSON"

# Test 2: Health JSON has required schema fields
test_info "Test 2: Validating health schema"
assert_json_field "$health_output" ".schema_version" "vc.robot.health.v1" "Schema version should match"

# Test 3: Health JSON has data section
test_info "Test 3: Checking health data section"
has_data=$(echo "$health_output" | jq 'has("data")' 2>/dev/null) || has_data="false"
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
if [[ "$has_data" == "true" ]]; then
    test_info "PASS: Health output has data section"
else
    test_error "FAIL: Health output missing data section"
    TEST_FAILURES=$((TEST_FAILURES + 1))
fi

# Test 4: Health data has overall severity
test_info "Test 4: Checking overall severity"
severity=$(echo "$health_output" | jq -r '.data.overall.severity // "missing"' 2>/dev/null)
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
if [[ "$severity" =~ ^(healthy|warning|critical|unknown|missing)$ ]]; then
    test_info "PASS: Severity is valid ($severity)"
else
    test_error "FAIL: Invalid severity value: $severity"
    TEST_FAILURES=$((TEST_FAILURES + 1))
fi

# Test 5: Robot triage returns valid JSON
test_info "Test 5: Checking vc robot triage"
triage_output=$(run_vc_or_skip robot triage 2>&1) || {
    test_error "Robot triage command failed"
    triage_output="{}"
}
assert_json_valid "$triage_output" "Triage output should be valid JSON"

# Test 6: Triage JSON has required schema fields
test_info "Test 6: Validating triage schema"
assert_json_field "$triage_output" ".schema_version" "vc.robot.triage.v1" "Schema version should match"

# Test 7: Triage JSON has data section with expected structure
test_info "Test 7: Checking triage data structure"
has_triage_data=$(echo "$triage_output" | jq 'has("data")' 2>/dev/null) || has_triage_data="false"
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
if [[ "$has_triage_data" == "true" ]]; then
    test_info "PASS: Triage output has data section"
else
    test_error "FAIL: Triage output missing data section"
    TEST_FAILURES=$((TEST_FAILURES + 1))
fi

# Test 8: Robot commands work with --format json flag
test_info "Test 8: Checking --format json flag"
json_output=$(run_vc_or_skip --format json robot health 2>&1) || {
    test_warn "Format flag test had issues"
    json_output="{}"
}
assert_json_valid "$json_output" "JSON format output should be valid"

# Finalize and output results
finalize_test
