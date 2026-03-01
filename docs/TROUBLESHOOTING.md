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

### Common Errors

**This section will be populated with specific error patterns in subtask-1-3:**
- AtCapacity (pool limits reached)
- HandleNotFound (unreleased handles)
- SpawnFailed (process creation failures)
- Zombie processes (orphaned PTY sessions)
- Session leaks (unreleased resources)

*→ See [Subtask 1-3](../.auto-claude/specs/010-add-troubleshooting-guide-for-common-runtime-error/implementation_plan.json) for implementation details.*

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

**This section will be populated with specific error patterns in subtask-1-4:**
- Connection establishment and handshake
- Heartbeat interval (30 seconds)
- Reconnection grace period (10 seconds)
- Idle timeout (5 minutes)
- TransportError (network failures)
- IpcError (daemon communication failures)

*→ See [Subtask 1-4](../.auto-claude/specs/010-add-troubleshooting-guide-for-common-runtime-error/implementation_plan.json) for implementation details.*

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
