# 2026-09-22 review: unverified findings (high/medium)

Source: multi-agent review of 2026-09-22 (12 finders, 92 unique findings). This file lists the 52 high and medium findings that no verifier confirmed or refuted. They are leads, not confirmed bugs: verify each one before acting on it. Confirmed findings are tracked separately; the fixes that landed are summarised in CHANGELOG.md under "Unreleased — 2026-09-22 review wave". Some items below may already be resolved by that wave.

Line numbers refer to `main` at the time of the review and may have drifted.

| # | Severity | Category | Finding | Location |
|---|---|---|---|---|
| 1 | high | security | Vulnerable rustls / rustls-webpki / h2 locked in Cargo.lock (cargo deny advisories FAILS) | `Cargo.lock:6167` |
| 2 | high | correctness | Model-ID rename is half-migrated: the default agent model now prices at $0, and the new Opus/Haiku prices are wrong | `crates/at-intelligence/src/cost_tracker.rs:43` |
| 3 | medium | error-handling | WebSocket reconnect backoff never grows: every close retries after 1s, forever | `app/leptos-ui/src/events.rs:216` |
| 4 | medium | performance | Timer closures outlive their pages: beads auto-refresh keeps polling and writing global state after navigating away | `app/leptos-ui/src/pages/beads.rs:467` |
| 5 | medium | licensing | at-tauri default feature pulls MPL-2.0 symphonia through rodio's default features | `app/tauri/Cargo.toml:29` |
| 6 | medium | security | Role tool allow-lists, turn limits, model preferences and the ToolApprovalSystem are never enforced when agents run | `crates/at-agents/src/executor.rs:427` |
| 7 | medium | error-handling | TaskRunner treats a phase timeout or the first output chunk as phase completion, so silent or slow agents end in 'Task completed successfully' | `crates/at-agents/src/task_runner.rs:256` |
| 8 | medium | security | New rig-core tools read_file/shell_exec are unsandboxed, bypass the approval system and allow_shell_exec, and block the async runtime with no timeout | `crates/at-agents/src/tools.rs:51` |
| 9 | medium | security | New public rig-core `shell_exec` tool runs arbitrary `sh -c` with no gate, is unused, and pulls rig-core into daemon/tauri | `crates/at-agents/src/tools.rs:51` |
| 10 | medium | api-design | at-api-types defines ApiStack but the bridge has no /api/stacks route; UI silently shows demo stacks | `crates/at-api-types/src/lib.rs:251` |
| 11 | medium | security | AuthLayer accepts an empty configured key, so auth is bypassed with an empty X-API-Key header | `crates/at-bridge/src/auth.rs:103` |
| 12 | medium | concurrency | MCP SSE sessions are force-closed after exactly 1 hour, and abandoned sessions stay in memory for that hour | `crates/at-bridge/src/http_api/mcp_sse.rs:99` |
| 13 | medium | api-design | MCP transport: tools run before the session is checked, and the shipped Claude Code config cannot authenticate | `crates/at-bridge/src/http_api/mcp_sse.rs:127` |
| 14 | medium | correctness | MCP SSE tools/call skips bead lifecycle checks, input sanitization and event publishing that the REST API enforces | `crates/at-bridge/src/http_api/mcp_sse.rs:507` |
| 15 | medium | correctness | Projects changed from Vec to HashMap, so list order and pagination are no longer stable and deleting the active project activates an arbitrary one | `crates/at-bridge/src/http_api/projects.rs:36` |
| 16 | medium | correctness | Vec to HashMap conversion makes project and attachment listing and pagination order random | `crates/at-bridge/src/http_api/projects.rs:36` |
| 17 | medium | error-handling | Worktree merge and resolve endpoints ignore git exit codes and report success; delete matches by substring and runs `git worktree remove --force` | `crates/at-bridge/src/http_api/worktrees.rs:212` |
| 18 | medium | performance | POST /api/ideation/generate holds the ideation_engine write lock across the LLM network call | `crates/at-bridge/src/intelligence_api.rs:532` |
| 19 | medium | api-design | GET /api/changelog?source=tasks mutates state: every call appends a duplicate entry and returns ever-growing markdown | `crates/at-bridge/src/intelligence_api.rs:1246` |
| 20 | medium | concurrency | Blocking flume::Sender::send on the bounded (256) PTY stdin channel inside an async task can wedge a tokio worker and the terminal | `crates/at-bridge/src/terminal_ws.rs:1159` |
| 21 | medium | concurrency | Concurrent or overlapping WS connections to one terminal split PTY output, and a stale disconnect kills the PTY under a live client | `crates/at-bridge/src/terminal_ws.rs:1279` |
| 22 | medium | correctness | `at run`/`at exec`/`at agent run` panic on task titles with non-ASCII characters longer than 120 bytes | `crates/at-cli/src/commands/run_task.rs:112` |
| 23 | medium | error-handling | A single unparseable row panics inside the tokio-rusqlite thread and permanently closes the cache DB | `crates/at-core/src/cache.rs:23` |
| 24 | medium | security | The daemon.key fallback can produce an empty API key (auth bypass), and the key file is created world-readable before chmod | `crates/at-core/src/config.rs:1078` |
| 25 | medium | security | EncryptionKey is never zeroized: #[zeroize(skip)] is on its only field | `crates/at-core/src/crypto.rs:107` |
| 26 | medium | correctness | Settings API and daemon use different config files, so security and daemon settings changed in the UI never take effect | `crates/at-core/src/settings.rs:18` |
| 27 | medium | licensing | Four unused Datadog crates in at-daemon pull in MPL-2.0 attohttpc and a vulnerable reqwest 0.11 / h2 0.3 stack | `crates/at-daemon/Cargo.toml:33` |
| 28 | medium | correctness | Daemon KPI loop overwrites the live ApiState.kpi with a snapshot from a CacheDb nothing writes to (MCP get_kpi reports zeros) | `crates/at-daemon/src/daemon.rs:280` |
| 29 | medium | correctness | TaskOrchestrator merges to main and marks the task Complete after QA fails or an agent phase fails or times out | `crates/at-daemon/src/orchestrator.rs:287` |
| 30 | medium | correctness | Orchestrator slices agent output at byte 1000 and panics on multi-byte UTF-8 | `crates/at-daemon/src/orchestrator.rs:321` |
| 31 | medium | concurrency | Circuit breaker HalfOpen state admits unlimited concurrent calls | `crates/at-harness/src/circuit_breaker.rs:199` |
| 32 | medium | error-handling | RateLimitConfig with a zero rate panics on the first rejected check (Duration::from_secs_f64(inf)) | `crates/at-harness/src/rate_limiter.rs:146` |
| 33 | medium | robustness | No HTTP timeouts on any GitHub, GitLab or Linear client, so a stalled upstream hangs request handlers indefinitely | `crates/at-integrations/src/gitlab/mod.rs:132` |
| 34 | medium | error-handling | Stub fallback turns placeholder tokens into made-up successes, including a fake 'created' MR | `crates/at-integrations/src/gitlab/mod.rs:193` |
| 35 | medium | correctness | Linear list_issues is hard-capped at 50 with no pagination, so bridge offset/limit can't reach past item 50 | `crates/at-integrations/src/linear/mod.rs:246` |
| 36 | medium | correctness | Linear sync push overwrites remote issue titles with their own IDs and descriptions with a fixed string | `crates/at-integrations/src/linear/sync.rs:142` |
| 37 | medium | correctness | Cost lookup needs an exact model-name match and silently returns $0; the renamed pricing rows also keep the old, wrong prices | `crates/at-intelligence/src/cost_tracker.rs:347` |
| 38 | medium | error-handling | Ideation's text fallback turns truncated or non-JSON LLM output into junk ideas like "{" and "\"ideas\": [" | `crates/at-intelligence/src/ideation.rs:240` |
| 39 | medium | error-handling | The cloud provider HTTP clients have no timeout, so a stalled Anthropic or OpenAI connection hangs the caller forever | `crates/at-intelligence/src/llm.rs:183` |
| 40 | medium | correctness | Anthropic prompt caching was added, but cache token counts are never parsed, so token and cost accounting undercount cached requests | `crates/at-intelligence/src/llm.rs:270` |
| 41 | medium | correctness | The rewritten GraphMemory decay ignores the public decay_rate, deletes entries about 5x sooner, and overwrites updated_at on every entry | `crates/at-intelligence/src/memory.rs:618` |
| 42 | medium | correctness | ModelRouter picks a model from another vendor and sends it to the caller's single provider; routing to Opus 4.7 also fails because temperature is always sent | `crates/at-intelligence/src/model_router.rs:191` |
| 43 | medium | build | clippy -D warnings fails: unused `error` imports in at-session and at-agents, plus unnecessary_map_or in at-bridge | `crates/at-session/src/pty_pool.rs:69` |
| 44 | medium | error-handling | PtyHandle has no Drop: dropping a handle leaks its pool slot and loses the only way to kill or reap the child (PtyPoolSpawner does exactly this) | `crates/at-session/src/pty_pool.rs:218` |
| 45 | medium | performance | PtyHandle::kill blocks the calling thread up to ~200ms (portable-pty SIGHUP grace loop), and is called on tokio workers while holding a write lock | `crates/at-session/src/pty_pool.rs:284` |
| 46 | medium | concurrency | PtyPool capacity check is check-then-act across fork/exec; concurrent spawns exceed max_ptys | `crates/at-session/src/pty_pool.rs:643` |
| 47 | medium | performance | metrics_middleware labels counters with the raw request path, giving unbounded cardinality and memory growth | `crates/at-telemetry/src/middleware.rs:13` |
| 48 | medium | performance | Uncommitted bootstrap change runs fetches one after another instead of in parallel | `crates/at-tui/src/api_client.rs:141` |
| 49 | medium | performance | TUI fetch_all is now slower than HEAD: bootstrap runs before the fan-out, and the fallback path runs serially | `crates/at-tui/src/api_client.rs:141` |
| 50 | medium | error-handling | Fetch errors wipe displayed data, and the poll rate is far above the bridge's rate limits | `crates/at-tui/src/api_client.rs:166` |
| 51 | medium | api-design | Headless create_bead/action/refresh do nothing but acknowledge {"event":"ok"} | `crates/at-tui/src/command.rs:196` |
| 52 | medium | licensing | deny.toml license allow-list is looser than project policy, so `licenses ok` does not enforce the rule | `deny.toml:36` |
