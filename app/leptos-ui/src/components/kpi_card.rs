use leptos::prelude::*;

#[component]
pub fn KpiCard(
    label: String,
    value: String,
    #[prop(default = String::new())]
    icon: String,
) -> impl IntoView {
    let has_icon = !icon.is_empty();
    view! {
        <div class="kpi-card">
            {has_icon.then(|| view! {
                <div class="kpi-icon">{icon.clone()}</div>
            })}
            <div class="value">{value}</div>
            <div class="label">{label}</div>
        </div>
    }
}
