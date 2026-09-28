//! MCP task plane: a thin re-export of the canonical task store.
//!
//! ARCH-001 moved the task domain's authoritative representation to
//! [`crate::core::tasks`]; this module only re-exports those types so
//! the MCP tool registration and dispatcher keep their import paths. It
//! deliberately owns no persistence: JSON-RPC translation lives in the
//! dispatcher, state lives in the core store.

pub use crate::core::tasks::{Task, TaskPriority, TaskStatus, TaskStore as TasksMcp};
