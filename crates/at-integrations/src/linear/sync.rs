//! Bidirectional sync engine for Linear ↔ auto-tundra.
//!
//! Supports pushing local task updates to Linear and pulling Linear issue
//! changes back into the local bead store.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{LinearClient, Result};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Direction of a sync operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncDirection {
    Push,
    Pull,
    Bidirectional,
}

/// Result of a single sync cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    pub direction: SyncDirection,
    pub pushed: u32,
    pub pulled: u32,
    pub conflicts: u32,
    pub dead_lettered: u32,
    pub synced_at: DateTime<Utc>,
}

/// Configuration for automatic sync scheduling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    pub direction: SyncDirection,
    pub interval_seconds: u64,
    pub team_id: Option<String>,
    pub auto_resolve_conflicts: bool,
    /// Maximum number of push retries before a change is moved to the dead
    /// letter queue. Defaults to 3.
    pub max_retries: u32,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            direction: SyncDirection::Bidirectional,
            interval_seconds: 300, // 5 minutes
            team_id: None,
            auto_resolve_conflicts: false,
            max_retries: 3,
        }
    }
}

/// A pending change that hasn't been synced yet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingChange {
    pub id: String,
    pub direction: SyncDirection,
    pub entity_type: String,
    pub entity_id: String,
    pub change_type: String,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Sync engine
// ---------------------------------------------------------------------------

/// An entry in the dead letter queue: the original change, its accumulated
/// retry count, and the stringified error from the last attempt.
pub type DeadLetter = (PendingChange, u32, String);

/// Manages bidirectional synchronization between Linear and auto-tundra.
pub struct LinearSyncEngine {
    client: LinearClient,
    config: SyncConfig,
    last_sync: Option<DateTime<Utc>>,
    pending_changes: Vec<PendingChange>,
    /// Retry counts keyed by change id.
    retry_counts: std::collections::HashMap<String, u32>,
    /// Changes that exhausted all retries.
    dead_letter: Vec<DeadLetter>,
    /// Entity IDs that should be treated as failing (test-only).
    #[cfg(test)]
    fail_ids: std::collections::HashSet<String>,
}

impl LinearSyncEngine {
    pub fn new(client: LinearClient, config: SyncConfig) -> Self {
        Self {
            client,
            config,
            last_sync: None,
            pending_changes: Vec::new(),
            retry_counts: std::collections::HashMap::new(),
            dead_letter: Vec::new(),
            #[cfg(test)]
            fail_ids: std::collections::HashSet::new(),
        }
    }

    /// Mark an entity ID as always-failing (test helper).
    #[cfg(test)]
    pub fn set_fail_ids(&mut self, ids: impl IntoIterator<Item = String>) {
        self.fail_ids = ids.into_iter().collect();
    }

    /// Run a full sync cycle.
    ///
    /// 1. **Push** – process each `PendingChange` by calling `update_issue`
    ///    on the Linear API (or stub).
    /// 2. **Pull** – fetch issues via `list_issues` and count those updated
    ///    since `last_sync`. On the very first sync all issues count as pulled.
    /// 3. **Conflict detection** – if an issue appears in both the push set
    ///    (by `entity_id`) and the pull set (updated remotely), increment the
    ///    conflict counter. When `auto_resolve_conflicts` is true the remote
    ///    version wins (the local change is simply dropped).
    pub async fn sync(&mut self) -> Result<SyncResult> {
        let mut pushed: u32 = 0;
        let mut dead_lettered: u32 = 0;
        let mut conflicts: u32 = 0;

        // -- Push phase: process pending changes --------------------------------
        let changes = std::mem::take(&mut self.pending_changes);

        // Collect entity_ids that were pushed so we can detect conflicts later.
        let mut pushed_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

        let should_push = matches!(
            self.config.direction,
            SyncDirection::Push | SyncDirection::Bidirectional
        );

        if should_push {
            for change in changes {
                // Map change_type to update_issue parameters.
                let (title, state, desc) = match change.change_type.as_str() {
                    "title_update" => (Some(change.entity_id.as_str()), None, None),
                    "status_update" => (None, Some("Updated"), None),
                    "description_update" => (None, None, Some("updated via sync")),
                    _ => (None, Some("Updated"), None),
                };

                // In test builds, honour the fail_ids set to simulate errors.
                #[cfg(test)]
                let result: std::result::Result<(), String> =
                    if self.fail_ids.contains(&change.entity_id) {
                        Err("simulated push failure".to_string())
                    } else {
                        self.client
                            .update_issue(&change.entity_id, title, state, desc)
                            .await
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                    };

                #[cfg(not(test))]
                let result: std::result::Result<(), String> = self
                    .client
                    .update_issue(&change.entity_id, title, state, desc)
                    .await
                    .map(|_| ())
                    .map_err(|e| e.to_string());

                match result {
                    Ok(()) => {
                        pushed += 1;
                        pushed_ids.insert(change.entity_id.clone());
                        // Successful — clear any accumulated retry count.
                        self.retry_counts.remove(&change.id);
                    }
                    Err(err) => {
                        let count = self.retry_counts.entry(change.id.clone()).or_insert(0);
                        *count += 1;

                        if *count >= self.config.max_retries {
                            let final_count = *count;
                            self.retry_counts.remove(&change.id);
                            self.dead_letter.push((change, final_count, err));
                            dead_lettered += 1;
                        } else {
                            // Put the change back for the next sync cycle.
                            self.pending_changes.push(change);
                        }
                    }
                }
            }
        }

        // -- Pull phase: fetch remote issues ------------------------------------
        let should_pull = matches!(
            self.config.direction,
            SyncDirection::Pull | SyncDirection::Bidirectional
        );

        let mut pulled: u32 = 0;

        if should_pull {
            let issues = self
                .client
                .list_issues(self.config.team_id.as_deref(), None)
                .await?;

            for issue in &issues {
                // Only count issues updated since last sync. If this is the
                // first sync (last_sync is None) everything counts as new.
                let dominated_by_last_sync = match self.last_sync {
                    Some(ts) => issue.updated_at <= ts,
                    None => false,
                };

                if dominated_by_last_sync {
                    continue;
                }

                // Conflict: same issue was both pushed locally and updated
                // remotely since last sync.
                if pushed_ids.contains(&issue.id) {
                    conflicts += 1;
                    if self.config.auto_resolve_conflicts {
                        // Remote wins – the push already happened but we count
                        // the pull (remote version is authoritative).
                        pulled += 1;
                    }
                    // When auto_resolve is false we still count the conflict
                    // but do NOT count it as pulled (caller decides resolution).
                } else {
                    pulled += 1;
                }
            }
        }

        let synced_at = Utc::now();
        self.last_sync = Some(synced_at);

        Ok(SyncResult {
            direction: self.config.direction,
            pushed,
            pulled,
            conflicts,
            dead_lettered,
            synced_at,
        })
    }

    /// Get the last sync timestamp.
    pub fn last_sync_time(&self) -> Option<DateTime<Utc>> {
        self.last_sync
    }

    /// Queue a local change for the next push.
    pub fn queue_change(&mut self, change: PendingChange) {
        self.pending_changes.push(change);
    }

    /// Return pending (un-synced) changes.
    pub fn pending_changes(&self) -> &[PendingChange] {
        &self.pending_changes
    }

    /// Update the sync configuration.
    pub fn set_config(&mut self, config: SyncConfig) {
        self.config = config;
    }

    /// Select which Linear team to sync with.
    pub fn set_team(&mut self, team_id: Option<String>) {
        self.config.team_id = team_id;
    }

    /// View the dead letter queue.
    pub fn dead_letter_queue(&self) -> &[DeadLetter] {
        &self.dead_letter
    }

    /// Move all dead-lettered changes back into `pending_changes` for another
    /// round of retries. Retry counts are reset to zero.
    pub fn retry_dead_letters(&mut self) {
        for (change, _count, _err) in self.dead_letter.drain(..) {
            self.retry_counts.remove(&change.id);
            self.pending_changes.push(change);
        }
    }

    /// Discard all dead-lettered changes.
    pub fn clear_dead_letters(&mut self) {
        self.dead_letter.clear();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_client() -> LinearClient {
        LinearClient::new("test_key").unwrap()
    }

    #[test]
    fn sync_config_defaults() {
        let cfg = SyncConfig::default();
        assert_eq!(cfg.direction, SyncDirection::Bidirectional);
        assert_eq!(cfg.interval_seconds, 300);
        assert!(cfg.team_id.is_none());
        assert!(!cfg.auto_resolve_conflicts);
    }

    #[tokio::test]
    async fn sync_cycle_stub() {
        let client = test_client();
        let mut engine = LinearSyncEngine::new(client, SyncConfig::default());
        let result = engine.sync().await.unwrap();

        assert_eq!(result.direction, SyncDirection::Bidirectional);
        assert!(result.pulled > 0);
        assert!(engine.last_sync_time().is_some());
        assert!(engine.pending_changes().is_empty());
    }

    #[test]
    fn queue_pending_changes() {
        let client = test_client();
        let mut engine = LinearSyncEngine::new(client, SyncConfig::default());

        engine.queue_change(PendingChange {
            id: "ch-1".into(),
            direction: SyncDirection::Push,
            entity_type: "task".into(),
            entity_id: "task-001".into(),
            change_type: "status_update".into(),
            created_at: Utc::now(),
        });

        assert_eq!(engine.pending_changes().len(), 1);
    }

    #[test]
    fn set_team_filter() {
        let client = test_client();
        let mut engine = LinearSyncEngine::new(client, SyncConfig::default());
        engine.set_team(Some("team-42".into()));
        assert_eq!(engine.config.team_id.as_deref(), Some("team-42"));
    }

    #[test]
    fn sync_result_serde_roundtrip() {
        let r = SyncResult {
            direction: SyncDirection::Pull,
            pushed: 0,
            pulled: 5,
            conflicts: 1,
            dead_lettered: 0,
            synced_at: Utc::now(),
        };
        let json = serde_json::to_string(&r).unwrap();
        let de: SyncResult = serde_json::from_str(&json).unwrap();
        assert_eq!(de.pulled, 5);
        assert_eq!(de.conflicts, 1);
        assert_eq!(de.dead_lettered, 0);
    }

    // -- Retry / dead letter tests ------------------------------------------

    fn make_change(id: &str, entity_id: &str) -> PendingChange {
        PendingChange {
            id: id.into(),
            direction: SyncDirection::Push,
            entity_type: "task".into(),
            entity_id: entity_id.into(),
            change_type: "status_update".into(),
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn failed_push_stays_in_pending_for_retry() {
        let client = test_client();
        let cfg = SyncConfig { direction: SyncDirection::Push, max_retries: 3, ..Default::default() };
        let mut engine = LinearSyncEngine::new(client, cfg);
        engine.set_fail_ids(["entity-bad".to_string()]);

        engine.queue_change(make_change("ch-1", "entity-bad"));
        let result = engine.sync().await.unwrap();

        // First failure: change goes back to pending (retry_count=1 < 3).
        assert_eq!(result.pushed, 0);
        assert_eq!(result.dead_lettered, 0);
        assert_eq!(engine.pending_changes().len(), 1);
        assert!(engine.dead_letter_queue().is_empty());
    }

    #[tokio::test]
    async fn exhausted_retries_goes_to_dead_letter() {
        let client = test_client();
        let cfg = SyncConfig { direction: SyncDirection::Push, max_retries: 2, ..Default::default() };
        let mut engine = LinearSyncEngine::new(client, cfg);
        engine.set_fail_ids(["entity-bad".to_string()]);

        engine.queue_change(make_change("ch-1", "entity-bad"));

        // Attempt 1: retry_count becomes 1 (< 2), stays pending.
        let r1 = engine.sync().await.unwrap();
        assert_eq!(r1.dead_lettered, 0);
        assert_eq!(engine.pending_changes().len(), 1);

        // Attempt 2: retry_count becomes 2 (>= 2), dead-lettered.
        let r2 = engine.sync().await.unwrap();
        assert_eq!(r2.dead_lettered, 1);
        assert!(engine.pending_changes().is_empty());
        assert_eq!(engine.dead_letter_queue().len(), 1);
        assert_eq!(engine.dead_letter_queue()[0].0.id, "ch-1");
        assert_eq!(engine.dead_letter_queue()[0].1, 2); // retry count
    }

    #[tokio::test]
    async fn retry_dead_letters_moves_back_to_pending() {
        let client = test_client();
        let cfg = SyncConfig { direction: SyncDirection::Push, max_retries: 1, ..Default::default() };
        let mut engine = LinearSyncEngine::new(client, cfg);
        engine.set_fail_ids(["entity-bad".to_string()]);

        engine.queue_change(make_change("ch-1", "entity-bad"));
        engine.sync().await.unwrap();

        assert_eq!(engine.dead_letter_queue().len(), 1);
        assert!(engine.pending_changes().is_empty());

        engine.retry_dead_letters();

        assert!(engine.dead_letter_queue().is_empty());
        assert_eq!(engine.pending_changes().len(), 1);
        assert_eq!(engine.pending_changes()[0].id, "ch-1");
    }

    #[test]
    fn clear_dead_letters_empties_queue() {
        let client = test_client();
        let mut engine = LinearSyncEngine::new(client, SyncConfig::default());

        // Manually push a dead letter entry for the test.
        engine
            .dead_letter
            .push((make_change("ch-dl", "ent-dl"), 3, "some error".to_string()));
        assert_eq!(engine.dead_letter_queue().len(), 1);

        engine.clear_dead_letters();
        assert!(engine.dead_letter_queue().is_empty());
    }

    #[tokio::test]
    async fn successful_push_clears_retry_count() {
        let client = test_client();
        let cfg = SyncConfig { direction: SyncDirection::Push, max_retries: 3, ..Default::default() };
        let mut engine = LinearSyncEngine::new(client, cfg);
        engine.set_fail_ids(["entity-flaky".to_string()]);

        // Queue and fail once.
        engine.queue_change(make_change("ch-1", "entity-flaky"));
        engine.sync().await.unwrap();
        assert_eq!(engine.pending_changes().len(), 1);

        // "Fix" the entity so it succeeds next time.
        engine.fail_ids.clear();
        let result = engine.sync().await.unwrap();
        assert_eq!(result.pushed, 1);
        assert_eq!(result.dead_lettered, 0);
        assert!(engine.pending_changes().is_empty());
        assert!(engine.dead_letter_queue().is_empty());
    }
}
