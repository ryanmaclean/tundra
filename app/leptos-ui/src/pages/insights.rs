use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatMessage {
    id: String,
    role: String, // "user" or "assistant"
    content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatSession {
    id: String,
    title: String,
    messages: Vec<ChatMessage>,
}

// API request types (for future backend integration)
#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct CreateSessionRequest {
    title: String,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct SendMessageRequest {
    content: String,
}

fn demo_sessions() -> Vec<ChatSession> {
    vec![
        ChatSession {
            id: "isess-001".into(),
            title: "Architecture Overview".into(),
            messages: vec![
                ChatMessage {
                    id: "msg-001".into(),
                    role: "user".into(),
                    content: "What is the overall architecture of auto-tundra?".into(),
                },
                ChatMessage {
                    id: "msg-002".into(),
                    role: "assistant".into(),
                    content: "Auto-tundra uses a multi-agent orchestration architecture with a central daemon (at-bridge) that manages agent lifecycle, task dispatch via beads, and integrates with external tools through MCP servers. The frontend is built with Leptos WASM for real-time monitoring.".into(),
                },
            ],
        },
        ChatSession {
            id: "isess-002".into(),
            title: "Performance Analysis".into(),
            messages: vec![
                ChatMessage {
                    id: "msg-003".into(),
                    role: "user".into(),
                    content: "How can we optimize token usage across agents?".into(),
                },
                ChatMessage {
                    id: "msg-004".into(),
                    role: "assistant".into(),
                    content: "Consider implementing token budgets per session, using cheaper models (Haiku) for routine tasks, and caching common prompts. Agent skill specialization can also reduce wasted tokens by routing tasks to the most efficient agent.".into(),
                },
            ],
        },
    ]
}

#[component]
pub fn InsightsPage() -> impl IntoView {
    let (sessions, set_sessions) = signal(demo_sessions());
    let (active_session_id, set_active_session_id) = signal(Option::<String>::None);
    let (input_text, set_input_text) = signal(String::new());

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

    let on_send = move |_| {
        let text = input_text.get();
        if text.trim().is_empty() {
            return;
        }
        let sid = match active_session_id.get() {
            Some(id) => id,
            None => return,
        };

        let user_msg = ChatMessage {
            id: format!("msg-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("000")),
            role: "user".into(),
            content: text.clone(),
        };

        // Add user message
        set_sessions.update(|sessions| {
            if let Some(session) = sessions.iter_mut().find(|s| s.id == sid) {
                session.messages.push(user_msg);
            }
        });

        set_input_text.set(String::new());

        // Simulate assistant response
        let sid_clone = sid.clone();
        let set_sessions_clone = set_sessions;
        leptos::task::spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(500).await;
            let assistant_msg = ChatMessage {
                id: format!("msg-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("000")),
                role: "assistant".into(),
                content: "I'll analyze that for you. Based on the codebase structure, here are my insights...".into(),
            };
            set_sessions_clone.update(|sessions| {
                if let Some(session) = sessions.iter_mut().find(|s| s.id == sid_clone) {
                    session.messages.push(assistant_msg);
                }
            });
        });
    };

    view! {
        <div class="page-header">
            <h2>"Insights"</h2>
        </div>

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
                        view! {
                            <button
                                class="insights-session-item"
                                class:active=move || active_session_id.get().as_deref() == Some(&sid)
                                on:click=move |_| set_active_session_id.set(Some(sid_click.clone()))
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
                                    if ev.key() == "Enter" {
                                        let text = input_text.get();
                                        if text.trim().is_empty() {
                                            return;
                                        }
                                        let sid = match active_session_id.get() {
                                            Some(id) => id,
                                            None => return,
                                        };
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
                                        let sid_clone = sid.clone();
                                        let set_sessions_clone = set_sessions;
                                        leptos::task::spawn_local(async move {
                                            gloo_timers::future::TimeoutFuture::new(500).await;
                                            let assistant_msg = ChatMessage {
                                                id: format!("msg-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("000")),
                                                role: "assistant".into(),
                                                content: "I'll analyze that for you. Based on the codebase structure, here are my insights...".into(),
                                            };
                                            set_sessions_clone.update(|sessions| {
                                                if let Some(session) = sessions.iter_mut().find(|s| s.id == sid_clone) {
                                                    session.messages.push(assistant_msg);
                                                }
                                            });
                                        });
                                    }
                                }
                            />
                            <button class="insights-send-btn" on:click=on_send>
                                "Send"
                            </button>
                        </div>
                    }.into_any(),

                    None => view! {
                        <div class="insights-empty-state">
                            <div class="placeholder-icon">"💬"</div>
                            <h3>"Select or create a session"</h3>
                            <p>"Start a new insights session to chat about your codebase"</p>
                        </div>
                    }.into_any(),
                }}
            </div>
        </div>
    }
}
