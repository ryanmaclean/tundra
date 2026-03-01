# Troubleshooting Guide

**[← Back to README](../README.md)** | **[Project Handbook](./PROJECT_HANDBOOK.md)** | **[Getting Started](../GETTING_STARTED.md)**

> Common runtime issues and solutions for Auto-Tundra. This guide covers LLM provider failures, PTY session management, WebSocket connections, database configuration, rate limiting, and diagnostic logging.

---

## 📚 Quick Navigation

- [Quick Fixes Checklist](#-quick-fixes-checklist)
- [LLM Provider Issues](#-llm-provider-issues)
- [PTY Session Management](#-pty-session-management)
- [WebSocket Connections](#-websocket-connections)
- [Dolt Database Configuration](#-dolt-database-configuration)
- [Rate Limiting & Circuit Breakers](#-rate-limiting--circuit-breakers)
- [Diagnostics & Logging](#-diagnostics--logging)
- [Error Reference Index](#-error-reference-index)

---

## ⚡ Quick Fixes Checklist

Before diving into specific issues, try these common solutions:

### First Steps
- [ ] Check RUST_LOG is set: `export RUST_LOG=info,at_daemon=debug`
- [ ] Verify all services are running: `pgrep at-daemon`
- [ ] Check API credentials are configured
- [ ] Restart the daemon: `pkill at-daemon && at-daemon`
- [ ] Review recent logs: `tail -f ~/.auto-tundra/logs/daemon.log`

### Common Quick Fixes
- [ ] **Connection failures?** → Check network connectivity and API key validity
- [ ] **Timeouts?** → Verify firewall rules and proxy settings
- [ ] **Port conflicts?** → Check if port 3306 (Dolt) or other ports are in use
- [ ] **Zombie processes?** → Clean up with `pkill -9 -f 'at-'` (use with caution)
- [ ] **High error rates?** → Circuit breaker may be open, wait 30s for recovery

---

## 🤖 LLM Provider Issues

> **Covers:** HttpError, ApiError, RateLimited, Timeout, ParseError, Unsupported errors, provider failover, and model availability.

### Overview

Auto-Tundra supports multiple LLM providers (Anthropic, OpenRouter, OpenAI) with automatic failover. Connection failures, rate limits, and API errors are common during operation. This section explains each error type, its causes, and recovery procedures.

### Provider Architecture

```
┌─────────────┐
│   at-agents │
└──────┬──────┘
       │
┌──────▼──────────┐
│ at-intelligence │  ← Model router, failover logic
└──────┬──────────┘
       │
┌──────▼──────┐
│ at-harness  │  ← Rate limiter, circuit breaker
└──────┬──────┘
       │
   ┌───┴───┬──────┬─────────┐
   │       │      │         │
┌──▼───┐ ┌▼────┐ ┌▼──────┐ ┌▼─────┐
│Anthro│ │OpenR│ │OpenAI │ │Local │
│pic   │ │outer│ │       │ │(vllm)│
└──────┘ └─────┘ └───────┘ └──────┘
```

### Common Errors

#### HttpError: Network-Level Failures

**Error Message:** `HTTP error: <details>`

**Symptoms:**
- Connection refused or connection reset errors
- DNS resolution failures
- TLS/SSL handshake errors
- Network unreachable messages
- Proxy connection failures

**Causes:**
- Network connectivity issues (offline, VPN disconnected)
- Incorrect API base URL configuration
- Firewall blocking outbound HTTPS connections
- DNS server failures
- TLS certificate validation failures (expired certs, MITM proxies)
- Provider endpoint temporarily unavailable

**Solutions:**

1. **Check network connectivity:**
   ```bash
   # Test basic connectivity
   ping -c 3 api.anthropic.com
   curl -I https://api.anthropic.com/v1/messages
   ```

2. **Verify API base URL in profile configuration:**
   ```bash
   # Check configured base URLs
   grep -r "base_url" ~/.auto-tundra/config/
   ```

3. **Test with different provider:**
   - Automatic failover should switch to next available provider
   - Check circuit breaker status: `tail -f ~/.auto-tundra/logs/daemon.log | grep circuit`

4. **Check proxy settings:**
   ```bash
   echo $HTTP_PROXY
   echo $HTTPS_PROXY
   # Test without proxy
   unset HTTP_PROXY HTTPS_PROXY
   ```

5. **Verify TLS certificate chain:**
   ```bash
   openssl s_client -connect api.anthropic.com:443 -showcerts
   ```

**Prevention:**
- Configure multiple provider profiles for automatic failover
- Set appropriate circuit breaker thresholds in `~/.auto-tundra/config/harness.toml`
- Use local inference (vllm.rs, Ollama) as fallback provider

---

#### ApiError: Provider Service Errors

**Error Message:** `API error (status <code>): <message>`

**Symptoms:**
- HTTP 401 Unauthorized: Invalid API key
- HTTP 403 Forbidden: Permission denied, account suspended
- HTTP 404 Not Found: Invalid endpoint or model not available
- HTTP 422 Unprocessable Entity: Invalid request parameters
- HTTP 500/502/503: Provider service errors

**Causes:**
- Missing or invalid API key in environment variables
- API key lacks required permissions
- Account billing issues or suspension
- Invalid model name in request
- Malformed request body (wrong parameters, invalid JSON)
- Provider service outage or degraded performance

**Solutions:**

1. **Verify API key configuration:**
   ```bash
   # Check environment variables
   echo $ANTHROPIC_API_KEY
   echo $OPENROUTER_API_KEY
   echo $OPENAI_API_KEY

   # Test API key manually
   curl https://api.anthropic.com/v1/messages \
     -H "x-api-key: $ANTHROPIC_API_KEY" \
     -H "content-type: application/json" \
     -d '{"model":"claude-sonnet-4-20250514","max_tokens":10,"messages":[{"role":"user","content":"Hi"}]}'
   ```

2. **Check account status:**
   - Visit provider dashboard to verify account is active
   - Check billing status and credit balance
   - Review usage limits and quotas

3. **Verify model availability:**
   ```bash
   # Check configured models
   grep -r "default_model" ~/.auto-tundra/config/
   ```
   - Ensure model names match provider's API (e.g., `claude-sonnet-4-20250514` for Anthropic)
   - Use `anthropic/claude-sonnet-4-20250514` format for OpenRouter

4. **Review request parameters:**
   - Enable debug logging: `export RUST_LOG=at_intelligence=debug`
   - Check logs for malformed requests: `tail -f ~/.auto-tundra/logs/daemon.log`

5. **Check provider status:**
   - Anthropic: https://status.anthropic.com
   - OpenAI: https://status.openai.com
   - OpenRouter: https://openrouter.ai/status

**Prevention:**
- Store API keys in environment variables, not config files
- Configure billing alerts on provider dashboards
- Use ProfileUsage tracking to monitor spend: check `~/.auto-tundra/data/profile_usage.json`
- Set up multiple providers for automatic failover

---

#### RateLimited: Quota Exhaustion

**Error Message:** `rate limited: retry after <seconds>s`

**Symptoms:**
- HTTP 429 Too Many Requests responses
- Requests failing after initial success
- "Retry-After" header in API responses
- Automatic failover to next provider (if configured)

**Causes:**
- Exceeded provider's requests-per-minute (RPM) limit
- Exceeded tokens-per-minute (TPM) limit
- Burst traffic exceeding rate limits
- Multiple concurrent agent sessions
- Tier-based limits for free/starter accounts

**Causes & Limits by Provider:**

| Provider | Tier | RPM | TPM | Notes |
|----------|------|-----|-----|-------|
| Anthropic | Free | 5 | 25k | Very low limits |
| Anthropic | Build | 50 | 100k | Production usage |
| Anthropic | Scale | 1000 | 400k | High-volume |
| OpenRouter | Free | 20 | varies | Per-model limits |
| OpenAI | Free | 3 | 40k | Extremely limited |
| OpenAI | Tier 1 | 500 | 30k | Paid accounts |

**Solutions:**

1. **Check current rate limits:**
   ```bash
   # View profile configuration
   cat ~/.auto-tundra/config/api_profiles.json | jq '.profiles[] | {name, rate_limit_rpm, rate_limit_tpm}'
   ```

2. **Configure per-profile rate limiting:**
   ```toml
   # ~/.auto-tundra/config/harness.toml
   [[profile]]
   name = "anthropic-primary"
   rate_limit_rpm = 45  # Slightly below API limit
   rate_limit_tpm = 90000
   ```

3. **Monitor usage and failover:**
   ```bash
   # Check ProfileUsage metrics
   cat ~/.auto-tundra/data/profile_usage.json | jq '.[] | {profile_id, total_requests, total_rate_limits}'

   # Watch failover events in logs
   tail -f ~/.auto-tundra/logs/daemon.log | grep -E "(rate.limit|failover)"
   ```

4. **Wait for rate limit reset:**
   - Provider rate limits reset on a rolling window (usually 1 minute)
   - Automatic retry with exponential backoff is built-in
   - Circuit breaker prevents hammering rate-limited endpoints

5. **Upgrade provider tier:**
   - Anthropic Build tier: 10x higher limits
   - OpenAI Tier 2+: Usage-based limit increases
   - OpenRouter: Per-model limits, no global cap

**Prevention:**
- Configure multiple API profiles with failover priority
- Set `rate_limit_rpm` slightly below provider limits (safety margin)
- Use token bucket rate limiter (automatically enabled)
- Monitor `ProfileUsage` to identify which provider is hitting limits
- Consider local inference for development/testing workloads

**Automatic Failover Behavior:**
```
Primary (rate limited) → Secondary → Tertiary → Local → Error
     ↓ (60s cooldown)       ↓           ↓          ↓
  Retry primary         Retry secondary ...     No providers
```

---

#### Timeout: Request Hanging or Slow

**Error Message:** `request timed out`

**Symptoms:**
- Requests taking >30 seconds to complete
- No response from provider API
- Intermittent timeouts under load
- Timeout after progress (partial response received)

**Causes:**
- Provider API experiencing high latency
- Large request payloads (very long conversations)
- Network congestion or packet loss
- Provider-side rate limiting (soft throttling)
- Server-side processing delays for complex prompts
- Streaming requests with slow token generation

**Solutions:**

1. **Check request size:**
   ```bash
   # Enable debug logging to see request sizes
   export RUST_LOG=at_intelligence=debug,reqwest=debug
   tail -f ~/.auto-tundra/logs/daemon.log | grep -E "(request_size|timeout)"
   ```

2. **Reduce message history:**
   - Large conversation contexts increase latency
   - Trim older messages from context window
   - Use summarization for long histories

3. **Test provider latency:**
   ```bash
   # Measure API round-trip time
   time curl https://api.anthropic.com/v1/messages \
     -H "x-api-key: $ANTHROPIC_API_KEY" \
     -H "content-type: application/json" \
     -d '{"model":"claude-sonnet-4-20250514","max_tokens":10,"messages":[{"role":"user","content":"Hi"}]}'
   ```

4. **Switch to faster provider:**
   - OpenRouter often has lower latency than direct APIs
   - Local inference has predictable latency (no network)
   - Check provider status pages for incident reports

5. **Increase timeout threshold (if appropriate):**
   ```rust
   // In code (not recommended for general use)
   // Default timeout is 30s, configurable in LlmProvider implementation
   // Increasing timeout may mask underlying issues
   ```

6. **Use streaming for long responses:**
   - Streaming provides incremental results
   - Reduces perceived latency
   - Allows early cancellation if needed

**Prevention:**
- Configure circuit breaker to open after 3 consecutive timeouts
- Use automatic failover to backup providers
- Monitor provider latency trends in logs
- Set reasonable `max_tokens` limits (don't request 4096 if you need 256)

---

#### ParseError: Malformed API Responses

**Error Message:** `parse error: <details>`

**Symptoms:**
- "unexpected EOF while parsing" errors
- "missing field" or "unknown field" JSON errors
- Deserialization failures
- Works intermittently, fails randomly

**Causes:**
- Provider API schema changes (breaking changes)
- Incomplete response due to network interruption
- Streaming response cut off mid-token
- Provider returning non-JSON error pages (5xx HTML)
- Charset/encoding issues in response body
- Provider API beta/unstable endpoint changes

**Solutions:**

1. **Inspect raw response:**
   ```bash
   # Enable detailed HTTP logging
   export RUST_LOG=reqwest=trace,at_intelligence=debug
   tail -f ~/.auto-tundra/logs/daemon.log | grep -A 20 "response_body"
   ```

2. **Check API version:**
   ```bash
   # Verify API version headers
   curl -I https://api.anthropic.com/v1/messages \
     -H "x-api-key: $ANTHROPIC_API_KEY"
   ```

3. **Test with minimal request:**
   ```bash
   # Simplest possible request to isolate parsing issue
   curl https://api.anthropic.com/v1/messages \
     -H "x-api-key: $ANTHROPIC_API_KEY" \
     -H "anthropic-version: 2023-06-01" \
     -H "content-type: application/json" \
     -d '{"model":"claude-sonnet-4-20250514","max_tokens":10,"messages":[{"role":"user","content":"Hi"}]}' | jq .
   ```

4. **Check for provider API updates:**
   - Review provider changelog/release notes
   - Update `at-intelligence` crate if needed
   - Check GitHub issues for similar parse errors

5. **Examine response content-type:**
   - Ensure provider is returning `application/json`
   - HTML error pages indicate server-side failure (500/503)

**Prevention:**
- Use well-tested provider SDK implementations
- Pin API versions in requests (e.g., `anthropic-version: 2023-06-01`)
- Configure circuit breaker to open on repeated parse errors
- Monitor provider API changelogs for breaking changes
- Keep `at-intelligence` crate updated

---

#### Unsupported: Model or Feature Unavailable

**Error Message:** `unsupported: <details>`

**Symptoms:**
- "Model not found" errors
- "Streaming not supported by this provider"
- "Tool calling not available for this model"
- Feature works with one provider but not another

**Causes:**
- Requesting a model not available on current provider
- Using streaming API with non-streaming provider
- Tool calling (function calling) not supported by model
- Vision/multimodal features on text-only models
- Provider doesn't implement specific API features
- Regional restrictions on model access

**Solutions:**

1. **Check model availability:**
   ```bash
   # Anthropic models
   # - claude-sonnet-4-20250514 (latest Sonnet)
   # - claude-opus-4-20250514 (latest Opus)
   # - claude-haiku-4-20250514 (latest Haiku)

   # OpenRouter models (prefix with provider)
   # - anthropic/claude-sonnet-4-20250514
   # - openai/gpt-4o
   # - google/gemini-pro

   # Verify configured models
   grep -r "default_model" ~/.auto-tundra/config/
   ```

2. **Test model access:**
   ```bash
   # Test Anthropic model
   curl https://api.anthropic.com/v1/messages \
     -H "x-api-key: $ANTHROPIC_API_KEY" \
     -H "anthropic-version: 2023-06-01" \
     -H "content-type: application/json" \
     -d '{"model":"claude-sonnet-4-20250514","max_tokens":10,"messages":[{"role":"user","content":"Hi"}]}'
   ```

3. **Check feature support:**
   | Feature | Anthropic | OpenRouter | OpenAI | Local (vllm) |
   |---------|-----------|------------|--------|--------------|
   | Streaming | ✅ | ✅ | ✅ | ✅ |
   | Tool calling | ✅ | Varies | ✅ | ⚠️ Model-dependent |
   | Vision | ✅ Claude 3+ | Varies | ✅ GPT-4o | ⚠️ Model-dependent |
   | JSON mode | ✅ | Varies | ✅ | ⚠️ Model-dependent |

4. **Fallback to supported provider:**
   - Configure multiple profiles with different capabilities
   - Use OpenRouter for broad model access (400+ models)
   - Local inference for development/testing

5. **Update model configuration:**
   ```toml
   # ~/.auto-tundra/config/api_profiles.json
   {
     "profiles": [
       {
         "name": "anthropic-primary",
         "provider": "anthropic",
         "default_model": "claude-sonnet-4-20250514",  # Use valid model ID
         "enabled": true
       }
     ]
   }
   ```

**Prevention:**
- Verify model names against provider documentation
- Use `ProviderKind::default_model_for()` helper for safe defaults
- Configure multiple providers to maximize feature availability
- Check provider API docs before using new features
- Test locally before deploying provider changes

---

### Provider Failover & Circuit Breaker

Auto-Tundra automatically switches to backup providers when errors occur. Understanding this behavior helps diagnose multi-provider issues.

#### Failover Priority

Providers are tried in priority order (lower number = higher priority):

```bash
# Check failover order
cat ~/.auto-tundra/config/api_profiles.json | jq '.profiles[] | {name, priority, enabled}' | sort -k priority
```

**Example configuration:**
```json
{
  "profiles": [
    {"name": "anthropic-primary", "priority": 0, "enabled": true},
    {"name": "openrouter-backup", "priority": 1, "enabled": true},
    {"name": "local-fallback", "priority": 2, "enabled": true}
  ]
}
```

#### Failover Triggers

Automatic failover occurs on:
- ✅ **HttpError**: Network failures, DNS errors
- ✅ **RateLimited**: 429 responses (with cooldown)
- ✅ **Timeout**: Request timeouts
- ✅ **ApiError (5xx)**: Server errors (500, 502, 503)
- ❌ **ApiError (4xx)**: Client errors (400, 401, 403) - NO failover (fix config instead)
- ❌ **ParseError**: Indicates code bug, not provider issue

#### Circuit Breaker Integration

Circuit breaker prevents cascading failures:

```
CLOSED → OPEN → HALF_OPEN → CLOSED
  ↓        ↓         ↓          ↓
Normal   Failed   Testing    Recovered
```

**States:**
- **CLOSED**: Normal operation, all requests allowed
- **OPEN**: Too many failures, reject all requests for 30s
- **HALF_OPEN**: Testing recovery, allow 1 request
- **CLOSED**: Recovery successful, resume normal operation

**Configuration:**
```rust
// Circuit breaker thresholds (at-harness/src/circuit_breaker.rs)
// - failure_threshold: 5 errors
// - timeout: 30 seconds
// - half_open_requests: 1
```

**Check circuit breaker status:**
```bash
# Watch circuit state changes
tail -f ~/.auto-tundra/logs/daemon.log | grep -E "(circuit|breaker|state)"
```

**Manual recovery:**
```bash
# Restart daemon to reset all circuit breakers
pkill at-daemon && at-daemon

# Or wait 30s for automatic half-open test
```

#### Rate Limiter Integration

Token bucket rate limiter enforces per-provider limits:

```
Bucket capacity: rate_limit_rpm requests/minute
Refill rate: rate_limit_rpm / 60 tokens/second
```

**How it works:**
1. Each request consumes 1 token from the bucket
2. Bucket refills at steady rate (e.g., 50 RPM = 0.83 tokens/sec)
3. If bucket empty, request waits until token available
4. Prevents bursts from triggering provider rate limits

**Configuration:**
```toml
# ~/.auto-tundra/config/harness.toml
[[profile]]
name = "anthropic-primary"
rate_limit_rpm = 45  # Set below provider limit (50) for safety margin
rate_limit_tpm = 90000

[[profile]]
name = "openrouter-backup"
rate_limit_rpm = 18  # Below OpenRouter free tier (20)
```

**Monitor rate limiting:**
```bash
# Check ProfileUsage for rate limit hits
cat ~/.auto-tundra/data/profile_usage.json | jq '.[] | {name: .profile_id, rate_limits: .total_rate_limits, requests: .total_requests}'
```

---

### Diagnostic Commands

**Check provider health:**
```bash
# View all configured profiles
cat ~/.auto-tundra/config/api_profiles.json | jq '.profiles[] | {name, provider, enabled, priority}'

# Test API key validity
curl -I https://api.anthropic.com/v1/messages -H "x-api-key: $ANTHROPIC_API_KEY"

# Check usage metrics
cat ~/.auto-tundra/data/profile_usage.json | jq .
```

**Monitor provider errors:**
```bash
# Watch LLM errors in real-time
tail -f ~/.auto-tundra/logs/daemon.log | grep -E "(LlmError|ProviderError|HttpError|ApiError)"

# Count errors by type
grep "LlmError" ~/.auto-tundra/logs/daemon.log | grep -oE "LlmError::[A-Za-z]+" | sort | uniq -c
```

**Test failover manually:**
```bash
# Disable primary provider to force failover
jq '.profiles[0].enabled = false' ~/.auto-tundra/config/api_profiles.json > /tmp/profiles.json
mv /tmp/profiles.json ~/.auto-tundra/config/api_profiles.json

# Restart daemon and test
pkill at-daemon && at-daemon
```

---

## 🖥️ PTY Session Management

> **Covers:** AtCapacity, HandleNotFound, SpawnFailed errors, zombie processes, PTY pool exhaustion, and cleanup procedures.

### Overview

Auto-Tundra uses a PTY (pseudo-terminal) pool to execute shell commands. Sessions can leak, processes can become zombies, and the pool can reach capacity. This section covers detection and recovery.

### PTY Pool Architecture

```
┌─────────────┐
│  at-session │
└──────┬──────┘
       │
┌──────▼──────────┐
│   PTY Pool      │  ← Max capacity, handle management
│   (pty_pool.rs) │
└──────┬──────────┘
       │
   ┌───┴────┬──────┬──────┐
   │        │      │      │
┌──▼──┐  ┌─▼──┐ ┌─▼──┐ ┌─▼──┐
│PTY 1│  │PTY2│ │PTY3│ │PTYn│
└─────┘  └────┘ └────┘ └────┘
```

### PTY Lifecycle

**Normal Flow:**
1. **Spawn**: `PtyPool::spawn()` creates PTY, starts child process, allocates reader/writer threads
2. **I/O**: Use `handle.send()` for stdin, `handle.reader.recv()` for stdout/stderr
3. **Cleanup**: Call `handle.kill()` to terminate process, then `PtyPool::release()` to free slot

**Thread Management:**
- Each PTY spawns 2 background threads: reader (stdout/stderr) and writer (stdin)
- Threads run until process exits or handle is dropped
- Bounded channels (256 messages) provide backpressure

**Resource Tracking:**
```rust
// PTY pool enforces strict capacity limit
let pool = PtyPool::new(10);  // Max 10 concurrent PTYs
pool.active_count()  // Current number of active PTYs
```

### Common Errors

#### AtCapacity: PTY Pool Exhaustion

**Error Message:** `pty pool is at capacity ({max})`

**Symptoms:**
- Cannot spawn new agent sessions or shell commands
- Error occurs when `active_count() >= max_ptys`
- Existing sessions continue to work normally
- New session requests fail immediately (not queued)
- Logs show "pool is at capacity" with maximum capacity count

**Causes:**
- Too many concurrent agent sessions running
- PTY handles not released after task completion (leaked handles)
- Long-running background tasks consuming pool slots
- Pool capacity set too low for workload (`max_ptys` configuration)
- Crashed sessions not properly cleaned up
- Zombie processes holding PTY slots without being released

**Solutions:**

1. **Check current pool usage:**
   ```bash
   # Enable debug logging to see PTY lifecycle
   export RUST_LOG=at_session=debug
   tail -f ~/.auto-tundra/logs/daemon.log | grep -E "(spawn|release|capacity)"
   ```

2. **Kill orphaned PTY sessions:**
   ```bash
   # Find all PTY-related processes
   ps aux | grep -E "(at-session|pty)" | grep -v grep

   # Kill specific zombie processes (use with caution)
   pkill -f "at-session"

   # Or restart daemon to clean up all sessions
   pkill at-daemon && at-daemon
   ```

3. **Release completed sessions:**
   - Ensure all `PtyHandle::kill()` calls are followed by `PtyPool::release()`
   - Check application code for missing cleanup in error paths
   - Use RAII patterns (drop handlers) to guarantee cleanup

4. **Increase pool capacity (temporary fix):**
   ```rust
   // In at-session configuration
   // Default: PtyPool::new(10)
   // Increase if you have legitimate high concurrency
   let pool = PtyPool::new(20);  // Adjust based on system resources
   ```

5. **Audit active handles:**
   ```bash
   # Check number of PTY master devices
   ls -la /dev/pts/ | wc -l

   # Check file descriptor usage
   lsof -p $(pgrep at-daemon) | grep pts
   ```

**Prevention:**
- Always pair `spawn()` with `kill()` + `release()` in try/finally or drop handlers
- Set `max_ptys` based on system limits: `ulimit -n` (file descriptors) / 3 (2 threads + master)
- Implement session timeout for idle PTYs
- Monitor pool usage metrics: `active_count() / max_ptys` ratio
- Use structured concurrency patterns to guarantee cleanup on task cancellation

**Capacity Planning:**
```bash
# Check system limits
ulimit -n  # Max file descriptors (macOS default: 256-1024, Linux: 1024-4096)

# Each PTY consumes:
# - 1 PTY master file descriptor
# - 2 threads (reader, writer)
# - 2 channel endpoints (flume sender/receiver)

# Safe capacity formula:
# max_ptys = min(
#   (ulimit -n) / 3,
#   available_threads / 2,
#   desired_concurrency
# )

# Example: ulimit -n = 1024
# max_ptys = 1024 / 3 ≈ 340 (theoretical max)
# Recommended: 10-50 for safety margin
```

---

#### HandleNotFound: Unreleased PTY Handle

**Error Message:** `pty handle not found: <uuid>`

**Symptoms:**
- Error when trying to kill or release a PTY handle
- UUID mismatch between handle and pool registry
- Handle was never registered or already released
- Intermittent failures when cleaning up sessions
- Double-release attempts causing errors

**Causes:**
- Calling `PtyPool::kill()` or `PtyPool::release()` with invalid UUID
- Handle was already released earlier in the code path
- Race condition: handle released by one thread, accessed by another
- Handle UUID generated outside pool (manual creation without registration)
- Database or state corruption causing UUID mismatch
- Session cleanup called multiple times for same handle

**Solutions:**

1. **Verify handle UUID tracking:**
   ```bash
   # Enable debug logging for handle lifecycle
   export RUST_LOG=at_session=debug,at_core=debug
   tail -f ~/.auto-tundra/logs/daemon.log | grep -E "(handle|uuid|release|kill)"
   ```

2. **Check handle state before cleanup:**
   ```rust
   // In application code, track handle state
   if handle.is_alive() {
       handle.kill()?;
   }
   // Only release if handle is registered in pool
   pool.release(handle.id)?;
   ```

3. **Audit double-release scenarios:**
   ```bash
   # Search for duplicate release calls in logs
   grep "release" ~/.auto-tundra/logs/daemon.log | sort | uniq -d

   # Check for race conditions in concurrent cleanup
   grep "HandleNotFound" ~/.auto-tundra/logs/daemon.log
   ```

4. **Synchronize handle cleanup:**
   ```rust
   // Use Arc<Mutex<Option<PtyHandle>>> to prevent double-release
   let handle_lock = Arc::new(Mutex::new(Some(handle)));

   // In cleanup code
   if let Some(h) = handle_lock.lock().unwrap().take() {
       h.kill()?;
       pool.release(h.id)?;
   }
   ```

5. **Restart daemon if state is corrupted:**
   ```bash
   # Clean restart to reset all handle registrations
   pkill at-daemon
   rm -f ~/.auto-tundra/state/pty_pool.json  # If state is persisted
   at-daemon
   ```

**Prevention:**
- Use RAII: store handles in structs with Drop implementation for automatic cleanup
- Track handle lifecycle in state machine (Spawned → Running → Killed → Released)
- Avoid manual UUID generation; always use `PtyPool::spawn()` return value
- Use `Arc<Mutex<_>>` or `RwLock` for handles shared across threads
- Implement idempotent cleanup: check if already released before calling `release()`
- Add telemetry to track handle lifecycle: spawn count, release count, mismatches

**Debug Pattern:**
```rust
// Add tracing to handle lifecycle
use tracing::{debug, warn};

impl PtyHandle {
    pub fn kill_and_release(self, pool: &PtyPool) -> Result<()> {
        let id = self.id;
        debug!(%id, "killing PTY handle");
        self.kill()?;

        debug!(%id, "releasing PTY handle from pool");
        pool.release(id).map_err(|e| {
            warn!(%id, ?e, "failed to release PTY handle");
            e
        })
    }
}
```

---

#### SpawnFailed: Process Creation Failure

**Error Message:** `pty spawn failed: <details>`

**Symptoms:**
- Cannot create new PTY sessions
- Error during `PtyPool::spawn()` call
- "Command not found" or "Permission denied" errors
- "Resource temporarily unavailable" (EAGAIN)
- PTY allocation failures
- Failed to execute binary errors

**Causes:**
- Binary not found in PATH (e.g., `bash`, `zsh`, custom agent CLI)
- Binary lacks execute permission (`chmod +x` not run)
- Insufficient system resources (out of file descriptors, memory, or PTY devices)
- PTY system failure (kernel limits reached: `/dev/pts` full)
- Invalid command arguments or environment variables
- SELinux/AppArmor blocking PTY creation
- macOS sandbox restrictions (unsigned binaries, entitlements)

**Solutions:**

1. **Verify binary exists and is executable:**
   ```bash
   # Check if binary is in PATH
   which bash
   which at-agent  # Or your custom agent binary

   # Check execute permissions
   ls -la $(which bash)

   # Test binary execution manually
   bash -c "echo test"
   ```

2. **Check system resource limits:**
   ```bash
   # File descriptor limits
   ulimit -n
   lsof | wc -l  # Current FD usage across all processes

   # PTY device availability
   ls -la /dev/pts/ | wc -l

   # Kernel PTY limits (Linux)
   cat /proc/sys/kernel/pty/max
   cat /proc/sys/kernel/pty/nr  # Current usage

   # Increase limits if needed
   ulimit -n 2048
   ```

3. **Test PTY creation manually:**
   ```bash
   # Use Python to test PTY allocation
   python3 -c "import pty; pty.openpty()"

   # If this fails, PTY system has issues
   ```

4. **Check command and arguments:**
   ```rust
   // Enable debug logging to see exact spawn command
   export RUST_LOG=at_session=debug

   // In logs, look for spawn attempt with full command line
   // Example: "spawning PTY: bash -c 'echo test'"
   ```

5. **Verify environment variables:**
   ```bash
   # Check PATH is set correctly
   echo $PATH

   # Test command with explicit path
   /bin/bash -c "echo test"

   # Check for restrictive environment
   env | grep -E "(PATH|LD_LIBRARY_PATH|SHELL)"
   ```

6. **Increase kernel PTY limits (Linux):**
   ```bash
   # Temporary increase
   sudo sysctl -w kernel.pty.max=4096

   # Permanent increase
   echo "kernel.pty.max = 4096" | sudo tee -a /etc/sysctl.conf
   sudo sysctl -p
   ```

7. **macOS sandbox workarounds:**
   ```bash
   # Check if binary is signed (required on macOS)
   codesign -dvv /path/to/binary

   # Disable sandbox for testing (development only)
   # Add entitlements or use signed binaries for production
   ```

**Prevention:**
- Validate binary paths before spawning (check `std::fs::metadata()`)
- Set appropriate file descriptor limits in systemd service or shell profile
- Monitor PTY device usage and set alerts for high utilization
- Use absolute paths in spawn commands to avoid PATH issues
- Test spawn in CI/CD with restrictive resource limits
- Implement graceful degradation: retry with backoff, fallback to non-PTY execution

**Diagnostic Commands:**
```bash
# Full PTY system diagnostic
echo "=== File Descriptor Limits ==="
ulimit -n

echo "=== PTY Device Count ==="
ls -la /dev/pts/ | wc -l

echo "=== Process PTY Usage ==="
lsof -p $(pgrep at-daemon) | grep pts | wc -l

echo "=== Kernel PTY Limits (Linux) ==="
if [ -f /proc/sys/kernel/pty/max ]; then
  echo "Max: $(cat /proc/sys/kernel/pty/max)"
  echo "Current: $(cat /proc/sys/kernel/pty/nr)"
fi

echo "=== Binary Permissions ==="
ls -la $(which bash)

echo "=== Test PTY Creation ==="
python3 -c "import pty; m, s = pty.openpty(); print(f'Master: {m}, Slave: {s}')"
```

---

### Zombie Processes and Session Leaks

#### Understanding Zombie Processes

**What is a zombie process?**
A zombie is a child process that has terminated but hasn't been reaped by its parent. The process no longer runs, but its entry remains in the process table, consuming a PID and metadata.

**PTY-specific zombies:**
In Auto-Tundra's PTY pool, zombies occur when:
- Child process exits but `PtyHandle::kill()` not called
- Parent process crashes before reaping child
- Background threads terminated without cleanup
- Handle dropped without calling `kill()` first

**Symptoms:**
```bash
# Zombie processes show as <defunct> in ps
ps aux | grep defunct

# Example output:
# user  1234  0.0  0.0      0     0 ?   Z    10:00   0:00 [bash] <defunct>
```

#### Detecting Zombie Processes

**Check for zombies in PTY pool:**
```bash
# Find all defunct processes related to at-session
ps aux | grep -E "(defunct|Z)" | grep -E "(at-|bash|zsh)"

# Count zombies
ps aux | grep defunct | wc -l

# Check parent process of zombies
ps -ef | grep defunct
```

**Monitor via logs:**
```bash
# Enable debug logging
export RUST_LOG=at_session=debug

# Watch for processes that exit without cleanup
tail -f ~/.auto-tundra/logs/daemon.log | grep -E "(exited|terminated|zombie|defunct)"
```

#### Cleaning Up Zombie Processes

**Method 1: Let parent reap (automatic):**
```bash
# Zombies are automatically reaped when parent process calls wait()
# at-daemon should do this automatically via PtyHandle::kill()

# If parent is running, zombies will clear when:
# 1. Parent calls wait() or waitpid()
# 2. Parent process terminates (init/systemd adopts and reaps)
```

**Method 2: Restart daemon (safe):**
```bash
# Graceful restart allows daemon to clean up
pkill -TERM at-daemon
sleep 2
at-daemon

# Force restart if graceful fails
pkill -9 at-daemon
at-daemon
```

**Method 3: System reboot (nuclear option):**
```bash
# Only if zombies persist after daemon restart
# Zombies cannot be killed with kill -9; they're already dead
# Reboot clears all zombie processes
sudo reboot
```

#### Session Leak Detection

**What is a session leak?**
A session leak occurs when a PTY handle is allocated but never released, consuming pool capacity without doing work.

**Common leak scenarios:**
1. Exception thrown before cleanup code
2. Async task cancelled before completion
3. Handle stored in data structure, never removed
4. Circular references preventing drop
5. Background task panics without cleanup

**Detect leaks:**
```bash
# Compare pool active count vs expected sessions
export RUST_LOG=at_session=debug
tail -f ~/.auto-tundra/logs/daemon.log | grep -E "active_count|spawn|release"

# Example diagnostic output:
# active_count=5  # Expected: 2 sessions running
# → 3 leaked handles

# Check PTY device usage
ls -la /dev/pts/ | wc -l
# Should match active_count + 1 (master terminal)

# Find long-running PTY sessions
ps aux | grep -E "(bash|zsh)" | awk '{print $9, $10, $11}'
# Look for sessions running longer than expected
```

#### Preventing Leaks

**Pattern 1: RAII with Drop:**
```rust
struct SessionGuard {
    handle: Option<PtyHandle>,
    pool: Arc<PtyPool>,
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        if let Some(h) = self.handle.take() {
            let _ = h.kill();
            let _ = self.pool.release(h.id);
        }
    }
}
```

**Pattern 2: Explicit cleanup in try/finally:**
```rust
async fn run_session(pool: &PtyPool) -> Result<()> {
    let handle = pool.spawn("bash", &[], &[])?;

    // Use defer pattern or scopeguard crate
    let _guard = scopeguard::guard(handle, |h| {
        let _ = h.kill();
        let _ = pool.release(h.id);
    });

    // Session work here...
    Ok(())
}
```

**Pattern 3: Timeout-based cleanup:**
```rust
// Set maximum session lifetime
const SESSION_TIMEOUT: Duration = Duration::from_secs(300); // 5 minutes

tokio::select! {
    result = run_session() => result,
    _ = tokio::time::sleep(SESSION_TIMEOUT) => {
        warn!("session timeout, forcing cleanup");
        handle.kill()?;
        pool.release(handle.id)?;
        Err(PtyError::Timeout)
    }
}
```

#### Capacity Management Best Practices

**1. Right-size pool capacity:**
```rust
// Formula: max_ptys = expected_concurrency + buffer
// Example: 5 concurrent agents + 5 buffer = 10 max_ptys
let pool = PtyPool::new(10);
```

**2. Monitor pool metrics:**
```rust
// Add metrics collection
let usage = pool.active_count() as f64 / pool.max_ptys() as f64;
if usage > 0.8 {
    warn!("PTY pool usage high: {:.1}%", usage * 100.0);
}
```

**3. Implement session limits:**
```rust
// Limit concurrent sessions per user/task
const MAX_SESSIONS_PER_USER: usize = 3;

if user_sessions.len() >= MAX_SESSIONS_PER_USER {
    return Err(PtyError::TooManySessions);
}
```

**4. Graceful degradation:**
```rust
// When pool is at capacity, queue or reject gracefully
match pool.spawn("bash", &[], &[]) {
    Err(PtyError::AtCapacity { max }) => {
        // Option 1: Queue request
        session_queue.push_back(request);

        // Option 2: Reject with retry-after
        Err(Error::TooManyRequests { retry_after: 5 })
    }
    result => result,
}
```

**5. Automated cleanup tasks:**
```rust
// Background task to clean up idle sessions
tokio::spawn(async move {
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;

        // Kill sessions idle > 5 minutes
        for (id, session) in sessions.iter() {
            if session.idle_time() > Duration::from_secs(300) {
                warn!(%id, "killing idle session");
                let _ = session.kill();
                let _ = pool.release(id);
            }
        }
    }
});
```

---

### Diagnostic Commands

**Check PTY pool status:**
```bash
# View active PTY sessions
export RUST_LOG=at_session=debug
tail -f ~/.auto-tundra/logs/daemon.log | grep -E "(active_count|spawn|release)"

# Count PTY devices
ls -la /dev/pts/ | wc -l

# Find PTY-related processes
ps aux | grep -E "(bash|zsh|at-session)" | grep -v grep
```

**Monitor resource usage:**
```bash
# File descriptor usage for daemon
lsof -p $(pgrep at-daemon) | wc -l

# PTY-specific file descriptors
lsof -p $(pgrep at-daemon) | grep pts

# System-wide PTY usage (Linux)
cat /proc/sys/kernel/pty/nr
```

**Clean up leaked sessions:**
```bash
# Kill all bash/zsh sessions (use with caution)
pkill -f "bash.*at-session"

# Restart daemon to reset pool
pkill at-daemon && at-daemon

# Force kill if graceful shutdown fails
pkill -9 at-daemon
```

**Test PTY spawn manually:**
```bash
# Verify PTY system is functional
python3 << 'EOF'
import pty
import os

master, slave = pty.openpty()
print(f"PTY created successfully: master={master}, slave={slave}")
os.close(master)
os.close(slave)
EOF
```

---

## 🔌 WebSocket Connections

> **Covers:** Disconnection handling, 10-second reconnection grace period, 5-minute idle timeout, heartbeat failures, TransportError, and IpcError.

### Overview

WebSocket connections provide real-time updates between `at-bridge` and clients. Connections can drop, timeout, or fail heartbeat checks. This section covers connection lifecycle, timeouts, and reconnection strategies.

### WebSocket Architecture

```
┌─────────────┐
│   Client    │
└──────┬──────┘
       │ WebSocket
┌──────▼──────────┐
│   at-bridge     │  ← HTTP/WS API server
└──────┬──────────┘
       │ IPC
┌──────▼──────────┐
│   at-daemon     │  ← Event bus, orchestration
└─────────────────┘
```

### Connection Lifecycle

WebSocket connections follow a state machine with automatic recovery mechanisms to handle network interruptions gracefully.

#### Active Connection State

When a WebSocket connection is successfully established:

1. **Terminal status transitions to `Active`**
2. **Buffered output is replayed** (if reconnecting within grace period)
3. **Three concurrent tasks are spawned:**
   - **Reader Task**: Reads PTY output and sends to WebSocket (5-minute idle timeout)
   - **Writer Task**: Reads WebSocket messages and writes to PTY stdin (5-minute idle timeout)
   - **Heartbeat Task**: Sends Ping frames every 30 seconds to detect half-open connections

**Example: Active connection logs**
```
[INFO] terminal_ws: WebSocket connection established for terminal abc123
[DEBUG] terminal_ws: Status transition: Disconnected → Active
[DEBUG] terminal_ws: Replaying 2048 bytes of buffered output
[DEBUG] terminal_ws: Spawned reader/writer/heartbeat tasks
```

---

#### Disconnection & Reconnection Grace Period

When the WebSocket disconnects (network failure, tab close, browser navigation):

1. **Terminal status transitions to `Disconnected`** with timestamp
2. **PTY process continues running in the background** (not killed)
3. **Output is buffered** (last 4KB) for **10 seconds** (WS_RECONNECT_GRACE)
4. **Two recovery scenarios:**

**Scenario A: Reconnection Within Grace Period (< 10 seconds)**
- Client reconnects before grace period expires
- Buffered output is replayed to restore terminal state
- Session resumes transparently without data loss
- Status transitions: `Disconnected` → `Active`

**Scenario B: Grace Period Expires (> 10 seconds)**
- PTY process is killed
- Terminal status transitions to `Dead`
- Subsequent reconnect attempts receive `410 Gone` HTTP status
- User must create a new terminal session

**Example: Successful reconnection**
```
[WARN] terminal_ws: WebSocket disconnected for terminal abc123
[DEBUG] terminal_ws: Status transition: Active → Disconnected
[DEBUG] terminal_ws: Buffering output for 10-second grace period
[INFO] terminal_ws: Client reconnected after 3 seconds (within grace period)
[DEBUG] terminal_ws: Status transition: Disconnected → Active
[INFO] terminal_ws: Replayed 1024 bytes of buffered output
```

**Example: Grace period expiration**
```
[WARN] terminal_ws: WebSocket disconnected for terminal abc123
[DEBUG] terminal_ws: Status transition: Active → Disconnected
[DEBUG] terminal_ws: Buffering output for 10-second grace period
[ERROR] terminal_ws: Grace period expired (10s) without reconnection
[INFO] terminal_ws: Killing PTY process for terminal abc123
[DEBUG] terminal_ws: Status transition: Disconnected → Dead
[WARN] terminal_ws: Reconnect attempt received, returning 410 Gone
```

---

#### Timeouts & Heartbeat Failures

WebSocket connections are monitored with multiple timeout mechanisms to detect failures and prevent resource leaks.

##### Idle Timeout (5 Minutes)

**Configuration:** `WS_IDLE_TIMEOUT = 300 seconds`

**Behavior:**
- Connection automatically closes if **no data flows in either direction** for 5 minutes
- Applies to both reader and writer tasks independently
- Prevents resource leaks from abandoned connections

**Symptoms:**
- Connection closes silently after 5 minutes of inactivity
- No error message (normal idle closure)
- Client should attempt reconnection

**Example: Idle timeout**
```
[DEBUG] terminal_ws: No data received for 300 seconds
[INFO] terminal_ws: Idle timeout reached, closing WebSocket
[DEBUG] terminal_ws: Status transition: Active → Disconnected
[DEBUG] terminal_ws: Starting 10-second reconnection grace period
```

**Solutions:**
1. **Client should implement automatic reconnection:**
   ```javascript
   let ws = new WebSocket('ws://localhost:3000/ws/terminal/abc123');
   ws.onclose = () => {
     console.log('Connection closed, reconnecting...');
     setTimeout(() => reconnect(), 1000);
   };
   ```

2. **Send periodic activity to keep connection alive:**
   ```javascript
   // Send heartbeat every 4 minutes to prevent idle timeout
   setInterval(() => {
     if (ws.readyState === WebSocket.OPEN) {
       ws.send(JSON.stringify({type: "ping"}));
     }
   }, 240000); // 4 minutes
   ```

##### Heartbeat Interval (30 Seconds)

**Configuration:** `WS_HEARTBEAT_INTERVAL = 30 seconds`

**Behavior:**
- Server sends **Ping frames** every 30 seconds
- Client must respond with **Pong frames** (handled automatically by browsers)
- Detects half-open TCP connections where client disconnected without sending Close frame

**Symptoms:**
- Connection closes if client fails to respond to Ping frames
- "Pong timeout" errors in logs
- Indicates network partition or client crash

**Example: Heartbeat success**
```
[TRACE] terminal_ws: Sending heartbeat ping (frame 42)
[TRACE] terminal_ws: Received pong response (frame 42)
```

**Example: Heartbeat failure**
```
[TRACE] terminal_ws: Sending heartbeat ping (frame 43)
[WARN] terminal_ws: Pong timeout after 10 seconds
[ERROR] terminal_ws: Heartbeat failure detected, closing connection
[DEBUG] terminal_ws: Status transition: Active → Disconnected
```

**Solutions:**
1. **Browser WebSocket clients:** Pong responses are automatic (no action needed)

2. **Custom WebSocket clients:** Ensure Pong frames are sent in response to Ping:
   ```rust
   // Rust example with tokio-tungstenite
   match msg {
       Message::Ping(payload) => {
           ws.send(Message::Pong(payload)).await?;
       }
       _ => {}
   }
   ```

3. **Check network stability:**
   ```bash
   # Monitor packet loss
   ping -c 100 api.yourdomain.com | grep loss

   # Check TCP connection stability
   netstat -an | grep ESTABLISHED | grep 3000
   ```

##### Connection Lifecycle Summary

| State | Description | Timeout | Recovery |
|-------|-------------|---------|----------|
| **Active** | WebSocket connected, data flowing | 5min idle, 30s heartbeat | Automatic heartbeat |
| **Disconnected** | Network failure, buffering output | 10s grace period | Reconnect within 10s |
| **Dead** | Grace period expired, PTY killed | N/A | Create new terminal |

---

### Common Errors

#### TransportError: Network Failures

**Error Message:** `transport error: <details>`

**Symptoms:**
- "Connection reset by peer" errors
- "Broken pipe" errors during writes
- Sudden disconnection without Close frame
- Network unreachable messages

**Causes:**
- Network connectivity loss (WiFi disconnect, VPN failure)
- Firewall blocking WebSocket traffic
- Proxy/load balancer timeout
- Client crash or tab close without graceful shutdown
- Browser enforced connection limits (too many tabs)

**Solutions:**

1. **Check network connectivity:**
   ```bash
   # Test basic connectivity to bridge
   curl -I http://localhost:3000/api/terminals

   # Check WebSocket upgrade capability
   wscat -c ws://localhost:3000/ws/terminal/abc123
   ```

2. **Verify firewall rules:**
   ```bash
   # macOS: Check if port 3000 is blocked
   sudo pfctl -s rules | grep 3000

   # Linux: Check iptables
   sudo iptables -L -n | grep 3000
   ```

3. **Monitor reconnection attempts:**
   ```bash
   # Enable debug logging for WebSocket connections
   export RUST_LOG=at_bridge=debug
   at-bridge

   # Watch for reconnection patterns
   tail -f ~/.auto-tundra/logs/bridge.log | grep -E '(Disconnected|Active|grace)'
   ```

4. **Configure proxy/load balancer timeouts:**
   - Ensure proxy timeout > 5 minutes (WS_IDLE_TIMEOUT)
   - Configure proxy to pass WebSocket upgrade headers
   - Example nginx configuration:
     ```nginx
     location /ws/ {
         proxy_pass http://localhost:3000;
         proxy_http_version 1.1;
         proxy_set_header Upgrade $http_upgrade;
         proxy_set_header Connection "upgrade";
         proxy_read_timeout 600s;  # 10 minutes > 5min idle timeout
         proxy_send_timeout 600s;
     }
     ```

**Prevention:**
- Implement automatic reconnection with exponential backoff in client
- Monitor network quality metrics (packet loss, latency)
- Use persistent terminal mode for long-running sessions
- Configure adequate grace period for expected network interruptions

---

#### IpcError: Daemon Communication Failures

**Error Message:** `IPC error: <details>`

**Symptoms:**
- "Failed to connect to daemon" errors
- "Daemon not responding" timeouts
- Events not received by client
- Terminal state changes not reflected in UI

**Causes:**
- `at-daemon` process not running
- Unix socket permission errors
- IPC socket file deleted or corrupted
- Daemon crashed or hung
- File descriptor exhaustion

**Solutions:**

1. **Verify daemon is running:**
   ```bash
   # Check daemon process
   pgrep -fl at-daemon

   # Restart if not running
   at-daemon &

   # Check daemon logs for crash reasons
   tail -100 ~/.auto-tundra/logs/daemon.log
   ```

2. **Check IPC socket:**
   ```bash
   # Find IPC socket location (usually /tmp or /var/run)
   ls -la /tmp/*.sock | grep tundra
   ls -la /var/run/*.sock | grep tundra

   # Verify permissions (should be readable/writable)
   stat /tmp/auto-tundra.sock

   # If corrupted, remove and restart daemon
   rm /tmp/auto-tundra.sock
   pkill at-daemon
   at-daemon &
   ```

3. **Test IPC communication:**
   ```bash
   # Enable IPC debug logging
   export RUST_LOG=at_bridge::ipc=debug,at_daemon::ipc=debug

   # Watch for IPC messages
   tail -f ~/.auto-tundra/logs/daemon.log | grep IPC
   ```

4. **Check file descriptor limits:**
   ```bash
   # Check current limits
   ulimit -n

   # Increase if needed (requires restart)
   ulimit -n 4096

   # Check daemon's open file descriptors
   lsof -p $(pgrep at-daemon) | wc -l
   ```

**Prevention:**
- Monitor daemon health with systemd or supervisor
- Configure daemon auto-restart on crash
- Set adequate file descriptor limits (`ulimit -n 4096`)
- Implement IPC connection retry logic in `at-bridge`
- Use structured logging to diagnose IPC failures

---

## 🗄️ Dolt Database Configuration

> **Covers:** Port 3306 MySQL conflicts, connection failures, database migration issues, and ConfigError handling.

### Overview

Auto-Tundra uses Dolt (Git for data) for versioned storage. Dolt runs on port 3306 by default, which conflicts with MySQL. Connection failures and configuration errors are common during setup.

### Dolt Architecture

```
┌─────────────┐
│  at-daemon  │
└──────┬──────┘
       │ SQL Connection
┌──────▼──────────┐
│  Dolt Server    │  ← Port 3306 (default)
│  (dolt sql-srv) │
└──────┬──────────┘
       │
┌──────▼──────────┐
│  Dolt Database  │  ← Versioned data store
│  (~/.dolt/)     │
└─────────────────┘
```

### Common Errors

**This section will be populated with specific error patterns in subtask-1-5:**
- Port 3306 conflicts with MySQL
- Connection refused errors
- ConfigError (missing/invalid configuration)
- Database migration failures
- Permission issues

*→ See [Subtask 1-5](../.auto-claude/specs/010-add-troubleshooting-guide-for-common-runtime-error/implementation_plan.json) for implementation details.*

---

## 🚦 Rate Limiting & Circuit Breakers

> **Covers:** RateLimitError::Exceeded, CircuitBreakerError::Open, token bucket exhaustion, failure thresholds, and state transitions.

### Overview

Auto-Tundra implements rate limiting (token bucket) and circuit breakers to prevent API abuse and fail fast during provider outages. Understanding these protective mechanisms helps diagnose and resolve "Too Many Requests" and "Service Unavailable" errors.

### Protection Architecture

```
┌─────────────┐
│  Request    │
└──────┬──────┘
       │
┌──────▼──────────────┐
│  Rate Limiter       │  ← Token bucket, refill rate
│  (rate_limiter.rs)  │
└──────┬──────────────┘
       │
┌──────▼──────────────┐
│  Circuit Breaker    │  ← Open/Closed/HalfOpen
│ (circuit_breaker.rs)│
└──────┬──────────────┘
       │
┌──────▼──────────┐
│  LLM Provider   │
└─────────────────┘
```

### State Machine

**This section will be populated with specific error patterns in subtask-1-6:**
- RateLimitError::Exceeded (retry_after timing)
- CircuitBreakerError::Open (failure threshold reached)
- State transitions (Closed → Open → HalfOpen → Closed)
- Recovery timeout and reset conditions

*→ See [Subtask 1-6](../.auto-claude/specs/010-add-troubleshooting-guide-for-common-runtime-error/implementation_plan.json) for implementation details.*

---

## 🔍 Diagnostics & Logging

> **Covers:** RUST_LOG configuration, default log levels, crate-specific filtering, and diagnostic output for troubleshooting.

### Overview

Auto-Tundra uses Rust's `tracing` ecosystem for structured logging. Proper RUST_LOG configuration is essential for diagnosing issues. This section covers log levels, crate-specific filtering, and how to capture diagnostic output.

### Logging Architecture

```
┌─────────────────┐
│  Application    │
└────────┬────────┘
         │ tracing macros
┌────────▼─────────┐
│  at-telemetry    │  ← Logging setup
│  (logging.rs)    │
└────────┬─────────┘
         │
    ┌────┴────┬──────────┬─────────┐
    │         │          │         │
┌───▼───┐ ┌──▼───┐ ┌────▼─────┐ ┌─▼─────┐
│stdout │ │File  │ │Journald  │ │Jaeger │
│       │ │logs  │ │(systemd) │ │(trace)│
└───────┘ └──────┘ └──────────┘ └───────┘
```

### RUST_LOG Configuration

**This section will be populated with specific patterns in subtask-1-7:**
- Default log levels (`info,at_daemon=debug`)
- Crate-specific filtering
- Module-level granularity
- Performance impact of verbose logging
- Log rotation and retention

*→ See [Subtask 1-7](../.auto-claude/specs/010-add-troubleshooting-guide-for-common-runtime-error/implementation_plan.json) for implementation details.*

---

## 📇 Error Reference Index

> **Comprehensive index of all error types with page references.**

**This section will be populated in subtask-1-8 with a complete index of all 33+ error types found across the workspace.**

### By Category

#### LLM Provider Errors
- HttpError → [LLM Provider Issues](#-llm-provider-issues)
- ApiError → [LLM Provider Issues](#-llm-provider-issues)
- RateLimited → [LLM Provider Issues](#-llm-provider-issues)
- Timeout → [LLM Provider Issues](#-llm-provider-issues)
- ParseError → [LLM Provider Issues](#-llm-provider-issues)
- Unsupported → [LLM Provider Issues](#-llm-provider-issues)

#### PTY Session Errors
- AtCapacity → [PTY Session Management](#-pty-session-management)
- HandleNotFound → [PTY Session Management](#-pty-session-management)
- SpawnFailed → [PTY Session Management](#-pty-session-management)

#### WebSocket Errors
- TransportError → [WebSocket Connections](#-websocket-connections)
- IpcError → [WebSocket Connections](#-websocket-connections)

#### Database Errors
- ConfigError → [Dolt Database Configuration](#-dolt-database-configuration)

#### Rate Limiting Errors
- RateLimitError::Exceeded → [Rate Limiting & Circuit Breakers](#-rate-limiting--circuit-breakers)
- CircuitBreakerError::Open → [Rate Limiting & Circuit Breakers](#-rate-limiting--circuit-breakers)

*→ Complete index will be added in [Subtask 1-8](../.auto-claude/specs/010-add-troubleshooting-guide-for-common-runtime-error/implementation_plan.json).*

---

## 🆘 Getting Additional Help

If you've tried the solutions in this guide and still need help:

1. **Check system status:**
   ```bash
   # Daemon status
   pgrep -fl at-daemon

   # Recent logs
   tail -50 ~/.auto-tundra/logs/daemon.log

   # System resources
   top -l 1 | grep -A 5 "CPU usage"
   ```

2. **Enable verbose logging:**
   ```bash
   export RUST_LOG=trace,at_daemon=trace,at_intelligence=debug
   at-daemon
   ```

3. **Collect diagnostics:**
   ```bash
   # Create diagnostic bundle
   mkdir -p /tmp/auto-tundra-diagnostics
   cp ~/.auto-tundra/logs/*.log /tmp/auto-tundra-diagnostics/
   env | grep -E '(RUST_LOG|ANTHROPIC|OPENROUTER)' > /tmp/auto-tundra-diagnostics/env.txt
   ps aux | grep -E 'at-(daemon|bridge)' > /tmp/auto-tundra-diagnostics/processes.txt
   ```

4. **Report an issue:**
   - Open an issue at the project repository
   - Include diagnostic bundle (redact sensitive data)
   - Describe the symptoms, steps to reproduce, and expected behavior

---

**Next Steps:**
- [Project Handbook](./PROJECT_HANDBOOK.md) - Architecture and component details
- [Getting Started](../GETTING_STARTED.md) - Initial setup and configuration
- [README](../README.md) - Project overview
