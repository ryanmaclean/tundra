//! API key authentication middleware for the auto-tundra HTTP API.
//!
//! When a configured API key is present, HTTP requests carry `X-API-Key` or
//! `Authorization: Bearer <token>`. Native WebSocket clients can do the same;
//! the legacy `?api_key=` WebSocket-only query path remains deprecated. Browser
//! clients issue a protected, one-use route-bound ticket and present it in a
//! WebSocket subprotocol offer, which this middleware consumes before 101.
//! See [`at_api_types::auth`] for the wire contract. With no configured key,
//! requests pass through (development mode).
//!
//! Empty keys never authenticate: an empty configured key rejects everything
//! and an empty presented key is treated as missing.

use crate::{origin_validation::OriginAllowlist, ws_tickets::WsTicketStore};
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
    ws_tickets: Option<WsTicketStore>,
    origins: Option<OriginAllowlist>,
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
            ws_tickets: None,
            origins: None,
        }
    }

    /// Protected API router's browser-ticket path. Ordinary native clients
    /// continue to authenticate with header, Bearer, or legacy WS query key.
    pub fn with_ws_tickets(
        api_key: Option<String>,
        ws_tickets: WsTicketStore,
        origins: OriginAllowlist,
    ) -> Self {
        let mut layer = Self::new(api_key);
        layer.ws_tickets = Some(ws_tickets);
        layer.origins = Some(origins);
        layer
    }
}

impl<S> Layer<S> for AuthLayer {
    type Service = AuthMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthMiddleware {
            inner,
            api_key: self.api_key.clone(),
            ws_tickets: self.ws_tickets.clone(),
            origins: self.origins.clone(),
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
    ws_tickets: Option<WsTicketStore>,
    origins: Option<OriginAllowlist>,
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
        let ws_tickets = self.ws_tickets.clone();
        let origins = self.origins.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            // If no API key is configured, pass through (dev mode).
            let expected = match api_key {
                Some(k) => k,
                None => return inner.call(req).await,
            };

            let provided = presented_api_key(&req);

            // A supplied but wrong master key has precedence. It cannot fall
            // through to a valid ticket on the same request.
            match provided {
                PresentedCredential::Key(token) => {
                    if !token.is_empty()
                        && !expected.is_empty()
                        && bool::from(token.as_bytes().ct_eq(expected.as_bytes()))
                    {
                        return inner.call(req).await;
                    }
                }
                PresentedCredential::Rejected => {}
                PresentedCredential::Absent => {
                    if is_ticket_websocket_upgrade(&req) {
                        if let (Some(store), Some(origins)) = (ws_tickets, origins) {
                            if origins.validate(req.headers()).is_ok()
                                && store.consume(req.headers(), req.uri().path()).await
                            {
                                return inner.call(req).await;
                            }
                        }
                    }
                }
            }
            let resp = (
                StatusCode::UNAUTHORIZED,
                axum::Json(serde_json::json!({"error": "unauthorized"})),
            )
                .into_response();
            Ok(resp)
        })
    }
}

/// A present credential is never equivalent to an absent credential, even if
/// its bytes cannot be decoded or its authorization scheme is unsupported.
enum PresentedCredential {
    Absent,
    Key(String),
    Rejected,
}

/// Extract the API key a request presents.
///
/// Order: `X-API-Key` header, `Authorization: Bearer <key>`, then (for
/// WebSocket upgrades only) the `api_key` query parameter.
fn presented_api_key(req: &Request<Body>) -> PresentedCredential {
    let headers = req.headers();
    let mut keys = headers.get_all(API_KEY_HEADER).iter();
    if let Some(raw) = keys.next() {
        if keys.next().is_some() { return PresentedCredential::Rejected; }
        return raw.to_str().map(|s| PresentedCredential::Key(s.to_owned()))
            .unwrap_or(PresentedCredential::Rejected);
    }
    let mut authorizations = headers.get_all(axum::http::header::AUTHORIZATION).iter();
    if let Some(raw) = authorizations.next() {
        if authorizations.next().is_some() { return PresentedCredential::Rejected; }
        return raw.to_str().ok()
            .and_then(|s| s.strip_prefix("Bearer "))
            .map(|s| PresentedCredential::Key(s.to_owned()))
            .unwrap_or(PresentedCredential::Rejected);
    }
    if is_websocket_upgrade(req) {
        if let Some(key) = ws_query_api_key(req.uri()) {
            return PresentedCredential::Key(key);
        }
    }
    PresentedCredential::Absent
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

/// The ticket branch demands a complete-looking WebSocket handshake before
/// consuming a one-use value. Axum performs the final protocol validation.
fn is_ticket_websocket_upgrade(req: &Request<Body>) -> bool {
    if !is_websocket_upgrade(req) || req.uri().query().is_some() {
        return false;
    }
    let headers = req.headers();
    headers
        .get(axum::http::header::CONNECTION)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| {
            s.split(',')
                .any(|part| part.trim().eq_ignore_ascii_case("upgrade"))
        })
        && headers
            .get("sec-websocket-version")
            .is_some_and(|v| v == "13")
        && headers.contains_key("sec-websocket-key")
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
        let resp = app
            .oneshot(ws_upgrade("/ping?api_key=secret123"))
            .await
            .unwrap();
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

    fn ticket_request(path: &str, ticket: uuid::Uuid) -> Request<Body> {
        Request::builder()
            .uri(path)
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
            .header("Origin", "http://localhost:3000")
            .header(
                "Sec-WebSocket-Protocol",
                format!("tundra.v2, tundra.ticket.{ticket}"),
            )
            .body(Body::empty())
            .unwrap()
    }

    #[tokio::test]
    async fn ticket_is_one_use_and_native_without_protocol_still_works() {
        let store = WsTicketStore::default();
        let app = Router::new()
            .route("/ws", get(|| async { "authorized" }))
            .route("/api/events/ws", get(|| async { "authorized" }))
            .layer(AuthLayer::with_ws_tickets(
                Some("secret123".into()),
                store.clone(),
                OriginAllowlist::default(),
            ));
        let ticket = store.issue("/ws", "http://localhost:3000").await.unwrap();
        assert_eq!(
            app.clone()
                .oneshot(ticket_request("/api/events/ws", ticket))
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        let mut no_origin = ticket_request("/ws", ticket);
        no_origin.headers_mut().remove("Origin");
        assert_eq!(
            app.clone().oneshot(no_origin).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
        let mut wrong_key = ticket_request("/ws", ticket);
        wrong_key
            .headers_mut()
            .insert("X-API-Key", "wrong".parse().unwrap());
        assert_eq!(
            app.clone().oneshot(wrong_key).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
        let non_ascii = axum::http::HeaderValue::from_bytes(b"hello\xfa").unwrap();
        let mut malformed_key = ticket_request("/ws", ticket);
        malformed_key.headers_mut().insert("X-API-Key", non_ascii.clone());
        assert_eq!(
            app.clone().oneshot(malformed_key).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
        let mut malformed_authorization = ticket_request("/ws", ticket);
        malformed_authorization.headers_mut().insert("Authorization", non_ascii);
        assert_eq!(
            app.clone().oneshot(malformed_authorization).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
        let mut unsupported_scheme = ticket_request("/ws", ticket);
        unsupported_scheme.headers_mut().insert("Authorization", "Basic abc".parse().unwrap());
        assert_eq!(
            app.clone().oneshot(unsupported_scheme).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
        let mut duplicate_key = ticket_request("/ws", ticket);
        duplicate_key.headers_mut().append("X-API-Key", "secret123".parse().unwrap());
        duplicate_key.headers_mut().append("X-API-Key", "secret123".parse().unwrap());
        assert_eq!(
            app.clone().oneshot(duplicate_key).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
        let mut duplicate_authorization = ticket_request("/ws", ticket);
        duplicate_authorization.headers_mut().append("Authorization", "Bearer secret123".parse().unwrap());
        duplicate_authorization.headers_mut().append("Authorization", "Bearer secret123".parse().unwrap());
        assert_eq!(
            app.clone().oneshot(duplicate_authorization).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            app.clone()
                .oneshot(ticket_request("/ws", ticket))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            app.clone()
                .oneshot(ticket_request("/ws", ticket))
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        let native = Request::builder()
            .uri("/ws")
            .header("X-API-Key", "secret123")
            .header("Origin", "http://localhost:3000")
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            app.clone().oneshot(native).await.unwrap().status(),
            StatusCode::OK
        );
        let bearer = Request::builder()
            .uri("/ws")
            .header("Authorization", "Bearer secret123")
            .header("Origin", "http://localhost:3000")
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            app.clone().oneshot(bearer).await.unwrap().status(),
            StatusCode::OK
        );
        let query = ws_upgrade("/ws?api_key=secret123");
        assert_eq!(app.oneshot(query).await.unwrap().status(), StatusCode::OK);
    }
}
