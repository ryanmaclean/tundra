//! API route organization by domain.
//!
//! ## Router organization
//!
//! This module organizes API routes into domain-specific sub-routers
//! using Axum's Router::nest() and Router::merge() patterns. Each domain
//! (github, kanban, projects, tasks, etc.) is defined in its own module
//! and exposes a public function that returns a Router<Arc<ApiState>>.
//!
//! The domain routers are merged into the main application router in
//! `http_api.rs`, providing a clean separation of concerns and making
//! it easy to locate and maintain related endpoints.

pub mod github;
pub mod kanban;
pub mod misc;
pub mod notifications;
pub mod projects;
pub mod queue;
pub mod settings;
pub mod tasks;
pub mod terminals;
pub mod worktrees;
