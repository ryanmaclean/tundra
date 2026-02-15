use leptos::prelude::*;

#[component]
pub fn KpiCard(
    label: String,
    value: String,
) -> impl IntoView {
    view! {
        <div class="kpi-card">
            <div class="value">{value}</div>
            <div class="label">{label}</div>
        </div>
    }
}
