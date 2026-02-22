use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::i18n::t;

/// Models for each provider's available models.
fn models_for_provider(provider: &str) -> Vec<&'static str> {
    match provider {
        "anthropic" => vec![
            "claude-opus-4-0-20250514",
            "claude-sonnet-4-0-20250514",
            "claude-3-5-haiku-20241022",
        ],
        "openai" => vec![
            "gpt-4o",
            "gpt-4o-mini",
            "o3-mini",
        ],
        "google" => vec![
            "gemini-2.0-flash",
            "gemini-2.0-pro",
            "gemini-1.5-pro",
        ],
        _ => vec![],
    }
}

#[component]
pub fn OnboardingPage() -> impl IntoView {
    // Wizard step: 1-7
    let (step, set_step) = signal(1u8);

    // Step 2: Auth
    let (auth_method, set_auth_method) = signal("env".to_string());
    let (cred_checking, set_cred_checking) = signal(false);
    let (cred_result, set_cred_result) = signal(Option::<Result<api::ApiCredentialStatus, String>>::None);

    // Step 3: Tool/IDE Preferences
    let (ide, set_ide) = signal("vscode".to_string());
    let (terminal, set_terminal) = signal("iterm2".to_string());
    let (git_tool, set_git_tool) = signal("git".to_string());

    // Step 4: Agent Configuration
    let (provider, set_provider) = signal("anthropic".to_string());
    let (model, set_model) = signal("claude-opus-4-0-20250514".to_string());
    let (thinking, set_thinking) = signal("medium".to_string());
    let (max_agents, set_max_agents) = signal(4u32);

    // Step 5: Memory System
    let (enable_memory, set_enable_memory) = signal(true);
    let (enable_graphiti, set_enable_graphiti) = signal(false);
    let (embedding_model, set_embedding_model) = signal("text-embedding-3-small".to_string());

    // Step 6: First Task
    let (task_title, set_task_title) = signal(String::new());
    let (task_desc, set_task_desc) = signal(String::new());
    let (task_submitting, set_task_submitting) = signal(false);

    // Global
    let (saving, set_saving) = signal(false);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);

    // Check credentials
    let on_check_creds = move |_| {
        set_cred_checking.set(true);
        set_cred_result.set(None);
        spawn_local(async move {
            let result = api::fetch_credential_status().await;
            set_cred_result.set(Some(result));
            set_cred_checking.set(false);
        });
    };

    // Save settings and advance from step 5 to 6
    let save_and_continue = move |_| {
        set_saving.set(true);
        set_error_msg.set(None);
        let ide_val = ide.get();
        let terminal_val = terminal.get();
        let provider_val = provider.get();
        let model_val = model.get();
        let thinking_val = thinking.get();
        let max_agents_val = max_agents.get();
        let memory_val = enable_memory.get();
        let graphiti_val = enable_graphiti.get();
        let embed_val = embedding_model.get();
        spawn_local(async move {
            // Fetch current settings, merge our values, then save
            match api::fetch_settings().await {
                Ok(mut settings) => {
                    settings.dev_tools.preferred_ide = ide_val;
                    settings.dev_tools.preferred_terminal = terminal_val;
                    settings.agent_profile.default_profile = provider_val;
                    settings.agents.max_concurrent = max_agents_val;
                    settings.memory.enable_memory = memory_val;
                    settings.memory.embedding_model = embed_val;
                    // Store model + thinking in phase_configs
                    settings.agent_profile.phase_configs = vec![
                        api::ApiPhaseConfig {
                            phase: "default".to_string(),
                            model: model_val,
                            thinking_level: thinking_val,
                        },
                    ];
                    if graphiti_val && settings.memory.graphiti_server_url.is_empty() {
                        settings.memory.graphiti_server_url = "http://localhost:8000".to_string();
                    }
                    match api::save_settings(&settings).await {
                        Ok(_) => set_step.set(6),
                        Err(e) => set_error_msg.set(Some(format!("Failed to save settings: {e}"))),
                    }
                }
                Err(e) => {
                    // If backend is not running, just advance anyway
                    leptos::logging::log!("Could not fetch settings: {e}");
                    set_step.set(6);
                }
            }
            set_saving.set(false);
        });
    };

    // Create first task
    let on_create_task = move |_| {
        let title = task_title.get();
        let desc = task_desc.get();
        if title.trim().is_empty() {
            set_error_msg.set(Some("Please enter a task title.".to_string()));
            return;
        }
        set_task_submitting.set(true);
        set_error_msg.set(None);
        spawn_local(async move {
            // Create a bead first, then a task linked to it
            let desc_opt = if desc.trim().is_empty() { None } else { Some(desc.as_str()) };
            match api::create_bead(&title, desc_opt, Some("backlog")).await {
                Ok(bead) => {
                    let _ = api::create_task(
                        &title,
                        desc_opt,
                        &bead.id,
                        "medium",
                        "medium",
                        "feature",
                    ).await;
                    set_step.set(7);
                }
                Err(e) => {
                    leptos::logging::log!("Could not create bead: {e}");
                    // Still advance if backend is offline
                    set_step.set(7);
                }
            }
            set_task_submitting.set(false);
        });
    };

    // Navigate to dashboard (tab 0)
    let go_to_dashboard = move |_| {
        // Reload the page to get back to the main app
        if let Some(window) = web_sys::window() {
            let _ = window.location().set_href("/");
        }
    };

    // Helper: option card class
    let option_class = move |current: &str, value: &str| -> &'static str {
        if current == value {
            "onboarding-option-card selected"
        } else {
            "onboarding-option-card"
        }
    };

    view! {
        <div class="onboarding-container">
            // Progress indicator
            <div class="onboarding-progress">
                {(1..=7).map(|i| {
                    view! {
                        <div class=move || {
                            if step.get() == i { "onboarding-dot active" }
                            else if step.get() > i { "onboarding-dot completed" }
                            else { "onboarding-dot" }
                        }>
                            {i}
                        </div>
                        {(i < 7).then(|| view! {
                            <div class=move || {
                                if step.get() > i { "onboarding-connector completed" }
                                else { "onboarding-connector" }
                            }></div>
                        })}
                    }
                }).collect::<Vec<_>>()}
            </div>

            {move || error_msg.get().map(|msg| view! {
                <div class="dashboard-error">{msg}</div>
            })}

            // ── Step 1: Welcome ──
            {move || (step.get() == 1).then(|| view! {
                <div class="onboarding-step">
                    <div class="onboarding-logo">
                        <div class="onboarding-logo-icon">"AT"</div>
                    </div>
                    <h1 class="onboarding-title">{t("onboarding-welcome")}</h1>
                    <p class="onboarding-subtitle">
                        "AI-powered development orchestrator. Manage agents, tasks, and workflows from a unified dashboard."
                    </p>
                    <div class="onboarding-actions">
                        <button
                            class="btn btn-primary btn-lg onboarding-btn-primary"
                            on:click=move |_| set_step.set(2)
                        >
                            "Get Started"
                        </button>
                    </div>
                </div>
            })}

            // ── Step 2: Auth Method ──
            {move || (step.get() == 2).then(|| view! {
                <div class="onboarding-step">
                    <h2 class="onboarding-step-title">{t("onboarding-auth")}</h2>
                    <p class="onboarding-step-desc">"Choose how Auto-Tundra accesses AI provider credentials."</p>

                    <div class="onboarding-option-grid">
                        <div
                            class=move || option_class(&auth_method.get(), "env")
                            on:click=move |_| set_auth_method.set("env".to_string())
                        >
                            <div class="onboarding-option-icon">"$"</div>
                            <div class="onboarding-option-label">"Environment Variables"</div>
                            <div class="onboarding-option-badge">"Recommended"</div>
                        </div>
                        <div
                            class=move || option_class(&auth_method.get(), "config")
                            on:click=move |_| set_auth_method.set("config".to_string())
                        >
                            <div class="onboarding-option-icon">"F"</div>
                            <div class="onboarding-option-label">"Config File"</div>
                            <div class="onboarding-option-desc">"~/.auto-tundra/credentials"</div>
                        </div>
                        <div
                            class=move || option_class(&auth_method.get(), "oauth")
                            on:click=move |_| set_auth_method.set("oauth".to_string())
                        >
                            <div class="onboarding-option-icon">"O"</div>
                            <div class="onboarding-option-label">"OAuth"</div>
                            <div class="onboarding-option-desc">"Browser-based sign-in"</div>
                        </div>
                    </div>

                    {move || (auth_method.get() == "env").then(|| view! {
                        <div class="onboarding-env-vars">
                            <h3>"Required Environment Variables"</h3>
                            <div class="onboarding-env-list">
                                <code>"ANTHROPIC_API_KEY"</code>
                                <code>"OPENAI_API_KEY"</code>
                                <code>"OPENROUTER_API_KEY"</code>
                                <code>"GITHUB_TOKEN"</code>
                            </div>
                            <p class="onboarding-env-hint">
                                "Set these in your shell profile (.zshrc, .bashrc) or .envrc file."
                            </p>
                        </div>
                    })}

                    <div class="onboarding-check-section">
                        <button
                            class="btn btn-outline"
                            on:click=on_check_creds
                            disabled=move || cred_checking.get()
                        >
                            {move || if cred_checking.get() { "Checking..." } else { "Verify Credentials" }}
                        </button>
                        {move || cred_result.get().map(|result| match result {
                            Ok(status) => {
                                let providers = status.providers.clone();
                                let count = providers.len();
                                view! {
                                    <div class="onboarding-check-result success">
                                        <span class="onboarding-check-icon">"[ok]"</span>
                                        <span>{format!("{} provider{} configured: {}", count, if count == 1 { "" } else { "s" }, providers.join(", "))}</span>
                                    </div>
                                }.into_any()
                            }
                            Err(e) => view! {
                                <div class="onboarding-check-result error">
                                    <span class="onboarding-check-icon">"[!]"</span>
                                    <span>{format!("Could not verify: {e}")}</span>
                                </div>
                            }.into_any(),
                        })}
                    </div>

                    <div class="onboarding-actions">
                        <button class="btn btn-outline" on:click=move |_| set_step.set(1)>{t("btn-back")}</button>
                        <button class="btn btn-primary onboarding-btn-primary" on:click=move |_| set_step.set(3)>{t("btn-next")}</button>
                    </div>
                </div>
            })}

            // ── Step 3: Tool/IDE Preferences ──
            {move || (step.get() == 3).then(|| view! {
                <div class="onboarding-step">
                    <h2 class="onboarding-step-title">{t("onboarding-tools")}</h2>
                    <p class="onboarding-step-desc">"Select your preferred development tools."</p>

                    <div class="onboarding-form-section">
                        <label class="onboarding-form-label">"IDE / Editor"</label>
                        <div class="onboarding-option-grid small">
                            {["vscode", "cursor", "zed", "neovim", "other"].iter().map(|val| {
                                let v = val.to_string();
                                let v2 = val.to_string();
                                let display = match *val {
                                    "vscode" => "VS Code",
                                    "cursor" => "Cursor",
                                    "zed" => "Zed",
                                    "neovim" => "Neovim",
                                    "other" => "Other",
                                    _ => val,
                                };
                                view! {
                                    <div
                                        class=move || option_class(&ide.get(), &v)
                                        on:click=move |_| set_ide.set(v2.clone())
                                    >
                                        <div class="onboarding-option-label">{display}</div>
                                    </div>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                    </div>

                    <div class="onboarding-form-section">
                        <label class="onboarding-form-label">"Terminal"</label>
                        <div class="onboarding-option-grid small">
                            {["iterm2", "terminal", "warp", "alacritty"].iter().map(|val| {
                                let v = val.to_string();
                                let v2 = val.to_string();
                                let display = match *val {
                                    "iterm2" => "iTerm2",
                                    "terminal" => "Terminal.app",
                                    "warp" => "Warp",
                                    "alacritty" => "Alacritty",
                                    _ => val,
                                };
                                view! {
                                    <div
                                        class=move || option_class(&terminal.get(), &v)
                                        on:click=move |_| set_terminal.set(v2.clone())
                                    >
                                        <div class="onboarding-option-label">{display}</div>
                                    </div>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                    </div>

                    <div class="onboarding-form-section">
                        <label class="onboarding-form-label">"Git Tool"</label>
                        <div class="onboarding-option-grid small">
                            <div
                                class=move || option_class(&git_tool.get(), "git")
                                on:click=move |_| set_git_tool.set("git".to_string())
                            >
                                <div class="onboarding-option-label">"git CLI"</div>
                                <div class="onboarding-option-badge">"Default"</div>
                            </div>
                            <div
                                class=move || option_class(&git_tool.get(), "gh")
                                on:click=move |_| set_git_tool.set("gh".to_string())
                            >
                                <div class="onboarding-option-label">"GitHub CLI"</div>
                            </div>
                        </div>
                    </div>

                    <div class="onboarding-actions">
                        <button class="btn btn-outline" on:click=move |_| set_step.set(2)>{t("btn-back")}</button>
                        <button class="btn btn-primary onboarding-btn-primary" on:click=move |_| set_step.set(4)>{t("btn-next")}</button>
                    </div>
                </div>
            })}

            // ── Step 4: Agent Configuration ──
            {move || (step.get() == 4).then(|| view! {
                <div class="onboarding-step">
                    <h2 class="onboarding-step-title">{t("onboarding-agent")}</h2>
                    <p class="onboarding-step-desc">"Configure the default AI agent behaviour."</p>

                    <div class="onboarding-form-section">
                        <label class="onboarding-form-label">"Default AI Provider"</label>
                        <div class="onboarding-option-grid">
                            <div
                                class=move || option_class(&provider.get(), "anthropic")
                                on:click=move |_| {
                                    set_provider.set("anthropic".to_string());
                                    set_model.set("claude-opus-4-0-20250514".to_string());
                                }
                            >
                                <div class="onboarding-option-icon">"A"</div>
                                <div class="onboarding-option-label">"Anthropic"</div>
                                <div class="onboarding-option-desc">"Claude"</div>
                            </div>
                            <div
                                class=move || option_class(&provider.get(), "openai")
                                on:click=move |_| {
                                    set_provider.set("openai".to_string());
                                    set_model.set("gpt-4o".to_string());
                                }
                            >
                                <div class="onboarding-option-icon">"O"</div>
                                <div class="onboarding-option-label">"OpenAI"</div>
                                <div class="onboarding-option-desc">"GPT"</div>
                            </div>
                            <div
                                class=move || option_class(&provider.get(), "google")
                                on:click=move |_| {
                                    set_provider.set("google".to_string());
                                    set_model.set("gemini-2.0-flash".to_string());
                                }
                            >
                                <div class="onboarding-option-icon">"G"</div>
                                <div class="onboarding-option-label">"Google"</div>
                                <div class="onboarding-option-desc">"Gemini"</div>
                            </div>
                        </div>
                    </div>

                    <div class="onboarding-form-section">
                        <label class="onboarding-form-label">"Default Model"</label>
                        <select
                            class="form-input onboarding-select"
                            prop:value=move || model.get()
                            on:change=move |ev| set_model.set(event_target_value(&ev))
                        >
                            {move || {
                                let prov = provider.get();
                                models_for_provider(&prov).iter().map(|m| {
                                    let m_str = m.to_string();
                                    let m_val = m.to_string();
                                    view! {
                                        <option value={m_val}>{m_str}</option>
                                    }
                                }).collect::<Vec<_>>()
                            }}
                        </select>
                    </div>

                    <div class="onboarding-form-section">
                        <label class="onboarding-form-label">"Thinking Level"</label>
                        <div class="onboarding-option-grid small">
                            {["none", "low", "medium", "high"].iter().map(|val| {
                                let v = val.to_string();
                                let v2 = val.to_string();
                                let display = match *val {
                                    "none" => "None",
                                    "low" => "Low",
                                    "medium" => "Medium",
                                    "high" => "High",
                                    _ => val,
                                };
                                view! {
                                    <div
                                        class=move || option_class(&thinking.get(), &v)
                                        on:click=move |_| set_thinking.set(v2.clone())
                                    >
                                        <div class="onboarding-option-label">{display}</div>
                                    </div>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                    </div>

                    <div class="onboarding-form-section">
                        <label class="onboarding-form-label">
                            {move || format!("Max Parallel Agents: {}", max_agents.get())}
                        </label>
                        <input
                            type="range"
                            class="onboarding-slider"
                            min="1"
                            max="12"
                            prop:value=move || max_agents.get().to_string()
                            on:input=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<u32>() {
                                    set_max_agents.set(v);
                                }
                            }
                        />
                        <div class="onboarding-slider-labels">
                            <span>"1"</span>
                            <span>"6"</span>
                            <span>"12"</span>
                        </div>
                    </div>

                    <div class="onboarding-actions">
                        <button class="btn btn-outline" on:click=move |_| set_step.set(3)>{t("btn-back")}</button>
                        <button class="btn btn-primary onboarding-btn-primary" on:click=move |_| set_step.set(5)>{t("btn-next")}</button>
                    </div>
                </div>
            })}

            // ── Step 5: Memory System ──
            {move || (step.get() == 5).then(|| view! {
                <div class="onboarding-step">
                    <h2 class="onboarding-step-title">{t("onboarding-memory")}</h2>
                    <p class="onboarding-step-desc">"Configure persistent memory for agents."</p>

                    <div class="onboarding-toggle-row">
                        <label class="onboarding-toggle-label">"Enable Memory System"</label>
                        <button
                            class=move || if enable_memory.get() { "onboarding-toggle on" } else { "onboarding-toggle" }
                            on:click=move |_| set_enable_memory.set(!enable_memory.get())
                        >
                            <span class="onboarding-toggle-knob"></span>
                        </button>
                    </div>

                    <div class="onboarding-toggle-row">
                        <label class="onboarding-toggle-label">"Enable Graphiti Graph Memory"</label>
                        <button
                            class=move || if enable_graphiti.get() { "onboarding-toggle on" } else { "onboarding-toggle" }
                            on:click=move |_| set_enable_graphiti.set(!enable_graphiti.get())
                        >
                            <span class="onboarding-toggle-knob"></span>
                        </button>
                    </div>

                    <div class="onboarding-form-section">
                        <label class="onboarding-form-label">"Embeddings Model"</label>
                        <select
                            class="form-input onboarding-select"
                            prop:value=move || embedding_model.get()
                            on:change=move |ev| set_embedding_model.set(event_target_value(&ev))
                        >
                            <option value="text-embedding-3-small">"text-embedding-3-small (OpenAI)"</option>
                            <option value="text-embedding-3-large">"text-embedding-3-large (OpenAI)"</option>
                            <option value="voyage-3">"voyage-3 (Voyage AI)"</option>
                            <option value="nomic-embed-text">"nomic-embed-text (Local)"</option>
                        </select>
                    </div>

                    <div class="onboarding-actions">
                        <button class="btn btn-outline" on:click=move |_| set_step.set(4)>{t("btn-back")}</button>
                        <button
                            class="btn btn-primary onboarding-btn-primary"
                            on:click=save_and_continue
                            disabled=move || saving.get()
                        >
                            {move || if saving.get() { "Saving..." } else { "Save & Continue" }}
                        </button>
                    </div>
                </div>
            })}

            // ── Step 6: First Task ──
            {move || (step.get() == 6).then(|| view! {
                <div class="onboarding-step">
                    <h2 class="onboarding-step-title">{t("onboarding-first-task")}</h2>
                    <p class="onboarding-step-desc">"Optionally create a task to get started right away."</p>

                    <div class="onboarding-form-section">
                        <label class="onboarding-form-label">"Task Title"</label>
                        <input
                            type="text"
                            class="form-input"
                            placeholder="e.g. Set up CI/CD pipeline"
                            prop:value=move || task_title.get()
                            on:input=move |ev| set_task_title.set(event_target_value(&ev))
                        />
                    </div>

                    <div class="onboarding-form-section">
                        <label class="onboarding-form-label">"Description (optional)"</label>
                        <textarea
                            class="form-textarea"
                            rows="4"
                            placeholder="Describe what this task should accomplish..."
                            prop:value=move || task_desc.get()
                            on:input=move |ev| set_task_desc.set(event_target_value(&ev))
                        ></textarea>
                    </div>

                    <div class="onboarding-actions">
                        <button class="btn btn-outline" on:click=move |_| set_step.set(5)>{t("btn-back")}</button>
                        <button
                            class="btn btn-outline"
                            on:click=move |_| set_step.set(7)
                        >
                            {t("onboarding-skip")}
                        </button>
                        <button
                            class="btn btn-primary onboarding-btn-primary"
                            on:click=on_create_task
                            disabled=move || task_submitting.get()
                        >
                            {move || if task_submitting.get() { "Creating..." } else { "Create Task" }}
                        </button>
                    </div>
                </div>
            })}

            // ── Step 7: Complete ──
            {move || (step.get() == 7).then(|| view! {
                <div class="onboarding-step">
                    <div class="onboarding-logo">
                        <div class="onboarding-logo-icon complete">"[ok]"</div>
                    </div>
                    <h2 class="onboarding-step-title">{t("onboarding-complete")}</h2>
                    <p class="onboarding-step-desc">"Auto-Tundra is configured and ready to go."</p>

                    <div class="onboarding-summary">
                        <div class="onboarding-summary-row">
                            <span class="onboarding-summary-label">"Auth Method"</span>
                            <span class="onboarding-summary-value">{move || match auth_method.get().as_str() {
                                "env" => "Environment Variables",
                                "config" => "Config File",
                                "oauth" => "OAuth",
                                _ => "Unknown",
                            }}</span>
                        </div>
                        <div class="onboarding-summary-row">
                            <span class="onboarding-summary-label">"IDE"</span>
                            <span class="onboarding-summary-value">{move || ide.get()}</span>
                        </div>
                        <div class="onboarding-summary-row">
                            <span class="onboarding-summary-label">"Provider"</span>
                            <span class="onboarding-summary-value">{move || provider.get()}</span>
                        </div>
                        <div class="onboarding-summary-row">
                            <span class="onboarding-summary-label">"Model"</span>
                            <span class="onboarding-summary-value">{move || model.get()}</span>
                        </div>
                        <div class="onboarding-summary-row">
                            <span class="onboarding-summary-label">"Thinking"</span>
                            <span class="onboarding-summary-value">{move || thinking.get()}</span>
                        </div>
                        <div class="onboarding-summary-row">
                            <span class="onboarding-summary-label">"Max Agents"</span>
                            <span class="onboarding-summary-value">{move || max_agents.get().to_string()}</span>
                        </div>
                        <div class="onboarding-summary-row">
                            <span class="onboarding-summary-label">"Memory"</span>
                            <span class="onboarding-summary-value">{move || if enable_memory.get() { "Enabled" } else { "Disabled" }}</span>
                        </div>
                    </div>

                    <div class="onboarding-actions">
                        <button
                            class="btn btn-primary btn-lg onboarding-btn-primary"
                            on:click=go_to_dashboard
                        >
                            "Open Dashboard"
                        </button>
                    </div>
                </div>
            })}
        </div>
    }
}
