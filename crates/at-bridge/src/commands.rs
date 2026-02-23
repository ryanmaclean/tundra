use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::command_registry::{
    CommandCategory, CommandContext, CommandDescriptor, CommandError, CommandHandler,
    CommandOutput, CommandRegistry, CommandSource, Result,
};
use crate::event_bus::EventBus;
use crate::protocol::BridgeMessage;

use at_core::types::{Agent, AgentStatus, Bead, KpiSnapshot, Lane, Task, TaskPhase};

// ---------------------------------------------------------------------------
// Shared state handle for command handlers
// ---------------------------------------------------------------------------

/// State accessible to built-in command handlers.
#[derive(Clone)]
pub struct CommandState {
    pub beads: Arc<RwLock<Vec<Bead>>>,
    pub agents: Arc<RwLock<Vec<Agent>>>,
    pub tasks: Arc<RwLock<Vec<Task>>>,
    pub kpi: Arc<RwLock<KpiSnapshot>>,
    pub event_bus: EventBus,
}

// ---------------------------------------------------------------------------
// Built-in command handlers
// ---------------------------------------------------------------------------

struct ListBeadsHandler(CommandState);

#[async_trait]
impl CommandHandler for ListBeadsHandler {
    async fn execute(&self, _ctx: CommandContext) -> Result<CommandOutput> {
        let beads = self.0.beads.read().await;
        let data = serde_json::to_value(&*beads)
            .map_err(|e| CommandError::ExecutionFailed(e.to_string()))?;
        Ok(CommandOutput::ok_data(data))
    }
}

struct CreateBeadHandler(CommandState);

#[async_trait]
impl CommandHandler for CreateBeadHandler {
    async fn execute(&self, ctx: CommandContext) -> Result<CommandOutput> {
        let title = ctx
            .get_str("title")
            .ok_or_else(|| CommandError::InvalidArgs("missing 'title' parameter".into()))?
            .to_string();

        let lane = match ctx.get_str("lane") {
            Some("critical") => Lane::Critical,
            Some("experimental") => Lane::Experimental,
            _ => Lane::Standard,
        };

        let bead = Bead::new(title, lane);
        let bead_json = serde_json::to_value(&bead)
            .map_err(|e| CommandError::ExecutionFailed(e.to_string()))?;

        let mut beads = self.0.beads.write().await;
        beads.push(bead);
        self.0
            .event_bus
            .publish(BridgeMessage::BeadList(beads.clone()));

        Ok(CommandOutput::ok_data(bead_json))
    }
}

struct ListAgentsHandler(CommandState);

#[async_trait]
impl CommandHandler for ListAgentsHandler {
    async fn execute(&self, _ctx: CommandContext) -> Result<CommandOutput> {
        let agents = self.0.agents.read().await;
        let data = serde_json::to_value(&*agents)
            .map_err(|e| CommandError::ExecutionFailed(e.to_string()))?;
        Ok(CommandOutput::ok_data(data))
    }
}

struct StopAgentHandler(CommandState);

#[async_trait]
impl CommandHandler for StopAgentHandler {
    async fn execute(&self, ctx: CommandContext) -> Result<CommandOutput> {
        let name = ctx
            .get_str("name")
            .ok_or_else(|| CommandError::InvalidArgs("missing 'name' parameter".into()))?
            .to_string();

        let mut agents = self.0.agents.write().await;
        let agent = agents
            .iter_mut()
            .find(|a| a.name == name)
            .ok_or_else(|| CommandError::ExecutionFailed(format!("agent '{}' not found", name)))?;

        agent.status = AgentStatus::Stopped;
        agent.last_seen = chrono::Utc::now();

        Ok(CommandOutput::ok(format!("agent '{}' stopped", name)))
    }
}

struct ListTasksHandler(CommandState);

#[async_trait]
impl CommandHandler for ListTasksHandler {
    async fn execute(&self, _ctx: CommandContext) -> Result<CommandOutput> {
        let tasks = self.0.tasks.read().await;
        let data = serde_json::to_value(&*tasks)
            .map_err(|e| CommandError::ExecutionFailed(e.to_string()))?;
        Ok(CommandOutput::ok_data(data))
    }
}

struct GetKpiHandler(CommandState);

#[async_trait]
impl CommandHandler for GetKpiHandler {
    async fn execute(&self, _ctx: CommandContext) -> Result<CommandOutput> {
        let kpi = self.0.kpi.read().await;
        let data = serde_json::to_value(&*kpi)
            .map_err(|e| CommandError::ExecutionFailed(e.to_string()))?;
        Ok(CommandOutput::ok_data(data))
    }
}

struct AdvanceTaskPhaseHandler(CommandState);

#[async_trait]
impl CommandHandler for AdvanceTaskPhaseHandler {
    async fn execute(&self, ctx: CommandContext) -> Result<CommandOutput> {
        let task_id_str = ctx
            .get_str("task_id")
            .ok_or_else(|| CommandError::InvalidArgs("missing 'task_id' parameter".into()))?;

        let task_id: uuid::Uuid = task_id_str
            .parse()
            .map_err(|_| CommandError::InvalidArgs("invalid UUID".into()))?;

        let phase_str = ctx
            .get_str("phase")
            .ok_or_else(|| CommandError::InvalidArgs("missing 'phase' parameter".into()))?;

        let phase: TaskPhase = serde_json::from_value(serde_json::json!(phase_str))
            .map_err(|e| CommandError::InvalidArgs(format!("invalid phase: {}", e)))?;

        let mut tasks = self.0.tasks.write().await;
        let task = tasks
            .iter_mut()
            .find(|t| t.id == task_id)
            .ok_or_else(|| CommandError::ExecutionFailed("task not found".into()))?;

        if !task.phase.can_transition_to(&phase) {
            return Err(CommandError::ExecutionFailed(format!(
                "cannot transition from {:?} to {:?}",
                task.phase, phase
            )));
        }

        task.set_phase(phase);
        let snapshot = task.clone();
        drop(tasks);

        self.0
            .event_bus
            .publish(BridgeMessage::TaskUpdate(Box::new(snapshot)));

        Ok(CommandOutput::ok("phase advanced"))
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// Register all built-in commands into the given registry.
pub fn register_default_commands(registry: &mut CommandRegistry, state: CommandState) {
    let all_sources = vec![
        CommandSource::Tui,
        CommandSource::Web,
        CommandSource::Cli,
        CommandSource::Api,
        CommandSource::Keybinding,
    ];

    // ---- Bead commands ----
    registry.register(
        CommandDescriptor {
            name: "bead.list".into(),
            title: "List Beads".into(),
            description: "List all beads in the system".into(),
            category: CommandCategory::Bead,
            keybinding: Some("ctrl+b".into()),
            available_from: all_sources.clone(),
            enabled: true,
        },
        Arc::new(ListBeadsHandler(state.clone())),
    );

    registry.register(
        CommandDescriptor {
            name: "bead.create".into(),
            title: "Create Bead".into(),
            description: "Create a new bead (params: title, lane)".into(),
            category: CommandCategory::Bead,
            keybinding: Some("ctrl+n".into()),
            available_from: all_sources.clone(),
            enabled: true,
        },
        Arc::new(CreateBeadHandler(state.clone())),
    );

    // ---- Agent commands ----
    registry.register(
        CommandDescriptor {
            name: "agent.list".into(),
            title: "List Agents".into(),
            description: "List all registered agents".into(),
            category: CommandCategory::Agent,
            keybinding: Some("ctrl+a".into()),
            available_from: all_sources.clone(),
            enabled: true,
        },
        Arc::new(ListAgentsHandler(state.clone())),
    );

    registry.register(
        CommandDescriptor {
            name: "agent.stop".into(),
            title: "Stop Agent".into(),
            description: "Stop an agent by name (params: name)".into(),
            category: CommandCategory::Agent,
            keybinding: None,
            available_from: all_sources.clone(),
            enabled: true,
        },
        Arc::new(StopAgentHandler(state.clone())),
    );

    // ---- Task commands ----
    registry.register(
        CommandDescriptor {
            name: "task.list".into(),
            title: "List Tasks".into(),
            description: "List all tasks".into(),
            category: CommandCategory::Session,
            keybinding: Some("ctrl+t".into()),
            available_from: all_sources.clone(),
            enabled: true,
        },
        Arc::new(ListTasksHandler(state.clone())),
    );

    registry.register(
        CommandDescriptor {
            name: "task.advance_phase".into(),
            title: "Advance Task Phase".into(),
            description: "Move a task to the next phase (params: task_id, phase)".into(),
            category: CommandCategory::Session,
            keybinding: None,
            available_from: all_sources.clone(),
            enabled: true,
        },
        Arc::new(AdvanceTaskPhaseHandler(state.clone())),
    );

    // ---- System commands ----
    registry.register(
        CommandDescriptor {
            name: "system.kpi".into(),
            title: "Show KPI".into(),
            description: "Display current KPI metrics".into(),
            category: CommandCategory::System,
            keybinding: Some("ctrl+k".into()),
            available_from: all_sources,
            enabled: true,
        },
        Arc::new(GetKpiHandler(state)),
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::EventBus;

    fn test_state() -> CommandState {
        CommandState {
            beads: Arc::new(RwLock::new(Vec::new())),
            agents: Arc::new(RwLock::new(Vec::new())),
            tasks: Arc::new(RwLock::new(Vec::new())),
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
            event_bus: EventBus::new(),
        }
    }

    #[tokio::test]
    async fn list_beads_empty() {
        let state = test_state();
        let mut reg = CommandRegistry::new();
        register_default_commands(&mut reg, state);

        let ctx = CommandContext::new(CommandSource::Tui, "");
        let output = reg.execute("bead.list", ctx).await.unwrap();
        assert!(output.success);
        let data = output.data.unwrap();
        assert!(data.as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn create_bead_via_command() {
        let state = test_state();
        let mut reg = CommandRegistry::new();
        register_default_commands(&mut reg, state.clone());

        let ctx = CommandContext::new(CommandSource::Cli, "")
            .with_param("title", serde_json::json!("My bead"));
        let output = reg.execute("bead.create", ctx).await.unwrap();
        assert!(output.success);

        let beads = state.beads.read().await;
        assert_eq!(beads.len(), 1);
        assert_eq!(beads[0].title, "My bead");
    }

    #[tokio::test]
    async fn create_bead_missing_title() {
        let state = test_state();
        let mut reg = CommandRegistry::new();
        register_default_commands(&mut reg, state);

        let ctx = CommandContext::new(CommandSource::Tui, "");
        let result = reg.execute("bead.create", ctx).await;
        assert!(matches!(result, Err(CommandError::InvalidArgs(_))));
    }

    #[tokio::test]
    async fn list_agents_empty() {
        let state = test_state();
        let mut reg = CommandRegistry::new();
        register_default_commands(&mut reg, state);

        let ctx = CommandContext::new(CommandSource::Web, "");
        let output = reg.execute("agent.list", ctx).await.unwrap();
        assert!(output.success);
    }

    #[tokio::test]
    async fn stop_agent_not_found() {
        let state = test_state();
        let mut reg = CommandRegistry::new();
        register_default_commands(&mut reg, state);

        let ctx = CommandContext::new(CommandSource::Tui, "")
            .with_param("name", serde_json::json!("ghost"));
        let result = reg.execute("agent.stop", ctx).await;
        assert!(matches!(result, Err(CommandError::ExecutionFailed(_))));
    }

    #[tokio::test]
    async fn get_kpi_command() {
        let state = test_state();
        let mut reg = CommandRegistry::new();
        register_default_commands(&mut reg, state);

        let ctx = CommandContext::new(CommandSource::Tui, "");
        let output = reg.execute("system.kpi", ctx).await.unwrap();
        assert!(output.success);
        let data = output.data.unwrap();
        assert_eq!(data["total_beads"], 0);
    }
}
