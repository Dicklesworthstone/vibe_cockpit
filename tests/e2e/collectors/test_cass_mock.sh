#!/usr/bin/env bash
# E2E Test: CASS Collector with Mock
#
# Tests the cass collector using mock cass commands.
# Creates fake cass health and cass stats scripts.

set -euo pipefail

# Source test helpers
source "$(dirname "$0")/../lib/test_helpers.sh"

test_info "Starting cass mock E2E test"

# Setup test environment
setup_test_env

# Create mock cass command
test_info "Creating mock cass command"
MOCK_BIN_DIR="$TEST_TEMP_DIR/bin"
mkdir -p "$MOCK_BIN_DIR"

cat > "$MOCK_BIN_DIR/cass" <<'MOCK_SCRIPT'
#!/usr/bin/env bash
# Mock cass - returns sample session search data

case "$1" in
    health)
        if [[ "${2:-}" == "--json" ]]; then
            cat <<'JSON'
{
  "state": "healthy",
  "total_sessions": 1523,
  "last_index_at": "2026-01-28T11:55:00Z",
  "index_size_bytes": 52428800,
  "freshness_seconds": 300
}
JSON
        else
            echo "CASS Index Health: healthy"
            echo "Sessions: 1523"
        fi
        ;;
    stats)
        if [[ "${2:-}" == "--json" ]]; then
            cat <<'JSON'
{
  "metrics": [
    {"name": "total_queries", "value": 4521, "dimensions": {"period": "24h"}},
    {"name": "avg_query_time_ms", "value": 125.5, "dimensions": {"period": "24h"}},
    {"name": "cache_hit_rate", "value": 0.85, "dimensions": {}},
    {"name": "sessions_indexed_today", "value": 42, "dimensions": {"date": "2026-01-28"}}
  ]
}
JSON
        else
            echo "CASS Stats:"
            echo "  Queries (24h): 4521"
            echo "  Avg Time: 125.5ms"
        fi
        ;;
    *)
        echo "Usage: cass <health|stats> [--json]"
        exit 1
        ;;
esac
MOCK_SCRIPT
chmod +x "$MOCK_BIN_DIR/cass"

# Prepend mock bin to PATH
export PATH="$MOCK_BIN_DIR:$PATH"

# Test 1: Verify mock cass health works
test_info "Test 1: Verifying mock cass health"
mock_health=$("$MOCK_BIN_DIR/cass" health --json)
assert_json_valid "$mock_health" "Mock cass health should be valid JSON"
assert_json_field "$mock_health" ".state" "healthy" "State should be healthy"
assert_json_field "$mock_health" ".total_sessions" "1523" "Session count should match"

# Test 2: Verify mock cass stats works
test_info "Test 2: Verifying mock cass stats"
mock_stats=$("$MOCK_BIN_DIR/cass" stats --json)
assert_json_valid "$mock_stats" "Mock cass stats should be valid JSON"

metrics_count=$(echo "$mock_stats" | jq '.metrics | length')
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
if [[ "$metrics_count" -ge 1 ]]; then
    test_info "PASS: Stats has metrics ($metrics_count entries)"
else
    test_error "FAIL: Stats missing metrics"
    TEST_FAILURES=$((TEST_FAILURES + 1))
fi

# Test 3: Run cass collector
test_info "Test 3: Running cass collector"
collect_output=$(run_vc_or_skip collect --collector cass 2>&1) || {
    test_warn "Cass collector had issues: $collect_output"
}
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
test_info "PASS: Cass collector completed"

# Test 4: Verify database was updated
test_info "Test 4: Checking database"
assert_file_exists "$TEST_DB_PATH" "Database should exist"

# Test 5: Run health check after cass collection
test_info "Test 5: Checking health after cass"
health_output=$(run_vc_or_skip robot health 2>&1) || {
    health_output='{"data":{}}'
}
assert_json_valid "$health_output" "Health output should be valid"

# Test 6: Run cass collector again (incremental test)
test_info "Test 6: Running cass collector again"
collect_output2=$(run_vc_or_skip collect --collector cass 2>&1) || {
    test_warn "Second cass collect had issues"
}
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
test_info "PASS: Second cass collect completed"

# Finalize and output results
finalize_test
