use leptos::ev::MouseEvent;
use leptos::prelude::*;

use crate::state::use_app_state;
use crate::types::{BeadResponse, BeadStatus, Lane};

#[component]
pub fn NewTaskModal(
    target_lane: Lane,
    on_close: impl Fn(MouseEvent) + Clone + 'static,
) -> impl IntoView {
    let state = use_app_state();
    let set_beads = state.set_beads;

    // Wizard step: 0=Basic Info, 1=Classification, 2=Context, 3=Review
    let (step, set_step) = signal(0u8);

    // Step 1: Basic Info
    let (title, set_title) = signal(String::new());
    let (description, set_description) = signal(String::new());

    // Step 2: Classification
    let (category, set_category) = signal("Feature".to_string());
    let (priority, set_priority) = signal("Medium".to_string());
    let (complexity, set_complexity) = signal("Medium".to_string());

    // Step 3: Context
    let (tags_input, set_tags_input) = signal(String::new());
    let (referenced_files, set_referenced_files) = signal(String::new());
    let (notes, set_notes) = signal(String::new());

    let on_close_bg = on_close.clone();
    let on_close_cancel = on_close.clone();

    let do_submit = move || {
        let t = title.get();
        if t.is_empty() {
            return;
        }
        let d = description.get();
        let cat = category.get();
        let pri = priority.get();

        let mut tags: Vec<String> = tags_input
            .get()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        // Add category and priority as tags
        tags.insert(0, cat);
        if pri != "Medium" {
            tags.push(pri);
        }

        let id = format!(
            "bead-{}",
            uuid::Uuid::new_v4()
                .to_string()
                .split('-')
                .next()
                .unwrap_or("xxx")
        );

        // Map target lane to appropriate status and action
        let (status, progress_stage, action) = match &target_lane {
            Lane::Backlog => (
                BeadStatus::Planning,
                "plan".to_string(),
                Some("start".to_string()),
            ),
            Lane::InProgress => (BeadStatus::InProgress, "code".to_string(), None),
            Lane::AiReview => (BeadStatus::AiReview, "qa".to_string(), None),
            Lane::Done => (BeadStatus::Done, "done".to_string(), None),
            _ => (
                BeadStatus::Planning,
                "plan".to_string(),
                Some("start".to_string()),
            ),
        };

        let new_bead = BeadResponse {
            id,
            title: t,
            status,
            lane: target_lane.clone(),
            agent_id: None,
            description: d,
            tags,
            progress_stage,
            agent_names: vec![],
            timestamp: "just now".to_string(),
            action,
        };

        set_beads.update(|beads| {
            beads.insert(0, new_bead);
        });
    };

    let step_labels = ["Basic Info", "Classification", "Context", "Review"];

    view! {
        <div class="new-task-overlay" on:click=move |ev| on_close_bg(ev)>
        </div>
        <div class="new-task-modal wizard-modal">
            <h2>"Create New Task"</h2>

            // Step indicators
            <div class="wizard-steps">
                {step_labels.iter().enumerate().map(|(i, label)| {
                    let idx = i as u8;
                    let cls = move || {
                        if step.get() == idx {
                            "wizard-step active"
                        } else if step.get() > idx {
                            "wizard-step completed"
                        } else {
                            "wizard-step"
                        }
                    };
                    view! {
                        <div class=cls>
                            <span class="wizard-step-number">{i + 1}</span>
                            <span class="wizard-step-label">{*label}</span>
                        </div>
                    }
                }).collect::<Vec<_>>()}
            </div>

            // Step 0: Basic Info
            {move || (step.get() == 0).then(|| view! {
                <div class="wizard-step-content">
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
                </div>
            })}

            // Step 1: Classification
            {move || (step.get() == 1).then(|| view! {
                <div class="wizard-step-content">
                    <div class="form-group">
                        <label>"Category"</label>
                        <select
                            prop:value=move || category.get()
                            on:change=move |ev| {
                                set_category.set(event_target_value(&ev));
                            }
                        >
                            <option value="Feature">"Feature"</option>
                            <option value="Bug Fix">"Bug Fix"</option>
                            <option value="Refactoring">"Refactoring"</option>
                            <option value="Documentation">"Documentation"</option>
                            <option value="Security">"Security"</option>
                            <option value="Performance">"Performance"</option>
                            <option value="UI/UX">"UI/UX"</option>
                            <option value="Infrastructure">"Infrastructure"</option>
                            <option value="Testing">"Testing"</option>
                        </select>
                    </div>
                    <div class="form-group">
                        <label>"Priority"</label>
                        <select
                            prop:value=move || priority.get()
                            on:change=move |ev| {
                                set_priority.set(event_target_value(&ev));
                            }
                        >
                            <option value="Low">"Low"</option>
                            <option value="Medium">"Medium"</option>
                            <option value="High">"High"</option>
                            <option value="Urgent">"Urgent"</option>
                        </select>
                    </div>
                    <div class="form-group">
                        <label>"Complexity"</label>
                        <select
                            prop:value=move || complexity.get()
                            on:change=move |ev| {
                                set_complexity.set(event_target_value(&ev));
                            }
                        >
                            <option value="Trivial">"Trivial"</option>
                            <option value="Small">"Small"</option>
                            <option value="Medium">"Medium"</option>
                            <option value="Large">"Large"</option>
                            <option value="Complex">"Complex"</option>
                        </select>
                    </div>
                </div>
            })}

            // Step 2: Context
            {move || (step.get() == 2).then(|| view! {
                <div class="wizard-step-content">
                    <div class="form-group">
                        <label>"Tags (comma separated)"</label>
                        <input
                            type="text"
                            placeholder="e.g. api, backend, urgent..."
                            prop:value=move || tags_input.get()
                            on:input=move |ev| {
                                set_tags_input.set(event_target_value(&ev));
                            }
                        />
                    </div>
                    <div class="form-group">
                        <label>"Referenced Files"</label>
                        <input
                            type="text"
                            placeholder="e.g. src/main.rs, lib/config.rs..."
                            prop:value=move || referenced_files.get()
                            on:input=move |ev| {
                                set_referenced_files.set(event_target_value(&ev));
                            }
                        />
                    </div>
                    <div class="form-group">
                        <label>"Optional Notes"</label>
                        <textarea
                            placeholder="Any additional context or notes..."
                            prop:value=move || notes.get()
                            on:input=move |ev| {
                                set_notes.set(event_target_value(&ev));
                            }
                        ></textarea>
                    </div>
                </div>
            })}

            // Step 3: Review & Submit
            {move || (step.get() == 3).then(|| view! {
                <div class="wizard-step-content">
                    <div class="wizard-review">
                        <div class="review-section">
                            <h4>"Basic Info"</h4>
                            <div class="review-row">
                                <span class="review-label">"Title:"</span>
                                <span class="review-value">{title.get()}</span>
                            </div>
                            <div class="review-row">
                                <span class="review-label">"Description:"</span>
                                <span class="review-value">{description.get()}</span>
                            </div>
                        </div>
                        <div class="review-section">
                            <h4>"Classification"</h4>
                            <div class="review-row">
                                <span class="review-label">"Category:"</span>
                                <span class="review-value">{category.get()}</span>
                            </div>
                            <div class="review-row">
                                <span class="review-label">"Priority:"</span>
                                <span class="review-value">{priority.get()}</span>
                            </div>
                            <div class="review-row">
                                <span class="review-label">"Complexity:"</span>
                                <span class="review-value">{complexity.get()}</span>
                            </div>
                        </div>
                        <div class="review-section">
                            <h4>"Context"</h4>
                            <div class="review-row">
                                <span class="review-label">"Tags:"</span>
                                <span class="review-value">
                                    {move || {
                                        let t = tags_input.get();
                                        if t.is_empty() { "(none)".to_string() } else { t }
                                    }}
                                </span>
                            </div>
                            <div class="review-row">
                                <span class="review-label">"Files:"</span>
                                <span class="review-value">
                                    {move || {
                                        let f = referenced_files.get();
                                        if f.is_empty() { "(none)".to_string() } else { f }
                                    }}
                                </span>
                            </div>
                            <div class="review-row">
                                <span class="review-label">"Notes:"</span>
                                <span class="review-value">
                                    {move || {
                                        let n = notes.get();
                                        if n.is_empty() { "(none)".to_string() } else { n }
                                    }}
                                </span>
                            </div>
                        </div>
                    </div>
                </div>
            })}

            // Navigation buttons
            <div class="modal-actions wizard-nav">
                <button class="btn-cancel" on:click=move |ev| on_close_cancel(ev)>
                    "Cancel"
                </button>
                <div class="wizard-nav-right">
                    {move || (step.get() > 0).then(|| view! {
                        <button
                            class="btn-back"
                            on:click=move |_| set_step.set(step.get() - 1)
                        >
                            "Back"
                        </button>
                    })}
                    {move || (step.get() < 3).then(|| view! {
                        <button
                            class="btn-next"
                            on:click=move |_| set_step.set(step.get() + 1)
                        >
                            "Next"
                        </button>
                    })}
                </div>
            </div>

            // Submit button rendered outside reactive closure to avoid FnOnce issues
            <div
                class="modal-actions"
                style=move || if step.get() == 3 { "display:flex; justify-content:flex-end;" } else { "display:none;" }
            >
                <button
                    class="btn-create"
                    on:click=move |ev| {
                        do_submit();
                        on_close(ev);
                    }
                >
                    "Create Task"
                </button>
            </div>
        </div>
    }
}
