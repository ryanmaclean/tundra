// ---------------------------------------------------------------------------
// HTTP API module directory
// ---------------------------------------------------------------------------
//
// Split from the original monolith `http_api.rs` (10 000+ lines) into
// domain-oriented sub-modules.  This file wires them together, owns the
// Axum router, and re-exports public items so that downstream crates
// (`at-daemon`, `intelligence_api`, `terminal_ws`) keep compiling without
// any import-path changes.

mod agents;
mod beads;
mod github;
mod integrations;
mod kanban;
mod mcp;
mod metrics;
mod misc;
mod notifications;
mod pipeline;
mod projects;
mod queue;
mod sessions;
mod settings;
pub mod state;
mod tasks;
#[cfg(test)]
mod tests;
pub mod types;
mod websocket;
mod worktrees;

// ---- Re-exports for backward compatibility --------------------------------

pub use state::ApiState;
pub use types::*;

// Re-export items used by intelligence_api.rs
pub(crate) use kanban::simulate_planning_poker_for_bead;

// Re-export items used by at-daemon
pub use self::router::{api_router, api_router_with_auth};

// Re-export spawn_oauth_token_refresh_monitor (used by at-daemon)
pub use self::oauth_monitor::spawn_oauth_token_refresh_monitor;

// Re-export spawn_pr_poller (used by at-daemon)
pub use github::spawn_pr_poller;

// ---------------------------------------------------------------------------
// Shared utilities used across multiple handler modules
// ---------------------------------------------------------------------------

use at_harness::security::{InputSanitizer, SecurityError};

/// Validate a user-supplied text field (title, description, etc.).
pub(crate) fn validate_text_field(input: &str) -> Result<(), SecurityError> {
    let sanitizer = InputSanitizer::default();
    sanitizer.sanitize(input).map(|_| ())
}

/// Deep-merge `patch` into `target`. Objects are merged recursively; other
/// values are replaced.
pub(crate) fn merge_json(target: &mut serde_json::Value, patch: &serde_json::Value) {
    match (target.is_object(), patch.is_object()) {
        (true, true) => {
            let t = target.as_object_mut().expect("target.is_object() already verified");
            let p = patch.as_object().expect("patch.is_object() already verified");
            for (key, value) in p {
                let entry = t.entry(key.clone()).or_insert(serde_json::Value::Null);
                merge_json(entry, value);
            }
        }
        _ => {
            *target = patch.clone();
        }
    }
}

// ---------------------------------------------------------------------------
// Router + middleware
// ---------------------------------------------------------------------------

mod router {
    use super::*;
    use axum::{
        body::Body,
        extract::Request,
        middleware::{self as axum_middleware, Next},
        response::Response,
        routing::{get, patch, post, put},
        Router,
    };
    use std::sync::Arc;
    use tower_http::cors::CorsLayer;

    use crate::auth::AuthLayer;
    use crate::intelligence_api;
    use crate::terminal_ws;
    use at_telemetry::middleware::metrics_middleware;
    use at_telemetry::tracing_setup::request_id_middleware;

    /// Build the full API router with all REST and WebSocket routes.
    ///
    /// When `api_key` is `Some`, the [`AuthLayer`] middleware will require
    /// every request to carry a valid key. When `None`, all requests pass
    /// through (development mode).
    pub fn api_router(state: Arc<ApiState>) -> Router {
        api_router_with_auth(state, None, vec![])
    }

    /// Add browser cross-origin isolation headers needed for threaded WASM paths.
    async fn isolation_headers_middleware(request: Request<Body>, next: Next) -> Response {
        let mut response = next.run(request).await;
        let headers = response.headers_mut();
        headers.insert(
            "Cross-Origin-Opener-Policy",
            axum::http::HeaderValue::from_static("same-origin"),
        );
        headers.insert(
            "Cross-Origin-Embedder-Policy",
            axum::http::HeaderValue::from_static("credentialless"),
        );
        headers.insert(
            "Cross-Origin-Resource-Policy",
            axum::http::HeaderValue::from_static("same-origin"),
        );
        headers.insert(
            "X-Content-Type-Options",
            axum::http::HeaderValue::from_static("nosniff"),
        );
        headers.insert(
            "X-Frame-Options",
            axum::http::HeaderValue::from_static("DENY"),
        );
        headers.insert(
            "Strict-Transport-Security",
            axum::http::HeaderValue::from_static("max-age=63072000; includeSubDomains"),
        );
        headers.insert(
            "X-XSS-Protection",
            axum::http::HeaderValue::from_static("1; mode=block"),
        );
        headers.insert(
            "Referrer-Policy",
            axum::http::HeaderValue::from_static("strict-origin-when-cross-origin"),
        );
        response
    }

    /// Build the API router with optional authentication.
    pub fn api_router_with_auth(
        state: Arc<ApiState>,
        api_key: Option<String>,
        allowed_origins: Vec<String>,
    ) -> Router {
        Router::new()
            .route("/api/status", get(misc::get_status))
            .route("/api/beads", get(beads::list_beads))
            .route("/api/beads", post(beads::create_bead))
            .route("/api/beads/{id}", axum::routing::delete(beads::delete_bead))
            .route("/api/beads/{id}/status", post(beads::update_bead_status))
            .route("/api/agents", get(agents::list_agents))
            .route("/api/agents/{id}/nudge", post(agents::nudge_agent))
            .route("/api/agents/{id}/stop", post(agents::stop_agent))
            .route("/api/kpi", get(misc::get_kpi))
            .route("/api/tasks", get(tasks::list_tasks))
            .route("/api/tasks", post(tasks::create_task))
            .route("/api/tasks/{id}", get(tasks::get_task))
            .route("/api/tasks/{id}", put(tasks::update_task))
            .route("/api/tasks/{id}", axum::routing::delete(tasks::delete_task))
            .route("/api/tasks/{id}/phase", post(tasks::update_task_phase))
            .route("/api/tasks/{id}/logs", get(tasks::get_task_logs))
            .route(
                "/api/tasks/{id}/execute",
                post(pipeline::execute_task_pipeline),
            )
            .route("/api/tasks/{id}/build-logs", get(pipeline::get_build_logs))
            .route(
                "/api/tasks/{id}/build-status",
                get(pipeline::get_build_status),
            )
            .route(
                "/api/pipeline/queue",
                get(pipeline::get_pipeline_queue_status),
            )
            .route("/api/terminals", get(terminal_ws::list_terminals))
            .route("/api/terminals", post(terminal_ws::create_terminal))
            .route(
                "/api/terminals/{id}",
                axum::routing::delete(terminal_ws::delete_terminal),
            )
            .route("/ws/terminal/{id}", get(terminal_ws::terminal_ws))
            .route(
                "/api/terminals/{id}/settings",
                patch(terminal_ws::update_terminal_settings),
            )
            .route(
                "/api/terminals/{id}/auto-name",
                post(terminal_ws::auto_name_terminal),
            )
            .route(
                "/api/terminals/persistent",
                get(terminal_ws::list_persistent_terminals),
            )
            .route("/api/settings", get(settings::get_settings))
            .route("/api/settings", put(settings::put_settings))
            .route("/api/settings", patch(settings::patch_settings))
            .route("/api/credentials/status", get(misc::get_credentials_status))
            .route("/api/github/sync", post(github::trigger_github_sync))
            .route("/api/github/sync/status", get(github::get_sync_status))
            .route("/api/github/issues", get(github::list_github_issues))
            .route(
                "/api/github/issues/{number}/import",
                post(github::import_github_issue),
            )
            .route("/api/github/prs", get(github::list_github_prs))
            .route("/api/github/pr/{task_id}", post(github::create_pr_for_task))
            // GitHub OAuth
            .route(
                "/api/github/oauth/authorize",
                get(github::github_oauth_authorize),
            )
            .route(
                "/api/github/oauth/callback",
                post(github::github_oauth_callback),
            )
            .route("/api/github/oauth/status", get(github::github_oauth_status))
            .route(
                "/api/github/oauth/revoke",
                post(github::github_oauth_revoke),
            )
            .route(
                "/api/github/oauth/refresh",
                post(github::github_oauth_refresh),
            )
            // GitLab integration
            .route("/api/gitlab/issues", get(integrations::list_gitlab_issues))
            .route(
                "/api/gitlab/merge-requests",
                get(integrations::list_gitlab_merge_requests),
            )
            .route(
                "/api/gitlab/merge-requests/{iid}/review",
                post(integrations::review_gitlab_merge_request),
            )
            // Linear integration
            .route("/api/linear/issues", get(integrations::list_linear_issues))
            .route(
                "/api/linear/import",
                post(integrations::import_linear_issues),
            )
            .route("/api/kanban/columns", get(kanban::get_kanban_columns))
            .route("/api/kanban/columns", patch(kanban::patch_kanban_columns))
            .route(
                "/api/kanban/poker/start",
                post(kanban::start_planning_poker),
            )
            .route(
                "/api/kanban/poker/vote",
                post(kanban::submit_planning_poker_vote),
            )
            .route(
                "/api/kanban/poker/reveal",
                post(kanban::reveal_planning_poker),
            )
            .route(
                "/api/kanban/poker/simulate",
                post(kanban::simulate_planning_poker),
            )
            .route(
                "/api/kanban/poker/{bead_id}",
                get(kanban::get_planning_poker_session),
            )
            // MCP servers
            .route("/api/mcp/servers", get(mcp::list_mcp_servers))
            .route("/api/mcp/tools/call", post(mcp::call_mcp_tool))
            // Worktrees
            .route("/api/worktrees", get(worktrees::list_worktrees))
            .route(
                "/api/worktrees/{id}",
                axum::routing::delete(worktrees::delete_worktree),
            )
            .route("/api/worktrees/{id}/merge", post(worktrees::merge_worktree))
            .route(
                "/api/worktrees/{id}/merge-preview",
                get(worktrees::merge_preview),
            )
            .route(
                "/api/worktrees/{id}/resolve",
                post(worktrees::resolve_conflict),
            )
            // Agent Queue
            .route("/api/queue", get(queue::list_queue))
            .route("/api/queue/reorder", post(queue::reorder_queue))
            .route(
                "/api/queue/{task_id}/prioritize",
                post(queue::prioritize_task),
            )
            // Direct mode
            .route("/api/settings/direct-mode", post(misc::toggle_direct_mode))
            // Costs
            .route("/api/costs", get(misc::get_costs))
            // CLI availability
            .route("/api/cli/available", get(misc::list_available_clis))
            // Agent sessions
            .route("/api/sessions", get(misc::list_agent_sessions))
            // Convoys
            .route("/api/convoys", get(misc::list_convoys))
            // Notification endpoints
            .route("/api/notifications", get(notifications::list_notifications))
            .route(
                "/api/notifications/count",
                get(notifications::notification_count),
            )
            .route(
                "/api/notifications/{id}/read",
                post(notifications::mark_notification_read),
            )
            .route(
                "/api/notifications/read-all",
                post(notifications::mark_all_notifications_read),
            )
            .route(
                "/api/notifications/{id}",
                axum::routing::delete(notifications::delete_notification),
            )
            // Metrics endpoints
            .route("/api/metrics", get(metrics::get_metrics_prometheus))
            .route("/api/metrics/json", get(metrics::get_metrics_json))
            // Session endpoints
            .route("/api/sessions/ui", get(sessions::get_ui_session))
            .route("/api/sessions/ui", put(sessions::save_ui_session))
            .route("/api/sessions/ui/list", get(sessions::list_ui_sessions))
            // Projects
            .route("/api/projects", get(projects::list_projects))
            .route("/api/projects", post(projects::create_project))
            .route("/api/projects/{id}", put(projects::update_project))
            .route(
                "/api/projects/{id}",
                axum::routing::delete(projects::delete_project),
            )
            .route(
                "/api/projects/{id}/activate",
                post(projects::activate_project),
            )
            // PR polling
            .route("/api/github/pr/{number}/watch", post(github::watch_pr))
            .route(
                "/api/github/pr/{number}/watch",
                axum::routing::delete(github::unwatch_pr),
            )
            .route("/api/github/pr/watched", get(github::list_watched_prs))
            // GitHub releases
            .route("/api/github/releases", post(github::create_release))
            .route("/api/github/releases", get(github::list_releases))
            // Task archival
            .route("/api/tasks/{id}/archive", post(misc::archive_task))
            .route("/api/tasks/{id}/unarchive", post(misc::unarchive_task))
            .route("/api/tasks/archived", get(misc::list_archived_tasks))
            // Attachments
            .route(
                "/api/tasks/{task_id}/attachments",
                get(misc::list_attachments),
            )
            .route(
                "/api/tasks/{task_id}/attachments",
                post(misc::add_attachment),
            )
            .route(
                "/api/tasks/{task_id}/attachments/{id}",
                axum::routing::delete(misc::delete_attachment),
            )
            // Task drafts
            .route("/api/tasks/drafts", get(misc::list_task_drafts))
            .route("/api/tasks/drafts", post(misc::save_task_draft))
            .route("/api/tasks/drafts/{id}", get(misc::get_task_draft))
            .route(
                "/api/tasks/drafts/{id}",
                axum::routing::delete(misc::delete_task_draft),
            )
            // Kanban column locking
            .route("/api/kanban/columns/lock", post(misc::lock_column))
            // Task ordering
            .route("/api/kanban/ordering", post(misc::save_task_ordering))
            // File watching
            .route("/api/files/watch", post(misc::start_file_watch))
            .route("/api/files/unwatch", post(misc::stop_file_watch))
            // Competitor analysis
            .route(
                "/api/roadmap/competitor-analysis",
                post(misc::run_competitor_analysis),
            )
            // Profile swap notification
            .route(
                "/api/notifications/profile-swap",
                post(misc::notify_profile_swap),
            )
            // App update check
            .route("/api/notifications/app-update", get(misc::check_app_update))
            // WebSocket endpoints
            .route("/ws", get(websocket::ws_handler))
            .route("/api/events/ws", get(websocket::events_ws_handler))
            .merge(intelligence_api::intelligence_router())
            .layer(axum_middleware::from_fn(metrics_middleware))
            .layer(axum_middleware::from_fn(request_id_middleware))
            .layer(axum_middleware::from_fn(isolation_headers_middleware))
            .layer(AuthLayer::new(api_key))
            .layer(
                CorsLayer::new()
                    .allow_origin(tower_http::cors::AllowOrigin::predicate(
                        move |origin: &axum::http::HeaderValue,
                              _request_parts: &axum::http::request::Parts| {
                            if let Ok(origin_str) = origin.to_str() {
                                if origin_str.starts_with("http://localhost")
                                    || origin_str.starts_with("http://127.0.0.1")
                                    || origin_str.starts_with("https://localhost")
                                    || origin_str.starts_with("https://127.0.0.1")
                                {
                                    return true;
                                }
                                allowed_origins.iter().any(|allowed| origin_str == allowed)
                            } else {
                                false
                            }
                        },
                    ))
                    .allow_methods([
                        axum::http::Method::GET,
                        axum::http::Method::POST,
                        axum::http::Method::PUT,
                        axum::http::Method::DELETE,
                        axum::http::Method::PATCH,
                        axum::http::Method::OPTIONS,
                    ])
                    .allow_headers([
                        axum::http::header::CONTENT_TYPE,
                        axum::http::header::AUTHORIZATION,
                    ])
                    .allow_credentials(true),
            )
            .with_state(state)
    }
}

// ---------------------------------------------------------------------------
// OAuth token refresh monitor
// ---------------------------------------------------------------------------

mod oauth_monitor {
    use super::state::ApiState;
    use at_integrations::github::oauth as gh_oauth;
    use std::sync::Arc;

    /// Spawn a background task to monitor OAuth token expiration and refresh when needed.
    ///
    /// This task runs every 5 minutes and checks if the OAuth token needs to be refreshed
    /// (i.e., will expire within the next 5 minutes). If refresh is needed, it attempts
    /// to refresh the token using GitHub's refresh_token mechanism.
    ///
    /// # Arguments
    /// * `state` - The shared API state containing the OAuth token manager
    ///
    /// # Example
    /// ```no_run
    /// use std::sync::Arc;
    /// use at_bridge::http_api::{ApiState, spawn_oauth_token_refresh_monitor};
    /// use at_bridge::event_bus::EventBus;
    ///
    /// # async fn example() {
    /// let event_bus = EventBus::new();
    /// let state = Arc::new(ApiState::new(event_bus));
    /// spawn_oauth_token_refresh_monitor(state);
    /// # }
    /// ```
    pub fn spawn_oauth_token_refresh_monitor(state: Arc<ApiState>) {
        tokio::spawn(async move {
            use std::time::Duration;
            use tracing::{debug, info, warn};

            let mut interval = tokio::time::interval(Duration::from_secs(300));
            interval.tick().await;

            info!("OAuth token refresh monitor started");

            loop {
                interval.tick().await;

                debug!("Checking OAuth token expiration status");

                let token_manager = state.oauth_token_manager.read().await;

                if token_manager.should_refresh().await {
                    info!("OAuth token approaching expiration, attempting refresh");

                    let client_id = match std::env::var("GITHUB_OAUTH_CLIENT_ID") {
                        Ok(v) if !v.is_empty() => v,
                        _ => {
                            warn!("Cannot refresh OAuth token: GITHUB_OAUTH_CLIENT_ID not set");
                            continue;
                        }
                    };

                    let client_secret = match std::env::var("GITHUB_OAUTH_CLIENT_SECRET") {
                        Ok(v) if !v.is_empty() => v,
                        _ => {
                            warn!("Cannot refresh OAuth token: GITHUB_OAUTH_CLIENT_SECRET not set");
                            continue;
                        }
                    };

                    let redirect_uri =
                        std::env::var("GITHUB_OAUTH_REDIRECT_URI").unwrap_or_else(|_| {
                            "http://localhost:3000/api/github/oauth/callback".into()
                        });

                    let scopes = std::env::var("GITHUB_OAUTH_SCOPES")
                        .unwrap_or_else(|_| "repo,read:user,user:email".into())
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect::<Vec<_>>();

                    let oauth_config = gh_oauth::GitHubOAuthConfig {
                        client_id,
                        client_secret,
                        redirect_uri,
                        scopes,
                    };

                    let _oauth_client = gh_oauth::GitHubOAuthClient::new(oauth_config);

                    warn!(
                        "OAuth token needs refresh but refresh_token support not yet implemented. \
                         This will be added in subtask-3-2. User will need to re-authenticate."
                    );

                    drop(token_manager);
                } else {
                    debug!("OAuth token is valid, no refresh needed");
                }
            }
        });
    }
}
