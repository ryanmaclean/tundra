use leptos::ev::{KeyboardEvent, MouseEvent};
use leptos::prelude::*;
use leptos::task::spawn_local;
#[cfg(feature = "markdown")]
use pulldown_cmark::{html, Parser};
use web_sys;

use crate::components::focus_trap::use_focus_trap;
use crate::i18n::{t, t_str};
use crate::state::use_app_state;
use crate::types::{BeadResponse, BeadStatus, Lane};

fn progress_percent(stage: &str) -> u32 {
    match stage {
        "plan" => 25,
        "code" => 50,
        "qa" => 75,
        "done" => 100,
        _ => 0,
    }
}

fn status_badge_class(status: &BeadStatus) -> &'static str {
    match status {
        BeadStatus::Planning => "td-phase-badge td-phase-planning",
        BeadStatus::InProgress => "td-phase-badge td-phase-in-progress",
        BeadStatus::AiReview => "td-phase-badge td-phase-review",
        BeadStatus::HumanReview => "td-phase-badge td-phase-review",
        BeadStatus::Done => "td-phase-badge td-phase-done",
        BeadStatus::Failed => "td-phase-badge td-phase-failed",
    }
}

fn category_label(tags: &[String]) -> String {
    let skip = [
        "Critical",
        "High",
        "Medium",
        "Low",
        "Stuck",
        "Needs Recovery",
        "PR Created",
        "Incomplete",
        "Needs Resume",
    ];
    for tag in tags {
        if !skip.contains(&tag.as_str()) {
            return tag.clone();
        }
    }
    "Feature".to_string()
}

/// Generate a branch name from the task title
fn branch_name(title: &str) -> String {
    let slug: String = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let trimmed = slug.trim_matches('-').to_string();
    let short = if trimmed.len() > 40 {
        &trimmed[..40]
    } else {
        &trimmed
    };
    format!("feat/{}", short.trim_end_matches('-'))
}

fn execution_log(bead: &BeadResponse) -> Vec<(String, String, &'static str)> {
    let mut logs = vec![(
        "Task created".to_string(),
        bead.timestamp.clone(),
        "log-info",
    )];
    match bead.status {
        BeadStatus::Planning => {
            logs.push((
                "Planning phase started".to_string(),
                bead.timestamp.clone(),
                "log-info",
            ));
        }
        BeadStatus::InProgress => {
            logs.push((
                "Planning completed".to_string(),
                "earlier".to_string(),
                "log-success",
            ));
            logs.push((
                "Implementation started".to_string(),
                bead.timestamp.clone(),
                "log-info",
            ));
            if !bead.agent_names.is_empty() {
                logs.push((
                    format!("Assigned to {}", bead.agent_names.join(", ")),
                    bead.timestamp.clone(),
                    "log-info",
                ));
            }
        }
        BeadStatus::AiReview => {
            logs.push((
                "Planning completed".to_string(),
                "earlier".to_string(),
                "log-success",
            ));
            logs.push((
                "Implementation completed".to_string(),
                "earlier".to_string(),
                "log-success",
            ));
            logs.push((
                "AI review started".to_string(),
                bead.timestamp.clone(),
                "log-info",
            ));
        }
        BeadStatus::HumanReview => {
            logs.push((
                "Planning completed".to_string(),
                "earlier".to_string(),
                "log-success",
            ));
            logs.push((
                "Implementation completed".to_string(),
                "earlier".to_string(),
                "log-success",
            ));
            logs.push((
                "AI review passed".to_string(),
                "earlier".to_string(),
                "log-success",
            ));
            logs.push((
                "Awaiting human review".to_string(),
                bead.timestamp.clone(),
                "log-warning",
            ));
        }
        BeadStatus::Done => {
            logs.push((
                "Planning completed".to_string(),
                "earlier".to_string(),
                "log-success",
            ));
            logs.push((
                "Implementation completed".to_string(),
                "earlier".to_string(),
                "log-success",
            ));
            logs.push((
                "Review passed".to_string(),
                "earlier".to_string(),
                "log-success",
            ));
            logs.push((
                "Task completed".to_string(),
                bead.timestamp.clone(),
                "log-success",
            ));
        }
        BeadStatus::Failed => {
            logs.push((
                "Task failed".to_string(),
                bead.timestamp.clone(),
                "log-error",
            ));
        }
    }
    logs
}

/// Generate demo subtasks based on bead state
fn demo_subtasks(bead: &BeadResponse) -> Vec<(String, bool)> {
    let base_tasks = vec![
        format!("Create spec for: {}", bead.title),
        "Define acceptance criteria".to_string(),
        "Implement core logic".to_string(),
        "Write unit tests".to_string(),
        "Integration testing".to_string(),
        "Code review".to_string(),
        "Documentation update".to_string(),
    ];
    let completed_count = match bead.status {
        BeadStatus::Planning => 0,
        BeadStatus::InProgress => 2,
        BeadStatus::AiReview => 4,
        BeadStatus::HumanReview => 5,
        BeadStatus::Done => 7,
        BeadStatus::Failed => 1,
    };
    base_tasks
        .into_iter()
        .enumerate()
        .map(|(i, t)| (t, i < completed_count))
        .collect()
}

/// Generate demo file tree based on bead
fn demo_file_tree(bead: &BeadResponse) -> Vec<(String, &'static str)> {
    let slug = bead
        .title
        .to_lowercase()
        .replace(' ', "_")
        .chars()
        .take(20)
        .collect::<String>();
    vec![
        (format!("src/{}.rs", slug), "modified"),
        (format!("src/{}_test.rs", slug), "added"),
        ("src/lib.rs".to_string(), "modified"),
        ("Cargo.toml".to_string(), "modified"),
    ]
}

/// Generate mock diff content for a file
fn mock_diff_for_file(path: &str, change_type: &str) -> Vec<(String, &'static str)> {
    match change_type {
        "added" => vec![
            ("@@ -0,0 +1,15 @@".to_string(), "diff-context"),
            ("+// Auto-generated test file".to_string(), "diff-add"),
            ("+use super::*;".to_string(), "diff-add"),
            ("+".to_string(), "diff-add"),
            ("+#[cfg(test)]".to_string(), "diff-add"),
            ("+mod tests {".to_string(), "diff-add"),
            ("+    use super::*;".to_string(), "diff-add"),
            ("+".to_string(), "diff-add"),
            ("+    #[test]".to_string(), "diff-add"),
            ("+    fn test_basic() {".to_string(), "diff-add"),
            ("+        assert!(true);".to_string(), "diff-add"),
            ("+    }".to_string(), "diff-add"),
            ("+}".to_string(), "diff-add"),
        ],
        "modified" => {
            let mod_name = path.split('/').next_back().unwrap_or("file");
            vec![
                (
                    format!("@@ -10,6 +10,12 @@ // {}", mod_name),
                    "diff-context",
                ),
                (
                    " use std::collections::HashMap;".to_string(),
                    "diff-context",
                ),
                (" ".to_string(), "diff-context"),
                ("-fn old_function() -> bool {".to_string(), "diff-remove"),
                ("-    false".to_string(), "diff-remove"),
                (
                    "+fn improved_function() -> Result<bool, Error> {".to_string(),
                    "diff-add",
                ),
                ("+    // Enhanced implementation".to_string(), "diff-add"),
                ("+    Ok(true)".to_string(), "diff-add"),
                (" }".to_string(), "diff-context"),
                (" ".to_string(), "diff-context"),
                ("+/// New helper function".to_string(), "diff-add"),
                ("+fn helper() -> String {".to_string(), "diff-add"),
                ("+    String::from(\"helper\")".to_string(), "diff-add"),
                ("+}".to_string(), "diff-add"),
            ]
        }
        "deleted" => vec![
            ("@@ -1,5 +0,0 @@".to_string(), "diff-context"),
            ("-// Deprecated module".to_string(), "diff-remove"),
            ("-fn deprecated() {}".to_string(), "diff-remove"),
        ],
        _ => vec![],
    }
}

/// Generate mock QA checks based on bead state
fn mock_qa_checks(bead: &BeadResponse) -> (bool, Vec<(String, bool, String)>, Vec<String>) {
    let is_qa_fail = bead.tags.iter().any(|t| t == "QA Failed");
    let checks = vec![
        (
            "Unit Tests".to_string(),
            !is_qa_fail,
            if is_qa_fail {
                "2 tests failed in module::tests".to_string()
            } else {
                "All 24 tests passed".to_string()
            },
        ),
        (
            "Integration Tests".to_string(),
            true,
            "12 integration tests passed".to_string(),
        ),
        (
            "Clippy Lints".to_string(),
            !is_qa_fail,
            if is_qa_fail {
                "3 warnings found".to_string()
            } else {
                "No warnings".to_string()
            },
        ),
        (
            "Format Check".to_string(),
            true,
            "All files formatted".to_string(),
        ),
        (
            "Security Audit".to_string(),
            true,
            "No vulnerabilities found".to_string(),
        ),
        (
            "Documentation".to_string(),
            true,
            "All public items documented".to_string(),
        ),
    ];
    let suggestions = if is_qa_fail {
        vec![
            "Fix failing tests in module::tests::test_edge_case".to_string(),
            "Address clippy warning: unnecessary clone in line 42".to_string(),
            "Consider adding error handling for the new helper function".to_string(),
        ]
    } else {
        vec![
            "Consider adding more edge case tests".to_string(),
            "Documentation coverage could be improved for private functions".to_string(),
        ]
    };
    let all_pass = checks.iter().all(|(_, p, _)| *p);
    (all_pass, checks, suggestions)
}

fn is_stuck(bead: &BeadResponse) -> bool {
    bead.tags
        .iter()
        .any(|t| t == "Stuck" || t == "Needs Recovery")
}

fn has_merge_conflict(bead: &BeadResponse) -> bool {
    bead.tags.iter().any(|t| t == "Merge Conflict")
}

fn has_rate_limit(bead: &BeadResponse) -> bool {
    bead.tags.iter().any(|t| t == "Rate Limited")
}

fn has_qa_failure(bead: &BeadResponse) -> bool {
    bead.tags.iter().any(|t| t == "QA Failed")
}

/// Demo build stats based on bead state
fn build_stats(bead: &BeadResponse) -> (u32, u32, u32, u32) {
    // (files_changed, commits, additions, deletions)
    match bead.status {
        BeadStatus::Planning => (0, 0, 0, 0),
        BeadStatus::InProgress => (4, 2, 186, 23),
        BeadStatus::AiReview => (8, 5, 342, 67),
        BeadStatus::HumanReview => (12, 8, 521, 94),
        BeadStatus::Done => (15, 12, 847, 156),
        BeadStatus::Failed => (3, 1, 45, 12),
    }
}

#[component]
fn TaskDetailInner(
    bead: BeadResponse,
    set_beads: WriteSignal<Vec<BeadResponse>>,
    on_close: impl Fn(MouseEvent) + Clone + 'static,
) -> impl IntoView {
    let title = bead.title.clone();
    let description = bead.description.clone();
    let status = bead.status;
    let status_display = format!("{}", bead.status);
    let s_cls = status_badge_class(&bead.status);
    let cat = category_label(&bead.tags);
    let _bid = bead.id.clone();
    let progress = progress_percent(&bead.progress_stage);
    let agents = bead.agent_names.clone();
    let logs = execution_log(&bead);
    let subtasks = demo_subtasks(&bead);
    let file_tree = demo_file_tree(&bead);
    let stuck = is_stuck(&bead);
    let merge_conflict = has_merge_conflict(&bead);
    let rate_limited = has_rate_limit(&bead);
    let qa_failed = has_qa_failure(&bead);
    let branch = branch_name(&bead.title);
    let (files_changed, commits, additions, deletions) = build_stats(&bead);

    let _completed_subtasks = subtasks.iter().filter(|(_, done)| *done).count();
    let total_subtasks = subtasks.len();

    // Tab state: 0=Overview, 1=Subtasks, 2=Logs, 3=Files, 4=History
    let (active_tab, set_active_tab) = signal(0u8);

    // Edit modal state
    let (show_edit, set_show_edit) = signal(false);

    // Diff dialog state
    let (show_diff, set_show_diff) = signal(false);
    let (diff_selected_file, set_diff_selected_file) = signal(0usize);

    // Assign agent state
    let (assigning, set_assigning) = signal(false);
    let (assign_msg, set_assign_msg) = signal(Option::<(bool, String)>::None);

    // Request changes state
    let (changes_text, set_changes_text) = signal(String::new());

    // Interactive subtask state
    let default_checked: std::collections::HashSet<usize> = subtasks
        .iter()
        .enumerate()
        .filter(|(_, (_, done))| *done)
        .map(|(i, _)| i)
        .collect();
    let (checked_subtasks, set_checked_subtasks) = signal(default_checked);

    // Discard dialog state
    let (show_discard, set_show_discard) = signal(false);
    let (discarding, set_discarding) = signal(false);

    // QA state
    let (qa_running, set_qa_running) = signal(false);
    let (qa_report, set_qa_report) = signal(mock_qa_checks(&bead));

    let show_delete = status == BeadStatus::Done || status == BeadStatus::Failed;
    let show_resume = status == BeadStatus::Failed || stuck;
    let show_build = status != BeadStatus::Planning;
    let has_pr = bead.tags.iter().any(|t| t == "PR Created");

    let id_delete = bead.id.clone();
    let id_resume = bead.id.clone();
    let id_discard = bead.id.clone();

    let close_delete = on_close.clone();
    let close_x = on_close.clone();
    let close_bottom = on_close.clone();
    let _close_discard = on_close.clone();

    let resume_action = move |_: MouseEvent| {
        let id = id_resume.clone();
        set_beads.update(|v| {
            if let Some(b) = v.iter_mut().find(|b| b.id == id) {
                b.status = BeadStatus::InProgress;
                b.lane = Lane::InProgress;
                b.progress_stage = "code".to_string();
                b.action = None;
                b.timestamp = "just now".to_string();
                b.tags.retain(|t| t != "Stuck" && t != "Needs Recovery");
            }
        });
    };

    let delete_action = move |ev: MouseEvent| {
        let id = id_delete.clone();
        spawn_local({
            let id = id.clone();
            async move {
                let _ = crate::api::delete_bead(&id).await;
            }
        });
        set_beads.update(|v| {
            v.retain(|b| b.id != id);
        });
        close_delete(ev);
    };

    let discard_action = {
        move |_ev: MouseEvent| {
            let id = id_discard.clone();
            set_discarding.set(true);
            spawn_local(async move {
                let _ = crate::api::delete_worktree(&id).await;
                set_beads.update(|v| {
                    v.retain(|b| b.id != id);
                });
                set_discarding.set(false);
                set_show_discard.set(false);
            });
        }
    };

    let rerun_qa = {
        let bid_qa = bead.id.clone();
        move |_: MouseEvent| {
            set_qa_running.set(true);
            let bid = bid_qa.clone();
            spawn_local(async move {
                match crate::api::run_task_qa(&bid).await {
                    Ok(report) => {
                        let checks: Vec<(String, bool, String)> = report
                            .checks
                            .iter()
                            .map(|c| (c.name.clone(), c.passed, c.message.clone()))
                            .collect();
                        let all_pass = checks.iter().all(|(_, p, _)| *p);
                        set_qa_report.set((all_pass, checks, report.suggestions));
                    }
                    Err(_) => {
                        set_qa_report.update(|r| {
                            r.0 = !r.0;
                        });
                    }
                }
                set_qa_running.set(false);
            });
        }
    };

    // Progress bar color class
    let progress_color = match &status {
        BeadStatus::Planning => "progress-fill-planning",
        BeadStatus::InProgress => "progress-fill-in-progress",
        BeadStatus::AiReview => "progress-fill-ai-review",
        BeadStatus::HumanReview => "progress-fill-human-review",
        BeadStatus::Done => "progress-fill-done",
        BeadStatus::Failed => "progress-fill-failed",
    };

    let bid_assign = bead.id.clone();
    let bid_edit = bead.id.clone();
    let title_edit = bead.title.clone();
    let desc_edit = bead.description.clone();
    let tags_edit = bead.tags.clone();

    let title_overview = title.clone();

    // Clone rerun_qa for multiple tab closures
    let rerun_qa_overview = rerun_qa.clone();
    let rerun_qa_logs = rerun_qa;

    // Build diff data from file tree
    let diff_data: Vec<(String, String, Vec<(String, String)>)> = file_tree
        .iter()
        .map(|(path, ct)| {
            let lines: Vec<(String, String)> = mock_diff_for_file(path, ct)
                .into_iter()
                .map(|(line, cls)| (line, cls.to_string()))
                .collect();
            (path.clone(), ct.to_string(), lines)
        })
        .collect();
    let diff_data_dialog = diff_data.clone();

    // Worktree path
    let wt_branch = branch.clone();
    let wt_path = format!(
        "/home/dev/auto-tundra-wt/{}",
        branch.split('/').next_back().unwrap_or("work")
    );

    // Pre-clone fields used across multiple move closures in tab content
    let bead_status_overview = bead.status;
    let bead_status_history = bead.status;
    let bead_timestamp_overview = bead.timestamp.clone();
    let bead_timestamp_history = bead.timestamp.clone();
    let bead_timestamp_meta1 = bead.timestamp.clone();
    let bead_timestamp_meta2 = bead.timestamp.clone();
    let bead_agent_names_history = bead.agent_names.clone();

    (
        view! {
            <div class="task-detail-content">
                <div class="td-body">
                    <div class="td-main">
                        // Main content area (already well-structured)
                        // ── Header ──
                        <div class="td-header">
                            <div class="td-header-top">
                                <h2 id="task-detail-heading" class="td-title">{title.clone()}</h2>
                                <div class="td-header-actions">
                                    <button class="td-icon-btn" on:click=move |_| set_show_edit.set(true) title="Edit task">
                                        <span class="td-icon">"✎"</span>
                                    </button>
                                    <button class="td-icon-btn td-close-btn" on:click=close_x title="Close">
                                        <span class="td-icon">"×"</span>
                                    </button>
                                </div>
                            </div>
                            <div class="td-header-meta">
                                <span class="td-branch-badge">{branch.clone()}</span>
                                <span class={s_cls}>{status_display.clone()}</span>
                                {(total_subtasks > 0).then(|| view! {
                                    <span class="td-subtask-count">{format!("{} subtasks", total_subtasks)}</span>
                                })}
                            </div>
                        </div>

                // Warning banners
                {stuck.then(|| {
                    let resume_stuck = resume_action.clone();
                    view! {
                        <div class="task-warning stuck-warning-banner">
                            <span class="stuck-warning-icon">"!"</span>
                            <div class="stuck-warning-text">
                                <strong>{t("task-warning-stuck-title")}</strong>
                                <p>{t("task-warning-stuck-desc")}</p>
                            </div>
                            <button class="btn btn-warning" on:click=resume_stuck>
                                {t("btn-resume-restart")}
                            </button>
                        </div>
                    }
                })}

                {merge_conflict.then(|| {
                    let resume_conflict = resume_action.clone();
                    view! {
                        <div class="task-warning task-warning-merge">
                            <span class="task-warning-icon">"!"</span>
                            <div class="task-warning-body">
                                <strong>{t("task-warning-merge-title")}</strong>
                                <p>{t("task-warning-merge-desc")}</p>
                            </div>
                            <button class="btn btn-sm btn-outline" on:click=resume_conflict>{t("btn-resolve")}</button>
                        </div>
                    }
                })}

                {rate_limited.then(|| {
                    let resume_rate = resume_action.clone();
                    view! {
                        <div class="task-warning task-warning-rate-limit">
                            <span class="task-warning-icon">"!"</span>
                            <div class="task-warning-body">
                                <strong>{t("task-warning-rate-title")}</strong>
                                <p>{t("task-warning-rate-desc")}</p>
                            </div>
                            <button class="btn btn-sm btn-outline" on:click=resume_rate>{t("btn-retry-now")}</button>
                        </div>
                    }
                })}

                {qa_failed.then(|| view! {
                    <div class="task-warning task-warning-qa-fail">
                        <span class="task-warning-icon">"!"</span>
                        <div class="task-warning-body">
                            <strong>{t("task-warning-qa-title")}</strong>
                            <p>{t("task-warning-qa-desc")}</p>
                        </div>
                        <button class="btn btn-sm btn-outline" on:click=move |_| set_active_tab.set(2)>{t("btn-view-logs")}</button>
                    </div>
                })}

                // ── Progress bar with percentage ──
                <div class="td-progress-row">
                    <div class="td-progress-bar-wrap">
                        <div class={format!("td-progress-fill {}", progress_color)} style=format!("width: {}%", progress)></div>
                    </div>
                    <span class="td-progress-label">{format!("{}%", progress)}</span>
                </div>

                // ── Tabs ──
                <div class="task-detail-tabs">
                    <button
                        class=(move || if active_tab.get() == 0 { "task-tab active" } else { "task-tab" })
                        on:click=move |_| set_active_tab.set(0)
                    >{t("task-detail-tab-overview")}</button>
                    <button
                        class=(move || if active_tab.get() == 1 { "task-tab active" } else { "task-tab" })
                        on:click=move |_| set_active_tab.set(1)
                    >
                        {format!("{} ({})", t("task-detail-tab-subtasks"), total_subtasks)}
                    </button>
                    <button
                        class=(move || if active_tab.get() == 2 { "task-tab active" } else { "task-tab" })
                        on:click=move |_| set_active_tab.set(2)
                    >{t("task-detail-tab-logs")}</button>
                    <button
                        class=(move || if active_tab.get() == 3 { "task-tab active" } else { "task-tab" })
                        on:click=move |_| set_active_tab.set(3)
                    >{t("task-detail-tab-files")}</button>
                    <button
                        class=(move || if active_tab.get() == 4 { "task-tab active" } else { "task-tab" })
                        on:click=move |_| set_active_tab.set(4)
                    >{t("task-detail-tab-history")}</button>
                </div>

                // ── Tab content ──
                <div class="task-detail-tab-content">
                    // Overview tab
                    {move || (active_tab.get() == 0).then(|| {
                        let desc = description.clone();
                        let cat = cat.clone();
                        let wt_branch = wt_branch.clone();
                        let wt_path = wt_path.clone();
                        let _rerun = rerun_qa_overview.clone();
                        let (all_pass, checks, _suggestions) = qa_report.get();
                        #[cfg(feature = "markdown")]
                        let html_output = {
                            let mut html_output = String::new();
                            let parser = Parser::new(&desc);
                            html::push_html(&mut html_output, parser);
                            html_output
                        };
                        #[cfg(not(feature = "markdown"))]
                        let html_output = {
                            // When markdown is disabled, escape HTML and preserve line breaks
                            desc.replace('&', "&amp;")
                                .replace('<', "&lt;")
                                .replace('>', "&gt;")
                                .replace('\n', "<br>")
                        };
                        view! {
                            <div class="task-tab-overview">
                                // Tags row
                                <div class="td-tags-row">
                                    <span class="td-tag td-tag-category">{cat}</span>
                                    <span class="td-tag td-tag-roadmap">"roadmap"</span>
                                    <span class="td-tag-timestamp">{bead_timestamp_overview.clone()}</span>
                                </div>

                                // Spec card
                                <div class="td-spec-card">
                                    <div class="td-spec-header">
                                        <span class="td-spec-icon">"📋"</span>
                                        <span class="td-spec-title">{title_overview.clone()}</span>
                                    </div>

                                    {(!desc.is_empty()).then(|| {
                                        view! {
                                            <div class="td-spec-section">
                                                <h4 class="td-spec-heading">"Description"</h4>
                                                <div class="td-spec-text markdown-body" inner_html=html_output></div>
                                            </div>
                                        }
                                    })}

                                    <div class="td-spec-section">
                                        <h4 class="td-spec-heading">"Rationale"</h4>
                                        <p class="td-spec-text">"This feature improves the system's capabilities and addresses user needs identified during development planning."</p>
                                    </div>

                                    <div class="td-spec-section">
                                        <h4 class="td-spec-heading">"User Stories"</h4>
                                        <ul class="td-spec-list">
                                            <li>"As a developer, I want this feature so that I can improve my workflow."</li>
                                            <li>"As a DevOps engineer, I want this integrated so I can monitor effectively."</li>
                                        </ul>
                                    </div>

                                    <div class="td-spec-section">
                                        <h4 class="td-spec-heading">"Acceptance Criteria"</h4>
                                        <ul class="td-spec-checklist">
                                            <li class="td-check-item td-check-done">
                                                <span class="td-check-icon">"✓"</span>
                                                <span>"Implementation passes all unit tests"</span>
                                            </li>
                                            <li class="td-check-item td-check-done">
                                                <span class="td-check-icon">"✓"</span>
                                                <span>"Integration tests cover the main use cases"</span>
                                            </li>
                                            <li class="td-check-item">
                                                <span class="td-check-icon">"○"</span>
                                                <span>"Documentation is updated"</span>
                                            </li>
                                            <li class="td-check-item">
                                                <span class="td-check-icon">"○"</span>
                                                <span>"Code review approved"</span>
                                            </li>
                                        </ul>
                                    </div>
                                </div>

                                // QA Summary (compact)
                                {(bead_status_overview != BeadStatus::Planning).then(|| {
                                    view! {
                                        <div class="td-qa-summary">
                                            <div class="td-qa-summary-header">
                                                <span class="td-qa-summary-title">{t("task-detail-qa-status")}</span>
                                                {if all_pass {
                                                    view! { <span class="td-qa-badge td-qa-pass">{t("qa-status-passed")}</span> }.into_any()
                                                } else {
                                                    view! { <span class="td-qa-badge td-qa-fail">{t("qa-status-failed")}</span> }.into_any()
                                                }}
                                            </div>
                                            <div class="td-qa-checks-row">
                                                {checks.iter().map(|(name, passed, _msg)| {
                                                    let icon = if *passed { "✓" } else { "✗" };
                                                    let cls = if *passed { "td-qa-check td-qa-check-pass" } else { "td-qa-check td-qa-check-fail" };
                                                    view! {
                                                        <span class={cls}>{format!("{} {}", icon, name)}</span>
                                                    }
                                                }).collect::<Vec<_>>()}
                                            </div>
                                        </div>
                                    }
                                })}

                                // Build Ready section
                                {show_build.then(|| {
                                    let branch_display = wt_branch.clone();
                                    let path_display = wt_path.clone();
                                    view! {
                                        <div class="td-build-section">
                                            <div class="td-build-header">
                                                <span class="td-build-icon">"🔨"</span>
                                                <span class="td-build-title">{t("task-detail-build")}</span>
                                            </div>
                                            <div class="td-build-stats">
                                                <div class="td-build-stat">
                                                    <span class="td-build-stat-num">{files_changed}</span>
                                                    <span class="td-build-stat-label">"files"</span>
                                                </div>
                                                <div class="td-build-stat">
                                                    <span class="td-build-stat-num">{commits}</span>
                                                    <span class="td-build-stat-label">"commits"</span>
                                                </div>
                                                <div class="td-build-stat td-stat-add">
                                                    <span class="td-build-stat-num">{format!("+{}", additions)}</span>
                                                </div>
                                                <div class="td-build-stat td-stat-del">
                                                    <span class="td-build-stat-num">{format!("-{}", deletions)}</span>
                                                </div>
                                            </div>
                                            <div class="td-build-branch-row">
                                                <span class="td-build-branch-label">{branch_display}</span>
                                                <span class="td-build-arrow">"→"</span>
                                                <span class="td-build-branch-label">"main"</span>
                                            </div>
                                            <div class="td-build-path">
                                                <span class="td-build-path-label">{t("task-detail-worktree-prefix")}" "</span>
                                                <span class="td-build-path-value">{path_display}</span>
                                            </div>
                                            <div class="td-build-actions">
                                                <button class="td-build-btn td-build-btn-cursor">{t("btn-open-cursor")}</button>
                                                <button class="td-build-btn td-build-btn-ghostty">{t("btn-open-ghostty")}</button>
                                            </div>
                                        </div>
                                    }
                                })}

                                // Action buttons row
                                <div class="td-action-row">
                                    <button class="td-action-btn td-action-conflicts">{t("btn-check-conflicts")}</button>
                                    {if has_pr {
                                        view! {
                                            <button class="td-action-btn td-action-pr-created" disabled=true>{t("btn-pr-created")}</button>
                                        }.into_any()
                                    } else {
                                        view! {
                                            <button class="td-action-btn td-action-create-pr">{t("btn-create-pr")}</button>
                                        }.into_any()
                                    }}
                                    <button class="td-action-btn td-action-screenshot" title="Take screenshot">"📸"</button>
                                </div>

                                // Request Changes section
                                <div class="td-request-changes">
                                    <h4 class="td-rc-title">{t("task-detail-request-changes")}</h4>
                                    <textarea
                                        class="td-rc-textarea"
                                        placeholder={t("task-detail-placeholder-changes")}
                                        prop:value=move || changes_text.get()
                                        on:input=move |ev| {
                                            set_changes_text.set(event_target_value(&ev));
                                        }
                                    ></textarea>
                                    <div class="td-rc-actions">
                                        <button
                                            class="td-rc-submit"
                                            disabled=move || changes_text.get().trim().is_empty()
                                        >{t("btn-request-changes")}</button>
                                    </div>
                                </div>
                            </div>
                        }
                    })}

                    // Subtasks tab
                    {move || (active_tab.get() == 1).then(|| {
                        let subtasks = subtasks.clone();
                        let checked = checked_subtasks.get();
                        let done_count = checked.len();
                        view! {
                            <div class="task-tab-subtasks">
                                <div class="subtasks-progress-text">
                                    {format!("{} of {} completed", done_count, total_subtasks)}
                                </div>
                                <div class="subtasks-list">
                                    {subtasks.into_iter().enumerate().map(|(i, (task_name, _))| {
                                        let is_checked = checked.contains(&i);
                                        let check_class = if is_checked { "subtask-item completed" } else { "subtask-item" };
                                        let icon = if is_checked { "✓" } else { "○" };
                                        let icon_cls = if is_checked { "subtask-check subtask-check-done" } else { "subtask-check" };
                                        view! {
                                            <button
                                                class={check_class}
                                                style="cursor: pointer;"
                                                on:click=move |_| {
                                                    set_checked_subtasks.update(|set| {
                                                        if set.contains(&i) {
                                                            set.remove(&i);
                                                        } else {
                                                            set.insert(i);
                                                        }
                                                    });
                                                }
                                            >
                                                <span class={icon_cls}>{icon}</span>
                                                <span class="subtask-number">{format!("{}.", i + 1)}</span>
                                                <span class="subtask-name">{task_name}</span>
                                            </button>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                            </div>
                        }
                    })}

                    // Logs tab
                    {move || (active_tab.get() == 2).then(|| {
                        let logs = logs.clone();
                        let (_all_pass, checks, suggestions) = qa_report.get();
                        let rerun = rerun_qa_logs.clone();
                        view! {
                            <div class="task-tab-logs">
                                // Execution log
                                <div class="td-log-section-title">{t("task-detail-tab-logs")}</div>
                                <div class="task-detail-log">
                                    {logs.into_iter().map(|(msg, time, level_cls)| view! {
                                        <div class={format!("log-entry {}", level_cls)}>
                                            <span class="log-dot"></span>
                                            <span class="log-time-label">{time}</span>
                                            <span class="log-message">{msg}</span>
                                        </div>
                                    }).collect::<Vec<_>>()}
                                </div>

                                // QA Report
                                <div class="td-log-section-title td-log-section-qa">
                                    <span>{t("tasks-detail-qa")}</span>
                                    <button
                                        class="btn btn-sm btn-outline"
                                        on:click=rerun
                                        disabled=move || qa_running.get()
                                    >
                                        {move || if qa_running.get() { t("btn-running") } else { t("btn-rerun-qa") }}
                                    </button>
                                </div>
                                <div class="qa-checks-list">
                                    {checks.iter().cloned().map(|(name, passed, message)| {
                                        let icon = if passed { "\u{2713}" } else { "\u{2717}" };
                                        let item_cls = if passed { "qa-check-item qa-check-pass" } else { "qa-check-item qa-check-fail" };
                                        view! {
                                            <div class={item_cls}>
                                                <span class="qa-check-icon">{icon}</span>
                                                <div class="qa-check-info">
                                                    <span class="qa-check-name">{name}</span>
                                                    <span class="qa-check-message">{message}</span>
                                                </div>
                                            </div>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                                {(!suggestions.is_empty()).then(|| {
                                    let sugs = suggestions.clone();
                                    view! {
                                        <div class="qa-suggestions">
                                            <h4>{t("task-detail-suggestions")}</h4>
                                            <ul>
                                                {sugs.iter().cloned().map(|s| view! {
                                                    <li>{s}</li>
                                                }).collect::<Vec<_>>()}
                                            </ul>
                                        </div>
                                    }
                                })}
                            </div>
                        }
                    })}

                    // Files tab
                    {move || (active_tab.get() == 3).then(|| {
                        let files = file_tree.clone();
                        view! {
                            <div class="task-tab-code">
                                <div class="code-tab-header">
                                    <span class="code-tab-label">{t("task-detail-changed-files")}</span>
                                    <button class="btn btn-sm btn-outline" on:click=move |_| set_show_diff.set(true)>{t("btn-view-diff")}</button>
                                </div>
                                <div class="code-file-tree">
                                    {files.into_iter().map(|(path, change_type)| {
                                        let type_cls = match change_type {
                                            "added" => "file-change-added",
                                            "modified" => "file-change-modified",
                                            "deleted" => "file-change-deleted",
                                            _ => "file-change-unknown",
                                        };
                                        let type_label = match change_type {
                                            "added" => "A",
                                            "modified" => "M",
                                            "deleted" => "D",
                                            _ => "?",
                                        };
                                        view! {
                                            <div class="code-file-item">
                                                <span class={format!("code-file-indicator {}", type_cls)}>{type_label}</span>
                                                <span class="code-file-path">{path}</span>
                                            </div>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                            </div>
                        }
                    })}

                    // History tab
                    {move || (active_tab.get() == 4).then(|| {
                        let timestamp = bead_timestamp_history.clone();
                        view! {
                            <div class="task-tab-history">
                                <div class="td-history-timeline">
                                    <div class="td-history-entry">
                                        <div class="td-history-dot td-history-dot-created"></div>
                                        <div class="td-history-line"></div>
                                        <div class="td-history-content">
                                            <span class="td-history-action">"Task created"</span>
                                            <span class="td-history-time">{timestamp.clone()}</span>
                                        </div>
                                    </div>
                                    {(bead_status_history != BeadStatus::Planning).then(|| view! {
                                        <div class="td-history-entry">
                                            <div class="td-history-dot td-history-dot-progress"></div>
                                            <div class="td-history-line"></div>
                                            <div class="td-history-content">
                                                <span class="td-history-action">"Planning completed, implementation started"</span>
                                                <span class="td-history-time">"earlier"</span>
                                            </div>
                                        </div>
                                    })}
                                    {(!bead_agent_names_history.is_empty()).then(|| {
                                        let names = bead_agent_names_history.join(", ");
                                        view! {
                                            <div class="td-history-entry">
                                                <div class="td-history-dot td-history-dot-assign"></div>
                                                <div class="td-history-line"></div>
                                                <div class="td-history-content">
                                                    <span class="td-history-action">{t_str("task-assigned-to", "agents", &names)}</span>
                                                    <span class="td-history-time">"earlier"</span>
                                                </div>
                                            </div>
                                        }
                                    })}
                                    {(bead_status_history == BeadStatus::AiReview || bead_status_history == BeadStatus::HumanReview || bead_status_history == BeadStatus::Done).then(|| view! {
                                        <div class="td-history-entry">
                                            <div class="td-history-dot td-history-dot-review"></div>
                                            <div class="td-history-line"></div>
                                            <div class="td-history-content">
                                                <span class="td-history-action">"Code review started"</span>
                                                <span class="td-history-time">"earlier"</span>
                                            </div>
                                        </div>
                                    })}
                                    {(bead_status_history == BeadStatus::Done).then(|| view! {
                                        <div class="td-history-entry">
                                            <div class="td-history-dot td-history-dot-done"></div>
                                            <div class="td-history-line"></div>
                                            <div class="td-history-content">
                                                <span class="td-history-action">"Task completed"</span>
                                                <span class="td-history-time">{timestamp.clone()}</span>
                                            </div>
                                        </div>
                                    })}
                                    {(bead_status_history == BeadStatus::Failed).then(|| view! {
                                        <div class="td-history-entry">
                                            <div class="td-history-dot td-history-dot-failed"></div>
                                            <div class="td-history-line"></div>
                                            <div class="td-history-content">
                                                <span class="td-history-action">"Task failed"</span>
                                                <span class="td-history-time">{timestamp.clone()}</span>
                                            </div>
                                        </div>
                                    })}
                                </div>
                            </div>
                        }
                    })}
                </div>
            </div> // close td-main

                // ── Sidebar ──
                <div class="td-sidebar">
                    <div class="td-sidebar-section">
                        <h4>{t("task-detail-metadata")}</h4>
                        <div class="td-meta-row">
                            <span class="td-meta-label">{t("task-detail-status")}</span>
                            <span class="td-meta-value">{status_display.clone()}</span>
                        </div>
                        <div class="td-meta-row">
                            <span class="td-meta-label">{t("task-detail-assignee")}</span>
                            <span class="td-meta-value">{if agents.is_empty() { t("task-detail-unassigned") } else { agents.join(", ") }}</span>
                        </div>
                        <div class="td-meta-row">
                            <span class="td-meta-label">{t("task-detail-created")}</span>
                            <span class="td-meta-value">{bead_timestamp_meta1.clone()}</span>
                        </div>
                        <div class="td-meta-row">
                            <span class="td-meta-label">{t("task-detail-updated")}</span>
                            <span class="td-meta-value">{bead_timestamp_meta2.clone()}</span>
                        </div>
                        <div class="td-meta-row">
                            <span class="td-meta-label">{t("task-detail-due-date")}</span>
                            <span class="td-meta-value">{t("task-detail-none")}</span>
                        </div>
                    </div>
                </div>
            </div>

                // ── Footer ──
                <div class="td-footer">
                    <div class="td-footer-left">
                        {show_delete.then(|| {
                            view! {
                                <button class="td-footer-delete" on:click=delete_action>
                                    {t("btn-delete-task")}
                                </button>
                            }
                        })}
                        <button
                            class="td-footer-discard"
                            on:click=move |_| set_show_discard.set(true)
                        >
                            {t("btn-discard")}
                        </button>
                    </div>
                    <div class="td-footer-right">
                        <button class="td-footer-close" on:click=close_bottom>
                            {t("btn-close")}
                        </button>
                        {show_resume.then(|| {
                            let resume_footer = resume_action.clone();
                            view! {
                                <button class="td-footer-resume" on:click=resume_footer>
                                    {t("btn-resume-task")}
                                </button>
                            }
                        })}
                        <button
                            class="td-footer-assign"
                            on:click=move |_| {
                                set_assigning.set(true);
                                let bid = bid_assign.clone();
                                spawn_local(async move {
                                    match crate::api::assign_agent(&bid).await {
                                        Ok(_) => {
                                            set_assign_msg.set(Some((true, t("btn-assign-agent"))));
                                        }
                                        Err(e) => {
                                            set_assign_msg.set(Some((false, format!("Failed: {}", e))));
                                        }
                                    }
                                    set_assigning.set(false);
                                });
                            }
                            disabled=move || assigning.get()
                        >
                            {move || if assigning.get() { t("btn-assigning") } else { t("btn-assign-agent") }}
                        </button>
                    </div>
                </div>
                {move || assign_msg.get().map(|(ok, msg)| {
                    let cls = if ok { "td-assign-msg td-assign-ok" } else { "td-assign-msg td-assign-err" };
                    view! { <div class={cls}>{msg}</div> }
                })}
        </div>
        },
        // Edit task modal
        move || {
            show_edit.get().then(|| {
                view! {
                    <crate::components::edit_task_modal::EditTaskModal
                        bead_id=bid_edit.clone()
                        initial_title=title_edit.clone()
                        initial_description=desc_edit.clone()
                        initial_tags=tags_edit.clone()
                        on_close=move |_| set_show_edit.set(false)
                    />
                }
            })
        },
        // Diff View Dialog
        move || {
            show_diff.get().then(|| {
        let data = diff_data_dialog.clone();
        let file_names: Vec<String> = data.iter().map(|(p, _, _)| p.clone()).collect();
        let file_types: Vec<String> = data.iter().map(|(_, ct, _)| ct.clone()).collect();
        view! {
            <div class="diff-dialog-overlay" on:click=move |_| set_show_diff.set(false)></div>
            <div class="diff-dialog-modal" role="dialog" aria-modal="true" aria-labelledby="diff-dialog-heading" on:click=move |ev: MouseEvent| ev.stop_propagation()>
                <div class="diff-dialog-header">
                    <h3 id="diff-dialog-heading">{t("task-detail-file-changes")}</h3>
                    <button class="task-detail-close" on:click=move |_| set_show_diff.set(false)>"×"</button>
                </div>
                <div class="diff-dialog-body">
                    <div class="diff-file-list">
                        {file_names.iter().cloned().enumerate().map(|(i, name)| {
                            let ct = file_types.get(i).cloned().unwrap_or_default();
                            let type_cls = match ct.as_str() {
                                "added" => "file-change-added",
                                "modified" => "file-change-modified",
                                "deleted" => "file-change-deleted",
                                _ => "",
                            };
                            let type_label = match ct.as_str() {
                                "added" => "A",
                                "modified" => "M",
                                "deleted" => "D",
                                _ => "?",
                            };
                            view! {
                                <button
                                    class=(move || if diff_selected_file.get() == i { "diff-file-item active" } else { "diff-file-item" })
                                    on:click=move |_| set_diff_selected_file.set(i)
                                >
                                    <span class={format!("code-file-indicator {}", type_cls)}>{type_label}</span>
                                    <span class="diff-file-name">{name}</span>
                                </button>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                    <div class="diff-content-area">
                        {move || {
                            let idx = diff_selected_file.get();
                            let data = data.clone();
                            data.get(idx).map(|(path, _, lines)| {
                                let path = path.clone();
                                let lines = lines.clone();
                                view! {
                                    <div class="diff-content-header">{path}</div>
                                    <pre class="diff-view">
                                        {lines.iter().cloned().map(|(line, cls)| {
                                            view! {
                                                <div class={cls}>{line}</div>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </pre>
                                }
                            })
                        }}
                    </div>
                </div>
            </div>
        }
    })
        },
        // Discard Confirmation Dialog
        move || {
            show_discard.get().then(|| {
        let discard = discard_action.clone();
        view! {
            <div class="discard-dialog-overlay" on:click=move |_| set_show_discard.set(false)></div>
            <div class="discard-dialog" role="dialog" aria-modal="true" aria-labelledby="discard-dialog-heading">
                <div class="discard-dialog-header">
                    <h3 id="discard-dialog-heading">{t("task-detail-discard-title")}</h3>
                </div>
                <div class="discard-dialog-body">
                    <div class="discard-warning">
                        <span class="discard-warning-icon">"!"</span>
                        <p>{t("task-detail-discard-warning")}</p>
                    </div>
                    <p class="discard-detail">{t("task-detail-discard-detail")}</p>
                </div>
                <div class="discard-dialog-actions">
                    <button
                        class="btn btn-sm btn-outline"
                        on:click=move |_| set_show_discard.set(false)
                    >{t("btn-cancel")}</button>
                    <button
                        class="btn btn-sm btn-danger"
                        on:click=discard
                        disabled=move || discarding.get()
                    >
                        {move || if discarding.get() { t("btn-discarding") } else { t("btn-confirm-discard") }}
                    </button>
                </div>
            </div>
        }
    })
        },
    )
}

#[component]
pub fn TaskDetail(
    bead_id: String,
    on_close: impl Fn(MouseEvent) + Clone + 'static,
) -> impl IntoView {
    let state = use_app_state();
    let beads = state.beads;
    let set_beads = state.set_beads;
    let close_bg = on_close.clone();
    let initial_bead = beads.get().into_iter().find(|b| b.id == bead_id);

    let focus_trap = use_focus_trap();
    let on_close_clone = on_close.clone();

    // Combined keydown handler for focus trap and Escape key
    let handle_keydown = move |ev: KeyboardEvent| {
        // Handle Escape key to close modal
        if ev.key() == "Escape" {
            // Create a synthetic MouseEvent for on_close
            if let Ok(dummy_event) = web_sys::MouseEvent::new("click") {
                on_close_clone(dummy_event);
            }
            return;
        }

        // Handle Tab/Shift+Tab for focus trapping
        focus_trap(ev);
    };

    view! {
        <div class="task-detail-overlay" on:click=close_bg></div>
        <div class="task-detail-modal" role="dialog" aria-modal="true" aria-labelledby="task-detail-heading" on:keydown=handle_keydown>
            {match initial_bead {
                None => view! {
                    <div class="task-detail-empty">
                        <h3>{t("task-not-found")}</h3>
                        <p>{t("task-not-found-desc")}</p>
                    </div>
                }.into_any(),
                Some(bead) => view! {
                    <TaskDetailInner bead=bead set_beads=set_beads on_close=on_close />
                }.into_any(),
            }}
        </div>
    }
}
