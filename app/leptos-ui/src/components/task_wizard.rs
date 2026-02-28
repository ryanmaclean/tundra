use leptos::ev::{KeyboardEvent, MouseEvent};
use leptos::prelude::*;

use crate::components::focus_trap::use_focus_trap;
use crate::state::use_app_state;
use crate::types::{BeadResponse, BeadStatus, Lane};

#[component]
pub fn TaskWizard(on_close: impl Fn(MouseEvent) + Clone + 'static) -> impl IntoView {
    let state = use_app_state();
    let set_beads = state.set_beads;

    // Wizard step: 0=Basic Info, 1=Classification, 2=Referenced Files, 3=Review
    let (step, set_step) = signal(0u8);

    // Step 1: Basic Info
    let (title, set_title) = signal(String::new());
    let (description, set_description) = signal(String::new());

    // Step 2: Classification
    let (category, set_category) = signal("Feature".to_string());
    let (priority, set_priority) = signal("Medium".to_string());
    let (complexity, set_complexity) = signal("Medium".to_string());

    // Step 3: Referenced Files
    let (file_input, set_file_input) = signal(String::new());
    let (referenced_files, set_referenced_files) = signal(Vec::<String>::new());

    // Submission error
    let (error_msg, set_error_msg) = signal(Option::<String>::None);

    let on_close_bg = on_close.clone();
    let on_close_cancel = on_close.clone();

    let focus_trap = use_focus_trap();
    let on_close_clone = on_close.clone();

    // Combined keydown handler for focus trap and Escape key
    let handle_keydown = move |ev: KeyboardEvent| {
        // Handle Escape key to close modal
        if ev.key() == "Escape" {
            // Create a synthetic MouseEvent for on_close
            if let Ok(dummy_event) = web_sys::MouseEvent::new("click") {
                on_close_clone(dummy_event);
            }
            return;
        }

        // Handle Tab/Shift+Tab for focus trapping
        focus_trap(ev);
    };

    // Add a file to the list
    let add_file = move |_| {
        let f = file_input.get().trim().to_string();
        if !f.is_empty() {
            set_referenced_files.update(|files| {
                if !files.contains(&f) {
                    files.push(f);
                }
            });
            set_file_input.set(String::new());
        }
    };

    // Submission state
    let (is_submitting, set_is_submitting) = signal(false);
    // Auto-close signal: set to true when async creation succeeds
    let (wizard_done, set_wizard_done) = signal(false);
    {
        let on_close_done = on_close.clone();
        Effect::new(move |_| {
            if wizard_done.get() {
                // Create a synthetic click event to satisfy the MouseEvent callback
                if let Ok(evt) = web_sys::MouseEvent::new("click") {
                    on_close_done(evt);
                }
            }
        });
    }

    let do_submit = move || {
        let t = title.get();
        if t.is_empty() {
            set_error_msg.set(Some("Title is required".to_string()));
            return;
        }
        if is_submitting.get() {
            return;
        }
        set_is_submitting.set(true);

        let d = description.get();
        let cat = category.get();
        let pri = priority.get();
        let comp = complexity.get();

        // Optimistic local insert
        let mut tags: Vec<String> = vec![cat.clone()];
        if pri != "Medium" {
            tags.push(pri.clone());
        }

        let temp_id = format!(
            "bead-{}",
            uuid::Uuid::new_v4()
                .to_string()
                .split('-')
                .next()
                .unwrap_or("xxx")
        );

        let new_bead = BeadResponse {
            id: temp_id.clone(),
            title: t.clone(),
            status: BeadStatus::Planning,
            lane: Lane::Backlog,
            agent_id: None,
            description: d.clone(),
            tags,
            progress_stage: "plan".to_string(),
            agent_names: vec![],
            timestamp: "just now".to_string(),
            action: Some("start".to_string()),
            subtask_statuses: vec![],
        };

        set_beads.update(|beads| {
            beads.insert(0, new_bead);
        });

        // POST to API: create bead then create task
        let set_beads = set_beads.clone();
        leptos::task::spawn_local(async move {
            let desc_opt = if d.is_empty() { None } else { Some(d.as_str()) };
            match crate::api::create_bead(&t, desc_opt, Some("standard")).await {
                Ok(api_bead) => {
                    // Update the temp ID with the real ID from the API
                    set_beads.update(|beads| {
                        if let Some(b) = beads.iter_mut().find(|b| b.id == temp_id) {
                            b.id = api_bead.id.clone();
                        }
                    });

                    // Also create the task record
                    match crate::api::create_task(
                        &t,
                        desc_opt,
                        &api_bead.id,
                        &pri.to_lowercase(),
                        &comp.to_lowercase(),
                        &cat.to_lowercase(),
                    )
                    .await
                    {
                        Ok(_) => {
                            // Both bead and task created successfully — close the wizard
                            set_wizard_done.set(true);
                        }
                        Err(e) => {
                            set_error_msg.set(Some(format!(
                                "Bead created but task creation failed: {}",
                                e
                            )));
                        }
                    }
                }
                Err(e) => {
                    set_error_msg.set(Some(format!("Failed to create task: {}", e)));
                    // Remove the optimistic bead since API creation failed
                    set_beads.update(|beads| {
                        beads.retain(|b| b.id != temp_id);
                    });
                }
            }
            set_is_submitting.set(false);
        });
    };

    let step_labels = ["Basic Info", "Classification", "Files", "Review"];

    // Validation: can we proceed to next step?
    let can_next = move || match step.get() {
        0 => !title.get().trim().is_empty(),
        _ => true,
    };

    view! {
        <div class="new-task-overlay" on:click=move |ev| on_close_bg(ev)>
        </div>
        <div class="new-task-modal wizard-modal" on:keydown=handle_keydown>
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

            // Error message
            {move || error_msg.get().map(|msg| view! {
                <div class="wizard-error">{msg}</div>
            })}

            // Step 0: Basic Info
            {move || (step.get() == 0).then(|| view! {
                <div class="wizard-step-content">
                    <div class="form-group">
                        <label>"Title *"</label>
                        <input
                            type="text"
                            placeholder="What needs to be done?"
                            prop:value=move || title.get()
                            on:input=move |ev| {
                                set_title.set(event_target_value(&ev));
                                set_error_msg.set(None);
                            }
                        />
                    </div>
                    <div class="form-group">
                        <label>"Description"</label>
                        <textarea
                            class="wizard-rich-textarea"
                            placeholder="Add detailed requirements, acceptance criteria, context..."
                            rows="6"
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
                            <option value="Bug">"Bug"</option>
                            <option value="Refactor">"Refactor"</option>
                            <option value="Docs">"Docs"</option>
                            <option value="Test">"Test"</option>
                        </select>
                    </div>
                    <div class="form-group">
                        <label>"Priority"</label>
                        <div class="wizard-priority-group">
                            {["Low", "Medium", "High", "Critical"].into_iter().map(|p| {
                                let p_str = p.to_string();
                                let p_for_click = p_str.clone();
                                let cls = move || {
                                    let base = match p {
                                        "Low" => "priority-btn priority-low",
                                        "Medium" => "priority-btn priority-med",
                                        "High" => "priority-btn priority-high",
                                        "Critical" => "priority-btn priority-critical",
                                        _ => "priority-btn",
                                    };
                                    if priority.get() == p_str {
                                        format!("{} selected", base)
                                    } else {
                                        base.to_string()
                                    }
                                };
                                let p_click = p_for_click.clone();
                                view! {
                                    <button
                                        class=cls
                                        on:click=move |_| set_priority.set(p_click.clone())
                                    >
                                        {p}
                                    </button>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                    </div>
                    <div class="form-group">
                        <label>"Complexity"</label>
                        <div class="wizard-priority-group">
                            {["Simple", "Medium", "Complex"].into_iter().map(|c| {
                                let c_str = c.to_string();
                                let c_for_click = c_str.clone();
                                let cls = move || {
                                    if complexity.get() == c_str {
                                        "complexity-btn selected".to_string()
                                    } else {
                                        "complexity-btn".to_string()
                                    }
                                };
                                let c_click = c_for_click.clone();
                                view! {
                                    <button
                                        class=cls
                                        on:click=move |_| set_complexity.set(c_click.clone())
                                    >
                                        {c}
                                    </button>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                    </div>
                </div>
            })}

            // Step 2: Referenced Files
            {move || (step.get() == 2).then(|| view! {
                <div class="wizard-step-content">
                    <div class="form-group">
                        <label>"Referenced Files"</label>
                        <div class="wizard-file-input-row">
                            <input
                                type="text"
                                placeholder="e.g. src/main.rs"
                                prop:value=move || file_input.get()
                                on:input=move |ev| {
                                    set_file_input.set(event_target_value(&ev));
                                }
                                on:keydown=move |ev| {
                                    if ev.key() == "Enter" {
                                        ev.prevent_default();
                                        let f = file_input.get().trim().to_string();
                                        if !f.is_empty() {
                                            set_referenced_files.update(|files| {
                                                if !files.contains(&f) {
                                                    files.push(f);
                                                }
                                            });
                                            set_file_input.set(String::new());
                                        }
                                    }
                                }
                            />
                            <button class="btn-add-file" on:click=add_file>
                                "+ Add"
                            </button>
                        </div>
                    </div>
                    <div class="wizard-file-list">
                        {move || {
                            let files = referenced_files.get();
                            if files.is_empty() {
                                view! {
                                    <div class="wizard-file-empty">
                                        "No files referenced yet. Add file paths relevant to this task."
                                    </div>
                                }.into_any()
                            } else {
                                files.into_iter().enumerate().map(|(idx, file)| {
                                    let file_display = file.clone();
                                    view! {
                                        <div class="wizard-file-item">
                                            <span class="wizard-file-path">{file_display}</span>
                                            <button
                                                class="wizard-file-remove"
                                                on:click=move |_| {
                                                    set_referenced_files.update(|files| {
                                                        if idx < files.len() {
                                                            files.remove(idx);
                                                        }
                                                    });
                                                }
                                            >
                                                "x"
                                            </button>
                                        </div>
                                    }
                                }).collect::<Vec<_>>().into_any()
                            }
                        }}
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
                                <span class="review-value">{move || {
                                    let d = description.get();
                                    if d.is_empty() { "(none)".to_string() } else { d }
                                }}</span>
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
                                <span class="review-value review-priority">{priority.get()}</span>
                            </div>
                            <div class="review-row">
                                <span class="review-label">"Complexity:"</span>
                                <span class="review-value">{complexity.get()}</span>
                            </div>
                        </div>
                        <div class="review-section">
                            <h4>"Referenced Files"</h4>
                            {move || {
                                let files = referenced_files.get();
                                if files.is_empty() {
                                    view! {
                                        <div class="review-row">
                                            <span class="review-value">"(none)"</span>
                                        </div>
                                    }.into_any()
                                } else {
                                    files.into_iter().map(|f| {
                                        view! {
                                            <div class="review-row">
                                                <span class="review-value wizard-file-path">{f}</span>
                                            </div>
                                        }
                                    }).collect::<Vec<_>>().into_any()
                                }
                            }}
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
                    {move || (step.get() < 3).then(|| {
                        let disabled = !can_next();
                        view! {
                            <button
                                class="btn-next"
                                prop:disabled=disabled
                                on:click=move |_| {
                                    if can_next() {
                                        set_step.set(step.get() + 1);
                                    }
                                }
                            >
                                "Next"
                            </button>
                        }
                    })}
                </div>
            </div>

            // Submit button
            <div
                class="modal-actions"
                style=move || if step.get() == 3 { "display:flex; justify-content:flex-end;" } else { "display:none;" }
            >
                <button
                    class="btn-create"
                    prop:disabled=move || is_submitting.get()
                    on:click=move |_ev| {
                        do_submit();
                    }
                >
                    {move || if is_submitting.get() { "Creating..." } else { "Create Task" }}
                </button>
            </div>
        </div>
    }
}
