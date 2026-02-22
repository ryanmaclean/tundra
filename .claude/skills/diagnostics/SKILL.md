---
name: diagnostics
description: Use when troubleshooting build failures, test failures, daemon connectivity issues, or integration errors in Auto-Tundra.
allowed_tools: [Bash, Read]
references: [/Users/studio/rust-harness/docs/PROJECT_HANDBOOK.md, /Users/studio/rust-harness/AGENTS.md]
---

# Diagnostics

## Trigger
Use this skill when something is broken: build errors, test failures, daemon won't start, API returns errors, or integrations fail.

## Diagnostic Ladder (follow in order)

### Step 1: Environment check
```bash
# All-in-one health check
at doctor -p /Users/studio/rust-harness -j

# Manual checks
rustc --version                    # Must be 1.91+
env | grep -E 'API_KEY|TOKEN'      # Check credentials
curl -sf http://localhost:9090/api/status | jq .  # Daemon alive?
```

### Step 2: Build diagnostics
```bash
# Full workspace check (fast, no codegen)
cargo check 2>&1 | head -50

# Single crate
cargo check -p at-<crate> 2>&1 | head -50

# If linker errors on macOS:
xcode-select --install
```

### Step 3: Test diagnostics
```bash
# Run with output visible
cargo nextest run -p at-<crate> --no-capture 2>&1 | tail -80

# Run a single test
cargo nextest run -p at-<crate> -E 'test(test_name)' --no-capture

# Run with debug logging
RUST_LOG=debug cargo nextest run -p at-<crate> --no-capture
```

### Step 4: Daemon diagnostics
```bash
# Start daemon with debug logging
RUST_LOG=debug cargo run --bin at-daemon 2>&1 | tee /tmp/daemon.log

# Check if port is in use
lsof -i :9090

# Kill stale daemon
pkill -f at-daemon
```

### Step 5: Integration diagnostics
```bash
# GitLab
curl -sf http://localhost:9090/api/gitlab/issues 2>&1 | jq .
# Expected 503 with {"error":"...","env_var":"GITLAB_TOKEN"} if token missing

# Linear
curl -sf http://localhost:9090/api/linear/issues?team_id=X 2>&1 | jq .

# GitHub
curl -sf http://localhost:9090/api/github/issues 2>&1 | jq .
```

### Step 6: Dependency/security diagnostics
```bash
# Check for vulnerable dependencies
cargo deny check 2>&1 | head -30

# Check for outdated dependencies
cargo outdated -R 2>&1 | head -30
```

## Common Failure Patterns

| Symptom | Likely Cause | Fix |
|---------|-------------|-----|
| `Connection refused :9090` | Daemon not running | `cargo run --bin at-daemon` |
| `503 + env_var` in response | Missing credential | Export the named env var |
| `linking with cc failed` | Missing Xcode tools | `xcode-select --install` |
| `cannot find crate` | Missing workspace member | Check `Cargo.toml` members |
| Test timeout | Network-dependent test | Check `#[ignore]` gate or mock |
| `port already in use` | Stale process | `lsof -i :9090` then `kill` |

## Rules
1. Always start at Step 1 and work down — don't jump to Step 5.
2. Capture output with `2>&1 | head -N` to avoid flooding context.
3. When reporting errors, include the exact error message and the command that produced it.
4. Never blindly `cargo clean` — it wastes 5-10 min of rebuild time. Only use if genuinely corrupt.
