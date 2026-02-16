use leptos::prelude::*;

use crate::state::use_app_state;

#[component]
pub fn GithubIssuesPage() -> impl IntoView {
    let state = use_app_state();
    let issues = state.github_issues;

    // Sync state signals
    let (is_syncing, set_is_syncing) = signal(false);
    let (last_synced, set_last_synced) = signal(Option::<String>::None);
    let (sync_count, set_sync_count) = signal(0u64);

    // Trigger sync handler
    let trigger_sync = move |_| {
        set_is_syncing.set(true);
        // In production this would call POST /api/github/sync via fetch.
        // For the demo UI, simulate a short sync delay.
        set_timeout(
            move || {
                set_is_syncing.set(false);
                set_last_synced.set(Some("just now".to_string()));
                set_sync_count.set(sync_count.get() + 1);
            },
            std::time::Duration::from_millis(800),
        );
    };

    view! {
        <div class="page-header">
            <h2>"GitHub Issues"</h2>
            <div class="sync-controls">
                <button
                    class="btn btn-primary"
                    on:click=trigger_sync
                    disabled=move || is_syncing.get()
                >
                    {move || if is_syncing.get() {
                        "Syncing..."
                    } else {
                        "Sync Now"
                    }}
                </button>
                <span class="sync-status">
                    {move || match last_synced.get() {
                        Some(t) => format!("Last synced: {} ({} items)", t, sync_count.get()),
                        None => "Not synced yet".to_string(),
                    }}
                </span>
                {move || if is_syncing.get() {
                    Some(view! { <span class="sync-spinner">"[syncing]"</span> })
                } else {
                    None
                }}
            </div>
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
                    <th>"Actions"</th>
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
                    let issue_number = issue.number;
                    let import_handler = move |_| {
                        // In production: POST /api/github/issues/{issue_number}/import
                        leptos::logging::log!("Import issue #{} as task", issue_number);
                    };
                    view! {
                        <tr>
                            <td class="issue-number">{format!("#{}", issue.number)}</td>
                            <td class="issue-title">{issue.title.clone()}</td>
                            <td class="issue-labels">{labels_view}</td>
                            <td>{assignee_text}</td>
                            <td><span class={state_class}>{issue.state.clone()}</span></td>
                            <td class="timestamp-cell">{issue.created.clone()}</td>
                            <td>
                                <button
                                    class="btn btn-sm"
                                    on:click=import_handler
                                >
                                    "Import as Task"
                                </button>
                            </td>
                        </tr>
                    }
                }).collect::<Vec<_>>()}
            </tbody>
        </table>
    }
}
