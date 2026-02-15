use at_core::types::{AgentRole, Bead};
use uuid::Uuid;
use std::collections::BinaryHeap;
use std::cmp::Ordering;

use crate::lifecycle::{AgentLifecycle, Result};

// ---------------------------------------------------------------------------
// PrioritizedBead — used by MayorAgent's priority queue
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct PrioritizedBead {
    priority: i32,
    bead_id: Uuid,
}

impl PartialEq for PrioritizedBead {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.bead_id == other.bead_id
    }
}

impl Eq for PrioritizedBead {}

impl PartialOrd for PrioritizedBead {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PrioritizedBead {
    fn cmp(&self, other: &Self) -> Ordering {
        self.priority.cmp(&other.priority)
    }
}

// ===========================================================================
// MayorAgent — orchestrator, assigns work, manages convoys
// ===========================================================================

pub struct MayorAgent {
    queue: BinaryHeap<PrioritizedBead>,
}

impl MayorAgent {
    pub fn new() -> Self {
        Self {
            queue: BinaryHeap::new(),
        }
    }

    /// Number of beads in the priority queue.
    pub fn queue_len(&self) -> usize {
        self.queue.len()
    }
}

impl Default for MayorAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AgentLifecycle for MayorAgent {
    fn role(&self) -> AgentRole {
        AgentRole::Mayor
    }

    async fn on_start(&mut self) -> Result<()> {
        tracing::info!("MayorAgent started — ready to orchestrate");
        Ok(())
    }

    async fn on_task_assigned(&mut self, bead: &Bead) -> Result<()> {
        tracing::info!(bead_id = %bead.id, "MayorAgent received bead, enqueuing");
        self.queue.push(PrioritizedBead {
            priority: bead.priority,
            bead_id: bead.id,
        });
        Ok(())
    }

    async fn on_task_completed(&mut self, bead_id: Uuid) -> Result<()> {
        tracing::info!(bead_id = %bead_id, "MayorAgent noted bead completion");
        Ok(())
    }

    async fn on_heartbeat(&mut self) -> Result<()> {
        tracing::debug!(queue_len = self.queue.len(), "MayorAgent heartbeat");
        Ok(())
    }

    async fn on_stop(&mut self) -> Result<()> {
        tracing::info!("MayorAgent stopping");
        Ok(())
    }
}

// ===========================================================================
// DeaconAgent — patrol agent, periodic checks, health monitoring
// ===========================================================================

pub struct DeaconAgent {
    checks_performed: u64,
}

impl DeaconAgent {
    pub fn new() -> Self {
        Self {
            checks_performed: 0,
        }
    }

    pub fn checks_performed(&self) -> u64 {
        self.checks_performed
    }
}

impl Default for DeaconAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AgentLifecycle for DeaconAgent {
    fn role(&self) -> AgentRole {
        AgentRole::Deacon
    }

    async fn on_start(&mut self) -> Result<()> {
        tracing::info!("DeaconAgent started — patrol mode active");
        Ok(())
    }

    async fn on_task_assigned(&mut self, bead: &Bead) -> Result<()> {
        tracing::info!(bead_id = %bead.id, "DeaconAgent assigned health-check bead");
        Ok(())
    }

    async fn on_task_completed(&mut self, bead_id: Uuid) -> Result<()> {
        tracing::info!(bead_id = %bead_id, "DeaconAgent completed check");
        self.checks_performed += 1;
        Ok(())
    }

    async fn on_heartbeat(&mut self) -> Result<()> {
        tracing::debug!(checks = self.checks_performed, "DeaconAgent heartbeat");
        Ok(())
    }

    async fn on_stop(&mut self) -> Result<()> {
        tracing::info!("DeaconAgent stopping");
        Ok(())
    }
}

// ===========================================================================
// WitnessAgent — observer, logs events, generates audit trail
// ===========================================================================

pub struct WitnessAgent {
    events_observed: u64,
}

impl WitnessAgent {
    pub fn new() -> Self {
        Self {
            events_observed: 0,
        }
    }

    pub fn events_observed(&self) -> u64 {
        self.events_observed
    }
}

impl Default for WitnessAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AgentLifecycle for WitnessAgent {
    fn role(&self) -> AgentRole {
        AgentRole::Witness
    }

    async fn on_start(&mut self) -> Result<()> {
        tracing::info!("WitnessAgent started — observing");
        Ok(())
    }

    async fn on_task_assigned(&mut self, bead: &Bead) -> Result<()> {
        tracing::info!(bead_id = %bead.id, "WitnessAgent observing bead");
        self.events_observed += 1;
        Ok(())
    }

    async fn on_task_completed(&mut self, bead_id: Uuid) -> Result<()> {
        tracing::info!(bead_id = %bead_id, "WitnessAgent recorded completion");
        self.events_observed += 1;
        Ok(())
    }

    async fn on_heartbeat(&mut self) -> Result<()> {
        tracing::debug!(events = self.events_observed, "WitnessAgent heartbeat");
        Ok(())
    }

    async fn on_stop(&mut self) -> Result<()> {
        tracing::info!("WitnessAgent stopping — audit trail sealed");
        Ok(())
    }
}

// ===========================================================================
// RefineryAgent — code quality, linting/formatting/tests
// ===========================================================================

pub struct RefineryAgent {
    runs_completed: u64,
}

impl RefineryAgent {
    pub fn new() -> Self {
        Self {
            runs_completed: 0,
        }
    }

    pub fn runs_completed(&self) -> u64 {
        self.runs_completed
    }
}

impl Default for RefineryAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AgentLifecycle for RefineryAgent {
    fn role(&self) -> AgentRole {
        AgentRole::Refinery
    }

    async fn on_start(&mut self) -> Result<()> {
        tracing::info!("RefineryAgent started — quality gates armed");
        Ok(())
    }

    async fn on_task_assigned(&mut self, bead: &Bead) -> Result<()> {
        tracing::info!(bead_id = %bead.id, "RefineryAgent queued quality run");
        Ok(())
    }

    async fn on_task_completed(&mut self, bead_id: Uuid) -> Result<()> {
        tracing::info!(bead_id = %bead_id, "RefineryAgent quality run complete");
        self.runs_completed += 1;
        Ok(())
    }

    async fn on_heartbeat(&mut self) -> Result<()> {
        tracing::debug!(runs = self.runs_completed, "RefineryAgent heartbeat");
        Ok(())
    }

    async fn on_stop(&mut self) -> Result<()> {
        tracing::info!("RefineryAgent stopping");
        Ok(())
    }
}

// ===========================================================================
// PolecatAgent — git worktree manager, branch creation/cleanup
// ===========================================================================

pub struct PolecatAgent {
    branches_managed: u64,
}

impl PolecatAgent {
    pub fn new() -> Self {
        Self {
            branches_managed: 0,
        }
    }

    pub fn branches_managed(&self) -> u64 {
        self.branches_managed
    }
}

impl Default for PolecatAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AgentLifecycle for PolecatAgent {
    fn role(&self) -> AgentRole {
        AgentRole::Polecat
    }

    async fn on_start(&mut self) -> Result<()> {
        tracing::info!("PolecatAgent started — worktree manager online");
        Ok(())
    }

    async fn on_task_assigned(&mut self, bead: &Bead) -> Result<()> {
        tracing::info!(bead_id = %bead.id, branch = ?bead.git_branch, "PolecatAgent managing branch");
        Ok(())
    }

    async fn on_task_completed(&mut self, bead_id: Uuid) -> Result<()> {
        tracing::info!(bead_id = %bead_id, "PolecatAgent branch work complete");
        self.branches_managed += 1;
        Ok(())
    }

    async fn on_heartbeat(&mut self) -> Result<()> {
        tracing::debug!(branches = self.branches_managed, "PolecatAgent heartbeat");
        Ok(())
    }

    async fn on_stop(&mut self) -> Result<()> {
        tracing::info!("PolecatAgent stopping — cleaning up worktrees");
        Ok(())
    }
}

// ===========================================================================
// CrewAgent — general worker, executes assigned beads
// ===========================================================================

pub struct CrewAgent {
    beads_executed: u64,
}

impl CrewAgent {
    pub fn new() -> Self {
        Self {
            beads_executed: 0,
        }
    }

    pub fn beads_executed(&self) -> u64 {
        self.beads_executed
    }
}

impl Default for CrewAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AgentLifecycle for CrewAgent {
    fn role(&self) -> AgentRole {
        AgentRole::Crew
    }

    async fn on_start(&mut self) -> Result<()> {
        tracing::info!("CrewAgent started — ready for work");
        Ok(())
    }

    async fn on_task_assigned(&mut self, bead: &Bead) -> Result<()> {
        tracing::info!(bead_id = %bead.id, "CrewAgent picked up bead");
        Ok(())
    }

    async fn on_task_completed(&mut self, bead_id: Uuid) -> Result<()> {
        tracing::info!(bead_id = %bead_id, "CrewAgent finished bead");
        self.beads_executed += 1;
        Ok(())
    }

    async fn on_heartbeat(&mut self) -> Result<()> {
        tracing::debug!(executed = self.beads_executed, "CrewAgent heartbeat");
        Ok(())
    }

    async fn on_stop(&mut self) -> Result<()> {
        tracing::info!("CrewAgent stopping");
        Ok(())
    }
}
