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
            // Extract client IP from connection info or X-Forwarded-For header.
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
                        .map(|s| s.to_string())
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
}
