use crate::state::use_app_state;
use crate::themed::{themed, Prompt};
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::{Deserialize, Serialize};

use crate::api;
use crate::i18n::t;

fn insights_title_icon_svg() -> &'static str {
    r#"<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3v3"/><path d="M18.36 5.64l-2.12 2.12"/><path d="M21 12h-3"/><path d="M18.36 18.36l-2.12-2.12"/><path d="M12 21v-3"/><path d="M5.64 18.36l2.12-2.12"/><path d="M3 12h3"/><path d="M5.64 5.64l2.12 2.12"/><circle cx="12" cy="12" r="3"/></svg>"#
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatMessage {
    id: String,
    role: String,
    content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatSession {
    id: String,
    title: String,
    messages: Vec<ChatMessage>,
}

fn demo_insights_sessions() -> Vec<ChatSession> {
    vec![ChatSession {
        id: "demo-insights-1".to_string(),
        title: "What features could I add next?".to_string(),
        messages: vec![
            ChatMessage {
                id: "demo-msg-1".to_string(),
                role: "assistant".to_string(),
                content: "If I were you, I’d tackle these in order:\n1. README + docs\n2. CI/CD pipeline\n3. Rotate exposed secrets\n4. AI operations dashboard\n5. Quality metrics tracking".to_string(),
            },
            ChatMessage {
                id: "demo-msg-2".to_string(),
                role: "assistant".to_string(),
                content: "What would make this unique:\n- AI cost transparency\n- Model quality tracking\n- Multi-model switching\n- Enterprise audit trail\n- Offline mode".to_string(),
            },
        ],
    }]
}

#[component]
pub fn InsightsPage() -> impl IntoView {
    let app_state = use_app_state();
    let display_mode = app_state.display_mode;
    let (sessions, set_sessions) = signal(Vec::<ChatSession>::new());
    let (active_session_id, set_active_session_id) = signal(Option::<String>::None);
    let (input_text, set_input_text) = signal(String::new());
    let (loading, set_loading) = signal(true);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);
    let (offline_demo, set_offline_demo) = signal(false);
    let (sending, set_sending) = signal(false);
    let (sidebar_collapsed, set_sidebar_collapsed) = signal(false);
    let (selected_model, set_selected_model) = signal("claude-sonnet".to_string());

    let do_refresh = move || {
        set_loading.set(true);
        set_error_msg.set(None);
        spawn_local(async move {
            match api::fetch_insights_sessions().await {
                Ok(data) => {
                    set_offline_demo.set(false);
                    let chat_sessions: Vec<ChatSession> = data
                        .into_iter()
                        .map(|s| ChatSession {
                            id: s.id,
                            title: s.title,
                            messages: vec![],
                        })
                        .collect();
                    set_sessions.set(chat_sessions);
                }
                Err(e) => {
                    if api::is_connection_error(&e) {
                        set_offline_demo.set(true);
                        set_sessions.set(demo_insights_sessions());
                        set_error_msg.set(None);
                    } else {
                        set_error_msg.set(Some(format!("Failed to fetch sessions: {e}")));
                    }
                }
            }
            set_loading.set(false);
        });
    };

    do_refresh();

    // Load messages when session is selected
    let load_messages = move |session_id: String| {
        set_active_session_id.set(Some(session_id.clone()));
        spawn_local(async move {
            match api::fetch_insights_messages(&session_id).await {
                Ok(msgs) => {
                    let chat_msgs: Vec<ChatMessage> = msgs
                        .into_iter()
                        .map(|m| ChatMessage {
                            id: m.id,
                            role: m.role,
                            content: m.content,
                        })
                        .collect();
                    set_sessions.update(|sessions| {
                        if let Some(session) = sessions.iter_mut().find(|s| s.id == session_id) {
                            session.messages = chat_msgs;
                            if session.messages.len() > 200 {
                                session.messages.drain(..session.messages.len() - 200);
                            }
                        }
                    });
                }
                Err(e) => {
                    if api::is_connection_error(&e) {
                        set_offline_demo.set(true);
                        set_sessions.set(demo_insights_sessions());
                    } else {
                        web_sys::console::error_1(&format!("Failed to load messages: {e}").into());
                    }
                }
            }
        });
    };

    let active_session = move || {
        let sid = active_session_id.get();
        sid.and_then(|id| sessions.get().into_iter().find(|s| s.id == id))
    };

    let on_new_session = move |_| {
        let new_id = format!(
            "isess-{}",
            uuid::Uuid::new_v4()
                .to_string()
                .split('-')
                .next()
                .unwrap_or("000")
        );
        let session = ChatSession {
            id: new_id.clone(),
            title: "New Chat".into(),
            messages: vec![],
        };
        set_sessions.update(|s| {
            s.push(session);
            if s.len() > 50 {
                s.drain(..s.len() - 50);
            }
        });
        set_active_session_id.set(Some(new_id));
    };

    let send_message = move || {
        let text = input_text.get();
        if text.trim().is_empty() {
            return;
        }
        let sid = match active_session_id.get() {
            Some(id) => id,
            None => return,
        };

        // Snapshot messages for rollback on API failure
        let prev_sessions = sessions.get_untracked();

        // Add user message locally (optimistic)
        let user_msg = ChatMessage {
            id: format!(
                "msg-{}",
                uuid::Uuid::new_v4()
                    .to_string()
                    .split('-')
                    .next()
                    .unwrap_or("000")
            ),
            role: "user".into(),
            content: text.clone(),
        };
        set_sessions.update(|sessions| {
            if let Some(session) = sessions.iter_mut().find(|s| s.id == sid) {
                session.messages.push(user_msg);
                if session.messages.len() > 200 {
                    session.messages.drain(..session.messages.len() - 200);
                }
            }
        });
        set_input_text.set(String::new());

        // Send to API
        set_sending.set(true);
        let sid_clone = sid.clone();
        let model = selected_model.get();
        spawn_local(async move {
            match api::send_insights_message_with_model(&sid_clone, &text, Some(&model)).await {
                Ok(response) => {
                    set_offline_demo.set(false);
                    let assistant_msg = ChatMessage {
                        id: response.id,
                        role: response.role,
                        content: response.content,
                    };
                    set_sessions.update(|sessions| {
                        if let Some(session) = sessions.iter_mut().find(|s| s.id == sid_clone) {
                            session.messages.push(assistant_msg);
                            if session.messages.len() > 200 {
                                session.messages.drain(..session.messages.len() - 200);
                            }
                        }
                    });
                }
                Err(e) => {
                    if api::is_connection_error(&e) {
                        set_offline_demo.set(true);
                        let assistant_msg = ChatMessage {
                            id: format!(
                                "demo-{}",
                                uuid::Uuid::new_v4()
                                    .to_string()
                                    .split('-')
                                    .next()
                                    .unwrap_or("000")
                            ),
                            role: "assistant".into(),
                            content: "Offline demo mode: connect the daemon to run real insights against your codebase.".into(),
                        };
                        set_sessions.update(|sessions| {
                            if let Some(session) = sessions.iter_mut().find(|s| s.id == sid_clone) {
                                session.messages.push(assistant_msg);
                                if session.messages.len() > 200 {
                                    session.messages.drain(..session.messages.len() - 200);
                                }
                            }
                        });
                        set_error_msg.set(None);
                    } else {
                        // Rollback optimistic user message
                        set_sessions.set(prev_sessions);
                        set_error_msg.set(Some(format!("Failed to send message: {e}")));
                    }
                }
            }
            set_sending.set(false);
        });
    };

    let on_send = move |_| {
        send_message();
    };

    /// Render message content with basic formatting
    fn render_content(content: &str) -> Vec<AnyView> {
        content
            .lines()
            .map(|line| {
                if line.starts_with("- ") || line.starts_with("* ") {
                    let text = line.trim_start_matches("- ").trim_start_matches("* ");
                    view! { <li class="chat-list-item">{text.to_string()}</li> }.into_any()
                } else if line.starts_with("```") {
                    view! { <div class="chat-code-fence">{line.to_string()}</div> }.into_any()
                } else if line.starts_with("## ") {
                    let text = line.trim_start_matches("## ");
                    view! { <h4 class="chat-heading">{text.to_string()}</h4> }.into_any()
                } else if line.starts_with("# ") {
                    let text = line.trim_start_matches("# ");
                    view! { <h3 class="chat-heading">{text.to_string()}</h3> }.into_any()
                } else if line.trim().is_empty() {
                    view! { <br /> }.into_any()
                } else {
                    view! { <p class="chat-paragraph">{line.to_string()}</p> }.into_any()
                }
            })
            .collect()
    }

    view! {
        <div class="page-header insights-header">
            <div class="insights-header-main">
                <h2 class="insights-title-row">
                    <span
                        class="insights-title-icon"
                        inner_html=insights_title_icon_svg()
                    ></span>
                    <span>{t("insights-title")}</span>
                </h2>
                <p class="insights-subtitle">"Ask questions about your codebase"</p>
            </div>
        </div>

        {move || error_msg.get().map(|msg| view! {
            <div class="state-banner state-banner-error">
                <span
                    class="state-banner-icon"
                    inner_html=r#"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>"#
                ></span>
                <span>{msg}</span>
            </div>
        })}

        {move || offline_demo.get().then(|| view! {
            <div class="state-banner state-banner-info">
                <span
                    class="state-banner-icon"
                    inner_html=r#"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"14\" height=\"14\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><circle cx=\"12\" cy=\"12\" r=\"10\"/><path d=\"M12 8v4\"/><path d=\"M12 16h.01\"/></svg>"#
                ></span>
                <span>"Offline demo mode: showing local insights history fallback."</span>
            </div>
        })}

        {move || loading.get().then(|| view! {
            <div class="dashboard-loading">{move || themed(display_mode.get(), Prompt::Loading)}</div>
        })}

        <div class="insights-layout">
            // Left sidebar: Chat History
            <div class=move || if sidebar_collapsed.get() { "insights-sidebar collapsed" } else { "insights-sidebar" }>
                <div class="insights-sidebar-header">
                    <h3>
                        <span
                            class="insights-history-icon"
                            inner_html=r#"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 8v5l3 3"/><path d="M3.05 11a9 9 0 1 1 .5 4"/><path d="M3 4v7h7"/></svg>"#
                        ></span>
                        <span>{t("insights-history")}</span>
                    </h3>
                    <button class="insights-collapse-btn" on:click=move |_| set_sidebar_collapsed.set(!sidebar_collapsed.get())>
                        {move || if sidebar_collapsed.get() { ">" } else { "<" }}
                    </button>
                </div>
                <div class="insights-sidebar-actions">
                    <button class="insights-new-session-btn" on:click=on_new_session>
                        {format!("+ {}", t("insights-new-chat"))}
                    </button>
                    <select
                        class="model-selector"
                        prop:value=move || selected_model.get()
                        on:change=move |ev| set_selected_model.set(event_target_value(&ev))
                    >
                        <option value="auto">"Auto (Profile Default)"</option>
                        <option value="claude-sonnet">"Claude Sonnet"</option>
                        <option value="claude-opus">"Claude Opus"</option>
                        <option value="gpt-4">"GPT-4"</option>
                        <option value="gemini-pro">"Gemini Pro"</option>
                        <option value="ollama/qwen2.5-coder:latest">"Ollama · Qwen2.5 Coder"</option>
                        <option value="ollama/llama3.2:latest">"Ollama · Llama 3.2"</option>
                        <option value="ollama/deepseek-coder-v2:latest">"Ollama · DeepSeek Coder V2"</option>
                    </select>
                </div>
                <div class="insights-session-list">
                    <For
                        each=move || sessions.get()
                        key=|session| session.id.clone()
                        let:session
                    >
                        {
                            let sid = session.id.clone();
                            let sid_click = session.id.clone();
                            let title = session.title.clone();
                            let msg_count = session.messages.len();
                            let load = load_messages.clone();
                            view! {
                                <button
                                    class="insights-session-item"
                                    class:active=move || active_session_id.get().as_deref() == Some(&sid)
                                    on:click=move |_| load(sid_click.clone())
                                >
                                    <span class="session-title">{title}</span>
                                    {(msg_count > 0).then(|| view! {
                                        <span class="session-msg-count">{format!("{}", msg_count)}</span>
                                    })}
                                </button>
                            }
                        }
                    </For>
                </div>
            </div>

            // Main chat area
            <div class="insights-chat-area">
                {move || match active_session() {
                    Some(session) => {
                        let messages = session.messages.clone();
                        view! {
                            <div class="insights-chat-messages">
                                {messages.into_iter().map(|msg| {
                                    let is_user = msg.role == "user";
                                    let bubble_class = if is_user { "chat-bubble chat-bubble-user" } else { "chat-bubble chat-bubble-assistant" };
                                    let role_label = if is_user { "You" } else { "Assistant" };
                                    let content_parts = render_content(&msg.content);
                                    view! {
                                        <div class={bubble_class}>
                                            <div class="chat-bubble-header">
                                                <span class=if is_user { "chat-avatar chat-avatar-user" } else { "chat-avatar chat-avatar-assistant" }>
                                                    {if is_user { "Y" } else { "C" }}
                                                </span>
                                                <span class="chat-bubble-role">{role_label}</span>
                                            </div>
                                            <div class="chat-bubble-content">
                                                {content_parts}
                                            </div>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>

                            <div class="insights-chat-input">
                                <input
                                    type="text"
                                    class="insights-input-field"
                                    placeholder="Ask about your codebase..."
                                    prop:value=move || input_text.get()
                                    on:input=move |ev| set_input_text.set(event_target_value(&ev))
                                    on:keydown=move |ev| {
                                        if ev.key() == "Enter" && !sending.get() {
                                            send_message();
                                        }
                                    }
                                    disabled=move || sending.get()
                                />
                                <button
                                    class="insights-send-btn"
                                    on:click=on_send
                                    disabled=move || sending.get() || input_text.get().trim().is_empty()
                                >
                                    {move || if sending.get() {
                                        "Sending...".to_string()
                                    } else {
                                        "Send".to_string()
                                    }}
                                    <span
                                        class="send-icon"
                                        inner_html=r#"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M22 2 11 13"/><path d="m22 2-7 20-4-9-9-4z"/></svg>"#
                                    ></span>
                                </button>
                            </div>
                        }.into_any()
                    }

                    None => {
                        let on_start_chat = on_new_session.clone();
                        view! {
                            <div class="insights-empty-state">
                                <div
                                    class="placeholder-icon insights-empty-icon-svg"
                                    inner_html=r#"<svg xmlns="http://www.w3.org/2000/svg" width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/><path d="M8 9h8"/><path d="M8 13h5"/></svg>"#
                                ></div>
                                <h3>"Select or create a chat session"</h3>
                                <p>"Start a new insights session to chat about your codebase"</p>
                                <button class="insights-start-chat-btn" on:click=on_start_chat>
                                    "Start Chat"
                                </button>

                                <div class="insights-feature-suggestions">
                                    <h4>"What Would Make Your Project Unique?"</h4>
                                    <ul class="insights-suggestion-list">
                                        <li class="insights-suggestion-item">
                                            <span class="insights-suggestion-icon">"\u{1F4B0}"</span>
                                            <div>
                                                <strong>"AI cost transparency"</strong>
                                                <span>" - Track spend per task and model usage"</span>
                                            </div>
                                        </li>
                                        <li class="insights-suggestion-item">
                                            <span class="insights-suggestion-icon">"\u{1F4CA}"</span>
                                            <div>
                                                <strong>"Model quality tracking"</strong>
                                                <span>" - Compare output quality across providers"</span>
                                            </div>
                                        </li>
                                        <li class="insights-suggestion-item">
                                            <span class="insights-suggestion-icon">"\u{1F500}"</span>
                                            <div>
                                                <strong>"Multi-model switching"</strong>
                                                <span>" - Route tasks to the best model dynamically"</span>
                                            </div>
                                        </li>
                                        <li class="insights-suggestion-item">
                                            <span class="insights-suggestion-icon">"\u{1F50F}"</span>
                                            <div>
                                                <strong>"Enterprise audit trail"</strong>
                                                <span>" - Full provenance for every AI-generated change"</span>
                                            </div>
                                        </li>
                                        <li class="insights-suggestion-item">
                                            <span class="insights-suggestion-icon">"\u{1F4E1}"</span>
                                            <div>
                                                <strong>"Offline mode"</strong>
                                                <span>" - Local inference with no cloud dependency"</span>
                                            </div>
                                        </li>
                                    </ul>
                                    <p class="insights-task-prompt">"Want me to create a task for any of these?"</p>
                                </div>
                            </div>
                        }.into_any()
                    },
                }}
            </div>
        </div>
    }
}
