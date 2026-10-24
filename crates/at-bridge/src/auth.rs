//! API key authentication middleware for the auto-tundra HTTP API.
//!
//! When a configured API key is present, every request must carry it via
//! either the `X-API-Key` header or the `Authorization: Bearer <token>` header.
//! The key is **never** accepted from a URL query: URLs land in access logs,
//! proxies, browser history/devtools and crash reports, and the key is the
//! reusable owner credential.
//!
//! Browsers cannot set custom headers on a WebSocket handshake, so WebSocket
//! upgrade requests (`GET` + `Upgrade: websocket`) may instead present a
//! single-use ticket as the `?ws_ticket=` query parameter. Tickets are issued
//! by `POST /api/v1/ws-tickets` (itself key-authenticated, see
//! [`issue_ws_ticket`]), are bound to one WebSocket path, are burned on first
//! presentation and expire after
//! [`WS_TICKET_TTL_SECS`](at_api_types::auth::WS_TICKET_TTL_SECS) seconds. See
//! [`at_api_types::auth`] for the wire contract. When no API key is
//! configured (the `Option` is `None`), all requests are allowed through
//! (development mode).
//!
//! Empty keys never authenticate: an empty configured key rejects everything
//! and an empty presented key is treated as missing.

use at_api_types::auth::{
    is_ws_ticket_path, WsTicketRequest, WsTicketResponse, API_KEY_HEADER, WS_TICKET_QUERY_PARAM,
    WS_TICKET_SCHEMA_VERSION, WS_TICKET_TTL_SECS,
};
use axum::{
    body::Body,
    extract::Query,
    http::{Request, Response, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use subtle::ConstantTimeEq;
use tower::{Layer, Service};

// ---------------------------------------------------------------------------
// WebSocket tickets
// ---------------------------------------------------------------------------

/// Upper bound on outstanding (unredeemed, unexpired) tickets.
const WS_TICKET_MAX_OUTSTANDING: usize = 256;

/// Why a ticket could not be issued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WsTicketError {
    /// The requested path is not one of the WebSocket routes.
    InvalidPath,
    /// Too many outstanding tickets; retry after some expire.
    TooMany,
}

/// In-memory store of single-use, path-bound, short-lived WebSocket tickets.
///
/// Shared by [`AuthLayer`] (redeems) and [`issue_ws_ticket`] (issues).
#[derive(Debug)]
pub struct WsTicketStore {
    ttl: Duration,
    /// ticket -> (bound path, expiry)
    tickets: Mutex<HashMap<String, (String, Instant)>>,
}

impl Default for WsTicketStore {
    fn default() -> Self {
        Self::new()
    }
}

impl WsTicketStore {
    /// A store whose tickets live [`WS_TICKET_TTL_SECS`] seconds.
    pub fn new() -> Self {
        Self::with_ttl(Duration::from_secs(WS_TICKET_TTL_SECS))
    }

    /// A store with a custom ticket lifetime (tests).
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            ttl,
            tickets: Mutex::new(HashMap::new()),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, (String, Instant)>> {
        // A poisoned lock only means another thread panicked mid-insert; the
        // map is still a valid map, so keep serving rather than failing open
        // or closed forever.
        self.tickets.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Issue a ticket that opens exactly `path` once.
    pub fn issue(&self, path: &str) -> Result<String, WsTicketError> {
        if !is_ws_ticket_path(path) {
            return Err(WsTicketError::InvalidPath);
        }
        let now = Instant::now();
        let mut map = self.lock();
        if map.len() >= WS_TICKET_MAX_OUTSTANDING {
            map.retain(|_, (_, exp)| *exp > now);
            if map.len() >= WS_TICKET_MAX_OUTSTANDING {
                return Err(WsTicketError::TooMany);
            }
        }
        // Two v4 UUIDs: 244 bits from the OS CSPRNG (getrandom), hex only.
        let ticket = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        map.insert(ticket.clone(), (path.to_string(), now + self.ttl));
        Ok(ticket)
    }

    /// Redeem `ticket` for `path`. The ticket is removed whatever the
    /// outcome, so a ticket presented on the wrong route or after expiry can
    /// never be retried.
    pub fn redeem(&self, ticket: &str, path: &str) -> bool {
        if ticket.is_empty() {
            return false;
        }
        let now = Instant::now();
        let mut map = self.lock();
        let Some((bound, exp)) = map.remove(ticket) else {
            return false;
        };
        exp > now && bound == path
    }

    /// Number of outstanding tickets (tests/metrics).
    pub fn outstanding(&self) -> usize {
        self.lock().len()
    }
}

/// `POST /api/v1/ws-tickets` -- issue a single-use WebSocket ticket.
///
/// Mounted behind [`AuthLayer`], so the caller has already proved the key via
/// a header. Body: [`WsTicketRequest`]; response: [`WsTicketResponse`].
pub async fn issue_ws_ticket(
    Extension(store): Extension<Arc<WsTicketStore>>,
    Json(req): Json<WsTicketRequest>,
) -> axum::response::Response {
    match store.issue(&req.path) {
        Ok(ticket) => (
            StatusCode::CREATED,
            Json(WsTicketResponse {
                schema_version: WS_TICKET_SCHEMA_VERSION.to_string(),
                ticket,
                path: req.path,
                query_param: WS_TICKET_QUERY_PARAM.to_string(),
                expires_in_secs: store.ttl.as_secs(),
            }),
        )
            .into_response(),
        Err(WsTicketError::InvalidPath) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "path is not a WebSocket route",
                "allowed": ["/ws", "/api/events/ws", "/ws/terminal/{id}"],
            })),
        )
            .into_response(),
        Err(WsTicketError::TooMany) => (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({"error": "too many outstanding WebSocket tickets"})),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// AuthLayer
// ---------------------------------------------------------------------------

/// A [`tower::Layer`] that wraps services with [`AuthMiddleware`].
#[derive(Clone)]
pub struct AuthLayer {
    /// `None` = development mode (all requests pass through).
    api_key: Option<Arc<String>>,
    tickets: Arc<WsTicketStore>,
}

impl AuthLayer {
    /// Create a new `AuthLayer` with its own (private) ticket store.
    ///
    /// * `api_key` -- `Some(key)` to enforce auth, `None` to allow all.
    pub fn new(api_key: Option<String>) -> Self {
        Self::with_tickets(api_key, Arc::new(WsTicketStore::new()))
    }

    /// Create an `AuthLayer` that redeems WebSocket tickets from `tickets`
    /// (the store [`issue_ws_ticket`] issues into).
    pub fn with_tickets(api_key: Option<String>, tickets: Arc<WsTicketStore>) -> Self {
        if matches!(api_key.as_deref(), Some(k) if k.trim().is_empty()) {
            tracing::error!("empty API key configured; all requests will be rejected");
        }
        Self {
            api_key: api_key.map(Arc::new),
            tickets,
        }
    }
}

impl<S> Layer<S> for AuthLayer {
    type Service = AuthMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthMiddleware {
            inner,
            api_key: self.api_key.clone(),
            tickets: self.tickets.clone(),
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
    tickets: Arc<WsTicketStore>,
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
        let tickets = self.tickets.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            // If no API key is configured, pass through (dev mode).
            let expected = match api_key {
                Some(k) => k,
                None => return inner.call(req).await,
            };

            let key_ok = matches!(
                presented_api_key(&req),
                Some(ref token)
                    if !token.is_empty()
                        && !expected.is_empty()
                        && bool::from(token.as_bytes().ct_eq(expected.as_bytes()))
            );
            // An empty configured key fails closed for tickets too.
            let ticket_ok = !key_ok
                && !expected.is_empty()
                && is_websocket_upgrade(&req)
                && ws_query_ticket(req.uri()).is_some_and(|t| tickets.redeem(&t, req.uri().path()));

            if key_ok || ticket_ok {
                inner.call(req).await
            } else {
                let resp = (
                    StatusCode::UNAUTHORIZED,
                    axum::Json(serde_json::json!({"error": "unauthorized"})),
                )
                    .into_response();
                Ok(resp)
            }
        })
    }
}

/// Extract the API key a request presents: `X-API-Key` header, then
/// `Authorization: Bearer <key>`. Never from the URL.
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

/// Percent-decoded value of the `ws_ticket` query parameter, if present.
fn ws_query_ticket(uri: &axum::http::Uri) -> Option<String> {
    let Query(mut params) = Query::<HashMap<String, String>>::try_from_uri(uri).ok()?;
    params.remove(WS_TICKET_QUERY_PARAM)
}

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

    /// Router with the real WS route shapes behind one shared ticket store.
    fn ticket_router(api_key: Option<&str>, store: Arc<WsTicketStore>) -> Router {
        Router::new()
            .route("/ws", get(|| async { "ws" }))
            .route("/api/events/ws", get(|| async { "events" }))
            .route("/ws/terminal/{id}", get(|| async { "term" }))
            .route("/ping", get(|| async { "pong" }))
            .layer(AuthLayer::with_tickets(api_key.map(String::from), store))
    }

    async fn status(app: &Router, req: Request<Body>) -> StatusCode {
        app.clone().oneshot(req).await.unwrap().status()
    }

    #[tokio::test]
    async fn ws_ticket_opens_its_path_exactly_once() {
        let store = Arc::new(WsTicketStore::new());
        let app = ticket_router(Some("secret123"), store.clone());
        for path in ["/ws", "/api/events/ws", "/ws/terminal/t-1"] {
            let t = store.issue(path).unwrap();
            assert!(!t.contains("secret123"));
            let uri = format!("{path}?ws_ticket={t}");
            assert_eq!(
                status(&app, ws_upgrade(&uri)).await,
                StatusCode::OK,
                "{path}"
            );
            // Replay of the same ticket is refused.
            assert_eq!(
                status(&app, ws_upgrade(&uri)).await,
                StatusCode::UNAUTHORIZED,
                "replay {path}"
            );
        }
        assert_eq!(store.outstanding(), 0);
    }

    #[tokio::test]
    async fn ws_ticket_for_another_route_is_refused_and_burned() {
        let store = Arc::new(WsTicketStore::new());
        let app = ticket_router(Some("secret123"), store.clone());
        let t = store.issue("/ws/terminal/a").unwrap();
        for wrong in ["/ws/terminal/b", "/ws", "/api/events/ws"] {
            assert_eq!(
                status(&app, ws_upgrade(&format!("{wrong}?ws_ticket={t}"))).await,
                StatusCode::UNAUTHORIZED,
                "{wrong}"
            );
        }
        // Burned by the first wrong-route attempt: no longer opens its own path.
        assert_eq!(
            status(&app, ws_upgrade(&format!("/ws/terminal/a?ws_ticket={t}"))).await,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn expired_ws_ticket_is_refused() {
        let store = Arc::new(WsTicketStore::with_ttl(Duration::from_millis(0)));
        let app = ticket_router(Some("secret123"), store.clone());
        let t = store.issue("/ws").unwrap();
        assert_eq!(
            status(&app, ws_upgrade(&format!("/ws?ws_ticket={t}"))).await,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn ws_ticket_only_works_on_websocket_upgrades() {
        let store = Arc::new(WsTicketStore::new());
        let app = ticket_router(Some("secret123"), store.clone());
        let t = store.issue("/ws").unwrap();
        let plain = Request::builder()
            .uri(format!("/ws?ws_ticket={t}"))
            .body(Body::empty())
            .unwrap();
        assert_eq!(status(&app, plain).await, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn forged_or_empty_ws_ticket_is_refused() {
        let store = Arc::new(WsTicketStore::new());
        let app = ticket_router(Some("secret123"), store.clone());
        let _real = store.issue("/ws").unwrap();
        for uri in [
            "/ws?ws_ticket=",
            "/ws?ws_ticket=deadbeef",
            "/ws?ws_ticket=secret123",
        ] {
            assert_eq!(
                status(&app, ws_upgrade(uri)).await,
                StatusCode::UNAUTHORIZED,
                "{uri}"
            );
        }
    }

    #[tokio::test]
    async fn empty_configured_key_refuses_tickets_too() {
        let store = Arc::new(WsTicketStore::new());
        let app = ticket_router(Some(""), store.clone());
        let t = store.issue("/ws").unwrap();
        assert_eq!(
            status(&app, ws_upgrade(&format!("/ws?ws_ticket={t}"))).await,
            StatusCode::UNAUTHORIZED
        );
    }

    #[test]
    fn tickets_are_only_issued_for_ws_routes_and_are_bounded() {
        let store = WsTicketStore::new();
        for bad in ["/api/status", "/ws/terminal/", "/ws/terminal/a/b", "/ping"] {
            assert_eq!(store.issue(bad), Err(WsTicketError::InvalidPath), "{bad}");
        }
        let a = store.issue("/ws").unwrap();
        let b = store.issue("/ws").unwrap();
        assert_ne!(a, b);
        assert_eq!(a.len(), 64);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
        for _ in 2..WS_TICKET_MAX_OUTSTANDING {
            store.issue("/ws").unwrap();
        }
        assert_eq!(store.issue("/ws"), Err(WsTicketError::TooMany));
    }

    #[tokio::test]
    async fn ws_upgrade_wrong_query_key_returns_401() {
        let app = test_router(Some("secret123".into()));
        let resp = app.oneshot(ws_upgrade("/ping?api_key=nope")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn query_key_ignored_on_plain_http_request() {
        // The master key is never read from a URL, WS handshake or not.
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

    /// Security invariant (PR #101 review 2026-10-08): the reusable master
    /// key must never authenticate from a URL query, even on a WebSocket
    /// upgrade. URLs land in logs, proxies, devtools and crash reports.
    #[tokio::test]
    async fn ws_upgrade_never_accepts_master_key_in_query() {
        const SENTINEL: &str = "SENTINEL-master-key-7f3c";
        for uri in [
            format!("/ping?api_key={SENTINEL}"),
            format!("/ping?x=1&api_key={SENTINEL}"),
        ] {
            let app = test_router(Some(SENTINEL.into()));
            let resp = app.oneshot(ws_upgrade(&uri)).await.unwrap();
            assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "{uri}");
        }
    }
}
