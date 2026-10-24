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
//! `~/.auto-tundra/daemon.key` (next to the `daemon.lock` port file). Browser
//! clients receive it from the page that served them through the
//! [`WINDOW_API_KEY_GLOBAL`] global, which [`browser_bootstrap_script`] sets.

/// Wire-format version of this auth contract.
///
/// 2.0.0: the `api_key` WebSocket query parameter was removed in favour of
/// single-use, path-bound tickets ([`WS_TICKET_PATH`]).
pub const AUTH_CONTRACT_VERSION: &str = "2.0.0";

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
/// element or to pass to a webview initialization script.
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
        assert_eq!(AUTH_CONTRACT_VERSION, "2.0.0");
    }
}
