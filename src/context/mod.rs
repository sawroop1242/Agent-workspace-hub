//! Context Engine: proactive context management for the workspace runtime.
//!
//! This subsystem is an independent Rust implementation of the runtime
//! architecture popularized by Tencent's ContextPilot (planning, structured
//! long-term memory, soft context offloading, and explicit context budgets).
//! It is *architecturally aligned* with ContextPilot but shares no source
//! with it, and it deliberately omits the Python training stack (verl, vLLM,
//! Elasticsearch) — normal operation requires none of those.
//!
//! The engine keeps four kinds of information strictly separate:
//!
//! 1. **Active context** — information currently intended for the LLM.
//! 2. **Long-term memory** — persistent knowledge (via the existing
//!    [`crate::mcp::MemoryMcp`] store, never a second memory system).
//! 3. **Offloaded context** — inactive but fully recoverable item content.
//! 4. **Source files** — the user's real files, which the engine only reads
//!    and never rewrites.
//!
//! All engine state lives under the project's canonical `.agent/` state
//! directory (`.agent/context-engine/`), so projects remain isolated from
//! each other exactly like the existing memory/task stores.

/// Explicit token budget shared by the engine, selector, and policy.
pub mod budget;
/// Lossless context compression (dedup, structural extraction, truncation).
pub mod compressor;
/// The context manager: the single entry point over all engine operations.
pub mod engine;
/// The context item data model.
pub mod item;
/// Soft offloading: durable, restorable inactive context storage.
pub mod offload;
/// Deterministic planning over workspace, skills, memory, and offloads.
pub mod planner;
/// Replaceable decision policy (deterministic today, learned tomorrow).
pub mod policy;
/// Deterministic, documented relevance scoring.
pub mod scoring;
/// Budget-constrained context selection.
pub mod selector;
/// Context snapshots for inspection and restore.
pub mod snapshot;
/// Documented approximate token counting.
pub mod tokens;

pub use budget::{ContextBudget, ContextBudgetStatus};
pub use compressor::{CompressOptions, CompressedContent, ContextCompressor};
pub use engine::{ContextEngine, ContextEngineConfig, ContextRequest, ContextStatus, PolicyType};
pub use item::{ContextItem, ContextScope, ContextSource, ContextState};
pub use offload::OffloadStore;
pub use planner::{ContextPlanner, DeterministicPlanner, PlanHint};
pub use policy::{ContextAction, ContextDecision, ContextPolicy, DeterministicContextPolicy};
pub use scoring::{score_item, task_alignment, ScoreWeights};
pub use selector::{select_within_budget, SelectionOutcome};
pub use snapshot::{ContextSnapshot, SnapshotStore};
pub use tokens::{ApproxTokenCounter, TokenCounter};
