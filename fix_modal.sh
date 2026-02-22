#!/bin/bash
sed -i '' '/timestamp: chrono::Utc::now().to_rfc3339(),/d' app/leptos-ui/src/components/edit_task_modal.rs
