//! The repo's Claude Code MCP client config must be able to authenticate.
//!
//! `/mcp/sse` sits behind the API-key layer and the daemon always enforces a
//! key, so the project config needs the `x-api-key` header. Claude Code reads
//! project MCP servers from `.mcp.json` (not `.claude/settings.json`), and
//! the key must come from the environment, never be committed.

use std::path::PathBuf;

fn repo_file(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel)
}

fn read_json(rel: &str) -> serde_json::Value {
    let path = repo_file(rel);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

#[test]
fn mcp_json_registers_at_tundra_with_env_api_key() {
    let cfg = read_json(".mcp.json");
    let server = &cfg["mcpServers"]["at-tundra"];
    assert_eq!(server["type"], "sse");
    assert!(
        server["url"].as_str().unwrap().ends_with("/mcp/sse"),
        "{server}"
    );
    assert_eq!(
        server["headers"]["x-api-key"], "${AUTO_TUNDRA_API_KEY}",
        "key must be env-expanded, never a literal"
    );
}

#[test]
fn claude_settings_no_longer_declares_mcp_servers() {
    let settings = read_json(".claude/settings.json");
    assert!(
        settings.get("mcpServers").is_none(),
        "Claude Code ignores mcpServers in settings.json; use .mcp.json"
    );
}
