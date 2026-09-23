//! Per-domain route tables for the at-bridge HTTP API.
//!
//! Each `*_router()` returns a [`Domain`]: an axum sub-router nested under
//! one URL prefix plus the [`RouteSpec`] of every route in it. The top-level
//! router (`api_router_with_auth`) nests all of them and builds the
//! `GET /api/catalog` manifest from the same specs.
//!
//! Domains are grouped by URL prefix, not by handler module: e.g. task
//! archival and drafts live in `misc.rs` but are served under `/api/tasks`.

use crate::intelligence_api as intel;
use crate::terminal_ws;

use super::catalog::{get_catalog, Domain, RouteSpec as R, SMALL_BODY};
use super::{
    agents, beads, bootstrap, github, integrations, kanban, mcp, mcp_sse, metrics, misc,
    notifications, pipeline, projects, queue, sessions, settings, tasks, websocket, worktrees,
};

/// Every domain, in mount order.
pub(crate) fn all() -> Vec<Domain> {
    vec![
        catalog_router(),
        system_router(),
        beads_router(),
        agents_router(),
        tasks_router(),
        pipeline_router(),
        terminals_router(),
        settings_router(),
        github_router(),
        gitlab_router(),
        linear_router(),
        kanban_router(),
        mcp_router(),
        mcp_transport_router(),
        worktrees_router(),
        queue_router(),
        notifications_router(),
        metrics_router(),
        sessions_router(),
        projects_router(),
        files_router(),
        events_router(),
        websocket_router(),
        insights_router(),
        ideation_router(),
        roadmap_router(),
        memory_router(),
        changelog_router(),
        context_router(),
    ]
}

/// `/api/catalog`, `/api/v1/catalog` -- route discovery.
pub(crate) fn catalog_router() -> Domain {
    Domain::new("catalog", "/api")
        .route(
            R::get(
                "/catalog",
                "Route catalog: every endpoint with method, path, auth and types",
            )
            .res("ApiCatalog"),
            get_catalog,
        )
        .route(
            R::get("/v1/catalog", "Route catalog pinned to schema v1").res("ApiCatalog"),
            get_catalog,
        )
}

/// Single-route status/diagnostic endpoints directly under `/api`.
pub(crate) fn system_router() -> Domain {
    Domain::new("system", "/api")
        .route(
            R::get(
                "/bootstrap",
                "Startup snapshot: beads, agents, KPIs, version, uptime",
            )
            .res("BootstrapResponse"),
            bootstrap::get_bootstrap,
        )
        .route(
            R::get("/status", "Daemon version, uptime and counts").res("StatusResponse"),
            misc::get_status,
        )
        .route(
            R::get("/kpi", "Bead/agent KPI snapshot").res("KpiSnapshot"),
            misc::get_kpi,
        )
        .route(
            R::get("/costs", "Aggregated LLM token usage and cost").res("CostResponse"),
            misc::get_costs,
        )
        .route(
            R::get("/convoys", "List convoys (task groups)").res("Vec<ConvoyEntry>"),
            misc::list_convoys,
        )
        .route(
            R::get(
                "/credentials/status",
                "Which provider credentials are configured",
            ),
            misc::get_credentials_status,
        )
        .route(
            R::get(
                "/debug/memory",
                "In-memory collection sizes for leak debugging",
            ),
            misc::get_memory_usage,
        )
        .route(
            R::get(
                "/cli/available",
                "Detect installed agent CLIs (claude, codex, ...)",
            )
            .res("Vec<CliAvailabilityEntry>"),
            misc::list_available_clis,
        )
}

/// `/api/beads` -- bead (work item) CRUD and status transitions.
pub(crate) fn beads_router() -> Domain {
    Domain::new("beads", "/api/beads")
        .route(
            R::get("/", "List beads, filterable by lane/status").res("Vec<Bead>"),
            beads::list_beads,
        )
        .route(
            R::post("/", "Create a bead")
                .req("CreateBeadRequest")
                .res("Bead"),
            beads::create_bead,
        )
        .route(R::delete("/{id}", "Delete a bead"), beads::delete_bead)
        .route(
            R::post("/{id}/status", "Transition a bead to a new status")
                .req("UpdateBeadStatusRequest")
                .res("Bead")
                .limit(SMALL_BODY),
            beads::update_bead_status,
        )
}

/// `/api/agents` -- agent listing and control.
pub(crate) fn agents_router() -> Domain {
    Domain::new("agents", "/api/agents")
        .route(
            R::get("/", "List agents").res("Vec<Agent>"),
            agents::list_agents,
        )
        .route(
            R::post("/{id}/nudge", "Nudge a stalled agent").limit(SMALL_BODY),
            agents::nudge_agent,
        )
        .route(
            R::post("/{id}/stop", "Stop an agent").limit(SMALL_BODY),
            agents::stop_agent,
        )
}

/// `/api/tasks` -- tasks, their pipeline, archival, attachments and drafts.
pub(crate) fn tasks_router() -> Domain {
    Domain::new("tasks", "/api/tasks")
        .route(
            R::get("/", "List tasks, filterable and paginated").res("Vec<Task>"),
            tasks::list_tasks,
        )
        .route(
            R::post("/", "Create a task")
                .req("CreateTaskRequest")
                .res("Task"),
            tasks::create_task,
        )
        .route(
            R::get("/archived", "List archived task ids").res("Vec<Uuid>"),
            misc::list_archived_tasks,
        )
        .route(
            R::get("/drafts", "List task drafts").res("Vec<TaskDraft>"),
            misc::list_task_drafts,
        )
        .route(
            R::post("/drafts", "Save a task draft")
                .req("TaskDraft")
                .res("TaskDraft"),
            misc::save_task_draft,
        )
        .route(
            R::get("/drafts/{id}", "Get a task draft").res("TaskDraft"),
            misc::get_task_draft,
        )
        .route(
            R::delete("/drafts/{id}", "Delete a task draft"),
            misc::delete_task_draft,
        )
        .route(R::get("/{id}", "Get a task").res("Task"), tasks::get_task)
        .route(
            R::put("/{id}", "Update a task")
                .req("UpdateTaskRequest")
                .res("Task"),
            tasks::update_task,
        )
        .route(R::delete("/{id}", "Delete a task"), tasks::delete_task)
        .route(
            R::post("/{id}/phase", "Move a task to a new phase")
                .req("UpdateTaskPhaseRequest")
                .res("Task")
                .limit(SMALL_BODY),
            tasks::update_task_phase,
        )
        .route(
            R::get("/{id}/logs", "Task log entries").res("Vec<TaskLogEntry>"),
            tasks::get_task_logs,
        )
        .route(
            R::post(
                "/{id}/execute",
                "Run the coding -> QA -> fix pipeline for a task",
            )
            .req("ExecuteTaskRequest"),
            pipeline::execute_task_pipeline,
        )
        .route(
            R::get("/{id}/build-logs", "Pipeline build log lines"),
            pipeline::get_build_logs,
        )
        .route(
            R::get("/{id}/build-status", "Pipeline build status"),
            pipeline::get_build_status,
        )
        .route(
            R::post("/{id}/archive", "Archive a task").limit(SMALL_BODY),
            misc::archive_task,
        )
        .route(
            R::post("/{id}/unarchive", "Unarchive a task").limit(SMALL_BODY),
            misc::unarchive_task,
        )
        .route(
            R::get("/{task_id}/attachments", "List a task's attachments").res("Vec<Attachment>"),
            misc::list_attachments,
        )
        .route(
            R::post("/{task_id}/attachments", "Attach a file to a task")
                .res("Attachment")
                .limit(10 * 1024 * 1024),
            misc::add_attachment,
        )
        .route(
            R::delete("/{task_id}/attachments/{id}", "Delete a task attachment"),
            misc::delete_attachment,
        )
}

/// `/api/pipeline` -- pipeline queue.
pub(crate) fn pipeline_router() -> Domain {
    Domain::new("pipeline", "/api/pipeline").route(
        R::get("/queue", "Pipeline concurrency queue status").res("PipelineQueueStatus"),
        pipeline::get_pipeline_queue_status,
    )
}

/// `/api/terminals` -- PTY terminal lifecycle (I/O is on `/ws/terminal/{id}`).
pub(crate) fn terminals_router() -> Domain {
    Domain::new("terminals", "/api/terminals")
        .route(
            R::get("/", "List terminals").res("Vec<TerminalInfo>"),
            terminal_ws::list_terminals,
        )
        .route(
            R::post("/", "Spawn a shell terminal").res("TerminalInfo"),
            terminal_ws::create_terminal,
        )
        .route(
            R::get("/persistent", "List terminals marked persistent").res("Vec<TerminalInfo>"),
            terminal_ws::list_persistent_terminals,
        )
        .route(
            R::delete("/{id}", "Kill and remove a terminal"),
            terminal_ws::delete_terminal,
        )
        .route(
            R::patch(
                "/{id}/settings",
                "Update terminal font/cursor/persistence settings",
            ),
            terminal_ws::update_terminal_settings,
        )
        .route(
            R::post("/{id}/auto-name", "Set a terminal's auto-generated name")
                .req("AutoNameRequest"),
            terminal_ws::auto_name_terminal,
        )
}

/// `/api/settings` -- daemon configuration.
pub(crate) fn settings_router() -> Domain {
    Domain::new("settings", "/api/settings")
        .route(
            R::get("/", "Current configuration").res("Config"),
            settings::get_settings,
        )
        .route(
            R::put("/", "Replace the configuration")
                .req("Config")
                .res("Config"),
            settings::put_settings,
        )
        .route(
            R::patch("/", "Deep-merge a partial configuration").res("Config"),
            settings::patch_settings,
        )
        .route(
            R::post("/direct-mode", "Toggle direct (no-worktree) mode")
                .req("DirectModeRequest")
                .limit(SMALL_BODY),
            misc::toggle_direct_mode,
        )
}

/// `/api/github` -- GitHub sync, issues, PRs, releases and OAuth.
pub(crate) fn github_router() -> Domain {
    Domain::new("github", "/api/github")
        .route(
            R::post("/sync", "Sync GitHub issues into beads"),
            github::trigger_github_sync,
        )
        .route(
            R::get("/sync/status", "Last GitHub sync status"),
            github::get_sync_status,
        )
        .route(
            R::get("/issues", "List repository issues"),
            github::list_github_issues,
        )
        .route(
            R::post("/issues/{number}/import", "Import a GitHub issue as a task"),
            github::import_github_issue,
        )
        .route(
            R::get("/prs", "List repository pull requests"),
            github::list_github_prs,
        )
        .route(
            R::post("/pr/{task_id}", "Open a pull request for a task").req("CreatePrRequest"),
            github::create_pr_for_task,
        )
        .route(
            R::get("/pr/watched", "List watched pull requests").res("Vec<PrPollStatus>"),
            github::list_watched_prs,
        )
        .route(
            R::post("/pr/{number}/watch", "Start polling a pull request").limit(SMALL_BODY),
            github::watch_pr,
        )
        .route(
            R::delete("/pr/{number}/watch", "Stop polling a pull request"),
            github::unwatch_pr,
        )
        .route(
            R::get("/releases", "List GitHub releases"),
            github::list_releases,
        )
        .route(
            R::post("/releases", "Create a GitHub release").req("CreateReleaseRequest"),
            github::create_release,
        )
        .route(
            R::get(
                "/oauth/authorize",
                "Start the GitHub OAuth flow (authorize URL)",
            ),
            github::github_oauth_authorize,
        )
        .route(
            R::post("/oauth/callback", "Exchange an OAuth code for a token")
                .req("OAuthCallbackRequest"),
            github::github_oauth_callback,
        )
        .route(
            R::get("/oauth/status", "GitHub OAuth connection status"),
            github::github_oauth_status,
        )
        .route(
            R::post("/oauth/revoke", "Revoke the stored GitHub token"),
            github::github_oauth_revoke,
        )
        .route(
            R::post("/oauth/refresh", "Refresh the GitHub OAuth token"),
            github::github_oauth_refresh,
        )
}

/// `/api/gitlab` -- GitLab issues and merge requests.
pub(crate) fn gitlab_router() -> Domain {
    Domain::new("gitlab", "/api/gitlab")
        .route(
            R::get("/issues", "List GitLab issues"),
            integrations::list_gitlab_issues,
        )
        .route(
            R::get("/merge-requests", "List GitLab merge requests"),
            integrations::list_gitlab_merge_requests,
        )
        .route(
            R::post(
                "/merge-requests/{iid}/review",
                "Heuristic review of a merge request diff",
            )
            .req("ReviewGitLabMrBody"),
            integrations::review_gitlab_merge_request,
        )
}

/// `/api/linear` -- Linear issues.
pub(crate) fn linear_router() -> Domain {
    Domain::new("linear", "/api/linear")
        .route(
            R::get("/issues", "List Linear issues"),
            integrations::list_linear_issues,
        )
        .route(
            R::post("/import", "Import Linear issues as beads").req("ImportLinearBody"),
            integrations::import_linear_issues,
        )
}

/// `/api/kanban` -- board columns, ordering and planning poker.
pub(crate) fn kanban_router() -> Domain {
    Domain::new("kanban", "/api/kanban")
        .route(
            R::get("/columns", "Kanban column configuration").res("KanbanColumnConfig"),
            kanban::get_kanban_columns,
        )
        .route(
            R::patch("/columns", "Update kanban column configuration")
                .req("KanbanColumnConfig")
                .res("KanbanColumnConfig"),
            kanban::patch_kanban_columns,
        )
        .route(
            R::post("/columns/lock", "Lock or unlock a column")
                .req("LockColumnRequest")
                .limit(SMALL_BODY),
            misc::lock_column,
        )
        .route(
            R::post("/ordering", "Save task ordering within a column")
                .req("TaskOrderingRequest")
                .limit(SMALL_BODY),
            misc::save_task_ordering,
        )
        .route(
            R::post("/poker/start", "Start a planning poker session")
                .req("StartPlanningPokerRequest")
                .res("PlanningPokerSessionResponse"),
            kanban::start_planning_poker,
        )
        .route(
            R::post("/poker/vote", "Submit a planning poker vote")
                .req("SubmitPlanningPokerVoteRequest")
                .res("PlanningPokerSessionResponse"),
            kanban::submit_planning_poker_vote,
        )
        .route(
            R::post("/poker/reveal", "Reveal planning poker votes")
                .req("RevealPlanningPokerRequest")
                .res("PlanningPokerSessionResponse"),
            kanban::reveal_planning_poker,
        )
        .route(
            R::post(
                "/poker/simulate",
                "Simulate a planning poker round with virtual agents",
            )
            .req("SimulatePlanningPokerRequest")
            .res("PlanningPokerSessionResponse"),
            kanban::simulate_planning_poker,
        )
        .route(
            R::get("/poker/{bead_id}", "Planning poker session for a bead")
                .res("PlanningPokerSessionResponse"),
            kanban::get_planning_poker_session,
        )
}

/// `/api/mcp` -- MCP server registry and tool calls (REST).
pub(crate) fn mcp_router() -> Domain {
    Domain::new("mcp", "/api/mcp")
        .route(
            R::get("/servers", "List configured MCP servers").res("Vec<McpServer>"),
            mcp::list_mcp_servers,
        )
        .route(
            R::post("/tools/call", "Call an MCP tool").req("ToolCallRequest"),
            mcp::call_mcp_tool,
        )
}

/// `/mcp` -- MCP HTTP+SSE transport (Claude Code connects here).
pub(crate) fn mcp_transport_router() -> Domain {
    Domain::new("mcp_transport", "/mcp")
        .route(
            R::get(
                "/sse",
                "MCP SSE stream; first event names the message endpoint",
            ),
            mcp_sse::handle_sse,
        )
        .route(
            R::post("/messages", "MCP JSON-RPC message for an SSE session").req("JsonRpcRequest"),
            mcp_sse::handle_message,
        )
}

/// `/api/worktrees` -- git worktree inspection and merging.
pub(crate) fn worktrees_router() -> Domain {
    Domain::new("worktrees", "/api/worktrees")
        .route(R::get("/", "List git worktrees"), worktrees::list_worktrees)
        .route(
            R::delete("/{id}", "Remove a worktree"),
            worktrees::delete_worktree,
        )
        .route(
            R::post("/{id}/merge", "Merge a worktree branch").limit(SMALL_BODY),
            worktrees::merge_worktree,
        )
        .route(
            R::get(
                "/{id}/merge-preview",
                "Preview a worktree merge (conflicts, diffstat)",
            ),
            worktrees::merge_preview,
        )
        .route(
            R::post("/{id}/resolve", "Resolve a merge conflict")
                .req("ResolveConflictRequest")
                .limit(SMALL_BODY),
            worktrees::resolve_conflict,
        )
}

/// `/api/queue` -- agent task queue.
pub(crate) fn queue_router() -> Domain {
    Domain::new("queue", "/api/queue")
        .route(R::get("/", "List the agent task queue"), queue::list_queue)
        .route(
            R::post("/reorder", "Reorder the queue")
                .req("QueueReorderRequest")
                .limit(SMALL_BODY),
            queue::reorder_queue,
        )
        .route(
            R::post("/{task_id}/prioritize", "Change a queued task's priority")
                .req("PrioritizeRequest")
                .limit(SMALL_BODY),
            queue::prioritize_task,
        )
}

/// `/api/notifications` -- notification inbox.
pub(crate) fn notifications_router() -> Domain {
    Domain::new("notifications", "/api/notifications")
        .route(
            R::get("/", "List notifications"),
            notifications::list_notifications,
        )
        .route(
            R::get("/count", "Unread notification count"),
            notifications::notification_count,
        )
        .route(
            R::post("/read-all", "Mark all notifications read").limit(SMALL_BODY),
            notifications::mark_all_notifications_read,
        )
        .route(
            R::post("/profile-swap", "Record an API profile swap notification").limit(SMALL_BODY),
            misc::notify_profile_swap,
        )
        .route(
            R::get("/app-update", "Check for an app update"),
            misc::check_app_update,
        )
        .route(
            R::post("/{id}/read", "Mark a notification read").limit(SMALL_BODY),
            notifications::mark_notification_read,
        )
        .route(
            R::delete("/{id}", "Delete a notification"),
            notifications::delete_notification,
        )
}

/// `/api/metrics` -- telemetry metrics.
pub(crate) fn metrics_router() -> Domain {
    Domain::new("metrics", "/api/metrics")
        .route(
            R::get("/", "Metrics in Prometheus text format"),
            metrics::get_metrics_prometheus,
        )
        .route(
            R::get("/json", "Metrics as JSON"),
            metrics::get_metrics_json,
        )
}

/// `/api/sessions` -- agent sessions and persisted UI sessions.
pub(crate) fn sessions_router() -> Domain {
    Domain::new("sessions", "/api/sessions")
        .route(
            R::get("/", "List agent sessions").res("Vec<AgentSessionEntry>"),
            misc::list_agent_sessions,
        )
        .route(
            R::get("/ui", "Most recent UI session state").res("Option<SessionState>"),
            sessions::get_ui_session,
        )
        .route(
            R::put("/ui", "Save UI session state").req("SessionState"),
            sessions::save_ui_session,
        )
        .route(
            R::get("/ui/list", "List saved UI sessions (paginated)"),
            sessions::list_ui_sessions,
        )
}

/// `/api/projects` -- multi-project management.
pub(crate) fn projects_router() -> Domain {
    Domain::new("projects", "/api/projects")
        .route(
            R::get("/", "List projects").res("Vec<Project>"),
            projects::list_projects,
        )
        .route(
            R::post("/", "Create a project")
                .req("CreateProjectRequest")
                .res("Project"),
            projects::create_project,
        )
        .route(
            R::put("/{id}", "Update a project")
                .req("UpdateProjectRequest")
                .res("Project"),
            projects::update_project,
        )
        .route(
            R::delete("/{id}", "Delete a project"),
            projects::delete_project,
        )
        .route(
            R::post("/{id}/activate", "Make a project the active one")
                .res("Project")
                .limit(SMALL_BODY),
            projects::activate_project,
        )
}

/// `/api/files` -- file watching.
pub(crate) fn files_router() -> Domain {
    Domain::new("files", "/api/files")
        .route(
            R::post("/watch", "Start watching a path")
                .req("FileWatchRequest")
                .limit(SMALL_BODY),
            misc::start_file_watch,
        )
        .route(
            R::post("/unwatch", "Stop watching a path")
                .req("FileWatchRequest")
                .limit(SMALL_BODY),
            misc::stop_file_watch,
        )
}

/// `/api/events` -- event bus over WebSocket.
pub(crate) fn events_router() -> Domain {
    Domain::new("events", "/api/events").route(
        R::get(
            "/ws",
            "WebSocket stream of event bus messages (origin-checked)",
        ),
        websocket::events_ws_handler,
    )
}

/// `/ws` -- WebSocket endpoints.
pub(crate) fn websocket_router() -> Domain {
    Domain::new("websocket", "/ws")
        .route(
            R::get(
                "/",
                "WebSocket stream of event bus messages (origin-checked)",
            ),
            websocket::ws_handler,
        )
        .route(
            R::get("/terminal/{id}", "WebSocket terminal I/O for a terminal id"),
            terminal_ws::terminal_ws,
        )
}

/// `/api/insights` -- insights chat sessions.
pub(crate) fn insights_router() -> Domain {
    Domain::new("insights", "/api/insights")
        .route(
            R::get("/sessions", "List insights chat sessions"),
            intel::list_sessions,
        )
        .route(
            R::post("/sessions", "Create an insights chat session").req("CreateSessionRequest"),
            intel::create_session,
        )
        .route(
            R::delete("/sessions/{id}", "Delete an insights chat session"),
            intel::delete_session,
        )
        .route(
            R::get("/sessions/{id}/messages", "Messages in an insights session"),
            intel::get_session_messages,
        )
        .route(
            R::post(
                "/sessions/{id}/messages",
                "Add a message to an insights session",
            )
            .req("AddMessageRequest"),
            intel::add_message,
        )
}

/// `/api/ideation` -- idea generation.
pub(crate) fn ideation_router() -> Domain {
    Domain::new("ideation", "/api/ideation")
        .route(R::get("/ideas", "List ideas"), intel::list_ideas)
        .route(
            R::post("/generate", "Generate ideas").req("GenerateIdeasRequest"),
            intel::generate_ideas,
        )
        .route(
            R::post("/ideas/{id}/convert", "Convert an idea into a task"),
            intel::convert_idea,
        )
}

/// `/api/roadmap` -- roadmaps, features and competitor analysis.
pub(crate) fn roadmap_router() -> Domain {
    Domain::new("roadmap", "/api/roadmap")
        .route(R::get("/", "List roadmaps"), intel::list_roadmaps)
        .route(
            R::post("/", "Create a roadmap").req("CreateRoadmapRequest"),
            intel::create_roadmap,
        )
        .route(
            R::post("/generate", "Generate a roadmap").req("GenerateRoadmapRequest"),
            intel::generate_roadmap,
        )
        .route(
            R::post("/features", "Add a feature to the latest roadmap")
                .req("AddFeatureToLatestRequest"),
            intel::add_feature_to_latest,
        )
        .route(
            R::put(
                "/features/{fid}/status",
                "Set a feature's status by feature id",
            )
            .req("UpdateFeatureStatusRequest"),
            intel::update_feature_status_by_id,
        )
        .route(
            R::post("/{id}/features", "Add a feature to a roadmap").req("AddFeatureRequest"),
            intel::add_feature,
        )
        .route(
            R::patch("/{id}/features/{fid}", "Set a roadmap feature's status")
                .req("UpdateFeatureStatusRequest"),
            intel::update_feature_status,
        )
        .route(
            R::post("/competitor-analysis", "Run a competitor analysis")
                .req("CompetitorAnalysisRequest")
                .res("CompetitorAnalysisResult"),
            misc::run_competitor_analysis,
        )
}

/// `/api/memory` -- project memory entries.
pub(crate) fn memory_router() -> Domain {
    Domain::new("memory", "/api/memory")
        .route(
            R::get("/", "List memory entries").res("Vec<MemoryEntry>"),
            intel::list_memory,
        )
        .route(
            R::post("/", "Add a memory entry").req("AddMemoryRequest"),
            intel::add_memory,
        )
        .route(
            R::get("/search", "Search memory entries").res("Vec<MemoryEntry>"),
            intel::search_memory,
        )
        .route(
            R::delete("/{id}", "Delete a memory entry"),
            intel::delete_memory,
        )
}

/// `/api/changelog` -- changelog generation.
pub(crate) fn changelog_router() -> Domain {
    Domain::new("changelog", "/api/changelog")
        .route(R::get("/", "Current changelog"), intel::get_changelog)
        .route(
            R::post("/generate", "Generate a changelog from commit messages")
                .req("GenerateChangelogRequest"),
            intel::generate_changelog,
        )
}

/// `/api/context` -- project context for agents.
pub(crate) fn context_router() -> Domain {
    Domain::new("context", "/api/context").route(
        R::get(
            "/",
            "Project context (CLAUDE.md, AGENTS.md, TODO.md, agent/skill counts)",
        ),
        intel::get_context,
    )
}
