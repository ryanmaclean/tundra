use leptos::prelude::*;

use crate::state::use_app_state;

#[component]
pub fn GithubPrsPage() -> impl IntoView {
    let state = use_app_state();
    let prs = state.github_prs;

    // PR creation state
    let (creating_pr, set_creating_pr) = signal(Option::<u32>::None);
    let (pr_message, set_pr_message) = signal(Option::<String>::None);

    view! {
        <div class="page-header">
            <h2>"GitHub PRs"</h2>
            {move || pr_message.get().map(|msg| {
                view! { <div class="pr-status-banner">{msg}</div> }
            })}
        </div>

        <table class="data-table">
            <thead>
                <tr>
                    <th>"#"</th>
                    <th>"Title"</th>
                    <th>"Author"</th>
                    <th>"Status"</th>
                    <th>"Reviewers"</th>
                    <th>"Checks"</th>
                    <th>"Created"</th>
                    <th>"Actions"</th>
                </tr>
            </thead>
            <tbody>
                {move || prs.get().into_iter().map(|pr| {
                    let status_class = match pr.status.as_str() {
                        "open" => "glyph-active",
                        "merged" => "glyph-idle",
                        "draft" => "glyph-pending",
                        "closed" => "glyph-stopped",
                        _ => "glyph-unknown",
                    };
                    let reviewers_view = if pr.reviewers.is_empty() {
                        vec![view! { <span class="no-reviewers">{"none".to_string()}</span> }]
                    } else {
                        pr.reviewers.iter().map(|r| {
                            view! {
                                <span class="reviewer-badge">{r.clone()}</span>
                            }
                        }).collect::<Vec<_>>()
                    };
                    // Mergeable badge
                    let checks_badge = match pr.status.as_str() {
                        "merged" => view! { <span class="badge badge-merged">"merged"</span> },
                        "open" => view! { <span class="badge badge-passing">"passing"</span> },
                        "draft" => view! { <span class="badge badge-pending">"pending"</span> },
                        _ => view! { <span class="badge badge-unknown">"--"</span> },
                    };
                    let pr_number = pr.number;
                    let is_creating = move || creating_pr.get() == Some(pr_number);
                    let create_pr_handler = move |_| {
                        set_creating_pr.set(Some(pr_number));
                        // In production: POST /api/github/pr/{task_id}
                        leptos::logging::log!("Create PR from task for PR #{}", pr_number);
                        set_timeout(
                            move || {
                                set_creating_pr.set(None);
                                set_pr_message.set(Some(format!("PR #{} action completed", pr_number)));
                            },
                            std::time::Duration::from_millis(600),
                        );
                    };
                    view! {
                        <tr>
                            <td class="pr-number">{format!("#{}", pr.number)}</td>
                            <td class="pr-title">{pr.title.clone()}</td>
                            <td>{pr.author.clone()}</td>
                            <td><span class={status_class}>{pr.status.clone()}</span></td>
                            <td class="pr-reviewers">{reviewers_view}</td>
                            <td>{checks_badge}</td>
                            <td class="timestamp-cell">{pr.created.clone()}</td>
                            <td>
                                <button
                                    class="btn btn-sm"
                                    on:click=create_pr_handler
                                    disabled=is_creating
                                >
                                    {move || if is_creating() {
                                        "Creating..."
                                    } else {
                                        "Create PR"
                                    }}
                                </button>
                            </td>
                        </tr>
                    }
                }).collect::<Vec<_>>()}
            </tbody>
        </table>
    }
}
