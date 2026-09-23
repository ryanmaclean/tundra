//! Rate limiting middleware for the auto-tundra HTTP API.
//!
//! Enforces three-tier rate limiting using the MultiKeyRateLimiter:
//! - **Global**: limits total requests across all users
//! - **Per-user**: limits requests per client IP
//! - **Per-endpoint**: limits requests per URI path
//!
//! When a limit is exceeded, returns HTTP 429 (Too Many Requests) with a
//! `Retry-After` header indicating how long to wait before retrying.
//!
//! # Rate Limit Tiers
//!
//! ## 1. Global Rate Limit
//! Applies to ALL requests across the entire API, regardless of client or endpoint.
//! This protects the server from being overwhelmed by total traffic.
//!
//! ## 2. Per-User Rate Limit
//! Applies to each client identity. The identity is the socket peer IP taken
//! from axum's `ConnectInfo<SocketAddr>` (the server must be started with
//! `into_make_service_with_connect_info::<SocketAddr>()`). `X-Forwarded-For` /
//! `X-Real-IP` are honoured **only** when [`RateLimitPolicy::trust_proxy_headers`]
//! is set, because any client can forge them. Without connect info the
//! identity falls back to `"unknown"`.
//!
//! This prevents any single client from monopolizing server resources.
//!
//! ## 3. Per-Endpoint Rate Limit
//! Applies to each `(client, method, route template)` triple, using axum's
//! `MatchedPath` (e.g. `/api/beads/{id}/status`) rather than the raw URI so
//! the bucket map cannot grow without bound. Unmatched paths share a single
//! `<unmatched>` bucket per client.
//!
//! ## Loopback exemption
//! When [`RateLimitPolicy::exempt_loopback`] is set (the default), direct
//! connections from a loopback peer (the local TUI, desktop app, CLI and MCP
//! clients) skip the per-user and per-endpoint tiers. They still count toward
//! the global tier, which protects the daemon as a whole.
//!
//! This protects expensive endpoints (like AI generation or GitHub sync) from abuse
//! while allowing high-frequency polling of lightweight endpoints like status checks.
//!
//! # Configuration
//!
//! Rate limits are configured when creating the `MultiKeyRateLimiter`:
//!
//! ```rust,ignore
//! use at_harness::rate_limiter::{MultiKeyRateLimiter, RateLimitConfig};
//!
//! let limiter = MultiKeyRateLimiter::new(
//!     RateLimitConfig::per_minute(100),  // Global: 100 requests/minute total
//!     RateLimitConfig::per_minute(20),   // Per-user: 20 requests/minute per IP
//!     RateLimitConfig::per_minute(10),   // Per-endpoint: 10 requests/minute per path
//! );
//! ```
//!
//! ## Adjusting Limits
//!
//! Use `RateLimitConfig` factory methods to set limits:
//! - `RateLimitConfig::per_second(n)` - n requests per second
//! - `RateLimitConfig::per_minute(n)` - n requests per minute
//! - `RateLimitConfig::per_hour(n)` - n requests per hour
//!
//! **Example: High-traffic production configuration**
//! ```rust,ignore
//! let limiter = MultiKeyRateLimiter::new(
//!     RateLimitConfig::per_minute(1000), // High global capacity
//!     RateLimitConfig::per_minute(50),   // Moderate per-user limit
//!     RateLimitConfig::per_minute(20),   // Conservative per-endpoint limit
//! );
//! ```
//!
//! **Example: Development/testing configuration**
//! ```rust,ignore
//! let limiter = MultiKeyRateLimiter::new(
//!     RateLimitConfig::per_second(100), // Generous global limit
//!     RateLimitConfig::per_second(10),  // Relaxed per-user limit
//!     RateLimitConfig::per_second(5),   // Relaxed per-endpoint limit
//! );
//! ```
//!
//! ## Configuration Location
//!
//! The rate limiter is initialized in `ApiState::new()` in `http_api/state.rs`.
//! To change limits, modify the configuration there and rebuild the service.

use axum::{
    body::Body,
    extract::{ConnectInfo, MatchedPath, Request},
    http::{Response, StatusCode},
    response::IntoResponse,
};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::task::{Context, Poll};
use tower::{Layer, Service};
use tracing::warn;

use at_harness::rate_limiter::MultiKeyRateLimiter;

// ---------------------------------------------------------------------------
// Policy
// ---------------------------------------------------------------------------

/// How the middleware derives client identity and which clients are exempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitPolicy {
    /// Skip the per-user and per-endpoint tiers for direct loopback peers.
    pub exempt_loopback: bool,
    /// Trust `X-Forwarded-For` / `X-Real-IP` for client identity. Only enable
    /// this behind a reverse proxy that overwrites those headers.
    pub trust_proxy_headers: bool,
}

impl Default for RateLimitPolicy {
    fn default() -> Self {
        Self {
            exempt_loopback: true,
            trust_proxy_headers: false,
        }
    }
}

/// Resolved client identity for one request.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ClientIdentity {
    key: String,
    exempt: bool,
}

fn forwarded_ip(req: &Request<Body>) -> Option<String> {
    req.headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            req.headers()
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
}

fn client_identity(req: &Request<Body>, policy: RateLimitPolicy) -> ClientIdentity {
    let peer: Option<IpAddr> = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| addr.ip());

    if policy.trust_proxy_headers {
        if let Some(ip) = forwarded_ip(req) {
            return ClientIdentity {
                key: ip,
                exempt: false,
            };
        }
    }

    match peer {
        Some(ip) => ClientIdentity {
            key: ip.to_string(),
            exempt: policy.exempt_loopback && is_loopback(ip),
        },
        None => ClientIdentity {
            key: "unknown".to_string(),
            exempt: false,
        },
    }
}

fn is_loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => {
            v6.is_loopback() || v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback())
        }
    }
}

fn endpoint_key(req: &Request<Body>, client: &str) -> String {
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str())
        .unwrap_or("<unmatched>");
    format!("{client}|{} {route}", req.method())
}

// ---------------------------------------------------------------------------
// RateLimitLayer
// ---------------------------------------------------------------------------

/// A [`tower::Layer`] that wraps services with [`RateLimitMiddleware`].
#[derive(Clone)]
pub struct RateLimitLayer {
    rate_limiter: Arc<MultiKeyRateLimiter>,
    policy: RateLimitPolicy,
}

impl RateLimitLayer {
    /// Create a new `RateLimitLayer` with the given rate limiter and the
    /// default [`RateLimitPolicy`].
    pub fn new(rate_limiter: Arc<MultiKeyRateLimiter>) -> Self {
        Self {
            rate_limiter,
            policy: RateLimitPolicy::default(),
        }
    }

    /// Override the identity / exemption policy.
    pub fn with_policy(mut self, policy: RateLimitPolicy) -> Self {
        self.policy = policy;
        self
    }
}

impl<S> Layer<S> for RateLimitLayer {
    type Service = RateLimitMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RateLimitMiddleware {
            inner,
            rate_limiter: self.rate_limiter.clone(),
            policy: self.policy,
        }
    }
}

// ---------------------------------------------------------------------------
// RateLimitMiddleware
// ---------------------------------------------------------------------------

/// The actual middleware service produced by [`RateLimitLayer`].
#[derive(Clone)]
pub struct RateLimitMiddleware<S> {
    inner: S,
    rate_limiter: Arc<MultiKeyRateLimiter>,
    policy: RateLimitPolicy,
}

impl<S> Service<Request<Body>> for RateLimitMiddleware<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let rate_limiter = self.rate_limiter.clone();
        let policy = self.policy;
        let mut inner = self.inner.clone();

        Box::pin(async move {
            let client = client_identity(&req, policy);
            let client_ip = client.key.clone();
            let endpoint = endpoint_key(&req, &client.key);

            // Exempt clients only count toward the global tier.
            let result = if client.exempt {
                rate_limiter.check_tiers(None, None, 1.0)
            } else {
                rate_limiter.check_all(&client.key, &endpoint)
            };

            match result {
                Ok(()) => {
                    // Rate limit not exceeded, pass through.
                    inner.call(req).await
                }
                Err(err) => {
                    // Rate limit exceeded, return 429 with Retry-After header.
                    warn!(
                        client_ip,
                        endpoint,
                        error = %err,
                        "rate limit exceeded"
                    );

                    // Extract retry_after duration from error.
                    let retry_after_secs = match err {
                        at_harness::rate_limiter::RateLimitError::Exceeded {
                            retry_after, ..
                        } => retry_after.as_secs().max(1),
                    };

                    let resp = (
                        StatusCode::TOO_MANY_REQUESTS,
                        [("Retry-After", retry_after_secs.to_string())],
                        axum::Json(serde_json::json!({
                            "error": "rate_limit_exceeded",
                            "message": err.to_string(),
                            "retry_after": retry_after_secs
                        })),
                    )
                        .into_response();
                    Ok(resp)
                }
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use at_harness::rate_limiter::RateLimitConfig;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    fn test_router(rate_limiter: Arc<MultiKeyRateLimiter>) -> Router {
        Router::new()
            .route("/ping", get(|| async { "pong" }))
            .layer(RateLimitLayer::new(rate_limiter))
    }

    /// Like [`test_router`], but with `trust_proxy_headers: true`.
    ///
    /// The middleware only reads `X-Forwarded-For` / `X-Real-IP` when a
    /// reverse proxy is explicitly trusted (`client_identity`'s default is to
    /// key on the real `ConnectInfo` peer address instead, since blindly
    /// trusting client-supplied headers would let any caller pick its own
    /// rate-limit bucket). Tests that specifically exercise the header
    /// parsing logic need this policy; a synthetic `oneshot()` request has no
    /// `ConnectInfo` extension at all, so without it every request falls
    /// into the shared "unknown" bucket regardless of its headers.
    fn test_router_trusting_proxy(rate_limiter: Arc<MultiKeyRateLimiter>) -> Router {
        Router::new().route("/ping", get(|| async { "pong" })).layer(
            RateLimitLayer::new(rate_limiter).with_policy(RateLimitPolicy {
                trust_proxy_headers: true,
                ..Default::default()
            }),
        )
    }

    #[tokio::test]
    async fn allows_requests_within_limit() {
        let limiter = Arc::new(MultiKeyRateLimiter::new(
            RateLimitConfig::per_second(100),
            RateLimitConfig::per_second(10),
            RateLimitConfig::per_second(5),
        ));

        let app = test_router(limiter);

        // First request should succeed.
        let req = Request::builder().uri("/ping").body(Body::empty()).unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Second request should also succeed.
        let req = Request::builder().uri("/ping").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn rejects_requests_exceeding_limit() {
        // Very restrictive limit: 2 requests per second.
        let limiter = Arc::new(MultiKeyRateLimiter::new(
            RateLimitConfig::per_second(100),
            RateLimitConfig::per_second(100),
            RateLimitConfig::per_second(2),
        ));

        let app = test_router(limiter);

        // First two requests should succeed.
        for _ in 0..2 {
            let req = Request::builder().uri("/ping").body(Body::empty()).unwrap();
            let resp = app.clone().oneshot(req).await.unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
        }

        // Third request should be rate limited.
        let req = Request::builder().uri("/ping").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);

        // Check for Retry-After header.
        assert!(resp.headers().contains_key("retry-after"));
    }

    #[tokio::test]
    async fn includes_retry_after_header() {
        let limiter = Arc::new(MultiKeyRateLimiter::new(
            RateLimitConfig::per_second(1),
            RateLimitConfig::per_second(1),
            RateLimitConfig::per_second(1),
        ));

        let app = test_router(limiter);

        // First request succeeds.
        let req = Request::builder().uri("/ping").body(Body::empty()).unwrap();
        let _ = app.clone().oneshot(req).await.unwrap();

        // Second request should be rate limited.
        let req = Request::builder().uri("/ping").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
        let retry_after = resp.headers().get("retry-after").unwrap();
        assert!(retry_after.to_str().unwrap().parse::<u64>().is_ok());
    }

    fn req_from(uri: &str, peer: Option<&str>) -> Request<Body> {
        let mut req = Request::builder().uri(uri).body(Body::empty()).unwrap();
        if let Some(peer) = peer {
            let addr: SocketAddr = peer.parse().unwrap();
            req.extensions_mut().insert(ConnectInfo(addr));
        }
        req
    }

    fn tight_limiter() -> Arc<MultiKeyRateLimiter> {
        Arc::new(MultiKeyRateLimiter::new(
            RateLimitConfig::per_minute(1000),
            RateLimitConfig::per_minute(2),
            RateLimitConfig::per_minute(1000),
        ))
    }

    #[tokio::test]
    async fn loopback_peer_is_exempt_from_per_client_tiers() {
        let app = test_router(tight_limiter());
        for peer in ["127.0.0.1:50000", "[::1]:50001", "[::ffff:127.0.0.1]:50002"] {
            for _ in 0..10 {
                let resp = app
                    .clone()
                    .oneshot(req_from("/ping", Some(peer)))
                    .await
                    .unwrap();
                assert_eq!(resp.status(), StatusCode::OK, "peer {peer}");
            }
        }
    }

    #[tokio::test]
    async fn loopback_exemption_can_be_disabled() {
        let app = Router::new()
            .route("/ping", get(|| async { "pong" }))
            .layer(
                RateLimitLayer::new(tight_limiter()).with_policy(RateLimitPolicy {
                    exempt_loopback: false,
                    trust_proxy_headers: false,
                }),
            );
        for _ in 0..2 {
            let resp = app
                .clone()
                .oneshot(req_from("/ping", Some("127.0.0.1:1")))
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
        }
        let resp = app
            .oneshot(req_from("/ping", Some("127.0.0.1:1")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn remote_peers_get_independent_buckets() {
        let app = test_router(tight_limiter());
        for _ in 0..2 {
            let resp = app
                .clone()
                .oneshot(req_from("/ping", Some("10.0.0.1:1")))
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
        }
        let resp = app
            .clone()
            .oneshot(req_from("/ping", Some("10.0.0.1:2")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
        // A different client is unaffected.
        let resp = app
            .oneshot(req_from("/ping", Some("10.0.0.2:1")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn forwarded_headers_ignored_unless_trusted() {
        let app = test_router(tight_limiter());
        // Rotating a spoofed X-Forwarded-For must not mint fresh buckets.
        let mut statuses = Vec::new();
        for i in 0..3 {
            let mut req = req_from("/ping", Some("10.0.0.9:1"));
            req.headers_mut()
                .insert("x-forwarded-for", format!("1.2.3.{i}").parse().unwrap());
            statuses.push(app.clone().oneshot(req).await.unwrap().status());
        }
        assert_eq!(statuses[2], StatusCode::TOO_MANY_REQUESTS);

        // Behind a trusted proxy the forwarded address is the identity, and a
        // forwarded loopback peer is not exempt.
        let trusted = Router::new()
            .route("/ping", get(|| async { "pong" }))
            .layer(
                RateLimitLayer::new(tight_limiter()).with_policy(RateLimitPolicy {
                    exempt_loopback: true,
                    trust_proxy_headers: true,
                }),
            );
        for i in 0..3 {
            let mut req = req_from("/ping", Some("127.0.0.1:1"));
            req.headers_mut()
                .insert("x-forwarded-for", format!("1.2.3.{i}").parse().unwrap());
            assert_eq!(
                trusted.clone().oneshot(req).await.unwrap().status(),
                StatusCode::OK
            );
        }
        for _ in 0..2 {
            let mut req = req_from("/ping", Some("127.0.0.1:1"));
            req.headers_mut()
                .insert("x-forwarded-for", "9.9.9.9".parse().unwrap());
            assert_eq!(
                trusted.clone().oneshot(req).await.unwrap().status(),
                StatusCode::OK
            );
        }
        let mut req = req_from("/ping", Some("127.0.0.1:1"));
        req.headers_mut()
            .insert("x-forwarded-for", "9.9.9.9".parse().unwrap());
        assert_eq!(
            trusted.oneshot(req).await.unwrap().status(),
            StatusCode::TOO_MANY_REQUESTS
        );
    }

    #[tokio::test]
    async fn per_endpoint_tier_keys_on_route_template() {
        let limiter = Arc::new(MultiKeyRateLimiter::new(
            RateLimitConfig::per_minute(1000),
            RateLimitConfig::per_minute(1000),
            RateLimitConfig::per_minute(2),
        ));
        let app = Router::new()
            .route("/items/{id}", get(|| async { "item" }))
            .layer(RateLimitLayer::new(limiter));
        for id in ["a", "b"] {
            let resp = app
                .clone()
                .oneshot(req_from(&format!("/items/{id}"), Some("10.0.0.1:1")))
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
        }
        // Third distinct id hits the same `/items/{id}` bucket.
        let resp = app
            .clone()
            .oneshot(req_from("/items/c", Some("10.0.0.1:1")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
        // Per-endpoint buckets are per client.
        let resp = app
            .oneshot(req_from("/items/c", Some("10.0.0.2:1")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn different_endpoints_have_separate_limits() {
        let limiter = Arc::new(MultiKeyRateLimiter::new(
            RateLimitConfig::per_second(100),
            RateLimitConfig::per_second(100),
            RateLimitConfig::per_second(1),
        ));

        let app = Router::new()
            .route("/ping", get(|| async { "pong" }))
            .route("/health", get(|| async { "ok" }))
            .layer(RateLimitLayer::new(limiter));

        // First request to /ping succeeds.
        let req = Request::builder().uri("/ping").body(Body::empty()).unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // First request to /health should also succeed (different endpoint).
        let req = Request::builder()
            .uri("/health")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // -----------------------------------------------------------------------
    // IP-extraction branch tests
    //
    // Strategy: configure per-user limit = 1.  Two requests sharing the same
    // bucket key exhaust the allowance and the second returns 429.  Two
    // requests from *different* bucket keys each get their own fresh bucket
    // and both succeed.  This lets us assert the bucket key purely through
    // observable behavior without touching production code.
    // -----------------------------------------------------------------------

    /// X-Forwarded-For with a single IP is used directly as the bucket key.
    ///
    /// Two requests carrying `X-Forwarded-For: 198.51.100.7` must share one
    /// bucket: first OK, second 429.
    #[tokio::test]
    async fn x_forwarded_for_single_ip_uses_correct_bucket() {
        // per-user limit = 1 so a second request from the same key gets 429.
        let limiter = Arc::new(MultiKeyRateLimiter::new(
            RateLimitConfig::per_second(100), // global — not the bottleneck
            RateLimitConfig::per_second(1),   // per-user — the limit under test
            RateLimitConfig::per_second(100), // per-endpoint — not the bottleneck
        ));
        let app = test_router_trusting_proxy(limiter);

        let make_req = || {
            Request::builder()
                .uri("/ping")
                .header("x-forwarded-for", "198.51.100.7")
                .body(Body::empty())
                .unwrap()
        };

        // First request from 198.51.100.7 — must succeed (bucket has 1 token).
        let resp = app.clone().oneshot(make_req()).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "first request with X-Forwarded-For single IP should be allowed"
        );

        // Second request from the SAME IP — must be rejected (bucket exhausted).
        let resp = app.clone().oneshot(make_req()).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "second request from same X-Forwarded-For IP must share the bucket and be rate-limited"
        );
    }

    /// X-Forwarded-For with a multi-hop list uses only the leftmost (client) IP.
    ///
    /// The header `198.51.100.7, 10.0.0.1, 10.0.0.2` represents a request
    /// that traversed two proxies.  Only `198.51.100.7` (the originating
    /// client) should be the bucket key.
    ///
    /// Proof: a request with the full multi-hop header shares a bucket with a
    /// request carrying only `198.51.100.7` (both exhaust the same per-user
    /// slot), while a request carrying only `10.0.0.2` (the rightmost proxy)
    /// gets a *fresh* bucket and succeeds.
    #[tokio::test]
    async fn x_forwarded_for_multi_hop_uses_leftmost_ip() {
        let limiter = Arc::new(MultiKeyRateLimiter::new(
            RateLimitConfig::per_second(100),
            RateLimitConfig::per_second(1), // per-user limit under test
            RateLimitConfig::per_second(100),
        ));
        let app = test_router_trusting_proxy(limiter);

        // Request A: full multi-hop list — leftmost is 198.51.100.7.
        let req_multi = Request::builder()
            .uri("/ping")
            .header("x-forwarded-for", "198.51.100.7, 10.0.0.1, 10.0.0.2")
            .body(Body::empty())
            .unwrap();

        // Request B: only the leftmost IP, simulating the same client via a
        // different proxy chain.  Must land in the SAME bucket as A.
        let req_leftmost = Request::builder()
            .uri("/ping")
            .header("x-forwarded-for", "198.51.100.7")
            .body(Body::empty())
            .unwrap();

        // Request C: only the rightmost proxy IP.  If the middleware were
        // accidentally using the rightmost entry, this would be in the same
        // bucket as A — but it must NOT be.
        let req_rightmost = Request::builder()
            .uri("/ping")
            .header("x-forwarded-for", "10.0.0.2")
            .body(Body::empty())
            .unwrap();

        // A exhausts the bucket for 198.51.100.7.
        let resp = app.clone().oneshot(req_multi).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "multi-hop XFF first request should be allowed"
        );

        // B shares the same bucket (leftmost = 198.51.100.7) → 429.
        let resp = app.clone().oneshot(req_leftmost).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "request with only the leftmost IP must share the bucket with the multi-hop request"
        );

        // C uses a different key (10.0.0.2) → fresh bucket → 200.
        let resp = app.clone().oneshot(req_rightmost).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "request with only the rightmost proxy IP must use a separate bucket and be allowed"
        );
    }

    /// X-Real-IP is used as the bucket key when X-Forwarded-For is absent.
    ///
    /// Two requests with `X-Real-IP: 203.0.113.42` must share one bucket.
    #[tokio::test]
    async fn x_real_ip_fallback_uses_correct_bucket() {
        let limiter = Arc::new(MultiKeyRateLimiter::new(
            RateLimitConfig::per_second(100),
            RateLimitConfig::per_second(1), // per-user limit under test
            RateLimitConfig::per_second(100),
        ));
        let app = test_router_trusting_proxy(limiter);

        let make_req = || {
            Request::builder()
                .uri("/ping")
                .header("x-real-ip", "203.0.113.42")
                .body(Body::empty())
                .unwrap()
        };

        // First request from 203.0.113.42 via X-Real-IP — must succeed.
        let resp = app.clone().oneshot(make_req()).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "first request with X-Real-IP should be allowed"
        );

        // Second request from the SAME X-Real-IP — must be rate-limited.
        let resp = app.clone().oneshot(make_req()).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "second request from same X-Real-IP must share the bucket and be rate-limited"
        );

        // A request with a DIFFERENT X-Real-IP must land in a fresh bucket.
        let req_other = Request::builder()
            .uri("/ping")
            .header("x-real-ip", "203.0.113.99")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req_other).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "request with a different X-Real-IP must use a separate bucket and be allowed"
        );
    }

    /// When neither X-Forwarded-For nor X-Real-IP is present (and there is no
    /// ConnectInfo), all requests fall into the single "unknown" bucket.
    ///
    /// Two header-less requests must share that bucket: first OK, second 429.
    #[tokio::test]
    async fn unknown_fallback_shares_single_bucket() {
        let limiter = Arc::new(MultiKeyRateLimiter::new(
            RateLimitConfig::per_second(100),
            RateLimitConfig::per_second(1), // per-user limit under test
            RateLimitConfig::per_second(100),
        ));
        let app = test_router(limiter);

        let make_req = || Request::builder().uri("/ping").body(Body::empty()).unwrap();

        // First header-less request — "unknown" bucket has 1 token → OK.
        let resp = app.clone().oneshot(make_req()).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "first request with no IP headers should fall into the 'unknown' bucket and be allowed"
        );

        // Second header-less request — same "unknown" bucket, now exhausted → 429.
        let resp = app.clone().oneshot(make_req()).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "second request with no IP headers must share the 'unknown' bucket and be rate-limited"
        );
    }

    /// X-Forwarded-For present but X-Real-IP also present: XFF must win.
    ///
    /// The middleware checks XFF first; X-Real-IP is only a fallback.  A
    /// request with both headers must be keyed on the XFF value, not
    /// X-Real-IP.
    #[tokio::test]
    async fn x_forwarded_for_takes_precedence_over_x_real_ip() {
        let limiter = Arc::new(MultiKeyRateLimiter::new(
            RateLimitConfig::per_second(100),
            RateLimitConfig::per_second(1), // per-user limit under test
            RateLimitConfig::per_second(100),
        ));
        let app = test_router_trusting_proxy(limiter);

        // Request A: both headers present — should be keyed on XFF IP.
        let req_both = Request::builder()
            .uri("/ping")
            .header("x-forwarded-for", "198.51.100.7")
            .header("x-real-ip", "203.0.113.42")
            .body(Body::empty())
            .unwrap();

        // Request B: only XFF, same XFF value as A — must share A's bucket.
        let req_xff_only = Request::builder()
            .uri("/ping")
            .header("x-forwarded-for", "198.51.100.7")
            .body(Body::empty())
            .unwrap();

        // Request C: only X-Real-IP with the same value as A's X-Real-IP
        // header — must be in a DIFFERENT bucket (XFF took precedence in A).
        let req_xri_only = Request::builder()
            .uri("/ping")
            .header("x-real-ip", "203.0.113.42")
            .body(Body::empty())
            .unwrap();

        // A exhausts the 198.51.100.7 bucket.
        let resp = app.clone().oneshot(req_both).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "first request with both headers should be allowed"
        );

        // B shares the XFF bucket → 429.
        let resp = app.clone().oneshot(req_xff_only).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "request with same XFF IP must share the bucket with the dual-header request"
        );

        // C uses the X-Real-IP bucket (203.0.113.42) which was never touched → 200.
        let resp = app.clone().oneshot(req_xri_only).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "request with only X-Real-IP must use a separate bucket from the XFF-keyed request"
        );
    }

    /// X-Forwarded-For present but with whitespace padding around the IP.
    ///
    /// The code calls `.trim()` on the first segment, so `" 198.51.100.7 "`
    /// must produce the same bucket key as `"198.51.100.7"`.
    #[tokio::test]
    async fn x_forwarded_for_whitespace_is_trimmed() {
        let limiter = Arc::new(MultiKeyRateLimiter::new(
            RateLimitConfig::per_second(100),
            RateLimitConfig::per_second(1), // per-user limit under test
            RateLimitConfig::per_second(100),
        ));
        let app = test_router_trusting_proxy(limiter);

        // Request A: padded whitespace around the IP.
        let req_padded = Request::builder()
            .uri("/ping")
            .header("x-forwarded-for", "  198.51.100.7  ")
            .body(Body::empty())
            .unwrap();

        // Request B: no padding — must land in the SAME bucket as A.
        let req_clean = Request::builder()
            .uri("/ping")
            .header("x-forwarded-for", "198.51.100.7")
            .body(Body::empty())
            .unwrap();

        // A exhausts the 198.51.100.7 bucket.
        let resp = app.clone().oneshot(req_padded).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "first request with padded XFF should be allowed"
        );

        // B must be in the same trimmed bucket → 429.
        let resp = app.clone().oneshot(req_clean).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "whitespace-padded XFF must produce the same bucket key as the clean IP"
        );
    }

    /// X-Real-IP whitespace must also be trimmed (Wave 3C regression test).
    #[tokio::test]
    async fn x_real_ip_whitespace_is_trimmed() {
        let limiter = Arc::new(MultiKeyRateLimiter::new(
            RateLimitConfig::per_second(100),
            RateLimitConfig::per_second(1),
            RateLimitConfig::per_second(100),
        ));
        let app = test_router_trusting_proxy(limiter);

        let req_padded = Request::builder()
            .uri("/ping")
            .header("x-real-ip", "  203.0.113.42  ")
            .body(Body::empty())
            .unwrap();
        let req_clean = Request::builder()
            .uri("/ping")
            .header("x-real-ip", "203.0.113.42")
            .body(Body::empty())
            .unwrap();
        let req_other = Request::builder()
            .uri("/ping")
            .header("x-real-ip", "198.51.100.99")
            .body(Body::empty())
            .unwrap();

        let resp = app.clone().oneshot(req_padded).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "padded X-Real-IP allowed first time"
        );
        let resp = app.clone().oneshot(req_clean).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "trimmed X-Real-IP shares bucket"
        );
        let resp = app.clone().oneshot(req_other).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "different X-Real-IP gets fresh bucket"
        );
    }
}
