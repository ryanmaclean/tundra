---
name: wave-execution
description: Use for multi-lane implementation waves in rust-harness when coordinating sub-agents, sequencing risk, and enforcing compile/test gates before reporting completion.
allowed_tools: [Bash, Read, Edit, Write]
references: [/Users/studio/rust-harness/AGENTS.md, /Users/studio/rust-harness/docs/research/wave2/WAVE2_EXECUTION_BOARD.md]
---

# Wave Execution

## Trigger
Use this skill when the user asks to "continue the next wave", "assign agents", or "think sequentially".

## Non-Negotiable Guardrails
1. Split work into 2-4 named lanes with explicit deliverables.
2. Do read/discovery in parallel, edits sequentially.
3. Never claim completion before running targeted checks for touched crates/files.
4. In dirty worktrees, only touch files required by the lane.
5. If a lane surfaces unrelated compile errors, fix only minimal blockers needed to validate lane output.

## Standard Lane Template
- `Lane`: short name
- `Owner`: logical team (Core/Bridge/Intelligence/UI)
- `Scope`: exact files + behavior change
- `Validation`: exact commands
- `Rollback`: fallback behavior preserved?

## Required Validation Pattern
1. `cargo check` for touched crates.
2. Focused tests for touched modules.
3. One integration smoke command/script if available.

## Reporting Format
Always report:
- Files changed
- Behavior changed
- Validation commands and result
- Remaining risks / next lane candidates
