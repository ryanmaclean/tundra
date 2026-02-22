use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::i18n::t;

// ---------------------------------------------------------------------------
// Layout enum (mirrors terminals page)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GridLayout {
    Single,
    Double,
    Quad,
}

impl GridLayout {
    fn css_class(self) -> &'static str {
        match self {
            GridLayout::Single => "layout-1",
            GridLayout::Double => "layout-2",
            GridLayout::Quad => "layout-4",
        }
    }

    fn max_panes(self) -> usize {
        match self {
            GridLayout::Single => 1,
            GridLayout::Double => 2,
            GridLayout::Quad => 4,
        }
    }
}

// ---------------------------------------------------------------------------
// Mock terminal output based on agent role
// ---------------------------------------------------------------------------

fn mock_terminal_output(role: &str, name: &str) -> String {
    match role {
        "coder" | "Coder" | "coding" => format!(
            "$ claude --role coder --task implement\n\
             [{name}] Analyzing codebase structure...\n\
             [{name}] Reading src/lib.rs (245 lines)\n\
             [{name}] Applying patch to src/handlers.rs\n\
             [{name}]   + pub fn handle_request(req: Request) -> Response {{\n\
             [{name}]   +     let body = req.body();\n\
             [{name}]   +     Response::ok(body)\n\
             [{name}]   + }}\n\
             [{name}] Running cargo check... OK\n\
             [{name}] 1 file changed, 4 insertions(+)"
        ),
        "qa" | "QA" | "reviewer" | "Reviewer" => format!(
            "$ claude --role qa --task review\n\
             [{name}] Starting code review pass...\n\
             [{name}] Checking src/handlers.rs\n\
             [{name}]   WARN: missing error handling on line 42\n\
             [{name}]   INFO: function complexity OK (cyclomatic: 3)\n\
             [{name}]   PASS: no unsafe blocks found\n\
             [{name}] Checking tests/integration.rs\n\
             [{name}]   INFO: 12/12 test cases covered\n\
             [{name}] Review complete: 1 warning, 0 errors"
        ),
        "architect" | "Architect" | "planning" => format!(
            "$ claude --role architect --task plan\n\
             [{name}] Evaluating architecture constraints...\n\
             [{name}] Module graph: 8 crates, 23 dependencies\n\
             [{name}] Identifying coupling hotspots...\n\
             [{name}]   at-core <-> at-bridge: 14 shared types\n\
             [{name}]   at-agents -> at-intelligence: 6 calls\n\
             [{name}] Suggested refactor: extract shared types to at-types\n\
             [{name}] Generating design document..."
        ),
        "ops" | "Ops" | "devops" | "DevOps" => format!(
            "$ claude --role ops --task deploy\n\
             [{name}] Checking deployment prerequisites...\n\
             [{name}] Building release artifacts...\n\
             [{name}]   cargo build --release (2m 14s)\n\
             [{name}] Running health checks...\n\
             [{name}]   /health -> 200 OK (12ms)\n\
             [{name}]   /ready  -> 200 OK (8ms)\n\
             [{name}] Deployment staged. Awaiting approval."
        ),
        _ => format!(
            "$ claude --role {role}\n\
             [{name}] Agent initialized\n\
             [{name}] Loading context...\n\
             [{name}] Processing task queue...\n\
             [{name}] Waiting for instructions..."
        ),
    }
}

fn status_dot_class(status: &str) -> &'static str {
    match status {
        "active" | "running" => "agent-status-dot dot-active",
        "idle" => "agent-status-dot dot-idle",
        "pending" | "starting" => "agent-status-dot dot-pending",
        "stopped" | "dead" => "agent-status-dot dot-stopped",
        _ => "agent-status-dot dot-unknown",
    }
}

// ---------------------------------------------------------------------------
// Agents Page
// ---------------------------------------------------------------------------

#[component]
pub fn AgentsPage() -> impl IntoView {
    let (agents, set_agents) = signal(Vec::<api::ApiAgent>::new());
    let (layout, set_layout) = signal(GridLayout::Quad);
    let (loading, set_loading) = signal(true);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);
    let set_current_tab = use_context::<WriteSignal<usize>>();

    let do_refresh = move || {
        set_loading.set(true);
        set_error_msg.set(None);
        spawn_local(async move {
            match api::fetch_agents().await {
                Ok(data) => set_agents.set(data),
                Err(e) => set_error_msg.set(Some(format!("Failed to fetch agents: {e}"))),
            }
            set_loading.set(false);
        });
    };

    // Initial fetch
    do_refresh();

    let stop_agent = move |id: String| {
        spawn_local(async move {
            match api::stop_agent(&id).await {
                Ok(_) => {
                    match api::fetch_agents().await {
                        Ok(data) => set_agents.set(data),
                        Err(_) => {}
                    }
                }
                Err(e) => {
                    web_sys::console::error_1(&format!("Failed to stop agent: {e}").into());
                }
            }
        });
    };

    let agent_count = move || agents.get().len();
    let terminal_count_display = move || {
        let count = agent_count();
        if layout.get() == GridLayout::Quad {
            count.max(3)
        } else {
            count
        }
    };

    view! {
        <div class="page-header">
            <h2>{t("agents-title")}</h2>
            <div class="page-header-actions">
                <span class="terminal-count">
                    {move || {
                        let max = layout.get().max_panes();
                        format!("{}/{} agents", agent_count(), max)
                    }}
                </span>
                <button class="refresh-btn dashboard-refresh-btn" on:click=move |_| do_refresh()>
                    "\u{21BB} Refresh"
                </button>
            </div>
        </div>

        <div class="terminal-command-bar terminal-command-bar-agent">
            <div class="terminal-cmd-left">
                <span class="terminal-count-pill">
                    {move || format!("{}/{} terminals", terminal_count_display(), 12)}
                </span>
            </div>
            <div class="terminal-cmd-right">
                <button class="terminal-cmd-btn" type="button">
                    "History \u{25BE}"
                </button>
                <button class="terminal-cmd-btn" type="button" on:click=move |_| do_refresh()>
                    "\u{2699} Invoke Claude All"
                </button>
                <button
                    class="terminal-cmd-btn terminal-cmd-btn-magenta"
                    type="button"
                    on:click=move |_| {
                        if let Some(set_tab) = set_current_tab {
                            set_tab.set(14);
                        }
                    }
                >
                    "+ New Terminal"
                </button>
                <button class="terminal-cmd-btn" type="button">
                    "\u{2398} Files"
                </button>
            </div>
        </div>

        // Layout selector toolbar
        <div class="terminal-toolbar">
            <button
                class=(move || if layout.get() == GridLayout::Single { "layout-btn active" } else { "layout-btn" })
                on:click=move |_| set_layout.set(GridLayout::Single)
            >
                "Single"
            </button>
            <button
                class=(move || if layout.get() == GridLayout::Double { "layout-btn active" } else { "layout-btn" })
                on:click=move |_| set_layout.set(GridLayout::Double)
            >
                "Double"
            </button>
            <button
                class=(move || if layout.get() == GridLayout::Quad { "layout-btn active" } else { "layout-btn" })
                on:click=move |_| set_layout.set(GridLayout::Quad)
            >
                "Quad"
            </button>
        </div>

        {move || error_msg.get().map(|msg| view! {
            <div class="terminal-error">{msg}</div>
        })}

        {move || loading.get().then(|| view! {
            <div class="dashboard-loading">"Loading agents..."</div>
        })}

        <div class=move || format!("terminal-grid terminal-grid-agent {}", layout.get().css_class())>
            {move || {
                let max = layout.get().max_panes();
                let list = agents.get();
                let visible: Vec<_> = list.iter().take(max).cloned().collect();
                let min_slots = if layout.get() == GridLayout::Quad { 3 } else { 1 };
                let slot_count = visible.len().max(min_slots).min(max);
                let mut panes = Vec::with_capacity(slot_count);

                for idx in 0..slot_count {
                    if let Some(agent) = visible.get(idx) {
                        let name = agent.name.clone();
                        let role = agent.role.clone();
                        let status = agent.status.clone();
                        let id = agent.id.clone();
                        let id_stop = agent.id.clone();
                        let dot_cls = status_dot_class(&status);
                        let output = mock_terminal_output(&role, &name);
                        let is_active = status == "active" || status == "running" || status == "idle";
                        let stop_agent = stop_agent.clone();
                        let terminal_name = format!("Terminal {}", idx + 1);
                        let role_badge = role.to_uppercase();
                        panes.push(
                            view! {
                                <div class="terminal-emulator">
                                    <div class="terminal-pane-header">
                                        <div class="agent-pane-info">
                                            <span class={dot_cls}></span>
                                            <span class="terminal-title">{terminal_name}</span>
                                            <span class="terminal-worktree-badge">"Worktree"</span>
                                        </div>
                                        <div class="agent-pane-actions">
                                            <span class="agent-role-badge">{role_badge}</span>
                                            <span class="terminal-dimensions">{id}</span>
                                            <span class="terminal-model-badge">"\u{269B} Claude"</span>
                                            <button class="terminal-pane-icon-btn" type="button" title="Maximize">
                                                "\u{2197}"
                                            </button>
                                            {is_active.then(|| {
                                                let stop = stop_agent.clone();
                                                view! {
                                                    <button
                                                        class="terminal-pane-icon-btn"
                                                        on:click=move |_| stop(id_stop.clone())
                                                        title="Stop agent"
                                                    >
                                                        "\u{2715}"
                                                    </button>
                                                }
                                            })}
                                        </div>
                                    </div>
                                    <div class="agent-terminal-content">
                                        <pre class="agent-terminal-pre">{output}</pre>
                                    </div>
                                </div>
                            }.into_any()
                        );
                    } else {
                        let terminal_name = format!("Terminal {}", idx + 1);
                        panes.push(
                            view! {
                                <div class="terminal-emulator terminal-emulator-placeholder">
                                    <div class="terminal-pane-header">
                                        <div class="agent-pane-info">
                                            <span class="agent-status-dot dot-unknown"></span>
                                            <span class="terminal-title">{terminal_name}</span>
                                            <span class="terminal-worktree-badge">"Worktree"</span>
                                        </div>
                                        <div class="agent-pane-actions">
                                            <span class="terminal-model-badge">"\u{269B} Claude"</span>
                                        </div>
                                    </div>
                                    <div class="agent-terminal-content agent-terminal-empty-pane">
                                        <pre class="agent-terminal-pre">"$ waiting for terminal assignment..."</pre>
                                    </div>
                                </div>
                            }.into_any()
                        );
                    }
                }
                panes
            }}
        </div>

        {move || (!loading.get() && agents.get().is_empty() && error_msg.get().is_none()).then(|| view! {
            <div class="terminal-empty">
                <div class="terminal-empty-icon">"\u{1F916}"</div>
                <div class="terminal-empty-text">"No agents running"</div>
                <div class="terminal-empty-hint">"Agents will appear here when tasks are executing"</div>
            </div>
        })}
    }
}
