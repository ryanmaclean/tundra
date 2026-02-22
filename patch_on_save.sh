#!/bin/bash
sed -i '' '/let new_effort = effort.get();/a\
\
        let req = crate::api::ApiBead {\
            id: id.clone(),\
            title: new_title.clone(),\
            description: Some(new_desc.clone()),\
            status: "pending".to_string(),\
            lane: "backlog".to_string(),\
            priority: if new_pri == "High" { 1 } else { 0 },\
            category: Some(new_cat.clone()),\
            priority_label: Some(new_pri.clone()),\
            agent_profile: Some(new_agent_profile.clone()),\
            model: Some(new_model.clone()),\
            thinking_level: Some(new_thinking.clone()),\
            complexity: Some(new_complexity.clone()),\
            impact: Some(new_impact.clone()),\
            effort: Some(new_effort.clone()),\
            timestamp: chrono::Utc::now().to_rfc3339(),\
        };\
\
        leptos::task::spawn_local(async move {\
            let _ = crate::api::update_bead(&id, &req).await;\
        });\
' app/leptos-ui/src/components/edit_task_modal.rs
