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
/// Typed runtime identity contracts (TW-001).
pub mod identity;
/// Memory persistence.
pub mod memory;
/// Workspace policy rule persistence.
pub mod policy;
/// Project persistence.
pub mod project;
/// Runtime session persistence (TW-002).
pub mod sessions;
/// Task persistence.
pub mod tasks;
/// Workspace root resolution.
pub mod workspace;
