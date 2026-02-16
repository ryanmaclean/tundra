use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// TraceContext — W3C Trace Context propagation
// ---------------------------------------------------------------------------

/// Lightweight W3C Trace Context for propagating correlation IDs across
/// agent spawns, HTTP requests, and async task boundaries.
///
/// This is a simplified implementation of the W3C Trace Context specification
/// that carries a trace ID, span ID, and optional baggage through the system.
///
/// Usage:
/// ```ignore
/// let ctx = TraceContext::new();
/// let child = ctx.child_span();
/// // Pass ctx.to_headers() when spawning an agent or making an HTTP call
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceContext {
    /// Unique trace ID (spans the entire request lifecycle).
    pub trace_id: String,
    /// Current span ID.
    pub span_id: String,
    /// Parent span ID (None for root spans).
    pub parent_span_id: Option<String>,
    /// Sampling flag (true = sampled).
    pub sampled: bool,
    /// Arbitrary key-value baggage propagated across boundaries.
    pub baggage: HashMap<String, String>,
}

impl TraceContext {
    /// Create a new root trace context.
    pub fn new() -> Self {
        Self {
            trace_id: Uuid::new_v4().to_string().replace('-', ""),
            span_id: Self::generate_span_id(),
            parent_span_id: None,
            sampled: true,
            baggage: HashMap::new(),
        }
    }

    /// Create a child span under this context (same trace, new span).
    pub fn child_span(&self) -> Self {
        Self {
            trace_id: self.trace_id.clone(),
            span_id: Self::generate_span_id(),
            parent_span_id: Some(self.span_id.clone()),
            sampled: self.sampled,
            baggage: self.baggage.clone(),
        }
    }

    /// Add a baggage item.
    pub fn with_baggage(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.baggage.insert(key.into(), value.into());
        self
    }

    /// Set sampling flag.
    pub fn with_sampled(mut self, sampled: bool) -> Self {
        self.sampled = sampled;
        self
    }

    /// Serialize to W3C `traceparent` header format.
    ///
    /// Format: `{version}-{trace_id}-{span_id}-{flags}`
    pub fn to_traceparent(&self) -> String {
        let flags = if self.sampled { "01" } else { "00" };
        // Pad/truncate trace_id to 32 hex chars, span_id to 16 hex chars
        let trace_id = format!("{:0>32}", &self.trace_id[..self.trace_id.len().min(32)]);
        let span_id = format!("{:0>16}", &self.span_id[..self.span_id.len().min(16)]);
        format!("00-{}-{}-{}", trace_id, span_id, flags)
    }

    /// Parse from a W3C `traceparent` header.
    pub fn from_traceparent(header: &str) -> Option<Self> {
        let parts: Vec<&str> = header.split('-').collect();
        if parts.len() != 4 {
            return None;
        }

        let _version = parts[0];
        let trace_id = parts[1].to_string();
        let span_id = parts[2].to_string();
        let flags = parts[3];

        if trace_id.len() != 32 || span_id.len() != 16 {
            return None;
        }

        let sampled = flags.ends_with('1');

        Some(Self {
            trace_id,
            span_id,
            parent_span_id: None,
            sampled,
            baggage: HashMap::new(),
        })
    }

    /// Convert to HTTP headers for propagation.
    pub fn to_headers(&self) -> HashMap<String, String> {
        let mut headers = HashMap::new();
        headers.insert("traceparent".to_string(), self.to_traceparent());

        if !self.baggage.is_empty() {
            let baggage_str = self
                .baggage
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join(",");
            headers.insert("baggage".to_string(), baggage_str);
        }

        headers
    }

    /// Parse from HTTP headers.
    pub fn from_headers(headers: &HashMap<String, String>) -> Option<Self> {
        let traceparent = headers.get("traceparent")?;
        let mut ctx = Self::from_traceparent(traceparent)?;

        if let Some(baggage_str) = headers.get("baggage") {
            for item in baggage_str.split(',') {
                if let Some((key, value)) = item.split_once('=') {
                    ctx.baggage
                        .insert(key.trim().to_string(), value.trim().to_string());
                }
            }
        }

        Some(ctx)
    }

    /// Convert to environment variables for subprocess propagation.
    pub fn to_env_vars(&self) -> HashMap<String, String> {
        let mut env = HashMap::new();
        env.insert("TRACEPARENT".to_string(), self.to_traceparent());
        env.insert("AT_TRACE_ID".to_string(), self.trace_id.clone());
        env.insert("AT_SPAN_ID".to_string(), self.span_id.clone());
        if let Some(ref parent) = self.parent_span_id {
            env.insert("AT_PARENT_SPAN_ID".to_string(), parent.clone());
        }
        for (k, v) in &self.baggage {
            env.insert(format!("AT_BAGGAGE_{}", k.to_uppercase()), v.clone());
        }
        env
    }

    /// Reconstruct from environment variables.
    pub fn from_env_vars(env: &HashMap<String, String>) -> Option<Self> {
        if let Some(traceparent) = env.get("TRACEPARENT") {
            return Self::from_traceparent(traceparent);
        }

        let trace_id = env.get("AT_TRACE_ID")?.clone();
        let span_id = env.get("AT_SPAN_ID")?.clone();
        let parent_span_id = env.get("AT_PARENT_SPAN_ID").cloned();

        let mut baggage = HashMap::new();
        for (k, v) in env {
            if let Some(key) = k.strip_prefix("AT_BAGGAGE_") {
                baggage.insert(key.to_lowercase(), v.clone());
            }
        }

        Some(Self {
            trace_id,
            span_id,
            parent_span_id,
            sampled: true,
            baggage,
        })
    }

    /// Check if this is a root span.
    pub fn is_root(&self) -> bool {
        self.parent_span_id.is_none()
    }

    fn generate_span_id() -> String {
        let id = Uuid::new_v4();
        let bytes = id.as_bytes();
        hex::encode(&bytes[..8])
    }
}

impl Default for TraceContext {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for TraceContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_traceparent())
    }
}

// ---------------------------------------------------------------------------
// hex encoding (minimal, avoids adding the `hex` crate just for 8 bytes)
// ---------------------------------------------------------------------------

mod hex {
    const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";

    pub fn encode(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for &b in bytes {
            s.push(HEX_CHARS[(b >> 4) as usize] as char);
            s.push(HEX_CHARS[(b & 0x0f) as usize] as char);
        }
        s
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_root_span() {
        let ctx = TraceContext::new();
        assert!(ctx.is_root());
        assert!(ctx.sampled);
        assert_eq!(ctx.trace_id.len(), 32);
        assert_eq!(ctx.span_id.len(), 16);
    }

    #[test]
    fn child_span_inherits_trace_id() {
        let parent = TraceContext::new();
        let child = parent.child_span();
        assert_eq!(child.trace_id, parent.trace_id);
        assert_ne!(child.span_id, parent.span_id);
        assert_eq!(child.parent_span_id.as_ref(), Some(&parent.span_id));
        assert!(!child.is_root());
    }

    #[test]
    fn child_inherits_baggage() {
        let parent = TraceContext::new().with_baggage("user_id", "abc");
        let child = parent.child_span();
        assert_eq!(child.baggage.get("user_id").unwrap(), "abc");
    }

    #[test]
    fn child_inherits_sampled() {
        let parent = TraceContext::new().with_sampled(false);
        let child = parent.child_span();
        assert!(!child.sampled);
    }

    #[test]
    fn traceparent_roundtrip() {
        let ctx = TraceContext::new();
        let header = ctx.to_traceparent();
        let parsed = TraceContext::from_traceparent(&header).unwrap();
        assert_eq!(parsed.trace_id, ctx.trace_id);
        assert_eq!(parsed.span_id, ctx.span_id);
        assert_eq!(parsed.sampled, ctx.sampled);
    }

    #[test]
    fn traceparent_format() {
        let ctx = TraceContext::new().with_sampled(true);
        let header = ctx.to_traceparent();
        let parts: Vec<&str> = header.split('-').collect();
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[0], "00"); // version
        assert_eq!(parts[1].len(), 32); // trace_id
        assert_eq!(parts[2].len(), 16); // span_id
        assert_eq!(parts[3], "01"); // sampled
    }

    #[test]
    fn traceparent_unsampled() {
        let ctx = TraceContext::new().with_sampled(false);
        let header = ctx.to_traceparent();
        assert!(header.ends_with("-00"));
    }

    #[test]
    fn from_traceparent_invalid_parts() {
        assert!(TraceContext::from_traceparent("invalid").is_none());
        assert!(TraceContext::from_traceparent("00-short-id-01").is_none());
    }

    #[test]
    fn headers_roundtrip() {
        let ctx = TraceContext::new()
            .with_baggage("task_id", "t123")
            .with_baggage("agent", "mayor");
        let headers = ctx.to_headers();
        let parsed = TraceContext::from_headers(&headers).unwrap();
        assert_eq!(parsed.trace_id, ctx.trace_id);
        assert_eq!(parsed.baggage.get("task_id").unwrap(), "t123");
        assert_eq!(parsed.baggage.get("agent").unwrap(), "mayor");
    }

    #[test]
    fn env_vars_roundtrip() {
        let ctx = TraceContext::new().with_baggage("session", "s456");
        let env = ctx.to_env_vars();
        assert!(env.contains_key("TRACEPARENT"));
        assert!(env.contains_key("AT_TRACE_ID"));
        assert!(env.contains_key("AT_SPAN_ID"));
        assert_eq!(env.get("AT_BAGGAGE_SESSION").unwrap(), "s456");

        let parsed = TraceContext::from_env_vars(&env).unwrap();
        assert_eq!(parsed.trace_id, ctx.trace_id);
    }

    #[test]
    fn env_vars_without_traceparent_uses_at_fields() {
        let mut env = HashMap::new();
        env.insert("AT_TRACE_ID".to_string(), "a".repeat(32));
        env.insert("AT_SPAN_ID".to_string(), "b".repeat(16));
        env.insert("AT_PARENT_SPAN_ID".to_string(), "c".repeat(16));

        let ctx = TraceContext::from_env_vars(&env).unwrap();
        assert_eq!(ctx.trace_id, "a".repeat(32));
        assert_eq!(ctx.parent_span_id.as_ref().unwrap(), &"c".repeat(16));
    }

    #[test]
    fn env_vars_missing_returns_none() {
        let env = HashMap::new();
        assert!(TraceContext::from_env_vars(&env).is_none());
    }

    #[test]
    fn display_shows_traceparent() {
        let ctx = TraceContext::new();
        let display = format!("{}", ctx);
        assert!(display.starts_with("00-"));
    }

    #[test]
    fn with_baggage_builder() {
        let ctx = TraceContext::new()
            .with_baggage("a", "1")
            .with_baggage("b", "2");
        assert_eq!(ctx.baggage.len(), 2);
    }

    #[test]
    fn serialization_roundtrip() {
        let ctx = TraceContext::new().with_baggage("key", "val");
        let json = serde_json::to_string(&ctx).unwrap();
        let parsed: TraceContext = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ctx);
    }

    #[test]
    fn default_is_root() {
        let ctx = TraceContext::default();
        assert!(ctx.is_root());
    }

    #[test]
    fn hex_encode_produces_correct_output() {
        assert_eq!(hex::encode(&[0xff, 0x00, 0xab]), "ff00ab");
        assert_eq!(hex::encode(&[0x01, 0x23]), "0123");
    }

    #[test]
    fn multi_level_child_spans() {
        let root = TraceContext::new();
        let child = root.child_span();
        let grandchild = child.child_span();

        assert_eq!(root.trace_id, child.trace_id);
        assert_eq!(root.trace_id, grandchild.trace_id);
        assert_eq!(grandchild.parent_span_id.as_ref(), Some(&child.span_id));
        assert!(root.is_root());
        assert!(!child.is_root());
        assert!(!grandchild.is_root());
    }
}
