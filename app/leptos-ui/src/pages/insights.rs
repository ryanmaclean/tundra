use leptos::prelude::*;
use crate::state::use_app_state;
use crate::themed::{themed, Prompt};
use leptos::task::spawn_local;
use serde::{Deserialize, Serialize};

use crate::api;
use crate::i18n::t;

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

#[component]
pub fn InsightsPage() -> impl IntoView {
    let app_state = use_app_state();
    let display_mode = app_state.display_mode;
    let (sessions, set_sessions) = signal(Vec::<ChatSession>::new());
    let (active_session_id, set_active_session_id) = signal(Option::<String>::None);
    let (input_text, set_input_text) = signal(String::new());
    let (loading, set_loading) = signal(true);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);
    let (sending, set_sending) = signal(false);
    let (sidebar_collapsed, set_sidebar_collapsed) = signal(false);
    let (selected_model, set_selected_model) = signal("claude-sonnet".to_string());

    let do_refresh = move || {
        set_loading.set(true);
        set_error_msg.set(None);
        spawn_local(async move {
            match api::fetch_insights_sessions().await {
                Ok(data) => {
                    let chat_sessions: Vec<ChatSession> = data.into_iter().map(|s| {
                        ChatSession {
                            id: s.id,
                            title: s.title,
                            messages: vec![],
                        }
                    }).collect();
                    set_sessions.set(chat_sessions);
                }
                Err(e) => set_error_msg.set(Some(format!("Failed to fetch sessions: {e}"))),
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
                    let chat_msgs: Vec<ChatMessage> = msgs.into_iter().map(|m| {
                        ChatMessage {
                            id: m.id,
                            role: m.role,
                            content: m.content,
                        }
                    }).collect();
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
                    web_sys::console::error_1(&format!("Failed to load messages: {e}").into());
                }
            }
        });
    };

    let active_session = move || {
        let sid = active_session_id.get();
        sid.and_then(|id| {
            sessions.get().into_iter().find(|s| s.id == id)
        })
    };

    let on_new_session = move |_| {
        let new_id = format!("isess-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("000"));
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
            id: format!("msg-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("000")),
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
                    // Rollback optimistic user message
                    set_sessions.set(prev_sessions);
                    set_error_msg.set(Some(format!("Failed to send message: {e}")));
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
        content.lines().map(|line| {
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
        }).collect()
    }

    view! {
        <div class="page-header insights-header">
            <h2>{t("insights-title")}</h2>
            <p class="insights-subtitle">"Ask questions about your codebase"</p>
        </div>

        {move || error_msg.get().map(|msg| view! {
            <div class="dashboard-error">{msg}</div>
        })}

        {move || loading.get().then(|| view! {
            <div class="dashboard-loading">{move || themed(display_mode.get(), Prompt::Loading)}</div>
        })}

        <div class="insights-layout">
            // Left sidebar: Chat History
            <div class=move || if sidebar_collapsed.get() { "insights-sidebar collapsed" } else { "insights-sidebar" }>
                <div class="insights-sidebar-header">
                    <h3>{t("insights-history")}</h3>
                    <button class="btn btn-xs btn-outline" on:click=move |_| set_sidebar_collapsed.set(!sidebar_collapsed.get())>
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
                        <option value="claude-sonnet">"Claude Sonnet"</option>
                        <option value="claude-opus">"Claude Opus"</option>
                        <option value="gpt-4">"GPT-4"</option>
                        <option value="gemini-pro">"Gemini Pro"</option>
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
                                    {move || if sending.get() { "Sending...".to_string() } else { t("insights-send") }}
                                </button>
                            </div>
                        }.into_any()
                    }

                    None => view! {
                        <div class="insights-empty-state">
                            <div class="placeholder-icon">"--"</div>
                            <h3>"Select or create a chat session"</h3>
                            <p>"Start a new insights session to chat about your codebase"</p>
                        </div>
                    }.into_any(),
                }}
            </div>
        </div>
    }
}
