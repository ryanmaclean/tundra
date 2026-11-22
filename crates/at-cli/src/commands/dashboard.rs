//! `at dashboard` -- sign the browser in to the web UI without ever putting the
//! daemon API key in a page.
//!
//! Reading the `0600` key file (done by [`api_client`]) is the owner check.
//! The key mints a single-use, 60 s bootstrap code (`POST /api/v1/bootstrap-codes`);
//! the code goes in the URL *fragment* (`#bootstrap=<code>`), which a browser
//! never sends to a server or writes to an access log. The page exchanges it
//! for an httpOnly, SameSite=Strict cookie bound to the page's origin.

use at_api_types::auth::{bootstrap_url, BootstrapCodeResponse, BOOTSTRAP_CODE_PATH};

use super::{api_client, friendly_error};

/// Mint a bootstrap code with `client` (which carries the API key) and return
/// the URL that signs a browser in at `frontend_url`.
pub async fn mint_bootstrap_url(
    client: &reqwest::Client,
    api_url: &str,
    frontend_url: &str,
) -> anyhow::Result<String> {
    let resp = client
        .post(format!("{api_url}{BOOTSTRAP_CODE_PATH}"))
        .send()
        .await
        .map_err(friendly_error)?;
    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        anyhow::bail!(
            "the daemon rejected the API key (HTTP 401); is ~/.auto-tundra/daemon.key \
             (or AUTO_TUNDRA_API_KEY) the key of the running daemon?"
        );
    }
    if !status.is_success() {
        anyhow::bail!("could not mint a bootstrap code (HTTP {status})");
    }
    let body: BootstrapCodeResponse = resp.json().await.map_err(friendly_error)?;
    Ok(bootstrap_url(frontend_url, &body.code))
}

/// Frontend base URL from the live daemon lockfile. A wildcard bind address
/// is not browsable, so it maps to the IPv4 loopback.
fn lockfile_frontend_url() -> Option<String> {
    let lock = at_core::lockfile::DaemonLockfile::read_valid()?;
    let host = match lock.host.as_str() {
        "0.0.0.0" | "::" | "[::]" | "" => "127.0.0.1",
        h => h,
    };
    Some(format!("http://{host}:{}", lock.frontend_port))
}

/// Best-effort: ask the OS to open `url` in the default browser.
fn open_in_browser(url: &str) -> bool {
    let (program, args): (&str, Vec<&str>) = if cfg!(target_os = "macos") {
        ("open", vec![url])
    } else if cfg!(target_os = "windows") {
        ("cmd", vec!["/C", "start", "", url])
    } else {
        ("xdg-open", vec![url])
    };
    std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok()
}

/// Print (and, unless `no_open`, open) the sign-in URL.
pub async fn run(api_url: &str, frontend_url: Option<&str>, no_open: bool) -> anyhow::Result<()> {
    let frontend = match frontend_url {
        Some(url) => url.to_string(),
        None => lockfile_frontend_url().ok_or_else(|| {
            anyhow::anyhow!("no running daemon found; start at-daemon or pass --frontend-url")
        })?,
    };
    let url = mint_bootstrap_url(&api_client(), api_url, &frontend).await?;
    println!("{url}");
    eprintln!("single-use link, valid for 60 s; it signs in the first browser tab that opens it");
    if !no_open && !open_in_browser(&url) {
        eprintln!("could not open a browser; open the link above yourself");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::api_client_with_key;
    use axum::{http::HeaderMap, http::StatusCode, routing::post, Json, Router};

    async fn mock(expected_key: &'static str) -> String {
        let app = Router::new().route(
            BOOTSTRAP_CODE_PATH,
            post(move |headers: HeaderMap| async move {
                match headers.get("x-api-key").and_then(|v| v.to_str().ok()) {
                    Some(k) if k == expected_key => (
                        StatusCode::CREATED,
                        Json(serde_json::json!({
                            "schema_version": "1.0.0",
                            "code": "abc123",
                            "expires_in_secs": 60,
                        })),
                    ),
                    _ => (
                        StatusCode::UNAUTHORIZED,
                        Json(serde_json::json!({"error": "unauthorized"})),
                    ),
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
    async fn mints_a_code_with_the_key_and_puts_it_in_the_fragment() {
        let base = mock("k-1").await;
        let url = mint_bootstrap_url(
            &api_client_with_key(Some("k-1")),
            &base,
            "http://127.0.0.1:5173/",
        )
        .await
        .unwrap();
        assert_eq!(url, "http://127.0.0.1:5173/#bootstrap=abc123");
        // The key itself never appears in the URL.
        assert!(!url.contains("k-1"));
    }

    #[tokio::test]
    async fn wrong_key_is_a_clear_error_and_yields_no_url() {
        let base = mock("k-1").await;
        let err = mint_bootstrap_url(
            &api_client_with_key(Some("nope")),
            &base,
            "http://127.0.0.1:5173",
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(err.contains("401"), "{err}");
    }
}
