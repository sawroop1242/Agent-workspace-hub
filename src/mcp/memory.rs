//! MCP memory plane: a thin re-export of the canonical memory store.
//!
//! ARCH-001 moved the memory domain's authoritative representation to
//! [`crate::core::memory`]; this module only re-exports those types so
//! the MCP tool registration and dispatcher keep their import paths. It
//! deliberately owns no persistence: JSON-RPC translation lives in the
//! dispatcher, state lives in the core store.

pub use crate::core::memory::{MemoryEntry, MemoryScope, MemoryStore as MemoryMcp};
