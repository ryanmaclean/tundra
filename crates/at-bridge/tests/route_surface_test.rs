//! Route-surface regression test for the at-bridge HTTP API.
//!
//! `tests/fixtures/routes.txt` is a snapshot of every `(METHOD, PATH)` pair
//! the flat router on `main` served before it was split into per-domain
//! sub-routers. This test proves the nested router still serves each of them
//! (the request reaches a handler instead of the router fallback, and the
//! method is not rejected with 405) and that every one sits behind the API key
//! auth layer.

use std::collections::BTreeSet;
use std::sync::Arc;

use at_bridge::event_bus::EventBus;
use at_bridge::http_api::{api_router_with_auth, ApiState};
use axum::body::Body;
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

fn app() -> Router {
    let state = Arc::new(ApiState::new(EventBus::new()).with_relaxed_rate_limits());
    api_router_with_auth(state, Some(API_KEY.to_string()), vec![])
        .fallback(|| async { (ROUTE_MISS, "route-miss") })
}

fn concrete(path: &str) -> String {
    path.split('/')
        .map(|seg| {
            if seg.starts_with('{') && seg.ends_with('}') {
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
    assert!(misses.is_empty(), "routes not served:\n{}", misses.join("\n"));
}

#[test]
fn fixture_is_well_formed() {
    let routes = fixture_routes();
    assert_eq!(routes.len(), 134, "snapshot of main has 134 (method, path) pairs");
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
    assert!(open.is_empty(), "routes reachable without a key:\n{}", open.join("\n"));
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
