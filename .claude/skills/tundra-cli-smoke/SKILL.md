---
name: tundra-cli-smoke
description: Use when developing or refactoring at-cli. Runs deterministic smoke checks for run/agent/skill/doctor commands, including --dry-run and --out artifact paths.
allowed_tools: [Bash, Read]
references: [/Users/studio/rust-harness/AGENTS.md, /Users/studio/rust-harness/crates/at-cli/src/main.rs, /Users/studio/rust-harness/crates/at-cli/src/commands/run_task.rs, /Users/studio/rust-harness/crates/at-cli/src/commands/doctor.rs]
---

# Tundra CLI Smoke

## Trigger
Use this skill whenever CLI behavior changes in `/Users/studio/rust-harness/crates/at-cli/`, or when validating API contract drift from the CLI side.

## Required Checks
Run these in order:

1. Build and unit tests
```bash
cargo check -p at-cli
cargo test -p at-cli
```

2. Skill discovery/validation
```bash
cargo run -p at-cli -- skill list -p /Users/studio/rust-harness -j
cargo run -p at-cli -- skill validate -p /Users/studio/rust-harness -j
```

3. Dry-run payload checks with artifacts
```bash
OUT1="$(mktemp -t at-run.XXXXXX).json"
OUT2="$(mktemp -t at-agent.XXXXXX).json"

cargo run -p at-cli -- run -t "cli smoke run" --dry-run --emit-prompt -j --out "$OUT1"
cargo run -p at-cli -- agent run -r qa-reviewer -t "cli smoke agent" --dry-run -j --out "$OUT2"
```

4. Doctor checks
```bash
OUT3="$(mktemp -t at-doctor.XXXXXX).json"
cargo run -p at-cli -- doctor -p /Users/studio/rust-harness -j --out "$OUT3"
```

## Expected Invariants
- `run` and `agent run` dry-runs must emit valid JSON with `mode: dry-run`.
- `--out` must write a pretty JSON artifact file.
- `skill validate` should return `ok: true` for clean repo skill definitions.
- `doctor` JSON must include `api`, `project_exists`, `skill_count`, `env`, and `failures`.

## Reporting Format
Always report:
- files changed
- commands executed
- pass/fail by command
- any artifact path used for validation
- any regression found and the likely root cause
