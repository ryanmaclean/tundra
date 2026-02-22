use leptos::prelude::*;
use crate::state::{use_app_state, DisplayMode};
use crate::i18n::Locale;
use crate::api::{
    self, ApiSettings, ApiGeneralSettings, ApiDisplaySettings, ApiAgentsSettings,
    ApiTerminalSettings, ApiSecuritySettings, ApiIntegrationSettings,
    ApiAppearanceSettings, ApiLanguageSettings, ApiDevToolsSettings,
    ApiAgentProfileSettings, ApiPhaseConfig, ApiPathsSettings,
    ApiApiProfilesSettings, ApiUpdatesSettings,
    ApiNotificationSettings, ApiDebugSettings, ApiMemorySettings,
};

/// Helper to populate all signal setters from an `ApiSettings` struct.
fn apply_settings_to_signals(
    s: &ApiSettings,
    // Appearance
    set_appearance_mode: WriteSignal<String>,
    set_color_theme: WriteSignal<String>,
    // Display
    set_scale_preset: WriteSignal<String>,
    set_fine_scale: WriteSignal<u32>,
    // Language
    set_interface_language: WriteSignal<String>,
    // Dev Tools
    set_preferred_ide: WriteSignal<String>,
    set_preferred_terminal: WriteSignal<String>,
    set_auto_name_terminals: WriteSignal<bool>,
    set_yolo_mode: WriteSignal<bool>,
    // Agent Settings
    set_default_profile: WriteSignal<String>,
    set_agent_framework: WriteSignal<String>,
    set_ai_terminal_naming: WriteSignal<bool>,
    // Paths
    set_python_path: WriteSignal<String>,
    set_git_path: WriteSignal<String>,
    set_github_cli_path: WriteSignal<String>,
    set_claude_cli_path: WriteSignal<String>,
    // Integrations
    set_github_token_env: WriteSignal<String>,
    set_github_owner: WriteSignal<String>,
    set_github_repo: WriteSignal<String>,
    set_gitlab_token_env: WriteSignal<String>,
    set_linear_api_key_env: WriteSignal<String>,
    set_linear_team_id: WriteSignal<String>,
    set_openai_api_key_env: WriteSignal<String>,
    // Updates
    set_version: WriteSignal<String>,
    set_auto_update_projects: WriteSignal<bool>,
    set_beta_updates: WriteSignal<bool>,
    // Notifications
    set_on_task_complete: WriteSignal<bool>,
    set_on_task_failed: WriteSignal<bool>,
    set_on_review_needed: WriteSignal<bool>,
    set_sound_enabled: WriteSignal<bool>,
    // Debug
    set_anonymous_reporting: WriteSignal<bool>,
    // Memory
    set_enable_memory: WriteSignal<bool>,
    set_enable_agent_memory: WriteSignal<bool>,
    set_graphiti_url: WriteSignal<String>,
    set_embedding_provider: WriteSignal<String>,
    set_embedding_model: WriteSignal<String>,
) {
    // Appearance
    let mode = if s.appearance.appearance_mode.is_empty() { "Dark".to_string() } else { s.appearance.appearance_mode.clone() };
    set_appearance_mode.set(mode);
    let theme = if s.appearance.color_theme.is_empty() { "Neo".to_string() } else { s.appearance.color_theme.clone() };
    set_color_theme.set(theme);
    // Display
    set_scale_preset.set("100".to_string());
    set_fine_scale.set(if s.display.font_size > 0 { (s.display.font_size as u32) * 100 / 14 } else { 100 });
    // Language
    let lang = if s.language.interface_language.is_empty() { "en".to_string() } else { s.language.interface_language.clone() };
    set_interface_language.set(lang);
    // Dev Tools
    let ide = if s.dev_tools.preferred_ide.is_empty() { "vscode".to_string() } else { s.dev_tools.preferred_ide.clone() };
    set_preferred_ide.set(ide);
    let term = if s.dev_tools.preferred_terminal.is_empty() { "system".to_string() } else { s.dev_tools.preferred_terminal.clone() };
    set_preferred_terminal.set(term);
    set_auto_name_terminals.set(s.dev_tools.auto_name_terminals);
    set_yolo_mode.set(s.dev_tools.yolo_mode);
    // Agent Settings
    let prof = if s.agent_profile.default_profile.is_empty() { "auto".to_string() } else { s.agent_profile.default_profile.clone() };
    set_default_profile.set(prof);
    let fw = if s.agent_profile.agent_framework.is_empty() { "Auto Claude".to_string() } else { s.agent_profile.agent_framework.clone() };
    set_agent_framework.set(fw);
    set_ai_terminal_naming.set(s.agent_profile.ai_terminal_naming);
    // Paths
    set_python_path.set(s.paths.python_path.clone());
    set_git_path.set(s.paths.git_path.clone());
    set_github_cli_path.set(s.paths.github_cli_path.clone());
    set_claude_cli_path.set(s.paths.claude_cli_path.clone());
    // Integrations
    set_github_token_env.set(s.integrations.github_token_env.clone());
    set_github_owner.set(s.integrations.github_owner.clone().unwrap_or_default());
    set_github_repo.set(s.integrations.github_repo.clone().unwrap_or_default());
    set_gitlab_token_env.set(s.integrations.gitlab_token_env.clone());
    set_linear_api_key_env.set(s.integrations.linear_api_key_env.clone());
    set_linear_team_id.set(s.integrations.linear_team_id.clone().unwrap_or_default());
    set_openai_api_key_env.set(s.integrations.openai_api_key_env.clone());
    // Updates
    let ver = if s.updates.version.is_empty() { "0.1.0".to_string() } else { s.updates.version.clone() };
    set_version.set(ver);
    set_auto_update_projects.set(s.updates.auto_update_projects);
    set_beta_updates.set(s.updates.beta_updates);
    // Notifications
    set_on_task_complete.set(s.notifications.on_task_complete);
    set_on_task_failed.set(s.notifications.on_task_failed);
    set_on_review_needed.set(s.notifications.on_review_needed);
    set_sound_enabled.set(s.notifications.sound_enabled);
    // Debug
    set_anonymous_reporting.set(s.debug.anonymous_error_reporting);
    // Memory
    set_enable_memory.set(s.memory.enable_memory);
    set_enable_agent_memory.set(s.memory.enable_agent_memory_access);
    let gurl = if s.memory.graphiti_server_url.is_empty() { "http://localhost:8000/api".to_string() } else { s.memory.graphiti_server_url.clone() };
    set_graphiti_url.set(gurl);
    let ep = if s.memory.embedding_provider.is_empty() { "ollama".to_string() } else { s.memory.embedding_provider.clone() };
    set_embedding_provider.set(ep);
    set_embedding_model.set(s.memory.embedding_model.clone());
}

fn build_settings_from_signals(
    appearance_mode: &ReadSignal<String>,
    color_theme: &ReadSignal<String>,
    fine_scale: &ReadSignal<u32>,
    interface_language: &ReadSignal<String>,
    preferred_ide: &ReadSignal<String>,
    preferred_terminal: &ReadSignal<String>,
    auto_name_terminals: &ReadSignal<bool>,
    yolo_mode: &ReadSignal<bool>,
    default_profile: &ReadSignal<String>,
    agent_framework: &ReadSignal<String>,
    ai_terminal_naming: &ReadSignal<bool>,
    python_path: &ReadSignal<String>,
    git_path: &ReadSignal<String>,
    github_cli_path: &ReadSignal<String>,
    claude_cli_path: &ReadSignal<String>,
    github_token_env: &ReadSignal<String>,
    github_owner: &ReadSignal<String>,
    github_repo: &ReadSignal<String>,
    gitlab_token_env: &ReadSignal<String>,
    linear_api_key_env: &ReadSignal<String>,
    linear_team_id: &ReadSignal<String>,
    openai_api_key_env: &ReadSignal<String>,
    auto_update_projects: &ReadSignal<bool>,
    beta_updates: &ReadSignal<bool>,
    on_task_complete: &ReadSignal<bool>,
    on_task_failed: &ReadSignal<bool>,
    on_review_needed: &ReadSignal<bool>,
    sound_enabled: &ReadSignal<bool>,
    anonymous_reporting: &ReadSignal<bool>,
    enable_memory: &ReadSignal<bool>,
    enable_agent_memory: &ReadSignal<bool>,
    graphiti_url: &ReadSignal<String>,
    embedding_provider: &ReadSignal<String>,
    embedding_model: &ReadSignal<String>,
    // Phase config signals
    spec_model: &ReadSignal<String>, spec_thinking: &ReadSignal<String>,
    ideation_model: &ReadSignal<String>, ideation_thinking: &ReadSignal<String>,
    roadmap_model: &ReadSignal<String>, roadmap_thinking: &ReadSignal<String>,
    gh_issues_model: &ReadSignal<String>, gh_issues_thinking: &ReadSignal<String>,
    gh_pr_model: &ReadSignal<String>, gh_pr_thinking: &ReadSignal<String>,
    utility_model: &ReadSignal<String>, utility_thinking: &ReadSignal<String>,
) -> ApiSettings {
    let gh_owner = github_owner.get();
    let gh_repo = github_repo.get();
    let scale = fine_scale.get();
    let font_sz = ((scale as f64 / 100.0) * 14.0).round() as u8;

    ApiSettings {
        general: ApiGeneralSettings {
            project_name: "auto-tundra".to_string(),
            log_level: "info".to_string(),
            workspace_root: None,
        },
        display: ApiDisplaySettings {
            theme: appearance_mode.get(),
            font_size: font_sz,
            compact_mode: false,
        },
        agents: ApiAgentsSettings {
            max_concurrent: 4,
            heartbeat_interval_secs: 30,
            auto_restart: false,
        },
        terminal: ApiTerminalSettings {
            font_family: "JetBrains Mono".to_string(),
            font_size: 14,
            cursor_style: "block".to_string(),
        },
        security: ApiSecuritySettings {
            allow_shell_exec: false,
            sandbox: true,
            allowed_paths: Vec::new(),
            auto_lock_timeout_mins: 15,
            sandbox_mode: true,
        },
        integrations: ApiIntegrationSettings {
            github_token_env: github_token_env.get(),
            github_owner: if gh_owner.is_empty() { None } else { Some(gh_owner) },
            github_repo: if gh_repo.is_empty() { None } else { Some(gh_repo) },
            gitlab_token_env: gitlab_token_env.get(),
            linear_api_key_env: linear_api_key_env.get(),
            linear_team_id: { let t = linear_team_id.get(); if t.is_empty() { None } else { Some(t) } },
            openai_api_key_env: openai_api_key_env.get(),
        },
        appearance: ApiAppearanceSettings {
            appearance_mode: appearance_mode.get(),
            color_theme: color_theme.get(),
        },
        language: ApiLanguageSettings {
            interface_language: interface_language.get(),
        },
        dev_tools: ApiDevToolsSettings {
            preferred_ide: preferred_ide.get(),
            preferred_terminal: preferred_terminal.get(),
            auto_name_terminals: auto_name_terminals.get(),
            yolo_mode: yolo_mode.get(),
        },
        agent_profile: ApiAgentProfileSettings {
            default_profile: default_profile.get(),
            agent_framework: agent_framework.get(),
            ai_terminal_naming: ai_terminal_naming.get(),
            phase_configs: vec![
                ApiPhaseConfig { phase: "Spec Creation".to_string(), model: spec_model.get(), thinking_level: spec_thinking.get() },
                ApiPhaseConfig { phase: "Ideation".to_string(), model: ideation_model.get(), thinking_level: ideation_thinking.get() },
                ApiPhaseConfig { phase: "Roadmap".to_string(), model: roadmap_model.get(), thinking_level: roadmap_thinking.get() },
                ApiPhaseConfig { phase: "GitHub Issues".to_string(), model: gh_issues_model.get(), thinking_level: gh_issues_thinking.get() },
                ApiPhaseConfig { phase: "GitHub PR Review".to_string(), model: gh_pr_model.get(), thinking_level: gh_pr_thinking.get() },
                ApiPhaseConfig { phase: "Utility".to_string(), model: utility_model.get(), thinking_level: utility_thinking.get() },
            ],
        },
        paths: ApiPathsSettings {
            python_path: python_path.get(),
            git_path: git_path.get(),
            github_cli_path: github_cli_path.get(),
            claude_cli_path: claude_cli_path.get(),
            auto_claude_path: String::new(),
        },
        api_profiles: ApiApiProfilesSettings { profiles: Vec::new() },
        updates: ApiUpdatesSettings {
            version: "0.1.0".to_string(),
            is_latest: true,
            auto_update_projects: auto_update_projects.get(),
            beta_updates: beta_updates.get(),
        },
        notifications: ApiNotificationSettings {
            on_task_complete: on_task_complete.get(),
            on_task_failed: on_task_failed.get(),
            on_review_needed: on_review_needed.get(),
            sound_enabled: sound_enabled.get(),
        },
        debug: ApiDebugSettings {
            anonymous_error_reporting: anonymous_reporting.get(),
        },
        memory: ApiMemorySettings {
            enable_memory: enable_memory.get(),
            enable_agent_memory_access: enable_agent_memory.get(),
            graphiti_server_url: graphiti_url.get(),
            embedding_provider: embedding_provider.get(),
            embedding_model: embedding_model.get(),
        },
    }
}

#[component]
pub fn ConfigPage(
    #[prop(optional)] on_close: Option<Callback<()>>,
) -> impl IntoView {
    // App-level display mode state
    let app_state = use_app_state();

    // Active settings tab
    let (active_tab, set_active_tab) = signal(0usize);

    // Toast state
    let (show_toast, set_show_toast) = signal(false);
    let (toast_msg, set_toast_msg) = signal(String::new());

    // -- Appearance Tab signals --
    let (appearance_mode, set_appearance_mode) = signal("Dark".to_string());
    let (color_theme, set_color_theme) = signal("Neo".to_string());

    // -- Display Tab signals --
    let (scale_preset, set_scale_preset) = signal("100".to_string());
    let (fine_scale, set_fine_scale) = signal(100u32);

    // -- Language Tab signals --
    let (interface_language, set_interface_language) = signal("en".to_string());

    // -- Developer Tools Tab signals --
    let (preferred_ide, set_preferred_ide) = signal("vscode".to_string());
    let (preferred_terminal, set_preferred_terminal) = signal("system".to_string());
    let (auto_name_terminals, set_auto_name_terminals) = signal(true);
    let (yolo_mode, set_yolo_mode) = signal(false);

    // -- Agent Settings Tab signals --
    let (default_profile, set_default_profile) = signal("auto".to_string());
    let (agent_framework, set_agent_framework) = signal("Auto Claude".to_string());
    let (ai_terminal_naming, set_ai_terminal_naming) = signal(true);
    // Phase config signals
    let (spec_model, set_spec_model) = signal("Claude Sonnet 4.5".to_string());
    let (spec_thinking, set_spec_thinking) = signal("Medium".to_string());
    let (ideation_model, set_ideation_model) = signal("Claude Opus 4.5".to_string());
    let (ideation_thinking, set_ideation_thinking) = signal("High".to_string());
    let (roadmap_model, set_roadmap_model) = signal("Claude Opus 4.5".to_string());
    let (roadmap_thinking, set_roadmap_thinking) = signal("High".to_string());
    let (gh_issues_model, set_gh_issues_model) = signal("Claude Opus 4.5".to_string());
    let (gh_issues_thinking, set_gh_issues_thinking) = signal("Medium".to_string());
    let (gh_pr_model, set_gh_pr_model) = signal("Claude Opus 4.5".to_string());
    let (gh_pr_thinking, set_gh_pr_thinking) = signal("Medium".to_string());
    let (utility_model, set_utility_model) = signal("Claude Haiku 4.5".to_string());
    let (utility_thinking, set_utility_thinking) = signal("Low".to_string());

    // -- Paths Tab signals --
    let (python_path, set_python_path) = signal(String::new());
    let (git_path, set_git_path) = signal(String::new());
    let (github_cli_path, set_github_cli_path) = signal(String::new());
    let (claude_cli_path, set_claude_cli_path) = signal(String::new());

    // -- Integrations Tab signals --
    let (github_token_env, set_github_token_env) = signal("GITHUB_TOKEN".to_string());
    let (github_owner, set_github_owner) = signal(String::new());
    let (github_repo, set_github_repo) = signal(String::new());
    let (gitlab_token_env, set_gitlab_token_env) = signal("GITLAB_TOKEN".to_string());
    let (linear_api_key_env, set_linear_api_key_env) = signal("LINEAR_API_KEY".to_string());
    let (linear_team_id, set_linear_team_id) = signal(String::new());
    let (openai_api_key_env, set_openai_api_key_env) = signal(String::new());

    // -- API Profiles Tab signals --
    // (empty state for now)

    // -- Updates Tab signals --
    let (version, set_version) = signal("0.1.0".to_string());
    let (auto_update_projects, set_auto_update_projects) = signal(true);
    let (beta_updates, set_beta_updates) = signal(true);

    // -- Notifications Tab signals --
    let (on_task_complete, set_on_task_complete) = signal(true);
    let (on_task_failed, set_on_task_failed) = signal(true);
    let (on_review_needed, set_on_review_needed) = signal(true);
    let (sound_enabled, set_sound_enabled) = signal(true);

    // -- Debug & Logs Tab signals --
    let (anonymous_reporting, set_anonymous_reporting) = signal(true);

    // -- Memory Tab signals --
    let (enable_memory, set_enable_memory) = signal(false);
    let (enable_agent_memory, set_enable_agent_memory) = signal(false);
    let (graphiti_url, set_graphiti_url) = signal("http://localhost:8000/api".to_string());
    let (embedding_provider, set_embedding_provider) = signal("ollama".to_string());
    let (embedding_model, set_embedding_model) = signal(String::new());

    // -- Apply color theme CSS variables reactively --
    Effect::new(move |_| {
        let theme = color_theme.get();
        let (accent_purple, accent_magenta) = match theme.as_str() {
            "Default" => ("#eab308", "#c026d3"),
            "Dusk" => ("#f97316", "#e879f9"),
            "Lime" => ("#84cc16", "#c026d3"),
            "Ocean" => ("#3b82f6", "#06b6d4"),
            "Retro" => ("#f97316", "#eab308"),
            "Neo" => ("#c026d3", "#ec4899"),
            "Forest" => ("#22c55e", "#16a34a"),
            _ => ("#c026d3", "#ec4899"), // default to Neo
        };
        if let Some(window) = web_sys::window() {
            if let Some(document) = window.document() {
                if let Some(body) = document.body() {
                    let style = body.style();
                    let _ = style.set_property("--accent-purple", accent_purple);
                    let _ = style.set_property("--accent-magenta", accent_magenta);
                }
            }
        }
    });

    // -- Detected tools (loaded from API) --
    let (detected_tools, set_detected_tools) = signal(Vec::<String>::new());

    // -- Credential status (loaded from API) --
    let (cred_providers, set_cred_providers) = signal(Vec::<String>::new());

    // -- Account management --
    let (extra_accounts, set_extra_accounts) = signal(Vec::<String>::new());
    let (new_account_name, set_new_account_name) = signal(String::new());

    // -- API Profile creation --
    let (show_profile_form, set_show_profile_form) = signal(false);
    let (new_profile_name, set_new_profile_name) = signal(String::new());
    let (new_profile_url, set_new_profile_url) = signal(String::new());
    let (new_profile_key_env, set_new_profile_key_env) = signal(String::new());
    let (local_provider_url, set_local_provider_url) = signal("http://127.0.0.1:11434".to_string());
    let (local_provider_model, set_local_provider_model) = signal("qwen2.5-coder:14b".to_string());
    let (local_provider_key_env, set_local_provider_key_env) = signal("LOCAL_API_KEY".to_string());
    let (local_probe_loading, set_local_probe_loading) = signal(false);
    let (local_probe_status, set_local_probe_status) = signal(Option::<(bool, String)>::None);
    let (local_probe_models, set_local_probe_models) = signal(Vec::<String>::new());

    // -- Updates check --
    let (update_status_msg, set_update_status_msg) = signal(Option::<String>::None);

    // -- Debug info --
    let (debug_textarea_visible, set_debug_textarea_visible) = signal(false);
    let (debug_paste_content, set_debug_paste_content) = signal(String::new());

    // -- Load settings from API on mount --
    leptos::task::spawn_local(async move {
        match api::fetch_settings().await {
            Ok(s) => {
                apply_settings_to_signals(
                    &s,
                    set_appearance_mode, set_color_theme,
                    set_scale_preset, set_fine_scale,
                    set_interface_language,
                    set_preferred_ide, set_preferred_terminal, set_auto_name_terminals, set_yolo_mode,
                    set_default_profile, set_agent_framework, set_ai_terminal_naming,
                    set_python_path, set_git_path, set_github_cli_path, set_claude_cli_path,
                    set_github_token_env, set_github_owner, set_github_repo,
                    set_gitlab_token_env, set_linear_api_key_env, set_linear_team_id, set_openai_api_key_env,
                    set_version, set_auto_update_projects, set_beta_updates,
                    set_on_task_complete, set_on_task_failed, set_on_review_needed, set_sound_enabled,
                    set_anonymous_reporting,
                    set_enable_memory, set_enable_agent_memory, set_graphiti_url,
                    set_embedding_provider, set_embedding_model,
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

        match api::fetch_local_provider_settings().await {
            Ok(local) => {
                set_local_provider_url.set(local.base_url);
                set_local_provider_model.set(local.model);
                set_local_provider_key_env.set(local.api_key_env);
            }
            Err(e) => {
                web_sys::console::warn_1(&format!("Failed to load local provider settings: {e}").into());
            }
        }
    });

    let _show_toast_fn = move |msg: &str| {
        set_toast_msg.set(msg.to_string());
        set_show_toast.set(true);
        let set_show = set_show_toast;
        leptos::task::spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(3_000).await;
            set_show.set(false);
        });
    };

    let on_save = move |_| {
        let settings = build_settings_from_signals(
            &appearance_mode, &color_theme, &fine_scale,
            &interface_language,
            &preferred_ide, &preferred_terminal, &auto_name_terminals, &yolo_mode,
            &default_profile, &agent_framework, &ai_terminal_naming,
            &python_path, &git_path, &github_cli_path, &claude_cli_path,
            &github_token_env, &github_owner, &github_repo,
            &gitlab_token_env, &linear_api_key_env, &linear_team_id,
            &openai_api_key_env,
            &auto_update_projects, &beta_updates,
            &on_task_complete, &on_task_failed, &on_review_needed, &sound_enabled,
            &anonymous_reporting,
            &enable_memory, &enable_agent_memory, &graphiti_url,
            &embedding_provider, &embedding_model,
            &spec_model, &spec_thinking,
            &ideation_model, &ideation_thinking,
            &roadmap_model, &roadmap_thinking,
            &gh_issues_model, &gh_issues_thinking,
            &gh_pr_model, &gh_pr_thinking,
            &utility_model, &utility_thinking,
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

    let _on_reset = move |_: web_sys::MouseEvent| {
        // Reset to defaults
        set_appearance_mode.set("Dark".to_string());
        set_color_theme.set("Neo".to_string());
        set_scale_preset.set("100".to_string());
        set_fine_scale.set(100);
        set_interface_language.set("en".to_string());
        set_preferred_ide.set("vscode".to_string());
        set_preferred_terminal.set("system".to_string());
        set_auto_name_terminals.set(true);
        set_yolo_mode.set(false);
        set_default_profile.set("auto".to_string());
        set_agent_framework.set("Auto Claude".to_string());
        set_ai_terminal_naming.set(true);
        set_spec_model.set("Claude Sonnet 4.5".to_string());
        set_spec_thinking.set("Medium".to_string());
        set_ideation_model.set("Claude Opus 4.5".to_string());
        set_ideation_thinking.set("High".to_string());
        set_roadmap_model.set("Claude Opus 4.5".to_string());
        set_roadmap_thinking.set("High".to_string());
        set_gh_issues_model.set("Claude Opus 4.5".to_string());
        set_gh_issues_thinking.set("Medium".to_string());
        set_gh_pr_model.set("Claude Opus 4.5".to_string());
        set_gh_pr_thinking.set("Medium".to_string());
        set_utility_model.set("Claude Haiku 4.5".to_string());
        set_utility_thinking.set("Low".to_string());
        set_python_path.set(String::new());
        set_git_path.set(String::new());
        set_github_cli_path.set(String::new());
        set_claude_cli_path.set(String::new());
        set_github_token_env.set("GITHUB_TOKEN".to_string());
        set_github_owner.set(String::new());
        set_github_repo.set(String::new());
        set_gitlab_token_env.set("GITLAB_TOKEN".to_string());
        set_linear_api_key_env.set("LINEAR_API_KEY".to_string());
        set_linear_team_id.set(String::new());
        set_openai_api_key_env.set(String::new());
        set_auto_update_projects.set(true);
        set_beta_updates.set(true);
        set_on_task_complete.set(true);
        set_on_task_failed.set(true);
        set_on_review_needed.set(true);
        set_sound_enabled.set(true);
        set_anonymous_reporting.set(true);
        set_enable_memory.set(false);
        set_enable_agent_memory.set(false);
        set_graphiti_url.set("http://localhost:8000/api".to_string());
        set_embedding_provider.set("ollama".to_string());
        set_embedding_model.set(String::new());
        set_local_provider_url.set("http://127.0.0.1:11434".to_string());
        set_local_provider_model.set("qwen2.5-coder:14b".to_string());
        set_local_provider_key_env.set("LOCAL_API_KEY".to_string());
        set_local_probe_status.set(None);
        set_local_probe_models.set(Vec::new());

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

    // Sidebar items: (label, hint, is_section_header, is_button)
    let sidebar_items: Vec<(&str, &str, bool, bool)> = vec![
        ("Appearance", "Customize how Auto Tundra looks", false, false),
        ("Display", "Adjust the size of UI elements", false, false),
        ("Language", "Choose your preferred language", false, false),
        ("Developer Tools", "IDE and terminal preferences", false, false),
        ("Agent Settings", "Default model and framework", false, false),
        ("Paths", "CLI tools and framework paths", false, false),
        ("Integrations", "API keys & Claude accounts", false, false),
        ("API Profiles", "Custom API endpoint profiles", false, false),
        ("Updates", "Auto Tundra updates", false, false),
        ("Notifications", "Alert preferences", false, false),
        ("Debug & Logs", "Troubleshooting tools", false, false),
        ("Memory", "Agent memory configuration", false, false),
        ("Re-run Wizard", "Start the setup wizard again", false, true),
    ];

    let close_modal = move |_| {
        if let Some(cb) = on_close {
            cb.run(());
        }
    };

    view! {
        <div class="settings-modal-overlay" on:click=close_modal.clone()>
            <div class="settings-modal" on:click=move |ev: web_sys::MouseEvent| ev.stop_propagation()>
                // Modal header
                <div class="settings-modal-header">
                    <div class="settings-modal-header-left">
                        <span class="settings-modal-gear-icon" inner_html=r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 010 2.83 2 2 0 01-2.83 0l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 01-4 0v-.09A1.65 1.65 0 009 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 01-2.83-2.83l.06-.06A1.65 1.65 0 004.68 15a1.65 1.65 0 00-1.51-1H3a2 2 0 010-4h.09A1.65 1.65 0 004.6 9a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 012.83-2.83l.06.06A1.65 1.65 0 009 4.68a1.65 1.65 0 001-1.51V3a2 2 0 014 0v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 012.83 2.83l-.06.06A1.65 1.65 0 0019.32 9a1.65 1.65 0 001.51 1H21a2 2 0 010 4h-.09a1.65 1.65 0 00-1.51 1z"/></svg>"#></span>
                        <div>
                            <div class="settings-modal-title">"Settings"</div>
                            <div class="settings-modal-subtitle">"App Settings & Project Settings"</div>
                        </div>
                    </div>
                    <button class="settings-modal-close" on:click=close_modal.clone() aria-label="Close settings">
                        <span inner_html=r#"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6L6 18"/><path d="M6 6l12 12"/></svg>"#></span>
                    </button>
                </div>

                // Modal body: sidebar + content
                <div class="settings-modal-body">
                    <div class="settings-tabbed-layout">
                        // Left sidebar tabs
                        <div class="settings-tab-sidebar">
                            <div class="settings-sidebar-section-header">"APP SETTINGS"</div>
                {sidebar_items.into_iter().enumerate().map(|(i, (label, hint, _is_header, is_button))| {
                    if is_button {
                        view! {
                            <button
                                class="settings-tab-btn settings-tab-btn-action"
                                on:click=move |_| {
                                    // Navigate to onboarding page
                                    if let Some(window) = web_sys::window() {
                                        let _ = window.location().set_hash("#onboarding");
                                    }
                                }
                            >
                                <div class="settings-tab-btn-content">
                                    <span class="settings-tab-label">{label}</span>
                                    <span class="settings-tab-hint">{hint}</span>
                                </div>
                            </button>
                        }.into_any()
                    } else {
                        view! {
                            <button
                                class="settings-tab-btn"
                                class:active=move || active_tab.get() == i
                                on:click=move |_| set_active_tab.set(i)
                            >
                                <div class="settings-tab-btn-content">
                                    <span class="settings-tab-label">{label}</span>
                                    <span class="settings-tab-hint">{hint}</span>
                                </div>
                            </button>
                        }.into_any()
                    }
                }).collect::<Vec<_>>()}
                <div class="settings-sidebar-section-header">"PROJECT SETTINGS"</div>
            </div>

            // Right content area
            <div class="settings-tab-content">
                {move || match active_tab.get() {
                    // ── 0: Appearance ──
                    0 => view! {
                        <div class="settings-panel">
                            <h3 class="settings-panel-title">"Appearance"</h3>
                            <p class="settings-panel-subtitle">"Customize how Auto Tundra looks"</p>

                            <div class="settings-section">
                                <h4 class="settings-section-title">"Appearance Mode"</h4>
                                <p class="settings-hint">"Choose light, dark, or system preference"</p>
                                <div class="settings-card-grid settings-card-grid-3">
                                    <button
                                        class="settings-card-option"
                                        class:selected=move || appearance_mode.get() == "System"
                                        on:click=move |_| set_appearance_mode.set("System".to_string())
                                    >
                                        <span class="settings-card-icon">"\u{1F4BB}"</span>
                                        <span class="settings-card-label">"System"</span>
                                    </button>
                                    <button
                                        class="settings-card-option"
                                        class:selected=move || appearance_mode.get() == "Light"
                                        on:click=move |_| set_appearance_mode.set("Light".to_string())
                                    >
                                        <span class="settings-card-icon">"\u{2600}"</span>
                                        <span class="settings-card-label">"Light"</span>
                                    </button>
                                    <button
                                        class="settings-card-option"
                                        class:selected=move || appearance_mode.get() == "Dark"
                                        on:click=move |_| set_appearance_mode.set("Dark".to_string())
                                    >
                                        <span class="settings-card-icon">"\u{263D}"</span>
                                        <span class="settings-card-label">"Dark"</span>
                                    </button>
                                </div>
                            </div>

                            <div class="settings-section">
                                <h4 class="settings-section-title">"Color Theme"</h4>
                                <p class="settings-hint">"Select a color palette for the interface"</p>
                                <div class="settings-card-grid settings-card-grid-3">
                                    {[
                                        ("Default", "#e8b931", "Obscura-inspired with pale yellow accent"),
                                        ("Dusk", "#c77dba", "Warmer variant with slightly lighter dark mode"),
                                        ("Lime", "#a3e635", "Fresh, energetic lime with purple accents"),
                                        ("Ocean", "#3b82f6", "Calm, professional blue tones"),
                                        ("Retro", "#f59e0b", "Warm, nostalgic amber vibes"),
                                        ("Neo", "#e879f9", "Modern cyberpunk pink/magenta"),
                                        ("Forest", "#22c55e", "Natural, earthy green tones"),
                                    ].into_iter().map(|(name, color, desc)| {
                                        let name_str = name.to_string();
                                        let name_cmp = name.to_string();
                                        view! {
                                            <button
                                                class="settings-card-option settings-theme-card"
                                                class:selected=move || color_theme.get() == name_cmp
                                                on:click=move |_| set_color_theme.set(name_str.clone())
                                            >
                                                <span class="settings-theme-dot" style=format!("background-color: {color}")></span>
                                                <span class="settings-card-label">{name}</span>
                                                <span class="settings-card-desc">{desc}</span>
                                            </button>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                            </div>

                            <div class="settings-section">
                                <h4 class="settings-section-title">"Display Mode"</h4>
                                <p class="settings-hint">"Choose a visual theme for the entire interface"</p>
                                <div class="settings-theme-grid">
                                    <div
                                        class=(move || if app_state.display_mode.get() == DisplayMode::Standard { "settings-card-option settings-theme-card selected" } else { "settings-card-option settings-theme-card" })
                                        on:click=move |_| app_state.set_display_mode.set(DisplayMode::Standard)
                                    >
                                        <div class="settings-theme-preview standard-preview"></div>
                                        <span class="settings-card-label">"Standard"</span>
                                        <span class="settings-card-desc">"Default dark theme"</span>
                                    </div>
                                    <div
                                        class=(move || if app_state.display_mode.get() == DisplayMode::Foil { "settings-card-option settings-theme-card selected" } else { "settings-card-option settings-theme-card" })
                                        on:click=move |_| app_state.set_display_mode.set(DisplayMode::Foil)
                                    >
                                        <div class="settings-theme-preview foil-preview"></div>
                                        <span class="settings-card-label">"Foil"</span>
                                        <span class="settings-card-desc">"Holographic shimmer effects"</span>
                                    </div>
                                    <div
                                        class=(move || if app_state.display_mode.get() == DisplayMode::Vt100 { "settings-card-option settings-theme-card selected" } else { "settings-card-option settings-theme-card" })
                                        on:click=move |_| app_state.set_display_mode.set(DisplayMode::Vt100)
                                    >
                                        <div class="settings-theme-preview vt100-preview"></div>
                                        <span class="settings-card-label">"VT100"</span>
                                        <span class="settings-card-desc">"Retro terminal aesthetic"</span>
                                    </div>
                                </div>
                            </div>

                            <div class="settings-section">
                                <h4 class="settings-section-title">"Accessibility"</h4>
                                <div class="settings-item">
                                    <div>
                                        <label class="settings-label">"Reduce Motion"</label>
                                        <p class="settings-hint">"Disable all animations and transitions"</p>
                                    </div>
                                    <button
                                        class=(move || if app_state.reduce_motion.get() { "toggle-btn active" } else { "toggle-btn" })
                                        on:click=move |_| app_state.set_reduce_motion.set(!app_state.reduce_motion.get_untracked())
                                    >
                                        {move || if app_state.reduce_motion.get() { "ON" } else { "OFF" }}
                                    </button>
                                </div>
                            </div>
                        </div>
                    }.into_any(),

                    // ── 1: Display ──
                    1 => view! {
                        <div class="settings-panel">
                            <h3 class="settings-panel-title">"Display"</h3>
                            <p class="settings-panel-subtitle">"Adjust the size of UI elements"</p>

                            <div class="settings-section">
                                <h4 class="settings-section-title">"Scale Presets"</h4>
                                <p class="settings-hint">"Quick scale options for common preferences"</p>
                                <div class="settings-card-grid settings-card-grid-3">
                                    {[
                                        ("100", "Default"),
                                        ("125", "Comfortable"),
                                        ("150", "Large"),
                                    ].into_iter().map(|(val, label)| {
                                        let val_str = val.to_string();
                                        let val_cmp = val.to_string();
                                        let val_num: u32 = val.parse().unwrap_or(100);
                                        view! {
                                            <button
                                                class="settings-card-option"
                                                class:selected=move || scale_preset.get() == val_cmp
                                                on:click=move |_| {
                                                    set_scale_preset.set(val_str.clone());
                                                    set_fine_scale.set(val_num);
                                                }
                                            >
                                                <span class="settings-card-icon">"\u{1F4BB}"</span>
                                                <span class="settings-card-label">{format!("{val}%")}</span>
                                                <span class="settings-card-desc">{label}</span>
                                            </button>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                            </div>

                            <div class="settings-section">
                                <h4 class="settings-section-title">"Fine-tune Scale"</h4>
                                <p class="settings-hint">
                                    "Adjust from 75% to 200% in 5% increments"
                                    <span class="settings-scale-value">{move || format!("{}%", fine_scale.get())}</span>
                                </p>
                                <div class="settings-slider-row">
                                    <span class="settings-slider-label">"75%"</span>
                                    <input
                                        type="range"
                                        class="settings-slider"
                                        min="75"
                                        max="200"
                                        step="5"
                                        prop:value=move || fine_scale.get().to_string()
                                        on:input=move |ev| {
                                            if let Ok(v) = event_target_value(&ev).parse::<u32>() {
                                                set_fine_scale.set(v);
                                            }
                                        }
                                    />
                                    <span class="settings-slider-label">"200%"</span>
                                    <button class="btn-small" on:click=move |_| {
                                        let val = fine_scale.get();
                                        if let Some(window) = web_sys::window() {
                                            if let Some(document) = window.document() {
                                                if let Some(body) = document.body() {
                                                    let _ = body.style().set_property("--app-scale", &format!("{}%", val));
                                                    let font_px = 14.0 * (val as f64) / 100.0;
                                                    let _ = body.style().set_property("font-size", &format!("{:.1}px", font_px));
                                                }
                                            }
                                        }
                                    }>"Apply"</button>
                                </div>
                            </div>
                        </div>
                    }.into_any(),

                    // ── 2: Language ──
                    2 => view! {
                        <div class="settings-panel">
                            <h3 class="settings-panel-title">"Language"</h3>
                            <p class="settings-panel-subtitle">"Choose your preferred language"</p>

                            <div class="settings-section">
                                <h4 class="settings-section-title">"Interface Language"</h4>
                                <p class="settings-hint">"Select the language for the application interface"</p>
                                <div class="settings-card-grid settings-card-grid-2">
                                    <button
                                        class="settings-card-option settings-lang-card"
                                        class:selected=move || interface_language.get() == "en"
                                        on:click=move |_| {
                                            set_interface_language.set("en".to_string());
                                            let set_locale: WriteSignal<Locale> = use_context().expect("i18n set_locale not provided");
                                            set_locale.set(Locale::En);
                                        }
                                    >
                                        <span class="settings-card-icon">"\u{1F310}"</span>
                                        <div class="settings-lang-text">
                                            <span class="settings-card-label">"English"</span>
                                            <span class="settings-card-desc">"English"</span>
                                        </div>
                                    </button>
                                    <button
                                        class="settings-card-option settings-lang-card"
                                        class:selected=move || interface_language.get() == "fr"
                                        on:click=move |_| {
                                            set_interface_language.set("fr".to_string());
                                            let set_locale: WriteSignal<Locale> = use_context().expect("i18n set_locale not provided");
                                            set_locale.set(Locale::Fr);
                                        }
                                    >
                                        <span class="settings-card-icon">"\u{1F310}"</span>
                                        <div class="settings-lang-text">
                                            <span class="settings-card-label">"Fran\u{00E7}ais"</span>
                                            <span class="settings-card-desc">"French"</span>
                                        </div>
                                    </button>
                                </div>
                            </div>
                        </div>
                    }.into_any(),

                    // ── 3: Developer Tools ──
                    3 => view! {
                        <div class="settings-panel">
                            <h3 class="settings-panel-title">"Developer Tools"</h3>
                            <p class="settings-panel-subtitle">"Configure your preferred IDE and terminal for working with worktrees"</p>

                            <div class="settings-section">
                                <h4 class="settings-section-title">"Preferred IDE"</h4>
                                <div class="settings-control">
                                    <select
                                        class="settings-select settings-select-full"
                                        prop:value=move || preferred_ide.get()
                                        on:change=move |ev| set_preferred_ide.set(event_target_value(&ev))
                                    >
                                        <option value="vscode">"Visual Studio Code"</option>
                                        <option value="cursor">"Cursor"</option>
                                        <option value="windsurf">"Windsurf"</option>
                                        <option value="pycharm">"PyCharm"</option>
                                        <option value="vim">"Vim"</option>
                                        <option value="xcode">"Xcode"</option>
                                        <option value="vscodium">"VSCodium"</option>
                                    </select>
                                </div>
                                <p class="settings-hint">"Auto Tundra will open worktrees in this editor"</p>
                            </div>

                            <div class="settings-section">
                                <h4 class="settings-section-title">"Preferred Terminal"</h4>
                                <div class="settings-control">
                                    <select
                                        class="settings-select settings-select-full"
                                        prop:value=move || preferred_terminal.get()
                                        on:change=move |ev| set_preferred_terminal.set(event_target_value(&ev))
                                    >
                                        <option value="system">"System Terminal"</option>
                                        <option value="iterm2">"iTerm2"</option>
                                        <option value="warp">"Warp"</option>
                                        <option value="ghostty">"Ghostty"</option>
                                        <option value="alacritty">"Alacritty"</option>
                                        <option value="kitty">"Kitty"</option>
                                    </select>
                                </div>
                                <p class="settings-hint">"Auto Tundra will open terminal sessions here"</p>
                            </div>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"Auto-name Claude terminals"</span>
                                    <span class="settings-hint">"Use AI to generate a descriptive name for Claude terminals based on your first message"</span>
                                </div>
                                <div class="settings-control">
                                    <label class="toggle-switch">
                                        <input
                                            type="checkbox"
                                            prop:checked=move || auto_name_terminals.get()
                                            on:change=move |ev| set_auto_name_terminals.set(event_target_checked(&ev))
                                        />
                                        <span class="toggle-slider"></span>
                                    </label>
                                </div>
                            </div>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label settings-label-warning">"YOLO Mode"</span>
                                    <span class="settings-hint">"Start Claude with --dangerously-skip-permissions flag, bypassing all safety prompts. Use with extreme caution."</span>
                                </div>
                                <div class="settings-control">
                                    <label class="toggle-switch">
                                        <input
                                            type="checkbox"
                                            prop:checked=move || yolo_mode.get()
                                            on:change=move |ev| set_yolo_mode.set(event_target_checked(&ev))
                                        />
                                        <span class="toggle-slider"></span>
                                    </label>
                                </div>
                            </div>

                            {move || yolo_mode.get().then(|| view! {
                                <div class="settings-warning-banner">
                                    "This mode bypasses Claude's permission system. Only enable if you fully trust the code being executed."
                                </div>
                            })}

                            <div class="settings-section">
                                <h4 class="settings-section-title">"Detected Tools"</h4>
                                <ul class="settings-detected-list">
                                    {move || {
                                        let tools = detected_tools.get();
                                        if tools.is_empty() {
                                            vec!["VSCodium", "Cursor", "Windsurf", "PyCharm", "Vim", "GNU Nano", "Xcode", "Terminal.app", "iTerm2", "Ghostty"]
                                                .into_iter()
                                                .map(|t| view! { <li>{t}</li> }.into_any())
                                                .collect::<Vec<_>>()
                                        } else {
                                            tools.iter().cloned()
                                                .map(|t| view! { <li>{t}</li> }.into_any())
                                                .collect::<Vec<_>>()
                                        }
                                    }}
                                </ul>
                                <button class="btn-small" style="margin-top: 8px;" on:click=move |_| {
                                    leptos::task::spawn_local(async move {
                                        match api::fetch_cli_available().await {
                                            Ok(result) => {
                                                set_detected_tools.set(result.tools);
                                            }
                                            Err(e) => {
                                                web_sys::console::warn_1(&format!("Failed to detect tools: {e}").into());
                                            }
                                        }
                                    });
                                }>"Detect Again"</button>
                            </div>
                        </div>
                    }.into_any(),

                    // ── 4: Agent Settings ──
                    4 => view! {
                        <div class="settings-panel">
                            <h3 class="settings-panel-title">"Agent Settings"</h3>
                            <p class="settings-panel-subtitle">"Default model and framework"</p>

                            <div class="settings-section">
                                <h4 class="settings-section-title">"Default Agent Profile"</h4>
                                <p class="settings-hint">"Select a preset configuration for model and thinking level"</p>

                                <div class="settings-card-grid settings-card-grid-2">
                                    <button
                                        class="settings-card-option"
                                        class:selected=move || default_profile.get() == "auto"
                                        on:click=move |_| set_default_profile.set("auto".to_string())
                                    >
                                        <span class="settings-card-label">"Auto Optimized"</span>
                                        <span class="settings-card-badge">"Default"</span>
                                        <span class="settings-card-desc">"Intelligent model selection"</span>
                                    </button>
                                    <button
                                        class="settings-card-option"
                                        class:selected=move || default_profile.get() == "complex"
                                        on:click=move |_| set_default_profile.set("complex".to_string())
                                    >
                                        <span class="settings-card-label">"Complex Tasks"</span>
                                        <span class="settings-card-desc">"Best model, high thinking"</span>
                                    </button>
                                    <button
                                        class="settings-card-option"
                                        class:selected=move || default_profile.get() == "balanced"
                                        on:click=move |_| set_default_profile.set("balanced".to_string())
                                    >
                                        <span class="settings-card-label">"Balanced"</span>
                                        <span class="settings-card-desc">"Good balance of speed and quality"</span>
                                    </button>
                                    <button
                                        class="settings-card-option"
                                        class:selected=move || default_profile.get() == "quick"
                                        on:click=move |_| set_default_profile.set("quick".to_string())
                                    >
                                        <span class="settings-card-label">"Quick Edits"</span>
                                        <span class="settings-card-desc">"Fast model, minimal thinking"</span>
                                    </button>
                                </div>

                                <button class="settings-link-btn" on:click=move |_| {
                                    set_default_profile.set("auto".to_string());
                                    // Also reset phase configs to defaults
                                    set_spec_model.set("Claude Sonnet 4.5".to_string());
                                    set_spec_thinking.set("Medium".to_string());
                                    set_ideation_model.set("Claude Opus 4.5".to_string());
                                    set_ideation_thinking.set("High".to_string());
                                    set_roadmap_model.set("Claude Opus 4.5".to_string());
                                    set_roadmap_thinking.set("High".to_string());
                                    set_gh_issues_model.set("Claude Opus 4.5".to_string());
                                    set_gh_issues_thinking.set("Medium".to_string());
                                    set_gh_pr_model.set("Claude Opus 4.5".to_string());
                                    set_gh_pr_thinking.set("Medium".to_string());
                                    set_utility_model.set("Claude Haiku 4.5".to_string());
                                    set_utility_thinking.set("Low".to_string());
                                }>
                                    "Reset to Auto Optimized defaults"
                                </button>
                            </div>

                            <div class="settings-section">
                                <h4 class="settings-section-title">"Agent Framework"</h4>
                                <p class="settings-hint">"Select the agent framework used for task execution"</p>
                                <div class="settings-control">
                                    <select
                                        class="settings-select settings-select-full"
                                        prop:value=move || agent_framework.get()
                                        on:change=move |ev| set_agent_framework.set(event_target_value(&ev))
                                    >
                                        <option value="Auto Claude">"Auto Claude"</option>
                                        <option value="Custom">"Custom"</option>
                                    </select>
                                </div>
                            </div>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"AI Terminal Naming"</span>
                                    <span class="settings-hint">"Use AI to generate descriptive names for agent terminal sessions"</span>
                                </div>
                                <div class="settings-control">
                                    <label class="toggle-switch">
                                        <input
                                            type="checkbox"
                                            prop:checked=move || ai_terminal_naming.get()
                                            on:change=move |ev| set_ai_terminal_naming.set(event_target_checked(&ev))
                                        />
                                        <span class="toggle-slider"></span>
                                    </label>
                                </div>
                            </div>

                            <div class="settings-section">
                                <h4 class="settings-section-title">"Phase Configuration"</h4>
                                <p class="settings-hint">"Customize models and thinking level for each phase"</p>

                                <table class="settings-phase-table">
                                    <thead>
                                        <tr>
                                            <th>"Phase"</th>
                                            <th>"Model"</th>
                                            <th>"Thinking Level"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        <tr>
                                            <td>"Spec Creation"</td>
                                            <td>
                                                <select class="settings-select" prop:value=move || spec_model.get()
                                                    on:change=move |ev| set_spec_model.set(event_target_value(&ev))>
                                                    <option value="Claude Sonnet 4.5">"Claude Sonnet 4.5"</option>
                                                    <option value="Claude Opus 4.5">"Claude Opus 4.5"</option>
                                                    <option value="Claude Haiku 4.5">"Claude Haiku 4.5"</option>
                                                </select>
                                            </td>
                                            <td>
                                                <select class="settings-select" prop:value=move || spec_thinking.get()
                                                    on:change=move |ev| set_spec_thinking.set(event_target_value(&ev))>
                                                    <option value="Low">"Low"</option>
                                                    <option value="Medium">"Medium"</option>
                                                    <option value="High">"High"</option>
                                                </select>
                                            </td>
                                        </tr>
                                        <tr>
                                            <td>"Ideation"</td>
                                            <td>
                                                <select class="settings-select" prop:value=move || ideation_model.get()
                                                    on:change=move |ev| set_ideation_model.set(event_target_value(&ev))>
                                                    <option value="Claude Sonnet 4.5">"Claude Sonnet 4.5"</option>
                                                    <option value="Claude Opus 4.5">"Claude Opus 4.5"</option>
                                                    <option value="Claude Haiku 4.5">"Claude Haiku 4.5"</option>
                                                </select>
                                            </td>
                                            <td>
                                                <select class="settings-select" prop:value=move || ideation_thinking.get()
                                                    on:change=move |ev| set_ideation_thinking.set(event_target_value(&ev))>
                                                    <option value="Low">"Low"</option>
                                                    <option value="Medium">"Medium"</option>
                                                    <option value="High">"High"</option>
                                                </select>
                                            </td>
                                        </tr>
                                        <tr>
                                            <td>"Roadmap"</td>
                                            <td>
                                                <select class="settings-select" prop:value=move || roadmap_model.get()
                                                    on:change=move |ev| set_roadmap_model.set(event_target_value(&ev))>
                                                    <option value="Claude Sonnet 4.5">"Claude Sonnet 4.5"</option>
                                                    <option value="Claude Opus 4.5">"Claude Opus 4.5"</option>
                                                    <option value="Claude Haiku 4.5">"Claude Haiku 4.5"</option>
                                                </select>
                                            </td>
                                            <td>
                                                <select class="settings-select" prop:value=move || roadmap_thinking.get()
                                                    on:change=move |ev| set_roadmap_thinking.set(event_target_value(&ev))>
                                                    <option value="Low">"Low"</option>
                                                    <option value="Medium">"Medium"</option>
                                                    <option value="High">"High"</option>
                                                </select>
                                            </td>
                                        </tr>
                                        <tr>
                                            <td>"GitHub Issues"</td>
                                            <td>
                                                <select class="settings-select" prop:value=move || gh_issues_model.get()
                                                    on:change=move |ev| set_gh_issues_model.set(event_target_value(&ev))>
                                                    <option value="Claude Sonnet 4.5">"Claude Sonnet 4.5"</option>
                                                    <option value="Claude Opus 4.5">"Claude Opus 4.5"</option>
                                                    <option value="Claude Haiku 4.5">"Claude Haiku 4.5"</option>
                                                </select>
                                            </td>
                                            <td>
                                                <select class="settings-select" prop:value=move || gh_issues_thinking.get()
                                                    on:change=move |ev| set_gh_issues_thinking.set(event_target_value(&ev))>
                                                    <option value="Low">"Low"</option>
                                                    <option value="Medium">"Medium"</option>
                                                    <option value="High">"High"</option>
                                                </select>
                                            </td>
                                        </tr>
                                        <tr>
                                            <td>"GitHub PR Review"</td>
                                            <td>
                                                <select class="settings-select" prop:value=move || gh_pr_model.get()
                                                    on:change=move |ev| set_gh_pr_model.set(event_target_value(&ev))>
                                                    <option value="Claude Sonnet 4.5">"Claude Sonnet 4.5"</option>
                                                    <option value="Claude Opus 4.5">"Claude Opus 4.5"</option>
                                                    <option value="Claude Haiku 4.5">"Claude Haiku 4.5"</option>
                                                </select>
                                            </td>
                                            <td>
                                                <select class="settings-select" prop:value=move || gh_pr_thinking.get()
                                                    on:change=move |ev| set_gh_pr_thinking.set(event_target_value(&ev))>
                                                    <option value="Low">"Low"</option>
                                                    <option value="Medium">"Medium"</option>
                                                    <option value="High">"High"</option>
                                                </select>
                                            </td>
                                        </tr>
                                        <tr>
                                            <td>"Utility"</td>
                                            <td>
                                                <select class="settings-select" prop:value=move || utility_model.get()
                                                    on:change=move |ev| set_utility_model.set(event_target_value(&ev))>
                                                    <option value="Claude Sonnet 4.5">"Claude Sonnet 4.5"</option>
                                                    <option value="Claude Opus 4.5">"Claude Opus 4.5"</option>
                                                    <option value="Claude Haiku 4.5">"Claude Haiku 4.5"</option>
                                                </select>
                                            </td>
                                            <td>
                                                <select class="settings-select" prop:value=move || utility_thinking.get()
                                                    on:change=move |ev| set_utility_thinking.set(event_target_value(&ev))>
                                                    <option value="Low">"Low"</option>
                                                    <option value="Medium">"Medium"</option>
                                                    <option value="High">"High"</option>
                                                </select>
                                            </td>
                                        </tr>
                                    </tbody>
                                </table>
                            </div>
                        </div>
                    }.into_any(),

                    // ── 5: Paths ──
                    5 => view! {
                        <div class="settings-panel">
                            <h3 class="settings-panel-title">"Paths"</h3>
                            <p class="settings-panel-subtitle">"Configure executable and framework paths"</p>

                            <div class="settings-section">
                                <h4 class="settings-section-title">"Python Path"</h4>
                                <p class="settings-hint">"Path to Python executable (leave empty for auto-detection)"</p>
                                <input
                                    type="text"
                                    class="settings-text-input settings-text-input-full"
                                    placeholder="python3 (default)"
                                    prop:value=move || python_path.get()
                                    on:input=move |ev| set_python_path.set(event_target_value(&ev))
                                />
                                <p class="settings-auto-detected">"Auto-detected: python3 | Source: System PATH"</p>
                            </div>

                            <div class="settings-section">
                                <h4 class="settings-section-title">"Git Path"</h4>
                                <p class="settings-hint">"Path to Git executable (leave empty for auto-detection)"</p>
                                <input
                                    type="text"
                                    class="settings-text-input settings-text-input-full"
                                    placeholder="git (default)"
                                    prop:value=move || git_path.get()
                                    on:input=move |ev| set_git_path.set(event_target_value(&ev))
                                />
                                <p class="settings-auto-detected">"Auto-detected: /usr/bin/git | Version: 2.50.1 | Source: System PATH"</p>
                            </div>

                            <div class="settings-section">
                                <h4 class="settings-section-title">"GitHub CLI Path"</h4>
                                <p class="settings-hint">"Path to GitHub CLI (gh) executable (leave empty for auto-detection)"</p>
                                <input
                                    type="text"
                                    class="settings-text-input settings-text-input-full"
                                    placeholder="gh (default)"
                                    prop:value=move || github_cli_path.get()
                                    on:input=move |ev| set_github_cli_path.set(event_target_value(&ev))
                                />
                                <p class="settings-auto-detected">"Auto-detected: /opt/homebrew/bin/gh | Version: 2.86.0 | Source: Homebrew"</p>
                            </div>

                            <div class="settings-section">
                                <h4 class="settings-section-title">"Claude CLI Path"</h4>
                                <p class="settings-hint">"Path to Claude CLI executable (leave empty for auto-detection)"</p>
                                <input
                                    type="text"
                                    class="settings-text-input settings-text-input-full"
                                    placeholder="claude (default)"
                                    prop:value=move || claude_cli_path.get()
                                    on:input=move |ev| set_claude_cli_path.set(event_target_value(&ev))
                                />
                                <p class="settings-auto-detected">"Auto-detected: /opt/homebrew/bin/claude | Version: 2.1.42 | Source: Homebrew"</p>
                            </div>

                            <div class="settings-section">
                                <h4 class="settings-section-title">"Auto Tundra Path"</h4>
                                <p class="settings-hint">"Auto Tundra resources directory in project (read-only)"</p>
                                <input
                                    type="text"
                                    class="settings-text-input settings-text-input-full settings-text-input-readonly"
                                    readonly=true
                                    value="/Users/studio/rust-harness"
                                />
                            </div>
                        </div>
                    }.into_any(),

                    // ── 6: Integrations ──
                    6 => view! {
                        <div class="settings-panel">
                            <h3 class="settings-panel-title">"Integrations"</h3>
                            <p class="settings-panel-subtitle">"Manage Claude accounts and API keys"</p>

                            <div class="settings-section">
                                <h4 class="settings-section-title">"Claude Accounts"</h4>
                                <p class="settings-hint">"Add multiple Claude subscriptions to automatically switch between them when you hit rate limits."</p>

                                <div class="settings-account-card">
                                    <div class="settings-account-info">
                                        <span class="settings-account-avatar">"D"</span>
                                        <span class="settings-card-label">"Default"</span>
                                        <span class="settings-badge settings-badge-default">"Default"</span>
                                        <span class="settings-badge settings-badge-active">"Active"</span>
                                        <span class="settings-badge settings-badge-auth">"Authenticated"</span>
                                    </div>
                                </div>
                                // Display extra accounts
                                {move || extra_accounts.get().into_iter().map(|acct| {
                                    let initial = acct.chars().next().unwrap_or('?').to_uppercase().to_string();
                                    view! {
                                        <div class="settings-account-card">
                                            <div class="settings-account-info">
                                                <span class="settings-account-avatar">{initial}</span>
                                                <span class="settings-card-label">{acct}</span>
                                            </div>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                                <div class="settings-account-add">
                                    <input
                                        type="text"
                                        class="settings-text-input"
                                        placeholder="Account name (e.g., Work, Personal)"
                                        prop:value=move || new_account_name.get()
                                        on:input=move |ev| set_new_account_name.set(event_target_value(&ev))
                                    />
                                    <button class="btn-small" on:click=move |_| {
                                        let name = new_account_name.get();
                                        if !name.trim().is_empty() {
                                            let mut accounts = extra_accounts.get();
                                            accounts.push(name.trim().to_string());
                                            set_extra_accounts.set(accounts);
                                            set_new_account_name.set(String::new());
                                        }
                                    }>"+ Add"</button>
                                </div>
                            </div>

                            <div class="settings-section">
                                <h4 class="settings-section-title">"API Keys"</h4>
                                <div class="settings-info-banner">
                                    "Keys set here are used as defaults. Individual projects can override these in their settings."
                                </div>
                            </div>

                            <div class="settings-section">
                                <h4 class="settings-section-title">"OpenAI API Key"</h4>
                                <p class="settings-hint">"Required for Graphiti memory backend (embeddings)"</p>
                                <input
                                    type="password"
                                    class="settings-text-input settings-text-input-full"
                                    placeholder="sk-..."
                                    prop:value=move || openai_api_key_env.get()
                                    on:input=move |ev| set_openai_api_key_env.set(event_target_value(&ev))
                                />
                            </div>

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

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"Linear Team ID"</span>
                                    <span class="settings-hint">"Your Linear team identifier for issue sync (e.g. TEAM-123)"</span>
                                </div>
                                <div class="settings-control">
                                    <input
                                        type="text"
                                        class="settings-text-input"
                                        placeholder="e.g. TEAM-123"
                                        prop:value=move || linear_team_id.get()
                                        on:input=move |ev| set_linear_team_id.set(event_target_value(&ev))
                                    />
                                </div>
                            </div>
                        </div>
                    }.into_any(),

                    // ── 7: API Profiles ──
                    7 => view! {
                        <div class="settings-panel">
                            <h3 class="settings-panel-title">"API Profiles"</h3>
                            <p class="settings-panel-subtitle">"Configure custom Anthropic-compatible API endpoints"</p>
                            <div class="settings-info-banner">
                                "Local Ollama/OpenAI-compatible profile is built in. Default endpoint: http://127.0.0.1:11434, default model: qwen2.5-coder:14b. Override via providers.local_base_url and providers.local_model in config."
                            </div>

                            <div class="settings-local-provider-card">
                                <div class="settings-local-provider-row">
                                    <span class="settings-label">"Endpoint"</span>
                                    <code class="settings-inline-code">{move || local_provider_url.get()}</code>
                                </div>
                                <div class="settings-local-provider-row">
                                    <span class="settings-label">"Default Model"</span>
                                    <code class="settings-inline-code">{move || local_provider_model.get()}</code>
                                </div>
                                <div class="settings-local-provider-row">
                                    <span class="settings-label">"API Key Env"</span>
                                    <code class="settings-inline-code">{move || local_provider_key_env.get()}</code>
                                </div>
                                <div class="settings-local-provider-actions">
                                    <button
                                        class="btn-secondary"
                                        on:click=move |_| {
                                            set_local_probe_loading.set(true);
                                            set_local_probe_status.set(None);
                                            set_local_probe_models.set(Vec::new());
                                            let endpoint = local_provider_url.get();
                                            leptos::task::spawn_local(async move {
                                                match api::probe_local_provider(&endpoint).await {
                                                    Ok(result) => {
                                                        set_local_probe_status.set(Some((true, result.message)));
                                                        set_local_probe_models.set(result.sample_models);
                                                    }
                                                    Err(e) => {
                                                        set_local_probe_status.set(Some((false, e)));
                                                    }
                                                }
                                                set_local_probe_loading.set(false);
                                            });
                                        }
                                        disabled=move || local_probe_loading.get()
                                    >
                                        {move || if local_probe_loading.get() { "Testing..." } else { "Test Local Provider" }}
                                    </button>
                                </div>
                                {move || local_probe_status.get().map(|(ok, msg)| {
                                    view! {
                                        <div class=move || if ok { "settings-probe-result ok" } else { "settings-probe-result err" }>
                                            {msg}
                                        </div>
                                    }
                                })}
                                {move || (!local_probe_models.get().is_empty()).then(|| {
                                    view! {
                                        <div class="settings-probe-models">
                                            <span class="settings-hint">"Detected models:"</span>
                                            <div class="settings-probe-model-list">
                                                {move || local_probe_models.get().into_iter().map(|m| view! {
                                                    <span class="settings-probe-model-chip">{m}</span>
                                                }).collect::<Vec<_>>()}
                                            </div>
                                        </div>
                                    }
                                })}
                            </div>

                            {move || if !show_profile_form.get() {
                                view! {
                                    <div class="settings-empty-state">
                                        <div class="settings-empty-icon">"\u{1F4E6}"</div>
                                        <h4>"No API profiles configured"</h4>
                                        <p>"Create a profile to configure custom API endpoints for your builds."</p>
                                        <button class="btn-primary" on:click=move |_| set_show_profile_form.set(true)>
                                            "+ Create First Profile"
                                        </button>
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <div class="settings-section">
                                        <h4 class="settings-section-title">"New API Profile"</h4>
                                        <div class="settings-row">
                                            <div class="settings-row-info">
                                                <span class="settings-label">"Profile Name"</span>
                                            </div>
                                            <div class="settings-control">
                                                <input
                                                    type="text"
                                                    class="settings-text-input"
                                                    placeholder="e.g., OpenRouter, Custom"
                                                    prop:value=move || new_profile_name.get()
                                                    on:input=move |ev| set_new_profile_name.set(event_target_value(&ev))
                                                />
                                            </div>
                                        </div>
                                        <div class="settings-row">
                                            <div class="settings-row-info">
                                                <span class="settings-label">"Base URL"</span>
                                            </div>
                                            <div class="settings-control">
                                                <input
                                                    type="text"
                                                    class="settings-text-input"
                                                    placeholder="https://api.example.com/v1"
                                                    prop:value=move || new_profile_url.get()
                                                    on:input=move |ev| set_new_profile_url.set(event_target_value(&ev))
                                                />
                                            </div>
                                        </div>
                                        <div class="settings-row">
                                            <div class="settings-row-info">
                                                <span class="settings-label">"API Key Env Var"</span>
                                            </div>
                                            <div class="settings-control">
                                                <input
                                                    type="text"
                                                    class="settings-text-input"
                                                    placeholder="e.g., OPENROUTER_API_KEY"
                                                    prop:value=move || new_profile_key_env.get()
                                                    on:input=move |ev| set_new_profile_key_env.set(event_target_value(&ev))
                                                />
                                            </div>
                                        </div>
                                        <div class="settings-button-group" style="margin-top: 12px;">
                                            <button class="btn-primary" on:click=move |_| {
                                                let name = new_profile_name.get();
                                                let url = new_profile_url.get();
                                                let key_env = new_profile_key_env.get();
                                                if !name.trim().is_empty() {
                                                    leptos::logging::log!("Created API profile: {} ({}) key_env={}", name, url, key_env);
                                                    // Reset form
                                                    set_new_profile_name.set(String::new());
                                                    set_new_profile_url.set(String::new());
                                                    set_new_profile_key_env.set(String::new());
                                                    set_show_profile_form.set(false);
                                                }
                                            }>"Save Profile"</button>
                                            <button class="btn-secondary" on:click=move |_| {
                                                set_show_profile_form.set(false);
                                            }>"Cancel"</button>
                                        </div>
                                    </div>
                                }.into_any()
                            }}
                        </div>
                    }.into_any(),

                    // ── 8: Updates ──
                    8 => view! {
                        <div class="settings-panel">
                            <h3 class="settings-panel-title">"Updates"</h3>
                            <p class="settings-panel-subtitle">"Manage Auto Tundra updates"</p>

                            <div class="settings-version-card">
                                <div class="settings-version-info">
                                    <span class="settings-version-label">"VERSION"</span>
                                    <span class="settings-version-number">{move || version.get()}</span>
                                    <span class="settings-version-status">"You're running the latest version."</span>
                                </div>
                                <span class="settings-version-check">"\u{2705}"</span>
                            </div>
                            <button class="btn-small settings-check-updates-btn" on:click=move |_| {
                                set_update_status_msg.set(Some("Checking for updates...".to_string()));
                                leptos::task::spawn_local(async move {
                                    match api::check_updates().await {
                                        Ok(info) => {
                                            if info.is_latest {
                                                set_update_status_msg.set(Some("You're running the latest version.".to_string()));
                                            } else {
                                                set_update_status_msg.set(Some(format!("Update available: v{}", info.version)));
                                            }
                                        }
                                        Err(_) => {
                                            set_update_status_msg.set(Some("Up to date (could not reach update server).".to_string()));
                                        }
                                    }
                                });
                            }>"Check for Updates"</button>
                            {move || update_status_msg.get().map(|msg| view! {
                                <div class="settings-hint" style="margin-top: 4px;">{msg}</div>
                            })}

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"Auto-Update Projects"</span>
                                    <span class="settings-hint">"Automatically update Auto Tundra in projects when a new version is available"</span>
                                </div>
                                <div class="settings-control">
                                    <label class="toggle-switch">
                                        <input
                                            type="checkbox"
                                            prop:checked=move || auto_update_projects.get()
                                            on:change=move |ev| set_auto_update_projects.set(event_target_checked(&ev))
                                        />
                                        <span class="toggle-slider"></span>
                                    </label>
                                </div>
                            </div>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"Beta Updates"</span>
                                    <span class="settings-hint">"Receive pre-release beta versions with new features (may be less stable)"</span>
                                </div>
                                <div class="settings-control">
                                    <label class="toggle-switch">
                                        <input
                                            type="checkbox"
                                            prop:checked=move || beta_updates.get()
                                            on:change=move |ev| set_beta_updates.set(event_target_checked(&ev))
                                        />
                                        <span class="toggle-slider"></span>
                                    </label>
                                </div>
                            </div>
                        </div>
                    }.into_any(),

                    // ── 9: Notifications ──
                    9 => view! {
                        <div class="settings-panel">
                            <h3 class="settings-panel-title">"Notifications"</h3>
                            <p class="settings-panel-subtitle">"Alert preferences"</p>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"On Task Complete"</span>
                                    <span class="settings-hint">"Notify when an agent completes a task"</span>
                                </div>
                                <div class="settings-control">
                                    <label class="toggle-switch">
                                        <input
                                            type="checkbox"
                                            prop:checked=move || on_task_complete.get()
                                            on:change=move |ev| set_on_task_complete.set(event_target_checked(&ev))
                                        />
                                        <span class="toggle-slider"></span>
                                    </label>
                                </div>
                            </div>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"On Task Failed"</span>
                                    <span class="settings-hint">"Notify when an agent encounters a failure"</span>
                                </div>
                                <div class="settings-control">
                                    <label class="toggle-switch">
                                        <input
                                            type="checkbox"
                                            prop:checked=move || on_task_failed.get()
                                            on:change=move |ev| set_on_task_failed.set(event_target_checked(&ev))
                                        />
                                        <span class="toggle-slider"></span>
                                    </label>
                                </div>
                            </div>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"On Review Needed"</span>
                                    <span class="settings-hint">"Notify when a task requires human review"</span>
                                </div>
                                <div class="settings-control">
                                    <label class="toggle-switch">
                                        <input
                                            type="checkbox"
                                            prop:checked=move || on_review_needed.get()
                                            on:change=move |ev| set_on_review_needed.set(event_target_checked(&ev))
                                        />
                                        <span class="toggle-slider"></span>
                                    </label>
                                </div>
                            </div>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"Sound"</span>
                                    <span class="settings-hint">"Play a sound for notifications"</span>
                                </div>
                                <div class="settings-control">
                                    <label class="toggle-switch">
                                        <input
                                            type="checkbox"
                                            prop:checked=move || sound_enabled.get()
                                            on:change=move |ev| set_sound_enabled.set(event_target_checked(&ev))
                                        />
                                        <span class="toggle-slider"></span>
                                    </label>
                                </div>
                            </div>
                        </div>
                    }.into_any(),

                    // ── 10: Debug & Logs ──
                    10 => view! {
                        <div class="settings-panel">
                            <h3 class="settings-panel-title">"Debug & Logs"</h3>
                            <p class="settings-panel-subtitle">"Troubleshooting tools"</p>

                            <div class="settings-row">
                                <div class="settings-row-info">
                                    <span class="settings-label">"Anonymous Error Reporting"</span>
                                    <span class="settings-hint">"Help improve Auto Tundra by automatically sending anonymous crash reports and error data"</span>
                                </div>
                                <div class="settings-control">
                                    <label class="toggle-switch">
                                        <input
                                            type="checkbox"
                                            prop:checked=move || anonymous_reporting.get()
                                            on:change=move |ev| set_anonymous_reporting.set(event_target_checked(&ev))
                                        />
                                        <span class="toggle-slider"></span>
                                    </label>
                                </div>
                            </div>

                            <div class="settings-section">
                                <h4 class="settings-section-title">"Actions"</h4>
                                <div class="settings-button-group">
                                    <button class="btn-secondary" on:click=move |_| {
                                        if let Some(window) = web_sys::window() {
                                            let _ = window.open_with_url_and_target(
                                                "file:///tmp/auto-tundra/logs",
                                                "_blank",
                                            );
                                        }
                                    }>"Open Logs Folder"</button>
                                    <button class="btn-secondary" on:click=move |_| {
                                        let debug_info = format!(
                                            "{{\"version\": \"{}\", \"anonymous_reporting\": {}, \"memory_enabled\": {}, \"graphiti_url\": \"{}\"}}",
                                            version.get(),
                                            anonymous_reporting.get(),
                                            enable_memory.get(),
                                            graphiti_url.get(),
                                        );
                                        if let Some(window) = web_sys::window() {
                                            let clipboard = window.navigator().clipboard();
                                            let _ = clipboard.write_text(&debug_info);
                                        }
                                    }>"Copy Debug Info"</button>
                                    <button class="btn-secondary" on:click=move |_| {
                                        set_debug_textarea_visible.set(!debug_textarea_visible.get());
                                    }>"Load Debug Info"</button>
                                </div>
                                {move || debug_textarea_visible.get().then(|| view! {
                                    <div style="margin-top: 8px;">
                                        <textarea
                                            class="settings-text-input"
                                            style="width: 100%; min-height: 120px; font-family: monospace; font-size: 12px;"
                                            placeholder="Paste debug info JSON here..."
                                            prop:value=move || debug_paste_content.get()
                                            on:input=move |ev| set_debug_paste_content.set(event_target_value(&ev))
                                        ></textarea>
                                        <button class="btn-small" style="margin-top: 4px;" on:click=move |_| {
                                            let content = debug_paste_content.get();
                                            if !content.trim().is_empty() {
                                                leptos::logging::log!("Debug info loaded: {}", content);
                                            }
                                            set_debug_textarea_visible.set(false);
                                        }>"Apply Debug Info"</button>
                                    </div>
                                })}
                            </div>

                            <div class="settings-section">
                                <h4 class="settings-section-title">"Reporting Issues"</h4>
                                <p class="settings-hint">
                                    "If you encounter a bug, please open an issue on our GitHub repository with the debug info copied above. "
                                    "This helps us reproduce and fix the problem quickly."
                                </p>
                            </div>
                        </div>
                    }.into_any(),

                    // ── 11: Memory ──
                    11 => view! {
                        <div class="settings-panel">
                            <h3 class="settings-panel-title">"Memory"</h3>
                            <p class="settings-panel-subtitle">"Configure persistent cross-session memory for agents"</p>

                            <div class="settings-section settings-collapsible">
                                <h4 class="settings-section-title">"Memory"</h4>

                                <div class="settings-row">
                                    <div class="settings-row-info">
                                        <span class="settings-label">"Enable Memory"</span>
                                        <span class="settings-hint">"Allow persistent memory saving, using graphiti embedded datastore"</span>
                                    </div>
                                    <div class="settings-control">
                                        <label class="toggle-switch">
                                            <input
                                                type="checkbox"
                                                prop:checked=move || enable_memory.get()
                                                on:change=move |ev| set_enable_memory.set(event_target_checked(&ev))
                                            />
                                            <span class="toggle-slider"></span>
                                        </label>
                                    </div>
                                </div>

                                <div class="settings-row">
                                    <div class="settings-row-info">
                                        <span class="settings-label">"Enable Agent Memory Access"</span>
                                        <span class="settings-hint">"Allow agents to retrieve knowledge graph via MCP"</span>
                                    </div>
                                    <div class="settings-control">
                                        <label class="toggle-switch">
                                            <input
                                                type="checkbox"
                                                prop:checked=move || enable_agent_memory.get()
                                                on:change=move |ev| set_enable_agent_memory.set(event_target_checked(&ev))
                                            />
                                            <span class="toggle-slider"></span>
                                        </label>
                                    </div>
                                </div>

                                <div class="settings-section">
                                    <h4 class="settings-section-title">"Graphiti MCP Server URL"</h4>
                                    <p class="settings-hint">"URL for the Graphiti agent memory microservice"</p>
                                    <input
                                        type="text"
                                        class="settings-text-input settings-text-input-full"
                                        prop:value=move || graphiti_url.get()
                                        on:input=move |ev| set_graphiti_url.set(event_target_value(&ev))
                                    />
                                </div>

                                <div class="settings-section">
                                    <h4 class="settings-section-title">"Embedding Provider"</h4>
                                    <p class="settings-hint">"Provider for embeddings - optional - required search works without"</p>
                                    <select
                                        class="settings-select settings-select-full"
                                        prop:value=move || embedding_provider.get()
                                        on:change=move |ev| set_embedding_provider.set(event_target_value(&ev))
                                    >
                                        <option value="ollama">"Ollama Local - Free"</option>
                                        <option value="openai">"OpenAI"</option>
                                    </select>
                                </div>

                                <div class="settings-section">
                                    <h4 class="settings-section-title">"Select Embedding Model"</h4>
                                    <div class="settings-model-list">
                                        {[
                                            ("snowflake-arctic-embed2:xs", "92 MB", "Compact but lower quality and speed"),
                                            ("snowflake-arctic-embed2:s", "172 MB", "Good balance quality and speed"),
                                            ("nomic-embed-text", "274 MB", "Good general purpose embeddings"),
                                            ("snowflake-arctic-embed2:l", "568 MB", "Higher quality larger size"),
                                            ("mxbai-embed-large", "670 MB", "High quality general purpose"),
                                            ("snowflake-arctic-embed-l-v2.0", "1.3 GB", "Top quality"),
                                        ].into_iter().map(|(name, size, _desc)| {
                                            let name_str = name.to_string();
                                            let name_cmp = name.to_string();
                                            view! {
                                                <div class="settings-model-row">
                                                    <div class="settings-model-info">
                                                        <span class="settings-model-name">{name}</span>
                                                        <span class="settings-model-size">{size}</span>
                                                    </div>
                                                    <button
                                                        class="btn-small"
                                                        class:btn-selected=move || embedding_model.get() == name_cmp
                                                        on:click=move |_| set_embedding_model.set(name_str.clone())
                                                    >
                                                        {move || if embedding_model.get() == name.to_string() { "Selected" } else { "Download" }}
                                                    </button>
                                                </div>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </div>
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

                    </div> // settings-tab-content
                </div> // settings-tabbed-layout
            </div> // settings-modal-body

                // Modal footer
                <div class="settings-modal-footer">
                    <button class="settings-modal-cancel-btn" on:click=close_modal.clone()>
                        "Cancel"
                    </button>
                    <button class="settings-modal-save-btn" on:click=on_save>
                        <span inner_html=r#"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M19 21H5a2 2 0 01-2-2V5a2 2 0 012-2h11l5 5v11a2 2 0 01-2 2z"/><polyline points="17 21 17 13 7 13 7 21"/><polyline points="7 3 7 8 15 8"/></svg>"#></span>
                        " Save Settings"
                    </button>
                </div>
            </div> // settings-modal
        </div> // settings-modal-overlay

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
