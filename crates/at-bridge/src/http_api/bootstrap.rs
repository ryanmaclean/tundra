use axum::{extract::State, Json};
use serde::Serialize;
use std::sync::Arc;

use at_core::types::{Agent, Bead, BeadStatus, KpiSnapshot};

use super::state::ApiState;

#[derive(Serialize)]
pub(crate) struct BootstrapResponse {
    beads: Vec<Bead>,
    agents: Vec<Agent>,
    kpi: KpiSnapshot,
    server_version: &'static str,
    uptime_seconds: u64,
}

/// GET /api/bootstrap -- single-request startup snapshot for the TUI.
///
/// Acquires all required read locks concurrently via `tokio::join!` to avoid
/// serialised lock acquisition overhead. Excludes GitHub endpoints (external
/// API calls) and git worktrees (blocking shell invocation).
pub(crate) async fn get_bootstrap(State(state): State<Arc<ApiState>>) -> Json<BootstrapResponse> {
    let (beads_guard, agents_guard) = tokio::join!(state.beads.read(), state.agents.read());

    let (backlog, hooked, slung, review, done, failed, escalated) = beads_guard.values().fold(
        (0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64),
        |(bl, ho, sl, rv, dn, fa, es), b| match b.status {
            BeadStatus::Backlog => (bl + 1, ho, sl, rv, dn, fa, es),
            BeadStatus::Hooked => (bl, ho + 1, sl, rv, dn, fa, es),
            BeadStatus::Slung => (bl, ho, sl + 1, rv, dn, fa, es),
            BeadStatus::Review => (bl, ho, sl, rv + 1, dn, fa, es),
            BeadStatus::Done => (bl, ho, sl, rv, dn + 1, fa, es),
            BeadStatus::Failed => (bl, ho, sl, rv, dn, fa + 1, es),
            BeadStatus::Escalated => (bl, ho, sl, rv, dn, fa, es + 1),
        },
    );

    let kpi = KpiSnapshot {
        total_beads: beads_guard.len() as u64,
        backlog,
        hooked,
        slung,
        review,
        done,
        failed,
        escalated,
        active_agents: agents_guard.len() as u64,
        timestamp: chrono::Utc::now(),
    };

    Json(BootstrapResponse {
        beads: beads_guard.values().cloned().collect(),
        agents: agents_guard.values().cloned().collect(),
        kpi,
        server_version: env!("CARGO_PKG_VERSION"),
        uptime_seconds: state.start_time.elapsed().as_secs(),
    })
}
