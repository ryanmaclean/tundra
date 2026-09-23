//! Route-surface regression test for the at-bridge HTTP API.
//!
//! `tests/fixtures/routes.txt` is a snapshot of every `(METHOD, PATH)` pair
//! the flat router on `main` served before it was split into per-domain
//! sub-routers. This test proves the nested router still serves each of them
//! (the request reaches a handler instead of the router fallback, and the
//! method is not rejected with 405) and that every one sits behind the API key
//! auth layer. It also checks that `GET /api/catalog` lists exactly the routes
//! the router serves: every catalogued route is served, and on every
//! catalogued path each uncatalogued method is rejected with 405. The two
//! catalog routes and the JSON Schema routes are the only unauthenticated
//! ones: they answer without a key, are rate limited, and report
//! `auth: none`. Routes added after the snapshot are listed in
//! [`ADDED_SINCE_SNAPSHOT`] (the fixture itself stays frozen).

use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::sync::Arc;

use at_api_types::catalog::{ApiCatalog, RouteAuth, CATALOG_PATH, CATALOG_V1_PATH};
use at_bridge::event_bus::EventBus;
use at_bridge::http_api::{api_router_with_auth, ApiState};
use at_harness::rate_limiter::{MultiKeyRateLimiter, RateLimitConfig};
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Method, Request, StatusCode};
use axum::Router;
use tower::ServiceExt;

const API_KEY: &str = "route-surface-test-key";
/// Status the test fallback returns, so a router miss is distinguishable from
/// a handler that legitimately answers 404 for an unknown id.
const ROUTE_MISS: StatusCode = StatusCode::IM_A_TEAPOT;
/// Value substituted for every `{param}` segment. A UUID satisfies the
/// `Path<Uuid>` / `Path<String>` extractors; numeric params reject it with
/// 400, which still proves the route matched.
const PARAM_VALUE: &str = "00000000-0000-0000-0000-000000000001";

/// Routes added after the `routes.txt` snapshot was taken.
const ADDED_SINCE_SNAPSHOT: &[(&str, &str)] = &[
    ("GET", "/api/tasks/{id}/merge-gate"),
    ("POST", "/api/tasks/{id}/merge"),
    ("GET", "/api/v1/schemas"),
    ("GET", "/api/v1/schemas/{*id}"),
];

/// Paths served without the API key (cold discovery).
fn is_public(path: &str) -> bool {
    path == CATALOG_PATH || path == CATALOG_V1_PATH || path.starts_with("/api/v1/schemas")
}

fn fixture_routes() -> BTreeSet<(String, String)> {
    include_str!("fixtures/routes.txt")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let (m, p) = l.split_once(' ').expect("fixture line is `METHOD PATH`");
            (m.to_string(), p.to_string())
        })
        .collect()
}

fn app_with_key(api_key: Option<&str>) -> Router {
    let state = Arc::new(ApiState::new(EventBus::new()).with_relaxed_rate_limits());
    api_router_with_auth(state, api_key.map(str::to_string), vec![])
        .fallback(|| async { (ROUTE_MISS, "route-miss") })
}

fn app() -> Router {
    app_with_key(Some(API_KEY))
}

async fn fetch_catalog(app: &Router, path: &str, api_key: Option<&str>) -> ApiCatalog {
    let resp = app
        .clone()
        .oneshot(request("GET", path, api_key))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "GET {path}");
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).expect("catalog deserializes as ApiCatalog")
}

fn catalog_routes(catalog: &ApiCatalog) -> BTreeSet<(String, String)> {
    catalog
        .cards
        .iter()
        .map(|c| (c.method.clone(), c.path.clone()))
        .collect()
}

fn concrete(path: &str) -> String {
    path.split('/')
        .map(|seg| {
            if seg == "{*id}" {
                "at.merge_gate.report/v1"
            } else if seg.starts_with('{') && seg.ends_with('}') {
                PARAM_VALUE
            } else {
                seg
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Build a request that reaches the route but, for body-taking methods, fails
/// JSON extraction so no mutating handler logic runs.
fn request(method: &str, path: &str, api_key: Option<&str>) -> Request<Body> {
    let method = Method::from_bytes(method.as_bytes()).expect("valid method");
    let has_body = matches!(method, Method::POST | Method::PUT | Method::PATCH);
    let mut b = Request::builder().method(method).uri(concrete(path));
    if let Some(k) = api_key {
        b = b.header("x-api-key", k);
    }
    if has_body {
        b.header("content-type", "application/json")
            .body(Body::from("{"))
            .unwrap()
    } else {
        b.body(Body::empty()).unwrap()
    }
}

async fn assert_served(app: &Router, routes: &BTreeSet<(String, String)>) {
    let mut misses = Vec::new();
    for (method, path) in routes {
        let status = app
            .clone()
            .oneshot(request(method, path, Some(API_KEY)))
            .await
            .unwrap()
            .status();
        if status == ROUTE_MISS || status == StatusCode::METHOD_NOT_ALLOWED {
            misses.push(format!("{method} {path} -> {status}"));
        }
    }
    assert!(
        misses.is_empty(),
        "routes not served:\n{}",
        misses.join("\n")
    );
}

#[test]
fn fixture_is_well_formed() {
    let routes = fixture_routes();
    assert_eq!(
        routes.len(),
        134,
        "snapshot of main has 134 (method, path) pairs"
    );
    for (m, p) in &routes {
        assert!(
            ["GET", "POST", "PUT", "PATCH", "DELETE"].contains(&m.as_str()),
            "{m}"
        );
        assert!(p.starts_with('/'), "{p}");
    }
}

#[tokio::test]
async fn every_snapshot_route_is_still_served() {
    assert_served(&app(), &fixture_routes()).await;
}

#[tokio::test]
async fn every_snapshot_route_requires_the_api_key() {
    let app = app();
    let mut open = Vec::new();
    for (method, path) in fixture_routes() {
        let status = app
            .clone()
            .oneshot(request(&method, &path, None))
            .await
            .unwrap()
            .status();
        if status != StatusCode::UNAUTHORIZED {
            open.push(format!("{method} {path} -> {status}"));
        }
    }
    assert!(
        open.is_empty(),
        "routes reachable without a key:\n{}",
        open.join("\n")
    );
}

#[tokio::test]
async fn unknown_path_hits_the_fallback() {
    let status = app()
        .oneshot(request("GET", "/api/definitely-not-a-route", Some(API_KEY)))
        .await
        .unwrap()
        .status();
    assert_eq!(status, ROUTE_MISS);
}

// ---------------------------------------------------------------------------
// Catalog
// ---------------------------------------------------------------------------

#[tokio::test]
async fn catalog_lists_exactly_the_snapshot_plus_itself() {
    let catalog = fetch_catalog(&app(), CATALOG_PATH, Some(API_KEY)).await;
    let mut expected = fixture_routes();
    expected.insert(("GET".into(), CATALOG_PATH.into()));
    expected.insert(("GET".into(), CATALOG_V1_PATH.into()));
    for (m, p) in ADDED_SINCE_SNAPSHOT {
        expected.insert((m.to_string(), p.to_string()));
    }
    let listed = catalog_routes(&catalog);
    assert_eq!(listed.len(), catalog.cards.len(), "no duplicate cards");
    let missing: Vec<_> = expected.difference(&listed).collect();
    let extra: Vec<_> = listed.difference(&expected).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "catalog drift\nmissing: {missing:?}\nextra: {extra:?}"
    );
}

#[tokio::test]
async fn every_catalogued_route_is_served() {
    let app = app();
    let catalog = fetch_catalog(&app, CATALOG_PATH, Some(API_KEY)).await;
    assert_served(&app, &catalog_routes(&catalog)).await;
}

#[tokio::test]
async fn catalogued_paths_serve_no_uncatalogued_methods() {
    let app = app();
    let catalog = fetch_catalog(&app, CATALOG_PATH, Some(API_KEY)).await;
    let listed = catalog_routes(&catalog);
    let paths: BTreeSet<&String> = listed.iter().map(|(_, p)| p).collect();
    let mut extra = Vec::new();
    for path in paths {
        for method in ["GET", "POST", "PUT", "PATCH", "DELETE"] {
            if listed.contains(&(method.to_string(), path.clone())) {
                continue;
            }
            let status = app
                .clone()
                .oneshot(request(method, path, Some(API_KEY)))
                .await
                .unwrap()
                .status();
            if status != StatusCode::METHOD_NOT_ALLOWED {
                extra.push(format!("{method} {path} -> {status}"));
            }
        }
    }
    assert!(
        extra.is_empty(),
        "served but not catalogued:\n{}",
        extra.join("\n")
    );
}

#[tokio::test]
async fn catalog_matches_bop_catalog_v1_shape() {
    let app = app();
    let catalog = fetch_catalog(&app, CATALOG_PATH, Some(API_KEY)).await;
    assert_eq!(catalog.schema_version, "v1");
    chrono::DateTime::parse_from_rfc3339(&catalog.generated_at).expect("RFC 3339 generated_at");
    assert_eq!(catalog.source, "at-bridge");
    assert_eq!(catalog.service.name, "auto-tundra");
    assert!(catalog.auth.enforced);
    assert_eq!(catalog.auth.scheme, "api_key");

    let mut ids = BTreeSet::new();
    for card in &catalog.cards {
        assert!(ids.insert(card.id.clone()), "duplicate id {}", card.id);
        assert!(
            card.id.starts_with(|c: char| c.is_ascii_lowercase())
                && card
                    .id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "id not kebab-case: {}",
            card.id
        );
        assert_eq!(
            card.version.split('.').count(),
            3,
            "SemVer: {}",
            card.version
        );
        assert_eq!(card.title, format!("{} {}", card.method, card.path));
        assert!(
            !card.description.is_empty(),
            "{} has no description",
            card.title
        );
        assert!(!card.domain.is_empty());
        let expected_auth = if is_public(&card.path) {
            RouteAuth::None
        } else {
            RouteAuth::ApiKey
        };
        assert_eq!(card.auth, expected_auth, "{}", card.title);
    }
    let sorted = {
        let mut v = catalog.cards.clone();
        v.sort_by(|a, b| (&a.path, &a.method).cmp(&(&b.path, &b.method)));
        v
    };
    assert_eq!(catalog.cards, sorted, "cards sorted by path, method");

    // Raw JSON carries the fields bop's catalog.v1.json requires.
    let raw = app
        .clone()
        .oneshot(request("GET", CATALOG_PATH, Some(API_KEY)))
        .await
        .unwrap();
    let body = axum::body::to_bytes(raw.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    for key in ["schema_version", "generated_at", "cards"] {
        assert!(json.get(key).is_some(), "missing top-level {key}");
    }
    for card in json["cards"].as_array().unwrap() {
        assert!(card["id"].is_string() && card["version"].is_string());
    }
}

#[tokio::test]
async fn catalog_v1_alias_serves_the_same_catalog() {
    let app = app();
    let current = fetch_catalog(&app, CATALOG_PATH, Some(API_KEY)).await;
    let v1 = fetch_catalog(&app, CATALOG_V1_PATH, Some(API_KEY)).await;
    assert_eq!(current, v1);
}

#[tokio::test]
async fn catalog_is_served_without_the_api_key_and_reports_dev_mode() {
    let app = app();
    for path in [CATALOG_PATH, CATALOG_V1_PATH] {
        let catalog = fetch_catalog(&app, path, None).await;
        // Unauthenticated callers still learn that the rest needs a key.
        assert!(catalog.auth.enforced, "{path}");
        let own: Vec<_> = catalog
            .cards
            .iter()
            .filter(|c| c.path == CATALOG_PATH || c.path == CATALOG_V1_PATH)
            .collect();
        assert_eq!(own.len(), 2, "{path}");
        assert!(own.iter().all(|c| c.auth == RouteAuth::None), "{path}");
        assert!(
            catalog
                .cards
                .iter()
                .filter(|c| !is_public(&c.path))
                .all(|c| c.auth == RouteAuth::ApiKey),
            "{path}"
        );
    }
    // Raw JSON spells the unauthenticated value `none`.
    let raw = app
        .oneshot(request("GET", CATALOG_PATH, None))
        .await
        .unwrap();
    let body = axum::body::to_bytes(raw.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    for card in json["cards"].as_array().unwrap() {
        let path = card["path"].as_str().unwrap();
        let want = if is_public(path) {
            "none"
        } else {
            "api_key"
        };
        assert_eq!(card["auth"], want, "{path}");
    }

    let dev = fetch_catalog(&app_with_key(None), CATALOG_PATH, None).await;
    assert!(!dev.auth.enforced);
}

/// App with a per-endpoint limit of 2 requests/minute (global and per-client
/// tiers left generous) so the third catalog request is throttled.
fn tightly_limited_app() -> Router {
    let mut state = ApiState::new(EventBus::new());
    state.rate_limiter = Arc::new(MultiKeyRateLimiter::new(
        RateLimitConfig::per_second(10_000),
        RateLimitConfig::per_second(10_000),
        RateLimitConfig::per_minute(2),
    ));
    api_router_with_auth(Arc::new(state), Some(API_KEY.to_string()), vec![])
}

fn from_peer(mut req: Request<Body>, peer: &str) -> Request<Body> {
    let addr: SocketAddr = peer.parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(addr));
    req
}

#[tokio::test]
async fn unauthenticated_catalog_is_rate_limited() {
    let app = tightly_limited_app();
    let mut statuses = Vec::new();
    for _ in 0..3 {
        let req = from_peer(request("GET", CATALOG_PATH, None), "203.0.113.7:4000");
        statuses.push(app.clone().oneshot(req).await.unwrap().status());
    }
    assert_eq!(
        statuses,
        [
            StatusCode::OK,
            StatusCode::OK,
            StatusCode::TOO_MANY_REQUESTS
        ]
    );
}

#[tokio::test]
async fn unauthenticated_catalog_exempts_loopback_like_the_rest() {
    let app = tightly_limited_app();
    for _ in 0..5 {
        let req = from_peer(request("GET", CATALOG_PATH, None), "127.0.0.1:4000");
        assert_eq!(
            app.clone().oneshot(req).await.unwrap().status(),
            StatusCode::OK
        );
    }
}

#[tokio::test]
async fn unmatched_path_still_requires_the_api_key() {
    // No custom fallback: `merge` must keep the auth-layered default one.
    let state = Arc::new(ApiState::new(EventBus::new()).with_relaxed_rate_limits());
    let app = api_router_with_auth(state, Some(API_KEY.to_string()), vec![]);
    let status = app
        .oneshot(request("GET", "/api/definitely-not-a-route", None))
        .await
        .unwrap()
        .status();
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// JSON Schemas referenced by catalog cards
// ---------------------------------------------------------------------------

async fn get_raw(app: &Router, uri: &str, api_key: Option<&str>) -> (StatusCode, Vec<u8>) {
    let mut b = Request::builder().method("GET").uri(uri);
    if let Some(k) = api_key {
        b = b.header("x-api-key", k);
    }
    let resp = app.clone().oneshot(b.body(Body::empty()).unwrap()).await.unwrap();
    let status = resp.status();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec();
    (status, body)
}

#[tokio::test]
async fn every_catalogued_schema_resolves_without_a_key() {
    let app = app();
    let catalog = fetch_catalog(&app, CATALOG_PATH, None).await;
    let ids: BTreeSet<String> = catalog
        .cards
        .iter()
        .flat_map(|c| c.schemas.iter().cloned())
        .collect();
    assert!(
        ids.contains(at_api_types::merge_gate::MERGE_GATE_SCHEMA_ID),
        "merge routes reference the gate report schema: {ids:?}"
    );
    for id in &ids {
        let (status, body) =
            get_raw(&app, &at_api_types::schemas::path_for(id), None).await;
        assert_eq!(status, StatusCode::OK, "{id}");
        let doc: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(doc["$id"], id.as_str(), "{id}");
    }

    let (status, body) = get_raw(&app, "/api/v1/schemas", None).await;
    assert_eq!(status, StatusCode::OK);
    let listing: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let listed: BTreeSet<String> = listing["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_string())
        .collect();
    assert!(ids.is_subset(&listed), "{ids:?} vs {listed:?}");

    let (status, _) = get_raw(&app, "/api/v1/schemas/nope/v9", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn merge_routes_carry_types_and_schema_ids() {
    let catalog = fetch_catalog(&app(), CATALOG_PATH, Some(API_KEY)).await;
    let card = |id: &str| {
        catalog
            .cards
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("no card {id}"))
            .clone()
    };
    let gate = card("get-api-tasks-id-merge-gate");
    assert_eq!(gate.schemas, vec!["at.merge_gate.report/v1".to_string()]);
    let merge = card("post-api-tasks-id-merge");
    assert_eq!(merge.response.as_deref(), Some("ApiMergeResponse"));
    assert_eq!(merge.schemas, vec!["at.merge_gate.report/v1".to_string()]);
    let wt = card("post-api-worktrees-id-merge");
    assert_eq!(wt.response.as_deref(), Some("ApiMergeResponse"));
    let exec = card("post-api-tasks-id-execute");
    assert_eq!(exec.response.as_deref(), Some("ExecuteTaskResponse"));
    assert!(card("post-api-tasks").description.contains("acceptance_criteria"));
    assert!(card("put-api-tasks-id").description.contains("acceptance_criteria"));
    let schema_route = card("get-api-v1-schemas-id");
    assert_eq!(schema_route.auth, RouteAuth::None);
}
