use leptos::prelude::*;

#[component]
pub fn ConfigPage() -> impl IntoView {
    // Section collapse states
    let (general_open, set_general_open) = signal(true);
    let (agent_open, set_agent_open) = signal(true);
    let (integration_open, set_integration_open) = signal(true);
    let (display_open, set_display_open) = signal(true);

    // General Settings
    let (dark_theme, set_dark_theme) = signal(true);
    let (agent_max_count, set_agent_max_count) = signal(8u32);
    let (heartbeat_interval, set_heartbeat_interval) = signal(30u32);

    // Agent Configuration
    let (default_cli, set_default_cli) = signal("Claude".to_string());
    let (default_model, set_default_model) = signal("claude-sonnet-4".to_string());
    let (yolo_mode, set_yolo_mode) = signal(false);

    // Integration Settings
    let (github_token, set_github_token) = signal(String::new());
    let (project_dir, set_project_dir) = signal("/home/dev/auto-tundra".to_string());
    let (worktree_base, set_worktree_base) = signal("/home/dev/auto-tundra-wt".to_string());

    // Display Settings
    let (sidebar_collapsed, set_sidebar_collapsed) = signal(false);
    let (log_order, set_log_order) = signal("Chronological".to_string());
    let (ui_scale, set_ui_scale) = signal(100u32);

    let on_save = move |_| {
        web_sys::console::log_1(&format!(
            "Settings saved: theme={}, agents={}, heartbeat={}s, cli={}, model={}, yolo={}, dir={}, scale={}%",
            if dark_theme.get() { "dark" } else { "light" },
            agent_max_count.get(),
            heartbeat_interval.get(),
            default_cli.get(),
            default_model.get(),
            yolo_mode.get(),
            project_dir.get(),
            ui_scale.get(),
        ).into());
    };

    view! {
        <div class="page-header">
            <h2>"Settings"</h2>
        </div>
        <div class="settings-page">
            // General Settings
            <div class="settings-card">
                <div
                    class="settings-card-header"
                    on:click=move |_| set_general_open.set(!general_open.get())
                >
                    <span class="settings-card-title">"General Settings"</span>
                    <span class="settings-card-chevron">
                        {move || if general_open.get() { "\u{25BC}" } else { "\u{25B6}" }}
                    </span>
                </div>
                {move || general_open.get().then(|| view! {
                    <div class="settings-card-body">
                        <div class="settings-row">
                            <div class="settings-row-info">
                                <span class="settings-label">"Theme"</span>
                                <span class="settings-hint">"Toggle between dark and light mode"</span>
                            </div>
                            <div class="settings-control">
                                <label class="toggle-switch">
                                    <input
                                        type="checkbox"
                                        prop:checked=move || dark_theme.get()
                                        on:change=move |ev| {
                                            let checked = event_target_checked(&ev);
                                            set_dark_theme.set(checked);
                                        }
                                    />
                                    <span class="toggle-slider"></span>
                                </label>
                                <span class="toggle-label">
                                    {move || if dark_theme.get() { "Dark" } else { "Light" }}
                                </span>
                            </div>
                        </div>
                        <div class="settings-row">
                            <div class="settings-row-info">
                                <span class="settings-label">"Max Agent Count"</span>
                                <span class="settings-hint">"Maximum concurrent agents (1-12)"</span>
                            </div>
                            <div class="settings-control">
                                <input
                                    type="number"
                                    class="settings-number-input"
                                    min="1"
                                    max="12"
                                    prop:value=move || agent_max_count.get().to_string()
                                    on:input=move |ev| {
                                        if let Ok(v) = event_target_value(&ev).parse::<u32>() {
                                            set_agent_max_count.set(v.clamp(1, 12));
                                        }
                                    }
                                />
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
                    </div>
                })}
            </div>

            // Agent Configuration
            <div class="settings-card">
                <div
                    class="settings-card-header"
                    on:click=move |_| set_agent_open.set(!agent_open.get())
                >
                    <span class="settings-card-title">"Agent Configuration"</span>
                    <span class="settings-card-chevron">
                        {move || if agent_open.get() { "\u{25BC}" } else { "\u{25B6}" }}
                    </span>
                </div>
                {move || agent_open.get().then(|| view! {
                    <div class="settings-card-body">
                        <div class="settings-row">
                            <div class="settings-row-info">
                                <span class="settings-label">"Default CLI Type"</span>
                                <span class="settings-hint">"Which CLI to use for agent sessions"</span>
                            </div>
                            <div class="settings-control">
                                <select
                                    class="settings-select"
                                    prop:value=move || default_cli.get()
                                    on:change=move |ev| {
                                        set_default_cli.set(event_target_value(&ev));
                                    }
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
                                <span class="settings-label">"Default Model"</span>
                                <span class="settings-hint">"Model identifier for new agents"</span>
                            </div>
                            <div class="settings-control">
                                <input
                                    type="text"
                                    class="settings-text-input"
                                    prop:value=move || default_model.get()
                                    on:input=move |ev| {
                                        set_default_model.set(event_target_value(&ev));
                                    }
                                />
                            </div>
                        </div>
                        <div class="settings-row">
                            <div class="settings-row-info">
                                <span class="settings-label">"YOLO Mode"</span>
                                <span class="settings-hint-warning">"Dangerously skip all permission prompts"</span>
                            </div>
                            <div class="settings-control">
                                <label class="toggle-switch">
                                    <input
                                        type="checkbox"
                                        prop:checked=move || yolo_mode.get()
                                        on:change=move |ev| {
                                            set_yolo_mode.set(event_target_checked(&ev));
                                        }
                                    />
                                    <span class="toggle-slider toggle-danger"></span>
                                </label>
                                <span class="toggle-label">
                                    {move || if yolo_mode.get() { "Enabled" } else { "Disabled" }}
                                </span>
                            </div>
                        </div>
                    </div>
                })}
            </div>

            // Integration Settings
            <div class="settings-card">
                <div
                    class="settings-card-header"
                    on:click=move |_| set_integration_open.set(!integration_open.get())
                >
                    <span class="settings-card-title">"Integration Settings"</span>
                    <span class="settings-card-chevron">
                        {move || if integration_open.get() { "\u{25BC}" } else { "\u{25B6}" }}
                    </span>
                </div>
                {move || integration_open.get().then(|| view! {
                    <div class="settings-card-body">
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
                                    on:input=move |ev| {
                                        set_github_token.set(event_target_value(&ev));
                                    }
                                />
                            </div>
                        </div>
                        <div class="settings-row">
                            <div class="settings-row-info">
                                <span class="settings-label">"Project Directory"</span>
                                <span class="settings-hint">"Root directory of the project"</span>
                            </div>
                            <div class="settings-control">
                                <input
                                    type="text"
                                    class="settings-text-input"
                                    prop:value=move || project_dir.get()
                                    on:input=move |ev| {
                                        set_project_dir.set(event_target_value(&ev));
                                    }
                                />
                            </div>
                        </div>
                        <div class="settings-row">
                            <div class="settings-row-info">
                                <span class="settings-label">"Worktree Base Path"</span>
                                <span class="settings-hint">"Directory for git worktree checkouts"</span>
                            </div>
                            <div class="settings-control">
                                <input
                                    type="text"
                                    class="settings-text-input"
                                    prop:value=move || worktree_base.get()
                                    on:input=move |ev| {
                                        set_worktree_base.set(event_target_value(&ev));
                                    }
                                />
                            </div>
                        </div>
                    </div>
                })}
            </div>

            // Display Settings
            <div class="settings-card">
                <div
                    class="settings-card-header"
                    on:click=move |_| set_display_open.set(!display_open.get())
                >
                    <span class="settings-card-title">"Display Settings"</span>
                    <span class="settings-card-chevron">
                        {move || if display_open.get() { "\u{25BC}" } else { "\u{25B6}" }}
                    </span>
                </div>
                {move || display_open.get().then(|| view! {
                    <div class="settings-card-body">
                        <div class="settings-row">
                            <div class="settings-row-info">
                                <span class="settings-label">"Sidebar Collapsed"</span>
                                <span class="settings-hint">"Start with the sidebar collapsed"</span>
                            </div>
                            <div class="settings-control">
                                <label class="toggle-switch">
                                    <input
                                        type="checkbox"
                                        prop:checked=move || sidebar_collapsed.get()
                                        on:change=move |ev| {
                                            set_sidebar_collapsed.set(event_target_checked(&ev));
                                        }
                                    />
                                    <span class="toggle-slider"></span>
                                </label>
                                <span class="toggle-label">
                                    {move || if sidebar_collapsed.get() { "Collapsed" } else { "Expanded" }}
                                </span>
                            </div>
                        </div>
                        <div class="settings-row">
                            <div class="settings-row-info">
                                <span class="settings-label">"Log Order"</span>
                                <span class="settings-hint">"Order of log entries in the feed"</span>
                            </div>
                            <div class="settings-control">
                                <select
                                    class="settings-select"
                                    prop:value=move || log_order.get()
                                    on:change=move |ev| {
                                        set_log_order.set(event_target_value(&ev));
                                    }
                                >
                                    <option value="Chronological">"Chronological"</option>
                                    <option value="Reverse">"Reverse"</option>
                                </select>
                            </div>
                        </div>
                        <div class="settings-row">
                            <div class="settings-row-info">
                                <span class="settings-label">"UI Scale"</span>
                                <span class="settings-hint">
                                    {move || format!("Interface scaling ({}%)", ui_scale.get())}
                                </span>
                            </div>
                            <div class="settings-control settings-slider-control">
                                <span class="slider-label">"75%"</span>
                                <input
                                    type="range"
                                    class="settings-slider"
                                    min="75"
                                    max="200"
                                    step="5"
                                    prop:value=move || ui_scale.get().to_string()
                                    on:input=move |ev| {
                                        if let Ok(v) = event_target_value(&ev).parse::<u32>() {
                                            set_ui_scale.set(v);
                                        }
                                    }
                                />
                                <span class="slider-label">"200%"</span>
                            </div>
                        </div>
                    </div>
                })}
            </div>

            // Save button
            <div class="settings-actions">
                <button class="btn-save-settings" on:click=on_save>
                    "Save Settings"
                </button>
            </div>
        </div>
    }
}

fn event_target_checked(ev: &leptos::ev::Event) -> bool {
    use wasm_bindgen::JsCast;
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|el| el.checked())
        .unwrap_or(false)
}
