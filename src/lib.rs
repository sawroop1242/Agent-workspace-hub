//! Security-first workspace runtime for AI agents.
//!
//! This crate provides persisted project state (context, memory, tasks,
//! files), an MCP tool server with permission and sandbox enforcement, and
//! skill installation/validation, all scoped under a per-project workspace
//! directory. The Context Engine (see [`context`]) adds proactive context
//! management — planning, structured long-term memory, soft offloading, and
//! token budgets — on top of the same per-project state layout.

pub mod api;
pub mod context;
pub mod core;
pub mod mcp;
pub mod models;
pub mod services;
pub mod skills;
pub mod tui;
pub mod tunnel;
