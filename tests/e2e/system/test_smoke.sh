#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT_DIR"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not found" >&2
  exit 2
fi

# Basic workspace sanity checks without building.
[ -f Cargo.toml ]
[ -d crates ]

# Ensure Cargo can parse the workspace manifest.
cargo metadata --format-version 1 --no-deps >/dev/null
