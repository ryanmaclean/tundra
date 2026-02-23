//! Terminal session management and PTY pooling for auto-tundra agents.
//!
//! This crate provides persistent terminal sessions with PTY (pseudo-terminal)
//! pooling, CLI adaptation, and state persistence. It enables agents to
//! maintain long-running shell environments across task executions, preserving
//! working directories, environment variables, and command history.
//!
//! Key components:
//! - Session management with state tracking
//! - PTY pool for efficient terminal allocation
//! - CLI adapter for bridging agent commands to shell execution
//! - Terminal persistence for state recovery across restarts

pub mod cli_adapter;
pub mod pty_pool;
pub mod session;
pub mod terminal_persistence;
