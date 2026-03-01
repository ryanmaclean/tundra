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

Before diving into specific issues, try these common solutions organized by symptom:

### 🔍 First Steps (Always Start Here)
- [ ] **Enable debug logging:** `export RUST_LOG=info,at_daemon=debug,at_intelligence=debug`
- [ ] **Check daemon status:** `pgrep -fl at-daemon` (should show running process)
- [ ] **Verify configuration:** `ls -la ~/.auto-tundra/config/` (config files exist?)
- [ ] **Review recent errors:** `tail -50 ~/.auto-tundra/logs/daemon.log | grep -i error`
- [ ] **Check disk space:** `df -h ~/.auto-tundra` (at least 1GB free?)
- [ ] **Verify permissions:** `ls -la ~/.auto-tundra/` (directories readable/writable?)

### 🌐 Network & API Issues
**Symptoms:** Connection failures, timeouts, 401/403 errors, "API error" messages

- [ ] **Test network connectivity:**
  ```bash
  ping -c 3 api.anthropic.com
  curl -I https://api.anthropic.com/v1/messages
  ```
- [ ] **Verify API keys are set:**
  ```bash
  echo $ANTHROPIC_API_KEY | grep -o "^sk-ant"  # Should output: sk-ant
  echo $OPENROUTER_API_KEY | grep -o "^sk-or"  # Should output: sk-or
  ```
- [ ] **Test API key validity:**
  ```bash
  curl https://api.anthropic.com/v1/messages \
    -H "x-api-key: $ANTHROPIC_API_KEY" \
    -H "anthropic-version: 2023-06-01" \
    -H "content-type: application/json" \
    -d '{"model":"claude-sonnet-4-20250514","max_tokens":10,"messages":[{"role":"user","content":"test"}]}'
  ```
- [ ] **Check proxy settings:** `env | grep -i proxy`
- [ ] **Bypass firewall temporarily:** Test from different network
- [ ] **Switch to fallback provider:** Edit `~/.auto-tundra/config/profiles.toml`

### 💾 Database & Session Issues
**Symptoms:** "ConfigError", "database connection failed", "session not found", PTY spawn failures

- [ ] **Check Dolt is running:** `pgrep -fl dolt-sql-server`
- [ ] **Verify database port:** `lsof -i :3306` (should show dolt-sql-server)
- [ ] **Test database connection:**
  ```bash
  mysql -h 127.0.0.1 -P 3306 -u root --protocol=tcp -e "SELECT 1"
  ```
- [ ] **Check database directory:** `ls ~/.auto-tundra/data/dolt/`
- [ ] **Verify PTY pool capacity:** Check logs for "AtCapacity" errors
- [ ] **Release zombie sessions:**
  ```bash
  # List active sessions
  ps aux | grep -E 'zsh|bash' | grep auto-tundra
  # Kill stale sessions (use with caution)
  pkill -f 'auto-tundra.*zsh'
  ```

### ⚡ Performance & Resource Issues
**Symptoms:** Slow responses, timeouts, high CPU/memory, "quota exceeded", circuit breaker open

- [ ] **Check system resources:**
  ```bash
  top -l 1 | grep -A 5 "CPU usage"
  ps aux | grep at-daemon  # Check memory usage
  ```
- [ ] **Monitor rate limits:**
  ```bash
  tail -f ~/.auto-tundra/logs/daemon.log | grep -E 'rate.*limit|quota'
  ```
- [ ] **Check circuit breaker status:**
  ```bash
  tail -f ~/.auto-tundra/logs/daemon.log | grep circuit
  ```
- [ ] **Wait for recovery:** Circuit breakers auto-reset after 30s
- [ ] **Review token usage:** `cat ~/.auto-tundra/data/profile_usage.json`
- [ ] **Reduce concurrency:** Lower `max_concurrent_requests` in config
- [ ] **Increase timeouts:** Adjust `timeout_seconds` in `~/.auto-tundra/config/harness.toml`

### 🔧 Process & State Issues
**Symptoms:** "zombie processes", "port already in use", daemon won't start, stale locks

- [ ] **Check for port conflicts:**
  ```bash
  lsof -i :3306   # Dolt database port
  lsof -i :8080   # Common API port (if applicable)
  ```
- [ ] **Kill zombie processes:**
  ```bash
  # List all auto-tundra processes
  pgrep -fl 'at-'
  # Clean shutdown
  pkill at-daemon
  # Force kill if needed (use with caution)
  pkill -9 -f 'at-'
  ```
- [ ] **Remove stale locks:**
  ```bash
  rm -f ~/.auto-tundra/daemon.lock
  rm -f ~/.auto-tundra/data/dolt/*.lock
  ```
- [ ] **Clean restart:**
  ```bash
  pkill at-daemon
  sleep 2
  export RUST_LOG=info,at_daemon=debug
  at-daemon
  ```
- [ ] **Check for conflicting instances:** Only one daemon should run

### 🔐 Authentication & Authorization Issues
**Symptoms:** "OAuth error", "token expired", "permission denied", GitHub/GitLab integration failures

- [ ] **Check OAuth token validity:**
  ```bash
  # GitHub
  curl -H "Authorization: token $GITHUB_TOKEN" https://api.github.com/user
  # GitLab
  curl -H "PRIVATE-TOKEN: $GITLAB_TOKEN" https://gitlab.com/api/v4/user
  ```
- [ ] **Refresh OAuth tokens:** Re-authenticate via UI or CLI
- [ ] **Verify integration permissions:** Check scopes in provider dashboard
- [ ] **Check token file permissions:**
  ```bash
  ls -la ~/.auto-tundra/data/oauth_tokens.json
  # Should be: -rw------- (600)
  ```

### 📋 Configuration Issues
**Symptoms:** "ConfigError", "missing field", "invalid configuration", startup failures

- [ ] **Validate configuration files:**
  ```bash
  # Check for syntax errors
  cat ~/.auto-tundra/config/profiles.toml
  cat ~/.auto-tundra/config/harness.toml
  ```
- [ ] **Reset to defaults:** Backup and remove `~/.auto-tundra/config/`
- [ ] **Check required fields:** Compare against documentation
- [ ] **Verify file permissions:** Config files should be readable (644 or 600)

### 🚨 When Nothing Works
- [ ] **Enable trace logging:**
  ```bash
  export RUST_LOG=trace,at_daemon=trace,at_intelligence=debug
  at-daemon > /tmp/at-daemon-debug.log 2>&1
  ```
- [ ] **Collect full diagnostics:**
  ```bash
  mkdir -p /tmp/auto-tundra-diagnostics
  cp -r ~/.auto-tundra/logs /tmp/auto-tundra-diagnostics/
  cp -r ~/.auto-tundra/config /tmp/auto-tundra-diagnostics/
  env | grep -E '(RUST_LOG|API_KEY|TOKEN)' > /tmp/auto-tundra-diagnostics/env.txt
  ps aux | grep at- > /tmp/auto-tundra-diagnostics/processes.txt
  ```
- [ ] **Nuclear option - fresh start (CAUTION):**
  ```bash
  # Backup first!
  mv ~/.auto-tundra ~/.auto-tundra.backup.$(date +%Y%m%d)
  # Reinstall and reconfigure
  ```
- [ ] **Report bug with diagnostics:** Include logs, config (redact sensitive data)

### 📊 Quick Health Check Command
Run this one-liner for instant system status:
```bash
echo "=== Daemon ===" && pgrep -fl at-daemon && \
echo "=== Database ===" && pgrep -fl dolt-sql-server && \
echo "=== API Keys ===" && env | grep -E '(ANTHROPIC|OPENROUTER)_API_KEY' | cut -d= -f1 && \
echo "=== Disk Space ===" && df -h ~/.auto-tundra && \
echo "=== Recent Errors ===" && tail -20 ~/.auto-tundra/logs/daemon.log | grep -i error
```

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

#### Port 3306 Conflict (MySQL Already Running)

**Error Message:** `Address already in use (os error 48)` or `Cannot bind to port 3306`

**Symptoms:**
- Dolt server fails to start with "address already in use" error
- Auto-Tundra daemon startup hangs or fails during database initialization
- `lsof -i :3306` shows MySQL (`mysqld`) process bound to port
- Database connection attempts timeout or return connection refused
- Logs show `failed to start Dolt SQL server` errors

**Causes:**
- MySQL server running on default port 3306
- Previous Dolt instance not properly shut down
- Another database service (MariaDB, Percona) using port 3306
- Port forwarding or tunneling conflict
- System service manager (systemd, launchd) auto-starting MySQL

**Solutions:**

1. **Stop conflicting MySQL service:**
   ```bash
   # On macOS
   brew services stop mysql
   # or
   sudo launchctl unload /Library/LaunchDaemons/com.mysql.mysql.plist

   # On Linux (systemd)
   sudo systemctl stop mysql
   sudo systemctl disable mysql

   # Verify port is free
   lsof -i :3306
   ```

2. **Configure Dolt to use different port:**
   ```bash
   # Edit ~/.auto-tundra/config.toml
   [dolt]
   port = 3307  # or any free port (3307, 13306, etc.)
   dir = "~/.auto-tundra/dolt"
   auto_commit = false
   ```

   Then restart the daemon:
   ```bash
   pkill at-daemon
   at-daemon
   ```

3. **Find and kill rogue Dolt process:**
   ```bash
   # Find Dolt process
   ps aux | grep dolt
   lsof -i :3306

   # Kill by PID
   kill -9 <PID>

   # Or kill all Dolt processes
   pkill -9 dolt
   ```

4. **Check for port forwarding conflicts:**
   ```bash
   # List all listening ports
   netstat -an | grep LISTEN | grep 3306
   # or
   lsof -iTCP -sTCP:LISTEN | grep 3306

   # Check SSH tunnels
   ps aux | grep ssh | grep 3306
   ```

**Prevention:**
- Configure Dolt to use non-standard port (3307, 13306) in `config.toml`
- Disable MySQL auto-start: `sudo systemctl disable mysql`
- Use Docker for Dolt with port mapping: `-p 13306:3306`
- Document port assignments in team wiki/README
- Add port check to daemon startup script

---

#### Connection Refused: Dolt Not Running

**Error Message:** `Connection refused (os error 61)` or `Can't connect to MySQL server on '127.0.0.1:3306'`

**Symptoms:**
- Database queries fail with connection refused
- Daemon startup sequence hangs at database initialization
- `telnet localhost 3306` fails immediately
- No `dolt sql-server` process in `ps aux` output
- Logs show repeated connection retry attempts

**Causes:**
- Dolt server never started (installation incomplete)
- Dolt process crashed after startup
- Incorrect host/port in connection string
- Dolt binary not in PATH
- Database directory not initialized (missing `.dolt/` folder)
- Permissions prevent Dolt from binding to port

**Solutions:**

1. **Verify Dolt installation:**
   ```bash
   # Check Dolt is installed
   which dolt
   dolt version

   # If not found, install:
   # macOS
   brew install dolt
   # Linux
   curl -L https://github.com/dolthub/dolt/releases/latest/download/install.sh | bash
   ```

2. **Initialize Dolt database:**
   ```bash
   # Navigate to database directory
   cd ~/.auto-tundra/dolt

   # Initialize if not exists
   dolt init

   # Configure user (required for commits)
   dolt config --global --add user.name "Auto Tundra"
   dolt config --global --add user.email "auto@tundra.local"
   ```

3. **Start Dolt server manually:**
   ```bash
   # Start in foreground for debugging
   cd ~/.auto-tundra/dolt
   dolt sql-server --host 0.0.0.0 --port 3306 --user root

   # Or start in background
   dolt sql-server --host 0.0.0.0 --port 3306 --user root &

   # Test connection
   mysql -h 127.0.0.1 -P 3306 -u root
   ```

4. **Check daemon is starting Dolt:**
   ```bash
   # Enable debug logging
   export RUST_LOG=at_daemon=debug,at_core=debug

   # Watch startup sequence
   at-daemon 2>&1 | grep -i dolt

   # Look for Dolt initialization errors
   tail -f ~/.auto-tundra/logs/daemon.log | grep -i dolt
   ```

5. **Verify connection configuration:**
   ```bash
   # Check configured port matches running server
   grep -A 3 "\[dolt\]" ~/.auto-tundra/config.toml

   # Test connection with mysql client
   mysql -h 127.0.0.1 -P 3306 -u root -e "SHOW DATABASES;"
   ```

**Prevention:**
- Add Dolt service health check to daemon startup
- Configure auto-restart for Dolt process (systemd, supervisor)
- Document Dolt initialization in onboarding/setup guide
- Use connection retry with exponential backoff in daemon
- Monitor Dolt process with systemd or launchd

---

#### ConfigError: Missing or Invalid Configuration

**Error Message:** `ConfigError::Validation("invalid dolt configuration")` or `ConfigError::Parse("missing field 'dir'")`

**Symptoms:**
- Daemon fails to start with configuration validation error
- `config.toml` missing required `[dolt]` section
- Invalid port number (0, negative, >65535)
- Directory path contains invalid characters or doesn't exist
- TOML syntax errors in configuration file

**Causes:**
- Fresh installation without config file initialization
- Manual editing introduced TOML syntax errors
- Missing required fields: `dir`, `port`
- Invalid data types (string for port, number for dir)
- Path expansion issues (unresolved `~`, invalid `$VAR`)
- Incompatible config version after upgrade

**Solutions:**

1. **Validate configuration syntax:**
   ```bash
   # Check for TOML syntax errors
   cat ~/.auto-tundra/config.toml

   # Use online validator if needed
   # https://www.toml-lint.com/
   ```

2. **Create minimal valid configuration:**
   ```bash
   # Create config directory if missing
   mkdir -p ~/.auto-tundra

   # Create minimal config.toml
   cat > ~/.auto-tundra/config.toml << 'EOF'
[general]
project_name = "auto-tundra"
log_level = "info"

[dolt]
dir = "~/.auto-tundra/dolt"
port = 3306
auto_commit = false
EOF

   # Create Dolt database directory
   mkdir -p ~/.auto-tundra/dolt
   ```

3. **Fix common validation errors:**
   ```toml
   # ❌ WRONG - port as string
   [dolt]
   port = "3306"

   # ✅ CORRECT - port as integer
   [dolt]
   port = 3306

   # ❌ WRONG - missing required field
   [dolt]
   port = 3306

   # ✅ CORRECT - all required fields
   [dolt]
   dir = "~/.auto-tundra/dolt"
   port = 3306
   auto_commit = false
   ```

4. **Check file permissions:**
   ```bash
   # Verify config file is readable
   ls -la ~/.auto-tundra/config.toml

   # Should be -rw-r--r-- or -rw-------
   chmod 644 ~/.auto-tundra/config.toml

   # Verify directory is writable
   test -w ~/.auto-tundra && echo "Writable" || echo "Not writable"
   ```

5. **Test configuration loading:**
   ```bash
   # Enable config debug logging
   export RUST_LOG=at_core::config=debug

   # Run daemon to see config loading
   at-daemon 2>&1 | head -50
   ```

**Prevention:**
- Provide `config.toml.example` with all valid fields
- Validate config on save with `Config::validate()`
- Use config migration scripts for version upgrades
- Document all required fields in configuration reference
- Add `--validate-config` flag to daemon for dry-run testing

---

#### Database Migration Failures

**Error Message:** `Migration failed: <sql error>` or `Schema version mismatch: expected v5, found v3`

**Symptoms:**
- Daemon startup fails after upgrade with migration error
- SQL schema incompatible with code expectations
- Missing tables or columns in database queries
- "Table doesn't exist" errors for expected tables
- Version mismatch between Dolt database and application code

**Causes:**
- Upgrading Auto-Tundra skipped intermediate versions
- Manual database modifications outside migration system
- Interrupted migration (daemon killed mid-migration)
- Corrupt Dolt database (disk full, power loss)
- Migration rollback not implemented for failed upgrade

**Solutions:**

1. **Check migration status:**
   ```bash
   # Connect to Dolt database
   mysql -h 127.0.0.1 -P 3306 -u root

   # Check for schema_migrations table
   SHOW TABLES;
   SELECT * FROM schema_migrations ORDER BY version DESC;

   # Verify expected tables exist
   SHOW TABLES;
   ```

2. **Backup database before migration:**
   ```bash
   # Create Dolt commit before upgrade
   cd ~/.auto-tundra/dolt
   dolt add .
   dolt commit -m "Pre-upgrade backup $(date +%Y%m%d)"

   # Or export SQL dump
   mysqldump -h 127.0.0.1 -P 3306 -u root --all-databases > backup.sql
   ```

3. **Rollback to previous version:**
   ```bash
   # Using Dolt version control
   cd ~/.auto-tundra/dolt
   dolt log  # Find previous commit
   dolt reset --hard <commit-hash>

   # Restart daemon with previous version
   pkill at-daemon
   at-daemon
   ```

4. **Force re-run migrations:**
   ```bash
   # WARNING: This may cause data loss!
   # Delete migration tracking table
   mysql -h 127.0.0.1 -P 3306 -u root -e "DROP TABLE IF EXISTS schema_migrations;"

   # Restart daemon to re-run migrations
   pkill at-daemon
   export RUST_LOG=at_daemon=debug
   at-daemon
   ```

5. **Manual migration repair:**
   ```bash
   # If specific migration failed, apply missing changes manually
   mysql -h 127.0.0.1 -P 3306 -u root

   # Example: add missing column
   ALTER TABLE tasks ADD COLUMN priority INT DEFAULT 0;

   # Update migration version
   INSERT INTO schema_migrations (version, applied_at) VALUES (5, NOW());
   ```

**Prevention:**
- Always backup Dolt database before upgrades: `dolt commit -m "pre-upgrade"`
- Test migrations on staging/dev environment first
- Implement rollback logic for all migrations
- Use transactional DDL (Dolt supports this)
- Document migration procedures in UPGRADING.md
- Add migration dry-run mode for validation

---

#### Permission Denied: Database Access Issues

**Error Message:** `Permission denied (os error 13)` or `Access denied for user 'root'@'localhost'`

**Symptoms:**
- Cannot create or write to database directory
- Dolt fails to initialize in `~/.auto-tundra/dolt`
- Connection succeeds but queries fail with permission errors
- File ownership issues in Dolt directory
- SELinux or AppArmor blocking database access

**Causes:**
- Insufficient filesystem permissions on `~/.auto-tundra/dolt`
- Database directory owned by different user (root vs regular user)
- SELinux/AppArmor policies blocking Dolt binary
- Read-only filesystem (mounted partition, container)
- Disk quota exceeded for user
- Incorrect umask preventing file creation

**Solutions:**

1. **Check and fix directory permissions:**
   ```bash
   # Check ownership and permissions
   ls -la ~/.auto-tundra/
   ls -la ~/.auto-tundra/dolt/

   # Fix ownership (replace 'username' with your user)
   sudo chown -R $USER:$USER ~/.auto-tundra/

   # Fix permissions
   chmod 755 ~/.auto-tundra/
   chmod -R 755 ~/.auto-tundra/dolt/
   ```

2. **Verify disk space and quotas:**
   ```bash
   # Check disk space
   df -h ~/.auto-tundra/

   # Check user quota (if enabled)
   quota -s

   # Check inode usage (can run out even with disk space)
   df -i ~/.auto-tundra/
   ```

3. **Check SELinux/AppArmor policies:**
   ```bash
   # Check SELinux status
   sestatus
   getenforce

   # View Dolt denials
   sudo ausearch -m avc -ts recent | grep dolt

   # Temporarily disable for testing (NOT for production)
   sudo setenforce 0

   # Or create proper SELinux policy
   sudo semanage fcontext -a -t user_home_t "~/.auto-tundra(/.*)?"
   sudo restorecon -Rv ~/.auto-tundra/
   ```

4. **Fix file ownership conflicts:**
   ```bash
   # If Dolt directory owned by root (ran with sudo accidentally)
   sudo chown -R $USER:$USER ~/.auto-tundra/dolt/

   # If files have wrong group
   chgrp -R $USER ~/.auto-tundra/dolt/
   ```

5. **Test with explicit permissions:**
   ```bash
   # Create test database in /tmp (always writable)
   mkdir -p /tmp/test-dolt
   cd /tmp/test-dolt
   dolt init
   dolt sql-server --port 3307 &

   # If works, permissions issue in ~/.auto-tundra/
   # If fails, Dolt binary or SELinux issue
   ```

**Prevention:**
- Document required permissions in installation guide
- Add permission check to daemon startup (warn if wrong)
- Use restrictive but functional permissions: `755` for dirs, `644` for files
- Avoid running daemon with `sudo` (creates ownership issues)
- Configure SELinux/AppArmor policies in packaging
- Test installation as non-root user in CI/CD

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

### Common Errors

#### RateLimitError::Exceeded: Token Bucket Exhaustion

**Error Message:** `rate limit exceeded for key '<key>' – retry after <duration>`

**Symptoms:**
- Error includes exact `retry_after` duration (e.g., "retry after 2.5s")
- Affects specific keys: `global`, `user:<id>`, `endpoint:<name>`
- May appear intermittently during traffic spikes
- Logs show: `rate limit exceeded` with key and retry_after

**Causes:**
- Request rate exceeds configured tokens_per_second limit
- Token bucket depleted (no available tokens)
- Burst capacity (max_burst) exhausted during spike
- Multiple rate limit tiers triggered (global, per-user, per-endpoint)
- High-cost operations consuming multiple tokens

**How Rate Limiting Works:**

Auto-Tundra uses a **token bucket algorithm** with automatic refill:

1. **Token Bucket Parameters:**
   - `tokens_per_second`: Refill rate (e.g., 10 tokens/sec)
   - `max_burst`: Maximum bucket capacity (e.g., 100 tokens)
   - Tokens refill continuously based on elapsed time
   - Each request consumes 1 token (or custom cost)

2. **Multi-Tier Enforcement:**
   ```
   Request → Global Limit → Per-User Limit → Per-Endpoint Limit → Provider
   ```
   - **Global:** Protects entire system (e.g., 1000 req/min)
   - **Per-User:** Prevents single-user abuse (e.g., 100 req/min)
   - **Per-Endpoint:** Protects specific endpoints (e.g., 50 req/min)
   - First tier to reject returns `RateLimitError::Exceeded`

3. **Retry Timing Calculation:**
   - When tokens insufficient, calculates wait time: `deficit / tokens_per_second`
   - Example: Need 1 token, have 0.5 tokens, rate is 10/sec → wait 0.05s
   - `retry_after` is exact minimum wait, not a suggestion

**Solutions:**

1. **Respect retry_after duration:**
   ```rust
   // Automatic retry in at-intelligence layer
   match llm_call().await {
       Err(RateLimitError::Exceeded { retry_after, .. }) => {
           tokio::time::sleep(retry_after).await;
           llm_call().await // Retry after waiting
       }
   }
   ```

2. **Check remaining tokens:**
   ```bash
   # Enable rate limiter debug logging
   export RUST_LOG=at_harness::rate_limiter=debug

   # Look for token bucket state in logs
   tail -f ~/.auto-tundra/logs/daemon.log | grep "tokens remaining"
   ```

3. **Adjust rate limits in configuration:**
   ```toml
   # ~/.auto-tundra/config/harness.toml
   [rate_limit.global]
   tokens_per_second = 100.0
   max_burst = 200.0

   [rate_limit.per_user]
   tokens_per_second = 10.0
   max_burst = 50.0

   [rate_limit.per_endpoint]
   tokens_per_second = 5.0
   max_burst = 20.0
   ```

4. **Use cost-based limiting for expensive operations:**
   - Large context requests may consume multiple tokens
   - Streaming responses may have higher cost
   - Check logs for `cost=` parameter in rate limit messages

5. **Identify which tier is limiting:**
   ```bash
   # Check logs for rate limit key
   grep "rate limit exceeded" ~/.auto-tundra/logs/daemon.log

   # Key patterns:
   # - "key `global`" → Global limit hit
   # - "key `user:<uuid>`" → Per-user limit hit
   # - "key `endpoint:chat`" → Per-endpoint limit hit
   ```

**Prevention:**
- Configure `max_burst` to handle traffic spikes (2-5x tokens_per_second recommended)
- Use exponential backoff for retries instead of fixed delays
- Implement client-side request queuing for high-volume operations
- Monitor token bucket state with `RUST_LOG=at_harness=debug`
- Spread large batch operations over time instead of bursting

---

#### CircuitBreakerError::Open: Service Protection Active

**Error Message:** `circuit is open – refusing call`

**Symptoms:**
- All requests to provider immediately rejected (no network call)
- Error appears after repeated failures (5 consecutive by default)
- Logs show: `circuit breaker transitioning Closed -> Open`
- Requests fail instantly without retry attempts
- State persists for timeout period (60s by default)

**Causes:**
- Consecutive failures reached `failure_threshold` (default: 5)
- Provider experiencing outage or high error rate
- Network connectivity issues causing repeated timeouts
- API key invalidation or account suspension
- Request timeout exceeded `call_timeout` (default: 30s) multiple times

**How Circuit Breaker Works:**

Auto-Tundra implements a **three-state circuit breaker** with automatic recovery:

1. **Closed (Normal Operation):**
   - All requests pass through to provider
   - Tracks consecutive failures
   - On success: resets failure_count to 0
   - On failure: increments failure_count
   - Transitions to **Open** when `failure_count >= 5`

2. **Open (Service Protection):**
   - Immediately rejects all requests with `CircuitBreakerError::Open`
   - No network calls made (fail fast)
   - Tracks time since last failure
   - After `timeout` (60s): transitions to **HalfOpen**
   - Prevents cascading failures and resource exhaustion

3. **HalfOpen (Testing Recovery):**
   - Allows limited requests through to test provider health
   - On success: increments success_count
   - On failure: immediately transitions back to **Open**
   - After `success_threshold` (2) consecutive successes: transitions to **Closed**
   - Acts as a probe to verify provider recovery

**State Transition Diagram:**

```
         failure_count >= 5
    ┌────────────────────────┐
    │                        ▼
┌───┴────┐              ┌──────┐
│ Closed │              │ Open │
│        │◄──┐          │      │
└────────┘   │          └───┬──┘
    ▲        │              │
    │        │              │ timeout (60s)
    │        │              │
    │        │              ▼
    │    success_count >= 2
    │        │          ┌──────────┐
    └────────┴──────────┤ HalfOpen │
             failure    │          │
                        └──────────┘
```

**Default Configuration:**
- `failure_threshold`: 5 consecutive failures
- `success_threshold`: 2 consecutive successes (in HalfOpen)
- `timeout`: 60 seconds (Open → HalfOpen)
- `call_timeout`: 30 seconds per individual request

**Solutions:**

1. **Wait for automatic recovery:**
   ```bash
   # Circuit will automatically transition after timeout
   # Open (60s wait) → HalfOpen (test) → Closed (if 2 successes)

   # Monitor state transitions in logs
   tail -f ~/.auto-tundra/logs/daemon.log | grep "circuit breaker transitioning"
   ```

   Expected log sequence:
   ```
   circuit breaker transitioning Closed -> Open
   # ... 60 seconds later ...
   circuit breaker transitioning Open -> HalfOpen
   # ... after 2 successful calls ...
   circuit breaker transitioning HalfOpen -> Closed
   ```

2. **Check circuit breaker state:**
   ```bash
   # Enable circuit breaker debug logging
   export RUST_LOG=at_harness::circuit_breaker=info

   # Check current state and failure count
   grep "circuit" ~/.auto-tundra/logs/daemon.log | tail -20
   ```

3. **Manually reset circuit (advanced):**
   ```bash
   # Restart daemon to reset all circuit breakers
   pkill at-daemon && at-daemon

   # Or send SIGHUP for graceful reload (if implemented)
   pkill -HUP at-daemon
   ```

4. **Configure circuit breaker thresholds:**
   ```toml
   # ~/.auto-tundra/config/harness.toml
   [circuit_breaker]
   failure_threshold = 5       # Consecutive failures before opening
   success_threshold = 2       # Consecutive successes before closing
   timeout_secs = 60           # Seconds to wait in Open state
   call_timeout_secs = 30      # Timeout per individual call
   ```

5. **Investigate root cause during Open state:**
   ```bash
   # Check provider API status
   curl -I https://api.anthropic.com/v1/messages

   # Verify API key
   echo $ANTHROPIC_API_KEY

   # Test network connectivity
   ping -c 3 api.anthropic.com

   # Check for firewall blocks
   sudo iptables -L | grep -i drop
   ```

6. **Failover to alternative provider:**
   - Circuit breaker operates per-provider
   - Configure multiple providers in `~/.auto-tundra/config/profiles.toml`
   - at-intelligence layer automatically fails over to next available provider
   - Each provider has independent circuit breaker state

**Prevention:**
- Set appropriate `call_timeout` for your network conditions (increase if slow connection)
- Reduce `failure_threshold` to open circuit faster during outages (fail fast)
- Increase `timeout` if provider recovery typically takes longer than 60s
- Configure multiple providers for automatic failover
- Monitor provider status pages proactively
- Implement retry logic with exponential backoff at application layer

---

#### CircuitBreakerError::Timeout: Request Deadline Exceeded

**Error Message:** `call timed out after <duration>`

**Symptoms:**
- Individual requests exceed `call_timeout` (default: 30s)
- Contributes to circuit breaker failure count
- May trigger circuit opening after repeated timeouts
- Different from rate limiting or network errors

**Causes:**
- Large context windows causing slow provider responses
- Provider experiencing degraded performance
- Network latency or slow connection
- Complex multi-tool agent operations
- Streaming responses buffering delays

**Solutions:**

1. **Increase call_timeout for slow operations:**
   ```toml
   # ~/.auto-tundra/config/harness.toml
   [circuit_breaker]
   call_timeout_secs = 60  # Increase from 30s default
   ```

2. **Reduce request complexity:**
   - Use smaller context windows
   - Break large operations into smaller chunks
   - Disable unnecessary tool calls

3. **Check network performance:**
   ```bash
   # Measure latency to provider
   curl -w "@-" -o /dev/null -s https://api.anthropic.com/v1/messages <<'EOF'
   time_namelookup:  %{time_namelookup}s
   time_connect:     %{time_connect}s
   time_starttransfer: %{time_starttransfer}s
   time_total:       %{time_total}s
   EOF
   ```

4. **Monitor provider latency:**
   ```bash
   # Enable timing logs
   export RUST_LOG=at_harness=debug

   # Check request duration in logs
   grep "call duration" ~/.auto-tundra/logs/daemon.log
   ```

**Prevention:**
- Set `call_timeout` appropriate for your use case (streaming may need 60s+)
- Use timeout safety margin (timeout > 95th percentile latency)
- Monitor provider performance degradation trends
- Implement client-side timeout with retry for critical operations

---

### Recovery Procedures

#### Automatic Recovery (Recommended)

The circuit breaker handles recovery automatically:

1. **Detection Phase (Closed → Open):**
   - System detects 5 consecutive failures
   - Circuit opens immediately
   - All requests fail fast with `CircuitBreakerError::Open`
   - Logs: `circuit breaker transitioning Closed -> Open`

2. **Waiting Phase (Open):**
   - Circuit remains open for 60 seconds
   - No requests sent to provider (fail fast)
   - Preserves system resources
   - Prevents cascading failures

3. **Testing Phase (Open → HalfOpen):**
   - After 60s timeout, circuit transitions to HalfOpen
   - Allows probe requests through
   - Logs: `circuit breaker transitioning Open -> HalfOpen`
   - System tests provider health

4. **Recovery Phase (HalfOpen → Closed):**
   - If 2 consecutive requests succeed: circuit closes
   - Logs: `circuit breaker transitioning HalfOpen -> Closed`
   - Normal operation resumes
   - Failure count resets to 0

5. **Re-trigger Protection (HalfOpen → Open):**
   - Any failure during HalfOpen immediately reopens circuit
   - Logs: `circuit breaker transitioning HalfOpen -> Open (failure during probe)`
   - Returns to 60s waiting phase

#### Manual Recovery (When Needed)

If automatic recovery fails or you need immediate reset:

1. **Restart the daemon:**
   ```bash
   # Graceful shutdown
   pkill at-daemon

   # Restart (resets all circuit breakers to Closed)
   at-daemon
   ```

2. **Fix underlying issue first:**
   - Verify API key is valid
   - Check provider status page
   - Test connectivity manually
   - Review error logs for root cause

3. **Monitor recovery:**
   ```bash
   # Watch circuit breaker state changes
   tail -f ~/.auto-tundra/logs/daemon.log | grep "circuit breaker"

   # Look for successful state transitions
   # Open -> HalfOpen (after 60s)
   # HalfOpen -> Closed (after 2 successes)
   ```

#### Combined Rate Limiting + Circuit Breaker Scenarios

**Scenario 1: Rate limit triggers circuit breaker**
- High request volume hits rate limit repeatedly
- Rate limit errors counted as failures (if not handled)
- After 5 rate limit failures: circuit opens
- **Solution:** Implement retry with backoff for rate limit errors

**Scenario 2: Circuit breaker prevents rate limit recovery**
- Circuit opens due to failures
- Rate limit tokens refill during Open period
- After recovery, full token bucket available
- **Benefit:** Natural rate limiting reset during outages

**Scenario 3: Cascading failures across providers**
- Primary provider circuit opens
- Traffic fails over to secondary provider
- Secondary may hit rate limits from sudden traffic spike
- **Solution:** Configure appropriate per-provider rate limits

#### Health Check Commands

```bash
# Check circuit breaker state
grep "circuit breaker transitioning" ~/.auto-tundra/logs/daemon.log | tail -5

# Check rate limit errors
grep "rate limit exceeded" ~/.auto-tundra/logs/daemon.log | tail -10

# Monitor failure counts
export RUST_LOG=at_harness=debug
tail -f ~/.auto-tundra/logs/daemon.log | grep -E "(failure_count|success_count)"

# Verify provider health
curl -I https://api.anthropic.com/v1/messages \
  -H "x-api-key: $ANTHROPIC_API_KEY"
```

#### Tuning Recommendations

**For Development Environments:**
```toml
# ~/.auto-tundra/config/harness.toml
[circuit_breaker]
failure_threshold = 3       # Open faster during testing
success_threshold = 1       # Close faster after recovery
timeout_secs = 10           # Shorter wait during development
call_timeout_secs = 60      # Longer for debugging

[rate_limit.global]
tokens_per_second = 100.0   # Higher limits for testing
max_burst = 200.0
```

**For Production Environments:**
```toml
[circuit_breaker]
failure_threshold = 5       # More tolerance before opening
success_threshold = 2       # Verify stable recovery
timeout_secs = 60           # Standard recovery period
call_timeout_secs = 30      # Reasonable request deadline

[rate_limit.global]
tokens_per_second = 50.0    # Conservative global limit
max_burst = 100.0           # Handle moderate spikes
```

**For High-Throughput Environments:**
```toml
[circuit_breaker]
failure_threshold = 10      # Higher tolerance
timeout_secs = 30           # Faster recovery attempts
call_timeout_secs = 45      # More time for complex requests

[rate_limit.global]
tokens_per_second = 200.0   # High throughput
max_burst = 500.0           # Large spike tolerance
```

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

The `RUST_LOG` environment variable controls logging verbosity using Rust's `tracing` ecosystem. Auto-Tundra's logging is initialized in `at-telemetry::logging` and configured per-environment in `at-daemon::environment`.

#### Default Log Levels

Auto-Tundra uses environment-specific logging defaults:

| Environment | RUST_LOG Value | Description |
|-------------|----------------|-------------|
| **Development** | `info,at_daemon=debug,at_core=debug` | Verbose output for debugging |
| **Staging** | `info,at_daemon=debug` | Moderate verbosity for pre-production testing |
| **Production** | `info,at_daemon=warn` | Minimal output, warnings and errors only |
| **Default (if unset)** | `info,at_daemon=debug` | Falls back to development-like levels |

**Setting the environment:**
```bash
# Explicitly set environment (development, staging, production)
at-daemon development  # Uses development log levels

# Or configure RUST_LOG directly
export RUST_LOG=info,at_daemon=debug
at-daemon
```

#### Crate-Specific Filtering

Use comma-separated directives to control logging per crate:

**Basic Examples:**
```bash
# Show all info-level logs, debug for at_daemon
export RUST_LOG=info,at_daemon=debug

# Add debug for at_intelligence LLM routing
export RUST_LOG=info,at_daemon=debug,at_intelligence=debug

# Verbose output for multiple crates
export RUST_LOG=info,at_daemon=trace,at_core=debug,at_agents=debug

# Quiet external dependencies, verbose for Auto-Tundra crates
export RUST_LOG=warn,at_daemon=debug,at_core=debug,at_intelligence=debug
```

**Module-Level Granularity:**
```bash
# Debug only the spawn module in at_daemon
export RUST_LOG=info,at_daemon::spawn=debug

# Trace-level logs for PTY session management
export RUST_LOG=info,at_sessions::pty=trace

# Combine crate and module filters
export RUST_LOG=warn,at_daemon::environment=debug,at_intelligence::router=trace
```

**Common Crates and Modules:**
- `at_daemon` - Main daemon process, environment configuration
- `at_core` - Core utilities, shared types
- `at_intelligence` - LLM routing, provider failover
- `at_agents` - Agent execution, task management
- `at_harness` - Rate limiting, circuit breakers
- `at_sessions` - PTY session management
- `at_telemetry` - Logging and tracing initialization
- `at_bridge` - VSCode extension bridge

#### Log Levels

Rust's `tracing` supports five log levels (from least to most verbose):

| Level | Purpose | Example Output |
|-------|---------|----------------|
| `error` | Critical failures requiring immediate attention | `Error loading config: file not found` |
| `warn` | Warning conditions that may require investigation | `Circuit breaker opened for provider: anthropic` |
| `info` | Informational messages about normal operations | `Logging initialised (human-readable)` |
| `debug` | Diagnostic information for troubleshooting | `Loaded environment configuration from environment/development.env` |
| `trace` | Very detailed trace information (performance impact) | `Entering function handle_request with params: {...}` |

**Example output (info level):**
```
2026-03-01T10:30:45.123Z  INFO at_daemon: logging initialised (human-readable) service="at-daemon"
2026-03-01T10:30:45.150Z  INFO at_daemon::environment: Loaded environment configuration from environment/development.env
2026-03-01T10:30:45.155Z  INFO at_daemon::environment: Environment configuration loaded for: development
```

**Example output (debug level with RUST_LOG=debug,at_daemon=trace):**
```
2026-03-01T10:30:45.155Z DEBUG at_daemon::environment: set_default_env_vars file="crates/at-daemon/src/environment.rs" line=25
2026-03-01T10:30:45.156Z TRACE at_daemon::environment: Setting DD_SERVICE=at-daemon file="crates/at-daemon/src/environment.rs" line=28
2026-03-01T10:30:45.156Z TRACE at_daemon::environment: Setting DD_ENV=development file="crates/at-daemon/src/environment.rs" line=32
2026-03-01T10:30:45.157Z  INFO at_daemon::environment: Datadog Configuration: file="crates/at-daemon/src/environment.rs" line=60
2026-03-01T10:30:45.157Z  INFO at_daemon::environment:   Service: at-daemon file="crates/at-daemon/src/environment.rs" line=61
2026-03-01T10:30:45.157Z  INFO at_daemon::environment:   Environment: development file="crates/at-daemon/src/environment.rs" line=62
```

#### Diagnostic Output Examples

**Enable diagnostic tracing for LLM provider debugging:**
```bash
# Show request/response details for Anthropic API calls
export RUST_LOG=info,at_intelligence=debug,at_harness=debug
export DD_TRACE_DEBUG=true  # Enable Datadog trace debugging
at-daemon development
```

**Output shows:**
- HTTP request/response bodies (sanitized)
- Rate limit headers and retry-after values
- Circuit breaker state transitions
- Provider failover decisions
- Token usage and cost tracking

**Debug PTY session spawn failures:**
```bash
# Trace-level logging for session management
export RUST_LOG=info,at_sessions=trace,at_daemon::spawn=trace
at-daemon
```

**Output shows:**
- Shell environment variable setup
- PTY device allocation
- Process fork and exec details
- File descriptor handling
- Session capacity tracking

**Diagnose WebSocket connection issues:**
```bash
# Debug transport and IPC layers
export RUST_LOG=info,at_bridge=debug,at_sessions::websocket=debug
at-daemon
```

**Output shows:**
- WebSocket handshake details
- IPC message routing
- Connection state transitions
- Ping/pong frame timing

#### JSON Output for Production

For production deployments with log aggregation (Vector, Loki, ELK), use JSON output:

**Code configuration in `at-telemetry::logging`:**
```rust
// Initialize JSON logging for structured output
init_logging_json("at-daemon", "info,at_daemon=warn");
```

**JSON output format:**
```json
{
  "timestamp": "2026-03-01T10:30:45.123456Z",
  "level": "INFO",
  "target": "at_daemon::environment",
  "fields": {
    "message": "Environment configuration loaded for: production",
    "service": "at-daemon"
  },
  "file": "crates/at-daemon/src/environment.rs",
  "line": 54
}
```

**Benefits:**
- Structured parsing for log aggregation systems
- Consistent field extraction for queries
- Metadata-rich (file, line, target, timestamp)
- No regex parsing required

#### Performance Impact

Verbose logging has measurable overhead:

| Level | Performance Impact | Use Case |
|-------|-------------------|----------|
| `info` | Negligible (~1% overhead) | Production default |
| `debug` | Low (~5-10% overhead) | Staging, troubleshooting |
| `trace` | High (~20-30% overhead) | Specific issue diagnosis only |

**Guidelines:**
- **Production:** Use `info` or `warn` for minimal overhead
- **Staging:** Use `debug` for pre-production validation
- **Development:** Use `debug` or `trace` freely
- **Troubleshooting:** Enable `trace` for specific modules only, not globally

**Example of targeted trace logging:**
```bash
# ❌ BAD - Global trace (huge performance impact)
export RUST_LOG=trace

# ✅ GOOD - Trace only the problematic module
export RUST_LOG=info,at_sessions::pty::spawn=trace
```

#### Log Rotation and Retention

**Default configuration:**
- Log directory: `~/.auto-tundra/logs/`
- Log file: `daemon.log`
- Rotation: No automatic rotation (managed externally)
- Retention: Indefinite (clean up manually)

**Manual log management:**
```bash
# View recent logs
tail -f ~/.auto-tundra/logs/daemon.log

# Archive old logs
gzip ~/.auto-tundra/logs/daemon.log
mv ~/.auto-tundra/logs/daemon.log.gz ~/.auto-tundra/logs/daemon-$(date +%Y%m%d).log.gz

# Truncate current log file (daemon must be restarted)
pkill at-daemon
> ~/.auto-tundra/logs/daemon.log
at-daemon
```

**Production log rotation (systemd example):**
```ini
# /etc/systemd/system/at-daemon.service
[Service]
StandardOutput=journal
StandardError=journal
SyslogIdentifier=at-daemon

# Logs managed by journald
```

**Query journald logs:**
```bash
# View logs for at-daemon service
journalctl -u at-daemon -f

# Filter by log level
journalctl -u at-daemon -p warning

# Export logs for analysis
journalctl -u at-daemon --since "1 hour ago" > /tmp/at-daemon.log
```

#### Quick Reference

**Common troubleshooting scenarios:**

| Issue | RUST_LOG Setting | Purpose |
|-------|------------------|---------|
| General debugging | `info,at_daemon=debug` | Standard troubleshooting level |
| LLM provider failures | `info,at_intelligence=debug,at_harness=debug` | API calls, rate limits, failover |
| PTY spawn failures | `info,at_sessions=trace,at_daemon::spawn=trace` | Process creation, environment setup |
| WebSocket disconnects | `info,at_bridge=debug,at_sessions::websocket=debug` | Connection management, IPC routing |
| Database errors | `info,at_daemon=debug,sqlx=debug` | SQL queries, connection pool |
| Performance issues | `info,at_daemon=debug` (avoid `trace`) | Diagnose without degrading performance further |
| Full diagnostic capture | `debug` (all crates) | Maximum verbosity for bug reports |

---

## 📇 Error Reference Index

> **Comprehensive index of all 37+ error types found in the codebase, organized by category for quick lookup.**

This index covers every error type defined in Auto-Tundra's workspace. Use it to quickly jump to the relevant troubleshooting section.

---

### By Category

#### 🤖 LLM Provider & Intelligence Errors

**LlmError variants** (from `at-intelligence/src/llm.rs`):
- **HttpError** → [HttpError: Network-Level Failures](#httperror-network-level-failures)
- **ApiError** → [ApiError: Provider Service Errors](#apierror-provider-service-errors)
- **RateLimited** → [RateLimited: Quota Exhaustion](#ratelimited-quota-exhaustion)
- **Timeout** → [Timeout: Request Deadline Exceeded (LLM)](#timeout-request-deadline-exceeded-llm)
- **ParseError** → [ParseError: Malformed API Responses](#parseerror-malformed-api-responses)
- **Unsupported** → [Unsupported: Feature Not Available](#unsupported-feature-not-available)

**IntelligenceError** (from `at-intelligence/src/lib.rs`):
- Wraps `LlmError`, `ProviderError`, `SpecError` → See respective sections below

**ResilientCallError** (from `at-intelligence/src/api_profiles.rs`):
- Handles failover logic → See [LLM Provider Issues](#-llm-provider-issues) for failover behavior

**SpecError** (from `at-intelligence/src/spec.rs`):
- Specification parsing and validation errors → See [Configuration Issues](#-configuration-issues) in Quick Fixes

---

#### 🔌 Provider & Circuit Breaker Errors

**ProviderError** (from `at-harness/src/provider.rs`):
- HTTP client errors, request building failures
- Covered by → [HttpError: Network-Level Failures](#httperror-network-level-failures)

**CircuitBreakerError** (from `at-harness/src/circuit_breaker.rs`):
- **CircuitBreakerError::Open** → [CircuitBreakerError::Open: Service Protection Active](#circuitbreakererroropen-service-protection-active)
- **CircuitBreakerError::Timeout** → [CircuitBreakerError::Timeout: Request Deadline Exceeded](#circuitbreakerrortimeout-request-deadline-exceeded)

**RateLimitError** (from `at-harness/src/rate_limiter.rs`):
- **RateLimitError::Exceeded** → [RateLimitError::Exceeded: Token Bucket Exhaustion](#ratelimiterrorexceeded-token-bucket-exhaustion)

**SecurityError** (from `at-harness/src/security.rs`):
- Request validation, content filtering failures
- Covered by → [ApiError: Provider Service Errors](#apierror-provider-service-errors)

---

#### 🖥️ PTY Session & Process Management Errors

**PtyError** (from `at-session/src/pty_pool.rs`):
- **PtyError::AtCapacity** → [AtCapacity: PTY Pool Exhaustion](#atcapacity-pty-pool-exhaustion)
- **PtyError::HandleNotFound** → [HandleNotFound: Unreleased PTY Handle](#handlenotfound-unreleased-pty-handle)
- **PtyError::SpawnFailed** → [SpawnFailed: Process Creation Failure](#spawnfailed-process-creation-failure)

**SessionError** (from `at-agents/src/claude_session.rs`):
- Session lifecycle errors (creation, state management)
- Related to → [PTY Session Management](#-pty-session-management)

**SessionStoreError** (from `at-core/src/session_store.rs`):
- Database operations for session persistence
- Related to → [Dolt Database Configuration](#-dolt-database-configuration)

---

#### 🌐 WebSocket & Transport Errors

**TransportError** (from `at-bridge/src/transport.rs`):
- **TransportError::ConnectionClosed** → [TransportError: Network Failures](#transporterror-network-failures)
- **TransportError::SendFailed** → [TransportError: Network Failures](#transporterror-network-failures)
- **TransportError::ReceiveFailed** → [TransportError: Network Failures](#transporterror-network-failures)
- **TransportError::SerializationError** → [TransportError: Network Failures](#transporterror-network-failures)

**IpcError** (from `at-bridge/src/ipc.rs`):
- **IpcError::UnknownMessage** → [IpcError: Daemon Communication Failures](#ipcerror-daemon-communication-failures)
- **IpcError::Internal** → [IpcError: Daemon Communication Failures](#ipcerror-daemon-communication-failures)

**ApiError** (from `at-bridge/src/api_error.rs`):
- HTTP API errors from bridge layer
- Related to → [ApiError: Provider Service Errors](#apierror-provider-service-errors)

---

#### 💾 Database & Configuration Errors

**ConfigError** (from `at-core/src/config.rs`):
- **ConfigError::MissingDatabaseUrl** → [ConfigError: Missing or Invalid Configuration](#configerror-missing-or-invalid-configuration)
- **ConfigError::InvalidServerConfig** → [ConfigError: Missing or Invalid Configuration](#configerror-missing-or-invalid-configuration)
- **ConfigError::ParseError** → [ConfigError: Missing or Invalid Configuration](#configerror-missing-or-invalid-configuration)

**RepoError** (from `at-core/src/repo.rs`):
- Git repository operations (clone, fetch, checkout)
- Related to → [Database & Session Issues](#-database--session-issues) in Quick Fixes

**WorktreeError** (from `at-core/src/worktree.rs`):
- Git worktree creation and management
- Related to → [Database & Session Issues](#-database--session-issues) in Quick Fixes

**WorktreeManagerError** (from `at-core/src/worktree_manager.rs`):
- Worktree pool management
- Related to → [Database & Session Issues](#-database--session-issues) in Quick Fixes

**GitReadError** (from `at-core/src/git_read_adapter.rs`):
- Git object reading failures
- Related to → [Database & Session Issues](#-database--session-issues) in Quick Fixes

---

#### 🤖 Agent Execution & Orchestration Errors

**ExecutorError** (from `at-agents/src/executor.rs`):
- **ExecutorError::ToolFailed** → Tool execution failures
- **ExecutorError::InvalidState** → State machine violations
- Related to → [PTY Session Management](#-pty-session-management) and [Agent Execution](#agent-execution-errors)

**ToolErrorRecovery** (from `at-agents/src/executor.rs`):
- Automatic recovery strategies for tool failures
- Related to → [Rate Limiting & Circuit Breakers](#-rate-limiting--circuit-breakers)

**TaskRunnerError** (from `at-agents/src/task_runner.rs`):
- Task pipeline execution failures
- Related to → [Agent Execution](#agent-execution-errors)

**PipelineError** (from `at-agents/src/task_orchestrator.rs`):
- Multi-stage task orchestration failures
- Related to → [Agent Execution](#agent-execution-errors)

**StateMachineError** (from `at-agents/src/state_machine.rs`):
- Invalid state transitions
- Related to → [Agent Execution](#agent-execution-errors)

**ApprovalError** (from `at-agents/src/approval.rs`):
- User approval workflow failures
- Related to → [Agent Execution](#agent-execution-errors)

**LifecycleError** (from `at-agents/src/lifecycle.rs`):
- Agent lifecycle management (start, stop, pause)
- Related to → [Agent Execution](#agent-execution-errors)

**SupervisorError** (from `at-agents/src/supervisor.rs`):
- Agent supervision and monitoring
- Related to → [Agent Execution](#agent-execution-errors)

**RegistryError** (from `at-agents/src/registry.rs`):
- Agent registration and discovery
- Related to → [Agent Execution](#agent-execution-errors)

---

#### 🔗 Integration Errors (Third-Party APIs)

**GitHubError** (from `at-integrations/src/github/client.rs`):
- GitHub API request failures
- **OAuthError** (GitHub OAuth flow)
- Related to → [Authentication & Authorization Issues](#-authentication--authorization-issues) in Quick Fixes

**GitLabError** (from `at-integrations/src/gitlab/mod.rs`):
- GitLab API request failures
- **OAuthError** (GitLab OAuth flow)
- Related to → [Authentication & Authorization Issues](#-authentication--authorization-issues) in Quick Fixes

**LinearError** (from `at-integrations/src/linear/mod.rs`):
- Linear API request failures
- Related to → [Authentication & Authorization Issues](#-authentication--authorization-issues) in Quick Fixes

**TokenManagerError** (from `at-bridge/src/oauth_token_manager.rs`):
- OAuth token storage and refresh failures
- Related to → [Authentication & Authorization Issues](#-authentication--authorization-issues) in Quick Fixes

---

#### 🎯 Daemon & Orchestration Errors

**OrchestratorError** (from `at-daemon/src/orchestrator.rs`):
- Top-level orchestration failures
- Aggregates multiple error types
- Related to → All sections (check specific error variant)

**CommandError** (from `at-bridge/src/command_registry.rs`):
- Command registration and dispatch failures
- Related to → [IpcError: Daemon Communication Failures](#ipcerror-daemon-communication-failures)

---

#### 🖼️ UI & Tauri Errors

**TauriError** (from `app/tauri/src/error.rs`):
- Desktop application errors (file system, IPC, window management)
- Related to → [WebSocket Connections](#-websocket-connections) for IPC issues

---

#### 🔐 Cryptography Errors

**CryptoError** (from `at-core/src/crypto.rs`):
- Encryption/decryption failures, key derivation errors
- Related to → [Authentication & Authorization Issues](#-authentication--authorization-issues) in Quick Fixes

---

### Agent Execution Errors

> **Note:** Agent execution errors (`ExecutorError`, `TaskRunnerError`, `PipelineError`, `StateMachineError`, `ApprovalError`, `LifecycleError`, `SupervisorError`, `RegistryError`) are not yet covered in dedicated troubleshooting sections. These errors typically surface as:
> - Tool execution failures → Check PTY session and WebSocket sections
> - State machine violations → Check daemon logs for invalid transitions
> - Approval timeouts → Check WebSocket connection to UI
> - Lifecycle errors → Check daemon orchestrator status

**Common Solutions:**
1. **Enable agent tracing:**
   ```bash
   export RUST_LOG=info,at_agents=debug,at_agents::executor=trace
   ```
2. **Check agent state:**
   ```bash
   tail -f ~/.auto-tundra/logs/daemon.log | grep -E 'agent|executor|state_machine'
   ```
3. **Verify tool availability:** Ensure required tools (git, npm, etc.) are installed
4. **Check approval UI:** If approval workflow fails, verify WebSocket connection

---

### Alphabetical Index

Quick alphabetical lookup of all error types:

- **ApiError** → [ApiError: Provider Service Errors](#apierror-provider-service-errors)
- **ApprovalError** → [Agent Execution Errors](#agent-execution-errors)
- **AtCapacity** → [AtCapacity: PTY Pool Exhaustion](#atcapacity-pty-pool-exhaustion)
- **CircuitBreakerError::Open** → [CircuitBreakerError::Open: Service Protection Active](#circuitbreakererroropen-service-protection-active)
- **CircuitBreakerError::Timeout** → [CircuitBreakerError::Timeout: Request Deadline Exceeded](#circuitbreakerrortimeout-request-deadline-exceeded)
- **CommandError** → [Daemon & Orchestration Errors](#-daemon--orchestration-errors)
- **ConfigError** → [ConfigError: Missing or Invalid Configuration](#configerror-missing-or-invalid-configuration)
- **CryptoError** → [Cryptography Errors](#-cryptography-errors)
- **ExecutorError** → [Agent Execution Errors](#agent-execution-errors)
- **GitHubError** → [Integration Errors](#-integration-errors-third-party-apis)
- **GitLabError** → [Integration Errors](#-integration-errors-third-party-apis)
- **GitReadError** → [Database & Configuration Errors](#-database--configuration-errors)
- **HandleNotFound** → [HandleNotFound: Unreleased PTY Handle](#handlenotfound-unreleased-pty-handle)
- **HttpError** → [HttpError: Network-Level Failures](#httperror-network-level-failures)
- **IntelligenceError** → [LLM Provider & Intelligence Errors](#-llm-provider--intelligence-errors)
- **IpcError** → [IpcError: Daemon Communication Failures](#ipcerror-daemon-communication-failures)
- **LifecycleError** → [Agent Execution Errors](#agent-execution-errors)
- **LinearError** → [Integration Errors](#-integration-errors-third-party-apis)
- **LlmError** → [LLM Provider & Intelligence Errors](#-llm-provider--intelligence-errors)
- **OAuthError** → [Integration Errors](#-integration-errors-third-party-apis)
- **OrchestratorError** → [Daemon & Orchestration Errors](#-daemon--orchestration-errors)
- **ParseError** → [ParseError: Malformed API Responses](#parseerror-malformed-api-responses)
- **PipelineError** → [Agent Execution Errors](#agent-execution-errors)
- **ProviderError** → [Provider & Circuit Breaker Errors](#-provider--circuit-breaker-errors)
- **PtyError** → [PTY Session & Process Management Errors](#-pty-session--process-management-errors)
- **RateLimitError::Exceeded** → [RateLimitError::Exceeded: Token Bucket Exhaustion](#ratelimiterrorexceeded-token-bucket-exhaustion)
- **RateLimited** → [RateLimited: Quota Exhaustion](#ratelimited-quota-exhaustion)
- **RegistryError** → [Agent Execution Errors](#agent-execution-errors)
- **RepoError** → [Database & Configuration Errors](#-database--configuration-errors)
- **ResilientCallError** → [LLM Provider & Intelligence Errors](#-llm-provider--intelligence-errors)
- **SecurityError** → [Provider & Circuit Breaker Errors](#-provider--circuit-breaker-errors)
- **SessionError** → [PTY Session & Process Management Errors](#-pty-session--process-management-errors)
- **SessionStoreError** → [PTY Session & Process Management Errors](#-pty-session--process-management-errors)
- **SpawnFailed** → [SpawnFailed: Process Creation Failure](#spawnfailed-process-creation-failure)
- **SpecError** → [LLM Provider & Intelligence Errors](#-llm-provider--intelligence-errors)
- **StateMachineError** → [Agent Execution Errors](#agent-execution-errors)
- **SupervisorError** → [Agent Execution Errors](#agent-execution-errors)
- **TaskRunnerError** → [Agent Execution Errors](#agent-execution-errors)
- **TauriError** → [UI & Tauri Errors](#-ui--tauri-errors)
- **Timeout** → [Timeout: Request Deadline Exceeded (LLM)](#timeout-request-deadline-exceeded-llm)
- **TokenManagerError** → [Integration Errors](#-integration-errors-third-party-apis)
- **ToolErrorRecovery** → [Agent Execution Errors](#agent-execution-errors)
- **TransportError** → [TransportError: Network Failures](#transporterror-network-failures)
- **Unsupported** → [Unsupported: Feature Not Available](#unsupported-feature-not-available)
- **WorktreeError** → [Database & Configuration Errors](#-database--configuration-errors)
- **WorktreeManagerError** → [Database & Configuration Errors](#-database--configuration-errors)

**Total Error Types Indexed: 37** ✓

---

### Error Severity Guide

Understanding error severity helps prioritize troubleshooting:

| Severity | Impact | Examples | Action |
|----------|--------|----------|--------|
| **Critical** | Service unavailable | `ConfigError::MissingDatabaseUrl`, `OrchestratorError` | Immediate action required |
| **High** | Feature degraded | `CircuitBreakerError::Open`, `AtCapacity` | Address within minutes |
| **Medium** | Temporary failure | `RateLimited`, `Timeout`, `HttpError` | Auto-recovers, monitor |
| **Low** | Degraded experience | `ParseError`, `HandleNotFound` | Fix when convenient |
| **Info** | Recoverable | `ToolErrorRecovery`, automatic retries | No action needed |

**Using Severity in Logs:**
```bash
# Filter by critical errors
tail -f ~/.auto-tundra/logs/daemon.log | grep -E 'ConfigError|OrchestratorError'

# Monitor high-severity errors
tail -f ~/.auto-tundra/logs/daemon.log | grep -E 'CircuitBreakerError|AtCapacity'

# Track recovery events
tail -f ~/.auto-tundra/logs/daemon.log | grep -E 'recovered|retry.*success'
```

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
