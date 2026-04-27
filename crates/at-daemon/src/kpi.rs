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

#[cfg(test)]
mod tests {
    use super::*;
    use at_core::cache::CacheDb;
    use at_core::types::{Bead, BeadStatus, KpiSnapshot, Lane};

    /// Build a Bead in the requested status. We start from `Bead::new` (which
    /// creates a Backlog bead) and adjust the status field directly so we can
    /// produce any state without going through transition validation. This is
    /// fine for cache-layer tests because `compute_kpi_snapshot` simply groups
    /// rows by their stored `status` column.
    fn bead_with_status(title: &str, status: BeadStatus) -> Bead {
        let mut b = Bead::new(title.to_string(), Lane::Standard);
        b.status = status;
        b
    }

    #[test]
    fn test_kpi_collector_new_and_default_construct() {
        // Both constructors compile and return values without panic.
        let _ = KpiCollector::new();
        let _ = KpiCollector;
        // `Default` is wired up; we verify it routes through `new`.
        let _: KpiCollector = Default::default();
    }

    #[tokio::test]
    async fn test_collect_snapshot_empty_cache_yields_zeros() {
        let cache = CacheDb::new_in_memory().await.expect("in-memory cache");
        let collector = KpiCollector::new();
        let snap = collector
            .collect_snapshot(&cache)
            .await
            .expect("snapshot ok");
        assert_eq!(snap.total_beads, 0);
        assert_eq!(snap.backlog, 0);
        assert_eq!(snap.hooked, 0);
        assert_eq!(snap.slung, 0);
        assert_eq!(snap.review, 0);
        assert_eq!(snap.done, 0);
        assert_eq!(snap.failed, 0);
        assert_eq!(snap.escalated, 0);
        assert_eq!(snap.active_agents, 0);
    }

    #[tokio::test]
    async fn test_collect_snapshot_counts_each_status_bucket() {
        let cache = CacheDb::new_in_memory().await.expect("in-memory cache");
        // Insert a known mix: 2 backlog, 1 hooked, 3 done, 1 failed.
        for (title, status) in [
            ("a", BeadStatus::Backlog),
            ("b", BeadStatus::Backlog),
            ("c", BeadStatus::Hooked),
            ("d", BeadStatus::Done),
            ("e", BeadStatus::Done),
            ("f", BeadStatus::Done),
            ("g", BeadStatus::Failed),
        ] {
            let bead = bead_with_status(title, status);
            cache.upsert_bead(&bead).await.expect("upsert");
        }

        let collector = KpiCollector::new();
        let snap = collector
            .collect_snapshot(&cache)
            .await
            .expect("snapshot ok");

        assert_eq!(snap.total_beads, 7);
        assert_eq!(snap.backlog, 2);
        assert_eq!(snap.hooked, 1);
        assert_eq!(snap.slung, 0);
        assert_eq!(snap.review, 0);
        assert_eq!(snap.done, 3);
        assert_eq!(snap.failed, 1);
        assert_eq!(snap.escalated, 0);
        // No agents inserted ⇒ active_agents must be 0.
        assert_eq!(snap.active_agents, 0);
    }

    #[tokio::test]
    async fn test_collect_snapshot_total_equals_sum_of_buckets() {
        let cache = CacheDb::new_in_memory().await.expect("in-memory cache");
        for (i, status) in [
            BeadStatus::Backlog,
            BeadStatus::Hooked,
            BeadStatus::Slung,
            BeadStatus::Review,
            BeadStatus::Done,
            BeadStatus::Failed,
            BeadStatus::Escalated,
        ]
        .into_iter()
        .enumerate()
        {
            let bead = bead_with_status(&format!("b{}", i), status);
            cache.upsert_bead(&bead).await.expect("upsert");
        }

        let collector = KpiCollector::new();
        let snap = collector
            .collect_snapshot(&cache)
            .await
            .expect("snapshot ok");

        // Invariant: total = sum of all per-status buckets.
        let sum = snap.backlog
            + snap.hooked
            + snap.slung
            + snap.review
            + snap.done
            + snap.failed
            + snap.escalated;
        assert_eq!(snap.total_beads, sum);
        assert_eq!(snap.total_beads, 7);
    }

    /// `KpiSnapshot` is `Serialize + Deserialize` (defined in `at-core`). We
    /// own the collector, so we test the envelope round-trips through JSON
    /// without losing any field. We compare field-by-field because
    /// `KpiSnapshot` does not derive `PartialEq` upstream.
    #[tokio::test]
    async fn test_kpi_snapshot_json_round_trip_preserves_fields() {
        let cache = CacheDb::new_in_memory().await.expect("in-memory cache");
        let bead = bead_with_status("only", BeadStatus::Review);
        cache.upsert_bead(&bead).await.expect("upsert");

        let collector = KpiCollector::new();
        let original = collector
            .collect_snapshot(&cache)
            .await
            .expect("snapshot ok");

        let json = serde_json::to_string(&original).expect("serialize");
        let decoded: KpiSnapshot =
            serde_json::from_str(&json).expect("deserialize");

        assert_eq!(decoded.total_beads, original.total_beads);
        assert_eq!(decoded.backlog, original.backlog);
        assert_eq!(decoded.hooked, original.hooked);
        assert_eq!(decoded.slung, original.slung);
        assert_eq!(decoded.review, original.review);
        assert_eq!(decoded.done, original.done);
        assert_eq!(decoded.failed, original.failed);
        assert_eq!(decoded.escalated, original.escalated);
        assert_eq!(decoded.active_agents, original.active_agents);
        assert_eq!(decoded.timestamp, original.timestamp);
    }

    #[tokio::test]
    async fn test_kpi_snapshot_json_field_names_are_snake_case() {
        // Pin the on-the-wire shape so a serde rename in at-core is caught here.
        let cache = CacheDb::new_in_memory().await.expect("in-memory cache");
        let collector = KpiCollector::new();
        let snap = collector
            .collect_snapshot(&cache)
            .await
            .expect("snapshot ok");
        let v = serde_json::to_value(&snap).expect("to_value");
        let obj = v.as_object().expect("object");
        for key in [
            "total_beads",
            "backlog",
            "hooked",
            "slung",
            "review",
            "done",
            "failed",
            "escalated",
            "active_agents",
            "timestamp",
        ] {
            assert!(
                obj.contains_key(key),
                "expected key {} in KpiSnapshot JSON, got {:?}",
                key,
                obj.keys().collect::<Vec<_>>()
            );
        }
    }

    #[tokio::test]
    async fn test_two_consecutive_snapshots_are_monotonic_in_timestamp() {
        let cache = CacheDb::new_in_memory().await.expect("in-memory cache");
        let collector = KpiCollector::new();
        let s1 = collector
            .collect_snapshot(&cache)
            .await
            .expect("snapshot 1");
        // Yield the runtime so a fresh `Utc::now()` will return >= s1.timestamp.
        tokio::task::yield_now().await;
        let s2 = collector
            .collect_snapshot(&cache)
            .await
            .expect("snapshot 2");
        assert!(s2.timestamp >= s1.timestamp);
    }
}
