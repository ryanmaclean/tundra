#!/bin/bash
sed -i '' 's/let _ = crate::api::update_bead(&id, &req).await;/let id_clone = id.clone();\
            let _ = crate::api::update_bead(\&id_clone, \&req).await;/g' app/leptos-ui/src/components/edit_task_modal.rs
