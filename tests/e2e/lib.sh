#!/usr/bin/env bash
set -euo pipefail

E2E_LOG_DIR_DEFAULT="tests/logs"

_e2e_now_ms() {
  if command -v python3 >/dev/null 2>&1; then
    python3 - <<'PY'
import time
print(int(time.time() * 1000))
PY
  else
    # Fallback to seconds precision
    echo "$(( $(date +%s) * 1000 ))"
  fi
}

_e2e_sanitize_name() {
  local rel="$1"
  echo "$rel" | sed 's#^tests/e2e/##' | sed 's#/#_#g' | sed 's#\.sh$##'
}

_e2e_write_json() {
  local name="$1"
  local status="$2"
  local duration_ms="$3"
  local exit_code="$4"
  local out_file="$5"
  local err_file="$6"
  local json_file="$7"

  if command -v python3 >/dev/null 2>&1; then
    python3 - <<'PY'
import json
import os

name = os.environ['VC_E2E_NAME']
status = os.environ['VC_E2E_STATUS']
duration_ms = int(os.environ['VC_E2E_DURATION_MS'])
exit_code = int(os.environ['VC_E2E_EXIT_CODE'])
out_file = os.environ['VC_E2E_OUT_FILE']
err_file = os.environ['VC_E2E_ERR_FILE']
json_file = os.environ['VC_E2E_JSON_FILE']

error_excerpt = ""
try:
    with open(err_file, 'r', encoding='utf-8', errors='replace') as f:
        lines = f.readlines()
        error_excerpt = "".join(lines[:50]).strip()
except FileNotFoundError:
    error_excerpt = ""

payload = {
    "test_name": name,
    "status": status,
    "duration_ms": duration_ms,
    "exit_code": exit_code,
    "assertions": 0,
    "failures": 0 if status == "pass" else 1,
    "stdout_path": out_file,
    "stderr_path": err_file,
}
if error_excerpt:
    payload["error_excerpt"] = error_excerpt

with open(json_file, 'w', encoding='utf-8') as f:
    json.dump(payload, f, indent=2, sort_keys=True)
PY
  else
    cat > "$json_file" <<JSON
{
  "test_name": "${name}",
  "status": "${status}",
  "duration_ms": ${duration_ms},
  "exit_code": ${exit_code},
  "assertions": 0,
  "failures": $( [ "$status" = "pass" ] && echo 0 || echo 1 ),
  "stdout_path": "${out_file}",
  "stderr_path": "${err_file}"
}
JSON
  fi
}

_e2e_run_test() {
  local script="$1"
  local rel="$2"
  local name
  name=$(_e2e_sanitize_name "$rel")

  local log_dir="${VC_E2E_LOG_DIR:-$E2E_LOG_DIR_DEFAULT}"
  mkdir -p "$log_dir"

  local out_file="$log_dir/${name}.out"
  local err_file="$log_dir/${name}.err"
  local json_file="$log_dir/${name}.json"

  local start_ms
  start_ms=$(_e2e_now_ms)

  local exit_code=0
  if bash -x "$script" >"$out_file" 2>"$err_file"; then
    exit_code=0
  else
    exit_code=$?
  fi

  local end_ms
  end_ms=$(_e2e_now_ms)
  local duration_ms=$((end_ms - start_ms))

  local status="pass"
  if [ "$exit_code" -eq 1 ]; then
    status="fail"
  elif [ "$exit_code" -eq 2 ]; then
    status="skip"
  fi

  VC_E2E_NAME="$name" \
  VC_E2E_STATUS="$status" \
  VC_E2E_DURATION_MS="$duration_ms" \
  VC_E2E_EXIT_CODE="$exit_code" \
  VC_E2E_OUT_FILE="$out_file" \
  VC_E2E_ERR_FILE="$err_file" \
  VC_E2E_JSON_FILE="$json_file" \
  _e2e_write_json "$name" "$status" "$duration_ms" "$exit_code" "$out_file" "$err_file" "$json_file"

  if [ -n "${VC_E2E_VERBOSE:-}" ]; then
    echo "$status $name (stdout: $out_file, stderr: $err_file, json: $json_file)"
  else
    echo "$status $name"
  fi
  return "$exit_code"
}

export -f _e2e_run_test _e2e_now_ms _e2e_sanitize_name _e2e_write_json
