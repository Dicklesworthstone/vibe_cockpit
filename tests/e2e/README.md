# E2E Test Harness

This directory contains the end-to-end test harness used by bd-30z.
Feature beads should add their own `test_*.sh` scripts under the appropriate
subcategory (collectors/, remote/, tui/, web/, system/).

## Runner

```bash
./tests/e2e/run_all.sh
./tests/e2e/run_all.sh --filter collectors
./tests/e2e/run_all.sh --category collectors
./tests/e2e/run_all.sh --parallel 4
./tests/e2e/run_all.sh --verbose
```

Exit codes:
- 0 = pass
- 1 = fail
- 2 = skip/no tests

## Logging

All tests are executed with `bash -x` for command tracing. Per-test artifacts
are stored in `tests/logs/`:
- `<test>.out` stdout
- `<test>.err` stderr
- `<test>.json` JSON summary

A `tests/logs/summary.json` aggregate report is produced after a run.

## Categories

Supported categories for `--category`:
`collectors`, `remote`, `tui`, `web`, `system`.

## Writing Tests

- Name scripts `test_<feature>.sh`.
- Use explicit, deterministic fixtures.
- Prefer read-only operations where possible.
- Return 2 when a test is intentionally skipped (missing dependency).
