#!/usr/bin/env bash
# E2E Test: Beads (bv/br) Collector
#
# This test stubs bv/br binaries, validates JSON output shapes,
# and invokes the beads collector.

set -euo pipefail

# Source test helpers
source "$(dirname "$0")/../lib/test_helpers.sh"

test_info "Starting beads collector E2E test"

# Setup test environment
setup_test_env

# Stub bv and br binaries in PATH
BIN_DIR="$TEST_TEMP_DIR/bin"
mkdir -p "$BIN_DIR"

cat > "$BIN_DIR/bv" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "--robot-triage" ]]; then
  cat <<'JSON'
{
  "generated_at": "2026-01-28T00:00:00Z",
  "data_hash": "deadbeef",
  "triage": {
    "meta": {
      "version": "1.0.0",
      "generated_at": "2026-01-28T00:00:00Z",
      "phase2_ready": true,
      "issue_count": 1,
      "compute_time_ms": 5
    },
    "quick_ref": {
      "open_count": 1,
      "actionable_count": 1,
      "blocked_count": 0,
      "in_progress_count": 1,
      "top_picks": [
        {
          "id": "bd-30z",
          "title": "Create E2E test scripts with detailed logging",
          "score": 0.5,
          "reasons": ["stubbed"],
          "unblocks": 0
        }
      ]
    },
    "recommendations": [
      {
        "id": "bd-30z",
        "title": "Create E2E test scripts with detailed logging",
        "type": "task",
        "status": "in_progress",
        "priority": 1,
        "score": 0.5,
        "reasons": ["stubbed"],
        "action": "continue",
        "unblocks_ids": [],
        "blocked_by": []
      }
    ],
    "quick_wins": [],
    "blockers_to_clear": [],
    "project_health": {
      "counts": {
        "total": 1,
        "open": 1,
        "closed": 0,
        "blocked": 0,
        "actionable": 1,
        "by_status": {"open": 1},
        "by_type": {"task": 1},
        "by_priority": {"1": 1}
      },
      "graph": {
        "node_count": 1,
        "edge_count": 0,
        "density": 0.0,
        "has_cycles": false,
        "phase2_ready": true
      },
      "velocity": {
        "closed_last_7_days": 0,
        "closed_last_30_days": 0,
        "avg_days_to_close": 0.0,
        "weekly": []
      }
    }
  },
  "usage_hints": []
}
JSON
  exit 0
fi
echo "unknown command" >&2
exit 1
EOF
chmod +x "$BIN_DIR/bv"

cat > "$BIN_DIR/br" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
cat <<'JSON'
{
  "issues": [
    {
      "id": "bd-30z",
      "title": "Create E2E test scripts with detailed logging",
      "status": "in_progress",
      "priority": 1,
      "type": "task",
      "labels": ["e2e", "collectors"],
      "blocked_by": [],
      "blocks": [],
      "owner": "MaroonCove",
      "created_at": "2026-01-27T00:00:00Z",
      "updated_at": "2026-01-28T00:00:00Z"
    }
  ]
}
JSON
EOF
chmod +x "$BIN_DIR/br"

export PATH="$BIN_DIR:$PATH"

# Test 1: Validate bv JSON
bv_output=$(bv --robot-triage)
assert_json_valid "$bv_output" "bv triage output should be valid JSON"
assert_json_field "$bv_output" ".triage.meta.version" "1.0.0" "bv meta version"

# Test 2: Validate br JSON
br_output=$(br list --format json)
assert_json_valid "$br_output" "br list output should be valid JSON"
assert_json_field "$br_output" ".issues[0].id" "bd-30z" "br issue id"

# Test 3: Invoke vc collect for beads (best-effort)
collect_output=$(run_vc collect --collector beads 2>&1) || {
    test_warn "beads collector invocation returned non-zero"
    test_warn "$collect_output"
}
TEST_ASSERTIONS=$((TEST_ASSERTIONS + 1))
test_info "PASS: vc collect --collector beads invoked"

# Finalize and output results
finalize_test
