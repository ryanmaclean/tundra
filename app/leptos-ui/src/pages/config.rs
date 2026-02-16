use leptos::prelude::*;
use crate::api::{self, ApiSettings, ApiGeneralSettings, ApiDisplaySettings, ApiAgentsSettings, ApiTerminalSettings, ApiSecuritySettings, ApiIntegrationSettings, ApiCredentialStatus};

/// Helper to populate all signal setters from an `ApiSettings` struct.
fn apply_settings_to_signals(
    s: &ApiSettings,
    set_project_name: WriteSignal<String>,
    set_theme: WriteSignal<String>,
    set_font_size: WriteSignal<String>,
    set_compact_mode: WriteSignal<bool>,
    set_heartbeat_interval: WriteSignal<u32>,
    set_max_parallel: WriteSignal<u32>,
    set_term_font_family: WriteSignal<String>,
    set_term_font_size: WriteSignal<u32>,
    set_cursor_style: WriteSignal<String>,
    set_auto_lock_timeout: WriteSignal<u32>,
    set_sandbox_mode: WriteSignal<bool>,
    set_github_token_env: WriteSignal<String>,
    set_github_owner: WriteSignal<String>,
    set_github_repo: WriteSignal<String>,
    set_gitlab_token_env: WriteSignal<String>,
    set_linear_api_key_env: WriteSignal<String>,
) {
    set_project_name.set(s.general.project_name.clone());
    set_theme.set(s.display.theme.clone());
    set_font_size.set(s.display.font_size.to_string());
    set_compact_mode.set(s.display.compact_mode);
    set_heartbeat_interval.set(s.agents.heartbeat_interval_secs as u32);
    set_max_parallel.set(s.agents.max_concurrent);
    set_term_font_family.set(s.terminal.font_family.clone());
    set_term_font_size.set(s.terminal.font_size as u32);
    set_cursor_style.set(s.terminal.cursor_style.clone());
    set_auto_lock_timeout.set(s.security.auto_lock_timeout_mins);
    set_sandbox_mode.set(s.security.sandbox_mode);
    set_github_token_env.set(s.integrations.github_token_env.clone());
    set_github_owner.set(s.integrations.github_owner.clone().unwrap_or_default());
    set_github_repo.set(s.integrations.github_repo.clone().unwrap_or_default());
    set_gitlab_token_env.set(s.integrations.gitlab_token_env.clone());
    set_linear_api_key_env.set(s.integrations.linear_api_key_env.clone());
}

#[component]
pub fn ConfigPage() -> impl IntoView {
    // Active settings tab
    let (active_tab, set_active_tab) = signal(0usize);

    // Toast state
    let (show_toast, set_show_toast) = signal(false);
    let (toast_msg, set_toast_msg) = signal(String::new());

    // -- General Tab signals --
    let (project_name, set_project_name) = signal("auto-tundra".to_string());
    let (default_cli, set_default_cli) = signal("Claude".to_string());
    let (auto_save, set_auto_save) = signal(true);
    let (max_parallel, set_max_parallel) = signal(4u32);

    // -- Display Tab signals --
    let (theme, set_theme) = signal("Dark".to_string());
    let (font_size, set_font_size) = signal("14".to_string());
    let (compact_mode, set_compact_mode) = signal(false);

    // -- Agent Tab signals --
    let (default_model, set_default_model) = signal("sonnet".to_string());
    let (thinking_level, set_thinking_level) = signal("medium".to_string());
    let (heartbeat_interval, set_heartbeat_interval) = signal(30u32);
    let (max_retries, set_max_retries) = signal(3u32);

    // -- Terminal Tab signals --
    let (term_font_family, set_term_font_family) = signal("JetBrains Mono".to_string());
    let (term_font_size, set_term_font_size) = signal(14u32);
    let (cursor_style, set_cursor_style) = signal("block".to_string());

    // -- Security Tab signals --
    let (auto_lock_timeout, set_auto_lock_timeout) = signal(15u32);
    let (sandbox_mode, set_sandbox_mode) = signal(true);

    // -- Integration Tab signals (env var names, not secrets) --
    let (github_token_env, set_github_token_env) = signal("GITHUB_TOKEN".to_string());
    let (github_owner, set_github_owner) = signal(String::new());
    let (github_repo, set_github_repo) = signal(String::new());
    let (gitlab_token_env, set_gitlab_token_env) = signal("GITLAB_TOKEN".to_string());
    let (linear_api_key_env, set_linear_api_key_env) = signal("LINEAR_API_KEY".to_string());

    // -- Credential status (loaded from API) --
    let (cred_providers, set_cred_providers) = signal(Vec::<String>::new());

    // -- Load settings from API on mount --
    leptos::task::spawn_local(async move {
        match api::fetch_settings().await {
            Ok(s) => {
                apply_settings_to_signals(
                    &s,
                    set_project_name, set_theme, set_font_size, set_compact_mode,
                    set_heartbeat_interval, set_max_parallel,
                    set_term_font_family, set_term_font_size, set_cursor_style,
                    set_auto_lock_timeout, set_sandbox_mode,
                    set_github_token_env, set_github_owner, set_github_repo,
                    set_gitlab_token_env, set_linear_api_key_env,
                );
            }
            Err(e) => {
                web_sys::console::warn_1(&format!("Failed to load settings: {e}").into());
            }
        }
        // Also load credential status
        match api::fetch_credential_status().await {
            Ok(status) => {
                set_cred_providers.set(status.providers);
            }
            Err(e) => {
                web_sys::console::warn_1(&format!("Failed to load credential status: {e}").into());
            }
        }
    });

    let tab_labels = vec![
        "General",
        "Display",
        "Agent",
        "Terminal",
        "Security",
        "Integration",
    ];

    let _show_toast_fn = move |msg: &str| {
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
        let settings = build_settings_from_signals(
            &project_name, &theme, &font_size, &compact_mode,
            &heartbeat_interval, &max_parallel,
            &term_font_family, &term_font_size, &cursor_style,
            &auto_lock_timeout, &sandbox_mode,
            &github_token_env, &github_owner, &github_repo,
            &gitlab_token_env, &linear_api_key_env,
        );
        leptos::task::spawn_local(async move {
            match api::save_settings(&settings).await {
                Ok(_) => {
                    set_toast_msg.set("Settings saved successfully!".to_string());
                    set_show_toast.set(true);
                    let set_show = set_show_toast;
                    leptos::task::spawn_local(async move {
                        gloo_timers::future::TimeoutFuture::new(3_000).await;
                        set_show.set(false);
                    });
                }
                Err(e) => {
                    set_toast_msg.set(format!("Failed to save: {e}"));
                    set_show_toast.set(true);
                    let set_show = set_show_toast;
                    leptos::task::spawn_local(async move {
                        gloo_timers::future::TimeoutFuture::new(3_000).await;
                        set_show.set(false);
                    });
                }
            }
        });
    };

    let on_reset = move |_| {
        // Reset to defaults locally
        set_project_name.set("auto-tundra".to_string());
        set_default_cli.set("Claude".to_string());
        set_auto_save.set(true);
        set_max_parallel.set(4);
        set_theme.set("dark".to_string());
        set_font_size.set("14".to_string());
        set_compact_mode.set(false);
        set_default_model.set("sonnet".to_string());
        set_thinking_level.set("medium".to_string());
        set_heartbeat_interval.set(30);
        set_max_retries.set(3);
        set_term_font_family.set("JetBrains Mono".to_string());
        set_term_font_size.set(14);
        set_cursor_style.set("block".to_string());
        set_auto_lock_timeout.set(15);
        set_sandbox_mode.set(true);
        set_github_token_env.set("GITHUB_TOKEN".to_string());
        set_github_owner.set(String::new());
        set_github_repo.set(String::new());
        set_gitlab_token_env.set("GITLAB_TOKEN".to_string());
        set_linear_api_key_env.set("LINEAR_API_KEY".to_string());

        // Save defaults to backend
        let settings = ApiSettings::default();
        leptos::task::spawn_local(async move {
            match api::save_settings(&settings).await {
                Ok(_) => {
                    set_toast_msg.set("Settings reset to defaults".to_string());
                    set_show_toast.set(true);
                    let set_show = set_show_toast;
                    leptos::task::spawn_local(async move {
                        gloo_timers::future::TimeoutFuture::new(3_000).await;
                        set_show.set(false);
                    });
                }
                Err(e) => {
                    set_toast_msg.set(format!("Failed to reset: {e}"));
                    set_show_toast.set(true);
                    let set_show = set_show_toast;
                    leptos::task::spawn_local(async move {
                        gloo_timers::future::TimeoutFuture::new(3_000).await;
                        set_show.set(false);
                    });
                }
            }
        });
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
                            <p class="settings-hint">
                                "Credentials are read from environment variables at runtime. "
                                "Set the env vars in your shell profile, then restart the daemon."
                            </p>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"GitHub Token Env Var"</span>
                                    <span class="settings-hint">"Name of the environment variable holding your GitHub PAT"</span>
                                </div>
                                <div class="settings-control">
                                    <input
                                        type="text"
                                        class="settings-text-input"
                                        prop:value=move || github_token_env.get()
                                        on:input=move |ev| set_github_token_env.set(event_target_value(&ev))
                                    />
                                    <span class="settings-badge">
                                        {move || if cred_providers.get().contains(&"github".to_string()) { "Connected" } else { "Not set" }}
                                    </span>
                                </div>
                            </div>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"GitHub Owner"</span>
                                    <span class="settings-hint">"GitHub org or user name"</span>
                                </div>
                                <div class="settings-control">
                                    <input
                                        type="text"
                                        class="settings-text-input"
                                        placeholder="my-org"
                                        prop:value=move || github_owner.get()
                                        on:input=move |ev| set_github_owner.set(event_target_value(&ev))
                                    />
                                </div>
                            </div>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"GitHub Repo"</span>
                                    <span class="settings-hint">"GitHub repository name"</span>
                                </div>
                                <div class="settings-control">
                                    <input
                                        type="text"
                                        class="settings-text-input"
                                        placeholder="my-repo"
                                        prop:value=move || github_repo.get()
                                        on:input=move |ev| set_github_repo.set(event_target_value(&ev))
                                    />
                                </div>
                            </div>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"GitLab Token Env Var"</span>
                                    <span class="settings-hint">"Name of the environment variable holding your GitLab token"</span>
                                </div>
                                <div class="settings-control">
                                    <input
                                        type="text"
                                        class="settings-text-input"
                                        prop:value=move || gitlab_token_env.get()
                                        on:input=move |ev| set_gitlab_token_env.set(event_target_value(&ev))
                                    />
                                    <span class="settings-badge">
                                        {move || if cred_providers.get().contains(&"gitlab".to_string()) { "Connected" } else { "Not set" }}
                                    </span>
                                </div>
                            </div>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"Linear API Key Env Var"</span>
                                    <span class="settings-hint">"Name of the environment variable holding your Linear API key"</span>
                                </div>
                                <div class="settings-control">
                                    <input
                                        type="text"
                                        class="settings-text-input"
                                        prop:value=move || linear_api_key_env.get()
                                        on:input=move |ev| set_linear_api_key_env.set(event_target_value(&ev))
                                    />
                                    <span class="settings-badge">
                                        {move || if cred_providers.get().contains(&"linear".to_string()) { "Connected" } else { "Not set" }}
                                    </span>
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

fn build_settings_from_signals(
    project_name: &ReadSignal<String>,
    theme: &ReadSignal<String>,
    font_size: &ReadSignal<String>,
    compact_mode: &ReadSignal<bool>,
    heartbeat_interval: &ReadSignal<u32>,
    max_parallel: &ReadSignal<u32>,
    term_font_family: &ReadSignal<String>,
    term_font_size: &ReadSignal<u32>,
    cursor_style: &ReadSignal<String>,
    auto_lock_timeout: &ReadSignal<u32>,
    sandbox_mode: &ReadSignal<bool>,
    github_token_env: &ReadSignal<String>,
    github_owner: &ReadSignal<String>,
    github_repo: &ReadSignal<String>,
    gitlab_token_env: &ReadSignal<String>,
    linear_api_key_env: &ReadSignal<String>,
) -> ApiSettings {
    let gh_owner = github_owner.get();
    let gh_repo = github_repo.get();

    ApiSettings {
        general: ApiGeneralSettings {
            project_name: project_name.get(),
            log_level: "info".to_string(),
            workspace_root: None,
        },
        display: ApiDisplaySettings {
            theme: theme.get(),
            font_size: font_size.get().parse().unwrap_or(14),
            compact_mode: compact_mode.get(),
        },
        agents: ApiAgentsSettings {
            max_concurrent: max_parallel.get(),
            heartbeat_interval_secs: heartbeat_interval.get() as u64,
            auto_restart: false,
        },
        terminal: ApiTerminalSettings {
            font_family: term_font_family.get(),
            font_size: term_font_size.get() as u8,
            cursor_style: cursor_style.get(),
        },
        security: ApiSecuritySettings {
            allow_shell_exec: false,
            sandbox: sandbox_mode.get(),
            allowed_paths: Vec::new(),
            auto_lock_timeout_mins: auto_lock_timeout.get(),
            sandbox_mode: sandbox_mode.get(),
        },
        integrations: ApiIntegrationSettings {
            github_token_env: github_token_env.get(),
            github_owner: if gh_owner.is_empty() { None } else { Some(gh_owner) },
            github_repo: if gh_repo.is_empty() { None } else { Some(gh_repo) },
            gitlab_token_env: gitlab_token_env.get(),
            linear_api_key_env: linear_api_key_env.get(),
        },
    }
}
