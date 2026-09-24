//! The daemon's real `ApiState` must carry a PTY pool so the terminal API
//! works in production (regression: `POST /api/terminals` always returned 503
//! because `Daemon` built its state with `ApiState::new`).

use std::sync::Arc;
use std::time::Duration;

use at_bridge::http_api::api_router;
use at_core::cache::CacheDb;
use at_core::config::Config;
use at_daemon::daemon::Daemon;

async fn serve_daemon_state(config: Config) -> (String, Daemon) {
    let cache = Arc::new(CacheDb::new_in_memory().await.unwrap());
    let daemon = Daemon::with_cache(config, cache);
    let router = api_router(daemon.api_state().clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (format!("http://{addr}"), daemon)
}

#[tokio::test]
async fn daemon_state_can_create_terminals() {
    let (base, daemon) = serve_daemon_state(Config::default()).await;
    assert!(daemon.api_state().pty_pool.is_some());
    assert_eq!(
        daemon.api_state().terminal_ws.liveness_timeout,
        Some(Duration::from_secs(120))
    );

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{base}/api/terminals"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "terminal creation must not be 503");
    let body: serde_json::Value = resp.json().await.unwrap();
    let id = body["id"].as_str().unwrap().to_string();

    let resp = client
        .delete(format!("{base}/api/terminals/{id}"))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
}

#[tokio::test]
async fn pty_pool_can_be_disabled_by_config() {
    let mut config = Config::default();
    config.terminal.pty_pool_enabled = false;
    config.terminal.ws_liveness_timeout_secs = 0;
    let (base, daemon) = serve_daemon_state(config).await;
    assert!(daemon.api_state().pty_pool.is_none());
    assert_eq!(daemon.api_state().terminal_ws.liveness_timeout, None);

    let resp = reqwest::Client::new()
        .post(format!("{base}/api/terminals"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 503);
}
