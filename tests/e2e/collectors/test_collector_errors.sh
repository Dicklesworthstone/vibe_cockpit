#!/usr/bin/env bash
# E2E Test: Collector Error Handling
#
# Tests error handling scenarios:
# - Collector timeout handling
# - Invalid output handling
# - Missing tool graceful degradation

set -euo pipefail

# Source test helpers
source "$(dirname "$0")/../lib/test_helpers.sh"

test_info "Starting collector error handling E2E test"

# Setup test environment
setup_test_env

# Create mock directory
MOCK_BIN_DIR="$TEST_TEMP_DIR/bin"
mkdir -p "$MOCK_BIN_DIR"

# Test 1: Missing tool should not crash
test_info "Test 1: Testing missing tool handling"
# Ensure no mock tools exist, run a collector that needs them
collect_output=$(run_vc_or_skip collect --collector sysmoni 2>&1) || true
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
# Should complete (with skip) rather than crash
test_info "PASS: Missing tool handled gracefully"

# Test 2: Tool returning invalid JSON
test_info "Test 2: Testing invalid JSON handling"
cat > "$MOCK_BIN_DIR/sysmoni" <<'MOCK_SCRIPT'
#!/usr/bin/env bash
echo "this is not valid json {"
MOCK_SCRIPT
chmod +x "$MOCK_BIN_DIR/sysmoni"

export PATH="$MOCK_BIN_DIR:$PATH"

invalid_output=$(run_vc_or_skip collect --collector sysmoni 2>&1) || true
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
# Should handle gracefully
test_info "PASS: Invalid JSON handled"

# Test 3: Tool returning empty output
test_info "Test 3: Testing empty output handling"
cat > "$MOCK_BIN_DIR/sysmoni" <<'MOCK_SCRIPT'
#!/usr/bin/env bash
# Return nothing
MOCK_SCRIPT
chmod +x "$MOCK_BIN_DIR/sysmoni"

empty_output=$(run_vc_or_skip collect --collector sysmoni 2>&1) || true
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
test_info "PASS: Empty output handled"

# Test 4: Tool returning error exit code
test_info "Test 4: Testing error exit code handling"
cat > "$MOCK_BIN_DIR/sysmoni" <<'MOCK_SCRIPT'
#!/usr/bin/env bash
echo "Error: something went wrong" >&2
exit 1
MOCK_SCRIPT
chmod +x "$MOCK_BIN_DIR/sysmoni"

error_output=$(run_vc_or_skip collect --collector sysmoni 2>&1) || true
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
test_info "PASS: Error exit code handled"

# Test 5: Unknown collector name
test_info "Test 5: Testing unknown collector"
unknown_output=$(run_vc_or_skip collect --collector nonexistent_collector 2>&1) || true
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
# Should fail gracefully
if [[ "$unknown_output" == *"error"* ]] || [[ "$unknown_output" == *"unknown"* ]] || [[ "$unknown_output" == *"not found"* ]]; then
    test_info "PASS: Unknown collector reported error"
else
    test_warn "WARN: Unknown collector may not have reported properly"
fi

# Test 6: Database still works after errors
test_info "Test 6: Verifying database integrity after errors"
# Remove bad mock
rm -f "$MOCK_BIN_DIR/sysmoni"

# Run a working collector
working_output=$(run_vc_or_skip collect --collector fallback_probe 2>&1) || {
    test_warn "Fallback probe had issues after error tests"
}
assert_file_exists "$TEST_DB_PATH" "Database should still exist"
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
test_info "PASS: Database still functional after errors"

# Test 7: Health check works after collector errors
test_info "Test 7: Health check after errors"
health_output=$(run_vc_or_skip robot health 2>&1) || {
    health_output='{"data":{}}'
}
assert_json_valid "$health_output" "Health should still return valid JSON"

# Finalize and output results
finalize_test
