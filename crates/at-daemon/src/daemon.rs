use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Utc;
use at_bridge::event_bus::EventBus;
use at_bridge::http_api::ApiState;
use at_core::cache::CacheDb;
use at_core::config::Config;
use tracing::{error, info, warn};

use crate::heartbeat::HeartbeatMonitor;
use crate::kpi::KpiCollector;
use crate::patrol::PatrolRunner;
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
/// configurable intervals. Shuts down gracefully when a signal is
/// received via the internal flume channel.
pub struct Daemon {
    config: Config,
    cache: Arc<CacheDb>,
    intervals: DaemonIntervals,
    shutdown_tx: flume::Sender<()>,
    shutdown_rx: flume::Receiver<()>,
    event_bus: EventBus,
    api_state: Arc<ApiState>,
}

impl Daemon {
    /// Create a new daemon backed by the given cache database.
    pub fn with_cache(config: Config, cache: Arc<CacheDb>) -> Self {
        let (shutdown_tx, shutdown_rx) = flume::bounded(1);
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
            shutdown_tx,
            shutdown_rx,
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
    pub fn shutdown_handle(&self) -> flume::Sender<()> {
        self.shutdown_tx.clone()
    }

    /// Send the shutdown signal.
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.try_send(());
    }

    /// Returns a reference to the event bus.
    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }

    /// Returns a reference to the shared API state.
    pub fn api_state(&self) -> &Arc<ApiState> {
        &self.api_state
    }

    /// Run the main event loop.
    ///
    /// This drives the patrol, heartbeat, and KPI loops concurrently using
    /// `tokio::select!`. The loop exits when a shutdown signal is received.
    pub async fn run(&self) -> Result<()> {
        info!(
            patrol_secs = self.intervals.patrol_secs,
            heartbeat_secs = self.intervals.heartbeat_secs,
            kpi_secs = self.intervals.kpi_secs,
            "daemon starting event loop"
        );

        // Spawn the HTTP/WS API server.
        let api_router = at_bridge::http_api::api_router(self.api_state.clone());
        let listener = tokio::net::TcpListener::bind("0.0.0.0:9090").await?;
        tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, api_router).await {
                error!(error = %e, "API server error");
            }
        });
        info!("API server listening on 0.0.0.0:9090");

        let patrol_runner = PatrolRunner::new(
            self.config.agents.heartbeat_interval_secs,
        );
        let heartbeat_monitor = HeartbeatMonitor::new(
            Duration::from_secs(self.config.agents.heartbeat_interval_secs * 2),
        );
        let kpi_collector = KpiCollector::new();
        let _scheduler = TaskScheduler::new();

        let mut patrol_interval =
            tokio::time::interval(Duration::from_secs(self.intervals.patrol_secs));
        let mut heartbeat_interval =
            tokio::time::interval(Duration::from_secs(self.intervals.heartbeat_secs));
        let mut kpi_interval =
            tokio::time::interval(Duration::from_secs(self.intervals.kpi_secs));

        // Consume the first immediate tick so loops don't all fire at t=0.
        patrol_interval.tick().await;
        heartbeat_interval.tick().await;
        kpi_interval.tick().await;

        loop {
            tokio::select! {
                _ = patrol_interval.tick() => {
                    match patrol_runner.run_patrol(&self.cache).await {
                        Ok(report) => {
                            info!(
                                stale_agents = report.stale_agents,
                                stuck_beads = report.stuck_beads,
                                orphan_ptys = report.orphan_ptys,
                                "patrol completed"
                            );
                            self.event_bus.publish(
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
                    match heartbeat_monitor.check_agents(&self.cache).await {
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
                    match kpi_collector.collect_snapshot(&self.cache).await {
                        Ok(snapshot) => {
                            info!(
                                total = snapshot.total_beads,
                                backlog = snapshot.backlog,
                                active_agents = snapshot.active_agents,
                                "kpi snapshot collected"
                            );
                            // Update shared KPI state for REST endpoint.
                            {
                                let mut kpi = self.api_state.kpi.write().await;
                                *kpi = snapshot.clone();
                            }
                            self.event_bus.publish(
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
                _ = self.shutdown_rx.recv_async() => {
                    info!("shutdown signal received, stopping daemon");
                    break;
                }
            }
        }

        info!("daemon stopped");
        Ok(())
    }
}
