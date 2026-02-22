---
name: integration-hardening
description: Use for GitHub/GitLab/Linear wiring in rust-harness to remove stub-token paths, enforce env-driven credential resolution, and add regression tests for real-client behavior with safe fallbacks.
allowed_tools: [Bash, Read, Edit, Write]
references: [/Users/studio/rust-harness/crates/at-core/src/config.rs, /Users/studio/rust-harness/crates/at-bridge/src/http_api.rs, /Users/studio/rust-harness/crates/at-integrations/src/gitlab/mod.rs, /Users/studio/rust-harness/crates/at-integrations/src/linear/mod.rs]
---

# Integration Hardening

## Trigger
Use this skill when changing GitHub/GitLab/Linear endpoints, token handling, or import/review flows.

## Rules
1. Never hardcode `"stub-token"` or `"stub-key"` in API handlers.
2. Resolve token env names from `Config.integrations.*_token_env`.
3. Return actionable `503` with `env_var` when missing credentials.
4. Preserve runtime stub fallback only inside integration clients for explicit test tokens.
5. Keep route comments truthful (remove "(stubs)" when real wiring is present).

## Regression Checklist
1. Missing token path returns `503` + env var name.
2. Configured token path constructs real client.
3. Existing tests still pass in no-network environments.
4. API response shape remains backward-compatible unless explicitly versioned.

## Preferred Test Scope
- Unit tests for client fallback behavior.
- Handler tests for auth/config edge cases.
- Targeted `cargo check -p at-bridge -p at-integrations`.
