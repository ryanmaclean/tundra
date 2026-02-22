# Team C — Inference/Agent Integration Touchpoints

## Existing Touchpoints
| Area | File(s) | Notes |
|---|---|---|
| Provider profiles/failover | `crates/at-intelligence/src/api_profiles.rs` | includes `ProviderKind::Local` and failover registry |
| LLM providers | `crates/at-intelligence/src/llm.rs` | local OpenAI-compatible provider already present |
| Agent execution | `crates/at-agents/src/executor.rs` | orchestration entry point |
| PTY/session runtime | `crates/at-session/src/*` | terminal lifecycle integration point |

## claude-sdk-rs Mapping (Proposed)
1. `SessionManager` lifecycle tied to PTY session creation/teardown.
2. Stream tokens/events into terminal websocket channels.
3. Persist session IDs in task metadata for resume/replay.

## Local Inference Mapping (Proposed)
1. Keep `ProviderKind::Local` for OpenAI-compatible servers (`vllm.rs` endpoint).
2. Add profile templates for local model classes (small/medium/large).
3. Use existing failover chain for automatic fallback behavior.

## Candle Track
- Keep separate from networked local server integration.
- Use Candle for embedded/offline inference experiments in a dedicated crate.

## Next Steps
1. Add provider config schema for local endpoint/model aliases.
2. Add failover tests: cloud error -> local success -> cloud recovery.
3. Define terminal UX for “provider switched to local” events.
