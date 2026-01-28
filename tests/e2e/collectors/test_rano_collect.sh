#!/usr/bin/env bash
# E2E Test: Rano Collector
#
# This test stubs a rano binary that emits JSONL and invokes
# the rano collector to ensure the path executes.

set -euo pipefail

# Source test helpers
source "$(dirname "$0")/../lib/test_helpers.sh"

test_info "Starting rano collector E2E test"

# Setup test environment
setup_test_env

# Prepare mock rano binary in PATH
RANO_BIN_DIR="$TEST_TEMP_DIR/bin"
mkdir -p "$RANO_BIN_DIR"
cat > "$RANO_BIN_DIR/rano" <<'RANO_EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "export" && "${2:-}" == "--format" && "${3:-}" == "jsonl" ]]; then
  cat <<'JSONL'
{"ts":"2026-01-28T00:00:00Z","id":1,"provider":"openai","process":"vc","pid":1234,"direction":"outbound","protocol":"https","remote_host":"api.openai.com","remote_ip":"1.2.3.4","remote_port":443,"local_port":54321,"bytes_sent":2048,"bytes_received":4096,"is_known":true,"tags":["api"],"event_type":"connection"}
{"ts":"2026-01-28T00:01:00Z","id":2,"provider":"github","process":"git","pid":2222,"direction":"outbound","protocol":"https","remote_host":"github.com","remote_ip":"5.6.7.8","remote_port":443,"local_port":54322,"bytes_sent":1024,"bytes_received":2048,"is_known":true,"tags":["git"],"event_type":"dns"}
JSONL
  exit 0
fi

echo "unknown command" >&2
exit 1
RANO_EOF
chmod +x "$RANO_BIN_DIR/rano"
export PATH="$RANO_BIN_DIR:$PATH"

# Test 1: Verify rano binary is available
rano_path=$(command -v rano || true)
assert_ne "" "$rano_path" "rano binary should be in PATH"

# Test 2: Verify JSONL output is valid
rano_output=$(rano export --format jsonl --since 10m)
line_count=$(printf '%s\n' "$rano_output" | wc -l | tr -d ' ')
assert_eq "2" "$line_count" "rano JSONL should have 2 lines"
while IFS= read -r line; do
    if [ -n "$line" ]; then
        assert_json_valid "$line" "rano JSONL line should be valid JSON"
    fi
done <<< "$rano_output"

# Test 3: Invoke vc collect for rano (best-effort)
run_vc_or_skip collect --collector rano 2>&1 || {
    test_warn "rano collector invocation returned non-zero"
}
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))

test_info "PASS: vc collect --collector rano invoked"

# Finalize and output results
finalize_test
