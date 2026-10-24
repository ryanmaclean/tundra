//! End-to-end auth tests: the real router with `Some(key)` (as every daemon
//! start path configures it) against the request shapes first-party clients
//! actually send.
//!
//! - CLI / TUI: reqwest with the `DaemonConnection::auth_header()` default header
//! - Web UI fetch: `X-API-Key` + `Origin`, including the CORS preflight
//! - Web UI / Tauri WebSocket: `POST /api/v1/ws-tickets` with `X-API-Key`,
//!   then `?ws_ticket=` on the upgrade + webview `Origin` (the master key is
//!   never put in a URL)

use std::sync::Arc;

use at_api_types::auth::{
    WsTicketResponse, API_KEY_ENV, API_KEY_HEADER, WS_TICKET_PATH, WS_TICKET_QUERY_PARAM,
};
use at_bridge::event_bus::EventBus;
use at_bridge::http_api::{api_router_with_auth, ApiState};
use at_bridge::origin_validation::TAURI_WEBVIEW_ORIGINS;
use at_core::lockfile::{DaemonConnection, DiscoverySource};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;

const KEY: &str = "e2e-test-key-0123456789abcdef";

async fn start(allowed_origins: Vec<String>, with_pty: bool) -> String {
    let bus = EventBus::new();
    let state = if with_pty {
        let pool = Arc::new(at_session::pty_pool::PtyPool::new(2));
        ApiState::with_pty_pool(bus, pool)
    } else {
        ApiState::new(bus)
    };
    let state = Arc::new(state.with_relaxed_rate_limits());
    let router = api_router_with_auth(state, Some(KEY.to_string()), allowed_origins);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    format!("http://{addr}")
}

/// Build a reqwest client exactly the way the CLI/TUI do.
fn native_client(conn: &DaemonConnection) -> reqwest::Client {
    let mut headers = reqwest::header::HeaderMap::new();
    if let Some((name, value)) = conn.auth_header() {
        headers.insert(name, value.parse().unwrap());
    }
    reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .unwrap()
}

fn ws_url(base: &str, path: &str, ticket: Option<&str>) -> String {
    let mut url = format!("{}{path}", base.replace("http://", "ws://"));
    if let Some(t) = ticket {
        url.push_str(&format!("?{WS_TICKET_QUERY_PARAM}={t}"));
    }
    url
}

/// The browser flow: fetch a ticket for `path` with the key in a header.
async fn ticket(base: &str, path: &str) -> String {
    let resp = reqwest::Client::new()
        .post(format!("{base}{WS_TICKET_PATH}"))
        .header("X-API-Key", KEY)
        .json(&serde_json::json!({"path": path}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "ticket for {path}");
    let t: WsTicketResponse = resp.json().await.unwrap();
    assert_eq!(t.path, path);
    assert_eq!(t.query_param, WS_TICKET_QUERY_PARAM);
    assert!(!t.ticket.contains(KEY));
    t.ticket
}

async fn ws_connect(url: &str, origin: &'static str) -> Result<(), u16> {
    let mut req = url.into_client_request().unwrap();
    req.headers_mut()
        .insert("origin", HeaderValue::from_static(origin));
    match tokio_tungstenite::connect_async(req).await {
        Ok(_) => Ok(()),
        Err(tokio_tungstenite::tungstenite::Error::Http(resp)) => Err(resp.status().as_u16()),
        Err(e) => panic!("unexpected ws error: {e}"),
    }
}

#[test]
fn native_and_wire_constants_agree() {
    // at-core cannot depend on at-api-types; this pins them together.
    assert_eq!(at_core::lockfile::API_KEY_HEADER, API_KEY_HEADER);
    assert_eq!(at_core::config::DAEMON_API_KEY_ENV, API_KEY_ENV);
}

#[tokio::test]
async fn native_client_with_discovered_key_is_authorized() {
    let base = start(vec![], false).await;
    let conn = DaemonConnection::new(&base, Some(KEY.into()), DiscoverySource::Override);
    let client = native_client(&conn);
    for path in ["/api/status", "/api/bootstrap", "/api/beads", "/api/tasks"] {
        let resp = client
            .get(format!("{}{path}", conn.api_url))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200, "{path}");
    }
}

#[tokio::test]
async fn native_client_without_key_gets_401() {
    let base = start(vec![], false).await;
    let conn = DaemonConnection::new(&base, None, DiscoverySource::Default);
    let resp = native_client(&conn)
        .get(format!("{base}/api/status"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn browser_fetch_shape_is_authorized_and_cors_allows_key_header() {
    let base = start(vec![], false).await;
    let client = reqwest::Client::new();

    // Preflight for a cross-origin fetch carrying X-API-Key.
    let pre = client
        .request(reqwest::Method::OPTIONS, format!("{base}/api/beads"))
        .header("Origin", "http://localhost:5173")
        .header("Access-Control-Request-Method", "POST")
        .header("Access-Control-Request-Headers", "content-type,x-api-key")
        .send()
        .await
        .unwrap();
    assert!(pre.status().is_success(), "preflight {}", pre.status());
    let allowed = pre
        .headers()
        .get("access-control-allow-headers")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    assert!(allowed.contains("x-api-key"), "allow-headers: {allowed}");

    // The request the Leptos helpers send.
    let resp = client
        .get(format!("{base}/api/beads"))
        .header("Origin", "http://localhost:5173")
        .header("Accept", "application/json")
        .header("X-API-Key", KEY)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("access-control-allow-origin")
            .and_then(|v| v.to_str().ok()),
        Some("http://localhost:5173")
    );
}

#[tokio::test]
async fn cors_rejects_localhost_prefix_lookalike_origin() {
    let base = start(vec![], false).await;
    let resp = reqwest::Client::new()
        .get(format!("{base}/api/status"))
        .header("Origin", "http://localhost.evil.com")
        .header("X-API-Key", KEY)
        .send()
        .await
        .unwrap();
    assert!(resp.headers().get("access-control-allow-origin").is_none());
}

#[tokio::test]
async fn events_ws_requires_ticket_and_accepts_fresh_one() {
    let base = start(vec![], false).await;
    assert_eq!(
        ws_connect(
            &ws_url(&base, "/api/events/ws", None),
            "http://localhost:5173"
        )
        .await,
        Err(401)
    );
    assert_eq!(
        ws_connect(
            &ws_url(&base, "/api/events/ws", Some("wrong")),
            "http://localhost:5173"
        )
        .await,
        Err(401)
    );
    let t = ticket(&base, "/api/events/ws").await;
    assert_eq!(
        ws_connect(
            &ws_url(&base, "/api/events/ws", Some(&t)),
            "http://localhost:5173"
        )
        .await,
        Ok(())
    );
    // Single use: the same ticket cannot open a second socket.
    assert_eq!(
        ws_connect(
            &ws_url(&base, "/api/events/ws", Some(&t)),
            "http://localhost:5173"
        )
        .await,
        Err(401)
    );
    let t = ticket(&base, "/ws").await;
    assert_eq!(
        ws_connect(&ws_url(&base, "/ws", Some(&t)), "http://127.0.0.1:1").await,
        Ok(())
    );
}

#[tokio::test]
async fn ws_ticket_endpoint_requires_header_key_and_ws_path() {
    let base = start(vec![], false).await;
    let client = reqwest::Client::new();
    let no_key = client
        .post(format!("{base}{WS_TICKET_PATH}"))
        .json(&serde_json::json!({"path": "/ws"}))
        .send()
        .await
        .unwrap();
    assert_eq!(no_key.status(), 401);
    let key_in_query = client
        .post(format!("{base}{WS_TICKET_PATH}?api_key={KEY}"))
        .json(&serde_json::json!({"path": "/ws"}))
        .send()
        .await
        .unwrap();
    assert_eq!(key_in_query.status(), 401);
    for bad in ["/api/status", "/ws/terminal/a/b", "/ws/"] {
        let resp = client
            .post(format!("{base}{WS_TICKET_PATH}"))
            .header("X-API-Key", KEY)
            .json(&serde_json::json!({"path": bad}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 400, "{bad}");
    }
}

const NO_TERM: &str = "/ws/terminal/00000000-0000-4000-8000-000000000000";

#[tokio::test]
async fn ws_ticket_is_bound_to_its_route() {
    let base = start(vec![], false).await;
    let t = ticket(&base, NO_TERM).await;
    assert_eq!(
        ws_connect(
            &ws_url(&base, "/api/events/ws", Some(&t)),
            "http://localhost:5173"
        )
        .await,
        Err(401)
    );
    // Burned by the wrong-route attempt.
    assert_eq!(
        ws_connect(&ws_url(&base, NO_TERM, Some(&t)), "http://localhost:5173").await,
        Err(401)
    );
    // A fresh ticket for a (nonexistent, UUID-shaped) terminal passes auth
    // and reaches the terminal handler, which answers 404 -- no PTY needed.
    let t = ticket(&base, NO_TERM).await;
    assert_eq!(
        ws_connect(&ws_url(&base, NO_TERM, Some(&t)), "http://localhost:5173").await,
        Err(404)
    );
}

#[tokio::test]
async fn catalog_advertises_ticket_flow_not_query_key() {
    let base = start(vec![], false).await;
    let cat: serde_json::Value = reqwest::get(format!("{base}/api/catalog"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(cat["auth"]["ws_query_param"], WS_TICKET_QUERY_PARAM);
    assert_eq!(cat["auth"]["ws_ticket_path"], WS_TICKET_PATH);
    assert_eq!(cat["auth"]["contract_version"], "2.0.0");
    assert!(cat["cards"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["method"] == "POST" && c["path"] == WS_TICKET_PATH));
}

#[tokio::test]
async fn tauri_origin_needs_to_be_configured_and_then_works() {
    let base = start(vec![], false).await;
    let t = ticket(&base, "/api/events/ws").await;
    assert_eq!(
        ws_connect(
            &ws_url(&base, "/api/events/ws", Some(&t)),
            "tauri://localhost"
        )
        .await,
        Err(403)
    );

    let tauri: Vec<String> = TAURI_WEBVIEW_ORIGINS
        .iter()
        .map(|s| s.to_string())
        .collect();
    let base = start(tauri, false).await;
    let t = ticket(&base, "/api/events/ws").await;
    assert_eq!(
        ws_connect(
            &ws_url(&base, "/api/events/ws", Some(&t)),
            "tauri://localhost"
        )
        .await,
        Ok(())
    );
    let t = ticket(&base, "/ws").await;
    assert_eq!(
        ws_connect(&ws_url(&base, "/ws", Some(&t)), "http://tauri.localhost").await,
        Ok(())
    );
}

#[tokio::test]
async fn terminal_ws_honours_configured_origin_and_ticket() {
    let base = start(vec!["https://app.example.test".into()], true).await;
    let conn = DaemonConnection::new(&base, Some(KEY.into()), DiscoverySource::Override);
    let resp = native_client(&conn)
        .post(format!("{base}/api/terminals"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.unwrap();
    let id = body["id"].as_str().unwrap().to_string();
    let path = format!("/ws/terminal/{id}");

    assert_eq!(
        ws_connect(&ws_url(&base, &path, None), "https://app.example.test").await,
        Err(401)
    );
    let t = ticket(&base, &path).await;
    assert_eq!(
        ws_connect(&ws_url(&base, &path, Some(&t)), "https://evil.example.test").await,
        Err(403)
    );
    let t = ticket(&base, &path).await;
    assert_eq!(
        ws_connect(&ws_url(&base, &path, Some(&t)), "https://app.example.test").await,
        Ok(())
    );
}

/// Security invariant (PR #101 review 2026-10-08): `?api_key=<master key>`
/// must not open any of the three WebSocket routes, from an allowed Origin.
#[tokio::test]
async fn master_key_in_ws_query_is_rejected_on_every_ws_route() {
    // Auth runs before routing, so a made-up terminal id still exercises the
    // auth layer (an authorised upgrade would reach the handler: 404).
    let base = start(vec![], false).await;
    for path in ["/ws", "/api/events/ws", "/ws/terminal/no-such-terminal"] {
        let url = format!("{}{path}?api_key={KEY}", base.replace("http://", "ws://"));
        assert_eq!(
            ws_connect(&url, "http://localhost:5173").await,
            Err(401),
            "{path}"
        );
    }
}
