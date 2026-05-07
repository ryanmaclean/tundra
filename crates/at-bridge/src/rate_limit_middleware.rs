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
//! Applies to each unique client IP address. Client IP is extracted from:
//! - `X-Forwarded-For` header (preferred, uses first IP in comma-separated list)
//! - `X-Real-IP` header (fallback)
//! - "unknown" if no IP headers are present
//!
//! This prevents any single client from monopolizing server resources.
//!
//! ## 3. Per-Endpoint Rate Limit
//! Applies to each unique URI path (e.g., `/api/tasks`, `/api/beads`).
//! Each endpoint has its own independent rate limit bucket per client.
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
    extract::Request,
    http::{Response, StatusCode},
    response::IntoResponse,
};
use std::sync::Arc;
use std::task::{Context, Poll};
use tower::{Layer, Service};
use tracing::warn;

use at_harness::rate_limiter::MultiKeyRateLimiter;

// ---------------------------------------------------------------------------
// RateLimitLayer
// ---------------------------------------------------------------------------

/// A [`tower::Layer`] that wraps services with [`RateLimitMiddleware`].
#[derive(Clone)]
pub struct RateLimitLayer {
    rate_limiter: Arc<MultiKeyRateLimiter>,
}

impl RateLimitLayer {
    /// Create a new `RateLimitLayer` with the given rate limiter.
    pub fn new(rate_limiter: Arc<MultiKeyRateLimiter>) -> Self {
        Self { rate_limiter }
    }
}

impl<S> Layer<S> for RateLimitLayer {
    type Service = RateLimitMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RateLimitMiddleware {
            inner,
            rate_limiter: self.rate_limiter.clone(),
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
        let mut inner = self.inner.clone();

        Box::pin(async move {
            // Extract client IP from X-Forwarded-For (leftmost), falling back to X-Real-IP, then "unknown".
            let client_ip = req
                .headers()
                .get("x-forwarded-for")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.split(',').next())
                .map(|s| s.trim().to_string())
                .or_else(|| {
                    req.headers()
                        .get("x-real-ip")
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.trim().to_string())
                })
                .unwrap_or_else(|| "unknown".to_string());

            // Extract endpoint path for per-endpoint limiting.
            let endpoint = req.uri().path().to_string();

            // Check all three rate limit tiers.
            match rate_limiter.check_all(&client_ip, &endpoint) {
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
        let app = test_router(limiter);

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
        let app = test_router(limiter);

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
        let app = test_router(limiter);

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
        let app = test_router(limiter);

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
        let app = test_router(limiter);

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
        let app = test_router(limiter);

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
        assert_eq!(resp.status(), StatusCode::OK, "padded X-Real-IP allowed first time");
        let resp = app.clone().oneshot(req_clean).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS, "trimmed X-Real-IP shares bucket");
        let resp = app.clone().oneshot(req_other).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "different X-Real-IP gets fresh bucket");
    }
}
