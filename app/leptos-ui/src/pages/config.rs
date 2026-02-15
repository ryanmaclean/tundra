use leptos::prelude::*;

#[component]
pub fn ConfigPage() -> impl IntoView {
    let config_text = r#"[daemon]
host = "127.0.0.1"
port = 9400
log_level = "info"

[agents]
max_concurrent = 8
default_model = "claude-sonnet-4"
timeout_secs = 300

[session]
persist = true
storage_path = "~/.auto-tundra/sessions"

[telemetry]
enabled = true
cost_tracking = true
export_format = "json"

[mcp]
auto_discover = true
timeout_ms = 5000

[[mcp.servers]]
name = "filesystem"
command = "mcp-fs"
transport = "stdio"

[[mcp.servers]]
name = "git"
command = "mcp-git"
transport = "stdio"

[[mcp.servers]]
name = "web-search"
endpoint = "http://localhost:3100"
transport = "http""#;

    view! {
        <div class="page-header">
            <h2>"Configuration"</h2>
        </div>
        <div class="config-block">
            {config_text}
        </div>
    }
}
