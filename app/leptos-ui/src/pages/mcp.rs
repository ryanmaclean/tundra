use leptos::prelude::*;
use crate::state::use_app_state;
use crate::themed::{themed, Prompt};
use leptos::task::spawn_local;

use crate::api;

/// Built-in MCP server definitions
struct McpServerDef {
    icon: &'static str,
    name: &'static str,
    description: &'static str,
    active: bool,
}

const BUILTIN_SERVERS: &[McpServerDef] = &[
    McpServerDef { icon: "\u{1F4DA}", name: "Context7", description: "Smart context for Markle", active: true },
    McpServerDef { icon: "\u{1F9E0}", name: "Graphiti Memory", description: "Memory system (see Memory settings)", active: false },
    McpServerDef { icon: "\u{1F517}", name: "Linear", description: "Require Linear integration (see Client settings)", active: false },
    McpServerDef { icon: "\u{1F4AD}", name: "Sequential Thinking", description: "Enhanced reasoning via Claude Sonnet", active: true },
    McpServerDef { icon: "\u{1F4C1}", name: "Filesystem", description: "File system automations for Claude Sonnet", active: true },
    McpServerDef { icon: "\u{1F310}", name: "Puppeteer", description: "Web browser automation for testing", active: true },
    McpServerDef { icon: "\u{2699}\u{FE0F}", name: "Auto Claude Tools", description: "Core built-in tools (always enabled)", active: true },
];

/// Agent definition for display
struct AgentDef {
    name: &'static str,
    model: &'static str,
    thinking: &'static str,
    mcp_count: u8,
    description: &'static str,
}

const SPEC_AGENTS: &[AgentDef] = &[
    AgentDef { name: "Spec Gatherer", model: "Sonnet 4.5", thinking: "Medium", mcp_count: 4, description: "Gather requirements from issues and context" },
    AgentDef { name: "Spec Researcher", model: "Sonnet 4.5", thinking: "Medium", mcp_count: 3, description: "Research codebase for relevant patterns" },
    AgentDef { name: "Spec Writer", model: "Sonnet 4.5", thinking: "Medium", mcp_count: 4, description: "Create the spec and breakdown" },
    AgentDef { name: "Spec Critic", model: "Sonnet 4.5", thinking: "Medium", mcp_count: 3, description: "Spot critique using deep analysis" },
    AgentDef { name: "Spec Discovery", model: "Sonnet 4.5", thinking: "Medium", mcp_count: 4, description: "Discover related specs and blockers" },
    AgentDef { name: "Spec Context", model: "Sonnet 4.5", thinking: "Medium", mcp_count: 3, description: "Enrich context from existing codebase" },
    AgentDef { name: "Spec Validator", model: "Sonnet 4.5", thinking: "Medium", mcp_count: 3, description: "Validate spec completeness and quality" },
];

const BUILD_AGENTS: &[AgentDef] = &[
    AgentDef { name: "Planner", model: "Sonnet 4.5", thinking: "Medium", mcp_count: 3, description: "Plan task in Puppeteer based on project type" },
    AgentDef { name: "Coder", model: "Sonnet 4.5", thinking: "Medium", mcp_count: 5, description: "Write code based on spec" },
    AgentDef { name: "QA Flash", model: "Sonnet 4.5", thinking: "Medium", mcp_count: 3, description: "Quick check: Does build + Puppeteer based on project type" },
    AgentDef { name: "Coder2", model: "Sonnet 4.5", thinking: "Medium", mcp_count: 5, description: "Fix code if QA fails" },
];

const QA_AGENTS: &[AgentDef] = &[
    AgentDef { name: "QA Reviewer", model: "Sonnet 4.5", thinking: "Medium", mcp_count: 3, description: "Deep QA review: Static Build + Puppeteer based on project type" },
    AgentDef { name: "QA Flash", model: "Sonnet 4.5", thinking: "Medium", mcp_count: 3, description: "Quick smoke test for build and linting" },
];

const UTILITY_AGENTS: &[AgentDef] = &[
    AgentDef { name: "PR Reviewer", model: "Sonnet 4.5", thinking: "Medium", mcp_count: 3, description: "Reviews GitHub pull requests" },
    AgentDef { name: "Commit Message", model: "Haiku 4.1", thinking: "Medium", mcp_count: 2, description: "Generates commit messages" },
    AgentDef { name: "Merge Resolver", model: "Sonnet 4.5", thinking: "Medium", mcp_count: 4, description: "Resolves merge conflicts" },
    AgentDef { name: "Insights", model: "Sonnet 4.5", thinking: "Medium", mcp_count: 3, description: "Generates insights from data" },
    AgentDef { name: "Analysis", model: "Sonnet 4.5", thinking: "Medium", mcp_count: 3, description: "Contextual analysis with context sorting" },
    AgentDef { name: "Batch Analysis", model: "Sonnet 4.5", thinking: "Medium", mcp_count: 3, description: "Batch processing of issues or items" },
];

const IDEATION_AGENTS: &[AgentDef] = &[
    AgentDef { name: "Roadmap Analysis", model: "Sonnet 4.5", thinking: "Medium", mcp_count: 3, description: "Analyzes roadmap and suggests next steps" },
];

#[component]
pub fn McpPage() -> impl IntoView {
    let app_state = use_app_state();
    let display_mode = app_state.display_mode;
    let (servers, set_servers) = signal(Vec::<api::ApiMcpServer>::new());
    let (loading, set_loading) = signal(true);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);
    let (_endpoint_missing, set_endpoint_missing) = signal(false);

    // Add custom server form state
    let (show_add_form, set_show_add_form) = signal(false);
    let (new_name, set_new_name) = signal(String::new());
    let (new_command, set_new_command) = signal(String::new());
    let (new_args, set_new_args) = signal(String::new());
    let (adding, set_adding) = signal(false);
    let (add_success, set_add_success) = signal(Option::<String>::None);

    let do_refresh = move || {
        set_loading.set(true);
        set_error_msg.set(None);
        set_endpoint_missing.set(false);
        spawn_local(async move {
            match api::fetch_mcp_servers().await {
                Ok(data) => set_servers.set(data),
                Err(e) => {
                    if e.contains("404") || e.contains("Not Found") {
                        set_endpoint_missing.set(true);
                        set_servers.set(Vec::new());
                    } else {
                        set_error_msg.set(Some(format!("Failed to fetch MCP servers: {e}")));
                    }
                }
            }
            set_loading.set(false);
        });
    };

    do_refresh();

    // Track locally disabled built-in servers
    let (disabled_servers, set_disabled_servers) = signal(std::collections::HashSet::<String>::new());

    let active_count = move || {
        let disabled = disabled_servers.get();
        BUILTIN_SERVERS.iter().filter(|s| {
            let name = s.name.to_string();
            if disabled.contains(&name) { false } else { s.active }
        }).count()
    };

    view! {
        <div class="page-header" style="border-bottom: none; flex-wrap: wrap; gap: 8px;">
            <div>
                <h2 style="display: flex; align-items: center; gap: 8px;">
                    "MCP Server Overview"
                    <span class="mcp-header-subtitle">" for "<em>"auto-tundra"</em></span>
                </h2>
                <span class="mcp-header-subtitle">"Configure which MCP servers are available for agents in this project"</span>
            </div>
            <div style="display: flex; align-items: center; gap: 10px; margin-left: auto;">
                <span class="mcp-enabled-badge">{move || format!("{} servers enabled", active_count())}</span>
                <button class="refresh-btn dashboard-refresh-btn" on:click=move |_| do_refresh()>
                    "\u{21BB} Refresh"
                </button>
            </div>
        </div>

        {move || error_msg.get().map(|msg| view! {
            <div class="dashboard-error" style="margin: 0 16px 8px;">{msg}</div>
        })}

        {move || loading.get().then(|| view! {
            <div class="dashboard-loading" style="padding: 0 16px;">{move || themed(display_mode.get(), Prompt::Loading)}</div>
        })}

        // ── MCP Server Configuration ──
        <div class="mcp-section-title">"MCP Server Configuration"</div>
        <div class="mcp-server-list">
            {BUILTIN_SERVERS.iter().map(|s| {
                let name = s.name.to_string();
                let name_toggle = name.clone();
                let default_active = s.active;
                let is_active = move || {
                    let disabled = disabled_servers.get();
                    if disabled.contains(&name) {
                        false
                    } else {
                        default_active
                    }
                };
                view! {
                    <div class="mcp-server-item">
                        <span class="mcp-server-icon">{s.icon}</span>
                        <div class="mcp-server-info">
                            <div class="mcp-server-name">{s.name}</div>
                            <div class="mcp-server-desc">{s.description}</div>
                        </div>
                        <button
                            class="btn btn-sm"
                            style="min-width: 70px; margin-left: auto; margin-right: 8px;"
                            on:click=move |_| {
                                let n = name_toggle.clone();
                                set_disabled_servers.update(|set| {
                                    if set.contains(&n) {
                                        set.remove(&n);
                                    } else {
                                        set.insert(n);
                                    }
                                });
                            }
                        >
                            {let is_active_btn = is_active.clone(); move || if is_active_btn() { "Enabled" } else { "Disabled" }}
                        </button>
                        <div class=(move || if is_active() { "mcp-server-status mcp-status-active" } else { "mcp-server-status mcp-status-inactive" })></div>
                    </div>
                }
            }).collect::<Vec<_>>()}
        </div>

        // Any API-fetched servers
        {move || {
            let api_servers = servers.get();
            (!api_servers.is_empty()).then(|| {
                view! {
                    <div class="mcp-server-list" style="margin-top: 8px;">
                        {api_servers.into_iter().map(|s| {
                            let active = s.status == "connected" || s.status == "active";
                            let status_class = if active { "mcp-server-status mcp-status-active" } else { "mcp-server-status mcp-status-inactive" };
                            view! {
                                <div class="mcp-server-item">
                                    <span class="mcp-server-icon">"\u{1F50C}"</span>
                                    <div class="mcp-server-info">
                                        <div class="mcp-server-name">{s.name}</div>
                                        <div class="mcp-server-desc">{s.tools.join(", ")}</div>
                                    </div>
                                    <div class={status_class}></div>
                                </div>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                }
            })
        }}

        // Custom servers section
        <div class="mcp-section-title">
            "CUSTOM SERVERS"
            <button
                class="mcp-add-btn"
                on:click=move |_| set_show_add_form.set(!show_add_form.get())
            >{move || if show_add_form.get() { "Cancel" } else { "+ Add Custom Server" }}</button>
        </div>

        // Success toast
        {move || add_success.get().map(|msg| view! {
            <div class="dashboard-success" style="margin: 0 16px 8px; color: #3fb950; background: #0d1117; border: 1px solid #238636; border-radius: 6px; padding: 8px 12px;">
                {msg}
            </div>
        })}

        // Add custom server form
        {move || show_add_form.get().then(|| view! {
            <div class="mcp-add-form" style="margin: 0 16px 16px; padding: 16px; background: var(--card-bg, #161b22); border: 1px solid var(--border-color, #30363d); border-radius: 8px;">
                <h3 style="margin: 0 0 12px; font-size: 14px;">"Add Custom MCP Server"</h3>
                <div style="display: flex; flex-direction: column; gap: 10px;">
                    <input
                        type="text"
                        class="form-input"
                        placeholder="Server name (e.g. my-tool-server)"
                        prop:value=move || new_name.get()
                        on:input=move |ev| set_new_name.set(event_target_value(&ev))
                    />
                    <input
                        type="text"
                        class="form-input"
                        placeholder="Command (e.g. npx -y @my/mcp-server)"
                        prop:value=move || new_command.get()
                        on:input=move |ev| set_new_command.set(event_target_value(&ev))
                    />
                    <input
                        type="text"
                        class="form-input"
                        placeholder="Arguments (space-separated, optional)"
                        prop:value=move || new_args.get()
                        on:input=move |ev| set_new_args.set(event_target_value(&ev))
                    />
                    <button
                        class="action-btn action-start"
                        disabled=move || adding.get() || new_name.get().trim().is_empty() || new_command.get().trim().is_empty()
                        on:click=move |_| {
                            let name = new_name.get();
                            let command = new_command.get();
                            let args_str = new_args.get();
                            let args = if args_str.trim().is_empty() {
                                None
                            } else {
                                Some(args_str.split_whitespace().map(|s| s.to_string()).collect::<Vec<_>>())
                            };
                            set_adding.set(true);
                            set_error_msg.set(None);
                            set_add_success.set(None);
                            spawn_local(async move {
                                match api::add_mcp_server(&name, &command, args).await {
                                    Ok(server) => {
                                        set_servers.update(|s| s.push(server));
                                        set_add_success.set(Some(format!("Server '{}' added successfully.", name)));
                                        set_new_name.set(String::new());
                                        set_new_command.set(String::new());
                                        set_new_args.set(String::new());
                                        set_show_add_form.set(false);
                                    }
                                    Err(e) => {
                                        set_error_msg.set(Some(format!("Failed to add server: {e}")));
                                    }
                                }
                                set_adding.set(false);
                            });
                        }
                    >
                        {move || if adding.get() { "Adding..." } else { "Add Server" }}
                    </button>
                </div>
            </div>
        })}

        {move || {
            let api_custom = servers.get();
            if api_custom.is_empty() && !show_add_form.get() {
                Some(view! {
                    <div class="mcp-custom-empty">
                        "No custom servers configured. Add one to use with your agents."
                    </div>
                })
            } else {
                None
            }
        }}

        // ── Agent Grids ──
        {render_agent_section("Specs Creation", SPEC_AGENTS)}
        {render_agent_section("Build", BUILD_AGENTS)}
        {render_agent_section("QA", QA_AGENTS)}
        {render_agent_section("Utility", UTILITY_AGENTS)}
        {render_agent_section("Ideation", IDEATION_AGENTS)}

        // Bottom spacing
        <div style="height: 24px;"></div>
    }
}

fn render_agent_section(title: &'static str, agents: &'static [AgentDef]) -> impl IntoView {
    view! {
        <div class="mcp-agents-section-title">
            {title}
            <span class="count">{format!("{} agents", agents.len())}</span>
        </div>
        <div class="mcp-agent-grid">
            {agents.iter().map(|a| {
                view! {
                    <div class="mcp-agent-card">
                        <div class="mcp-agent-header">
                            <span class="mcp-agent-name">{a.name}</span>
                            <span class="mcp-agent-mcp-badge">{format!("{} MCP", a.mcp_count)}</span>
                        </div>
                        <div class="mcp-agent-meta">
                            <span class="mcp-agent-model">{a.model}</span>
                            <span class="mcp-agent-thinking">{a.thinking}</span>
                        </div>
                        <div class="mcp-agent-desc">{a.description}</div>
                    </div>
                }
            }).collect::<Vec<_>>()}
        </div>
    }
}
