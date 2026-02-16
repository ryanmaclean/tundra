use leptos::prelude::*;

#[derive(Debug, Clone, PartialEq)]
enum FeatureStatus {
    Planned,
    InProgress,
    Done,
}

impl FeatureStatus {
    fn label(&self) -> &'static str {
        match self {
            FeatureStatus::Planned => "Planned",
            FeatureStatus::InProgress => "In Progress",
            FeatureStatus::Done => "Done",
        }
    }

    fn css_class(&self) -> &'static str {
        match self {
            FeatureStatus::Planned => "roadmap-badge-planned",
            FeatureStatus::InProgress => "roadmap-badge-inprogress",
            FeatureStatus::Done => "roadmap-badge-done",
        }
    }
}

#[derive(Debug, Clone)]
struct FeatureCard {
    id: String,
    title: String,
    description: String,
    status: FeatureStatus,
    priority: String,
}

fn demo_features() -> Vec<FeatureCard> {
    vec![
        FeatureCard {
            id: "feat-001".into(),
            title: "Multi-Agent Convoy Orchestration".into(),
            description: "Coordinate multiple agents working on related tasks with dependency tracking, parallel execution, and automatic conflict resolution between worktrees.".into(),
            status: FeatureStatus::InProgress,
            priority: "High".into(),
        },
        FeatureCard {
            id: "feat-002".into(),
            title: "Cost Optimization Engine".into(),
            description: "Intelligent model selection based on task complexity. Route simple tasks to Haiku, complex analysis to Opus, with configurable budget thresholds and alerts.".into(),
            status: FeatureStatus::Planned,
            priority: "High".into(),
        },
        FeatureCard {
            id: "feat-003".into(),
            title: "Plugin Architecture".into(),
            description: "Extensible plugin system for custom MCP tools, agent behaviors, and UI components. Support hot-reloading of plugins without daemon restart.".into(),
            status: FeatureStatus::Planned,
            priority: "Medium".into(),
        },
        FeatureCard {
            id: "feat-004".into(),
            title: "Session Replay & Debugging".into(),
            description: "Record and replay agent sessions for debugging. Step through agent decisions, inspect tool calls, and identify failure points with a visual timeline.".into(),
            status: FeatureStatus::Planned,
            priority: "Medium".into(),
        },
        FeatureCard {
            id: "feat-005".into(),
            title: "Project Scaffolding".into(),
            description: "Initialize workspace and crate structure with proper Cargo workspace configuration, CI templates, and development documentation.".into(),
            status: FeatureStatus::Done,
            priority: "High".into(),
        },
        FeatureCard {
            id: "feat-006".into(),
            title: "Core Type System".into(),
            description: "Define Bead, Agent, Session, and Convoy types with proper serialization, validation, and state machine transitions.".into(),
            status: FeatureStatus::Done,
            priority: "High".into(),
        },
        FeatureCard {
            id: "feat-007".into(),
            title: "Leptos WASM Frontend".into(),
            description: "Build the monitoring dashboard with Leptos CSR compiled to WASM. Real-time updates via WebSocket, kanban board, and agent terminal views.".into(),
            status: FeatureStatus::InProgress,
            priority: "High".into(),
        },
        FeatureCard {
            id: "feat-008".into(),
            title: "GitHub Integration".into(),
            description: "Sync issues and PRs with GitHub API. Auto-create PRs from completed beads, link issues to tasks, and update status bidirectionally.".into(),
            status: FeatureStatus::InProgress,
            priority: "Medium".into(),
        },
    ]
}

#[component]
pub fn RoadmapPage() -> impl IntoView {
    let (features, set_features) = signal(demo_features());
    let (expanded_id, set_expanded_id) = signal(Option::<String>::None);
    let (show_add_form, set_show_add_form) = signal(false);
    let (new_title, set_new_title) = signal(String::new());
    let (new_desc, set_new_desc) = signal(String::new());
    let (new_priority, set_new_priority) = signal("Medium".to_string());

    let on_generate = move |_| {
        web_sys::console::log_1(&"Generate Roadmap clicked".into());
    };

    let on_add_feature = move |_| {
        let title = new_title.get();
        let desc = new_desc.get();
        let priority = new_priority.get();
        if title.trim().is_empty() {
            return;
        }
        let feature = FeatureCard {
            id: format!("feat-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("000")),
            title,
            description: desc,
            status: FeatureStatus::Planned,
            priority,
        };
        set_features.update(|f| f.push(feature));
        set_new_title.set(String::new());
        set_new_desc.set(String::new());
        set_new_priority.set("Medium".to_string());
        set_show_add_form.set(false);
    };

    view! {
        <div class="page-header">
            <h2>"Roadmap"</h2>
            <div class="page-header-actions">
                <button class="action-btn action-start" on:click=on_generate>
                    "Generate Roadmap"
                </button>
                <button
                    class="action-btn action-forward"
                    on:click=move |_| set_show_add_form.set(!show_add_form.get())
                >
                    "+ Add Feature"
                </button>
            </div>
        </div>

        // Add Feature form
        {move || show_add_form.get().then(|| view! {
            <div class="roadmap-add-form">
                <h3>"Add New Feature"</h3>
                <div class="roadmap-form-fields">
                    <input
                        type="text"
                        class="form-input"
                        placeholder="Feature title..."
                        prop:value=move || new_title.get()
                        on:input=move |ev| set_new_title.set(event_target_value(&ev))
                    />
                    <textarea
                        class="form-textarea"
                        placeholder="Description..."
                        prop:value=move || new_desc.get()
                        on:input=move |ev| set_new_desc.set(event_target_value(&ev))
                    ></textarea>
                    <select
                        class="settings-select"
                        prop:value=move || new_priority.get()
                        on:change=move |ev| set_new_priority.set(event_target_value(&ev))
                    >
                        <option value="High">"High"</option>
                        <option value="Medium">"Medium"</option>
                        <option value="Low">"Low"</option>
                    </select>
                    <button class="action-btn action-start" on:click=on_add_feature>
                        "Add Feature"
                    </button>
                </div>
            </div>
        })}

        // Feature grid
        <div class="roadmap-grid">
            {move || features.get().into_iter().map(|feature| {
                let fid = feature.id.clone();
                let fid_click = feature.id.clone();
                let is_expanded = move || expanded_id.get().as_deref() == Some(&fid);
                let status_class = feature.status.css_class();
                let status_label = feature.status.label();
                let title = feature.title.clone();
                let desc_snippet = if feature.description.len() > 100 {
                    format!("{}...", &feature.description[..100])
                } else {
                    feature.description.clone()
                };
                let full_desc = feature.description.clone();
                let priority = feature.priority.clone();
                let priority_class = match priority.as_str() {
                    "High" => "roadmap-priority-high",
                    "Medium" => "roadmap-priority-medium",
                    "Low" => "roadmap-priority-low",
                    _ => "roadmap-priority-medium",
                };

                view! {
                    <div
                        class="roadmap-feature-card"
                        on:click=move |_| {
                            if expanded_id.get().as_deref() == Some(&fid_click) {
                                set_expanded_id.set(None);
                            } else {
                                set_expanded_id.set(Some(fid_click.clone()));
                            }
                        }
                    >
                        <div class="roadmap-feature-header">
                            <span class="roadmap-feature-title">{title}</span>
                            <span class={format!("roadmap-status-badge {}", status_class)}>
                                {status_label}
                            </span>
                        </div>
                        <div class="roadmap-feature-meta">
                            <span class={format!("roadmap-priority-badge {}", priority_class)}>
                                {priority}
                            </span>
                        </div>
                        <div class="roadmap-feature-desc">
                            {move || if is_expanded() { full_desc.clone() } else { desc_snippet.clone() }}
                        </div>
                    </div>
                }
            }).collect::<Vec<_>>()}
        </div>
    }
}
