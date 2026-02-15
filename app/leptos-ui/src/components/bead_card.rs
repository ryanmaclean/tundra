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

fn stage_label_class(current: &str, this_stage: &str) -> String {
    let stages = ["plan", "code", "qa", "done"];
    let current_idx = stages.iter().position(|s| *s == current).unwrap_or(0);
    let this_idx = stages.iter().position(|s| *s == this_stage).unwrap_or(0);
    if this_idx < current_idx {
        "completed".to_string()
    } else if this_idx == current_idx {
        "active".to_string()
    } else {
        String::new()
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

    let plan_lbl = stage_label_class(&progress_stage, "plan");
    let code_lbl = stage_label_class(&progress_stage, "code");
    let qa_lbl = stage_label_class(&progress_stage, "qa");
    let done_lbl = stage_label_class(&progress_stage, "done");

    let action_class = action.as_deref().map(action_btn_class);
    let action_label = action.as_deref().map(action_btn_label);

    let tag_views = tags.into_iter().map(|t| {
        let cls = tag_class(&t);
        view! { <span class={cls}>{t}</span> }
    }).collect::<Vec<_>>();

    let dot_views = agent_names.into_iter().enumerate().map(|(i, _name)| {
        let color_cls = format!("agent-dot color-{}", i % 6);
        view! { <span class={color_cls} title={_name}></span> }
    }).collect::<Vec<_>>();

    view! {
        <div class="bead-card">
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
                </div>
            })}
            {show_pipeline.then(|| view! {
                <div class="progress-pipeline">
                    <div class={plan_cls}></div>
                    <div class={code_cls}></div>
                    <div class={qa_cls}></div>
                    <div class={done_cls}></div>
                </div>
                <div class="pipeline-labels">
                    <span class={plan_lbl}>"Plan"</span>
                    <span class={code_lbl}>"Code"</span>
                    <span class={qa_lbl}>"QA"</span>
                    <span class={done_lbl}>"Done"</span>
                </div>
            })}
            {has_footer.then(|| view! {
                <div class="bead-card-footer">
                    <div class="agent-dots">
                        {dot_views}
                    </div>
                    {has_timestamp.then(|| view! {
                        <span class="bead-card-timestamp">{timestamp.clone()}</span>
                    })}
                    {action_class.map(|cls| view! {
                        <button class={cls}>{action_label.unwrap_or("Action")}</button>
                    })}
                </div>
            })}
        </div>
    }
}
