pub mod api_error;
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

// Re-export ApiError for convenience
pub use api_error::ApiError;
