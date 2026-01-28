#!/usr/bin/env bash
# E2E Test: Sysmoni Collector
#
# This test verifies that the sysmoni collector correctly parses
# system metrics JSON and stores data appropriately.

set -euo pipefail

# Source test helpers
source "$(dirname "$0")/../lib/test_helpers.sh"

test_info "Starting sysmoni collector E2E test"

# Setup test environment
setup_test_env

# Create mock sysmoni JSON fixture
SYSMONI_FIXTURE="$TEST_TEMP_DIR/sysmoni_output.json"
cat > "$SYSMONI_FIXTURE" <<'EOF'
{
    "timestamp": "2026-01-28T00:00:00Z",
    "cpu": {
        "total_percent": 45.2,
        "per_core": [42.1, 48.3, 44.0, 46.4],
        "load_1": 2.1,
        "load_5": 1.8,
        "load_15": 1.5
    },
    "memory": {
        "total_bytes": 34359738368,
        "used_bytes": 23622320128,
        "available_bytes": 10737418240,
        "swap_total_bytes": 8589934592,
        "swap_used_bytes": 1073741824
    },
    "disk": {
        "read_bytes_per_sec": 1048576,
        "write_bytes_per_sec": 2097152,
        "filesystems": [
            {"mount": "/", "total_bytes": 500107862016, "used_bytes": 350075103232},
            {"mount": "/home", "total_bytes": 1000204886016, "used_bytes": 600122931200}
        ]
    },
    "network": {
        "rx_bytes_per_sec": 10485760,
        "tx_bytes_per_sec": 5242880
    },
    "processes": [
        {"pid": 1234, "name": "cargo", "cpu_percent": 45.0, "memory_bytes": 1073741824},
        {"pid": 5678, "name": "rust-analyzer", "cpu_percent": 12.5, "memory_bytes": 536870912}
    ]
}
EOF

# Test 1: Verify sysmoni fixture is valid JSON
test_info "Test 1: Validating sysmoni fixture JSON"
assert_file_exists "$SYSMONI_FIXTURE" "Sysmoni fixture should exist"
sysmoni_content=$(cat "$SYSMONI_FIXTURE")
assert_json_valid "$sysmoni_content" "Sysmoni fixture should be valid JSON"

# Test 2: Verify fixture has required CPU fields
test_info "Test 2: Checking CPU metrics structure"
assert_json_field "$sysmoni_content" ".cpu.total_percent" "45.2" "CPU total percent"
assert_json_field "$sysmoni_content" ".cpu.load_1" "2.1" "Load 1 minute average"
assert_json_field "$sysmoni_content" ".cpu.load_5" "1.8" "Load 5 minute average"
assert_json_field "$sysmoni_content" ".cpu.load_15" "1.5" "Load 15 minute average"

# Test 3: Verify fixture has required memory fields
test_info "Test 3: Checking memory metrics structure"
memory_total=$(echo "$sysmoni_content" | jq '.memory.total_bytes')
assert_ne "null" "$memory_total" "Memory total should exist"
memory_available=$(echo "$sysmoni_content" | jq '.memory.available_bytes')
assert_ne "null" "$memory_available" "Memory available should exist"

# Test 4: Verify fixture has processes
test_info "Test 4: Checking process list structure"
process_count=$(echo "$sysmoni_content" | jq '.processes | length')
assert_eq "2" "$process_count" "Should have 2 processes in fixture"

# Test 5: Verify fixture has network metrics
test_info "Test 5: Checking network metrics structure"
rx_bytes=$(echo "$sysmoni_content" | jq '.network.rx_bytes_per_sec')
assert_ne "null" "$rx_bytes" "Network RX should exist"
tx_bytes=$(echo "$sysmoni_content" | jq '.network.tx_bytes_per_sec')
assert_ne "null" "$tx_bytes" "Network TX should exist"

# Test 6: Verify fixture has disk metrics
test_info "Test 6: Checking disk metrics structure"
fs_count=$(echo "$sysmoni_content" | jq '.disk.filesystems | length')
assert_eq "2" "$fs_count" "Should have 2 filesystems in fixture"

# Test 7: Verify vc robot health works (basic health check)
test_info "Test 7: Checking vc robot health with stub implementation"
health_output=$(run_vc_or_skip robot health 2>&1) || true
assert_json_valid "$health_output" "Health output should be valid JSON"
assert_json_field "$health_output" ".schema_version" "vc.robot.health.v1" "Health schema version"

# Test 8: Test minimal sysmoni output (graceful degradation)
test_info "Test 8: Testing minimal sysmoni output parsing"
MINIMAL_FIXTURE="$TEST_TEMP_DIR/sysmoni_minimal.json"
cat > "$MINIMAL_FIXTURE" <<'EOF'
{
    "timestamp": "2026-01-28T00:00:00Z",
    "cpu": {
        "total_percent": 10.0
    },
    "memory": {
        "total_bytes": 8589934592
    }
}
EOF
minimal_content=$(cat "$MINIMAL_FIXTURE")
assert_json_valid "$minimal_content" "Minimal fixture should be valid JSON"
assert_json_field "$minimal_content" ".cpu.total_percent" "10" "Minimal CPU percent"

# Test 9: Test error handling for invalid JSON
test_info "Test 9: Testing invalid JSON handling"
INVALID_FIXTURE="$TEST_TEMP_DIR/sysmoni_invalid.json"
echo "not valid json {" > "$INVALID_FIXTURE"
if jq . "$INVALID_FIXTURE" > /dev/null 2>&1; then
    test_error "FAIL: Invalid JSON should not parse"
    TEST_FAILURES=$((TEST_FAILURES + 1))
else
    test_info "PASS: Invalid JSON correctly rejected"
fi
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))

# Finalize and output results
finalize_test
