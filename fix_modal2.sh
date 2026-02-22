#!/bin/bash
sed -i '' '/effort: Some(new_effort.clone()),/a\
            metadata: None,\
' app/leptos-ui/src/components/edit_task_modal.rs
