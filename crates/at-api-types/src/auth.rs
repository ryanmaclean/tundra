//! Wire contract for authenticating to the auto-tundra daemon API.
//!
//! The daemon always runs with an API key (see
//! `at_core::config::CredentialProvider::ensure_daemon_api_key`). Every
//! first-party client presents it the same way:
//!
//! | Client                     | Transport          | How the key is sent                       |
//! |----------------------------|--------------------|-------------------------------------------|
//! | CLI / TUI / scripts        | HTTP               | `X-API-Key: <key>` header                 |
//! | Browser / webview (fetch)  | HTTP               | `X-API-Key: <key>` header                 |
//! | Browser / webview (WS)     | WebSocket upgrade  | `?ws_ticket=<ticket>` (see below)         |
//! | Standalone browser (fetch) | HTTP               | httpOnly session cookie (see below)       |
//!
//! Browsers cannot attach custom headers to a WebSocket upgrade. The reusable
//! key is **never** accepted in a URL (URLs end up in logs, proxies, devtools
//! and crash reports). Instead the page first calls
//! `POST` [`WS_TICKET_PATH`] with the `X-API-Key` header and a
//! [`WsTicketRequest`] naming the WebSocket path it is about to open, and
//! passes the returned [`WsTicketResponse::ticket`] as the
//! [`WS_TICKET_QUERY_PARAM`] query parameter. A ticket is random, bound to
//! that one path, single use (burned on the first presentation, matching or
//! not) and expires after [`WS_TICKET_TTL_SECS`] seconds. It is accepted
//! **only** on `GET` requests carrying `Upgrade: websocket`.
//!
//! Native clients discover the key from `AUTO_TUNDRA_API_KEY` or
//! `~/.auto-tundra/daemon.key` (next to the `daemon.lock` port file).
//!
//! # Standalone browser sessions
//!
//! The daemon's HTTP frontend never puts the key in the HTML it serves: any
//! local account that can reach loopback could read it. Instead the owner
//! proves ownership by reading the key file (mode 0600) and calls `POST`
//! [`BOOTSTRAP_CODE_PATH`] with `X-API-Key`; the reply carries a random,
//! single-use [`BootstrapCodeResponse::code`] valid for
//! [`BOOTSTRAP_CODE_TTL_SECS`] seconds. The owner opens the frontend at
//! `<frontend>/#bootstrap=<code>` (a URL fragment is never sent to a server
//! or logged). The page exchanges the code with `POST` [`SESSION_PATH`]
//! ([`SessionExchangeRequest`]); the daemon answers with a
//! `Set-Cookie: tundra_session=...; HttpOnly; SameSite=Strict; Path=/`
//! cookie that is bound to the exact `Origin` that redeemed the code, so a
//! page served by another local user's server on a different port cannot
//! ride it. Cookie requests must carry that `Origin`, and a cookie session
//! can never mint a bootstrap code. WebSocket tickets are minted from the
//! cookie session exactly as from the key.
//!
//! The embedded desktop webview is not served over HTTP: it receives the key
//! from its own initialization script ([`WINDOW_API_KEY_GLOBAL`], set by
//! [`browser_bootstrap_script`]).

/// Wire-format version of this auth contract.
///
/// 2.0.0: the `api_key` WebSocket query parameter was removed in favour of
/// single-use, path-bound tickets ([`WS_TICKET_PATH`]).
/// 2.1.0: additive. Standalone browsers authenticate with an httpOnly session
/// cookie obtained from a single-use bootstrap code ([`SESSION_PATH`],
/// [`BOOTSTRAP_CODE_PATH`]); the daemon no longer serves the key in HTML.
pub const AUTH_CONTRACT_VERSION: &str = "2.1.0";

/// Name of the httpOnly session cookie.
pub const SESSION_COOKIE_NAME: &str = "tundra_session";

/// `POST` endpoint (API-key authenticated; a cookie session is refused) that
/// mints a single-use bootstrap code.
pub const BOOTSTRAP_CODE_PATH: &str = "/api/v1/bootstrap-codes";

/// `POST` exchanges a bootstrap code for the session cookie (no key needed,
/// the code is the credential); `GET` reports whether the request's cookie is
/// a live session; `DELETE` ends it.
pub const SESSION_PATH: &str = "/api/v1/session";

/// Seconds a bootstrap code stays redeemable.
pub const BOOTSTRAP_CODE_TTL_SECS: u64 = 60;

/// Seconds a cookie session lasts (12 h).
pub const SESSION_TTL_SECS: u64 = 12 * 60 * 60;

/// Schema version of [`BootstrapCodeResponse`] and [`SessionResponse`].
pub const SESSION_SCHEMA_VERSION: &str = "1.0.0";

/// URL-fragment key that carries a bootstrap code: `#bootstrap=<code>`.
pub const BOOTSTRAP_FRAGMENT_KEY: &str = "bootstrap";

/// Response of `POST` [`BOOTSTRAP_CODE_PATH`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BootstrapCodeResponse {
    /// Always [`SESSION_SCHEMA_VERSION`].
    pub schema_version: String,
    /// Opaque single-use code (URL-safe, hex).
    pub code: String,
    /// Seconds until the code expires ([`BOOTSTRAP_CODE_TTL_SECS`]).
    pub expires_in_secs: u64,
}

/// Body of `POST` [`SESSION_PATH`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SessionExchangeRequest {
    /// The code from [`BootstrapCodeResponse::code`].
    pub code: String,
}

/// Response of `POST`/`GET`/`DELETE` [`SESSION_PATH`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SessionResponse {
    /// Always [`SESSION_SCHEMA_VERSION`].
    pub schema_version: String,
    /// Whether the request's cookie is a live session for its `Origin`.
    pub authenticated: bool,
    /// Seconds the session lasts (0 when not authenticated).
    pub expires_in_secs: u64,
}

/// The URL a browser opens to start a session: `frontend_url` plus the
/// `#bootstrap=<code>` fragment. `frontend_url` must not already have a
/// fragment.
pub fn bootstrap_url(frontend_url: &str, code: &str) -> String {
    format!(
        "{}/#{BOOTSTRAP_FRAGMENT_KEY}={code}",
        frontend_url.trim_end_matches('/')
    )
}

/// Extract the bootstrap code from `location.hash` (`#bootstrap=<code>`).
/// `None` when absent or empty; a code containing anything but ASCII
/// alphanumerics, `-` or `_` is rejected.
pub fn bootstrap_code_from_fragment(hash: &str) -> Option<&str> {
    let code = hash
        .strip_prefix('#')?
        .strip_prefix(BOOTSTRAP_FRAGMENT_KEY)?
        .strip_prefix('=')?;
    let ok = !code.is_empty()
        && code.len() <= 128
        && code
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    ok.then_some(code)
}

/// Header that carries the API key (HTTP header names are case-insensitive).
pub const API_KEY_HEADER: &str = "x-api-key";

/// Query parameter that carries a WebSocket ticket on upgrade requests.
pub const WS_TICKET_QUERY_PARAM: &str = "ws_ticket";

/// `POST` endpoint (API-key authenticated) that issues WebSocket tickets.
pub const WS_TICKET_PATH: &str = "/api/v1/ws-tickets";

/// Seconds a WebSocket ticket stays redeemable.
pub const WS_TICKET_TTL_SECS: u64 = 30;

/// Schema version of [`WsTicketResponse`].
pub const WS_TICKET_SCHEMA_VERSION: &str = "1.0.0";

/// Body of `POST` [`WS_TICKET_PATH`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WsTicketRequest {
    /// The WebSocket path the ticket will open, without query string:
    /// `/ws`, `/api/events/ws` or `/ws/terminal/{id}`.
    pub path: String,
}

/// Response of `POST` [`WS_TICKET_PATH`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WsTicketResponse {
    /// Always [`WS_TICKET_SCHEMA_VERSION`].
    pub schema_version: String,
    /// Opaque single-use ticket (URL-safe).
    pub ticket: String,
    /// The only path the ticket opens.
    pub path: String,
    /// Query parameter to send it in ([`WS_TICKET_QUERY_PARAM`]).
    pub query_param: String,
    /// Seconds until the ticket expires ([`WS_TICKET_TTL_SECS`]).
    pub expires_in_secs: u64,
}

/// `true` when `path` is one of the daemon's WebSocket routes, i.e. a path a
/// ticket may be issued for: `/ws`, `/api/events/ws`, `/ws/terminal/{id}`.
pub fn is_ws_ticket_path(path: &str) -> bool {
    match path {
        "/ws" | "/api/events/ws" => true,
        _ => path.strip_prefix("/ws/terminal/").is_some_and(|id| {
            !id.is_empty()
                && id.len() <= 128
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        }),
    }
}

/// Environment variable that overrides the on-disk daemon key.
pub const API_KEY_ENV: &str = "AUTO_TUNDRA_API_KEY";

/// `window` global holding the daemon API port (a number).
pub const WINDOW_API_PORT_GLOBAL: &str = "__TUNDRA_API_PORT__";

/// `window` global holding the daemon API key (a string).
pub const WINDOW_API_KEY_GLOBAL: &str = "__TUNDRA_API_KEY__";

/// Encode `s` as a JavaScript string literal that is also safe to embed inside
/// an HTML `<script>` element (no `</script>` breakout, no HTML comment tricks).
fn js_string_literal(s: &str) -> String {
    let json = serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string());
    json.replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

/// Build the JavaScript that hands the daemon connection to the web frontend.
///
/// Sets `window.__TUNDRA_API_PORT__` and, when a key is given,
/// `window.__TUNDRA_API_KEY__`. The output is safe to wrap in a `<script>`
/// element or to pass to a webview initialization script. Pass a key only to
/// a webview initialization script (the desktop shell); never put it in HTML
/// served over HTTP, where any local account could read it.
pub fn browser_bootstrap_script(api_port: u16, api_key: Option<&str>) -> String {
    let mut script = format!("window.{WINDOW_API_PORT_GLOBAL}={api_port};");
    if let Some(key) = api_key {
        script.push_str(&format!(
            "window.{WINDOW_API_KEY_GLOBAL}={};",
            js_string_literal(key)
        ));
    }
    script
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_script_sets_port_and_key() {
        let s = browser_bootstrap_script(4321, Some("abc-123"));
        assert_eq!(
            s,
            "window.__TUNDRA_API_PORT__=4321;window.__TUNDRA_API_KEY__=\"abc-123\";"
        );
    }

    #[test]
    fn bootstrap_script_without_key_only_sets_port() {
        assert_eq!(
            browser_bootstrap_script(9, None),
            "window.__TUNDRA_API_PORT__=9;"
        );
    }

    #[test]
    fn bootstrap_script_cannot_break_out_of_script_tag() {
        let s = browser_bootstrap_script(1, Some("\";alert(1)</script><script>x=\"&"));
        assert!(!s.contains("</script>"));
        assert!(!s.contains('<'));
        assert!(!s.contains('&'));
        // The quote is escaped, so the string literal is not terminated early.
        assert!(s.contains("\\\";alert(1)"));
    }

    #[test]
    fn ws_ticket_paths_are_exactly_the_ws_routes() {
        for ok in [
            "/ws",
            "/api/events/ws",
            "/ws/terminal/t-1",
            "/ws/terminal/550e8400-e29b-41d4",
        ] {
            assert!(is_ws_ticket_path(ok), "{ok}");
        }
        for bad in [
            "",
            "/",
            "/ws/",
            "/api/status",
            "/api/events/ws/",
            "/ws/terminal/",
            "/ws/terminal/a/b",
            "/ws/terminal/a?x=1",
            "/ws/terminal/../api",
            "/api/v1/ws-tickets",
        ] {
            assert!(!is_ws_ticket_path(bad), "{bad}");
        }
    }

    #[test]
    fn ws_ticket_contract_never_names_the_api_key() {
        assert_ne!(WS_TICKET_QUERY_PARAM, "api_key");
        assert_eq!(AUTH_CONTRACT_VERSION, "2.1.0");
    }

    #[test]
    fn bootstrap_url_carries_the_code_in_the_fragment_only() {
        let url = bootstrap_url("http://127.0.0.1:5173/", "abc123");
        assert_eq!(url, "http://127.0.0.1:5173/#bootstrap=abc123");
        // A fragment is never sent to the server.
        assert!(!url.contains('?'));
    }

    #[test]
    fn bootstrap_fragment_round_trips_and_rejects_junk() {
        assert_eq!(
            bootstrap_code_from_fragment("#bootstrap=abc-1_Z"),
            Some("abc-1_Z")
        );
        for bad in [
            "",
            "#",
            "#bootstrap",
            "#bootstrap=",
            "bootstrap=abc",
            "#other=abc",
            "#bootstrap=a b",
            "#bootstrap=a&x=1",
            "#bootstrap=<script>",
        ] {
            assert_eq!(bootstrap_code_from_fragment(bad), None, "{bad}");
        }
        let long = format!("#bootstrap={}", "a".repeat(129));
        assert_eq!(bootstrap_code_from_fragment(&long), None);
    }

    #[test]
    fn session_contract_paths_do_not_overlap_other_auth_paths() {
        assert_ne!(SESSION_PATH, WS_TICKET_PATH);
        assert!(!BOOTSTRAP_CODE_PATH.starts_with(SESSION_PATH));
        assert!(!SESSION_PATH.starts_with(BOOTSTRAP_CODE_PATH));
    }
}
