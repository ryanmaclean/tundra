//! API key authentication middleware for the auto-tundra HTTP API.
//!
//! When a configured API key is present, every request must carry it via
//! either the `X-API-Key` header or the `Authorization: Bearer <token>` header.
//! When no API key is configured (the `Option` is `None`), all requests are
//! allowed through (development mode).

use axum::{
    body::Body,
    http::{Request, Response, StatusCode},
    response::IntoResponse,
};
use std::sync::Arc;
use std::task::{Context, Poll};
use subtle::ConstantTimeEq;
use tower::{Layer, Service};

// ---------------------------------------------------------------------------
// AuthLayer
// ---------------------------------------------------------------------------

/// A [`tower::Layer`] that wraps services with [`AuthMiddleware`].
#[derive(Clone)]
pub struct AuthLayer {
    /// `None` = development mode (all requests pass through).
    api_key: Option<Arc<String>>,
}

impl AuthLayer {
    /// Create a new `AuthLayer`.
    ///
    /// * `api_key` -- `Some(key)` to enforce auth, `None` to allow all.
    pub fn new(api_key: Option<String>) -> Self {
        Self {
            api_key: api_key.map(Arc::new),
        }
    }
}

impl<S> Layer<S> for AuthLayer {
    type Service = AuthMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthMiddleware {
            inner,
            api_key: self.api_key.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// AuthMiddleware
// ---------------------------------------------------------------------------

/// The actual middleware service produced by [`AuthLayer`].
#[derive(Clone)]
pub struct AuthMiddleware<S> {
    inner: S,
    api_key: Option<Arc<String>>,
}

impl<S> Service<Request<Body>> for AuthMiddleware<S>
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
        let api_key = self.api_key.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            // If no API key is configured, pass through (dev mode).
            let expected = match api_key {
                Some(k) => k,
                None => return inner.call(req).await,
            };

            // Try X-API-Key header first, then Authorization: Bearer <token>.
            let provided = req
                .headers()
                .get("x-api-key")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
                .or_else(|| {
                    req.headers()
                        .get("authorization")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.strip_prefix("Bearer "))
                        .map(|s| s.to_string())
                });

            match provided {
                Some(ref token) if bool::from(token.as_bytes().ct_eq(expected.as_bytes())) => {
                    inner.call(req).await
                }
                _ => {
                    let resp = (
                        StatusCode::UNAUTHORIZED,
                        axum::Json(serde_json::json!({"error": "unauthorized"})),
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
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    fn test_router(api_key: Option<String>) -> Router {
        Router::new()
            .route("/ping", get(|| async { "pong" }))
            .layer(AuthLayer::new(api_key))
    }

    #[tokio::test]
    async fn no_key_configured_allows_all() {
        let app = test_router(None);
        let req = Request::builder().uri("/ping").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn valid_x_api_key_header() {
        let app = test_router(Some("secret123".into()));
        let req = Request::builder()
            .uri("/ping")
            .header("X-API-Key", "secret123")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn valid_bearer_token() {
        let app = test_router(Some("secret123".into()));
        let req = Request::builder()
            .uri("/ping")
            .header("Authorization", "Bearer secret123")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn missing_key_returns_401() {
        let app = test_router(Some("secret123".into()));
        let req = Request::builder().uri("/ping").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn wrong_key_returns_401() {
        let app = test_router(Some("secret123".into()));
        let req = Request::builder()
            .uri("/ping")
            .header("X-API-Key", "wrong")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn wrong_bearer_returns_401() {
        let app = test_router(Some("secret123".into()));
        let req = Request::builder()
            .uri("/ping")
            .header("Authorization", "Bearer wrong")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
