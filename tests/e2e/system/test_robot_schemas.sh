#!/usr/bin/env bash
# E2E test for robot output schema validation
# Verifies that robot outputs conform to JSON schemas
#
# Usage: ./test_robot_schemas.sh [--verbose]
#
# Exit codes:
#   0 - All tests passed
#   1 - One or more tests failed

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
LOGS_DIR="$PROJECT_ROOT/tests/logs"
SCHEMAS_DIR="$PROJECT_ROOT/docs/schemas"

# Ensure logs directory exists
mkdir -p "$LOGS_DIR"

# Test counters
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0
VERBOSE="${1:-}"

log() {
    if [[ "$VERBOSE" == "--verbose" ]]; then
        echo "$@"
    fi
}

fail() {
    echo "FAIL: $1"
    TESTS_FAILED=$((TESTS_FAILED + 1))
}

pass() {
    log "PASS: $1"
    TESTS_PASSED=$((TESTS_PASSED + 1))
}

run_test() {
    TESTS_RUN=$((TESTS_RUN + 1))
}

# Test: Schema files exist
test_schema_files_exist() {
    run_test
    log "Testing schema files exist..."

    local schemas=(
        "robot-envelope.json"
        "robot-health.json"
        "robot-status.json"
        "robot-triage.json"
        "index.json"
    )

    for schema in "${schemas[@]}"; do
        if [[ ! -f "$SCHEMAS_DIR/$schema" ]]; then
            fail "Schema file missing: $schema"
            return
        fi
    done

    pass "All schema files exist"
}

# Test: Schema files are valid JSON
test_schema_files_valid_json() {
    run_test
    log "Testing schema files are valid JSON..."

    for schema_file in "$SCHEMAS_DIR"/*.json; do
        if ! jq empty "$schema_file" 2>/dev/null; then
            fail "Invalid JSON in: $(basename "$schema_file")"
            return
        fi
    done

    pass "All schema files are valid JSON"
}

# Test: Schemas have required $schema field
test_schemas_have_schema_field() {
    run_test
    log "Testing schemas have \$schema field..."

    for schema_file in "$SCHEMAS_DIR"/robot-*.json; do
        local has_schema
        has_schema=$(jq 'has("$schema")' "$schema_file")
        if [[ "$has_schema" != "true" ]]; then
            fail "Missing \$schema field in: $(basename "$schema_file")"
            return
        fi
    done

    pass "All schemas have \$schema field"
}

# Test: Robot health output has required fields
test_robot_health_output() {
    run_test
    log "Testing robot health output structure..."

    # Build the CLI if needed
    if ! cargo build --bin vc --quiet 2>/dev/null; then
        fail "Failed to build vc"
        return
    fi

    # Run robot health command
    local output
    if output=$(cargo run --bin vc --quiet -- robot health 2>/dev/null); then
        # Check for required envelope fields
        local has_schema_version has_generated_at has_data
        has_schema_version=$(echo "$output" | jq 'has("schema_version")')
        has_generated_at=$(echo "$output" | jq 'has("generated_at")')
        has_data=$(echo "$output" | jq 'has("data")')

        if [[ "$has_schema_version" != "true" ]]; then
            fail "Robot health missing schema_version"
            return
        fi
        if [[ "$has_generated_at" != "true" ]]; then
            fail "Robot health missing generated_at"
            return
        fi
        if [[ "$has_data" != "true" ]]; then
            fail "Robot health missing data"
            return
        fi

        # Check schema_version format
        local schema_version
        schema_version=$(echo "$output" | jq -r '.schema_version')
        if [[ ! "$schema_version" =~ ^vc\.robot\.[a-z]+\.v[0-9]+$ ]]; then
            fail "Invalid schema_version format: $schema_version"
            return
        fi

        pass "Robot health output structure is valid"
    else
        fail "Failed to run robot health command"
    fi
}

# Test: Robot status output has required fields
test_robot_status_output() {
    run_test
    log "Testing robot status output structure..."

    local output
    if output=$(cargo run --bin vc --quiet -- robot status 2>/dev/null); then
        local schema_version
        schema_version=$(echo "$output" | jq -r '.schema_version')

        if [[ "$schema_version" != "vc.robot.status.v1" ]]; then
            fail "Robot status has wrong schema_version: $schema_version"
            return
        fi

        # Check data structure
        local has_fleet has_machines has_repos has_alerts
        has_fleet=$(echo "$output" | jq '.data | has("fleet")')
        has_machines=$(echo "$output" | jq '.data | has("machines")')
        has_repos=$(echo "$output" | jq '.data | has("repos")')
        has_alerts=$(echo "$output" | jq '.data | has("alerts")')

        if [[ "$has_fleet" != "true" ]] || [[ "$has_machines" != "true" ]] || \
           [[ "$has_repos" != "true" ]] || [[ "$has_alerts" != "true" ]]; then
            fail "Robot status data missing required fields"
            return
        fi

        pass "Robot status output structure is valid"
    else
        fail "Failed to run robot status command"
    fi
}

# Test: Robot triage output has required fields
test_robot_triage_output() {
    run_test
    log "Testing robot triage output structure..."

    local output
    if output=$(cargo run --bin vc --quiet -- robot triage 2>/dev/null); then
        local schema_version
        schema_version=$(echo "$output" | jq -r '.schema_version')

        if [[ "$schema_version" != "vc.robot.triage.v1" ]]; then
            fail "Robot triage has wrong schema_version: $schema_version"
            return
        fi

        # Check data structure
        local has_recommendations has_suggested_commands
        has_recommendations=$(echo "$output" | jq '.data | has("recommendations")')
        has_suggested_commands=$(echo "$output" | jq '.data | has("suggested_commands")')

        if [[ "$has_recommendations" != "true" ]] || [[ "$has_suggested_commands" != "true" ]]; then
            fail "Robot triage data missing required fields"
            return
        fi

        pass "Robot triage output structure is valid"
    else
        fail "Failed to run robot triage command"
    fi
}

# Generate JSON summary
generate_summary() {
    local timestamp
    timestamp=$(date -Iseconds)

    cat > "$LOGS_DIR/test_robot_schemas.json" << EOF
{
  "test_name": "test_robot_schemas",
  "timestamp": "$timestamp",
  "tests_run": $TESTS_RUN,
  "tests_passed": $TESTS_PASSED,
  "tests_failed": $TESTS_FAILED,
  "status": "$([ $TESTS_FAILED -eq 0 ] && echo "passed" || echo "failed")"
}
EOF
}

# Main execution
main() {
    echo "Running robot schema tests..."
    echo

    # Run all tests
    test_schema_files_exist
    test_schema_files_valid_json
    test_schemas_have_schema_field
    test_robot_health_output
    test_robot_status_output
    test_robot_triage_output

    # Generate summary
    generate_summary

    # Report results
    echo
    echo "================================"
    echo "Tests run:    $TESTS_RUN"
    echo "Tests passed: $TESTS_PASSED"
    echo "Tests failed: $TESTS_FAILED"
    echo "================================"
    echo
    echo "JSON summary: $LOGS_DIR/test_robot_schemas.json"

    # Exit with appropriate code
    if [[ $TESTS_FAILED -gt 0 ]]; then
        exit 1
    fi
    exit 0
}

main "$@"
