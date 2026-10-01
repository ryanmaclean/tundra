//! Harness — provider abstractions, MCP tool execution, and reliability
//! infrastructure for the auto-tundra agent runtime.
//!
//! This crate provides the foundational execution layer that sits between
//! the agent orchestration logic and external LLM/tool providers. It coordinates:
//! - Provider abstraction for LLM API calls (messages, completions, streaming)
//! - MCP (Model Context Protocol) tool definitions and execution
//! - Reliability patterns (circuit breaker, rate limiter) for external calls
//! - Security primitives for sandboxing and validation
//! - Outbound screening of agent output for leaked secrets and prompt injection
//!   ([`output_guard`]) and a hash-chained, tamper-evident audit log ([`audit_chain`])
//! - Operational concerns (shutdown coordination, distributed tracing context)
//! - Built-in "Tundra Tools" for agent self-management (run_task, get_build_status, etc.)

pub mod audit_chain;
pub mod builtin_tools;
pub mod circuit_breaker;
pub mod mcp;
pub mod output_guard;
pub mod provider;
pub mod rate_limiter;
pub mod security;
pub mod shutdown;
pub mod trace_ctx;
