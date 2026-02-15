use leptos::prelude::*;

use crate::state::use_app_state;

#[component]
pub fn GithubPrsPage() -> impl IntoView {
    let state = use_app_state();
    let prs = state.github_prs;

    view! {
        <div class="page-header">
            <h2>"GitHub PRs"</h2>
        </div>

        <table class="data-table">
            <thead>
                <tr>
                    <th>"#"</th>
                    <th>"Title"</th>
                    <th>"Author"</th>
                    <th>"Status"</th>
                    <th>"Reviewers"</th>
                    <th>"Created"</th>
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
                    view! {
                        <tr>
                            <td class="pr-number">{format!("#{}", pr.number)}</td>
                            <td class="pr-title">{pr.title.clone()}</td>
                            <td>{pr.author.clone()}</td>
                            <td><span class={status_class}>{pr.status.clone()}</span></td>
                            <td class="pr-reviewers">{reviewers_view}</td>
                            <td class="timestamp-cell">{pr.created.clone()}</td>
                        </tr>
                    }
                }).collect::<Vec<_>>()}
            </tbody>
        </table>
    }
}
