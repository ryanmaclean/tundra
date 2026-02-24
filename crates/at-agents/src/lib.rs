//! Agent execution, orchestration, and lifecycle management for auto-tundra.
//!
//! This crate provides the agent layer that coordinates Claude AI agents,
//! managing their execution, state, and task progression. It includes:
//! - Task orchestration and execution engines
//! - Agent lifecycle management and state machines
//! - Session and profile management
//! - Approval workflows and supervision
//! - Prompt registries and role definitions

pub mod approval;
pub mod claude_runtime;
pub mod claude_session;
pub mod executor;
pub mod lifecycle;
pub mod orchestrator;
pub mod profiles;
pub mod prompts;
pub mod registry;
pub mod roles;
pub mod state_machine;
pub mod supervisor;
pub mod task_orchestrator;
pub mod task_runner;
