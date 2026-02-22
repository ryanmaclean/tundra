use leptos::prelude::*;
use crate::themed::{themed, Prompt};

use crate::api;
use crate::i18n::t;
use crate::state::use_app_state;
use crate::types::GithubPr;

#[component]
pub fn GithubPrsPage() -> impl IntoView {
    let app_state = use_app_state();
    let display_mode = app_state.display_mode;
    let state = use_app_state();
    let prs = state.github_prs;
    let set_prs = state.set_github_prs;

    // PR creation state
    let (pr_message, set_pr_message) = signal(Option::<String>::None);

    // Search and filter
    let (search_query, set_search_query) = signal(String::new());
    let (active_sub_tab, set_active_sub_tab) = signal("prs".to_string());

    // Selected PR for detail pane
    let (selected_pr, set_selected_pr) = signal(Option::<u32>::None);

    // Claude Code dropdown state
    let (claude_dropdown_pr, set_claude_dropdown_pr) = signal(Option::<u32>::None);

    // Releases data
    let (releases, set_releases) = signal(Vec::<api::ApiGithubRelease>::new());
    // GitLab MR data
    let (gitlab_mrs, set_gitlab_mrs) = signal(Vec::<api::ApiGitLabMergeRequest>::new());
    let (gitlab_project_id, set_gitlab_project_id) = signal(String::new());
    let (gitlab_state, set_gitlab_state) = signal("opened".to_string());

    // Fetch PRs from API on mount
    {
        let set_prs = set_prs.clone();
        leptos::task::spawn_local(async move {
            match crate::api::fetch_github_prs().await {
                Ok(api_prs) => {
                    let ui_prs: Vec<GithubPr> = api_prs
                        .iter()
                        .map(|p| GithubPr {
                            number: p.number,
                            title: p.title.clone(),
                            author: p.author.clone(),
                            status: p.state.clone().unwrap_or_else(|| p.status.clone()),
                            reviewers: p.reviewers.clone(),
                            created: p.created_at.clone().unwrap_or_else(|| p.created.clone()),
                        })
                        .collect();
                    if !ui_prs.is_empty() {
                        set_prs.set(ui_prs);
                    }
                }
                Err(e) => {
                    leptos::logging::log!("Failed to fetch GitHub PRs from API: {}", e);
                }
            }
        });
    }

    let open_count = move || {
        prs.get().iter().filter(|p| p.status == "open" || p.status == "draft").count()
    };

    let filtered_prs = move || {
        let query = search_query.get().to_lowercase();
        prs.get().into_iter()
            .filter(|p| {
                if !query.is_empty() && !p.title.to_lowercase().contains(&query)
                    && !p.author.to_lowercase().contains(&query) {
                    return false;
                }
                true
            })
            .collect::<Vec<_>>()
    };

    let selected_pr_data = move || {
        selected_pr.get().and_then(|num| {
            prs.get().into_iter().find(|p| p.number == num)
        })
    };

    view! {
        <div class="page-header github-prs-header">
            <div class="page-header-left">
                <h2>{t("github-prs-title")}</h2>
                <span class="repo-name">"auto-tundra/rust-harness"</span>
                <span class="issue-count-badge">{move || format!("{} open", open_count())}</span>
            </div>
            <div class="page-header-right">
                <button class="btn btn-sm btn-outline" on:click=move |_| {
                    // Refresh
                    let set_prs = set_prs.clone();
                    leptos::task::spawn_local(async move {
                        if let Ok(api_prs) = crate::api::fetch_github_prs().await {
                            let ui_prs: Vec<GithubPr> = api_prs
                                .iter()
                                .map(|p| GithubPr {
                                    number: p.number,
                                    title: p.title.clone(),
                                    author: p.author.clone(),
                                    status: p.state.clone().unwrap_or_else(|| p.status.clone()),
                                    reviewers: p.reviewers.clone(),
                                    created: p.created_at.clone().unwrap_or_else(|| p.created.clone()),
                                })
                                .collect();
                            set_prs.set(ui_prs);
                        }
                    });
                }>
                    "Refresh"
                </button>
            </div>
        </div>

        {move || pr_message.get().map(|msg| {
            view! { <div class="pr-status-banner">{msg}</div> }
        })}

        // Sub-tabs: PRs | Contributors | All Releases
        <div class="prs-controls">
            <div class="prs-search-wrapper">
                <input
                    type="text"
                    class="prs-search-input"
                    placeholder="Search PRs..."
                    prop:value=move || search_query.get()
                    on:input=move |ev| set_search_query.set(event_target_value(&ev))
                />
            </div>
            <div class="prs-sub-tabs">
                <button
                    class=move || if active_sub_tab.get() == "prs" { "prs-sub-tab active" } else { "prs-sub-tab" }
                    on:click=move |_| set_active_sub_tab.set("prs".to_string())
                >"Pull Requests"</button>
                <button
                    class=move || if active_sub_tab.get() == "contributors" { "prs-sub-tab active" } else { "prs-sub-tab" }
                    on:click=move |_| set_active_sub_tab.set("contributors".to_string())
                >"Contributors"</button>
                <button
                    class=move || if active_sub_tab.get() == "releases" { "prs-sub-tab active" } else { "prs-sub-tab" }
                    on:click=move |_| {
                        set_active_sub_tab.set("releases".to_string());
                        let set_releases = set_releases;
                        leptos::task::spawn_local(async move {
                            match api::fetch_github_releases().await {
                                Ok(data) => set_releases.set(data),
                                Err(e) => leptos::logging::log!("Failed to fetch releases: {}", e),
                            }
                        });
                    }
                >"All Releases"</button>
                <button
                    class=move || if active_sub_tab.get() == "gitlab" { "prs-sub-tab active" } else { "prs-sub-tab" }
                    on:click=move |_| {
                        set_active_sub_tab.set("gitlab".to_string());
                        let set_gitlab_mrs = set_gitlab_mrs;
                        let set_pr_message = set_pr_message;
                        let pid = gitlab_project_id.get();
                        let state = gitlab_state.get();
                        leptos::task::spawn_local(async move {
                            let pid_ref = if pid.trim().is_empty() { None } else { Some(pid.trim()) };
                            let state_ref = if state.trim().is_empty() { None } else { Some(state.trim()) };
                            match api::fetch_gitlab_merge_requests(pid_ref, state_ref).await {
                                Ok(data) => {
                                    set_gitlab_mrs.set(data);
                                    set_pr_message.set(None);
                                }
                                Err(e) => {
                                    set_pr_message.set(Some(format!("Failed to fetch GitLab MRs: {}", e)));
                                }
                            }
                        });
                    }
                >"GitLab MRs"</button>
            </div>
        </div>

        // Contributors sub-tab content
        {move || (active_sub_tab.get() == "contributors").then(|| {
            let all_prs = prs.get();
            let mut authors: Vec<String> = all_prs.iter().map(|p| p.author.clone()).collect();
            authors.sort();
            authors.dedup();
            view! {
                <div class="prs-contributors-list">
                    <h3>"Contributors"</h3>
                    {authors.into_iter().map(|author| {
                        let pr_count = all_prs.iter().filter(|p| p.author == author).count();
                        let initial = author.chars().next().unwrap_or('?').to_uppercase().to_string();
                        view! {
                            <div class="contributor-row" style="display: flex; align-items: center; gap: 8px; padding: 8px 0; border-bottom: 1px solid var(--border);">
                                <span class="pr-avatar">{initial}</span>
                                <span style="flex: 1;">{author}</span>
                                <span class="issue-count-badge">{format!("{} PRs", pr_count)}</span>
                            </div>
                        }
                    }).collect::<Vec<_>>()}
                </div>
            }
        })}

        // Releases sub-tab content
        {move || (active_sub_tab.get() == "releases").then(|| {
            let rels = releases.get();
            if rels.is_empty() {
                view! {
                    <div class="issues-empty-state">
                        <div class="placeholder-icon">"--"</div>
                        <p>{move || themed(display_mode.get(), Prompt::EmptyKpi)}</p>
                    </div>
                }.into_any()
            } else {
                view! {
                    <div class="prs-releases-list">
                        {rels.into_iter().map(|rel| {
                            view! {
                                <div class="release-row" style="padding: 12px 0; border-bottom: 1px solid var(--border);">
                                    <div style="display: flex; align-items: center; gap: 8px;">
                                        <span class="issue-count-badge">{rel.tag_name.clone()}</span>
                                        <span style="font-weight: 500;">{rel.name.clone()}</span>
                                    </div>
                                </div>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                }.into_any()
            }
        })}

        // GitLab Merge Requests sub-tab content
        {move || (active_sub_tab.get() == "gitlab").then(|| {
            let mrs = gitlab_mrs.get();
            view! {
                <div class="prs-releases-list">
                    <div class="prs-controls">
                        <div class="prs-search-wrapper">
                            <input
                                type="text"
                                class="prs-search-input"
                                placeholder="GitLab project ID (optional)"
                                prop:value=move || gitlab_project_id.get()
                                on:input=move |ev| set_gitlab_project_id.set(event_target_value(&ev))
                            />
                        </div>
                        <div class="prs-search-wrapper">
                            <input
                                type="text"
                                class="prs-search-input"
                                placeholder="State (opened, closed, merged)"
                                prop:value=move || gitlab_state.get()
                                on:input=move |ev| set_gitlab_state.set(event_target_value(&ev))
                            />
                        </div>
                        <button
                            class="btn btn-sm btn-outline"
                            on:click=move |_| {
                                let set_gitlab_mrs = set_gitlab_mrs;
                                let set_pr_message = set_pr_message;
                                let pid = gitlab_project_id.get();
                                let state = gitlab_state.get();
                                leptos::task::spawn_local(async move {
                                    let pid_ref = if pid.trim().is_empty() { None } else { Some(pid.trim()) };
                                    let state_ref = if state.trim().is_empty() { None } else { Some(state.trim()) };
                                    match api::fetch_gitlab_merge_requests(pid_ref, state_ref).await {
                                        Ok(data) => {
                                            set_gitlab_mrs.set(data);
                                            set_pr_message.set(None);
                                        }
                                        Err(e) => {
                                            set_pr_message.set(Some(format!("Failed to fetch GitLab MRs: {}", e)));
                                        }
                                    }
                                });
                            }
                        >
                            "Refresh GitLab MRs"
                        </button>
                    </div>

                    {if mrs.is_empty() {
                        view! {
                            <div class="issues-empty-state">
                                <div class="placeholder-icon">"--"</div>
                                <p>"No merge requests loaded. Set project ID (if needed) and refresh."</p>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            {mrs.into_iter().map(|mr| {
                                let iid = mr.iid;
                                let title = mr.title.clone();
                                let author = if mr.author.name.is_empty() {
                                    mr.author.username.clone()
                                } else {
                                    mr.author.name.clone()
                                };
                                let state = mr.state.clone();
                                let created = mr.created_at.clone();
                                let set_pr_message = set_pr_message;
                                view! {
                                    <div class="pr-list-item">
                                        <div class="pr-list-item-main">
                                            <div class="pr-list-item-header">
                                                <span class="pr-number">{format!("!{}", iid)}</span>
                                                <span class="pr-title-text">{title}</span>
                                            </div>
                                            <div class="pr-list-item-meta">
                                                <span class="pr-status-badge pr-status-open">{state}</span>
                                                <span class="pr-author">{author}</span>
                                                <span class="pr-created">{created}</span>
                                            </div>
                                        </div>
                                        <div class="pr-list-item-actions">
                                            <button
                                                class="btn btn-xs btn-claude-code"
                                                on:click=move |_| {
                                                    let set_pr_message = set_pr_message;
                                                    let pid = gitlab_project_id.get();
                                                    leptos::task::spawn_local(async move {
                                                        set_pr_message.set(Some(format!("Reviewing GitLab MR !{}...", iid)));
                                                        let project_id = if pid.trim().is_empty() { None } else { Some(pid.trim()) };
                                                        match api::review_gitlab_merge_request(iid, project_id, Some(false), Some("medium")).await {
                                                            Ok(result) => {
                                                                set_pr_message.set(Some(format!(
                                                                    "GitLab MR !{} reviewed: {} findings, approved={}",
                                                                    iid,
                                                                    result.findings.len(),
                                                                    result.approved
                                                                )));
                                                            }
                                                            Err(e) => {
                                                                set_pr_message.set(Some(format!("GitLab MR !{} review failed: {}", iid, e)));
                                                            }
                                                        }
                                                    });
                                                }
                                            >
                                                "Review MR"
                                            </button>
                                        </div>
                                    </div>
                                }
                            }).collect::<Vec<_>>()}
                        }.into_any()
                    }}
                </div>
            }.into_any()
        })}

        // Two-pane layout (only shown on PRs tab)
        <div class="prs-two-pane" style=move || if active_sub_tab.get() == "prs" { "" } else { "display: none;" }>
            // Left pane: PR list
            <div class="prs-list-pane">
                {move || filtered_prs().is_empty().then(|| view! {
                    <div class="issues-empty-state">
                        <div class="placeholder-icon">"--"</div>
                        <p>{move || themed(display_mode.get(), Prompt::EmptyKpi)}</p>
                    </div>
                })}
                {move || {
                    let fp = filtered_prs();
                    if !fp.is_empty() {
                        fp.into_iter().map(|pr| {
                            let pr_number = pr.number;
                            let is_selected = move || selected_pr.get() == Some(pr_number);
                            let has_dropdown = move || claude_dropdown_pr.get() == Some(pr_number);

                            let status_class = match pr.status.as_str() {
                                "open" => "pr-status-open",
                                "merged" => "pr-status-merged",
                                "draft" => "pr-status-draft",
                                "closed" => "pr-status-closed",
                                _ => "pr-status-unknown",
                            };
                            let status_label = match pr.status.as_str() {
                                "open" => "Open",
                                "merged" => "Merged",
                                "draft" => "Draft",
                                "closed" => "Closed",
                                _ => "Unknown",
                            };

                            let author = pr.author.clone();
                            let title = pr.title.clone();
                            let created = pr.created.clone();
                            let reviewer_count = pr.reviewers.len();
                            let comment_count = (pr_number % 9 + 1) as usize;
                            let additions = (pr_number % 12 + 1) as usize;
                            let deletions = (pr_number % 5) as usize;
                            let branch = format!("auto-claude/{:03}-{}", pr_number, title.to_lowercase().replace(' ', "-").chars().take(28).collect::<String>());
                            let age_label = if created.len() >= 10 {
                                created[..10].to_string()
                            } else {
                                created.clone()
                            };

                            // Author avatar initial
                            let avatar_initial = author.chars().next().unwrap_or('?').to_uppercase().to_string();

                            view! {
                                <div
                                    class=move || if is_selected() { "pr-list-item selected" } else { "pr-list-item" }
                                    on:click=move |_| {
                                        set_selected_pr.set(Some(pr_number));
                                        set_claude_dropdown_pr.set(None);
                                    }
                                >
                                    <div class="pr-list-item-main">
                                        <div class="pr-list-item-header">
                                            <span class="pr-avatar">{avatar_initial.clone()}</span>
                                            <span class="pr-number">{format!("#{}", pr_number)}</span>
                                            <span class="pr-title-text">{title}</span>
                                            <span class="pr-branch-pill">{branch}</span>
                                            <span class="pr-meta-ghost-menu">"\u{22EF}"</span>
                                        </div>
                                        <div class="pr-list-item-meta">
                                            <span class={format!("pr-status-badge {}", status_class)}>{status_label}</span>
                                            <span class="pr-author">{author}</span>
                                            <span class="pr-comments">{format!("\u{1F5E8} {}", comment_count)}</span>
                                            <span class="pr-comments">{format!("\u{1F50D} {} reviews", reviewer_count)}</span>
                                            <span class="pr-comments pr-delta-pos">{format!("+{}", additions)}</span>
                                            <span class="pr-comments pr-delta-neg">{format!("-{}", deletions)}</span>
                                            <span class="pr-created">{age_label}</span>
                                        </div>
                                    </div>
                                    <div class="pr-list-item-actions">
                                        <button
                                            class="btn btn-xs btn-claude-code"
                                            on:click=move |ev: web_sys::MouseEvent| {
                                                ev.stop_propagation();
                                                if claude_dropdown_pr.get() == Some(pr_number) {
                                                    set_claude_dropdown_pr.set(None);
                                                } else {
                                                    set_claude_dropdown_pr.set(Some(pr_number));
                                                }
                                            }
                                        >
                                            "Claude Code"
                                        </button>
                                        // Claude Code dropdown
                                        {move || has_dropdown().then(|| view! {
                                            <div class="claude-code-dropdown">
                                                <div class="claude-code-dropdown-item" on:click=move |ev: web_sys::MouseEvent| {
                                                    ev.stop_propagation();
                                                    set_pr_message.set(Some(format!("Checking out branch for PR #{}...", pr_number)));
                                                    set_claude_dropdown_pr.set(None);
                                                    leptos::task::spawn_local(async move {
                                                        match api::checkout_pr_branch(pr_number as u64).await {
                                                            Ok(_) => set_pr_message.set(Some(format!("Branch for PR #{} checked out successfully", pr_number))),
                                                            Err(e) => set_pr_message.set(Some(format!("Failed to checkout PR #{}: {}", pr_number, e))),
                                                        }
                                                    });
                                                }>
                                                    "Checkout Branch"
                                                </div>
                                                <div class="claude-code-dropdown-item" on:click=move |ev: web_sys::MouseEvent| {
                                                    ev.stop_propagation();
                                                    set_pr_message.set(Some(format!("Starting Claude Code review on PR #{}...", pr_number)));
                                                    set_claude_dropdown_pr.set(None);
                                                    leptos::task::spawn_local(async move {
                                                        match api::review_pr(pr_number as u64).await {
                                                            Ok(_) => set_pr_message.set(Some(format!("Review started for PR #{}", pr_number))),
                                                            Err(e) => set_pr_message.set(Some(format!("Failed to start review for PR #{}: {}", pr_number, e))),
                                                        }
                                                    });
                                                }>
                                                    "Review with Claude Code"
                                                </div>
                                                <div class="claude-code-dropdown-item" on:click=move |ev: web_sys::MouseEvent| {
                                                    ev.stop_propagation();
                                                    set_pr_message.set(Some(format!("Merging PR #{}...", pr_number)));
                                                    set_claude_dropdown_pr.set(None);
                                                    leptos::task::spawn_local(async move {
                                                        match api::merge_pr(pr_number as u64).await {
                                                            Ok(_) => set_pr_message.set(Some(format!("PR #{} merged successfully", pr_number))),
                                                            Err(e) => set_pr_message.set(Some(format!("Failed to merge PR #{}: {}", pr_number, e))),
                                                        }
                                                    });
                                                }>
                                                    "Merge PR"
                                                </div>
                                                <div class="claude-code-dropdown-item" on:click=move |ev: web_sys::MouseEvent| {
                                                    ev.stop_propagation();
                                                    set_claude_dropdown_pr.set(None);
                                                    set_pr_message.set(Some("Open the Changelog page from the sidebar navigation".to_string()));
                                                }>
                                                    "View Claude Code Changelog"
                                                </div>
                                            </div>
                                        })}
                                    </div>
                                </div>
                            }
                        }).collect::<Vec<_>>()
                    } else {
                        Vec::new()
                    }
                }}
            </div>

            // Right pane: PR detail
            <div class="prs-detail-pane">
                {move || match selected_pr_data() {
                    Some(pr) => {
                        let reviewers = if pr.reviewers.is_empty() {
                            vec![view! { <span class="no-reviewers">{"none".to_string()}</span> }]
                        } else {
                            pr.reviewers.iter().map(|r| {
                                view! { <span class="reviewer-badge">{r.clone()}</span> }
                            }).collect::<Vec<_>>()
                        };
                        let status_cls = match pr.status.as_str() {
                            "open" => "pr-status-badge pr-status-open",
                            "merged" => "pr-status-badge pr-status-merged",
                            "draft" => "pr-status-badge pr-status-draft",
                            _ => "pr-status-badge pr-status-unknown",
                        };
                        view! {
                            <div class="pr-detail-content">
                                <h3>{format!("#{} {}", pr.number, pr.title)}</h3>
                                <div class="pr-detail-meta">
                                    <div class="issue-detail-row">
                                        <span class="meta-label">"Status"</span>
                                        <span class={status_cls}>{pr.status.clone()}</span>
                                    </div>
                                    <div class="issue-detail-row">
                                        <span class="meta-label">"Author"</span>
                                        <span>{pr.author.clone()}</span>
                                    </div>
                                    <div class="issue-detail-row">
                                        <span class="meta-label">"Reviewers"</span>
                                        <div class="pr-detail-reviewers">{reviewers}</div>
                                    </div>
                                    <div class="issue-detail-row">
                                        <span class="meta-label">"Created"</span>
                                        <span>{pr.created.clone()}</span>
                                    </div>
                                </div>
                            </div>
                        }.into_any()
                    }
                    None => view! {
                        <div class="issues-empty-state">
                            <div class="placeholder-icon">"--"</div>
                            <p>"Select a pull request to view details"</p>
                        </div>
                    }.into_any(),
                }}
            </div>
        </div>
    }
}
