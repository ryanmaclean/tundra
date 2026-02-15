use leptos::prelude::*;

use crate::state::use_app_state;

#[component]
pub fn GithubIssuesPage() -> impl IntoView {
    let state = use_app_state();
    let issues = state.github_issues;

    view! {
        <div class="page-header">
            <h2>"GitHub Issues"</h2>
        </div>

        <table class="data-table">
            <thead>
                <tr>
                    <th>"#"</th>
                    <th>"Title"</th>
                    <th>"Labels"</th>
                    <th>"Assignee"</th>
                    <th>"State"</th>
                    <th>"Created"</th>
                </tr>
            </thead>
            <tbody>
                {move || issues.get().into_iter().map(|issue| {
                    let labels_view = issue.labels.iter().map(|label| {
                        let label_class = match label.as_str() {
                            "bug" => "tag tag-stuck",
                            "critical" => "tag tag-high",
                            "enhancement" => "tag tag-feature",
                            "docs" => "tag tag-refactor",
                            _ => "tag",
                        };
                        view! {
                            <span class={label_class}>{label.clone()}</span>
                        }
                    }).collect::<Vec<_>>();
                    let state_class = match issue.state.as_str() {
                        "open" => "glyph-active",
                        "closed" => "glyph-stopped",
                        _ => "glyph-unknown",
                    };
                    let assignee_text = issue.assignee.clone().unwrap_or_else(|| "unassigned".into());
                    view! {
                        <tr>
                            <td class="issue-number">{format!("#{}", issue.number)}</td>
                            <td class="issue-title">{issue.title.clone()}</td>
                            <td class="issue-labels">{labels_view}</td>
                            <td>{assignee_text}</td>
                            <td><span class={state_class}>{issue.state.clone()}</span></td>
                            <td class="timestamp-cell">{issue.created.clone()}</td>
                        </tr>
                    }
                }).collect::<Vec<_>>()}
            </tbody>
        </table>
    }
}
