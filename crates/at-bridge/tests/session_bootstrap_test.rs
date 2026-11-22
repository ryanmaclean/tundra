//! Standalone-browser sign-in: the daemon serves no credential to a browser.
//! The owner mints a single-use bootstrap code with the API key, the page
//! exchanges it for an httpOnly, SameSite=Strict session cookie bound to its
//! `Origin`, and every later request must present that cookie and that exact
//! `Origin`. Real router, real sockets, request shapes a browser sends.

use std::sync::Arc;

use at_api_types::auth::{
    BootstrapCodeResponse, SessionResponse, WsTicketResponse, BOOTSTRAP_CODE_PATH,
    BOOTSTRAP_CODE_TTL_SECS, SESSION_COOKIE_NAME, SESSION_PATH, WS_TICKET_PATH,
};
use at_api_types::catalog::{ApiCatalog, RouteAuth};
use at_bridge::event_bus::EventBus;
use at_bridge::http_api::{api_router_with_auth, ApiState};

const KEY: &str = "session-test-key-0123456789abcdef";
/// The page origin (the daemon's frontend on its own loopback port).
const PAGE: &str = "http://127.0.0.1:5173";
/// Another local server's origin on a different loopback port.
const OTHER_PORT: &str = "http://127.0.0.1:6666";

async fn start() -> String {
    let state = Arc::new(ApiState::new(EventBus::new()).with_relaxed_rate_limits());
    let router = api_router_with_auth(state, Some(KEY.to_string()), vec![]);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    format!("http://{addr}")
}

/// Mint a bootstrap code the way `at dashboard` does: with the key.
async fn mint(base: &str) -> String {
    let resp = reqwest::Client::new()
        .post(format!("{base}{BOOTSTRAP_CODE_PATH}"))
        .header("X-API-Key", KEY)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let body: BootstrapCodeResponse = resp.json().await.unwrap();
    assert_eq!(body.expires_in_secs, BOOTSTRAP_CODE_TTL_SECS);
    assert!(!body.code.contains(KEY));
    body.code
}

/// Exchange as the page would: cross-origin POST with an `Origin`.
async fn exchange(base: &str, code: &str, origin: Option<&str>) -> reqwest::Response {
    let mut req = reqwest::Client::new()
        .post(format!("{base}{SESSION_PATH}"))
        .json(&serde_json::json!({ "code": code }));
    if let Some(o) = origin {
        req = req.header("Origin", o);
    }
    req.send().await.unwrap()
}

/// `tundra_session=<token>` from a `Set-Cookie` header.
fn cookie_pair(resp: &reqwest::Response) -> String {
    let set = resp
        .headers()
        .get("set-cookie")
        .expect("Set-Cookie")
        .to_str()
        .unwrap();
    set.split(';').next().unwrap().to_string()
}

/// Sign in and return the `Cookie` header value.
async fn sign_in(base: &str) -> String {
    let code = mint(base).await;
    let resp = exchange(base, &code, Some(PAGE)).await;
    assert_eq!(resp.status(), 200);
    cookie_pair(&resp)
}

async fn get_as(base: &str, path: &str, cookie: Option<&str>, origin: Option<&str>) -> u16 {
    let mut req = reqwest::Client::new().get(format!("{base}{path}"));
    if let Some(c) = cookie {
        req = req.header("Cookie", c);
    }
    if let Some(o) = origin {
        req = req.header("Origin", o);
    }
    req.send().await.unwrap().status().as_u16()
}

/// `GET /api/v1/session` as a browser at `origin` holding `cookie`.
async fn session_authenticated(base: &str, cookie: Option<&str>, origin: &str) -> bool {
    let mut req = reqwest::Client::new()
        .get(format!("{base}{SESSION_PATH}"))
        .header("Origin", origin);
    if let Some(c) = cookie {
        req = req.header("Cookie", c);
    }
    let resp = req.send().await.unwrap();
    assert_eq!(resp.status(), 200);
    resp.json::<SessionResponse>().await.unwrap().authenticated
}

#[tokio::test]
async fn bootstrap_code_needs_the_key() {
    let base = start().await;
    let c = reqwest::Client::new();
    let none = c
        .post(format!("{base}{BOOTSTRAP_CODE_PATH}"))
        .send()
        .await
        .unwrap();
    assert_eq!(none.status(), 401);
    let wrong = c
        .post(format!("{base}{BOOTSTRAP_CODE_PATH}"))
        .header("X-API-Key", "nope")
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), 401);
    // The key must never be accepted from the URL either.
    let query = c
        .post(format!("{base}{BOOTSTRAP_CODE_PATH}?api_key={KEY}"))
        .send()
        .await
        .unwrap();
    assert_eq!(query.status(), 401);
}

#[tokio::test]
async fn exchange_sets_an_httponly_samesite_strict_cookie_and_leaks_nothing() {
    let base = start().await;
    let code = mint(&base).await;
    let resp = exchange(&base, &code, Some(PAGE)).await;
    assert_eq!(resp.status(), 200);

    let set_cookie = resp
        .headers()
        .get("set-cookie")
        .expect("Set-Cookie")
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        set_cookie.starts_with(&format!("{SESSION_COOKIE_NAME}=")),
        "{set_cookie}"
    );
    assert!(set_cookie.contains("HttpOnly"), "{set_cookie}");
    assert!(set_cookie.contains("SameSite=Strict"), "{set_cookie}");
    assert!(set_cookie.contains("Path=/"), "{set_cookie}");
    assert!(set_cookie.contains("Max-Age="), "{set_cookie}");
    // Plain-http loopback: no Secure flag (browsers would drop it on 127.0.0.1).
    assert!(!set_cookie.contains("Secure"), "{set_cookie}");
    assert!(!set_cookie.contains(KEY));

    let text = resp.text().await.unwrap();
    assert!(!text.contains(KEY) && !text.contains(&code), "{text}");
    let body: SessionResponse = serde_json::from_str(&text).unwrap();
    assert!(body.authenticated);
}

#[tokio::test]
async fn code_is_single_use() {
    let base = start().await;
    let code = mint(&base).await;
    assert_eq!(exchange(&base, &code, Some(PAGE)).await.status(), 200);
    assert_eq!(exchange(&base, &code, Some(PAGE)).await.status(), 401);
}

#[tokio::test]
async fn exchange_rejects_missing_or_foreign_origin_without_burning_the_code() {
    let base = start().await;
    let code = mint(&base).await;
    for origin in [
        None,
        Some("http://evil.example"),
        Some("https://localhost.evil.example"),
    ] {
        assert_eq!(
            exchange(&base, &code, origin).await.status(),
            403,
            "{origin:?}"
        );
    }
    // The refused attempts did not consume the code.
    assert_eq!(exchange(&base, &code, Some(PAGE)).await.status(), 200);
}

#[tokio::test]
async fn forged_or_empty_code_is_refused() {
    let base = start().await;
    for code in ["", "deadbeef", KEY] {
        assert_eq!(
            exchange(&base, code, Some(PAGE)).await.status(),
            401,
            "{code}"
        );
    }
}

#[tokio::test]
async fn cookie_authenticates_api_calls_only_with_its_origin() {
    let base = start().await;
    let cookie = sign_in(&base).await;

    assert_eq!(
        get_as(&base, "/api/status", Some(&cookie), Some(PAGE)).await,
        200
    );
    // No Origin header (e.g. a cross-site no-cors GET): refused.
    assert_eq!(get_as(&base, "/api/status", Some(&cookie), None).await, 401);
    // Another loopback server (other port / other user) riding the cookie.
    assert_eq!(
        get_as(&base, "/api/status", Some(&cookie), Some(OTHER_PORT)).await,
        401
    );
    assert_eq!(
        get_as(
            &base,
            "/api/status",
            Some(&cookie),
            Some("http://evil.example")
        )
        .await,
        401
    );
    // Wrong or forged cookie.
    let forged = format!("{SESSION_COOKIE_NAME}=forged");
    assert_eq!(
        get_as(&base, "/api/status", Some(&forged), Some(PAGE)).await,
        401
    );
    // No credentials at all.
    assert_eq!(get_as(&base, "/api/status", None, Some(PAGE)).await, 401);
}

#[tokio::test]
async fn cookie_session_cannot_mint_bootstrap_codes() {
    let base = start().await;
    let cookie = sign_in(&base).await;
    let resp = reqwest::Client::new()
        .post(format!("{base}{BOOTSTRAP_CODE_PATH}"))
        .header("Cookie", &cookie)
        .header("Origin", PAGE)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn websocket_tickets_are_minted_from_the_cookie_session() {
    let base = start().await;
    let cookie = sign_in(&base).await;
    let ticket_req = |origin: &'static str| {
        reqwest::Client::new()
            .post(format!("{base}{WS_TICKET_PATH}"))
            .header("Cookie", &cookie)
            .header("Origin", origin)
            .json(&serde_json::json!({"path": "/api/events/ws"}))
            .send()
    };
    let resp = ticket_req(PAGE).await.unwrap();
    assert_eq!(resp.status(), 201);
    let t: WsTicketResponse = resp.json().await.unwrap();
    assert!(!t.ticket.contains(KEY));

    // The same request from another origin is refused.
    assert_eq!(ticket_req(OTHER_PORT).await.unwrap().status(), 401);
}

#[tokio::test]
async fn session_status_and_logout() {
    let base = start().await;
    let cookie = sign_in(&base).await;
    let c = reqwest::Client::new();

    assert!(session_authenticated(&base, Some(&cookie), PAGE).await);
    assert!(
        !session_authenticated(&base, Some(&cookie), OTHER_PORT).await,
        "wrong origin"
    );
    assert!(!session_authenticated(&base, None, PAGE).await);

    // A foreign origin cannot log the owner out.
    c.delete(format!("{base}{SESSION_PATH}"))
        .header("Cookie", &cookie)
        .header("Origin", OTHER_PORT)
        .send()
        .await
        .unwrap();
    assert!(session_authenticated(&base, Some(&cookie), PAGE).await);

    let out = c
        .delete(format!("{base}{SESSION_PATH}"))
        .header("Cookie", &cookie)
        .header("Origin", PAGE)
        .send()
        .await
        .unwrap();
    assert_eq!(out.status(), 200);
    assert!(out
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("Max-Age=0"));
    assert!(
        !session_authenticated(&base, Some(&cookie), PAGE).await,
        "session ended"
    );
    assert_eq!(
        get_as(&base, "/api/status", Some(&cookie), Some(PAGE)).await,
        401
    );
}

#[tokio::test]
async fn credentialed_cors_is_mirrored_for_allowed_origins_only() {
    let base = start().await;
    let c = reqwest::Client::new();

    let pre = c
        .request(reqwest::Method::OPTIONS, format!("{base}{SESSION_PATH}"))
        .header("Origin", PAGE)
        .header("Access-Control-Request-Method", "POST")
        .header("Access-Control-Request-Headers", "content-type")
        .send()
        .await
        .unwrap();
    let h = pre.headers();
    assert_eq!(h["access-control-allow-origin"], PAGE);
    assert_eq!(h["access-control-allow-credentials"], "true");

    let evil = c
        .request(reqwest::Method::OPTIONS, format!("{base}{SESSION_PATH}"))
        .header("Origin", "http://evil.example")
        .header("Access-Control-Request-Method", "POST")
        .send()
        .await
        .unwrap();
    assert!(evil.headers().get("access-control-allow-origin").is_none());
}

#[tokio::test]
async fn catalog_lists_the_session_routes_with_the_right_auth() {
    let base = start().await;
    let catalog: ApiCatalog = reqwest::Client::new()
        .get(format!("{base}/api/catalog"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let auth_of = |method: &str, path: &str| {
        catalog
            .cards
            .iter()
            .find(|c| c.method == method && c.path == path)
            .unwrap_or_else(|| panic!("{method} {path} not catalogued"))
            .auth
    };
    assert_eq!(auth_of("POST", SESSION_PATH), RouteAuth::None);
    assert_eq!(auth_of("GET", SESSION_PATH), RouteAuth::None);
    assert_eq!(auth_of("DELETE", SESSION_PATH), RouteAuth::None);
    assert_eq!(auth_of("POST", BOOTSTRAP_CODE_PATH), RouteAuth::ApiKey);
    assert_eq!(
        catalog.auth.contract_version,
        at_api_types::auth::AUTH_CONTRACT_VERSION
    );
}
