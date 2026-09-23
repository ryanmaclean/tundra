//! API key authentication middleware for the auto-tundra HTTP API.
//!
//! When a configured API key is present, every request must carry it via
//! either the `X-API-Key` header or the `Authorization: Bearer <token>` header.
//! WebSocket upgrade requests (`GET` + `Upgrade: websocket`) may instead pass
//! it as the `?api_key=` query parameter, because browsers cannot set custom
//! headers on a WebSocket handshake. See [`at_api_types::auth`] for the wire
//! contract. When no API key is configured (the `Option` is `None`), all
//! requests are allowed through (development mode).
//!
//! Empty keys never authenticate: an empty configured key rejects everything
//! and an empty presented key is treated as missing.

use at_api_types::auth::{API_KEY_HEADER, WS_API_KEY_QUERY_PARAM};
use axum::{
    body::Body,
    extract::Query,
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
        if matches!(api_key.as_deref(), Some(k) if k.trim().is_empty()) {
            tracing::error!("empty API key configured; all requests will be rejected");
        }
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

            let provided = presented_api_key(&req);

            match provided {
                Some(ref token)
                    if !token.is_empty()
                        && !expected.is_empty()
                        && bool::from(token.as_bytes().ct_eq(expected.as_bytes())) =>
                {
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

/// Extract the API key a request presents.
///
/// Order: `X-API-Key` header, `Authorization: Bearer <key>`, then (for
/// WebSocket upgrades only) the `api_key` query parameter.
fn presented_api_key(req: &Request<Body>) -> Option<String> {
    let headers = req.headers();
    headers
        .get(API_KEY_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| {
            headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
                .map(|s| s.to_string())
        })
        .or_else(|| {
            if is_websocket_upgrade(req) {
                ws_query_api_key(req.uri())
            } else {
                None
            }
        })
}

/// `true` for a WebSocket handshake: `GET` with `Upgrade: websocket`.
fn is_websocket_upgrade(req: &Request<Body>) -> bool {
    req.method() == axum::http::Method::GET
        && req
            .headers()
            .get(axum::http::header::UPGRADE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.eq_ignore_ascii_case("websocket"))
}

/// Percent-decoded value of the `api_key` query parameter, if present.
fn ws_query_api_key(uri: &axum::http::Uri) -> Option<String> {
    let Query(mut params) =
        Query::<std::collections::HashMap<String, String>>::try_from_uri(uri).ok()?;
    params.remove(WS_API_KEY_QUERY_PARAM)
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

    fn ws_upgrade(uri: &str) -> Request<Body> {
        Request::builder()
            .uri(uri)
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .body(Body::empty())
            .unwrap()
    }

    #[tokio::test]
    async fn ws_upgrade_accepts_query_key() {
        let app = test_router(Some("secret123".into()));
        let resp = app.oneshot(ws_upgrade("/ping?api_key=secret123")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn ws_upgrade_query_key_is_percent_decoded() {
        let app = test_router(Some("a+b/c=".into()));
        let resp = app
            .oneshot(ws_upgrade("/ping?x=1&api_key=a%2Bb%2Fc%3D"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn ws_upgrade_wrong_query_key_returns_401() {
        let app = test_router(Some("secret123".into()));
        let resp = app.oneshot(ws_upgrade("/ping?api_key=nope")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn query_key_ignored_on_plain_http_request() {
        // Keys in URLs leak into history/logs; only WS handshakes may use them.
        let app = test_router(Some("secret123".into()));
        let req = Request::builder()
            .uri("/ping?api_key=secret123")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn empty_presented_key_is_rejected() {
        let app = test_router(Some("secret123".into()));
        let req = Request::builder()
            .uri("/ping")
            .header("X-API-Key", "")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            app.oneshot(req).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn empty_configured_key_fails_closed() {
        let app = test_router(Some(String::new()));
        let req = Request::builder()
            .uri("/ping")
            .header("X-API-Key", "")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            app.oneshot(req).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
    }
}
