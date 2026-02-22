use leptos::prelude::*;
use leptos::task::spawn_local;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;

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

fn spinner_frame(tick: u64) -> &'static str {
    match tick % 4 {
        0 => "|",
        1 => "/",
        2 => "-",
        _ => "\\",
    }
}

fn mock_terminal_output(role: &str, name: &str, tick: u64) -> String {
    let spinner = spinner_frame(tick);
    let elapsed = format!("{:02}:{:02}", (tick / 60) % 60, tick % 60);
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
             [{name}] 1 file changed, 4 insertions(+)\n\
             [{name}] {spinner} tick {elapsed}"
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
             [{name}] Review complete: 1 warning, 0 errors\n\
             [{name}] {spinner} awaiting follow-up ({elapsed})"
        ),
        "architect" | "Architect" | "planning" => format!(
            "$ claude --role architect --task plan\n\
             [{name}] Evaluating architecture constraints...\n\
             [{name}] Module graph: 8 crates, 23 dependencies\n\
             [{name}] Identifying coupling hotspots...\n\
             [{name}]   at-core <-> at-bridge: 14 shared types\n\
             [{name}]   at-agents -> at-intelligence: 6 calls\n\
             [{name}] Suggested refactor: extract shared types to at-types\n\
             [{name}] Generating design document...\n\
             [{name}] {spinner} drafting sections ({elapsed})"
        ),
        "ops" | "Ops" | "devops" | "DevOps" => format!(
            "$ claude --role ops --task deploy\n\
             [{name}] Checking deployment prerequisites...\n\
             [{name}] Building release artifacts...\n\
             [{name}]   cargo build --release (2m 14s)\n\
             [{name}] Running health checks...\n\
             [{name}]   /health -> 200 OK (12ms)\n\
             [{name}]   /ready  -> 200 OK (8ms)\n\
             [{name}] Deployment staged. Awaiting approval.\n\
             [{name}] {spinner} pipeline idle ({elapsed})"
        ),
        _ => format!(
            "$ claude --role {role}\n\
             [{name}] Agent initialized\n\
             [{name}] Loading context...\n\
             [{name}] Processing task queue...\n\
             [{name}] Waiting for instructions...\n\
             [{name}] {spinner} heartbeat {elapsed}"
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

const TERMINAL_HISTORY_KEY: &str = "tundra_terminal_history_v1";
const TERMINAL_HISTORY_LIMIT: usize = 80;

fn history_timestamp() -> String {
    let d = js_sys::Date::new_0();
    format!(
        "{:02}:{:02}:{:02}",
        d.get_hours() as u8,
        d.get_minutes() as u8,
        d.get_seconds() as u8
    )
}

fn load_terminal_history() -> Vec<String> {
    let Some(window) = web_sys::window() else {
        return Vec::new();
    };
    let Ok(storage) = js_sys::Reflect::get(&window, &JsValue::from_str("localStorage")) else {
        return Vec::new();
    };
    if storage.is_null() || storage.is_undefined() {
        return Vec::new();
    }
    let Ok(get_item) = js_sys::Reflect::get(&storage, &JsValue::from_str("getItem")) else {
        return Vec::new();
    };
    let Some(get_item_fn) = get_item.dyn_ref::<js_sys::Function>() else {
        return Vec::new();
    };
    let Ok(raw_js) = get_item_fn.call1(&storage, &JsValue::from_str(TERMINAL_HISTORY_KEY)) else {
        return Vec::new();
    };
    let Some(raw) = raw_js.as_string() else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<String>>(&raw).unwrap_or_default()
}

fn save_terminal_history(entries: &[String]) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Ok(storage) = js_sys::Reflect::get(&window, &JsValue::from_str("localStorage")) else {
        return;
    };
    if storage.is_null() || storage.is_undefined() {
        return;
    }
    let Ok(set_item) = js_sys::Reflect::get(&storage, &JsValue::from_str("setItem")) else {
        return;
    };
    let Some(set_item_fn) = set_item.dyn_ref::<js_sys::Function>() else {
        return;
    };
    if let Ok(json) = serde_json::to_string(entries) {
        let _ = set_item_fn.call2(
            &storage,
            &JsValue::from_str(TERMINAL_HISTORY_KEY),
            &JsValue::from_str(&json),
        );
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
    let (stream_tick, set_stream_tick) = signal(0u64);
    let (show_history_menu, set_show_history_menu) = signal(false);
    let (show_files_drawer, set_show_files_drawer) = signal(false);
    let (history_focus, set_history_focus) = signal(Option::<String>::None);
    let (history_entries, set_history_entries) = signal(load_terminal_history());
    let (worktrees, set_worktrees) = signal(Vec::<api::ApiWorktree>::new());
    let (files_loading, set_files_loading) = signal(false);
    let (files_error, set_files_error) = signal(Option::<String>::None);
    let status_snapshot: Rc<RefCell<HashMap<String, String>>> = Rc::new(RefCell::new(HashMap::new()));

    let append_history = move |entry: String| {
        set_history_entries.update(|entries| {
            entries.insert(0, entry);
            if entries.len() > TERMINAL_HISTORY_LIMIT {
                entries.truncate(TERMINAL_HISTORY_LIMIT);
            }
            save_terminal_history(entries);
        });
    };

    let clear_history = move || {
        set_history_entries.set(Vec::new());
        save_terminal_history(&[]);
        set_history_focus.set(None);
    };

    let do_refresh_worktrees = move || {
        set_files_loading.set(true);
        set_files_error.set(None);
        spawn_local(async move {
            match api::fetch_worktrees().await {
                Ok(data) => set_worktrees.set(data),
                Err(e) => set_files_error.set(Some(format!("Failed to fetch worktrees: {e}"))),
            }
            set_files_loading.set(false);
        });
    };

    let do_refresh = move || {
        set_loading.set(true);
        set_error_msg.set(None);
        set_show_history_menu.set(false);
        set_show_files_drawer.set(false);
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

    Effect::new(move |_| {
        let current = agents.get();
        let mut previous = status_snapshot.borrow_mut();
        let mut now_ids = HashSet::new();
        let mut new_events: Vec<String> = Vec::new();

        for agent in &current {
            now_ids.insert(agent.id.clone());
            let status = if agent.status.is_empty() {
                "pending".to_string()
            } else {
                agent.status.clone()
            };
            match previous.get(&agent.id) {
                None => {
                    new_events.push(format!(
                        "{} {} ({}) joined as {}",
                        history_timestamp(),
                        agent.name,
                        agent.role,
                        status
                    ));
                }
                Some(last) if last != &status => {
                    new_events.push(format!(
                        "{} {} transitioned {} -> {}",
                        history_timestamp(),
                        agent.name,
                        last,
                        status
                    ));
                }
                _ => {}
            }
            previous.insert(agent.id.clone(), status);
        }

        let stale_ids: Vec<String> = previous
            .keys()
            .filter(|id| !now_ids.contains(*id))
            .cloned()
            .collect();
        for id in stale_ids {
            previous.remove(&id);
            new_events.push(format!(
                "{} agent session ended ({})",
                history_timestamp(),
                id
            ));
        }

        if !new_events.is_empty() {
            set_history_entries.update(|entries| {
                for evt in new_events {
                    entries.insert(0, evt);
                }
                if entries.len() > TERMINAL_HISTORY_LIMIT {
                    entries.truncate(TERMINAL_HISTORY_LIMIT);
                }
                save_terminal_history(entries);
            });
        }
    });

    // Terminal stream cadence ticker (kept lightweight, one shared tick).
    Effect::new(move |_| {
        let window = web_sys::window().expect("window");
        let cb = Closure::wrap(Box::new(move || {
            set_stream_tick.update(|t| *t = t.saturating_add(1));
        }) as Box<dyn FnMut()>);
        let _ = window.set_interval_with_callback_and_timeout_and_arguments_0(
            cb.as_ref().unchecked_ref(),
            1200,
        );
        cb.forget();
    });

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
    let recent_history = move || {
        let list = history_entries.get();
        if list.is_empty() {
            vec!["No terminal history yet".to_string()]
        } else {
            list.into_iter().take(12).collect::<Vec<_>>()
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
                <div class="terminal-dropdown">
                    <button
                        class="terminal-cmd-btn"
                        type="button"
                        on:click=move |_| {
                            set_show_history_menu.update(|v| *v = !*v);
                            set_show_files_drawer.set(false);
                        }
                    >
                        "History \u{25BE}"
                    </button>
                    {move || show_history_menu.get().then(|| view! {
                        <div class="terminal-dropdown-menu">
                            <button
                                class="terminal-dropdown-item terminal-dropdown-item-muted"
                                type="button"
                                on:click=move |_| {
                                    clear_history();
                                    set_show_history_menu.set(false);
                                }
                            >
                                "Clear persisted history"
                            </button>
                            {move || history_focus.get().map(|selected| view! {
                                <button
                                    class="terminal-dropdown-item terminal-dropdown-item-muted"
                                    type="button"
                                    on:click=move |_| {
                                        set_history_focus.set(None);
                                        set_show_history_menu.set(false);
                                    }
                                >
                                    {format!("Clear replay: {selected}")}
                                </button>
                            })}
                            {move || recent_history().into_iter().map(|entry| {
                                let select_entry = entry.clone();
                                view! {
                                    <button
                                        class="terminal-dropdown-item"
                                        type="button"
                                        on:click=move |_| {
                                            if select_entry == "No terminal history yet" {
                                                set_history_focus.set(None);
                                            } else {
                                                set_history_focus.set(Some(select_entry.clone()));
                                            }
                                            set_show_history_menu.set(false);
                                        }
                                    >
                                        {entry}
                                    </button>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                    })}
                </div>
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
                <button
                    class=(move || {
                        if show_files_drawer.get() {
                            "terminal-cmd-btn terminal-cmd-btn-active"
                        } else {
                            "terminal-cmd-btn"
                        }
                    })
                    type="button"
                    on:click=move |_| {
                        let opening = !show_files_drawer.get();
                        set_show_files_drawer.set(opening);
                        set_show_history_menu.set(false);
                        if opening {
                            do_refresh_worktrees();
                            append_history(format!("{} Opened files drawer", history_timestamp()));
                        }
                    }
                >
                    "\u{2398} Files"
                </button>
            </div>
        </div>
        {move || history_focus.get().map(|focus| view! {
            <div class="terminal-history-focus">
                <span>"Replaying session: "</span>
                <code>{focus}</code>
                <button
                    class="terminal-pane-icon-btn"
                    type="button"
                    title="Clear replay focus"
                    on:click=move |_| set_history_focus.set(None)
                >
                    "\u{2715}"
                </button>
            </div>
        })}
        {move || show_files_drawer.get().then(|| view! {
            <div class="terminal-files-drawer-layer">
                <div
                    class="terminal-files-drawer-backdrop"
                    on:click=move |_| set_show_files_drawer.set(false)
                ></div>
                <aside
                    class="terminal-files-drawer"
                    on:click=move |ev: leptos::ev::MouseEvent| ev.stop_propagation()
                >
                    <div class="terminal-files-drawer-header">
                        <div>
                            <div class="terminal-files-drawer-title">"Files Panel"</div>
                            <div class="terminal-files-drawer-subtitle">"Worktrees, context, and issue views"</div>
                        </div>
                        <button
                            class="terminal-pane-icon-btn"
                            type="button"
                            title="Close files drawer"
                            on:click=move |_| set_show_files_drawer.set(false)
                        >
                            "\u{2715}"
                        </button>
                    </div>

                    <div class="terminal-files-drawer-section">
                        <button class="terminal-files-link-btn" type="button" on:click=move |_| {
                            if let Some(set_tab) = set_current_tab {
                                set_tab.set(9);
                            }
                            set_show_files_drawer.set(false);
                        }>"Open Worktrees"</button>
                        <button class="terminal-files-link-btn" type="button" on:click=move |_| {
                            if let Some(set_tab) = set_current_tab {
                                set_tab.set(7);
                            }
                            set_show_files_drawer.set(false);
                        }>"Open Context"</button>
                        <button class="terminal-files-link-btn" type="button" on:click=move |_| {
                            if let Some(set_tab) = set_current_tab {
                                set_tab.set(10);
                            }
                            set_show_files_drawer.set(false);
                        }>"Open GitHub Issues"</button>
                        <button class="terminal-files-link-btn" type="button" on:click=move |_| {
                            if let Some(set_tab) = set_current_tab {
                                set_tab.set(11);
                            }
                            set_show_files_drawer.set(false);
                        }>"Open GitHub PRs"</button>
                    </div>

                    <div class="terminal-files-drawer-section terminal-files-drawer-section-scroll">
                        <div class="terminal-files-drawer-list-header">
                            <span>"Active Worktrees"</span>
                            <button class="terminal-files-refresh-btn" type="button" on:click=move |_| do_refresh_worktrees()>
                                "\u{21BB} Refresh"
                            </button>
                        </div>
                        {move || files_loading.get().then(|| view! {
                            <div class="terminal-files-drawer-empty">"Loading worktrees..."</div>
                        })}
                        {move || files_error.get().map(|msg| view! {
                            <div class="terminal-files-drawer-error">{msg}</div>
                        })}
                        {move || {
                            let rows = worktrees.get();
                            (!files_loading.get() && files_error.get().is_none() && rows.is_empty()).then(|| view! {
                                <div class="terminal-files-drawer-empty">"No worktrees found."</div>
                            })
                        }}
                        {move || worktrees.get().into_iter().map(|wt| {
                            let status = wt.status.clone();
                            let status_class = if status == "active" {
                                "terminal-files-status active"
                            } else {
                                "terminal-files-status"
                            };
                            view! {
                                <div class="terminal-files-item">
                                    <div class="terminal-files-item-main">
                                        <span class="terminal-files-branch">{wt.branch}</span>
                                        <span class="terminal-files-path">{wt.path}</span>
                                    </div>
                                    <span class={status_class}>{status}</span>
                                </div>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                </aside>
            </div>
        })}

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
                                        <pre class="agent-terminal-pre">{move || {
                                            let base = mock_terminal_output(&role, &name, stream_tick.get());
                                            if let Some(focus) = history_focus.get() {
                                                format!("$ replay --session \"{}\"\n{}", focus, base)
                                            } else {
                                                base
                                            }
                                        }}</pre>
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
                                        <pre class="agent-terminal-pre">{move || format!("$ {} waiting for terminal assignment...", spinner_frame(stream_tick.get()))}</pre>
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
