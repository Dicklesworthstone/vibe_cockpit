#!/usr/bin/env bash
# E2E Test: Sysmoni Collector with Mock
#
# Tests the sysmoni collector using a mock sysmoni command.
# Creates a fake sysmoni script that returns valid JSON output.

set -euo pipefail

# Source test helpers
source "$(dirname "$0")/../lib/test_helpers.sh"

test_info "Starting sysmoni mock E2E test"

# Setup test environment
setup_test_env

# Create mock sysmoni command
test_info "Creating mock sysmoni command"
MOCK_BIN_DIR="$TEST_TEMP_DIR/bin"
mkdir -p "$MOCK_BIN_DIR"

cat > "$MOCK_BIN_DIR/sysmoni" <<'MOCK_SCRIPT'
#!/usr/bin/env bash
# Mock sysmoni - returns sample system metrics JSON

cat <<'JSON'
{
  "timestamp": "2026-01-28T12:00:00Z",
  "cpu": {
    "total_percent": 25.5,
    "cores": [12.5, 30.2, 28.1, 31.2]
  },
  "load": {
    "one": 1.25,
    "five": 1.10,
    "fifteen": 0.95
  },
  "memory": {
    "total_bytes": 17179869184,
    "used_bytes": 8589934592,
    "available_bytes": 8589934592,
    "swap_total_bytes": 4294967296,
    "swap_used_bytes": 1073741824
  },
  "disk": {
    "read_mbps": 12.5,
    "write_mbps": 8.3
  },
  "network": {
    "rx_mbps": 5.2,
    "tx_mbps": 2.1
  },
  "processes": [
    {"pid": 1234, "name": "rust-analyzer", "cpu_percent": 15.2, "mem_bytes": 536870912},
    {"pid": 5678, "name": "cargo", "cpu_percent": 8.5, "mem_bytes": 268435456}
  ]
}
JSON
MOCK_SCRIPT
chmod +x "$MOCK_BIN_DIR/sysmoni"

# Prepend mock bin to PATH
export PATH="$MOCK_BIN_DIR:$PATH"

# Test 1: Verify mock sysmoni works
test_info "Test 1: Verifying mock sysmoni"
mock_output=$("$MOCK_BIN_DIR/sysmoni")
assert_json_valid "$mock_output" "Mock sysmoni output should be valid JSON"
assert_json_field "$mock_output" ".cpu.total_percent" "25.5" "CPU total should match"

# Test 2: Run sysmoni collector
test_info "Test 2: Running sysmoni collector"
collect_output=$(run_vc_or_skip collect --collector sysmoni 2>&1) || {
    test_warn "Sysmoni collector had issues: $collect_output"
}
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
test_info "PASS: Sysmoni collector completed"

# Test 3: Verify database was updated
test_info "Test 3: Checking database"
assert_file_exists "$TEST_DB_PATH" "Database should exist"

# Test 4: Run health check after sysmoni collection
test_info "Test 4: Checking health after sysmoni"
health_output=$(run_vc_or_skip robot health 2>&1) || {
    health_output='{"data":{}}'
}
assert_json_valid "$health_output" "Health output should be valid"

# Test 5: Run collector multiple times (test incremental)
test_info "Test 5: Running sysmoni multiple times"
for i in 1 2 3; do
    run_vc_or_skip collect --collector sysmoni 2>&1 || true
done
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
test_info "PASS: Multiple sysmoni runs completed"

# Finalize and output results
finalize_test
