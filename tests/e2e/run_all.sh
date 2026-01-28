#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
E2E_DIR="$ROOT_DIR/tests/e2e"
LOG_DIR="$ROOT_DIR/tests/logs"

FILTER=""
PARALLEL=""
CATEGORY=""
VERBOSE=""

usage() {
  cat <<USAGE
Usage: tests/e2e/run_all.sh [--filter <pattern>] [--parallel <n>] [--category <name>] [--verbose]

Options:
  --filter <pattern>   Only run tests whose path matches the pattern (grep -E)
  --parallel <n>       Run up to n tests in parallel
  --category <name>    Only run tests in a category (collectors, remote, tui, web, system)
  --verbose            Print log file locations for each test
USAGE
}

while [ $# -gt 0 ]; do
  case "$1" in
    --filter)
      FILTER="$2"
      shift 2
      ;;
    --parallel)
      PARALLEL="$2"
      shift 2
      ;;
    --category)
      CATEGORY="$2"
      shift 2
      ;;
    --verbose)
      VERBOSE=1
      shift 1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage
      exit 1
      ;;
  esac
done

mkdir -p "$LOG_DIR"

# shellcheck source=tests/e2e/lib.sh
source "$E2E_DIR/lib.sh"

mapfile -t tests < <(find "$E2E_DIR" -type f -name 'test_*.sh' | sort)

if [ "${#tests[@]}" -eq 0 ]; then
  echo "No e2e tests found." >&2
  exit 2
fi

if [ -n "$CATEGORY" ]; then
  mapfile -t tests < <(printf '%s\n' "${tests[@]}" | grep -E "$E2E_DIR/$CATEGORY/" || true)
fi

if [ -n "$FILTER" ]; then
  mapfile -t tests < <(printf '%s\n' "${tests[@]}" | grep -E "$FILTER" || true)
fi

if [ "${#tests[@]}" -eq 0 ]; then
  echo "No e2e tests matched filter." >&2
  exit 2
fi

results_file="$LOG_DIR/summary.json"

run_one() {
  local script="$1"
  local rel
  rel="${script#$ROOT_DIR/}"
  _e2e_run_test "$script" "$rel"
}

export ROOT_DIR
export E2E_DIR
export LOG_DIR
if [ -n "$VERBOSE" ]; then
  export VC_E2E_VERBOSE=1
fi
export -f run_one

fail_count=0
skip_count=0
pass_count=0

echo "Running ${#tests[@]} e2e tests..."

if [ -n "$PARALLEL" ]; then
  printf '%s\n' "${tests[@]}" | xargs -I{} -P "$PARALLEL" bash -c 'run_one "$@"' _ {}
else
  for script in "${tests[@]}"; do
    if run_one "$script"; then
      pass_count=$((pass_count + 1))
    else
      rc=$?
      if [ "$rc" -eq 2 ]; then
        skip_count=$((skip_count + 1))
      else
        fail_count=$((fail_count + 1))
      fi
    fi
  done
fi

if command -v python3 >/dev/null 2>&1; then
  python3 - <<'PY'
import glob
import json
import os

log_dir = os.environ["LOG_DIR"]
summary_path = os.path.join(log_dir, "summary.json")

entries = []
for path in sorted(glob.glob(os.path.join(log_dir, "*.json"))):
    if path.endswith("summary.json"):
        continue
    with open(path, "r", encoding="utf-8") as f:
        entries.append(json.load(f))

summary = {
    "total": len(entries),
    "passed": sum(1 for e in entries if e.get("status") == "pass"),
    "failed": sum(1 for e in entries if e.get("status") == "fail"),
    "skipped": sum(1 for e in entries if e.get("status") == "skip"),
    "results": entries,
}

with open(summary_path, "w", encoding="utf-8") as f:
    json.dump(summary, f, indent=2, sort_keys=True)
PY
fi

if [ "$fail_count" -gt 0 ]; then
  exit 1
fi

if [ "$pass_count" -eq 0 ]; then
  exit 2
fi

exit 0
