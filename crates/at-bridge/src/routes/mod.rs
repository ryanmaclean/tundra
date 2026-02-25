//! API route organization by domain.
//!
//! This module organizes API routes into domain-specific sub-routers
//! using Axum's Router::nest() and Router::merge() patterns.
//!
//! Each domain router is defined in its own file and exposes a public
//! function that returns a Router<Arc<ApiState>>.

pub mod github;
pub mod kanban;
pub mod notifications;
pub mod projects;
pub mod queue;
pub mod settings;
pub mod tasks;
pub mod terminals;
pub mod worktrees;
