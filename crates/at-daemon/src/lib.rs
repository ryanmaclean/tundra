//! Background daemon for the auto-tundra/auto-claude agent system.
//!
//! The daemon provides persistent background services including:
//! - Scheduled task execution and patrol loops
//! - System health monitoring and heartbeat tracking
//! - KPI collection and reporting
//! - Long-running orchestration workflows

pub mod daemon;
pub mod heartbeat;
pub mod kpi;
pub mod orchestrator;
pub mod patrol;
pub mod scheduler;
