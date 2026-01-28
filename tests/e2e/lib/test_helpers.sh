#!/usr/bin/env bash
# E2E Test Helper Library
#
# Source this file in test scripts:
#   source "$(dirname "$0")/../lib/test_helpers.sh"

set -euo pipefail

# Determine project root
TEST_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
E2E_DIR="$(dirname "$TEST_LIB_DIR")"
PROJECT_ROOT="$(cd "$E2E_DIR/../.." && pwd)"

# Find vc binary - check multiple locations
find_vc_binary() {
    # Check CARGO_TARGET_DIR first
    if [[ -n "${CARGO_TARGET_DIR:-}" && -x "$CARGO_TARGET_DIR/debug/vc" ]]; then
        echo "$CARGO_TARGET_DIR/debug/vc"
        return
    fi

    # Check common target locations
    for target_dir in \
        "$PROJECT_ROOT/target/debug/vc" \
        "/data/tmp/cargo-target/debug/vc" \
        "$HOME/.cargo/target/debug/vc"; do
        if [[ -x "$target_dir" ]]; then
            echo "$target_dir"
            return
        fi
    done

    # Fallback: use cargo run
    echo "cargo run --quiet --"
}

VC_BIN=$(find_vc_binary)

# Test state
TEST_NAME="${TEST_NAME:-$(basename "$0" .sh)}"
TEST_ASSERTIONS=0
TEST_FAILURES=0
TEST_START_TIME=$(date +%s%N)

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Logging
test_log() {
    local level="$1"
    shift
    echo "[$(date -Iseconds)] [$level] $*" >&2
}

test_info() { test_log "INFO" "$@"; }
test_warn() { test_log "WARN" "$@"; }
test_error() { test_log "ERROR" "$@"; }

# Setup and teardown
setup_test_env() {
    test_info "Setting up test environment"

    # Create temp directory for test
    TEST_TEMP_DIR=$(mktemp -d -t "vc_e2e_${TEST_NAME}_XXXXXX")
    export TEST_TEMP_DIR

    # Create test DuckDB path
    TEST_DB_PATH="$TEST_TEMP_DIR/test.duckdb"
    export TEST_DB_PATH

    # Create test config
    TEST_CONFIG_PATH="$TEST_TEMP_DIR/config.toml"
    cat > "$TEST_CONFIG_PATH" <<EOF
[global]
db_path = "$TEST_DB_PATH"
poll_interval_secs = 60

[machines.local]
name = "test-local"
ssh = ""
enabled = true
EOF
    export TEST_CONFIG_PATH

    test_info "Test temp dir: $TEST_TEMP_DIR"
}

cleanup_test_env() {
    local exit_code=$?
    test_info "Cleaning up test environment"

    if [[ -n "${TEST_TEMP_DIR:-}" && -d "$TEST_TEMP_DIR" ]]; then
        # Keep on failure for debugging
        if [[ $exit_code -ne 0 && "${KEEP_ON_FAILURE:-}" == "true" ]]; then
            test_warn "Keeping temp dir for debugging: $TEST_TEMP_DIR"
        else
            rm -rf "$TEST_TEMP_DIR"
        fi
    fi
}

# Register cleanup trap
trap cleanup_test_env EXIT

# Assertions
assert_eq() {
    local expected="$1"
    local actual="$2"
    local msg="${3:-Values should be equal}"

    TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))

    if [[ "$expected" == "$actual" ]]; then
        test_info "PASS: $msg"
    else
        test_error "FAIL: $msg"
        test_error "  Expected: $expected"
        test_error "  Actual:   $actual"
        TEST_FAILURES=$((TEST_FAILURES + 1))
    fi
}

assert_ne() {
    local unexpected="$1"
    local actual="$2"
    local msg="${3:-Values should not be equal}"

    TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))

    if [[ "$unexpected" != "$actual" ]]; then
        test_info "PASS: $msg"
    else
        test_error "FAIL: $msg"
        test_error "  Unexpected: $unexpected"
        test_error "  Actual:     $actual"
        TEST_FAILURES=$((TEST_FAILURES + 1))
    fi
}

assert_contains() {
    local haystack="$1"
    local needle="$2"
    local msg="${3:-String should contain substring}"

    TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))

    if [[ "$haystack" == *"$needle"* ]]; then
        test_info "PASS: $msg"
    else
        test_error "FAIL: $msg"
        test_error "  Looking for: $needle"
        test_error "  In: $haystack"
        TEST_FAILURES=$((TEST_FAILURES + 1))
    fi
}

assert_not_contains() {
    local haystack="$1"
    local needle="$2"
    local msg="${3:-String should not contain substring}"

    TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))

    if [[ "$haystack" != *"$needle"* ]]; then
        test_info "PASS: $msg"
    else
        test_error "FAIL: $msg"
        test_error "  Should not contain: $needle"
        test_error "  But found in: $haystack"
        TEST_FAILURES=$((TEST_FAILURES + 1))
    fi
}

assert_file_exists() {
    local path="$1"
    local msg="${2:-File should exist}"

    TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))

    if [[ -f "$path" ]]; then
        test_info "PASS: $msg ($path)"
    else
        test_error "FAIL: $msg"
        test_error "  Path: $path"
        TEST_FAILURES=$((TEST_FAILURES + 1))
    fi
}

assert_dir_exists() {
    local path="$1"
    local msg="${2:-Directory should exist}"

    TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))

    if [[ -d "$path" ]]; then
        test_info "PASS: $msg ($path)"
    else
        test_error "FAIL: $msg"
        test_error "  Path: $path"
        TEST_FAILURES=$((TEST_FAILURES + 1))
    fi
}

assert_exit_code() {
    local expected="$1"
    local actual="$2"
    local msg="${3:-Exit code should match}"

    assert_eq "$expected" "$actual" "$msg"
}

assert_json_valid() {
    local json="$1"
    local msg="${2:-JSON should be valid}"

    TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))

    if echo "$json" | jq . > /dev/null 2>&1; then
        test_info "PASS: $msg"
    else
        test_error "FAIL: $msg"
        test_error "  Invalid JSON: $json"
        TEST_FAILURES=$((TEST_FAILURES + 1))
    fi
}

assert_json_field() {
    local json="$1"
    local field="$2"
    local expected="$3"
    local msg="${4:-JSON field should match}"

    TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))

    local actual=$(echo "$json" | jq -r "$field" 2>/dev/null)
    if [[ "$actual" == "$expected" ]]; then
        test_info "PASS: $msg ($field = $expected)"
    else
        test_error "FAIL: $msg"
        test_error "  Field: $field"
        test_error "  Expected: $expected"
        test_error "  Actual: $actual"
        TEST_FAILURES=$((TEST_FAILURES + 1))
    fi
}

# Run vc command with test config
run_vc() {
    local output=""
    local status=0

    # Handle different VC_BIN forms
    if [[ "$VC_BIN" == "cargo run"* ]]; then
        # Using cargo run
        set +e
        output=$(cd "$PROJECT_ROOT" && cargo run --quiet -- --config "$TEST_CONFIG_PATH" "$@")
        status=$?
        set -e
    elif [[ -x "$VC_BIN" ]]; then
        # Using direct binary
        set +e
        output=$("$VC_BIN" --config "$TEST_CONFIG_PATH" "$@")
        status=$?
        set -e
    else
        test_error "vc binary not found at $VC_BIN"
        test_error "Run 'cargo build' first"
        return 1
    fi

    echo "$output"

    return "$status"
}

# Run vc and skip test if command is not implemented
run_vc_or_skip() {
    local output=""
    local status=0

    set +e
    output=$(run_vc "$@" 2>&1)
    status=$?
    set -e

    if [[ "$output" == *"Command not yet implemented"* ]]; then
        test_warn "Skipping: command not yet implemented (vc $*)"
        exit 2
    fi

    echo "$output"
    return "$status"
}

# Wait for condition with timeout
wait_for() {
    local condition="$1"
    local timeout="${2:-30}"
    local interval="${3:-1}"

    local elapsed=0
    while [[ $elapsed -lt $timeout ]]; do
        if eval "$condition"; then
            return 0
        fi
        sleep "$interval"
        elapsed=$((elapsed + interval))
    done

    return 1
}

# Generate test result JSON
finalize_test() {
    local end_time=$(date +%s%N)
    local duration_ms=$(( (end_time - TEST_START_TIME) / 1000000 ))

    local status="passed"
    local exit_code=0
    if [[ $TEST_FAILURES -gt 0 ]]; then
        status="failed"
        exit_code=1
    fi

    # Output JSON summary
    cat <<EOF
{
  "test_name": "$TEST_NAME",
  "status": "$status",
  "duration_ms": $duration_ms,
  "assertions": $TEST_ASSERTIONS,
  "failures": $TEST_FAILURES
}
EOF

    test_info "Test complete: $TEST_ASSERTIONS assertions, $TEST_FAILURES failures"

    exit $exit_code
}
