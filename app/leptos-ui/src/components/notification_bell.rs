use leptos::prelude::*;
use std::cell::Cell;
use std::rc::Rc;
use wasm_bindgen_futures::spawn_local;

use crate::api;
use crate::components::spinner::Spinner;
use crate::events::Toast;

/// Individual toast item with auto-dismiss timer and progress tracking.
#[component]
fn ToastItem(toast: Toast, on_dismiss: impl Fn(String) + 'static + Copy) -> impl IntoView {
    let toast_id = toast.id.clone();
    let dismiss_id = toast_id.clone();
    let level_class = format!("toast-{}", toast.level);

    // Track hover state
    let (is_hovered, set_is_hovered) = signal(false);
    // Track progress (0.0 to 100.0)
    let (progress, set_progress) = signal(100.0);

    // Auto-dismiss timer logic
    if toast.auto_dismiss {
        if let Some(duration_ms) = toast.duration_ms {
            // Track elapsed time in 100ms intervals
            let elapsed_ms = Rc::new(Cell::new(0u64));
            let is_running = Rc::new(Cell::new(true));

            // Clone for closures
            let elapsed_ms_clone = elapsed_ms.clone();
            let is_running_clone = is_running.clone();
            let dismiss_id_clone = dismiss_id.clone();

            // Progress update interval (every 100ms)
            spawn_local(async move {
                loop {
                    gloo_timers::future::TimeoutFuture::new(100).await;

                    // Check if component is still running
                    if !is_running_clone.get() {
                        break;
                    }

                    // If not hovered, update elapsed time and progress
                    if !is_hovered.get() {
                        let new_elapsed = elapsed_ms_clone.get() + 100;
                        elapsed_ms_clone.set(new_elapsed);

                        // Calculate progress percentage (100% to 0%)
                        let progress_pct =
                            100.0 - (new_elapsed as f64 / duration_ms as f64 * 100.0);
                        set_progress.set(progress_pct.max(0.0));

                        // Auto-dismiss when time is up
                        if new_elapsed >= duration_ms {
                            is_running_clone.set(false);
                            on_dismiss(dismiss_id_clone.clone());
                            break;
                        }
                    }
                    // If hovered, we just wait without incrementing elapsed_ms (timer paused)
                }
            });

            // Cleanup on unmount
            // SendWrapper is needed because Rc is not Send+Sync, but WASM is single-threaded.
            let is_running_cleanup = send_wrapper::SendWrapper::new(is_running.clone());
            on_cleanup(move || {
                is_running_cleanup.set(false);
            });
        }
    }

    let dismiss_handler = move |_| on_dismiss(dismiss_id.clone());
    let mouseenter_handler = move |_| set_is_hovered.set(true);
    let mouseleave_handler = move |_| set_is_hovered.set(false);

    view! {
        <div
            class={format!("toast-item {level_class}")}
            on:mouseenter=mouseenter_handler
            on:mouseleave=mouseleave_handler
        >
            <div class="toast-header">
                <span class="toast-title">{toast.title}</span>
                <button
                    class="toast-dismiss"
                    on:click=dismiss_handler
                >
                    "\u{2715}"
                </button>
            </div>
            <div class="toast-body">{toast.message}</div>
            {move || toast.auto_dismiss.then(|| view! {
                <div class="toast-progress-bar">
                    <div
                        class="toast-progress-fill"
                        style:width=move || format!("{}%", progress.get())
                    ></div>
                </div>
            })}
        </div>
    }
}

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
                            <span
                                class="notification-badge"
                                aria-live="polite"
                                aria-label={format!("{} unread notification{}", count, if count == 1 { "" } else { "s" })}
                            >{
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
                                view! { <Spinner size="sm" /> }.into_any()
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
            <div class="toast-container" role="alert" aria-live="assertive">
                {move || {
                    toasts.get().into_iter().map(|toast| {
                        view! {
                            <ToastItem toast=toast on_dismiss=dismiss_toast />
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
