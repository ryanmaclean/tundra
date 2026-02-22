use leptos::prelude::*;

fn tag_class(tag: &str) -> &'static str {
    let lower = tag.to_lowercase();
    if lower.contains("stuck") {
        "tag tag-stuck"
    } else if lower.contains("recovery") || lower.contains("needs recovery") {
        "tag tag-recovery"
    } else if lower.contains("feature") {
        "tag tag-feature"
    } else if lower.contains("high") {
        "tag tag-high"
    } else if lower.contains("refactor") {
        "tag tag-refactoring"
    } else if lower.contains("incomplete") {
        "tag tag-incomplete"
    } else if lower.contains("pr") {
        "tag tag-pr-created"
    } else if lower.contains("resume") {
        "tag tag-needs-resume"
    } else {
        "tag tag-default"
    }
}

fn stage_class(current: &str, this_stage: &str) -> &'static str {
    let stages = ["plan", "code", "qa", "done"];
    let current_idx = stages.iter().position(|s| *s == current).unwrap_or(0);
    let this_idx = stages.iter().position(|s| *s == this_stage).unwrap_or(0);
    if this_idx < current_idx {
        "stage completed"
    } else if this_idx == current_idx {
        "stage active"
    } else {
        "stage"
    }
}

fn action_btn_class(action: &str) -> String {
    match action {
        "start" => "action-btn action-btn-start".to_string(),
        "recover" => "action-btn action-btn-recover".to_string(),
        "resume" => "action-btn action-btn-resume".to_string(),
        _ => "action-btn".to_string(),
    }
}

fn action_btn_label(action: &str) -> &'static str {
    match action {
        "start" => "Start",
        "recover" => "Recover",
        "resume" => "Resume",
        _ => "Action",
    }
}

fn agent_initials(name: &str) -> String {
    match name.to_lowercase().as_str() {
        n if n.contains("crew") => "CR".to_string(),
        n if n.contains("swarm") => "SW".to_string(),
        n if n.contains("planner") => "PL".to_string(),
        n if n.contains("coder") => "CD".to_string(),
        n if n.contains("reviewer") => "RV".to_string(),
        n if n.contains("tester") => "TS".to_string(),
        n if n.contains("debugger") => "DB".to_string(),
        n if n.contains("architect") => "AR".to_string(),
        _ => name.chars().take(2).collect::<String>().to_uppercase(),
    }
}

fn relative_time(iso_timestamp: &str) -> String {
    if iso_timestamp.is_empty() {
        return String::new();
    }
    // Simple fallback: show the date portion (first 10 chars) of the ISO timestamp
    iso_timestamp.chars().take(10).collect()
}

#[component]
pub fn BeadCard(
    id: String,
    title: String,
    #[prop(default = String::new())]
    description: String,
    #[prop(default = String::new())]
    status: String,
    #[prop(default = Vec::new())]
    tags: Vec<String>,
    #[prop(default = String::from("plan"))]
    progress_stage: String,
    #[prop(default = Vec::new())]
    agent_names: Vec<String>,
    #[prop(default = String::new())]
    timestamp: String,
    #[prop(default = None)]
    action: Option<String>,
    #[prop(default = String::new())]
    lane: String,
    #[prop(default = String::new())]
    priority: String,
    #[prop(default = String::new())]
    updated_at: String,
    #[prop(optional)]
    on_action: Option<Callback<(String, String)>>,
) -> impl IntoView {
    let show_pipeline = !progress_stage.is_empty();
    let has_tags = !tags.is_empty();
    let has_agents = !agent_names.is_empty();
    let has_timestamp = !timestamp.is_empty();
    let has_footer = has_agents || has_timestamp || action.is_some();

    let plan_cls = stage_class(&progress_stage, "plan");
    let code_cls = stage_class(&progress_stage, "code");
    let qa_cls = stage_class(&progress_stage, "qa");
    let done_cls = stage_class(&progress_stage, "done");

    let action_class = action.as_deref().map(action_btn_class);
    let action_label = action.as_deref().map(action_btn_label);

    // Tag limit: show max 2 tags, with overflow indicator
    let visible_tags = tags.iter().take(2).cloned().collect::<Vec<_>>();
    let overflow_count = tags.len().saturating_sub(2);

    let tag_views = visible_tags.into_iter().map(|t| {
        let cls = tag_class(&t);
        view! { <span class={cls}>{t}</span> }
    }).collect::<Vec<_>>();

    // Agent initials badges instead of dots
    let agent_views = agent_names.into_iter().enumerate().map(|(i, name)| {
        let initials = agent_initials(&name);
        let color_cls = format!("agent-badge color-{}", i % 6);
        view! { <span class={color_cls} title={name}>{initials}</span> }
    }).collect::<Vec<_>>();

    // Time-in-status from updated_at
    let time_display = relative_time(&updated_at);
    let has_time = !time_display.is_empty();

    // Card-level data attributes for priority and lane
    let card_class = "bead-card".to_string();

    // Clone id for use in the action button callback (id is moved into the view below)
    let action_id = id.clone();

    view! {
        <div
            class={card_class}
            data-priority={(!priority.is_empty()).then(|| priority.clone())}
            data-lane={(!lane.is_empty()).then(|| lane.clone())}
        >
            // Lane badge (only shown for non-standard lanes)
            {(!lane.is_empty() && lane != "standard").then(|| {
                let lane_cls = format!("lane-badge lane-{}", lane);
                view! { <span class={lane_cls}>{lane.to_uppercase()}</span> }
            })}
            <div class="bead-title">{title}</div>
            <div class="bead-id">{id}</div>
            {(!description.is_empty()).then(|| view! {
                <div class="bead-description">{description}</div>
            })}
            {(!status.is_empty()).then(|| view! {
                <div class="bead-status">{status}</div>
            })}
            {has_tags.then(|| view! {
                <div class="tags-row">
                    {tag_views}
                    {(overflow_count > 0).then(|| view! {
                        <span class="tag tag-overflow">{format!("+{}", overflow_count)}</span>
                    })}
                </div>
            })}
            {show_pipeline.then(|| view! {
                <div class="progress-pipeline-compact">
                    <div class={plan_cls}><span class="stage-label">"P"</span></div>
                    <div class={code_cls}><span class="stage-label">"C"</span></div>
                    <div class={qa_cls}><span class="stage-label">"Q"</span></div>
                    <div class={done_cls}><span class="stage-label">"D"</span></div>
                </div>
            })}
            {has_footer.then(|| view! {
                <div class="bead-card-footer">
                    <div class="agent-badges">
                        {agent_views}
                    </div>
                    {has_time.then(|| view! {
                        <span class="time-in-status" title={updated_at.clone()}>
                            {format!("\u{23F1} {}", time_display)}
                        </span>
                    })}
                    {has_timestamp.then(|| view! {
                        <span class="bead-card-timestamp">{timestamp.clone()}</span>
                    })}
                    {action_class.map(|cls| {
                        let action_id = action_id.clone();
                        let action_value = action.clone().unwrap_or_default();
                        let on_action = on_action.clone();
                        view! {
                            <button class={cls} on:click=move |ev: leptos::ev::MouseEvent| {
                                ev.stop_propagation();
                                if let Some(ref cb) = on_action {
                                    cb.run((action_id.clone(), action_value.clone()));
                                }
                            }>{action_label.unwrap_or("Action")}</button>
                        }
                    })}
                </div>
            })}
        </div>
    }
}
