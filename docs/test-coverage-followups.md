# Test-coverage follow-ups

Architectural questions and tracked technical debt surfaced by the
test-coverage-improvement work landed on `claude/analyze-test-coverage-9RAkC`
(tip `e9e3074`). Each item is scoped out of the test-coverage PR and listed
here so reviewers and follow-up authors do not have to reconstruct the
reasoning.

## Open architectural questions

### 1. at-intelligence: fallback wrapper can't gate on error class

`ResilientRegistry::call_with_failover` at
`crates/at-intelligence/src/api_profiles.rs:615-677` accepts a closure
`F: FnMut(&ApiProfile) -> Fut` where `Fut: Future<Output = Result<T, E>>`
and `E: Display`. Inside the loop, every inner failure is collapsed into
`CircuitBreakerError::Inner(String)` (see the match arm at
`crates/at-intelligence/src/api_profiles.rs:666`) before the fallback
decision is made. Because the wrapper only ever sees a stringified error,
there is no way for a caller to short-circuit on auth-class failures
(401/403, which usually mean "no other provider can recover this either")
while still falling back on rate-limit or 5xx responses. The pinning test
`fallback_primary_unauthorized_currently_fans_out` at
`crates/at-intelligence/src/api_profiles.rs:1408` documents the current
"fans out on 401" behavior. A reasonable fix is to have the closure return
a `RetryDecision { Retry, GiveUp }` enum, or `Result<T, ResilientCallError>`
directly, so the wrapper can distinguish recoverable from terminal errors.
Out of scope for the test-coverage PR.

### 2. at-session: production glue methods needed `pub(crate) from_parts` for unit testing

`AgentSession` and `PtyHandle` had no constructor that did not spawn a real
PTY, which made fine-grained unit testing impossible. This PR resolved it
narrowly with `#[cfg(test)] pub(crate) fn from_parts(...)` on both:
`crates/at-session/src/session.rs:107` and
`crates/at-session/src/pty_pool.rs:257`. The same shape — a public type
that wraps an OS resource acquired in its only constructor — is likely
repeated in `at-bridge::terminal` and the orchestrator workers under
`at-daemon`. Flag for a systematic review when adding more
concurrent-stress tests, so each crate gets a uniform test-only
construction path rather than ad-hoc per-test workarounds.

### 3. at-core::rlm: not an algorithmic decomposer

The original test-coverage analysis assumed `crates/at-core/src/rlm.rs`
exposed a `decompose()` function. It does not. The public API is four
constructor-style state machines: `Decomposition` (manual recursive
build), `ContextFold`, `ProgressiveRefinement`, and `StuckDetector`.
Callers — notably `at-agents/src/orchestrator.rs` — supply the subtasks
themselves; the crate provides the bookkeeping, not the planner. Pinning
tests live in `crates/at-core/tests/rlm_integration.rs`. Implication for
future docs and agent guidance: the "decompose" step is a caller
responsibility, not an algorithm in this crate, and any documentation
that implies otherwise should be corrected.

### 4. at-bridge: OAuth token refresh has no single-flight deduplication

`OAuthTokenManager` at `crates/at-bridge/src/oauth_token_manager.rs` is a
storage primitive only — it encrypts tokens, tracks expiry, and exposes
getter/setter accessors. It does **not** perform OAuth HTTP refresh.

The actual HTTP refresh lives in `crates/at-integrations/src/github/oauth.rs`
(`GitHubOAuthClient::refresh_token`) and the GitLab equivalent at
`crates/at-integrations/src/gitlab/oauth.rs`. It is invoked from the
axum handler `github_oauth_refresh` at
`crates/at-bridge/src/http_api/github.rs:718`. That handler holds a
coarse-grained `tokio::sync::RwLock<OAuthTokenManager>` write lock while
issuing the outbound refresh — so concurrent refresh callers serialize
on the lock but each still issues its own outbound HTTP request. There
is no `Notify` / `oneshot` / held-`Mutex` single-flight that would
collapse N simultaneous refresh attempts into one HTTP call. Under load
this opens the classic refresh race: every concurrent expired-token
caller hits the OAuth endpoint, the endpoint may rate-limit the burst,
and a slower-arriving response can overwrite a freshly-stored token from
a faster sibling refresh.

The pinning tests for the storage primitive's contracts
(`concurrent_get_token_callers_all_see_same_value`,
`clear_during_concurrent_get_returns_typed_error_not_torn_state`,
`concurrent_writers_overwrite_atomically`,
`concurrent_callers_on_expired_token_all_see_typed_error`,
`store_after_expired_failure_recovers`) live in
`crates/at-bridge/tests/oauth_token_race.rs`. They pin the contracts a
future single-flight refresh fix would rely on (atomic writes, no torn
reads, no cached failure state) but do not — and cannot, without
crossing crate boundaries — pin HTTP-level dedup. Auth-class HTTP
behaviors (401, 429, 500 responses, retry/backoff posture) are
deliberately not covered here for the same reason.

Suggested fix shape for a follow-up PR: lift the refresh entry point
into `OAuthTokenManager` itself behind an injectable HTTP-call trait,
gate it with `tokio::sync::Mutex<Option<oneshot::Receiver<...>>>` (or
`Notify`-based fan-out), and add `wiremock`-backed tests for the
auth-class scenarios listed in the original test-coverage brief
(`refresh_429_does_not_retry_indefinitely`, `refresh_401_returns_typed_error`,
`concurrent_callers_share_one_refresh`,
`refresh_failure_propagates_to_all_concurrent_callers`,
`subsequent_call_after_failure_retries`).

### 5. at-leptos-ui tests carry ~94 pre-existing clippy warnings

The cleanup wave (D1 in this PR) fixed 17 targeted warnings: 7
`bool_assert_comparison` in `crates/at-cli/tests/smoke.rs`, 6 in
`crates/at-leptos-ui/tests/component_tests.rs`, and 1 unused-import in
`crates/at-agents/src/executor.rs`. Roughly 88 warnings remain in
`at-leptos-ui`'s test code and are out of scope for the test-coverage PR.
They do not fail CI today (no `-D warnings` flag on the clippy step), but
they obscure new warnings introduced by future changes. Suggested
follow-up: run `cargo clippy --fix --lib -p at-leptos-ui --tests` once
and review the diff in a dedicated PR rather than mixing the noise into
unrelated work.

## Supply-chain hardening notes

- Dependabot is enabled at `.github/dependabot.yml` for both `cargo` and
  `github-actions` ecosystems on a weekly grouped schedule.
- The `cargo audit` step in CI runs without `--deny warnings` because the
  workspace pulls 20+ transitive `unmaintained`/`unsound` advisories
  through tauri's gtk stack that we cannot fix from our own code.
  Vulnerabilities still gate CI: `cargo audit` exits non-zero on
  vulnerability advisories by default, so the absence of `--deny warnings`
  only suppresses the informational classes.
- `deny.toml` and `.cargo/audit.toml` are kept in sync — the same four
  `RUSTSEC-*` ignores are documented in both, each with
  `expires: 2026-07-26` and `owner: TBD`. **Action item:** assign real
  owners and a real expiry before 2026-07-26.
- `rustls-webpki` was bumped from `0.103.9` to `0.103.13` during this
  work to patch four CVEs (RUSTSEC-2026-0049, -0098, -0099, -0104). The
  cargo-audit step we added immediately caught these on first run, which
  is the strongest evidence the gate is wired correctly.

## Coverage measurement

- `cargo llvm-cov` runs as a non-blocking CI job (`continue-on-error: true`)
  and uploads `lcov.info` as a 14-day artifact.
- No threshold is enforced yet; the first run on `main` produces the
  baseline number.
- **Action item:** once the baseline is observed in CI, decide whether to
  enforce a floor — either workspace-wide line coverage (e.g. >= 60%) or
  per-crate floors that reflect each crate's risk profile. Until that
  decision is made, regressions in coverage will not block merges.

## Sandbox-specific test failures

Four tests under
`crates/at-core/src/git_read_adapter.rs::tests::shell_adapter_*_fixture`
and a small number of `git2_ops::tests::*` cases fail in the development
sandbox. Root cause: the sandbox intercepts `git commit` with a
code-signing wrapper that returns HTTP 400 on the empty-commit fixtures
those tests build. Behavior was verified pre-existing — they failed
identically on `299b833`, before any of this PR's work. They pass under
real CI (no signing wrapper). Listed here so future debuggers do not
spend cycles re-investigating a sandbox artifact.
