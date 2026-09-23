use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::Result;
use at_bridge::event_bus::EventBus;
use at_bridge::http_api::ApiState;
use at_bridge::protocol::{BridgeMessage, EventPayload};
use at_core::cache::CacheDb;
use at_core::config::PatrolConfig;
use at_core::types::{Agent, AgentStatus, Bead, BeadStatus};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Result of a single patrol sweep.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatrolReport {
    /// Number of stale agents discovered (no heartbeat in 2x interval).
    pub stale_agents: usize,
    /// Number of beads stuck in `Slung` status past the timeout.
    pub stuck_beads: usize,
    /// Number of orphan PTYs detected.
    pub orphan_ptys: usize,
    /// IDs of stuck beads found.
    pub stuck_bead_ids: Vec<uuid::Uuid>,
    /// Timestamp of this patrol run.
    pub timestamp: DateTime<Utc>,
}

/// Runs periodic patrol sweeps over the cache to detect anomalies.
pub struct PatrolRunner {
    /// Heartbeat interval in seconds; agents missing for 2x this are stale.
    _heartbeat_interval_secs: u64,
    /// Maximum duration a bead may remain in `Slung` before it is considered stuck.
    slung_timeout: ChronoDuration,
}

impl PatrolRunner {
    /// Create a new patrol runner.
    ///
    /// `heartbeat_interval_secs` is used to compute the staleness threshold
    /// (2x the heartbeat interval). The default slung timeout is 30 minutes.
    pub fn new(heartbeat_interval_secs: u64) -> Self {
        Self {
            _heartbeat_interval_secs: heartbeat_interval_secs,
            slung_timeout: ChronoDuration::minutes(30),
        }
    }

    /// Override the slung timeout.
    pub fn with_slung_timeout(mut self, timeout: ChronoDuration) -> Self {
        self.slung_timeout = timeout;
        self
    }

    /// Execute a full patrol sweep.
    ///
    /// Checks:
    /// - Stuck beads: beads in `Slung` status longer than the timeout.
    /// - Stale agents: detected via the heartbeat monitor (count reported but
    ///   agent enumeration requires external tracking since CacheDb does not
    ///   expose a list-all-agents API).
    /// - Orphan PTYs: placeholder for future PTY session tracking.
    pub async fn run_patrol(&self, cache: &CacheDb) -> Result<PatrolReport> {
        let now = Utc::now();
        debug!("patrol sweep starting");

        // --- Check for stuck beads (slung longer than timeout) ---
        let slung_beads = cache
            .list_beads_by_status(BeadStatus::Slung)
            .await
            .map_err(|e| anyhow::anyhow!("failed to query slung beads: {}", e))?;

        let stuck_bead_ids = self.stuck_bead_ids(&slung_beads, now);

        // Stale agent detection is handled by HeartbeatMonitor; patrol
        // reports a zero count here since we cannot enumerate all agents
        // without a list_agents API on CacheDb.
        let stale_agents = 0;

        // Orphan PTY detection is a placeholder for future implementation.
        let orphan_ptys = 0;

        let report = PatrolReport {
            stale_agents,
            stuck_beads: stuck_bead_ids.len(),
            orphan_ptys,
            stuck_bead_ids,
            timestamp: now,
        };

        debug!(stuck_beads = report.stuck_beads, "patrol sweep completed");

        Ok(report)
    }
}

impl PatrolRunner {
    /// IDs of beads that have sat in `Slung` longer than the timeout.
    pub fn stuck_bead_ids<'a>(
        &self,
        beads: impl IntoIterator<Item = &'a Bead>,
        now: DateTime<Utc>,
    ) -> Vec<Uuid> {
        let mut ids = Vec::new();
        for bead in beads {
            if bead.status != BeadStatus::Slung {
                continue;
            }
            if let Some(slung_at) = bead.slung_at {
                let elapsed = now.signed_duration_since(slung_at);
                if elapsed > self.slung_timeout {
                    ids.push(bead.id);
                    info!(
                        bead_id = %bead.id,
                        slung_at = %slung_at,
                        elapsed_mins = elapsed.num_minutes(),
                        "stuck bead detected"
                    );
                }
            }
        }
        ids
    }

    /// Patrol sweep over both `CacheDb` and the live in-memory beads in
    /// [`ApiState`]. The HTTP/MCP API keeps beads in `ApiState` only, so a
    /// cache-only sweep misses every bead created through the API.
    pub async fn run_patrol_live(
        &self,
        cache: &CacheDb,
        api_state: &ApiState,
    ) -> Result<PatrolReport> {
        let mut report = self.run_patrol(cache).await?;
        let live = {
            let beads = api_state.beads.read().await;
            self.stuck_bead_ids(beads.values(), report.timestamp)
        };
        for id in live {
            if !report.stuck_bead_ids.contains(&id) {
                report.stuck_bead_ids.push(id);
            }
        }
        report.stuck_beads = report.stuck_bead_ids.len();
        Ok(report)
    }
}

/// Reap orphaned PTY processes whose child has exited but remain in the
/// terminal registry.
///
/// Iterates every entry in the `terminal_registry`, checks the corresponding
/// `pty_handles` entry for liveness via `PtyHandle::is_alive()`, and removes
/// dead entries from both maps. Returns the number of orphans reaped.
pub async fn reap_orphan_ptys(state: &Arc<ApiState>) -> usize {
    // Collect terminal IDs that have a registered PTY handle.
    let terminal_ids: Vec<uuid::Uuid> = {
        let registry = state.terminal_registry.read().await;
        registry.list().iter().map(|t| t.id).collect()
    };

    let mut orphan_count = 0;

    for tid in &terminal_ids {
        let is_dead = {
            let handles = state.pty_handles.read().await;
            match handles.get(tid) {
                Some(handle) => !handle.is_alive(),
                // Terminal registered but no PTY handle at all — also an orphan.
                None => true,
            }
        };

        if is_dead {
            orphan_count += 1;
            warn!(terminal_id = %tid, "orphaned PTY detected — reaping");

            // Remove from pty_handles (and kill if still present).
            {
                let mut handles = state.pty_handles.write().await;
                if let Some(handle) = handles.remove(tid) {
                    let _ = handle.kill();
                }
            }

            // Mark as closed in the registry and remove.
            {
                let mut registry = state.terminal_registry.write().await;
                registry.unregister(tid);
            }
        }
    }

    if orphan_count > 0 {
        info!(orphan_count, "orphan PTY reaping complete");
    }

    orphan_count
}

// ---------------------------------------------------------------------------
// Stuck-session detection
//
// Policy derived from gastown `internal/deacon/stuck.go` and the health-check /
// force-kill flow in `internal/cmd/deacon.go` (ping -> wait -> count failure ->
// force-kill at threshold -> cooldown -> notify mayor).
// Copyright (c) 2025 Steve Yegge. Used under the MIT License:
// https://github.com/steveyegge/gastown/blob/main/LICENSE
// ---------------------------------------------------------------------------

/// `EventPayload.event_type` published when patrol force-kills a stuck agent.
/// Whoever owns the agent's process (executor / pipeline) should abort its task.
pub const EVENT_AGENT_FORCE_KILL: &str = "agent_force_kill";
/// `EventPayload.event_type` published once per force-kill for operators
/// (gastown: "notify mayor").
pub const EVENT_AGENT_STUCK_ESCALATION: &str = "agent_stuck_escalation";
/// `EventPayload.event_type` published when a killed agent's cooldown ends and
/// its slot may be reused.
pub const EVENT_AGENT_SLOT_RELEASED: &str = "agent_slot_released";

/// Thresholds for stuck-session detection (gastown `StuckConfig`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StuckPolicy {
    /// Heartbeat silence longer than this fails a health check.
    pub ping_timeout: ChronoDuration,
    /// Failed checks in a row before force-kill.
    pub consecutive_failures: u32,
    /// Time after a force-kill before the slot may be reused.
    pub cooldown: ChronoDuration,
}

impl Default for StuckPolicy {
    fn default() -> Self {
        Self::from(&PatrolConfig::default())
    }
}

impl From<&PatrolConfig> for StuckPolicy {
    fn from(cfg: &PatrolConfig) -> Self {
        Self {
            ping_timeout: secs(cfg.ping_timeout_secs),
            consecutive_failures: cfg.consecutive_failures.max(1),
            cooldown: secs(cfg.kill_cooldown_secs),
        }
    }
}

fn secs(n: u64) -> ChronoDuration {
    ChronoDuration::seconds(i64::try_from(n).unwrap_or(i64::MAX / 1000))
}

/// Outcome of one health check for one agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum StuckVerdict {
    /// Heartbeat is within the ping timeout; failure count reset.
    Healthy,
    /// Heartbeat is overdue; no action yet.
    Missed { consecutive_failures: u32 },
    /// Failure threshold reached: kill the agent and start the cooldown.
    ForceKill {
        consecutive_failures: u32,
        force_kill_count: u32,
    },
    /// Recently force-killed; slot not reusable yet.
    InCooldown { remaining_secs: i64 },
    /// Cooldown just ended; slot may be reused.
    SlotReleased,
}

/// Per-agent health bookkeeping (gastown `AgentHealthState`).
#[derive(Debug, Clone, Default)]
struct AgentHealth {
    consecutive_failures: u32,
    /// When the most recent failure was counted; failures are counted at most
    /// once per `ping_timeout` window regardless of how often patrol runs.
    last_failure_at: Option<DateTime<Utc>>,
    last_force_kill: Option<DateTime<Utc>>,
    force_kill_count: u32,
    /// Silence is measured from `max(last_seen, baseline)`; set when a slot is
    /// released so a reused agent starts with a clean window.
    baseline: Option<DateTime<Utc>>,
}

/// Pure stuck-session state machine. Callers supply `now`, so it is
/// deterministic under a fake clock.
#[derive(Debug, Default)]
pub struct StuckDetector {
    policy: StuckPolicy,
    health: HashMap<Uuid, AgentHealth>,
}

/// Agents that have a live session to be stuck in. `Pending` is excluded
/// (startup is owned by the spawn path) as is `Stopped`; agents with neither a
/// pid nor a session id are bookkeeping-only (gastown: "session not running").
pub fn is_monitored(agent: &Agent) -> bool {
    matches!(
        agent.status,
        AgentStatus::Active | AgentStatus::Idle | AgentStatus::Unknown
    ) && (agent.pid.is_some() || agent.session_id.is_some())
}

impl StuckDetector {
    pub fn new(policy: StuckPolicy) -> Self {
        Self {
            policy,
            health: HashMap::new(),
        }
    }

    pub fn policy(&self) -> &StuckPolicy {
        &self.policy
    }

    /// Run one health check for `agent` at `now`.
    ///
    /// Returns `None` when the agent is not monitored and has no cooldown.
    pub fn check(&mut self, agent: &Agent, now: DateTime<Utc>) -> Option<StuckVerdict> {
        if let Some(verdict) = self.advance_cooldown(agent.id, now) {
            return Some(verdict);
        }
        if !is_monitored(agent) {
            self.health.remove(&agent.id);
            return None;
        }

        let policy = self.policy;
        let h = self.health.entry(agent.id).or_default();
        let seen = h
            .baseline
            .map_or(agent.last_seen, |b| b.max(agent.last_seen));

        if now.signed_duration_since(seen) <= policy.ping_timeout {
            h.consecutive_failures = 0;
            h.last_failure_at = None;
            return Some(StuckVerdict::Healthy);
        }

        let window_open = h
            .last_failure_at
            .is_none_or(|t| now.signed_duration_since(t) >= policy.ping_timeout);
        if !window_open {
            return Some(StuckVerdict::Missed {
                consecutive_failures: h.consecutive_failures,
            });
        }

        h.consecutive_failures += 1;
        h.last_failure_at = Some(now);
        if h.consecutive_failures < policy.consecutive_failures {
            return Some(StuckVerdict::Missed {
                consecutive_failures: h.consecutive_failures,
            });
        }

        let failures = h.consecutive_failures;
        h.consecutive_failures = 0;
        h.last_failure_at = None;
        h.last_force_kill = Some(now);
        h.force_kill_count += 1;
        Some(StuckVerdict::ForceKill {
            consecutive_failures: failures,
            force_kill_count: h.force_kill_count,
        })
    }

    /// Cooldown handling shared by [`check`](Self::check) and
    /// [`prune`](Self::prune): `InCooldown` while it runs, `SlotReleased` once
    /// when it ends, `None` if the agent was not in cooldown.
    fn advance_cooldown(&mut self, id: Uuid, now: DateTime<Utc>) -> Option<StuckVerdict> {
        let cooldown = self.policy.cooldown;
        let h = self.health.get_mut(&id)?;
        let until = h.last_force_kill? + cooldown;
        if now < until {
            return Some(StuckVerdict::InCooldown {
                remaining_secs: until.signed_duration_since(now).num_seconds(),
            });
        }
        h.last_force_kill = None;
        h.consecutive_failures = 0;
        h.last_failure_at = None;
        h.baseline = Some(until);
        Some(StuckVerdict::SlotReleased)
    }

    /// Drop state for agents no longer present, releasing any whose cooldown
    /// has ended. Returns the released ids.
    pub fn prune(&mut self, present: &HashSet<Uuid>, now: DateTime<Utc>) -> Vec<Uuid> {
        let gone: Vec<Uuid> = self
            .health
            .keys()
            .filter(|id| !present.contains(id))
            .copied()
            .collect();
        let mut released = Vec::new();
        for id in gone {
            match self.advance_cooldown(id, now) {
                Some(StuckVerdict::InCooldown { .. }) => {}
                Some(StuckVerdict::SlotReleased) => {
                    self.health.remove(&id);
                    released.push(id);
                }
                _ => {
                    self.health.remove(&id);
                }
            }
        }
        released
    }

    /// Whether the agent's slot may be (re)used at `now`: false only while a
    /// post-kill cooldown is running.
    pub fn slot_reusable(&self, agent_id: Uuid, now: DateTime<Utc>) -> bool {
        self.health
            .get(&agent_id)
            .and_then(|h| h.last_force_kill)
            .is_none_or(|killed| now >= killed + self.policy.cooldown)
    }

    /// Current consecutive failed checks for an agent (0 if untracked).
    pub fn consecutive_failures(&self, agent_id: Uuid) -> u32 {
        self.health
            .get(&agent_id)
            .map_or(0, |h| h.consecutive_failures)
    }
}

/// Source of "now" for [`StuckMonitor`], swappable for a fake clock in tests.
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

/// Wall-clock [`Clock`].
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Summary of one [`StuckMonitor::sweep`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StuckSweep {
    /// Agents health-checked this sweep.
    pub checked: usize,
    /// Agents with an overdue heartbeat that were not killed.
    pub missed: Vec<Uuid>,
    /// Agents force-killed this sweep.
    pub killed: Vec<Uuid>,
    /// Agents whose post-kill cooldown is still running.
    pub cooling_down: Vec<Uuid>,
    /// Agents whose cooldown ended this sweep.
    pub released: Vec<Uuid>,
}

/// Applies [`StuckDetector`] verdicts to the live agent registry
/// (`ApiState.agents`) and publishes them on the event bus.
///
/// Force-kill marks the agent `Stopped`, publishes `AgentUpdated`, an
/// [`EVENT_AGENT_FORCE_KILL`] event (the abort request for the executor that
/// owns the agent's process) and one [`EVENT_AGENT_STUCK_ESCALATION`] event.
pub struct StuckMonitor {
    detector: StuckDetector,
    clock: Arc<dyn Clock>,
}

impl StuckMonitor {
    pub fn new(policy: StuckPolicy, clock: Arc<dyn Clock>) -> Self {
        Self {
            detector: StuckDetector::new(policy),
            clock,
        }
    }

    pub fn detector(&self) -> &StuckDetector {
        &self.detector
    }

    /// Health-check every agent in `agents` once.
    pub async fn sweep(
        &mut self,
        agents: &RwLock<HashMap<Uuid, Agent>>,
        bus: &EventBus,
    ) -> StuckSweep {
        let now = self.clock.now();
        let snapshot: Vec<Agent> = agents.read().await.values().cloned().collect();
        let present: HashSet<Uuid> = snapshot.iter().map(|a| a.id).collect();
        let mut out = StuckSweep::default();

        for agent in &snapshot {
            let Some(verdict) = self.detector.check(agent, now) else {
                continue;
            };
            match verdict {
                StuckVerdict::Healthy => out.checked += 1,
                StuckVerdict::Missed {
                    consecutive_failures,
                } => {
                    out.checked += 1;
                    out.missed.push(agent.id);
                    warn!(
                        agent_id = %agent.id,
                        name = %agent.name,
                        consecutive_failures,
                        threshold = self.detector.policy().consecutive_failures,
                        "agent missed heartbeat"
                    );
                }
                StuckVerdict::ForceKill {
                    consecutive_failures,
                    force_kill_count,
                } => {
                    out.checked += 1;
                    out.killed.push(agent.id);
                    force_kill(
                        agents,
                        bus,
                        agent,
                        consecutive_failures,
                        force_kill_count,
                        now,
                    )
                    .await;
                }
                StuckVerdict::InCooldown { .. } => out.cooling_down.push(agent.id),
                StuckVerdict::SlotReleased => {
                    out.released.push(agent.id);
                    publish_released(bus, agent.id, now);
                }
            }
        }

        for id in self.detector.prune(&present, now) {
            out.released.push(id);
            publish_released(bus, id, now);
        }

        out
    }
}

async fn force_kill(
    agents: &RwLock<HashMap<Uuid, Agent>>,
    bus: &EventBus,
    agent: &Agent,
    failures: u32,
    kill_count: u32,
    now: DateTime<Utc>,
) {
    let reason = format!(
        "unresponsive after {failures} consecutive health check failures \
         (last_seen={})",
        agent.last_seen.to_rfc3339()
    );
    warn!(agent_id = %agent.id, name = %agent.name, kill_count, %reason, "force-killing stuck agent");

    let updated = {
        let mut map = agents.write().await;
        map.get_mut(&agent.id).map(|a| {
            a.status = AgentStatus::Stopped;
            a.clone()
        })
    };
    if let Some(a) = updated {
        bus.publish(BridgeMessage::AgentUpdated(a));
    }

    bus.publish(event(EVENT_AGENT_FORCE_KILL, agent.id, reason.clone(), now));
    bus.publish(event(
        EVENT_AGENT_STUCK_ESCALATION,
        agent.id,
        format!(
            "agent '{}' force-killed by patrol (kill #{kill_count}): {reason}",
            agent.name
        ),
        now,
    ));
}

fn publish_released(bus: &EventBus, id: Uuid, now: DateTime<Utc>) {
    info!(agent_id = %id, "stuck-agent cooldown ended; slot reusable");
    bus.publish(event(
        EVENT_AGENT_SLOT_RELEASED,
        id,
        "post-kill cooldown ended; slot reusable".to_string(),
        now,
    ));
}

fn event(kind: &str, agent_id: Uuid, message: String, now: DateTime<Utc>) -> BridgeMessage {
    BridgeMessage::Event(EventPayload {
        event_type: kind.to_string(),
        agent_id: Some(agent_id),
        bead_id: None,
        message,
        timestamp: now,
    })
}

#[cfg(test)]
mod stuck_tests {
    use super::*;
    use at_core::types::{AgentRole, CliType};
    use chrono::TimeZone;
    use std::sync::Mutex;

    /// Manually advanced clock.
    struct FakeClock(Mutex<DateTime<Utc>>);

    impl FakeClock {
        fn at(t: DateTime<Utc>) -> Arc<Self> {
            Arc::new(Self(Mutex::new(t)))
        }
        fn advance(&self, d: ChronoDuration) {
            *self.0.lock().unwrap() += d;
        }
    }

    impl Clock for FakeClock {
        fn now(&self) -> DateTime<Utc> {
            *self.0.lock().unwrap()
        }
    }

    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()
    }

    fn live_agent(last_seen: DateTime<Utc>) -> Agent {
        let mut a = Agent::new("worker", AgentRole::Crew, CliType::Claude);
        a.status = AgentStatus::Active;
        a.session_id = Some("sess-1".into());
        a.last_seen = last_seen;
        a
    }

    fn registry(agents: &[Agent]) -> RwLock<HashMap<Uuid, Agent>> {
        RwLock::new(agents.iter().map(|a| (a.id, a.clone())).collect())
    }

    fn events(rx: &flume::Receiver<Arc<BridgeMessage>>, kind: &str) -> usize {
        rx.try_iter()
            .filter(|m| matches!(&**m, BridgeMessage::Event(e) if e.event_type == kind))
            .count()
    }

    fn s(n: i64) -> ChronoDuration {
        ChronoDuration::seconds(n)
    }

    #[test]
    fn policy_from_config_defaults() {
        let p = StuckPolicy::default();
        assert_eq!(p.ping_timeout, s(30));
        assert_eq!(p.consecutive_failures, 3);
        assert_eq!(p.cooldown, s(300));
    }

    #[test]
    fn healthy_agent_is_untouched() {
        let mut d = StuckDetector::default();
        let mut a = live_agent(t0());
        for i in 1..=10 {
            let now = t0() + s(25 * i);
            a.last_seen = now - s(5); // heartbeating
            assert_eq!(d.check(&a, now), Some(StuckVerdict::Healthy));
        }
        assert_eq!(d.consecutive_failures(a.id), 0);
        assert!(d.slot_reusable(a.id, t0() + s(1000)));
    }

    #[test]
    fn unmonitored_agents_are_ignored() {
        let mut d = StuckDetector::default();
        let mut no_session = live_agent(t0());
        no_session.session_id = None;
        let mut pending = live_agent(t0());
        pending.status = AgentStatus::Pending;
        let mut stopped = live_agent(t0());
        stopped.status = AgentStatus::Stopped;
        for a in [&no_session, &pending, &stopped] {
            assert_eq!(d.check(a, t0() + s(3600)), None);
        }
    }

    #[test]
    fn one_missed_ping_takes_no_action() {
        let mut d = StuckDetector::default();
        let a = live_agent(t0());
        assert_eq!(
            d.check(&a, t0() + s(31)),
            Some(StuckVerdict::Missed {
                consecutive_failures: 1
            })
        );
        // Re-checking inside the same ping window does not count again.
        assert_eq!(
            d.check(&a, t0() + s(45)),
            Some(StuckVerdict::Missed {
                consecutive_failures: 1
            })
        );
        assert!(d.slot_reusable(a.id, t0() + s(45)));
    }

    #[test]
    fn response_resets_failures() {
        let mut d = StuckDetector::default();
        let mut a = live_agent(t0());
        d.check(&a, t0() + s(31));
        d.check(&a, t0() + s(61));
        assert_eq!(d.consecutive_failures(a.id), 2);
        a.last_seen = t0() + s(80);
        assert_eq!(d.check(&a, t0() + s(91)), Some(StuckVerdict::Healthy));
        assert_eq!(d.consecutive_failures(a.id), 0);
    }

    #[test]
    fn three_misses_force_kill_then_cooldown() {
        let mut d = StuckDetector::default();
        let a = live_agent(t0());
        assert!(matches!(
            d.check(&a, t0() + s(31)),
            Some(StuckVerdict::Missed { .. })
        ));
        assert!(matches!(
            d.check(&a, t0() + s(61)),
            Some(StuckVerdict::Missed { .. })
        ));
        assert_eq!(
            d.check(&a, t0() + s(91)),
            Some(StuckVerdict::ForceKill {
                consecutive_failures: 3,
                force_kill_count: 1
            })
        );
        assert!(!d.slot_reusable(a.id, t0() + s(91)));
        assert_eq!(
            d.check(&a, t0() + s(121)),
            Some(StuckVerdict::InCooldown {
                remaining_secs: 270
            })
        );
        assert!(!d.slot_reusable(a.id, t0() + s(91 + 299)));
    }

    #[test]
    fn cooldown_expiry_makes_slot_reusable_with_fresh_window() {
        let mut d = StuckDetector::default();
        let a = live_agent(t0());
        for t in [31, 61, 91] {
            d.check(&a, t0() + s(t));
        }
        let expiry = t0() + s(91 + 300);
        assert!(d.slot_reusable(a.id, expiry));
        assert_eq!(d.check(&a, expiry), Some(StuckVerdict::SlotReleased));
        // A reused (still silent) agent gets a clean window from cooldown end,
        // so it is not instantly re-killed.
        assert_eq!(d.check(&a, expiry + s(10)), Some(StuckVerdict::Healthy));
        assert!(matches!(
            d.check(&a, expiry + s(31)),
            Some(StuckVerdict::Missed {
                consecutive_failures: 1
            })
        ));
    }

    #[tokio::test]
    async fn sweep_kills_marks_stopped_escalates_once_and_releases() {
        let clock = FakeClock::at(t0());
        let mut mon = StuckMonitor::new(StuckPolicy::default(), clock.clone());
        let stuck = live_agent(t0());
        let mut healthy = live_agent(t0());
        healthy.name = "healthy".into();
        let agents = registry(&[stuck.clone(), healthy.clone()]);
        let bus = EventBus::new();
        let rx = bus.subscribe();

        // Advance the clock, heartbeat the healthy agent, and sweep.
        async fn tick_with(
            mon: &mut StuckMonitor,
            clock: &FakeClock,
            agents: &RwLock<HashMap<Uuid, Agent>>,
            bus: &EventBus,
            healthy: Uuid,
        ) -> StuckSweep {
            clock.advance(s(31));
            agents.write().await.get_mut(&healthy).unwrap().last_seen = clock.now();
            mon.sweep(agents, bus).await
        }
        macro_rules! tick {
            () => {
                tick_with(&mut mon, &clock, &agents, &bus, healthy.id)
            };
        }

        let mut killed = Vec::new();
        for _ in 0..3 {
            killed.extend(tick!().await.killed);
        }
        assert_eq!(killed, vec![stuck.id]);
        {
            let map = agents.read().await;
            assert_eq!(map[&stuck.id].status, AgentStatus::Stopped);
            assert_eq!(map[&healthy.id].status, AgentStatus::Active);
        }
        let msgs: Vec<_> = rx.try_iter().collect();
        let count = |kind: &str| {
            msgs.iter()
                .filter(|m| matches!(&***m, BridgeMessage::Event(e) if e.event_type == kind && e.agent_id == Some(stuck.id)))
                .count()
        };
        assert_eq!(count(EVENT_AGENT_FORCE_KILL), 1);
        assert_eq!(count(EVENT_AGENT_STUCK_ESCALATION), 1);
        assert!(msgs
            .iter()
            .any(|m| matches!(&**m, BridgeMessage::AgentUpdated(a)
            if a.id == stuck.id && a.status == AgentStatus::Stopped)));

        // Many sweeps during cooldown: no further kill or escalation.
        // Killed at 3*31 = 93 s; cooldown runs until 393 s. Ticks 4..=12 (<= 372 s).
        for _ in 0..9 {
            let sweep = tick!().await;
            assert!(sweep.killed.is_empty());
            assert_eq!(sweep.cooling_down, vec![stuck.id]);
        }
        assert_eq!(events(&rx, EVENT_AGENT_STUCK_ESCALATION), 0);
        assert!(!mon.detector().slot_reusable(stuck.id, clock.now()));

        // Tick 13 = 403 s >= 393 s: releases the slot exactly once.
        let sweep = tick!().await;
        assert_eq!(sweep.released, vec![stuck.id]);
        assert!(mon.detector().slot_reusable(stuck.id, clock.now()));
        assert_eq!(events(&rx, EVENT_AGENT_SLOT_RELEASED), 1);

        let sweep = tick!().await;
        assert!(sweep.released.is_empty());
        assert!(sweep.killed.is_empty());
        assert_eq!(events(&rx, EVENT_AGENT_SLOT_RELEASED), 0);
        assert_eq!(events(&rx, EVENT_AGENT_STUCK_ESCALATION), 0);
    }

    #[tokio::test]
    async fn removed_agent_in_cooldown_is_released_on_expiry() {
        let clock = FakeClock::at(t0());
        let mut mon = StuckMonitor::new(StuckPolicy::default(), clock.clone());
        let stuck = live_agent(t0());
        let agents = registry(std::slice::from_ref(&stuck));
        let bus = EventBus::new();
        for _ in 0..3 {
            clock.advance(s(31));
            mon.sweep(&agents, &bus).await;
        }
        agents.write().await.clear();
        let rx = bus.subscribe();

        clock.advance(s(100));
        assert!(mon.sweep(&agents, &bus).await.released.is_empty());
        clock.advance(s(300));
        assert_eq!(mon.sweep(&agents, &bus).await.released, vec![stuck.id]);
        assert_eq!(events(&rx, EVENT_AGENT_SLOT_RELEASED), 1);
    }
}
