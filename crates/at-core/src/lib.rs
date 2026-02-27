//! Core library for auto-tundra — provides foundational types, context management,
//! repository operations, and RLM patterns.
//!
//! This crate is the heart of the auto-tundra system and provides:
//! - Context assembly and steering for AI agent execution
//! - Repository and worktree management primitives
//! - RLM (Recursive Language Model) decomposition and refinement
//! - Session and cache management
//! - Configuration and settings infrastructure
//! - File watching and change detection

pub mod cache;
pub mod config;
pub mod context_engine;
pub mod context_steering;
pub mod crypto;
pub mod file_watcher;
pub mod git_read_adapter;
pub mod lockfile;
pub mod repo;
pub mod rlm;
pub mod session_store;
pub mod settings;
pub mod types;
pub mod worktree;
pub mod worktree_manager;

#[cfg(feature = "libgit2")]
pub mod git2_ops;
