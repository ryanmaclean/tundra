use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::sync::{RwLock, Semaphore};
use uuid::Uuid;

use at_core::session_store::SessionStore;
use at_core::settings::SettingsManager;
use at_core::types::{Agent, Bead, BeadStatus, CliType, KpiSnapshot};
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
