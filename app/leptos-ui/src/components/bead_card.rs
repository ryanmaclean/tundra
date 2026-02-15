use leptos::prelude::*;

#[component]
pub fn BeadCard(
    id: String,
    title: String,
    status: String,
    description: String,
) -> impl IntoView {
    view! {
        <div class="bead-card">
            <div class="bead-title">{title}</div>
            <div class="bead-id">{id}</div>
            <div style="font-size: 0.8em; color: #8b949e; margin-top: 4px;">{description}</div>
            <div class="bead-status">{status}</div>
        </div>
    }
}
