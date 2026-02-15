use anyhow::Result;
use at_core::cache::CacheDb;
use at_core::types::KpiSnapshot;
use tracing::info;

/// Collects KPI snapshots from the cache database.
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

impl Default for KpiCollector {
    fn default() -> Self {
        Self::new()
    }
}
