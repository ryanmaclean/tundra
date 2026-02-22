---
name: crate-development
description: Use when adding features, fixing bugs, or refactoring code in any Auto-Tundra crate. Covers build, test, lint, and validation workflows.
allowed_tools: [Bash, Read, Edit, Write]
references: [/Users/studio/rust-harness/AGENTS.md, /Users/studio/rust-harness/CONTRIBUTING.md, /Users/studio/rust-harness/Cargo.toml]
---

# Crate Development

## Trigger
Use this skill when modifying Rust source code in any `crates/at-*` directory, `app/tauri`, or `app/leptos-ui`.

## Workspace Layout
```
crates/
  at-core/          # Types, config, context engine (foundation — minimal deps)
  at-agents/        # Agent roles, executor, registry, prompts
  at-intelligence/  # LLM providers, router, cost tracking, api_profiles
  at-harness/       # Rate limiting, circuit breaker, MCP, security
  at-daemon/        # Orchestrator, event bus, task pipeline
  at-bridge/        # HTTP/WS API (axum), the main handler file is http_api.rs
  at-session/       # PTY terminal pool
  at-integrations/  # GitHub, GitLab, Linear clients
  at-telemetry/     # Metrics, tracing, Datadog
  at-cli/           # CLI binary (clap)
  at-tui/           # TUI binary (ratatui)
app/
  tauri/            # Desktop shell
  leptos-ui/        # WASM frontend
```

## Non-Negotiable Workflow

### 1. Before editing: understand the dependency direction
```
at-core ← at-session, at-integrations, at-telemetry
       ← at-harness ← at-intelligence ← at-agents
                                        ← at-daemon ← at-bridge
                                                     ← at-cli, at-tui
```
Never add a dependency from a lower-layer crate to a higher one.

### 2. After every edit: validate
```bash
# Check the touched crate compiles
cargo check -p at-<crate>

# Run tests for the touched crate
cargo nextest run -p at-<crate>

# If touching at-core or at-harness, also check downstream:
cargo check -p at-agents -p at-intelligence -p at-bridge
```

### 3. Before claiming completion
```bash
# Lint
cargo clippy -p at-<crate> -- -D warnings

# Format
cargo fmt -- --check

# If touching API endpoints, verify with curl:
curl -s http://localhost:9090/api/<endpoint> | jq .
```

## Adding a New Feature

### New agent role
1. Add variant to `at-core/src/types.rs` → `AgentRole`
2. Implement `RoleConfig` in `at-agents/src/roles/<name>.rs`
3. Register in `at-agents/src/registry.rs`
4. Add tests

### New API endpoint
1. Add handler function in `at-bridge/src/http_api.rs`
2. Add route in `api_router_with_auth()`
3. Add request/response types
4. Add test in `crates/at-bridge/tests/http_api_test.rs`

### New CLI command
1. Add variant to `Commands` enum in `at-cli/src/main.rs`
2. Create handler in `at-cli/src/commands/<name>.rs`
3. Export from `at-cli/src/commands/mod.rs`
4. Wire in `main()` match arm
5. Add test

## Common Pitfalls
- `http_api.rs` is 4000+ lines — search carefully before adding duplicate routes.
- `at-core` is depended on by everything — breaking changes cascade.
- Always use `#[cfg(test)]` for test modules, not standalone test files in `src/`.
- Integration tests that hit the network should be gated behind `#[ignore]` or env vars.
- The Leptos frontend is WASM — `#[cfg(target_arch = "wasm32")]` guards apply.
