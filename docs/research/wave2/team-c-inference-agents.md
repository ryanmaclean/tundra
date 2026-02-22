# Team C — Inference & Agents

## Scope
- `claude-sdk-rs` session runtime for terminal agents
- Local inference track (`vllm.rs` + `candle`)
- Provider failover path in `at-intelligence`

## Deliverables
- Integration surface map (`at-agents`, `at-session`, `at-intelligence`)
- Provider failover design (`Claude -> Local -> OpenRouter`)
- Local model routing profile proposal

## Sub-Agent Breakdown
- **C1 (Claude Runtime):** map `claude-sdk-rs` sessions/streaming onto PTY agent lifecycle.
- **C2 (Local Provider):** wire `vllm.rs` OpenAI-compatible endpoint into `ProviderKind::Local`.
- **C3 (Model Runtime):** evaluate `candle` for native/embedded local model execution path.

## Kickoff Findings
- `claude-sdk-rs` has the primitives needed for session/stateful agent execution.
- `at-intelligence` already contains a `Local` provider shape and failover profile registry, reducing integration risk.
- `vllm.rs` is suitable as a local inference server; `candle` is better as a lower-level runtime path.

## Immediate Tasks
1. Map `claude-sdk-rs` session lifecycle to PTY lifecycle:
   - session creation, resume, streaming, cancellation
2. Define local provider integration:
   - OpenAI-compatible endpoint contract for `vllm.rs`
   - model selection classes (`small/medium/large`)
3. Specify failover semantics:
   - error classes that trigger provider switch
   - cooldown and retry behavior
4. Capture observability requirements:
   - per-provider latency/cost/success metrics

## Acceptance Criteria
- End-to-end sequence diagram for one agent task
- Config schema additions for local provider profiles
- Test matrix for failover and fallback behavior

## Status
- In Progress
