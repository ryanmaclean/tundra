# Cross-project lessons — 2026-09

Tundra is an intentionally broad demo/testbed. Treat it as a **mine of solved orchestration/telemetry problems**, not the target architecture.

## Reuse as reference

- provider routing and failover
- context/token budgeting
- telemetry/Datadog patterns
- approval/security gates
- event bus patterns
- PTY/session lifecycle

## Compare with lower-bound work

BOP/smolFire should deliberately avoid copying Tundra's database/API/UI/control-plane breadth unless measurements prove it is required.

Use Tundra to answer: what functionality disappears when filesystem-native state/history and tiny runtimes are available?

## Agent assignment

Copilot primary; `@codex` fallback.
