use leptos::ev::{KeyboardEvent, MouseEvent};
use leptos::prelude::*;
use web_sys;

use crate::components::focus_trap::use_focus_trap;
use crate::i18n::t;
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
    let on_close_escape = on_close.clone();

    let focus_trap = use_focus_trap();

    // Combined keydown handler for focus trap and Escape key
    let handle_keydown = move |ev: KeyboardEvent| {
        // Handle Escape key to close modal
        if ev.key() == "Escape" {
            // Create a synthetic MouseEvent for on_close
            if let Ok(dummy_event) = web_sys::MouseEvent::new("click") {
                on_close_escape(dummy_event);
            }
            return;
        }

        // Handle Tab/Shift+Tab for focus trapping
        focus_trap(ev);
    };

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
            subtask_statuses: vec![],
        };

        set_beads.update(|beads| {
            beads.insert(0, new_bead);
        });
    };

    let step_keys = [
        "wizard-step-basic",
        "wizard-step-classify",
        "wizard-step-context",
        "wizard-step-review",
    ];

    view! {
        <div class="new-task-overlay" on:click=on_close_bg>
        </div>
        <div class="new-task-modal wizard-modal" role="dialog" aria-modal="true" aria-labelledby="new-task-heading" on:keydown=handle_keydown>
            <h2 id="new-task-heading">{t("new-task-title")}</h2>

            // Step indicators
            <div class="wizard-steps">
                {step_keys.iter().enumerate().map(|(i, key)| {
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
                            <span class="wizard-step-label">{t(*key)}</span>
                        </div>
                    }
                }).collect::<Vec<_>>()}
            </div>

            // Step 0: Basic Info
            {move || (step.get() == 0).then(|| view! {
                <div class="wizard-step-content">
                    <div class="form-group">
                        <label>{t("form-label-title")}</label>
                        <input
                            id="task-title-input"
                            type="text"
                            placeholder={t("new-task-placeholder-title")}
                            prop:value=move || title.get()
                            on:input=move |ev| {
                                set_title.set(event_target_value(&ev));
                            }
                        />
                    </div>
                    <div class="form-group">
                        <label>{t("form-label-description")}</label>
                        <textarea
                            id="task-description-input"
                            placeholder={t("new-task-placeholder-description")}
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
                        <label>{t("form-label-category")}</label>
                        <select
                            id="task-category-select"
                            prop:value=move || category.get()
                            on:change=move |ev| {
                                set_category.set(event_target_value(&ev));
                            }
                        >
                            <option value="Feature">{t("tasks-category-feature")}</option>
                            <option value="Bug Fix">{t("category-bug-fix")}</option>
                            <option value="Refactoring">{t("category-refactoring")}</option>
                            <option value="Documentation">{t("category-documentation")}</option>
                            <option value="Security">{t("category-security")}</option>
                            <option value="Performance">{t("category-performance")}</option>
                            <option value="UI/UX">{t("category-ui-ux")}</option>
                            <option value="Infrastructure">{t("category-infrastructure")}</option>
                            <option value="Testing">{t("category-testing")}</option>
                        </select>
                    </div>
                    <div class="form-group">
                        <label>{t("form-label-priority")}</label>
                        <select
                            id="task-priority-select"
                            prop:value=move || priority.get()
                            on:change=move |ev| {
                                set_priority.set(event_target_value(&ev));
                            }
                        >
                            <option value="Low">{t("tasks-priority-low")}</option>
                            <option value="Medium">{t("tasks-priority-medium")}</option>
                            <option value="High">{t("tasks-priority-high")}</option>
                            <option value="Urgent">{t("priority-urgent")}</option>
                        </select>
                    </div>
                    <div class="form-group">
                        <label>{t("form-label-complexity")}</label>
                        <select
                            id="task-complexity-select"
                            prop:value=move || complexity.get()
                            on:change=move |ev| {
                                set_complexity.set(event_target_value(&ev));
                            }
                        >
                            <option value="Trivial">{t("complexity-trivial")}</option>
                            <option value="Small">{t("complexity-small")}</option>
                            <option value="Medium">{t("complexity-medium")}</option>
                            <option value="Large">{t("complexity-large")}</option>
                            <option value="Complex">{t("complexity-complex")}</option>
                        </select>
                    </div>
                </div>
            })}

            // Step 2: Context
            {move || (step.get() == 2).then(|| view! {
                <div class="wizard-step-content">
                    <div class="form-group">
                        <label>{t("form-label-tags")}</label>
                        <input
                            id="task-tags-input"
                            type="text"
                            placeholder={t("new-task-placeholder-tags")}
                            prop:value=move || tags_input.get()
                            on:input=move |ev| {
                                set_tags_input.set(event_target_value(&ev));
                            }
                        />
                    </div>
                    <div class="form-group">
                        <label>{t("form-label-referenced-files")}</label>
                        <input
                            id="task-files-input"
                            type="text"
                            placeholder={t("new-task-placeholder-files")}
                            prop:value=move || referenced_files.get()
                            on:input=move |ev| {
                                set_referenced_files.set(event_target_value(&ev));
                            }
                        />
                    </div>
                    <div class="form-group">
                        <label>{t("form-label-optional-notes")}</label>
                        <textarea
                            id="task-notes-textarea"
                            placeholder={t("new-task-placeholder-notes")}
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
                            <h4>{t("review-basic-info")}</h4>
                            <div class="review-row">
                                <span class="review-label">{t("review-label-title")}</span>
                                <span class="review-value">{title.get()}</span>
                            </div>
                            <div class="review-row">
                                <span class="review-label">{t("review-label-description")}</span>
                                <span class="review-value">{description.get()}</span>
                            </div>
                        </div>
                        <div class="review-section">
                            <h4>{t("review-classification")}</h4>
                            <div class="review-row">
                                <span class="review-label">{t("review-label-category")}</span>
                                <span class="review-value">{category.get()}</span>
                            </div>
                            <div class="review-row">
                                <span class="review-label">{t("review-label-priority")}</span>
                                <span class="review-value">{priority.get()}</span>
                            </div>
                            <div class="review-row">
                                <span class="review-label">{t("review-label-complexity")}</span>
                                <span class="review-value">{complexity.get()}</span>
                            </div>
                        </div>
                        <div class="review-section">
                            <h4>{t("review-context")}</h4>
                            <div class="review-row">
                                <span class="review-label">{t("review-label-tags")}</span>
                                <span class="review-value">
                                    {move || {
                                        let val = tags_input.get();
                                        if val.is_empty() { "(none)".to_string() } else { val }
                                    }}
                                </span>
                            </div>
                            <div class="review-row">
                                <span class="review-label">{t("review-label-files")}</span>
                                <span class="review-value">
                                    {move || {
                                        let f = referenced_files.get();
                                        if f.is_empty() { "(none)".to_string() } else { f }
                                    }}
                                </span>
                            </div>
                            <div class="review-row">
                                <span class="review-label">{t("review-label-notes")}</span>
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
                <button class="btn-cancel" on:click=on_close_cancel>
                    {t("btn-cancel")}
                </button>
                <div class="wizard-nav-right">
                    {move || (step.get() > 0).then(|| view! {
                        <button
                            class="btn-back"
                            on:click=move |_| set_step.set(step.get() - 1)
                        >
                            {t("btn-back")}
                        </button>
                    })}
                    {move || (step.get() < 3).then(|| view! {
                        <button
                            class="btn-next"
                            on:click=move |_| set_step.set(step.get() + 1)
                        >
                            {t("btn-next")}
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
                    {t("btn-create-task")}
                </button>
            </div>
        </div>
    }
}
