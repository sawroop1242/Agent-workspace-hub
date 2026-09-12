//! Core project-state persistence: agents, capability grants, context, files,
//! memory, projects, tasks, and the workspace root.

/// Agent identity persistence.
pub mod agents;
/// Capability grant persistence.
pub mod capability_grants;
/// Workspace context assembly.
pub mod context;
/// File helpers.
pub mod files;
/// Memory persistence.
pub mod memory;
/// Project persistence.
pub mod project;
/// Task persistence.
pub mod tasks;
/// Workspace root resolution.
pub mod workspace;
