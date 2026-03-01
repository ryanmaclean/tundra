use crate::components::spinner::Spinner;
use crate::state::use_app_state;
use crate::themed::{themed, Prompt};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;

/// Built-in MCP server definitions
struct McpServerDef {
    name: &'static str,
    description: &'static str,
    active: bool,
}

const BUILTIN_SERVERS: &[McpServerDef] = &[
    McpServerDef {
        name: "Context",
        description: "Project context and codebase indexing",
        active: true,
    },
    McpServerDef {
        name: "Graphiti Memory",
        description: "Graph memory system (requires OpenAI API key)",
        active: false,
    },
    McpServerDef {
        name: "Linear",
        description: "Linear integration for issue tracking (see Client settings)",
        active: false,
    },
    McpServerDef {
        name: "Playwright",
        description: "Browser automation for testing and QA",
        active: true,
    },
    McpServerDef {
        name: "Auto Claude Tools",
        description: "Core built-in tools (always enabled)",
        active: true,
    },
];

fn mcp_server_icon_svg(name: &str) -> &'static str {
    match name {
        "Context" => {
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M4 4h16v5H4z"/><path d="M4 9l3 11h10l3-11"/></svg>"#
        }
        "Graphiti Memory" => {
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 2a7 7 0 0 0-7 7c0 2 1 3.7 2.4 5A3 3 0 0 1 8.5 16H15a3 3 0 0 1 1.1-2c1.5-1.3 2.4-3 2.4-5a7 7 0 0 0-7-7z"/><path d="M9 20h6"/><path d="M10 17h4"/></svg>"#
        }
        "Linear" => {
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M5 4h4"/><path d="M5 12h8"/><path d="M5 20h14"/></svg>"#
        }
        "Playwright" => {
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="9"/><path d="M3 12h18"/><path d="M12 3a15 15 0 0 1 0 18"/><path d="M12 3a15 15 0 0 0 0 18"/></svg>"#
        }
        "Auto Claude Tools" => {
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14.7 6.3a1 1 0 0 0-1.4-1.4L7 11.2V14h2.8z"/><path d="M3 21h18"/></svg>"#
        }
        _ => {
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="9"/></svg>"#
        }
    }
}

fn mcp_agent_icon_svg(name: &str) -> &'static str {
    if name.contains("Spec") {
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><path d="M14 2v6h6"/></svg>"#
    } else if name.contains("QA") {
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33h.01A1.65 1.65 0 0 0 10 3.09V3a2 2 0 1 1 4 0v.09c0 .67.39 1.27 1 1.51h.01a1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06c-.47.47-.61 1.17-.33 1.82.25.61.84 1 1.51 1H21a2 2 0 1 1 0 4h-.09c-.67 0-1.27.39-1.51 1z"/></svg>"#
    } else if name.contains("Planner") || name.contains("Coder") {
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M6 3v12"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><path d="M18 9a9 9 0 0 1-9 9"/></svg>"#
    } else {
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="m14.5 4.5 5 5"/><path d="m16 3 5 5"/><path d="M19 7 7 19l-4 1 1-4Z"/></svg>"#
    }
}

fn mcp_section_icon_svg(title: &str) -> &'static str {
    match title {
        "Specs Creation" => {
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><path d="M14 2v6h6"/></svg>"#
        }
        "Build" => {
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>"#
        }
        "QA" => {
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33h.01A1.65 1.65 0 0 0 10 3.09V3a2 2 0 1 1 4 0v.09c0 .67.39 1.27 1 1.51h.01a1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06c-.47.47-.61 1.17-.33 1.82.25.61.84 1 1.51 1H21a2 2 0 1 1 0 4h-.09c-.67 0-1.27.39-1.51 1z"/></svg>"#
        }
        "Utility" => {
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="m14.7 6.3-2.4 2.4"/><path d="m5 19 3.6-1 9.2-9.2a2.1 2.1 0 0 0-3-3L5.6 15z"/><path d="M3 21h18"/></svg>"#
        }
        "Ideation" => {
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M9 18h6"/><path d="M10 22h4"/><path d="M12 2a7 7 0 0 0-4 12.8V16a2 2 0 0 0 2 2h4a2 2 0 0 0 2-2v-1.2A7 7 0 0 0 12 2z"/></svg>"#
        }
        _ => {
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="9"/></svg>"#
        }
    }
}

fn mcp_section_caret_svg() -> &'static str {
    r#"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><polyline points="6 9 12 15 18 9"/></svg>"#
}

/// Agent definition for display
struct AgentDef {
    name: &'static str,
    model: &'static str,
    thinking: &'static str,
    mcp_count: u8,
    description: &'static str,
}

const SPEC_AGENTS: &[AgentDef] = &[
    AgentDef {
        name: "Spec Gatherer",
        model: "Sonnet 4.5",
        thinking: "Medium",
        mcp_count: 0,
        description: "Collects initial requirements from user",
    },
    AgentDef {
        name: "Spec Researcher",
        model: "Sonnet 4.5",
        thinking: "Medium",
        mcp_count: 0,
        description: "Validates external integrations and APIs",
    },
    AgentDef {
        name: "Spec Writer",
        model: "Sonnet 4.5",
        thinking: "Medium",
        mcp_count: 0,
        description: "Creates the spec.md document",
    },
    AgentDef {
        name: "Spec Critic",
        model: "Sonnet 4.5",
        thinking: "Medium",
        mcp_count: 0,
        description: "Self-critique using deep analysis",
    },
    AgentDef {
        name: "Spec Discovery",
        model: "Sonnet 4.5",
        thinking: "Medium",
        mcp_count: 0,
        description: "Initial project discovery and analysis",
    },
    AgentDef {
        name: "Spec Validation",
        model: "Sonnet 4.5",
        thinking: "Medium",
        mcp_count: 0,
        description: "Validates spec completeness and quality",
    },
];

const BUILD_AGENTS: &[AgentDef] = &[
    AgentDef {
        name: "Planner",
        model: "Sonnet 4.5",
        thinking: "Medium",
        mcp_count: 3,
        description: "Creates implementation plan with subtasks",
    },
    AgentDef {
        name: "Coder",
        model: "Sonnet 4.5",
        thinking: "Medium",
        mcp_count: 3,
        description: "Implements individual subtasks",
    },
];

const QA_AGENTS: &[AgentDef] = &[
    AgentDef {
        name: "QA Reviewer",
        model: "Sonnet 4.5",
        thinking: "Medium",
        mcp_count: 3,
        description:
            "Validates acceptance criteria. Uses Electron or Puppeteer based on project type.",
    },
    AgentDef {
        name: "QA Fixer",
        model: "Sonnet 4.5",
        thinking: "Medium",
        mcp_count: 3,
        description: "Fixes QA-reported issues. Uses Electron or Puppeteer based on project type.",
    },
];

const UTILITY_AGENTS: &[AgentDef] = &[
    AgentDef {
        name: "PR Reviewer",
        model: "Opus 4.5",
        thinking: "Medium",
        mcp_count: 0,
        description: "Reviews GitHub pull requests",
    },
    AgentDef {
        name: "Commit Message",
        model: "Haiku 4.5",
        thinking: "Low",
        mcp_count: 0,
        description: "Generates commit messages",
    },
    AgentDef {
        name: "Merge Resolver",
        model: "Haiku 4.5",
        thinking: "Low",
        mcp_count: 0,
        description: "Resolves merge conflicts",
    },
    AgentDef {
        name: "Insights",
        model: "Sonnet 4.5",
        thinking: "Medium",
        mcp_count: 0,
        description: "Extracts code insights and analysis",
    },
];

const IDEATION_AGENTS: &[AgentDef] = &[
    AgentDef {
        name: "Ideation",
        model: "Opus 4.5",
        thinking: "High",
        mcp_count: 0,
        description: "Generates feature ideas",
    },
    AgentDef {
        name: "Roadmap Discovery",
        model: "Opus 4.5",
        thinking: "High",
        mcp_count: 0,
        description: "Discovers roadmap items",
    },
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
    let (disabled_servers, set_disabled_servers) =
        signal(std::collections::HashSet::<String>::new());

    let active_count = move || {
        let disabled = disabled_servers.get();
        BUILTIN_SERVERS
            .iter()
            .filter(|s| {
                let name = s.name.to_string();
                if disabled.contains(&name) {
                    false
                } else {
                    s.active
                }
            })
            .count()
    };

    view! {
        <div class="mcp-page">
        <div class="page-header mcp-page-header">
            <div>
                <h2 class="mcp-page-title">
                    <span
                        class="mcp-page-title-icon"
                        inner_html=r#"<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="6" rx="2"/><rect x="3" y="14" width="18" height="6" rx="2"/><line x1="7" y1="7" x2="7" y2="7"/><line x1="7" y1="17" x2="7" y2="17"/></svg>"#
                    ></span>
                    "MCP Server Overview"
                    <span class="mcp-header-subtitle">" for "<em>"auto-tundra"</em></span>
                </h2>
                <span class="mcp-header-subtitle">"Configure which MCP servers are available for agents in this project"</span>
            </div>
            <div class="mcp-page-actions">
                <span class="mcp-enabled-badge">{move || format!("{} servers enabled", active_count())}</span>
                <button class="refresh-btn dashboard-refresh-btn" on:click=move |_| do_refresh()>
                    "\u{21BB} Refresh"
                </button>
            </div>
        </div>

        {move || error_msg.get().map(|msg| view! {
            <div class="state-banner state-banner-error mcp-error">
                <span
                    class="state-banner-icon"
                    inner_html=r#"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>"#
                ></span>
                <span>{msg}</span>
            </div>
        })}

        {move || loading.get().then(|| view! {
            <div class="dashboard-loading mcp-loading"><Spinner size="md" label="" /></div>
        })}

        // ── MCP Server Configuration ──
        <div class="mcp-section-title">"MCP Server Configuration"</div>
        <div class="mcp-server-list">
            {BUILTIN_SERVERS.iter().map(|s| {
                let name = s.name.to_string();
                let name_toggle = name.clone();
                let default_active = s.active;
                let name_check1 = name.clone();
                let name_check2 = name.clone();
                let name_check3 = name.clone();
                let name_check4 = name.clone();
                let is_active_btn = move || {
                    let disabled = disabled_servers.get();
                    if disabled.contains(&name_check1) { false } else { default_active }
                };
                let is_active_label = move || {
                    let disabled = disabled_servers.get();
                    if disabled.contains(&name_check2) { false } else { default_active }
                };
                let is_active_text = move || {
                    let disabled = disabled_servers.get();
                    if disabled.contains(&name_check3) { false } else { default_active }
                };
                let is_active_dot = move || {
                    let disabled = disabled_servers.get();
                    if disabled.contains(&name_check4) { false } else { default_active }
                };
                view! {
                    <div class="mcp-server-item">
                        <span class="mcp-server-icon mcp-server-icon-svg" inner_html=mcp_server_icon_svg(s.name)></span>
                        <div class="mcp-server-info">
                            <div class="mcp-server-name">{s.name}</div>
                            <div class="mcp-server-desc">{s.description}</div>
                        </div>
                        <button
                            class=(move || {
                                if is_active_btn() { "toggle-switch mcp-toggle-switch active" } else { "toggle-switch mcp-toggle-switch" }
                            })
                            type="button"
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
                        ><span class="toggle-knob"></span></button>
                        <span class=(move || if is_active_label() { "mcp-status-label active" } else { "mcp-status-label" })>
                            {move || if is_active_text() { "Enabled" } else { "Disabled" }}
                        </span>
                        <div class=(move || if is_active_dot() { "mcp-server-status mcp-status-active" } else { "mcp-server-status mcp-status-inactive" })></div>
                    </div>
                }
            }).collect::<Vec<_>>()}
        </div>

        // Any API-fetched servers
        {move || {
            let api_servers = servers.get();
            (!api_servers.is_empty()).then(|| {
                view! {
                    <div class="mcp-server-list mcp-server-list-extra">
                        {api_servers.into_iter().map(|s| {
                            let active = s.status == "connected" || s.status == "active";
                            let status_class = if active { "mcp-server-status mcp-status-active" } else { "mcp-server-status mcp-status-inactive" };
                            view! {
                                <div class="mcp-server-item">
                                    <span class="mcp-server-icon mcp-server-icon-svg" inner_html=mcp_server_icon_svg("api")></span>
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
            <div class="dashboard-success mcp-success">
                {msg}
            </div>
        })}

        // Add custom server form
        {move || show_add_form.get().then(|| view! {
            <div class="mcp-add-form">
                <h3 class="mcp-add-form-title">"Add Custom MCP Server"</h3>
                <div class="mcp-add-form-fields">
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
                        <span
                            class="mcp-custom-empty-icon"
                            inner_html=r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 5v14"/><path d="M5 12h14"/></svg>"#
                        ></span>
                        <span>"No custom servers configured. Add one to use with your agents."</span>
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
        <div class="mcp-bottom-space"></div>
        </div>
    }
}

fn render_agent_section(title: &'static str, agents: &'static [AgentDef]) -> impl IntoView {
    view! {
        <section class="mcp-agent-section">
            <div class="mcp-agents-section-title">
                <span class="mcp-agent-section-label">
                    <span class="mcp-section-caret-svg" inner_html=mcp_section_caret_svg()></span>
                    <span class="mcp-agent-section-icon-svg" inner_html=mcp_section_icon_svg(title)></span>
                    <span>{title}</span>
                </span>
                <span class="count">{format!("({} agents)", agents.len())}</span>
            </div>
            <div class="mcp-agent-grid">
                {agents.iter().map(|a| {
                    let tooling = if a.mcp_count == 0 {
                        "No MCP".to_string()
                    } else {
                        format!("{} MCP", a.mcp_count)
                    };
                    view! {
                        <div class="mcp-agent-card">
                            <div class="mcp-agent-header">
                                <div class="mcp-agent-title-wrap">
                                    <span class="mcp-agent-icon mcp-server-icon-svg" inner_html=mcp_agent_icon_svg(a.name)></span>
                                    <span class="mcp-agent-name">{a.name}</span>
                                </div>
                                <div class="mcp-agent-header-right">
                                    <span class=(if a.mcp_count == 0 { "mcp-agent-mcp-badge muted" } else { "mcp-agent-mcp-badge" })>{tooling}</span>
                                    <span class="mcp-agent-chevron">"\u{203A}"</span>
                                </div>
                            </div>
                            <div class="mcp-agent-meta mcp-agent-meta-dense">
                                <span class="mcp-agent-model">{a.model}</span>
                                <span class="mcp-agent-thinking">{a.thinking}</span>
                            </div>
                            <div class="mcp-agent-desc">{a.description}</div>
                        </div>
                    }
                }).collect::<Vec<_>>()}
            </div>
        </section>
    }
}
