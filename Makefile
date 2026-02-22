# auto-tundra Makefile
# Shortcuts for common development and CI tasks

CARGO := /Users/studio/.cargo/bin/cargo

.PHONY: build test test-ci test-release check clippy fmt doc clean \
        dd-upload nextest-install wasm deny ast-grep security

# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------

build:
	$(CARGO) build

build-release:
	$(CARGO) build --release

wasm:
	$(CARGO) build --target=wasm32-unknown-unknown -p at-leptos-ui

# ---------------------------------------------------------------------------
# Test — local dev (fast, no JUnit)
# ---------------------------------------------------------------------------

test:
	$(CARGO) nextest run
	$(CARGO) test --doc

# ---------------------------------------------------------------------------
# Test — CI (JUnit XML output for Datadog)
# ---------------------------------------------------------------------------

test-ci:
	$(CARGO) nextest run --profile ci
	$(CARGO) test --doc

# ---------------------------------------------------------------------------
# Test — release validation (strict, retry flaky 3x)
# ---------------------------------------------------------------------------

test-release:
	$(CARGO) nextest run --profile release
	$(CARGO) test --doc

# ---------------------------------------------------------------------------
# Datadog — upload JUnit XML test results
#
# Required env vars:
#   DATADOG_API_KEY  — your Datadog API key
#   DD_ENV           — environment tag (ci, staging, local)
#   DATADOG_SITE     — Datadog site (e.g., datadoghq.com, us5.datadoghq.com)
# ---------------------------------------------------------------------------

dd-upload:
	@test -n "$(DATADOG_API_KEY)" || (echo "ERROR: DATADOG_API_KEY not set" && exit 1)
	datadog-ci junit upload \
		--service auto-tundra \
		--env $(or $(DD_ENV),ci) \
		target/nextest/ci/ci-junit.xml

# ---------------------------------------------------------------------------
# Full CI pipeline: test + upload to Datadog
# ---------------------------------------------------------------------------

ci: test-ci dd-upload

# ---------------------------------------------------------------------------
# Lint & format
# ---------------------------------------------------------------------------

check:
	$(CARGO) check --all-targets

clippy:
	$(CARGO) clippy --all-targets -- -D warnings

fmt:
	$(CARGO) fmt --all

fmt-check:
	$(CARGO) fmt --all -- --check

# ---------------------------------------------------------------------------
# Docs
# ---------------------------------------------------------------------------

doc:
	$(CARGO) doc --no-deps --all-features

# ---------------------------------------------------------------------------
# Clean
# ---------------------------------------------------------------------------

clean:
	$(CARGO) clean

# ---------------------------------------------------------------------------
# Setup — install tooling
# ---------------------------------------------------------------------------

nextest-install:
	$(CARGO) install cargo-nextest --locked

# ---------------------------------------------------------------------------
# Security scanning
# ---------------------------------------------------------------------------

deny:
	$(CARGO) deny check

ast-grep:
	ast-grep scan --config .ast-grep.yml --exclude '**/test*.rs' --exclude '**/*_test.rs' crates/

security: deny ast-grep

# ---------------------------------------------------------------------------
# Setup — install tooling (continued)
# ---------------------------------------------------------------------------

dd-ci-install:
	npm install -g @datadog/datadog-ci

deny-install:
	$(CARGO) install cargo-deny --locked

ast-grep-install:
	cargo install ast-grep --locked
