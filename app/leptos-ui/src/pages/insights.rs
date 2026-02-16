use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::{Deserialize, Serialize};

use crate::api;

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
    let (sessions, set_sessions) = signal(Vec::<ChatSession>::new());
    let (active_session_id, set_active_session_id) = signal(Option::<String>::None);
    let (input_text, set_input_text) = signal(String::new());
    let (loading, set_loading) = signal(true);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);
    let (sending, set_sending) = signal(false);

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
            title: "New Session".into(),
            messages: vec![],
        };
        set_sessions.update(|s| s.push(session));
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

        // Add user message locally
        let user_msg = ChatMessage {
            id: format!("msg-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("000")),
            role: "user".into(),
            content: text.clone(),
        };
        set_sessions.update(|sessions| {
            if let Some(session) = sessions.iter_mut().find(|s| s.id == sid) {
                session.messages.push(user_msg);
            }
        });
        set_input_text.set(String::new());

        // Send to API
        set_sending.set(true);
        let sid_clone = sid.clone();
        spawn_local(async move {
            match api::send_insights_message(&sid_clone, &text).await {
                Ok(response) => {
                    let assistant_msg = ChatMessage {
                        id: response.id,
                        role: response.role,
                        content: response.content,
                    };
                    set_sessions.update(|sessions| {
                        if let Some(session) = sessions.iter_mut().find(|s| s.id == sid_clone) {
                            session.messages.push(assistant_msg);
                        }
                    });
                }
                Err(e) => {
                    // Show error as assistant message
                    let err_msg = ChatMessage {
                        id: format!("err-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("000")),
                        role: "assistant".into(),
                        content: format!("Error: {e}"),
                    };
                    set_sessions.update(|sessions| {
                        if let Some(session) = sessions.iter_mut().find(|s| s.id == sid_clone) {
                            session.messages.push(err_msg);
                        }
                    });
                }
            }
            set_sending.set(false);
        });
    };

    let on_send = move |_| {
        send_message();
    };

    view! {
        <div class="page-header">
            <h2>"Insights"</h2>
        </div>

        {move || error_msg.get().map(|msg| view! {
            <div class="dashboard-error">{msg}</div>
        })}

        {move || loading.get().then(|| view! {
            <div class="dashboard-loading">"Loading sessions..."</div>
        })}

        <div class="insights-layout">
            // Left sidebar: session list
            <div class="insights-sidebar">
                <button class="insights-new-session-btn" on:click=on_new_session>
                    "+ New Session"
                </button>
                <div class="insights-session-list">
                    {move || sessions.get().into_iter().map(|session| {
                        let sid = session.id.clone();
                        let sid_click = session.id.clone();
                        let title = session.title.clone();
                        let load = load_messages.clone();
                        view! {
                            <button
                                class="insights-session-item"
                                class:active=move || active_session_id.get().as_deref() == Some(&sid)
                                on:click=move |_| load(sid_click.clone())
                            >
                                {title}
                            </button>
                        }
                    }).collect::<Vec<_>>()}
                </div>
            </div>

            // Main chat area
            <div class="insights-chat-area">
                {move || match active_session() {
                    Some(session) => view! {
                        <div class="insights-chat-messages">
                            {session.messages.iter().map(|msg| {
                                let is_user = msg.role == "user";
                                let bubble_class = if is_user { "chat-bubble chat-bubble-user" } else { "chat-bubble chat-bubble-assistant" };
                                let content = msg.content.clone();
                                view! {
                                    <div class={bubble_class}>
                                        <div class="chat-bubble-role">
                                            {if is_user { "You" } else { "Assistant" }}
                                        </div>
                                        <div class="chat-bubble-content">{content}</div>
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
                                disabled=move || sending.get()
                            >
                                {move || if sending.get() { "Sending..." } else { "Send" }}
                            </button>
                        </div>
                    }.into_any(),

                    None => view! {
                        <div class="insights-empty-state">
                            <div class="placeholder-icon">"--"</div>
                            <h3>"Select or create a session"</h3>
                            <p>"Start a new insights session to chat about your codebase"</p>
                        </div>
                    }.into_any(),
                }}
            </div>
        </div>
    }
}
