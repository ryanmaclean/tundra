# Team C — Session/PTY Lifecycle Mapping (C1)

## Objective
Map `claude-sdk-rs`-style session lifecycle semantics onto current `at-agents` + `at-session` PTY runtime.

## Current Building Blocks
- `crates/at-agents/src/claude_session.rs`
  - `ClaudeSessionManager` (session create/list/remove/send)
- `crates/at-session/src/*`
  - PTY pool/session primitives
- `crates/at-bridge/src/terminal_ws.rs`
  - websocket stream transport to UI

## Proposed Lifecycle
1. **Create Task Session**
   - Create Claude session ID + PTY ID
   - Persist mapping in task metadata (`task_id -> session_id, pty_id`)
2. **Interactive Run**
   - Stream model outputs into PTY websocket channel
   - Record token usage + provider metadata per turn
3. **Suspend/Resume**
   - Suspend: keep session, stop PTY streaming
   - Resume: reattach PTY stream to existing session
4. **Close**
   - Flush terminal buffer
   - remove session/PTY mapping

## Integration Contract (Draft)
- `AgentRuntime::start(task_id, profile)`
- `AgentRuntime::send(task_id, message)`
- `AgentRuntime::resume(task_id)`
- `AgentRuntime::stop(task_id)`

## Risks
- PTY and session state drift on network interruptions
- duplicate stream emission on reconnect
- missing idempotency for resume

## Validation
- unit tests for lifecycle transitions
- integration test for disconnect/reconnect replay
- telemetry events for create/resume/close/fail
