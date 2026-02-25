//! Miscellaneous API endpoints.
//!
//! This module contains routes that don't fit into a specific domain category,
//! including status, beads, agents, KPI, metrics, sessions, integrations
//! (Linear, GitLab), MCP servers, CLI tools, costs, convoys, and file watching.
//!
//! All routes are prefixed with `/api/` and must be merged into the main
//! router using `.merge()`.

use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;

use crate::http_api::{
    call_mcp_tool, create_bead, get_costs, get_kpi, get_metrics_json, get_metrics_prometheus,
    get_status, get_ui_session, import_linear_issues, list_agent_sessions, list_agents,
    list_available_clis, list_beads, list_convoys, list_gitlab_issues, list_gitlab_merge_requests,
    list_linear_issues, list_mcp_servers, list_ui_sessions, nudge_agent,
    review_gitlab_merge_request, save_ui_session, start_file_watch, stop_agent, stop_file_watch,
    update_bead_status, ApiState,
};

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Build the miscellaneous routes sub-router.
///
/// Includes endpoints for:
/// - Status: System status information
/// - Beads: Story/feature planning beads
/// - Agents: Agent management and control
/// - KPI: Key performance indicators
/// - Metrics: Prometheus and JSON metrics
/// - Sessions: UI session management
/// - Linear: Linear issue integration
/// - GitLab: GitLab issues and merge requests
/// - MCP: Model Context Protocol servers and tools
/// - CLI: CLI availability checking
/// - Costs: Cost tracking and reporting
/// - Convoys: Convoy management
/// - Files: File watching/unwatching
///
/// All routes are mounted under `/api/` — the caller is responsible for
/// merging this into the top-level router.
pub fn misc_router() -> Router<Arc<ApiState>> {
    Router::new()
        // Status
        .route("/api/status", get(get_status))
        // Beads
        .route("/api/beads", get(list_beads))
        .route("/api/beads", post(create_bead))
        .route("/api/beads/{id}/status", post(update_bead_status))
        // Agents
        .route("/api/agents", get(list_agents))
        .route("/api/agents/{id}/nudge", post(nudge_agent))
        .route("/api/agents/{id}/stop", post(stop_agent))
        // KPI
        .route("/api/kpi", get(get_kpi))
        // Metrics
        .route("/api/metrics", get(get_metrics_prometheus))
        .route("/api/metrics/json", get(get_metrics_json))
        // Sessions
        .route("/api/sessions", get(list_agent_sessions))
        .route("/api/sessions/ui", get(get_ui_session))
        .route("/api/sessions/ui", post(save_ui_session))
        .route("/api/sessions/ui/list", get(list_ui_sessions))
        // Linear integration
        .route("/api/linear/issues", get(list_linear_issues))
        .route("/api/linear/import", post(import_linear_issues))
        // GitLab integration
        .route("/api/gitlab/issues", get(list_gitlab_issues))
        .route(
            "/api/gitlab/merge-requests",
            get(list_gitlab_merge_requests),
        )
        .route(
            "/api/gitlab/merge-requests/{iid}/review",
            post(review_gitlab_merge_request),
        )
        // MCP servers
        .route("/api/mcp/servers", get(list_mcp_servers))
        .route("/api/mcp/tools/call", post(call_mcp_tool))
        // CLI availability
        .route("/api/cli/available", get(list_available_clis))
        // Costs
        .route("/api/costs", get(get_costs))
        // Convoys
        .route("/api/convoys", get(list_convoys))
        // File watching
        .route("/api/files/watch", post(start_file_watch))
        .route("/api/files/unwatch", post(stop_file_watch))
}
