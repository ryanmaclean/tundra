# Error Handling Guidelines

This document provides guidelines for handling errors in the at-bridge service to prevent information disclosure while maintaining operational observability.

## Table of Contents

1. [Security Rationale](#security-rationale)
2. [Error Sanitization Utilities](#error-sanitization-utilities)
3. [When to Sanitize vs. Expose](#when-to-sanitize-vs-expose)
4. [Request ID Correlation](#request-id-correlation)
5. [Logging Best Practices](#logging-best-practices)
6. [Examples](#examples)
7. [Common Patterns](#common-patterns)
8. [Testing Guidelines](#testing-guidelines)

---

## Security Rationale

Raw error messages can expose sensitive internal information that aids attacker reconnaissance:

- **Filesystem paths**: Reveals directory structures and installation locations
- **OS-level errors**: Exposes operating system details, versions, and error codes
- **Environment variables**: Discloses configuration presence/absence and names
- **Database details**: Leaks connection strings and internal service information
- **Stack traces**: Reveals source code structure and internal implementation

**OWASP Classification**: A05:2021 Security Misconfiguration

### Defense Strategy

1. **Sanitize** error responses sent to clients (generic messages only)
2. **Log** full error details internally for debugging
3. **Correlate** errors using request IDs from the `x-request-id` header
4. **Maintain** operational observability without exposing internals

---

## Error Sanitization Utilities

The `error_response` module provides utilities for creating sanitized error responses. All utilities are located in `crates/at-bridge/src/error_response.rs`.

### Available Functions

#### `sanitized_error(message: &str, request_id: &str)`

Returns a 500 Internal Server Error with a custom message.

**Use when**: You want to provide a specific error message while hiding implementation details.

```rust
sanitized_error("Failed to create terminal", &request_id)
// Returns: (StatusCode::INTERNAL_SERVER_ERROR, Json<{"error": "...", "request_id": "..."}>>)
```

#### `sanitized_error_with_status(status: StatusCode, message: &str, request_id: &str)`

Returns an error with a custom status code and message.

**Use when**: You need a specific HTTP status code (e.g., 400 Bad Request, 503 Service Unavailable).

```rust
sanitized_error_with_status(
    StatusCode::BAD_REQUEST,
    "Invalid configuration format",
    &request_id
)
```

#### `sanitized_500(request_id: &str)`

Returns a generic 500 Internal Server Error.

**Use when**: You want to hide all error details completely.

```rust
sanitized_500(&request_id)
// Returns: {"error": "Internal server error", "request_id": "..."}
```

#### `sanitized_503(message: &str, request_id: &str)`

Returns a 503 Service Unavailable error.

**Use when**: An external service or resource is temporarily unavailable.

```rust
sanitized_503("GitHub OAuth not configured", &request_id)
```

#### `extract_request_id(headers: &HeaderMap)`

Extracts the request ID from HTTP headers.

**Usage**: Call this at the start of every handler that may return errors.

```rust
let request_id = extract_request_id(&headers);
```

Returns "unknown" if the header is not present.

---

## When to Sanitize vs. Expose

### ✅ SAFE to expose (DO NOT sanitize)

These errors contain no sensitive information:

| Error Type | Example | Rationale |
|------------|---------|-----------|
| **Validation errors** | "Field 'name' is required" | User input validation doesn't reveal internals |
| **Format errors** | "Invalid JSON format" | Generic parsing errors are safe |
| **Business logic** | "Cannot transition from 'pending' to 'completed'" | Application logic is not sensitive |
| **Client errors** | "Missing required parameter" | Request validation is safe |

### ⛔ MUST SANITIZE (Always sanitize)

These errors can leak sensitive information:

| Error Type | What It Exposes | Sanitized Message |
|------------|-----------------|-------------------|
| **Filesystem operations** | Paths, permissions, disk space | "Failed to save settings" |
| **OS-level errors** | System calls, errno codes, process details | "Failed to create terminal" |
| **External API errors** | API endpoints, authentication details, rate limits | "Failed to connect to GitHub" |
| **Database errors** | Connection strings, table names, SQL queries | "Failed to load data" |
| **Environment variables** | Config names, presence/absence | "Service not configured" |
| **Git operations** | Paths, branch names, commit details | "Failed to list worktrees" |
| **Network errors** | Internal hostnames, ports, URLs | "Failed to connect to service" |
| **Serialization internals** | Type names, struct fields | "Invalid data format" |

### Decision Tree

```
Does the error contain:
├─ Filesystem paths? ──────────────────────────> SANITIZE
├─ OS error codes or details? ─────────────────> SANITIZE
├─ Environment variable names? ────────────────> SANITIZE
├─ External API responses? ────────────────────> SANITIZE
├─ Database connection details? ───────────────> SANITIZE
├─ Internal service names/URLs? ───────────────> SANITIZE
├─ Only user input validation failures? ───────> SAFE (can expose)
└─ Only business logic violations? ────────────> SAFE (can expose)
```

**When in doubt, SANITIZE.** It's better to be overly cautious with security.

---

## Request ID Correlation

Request IDs enable error correlation between client responses and server logs without exposing sensitive information.

### How It Works

1. **Middleware Generation**: The `request_id_middleware` (in `at-telemetry`) generates a unique `x-request-id` header for each request
2. **Client Provision**: Clients can provide their own request ID via the `x-request-id` header
3. **Handler Extraction**: Handlers extract the request ID using `extract_request_id()`
4. **Server Logging**: Full error details are logged with the request ID
5. **Client Response**: The same request ID is returned in the error response

### Request ID Flow

```
[Client Request]
   |
   | (optional) x-request-id: client-provided-id
   v
[Middleware] ──> Generates x-request-id if not present
   |
   v
[Handler] ──> extract_request_id(&headers)
   |
   ├─> [Server Log]: ERROR request_id=abc-123 error="Permission denied: /var/lib/settings.toml"
   |
   └─> [Client Response]: {"error": "Failed to save settings", "request_id": "abc-123"}
```

### Usage in Handlers

**Step 1**: Add `HeaderMap` parameter to your handler:

```rust
async fn my_handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,  // Add this
    // ... other parameters
) -> impl IntoResponse {
```

**Step 2**: Extract request ID early in the handler:

```rust
let request_id = extract_request_id(&headers);
```

**Step 3**: Use in error responses and logs:

```rust
match some_operation() {
    Ok(result) => /* handle success */,
    Err(e) => {
        tracing::error!(
            request_id = %request_id,
            error = %e,
            "Detailed context about what failed"
        );
        return sanitized_error("Generic message for client", &request_id);
    }
}
```

### Debugging with Request IDs

When a client reports an error:

1. Client provides the `request_id` from the error response
2. Search server logs for that specific request ID
3. View full error details, stack traces, and context
4. Debug without asking client for sensitive information

**Example**:

```bash
# Client sees:
{"error": "Failed to create terminal", "request_id": "req-abc-123"}

# Server logs contain:
ERROR request_id=req-abc-123 error="spawn failed: No such file or directory (os error 2)" Failed to spawn terminal
```

---

## Logging Best Practices

### Structured Logging with Request IDs

**Always use structured logging** with the `tracing` crate:

```rust
// ✅ GOOD - Structured logging with request ID
tracing::error!(
    request_id = %request_id,
    error = %e,
    context_field = "additional context",
    "Human-readable message"
);

// ❌ BAD - Unstructured logging
eprintln!("Error: {}", e);
```

### Log Levels

| Level | When to Use | Example |
|-------|-------------|---------|
| `error!` | Operation failures that return errors to clients | API endpoint failures, save errors |
| `warn!` | Recoverable issues that don't fail the request | Deprecated API usage, fallback behavior |
| `info!` | Normal operational events | Request started, configuration loaded |
| `debug!` | Detailed diagnostic information | Internal state changes, detailed flow |
| `trace!` | Very verbose debugging | Variable values, iteration details |

### Error Logging Template

```rust
match operation() {
    Ok(result) => result,
    Err(e) => {
        tracing::error!(
            request_id = %request_id,           // Always include request ID
            error = %e,                          // Full error details
            operation = "operation_name",        // What was being attempted
            context_key = "context_value",       // Additional context
            "Human-readable description of failure"
        );
        return sanitized_error("Generic message", &request_id);
    }
}
```

### What to Log

**DO log** (server-side only):
- Full error messages and stack traces
- Filesystem paths involved
- Environment variable names being checked
- OS error codes and system call details
- API endpoints and request details
- Database queries and connection info
- All debugging context needed to fix issues

**DON'T log** (anywhere):
- Passwords or API keys
- Session tokens or credentials
- Personally identifiable information (PII)
- Credit card or payment information
- Full request/response bodies with sensitive data

---

## Examples

### Example 1: Terminal Spawn Error

**Before** (exposes OS details):

```rust
match PtySession::new_bash(&pool, shell_config).await {
    Ok(session) => session,
    Err(e) => {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("spawn failed: {e}") }))
        );
    }
}
// Client sees: "spawn failed: No such file or directory (os error 2)"
```

**After** (sanitized):

```rust
match PtySession::new_bash(&pool, shell_config).await {
    Ok(session) => session,
    Err(e) => {
        let request_id = extract_request_id(&headers);
        tracing::error!(
            request_id = %request_id,
            error = %e,
            "Failed to spawn terminal"
        );
        return sanitized_error("Failed to create terminal", &request_id);
    }
}
// Client sees: {"error": "Failed to create terminal", "request_id": "req-123"}
// Server logs: ERROR request_id=req-123 error="No such file or directory (os error 2)" Failed to spawn terminal
```

### Example 2: Settings Save Error

**Before** (exposes filesystem paths):

```rust
if let Err(e) = settings_manager.save(&settings).await {
    return (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": e.to_string() }))
    );
}
// Client sees: "failed to write config to disk: Permission denied: /etc/at-bridge/settings.toml"
```

**After** (sanitized):

```rust
if let Err(e) = settings_manager.save(&settings).await {
    let request_id = extract_request_id(&headers);
    tracing::error!(
        request_id = %request_id,
        error = %e,
        "Failed to save settings"
    );
    return sanitized_error("Failed to save settings", &request_id);
}
// Client sees: {"error": "Failed to save settings", "request_id": "req-456"}
// Server logs: ERROR request_id=req-456 error="Permission denied: /etc/at-bridge/settings.toml" Failed to save settings
```

### Example 3: OAuth Configuration Error

**Before** (reveals environment variable names):

```rust
let (Some(client_id), Some(client_secret)) = (
    env::var("GITHUB_OAUTH_CLIENT_ID").ok(),
    env::var("GITHUB_OAUTH_CLIENT_SECRET").ok()
) else {
    return (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "error": "GITHUB_OAUTH_CLIENT_ID not set" }))
    );
};
// Client sees: "GITHUB_OAUTH_CLIENT_ID not set"
```

**After** (sanitized):

```rust
let request_id = extract_request_id(&headers);

let (Some(client_id), Some(client_secret)) = (
    env::var("GITHUB_OAUTH_CLIENT_ID").ok(),
    env::var("GITHUB_OAUTH_CLIENT_SECRET").ok()
) else {
    tracing::error!(
        request_id = %request_id,
        "GitHub OAuth configuration missing: GITHUB_OAUTH_CLIENT_ID or GITHUB_OAUTH_CLIENT_SECRET not set"
    );
    return sanitized_503("GitHub OAuth not configured", &request_id);
};
// Client sees: {"error": "GitHub OAuth not configured", "request_id": "req-789"}
// Server logs: ERROR request_id=req-789 GitHub OAuth configuration missing: GITHUB_OAUTH_CLIENT_ID or GITHUB_OAUTH_CLIENT_SECRET not set
```

### Example 4: External API Error

**Before** (exposes API details):

```rust
let client = GitHubClient::new(gh_config).map_err(|e| {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": e.to_string() }))
    )
})?;
// Client sees: "invalid authentication token format: expected 'ghp_' prefix"
```

**After** (sanitized):

```rust
let request_id = extract_request_id(&headers);

let client = match GitHubClient::new(gh_config) {
    Ok(c) => c,
    Err(e) => {
        tracing::error!(
            request_id = %request_id,
            error = %e,
            "Failed to initialize GitHub client"
        );
        return sanitized_error("Failed to connect to GitHub", &request_id);
    }
};
// Client sees: {"error": "Failed to connect to GitHub", "request_id": "req-101"}
// Server logs: ERROR request_id=req-101 error="invalid authentication token format: expected 'ghp_' prefix" Failed to initialize GitHub client
```

### Example 5: Safe Validation Error (No Sanitization Needed)

```rust
// Validation errors are safe to expose - they only contain user input info
let phase = body.phase.trim();
if phase.is_empty() {
    return (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": "Phase cannot be empty" }))
    );
}

// Business logic errors are also safe
if !valid_transitions.contains(&(current_phase, new_phase)) {
    return (
        StatusCode::BAD_REQUEST,
        Json(json!({
            "error": format!("Cannot transition from '{}' to '{}'", current_phase, new_phase)
        }))
    );
}
```

---

## Common Patterns

### Pattern 1: External API Client Initialization

```rust
async fn handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let request_id = extract_request_id(&headers);

    let client = match ExternalClient::new(config) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(
                request_id = %request_id,
                error = %e,
                "Failed to initialize [Service] client"
            );
            return sanitized_error("Failed to connect to [Service]", &request_id);
        }
    };

    // ... rest of handler
}
```

### Pattern 2: Filesystem Operations

```rust
async fn handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let request_id = extract_request_id(&headers);

    match filesystem_operation().await {
        Ok(result) => result,
        Err(e) => {
            tracing::error!(
                request_id = %request_id,
                error = %e,
                operation = "description of operation",
                "Failed to [action] [resource]"
            );
            return sanitized_error("Failed to [action] [resource]", &request_id);
        }
    }

    // ... rest of handler
}
```

### Pattern 3: Mixed Status Codes

```rust
async fn handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let request_id = extract_request_id(&headers);

    // Deserialization error - client's fault (400)
    let data = match serde_json::from_value(value) {
        Ok(d) => d,
        Err(e) => {
            tracing::error!(
                request_id = %request_id,
                error = %e,
                "Invalid data format"
            );
            return sanitized_error_with_status(
                StatusCode::BAD_REQUEST,
                "Invalid data format",
                &request_id
            );
        }
    };

    // Save error - server's fault (500)
    match storage.save(data).await {
        Ok(_) => /* success */,
        Err(e) => {
            tracing::error!(
                request_id = %request_id,
                error = %e,
                "Failed to save data"
            );
            return sanitized_error("Failed to save data", &request_id);
        }
    }
}
```

### Pattern 4: Service Unavailable (503)

```rust
async fn handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let request_id = extract_request_id(&headers);

    let config = match get_required_config() {
        Some(c) => c,
        None => {
            tracing::error!(
                request_id = %request_id,
                "Service not configured: missing required configuration"
            );
            return sanitized_503("Service not configured", &request_id);
        }
    };

    // ... rest of handler
}
```

---

## Testing Guidelines

### Unit Tests for Sanitization

Every error handler should have tests that verify:

1. ✅ Error responses include `request_id`
2. ✅ Error responses do NOT contain sensitive details
3. ✅ Custom request IDs are preserved
4. ✅ Error structure is consistent

**Example test**:

```rust
#[tokio::test]
async fn test_handler_sanitizes_errors() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Trigger an error condition
    let resp = client
        .post(format!("{base}/api/endpoint"))
        .header("x-request-id", "test-req-123")
        .json(&invalid_data)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let body: Value = resp.json().await.unwrap();

    // Verify sanitized response structure
    assert_eq!(body["error"], "Generic error message");
    assert_eq!(body["request_id"], "test-req-123");

    // Verify no sensitive information leaked
    let response_text = serde_json::to_string(&body).unwrap();
    assert!(!response_text.contains("/path/to/file"));
    assert!(!response_text.contains("ENVIRONMENT_VARIABLE"));
    assert!(!response_text.contains("Permission denied"));
}
```

### Security Test Checklist

- [ ] Error responses include `request_id` field
- [ ] No filesystem paths in responses
- [ ] No OS error codes in responses
- [ ] No environment variable names in responses
- [ ] No stack traces in responses
- [ ] No internal hostnames or URLs in responses
- [ ] No database connection details in responses
- [ ] Custom request IDs are preserved
- [ ] Error structure is consistent across all endpoints

### Test Coverage Requirements

For security-critical handlers, ensure:

- **Unit tests**: Test error sanitization utilities
- **Integration tests**: Test full request/response cycle
- **Security tests**: Verify no information leakage
- **Manual testing**: Trigger actual error conditions and inspect logs

---

## Quick Reference

### Essential Steps for Every Error Handler

1. **Add HeaderMap parameter** to extract request ID
2. **Extract request ID** early in the handler: `let request_id = extract_request_id(&headers);`
3. **Log errors** with structured logging: `tracing::error!(request_id = %request_id, error = %e, ...)`
4. **Return sanitized responses** using error utilities: `sanitized_error("Message", &request_id)`
5. **Test thoroughly** to verify no information leakage

### Status Code Guide

| Status | When to Use | Function |
|--------|-------------|----------|
| 400 | Client validation errors | `sanitized_error_with_status(StatusCode::BAD_REQUEST, ...)` |
| 500 | Internal server errors | `sanitized_error(...)` or `sanitized_500(...)` |
| 503 | Service/resource unavailable | `sanitized_503(...)` |

### Common Sanitized Messages

| Scenario | Sanitized Message |
|----------|------------------|
| Terminal spawn failed | "Failed to create terminal" |
| Settings save failed | "Failed to save settings" |
| External API client init | "Failed to connect to [Service]" |
| External API call | "Failed to retrieve [resource] from [Service]" |
| Git operations | "Failed to [action] worktree" |
| Session storage | "Failed to [load\|save\|list] session" |
| Configuration missing | "[Service] not configured" |
| JSON parsing | "Invalid data format" |

---

## Compliance

This error handling approach addresses:

- **OWASP Top 10 2021 - A05: Security Misconfiguration**
- **CWE-209**: Information Exposure Through an Error Message
- **Defense in Depth**: Multiple security layers (sanitization + logging + correlation)
- **Least Privilege Information**: Clients receive minimum necessary information
- **Auditability**: Full error details logged for incident response

---

## Additional Resources

- **Error Response Module**: `crates/at-bridge/src/error_response.rs`
- **Security Tests**: `crates/at-bridge/tests/error_sanitization_test.rs`
- **Audit Report**: `.auto-claude/specs/041-.../AUDIT.md`
- **Security Verification**: `.auto-claude/specs/041-.../SECURITY_VERIFICATION.md`
- **Request ID Middleware**: `crates/at-telemetry/src/tracing_setup.rs`

---

## Questions?

If you're unsure whether an error should be sanitized, **err on the side of caution and sanitize**. It's always better to protect sensitive information and rely on request ID correlation for debugging than to risk information disclosure.
