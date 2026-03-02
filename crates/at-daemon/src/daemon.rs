use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use at_bridge::event_bus::EventBus;
use at_bridge::http_api::ApiState;
use at_core::cache::CacheDb;
use at_core::config::{Config, CredentialProvider};
use at_intelligence::ResilientRegistry;
use chrono::Utc;
use tracing::{error, info, warn};

use at_harness::shutdown::ShutdownSignal;

use crate::heartbeat::HeartbeatMonitor;
use crate::kpi::KpiCollector;
use crate::patrol::{reap_orphan_ptys, PatrolRunner};
use crate::scheduler::TaskScheduler;

/// Configuration for daemon loop intervals.
#[derive(Debug, Clone)]
pub struct DaemonIntervals {
    /// How often the patrol loop runs (default: 60s).
    pub patrol_secs: u64,
    /// How often the heartbeat check runs (default: 30s).
    pub heartbeat_secs: u64,
    /// How often KPI snapshots are collected (default: 300s).
    pub kpi_secs: u64,
}

impl Default for DaemonIntervals {
    fn default() -> Self {
        Self {
            patrol_secs: 60,
            heartbeat_secs: 30,
            kpi_secs: 300,
        }
    }
}

/// The main auto-tundra background daemon.
///
/// Runs patrol loops, heartbeat monitoring, and KPI snapshots on
/// configurable intervals. Shuts down gracefully when the
/// `ShutdownSignal` is triggered (e.g. via ctrl-c or API call).
pub struct Daemon {
    config: Config,
    cache: Arc<CacheDb>,
    intervals: DaemonIntervals,
    shutdown: ShutdownSignal,
    event_bus: EventBus,
    api_state: Arc<ApiState>,
}

impl Daemon {
    /// Create a new daemon backed by the given cache database.
    pub fn with_cache(config: Config, cache: Arc<CacheDb>) -> Self {
        let shutdown = ShutdownSignal::new();
        let intervals = DaemonIntervals {
            heartbeat_secs: config.agents.heartbeat_interval_secs,
            ..DaemonIntervals::default()
        };
        let event_bus = EventBus::new();
        let api_state = Arc::new(ApiState::new(event_bus.clone()));
        Self {
            config,
            cache,
            intervals,
            shutdown,
            event_bus,
            api_state,
        }
    }

    /// Create a new daemon, opening (or creating) the cache database from config.
    pub async fn new(config: Config) -> Result<Self> {
        let cache_path = &config.cache.path;
        let cache = CacheDb::new(cache_path)
            .await
            .context("failed to open cache database")?;
        Ok(Self::with_cache(config, Arc::new(cache)))
    }

    /// Override the default loop intervals.
    pub fn set_intervals(&mut self, intervals: DaemonIntervals) {
        self.intervals = intervals;
    }

    /// Returns a handle that can be used to trigger shutdown from another task.
    pub fn shutdown_handle(&self) -> ShutdownSignal {
        self.shutdown.clone()
    }

    /// Send the shutdown signal.
    pub fn shutdown(&self) {
        self.shutdown.trigger();
    }

    /// Returns a reference to the event bus.
    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }

    /// Returns a reference to the shared API state.
    pub fn api_state(&self) -> &Arc<ApiState> {
        &self.api_state
    }

    /// Returns a reference to the config.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Log the LLM profile bootstrap info.
    fn log_profile_bootstrap(&self) {
        let reg = ResilientRegistry::from_config(&self.config);
        let total_count = reg.count();
        let best_profile = reg
            .registry
            .best_available()
            .map(|p| format!("{} ({:?})", p.name, p.provider))
            .unwrap_or_else(|| "none".to_string());

        info!(
            total_profiles = total_count,
            best_profile = %best_profile,
            "LLM profile bootstrap complete"
        );
    }

    // ------------------------------------------------------------------
    // Embedded mode — for Tauri desktop app
    // ------------------------------------------------------------------

    /// Start the daemon in embedded mode (non-blocking).
    ///
    /// Binds the API server to `127.0.0.1:0` (OS picks a free port),
    /// spawns background loops, and returns the bound port immediately.
    /// The caller owns the `Daemon` and can call `shutdown()` to stop.
    pub async fn start_embedded(&self) -> Result<u16> {
        self.log_profile_bootstrap();

        let api_key = CredentialProvider::ensure_daemon_api_key();
        info!("daemon API key ready — authentication enabled");

        // Seed demo data so the UI is functional on first launch.
        self.api_state.seed_demo_data().await;

        let allowed_origins = self.config.security.allowed_origins.clone();
        let api_router = at_bridge::http_api::api_router_with_auth(
            self.api_state.clone(),
            Some(api_key),
            allowed_origins,
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();

        tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, api_router).await {
                error!(error = %e, "API server error");
            }
        });
        info!(port, "embedded API server listening");

        // Note: Background cleanup task is spawned in spawn_background_loops()
        self.spawn_background_loops();
        Ok(port)
    }

    /// Spawn patrol, heartbeat, and KPI loops as background tasks.
    fn spawn_background_loops(&self) {
        let cache = self.cache.clone();
        let api_state = self.api_state.clone();
        let event_bus = self.event_bus.clone();
        let config = self.config.clone();
        let intervals = self.intervals.clone();
        let shutdown = self.shutdown.clone();

        // Spawn OAuth token refresh monitor
        at_bridge::http_api::spawn_oauth_token_refresh_monitor(api_state.clone());

        // Spawn background cleanup task for memory retention
        api_state.start_cleanup_task();

        tokio::spawn(async move {
            Self::run_loops(cache, api_state, event_bus, config, intervals, shutdown).await;
        });
    }

    /// The inner event loop shared by both standalone and embedded modes.
    async fn run_loops(
        cache: Arc<CacheDb>,
        api_state: Arc<ApiState>,
        event_bus: EventBus,
        config: Config,
        intervals: DaemonIntervals,
        shutdown: ShutdownSignal,
    ) {
        let patrol_runner = PatrolRunner::new(config.agents.heartbeat_interval_secs);
        let heartbeat_monitor = HeartbeatMonitor::new(Duration::from_secs(
            config.agents.heartbeat_interval_secs * 2,
        ));
        let kpi_collector = KpiCollector::new();
        let _scheduler = TaskScheduler::new(config.agents.max_concurrent);

        let mut patrol_interval = tokio::time::interval(Duration::from_secs(intervals.patrol_secs));
        let mut heartbeat_interval =
            tokio::time::interval(Duration::from_secs(intervals.heartbeat_secs));
        let mut kpi_interval = tokio::time::interval(Duration::from_secs(intervals.kpi_secs));

        // Consume the first immediate tick so loops don't all fire at t=0.
        patrol_interval.tick().await;
        heartbeat_interval.tick().await;
        kpi_interval.tick().await;

        let mut shutdown_rx = shutdown.subscribe();

        loop {
            tokio::select! {
                _ = patrol_interval.tick() => {
                    let reaped = reap_orphan_ptys(&api_state).await;
                    match patrol_runner.run_patrol(&cache).await {
                        Ok(mut report) => {
                            report.orphan_ptys = reaped;
                            info!(
                                stale_agents = report.stale_agents,
                                stuck_beads = report.stuck_beads,
                                orphan_ptys = report.orphan_ptys,
                                "patrol completed"
                            );
                            event_bus.publish(
                                at_bridge::protocol::BridgeMessage::Event(
                                    at_bridge::protocol::EventPayload {
                                        event_type: "patrol_completed".to_string(),
                                        agent_id: None,
                                        bead_id: None,
                                        message: format!(
                                            "stale_agents={} stuck_beads={} orphan_ptys={}",
                                            report.stale_agents, report.stuck_beads, report.orphan_ptys
                                        ),
                                        timestamp: Utc::now(),
                                    },
                                ),
                            );
                        }
                        Err(e) => {
                            error!(error = %e, "patrol failed");
                        }
                    }
                }
                _ = heartbeat_interval.tick() => {
                    match heartbeat_monitor.check_agents(&cache).await {
                        Ok(stale) => {
                            if !stale.is_empty() {
                                warn!(count = stale.len(), "stale agents detected");
                                for agent in &stale {
                                    warn!(
                                        agent_id = %agent.agent_id,
                                        last_seen = %agent.last_seen,
                                        stale_for_secs = agent.duration_since.as_secs(),
                                        "agent is stale"
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            error!(error = %e, "heartbeat check failed");
                        }
                    }
                }
                _ = kpi_interval.tick() => {
                    match kpi_collector.collect_snapshot(&cache).await {
                        Ok(snapshot) => {
                            info!(
                                total = snapshot.total_beads,
                                backlog = snapshot.backlog,
                                active_agents = snapshot.active_agents,
                                "kpi snapshot collected"
                            );
                            {
                                let mut kpi = api_state.kpi.write().await;
                                *kpi = snapshot.clone();
                            }
                            event_bus.publish(
                                at_bridge::protocol::BridgeMessage::KpiUpdate(
                                    at_bridge::protocol::KpiPayload {
                                        total_beads: snapshot.total_beads,
                                        backlog: snapshot.backlog,
                                        hooked: snapshot.hooked,
                                        slung: snapshot.slung,
                                        review: snapshot.review,
                                        done: snapshot.done,
                                        failed: snapshot.failed,
                                        active_agents: snapshot.active_agents,
                                    },
                                ),
                            );
                        }
                        Err(e) => {
                            error!(error = %e, "kpi snapshot failed");
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("shutdown signal received, stopping background loops");
                    break;
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Standalone mode — for headless/server use
    // ------------------------------------------------------------------

    /// Run the daemon as a standalone server using a pre-bound listener (blocking).
    ///
    /// The caller is responsible for binding the `TcpListener` (e.g. to port 0
    /// for OS-assigned ports). This enables dynamic port allocation in `main.rs`.
    pub async fn run_with_listener(&self, listener: tokio::net::TcpListener) -> Result<()> {
        self.log_profile_bootstrap();
        let reg = at_intelligence::ResilientRegistry::from_config(&self.config);
        if let Some(p) = reg.registry.best_available() {
            let api_key = std::env::var(&p.api_key_env).ok().filter(|s| !s.is_empty());
            let provider: Arc<dyn at_intelligence::llm::LlmProvider> = match p.provider {
                at_intelligence::ProviderKind::Local => Arc::new(
                    at_intelligence::llm::LocalProvider::new(&p.base_url, api_key),
                ),
                at_intelligence::ProviderKind::Anthropic => Arc::new(
                    at_intelligence::llm::AnthropicProvider::new(api_key.unwrap_or_default()),
                ),
                at_intelligence::ProviderKind::OpenAi => Arc::new(
                    at_intelligence::llm::OpenAiProvider::new(api_key.unwrap_or_default()),
                ),
                _ => Arc::new(at_intelligence::llm::LocalProvider::new(
                    &p.base_url,
                    api_key,
                )),
            };
            let mut engine = self.api_state.ideation_engine.write().await;
            *engine = at_intelligence::ideation::IdeationEngine::with_provider(
                provider,
                p.default_model.clone(),
            );
        }

        info!(
            patrol_secs = self.intervals.patrol_secs,
            heartbeat_secs = self.intervals.heartbeat_secs,
            kpi_secs = self.intervals.kpi_secs,
            "daemon starting event loop"
        );

        let api_key = CredentialProvider::ensure_daemon_api_key();
        info!("daemon API key ready — authentication enabled");
        // Seed demo data so the UI is functional on first launch.
        self.api_state.seed_demo_data().await;

        let allowed_origins = self.config.security.allowed_origins.clone();
        let api_router = at_bridge::http_api::api_router_with_auth(
            self.api_state.clone(),
            Some(api_key),
            allowed_origins,
        );
        let bind_addr = listener.local_addr()?;
        let api_handle = tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, api_router).await {
                error!(error = %e, "API server error");
            }
        });
        info!(%bind_addr, "API server listening");

        // Spawn background cleanup task for memory retention
        self.api_state.start_cleanup_task();

        // Run loops inline (blocking) for standalone mode.
        Self::run_loops(
            self.cache.clone(),
            self.api_state.clone(),
            self.event_bus.clone(),
            self.config.clone(),
            self.intervals.clone(),
            self.shutdown.clone(),
        )
        .await;

        api_handle.abort();
        info!("daemon stopped");
        Ok(())
    }

    /// Run the daemon as a standalone server (blocking).
    ///
    /// Binds to the port from config (default 9090), runs until shutdown.
    pub async fn run(&self) -> Result<()> {
        let port = self.config.daemon.port;
        let bind_addr = format!("{}:{}", self.config.daemon.host, port);

        self.log_profile_bootstrap();
        let reg = at_intelligence::ResilientRegistry::from_config(&self.config);
        if let Some(p) = reg.registry.best_available() {
            let api_key = std::env::var(&p.api_key_env).ok().filter(|s| !s.is_empty());
            let provider: Arc<dyn at_intelligence::llm::LlmProvider> = match p.provider {
                at_intelligence::ProviderKind::Local => Arc::new(
                    at_intelligence::llm::LocalProvider::new(&p.base_url, api_key),
                ),
                at_intelligence::ProviderKind::Anthropic => Arc::new(
                    at_intelligence::llm::AnthropicProvider::new(api_key.unwrap_or_default()),
                ),
                at_intelligence::ProviderKind::OpenAi => Arc::new(
                    at_intelligence::llm::OpenAiProvider::new(api_key.unwrap_or_default()),
                ),
                _ => Arc::new(at_intelligence::llm::LocalProvider::new(
                    &p.base_url,
                    api_key,
                )),
            };
            let mut engine = self.api_state.ideation_engine.write().await;
            *engine = at_intelligence::ideation::IdeationEngine::with_provider(
                provider,
                p.default_model.clone(),
            );
        }

        info!(
            patrol_secs = self.intervals.patrol_secs,
            heartbeat_secs = self.intervals.heartbeat_secs,
            kpi_secs = self.intervals.kpi_secs,
            "daemon starting event loop"
        );

        let api_key = CredentialProvider::ensure_daemon_api_key();
        info!("daemon API key ready — authentication enabled");
        // Seed demo data so the UI is functional on first launch.
        self.api_state.seed_demo_data().await;

        let allowed_origins = self.config.security.allowed_origins.clone();
        let api_router = at_bridge::http_api::api_router_with_auth(
            self.api_state.clone(),
            Some(api_key),
            allowed_origins,
        );
        let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
        let api_handle = tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, api_router).await {
                error!(error = %e, "API server error");
            }
        });
        info!(%bind_addr, "API server listening");

        // Spawn background cleanup task for memory retention
        self.api_state.start_cleanup_task();

        // Run loops inline (blocking) for standalone mode.
        Self::run_loops(
            self.cache.clone(),
            self.api_state.clone(),
            self.event_bus.clone(),
            self.config.clone(),
            self.intervals.clone(),
            self.shutdown.clone(),
        )
        .await;

        api_handle.abort();
        info!("daemon stopped");
        Ok(())
    }
}
