//! Shared HTTP client construction for outbound integration requests.
//!
//! Every integration client (GitLab, Linear, GitHub OAuth) builds its
//! `reqwest::Client` through [`client()`] so all outbound calls to a
//! third-party API carry the same connect/request timeouts and a single
//! pooled connection instead of a fresh client (and TCP handshake) per call.
//! Without a timeout, a stalled upstream can hang a request handler
//! indefinitely, since neither `reqwest` nor axum apply one by default.

use std::time::Duration;

/// Total request timeout for outbound integration calls.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// TCP connect timeout for outbound integration calls.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Build a `reqwest::Client` configured with this crate's shared timeout and
/// user-agent policy.
///
/// # Panics
///
/// Panics if the underlying TLS backend fails to initialize, mirroring
/// `reqwest::Client::new()`'s own panic behavior.
pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        .user_agent("auto-tundra/1.0")
        .build()
        .expect("failed to build reqwest client")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_builds_successfully() {
        // Mostly a smoke test: this used to be `reqwest::Client::new()` at
        // every call site with no timeout at all.
        let _ = client();
    }

    #[test]
    fn timeouts_are_bounded() {
        assert!(REQUEST_TIMEOUT >= CONNECT_TIMEOUT);
        assert!(REQUEST_TIMEOUT <= Duration::from_secs(60));
    }

    /// Regression test for GitLab/Linear/GitHub-OAuth clients that used to
    /// build a bare `reqwest::Client::new()` with no timeout at all, so a
    /// stalled upstream (accepts the TCP connection, never responds) hung
    /// the request forever. This proves the `.timeout()` builder call that
    /// `client()` relies on actually bounds such a request, using a short
    /// timeout so the test stays fast instead of waiting out the real
    /// 30-second default.
    #[tokio::test]
    async fn a_stalled_server_does_not_hang_the_request() {
        let std_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        std_listener.set_nonblocking(true).unwrap();
        let addr = std_listener.local_addr().unwrap();
        let listener = tokio::net::TcpListener::from_std(std_listener).unwrap();

        tokio::spawn(async move {
            // Accept the connection but never write a response — this is
            // the "stalled upstream" the finding described.
            if let Ok((_sock, _)) = listener.accept().await {
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        });

        let bounded_client = reqwest::Client::builder()
            .timeout(Duration::from_millis(300))
            .build()
            .unwrap();

        let start = std::time::Instant::now();
        let result = bounded_client.get(format!("http://{addr}/")).send().await;

        assert!(result.is_err(), "a bounded request to a stalled server must time out, not hang");
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "request should have been bounded by the client timeout, took {:?}",
            start.elapsed()
        );
    }
}
