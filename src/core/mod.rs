//! Core project-state persistence: context, files, memory, projects, tasks,
//! and the workspace root.

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
