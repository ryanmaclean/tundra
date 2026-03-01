use leptos::ev::{KeyboardEvent, MouseEvent};
use leptos::prelude::*;
use leptos::task::spawn_local;
use web_sys;

use crate::components::focus_trap::use_focus_trap;
use crate::state::use_app_state;

#[component]
pub fn EditTaskModal(
    bead_id: String,
    initial_title: String,
    initial_description: String,
    initial_tags: Vec<String>,
    on_close: impl Fn(MouseEvent) + Clone + 'static,
) -> impl IntoView {
    let state = use_app_state();
    let set_beads = state.set_beads;

    // Editable fields
    let (description, set_description) = signal(initial_description);
    let (task_title, set_task_title) = signal(initial_title);
    let (agent_profile, set_agent_profile) = signal("auto_optimized".to_string());
    let (model, set_model) = signal("claude-sonnet-4".to_string());
    let (thinking_level, set_thinking_level) = signal("medium".to_string());

    // Derive initial category/priority from tags
    let initial_category = {
        let skip = [
            "Critical",
            "High",
            "Medium",
            "Low",
            "Stuck",
            "Needs Recovery",
            "PR Created",
            "Incomplete",
            "Needs Resume",
        ];
        initial_tags
            .iter()
            .find(|t| !skip.contains(&t.as_str()))
            .cloned()
            .unwrap_or_else(|| "Feature".to_string())
    };
    let initial_priority = {
        initial_tags
            .iter()
            .find(|t| matches!(t.as_str(), "Critical" | "High" | "Low"))
            .cloned()
            .unwrap_or_else(|| "Medium".to_string())
    };

    let (category, set_category) = signal(initial_category);
    let (priority, set_priority) = signal(initial_priority);
    let (complexity, set_complexity) = signal("Medium".to_string());
    let (impact, set_impact) = signal("".to_string());
    let (effort, set_effort) = signal("".to_string());

    // Submission state
    let (is_submitting, set_is_submitting) = signal(false);
    // Auto-close signal: set to true when async update succeeds
    let (modal_done, set_modal_done) = signal(false);
    {
        let on_close_done = on_close.clone();
        Effect::new(move |_| {
            if modal_done.get() {
                // Create a synthetic click event to satisfy the MouseEvent callback
                if let Ok(evt) = web_sys::MouseEvent::new("click") {
                    on_close_done(evt);
                }
            }
        });
    }

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

    let bid = bead_id.clone();
    let on_save = move |ev: MouseEvent| {
        // Prevent double-submit
        if is_submitting.get() {
            return;
        }
        set_is_submitting.set(true);

        let id = bid.clone();
        let new_title = task_title.get();
        let new_desc = description.get();
        let new_cat = category.get();
        let new_pri = priority.get();
        let new_agent_profile = agent_profile.get();
        let new_model = model.get();
        let new_thinking = thinking_level.get();
        let new_complexity = complexity.get();
        let new_impact = impact.get();
        let new_effort = effort.get();

        let req = crate::api::ApiBead {
            id: id.clone(),
            title: new_title.clone(),
            description: Some(new_desc.clone()),
            status: "pending".to_string(),
            lane: "backlog".to_string(),
            priority: if new_pri == "High" { 1 } else { 0 },
            category: Some(new_cat.clone()),
            priority_label: Some(new_pri.clone()),
            agent_profile: Some(new_agent_profile.clone()),
            model: Some(new_model.clone()),
            thinking_level: Some(new_thinking.clone()),
            complexity: Some(new_complexity.clone()),
            impact: Some(new_impact.clone()),
            effort: Some(new_effort.clone()),
            metadata: None,
        };

        let async_id = id.clone();
        leptos::task::spawn_local(async move {
            let id_clone = async_id.clone();
            let _ = crate::api::update_bead(&id_clone, &req).await;
        });

        // Build tags including all classification fields
        let mut new_tags: Vec<String> = vec![new_cat.clone()];
        if new_pri != "Medium" {
            new_tags.push(new_pri.clone());
        }

        set_beads.update(|beads| {
            if let Some(b) = beads.iter_mut().find(|b| b.id == id) {
                if !new_title.is_empty() {
                    b.title = new_title.clone();
                }
                b.description = new_desc.clone();
                // Preserve special tags
                let mut tags = new_tags.clone();
                for tag in &b.tags {
                    if matches!(
                        tag.as_str(),
                        "Stuck" | "Needs Recovery" | "PR Created" | "Incomplete" | "Needs Resume"
                    ) {
                        tags.push(tag.clone());
                    }
                }
                b.tags = tags;
                b.timestamp = "just now".to_string();
            }
        });

        // Persist to backend
        let api_id = id.clone();
        let api_title = new_title;
        let api_desc = new_desc;
        let api_pri = new_pri;
        let api_lane = new_cat;
        spawn_local(async move {
            let payload = crate::api::ApiBead {
                id: api_id.clone(),
                title: api_title,
                description: Some(api_desc),
                status: String::new(),
                lane: api_lane.clone(),
                priority: match api_pri.as_str() {
                    "Critical" => 4,
                    "High" => 3,
                    "Medium" => 2,
                    "Low" => 1,
                    _ => 2,
                },
                category: Some(api_lane),
                priority_label: Some(api_pri),
                agent_profile: Some(new_agent_profile),
                model: Some(new_model),
                thinking_level: Some(new_thinking),
                complexity: Some(new_complexity),
                impact: if new_impact.is_empty() {
                    None
                } else {
                    Some(new_impact)
                },
                effort: if new_effort.is_empty() {
                    None
                } else {
                    Some(new_effort)
                },
                metadata: None,
            };
            let _ = crate::api::update_bead(&api_id, &payload).await;
        });

        on_close(ev);
    };

    view! {
        <div class="edit-task-overlay" on:click=move |ev| {
            ev.stop_propagation();
            on_close_bg(ev);
        }></div>
        <div class="edit-task-modal" on:click=move |ev: MouseEvent| ev.stop_propagation() on:keydown=handle_keydown>
            <div class="edit-task-header">
                <h2>"Edit Task"</h2>
                <p class="edit-task-subtitle">
                    "Update task details including title, description, classification, images, and settings. Changes will be saved to the spec file."
                </p>
            </div>

            <div class="edit-task-form">
                // Description
                <div class="form-group">
                    <label class="form-label">"Description"</label>
                    <textarea
                        class="form-textarea edit-task-textarea"
                        rows="6"
                        prop:value=move || description.get()
                        on:input=move |ev| set_description.set(event_target_value(&ev))
                    ></textarea>
                </div>

                // Task Title
                <div class="form-group">
                    <label class="form-label">
                        "Task Title "
                        <span class="form-hint">"(Optional)"</span>
                    </label>
                    <input
                        type="text"
                        class="form-input"
                        prop:value=move || task_title.get()
                        on:input=move |ev| set_task_title.set(event_target_value(&ev))
                    />
                </div>

                // Agent Profile
                <div class="form-group">
                    <label class="form-label">"Agent Profile"</label>
                    <div class="agent-profile-options">
                        <label class="radio-option">
                            <input
                                type="radio"
                                name="agent_profile"
                                value="auto_optimized"
                                checked=move || agent_profile.get() == "auto_optimized"
                                on:change=move |_| set_agent_profile.set("auto_optimized".to_string())
                            />
                            <span class="radio-label">"Auto Optimized"</span>
                        </label>
                        <span class="form-hint edit-hint">"+ Edit to customize"</span>
                    </div>
                </div>

                // Phase Configuration
                <div class="form-group">
                    <label class="form-label">"Phase Configuration"</label>
                    <div class="phase-config-row">
                        <div class="phase-config-item">
                            <label class="form-label-sm">"Model"</label>
                            <select
                                class="form-select"
                                prop:value=move || model.get()
                                on:change=move |ev| set_model.set(event_target_value(&ev))
                            >
                                <option value="claude-opus-4">"Opus 4"</option>
                                <option value="claude-sonnet-4">"Sonnet 4"</option>
                                <option value="claude-haiku-3">"Haiku 3"</option>
                            </select>
                        </div>
                        <div class="phase-config-item">
                            <label class="form-label-sm">"Thinking Level"</label>
                            <select
                                class="form-select"
                                prop:value=move || thinking_level.get()
                                on:change=move |ev| set_thinking_level.set(event_target_value(&ev))
                            >
                                <option value="low">"Low"</option>
                                <option value="medium">"Medium"</option>
                                <option value="high">"High"</option>
                            </select>
                        </div>
                    </div>
                </div>

                // Classification section
                <div class="form-group">
                    <label class="form-label">
                        "Classification "
                        <span class="form-hint">"(optional)"</span>
                    </label>
                    <div class="classification-grid">
                        <div class="classification-item">
                            <label class="form-label-sm">"Category"</label>
                            <select
                                class="form-select"
                                prop:value=move || category.get()
                                on:change=move |ev| set_category.set(event_target_value(&ev))
                            >
                                <option value="Feature">"Feature"</option>
                                <option value="Bug">"Bug"</option>
                                <option value="Refactor">"Refactor"</option>
                                <option value="Docs">"Docs"</option>
                                <option value="Test">"Test"</option>
                                <option value="Security">"Security"</option>
                                <option value="Performance">"Performance"</option>
                            </select>
                        </div>
                        <div class="classification-item">
                            <label class="form-label-sm">"Priority"</label>
                            <select
                                class="form-select"
                                prop:value=move || priority.get()
                                on:change=move |ev| set_priority.set(event_target_value(&ev))
                            >
                                <option value="Low">"Low"</option>
                                <option value="Medium">"Medium"</option>
                                <option value="High">"High"</option>
                                <option value="Critical">"Critical"</option>
                            </select>
                        </div>
                        <div class="classification-item">
                            <label class="form-label-sm">"Complexity"</label>
                            <select
                                class="form-select"
                                prop:value=move || complexity.get()
                                on:change=move |ev| set_complexity.set(event_target_value(&ev))
                            >
                                <option value="Simple">"Simple"</option>
                                <option value="Medium">"Medium"</option>
                                <option value="Complex">"Complex"</option>
                            </select>
                        </div>
                    </div>
                </div>

                // Impact / Effort
                <div class="form-group">
                    <div class="impact-effort-row">
                        <div class="impact-effort-item">
                            <label class="form-label-sm">"Impact"</label>
                            <select
                                class="form-select"
                                prop:value=move || impact.get()
                                on:change=move |ev| set_impact.set(event_target_value(&ev))
                            >
                                <option value="">"Select Impact"</option>
                                <option value="Low">"Low"</option>
                                <option value="Medium">"Medium"</option>
                                <option value="High">"High"</option>
                            </select>
                        </div>
                        <div class="impact-effort-item">
                            <label class="form-label-sm">"Effort"</label>
                            <select
                                class="form-select"
                                prop:value=move || effort.get()
                                on:change=move |ev| set_effort.set(event_target_value(&ev))
                            >
                                <option value="">"Select Effort"</option>
                                <option value="Low">"Low"</option>
                                <option value="Medium">"Medium"</option>
                                <option value="High">"High"</option>
                            </select>
                        </div>
                    </div>
                    <p class="form-hint">"These fields are optional but useful for filtering."</p>
                </div>
            </div>

            // Actions
            <div class="edit-task-actions">
                <button class="btn btn-outline" on:click=move |ev| on_close_cancel(ev)>"Cancel"</button>
                <button class="btn btn-primary" on:click=on_save>"Save Changes"</button>
            </div>
        </div>
    }
}
