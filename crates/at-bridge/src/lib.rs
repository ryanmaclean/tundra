//! Bridge layer connecting auto-tundra core to external interfaces.
//!
//! This crate provides the transport and integration layer for auto-tundra,
//! exposing the core agent system through multiple channels:
//! - HTTP API server with authentication
//! - WebSocket terminal connections
//! - IPC command registry and protocol
//! - Event bus for system-wide notifications
//! - Intelligence API client for LLM integration
//!
//! Key modules:
//! - [`http_api`] — Axum-based REST API
//! - [`terminal_ws`] — WebSocket terminal multiplexing
//! - [`ipc`] — Inter-process communication
//! - [`auth`] — API key authentication middleware
//! - [`event_bus`] — Pub/sub event system

pub mod auth;
pub mod command_registry;
pub mod commands;
pub mod event_bus;
pub mod http_api;
pub mod intelligence_api;
pub mod ipc;
pub mod notifications;
pub mod protocol;
pub mod terminal;
pub mod terminal_ws;
pub mod transport;
