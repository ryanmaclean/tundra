use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
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
}

impl Daemon {
    /// Create a new daemon backed by the given cache database.
    pub fn with_cache(config: Config, cache: Arc<CacheDb>) -> Self {
        let (shutdown_tx, shutdown_rx) = flume::bounded(1);
        let intervals = DaemonIntervals {
            heartbeat_secs: config.agents.heartbeat_interval_secs,
            ..DaemonIntervals::default()
        };
        Self {
            config,
            cache,
            intervals,
            shutdown_tx,
            shutdown_rx,
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
