use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::api;
use crate::events::Toast;

/// Notification bell icon with unread count badge and dropdown panel.
#[component]
pub fn NotificationBell(
    unread_count: ReadSignal<u64>,
    set_unread_count: WriteSignal<u64>,
    toasts: ReadSignal<Vec<Toast>>,
    set_toasts: WriteSignal<Vec<Toast>>,
) -> impl IntoView {
    let (open, set_open) = signal(false);
    let (notifications, set_notifications) = signal(Vec::<api::ApiNotification>::new());
    let (loading, set_loading) = signal(false);

    // Fetch notifications when panel is opened
    let fetch_notifs = move || {
        set_loading.set(true);
        spawn_local(async move {
            match api::fetch_notifications(false, 20, 0).await {
                Ok(list) => {
                    set_notifications.set(list);
                }
                Err(e) => {
                    web_sys::console::warn_1(&format!("Failed to fetch notifications: {e}").into());
                }
            }
            // Also refresh count
            if let Ok(count) = api::fetch_notification_count().await {
                set_unread_count.set(count.unread);
            }
            set_loading.set(false);
        });
    };

    let toggle_panel = move |_| {
        let will_open = !open.get();
        set_open.set(will_open);
        if will_open {
            fetch_notifs();
        }
    };

    let mark_all_read = move |_| {
        spawn_local(async move {
            let _ = api::mark_all_notifications_read().await;
            set_unread_count.set(0);
            set_notifications.update(|list| {
                for n in list.iter_mut() {
                    n.read = true;
                }
            });
        });
    };

    let dismiss_toast = move |id: String| {
        set_toasts.update(|list| {
            list.retain(|t| t.id != id);
        });
    };

    view! {
        <div class="notification-bell-container">
            // Bell button
            <button
                class="notification-bell-btn"
                on:click=toggle_panel
                title="Notifications"
            >
                <span
                    class=(move || if unread_count.get() > 0 { "bell-icon bell-icon-ringing" } else { "bell-icon" })
                    inner_html=r#"<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 8A6 6 0 006 8c0 7-3 9-3 9h18s-3-2-3-9"/><path d="M13.73 21a2 2 0 01-3.46 0"/></svg>"#
                ></span>
                {move || {
                    let count = unread_count.get();
                    if count > 0 {
                        Some(view! {
                            <span class="notification-badge">{
                                if count > 99 { "99+".to_string() } else { count.to_string() }
                            }</span>
                        })
                    } else {
                        None
                    }
                }}
            </button>

            // Dropdown panel
            {move || open.get().then(|| view! {
                <div class="notification-panel">
                    <div class="notification-panel-header">
                        <span class="notification-panel-title">"Notifications"</span>
                        <button
                            class="notification-mark-all-btn"
                            on:click=mark_all_read
                        >
                            "Mark all read"
                        </button>
                    </div>
                    <div class="notification-panel-body">
                        {move || {
                            if loading.get() {
                                view! { <div class="notification-loading">"Loading..."</div> }.into_any()
                            } else {
                                let list = notifications.get();
                                if list.is_empty() {
                                    view! { <div class="notification-empty">"No notifications"</div> }.into_any()
                                } else {
                                    view! {
                                        <div class="notification-list">
                                            {list.into_iter().map(|n| {
                                                let level_class = format!("notif-level-{}", n.level);
                                                let read_class = if n.read { "notif-read" } else { "notif-unread" };
                                                view! {
                                                    <div class={format!("notification-item {read_class}")}>
                                                        <span class={format!("notif-level-badge {level_class}")}>{
                                                            match n.level.as_str() {
                                                                "info" => "i",
                                                                "success" => "\u{2713}",
                                                                "warning" => "!",
                                                                "error" => "\u{2717}",
                                                                _ => "?",
                                                            }
                                                        }</span>
                                                        <div class="notif-content">
                                                            <div class="notif-title">{n.title.clone()}</div>
                                                            <div class="notif-message">{n.message.clone()}</div>
                                                            <div class="notif-meta">
                                                                <span class="notif-source">{n.source.clone()}</span>
                                                                <span class="notif-time">{format_time_ago(&n.created_at)}</span>
                                                            </div>
                                                        </div>
                                                    </div>
                                                }
                                            }).collect::<Vec<_>>()}
                                        </div>
                                    }.into_any()
                                }
                            }
                        }}
                    </div>
                </div>
            })}

            // Toast notifications overlay
            <div class="toast-container">
                {move || {
                    toasts.get().into_iter().map(|toast| {
                        let toast_id = toast.id.clone();
                        let dismiss_id = toast_id.clone();
                        let level_class = format!("toast-{}", toast.level);
                        view! {
                            <div class={format!("toast-item {level_class}")}>
                                <div class="toast-header">
                                    <span class="toast-title">{toast.title}</span>
                                    <button
                                        class="toast-dismiss"
                                        on:click=move |_| dismiss_toast(dismiss_id.clone())
                                    >
                                        "\u{2715}"
                                    </button>
                                </div>
                                <div class="toast-body">{toast.message}</div>
                            </div>
                        }
                    }).collect::<Vec<_>>()
                }}
            </div>
        </div>
    }
}

/// Simple time-ago formatter from an ISO timestamp string.
fn format_time_ago(timestamp: &str) -> String {
    // Parse ISO 8601 datetime
    match chrono::DateTime::parse_from_rfc3339(timestamp) {
        Ok(dt) => {
            let now = chrono::Utc::now();
            let diff = now.signed_duration_since(dt.with_timezone(&chrono::Utc));
            let secs = diff.num_seconds();
            if secs < 60 {
                "just now".to_string()
            } else if secs < 3600 {
                format!("{}m ago", secs / 60)
            } else if secs < 86400 {
                format!("{}h ago", secs / 3600)
            } else {
                format!("{}d ago", secs / 86400)
            }
        }
        Err(_) => timestamp.to_string(),
    }
}
