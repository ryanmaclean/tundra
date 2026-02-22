use leptos::prelude::*;
use crate::themed::{themed, Prompt};
use crate::state::use_app_state;

#[component]
pub fn StacksPage() -> impl IntoView {
    let app_state = use_app_state();
    let display_mode = app_state.display_mode;

    let (stacks, set_stacks) = signal(Vec::<crate::api::ApiStack>::new());

    let (loading, set_loading) = signal(true);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);
    let (selected_stack, set_selected_stack) = signal(Option::<String>::None);

    // Fetch stacks on mount
    {
        leptos::task::spawn_local(async move {
            match crate::api::fetch_stacks().await {
                Ok(data) => {
                    set_stacks.set(data);
                }
                Err(e) => {
                    set_error_msg.set(Some(format!("Failed to load stacks: {}", e)));
                }
            }
            set_loading.set(false);
        });
    }

    let refresh = move |_| {
        set_loading.set(true);
        set_error_msg.set(None);
        leptos::task::spawn_local(async move {
            match crate::api::fetch_stacks().await {
                Ok(data) => set_stacks.set(data),
                Err(e) => set_error_msg.set(Some(format!("Failed: {}", e))),
            }
            set_loading.set(false);
        });
    };

    let stack_count = move || stacks.get().len();
    let total_tasks = move || stacks.get().iter().map(|s| s.total).sum::<u32>();

    view! {
        <div class="page-content stacks-page">
            <div class="page-header">
                <div class="page-header-left">
                    <h2>"Stacked Diffs"</h2>
                    <span class="issue-count-badge">{move || format!("{} stacks", stack_count())}</span>
                    <span class="issue-count-badge badge-secondary">{move || format!("{} tasks", total_tasks())}</span>
                </div>
                <div class="page-header-right">
                    <button class="action-btn" on:click=refresh>
                        <span>"\u{21BB} Refresh"</span>
                    </button>
                </div>
            </div>

            // Error message
            {move || error_msg.get().map(|msg| {
                view! {
                    <div class="status-banner status-error">{msg}</div>
                }
            })}

            // Loading state
            {move || loading.get().then(|| {
                view! {
                    <div class="dashboard-loading">{move || themed(display_mode.get(), Prompt::Loading)}</div>
                }
            })}

            // Empty state
            {move || (!loading.get() && stacks.get().is_empty()).then(|| {
                view! {
                    <div class="empty-state">
                        <span class="empty-state-icon" inner_html=r##"<svg width="64" height="64" viewBox="0 0 64 64" fill="none" stroke="currentColor" stroke-width="1.5" class="empty-state-svg"><path d="M32 8L8 20l24 12 24-12L32 8z" opacity="0.3"><animate attributeName="opacity" values="0.3;0.5;0.3" dur="3s" repeatCount="indefinite"/></path><path d="M8 32l24 12 24-12" opacity="0.5"><animate attributeName="opacity" values="0.5;0.7;0.5" dur="3s" begin="0.3s" repeatCount="indefinite"/></path><path d="M8 44l24 12 24-12" opacity="0.7"><animate attributeName="opacity" values="0.7;0.9;0.7" dur="3s" begin="0.6s" repeatCount="indefinite"/></path></svg>"##></span>
                        <h3>"No Stacked Diffs"</h3>
                        <p>"Create parent-child task relationships to build stacked diffs."</p>
                        <p class="text-muted">"Stacked diffs let you break large changes into reviewable incremental PRs."</p>
                    </div>
                }
            })}

            // Stack list
            <div class="stacks-grid">
                {move || stacks.get().iter().cloned().map(|stack| {
                    let root_id = stack.root.id.clone();
                    let root_title = stack.root.title.clone();
                    let root_phase = stack.root.phase.clone();
                    let root_branch = stack.root.git_branch.clone().unwrap_or_default();
                    let root_pr = stack.root.pr_number;
                    let children = stack.children.clone();
                    let total = stack.total;
                    let is_selected = {
                        let root_id = root_id.clone();
                        move || selected_stack.get().as_deref() == Some(&root_id)
                    };
                    let select_stack = {
                        let root_id = root_id.clone();
                        move |_| set_selected_stack.set(Some(root_id.clone()))
                    };

                    view! {
                        <div class=(move || if is_selected() { "stack-card stack-card-selected" } else { "stack-card" })
                             on:click=select_stack>
                            // Stack header (root node)
                            <div class="stack-root-node">
                                <div class="stack-node-connector stack-node-root"></div>
                                <div class="stack-node-content">
                                    <div class="stack-node-header">
                                        <span class="stack-node-title">{root_title}</span>
                                        <span class=(move || format!("stack-phase-badge phase-{}", root_phase.to_lowercase()))>
                                            {root_phase.clone()}
                                        </span>
                                    </div>
                                    <div class="stack-node-meta">
                                        {if !root_branch.is_empty() {
                                            Some(view! {
                                                <span class="stack-branch-tag">
                                                    {root_branch.clone()}
                                                </span>
                                            })
                                        } else {
                                            None
                                        }}
                                        {root_pr.map(|pr| view! {
                                            <span class="stack-pr-tag">
                                                {format!("PR #{}", pr)}
                                            </span>
                                        })}
                                        <span class="stack-count-tag">{format!("{} in stack", total)}</span>
                                    </div>
                                </div>
                            </div>

                            // Children nodes
                            <div class="stack-children">
                                {let child_count = children.len() as u32;
                                children.iter().cloned().map(|child| {
                                    let child_branch = child.git_branch.clone().unwrap_or_default();
                                    let child_pr = child.pr_number;
                                    let child_phase = child.phase.clone();
                                    let is_last = child.stack_position >= child_count.saturating_sub(1);

                                    view! {
                                        <div class="stack-child-node">
                                            <div class=(move || if is_last { "stack-node-connector stack-node-last" } else { "stack-node-connector" })></div>
                                            <div class="stack-node-content">
                                                <div class="stack-node-header">
                                                    <span class="stack-node-title">{child.title.clone()}</span>
                                                    <span class=(move || format!("stack-phase-badge phase-{}", child_phase.to_lowercase()))>
                                                        {child_phase.clone()}
                                                    </span>
                                                </div>
                                                <div class="stack-node-meta">
                                                    {if !child_branch.is_empty() {
                                                        Some(view! {
                                                            <span class="stack-branch-tag">
                                                                {child_branch.clone()}
                                                            </span>
                                                        })
                                                    } else {
                                                        None
                                                    }}
                                                    {child_pr.map(|pr| view! {
                                                        <span class="stack-pr-tag">
                                                            {format!("PR #{}", pr)}
                                                        </span>
                                                    })}
                                                </div>
                                            </div>
                                            <div class="stack-node-actions">
                                                <button class="stack-action-btn" title="Rebase on parent"
                                                    on:click=move |e: web_sys::MouseEvent| { e.stop_propagation(); }>
                                                    "\u{21BB}"
                                                </button>
                                                <button class="stack-action-btn" title="Merge"
                                                    on:click=move |e: web_sys::MouseEvent| { e.stop_propagation(); }>
                                                    "\u{2386}"
                                                </button>
                                            </div>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        </div>
                    }
                }).collect::<Vec<_>>()}
            </div>
        </div>
    }
}
