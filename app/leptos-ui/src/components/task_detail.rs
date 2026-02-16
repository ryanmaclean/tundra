use leptos::prelude::*;
use leptos::ev::MouseEvent;

use crate::state::use_app_state;
use crate::types::{BeadResponse, BeadStatus, Lane};

fn progress_percent(stage: &str) -> u32 {
    match stage {
        "plan" => 25,
        "code" => 50,
        "qa" => 75,
        "done" => 100,
        _ => 0,
    }
}

fn status_badge_class(status: &BeadStatus) -> &'static str {
    match status {
        BeadStatus::Planning => "detail-badge badge-planning",
        BeadStatus::InProgress => "detail-badge badge-in-progress",
        BeadStatus::AiReview => "detail-badge badge-ai-review",
        BeadStatus::HumanReview => "detail-badge badge-human-review",
        BeadStatus::Done => "detail-badge badge-done",
        BeadStatus::Failed => "detail-badge badge-failed",
    }
}

fn priority_badge_class(tags: &[String]) -> &'static str {
    for tag in tags {
        match tag.as_str() {
            "Critical" => return "detail-badge badge-critical",
            "High" => return "detail-badge badge-high",
            "Low" => return "detail-badge badge-low",
            _ => {}
        }
    }
    "detail-badge badge-medium"
}

fn priority_label(tags: &[String]) -> &'static str {
    for tag in tags {
        match tag.as_str() {
            "Critical" => return "Critical",
            "High" => return "High",
            "Low" => return "Low",
            _ => {}
        }
    }
    "Medium"
}

fn category_label(tags: &[String]) -> String {
    let skip = ["Critical", "High", "Medium", "Low", "Stuck", "Needs Recovery", "PR Created", "Incomplete", "Needs Resume"];
    for tag in tags {
        if !skip.contains(&tag.as_str()) {
            return tag.clone();
        }
    }
    "Uncategorized".to_string()
}

fn execution_log(bead: &BeadResponse) -> Vec<(String, String)> {
    let mut logs = vec![("Task created".to_string(), bead.timestamp.clone())];
    match bead.status {
        BeadStatus::Planning => {
            logs.push(("Planning phase started".to_string(), bead.timestamp.clone()));
        }
        BeadStatus::InProgress => {
            logs.push(("Planning completed".to_string(), "earlier".to_string()));
            logs.push(("Implementation started".to_string(), bead.timestamp.clone()));
            if !bead.agent_names.is_empty() {
                logs.push((format!("Assigned to {}", bead.agent_names.join(", ")), bead.timestamp.clone()));
            }
        }
        BeadStatus::AiReview => {
            logs.push(("Planning completed".to_string(), "earlier".to_string()));
            logs.push(("Implementation completed".to_string(), "earlier".to_string()));
            logs.push(("AI review started".to_string(), bead.timestamp.clone()));
        }
        BeadStatus::HumanReview => {
            logs.push(("Planning completed".to_string(), "earlier".to_string()));
            logs.push(("Implementation completed".to_string(), "earlier".to_string()));
            logs.push(("AI review passed".to_string(), "earlier".to_string()));
            logs.push(("Awaiting human review".to_string(), bead.timestamp.clone()));
        }
        BeadStatus::Done => {
            logs.push(("Planning completed".to_string(), "earlier".to_string()));
            logs.push(("Implementation completed".to_string(), "earlier".to_string()));
            logs.push(("Review passed".to_string(), "earlier".to_string()));
            logs.push(("Task completed".to_string(), bead.timestamp.clone()));
        }
        BeadStatus::Failed => {
            logs.push(("Task failed".to_string(), bead.timestamp.clone()));
        }
    }
    logs
}

#[component]
fn TaskDetailInner(
    bead: BeadResponse,
    set_beads: WriteSignal<Vec<BeadResponse>>,
    on_close: impl Fn(MouseEvent) + Clone + 'static,
) -> impl IntoView {
    let title = bead.title.clone();
    let description = bead.description.clone();
    let status = bead.status.clone();
    let status_display = format!("{}", bead.status);
    let s_cls = status_badge_class(&bead.status);
    let p_cls = priority_badge_class(&bead.tags);
    let p_lbl = priority_label(&bead.tags);
    let cat = category_label(&bead.tags);
    let ts = bead.timestamp.clone();
    let bid = bead.id.clone();
    let progress = progress_percent(&bead.progress_stage);
    let prog_lbl = bead.progress_stage.clone();
    let agents = bead.agent_names.clone();
    let tags = bead.tags.clone();
    let logs = execution_log(&bead);

    let show_retry = status == BeadStatus::Failed;
    let show_cancel = status != BeadStatus::Done && status != BeadStatus::Failed;
    let show_archive = status == BeadStatus::Done || status == BeadStatus::Failed;

    let id_retry = bead.id.clone();
    let id_cancel = bead.id.clone();
    let id_archive = bead.id.clone();

    let retry_action = move |_: MouseEvent| {
        let id = id_retry.clone();
        set_beads.update(|v| {
            if let Some(b) = v.iter_mut().find(|b| b.id == id) {
                b.status = BeadStatus::Planning;
                b.lane = Lane::Planning;
                b.progress_stage = "plan".to_string();
                b.action = Some("start".to_string());
                b.timestamp = "just now".to_string();
            }
        });
    };

    let cancel_action = move |_: MouseEvent| {
        let id = id_cancel.clone();
        set_beads.update(|v| {
            if let Some(b) = v.iter_mut().find(|b| b.id == id) {
                b.status = BeadStatus::Failed;
                b.lane = Lane::Done;
                b.progress_stage = "done".to_string();
                b.action = None;
                b.timestamp = "just now".to_string();
            }
        });
    };

    let close_archive = on_close.clone();
    let close_x = on_close.clone();
    let archive_action = move |ev: MouseEvent| {
        let id = id_archive.clone();
        set_beads.update(|v| { v.retain(|b| b.id != id); });
        close_archive(ev);
    };

    view! {
        <div class="task-detail-content">
            <div class="task-detail-header">
                <div class="task-detail-title-row">
                    <h2 class="task-detail-title">{title}</h2>
                    <button class="task-detail-close" on:click=move |ev| close_x(ev)>"x"</button>
                </div>
                <div class="task-detail-badges">
                    <span class={s_cls}>{status_display}</span>
                    <span class={p_cls}>{p_lbl}</span>
                    <span class="detail-badge badge-category">{cat}</span>
                </div>
                <div class="task-detail-id">{bid}</div>
            </div>
            <div class="task-detail-meta">
                <div class="task-detail-meta-row">
                    <span class="meta-label">"Updated"</span>
                    <span class="meta-value">{ts}</span>
                </div>
                <div class="task-detail-meta-row">
                    <span class="meta-label">"Agents"</span>
                    <span class="meta-value">{if agents.is_empty() { "Unassigned".to_string() } else { agents.join(", ") }}</span>
                </div>
                <div class="task-detail-meta-row">
                    <span class="meta-label">"Tags"</span>
                    <span class="meta-value task-detail-tags">
                        {tags.iter().map(|t| view! { <span class="tag tag-default">{t.clone()}</span> }).collect::<Vec<_>>()}
                    </span>
                </div>
            </div>
            {(!description.is_empty()).then(|| view! {
                <div class="task-detail-section">
                    <h4>"Description"</h4>
                    <p class="task-detail-description">{description}</p>
                </div>
            })}
            <div class="task-detail-section">
                <h4>"Progress"</h4>
                <div class="task-detail-progress">
                    <div class="task-detail-progress-bar">
                        <div class="task-detail-progress-fill" style=format!("width: {}%", progress)></div>
                    </div>
                    <span class="task-detail-progress-label">{format!("{} ({}%)", prog_lbl.to_uppercase(), progress)}</span>
                </div>
            </div>
            <div class="task-detail-section">
                <h4>"Execution Log"</h4>
                <div class="task-detail-log">
                    {logs.into_iter().map(|(msg, time)| view! {
                        <div class="log-entry">
                            <span class="log-dot"></span>
                            <span class="log-message">{msg}</span>
                            <span class="log-time">{time}</span>
                        </div>
                    }).collect::<Vec<_>>()}
                </div>
            </div>
            <div class="task-detail-actions">
                {show_retry.then(|| view! {
                    <button class="detail-action-btn detail-action-retry" on:click=retry_action>"Retry"</button>
                })}
                {show_cancel.then(|| view! {
                    <button class="detail-action-btn detail-action-cancel" on:click=cancel_action>"Cancel"</button>
                })}
                {show_archive.then(|| view! {
                    <button class="detail-action-btn detail-action-archive" on:click=archive_action>"Archive"</button>
                })}
            </div>
        </div>
    }
}

#[component]
pub fn TaskDetail(
    bead_id: String,
    on_close: impl Fn(MouseEvent) + Clone + 'static,
) -> impl IntoView {
    let state = use_app_state();
    let beads = state.beads;
    let set_beads = state.set_beads;
    let close_bg = on_close.clone();
    let initial_bead = beads.get().into_iter().find(|b| b.id == bead_id);

    view! {
        <div class="task-detail-overlay" on:click=move |ev| close_bg(ev)></div>
        <div class="task-detail-modal">
            {match initial_bead {
                None => view! {
                    <div class="task-detail-empty">
                        <h3>"Task not found"</h3>
                        <p>"This task may have been removed."</p>
                    </div>
                }.into_any(),
                Some(bead) => view! {
                    <TaskDetailInner bead=bead set_beads=set_beads on_close=on_close />
                }.into_any(),
            }}
        </div>
    }
}
