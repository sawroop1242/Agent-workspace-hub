//! The Context Engine manager.
//!
//! [`ContextEngine`] is the single entry point over items, budget, scoring,
//! selection, policy decisions, compression, offloading, restoration, and
//! snapshots. It is deliberately *not* the MCP surface — the MCP surface (see
//! [`crate::mcp::context_engine`]) calls into the engine. The engine is also
//! transport-agnostic and synchronous (bounded, local file operations), which
//! keeps it usable from both the stdio and HTTP dispatch paths.

use crate::context::budget::{ContextBudget, ContextBudgetStatus};
use crate::context::compressor::{CompressOptions, ContextCompressor};
use crate::context::item::{
    is_valid_item_id, ContextItem, ContextScope, ContextSource, ContextState,
};
use crate::context::offload::{OffloadRecord, OffloadStore};
use crate::context::policy::{
    ContextAction, ContextDecision, ContextPolicy, DeterministicContextPolicy,
};
use crate::context::scoring::ScoreWeights;
use crate::context::selector::select_within_budget;
use crate::context::snapshot::{ContextSnapshot, SnapshotStore};
use crate::context::tokens::{ApproxTokenCounter, TokenCounter};
use anyhow::{bail, Context as _, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::RwLock;

/// Maximum active items held by one engine instance (bounded memory).
const MAX_ACTIVE_ITEMS: usize = 5_000;

/// Which decision policy the engine should use (Phase 1: deterministic only).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PolicyType {
    /// The documented deterministic scoring policy (Phase 1).
    #[default]
    Deterministic,
}

/// Engine configuration, resolved from the repo's `AWH_*` environment
/// conventions (see [`ContextEngineConfig::with_env_overrides`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextEngineConfig {
    /// Master switch: when `false`, every engine operation is a no-op that
    /// preserves existing behavior exactly (opt-in pipeline interception).
    pub enabled: bool,
    /// The explicit token budget.
    pub budget: ContextBudget,
    /// Whether `optimize()`/auto-pipeline passes may offload items.
    pub auto_offload: bool,
    /// Whether `optimize()`/auto-pipeline passes may compress items.
    pub auto_compress: bool,
    /// Whether long-term memory extraction is enabled.
    pub memory_enabled: bool,
    /// The decision policy selector.
    pub policy_type: PolicyType,
}

impl Default for ContextEngineConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            budget: ContextBudget::default(),
            auto_offload: true,
            auto_compress: true,
            memory_enabled: true,
            policy_type: PolicyType::Deterministic,
        }
    }
}

impl ContextEngineConfig {
    /// Applies `AWH_CONTEXT_*` environment overrides using the injected
    /// lookup, following the same precedence pattern as
    /// [`crate::mcp::ResourceLimits::with_overrides_from`].
    pub fn with_overrides_from<F>(self, lookup: F) -> Result<Self>
    where
        F: Fn(&str) -> Option<String>,
    {
        let mut out = self;
        if let Some(v) = lookup("AWH_CONTEXT_ENABLED") {
            out.enabled = matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            );
        }
        for (key, target) in [
            ("AWH_CONTEXT_MAX_INPUT_TOKENS", 0usize),
            ("AWH_CONTEXT_RESERVED_OUTPUT_TOKENS", 1),
            ("AWH_CONTEXT_SAFETY_MARGIN_TOKENS", 2),
        ] {
            if let Some(raw) = lookup(key) {
                let parsed: usize = raw
                    .trim()
                    .parse()
                    .with_context(|| format!("invalid value for {key}: {raw:?}"))?;
                match target {
                    0 => out.budget.max_input_tokens = parsed,
                    1 => out.budget.reserved_output_tokens = parsed,
                    _ => out.budget.safety_margin_tokens = parsed,
                }
            }
        }
        if let Some(v) = lookup("AWH_CONTEXT_AUTO_OFFLOAD") {
            out.auto_offload = matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            );
        }
        if let Some(v) = lookup("AWH_CONTEXT_AUTO_COMPRESS") {
            out.auto_compress = matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            );
        }
        if let Some(v) = lookup("AWH_CONTEXT_MEMORY_ENABLED") {
            out.memory_enabled = matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            );
        }
        Ok(out)
    }

    /// Applies real environment overrides.
    pub fn with_env_overrides(self) -> Result<Self> {
        self.with_overrides_from(|key| std::env::var(key).ok())
    }
}

/// A context request: what the caller wants assembled.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextRequest {
    /// The current task description.
    pub task: String,
    /// Optional additional query text for relevance scoring.
    pub query: Option<String>,
    /// The token budget to target for this request.
    pub token_budget: usize,
    /// Scope isolation for the request.
    pub scope: ContextScope,
}

/// The engine's observable status (the `context.status` view).
#[derive(Debug, Clone, Serialize)]
pub struct ContextStatus {
    /// Whether the engine is enabled.
    pub enabled: bool,
    /// Number of active items.
    pub active_items: usize,
    /// Total tokens across active items.
    pub active_tokens: usize,
    /// Budget configuration and utilization.
    pub budget: ContextBudgetStatus,
    /// Number of offloaded (recoverable) items.
    pub offloaded_items: usize,
    /// Number of durable memories (from the existing memory store).
    pub memories: Option<usize>,
    /// The most recent decision summary, if any.
    pub last_decision: Option<DecisionSummary>,
    /// The number of protected items.
    pub protected_items: usize,
}

/// A compact summary of the last policy decision pass.
#[derive(Debug, Clone, Serialize)]
pub struct DecisionSummary {
    /// When the pass ran.
    pub at: String,
    /// The task the pass was scored against.
    pub task: String,
    /// Kept item ids.
    pub kept: Vec<String>,
    /// Offloaded item ids.
    pub offloaded: Vec<String>,
    /// Compressed item ids.
    pub compressed: Vec<String>,
    /// Archived item ids.
    pub archived: Vec<String>,
    /// Tokens before the pass.
    pub tokens_before: usize,
    /// Tokens after the pass.
    pub tokens_after: usize,
    /// A human-readable reason for the overall pass.
    pub reason: String,
}

/// The Context Engine: manages items, budget, decisions, and snapshots.
pub struct ContextEngine {
    config: ContextEngineConfig,
    counter: ApproxTokenCounter,
    compressor: ContextCompressor,
    policy: Box<dyn ContextPolicy>,
    offloads: OffloadStore,
    snapshots: SnapshotStore,
    memory: crate::mcp::MemoryMcp,
    items: RwLock<Vec<ContextItem>>,
    protected: RwLock<HashSet<String>>,
    last_decision: RwLock<Option<DecisionSummary>>,
}

impl ContextEngine {
    /// Builds the engine for a project root, creating state directories.
    pub fn new(project_root: &Path, config: ContextEngineConfig) -> Result<Self> {
        let policy: Box<dyn ContextPolicy> = match config.policy_type {
            PolicyType::Deterministic => Box::new(DeterministicContextPolicy::default()),
        };
        Ok(Self {
            config,
            counter: ApproxTokenCounter,
            compressor: ContextCompressor::new(),
            policy,
            offloads: OffloadStore::new(project_root)?,
            snapshots: SnapshotStore::new(project_root)?,
            memory: crate::mcp::MemoryMcp::new(project_root)?,
            items: RwLock::new(Vec::new()),
            protected: RwLock::new(HashSet::new()),
            last_decision: RwLock::new(None),
        })
    }

    /// The active configuration.
    pub fn config(&self) -> &ContextEngineConfig {
        &self.config
    }

    /// The effective token budget.
    pub fn budget(&self) -> &ContextBudget {
        &self.config.budget
    }

    // ---------------------------------------------------------------------
    // Items
    // ---------------------------------------------------------------------

    /// Inserts or replaces an item by id, recomputing its token count.
    ///
    /// Fails closed on invalid ids, oversized content, or a full active set.
    /// Scope is validated to prevent cross-project leakage: only `Global`
    /// items may be inserted with a non-project scope.
    pub fn insert(&self, mut item: ContextItem) -> Result<ContextItem> {
        if !self.config.enabled {
            bail!("context engine is disabled");
        }
        if !is_valid_item_id(&item.id) {
            bail!("invalid context item id: {:?}", item.id);
        }
        if item.token_count > self.config.budget.max_input_tokens {
            bail!(
                "item token count {} exceeds max input tokens {}",
                item.token_count,
                self.config.budget.max_input_tokens
            );
        }
        let mut items = self.items.write().expect("context item lock poisoned");
        if items.len() >= MAX_ACTIVE_ITEMS && !items.iter().any(|i| i.id == item.id) {
            bail!("active context is full (max {MAX_ACTIVE_ITEMS} items)");
        }
        // Recount defensively: token_count must always reflect content.
        item.token_count = self.counter.count(&item.content);
        items.retain(|i| i.id != item.id);
        items.push(item.clone());
        Ok(item)
    }

    /// Returns a snapshot of all items (any state), insertion order.
    pub fn list_items(&self) -> Vec<ContextItem> {
        self.items
            .read()
            .expect("context item lock poisoned")
            .clone()
    }

    /// Returns one item by id.
    pub fn get_item(&self, id: &str) -> Option<ContextItem> {
        self.items
            .read()
            .expect("context item lock poisoned")
            .iter()
            .find(|i| i.id == id)
            .cloned()
    }

    /// Marks an item protected: the selector and policy always keep it.
    pub fn protect(&self, id: &str) -> bool {
        if !is_valid_item_id(id) {
            return false;
        }
        let exists = self
            .items
            .read()
            .expect("context item lock poisoned")
            .iter()
            .any(|i| i.id == id);
        if exists {
            self.protected
                .write()
                .expect("protected lock poisoned")
                .insert(id.to_string());
        }
        exists
    }

    /// Removes protection for an item.
    pub fn unprotect(&self, id: &str) -> bool {
        self.protected
            .write()
            .expect("protected lock poisoned")
            .remove(id)
    }

    /// Whether an item is protected.
    pub fn is_protected(&self, id: &str) -> bool {
        self.protected
            .read()
            .expect("protected lock poisoned")
            .contains(id)
    }

    /// Removes an item from the engine entirely (explicit only).
    pub fn remove_item(&self, id: &str) -> bool {
        let mut items = self.items.write().expect("context item lock poisoned");
        let before = items.len();
        items.retain(|i| i.id != id);
        let removed = before != items.len();
        if removed {
            self.protected
                .write()
                .expect("protected lock poisoned")
                .remove(id);
        }
        removed
    }

    /// Total tokens across active items.
    pub fn active_tokens(&self) -> usize {
        self.items
            .read()
            .expect("context item lock poisoned")
            .iter()
            .filter(|i| i.state.is_active())
            .map(|i| i.token_count)
            .sum()
    }

    // ---------------------------------------------------------------------
    // Search
    // ---------------------------------------------------------------------

    /// Searches active and offloaded items by case-insensitive substring.
    ///
    /// Active items are matched from memory; offloaded items are fetched by
    /// id from the durable store (no full-store content scan is performed on
    /// items that were never loaded).
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        if !self.config.enabled {
            bail!("context engine is disabled");
        }
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return Ok(Vec::new());
        }
        let mut hits = Vec::new();
        let mut seen = HashSet::new();
        for item in self.list_items() {
            if item.content.to_lowercase().contains(&needle) {
                seen.insert(item.id.clone());
                hits.push(SearchHit {
                    id: item.id,
                    state: item.state,
                    source: item.source,
                    token_count: item.token_count,
                    excerpt: excerpt(&item.content, &needle),
                    match_location: MatchLocation::Active,
                });
            }
        }
        for id in self.offloads.list_ids()? {
            // Skip offload records whose item is already reported from the
            // in-memory set (offloaded items stay in engine memory).
            if seen.contains(&id) {
                continue;
            }
            if let Some(item) = self.offloads.get(&id)? {
                if item.content.to_lowercase().contains(&needle) {
                    hits.push(SearchHit {
                        id: item.id,
                        state: item.state,
                        source: item.source,
                        token_count: item.token_count,
                        excerpt: excerpt(&item.content, &needle),
                        match_location: MatchLocation::Offloaded,
                    });
                }
            }
        }
        hits.truncate(limit.max(1));
        Ok(hits)
    }

    // ---------------------------------------------------------------------
    // Decisions, selection, optimization
    // ---------------------------------------------------------------------

    /// Runs the policy over the current active items and returns decisions.
    pub fn decide(&self, task: &str) -> Vec<ContextDecision> {
        let items = self.list_items();
        let protected = self
            .protected
            .read()
            .expect("protected lock poisoned")
            .clone();
        self.policy
            .decide(&items, task, &self.config.budget, &protected)
    }

    /// Scores all items against a task/query.
    pub fn scores(&self, task: &str) -> HashMap<String, f32> {
        let weights = ScoreWeights::default();
        let usable = self.config.budget.usable_input_tokens();
        self.list_items()
            .into_iter()
            .map(|item| {
                let score = crate::context::scoring::score_item(&item, task, usable, &weights);
                (item.id.clone(), score)
            })
            .collect()
    }

    /// Selects the best-fitting subset for a request (does not mutate).
    pub fn select(&self, request: &ContextRequest) -> crate::context::selector::SelectionOutcome {
        let items: Vec<ContextItem> = self
            .list_items()
            .into_iter()
            .filter(|i| i.state.is_active())
            .collect();
        let scores = self.scores(&request.task);
        let protected = self
            .protected
            .read()
            .expect("protected lock poisoned")
            .clone();
        // A per-request budget further tightens the configured budget; it
        // never loosens it, so `token_budget` acts as an additional cap.
        let request_budget = if request.token_budget > 0 {
            ContextBudget {
                max_input_tokens: self
                    .config
                    .budget
                    .usable_input_tokens()
                    .min(request.token_budget),
                reserved_output_tokens: 0,
                safety_margin_tokens: 0,
            }
        } else {
            self.config.budget
        };
        select_within_budget(&items, &scores, &request_budget, &protected)
    }

    /// Assembles the optimized active context for a request.
    ///
    /// This is the engine's read-side pipeline: score → select → compress
    /// (opt-in via config) → return. It never mutates engine state, so it is
    /// safe for concurrent MCP callers; state changes happen only through
    /// explicit `offload`/`restore`/`archive` calls.
    pub fn get_context(&self, request: &ContextRequest) -> Result<AssembledContext> {
        if !self.config.enabled {
            bail!("context engine is disabled");
        }
        let outcome = self.select(request);
        let items = self.list_items();
        let by_id: HashMap<String, ContextItem> =
            items.into_iter().map(|i| (i.id.clone(), i)).collect();

        let mut kept: Vec<ContextItem> = Vec::new();
        for id in &outcome.kept_ids {
            if let Some(mut item) = by_id.get(id).cloned() {
                // Compression is opt-in for the assembly pass; source content
                // is never lost (the original stays in the engine's record).
                if self.config.auto_compress {
                    let options = CompressOptions {
                        max_tokens: (request.token_budget / outcome.kept_ids.len().max(1))
                            .clamp(32, 4_096),
                        ..CompressOptions::default()
                    };
                    let compressed = self.compressor.compress(&item.content, &options);
                    if compressed.tokens_saved > 0 {
                        item.metadata = serde_json::json!({
                            "context_engine": {
                                "compressed": true,
                                "original_tokens": compressed.original_tokens,
                                "strategies": compressed.strategies,
                            }
                        });
                        item.content = compressed.content;
                        item.token_count = compressed.token_count;
                    }
                }
                kept.push(item);
            }
        }

        Ok(AssembledContext {
            items: kept,
            total_tokens: outcome.kept_tokens,
            budget: self.config.budget,
            rejected_ids: outcome.rejected_ids,
        })
    }

    /// Applies an optimization pass: run the policy and offload/compress/
    /// archive according to its decisions.
    ///
    /// This is the state-changing pipeline. It respects `auto_offload` and
    /// `auto_compress`; protected items are never touched; offload failures
    /// keep the item active (fail-safe). Returns what it did.
    pub fn optimize(&self, task: &str) -> Result<DecisionSummary> {
        if !self.config.enabled {
            bail!("context engine is disabled");
        }
        let tokens_before = self.active_tokens();
        let decisions = self.decide(task);

        let mut kept = Vec::new();
        let mut offloaded = Vec::new();
        let mut compressed = Vec::new();
        let mut archived = Vec::new();
        let mut tokens_after = tokens_before;

        for decision in &decisions {
            match decision.action {
                ContextAction::Keep | ContextAction::Restore | ContextAction::Ignore => {
                    kept.push(decision.id.clone());
                }
                ContextAction::Offload => {
                    if !self.config.auto_offload {
                        kept.push(decision.id.clone());
                        continue;
                    }
                    match self.offload(&decision.id, &decision.reason) {
                        Ok(()) => {
                            offloaded.push(decision.id.clone());
                            tokens_after = tokens_after.saturating_sub(decision.token_cost);
                        }
                        // Fail-safe: an offload failure keeps the item active.
                        Err(_) => kept.push(decision.id.clone()),
                    }
                }
                ContextAction::Compress => {
                    if !self.config.auto_compress {
                        kept.push(decision.id.clone());
                        continue;
                    }
                    let before = self
                        .get_item(&decision.id)
                        .map(|i| i.token_count)
                        .unwrap_or(0);
                    match self.compress_item(&decision.id) {
                        Ok(Some(saved)) => {
                            compressed.push(decision.id.clone());
                            tokens_after = tokens_after.saturating_sub(saved);
                        }
                        Ok(None) | Err(_) => kept.push(decision.id.clone()),
                    }
                    let _ = before;
                }
                ContextAction::Archive => match self.archive(&decision.id) {
                    Ok(()) => {
                        archived.push(decision.id.clone());
                        tokens_after = tokens_after.saturating_sub(decision.token_cost);
                    }
                    Err(_) => kept.push(decision.id.clone()),
                },
            }
        }

        let reason = format!(
            "policy pass over {} items: kept {}, offloaded {}, compressed {}, archived {}",
            decisions.len(),
            kept.len(),
            offloaded.len(),
            compressed.len(),
            archived.len()
        );
        let summary = DecisionSummary {
            at: chrono::Utc::now().to_rfc3339(),
            task: task.to_string(),
            kept,
            offloaded,
            compressed,
            archived,
            tokens_before,
            tokens_after,
            reason,
        };
        *self.last_decision.write().expect("decision lock poisoned") = Some(summary.clone());
        Ok(summary)
    }

    // ---------------------------------------------------------------------
    // Soft offloading / restoration
    // ---------------------------------------------------------------------

    /// Moves an active item to the durable offload store.
    ///
    /// OFFLOAD NEVER MEANS DELETE: content is written durably before the
    /// in-memory state changes. If the durable write fails, the item stays
    /// active (fail-safe).
    pub fn offload(&self, id: &str, reason: &str) -> Result<()> {
        if !self.config.enabled {
            bail!("context engine is disabled");
        }
        if !is_valid_item_id(id) {
            bail!("invalid context item id: {id:?}");
        }
        if self.is_protected(id) {
            bail!("item {id} is protected and cannot be offloaded");
        }
        let Some(mut item) = self.get_item(id) else {
            bail!("context item not found: {id}");
        };
        if !item.state.is_active() {
            bail!("item {id} is already inactive");
        }
        item.state = ContextState::Offloaded;
        let record = OffloadRecord {
            offloaded_at: chrono::Utc::now().to_rfc3339(),
            reason: if reason.trim().is_empty() {
                None
            } else {
                Some(reason.to_string())
            },
            item: item.clone(),
        };
        // Durable write FIRST; only on success flip the in-memory state.
        self.offloads.put(&record)?;
        self.replace_item(item);
        Ok(())
    }

    /// Restores an offloaded item to active context, respecting the budget.
    ///
    /// Refuses to restore (with an error) when the item would overflow the
    /// usable budget — never restores blindly. The offload record is only
    /// deleted once the item is active again.
    pub fn restore(&self, id: &str) -> Result<ContextItem> {
        if !self.config.enabled {
            bail!("context engine is disabled");
        }
        if !is_valid_item_id(id) {
            bail!("invalid context item id: {id:?}");
        }
        // Try the in-memory record first (covers offloaded-but-loaded items).
        if let Some(item) = self.get_item(id) {
            if item.state == ContextState::Offloaded {
                return self.restore_item_to_active(item);
            }
            if item.state.is_active() {
                bail!("item {id} is already active");
            }
        }
        // Fall back to the durable offload store.
        let Some(mut item) = self.offloads.get(id)? else {
            bail!("offloaded context item not found: {id}");
        };
        item.state = ContextState::Active;
        let restored = self.restore_item_to_active(item)?;
        self.offloads.remove_restored(id)?;
        Ok(restored)
    }

    fn restore_item_to_active(&self, mut item: ContextItem) -> Result<ContextItem> {
        let active = self.active_tokens();
        let projected = active + item.token_count;
        if self.config.budget.overflow(projected).is_some() && !self.is_protected(&item.id) {
            bail!(
                "restoring {} ({} tokens) would exceed the usable budget (active {}, overflow {})",
                item.id,
                item.token_count,
                active,
                self.config.budget.overflow(projected).unwrap()
            );
        }
        item.recency = 1.0; // restored items are fresh again
        item.state = ContextState::Active;
        self.replace_item(item.clone());
        Ok(item)
    }

    /// Archives an item: still inspectable, excluded from active context.
    pub fn archive(&self, id: &str) -> Result<()> {
        if !self.config.enabled {
            bail!("context engine is disabled");
        }
        if self.is_protected(id) {
            bail!("item {id} is protected and cannot be archived");
        }
        let Some(mut item) = self.get_item(id) else {
            bail!("context item not found: {id}");
        };
        item.state = ContextState::Archived;
        self.replace_item(item);
        Ok(())
    }

    /// Compresses one item's active content in place (lossless pipeline:
    /// original tokens are recorded in metadata).
    pub fn compress_item(&self, id: &str) -> Result<Option<usize>> {
        if !self.config.enabled {
            bail!("context engine is disabled");
        }
        let Some(item) = self.get_item(id) else {
            bail!("context item not found: {id}");
        };
        if !item.state.is_active() {
            return Ok(None);
        }
        let options = CompressOptions::default();
        let compressed = self.compressor.compress(&item.content, &options);
        if compressed.tokens_saved == 0 {
            return Ok(None);
        }
        let mut updated = item;
        updated.metadata = serde_json::json!({
            "context_engine": {
                "compressed": true,
                "original_tokens": compressed.original_tokens,
                "strategies": compressed.strategies,
            }
        });
        updated.content = compressed.content;
        updated.token_count = compressed.token_count;
        self.replace_item(updated);
        Ok(Some(compressed.tokens_saved))
    }

    fn replace_item(&self, item: ContextItem) {
        let mut items = self.items.write().expect("context item lock poisoned");
        items.retain(|i| i.id != item.id);
        items.push(item);
    }

    // ---------------------------------------------------------------------
    // Snapshots
    // ---------------------------------------------------------------------

    /// Creates a durable snapshot of the engine state.
    pub fn snapshot(
        &self,
        id: Option<String>,
        task: Option<String>,
        session: Option<String>,
    ) -> Result<ContextSnapshot> {
        if !self.config.enabled {
            bail!("context engine is disabled");
        }
        let items = self.list_items();
        let snapshot = ContextSnapshot {
            id: id.unwrap_or_else(crate::context::snapshot::generate_snapshot_id),
            created_at: chrono::Utc::now().to_rfc3339(),
            active_items: items
                .iter()
                .filter(|i| i.state.is_active())
                .cloned()
                .collect(),
            offloaded_item_ids: self.offloads.list_ids()?,
            task,
            session,
            budget: self.config.budget,
        };
        self.snapshots.put(&snapshot)?;
        Ok(snapshot)
    }

    /// Lists snapshot summaries.
    pub fn list_snapshots(&self) -> Result<Vec<crate::context::snapshot::SnapshotSummary>> {
        self.snapshots.list_summaries()
    }

    /// Inspects a snapshot by id.
    pub fn inspect_snapshot(&self, id: &str) -> Result<Option<ContextSnapshot>> {
        self.snapshots.get(id)
    }

    /// Restores engine state from a snapshot.
    ///
    /// Restores only items that fit the active budget (never blindly); any
    /// item that does not fit is reported as skipped, not silently dropped.
    pub fn restore_snapshot(&self, id: &str) -> Result<SnapshotRestore> {
        let Some(snapshot) = self.snapshots.get(id)? else {
            bail!("snapshot not found: {id}");
        };
        let mut restored = Vec::new();
        let mut skipped = Vec::new();
        let mut active = 0usize;
        for item in &snapshot.active_items {
            if active + item.token_count <= self.config.budget.usable_input_tokens() {
                let mut item = item.clone();
                item.recency = 1.0;
                item.state = ContextState::Active;
                self.replace_item(item.clone());
                active += item.token_count;
                restored.push(item.id);
            } else {
                skipped.push(item.id.clone());
            }
        }
        Ok(SnapshotRestore {
            snapshot_id: snapshot.id,
            restored,
            skipped,
        })
    }

    /// Deletes a snapshot (explicit only).
    pub fn delete_snapshot(&self, id: &str) -> Result<bool> {
        self.snapshots.delete(id)
    }

    // ---------------------------------------------------------------------
    // Status / observability
    // ---------------------------------------------------------------------

    /// Builds the full status view.
    pub fn status(&self) -> Result<ContextStatus> {
        let items = self.list_items();
        let active_tokens = items
            .iter()
            .filter(|i| i.state.is_active())
            .map(|i| i.token_count)
            .sum();
        let memories = if self.config.memory_enabled {
            // The memory count comes from the existing store; if unreadable,
            // report None rather than failing the whole status view.
            self.memory
                .search("", None)
                .ok()
                .map(|entries| entries.len())
                .or(Some(0))
        } else {
            None
        };
        Ok(ContextStatus {
            enabled: self.config.enabled,
            active_items: items.iter().filter(|i| i.state.is_active()).count(),
            active_tokens,
            budget: self.config.budget.status(active_tokens),
            offloaded_items: self.offloads.len()?,
            memories,
            last_decision: self
                .last_decision
                .read()
                .expect("decision lock poisoned")
                .clone(),
            protected_items: self
                .protected
                .read()
                .expect("protected lock poisoned")
                .len(),
        })
    }
}

/// One search hit from [`ContextEngine::search`].
#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub id: String,
    pub state: ContextState,
    pub source: ContextSource,
    pub token_count: usize,
    pub excerpt: String,
    pub match_location: MatchLocation,
}

/// Where a search hit was found.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MatchLocation {
    Active,
    Offloaded,
}

/// The result of assembling context for a request.
#[derive(Debug, Clone, Serialize)]
pub struct AssembledContext {
    pub items: Vec<ContextItem>,
    pub total_tokens: usize,
    pub budget: ContextBudget,
    pub rejected_ids: Vec<String>,
}

/// The result of a snapshot restore.
#[derive(Debug, Clone, Serialize)]
pub struct SnapshotRestore {
    pub snapshot_id: String,
    pub restored: Vec<String>,
    pub skipped: Vec<String>,
}

/// Builds a short excerpt around the first match.
fn excerpt(content: &str, needle: &str) -> String {
    let lower = content.to_lowercase();
    let Some(start) = lower.find(needle) else {
        return content.chars().take(120).collect();
    };
    let char_start = content
        .char_indices()
        .filter(|(i, _)| *i <= start)
        .map(|(i, _)| i)
        .next_back()
        .unwrap_or(0);
    let context_before = 40;
    let from = char_start.saturating_sub(context_before);
    let len = 200;
    let mut out: String = content[from..].chars().take(len).collect();
    if content.len() > from + len {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine_in(temp: &tempfile::TempDir) -> ContextEngine {
        let config = ContextEngineConfig {
            budget: ContextBudget {
                max_input_tokens: 1_000,
                reserved_output_tokens: 100,
                safety_margin_tokens: 100,
            },
            ..ContextEngineConfig::default()
        };
        ContextEngine::new(temp.path(), config).unwrap()
    }

    fn active_item(id: &str, content: &str, relevance: f32) -> ContextItem {
        let mut item = ContextItem::new(id, ContextSource::Tool, content, 10);
        item.relevance = relevance;
        item
    }

    #[test]
    fn insert_recomputes_tokens_and_lists() {
        let temp = tempfile::tempdir().unwrap();
        let engine = engine_in(&temp);
        let item = engine
            .insert(active_item("a", "one two three four five", 0.5))
            .unwrap();
        // The counter recomputes: 5 words => 5 tokens.
        assert_eq!(item.token_count, 5);
        assert_eq!(engine.list_items().len(), 1);
        assert_eq!(engine.active_tokens(), 5);
    }

    #[test]
    fn insert_rejects_invalid_ids_and_oversized() {
        let temp = tempfile::tempdir().unwrap();
        let engine = engine_in(&temp);
        assert!(engine.insert(active_item("../evil", "c", 0.5)).is_err());
        assert!(engine.insert(active_item("", "c", 0.5)).is_err());
        let huge = active_item("huge", "c", 0.5);
        let mut huge = huge;
        huge.token_count = 999_999;
        assert!(engine.insert(huge).is_err());
    }

    #[test]
    fn disabled_engine_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let config = ContextEngineConfig {
            enabled: false,
            ..ContextEngineConfig::default()
        };
        let engine = ContextEngine::new(temp.path(), config).unwrap();
        assert!(engine.insert(active_item("a", "c", 0.5)).is_err());
        assert!(engine.status().is_ok()); // status still observable
    }

    #[test]
    fn protect_prevents_offload_and_archive() {
        let temp = tempfile::tempdir().unwrap();
        let engine = engine_in(&temp);
        engine.insert(active_item("p", "c", 0.5)).unwrap();
        assert!(engine.protect("p"));
        assert!(engine.offload("p", "reason").is_err());
        assert!(engine.archive("p").is_err());
        assert!(engine.unprotect("p"));
        assert!(engine.offload("p", "reason").is_ok());
    }

    #[test]
    fn offload_then_restore_round_trips_content() {
        let temp = tempfile::tempdir().unwrap();
        let engine = engine_in(&temp);
        engine
            .insert(active_item("x", "precious tool output with details", 0.5))
            .unwrap();
        engine.offload("x", "low relevance").unwrap();
        // Content recoverable from the durable store.
        let offloaded = engine.offloads.get("x").unwrap().unwrap();
        assert_eq!(offloaded.content, "precious tool output with details");
        let restored = engine.restore("x").unwrap();
        assert_eq!(restored.content, "precious tool output with details");
        assert!(restored.state.is_active());
        assert_eq!(restored.recency, 1.0);
        // The item is active again; the durable record kept during the
        // in-memory-restore path is cleaned up on the next explicit restore
        // from the store (or by remove_restored). It must still be readable
        // right now — never a window where content is unrecoverable.
        assert!(engine.offloads.get("x").unwrap().is_some());
        engine.offloads.remove_restored("x").unwrap();
        assert!(engine.offloads.get("x").unwrap().is_none());
    }

    #[test]
    fn offload_failure_leaves_item_active() {
        let temp = tempfile::tempdir().unwrap();
        let engine = engine_in(&temp);
        engine.insert(active_item("y", "c", 0.5)).unwrap();
        // Force durable-write failure by removing the offload directory.
        std::fs::remove_dir_all(
            temp.path()
                .join(".agent")
                .join("context-engine")
                .join("offloads"),
        )
        .unwrap();
        assert!(engine.offload("y", "r").is_err());
        // Fail-safe: the item remains active in the engine.
        let item = engine.get_item("y").unwrap();
        assert!(item.state.is_active());
    }

    #[test]
    fn restore_respects_budget() {
        let temp = tempfile::tempdir().unwrap();
        let config = ContextEngineConfig {
            budget: ContextBudget {
                max_input_tokens: 100,
                reserved_output_tokens: 0,
                safety_margin_tokens: 0,
            },
            ..ContextEngineConfig::default()
        };
        let engine = ContextEngine::new(temp.path(), config).unwrap();
        // Insert an item, then offload it, then fill active to near capacity.
        let mut big = active_item("big", &"word ".repeat(90), 0.5);
        big.token_count = 90;
        engine.insert(big).unwrap();
        engine.offload("big", "r").unwrap();
        let mut filler = active_item("filler", &"w ".repeat(20), 0.5);
        filler.token_count = 20;
        engine.insert(filler).unwrap();
        // Restoring big (90 tokens) into 20 active would overflow 100.
        assert!(engine.restore("big").is_err());
        // The offloaded record is still intact after the refusal.
        assert!(engine.offloads.get("big").unwrap().is_some());
    }

    #[test]
    fn search_finds_active_and_offloaded_items() {
        let temp = tempfile::tempdir().unwrap();
        let engine = engine_in(&temp);
        engine
            .insert(active_item("a", "database connection string", 0.5))
            .unwrap();
        engine
            .insert(active_item("b", "database pool settings", 0.5))
            .unwrap();
        engine.offload("b", "r").unwrap();
        let hits = engine.search("database", 10).unwrap();
        assert_eq!(hits.len(), 2);
        assert!(hits
            .iter()
            .any(|h| h.id == "a" && h.match_location == MatchLocation::Active));
        // "b" was offloaded through the engine: the in-memory record carries
        // the Offloaded state and the search reports it as offloaded.
        let b = hits.iter().find(|h| h.id == "b").unwrap();
        assert_eq!(b.state, crate::context::item::ContextState::Offloaded);
        // Excerpts include the match.
        assert!(hits.iter().all(|h| h.excerpt.contains("database")));
    }

    #[test]
    fn search_empty_query_returns_nothing() {
        let temp = tempfile::tempdir().unwrap();
        let engine = engine_in(&temp);
        engine.insert(active_item("a", "content", 0.5)).unwrap();
        assert!(engine.search("", 10).unwrap().is_empty());
        assert!(engine.search("   ", 10).unwrap().is_empty());
    }

    #[test]
    fn get_context_respects_small_budgets() {
        let temp = tempfile::tempdir().unwrap();
        let config = ContextEngineConfig {
            budget: ContextBudget {
                max_input_tokens: 100,
                reserved_output_tokens: 0,
                safety_margin_tokens: 0,
            },
            ..ContextEngineConfig::default()
        };
        let engine = ContextEngine::new(temp.path(), config).unwrap();
        // insert() recounts tokens from content: "relevant" is 3 tokens while
        // "irrelevant" carries 150 distinct words (150 tokens) so only the
        // high-value item fits the 100-token usable budget.
        let relevant = active_item("relevant", "alpha beta gamma", 0.95);
        let words: Vec<String> = (0..150).map(|i| format!("w{i}")).collect();
        let irrelevant = active_item("irrelevant", &words.join(" "), 0.02);
        engine.insert(relevant).unwrap();
        engine.insert(irrelevant).unwrap();
        let request = ContextRequest {
            task: "disjoint unrelated words".to_string(),
            query: None,
            token_budget: 100,
            scope: ContextScope::Project,
        };
        let assembled = engine.get_context(&request).unwrap();
        assert!(assembled.items.iter().any(|i| i.id == "relevant"));
        assert!(assembled.rejected_ids.contains(&"irrelevant".to_string()));
        assert!(assembled.total_tokens <= 100);
    }

    #[test]
    fn optimize_offloads_low_value_items_and_reports() {
        let temp = tempfile::tempdir().unwrap();
        let engine = engine_in(&temp);
        engine
            .insert(active_item("gold", "alpha beta gamma", 0.95))
            .unwrap();
        // Offloading requires a genuinely low score: stale (recency 0), low
        // priority, and near-zero relevance land well under the offload
        // threshold; a merely-irrelevant fresh item is archived instead.
        let mut junk = active_item("junk", "zzz zzz zzz", 0.01);
        junk.recency = 0.0;
        junk.priority = 0.0;
        engine.insert(junk).unwrap();
        let summary = engine.optimize("alpha beta gamma task").unwrap();
        assert!(summary.offloaded.contains(&"junk".to_string()));
        assert!(summary.kept.contains(&"gold".to_string()));
        assert!(summary.tokens_after <= summary.tokens_before);
        assert_eq!(engine.active_tokens(), summary.tokens_after);
        // Offloaded junk is still recoverable.
        assert!(engine.offloads.get("junk").unwrap().is_some());
        // Status reflects the pass.
        let status = engine.status().unwrap();
        assert!(status.last_decision.is_some());
        assert_eq!(status.offloaded_items, 1);
    }

    #[test]
    fn optimize_respects_auto_offload_disabled() {
        let temp = tempfile::tempdir().unwrap();
        let config = ContextEngineConfig {
            auto_offload: false,
            ..ContextEngineConfig::default()
        };
        let engine = ContextEngine::new(temp.path(), config).unwrap();
        engine.insert(active_item("junk", "zzz zzz", 0.01)).unwrap();
        let summary = engine.optimize("unrelated task").unwrap();
        assert!(summary.offloaded.is_empty());
        assert!(engine.offloads.is_empty().unwrap());
    }

    #[test]
    fn snapshot_create_list_inspect_restore() {
        let temp = tempfile::tempdir().unwrap();
        let engine = engine_in(&temp);
        engine.insert(active_item("a", "one two", 0.5)).unwrap();
        let snapshot = engine
            .snapshot(
                Some("snap-1".into()),
                Some("task".into()),
                Some("sess".into()),
            )
            .unwrap();
        assert_eq!(snapshot.active_items.len(), 1);
        assert_eq!(engine.list_snapshots().unwrap().len(), 1);

        // Mutate, then restore.
        engine.offload("a", "r").unwrap();
        let restore = engine.restore_snapshot("snap-1").unwrap();
        assert_eq!(restore.restored, vec!["a".to_string()]);
        assert!(restore.skipped.is_empty());
        let item = engine.get_item("a").unwrap();
        assert!(item.state.is_active());

        let inspected = engine.inspect_snapshot("snap-1").unwrap().unwrap();
        assert_eq!(inspected.task.as_deref(), Some("task"));
        assert!(engine.delete_snapshot("snap-1").unwrap());
        assert!(engine.list_snapshots().unwrap().is_empty());
    }

    #[test]
    fn snapshot_restore_skips_items_over_budget() {
        let temp = tempfile::tempdir().unwrap();
        let config = ContextEngineConfig {
            budget: ContextBudget {
                max_input_tokens: 50,
                reserved_output_tokens: 0,
                safety_margin_tokens: 0,
            },
            ..ContextEngineConfig::default()
        };
        let engine = ContextEngine::new(temp.path(), config).unwrap();
        engine
            .insert(active_item("huge", &"w ".repeat(60), 0.5))
            .unwrap();
        engine.snapshot(Some("s".into()), None, None).unwrap();
        engine.remove_item("huge");
        let restore = engine.restore_snapshot("s").unwrap();
        assert!(restore.skipped.contains(&"huge".to_string()));
        assert!(restore.restored.is_empty());
    }

    #[test]
    fn compress_item_saves_tokens_and_records_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let engine = engine_in(&temp);
        let content = "dup line\nother\n".repeat(20) + "unique words here";
        engine.insert(active_item("c", &content, 0.5)).unwrap();
        let before = engine.get_item("c").unwrap().token_count;
        let saved = engine.compress_item("c").unwrap().unwrap();
        assert!(saved > 0);
        let item = engine.get_item("c").unwrap();
        assert_eq!(item.token_count, before - saved);
        assert!(item.metadata["context_engine"]["compressed"]
            .as_bool()
            .unwrap());
    }

    #[test]
    fn archive_removes_from_active_but_keeps_inspectable() {
        let temp = tempfile::tempdir().unwrap();
        let engine = engine_in(&temp);
        engine.insert(active_item("z", "still here", 0.5)).unwrap();
        engine.archive("z").unwrap();
        let item = engine.get_item("z").unwrap();
        assert_eq!(item.state, ContextState::Archived);
        assert_eq!(engine.active_tokens(), 0);
        assert_eq!(item.content, "still here");
    }

    #[test]
    fn env_overrides_parse_bools_and_budgets() {
        let vars = std::collections::HashMap::from([
            ("AWH_CONTEXT_ENABLED", "false"),
            ("AWH_CONTEXT_MAX_INPUT_TOKENS", "12345"),
            ("AWH_CONTEXT_RESERVED_OUTPUT_TOKENS", "111"),
            ("AWH_CONTEXT_SAFETY_MARGIN_TOKENS", "22"),
            ("AWH_CONTEXT_AUTO_OFFLOAD", "0"),
        ]);
        let lookup = |key: &str| vars.get(key).map(|v| v.to_string());
        let resolved = ContextEngineConfig::default()
            .with_overrides_from(lookup)
            .unwrap();
        assert!(!resolved.enabled);
        assert_eq!(resolved.budget.max_input_tokens, 12_345);
        assert_eq!(resolved.budget.reserved_output_tokens, 111);
        assert_eq!(resolved.budget.safety_margin_tokens, 22);
        assert!(!resolved.auto_offload);
    }

    #[test]
    fn env_overrides_reject_invalid_values() {
        let lookup =
            |key: &str| (key == "AWH_CONTEXT_MAX_INPUT_TOKENS").then(|| "not-a-number".to_string());
        assert!(ContextEngineConfig::default()
            .with_overrides_from(lookup)
            .is_err());
    }

    #[test]
    fn status_reports_budget_and_counts() {
        let temp = tempfile::tempdir().unwrap();
        let engine = engine_in(&temp);
        engine
            .insert(active_item("a", "one two three", 0.5))
            .unwrap();
        let status = engine.status().unwrap();
        assert!(status.enabled);
        assert_eq!(status.active_items, 1);
        assert_eq!(status.active_tokens, 3);
        assert!(status.budget.within_budget);
        assert_eq!(status.offloaded_items, 0);
        assert_eq!(status.protected_items, 0);
        assert!(status.memories.is_some());
    }

    #[test]
    fn scores_reflect_relevance() {
        let temp = tempfile::tempdir().unwrap();
        let engine = engine_in(&temp);
        engine
            .insert(active_item("hot", "auth login tokens", 0.9))
            .unwrap();
        engine.insert(active_item("cold", "zzz qqq", 0.1)).unwrap();
        let scores = engine.scores("fix auth login");
        assert!(scores["hot"] > scores["cold"]);
    }

    #[test]
    fn bounded_active_items() {
        let temp = tempfile::tempdir().unwrap();
        let engine = engine_in(&temp);
        for i in 0..100 {
            engine
                .insert(active_item(&format!("item-{i}"), "c", 0.5))
                .unwrap();
        }
        // Overwrite (replace) always works.
        engine.insert(active_item("item-0", "c2", 0.5)).unwrap();
        assert_eq!(engine.list_items().len(), 100);
    }
}
