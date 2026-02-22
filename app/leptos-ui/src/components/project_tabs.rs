use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::api;

/// Horizontal project tab bar displayed at the very top of the app.
#[component]
pub fn ProjectTabs() -> impl IntoView {
    let (projects, set_projects) = signal(Vec::<api::ApiProject>::new());
    let (show_modal, set_show_modal) = signal(false);
    let (new_name, set_new_name) = signal(String::new());
    let (new_path, set_new_path) = signal(String::new());
    let (loading, set_loading) = signal(false);

    // Fetch projects on mount
    let load_projects = move || {
        spawn_local(async move {
            match api::fetch_projects().await {
                Ok(list) => set_projects.set(list),
                Err(e) => {
                    web_sys::console::warn_1(
                        &format!("Failed to fetch projects: {e}").into(),
                    );
                }
            }
        });
    };

    // Initial load
    load_projects();

    let on_activate = move |id: String| {
        spawn_local(async move {
            match api::activate_project(&id).await {
                Ok(_) => {
                    // Refresh projects to update active state
                    match api::fetch_projects().await {
                        Ok(list) => set_projects.set(list),
                        Err(_) => {}
                    }
                }
                Err(e) => {
                    web_sys::console::warn_1(
                        &format!("Failed to activate project: {e}").into(),
                    );
                }
            }
        });
    };

    let on_delete = move |id: String| {
        spawn_local(async move {
            match api::delete_project(&id).await {
                Ok(_) => {
                    match api::fetch_projects().await {
                        Ok(list) => set_projects.set(list),
                        Err(_) => {}
                    }
                }
                Err(e) => {
                    web_sys::console::warn_1(
                        &format!("Failed to delete project: {e}").into(),
                    );
                }
            }
        });
    };

    let on_add_project = move |_| {
        let name = new_name.get();
        let path = new_path.get();
        if name.is_empty() || path.is_empty() {
            return;
        }
        set_loading.set(true);
        spawn_local(async move {
            match api::create_project(&name, &path).await {
                Ok(_) => {
                    set_show_modal.set(false);
                    set_new_name.set(String::new());
                    set_new_path.set(String::new());
                    match api::fetch_projects().await {
                        Ok(list) => set_projects.set(list),
                        Err(_) => {}
                    }
                }
                Err(e) => {
                    web_sys::console::warn_1(
                        &format!("Failed to create project: {e}").into(),
                    );
                }
            }
            set_loading.set(false);
        });
    };

    view! {
        <div class="project-tabs">
            {move || {
                let list = projects.get();
                list.iter()
                    .cloned()
                    .map(|p| {
                        let id_for_click = p.id.clone();
                        let id_for_close = p.id.clone();
                        let is_active = p.is_active;
                        let on_activate = on_activate.clone();
                        let on_delete = on_delete.clone();
                        let can_delete = list.len() > 1;
                        view! {
                            <div
                                class=(move || {
                                    if is_active {
                                        "project-tab active"
                                    } else {
                                        "project-tab"
                                    }
                                })
                                on:click={
                                    let on_activate = on_activate.clone();
                                    let id = id_for_click.clone();
                                    move |_| {
                                        let on_activate = on_activate.clone();
                                        let id = id.clone();
                                        on_activate(id);
                                    }
                                }
                            >
                                <span>{p.name.clone()}</span>
                                {if can_delete {
                                    let on_delete = on_delete.clone();
                                    let id = id_for_close.clone();
                                    Some(view! {
                                        <span
                                            class="project-tab-close"
                                            on:click={
                                                let on_delete = on_delete.clone();
                                                let id = id.clone();
                                                move |ev: web_sys::MouseEvent| {
                                                    ev.stop_propagation();
                                                    let on_delete = on_delete.clone();
                                                    let id = id.clone();
                                                    on_delete(id);
                                                }
                                            }
                                        >
                                            "\u{00D7}"
                                        </span>
                                    })
                                } else {
                                    None
                                }}
                            </div>
                        }
                    })
                    .collect::<Vec<_>>()
            }}
            <div
                class="project-tab-add"
                on:click=move |_| set_show_modal.set(true)
            >
                "+"
            </div>
        </div>

        {move || show_modal.get().then(|| view! {
            <div class="modal-overlay" on:click=move |_| set_show_modal.set(false)>
                <div class="add-project-modal" on:click=move |ev: web_sys::MouseEvent| ev.stop_propagation()>
                    <h3>"Add Project"</h3>
                    <div class="modal-field">
                        <label>"Name"</label>
                        <input
                            type="text"
                            placeholder="my-project"
                            prop:value=move || new_name.get()
                            on:input=move |ev| {
                                set_new_name.set(event_target_value(&ev));
                            }
                        />
                    </div>
                    <div class="modal-field">
                        <label>"Path"</label>
                        <div class="path-input-row">
                            <span class="folder-icon">"\u{1F4C2}"</span>
                            <input
                                type="text"
                                placeholder="/path/to/project"
                                prop:value=move || new_path.get()
                                on:input=move |ev| {
                                    set_new_path.set(event_target_value(&ev));
                                }
                            />
                        </div>
                    </div>
                    <div class="modal-actions">
                        <button
                            class="btn-secondary"
                            on:click=move |_| set_show_modal.set(false)
                        >
                            "Cancel"
                        </button>
                        <button
                            class="btn-primary"
                            on:click=on_add_project
                            disabled=move || loading.get()
                        >
                            {move || if loading.get() { "Adding..." } else { "Add Project" }}
                        </button>
                    </div>
                </div>
            </div>
        })}
    }
}
