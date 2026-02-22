use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use crate::agent::{Agent, AgentError, AgentEvent};
use crate::provider::LlmProvider;
use crate::types::*;

type RoutingFn = Arc<dyn Fn(&str, &[String]) -> Vec<String> + Send + Sync>;

/// Strategy for routing messages to agents.
#[derive(Clone)]
pub enum RoutingStrategy {
    /// Always route to a specific agent.
    Fixed(String),
    /// Route based on a keyword map (keyword -> agent name).
    KeywordBased(HashMap<String, String>),
    /// Use a "router" agent/LLM to decide which agent should handle the request.
    LlmRouter {
        /// System prompt for the router that should output the agent name.
        router_prompt: String,
    },
    /// Execute agents in a fixed sequence (pipeline).
    Pipeline(Vec<String>),
    /// Let all agents run in parallel, then merge results.
    Parallel,
    /// Custom routing via a callback.
    Custom(RoutingFn),
}

/// The multi-agent orchestrator.
pub struct Orchestrator {
    agents: HashMap<String, Agent>,
    strategy: RoutingStrategy,
    router_provider: Option<Arc<dyn LlmProvider>>,
    event_tx: Option<mpsc::Sender<AgentEvent>>,
}

impl Orchestrator {
    pub fn builder() -> OrchestratorBuilder {
        OrchestratorBuilder::new()
    }

    /// Run the orchestrator with user input and return the final output.
    pub async fn run(
        &self,
        conversation_id: &str,
        user_input: &str,
    ) -> Result<String, OrchestratorError> {
        let agent_names: Vec<String> = self.agents.keys().cloned().collect();
        info!(
            "Orchestrator running with {} agents, strategy: {:?}",
            agent_names.len(),
            strategy_name(&self.strategy)
        );

        let targets = self.route(user_input, &agent_names).await?;

        if targets.is_empty() {
            return Err(OrchestratorError::NoAgentSelected);
        }

        match &self.strategy {
            RoutingStrategy::Pipeline(_) => {
                self.run_pipeline(conversation_id, user_input, &targets)
                    .await
            }
            RoutingStrategy::Parallel => {
                self.run_parallel(conversation_id, user_input, &targets)
                    .await
            }
            _ => {
                // Single agent execution (Fixed, Keyword, LlmRouter, Custom all resolve to targets).
                let agent_name = &targets[0];
                self.run_single(conversation_id, user_input, agent_name)
                    .await
            }
        }
    }

    /// Route user input to determine which agent(s) should handle it.
    async fn route(
        &self,
        user_input: &str,
        agent_names: &[String],
    ) -> Result<Vec<String>, OrchestratorError> {
        match &self.strategy {
            RoutingStrategy::Fixed(name) => Ok(vec![name.clone()]),

            RoutingStrategy::KeywordBased(map) => {
                let lower = user_input.to_lowercase();
                for (keyword, agent) in map {
                    if lower.contains(&keyword.to_lowercase()) {
                        return Ok(vec![agent.clone()]);
                    }
                }
                // Default to first agent.
                Ok(vec![agent_names.first().cloned().unwrap_or_default()])
            }

            RoutingStrategy::LlmRouter { router_prompt } => {
                let provider = self.router_provider.as_ref().ok_or_else(|| {
                    OrchestratorError::Config("LlmRouter requires a router_provider".into())
                })?;

                let prompt = format!(
                    "{}\n\nAvailable agents: {}\n\nUser message: {}\n\nRespond with ONLY the agent name.",
                    router_prompt,
                    agent_names.join(", "),
                    user_input,
                );

                let messages = vec![Message::user(&prompt)];
                let resp = provider
                    .chat_completion(&messages, &[])
                    .await
                    .map_err(|e| OrchestratorError::Routing(e.to_string()))?;

                let chosen = resp.content.trim().to_string();
                if agent_names.contains(&chosen) {
                    Ok(vec![chosen])
                } else {
                    // Fuzzy match.
                    let lower = chosen.to_lowercase();
                    if let Some(name) = agent_names.iter().find(|n| n.to_lowercase() == lower) {
                        Ok(vec![name.clone()])
                    } else {
                        debug!(
                            "Router returned unknown agent '{}', defaulting to first",
                            chosen
                        );
                        Ok(vec![agent_names.first().cloned().unwrap_or_default()])
                    }
                }
            }

            RoutingStrategy::Pipeline(order) => Ok(order.clone()),

            RoutingStrategy::Parallel => Ok(agent_names.to_vec()),

            RoutingStrategy::Custom(f) => Ok(f(user_input, agent_names)),
        }
    }

    async fn run_single(
        &self,
        conversation_id: &str,
        user_input: &str,
        agent_name: &str,
    ) -> Result<String, OrchestratorError> {
        let agent = self
            .agents
            .get(agent_name)
            .ok_or_else(|| OrchestratorError::AgentNotFound(agent_name.to_string()))?;

        agent
            .run(conversation_id, user_input, self.event_tx.as_ref())
            .await
            .map_err(OrchestratorError::Agent)
    }

    async fn run_pipeline(
        &self,
        conversation_id: &str,
        user_input: &str,
        order: &[String],
    ) -> Result<String, OrchestratorError> {
        let mut current_input = user_input.to_string();

        for (i, agent_name) in order.iter().enumerate() {
            info!(
                "Pipeline step {}/{}: agent '{}'",
                i + 1,
                order.len(),
                agent_name
            );

            let agent = self
                .agents
                .get(agent_name)
                .ok_or_else(|| OrchestratorError::AgentNotFound(agent_name.to_string()))?;

            let conv_id = format!("{}_pipeline_{}", conversation_id, agent_name);
            current_input = agent
                .run(&conv_id, &current_input, self.event_tx.as_ref())
                .await
                .map_err(OrchestratorError::Agent)?;
        }

        Ok(current_input)
    }

    async fn run_parallel(
        &self,
        conversation_id: &str,
        user_input: &str,
        targets: &[String],
    ) -> Result<String, OrchestratorError> {
        let mut handles = vec![];

        for agent_name in targets {
            let agent = self
                .agents
                .get(agent_name)
                .ok_or_else(|| OrchestratorError::AgentNotFound(agent_name.to_string()))?;

            // We need to move data into the spawned task.
            let conv_id = format!("{}_parallel_{}", conversation_id, agent_name);
            let input = user_input.to_string();
            let name = agent_name.clone();

            // Since Agent isn't Clone, we run sequentially for now.
            // In production, you'd use Arc<Agent>.
            let result = agent.run(&conv_id, &input, self.event_tx.as_ref()).await;
            handles.push((name, result));
        }

        // Merge results.
        let mut merged = String::new();
        for (name, result) in handles {
            match result {
                Ok(content) => {
                    merged.push_str(&format!("--- {} ---\n{}\n\n", name, content));
                }
                Err(e) => {
                    error!("Agent '{}' failed: {}", name, e);
                    merged.push_str(&format!("--- {} ---\n[Error: {}]\n\n", name, e));
                }
            }
        }

        Ok(merged)
    }
}

fn strategy_name(s: &RoutingStrategy) -> &'static str {
    match s {
        RoutingStrategy::Fixed(_) => "Fixed",
        RoutingStrategy::KeywordBased(_) => "KeywordBased",
        RoutingStrategy::LlmRouter { .. } => "LlmRouter",
        RoutingStrategy::Pipeline(_) => "Pipeline",
        RoutingStrategy::Parallel => "Parallel",
        RoutingStrategy::Custom(_) => "Custom",
    }
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

pub struct OrchestratorBuilder {
    agents: HashMap<String, Agent>,
    strategy: RoutingStrategy,
    router_provider: Option<Arc<dyn LlmProvider>>,
    event_tx: Option<mpsc::Sender<AgentEvent>>,
}

impl Default for OrchestratorBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl OrchestratorBuilder {
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
            strategy: RoutingStrategy::Fixed("default".into()),
            router_provider: None,
            event_tx: None,
        }
    }

    /// Add an agent to the orchestrator.
    pub fn add_agent(mut self, agent: Agent) -> Self {
        self.agents.insert(agent.config.name.clone(), agent);
        self
    }

    /// Set the routing strategy.
    pub fn strategy(mut self, strategy: RoutingStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Set the LLM provider used for LlmRouter strategy.
    pub fn router_provider(mut self, provider: Arc<dyn LlmProvider>) -> Self {
        self.router_provider = Some(provider);
        self
    }

    /// Set the event channel for receiving agent events.
    pub fn event_channel(mut self, tx: mpsc::Sender<AgentEvent>) -> Self {
        self.event_tx = Some(tx);
        self
    }

    pub fn build(self) -> Orchestrator {
        Orchestrator {
            agents: self.agents,
            strategy: self.strategy,
            router_provider: self.router_provider,
            event_tx: self.event_tx,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OrchestratorError {
    #[error("Agent error: {0}")]
    Agent(#[from] AgentError),
    #[error("Agent not found: {0}")]
    AgentNotFound(String),
    #[error("No agent selected by routing")]
    NoAgentSelected,
    #[error("Routing error: {0}")]
    Routing(String),
    #[error("Configuration error: {0}")]
    Config(String),
}
