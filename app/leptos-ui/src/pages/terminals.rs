use crate::components::terminal_view::TerminalView;
use crate::i18n::t;
use crate::api::get_api_base;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, Response};

// ---------------------------------------------------------------------------
// API types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalInfo {
    pub id: String,
    pub title: String,
    pub status: String,
    pub cols: u16,
    pub rows: u16,
    #[serde(default = "default_font_size")]
    pub font_size: u16,
    #[serde(default = "default_cursor_style")]
    pub cursor_style: String,
    #[serde(default = "default_cursor_blink")]
    pub cursor_blink: bool,
    #[serde(default)]
    pub auto_name: Option<String>,
    #[serde(default)]
    pub persistent: bool,
}

fn default_font_size() -> u16 { 14 }
fn default_cursor_style() -> String { "block".to_string() }
fn default_cursor_blink() -> bool { true }

// ---------------------------------------------------------------------------
// API helpers
// ---------------------------------------------------------------------------

async fn api_create_terminal() -> Result<TerminalInfo, String> {
    let opts = RequestInit::new();
    opts.set_method("POST");
    let api_base = get_api_base();

    let request = Request::new_with_str_and_init(&format!("{api_base}/api/terminals"), &opts)
        .map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|e| format!("{e:?}"))?;

    let window = web_sys::window().ok_or("no global window")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: Response = resp_value.dyn_into().map_err(|e| format!("{e:?}"))?;
    let json = JsFuture::from(resp.json().map_err(|e| format!("{e:?}"))?)
        .await
        .map_err(|e| format!("{e:?}"))?;
    serde_wasm_bindgen::from_value(json).map_err(|e| format!("{e:?}"))
}

async fn api_list_terminals() -> Result<Vec<TerminalInfo>, String> {
    let opts = RequestInit::new();
    opts.set_method("GET");
    let api_base = get_api_base();

    let request = Request::new_with_str_and_init(&format!("{api_base}/api/terminals"), &opts)
        .map_err(|e| format!("{e:?}"))?;

    let window = web_sys::window().ok_or("no global window")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: Response = resp_value.dyn_into().map_err(|e| format!("{e:?}"))?;
    let json = JsFuture::from(resp.json().map_err(|e| format!("{e:?}"))?)
        .await
        .map_err(|e| format!("{e:?}"))?;
    serde_wasm_bindgen::from_value(json).map_err(|e| format!("{e:?}"))
}

async fn api_delete_terminal(id: &str) -> Result<(), String> {
    let opts = RequestInit::new();
    opts.set_method("DELETE");
    let api_base = get_api_base();

    let request =
        Request::new_with_str_and_init(&format!("{api_base}/api/terminals/{id}"), &opts)
            .map_err(|e| format!("{e:?}"))?;

    let window = web_sys::window().ok_or("no global window")?;
    JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Layout enum
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
// Terminals Page
// ---------------------------------------------------------------------------

#[component]
pub fn TerminalsPage() -> impl IntoView {
    let (terminals, set_terminals) = signal(Vec::<TerminalInfo>::new());
    let (layout, set_layout) = signal(GridLayout::Quad);
    let (error_msg, set_error_msg) = signal(None::<String>);

    // Load terminals on mount.
    Effect::new(move |_| {
        wasm_bindgen_futures::spawn_local(async move {
            match api_list_terminals().await {
                Ok(list) => set_terminals.set(list),
                Err(e) => set_error_msg.set(Some(format!("Failed to load terminals: {e}"))),
            }
        });
    });

    // Create terminal handler.
    let create_terminal = move |_| {
        wasm_bindgen_futures::spawn_local(async move {
            match api_create_terminal().await {
                Ok(info) => {
                    set_terminals.update(|list| list.push(info));
                    set_error_msg.set(None);
                }
                Err(e) => set_error_msg.set(Some(format!("Failed to create terminal: {e}"))),
            }
        });
    };

    // Close single terminal.
    let close_terminal = Callback::new(move |id: String| {
        let id_clone = id.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let _ = api_delete_terminal(&id_clone).await;
        });
        set_terminals.update(|list| list.retain(|t| t.id != id));
    });

    // Kill all terminals.
    let kill_all = move |_| {
        let current = terminals.get_untracked();
        for t in &current {
            let id = t.id.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let _ = api_delete_terminal(&id).await;
            });
        }
        set_terminals.set(Vec::new());
    };

    let terminal_count = move || terminals.get().len();
    let noop = move |_| {};

    view! {
        <div class="page-header">
            <h2>{t("terminals-title")}</h2>
            <div class="page-header-actions">
                <span class="terminal-count terminal-count-pill">
                    {move || {
                        let max = layout.get().max_panes();
                        format!("{}/{} terminals", terminal_count(), max)
                    }}
                </span>
            </div>
        </div>

        <div class="terminal-command-bar">
            <button class="terminal-cmd-btn" on:click=noop>
                "\u{21BB} History"
            </button>
            <button class="terminal-cmd-btn terminal-cmd-btn-magenta" on:click=noop>
                "\u{2699} Invoke Claude All"
            </button>
            <button
                class="new-terminal-btn"
                on:click=create_terminal
                disabled=move || terminal_count() >= layout.get().max_panes()
            >
                {format!("+ {}", t("terminals-new"))}
            </button>
            <button class="terminal-cmd-btn" on:click=noop>
                "\u{1F5C2} Files"
            </button>
        </div>

        // Toolbar with layout selector and kill all.
        <div class="terminal-toolbar">
            <button
                class=move || if layout.get() == GridLayout::Single { "layout-btn active" } else { "layout-btn" }
                on:click=move |_| set_layout.set(GridLayout::Single)
            >
                {t("terminals-layout-single")}
            </button>
            <button
                class=move || if layout.get() == GridLayout::Double { "layout-btn active" } else { "layout-btn" }
                on:click=move |_| set_layout.set(GridLayout::Double)
            >
                {t("terminals-layout-double")}
            </button>
            <button
                class=move || if layout.get() == GridLayout::Quad { "layout-btn active" } else { "layout-btn" }
                on:click=move |_| set_layout.set(GridLayout::Quad)
            >
                {t("terminals-layout-quad")}
            </button>
            <div style="flex: 1;"></div>
            <button
                class="kill-all-btn"
                on:click=kill_all
                disabled=move || terminal_count() == 0
            >
                {t("terminals-kill-all")}
            </button>
        </div>

        {move || error_msg.get().map(|msg| view! {
            <div class="terminal-error">{msg}</div>
        })}

        <div class=move || format!("terminal-grid {}", layout.get().css_class())>
            <For
                each=move || terminals.get()
                key=|info| info.id.clone()
                let:info
            >
                {
                    let on_close = close_terminal.clone();
                    let tid = info.id.clone();
                    let title = info.title.clone();
                    let c = info.cols as u32;
                    let r = info.rows as u32;
                    view! {
                        <TerminalView
                            terminal_id=tid
                            terminal_title=title
                            cols=c
                            rows=r
                            on_close=on_close
                        />
                    }
                }
            </For>
        </div>

        {move || terminals.get().is_empty().then(|| view! {
            <div class="terminal-empty">
                <div class="terminal-empty-icon">{"\u{1F5A5}\u{FE0F}"}</div>
                <div class="terminal-empty-text">"No terminals running"</div>
                <div class="terminal-empty-hint">"Click \"+ New Terminal\" to start a shell session"</div>
            </div>
        })}
    }
}
