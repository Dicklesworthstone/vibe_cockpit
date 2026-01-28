#!/usr/bin/env bash
# E2E Test: Fallback Probe Collector
#
# This test verifies that the fallback system probe collector works
# correctly when no external tools (sysmoni, etc.) are available.

set -euo pipefail

# Source test helpers
source "$(dirname "$0")/../lib/test_helpers.sh"

test_info "Starting fallback probe E2E test"

# Setup test environment
setup_test_env

# Test 1: Verify vc command is available
test_info "Test 1: Checking vc command availability"
# Skip file exists check if using cargo run
if [[ "$VC_BIN" != "cargo run"* ]]; then
    assert_file_exists "$VC_BIN" "vc binary should exist"
else
    test_info "Using cargo run for vc"
fi

# Test 2: Verify vc --version works
test_info "Test 2: Checking vc --version"
version_output=$(run_vc_or_skip --version) || true
assert_contains "$version_output" "vc" "Version should contain 'vc'"
assert_contains "$version_output" "0.1.0" "Version should contain version number"

# Test 3: Verify vc robot health returns valid JSON
test_info "Test 3: Checking vc robot health"
health_output=$(run_vc_or_skip robot health | sed -n '/^{/,$p') || true
assert_json_valid "$health_output" "Health output should be valid JSON"
assert_json_field "$health_output" ".schema_version" "vc.robot.health.v1" "Schema version should match"
assert_json_field "$health_output" ".data.overall.severity" "healthy" "Severity should be healthy"

# Test 4: Verify vc robot triage returns valid JSON
test_info "Test 4: Checking vc robot triage"
triage_output=$(run_vc_or_skip robot triage | sed -n '/^{/,$p') || true
assert_json_valid "$triage_output" "Triage output should be valid JSON"
assert_json_field "$triage_output" ".schema_version" "vc.robot.triage.v1" "Schema version should match"

# Test 5: Verify config parsing with test config
test_info "Test 5: Checking config parsing"
assert_file_exists "$TEST_CONFIG_PATH" "Test config should exist"
# Config parsing is implicitly tested by the above commands succeeding

# Finalize and output results
finalize_test
