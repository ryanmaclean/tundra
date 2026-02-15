use leptos::prelude::*;
use leptos::ev::MouseEvent;

use crate::state::use_app_state;
use crate::types::{BeadResponse, BeadStatus, Lane};

#[component]
pub fn NewTaskModal(
    on_close: impl Fn(MouseEvent) + Clone + 'static,
) -> impl IntoView {
    let state = use_app_state();
    let set_beads = state.set_beads;

    let (title, set_title) = signal(String::new());
    let (description, set_description) = signal(String::new());
    let (tags_input, set_tags_input) = signal(String::new());

    let on_close_bg = on_close.clone();
    let on_close_cancel = on_close.clone();
    let on_close_submit = on_close.clone();

    let on_submit = move |ev: MouseEvent| {
        let t = title.get();
        if t.is_empty() {
            return;
        }
        let d = description.get();
        let tags: Vec<String> = tags_input.get()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let id = format!("bead-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("xxx"));

        let new_bead = BeadResponse {
            id,
            title: t,
            status: BeadStatus::Planning,
            lane: Lane::Planning,
            agent_id: None,
            description: d,
            tags,
            progress_stage: "plan".to_string(),
            agent_names: vec![],
            timestamp: "just now".to_string(),
            action: Some("start".to_string()),
        };

        set_beads.update(|beads| {
            beads.insert(0, new_bead);
        });

        on_close_submit(ev);
    };

    view! {
        <div class="new-task-overlay" on:click=move |ev| on_close_bg(ev)>
        </div>
        <div class="new-task-modal">
            <h2>"Create New Task"</h2>

            <div class="form-group">
                <label>"Title"</label>
                <input
                    type="text"
                    placeholder="What needs to be done?"
                    prop:value=move || title.get()
                    on:input=move |ev| {
                        set_title.set(event_target_value(&ev));
                    }
                />
            </div>

            <div class="form-group">
                <label>"Description"</label>
                <textarea
                    placeholder="Add details about this task..."
                    prop:value=move || description.get()
                    on:input=move |ev| {
                        set_description.set(event_target_value(&ev));
                    }
                ></textarea>
            </div>

            <div class="form-group">
                <label>"Tags (comma separated)"</label>
                <input
                    type="text"
                    placeholder="Feature, High, Refactoring..."
                    prop:value=move || tags_input.get()
                    on:input=move |ev| {
                        set_tags_input.set(event_target_value(&ev));
                    }
                />
            </div>

            <div class="modal-actions">
                <button class="btn-cancel" on:click=move |ev| on_close_cancel(ev)>
                    "Cancel"
                </button>
                <button class="btn-create" on:click=on_submit>
                    "Create Task"
                </button>
            </div>
        </div>
    }
}
