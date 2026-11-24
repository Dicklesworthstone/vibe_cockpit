# Vibe Cockpit
# https://github.com/Dicklesworthstone/vibe_cockpit

BINARY_NAME := vc
VERSION := $(shell git describe --tags --always --dirty 2>/dev/null || echo "dev")

CARGO := cargo

# Release target dir for the current host.
RELEASE_BIN := target/release/$(BINARY_NAME)

.PHONY: all build release install install-user uninstall run test test-e2e \
        lint fmt fmt-check check clippy clean deps siblings version help

all: build

## Build (debug) for the current platform
build:
	$(CARGO) build --workspace

## Build (release) for the current platform
release:
	$(CARGO) build --release --bin $(BINARY_NAME)

## Install to /usr/local/bin
install: release
	install -m 0755 $(RELEASE_BIN) /usr/local/bin/$(BINARY_NAME)
	@echo "Installed $(BINARY_NAME) to /usr/local/bin/"

## Install to ~/.local/bin
install-user: release
	@mkdir -p $(HOME)/.local/bin
	install -m 0755 $(RELEASE_BIN) $(HOME)/.local/bin/$(BINARY_NAME)
	@echo "Installed $(BINARY_NAME) to ~/.local/bin/"
	@echo "Make sure ~/.local/bin is in your PATH"

## Uninstall
uninstall:
	rm -f /usr/local/bin/$(BINARY_NAME)
	rm -f $(HOME)/.local/bin/$(BINARY_NAME)
	@echo "Uninstalled $(BINARY_NAME)"

## Run the debug binary (pass args with ARGS="...")
run:
	$(CARGO) run --bin $(BINARY_NAME) -- $(ARGS)

## Run the test suite
test:
	$(CARGO) test --workspace

## Run the end-to-end tests only
test-e2e:
	$(CARGO) test --test tui_rendering --test migration_integrity

## Type-check everything CI checks
check:
	$(CARGO) check --workspace --all-targets

## Run clippy the way CI runs it (warnings are errors)
clippy:
	$(CARGO) clippy --workspace --all-targets -- -D warnings

## Run every CI gate locally (fmt + clippy + check + test)
lint: fmt-check clippy check

## Format the code
fmt:
	$(CARGO) fmt --all

## Verify formatting without rewriting (this is the CI gate)
fmt-check:
	$(CARGO) fmt --all -- --check

## Clean build artifacts
clean:
	$(CARGO) clean

## Fetch dependencies
deps:
	$(CARGO) fetch

## Kept for old scripts: no sibling checkouts are needed any more
siblings:
	@echo "vc resolves ftui (frankentui) and fsqlite (frankensqlite) from crates.io;"
	@echo "no sibling checkouts are needed to build it."

## Show version
version:
	@echo $(VERSION)

## Show help
help:
	@echo "Vibe Cockpit"
	@echo ""
	@echo "Usage: make <target>"
	@echo ""
	@echo "Targets:"
	@grep -E '^## ' $(MAKEFILE_LIST) | sed 's/## /  /'
