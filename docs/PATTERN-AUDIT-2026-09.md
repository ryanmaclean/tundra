# Pattern audit — Tundra orchestration/telemetry surfaces (2026-09)

Fixes #100. Companion to `docs/CROSS-PROJECT-LESSONS-2026-09.md`.

Scope: classify six Tundra subsystems as **reuse directly**, **adapt into
Moth-BOP**, or **unnecessary if filesystem-native state/history exists**, so
BOP/smolFire can pull solved problems without importing Tundra's demo-scale
breadth (database/API/UI/control-plane).

## 1. Provider failover/routing — `crates/at-intelligence/src/api_profiles.rs` (1968 lines)

`ApiProfile` + `ProviderKind` (Anthropic/OpenRouter/OpenAI/Local/Custom) with
per-profile `CircuitBreaker` (from `at-harness::circuit_breaker`) and
token-bucket `RateLimiter`. Failover picks the next enabled profile by
`priority` when a circuit opens or a rate limit is hit; cost/usage is tracked
per profile.

**Classification: adapt into Moth-BOP.** The circuit-breaker + rate-limiter
primitives (`at-harness::circuit_breaker`, `at-harness::rate_limiter`) are
provider-agnostic and small enough to vendor without the rest of
`at-intelligence`. The `ApiProfile` struct itself is over-general for a
single-agent runtime (5 provider kinds, custom headers, per-profile cost
tracking) — BOP needs "try provider A, fall back to B on 429/5xx," not a
full profile-management API. Port the breaker+limiter pair; reimplement
failover as a 20-30 line function over a static provider list.

## 2. Token/context budgeting — `crates/at-core/src/context_steering.rs` (1520 lines)

`DisclosureLevel` (Identity/Project/Task/Deep, 200/2K/4K/16K token budgets)
drives progressive context assembly (`ContextSteerer.assemble()`), plus
Cortex-mem-style `MemoryWeight` (L0/L1/L2 tiers, 7-day temporal decay,
frequency bonus, auto-promote/demote).

**Classification: unnecessary if filesystem-native state/history exists.**
The 4-level disclosure model exists to answer "what does an agent need to
know before touching this task," but it's solving for a system with no
persistent working directory: memories decay and get promoted because there
is no cheap "just read the file that's already there." BOP/smolFire agents
operate directly against a git worktree with CLAUDE.md/AGENTS.md always in
context — L0/L1 collapse to "read the file," and L2/L3 collapse to "read the
diff/spec that's already on disk." The temporal-decay/promotion machinery is
solving a problem (context that isn't there unless fetched) that a
filesystem-native runtime doesn't have. Recommend: do not port; reference
this file if BOP ever needs a *long-running, memoryless* agent pool (e.g. a
hosted multi-tenant service) where re-fetching context from disk isn't an
option.

## 3. Datadog telemetry — `crates/at-telemetry/` (logging, metrics, middleware, tracing_setup)

Unified `tracing`-based logging (human + JSON), Prometheus-compatible
counters/gauges/histograms, Axum middleware for request metrics + trace-id
injection, OTel-compatible trace/span ID generation.

**Classification: reuse directly.** This is a thin, dependency-light
wrapper around `tracing` + `tracing-subscriber` with no coupling to
Tundra's DB/API layer. The metrics and trace-id-correlation pieces are
exactly the shape BOP/smolFire need for fleet observability (Datadog agent
scrape + IRC/log correlation) and match the project's existing MIT/BSD/
Apache-2.0 licensing constraint (tracing ecosystem is MIT). Vendor
`crates/at-telemetry` close to as-is; drop the Axum middleware module if
BOP has no HTTP surface.

## 4. Approval/security gates — `crates/at-agents/src/approval.rs` (684 lines)

`ApprovalPolicy` (AutoApprove/RequireApproval/Deny) gates tool invocations
per `AgentRole`; `PendingApproval` requests are recorded through
`at-harness::audit_chain::AuditChain` for a tamper-evident audit trail.

**Classification: adapt into Moth-BOP.** The three-state policy enum and
the "every gated call gets an audit-chain entry" pattern are directly
applicable to any agent that can run Bash/network actions — this is the
same shape as this very session's own permission-category rules
(prohibited / explicit-permission / regular). Port the policy enum and the
`AuditChain` append-only log; drop the `HashMap<AgentRole, ApprovalPolicy>`
role-matrix in favor of BOP's flatter role set (probably 3-5 roles vs
Tundra's 35-variant `AgentRole`).

## 5. Event bus — `crates/at-bridge/src/event_bus.rs` (636 lines)

Broadcast pub/sub over `flume` channels; `Arc<BridgeMessage>` payloads to
avoid deep-cloning `Vec<Bead>`/`Vec<Agent>` per subscriber; supports
filtered/per-agent subscriptions via boxed predicates.

**Classification: reuse directly** (already flume-based per repo
convention — no new dependency for BOP). The subscribe/publish/filter
surface is generic pub/sub with no coupling to Tundra's task/bead model.
Vendor as-is; the `BridgeMessage` payload type is the only Tundra-specific
piece and is trivially swappable for BOP's own event enum.

## 6. PTY/session lifecycle — `crates/at-session/src/pty_pool.rs` (1182 lines)

`PtyPool` enforces a capacity limit (`max_ptys`) across concurrent PTY
sessions; each `PtyHandle` gets dedicated reader/writer threads and
`flume` I/O channels; `Drop` idempotently kills the process and releases
the pool slot (no leaked children or slots).

**Classification: reuse directly.** Capacity-bounded PTY pooling with
leak-proof `Drop` semantics is exactly the primitive a terminal-driving
agent runtime needs, and it has no dependency on Tundra's broader
control-plane (DB, API, UI). Vendor `pty_pool.rs` with its existing
`portable-pty`/`flume` deps; this is one of the cleanest single-file ports
in the audit (self-contained, already documented with a lifecycle
diagram in its module doc comment).

## Summary table

| Area | Classification | Port unit |
|---|---|---|
| Provider failover/routing | Adapt | `at-harness::circuit_breaker` + `rate_limiter` only |
| Token/context budgeting | Unnecessary (filesystem-native) | none — reference only |
| Datadog telemetry | Reuse directly | `crates/at-telemetry/` |
| Approval/security gates | Adapt | policy enum + `AuditChain` |
| Event bus | Reuse directly | `crates/at-bridge/src/event_bus.rs` |
| PTY/session lifecycle | Reuse directly | `crates/at-session/src/pty_pool.rs` |

## Non-goals

This audit does not touch Tundra's HTTP API surface, SQLite persistence
layer, or Leptos frontend — those are the "demo-scale breadth" this issue
explicitly asks to keep out of the lower-bound runtime.
