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
//! | Browser / webview (WS)     | WebSocket upgrade  | `?api_key=<key>` query parameter          |
//!
//! Browsers cannot attach custom headers to a WebSocket upgrade, so the query
//! parameter is accepted **only** on `GET` requests carrying
//! `Upgrade: websocket`.
//!
//! Native clients discover the key from `AUTO_TUNDRA_API_KEY` or
//! `~/.auto-tundra/daemon.key` (next to the `daemon.lock` port file). Browser
//! clients receive it from the page that served them through the
//! [`WINDOW_API_KEY_GLOBAL`] global, which [`browser_bootstrap_script`] sets.

/// Wire-format version of this auth contract.
pub const AUTH_CONTRACT_VERSION: &str = "1.0.0";

/// Header that carries the API key (HTTP header names are case-insensitive).
pub const API_KEY_HEADER: &str = "x-api-key";

/// Query parameter that carries the API key on WebSocket upgrade requests.
pub const WS_API_KEY_QUERY_PARAM: &str = "api_key";

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
}
