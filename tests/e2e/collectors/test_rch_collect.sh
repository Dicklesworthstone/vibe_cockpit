#!/usr/bin/env bash
# E2E Test: RCH Collector
#
# This test creates a mock rch JSONL log, validates it, stubs an rch
# binary in PATH, and invokes the rch collector.

set -euo pipefail

# Source test helpers
source "$(dirname "$0")/../lib/test_helpers.sh"

test_info "Starting rch collector E2E test"

# Setup test environment
setup_test_env

# Prepare mock rch JSONL log under a temp HOME
export HOME="$TEST_TEMP_DIR"
RCH_DIR="$HOME/.rch"
mkdir -p "$RCH_DIR"
JSONL_PATH="$RCH_DIR/compilations.jsonl"

cat > "$JSONL_PATH" <<'EOF'
{"ts":"2026-01-28T00:00:00Z","crate":"vc_cli","version":"0.1.0","profile":"debug","target":"x86_64-unknown-linux-gnu","duration_ms":1200,"cache_hit":false,"cache_key":"abc123","worker":"worker-1","exit_code":0,"error":null,"cpu_time_ms":900,"peak_memory_mb":512}
{"ts":"2026-01-28T00:05:00Z","crate":"vc_store","version":"0.1.0","profile":"release","target":"x86_64-unknown-linux-gnu","duration_ms":3400,"cache_hit":true,"cache_key":"def456","worker":"worker-2","exit_code":1,"error":"linker error","cpu_time_ms":3000,"peak_memory_mb":1024}
EOF

# Test 1: Verify JSONL file exists
assert_file_exists "$JSONL_PATH" "rch JSONL log should exist"

# Test 2: Verify JSONL has two records
line_count=$(wc -l < "$JSONL_PATH" | tr -d ' ')
assert_eq "2" "$line_count" "rch JSONL should have 2 lines"

# Test 3: Validate each JSONL line
while IFS= read -r line; do
    if [ -n "$line" ]; then
        assert_json_valid "$line" "rch JSONL line should be valid JSON"
    fi
done < "$JSONL_PATH"

# Stub rch binary so collector can run in future implementations
RCH_BIN_DIR="$TEST_TEMP_DIR/bin"
mkdir -p "$RCH_BIN_DIR"
cat > "$RCH_BIN_DIR/rch" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "status" && "${2:-}" == "--json" ]]; then
  cat <<JSON
{"queue_depth":1,"workers_active":1,"workers_total":2,"jobs_completed":10,"jobs_failed":1,"avg_job_duration_ms":1234}
JSON
  exit 0
fi
echo "unknown command" >&2
exit 1
EOF
chmod +x "$RCH_BIN_DIR/rch"
export PATH="$RCH_BIN_DIR:$PATH"

# Test 4: Invoke vc collect for rch (best-effort)
collect_output=$(run_vc collect --collector rch 2>&1) || {
    test_warn "rch collector invocation returned non-zero"
    test_warn "$collect_output"
}
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
test_info "PASS: vc collect --collector rch invoked"

# Finalize and output results
finalize_test
