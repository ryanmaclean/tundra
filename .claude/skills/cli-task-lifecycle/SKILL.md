---
name: cli-task-lifecycle
description: Use the `at` CLI to create, execute, monitor, and complete tasks on the Auto-Tundra daemon. Covers the full bead lifecycle from sling to done.
allowed_tools: [Bash, Read]
references: [/Users/studio/rust-harness/docs/PROJECT_HANDBOOK.md]
---

# CLI Task Lifecycle

## Trigger
Use this skill when you need to create a task, execute it, check status, or complete work via the Auto-Tundra CLI.

## Prerequisites
1. Daemon must be running: `cargo run --bin at-daemon` (or check with `at doctor -p .`).
2. At least one API key must be set: `ANTHROPIC_API_KEY`, `OPENROUTER_API_KEY`, or `OPENAI_API_KEY`.

## Quick Reference

### Check health first
```bash
at doctor -p /Users/studio/rust-harness -j
at status
```

### Create a task (bead)
```bash
# Simple
at sling "Fix login bug"

# With priority lane
at sling "Security patch" --lane critical
```

### Create a skill-aware task
```bash
# Create + execute
at run \
  -t "Wire GitLab env handling" \
  -s integration-hardening \
  -p /Users/studio/rust-harness

# Create without executing
at run \
  -t "Wire GitLab env handling" \
  -s integration-hardening \
  -p /Users/studio/rust-harness \
  --no-execute
```

### Preview a task without network calls
```bash
at run \
  -t "Plan integration wave" \
  -s wave-execution \
  -p /Users/studio/rust-harness \
  --dry-run --emit-prompt -j
```

### Role-scoped execution
```bash
at agent run \
  -r qa-reviewer \
  -t "Audit auth edge cases" \
  -s integration-hardening \
  -p /Users/studio/rust-harness \
  -m sonnet -n 2 -j
```

### Monitor and complete
```bash
at status                    # Check all beads
at hook <bead-id>            # Start processing
at done <bead-id>            # Mark complete
at nudge <agent-id>          # Restart stuck agent (use sparingly)
```

## Output Parsing Rules

1. Always use `-j` flag for machine-readable JSON output.
2. Use `-o /tmp/result.json` to write output to a file for later reading.
3. Non-zero exit codes indicate errors; stderr will contain the error message.
4. When the daemon is unreachable, `at doctor` will report it cleanly.

## Common Pitfalls
- Do NOT call `at run` or `at agent run` without `-p` (project path) — it defaults to `.` which may be wrong.
- Do NOT execute tasks without first running `at doctor` to verify connectivity.
- Role names for `at agent run -r` must map to valid task categories (e.g., `qa-reviewer` → QA, `builder` → Build).
- If the daemon returns non-JSON (e.g., HTML error page), the CLI handles graceful fallback — but check exit code.
