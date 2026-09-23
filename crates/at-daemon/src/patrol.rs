use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::Result;
use at_bridge::event_bus::EventBus;
use at_bridge::http_api::ApiState;
use at_bridge::protocol::{BridgeMessage, EventPayload};
use at_core::cache::{CacheDb, CacheError};
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
            .map_err(|e| match e {
                CacheError::InvalidRow { ref context, .. } => {
                    // Persistent schema corruption — log at error; operator action required.
                    tracing::error!(
                        context = %context,
                        "slung bead row has corrupt data (schema drift?); \
                         patrol cannot complete stuck-bead check"
                    );
                    anyhow::anyhow!("slung beads contain corrupt row data — {}: {}", context, e)
                }
                CacheError::Db(ref db_err) => {
                    // Transient DB error — patrol will retry on the next cycle.
                    tracing::warn!(
                        error = %db_err,
                        "transient DB error querying slung beads; patrol will retry next cycle"
                    );
                    anyhow::anyhow!("transient DB error querying slung beads: {}", db_err)
                }
            })?;

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
/// The `at-agents` executor that owns the agent's process aborts it on receipt.
pub const EVENT_AGENT_FORCE_KILL: &str = at_bridge::protocol::EVENT_AGENT_FORCE_KILL;
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

#[cfg(test)]
mod tests {
    use super::*;
    use at_core::types::{Bead, Lane};

    // -----------------------------------------------------------------------
    // Mock plumbing for reap_orphan_ptys tests
    // -----------------------------------------------------------------------

    use at_bridge::event_bus::EventBus;
    use at_bridge::http_api::ApiState;
    use at_bridge::terminal::{TerminalInfo, TerminalRegistry, TerminalStatus};
    use at_session::pty_pool::PtyHandle;
    use portable_pty::{Child, ChildKiller, ExitStatus, MasterPty, PtySize};
    use std::io;
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    // ----- MockChildState ---------------------------------------------------

    struct MockChildState {
        /// When `true`, `try_wait` returns `Ok(Some(exited))`, making
        /// `PtyHandle::is_alive()` return `false`.
        is_dead: bool,
        /// When `true`, `kill()` returns an `Err`.
        should_fail_kill: bool,
        /// Records whether `kill()` was called.
        kill_called: bool,
    }

    impl std::fmt::Debug for MockChildState {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("MockChildState")
                .field("is_dead", &self.is_dead)
                .field("kill_called", &self.kill_called)
                .finish()
        }
    }

    // ----- MockChild (shared via Arc<Mutex<_>>) ----------------------------

    #[derive(Debug, Clone)]
    struct MockChild(Arc<Mutex<MockChildState>>);

    impl MockChild {
        fn new(is_dead: bool, should_fail_kill: bool) -> Self {
            Self(Arc::new(Mutex::new(MockChildState {
                is_dead,
                should_fail_kill,
                kill_called: false,
            })))
        }

        fn kill_was_called(&self) -> bool {
            self.0.lock().unwrap().kill_called
        }
    }

    impl ChildKiller for MockChild {
        fn kill(&mut self) -> io::Result<()> {
            let mut state = self.0.lock().unwrap();
            state.kill_called = true;
            if state.should_fail_kill {
                Err(io::Error::other("mock kill failure"))
            } else {
                Ok(())
            }
        }

        fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
            Box::new(self.clone())
        }
    }

    impl Child for MockChild {
        fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
            let state = self.0.lock().unwrap();
            if state.is_dead {
                Ok(Some(ExitStatus::with_exit_code(0)))
            } else {
                Ok(None)
            }
        }

        fn wait(&mut self) -> io::Result<ExitStatus> {
            Ok(ExitStatus::with_exit_code(0))
        }

        fn process_id(&self) -> Option<u32> {
            None
        }
    }

    // ----- MockMasterPty ---------------------------------------------------

    #[derive(Debug)]
    struct MockMasterPty;

    impl MasterPty for MockMasterPty {
        fn resize(&self, _size: PtySize) -> Result<(), anyhow::Error> {
            Ok(())
        }

        fn get_size(&self) -> Result<PtySize, anyhow::Error> {
            Ok(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
        }

        fn try_clone_reader(&self) -> Result<Box<dyn io::Read + Send>, anyhow::Error> {
            // Return a reader that immediately yields EOF.
            Ok(Box::new(io::empty()))
        }

        fn take_writer(&self) -> Result<Box<dyn io::Write + Send>, anyhow::Error> {
            Ok(Box::new(io::sink()))
        }

        #[cfg(unix)]
        fn process_group_leader(&self) -> Option<libc::pid_t> {
            None
        }

        #[cfg(unix)]
        fn as_raw_fd(&self) -> Option<std::os::unix::io::RawFd> {
            None
        }
    }

    // ----- builder helpers -------------------------------------------------

    /// Build a `PtyHandle` backed entirely by in-memory mocks.
    ///
    /// Returns `(handle, mock_child)` so callers can inspect `kill_was_called`
    /// after the function under test has run.  Requires the `test-helpers`
    /// feature on `at-session` (declared in `[dev-dependencies]`).
    fn make_mock_handle(id: Uuid, is_dead: bool, should_fail_kill: bool) -> (PtyHandle, MockChild) {
        let child = MockChild::new(is_dead, should_fail_kill);
        let (_, rx) = flume::bounded::<Vec<u8>>(1);
        let (tx, _) = flume::bounded::<Vec<u8>>(1);
        let handle = PtyHandle::from_parts(
            id,
            rx,
            tx,
            Arc::new(Mutex::new(
                Box::new(child.clone()) as Box<dyn portable_pty::Child + Send + Sync>
            )),
            Arc::new(Mutex::new(
                Box::new(MockMasterPty) as Box<dyn MasterPty + Send>
            )),
        );
        (handle, child)
    }

    /// Build a minimal `TerminalInfo` with the given `id`.
    fn make_terminal_info(id: Uuid) -> TerminalInfo {
        TerminalInfo {
            id,
            agent_id: Uuid::new_v4(),
            title: "test".into(),
            status: TerminalStatus::Active,
            cols: 80,
            rows: 24,
            font_size: 14,
            font_family: "monospace".into(),
            line_height: 1.2,
            letter_spacing: 0.0,
            profile: "default".into(),
            cursor_style: "block".into(),
            cursor_blink: false,
            auto_name: None,
            persistent: false,
        }
    }

    /// Create a fresh `ApiState` seeded with the given registries.
    ///
    /// The seam is implemented via direct field writes to the `pub` fields
    /// `terminal_registry` and `pty_handles` on `ApiState` — no new constructor
    /// is required.  This keeps the seam minimal and avoids gating issues with
    /// `#[cfg(test)]` across crate boundaries.
    async fn make_state_with(
        registry: TerminalRegistry,
        handles: std::collections::HashMap<Uuid, PtyHandle>,
    ) -> Arc<ApiState> {
        let state = Arc::new(ApiState::new(EventBus::new()));
        *state.terminal_registry.write().await = registry;
        *state.pty_handles.write().await = handles;
        state
    }

    // -----------------------------------------------------------------------
    // Tests for reap_orphan_ptys
    // -----------------------------------------------------------------------

    /// Both registries are fully consistent (live PTY for each terminal entry).
    /// `reap_orphan_ptys` must return 0, call no kills, and leave both maps intact.
    #[tokio::test]
    async fn reap_orphan_ptys_no_orphans_no_kills() {
        let tid = Uuid::new_v4();

        let mut registry = TerminalRegistry::new();
        registry.register(make_terminal_info(tid));

        // is_dead=false => PtyHandle::is_alive() returns true
        let (handle, mock_child) = make_mock_handle(tid, false, false);
        let mut handles = std::collections::HashMap::new();
        handles.insert(tid, handle);

        let state = make_state_with(registry, handles).await;

        let reaped = reap_orphan_ptys(&state).await;

        assert_eq!(
            reaped, 0,
            "no orphans expected when both maps are consistent"
        );
        assert!(
            !mock_child.kill_was_called(),
            "kill() must not be called when the PTY is alive"
        );
        assert_eq!(
            state.terminal_registry.read().await.list().len(),
            1,
            "terminal_registry must be unchanged"
        );
        assert_eq!(
            state.pty_handles.read().await.len(),
            1,
            "pty_handles must be unchanged"
        );
    }

    /// A terminal-registry entry whose backing PTY has died (is_alive() == false)
    /// is an orphan.  The function must call kill() on the handle and remove it
    /// from both maps.
    #[tokio::test]
    async fn reap_orphan_ptys_kills_orphan_pty_with_no_terminal() {
        let tid = Uuid::new_v4();

        let mut registry = TerminalRegistry::new();
        registry.register(make_terminal_info(tid));

        // is_dead=true => PtyHandle::is_alive() returns false
        let (handle, mock_child) = make_mock_handle(tid, true, false);
        let mut handles = std::collections::HashMap::new();
        handles.insert(tid, handle);

        let state = make_state_with(registry, handles).await;

        let reaped = reap_orphan_ptys(&state).await;

        assert_eq!(reaped, 1, "exactly one orphan expected");
        assert!(
            mock_child.kill_was_called(),
            "kill() must be called on the dead PTY handle"
        );
        assert!(
            state.pty_handles.read().await.is_empty(),
            "dead handle must be removed from pty_handles"
        );
        assert!(
            state.terminal_registry.read().await.list().is_empty(),
            "dead terminal entry must be removed from terminal_registry"
        );
    }

    /// A terminal-registry entry with no matching PTY handle at all is an orphan.
    /// The function must remove the terminal entry and must NOT panic (there is
    /// nothing to kill since no handle exists).
    #[tokio::test]
    async fn reap_orphan_ptys_removes_terminal_entry_with_no_pty() {
        let tid = Uuid::new_v4();

        let mut registry = TerminalRegistry::new();
        registry.register(make_terminal_info(tid));

        // pty_handles is empty — no handle for this terminal at all.
        let state = make_state_with(registry, std::collections::HashMap::new()).await;

        let reaped = reap_orphan_ptys(&state).await;

        assert_eq!(reaped, 1, "terminal with no PTY handle is an orphan");
        assert!(
            state.terminal_registry.read().await.list().is_empty(),
            "orphan terminal entry must be removed from terminal_registry"
        );
        // pty_handles was empty and must remain empty — no crash.
        assert!(state.pty_handles.read().await.is_empty());
    }

    /// When `kill()` fails on the underlying child, `reap_orphan_ptys` must NOT
    /// panic.  The production code uses `let _ = handle.kill()` which discards
    /// the error, so the entry is removed regardless of kill outcome.
    #[tokio::test]
    async fn reap_orphan_ptys_handles_kill_failure_gracefully() {
        let tid = Uuid::new_v4();

        let mut registry = TerminalRegistry::new();
        registry.register(make_terminal_info(tid));

        // is_dead=true, should_fail_kill=true => kill() returns Err.
        let (handle, mock_child) = make_mock_handle(tid, true, true);
        let mut handles = std::collections::HashMap::new();
        handles.insert(tid, handle);

        let state = make_state_with(registry, handles).await;

        // Must NOT panic even though kill() returns Err.
        let reaped = reap_orphan_ptys(&state).await;

        assert_eq!(reaped, 1, "orphan count must be 1 even after kill failure");
        assert!(
            mock_child.kill_was_called(),
            "kill() must still be attempted on the orphaned handle"
        );
        // Graceful degradation: entry is removed regardless of kill outcome.
        assert!(
            state.pty_handles.read().await.is_empty(),
            "handle must be removed from pty_handles even when kill() fails"
        );
        assert!(
            state.terminal_registry.read().await.list().is_empty(),
            "terminal must be removed from terminal_registry even when kill() fails"
        );
    }

    fn make_slung_bead(slung_at: Option<DateTime<Utc>>) -> Bead {
        let mut bead = Bead::new("test bead", Lane::Standard);
        bead.status = BeadStatus::Slung;
        bead.slung_at = slung_at;
        bead
    }

    async fn insert_beads(cache: &CacheDb, beads: &[Bead]) {
        for b in beads {
            cache.upsert_bead(b).await.expect("upsert bead");
        }
    }

    #[test]
    fn new_uses_default_thirty_minute_slung_timeout() {
        let runner = PatrolRunner::new(60);
        assert_eq!(runner.slung_timeout, ChronoDuration::minutes(30));
    }

    #[test]
    fn new_records_heartbeat_interval() {
        let runner = PatrolRunner::new(123);
        assert_eq!(runner._heartbeat_interval_secs, 123);
    }

    #[test]
    fn with_slung_timeout_overrides_default() {
        let runner = PatrolRunner::new(60).with_slung_timeout(ChronoDuration::minutes(5));
        assert_eq!(runner.slung_timeout, ChronoDuration::minutes(5));
    }

    #[test]
    fn with_slung_timeout_accepts_zero() {
        let runner = PatrolRunner::new(60).with_slung_timeout(ChronoDuration::zero());
        assert_eq!(runner.slung_timeout, ChronoDuration::zero());
    }

    #[test]
    fn patrol_report_serde_round_trip() {
        let report = PatrolReport {
            stale_agents: 1,
            stuck_beads: 2,
            orphan_ptys: 3,
            stuck_bead_ids: vec![uuid::Uuid::new_v4(), uuid::Uuid::new_v4()],
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&report).expect("serialize");
        let back: PatrolReport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.stuck_beads, report.stuck_beads);
        assert_eq!(back.stale_agents, report.stale_agents);
        assert_eq!(back.orphan_ptys, report.orphan_ptys);
        assert_eq!(back.stuck_bead_ids, report.stuck_bead_ids);
    }

    #[tokio::test]
    async fn run_patrol_on_empty_cache_yields_zero_counts() {
        let runner = PatrolRunner::new(60);
        let cache = CacheDb::new_in_memory().await.expect("cache");

        let report = runner.run_patrol(&cache).await.expect("patrol");
        assert_eq!(report.stuck_beads, 0);
        assert_eq!(report.stale_agents, 0);
        assert_eq!(report.orphan_ptys, 0);
        assert!(report.stuck_bead_ids.is_empty());
    }

    #[tokio::test]
    async fn run_patrol_skips_beads_without_slung_at() {
        let runner = PatrolRunner::new(60);
        let cache = CacheDb::new_in_memory().await.expect("cache");

        // Slung beads with no slung_at timestamp must not be flagged.
        let bead = make_slung_bead(None);
        insert_beads(&cache, &[bead]).await;

        let report = runner.run_patrol(&cache).await.expect("patrol");
        assert_eq!(report.stuck_beads, 0);
        assert!(report.stuck_bead_ids.is_empty());
    }

    #[tokio::test]
    async fn run_patrol_does_not_flag_recently_slung_beads() {
        let runner = PatrolRunner::new(60);
        let cache = CacheDb::new_in_memory().await.expect("cache");

        // Slung 5 minutes ago, default timeout 30 minutes — not stuck.
        let recent = make_slung_bead(Some(Utc::now() - ChronoDuration::minutes(5)));
        insert_beads(&cache, &[recent]).await;

        let report = runner.run_patrol(&cache).await.expect("patrol");
        assert_eq!(report.stuck_beads, 0);
    }

    #[tokio::test]
    async fn run_patrol_flags_stuck_beads_past_timeout() {
        let runner = PatrolRunner::new(60);
        let cache = CacheDb::new_in_memory().await.expect("cache");

        // Slung 45 minutes ago, default timeout 30 minutes — stuck.
        let stuck = make_slung_bead(Some(Utc::now() - ChronoDuration::minutes(45)));
        let stuck_id = stuck.id;
        insert_beads(&cache, &[stuck]).await;

        let report = runner.run_patrol(&cache).await.expect("patrol");
        assert_eq!(report.stuck_beads, 1);
        assert_eq!(report.stuck_bead_ids, vec![stuck_id]);
    }

    #[tokio::test]
    async fn run_patrol_respects_custom_slung_timeout() {
        let runner = PatrolRunner::new(60).with_slung_timeout(ChronoDuration::seconds(10));
        let cache = CacheDb::new_in_memory().await.expect("cache");

        // Slung 30 seconds ago — exceeds the custom 10-second timeout.
        let stuck = make_slung_bead(Some(Utc::now() - ChronoDuration::seconds(30)));
        let stuck_id = stuck.id;
        insert_beads(&cache, &[stuck]).await;

        let report = runner.run_patrol(&cache).await.expect("patrol");
        assert_eq!(report.stuck_beads, 1);
        assert_eq!(report.stuck_bead_ids, vec![stuck_id]);
    }

    #[tokio::test]
    async fn run_patrol_ignores_non_slung_beads() {
        let runner = PatrolRunner::new(60);
        let cache = CacheDb::new_in_memory().await.expect("cache");

        // Backlog/Done beads must not be considered stuck regardless of
        // slung_at, because list_beads_by_status(Slung) excludes them.
        let mut backlog = Bead::new("backlog", Lane::Standard);
        backlog.slung_at = Some(Utc::now() - ChronoDuration::hours(99));
        let mut done = Bead::new("done", Lane::Standard);
        done.status = BeadStatus::Done;
        done.slung_at = Some(Utc::now() - ChronoDuration::hours(99));

        insert_beads(&cache, &[backlog, done]).await;

        let report = runner.run_patrol(&cache).await.expect("patrol");
        assert_eq!(report.stuck_beads, 0);
        assert!(report.stuck_bead_ids.is_empty());
    }

    #[tokio::test]
    async fn run_patrol_separates_stuck_from_fresh_in_mixed_population() {
        let runner = PatrolRunner::new(60);
        let cache = CacheDb::new_in_memory().await.expect("cache");

        let fresh = make_slung_bead(Some(Utc::now() - ChronoDuration::minutes(1)));
        let stuck1 = make_slung_bead(Some(Utc::now() - ChronoDuration::hours(1)));
        let stuck2 = make_slung_bead(Some(Utc::now() - ChronoDuration::hours(3)));
        let no_slung = make_slung_bead(None);

        let stuck_ids = [stuck1.id, stuck2.id];
        insert_beads(&cache, &[fresh, stuck1, stuck2, no_slung]).await;

        let report = runner.run_patrol(&cache).await.expect("patrol");

        assert_eq!(report.stuck_beads, 2);
        for id in stuck_ids {
            assert!(
                report.stuck_bead_ids.contains(&id),
                "expected stuck id {id} in report",
            );
        }
    }

    #[tokio::test]
    async fn run_patrol_timestamp_is_recent() {
        let runner = PatrolRunner::new(60);
        let cache = CacheDb::new_in_memory().await.expect("cache");

        let before = Utc::now();
        let report = runner.run_patrol(&cache).await.expect("patrol");
        let after = Utc::now();

        assert!(report.timestamp >= before);
        assert!(report.timestamp <= after);
    }

    #[tokio::test]
    async fn run_patrol_stale_agents_count_is_zero_placeholder() {
        // Per the impl, run_patrol always reports 0 stale agents because
        // CacheDb has no list-all-agents API. This test pins that contract.
        let runner = PatrolRunner::new(60);
        let cache = CacheDb::new_in_memory().await.expect("cache");
        let report = runner.run_patrol(&cache).await.expect("patrol");
        assert_eq!(report.stale_agents, 0);
        assert_eq!(report.orphan_ptys, 0);
    }

    // ----- CacheError::InvalidRow regression tests -----

    /// `CacheDb::list_beads_by_status` follows a skip-and-continue policy: a
    /// slung row with a corrupt lane value is skipped (logged + counted)
    /// rather than failing the whole query, so `run_patrol` must still
    /// return `Ok` (not propagate an error) and simply not count the corrupt
    /// row as stuck.
    #[tokio::test]
    async fn run_patrol_skips_invalid_row_in_slung_beads() {
        let runner = PatrolRunner::new(60);
        let cache = CacheDb::new_in_memory().await.expect("cache");

        // Insert a slung bead with a bogus lane — row_to_bead cannot decode
        // it, so list_beads_by_status skips this row instead of erroring.
        cache
            .insert_raw_bead_for_test(
                "550e8400-e29b-41d4-a716-446655440002",
                "slung",
                "GALAXY_BRAIN_LANE",
            )
            .await
            .expect("raw insert");

        let report = runner
            .run_patrol(&cache)
            .await
            .expect("run_patrol must not fail when a slung row is corrupt — it is skipped");
        assert_eq!(
            report.stuck_beads, 0,
            "the corrupt row was skipped, not counted as stuck"
        );
    }

    /// One corrupt slung row must not stop the stuck-bead check from finding
    /// a real stuck bead sitting alongside it.
    #[tokio::test]
    async fn run_patrol_finds_stuck_bead_when_another_slung_row_is_corrupt() {
        let runner = PatrolRunner::new(60);
        let cache = CacheDb::new_in_memory().await.expect("cache");

        // Corrupt slung row — unrecognised lane value, skipped by the cache.
        cache
            .insert_raw_bead_for_test(
                "550e8400-e29b-41d4-a716-446655440003",
                "slung",
                "GALAXY_BRAIN_LANE",
            )
            .await
            .expect("raw insert corrupt bead");

        // Well-formed stuck bead alongside it.
        let stuck = make_slung_bead(Some(Utc::now() - ChronoDuration::hours(1)));
        let stuck_id = stuck.id;
        insert_beads(&cache, &[stuck]).await;

        let report = runner
            .run_patrol(&cache)
            .await
            .expect("run_patrol must still succeed despite the corrupt row");
        assert_eq!(report.stuck_beads, 1);
        assert_eq!(report.stuck_bead_ids, vec![stuck_id]);
    }

    /// When the slung bead query succeeds (no corruption), `run_patrol` must
    /// return `Ok` even if there are stuck beads mixed with fresh ones — confirming
    /// the baseline happy-path is unaffected by the error-handling changes.
    #[tokio::test]
    async fn run_patrol_ok_with_valid_slung_beads_after_error_handling_change() {
        let runner = PatrolRunner::new(60).with_slung_timeout(ChronoDuration::minutes(10));
        let cache = CacheDb::new_in_memory().await.expect("cache");

        let fresh = make_slung_bead(Some(Utc::now() - ChronoDuration::minutes(1)));
        let stuck = make_slung_bead(Some(Utc::now() - ChronoDuration::hours(1)));
        insert_beads(&cache, &[fresh, stuck]).await;

        let report = runner
            .run_patrol(&cache)
            .await
            .expect("patrol must succeed");
        assert_eq!(report.stuck_beads, 1);
    }
}
