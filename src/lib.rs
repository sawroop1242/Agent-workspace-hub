//! Security-first workspace runtime for AI agents.
//!
//! This crate provides persisted project state (context, memory, tasks,
//! files), an MCP tool server with permission and sandbox enforcement, and
//! skill installation/validation, all scoped under a per-project workspace
//! directory.

pub mod core;
pub mod mcp;
pub mod models;
pub mod skills;
