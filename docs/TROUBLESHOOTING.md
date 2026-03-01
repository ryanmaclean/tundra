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

**This section will be populated with specific error patterns in subtask-1-2:**
- HttpError (network failures, DNS issues)
- ApiError (authentication, authorization)
- RateLimited (quota exhaustion)
- Timeout (slow responses, hanging requests)
- ParseError (malformed responses)
- Unsupported (model/feature not available)

*→ See [Subtask 1-2](../.auto-claude/specs/010-add-troubleshooting-guide-for-common-runtime-error/implementation_plan.json) for implementation details.*

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
