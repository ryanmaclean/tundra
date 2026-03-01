use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::sync::{RwLock, Semaphore};
use uuid::Uuid;

use at_core::session_store::SessionStore;
use at_core::settings::SettingsManager;
use at_core::types::{Agent, Bead, BeadStatus, CliType, KpiSnapshot, RetentionConfig};
use at_harness::rate_limiter::{MultiKeyRateLimiter, RateLimitConfig};
use at_intelligence::{
    changelog::ChangelogEngine, ideation::IdeationEngine, insights::InsightsEngine,
    memory::MemoryStore, roadmap::RoadmapEngine,
};

use crate::event_bus::EventBus;
use crate::notifications::NotificationStore;
use crate::oauth_token_manager::OAuthTokenManager;
use crate::terminal::TerminalRegistry;

use super::types::{
    Attachment, KanbanColumn, KanbanColumnConfig, PlanningPokerSession, PrPollStatus, Project,
    SyncStatus, TaskDraft,
};

use at_integrations::types::GitHubRelease;

// ---------------------------------------------------------------------------
// Default configuration functions
// ---------------------------------------------------------------------------

pub(crate) fn default_kanban_columns() -> KanbanColumnConfig {
    KanbanColumnConfig {
        columns: vec![
            KanbanColumn {
                id: "backlog".into(),
                label: "Backlog".into(),
                width_px: Some(200),
            },
            KanbanColumn {
                id: "queue".into(),
                label: "Queue".into(),
                width_px: Some(180),
            },
            KanbanColumn {
                id: "in_progress".into(),
                label: "In Progress".into(),
                width_px: Some(220),
            },
            KanbanColumn {
                id: "review".into(),
                label: "Review".into(),
                width_px: Some(180),
            },
            KanbanColumn {
                id: "qa".into(),
                label: "QA".into(),
                width_px: Some(160),
            },
            KanbanColumn {
                id: "done".into(),
                label: "Done".into(),
                width_px: Some(180),
            },
            KanbanColumn {
                id: "pr_created".into(),
                label: "PR Created".into(),
                width_px: Some(180),
            },
            KanbanColumn {
                id: "error".into(),
                label: "Error".into(),
                width_px: Some(160),
            },
        ],
    }
}

/// Shared application state for all HTTP/WS handlers.
pub struct ApiState {
    pub event_bus: EventBus,
    pub beads: Arc<RwLock<std::collections::HashMap<Uuid, Bead>>>,
    pub agents: Arc<RwLock<std::collections::HashMap<Uuid, Agent>>>,
    pub kpi: Arc<RwLock<KpiSnapshot>>,
    pub tasks: Arc<RwLock<std::collections::HashMap<Uuid, at_core::types::Task>>>,
    /// Queue gate for task pipeline execution.
    pub pipeline_semaphore: Arc<Semaphore>,
    /// Max number of concurrently executing task pipelines.
    pub pipeline_max_concurrent: usize,
    /// Number of task executions waiting for a pipeline permit.
    pub pipeline_waiting: Arc<AtomicUsize>,
    /// Number of task executions currently running.
    pub pipeline_running: Arc<AtomicUsize>,
    /// Cached count of beads for lock-free status queries.
    pub bead_count: Arc<AtomicUsize>,
    /// Cached count of agents for lock-free status queries.
    pub agent_count: Arc<AtomicUsize>,
    /// Cached count of tasks for lock-free status queries.
    pub task_count: Arc<AtomicUsize>,
    pub start_time: std::time::Instant,
    pub pty_pool: Option<Arc<at_session::pty_pool::PtyPool>>,
    pub terminal_registry: Arc<RwLock<TerminalRegistry>>,
    /// Active PTY handles keyed by terminal ID.
    pub pty_handles: Arc<RwLock<std::collections::HashMap<Uuid, at_session::pty_pool::PtyHandle>>>,
    /// Settings persistence manager.
    pub settings_manager: Arc<SettingsManager>,
    /// GitHub sync status tracking.
    pub sync_status: Arc<RwLock<SyncStatus>>,
    // ---- Intelligence engines ------------------------------------------------
    pub insights_engine: Arc<RwLock<InsightsEngine>>,
    pub ideation_engine: Arc<RwLock<IdeationEngine>>,
    pub roadmap_engine: Arc<RwLock<RoadmapEngine>>,
    pub memory_store: Arc<RwLock<MemoryStore>>,
    pub changelog_engine: Arc<RwLock<ChangelogEngine>>,
    // ---- Notifications -------------------------------------------------------
    pub notification_store: Arc<RwLock<NotificationStore>>,
    // ---- Session persistence --------------------------------------------------
    pub session_store: Arc<SessionStore>,
    /// Kanban column config (8 columns: Backlog, Queue, In Progress, …, PR Created, Error).
    pub kanban_columns: Arc<RwLock<KanbanColumnConfig>>,
    /// Planning-poker sessions keyed by bead id.
    pub planning_poker_sessions: Arc<RwLock<std::collections::HashMap<Uuid, PlanningPokerSession>>>,
    // ---- GitHub OAuth ---------------------------------------------------
    pub github_oauth_token: Arc<RwLock<Option<String>>>,
    pub github_oauth_user: Arc<RwLock<Option<serde_json::Value>>>,
    /// Pending OAuth state parameters for CSRF protection.
    pub oauth_pending_states: Arc<RwLock<std::collections::HashMap<String, String>>>,
    pub oauth_token_manager: Arc<RwLock<OAuthTokenManager>>,
    // ---- Projects --------------------------------------------------------
    pub projects: Arc<RwLock<Vec<Project>>>,
    // ---- PR polling -------------------------------------------------------
    pub pr_poll_registry: Arc<RwLock<std::collections::HashMap<u32, PrPollStatus>>>,
    // ---- GitHub releases --------------------------------------------------
    pub releases: Arc<RwLock<Vec<GitHubRelease>>>,
    // ---- Task archival ----------------------------------------------------
    pub archived_tasks: Arc<RwLock<Vec<Uuid>>>,
    // ---- Attachments ------------------------------------------------------
    pub attachments: Arc<RwLock<Vec<Attachment>>>,
    // ---- Task drafts ------------------------------------------------------
    pub task_drafts: Arc<RwLock<std::collections::HashMap<Uuid, TaskDraft>>>,
    // ---- Disconnect buffers for terminal WS reconnection ------------------
    pub disconnect_buffers:
        Arc<RwLock<std::collections::HashMap<Uuid, crate::terminal::DisconnectBuffer>>>,
    // ---- Rate limiting -------------------------------------------------------
    /// Multi-tier rate limiter (global, per-user, per-endpoint).
    pub rate_limiter: Arc<MultiKeyRateLimiter>,
    // ---- Retention configuration ------------------------------------------
    /// Memory retention policies for cleanup (TTL, max entries, cleanup intervals).
    pub retention_config: Arc<RwLock<RetentionConfig>>,
}

impl ApiState {
    /// Create a new `ApiState` with empty collections and a fresh event bus.
    pub fn new(event_bus: EventBus) -> Self {
        let pipeline_max_concurrent = std::env::var("AT_PIPELINE_MAX_CONCURRENT")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(1);

        Self {
            event_bus,
            beads: Arc::new(RwLock::new(std::collections::HashMap::new())),
            agents: Arc::new(RwLock::new(std::collections::HashMap::new())),
            kpi: Arc::new(RwLock::new(KpiSnapshot {
                total_beads: 0,
                backlog: 0,
                hooked: 0,
                slung: 0,
                review: 0,
                done: 0,
                failed: 0,
                escalated: 0,
                active_agents: 0,
                timestamp: chrono::Utc::now(),
            })),
            tasks: Arc::new(RwLock::new(std::collections::HashMap::new())),
            pipeline_semaphore: Arc::new(Semaphore::new(pipeline_max_concurrent)),
            pipeline_max_concurrent,
            pipeline_waiting: Arc::new(AtomicUsize::new(0)),
            pipeline_running: Arc::new(AtomicUsize::new(0)),
            bead_count: Arc::new(AtomicUsize::new(0)),
            agent_count: Arc::new(AtomicUsize::new(0)),
            task_count: Arc::new(AtomicUsize::new(0)),
            start_time: std::time::Instant::now(),
            pty_pool: None,
            terminal_registry: Arc::new(RwLock::new(TerminalRegistry::new())),
            pty_handles: Arc::new(RwLock::new(std::collections::HashMap::new())),
            settings_manager: Arc::new(SettingsManager::default_path()),
            sync_status: Arc::new(RwLock::new(SyncStatus::default())),
            insights_engine: Arc::new(RwLock::new(InsightsEngine::new())),
            ideation_engine: Arc::new(RwLock::new(IdeationEngine::new())),
            roadmap_engine: Arc::new(RwLock::new(RoadmapEngine::new())),
            memory_store: Arc::new(RwLock::new(MemoryStore::new())),
            changelog_engine: Arc::new(RwLock::new(ChangelogEngine::new())),
            notification_store: Arc::new(RwLock::new(NotificationStore::default())),
            session_store: Arc::new(SessionStore::default_path()),
            kanban_columns: Arc::new(RwLock::new(default_kanban_columns())),
            planning_poker_sessions: Arc::new(RwLock::new(std::collections::HashMap::new())),
            github_oauth_token: Arc::new(RwLock::new(None)),
            github_oauth_user: Arc::new(RwLock::new(None)),
            oauth_pending_states: Arc::new(RwLock::new(std::collections::HashMap::new())),
            oauth_token_manager: Arc::new(RwLock::new(OAuthTokenManager::new())),
            pr_poll_registry: Arc::new(RwLock::new(std::collections::HashMap::new())),
            releases: Arc::new(RwLock::new(Vec::new())),
            archived_tasks: Arc::new(RwLock::new(Vec::new())),
            projects: Arc::new(RwLock::new(vec![Project {
                id: Uuid::new_v4(),
                name: "auto-tundra".to_string(),
                path: std::env::current_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| ".".to_string()),
                created_at: chrono::Utc::now().to_rfc3339(),
                is_active: true,
            }])),
            attachments: Arc::new(RwLock::new(Vec::new())),
            task_drafts: Arc::new(RwLock::new(std::collections::HashMap::new())),
            disconnect_buffers: Arc::new(RwLock::new(std::collections::HashMap::new())),
            // ---- Rate Limiter Configuration -------------------------------------
            // Three-tier rate limiting protects the API from abuse and overload:
            //
            // 1. Global Limit: 100 requests/minute across ALL clients
            //    - Prevents total server overload
            //    - First line of defense against DoS attacks
            //    - Shared bucket for entire API
            //
            // 2. Per-User Limit: 20 requests/minute per client IP
            //    - Prevents single client monopolization
            //    - IP extracted from X-Forwarded-For or X-Real-IP headers
            //    - Each IP gets independent bucket
            //
            // 3. Per-Endpoint Limit: 10 requests/minute per URI path
            //    - Prevents abuse of expensive endpoints (AI, GitHub sync)
            //    - Each endpoint (e.g., /api/tasks, /api/beads) tracked separately
            //    - Allows high-frequency status polling on cheap endpoints
            //
            // To adjust limits:
            // - Use RateLimitConfig::per_second(n), per_minute(n), or per_hour(n)
            // - For production: increase global and per-user limits
            // - For development: use per_second(n) for faster iteration
            //
            // When exceeded, middleware returns HTTP 429 with Retry-After header.
            rate_limiter: Arc::new(MultiKeyRateLimiter::new(
                RateLimitConfig::per_minute(100), // Global tier
                RateLimitConfig::per_minute(20),  // Per-user tier
                RateLimitConfig::per_minute(10),  // Per-endpoint tier
            )),
            retention_config: Arc::new(RwLock::new(RetentionConfig::default())),
        }
    }

    /// Create a new `ApiState` with a PTY pool for terminal support.
    pub fn with_pty_pool(
        event_bus: EventBus,
        pty_pool: Arc<at_session::pty_pool::PtyPool>,
    ) -> Self {
        let mut state = Self::new(event_bus);
        state.pty_pool = Some(pty_pool);
        state
    }

    /// Clean up archived tasks that are older than the specified TTL.
    ///
    /// Removes tasks from the tasks HashMap if they are:
    /// 1. In the archived_tasks list
    /// 2. Have a completed_at timestamp
    /// 3. completed_at is older than ttl_secs
    ///
    /// Returns the number of tasks removed.
    pub async fn cleanup_archived_tasks(&self, ttl_secs: u64) -> usize {
        let archived = self.archived_tasks.read().await;
        let mut tasks = self.tasks.write().await;

        let now = chrono::Utc::now();
        let cutoff = now - chrono::Duration::seconds(ttl_secs as i64);

        let mut removed_count = 0;
        let mut tasks_to_remove = Vec::new();

        // Identify tasks to remove
        for (task_id, task) in tasks.iter() {
            if archived.contains(task_id) {
                if let Some(completed_at) = task.completed_at {
                    if completed_at < cutoff {
                        tasks_to_remove.push(*task_id);
                    }
                }
            }
        }

        // Remove identified tasks
        for task_id in tasks_to_remove {
            tasks.remove(&task_id);
            removed_count += 1;
        }

        // Update task count atomic
        self.task_count.store(tasks.len(), Ordering::Relaxed);

        removed_count
    }

    /// Clean up disconnect buffers that are older than the specified TTL.
    ///
    /// Removes disconnect buffers from the HashMap if their disconnected_at
    /// timestamp is older than ttl_secs. This prevents unbounded memory growth
    /// from abandoned WebSocket connections.
    ///
    /// Returns the number of buffers removed.
    pub async fn cleanup_disconnect_buffers(&self, ttl_secs: u64) -> usize {
        let mut buffers = self.disconnect_buffers.write().await;

        let now = chrono::Utc::now();
        let cutoff = now - chrono::Duration::seconds(ttl_secs as i64);

        let mut removed_count = 0;
        let mut buffers_to_remove = Vec::new();

        // Identify buffers to remove
        for (terminal_id, buffer) in buffers.iter() {
            if buffer.disconnected_at < cutoff {
                buffers_to_remove.push(*terminal_id);
            }
        }

        // Remove identified buffers
        for terminal_id in buffers_to_remove {
            buffers.remove(&terminal_id);
            removed_count += 1;
        }

        removed_count
    }

    /// Start a background cleanup task that periodically removes expired data.
    ///
    /// This method spawns a tokio task that runs at the interval specified in
    /// the retention config, cleaning up:
    /// - Archived tasks older than task_ttl_secs
    /// - Disconnect buffers older than disconnect_buffer_ttl_secs
    /// - Notifications older than task_ttl_secs (reusing task TTL for notifications)
    ///
    /// The background task runs until the process exits. It logs the number of
    /// items removed during each cleanup cycle.
    pub fn start_cleanup_task(self: &Arc<Self>) {
        let state = Arc::clone(self);

        tokio::spawn(async move {
            // Read the cleanup interval from retention config
            let interval_secs = {
                let config = state.retention_config.read().await;
                config.cleanup_interval_secs
            };

            let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            interval.tick().await; // First tick completes immediately

            tracing::info!(
                interval_secs,
                "Background cleanup task started"
            );

            loop {
                interval.tick().await;

                // Capture counts before cleanup for metrics
                let tasks_before = state.task_count.load(Ordering::Relaxed);
                let buffers_before = state.disconnect_buffers.read().await.len();
                let notifications_before = state.notification_store.read().await.total_count();

                // Read TTLs from retention config
                let (task_ttl, buffer_ttl) = {
                    let config = state.retention_config.read().await;
                    (config.task_ttl_secs, config.disconnect_buffer_ttl_secs)
                };

                // Run all cleanup operations
                let tasks_removed = state.cleanup_archived_tasks(task_ttl).await;
                let buffers_removed = state.cleanup_disconnect_buffers(buffer_ttl).await;

                // Cleanup notifications (using task TTL)
                let notifications_removed = {
                    let mut notification_store = state.notification_store.write().await;
                    notification_store.cleanup_old(task_ttl)
                };

                // Capture counts after cleanup
                let tasks_after = state.task_count.load(Ordering::Relaxed);
                let buffers_after = state.disconnect_buffers.read().await.len();
                let notifications_after = state.notification_store.read().await.total_count();

                tracing::info!(
                    tasks_before,
                    tasks_removed,
                    tasks_after,
                    buffers_before,
                    buffers_removed,
                    buffers_after,
                    notifications_before,
                    notifications_removed,
                    notifications_after,
                    task_ttl_secs = task_ttl,
                    buffer_ttl_secs = buffer_ttl,
                    "Cleanup cycle completed"
                );
            }
        });
    }

    /// Seed lightweight demo data for local development/web UI previews.
    ///
    /// No-op when beads are already present.
    pub async fn seed_demo_data(&self) {
        let mut beads = self.beads.write().await;
        if !beads.is_empty() {
            return;
        }

        let mut b1 = Bead::new("Set up stacked diffs", at_core::types::Lane::Standard);
        b1.description = Some("Enable parent/child task stacks and branch chaining.".into());
        b1.status = BeadStatus::Backlog;
        b1.priority = 2;
        b1.metadata = Some(serde_json::json!({"tags":["feature","stacks"]}));

        let mut b2 = Bead::new(
            "Wire GitLab + Linear live sync",
            at_core::types::Lane::Standard,
        );
        b2.description = Some("Replace stubs with real integration calls and retries.".into());
        b2.status = BeadStatus::Hooked;
        b2.priority = 3;
        b2.metadata = Some(serde_json::json!({"tags":["integration","sync"]}));

        let mut b3 = Bead::new("Polish Tahoe native shell", at_core::types::Lane::Critical);
        b3.description = Some("Hybrid native chrome with HIG-aligned interactions.".into());
        b3.status = BeadStatus::Review;
        b3.priority = 4;
        b3.metadata = Some(serde_json::json!({"tags":["native-ux","macos"]}));

        beads.insert(b1.id, b1);
        beads.insert(b2.id, b2);
        beads.insert(b3.id, b3);

        let mut agents = self.agents.write().await;
        if agents.is_empty() {
            let agent1 = Agent::new("Crew", at_core::types::AgentRole::Crew, CliType::Claude);
            let agent2 = Agent::new(
                "Reviewer",
                at_core::types::AgentRole::SpecCritic,
                CliType::Claude,
            );
            agents.insert(agent1.id, agent1);
            agents.insert(agent2.id, agent2);
        }

        let snapshot = KpiSnapshot {
            total_beads: beads.len() as u64,
            backlog: beads
                .values()
                .filter(|b| b.status == BeadStatus::Backlog)
                .count() as u64,
            hooked: beads
                .values()
                .filter(|b| b.status == BeadStatus::Hooked)
                .count() as u64,
            slung: beads
                .values()
                .filter(|b| b.status == BeadStatus::Slung)
                .count() as u64,
            review: beads
                .values()
                .filter(|b| b.status == BeadStatus::Review)
                .count() as u64,
            done: beads
                .values()
                .filter(|b| b.status == BeadStatus::Done)
                .count() as u64,
            failed: beads
                .values()
                .filter(|b| b.status == BeadStatus::Failed)
                .count() as u64,
            escalated: beads
                .values()
                .filter(|b| b.status == BeadStatus::Escalated)
                .count() as u64,
            active_agents: agents.len() as u64,
            timestamp: chrono::Utc::now(),
        };
        *self.kpi.write().await = snapshot;

        // Initialize atomic counters to reflect seeded demo data
        self.bead_count.store(beads.len(), Ordering::Relaxed);
        self.agent_count.store(agents.len(), Ordering::Relaxed);
        let tasks = self.tasks.read().await;
        self.task_count.store(tasks.len(), Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use at_core::types::{Task, TaskPhase};
    use chrono::{Duration, Utc};

    fn create_test_state() -> ApiState {
        ApiState::new(EventBus::new())
    }

    fn create_test_task(id: Uuid, completed_at: Option<chrono::DateTime<Utc>>) -> Task {
        let bead_id = Uuid::new_v4();
        Task {
            id,
            title: "Test Task".to_string(),
            description: None,
            bead_id,
            phase: TaskPhase::Complete,
            progress_percent: 100,
            subtasks: vec![],
            worktree_path: None,
            git_branch: None,
            category: at_core::types::TaskCategory::Feature,
            priority: at_core::types::TaskPriority::Medium,
            complexity: at_core::types::TaskComplexity::Medium,
            impact: None,
            agent_profile: None,
            phase_configs: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            started_at: None,
            completed_at,
            error: None,
            logs: vec![],
            qa_report: None,
            source: None,
            parent_task_id: None,
            stack_position: None,
            pr_number: None,
            build_logs: vec![],
        }
    }

    #[tokio::test]
    async fn test_cleanup_archived_tasks_removes_old_archived_tasks() {
        let state = create_test_state();
        let task_id = Uuid::new_v4();

        // Create a task that was completed 10 days ago
        let old_completed_at = Utc::now() - Duration::days(10);
        let task = create_test_task(task_id, Some(old_completed_at));

        // Add task to tasks HashMap
        state.tasks.write().await.insert(task_id, task);
        state.task_count.store(1, Ordering::Relaxed);

        // Archive the task
        state.archived_tasks.write().await.push(task_id);

        // Cleanup with TTL of 7 days (604800 seconds)
        let removed = state.cleanup_archived_tasks(7 * 24 * 60 * 60).await;

        assert_eq!(removed, 1);
        assert_eq!(state.tasks.read().await.len(), 0);
        assert_eq!(state.task_count.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn test_cleanup_archived_tasks_keeps_recent_archived_tasks() {
        let state = create_test_state();
        let task_id = Uuid::new_v4();

        // Create a task that was completed 5 days ago
        let recent_completed_at = Utc::now() - Duration::days(5);
        let task = create_test_task(task_id, Some(recent_completed_at));

        // Add task to tasks HashMap
        state.tasks.write().await.insert(task_id, task);
        state.task_count.store(1, Ordering::Relaxed);

        // Archive the task
        state.archived_tasks.write().await.push(task_id);

        // Cleanup with TTL of 7 days (task is only 5 days old)
        let removed = state.cleanup_archived_tasks(7 * 24 * 60 * 60).await;

        assert_eq!(removed, 0);
        assert_eq!(state.tasks.read().await.len(), 1);
        assert_eq!(state.task_count.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_cleanup_archived_tasks_keeps_non_archived_tasks() {
        let state = create_test_state();
        let task_id = Uuid::new_v4();

        // Create a task that was completed 10 days ago but is NOT archived
        let old_completed_at = Utc::now() - Duration::days(10);
        let task = create_test_task(task_id, Some(old_completed_at));

        // Add task to tasks HashMap but don't archive it
        state.tasks.write().await.insert(task_id, task);
        state.task_count.store(1, Ordering::Relaxed);

        // Cleanup with TTL of 7 days
        let removed = state.cleanup_archived_tasks(7 * 24 * 60 * 60).await;

        assert_eq!(removed, 0);
        assert_eq!(state.tasks.read().await.len(), 1);
        assert_eq!(state.task_count.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_cleanup_archived_tasks_keeps_archived_tasks_without_completed_at() {
        let state = create_test_state();
        let task_id = Uuid::new_v4();

        // Create a task without completed_at timestamp
        let task = create_test_task(task_id, None);

        // Add task to tasks HashMap and archive it
        state.tasks.write().await.insert(task_id, task);
        state.task_count.store(1, Ordering::Relaxed);
        state.archived_tasks.write().await.push(task_id);

        // Cleanup with TTL of 7 days
        let removed = state.cleanup_archived_tasks(7 * 24 * 60 * 60).await;

        assert_eq!(removed, 0);
        assert_eq!(state.tasks.read().await.len(), 1);
        assert_eq!(state.task_count.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_cleanup_archived_tasks_handles_multiple_tasks() {
        let state = create_test_state();

        // Create 4 tasks with different scenarios
        let old_archived_id1 = Uuid::new_v4();
        let old_archived_id2 = Uuid::new_v4();
        let recent_archived_id = Uuid::new_v4();
        let non_archived_id = Uuid::new_v4();

        let old_completed_at = Utc::now() - Duration::days(10);
        let recent_completed_at = Utc::now() - Duration::days(5);

        // Old archived task 1
        let task1 = create_test_task(old_archived_id1, Some(old_completed_at));
        state.tasks.write().await.insert(old_archived_id1, task1);
        state.archived_tasks.write().await.push(old_archived_id1);

        // Old archived task 2
        let task2 = create_test_task(old_archived_id2, Some(old_completed_at));
        state.tasks.write().await.insert(old_archived_id2, task2);
        state.archived_tasks.write().await.push(old_archived_id2);

        // Recent archived task
        let task3 = create_test_task(recent_archived_id, Some(recent_completed_at));
        state.tasks.write().await.insert(recent_archived_id, task3);
        state.archived_tasks.write().await.push(recent_archived_id);

        // Non-archived task
        let task4 = create_test_task(non_archived_id, Some(old_completed_at));
        state.tasks.write().await.insert(non_archived_id, task4);

        state.task_count.store(4, Ordering::Relaxed);

        // Cleanup with TTL of 7 days
        let removed = state.cleanup_archived_tasks(7 * 24 * 60 * 60).await;

        assert_eq!(removed, 2);
        assert_eq!(state.tasks.read().await.len(), 2);
        assert_eq!(state.task_count.load(Ordering::Relaxed), 2);

        // Verify the correct tasks remain
        let tasks = state.tasks.read().await;
        assert!(tasks.contains_key(&recent_archived_id));
        assert!(tasks.contains_key(&non_archived_id));
        assert!(!tasks.contains_key(&old_archived_id1));
        assert!(!tasks.contains_key(&old_archived_id2));
    }

    #[tokio::test]
    async fn test_cleanup_archived_tasks_empty_state() {
        let state = create_test_state();

        // Cleanup with no tasks
        let removed = state.cleanup_archived_tasks(7 * 24 * 60 * 60).await;

        assert_eq!(removed, 0);
        assert_eq!(state.tasks.read().await.len(), 0);
        assert_eq!(state.task_count.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn test_cleanup_archived_tasks_with_zero_ttl() {
        let state = create_test_state();
        let task_id = Uuid::new_v4();

        // Create a task that was just completed
        let just_completed_at = Utc::now();
        let task = create_test_task(task_id, Some(just_completed_at));

        // Add task to tasks HashMap and archive it
        state.tasks.write().await.insert(task_id, task);
        state.task_count.store(1, Ordering::Relaxed);
        state.archived_tasks.write().await.push(task_id);

        // Cleanup with TTL of 0 seconds (should remove all archived tasks with completed_at)
        let removed = state.cleanup_archived_tasks(0).await;

        assert_eq!(removed, 1);
        assert_eq!(state.tasks.read().await.len(), 0);
        assert_eq!(state.task_count.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn test_cleanup_disconnect_buffers_removes_old_buffers() {
        let state = create_test_state();
        let terminal_id = Uuid::new_v4();

        // Create a buffer that was disconnected 10 minutes ago
        let mut buffer = crate::terminal::DisconnectBuffer::new(1024);
        buffer.disconnected_at = Utc::now() - Duration::minutes(10);

        // Add buffer to disconnect_buffers HashMap
        state
            .disconnect_buffers
            .write()
            .await
            .insert(terminal_id, buffer);

        // Cleanup with TTL of 5 minutes (300 seconds)
        let removed = state.cleanup_disconnect_buffers(5 * 60).await;

        assert_eq!(removed, 1);
        assert_eq!(state.disconnect_buffers.read().await.len(), 0);
    }

    #[tokio::test]
    async fn test_cleanup_disconnect_buffers_keeps_recent_buffers() {
        let state = create_test_state();
        let terminal_id = Uuid::new_v4();

        // Create a buffer that was disconnected 3 minutes ago
        let mut buffer = crate::terminal::DisconnectBuffer::new(1024);
        buffer.disconnected_at = Utc::now() - Duration::minutes(3);

        // Add buffer to disconnect_buffers HashMap
        state
            .disconnect_buffers
            .write()
            .await
            .insert(terminal_id, buffer);

        // Cleanup with TTL of 5 minutes (buffer is only 3 minutes old)
        let removed = state.cleanup_disconnect_buffers(5 * 60).await;

        assert_eq!(removed, 0);
        assert_eq!(state.disconnect_buffers.read().await.len(), 1);
    }

    #[tokio::test]
    async fn test_cleanup_disconnect_buffers_handles_multiple_buffers() {
        let state = create_test_state();

        // Create 3 buffers with different ages
        let old_buffer_id1 = Uuid::new_v4();
        let old_buffer_id2 = Uuid::new_v4();
        let recent_buffer_id = Uuid::new_v4();

        // Old buffer 1 (10 minutes ago)
        let mut buffer1 = crate::terminal::DisconnectBuffer::new(1024);
        buffer1.disconnected_at = Utc::now() - Duration::minutes(10);
        state
            .disconnect_buffers
            .write()
            .await
            .insert(old_buffer_id1, buffer1);

        // Old buffer 2 (15 minutes ago)
        let mut buffer2 = crate::terminal::DisconnectBuffer::new(1024);
        buffer2.disconnected_at = Utc::now() - Duration::minutes(15);
        state
            .disconnect_buffers
            .write()
            .await
            .insert(old_buffer_id2, buffer2);

        // Recent buffer (3 minutes ago)
        let mut buffer3 = crate::terminal::DisconnectBuffer::new(1024);
        buffer3.disconnected_at = Utc::now() - Duration::minutes(3);
        state
            .disconnect_buffers
            .write()
            .await
            .insert(recent_buffer_id, buffer3);

        // Cleanup with TTL of 5 minutes
        let removed = state.cleanup_disconnect_buffers(5 * 60).await;

        assert_eq!(removed, 2);
        assert_eq!(state.disconnect_buffers.read().await.len(), 1);

        // Verify the correct buffer remains
        let buffers = state.disconnect_buffers.read().await;
        assert!(buffers.contains_key(&recent_buffer_id));
        assert!(!buffers.contains_key(&old_buffer_id1));
        assert!(!buffers.contains_key(&old_buffer_id2));
    }

    #[tokio::test]
    async fn test_cleanup_disconnect_buffers_empty_state() {
        let state = create_test_state();

        // Cleanup with no buffers
        let removed = state.cleanup_disconnect_buffers(5 * 60).await;

        assert_eq!(removed, 0);
        assert_eq!(state.disconnect_buffers.read().await.len(), 0);
    }

    #[tokio::test]
    async fn test_cleanup_disconnect_buffers_with_zero_ttl() {
        let state = create_test_state();
        let terminal_id = Uuid::new_v4();

        // Create a buffer that was just disconnected
        let buffer = crate::terminal::DisconnectBuffer::new(1024);

        // Add buffer to disconnect_buffers HashMap
        state
            .disconnect_buffers
            .write()
            .await
            .insert(terminal_id, buffer);

        // Cleanup with TTL of 0 seconds (should remove all buffers)
        let removed = state.cleanup_disconnect_buffers(0).await;

        assert_eq!(removed, 1);
        assert_eq!(state.disconnect_buffers.read().await.len(), 0);
    }

    #[tokio::test]
    async fn test_cleanup_disconnect_buffers_exact_ttl_boundary() {
        let state = create_test_state();
        let terminal_id = Uuid::new_v4();

        // Create a buffer that was disconnected exactly 5 minutes ago
        let mut buffer = crate::terminal::DisconnectBuffer::new(1024);
        buffer.disconnected_at = Utc::now() - Duration::minutes(5);

        // Add buffer to disconnect_buffers HashMap
        state
            .disconnect_buffers
            .write()
            .await
            .insert(terminal_id, buffer);

        // Cleanup with TTL of 5 minutes (buffer is exactly at the boundary)
        // The buffer should be removed because disconnected_at < cutoff
        let removed = state.cleanup_disconnect_buffers(5 * 60).await;

        assert_eq!(removed, 1);
        assert_eq!(state.disconnect_buffers.read().await.len(), 0);
    }

    #[tokio::test]
    async fn test_cleanup_disconnect_buffers_preserves_data() {
        let state = create_test_state();
        let terminal_id = Uuid::new_v4();

        // Create a buffer with some data
        let mut buffer = crate::terminal::DisconnectBuffer::new(1024);
        buffer.push(b"test data");
        buffer.disconnected_at = Utc::now() - Duration::minutes(3);

        // Add buffer to disconnect_buffers HashMap
        state
            .disconnect_buffers
            .write()
            .await
            .insert(terminal_id, buffer);

        // Cleanup with TTL of 5 minutes (should not remove)
        let removed = state.cleanup_disconnect_buffers(5 * 60).await;

        assert_eq!(removed, 0);

        // Verify data is still present
        let buffers = state.disconnect_buffers.read().await;
        let remaining_buffer = buffers.get(&terminal_id).unwrap();
        assert_eq!(remaining_buffer.data.len(), 9); // "test data" is 9 bytes
    }

    #[tokio::test]
    async fn test_background_cleanup_starts_successfully() {
        let state = Arc::new(create_test_state());

        // Starting the background task should not panic
        state.start_cleanup_task();

        // Give the task a moment to start
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Test passes if we get here without panicking
        assert!(true);
    }

    #[tokio::test]
    async fn test_background_cleanup_removes_old_data() {
        let state = Arc::new(create_test_state());

        // Set a very short cleanup interval for testing (1 second)
        {
            let mut config = state.retention_config.write().await;
            config.cleanup_interval_secs = 1;
            config.task_ttl_secs = 0; // Immediate cleanup
            config.disconnect_buffer_ttl_secs = 0; // Immediate cleanup
        }

        // Create old archived task
        let task_id = Uuid::new_v4();
        let old_task = create_test_task(task_id, Some(Utc::now() - Duration::days(10)));
        state.tasks.write().await.insert(task_id, old_task);
        state.archived_tasks.write().await.push(task_id);
        state.task_count.store(1, Ordering::Relaxed);

        // Create old disconnect buffer
        let terminal_id = Uuid::new_v4();
        let mut buffer = crate::terminal::DisconnectBuffer::new(1024);
        buffer.disconnected_at = Utc::now() - Duration::minutes(10);
        state
            .disconnect_buffers
            .write()
            .await
            .insert(terminal_id, buffer);

        // Note: We cannot easily create old notifications for testing because
        // the notifications field is private and we can't manipulate timestamps.
        // The cleanup will run but won't remove anything since all notifications
        // will be recent. This is acceptable for this test - we're verifying
        // the cleanup runs without errors.

        // Verify data exists before cleanup
        assert_eq!(state.tasks.read().await.len(), 1);
        assert_eq!(state.disconnect_buffers.read().await.len(), 1);

        // Start the background cleanup task
        state.start_cleanup_task();

        // Wait for cleanup to run (interval is 1 second + buffer time)
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

        // Verify data was cleaned up
        assert_eq!(state.tasks.read().await.len(), 0);
        assert_eq!(state.disconnect_buffers.read().await.len(), 0);
        // Note: Notifications won't be cleaned because we can't create old ones in tests
    }

    #[tokio::test]
    async fn test_background_cleanup_respects_retention_config() {
        let state = Arc::new(create_test_state());

        // Set a very short cleanup interval but long TTLs for testing
        {
            let mut config = state.retention_config.write().await;
            config.cleanup_interval_secs = 1;
            config.task_ttl_secs = 7 * 24 * 60 * 60; // 7 days
            config.disconnect_buffer_ttl_secs = 5 * 60; // 5 minutes
        }

        // Create recent archived task (5 days old, should be kept)
        let task_id = Uuid::new_v4();
        let recent_task = create_test_task(task_id, Some(Utc::now() - Duration::days(5)));
        state.tasks.write().await.insert(task_id, recent_task);
        state.archived_tasks.write().await.push(task_id);
        state.task_count.store(1, Ordering::Relaxed);

        // Create recent disconnect buffer (3 minutes old, should be kept)
        let terminal_id = Uuid::new_v4();
        let mut buffer = crate::terminal::DisconnectBuffer::new(1024);
        buffer.disconnected_at = Utc::now() - Duration::minutes(3);
        state
            .disconnect_buffers
            .write()
            .await
            .insert(terminal_id, buffer);

        // Verify data exists before cleanup
        assert_eq!(state.tasks.read().await.len(), 1);
        assert_eq!(state.disconnect_buffers.read().await.len(), 1);

        // Start the background cleanup task
        state.start_cleanup_task();

        // Wait for cleanup to run
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

        // Verify recent data was NOT removed (respects TTL)
        assert_eq!(state.tasks.read().await.len(), 1);
        assert_eq!(state.disconnect_buffers.read().await.len(), 1);
    }

    #[tokio::test]
    async fn test_background_cleanup_multiple_cycles() {
        let state = Arc::new(create_test_state());

        // Set a very short cleanup interval for testing (500ms)
        {
            let mut config = state.retention_config.write().await;
            config.cleanup_interval_secs = 1; // Can't go below 1 second with Duration::from_secs
            config.task_ttl_secs = 0;
        }

        // Start the background cleanup task
        state.start_cleanup_task();

        // Add old data in multiple batches and verify cleanup runs multiple times
        for i in 0..3 {
            let task_id = Uuid::new_v4();
            let old_task = create_test_task(task_id, Some(Utc::now() - Duration::days(10)));
            state.tasks.write().await.insert(task_id, old_task);
            state.archived_tasks.write().await.push(task_id);
            state
                .task_count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

            // Wait for cleanup cycle
            tokio::time::sleep(std::time::Duration::from_millis(1200)).await;

            // Should be cleaned up
            assert_eq!(
                state.tasks.read().await.len(),
                0,
                "Iteration {} failed: task should be cleaned up",
                i
            );
        }
    }

    #[tokio::test]
    async fn test_background_cleanup_empty_state() {
        let state = Arc::new(create_test_state());

        // Set a very short cleanup interval
        {
            let mut config = state.retention_config.write().await;
            config.cleanup_interval_secs = 1;
        }

        // Start cleanup with empty state
        state.start_cleanup_task();

        // Wait for cleanup to run
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

        // Verify state is still empty (no panics or errors)
        assert_eq!(state.tasks.read().await.len(), 0);
        assert_eq!(state.disconnect_buffers.read().await.len(), 0);
    }

    #[tokio::test]
    async fn test_background_cleanup_logs_cleanup_counts() {
        let state = Arc::new(create_test_state());

        // Set a very short cleanup interval and immediate cleanup TTL
        {
            let mut config = state.retention_config.write().await;
            config.cleanup_interval_secs = 1;
            config.task_ttl_secs = 0;
            config.disconnect_buffer_ttl_secs = 0;
        }

        // Create some data to clean up
        let task_id = Uuid::new_v4();
        let old_task = create_test_task(task_id, Some(Utc::now() - Duration::days(10)));
        state.tasks.write().await.insert(task_id, old_task);
        state.archived_tasks.write().await.push(task_id);
        state.task_count.store(1, Ordering::Relaxed);

        // Start the background cleanup task
        state.start_cleanup_task();

        // Wait for cleanup to run
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

        // Test passes if cleanup ran without errors (logs are checked manually)
        // In a real scenario, you could use a tracing subscriber to capture logs
        assert_eq!(state.tasks.read().await.len(), 0);
    }
}
