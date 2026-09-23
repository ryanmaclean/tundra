pub mod agent;
pub mod doctor;
pub mod done;
pub mod exec_task;
pub mod hook;
pub mod nudge;
pub mod run_task;
pub mod skill;
pub mod sling;
pub mod smoke;
pub mod status;

use std::sync::OnceLock;

/// API key discovered once by `main` (via `DaemonConnection::discover`).
static API_KEY: OnceLock<Option<String>> = OnceLock::new();

/// Record the daemon API key for every client built by [`api_client`].
/// Called once from `main` with the discovered connection's key.
pub fn set_api_key(key: Option<String>) {
    let _ = API_KEY.set(key);
}

/// Build a reqwest client that authenticates to the daemon.
///
/// The daemon always requires `X-API-Key`; the key comes from the same
/// discovery path as the TUI (`AUTO_TUNDRA_API_KEY`, else
/// `~/.auto-tundra/daemon.key`). The CLI never generates a key.
pub fn api_client() -> reqwest::Client {
    let key = API_KEY
        .get_or_init(at_core::config::CredentialProvider::read_daemon_api_key)
        .as_deref();
    api_client_with_key(key)
}

/// Build a client that sends `key` as `X-API-Key` on every request.
pub fn api_client_with_key(key: Option<&str>) -> reqwest::Client {
    let mut headers = reqwest::header::HeaderMap::new();
    if let Some(key) = key.filter(|k| !k.is_empty()) {
        match reqwest::header::HeaderValue::from_str(key) {
            Ok(mut value) => {
                value.set_sensitive(true);
                headers.insert(at_core::lockfile::API_KEY_HEADER, value);
            }
            Err(_) => eprintln!("warning: daemon API key contains invalid header characters"),
        }
    }
    reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// Map common reqwest errors to user-friendly messages.
pub fn friendly_error(err: reqwest::Error) -> anyhow::Error {
    if err.is_connect() {
        anyhow::anyhow!(
            "Could not connect to the auto-tundra daemon. Is it running?\n  \
             (hint: start it with `at-daemon` or check --api-url)"
        )
    } else if err.is_timeout() {
        anyhow::anyhow!("Request timed out. The daemon may be overloaded.")
    } else {
        anyhow::anyhow!("API request failed: {err}")
    }
}
pub mod ideation;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{http::HeaderMap, http::StatusCode, routing::get, Router};

    /// Mock that behaves like the daemon's AuthLayer for one key.
    async fn start_auth_mock(expected: &'static str) -> String {
        let app = Router::new().route(
            "/api/status",
            get(move |headers: HeaderMap| async move {
                match headers.get("x-api-key").and_then(|v| v.to_str().ok()) {
                    Some(k) if k == expected => StatusCode::OK,
                    _ => StatusCode::UNAUTHORIZED,
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn client_sends_api_key_header() {
        let base = start_auth_mock("k-123").await;
        let ok = api_client_with_key(Some("k-123"))
            .get(format!("{base}/api/status"))
            .send()
            .await
            .unwrap();
        assert_eq!(ok.status(), 200);

        let missing = api_client_with_key(None)
            .get(format!("{base}/api/status"))
            .send()
            .await
            .unwrap();
        assert_eq!(missing.status(), 401);
    }
}
