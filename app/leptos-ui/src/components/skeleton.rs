use leptos::prelude::*;

/// Skeleton loading placeholder
#[component]
pub fn SkeletonCard() -> impl IntoView {
    view! {
        <div class="skeleton-card">
            <div class="skeleton skeleton-title"></div>
            <div class="skeleton skeleton-text"></div>
            <div class="skeleton skeleton-text skeleton-short"></div>
            <div class="skeleton-row">
                <div class="skeleton skeleton-badge"></div>
                <div class="skeleton skeleton-badge"></div>
            </div>
        </div>
    }
}

#[component]
pub fn SkeletonRow() -> impl IntoView {
    view! {
        <div class="skeleton-row-item">
            <div class="skeleton skeleton-avatar"></div>
            <div class="skeleton-row-content">
                <div class="skeleton skeleton-title"></div>
                <div class="skeleton skeleton-text skeleton-short"></div>
            </div>
        </div>
    }
}
