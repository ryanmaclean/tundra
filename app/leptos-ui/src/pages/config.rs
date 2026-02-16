use leptos::prelude::*;

#[component]
pub fn ConfigPage() -> impl IntoView {
    // Active settings tab
    let (active_tab, set_active_tab) = signal(0usize);

    // Toast state
    let (show_toast, set_show_toast) = signal(false);
    let (toast_msg, set_toast_msg) = signal(String::new());

    // ── General Tab signals ──
    let (project_name, set_project_name) = signal("auto-tundra".to_string());
    let (default_cli, set_default_cli) = signal("Claude".to_string());
    let (auto_save, set_auto_save) = signal(true);
    let (max_parallel, set_max_parallel) = signal(4u32);

    // ── Display Tab signals ──
    let (theme, set_theme) = signal("Dark".to_string());
    let (font_size, set_font_size) = signal("14".to_string());
    let (compact_mode, set_compact_mode) = signal(false);

    // ── Agent Tab signals ──
    let (default_model, set_default_model) = signal("sonnet".to_string());
    let (thinking_level, set_thinking_level) = signal("medium".to_string());
    let (heartbeat_interval, set_heartbeat_interval) = signal(30u32);
    let (max_retries, set_max_retries) = signal(3u32);

    // ── Terminal Tab signals ──
    let (term_font_family, set_term_font_family) = signal("JetBrains Mono".to_string());
    let (term_font_size, set_term_font_size) = signal(14u32);
    let (cursor_style, set_cursor_style) = signal("block".to_string());

    // ── Security Tab signals ──
    let (api_key_masking, set_api_key_masking) = signal(true);
    let (auto_lock_timeout, set_auto_lock_timeout) = signal(15u32);
    let (sandbox_mode, set_sandbox_mode) = signal(true);

    // ── Integration Tab signals ──
    let (github_token, set_github_token) = signal(String::new());
    let (gitlab_token, set_gitlab_token) = signal(String::new());
    let (linear_api_key, set_linear_api_key) = signal(String::new());

    let tab_labels = vec![
        "General",
        "Display",
        "Agent",
        "Terminal",
        "Security",
        "Integration",
    ];

    let show_toast_fn = move |msg: &str| {
        set_toast_msg.set(msg.to_string());
        set_show_toast.set(true);
        // Auto-hide after 3 seconds
        let set_show = set_show_toast;
        leptos::task::spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(3_000).await;
            set_show.set(false);
        });
    };

    let on_save = move |_| {
        web_sys::console::log_1(&"Settings saved".into());
        show_toast_fn("Settings saved successfully!");
    };

    let on_reset = move |_| {
        // Reset General
        set_project_name.set("auto-tundra".to_string());
        set_default_cli.set("Claude".to_string());
        set_auto_save.set(true);
        set_max_parallel.set(4);
        // Reset Display
        set_theme.set("Dark".to_string());
        set_font_size.set("14".to_string());
        set_compact_mode.set(false);
        // Reset Agent
        set_default_model.set("sonnet".to_string());
        set_thinking_level.set("medium".to_string());
        set_heartbeat_interval.set(30);
        set_max_retries.set(3);
        // Reset Terminal
        set_term_font_family.set("JetBrains Mono".to_string());
        set_term_font_size.set(14);
        set_cursor_style.set("block".to_string());
        // Reset Security
        set_api_key_masking.set(true);
        set_auto_lock_timeout.set(15);
        set_sandbox_mode.set(true);
        // Reset Integration
        set_github_token.set(String::new());
        set_gitlab_token.set(String::new());
        set_linear_api_key.set(String::new());

        show_toast_fn("Settings reset to defaults");
    };

    view! {
        <div class="page-header">
            <h2>"Settings"</h2>
        </div>

        <div class="settings-tabbed-layout">
            // Left sidebar tabs
            <div class="settings-tab-sidebar">
                {tab_labels.into_iter().enumerate().map(|(i, label)| {
                    view! {
                        <button
                            class="settings-tab-btn"
                            class:active=move || active_tab.get() == i
                            on:click=move |_| set_active_tab.set(i)
                        >
                            {label}
                        </button>
                    }
                }).collect::<Vec<_>>()}
            </div>

            // Right content area
            <div class="settings-tab-content">
                {move || match active_tab.get() {
                    0 => view! {
                        <div class="settings-panel">
                            <h3 class="settings-panel-title">"General Settings"</h3>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"Project Name"</span>
                                    <span class="settings-hint">"Name of your project"</span>
                                </div>
                                <div class="settings-control">
                                    <input
                                        type="text"
                                        class="settings-text-input"
                                        prop:value=move || project_name.get()
                                        on:input=move |ev| set_project_name.set(event_target_value(&ev))
                                    />
                                </div>
                            </div>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"Default CLI"</span>
                                    <span class="settings-hint">"Which CLI to use for agent sessions"</span>
                                </div>
                                <div class="settings-control">
                                    <select
                                        class="settings-select"
                                        prop:value=move || default_cli.get()
                                        on:change=move |ev| set_default_cli.set(event_target_value(&ev))
                                    >
                                        <option value="Claude">"Claude"</option>
                                        <option value="Codex">"Codex"</option>
                                        <option value="Gemini">"Gemini"</option>
                                        <option value="OpenCode">"OpenCode"</option>
                                    </select>
                                </div>
                            </div>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"Auto-Save"</span>
                                    <span class="settings-hint">"Automatically save changes"</span>
                                </div>
                                <div class="settings-control">
                                    <label class="toggle-switch">
                                        <input
                                            type="checkbox"
                                            prop:checked=move || auto_save.get()
                                            on:change=move |ev| set_auto_save.set(event_target_checked(&ev))
                                        />
                                        <span class="toggle-slider"></span>
                                    </label>
                                    <span class="toggle-label">
                                        {move || if auto_save.get() { "On" } else { "Off" }}
                                    </span>
                                </div>
                            </div>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"Max Parallel Agents"</span>
                                    <span class="settings-hint">"Maximum concurrent agents (1-12)"</span>
                                </div>
                                <div class="settings-control">
                                    <input
                                        type="number"
                                        class="settings-number-input"
                                        min="1"
                                        max="12"
                                        prop:value=move || max_parallel.get().to_string()
                                        on:input=move |ev| {
                                            if let Ok(v) = event_target_value(&ev).parse::<u32>() {
                                                set_max_parallel.set(v.clamp(1, 12));
                                            }
                                        }
                                    />
                                </div>
                            </div>
                        </div>
                    }.into_any(),

                    1 => view! {
                        <div class="settings-panel">
                            <h3 class="settings-panel-title">"Display Settings"</h3>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"Theme"</span>
                                    <span class="settings-hint">"Select color theme"</span>
                                </div>
                                <div class="settings-control settings-radio-group">
                                    <label class="settings-radio">
                                        <input
                                            type="radio"
                                            name="theme"
                                            value="Dark"
                                            prop:checked=move || theme.get() == "Dark"
                                            on:change=move |_| set_theme.set("Dark".to_string())
                                        />
                                        <span>"Dark"</span>
                                    </label>
                                    <label class="settings-radio">
                                        <input
                                            type="radio"
                                            name="theme"
                                            value="Light"
                                            prop:checked=move || theme.get() == "Light"
                                            on:change=move |_| set_theme.set("Light".to_string())
                                        />
                                        <span>"Light"</span>
                                    </label>
                                    <label class="settings-radio">
                                        <input
                                            type="radio"
                                            name="theme"
                                            value="System"
                                            prop:checked=move || theme.get() == "System"
                                            on:change=move |_| set_theme.set("System".to_string())
                                        />
                                        <span>"System"</span>
                                    </label>
                                </div>
                            </div>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"Font Size"</span>
                                    <span class="settings-hint">"Base font size for the interface"</span>
                                </div>
                                <div class="settings-control">
                                    <select
                                        class="settings-select"
                                        prop:value=move || font_size.get()
                                        on:change=move |ev| set_font_size.set(event_target_value(&ev))
                                    >
                                        <option value="12">"12px"</option>
                                        <option value="13">"13px"</option>
                                        <option value="14">"14px"</option>
                                        <option value="16">"16px"</option>
                                    </select>
                                </div>
                            </div>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"Compact Mode"</span>
                                    <span class="settings-hint">"Reduce spacing for denser UI"</span>
                                </div>
                                <div class="settings-control">
                                    <label class="toggle-switch">
                                        <input
                                            type="checkbox"
                                            prop:checked=move || compact_mode.get()
                                            on:change=move |ev| set_compact_mode.set(event_target_checked(&ev))
                                        />
                                        <span class="toggle-slider"></span>
                                    </label>
                                    <span class="toggle-label">
                                        {move || if compact_mode.get() { "On" } else { "Off" }}
                                    </span>
                                </div>
                            </div>
                        </div>
                    }.into_any(),

                    2 => view! {
                        <div class="settings-panel">
                            <h3 class="settings-panel-title">"Agent Settings"</h3>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"Default Model"</span>
                                    <span class="settings-hint">"Model for new agent sessions"</span>
                                </div>
                                <div class="settings-control">
                                    <select
                                        class="settings-select"
                                        prop:value=move || default_model.get()
                                        on:change=move |ev| set_default_model.set(event_target_value(&ev))
                                    >
                                        <option value="opus">"Opus"</option>
                                        <option value="sonnet">"Sonnet"</option>
                                        <option value="haiku">"Haiku"</option>
                                    </select>
                                </div>
                            </div>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"Thinking Level"</span>
                                    <span class="settings-hint">"Extended thinking depth"</span>
                                </div>
                                <div class="settings-control">
                                    <select
                                        class="settings-select"
                                        prop:value=move || thinking_level.get()
                                        on:change=move |ev| set_thinking_level.set(event_target_value(&ev))
                                    >
                                        <option value="none">"None"</option>
                                        <option value="low">"Low"</option>
                                        <option value="medium">"Medium"</option>
                                        <option value="high">"High"</option>
                                    </select>
                                </div>
                            </div>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"Heartbeat Interval"</span>
                                    <span class="settings-hint">"Agent heartbeat interval in seconds"</span>
                                </div>
                                <div class="settings-control">
                                    <input
                                        type="number"
                                        class="settings-number-input"
                                        min="5"
                                        max="300"
                                        prop:value=move || heartbeat_interval.get().to_string()
                                        on:input=move |ev| {
                                            if let Ok(v) = event_target_value(&ev).parse::<u32>() {
                                                set_heartbeat_interval.set(v.clamp(5, 300));
                                            }
                                        }
                                    />
                                    <span class="settings-unit">"sec"</span>
                                </div>
                            </div>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"Max Retries"</span>
                                    <span class="settings-hint">"Maximum retry attempts on failure"</span>
                                </div>
                                <div class="settings-control">
                                    <input
                                        type="number"
                                        class="settings-number-input"
                                        min="0"
                                        max="10"
                                        prop:value=move || max_retries.get().to_string()
                                        on:input=move |ev| {
                                            if let Ok(v) = event_target_value(&ev).parse::<u32>() {
                                                set_max_retries.set(v.clamp(0, 10));
                                            }
                                        }
                                    />
                                </div>
                            </div>
                        </div>
                    }.into_any(),

                    3 => view! {
                        <div class="settings-panel">
                            <h3 class="settings-panel-title">"Terminal Settings"</h3>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"Font Family"</span>
                                    <span class="settings-hint">"Terminal font family name"</span>
                                </div>
                                <div class="settings-control">
                                    <input
                                        type="text"
                                        class="settings-text-input"
                                        prop:value=move || term_font_family.get()
                                        on:input=move |ev| set_term_font_family.set(event_target_value(&ev))
                                    />
                                </div>
                            </div>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"Font Size"</span>
                                    <span class="settings-hint">"Terminal font size in pixels"</span>
                                </div>
                                <div class="settings-control">
                                    <input
                                        type="number"
                                        class="settings-number-input"
                                        min="8"
                                        max="32"
                                        prop:value=move || term_font_size.get().to_string()
                                        on:input=move |ev| {
                                            if let Ok(v) = event_target_value(&ev).parse::<u32>() {
                                                set_term_font_size.set(v.clamp(8, 32));
                                            }
                                        }
                                    />
                                    <span class="settings-unit">"px"</span>
                                </div>
                            </div>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"Cursor Style"</span>
                                    <span class="settings-hint">"Terminal cursor appearance"</span>
                                </div>
                                <div class="settings-control">
                                    <select
                                        class="settings-select"
                                        prop:value=move || cursor_style.get()
                                        on:change=move |ev| set_cursor_style.set(event_target_value(&ev))
                                    >
                                        <option value="block">"Block"</option>
                                        <option value="underline">"Underline"</option>
                                        <option value="bar">"Bar"</option>
                                    </select>
                                </div>
                            </div>
                        </div>
                    }.into_any(),

                    4 => view! {
                        <div class="settings-panel">
                            <h3 class="settings-panel-title">"Security Settings"</h3>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"API Key Masking"</span>
                                    <span class="settings-hint">"Mask API keys in the interface"</span>
                                </div>
                                <div class="settings-control">
                                    <label class="toggle-switch">
                                        <input
                                            type="checkbox"
                                            prop:checked=move || api_key_masking.get()
                                            on:change=move |ev| set_api_key_masking.set(event_target_checked(&ev))
                                        />
                                        <span class="toggle-slider"></span>
                                    </label>
                                    <span class="toggle-label">
                                        {move || if api_key_masking.get() { "On" } else { "Off" }}
                                    </span>
                                </div>
                            </div>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"Auto-Lock Timeout"</span>
                                    <span class="settings-hint">"Lock after inactivity (minutes)"</span>
                                </div>
                                <div class="settings-control">
                                    <input
                                        type="number"
                                        class="settings-number-input"
                                        min="1"
                                        max="120"
                                        prop:value=move || auto_lock_timeout.get().to_string()
                                        on:input=move |ev| {
                                            if let Ok(v) = event_target_value(&ev).parse::<u32>() {
                                                set_auto_lock_timeout.set(v.clamp(1, 120));
                                            }
                                        }
                                    />
                                    <span class="settings-unit">"min"</span>
                                </div>
                            </div>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"Sandbox Mode"</span>
                                    <span class="settings-hint">"Run agents in sandboxed environment"</span>
                                </div>
                                <div class="settings-control">
                                    <label class="toggle-switch">
                                        <input
                                            type="checkbox"
                                            prop:checked=move || sandbox_mode.get()
                                            on:change=move |ev| set_sandbox_mode.set(event_target_checked(&ev))
                                        />
                                        <span class="toggle-slider"></span>
                                    </label>
                                    <span class="toggle-label">
                                        {move || if sandbox_mode.get() { "On" } else { "Off" }}
                                    </span>
                                </div>
                            </div>
                        </div>
                    }.into_any(),

                    5 => view! {
                        <div class="settings-panel">
                            <h3 class="settings-panel-title">"Integration Settings"</h3>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"GitHub Token"</span>
                                    <span class="settings-hint">"Personal access token for GitHub API"</span>
                                </div>
                                <div class="settings-control">
                                    <input
                                        type="password"
                                        class="settings-text-input"
                                        placeholder="ghp_..."
                                        prop:value=move || github_token.get()
                                        on:input=move |ev| set_github_token.set(event_target_value(&ev))
                                    />
                                </div>
                            </div>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"GitLab Token"</span>
                                    <span class="settings-hint">"Personal access token for GitLab API"</span>
                                </div>
                                <div class="settings-control">
                                    <input
                                        type="password"
                                        class="settings-text-input"
                                        placeholder="glpat-..."
                                        prop:value=move || gitlab_token.get()
                                        on:input=move |ev| set_gitlab_token.set(event_target_value(&ev))
                                    />
                                </div>
                            </div>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"Linear API Key"</span>
                                    <span class="settings-hint">"API key for Linear integration"</span>
                                </div>
                                <div class="settings-control">
                                    <input
                                        type="password"
                                        class="settings-text-input"
                                        placeholder="lin_api_..."
                                        prop:value=move || linear_api_key.get()
                                        on:input=move |ev| set_linear_api_key.set(event_target_value(&ev))
                                    />
                                </div>
                            </div>
                        </div>
                    }.into_any(),

                    _ => view! {
                        <div class="settings-panel">
                            <p>"Select a tab"</p>
                        </div>
                    }.into_any(),
                }}

                // Action buttons
                <div class="settings-actions">
                    <button class="btn-reset-settings" on:click=on_reset>
                        "Reset to Defaults"
                    </button>
                    <button class="btn-save-settings" on:click=on_save>
                        "Save Settings"
                    </button>
                </div>
            </div>
        </div>

        // Toast notification
        {move || show_toast.get().then(|| view! {
            <div class="settings-toast">
                {toast_msg.get()}
            </div>
        })}
    }
}

fn event_target_checked(ev: &leptos::ev::Event) -> bool {
    use wasm_bindgen::JsCast;
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|el| el.checked())
        .unwrap_or(false)
}
