use anyhow::Result;
use at_bridge::event_bus::EventBus;
use at_bridge::http_api::ApiState;
use at_bridge::protocol::{BridgeMessage, KpiPayload};
use at_core::cache::CacheDb;
use at_core::types::KpiSnapshot;
use tracing::info;

/// Collects KPI snapshots.
///
/// The daemon uses [`KpiCollector::refresh_live`], which reads the live
/// beads and agents held by [`ApiState`]. [`KpiCollector::collect_snapshot`]
/// reads `CacheDb`, which the HTTP/MCP API does not write to.
pub struct KpiCollector;

impl KpiCollector {
    /// Create a new KPI collector.
    pub fn new() -> Self {
        Self
    }

    /// Collect a KPI snapshot from the cache and log it.
    ///
    /// Delegates to [`CacheDb::compute_kpi_snapshot`] and emits a structured
    /// tracing event with the key metrics.
    pub async fn collect_snapshot(&self, cache: &CacheDb) -> Result<KpiSnapshot> {
        let snapshot = cache
            .compute_kpi_snapshot()
            .await
            .map_err(|e| anyhow::anyhow!("failed to compute kpi snapshot: {}", e))?;

        info!(
            total_beads = snapshot.total_beads,
            backlog = snapshot.backlog,
            hooked = snapshot.hooked,
            slung = snapshot.slung,
            review = snapshot.review,
            done = snapshot.done,
            failed = snapshot.failed,
            escalated = snapshot.escalated,
            active_agents = snapshot.active_agents,
            timestamp = %snapshot.timestamp,
            "kpi snapshot"
        );

        Ok(snapshot)
    }
}

impl KpiCollector {
    /// Compute a snapshot from the live [`ApiState`], store it in
    /// `api_state.kpi` and broadcast it as a `KpiUpdate`.
    pub async fn refresh_live(&self, api_state: &ApiState, event_bus: &EventBus) -> KpiSnapshot {
        let snapshot = api_state.compute_kpi().await;
        info!(
            total_beads = snapshot.total_beads,
            backlog = snapshot.backlog,
            hooked = snapshot.hooked,
            slung = snapshot.slung,
            review = snapshot.review,
            done = snapshot.done,
            failed = snapshot.failed,
            escalated = snapshot.escalated,
            active_agents = snapshot.active_agents,
            timestamp = %snapshot.timestamp,
            "kpi snapshot"
        );
        *api_state.kpi.write().await = snapshot.clone();
        event_bus.publish(BridgeMessage::KpiUpdate(KpiPayload {
            total_beads: snapshot.total_beads,
            backlog: snapshot.backlog,
            hooked: snapshot.hooked,
            slung: snapshot.slung,
            review: snapshot.review,
            done: snapshot.done,
            failed: snapshot.failed,
            active_agents: snapshot.active_agents,
        }));
        snapshot
    }
}

impl Default for KpiCollector {
    fn default() -> Self {
        Self::new()
    }
}
