#!/bin/bash
sed -i '' 's/let id_clone = id.clone();/let id_clone = id.clone();/g' app/leptos-ui/src/components/edit_task_modal.rs
sed -i '' 's/leptos::task::spawn_local(async move {/let async_id = id.clone();\
        leptos::task::spawn_local(async move {/g' app/leptos-ui/src/components/edit_task_modal.rs
sed -i '' 's/let id_clone = id.clone();/let id_clone = async_id.clone();/g' app/leptos-ui/src/components/edit_task_modal.rs
