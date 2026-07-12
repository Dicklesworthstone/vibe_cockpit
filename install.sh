#!/usr/bin/env bash
#
# vc (Vibe Cockpit) installer
#
# One-liner install (with cache buster):
#   curl -fsSL "https://raw.githubusercontent.com/Dicklesworthstone/vibe_cockpit/main/install.sh?$(date +%s)" | bash
#
# Or without cache buster:
#   curl -fsSL https://raw.githubusercontent.com/Dicklesworthstone/vibe_cockpit/main/install.sh | bash
#
# Options:
#   --version vX.Y.Z   Install specific version (default: latest)
#   --dest DIR         Install to DIR (default: ~/.local/bin)
#   --system           Install to /usr/local/bin (requires sudo)
#   --easy-mode        Auto-update PATH in shell rc files
#   --verify           Run a self-test after install
#   --uninstall        Remove an installed vc and exit
#   --force            Reinstall even if the target version is already present
#   --quiet            Suppress non-error output
#   --offline          Skip network preflight checks
#   --no-verify        Skip checksum verification (for testing only)
#
# NOTE ON SOURCE BUILDS
# ---------------------
# Unlike the other Rust CLIs in this ecosystem, vc CANNOT be built from a bare
# `git clone` (and therefore cannot be `cargo install`ed). Its workspace
# Cargo.toml carries PATH dependencies on sibling checkouts:
#
#   ftui           = { path = "../frankentui/crates/ftui" }
#   fsqlite        = { path = "../frankensqlite/crates/fsqlite" }
#   fsqlite-error  = { path = "../frankensqlite/crates/fsqlite-error" }
#   fsqlite-types  = { path = "../frankensqlite/crates/fsqlite-types" }
#
# A source build requires Dicklesworthstone/frankentui and
# Dicklesworthstone/frankensqlite checked out as siblings of this repo. So this
# installer only ever fetches a PREBUILT BINARY from GitHub Releases — there is
# deliberately no --from-source path. See .github/workflows/release.yml for how
# the release binaries are produced.
#
set -euo pipefail
umask 022

VERSION="${VERSION:-}"
OWNER="${OWNER:-Dicklesworthstone}"
REPO="${REPO:-vibe_cockpit}"
BIN_NAME="vc"
DEST_DEFAULT="$HOME/.local/bin"
DEST="${DEST:-$DEST_DEFAULT}"
EASY=0
QUIET=0
VERIFY=0
UNINSTALL=0
SYSTEM=0
NO_CHECKSUM=0
FORCE_INSTALL=0
OFFLINE="${VC_OFFLINE:-0}"
CHECKSUM="${CHECKSUM:-}"
CHECKSUM_URL="${CHECKSUM_URL:-}"
ARTIFACT_URL="${ARTIFACT_URL:-}"
LOCK_FILE="/tmp/vc-install.lock"

# Installer's own version — bumped when the script's contract changes.
INSTALLER_VERSION="1.0.0"

# ─── Logging ──────────────────────────────────────────────────────────────────

info() { [ "$QUIET" -eq 1 ] && return 0; printf '\033[0;34m→\033[0m %s\n' "$*"; }
ok()   { [ "$QUIET" -eq 1 ] && return 0; printf '\033[0;32m✓\033[0m %s\n' "$*"; }
warn() { [ "$QUIET" -eq 1 ] && return 0; printf '\033[1;33m⚠\033[0m %s\n' "$*"; }
err()  { printf '\033[0;31m✗\033[0m %s\n' "$*" >&2; }

has_cmd() { command -v "$1" >/dev/null 2>&1; }

usage() {
  cat <<EOFU
vc installer — Vibe Cockpit (agent fleet monitoring and orchestration)

Usage: install.sh [--version vX.Y.Z] [--dest DIR] [--system] [--easy-mode] \\
                  [--verify] [--uninstall] [--force] [--quiet] [--offline] \\
                  [--artifact-url URL] [--checksum HEX] [--checksum-url URL] \\
                  [--no-verify]

Options:
  --version vX.Y.Z   Install a specific version (default: latest release).
                     With no value, prints this installer's version and exits.
  --dest DIR         Install to DIR (default: ~/.local/bin)
  --system           Install to /usr/local/bin (requires sudo)
  --easy-mode        Auto-update PATH in shell rc files
  --verify           Run a self-test (vc --version) after install
  --uninstall        Remove an installed vc and exit
  --force            Reinstall even if the target version is already installed
  --quiet, -q        Suppress non-error output
  --offline          Skip network preflight checks
  --artifact-url URL Install from an explicit tarball URL (testing)
  --checksum HEX     Expected SHA256 of the artifact (testing)
  --checksum-url URL Where to fetch the expected SHA256 (default: SHA256SUMS)
  --no-verify        Skip checksum verification (NOT recommended)
  -h, --help         Show this help

Examples:
  # Install the latest release
  curl -fsSL https://raw.githubusercontent.com/${OWNER}/${REPO}/main/install.sh | bash

  # Install a specific version
  curl -fsSL https://raw.githubusercontent.com/${OWNER}/${REPO}/main/install.sh | bash -s -- --version v0.1.0

  # Install system-wide
  curl -fsSL https://raw.githubusercontent.com/${OWNER}/${REPO}/main/install.sh | bash -s -- --system

  # Uninstall
  curl -fsSL https://raw.githubusercontent.com/${OWNER}/${REPO}/main/install.sh | bash -s -- --uninstall

vc ships as a prebuilt binary only. It cannot be 'cargo install'ed because its
workspace has path dependencies on the frankentui and frankensqlite sibling
checkouts. See the header of this script for details.
EOFU
}

require_option_value() {
  local option="$1"
  local value="${2:-}"

  if [ -z "$value" ] || [[ "$value" == -* ]]; then
    err "$option requires a value"
    usage
    exit 2
  fi
}

# ─── Argument parsing ─────────────────────────────────────────────────────────

while [ $# -gt 0 ]; do
  case "$1" in
    --version)
      # `--version vX.Y.Z` selects a release (the house idiom). A bare
      # `--version` with no value prints the installer's own version, which is
      # what a user typing it by reflex expects.
      if [ -z "${2:-}" ] || [[ "${2:-}" == -* ]]; then
        printf 'vc installer %s\n' "$INSTALLER_VERSION"
        exit 0
      fi
      VERSION="$2"; shift 2;;
    --version=*) VERSION="${1#*=}"; shift;;
    --dest) require_option_value "$1" "${2:-}"; DEST="$2"; shift 2;;
    --dest=*) DEST="${1#*=}"; shift;;
    --system) SYSTEM=1; DEST="/usr/local/bin"; shift;;
    --easy-mode) EASY=1; shift;;
    --verify) VERIFY=1; shift;;
    --uninstall) UNINSTALL=1; shift;;
    --force) FORCE_INSTALL=1; shift;;
    --quiet|-q) QUIET=1; shift;;
    --offline) OFFLINE=1; shift;;
    --artifact-url) require_option_value "$1" "${2:-}"; ARTIFACT_URL="$2"; shift 2;;
    --checksum) require_option_value "$1" "${2:-}"; CHECKSUM="$2"; shift 2;;
    --checksum-url) require_option_value "$1" "${2:-}"; CHECKSUM_URL="$2"; shift 2;;
    --no-verify) NO_CHECKSUM=1; shift;;
    -h|--help) usage; exit 0;;
    *) err "Unknown option: $1"; usage; exit 2;;
  esac
done

# ─── Uninstall ────────────────────────────────────────────────────────────────

uninstall_vc() {
  local found=0 candidate

  # Look in the explicit --dest first, then the standard prefixes, then
  # whatever `vc` currently resolves to on PATH (which may be none of these).
  local -a candidates=("$DEST/$BIN_NAME" "$HOME/.local/bin/$BIN_NAME" "/usr/local/bin/$BIN_NAME")
  if resolved=$(command -v "$BIN_NAME" 2>/dev/null); then
    candidates+=("$resolved")
  fi

  local -a seen=()
  for candidate in "${candidates[@]}"; do
    # De-duplicate: --dest may equal ~/.local/bin, and `command -v` may resolve
    # to a path we have already handled.
    local dup=0 s
    for s in "${seen[@]:-}"; do
      [ "$s" = "$candidate" ] && dup=1 && break
    done
    [ "$dup" -eq 1 ] && continue
    seen+=("$candidate")

    [ -e "$candidate" ] || continue
    found=1
    if rm -f "$candidate" 2>/dev/null; then
      ok "Removed $candidate"
    else
      info "Removing $candidate requires sudo..."
      if sudo rm -f "$candidate"; then
        ok "Removed $candidate"
      else
        err "Could not remove $candidate"
        exit 1
      fi
    fi
  done

  if [ "$found" -eq 0 ]; then
    warn "No vc binary found (looked in $DEST, ~/.local/bin, /usr/local/bin, and \$PATH)"
    exit 0
  fi

  echo ""
  ok "vc uninstalled"
  info "Configuration and local state were left in place. Remove them manually if you want a clean slate:"
  echo "  rm -rf ~/.config/vc ~/.local/share/vc"
  exit 0
}

if [ "$UNINSTALL" -eq 1 ]; then
  uninstall_vc
fi

# ─── Header ───────────────────────────────────────────────────────────────────

if [ "$QUIET" -eq 0 ]; then
  echo ""
  printf '\033[1;32mvc installer\033[0m\n'
  printf '\033[0;90mAgent fleet monitoring and orchestration\033[0m\n'
  echo ""
fi

# ─── Version resolution ───────────────────────────────────────────────────────

is_release_version() {
  local version="${1#v}"
  [[ "$version" =~ ^[0-9]+[.][0-9]+[.][0-9]+(-[0-9A-Za-z][0-9A-Za-z.-]*)?([+][0-9A-Za-z][0-9A-Za-z.-]*)?$ ]]
}

normalize_release_version() {
  local version="$1"

  if ! is_release_version "$version"; then
    err "Invalid version: $version (expected vX.Y.Z or X.Y.Z)"
    return 1
  fi

  printf 'v%s\n' "${version#v}"
}

resolve_version() {
  if [ -n "$ARTIFACT_URL" ]; then return 0; fi

  if [ -n "$VERSION" ]; then
    VERSION=$(normalize_release_version "$VERSION") || exit 1
    return 0
  fi

  info "Resolving latest version..."
  local latest_url="https://api.github.com/repos/${OWNER}/${REPO}/releases/latest"
  local tag=""

  if has_cmd curl; then
    tag=$(curl -fsSL -H "Accept: application/vnd.github.v3+json" "$latest_url" 2>/dev/null \
      | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/' | head -1) || tag=""
  elif has_cmd wget; then
    tag=$(wget -qO- "$latest_url" 2>/dev/null \
      | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/' | head -1) || tag=""
  else
    err "Neither curl nor wget found. Please install one."
    exit 1
  fi

  if [ -n "$tag" ] && is_release_version "$tag"; then
    VERSION=$(normalize_release_version "$tag")
    info "Resolved latest version: $VERSION"
    return 0
  fi

  # The API is rate-limited per-IP for unauthenticated callers; the
  # /releases/latest HTML endpoint redirects to /releases/tag/<tag> and is not,
  # so it is a reliable fallback when the JSON API says 403.
  local redirect_url="https://github.com/${OWNER}/${REPO}/releases/latest"
  if has_cmd curl; then
    tag=$(curl -fsSL -o /dev/null -w '%{url_effective}' "$redirect_url" 2>/dev/null | sed -E 's|.*/tag/||') || tag=""
    if [ -n "$tag" ] && [[ "$tag" != *"/"* ]] && is_release_version "$tag"; then
      VERSION=$(normalize_release_version "$tag")
      info "Resolved latest version via redirect: $VERSION"
      return 0
    fi
  fi

  err "Could not resolve the latest release for ${OWNER}/${REPO}."
  err "If no release exists yet, there is nothing to install."
  err "Otherwise re-run with an explicit --version vX.Y.Z."
  exit 1
}

# ─── Platform detection ───────────────────────────────────────────────────────

detect_platform() {
  OS=$(uname -s | tr 'A-Z' 'a-z')
  ARCH=$(uname -m)

  case "$ARCH" in
    x86_64|amd64) ARCH="x86_64" ;;
    arm64|aarch64) ARCH="aarch64" ;;
    *) : ;;
  esac

  TARGET=""
  case "${OS}-${ARCH}" in
    # Linux x86_64 ships as a glibc build produced on an older-glibc runner
    # rather than a fully-static musl binary. dcg can use musl because it has
    # no C dependencies; vc bundles DuckDB (C++) and links reqwest, so a static
    # musl build is not currently viable. See the build matrix comment in
    # .github/workflows/release.yml.
    linux-x86_64)   TARGET="x86_64-unknown-linux-gnu" ;;
    linux-aarch64)  TARGET="aarch64-unknown-linux-gnu" ;;
    darwin-x86_64)  TARGET="x86_64-apple-darwin" ;;
    darwin-aarch64) TARGET="aarch64-apple-darwin" ;;
    *) : ;;
  esac

  if [ -z "$TARGET" ] && [ -z "$ARTIFACT_URL" ]; then
    err "Unsupported platform: ${OS}/${ARCH}"
    err "vc publishes prebuilt binaries for:"
    err "  linux  x86_64 / aarch64"
    err "  macOS  x86_64 / aarch64 (Apple Silicon)"
    err ""
    err "There is no build-from-source fallback: vc's Cargo.toml has path"
    err "dependencies on the frankentui and frankensqlite sibling repos, so a"
    err "clone of this repo alone will not compile. To build it yourself, check"
    err "out all three repos side by side:"
    err "  git clone https://github.com/${OWNER}/frankentui.git"
    err "  git clone https://github.com/${OWNER}/frankensqlite.git"
    err "  git clone https://github.com/${OWNER}/${REPO}.git && cd ${REPO} && cargo build --release"
    exit 1
  fi

  [ -n "$TARGET" ] && info "Detected platform: ${OS}/${ARCH} (${TARGET})"
}

set_artifact_url() {
  if [ -n "$ARTIFACT_URL" ]; then
    TAR=$(basename "$ARTIFACT_URL")
    URL="$ARTIFACT_URL"
    return 0
  fi

  TAR="${BIN_NAME}-${TARGET}.tar.xz"
  URL="https://github.com/${OWNER}/${REPO}/releases/download/${VERSION}/${TAR}"
}

# ─── Preflight ────────────────────────────────────────────────────────────────

check_disk_space() {
  local min_kb=51200   # 50MB — vc bundles DuckDB and is a fat binary
  local path="$DEST"

  if [ ! -d "$path" ]; then
    path=$(dirname "$path")
  fi

  if has_cmd df; then
    local avail_kb
    avail_kb=$(df -Pk "$path" | awk 'NR==2 {print $4}')
    if [ -n "$avail_kb" ] && [ "$avail_kb" -lt "$min_kb" ]; then
      err "Insufficient disk space in $path (need at least 50MB)"
      exit 1
    fi
  else
    warn "df not found; skipping disk space check"
  fi
}

check_write_permissions() {
  if [ ! -d "$DEST" ]; then
    if ! mkdir -p "$DEST" 2>/dev/null; then
      if [ "$SYSTEM" -eq 1 ]; then
        info "Creating $DEST requires sudo..."
        sudo mkdir -p "$DEST" || { err "Cannot create $DEST"; exit 1; }
        return 0
      fi
      err "Cannot create $DEST (insufficient permissions)"
      err "Try --system (installs to /usr/local/bin via sudo) or choose a writable --dest"
      exit 1
    fi
  fi

  if [ ! -w "$DEST" ] && [ "$SYSTEM" -eq 0 ]; then
    err "No write permission to $DEST"
    err "Try --system (installs to /usr/local/bin via sudo) or choose a writable --dest"
    exit 1
  fi
}

check_existing_install() {
  if [ -x "$DEST/$BIN_NAME" ]; then
    local current
    current=$("$DEST/$BIN_NAME" --version 2>/dev/null | head -1 || echo "")
    if [ -n "$current" ]; then
      info "Existing vc detected: $current"
    fi
  fi
}

check_network() {
  if [ "$OFFLINE" -eq 1 ]; then
    info "Offline mode enabled; skipping network preflight"
    return 0
  fi
  if [ -z "${URL:-}" ]; then
    return 0
  fi
  if ! has_cmd curl; then
    warn "curl not found; skipping network check"
    return 0
  fi
  if ! curl -fsIL --connect-timeout 3 --max-time 10 -o /dev/null "$URL"; then
    warn "Network preflight failed for $URL"
    warn "Continuing; the download may still fail"
  fi
}

preflight_checks() {
  info "Running preflight checks"
  check_disk_space
  check_write_permissions
  check_existing_install
  check_network
}

# ─── Idempotency ──────────────────────────────────────────────────────────────

# Returns 0 when the already-installed vc matches the target version, so a
# re-run of the one-liner is a cheap no-op instead of a redundant download.
check_installed_version() {
  local target_version="$1"

  if [ ! -x "$DEST/$BIN_NAME" ]; then
    return 1
  fi

  local installed_version
  # `vc --version` prints "vc <semver> (<git sha> <date>)" — the build.rs
  # vergen metadata is appended, so match the leading semver only.
  installed_version=$("$DEST/$BIN_NAME" --version 2>/dev/null \
    | sed -n 's/.*[[:space:]]v\{0,1\}\([0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*\).*/\1/p' | head -1)

  if [ -z "$installed_version" ]; then
    return 1
  fi

  local target_clean="${target_version#v}"
  local installed_clean="${installed_version#v}"

  [ "$target_clean" = "$installed_clean" ]
}

# ─── Checksum verification ────────────────────────────────────────────────────

sha256_of() {
  local file="$1"

  if has_cmd sha256sum; then
    sha256sum "$file" | awk '{print $1}'
  elif has_cmd shasum; then
    shasum -a 256 "$file" | awk '{print $1}'
  else
    return 1
  fi
}

# Pull the expected hash for $TAR out of a SHA256SUMS-style manifest. Tolerates
# both the GNU "<hash>  <name>" and BSD "<hash> *<name>" spellings, plus CRLF.
expected_from_manifest() {
  local manifest="$1"
  local name="$2"

  awk -v want="$name" '
    {
      filename = $NF
      sub(/\r$/, "", filename)
      sub(/^\*/, "", filename)
      sub(/^.*\//, "", filename)
      if (filename == want) {
        print $1
        exit
      }
    }
  ' "$manifest"
}

verify_checksum() {
  local file="$1"
  local expected="$2"
  local actual=""

  if [ ! -f "$file" ]; then
    err "File not found: $file"
    return 1
  fi

  if ! actual=$(sha256_of "$file"); then
    err "Need sha256sum or shasum to verify the download"
    err "Use --no-verify to skip verification (not recommended)"
    return 1
  fi

  expected=$(printf '%s' "$expected" | tr '[:upper:]' '[:lower:]')
  actual=$(printf '%s' "$actual" | tr '[:upper:]' '[:lower:]')

  if [ "$actual" != "$expected" ]; then
    err "Checksum verification FAILED!"
    err "Expected: $expected"
    err "Got:      $actual"
    err "The downloaded file may be corrupted or tampered with."
    rm -f "$file"
    return 1
  fi

  ok "Checksum verified: ${actual:0:16}..."
  return 0
}

# ─── PATH guidance ────────────────────────────────────────────────────────────

maybe_add_path() {
  case ":$PATH:" in
    *:"$DEST":*)
      return 0
      ;;
    *)
      if [ "$EASY" -eq 1 ]; then
        local updated=0 rc
        for rc in "$HOME/.zshrc" "$HOME/.bashrc"; do
          if [ -e "$rc" ] && [ -w "$rc" ]; then
            if ! grep -F "$DEST" "$rc" >/dev/null 2>&1; then
              {
                echo ""
                echo "# Added by the vc installer"
                echo "export PATH=\"$DEST:\$PATH\""
              } >> "$rc"
            fi
            updated=1
          fi
        done
        if [ "$updated" -eq 1 ]; then
          warn "PATH updated in ~/.zshrc / ~/.bashrc; restart your shell to use vc"
        else
          warn "Add $DEST to your PATH to use vc"
        fi
      else
        warn "$DEST is not in your PATH"
        echo ""
        echo "Add this to your shell rc file:"
        echo "  export PATH=\"$DEST:\$PATH\""
        echo ""
        echo "Or re-run the installer with --easy-mode to have it done for you."
        echo ""
      fi
      ;;
  esac
}

# ─── Main ─────────────────────────────────────────────────────────────────────

resolve_version
detect_platform
set_artifact_url

# ~/.local/bin does not exist on a fresh system; create it before preflight so
# the write-permission check has something to test.
mkdir -p "$DEST" 2>/dev/null || true

preflight_checks

if [ "$FORCE_INSTALL" -eq 0 ] && [ -z "$ARTIFACT_URL" ] && check_installed_version "$VERSION"; then
  ok "vc $VERSION is already installed at $DEST/$BIN_NAME"
  info "Use --force to reinstall"
  maybe_add_path
  exit 0
fi

# Cross-platform locking via mkdir, which is atomic on every POSIX system
# (including macOS, where flock(1) does not exist).
LOCK_DIR="${LOCK_FILE}.d"
LOCKED=0
if mkdir "$LOCK_DIR" 2>/dev/null; then
  LOCKED=1
  echo $$ > "$LOCK_DIR/pid"
else
  if [ -f "$LOCK_DIR/pid" ]; then
    OLD_PID=$(cat "$LOCK_DIR/pid" 2>/dev/null || echo "")
    if [ -n "$OLD_PID" ] && ! kill -0 "$OLD_PID" 2>/dev/null; then
      rm -rf "$LOCK_DIR"
      if mkdir "$LOCK_DIR" 2>/dev/null; then
        LOCKED=1
        echo $$ > "$LOCK_DIR/pid"
      fi
    fi
  fi
  if [ "$LOCKED" -eq 0 ]; then
    err "Another vc installer is running (lock $LOCK_DIR)"
    exit 1
  fi
fi

cleanup() {
  [ -n "${TMP:-}" ] && rm -rf "$TMP"
  if [ "$LOCKED" -eq 1 ]; then rm -rf "$LOCK_DIR"; fi
}

TMP=$(mktemp -d)
trap cleanup EXIT

info "Downloading $URL"
if has_cmd curl; then
  if ! curl -fsSL "$URL" -o "$TMP/$TAR"; then
    err "Download failed: $URL"
    err "Check that release ${VERSION:-<artifact-url>} publishes an asset named $TAR"
    exit 1
  fi
elif has_cmd wget; then
  if ! wget -q "$URL" -O "$TMP/$TAR"; then
    err "Download failed: $URL"
    err "Check that release ${VERSION:-<artifact-url>} publishes an asset named $TAR"
    exit 1
  fi
else
  err "Neither curl nor wget found. Please install one."
  exit 1
fi

if [ "$NO_CHECKSUM" -eq 1 ]; then
  warn "Checksum verification skipped (--no-verify)"
else
  if [ -z "$CHECKSUM" ]; then
    # Prefer the aggregate SHA256SUMS manifest the release workflow publishes;
    # fall back to the per-artifact <name>.sha256 sidecar.
    if [ -z "$CHECKSUM_URL" ]; then
      CHECKSUM_URL="$(dirname "$URL")/SHA256SUMS"
    fi

    info "Fetching checksums from $CHECKSUM_URL"
    CHECKSUM_FILE="$TMP/SHA256SUMS"

    if curl -fsSL "$CHECKSUM_URL" -o "$CHECKSUM_FILE" 2>/dev/null; then
      CHECKSUM=$(expected_from_manifest "$CHECKSUM_FILE" "$TAR")
    fi

    if [ -z "$CHECKSUM" ]; then
      local_sidecar="${URL}.sha256"
      info "No entry in SHA256SUMS; falling back to $local_sidecar"
      if curl -fsSL "$local_sidecar" -o "$TMP/sidecar.sha256" 2>/dev/null; then
        CHECKSUM=$(awk '{print $1}' "$TMP/sidecar.sha256" | head -1)
      fi
    fi

    if [ -z "$CHECKSUM" ]; then
      err "Could not obtain a SHA256 checksum for $TAR"
      err "Use --no-verify to skip verification (not recommended)"
      exit 1
    fi
  fi

  if ! printf '%s' "$CHECKSUM" | grep -Eq '^[0-9A-Fa-f]{64}$'; then
    err "Malformed SHA256 checksum for $TAR: $CHECKSUM"
    exit 1
  fi

  if ! verify_checksum "$TMP/$TAR" "$CHECKSUM"; then
    err "Installation aborted due to checksum failure"
    exit 1
  fi
fi

info "Extracting"
if ! tar -xf "$TMP/$TAR" -C "$TMP"; then
  err "Could not extract $TAR"
  exit 1
fi

BIN="$TMP/$BIN_NAME"
if [ ! -x "$BIN" ] && [ -n "$TARGET" ]; then
  BIN="$TMP/${BIN_NAME}-${TARGET}/$BIN_NAME"
fi
if [ ! -x "$BIN" ]; then
  BIN=$(find "$TMP" -maxdepth 3 -type f -name "$BIN_NAME" -perm -111 | head -n 1)
fi

if [ ! -x "${BIN:-}" ]; then
  err "Could not find the $BIN_NAME binary inside $TAR"
  exit 1
fi

# Sanity-check the binary actually runs on this host before we overwrite a
# working install with it.
if ! "$BIN" --version >/dev/null 2>&1; then
  err "Downloaded binary failed to execute (wrong architecture, or missing system libraries)"
  err "Downloaded: $URL"
  exit 1
fi

if [ -w "$DEST" ]; then
  install -m 0755 "$BIN" "$DEST/$BIN_NAME"
else
  info "Installing to $DEST requires sudo..."
  sudo install -m 0755 "$BIN" "$DEST/$BIN_NAME"
fi

ok "Installed vc to $DEST/$BIN_NAME"
maybe_add_path

if [ "$VERIFY" -eq 1 ]; then
  info "Running self-test..."
  "$DEST/$BIN_NAME" --version
  ok "Self-test complete"
fi

if [ "$QUIET" -eq 0 ]; then
  echo ""
  ok "Installation complete!"
  echo ""
  echo "Quick start:"
  echo "  vc --help          # Full command reference"
  echo "  vc tui             # Launch the cockpit TUI"
  echo "  vc web             # Serve the web dashboard"
  echo ""
  echo "Run 'vc --version' to confirm the install."
  echo ""
fi
