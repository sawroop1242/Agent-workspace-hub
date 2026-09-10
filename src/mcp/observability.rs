//! MCP-level observability: lifecycle events, bounded observer hooks, and
//! per-tool metrics.
//!
//! These are *observers*, not authorities: hooks receive events after the
//! security-relevant decision has already been made by the trust/permission/
//! execution layers, and they can neither veto a call, alter arguments, nor
//! influence authorization. Security-critical checks run independently of
//! any registered hook, so an empty hook registry changes no behavior.

use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

/// Maximum number of hooks that may be registered (bounded resource).
pub const MAX_HOOKS: usize = 16;
/// Maximum number of distinct tools tracked by the metrics registry
/// (bounded resource; the catalog is far smaller — this bounds dynamic
/// provider tool names too).
pub const MAX_TRACKED_TOOLS: usize = 1024;

/// A lifecycle event observed by hooks.
///
/// Event payloads carry only safe identifiers (method names, tool names,
/// durations). Arguments, file contents, and secrets are deliberately absent.
#[derive(Debug, Clone)]
pub enum McpEvent<'a> {
    /// A JSON-RPC notification (no `id`) was accepted. Notifications produce
    /// no response; this event exists so servers can observe them.
    NotificationReceived { method: &'a str },
    /// The `initialize` exchange completed successfully.
    InitializeCompleted {
        protocol_version: &'a str,
        client_info: Option<(&'a str, &'a str)>,
    },
    /// A `tools/call` finished (success or failure) with its duration.
    ToolCallCompleted {
        name: &'a str,
        ok: bool,
        duration: Duration,
    },
    /// A `resources/read` completed successfully.
    ResourceRead { uri: &'a str },
    /// A `prompts/get` was requested (this server ships no prompt
    /// templates, so every request is a deterministic unknown-prompt error).
    PromptRequested { name: &'a str },
}

impl McpEvent<'_> {
    /// A short safe label identifying the event family, used for hook-failure
    /// logging. Never includes arguments, URIs' payloads, or secrets.
    fn method_hint(&self) -> &'static str {
        match self {
            McpEvent::NotificationReceived { .. } => "notification",
            McpEvent::InitializeCompleted { .. } => "initialize",
            McpEvent::ToolCallCompleted { .. } => "tool_call",
            McpEvent::ResourceRead { .. } => "resource_read",
            McpEvent::PromptRequested { .. } => "prompt",
        }
    }
}

/// A registered observer. Hooks are synchronous, must be quick, must not
/// re-enter the dispatcher (no dispatching from inside a hook), and cannot
/// modify anything — they receive events for observation only.
pub type McpHook = Arc<dyn Fn(&McpEvent<'_>) + Send + Sync>;

/// The bounded hook registry.
#[derive(Default)]
pub struct McpHooks {
    hooks: RwLock<Vec<McpHook>>,
}

impl McpHooks {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers an observer hook. Fails closed once the registry is full
    /// (`MAX_HOOKS`) so an unbounded number of observers can never be
    /// installed.
    pub fn register(&self, hook: McpHook) -> Result<(), String> {
        let mut hooks = self.hooks.write().unwrap_or_else(|e| e.into_inner());
        if hooks.len() >= MAX_HOOKS {
            return Err(format!("hook registry is full (max {MAX_HOOKS})"));
        }
        hooks.push(hook);
        Ok(())
    }

    /// The number of registered hooks.
    pub fn len(&self) -> usize {
        self.hooks.read().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// Whether no hooks are registered.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Fires an event to every registered hook.
    ///
    /// Each hook is individually isolated with `catch_unwind`: a hook that
    /// panics is recorded (tracing) and skipped, but the remaining hooks
    /// still run and the panic never escapes into the MCP request path.
    /// The hook list is cloned out of the lock before firing, so a hook
    /// that (against the contract) registers or unregisters hooks cannot
    /// deadlock, and a panic cannot poison the registry. The panic payload
    /// is deliberately dropped — hook internals must not leak to clients
    /// or logs.
    pub fn fire(&self, event: &McpEvent<'_>) {
        let hooks: Vec<McpHook> = {
            let hooks = self.hooks.read().unwrap_or_else(|e| e.into_inner());
            hooks.clone()
        };
        for (index, hook) in hooks.into_iter().enumerate() {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                hook(event);
            }));
            if outcome.is_err() {
                tracing::warn!(
                    event = "hook_panicked",
                    hook_index = index,
                    method_hint = event.method_hint(),
                    "observer hook panicked; remaining hooks continue"
                );
            }
        }
    }
}

/// Saturating atomic increment: a metric counter must never wrap to 0
/// (that would silently report "no failures"); it pins at `u64::MAX`.
/// `fetch_update` is a CAS loop, so concurrent increments are all applied —
/// no lost updates, no torn counts, no ordering hazard.
fn saturating_increment(counter: &AtomicU64) {
    counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            Some(current.saturating_add(1))
        })
        .ok();
}

/// Saturating atomic add for duration accounting.
fn saturating_add_to(counter: &AtomicU64, amount: u64) {
    counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            Some(current.saturating_add(amount))
        })
        .ok();
}

/// Per-tool call metrics (observability only — never a gate).
#[derive(Debug, Default)]
struct ToolStats {
    calls: AtomicU64,
    failures: AtomicU64,
    total_duration_ns: AtomicU64,
}

/// A snapshot of one tool's metrics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolMetricsSnapshot {
    pub name: String,
    pub calls: u64,
    pub failures: u64,
    pub avg_duration: Duration,
}

/// Bounded registry of per-tool call metrics.
///
/// Recording is best-effort for observability: once `MAX_TRACKED_TOOLS`
/// distinct names are tracked, new names are ignored (never a security
/// decision, never an error).
#[derive(Default)]
pub struct ToolMetrics {
    stats: Mutex<HashMap<String, Arc<ToolStats>>>,
    /// Upper bound on tracked names (set at construction).
    max_tools: usize,
}

impl ToolMetrics {
    /// Creates a metrics registry tracking at most `max_tools` names.
    pub fn new(max_tools: usize) -> Self {
        Self {
            stats: Mutex::new(HashMap::new()),
            max_tools,
        }
    }

    /// Records one completed `tools/call` (name, whether it failed, duration).
    pub fn record(&self, name: &str, ok: bool, duration: Duration) {
        if name.is_empty() {
            return;
        }
        let stats = {
            let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(existing) = stats.get(name) {
                Arc::clone(existing)
            } else if stats.len() < self.max_tools {
                let fresh = Arc::new(ToolStats::default());
                stats.insert(name.to_string(), Arc::clone(&fresh));
                fresh
            } else {
                // Registry saturated: skip observability for this name.
                return;
            }
        };
        saturating_increment(&stats.calls);
        if !ok {
            saturating_increment(&stats.failures);
        }
        // Saturating u128→u64 conversion: a duration past u64::MAX
        // nanoseconds (~584 years) is recorded as the cap rather than
        // truncating modulo 2^64 (which would silently corrupt averages).
        let duration_ns = duration.as_nanos().min(u64::MAX as u128) as u64;
        saturating_add_to(&stats.total_duration_ns, duration_ns);
    }

    /// A snapshot of every tracked tool's metrics, sorted by name.
    pub fn snapshot(&self) -> Vec<ToolMetricsSnapshot> {
        let stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        let mut out: Vec<ToolMetricsSnapshot> = stats
            .iter()
            .map(|(name, stats)| {
                let calls = stats.calls.load(Ordering::Relaxed);
                let failures = stats.failures.load(Ordering::Relaxed);
                let total_ns = stats.total_duration_ns.load(Ordering::Relaxed);
                ToolMetricsSnapshot {
                    name: name.clone(),
                    calls,
                    failures,
                    avg_duration: Duration::from_nanos(total_ns.checked_div(calls).unwrap_or(0)),
                }
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// Aggregate counters for health reporting: (tools, total_calls, failures).
    pub fn totals(&self) -> (usize, u64, u64) {
        let stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        let mut calls = 0u64;
        let mut failures = 0u64;
        for stats in stats.values() {
            calls = calls.saturating_add(stats.calls.load(Ordering::Relaxed));
            failures = failures.saturating_add(stats.failures.load(Ordering::Relaxed));
        }
        (stats.len(), calls, failures)
    }
}

/// The `clientInfo` accepted during the initialize exchange, reduced to the
/// safe (name, version) pair for hooks — never the full object.
pub fn client_name_version(client_info: Option<&Value>) -> Option<(&str, &str)> {
    let info = client_info?;
    let name = info.get("name")?.as_str()?;
    let version = info.get("version").and_then(Value::as_str).unwrap_or("");
    Some((name, version))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Test-only seeding: drives one tracked tool's counters to given
    /// values so overflow behavior can be exercised without performing
    /// 2^64 real calls.
    fn seed_counters(metrics: &ToolMetrics, name: &str, calls: u64, failures: u64) {
        let stats = metrics.stats.lock().unwrap_or_else(|e| e.into_inner());
        let entry = stats.get(name).expect("tool must be tracked first");
        entry.calls.store(calls, Ordering::Relaxed);
        entry.failures.store(failures, Ordering::Relaxed);
    }

    /// Test-only seeding for the duration counter.
    fn seed_duration(metrics: &ToolMetrics, name: &str, total_ns: u64) {
        let stats = metrics.stats.lock().unwrap_or_else(|e| e.into_inner());
        let entry = stats.get(name).expect("tool must be tracked first");
        entry.total_duration_ns.store(total_ns, Ordering::Relaxed);
    }

    #[test]
    fn tool_failure_counter_saturates_at_max() {
        // The boundary triple u64::MAX-2, MAX-1, MAX: each recorded
        // failure pins at MAX instead of wrapping to 0.
        for start in [u64::MAX - 2, u64::MAX - 1, u64::MAX] {
            let metrics = ToolMetrics::new(8);
            metrics.record("tool", true, Duration::from_nanos(1)); // track it
            seed_counters(&metrics, "tool", 1, start);
            metrics.record("tool", false, Duration::from_nanos(1));
            metrics.record("tool", false, Duration::from_nanos(1));
            let snapshot = &metrics.snapshot()[0];
            assert_eq!(
                snapshot.failures,
                u64::MAX,
                "failures must saturate from {start:#x}"
            );
            assert_eq!(snapshot.calls, 3, "calls still counted from zero");
        }
    }

    #[test]
    fn tool_call_counter_saturates_at_max() {
        for start in [u64::MAX - 2, u64::MAX - 1, u64::MAX] {
            let metrics = ToolMetrics::new(8);
            metrics.record("tool", true, Duration::from_nanos(1));
            seed_counters(&metrics, "tool", start, 0);
            metrics.record("tool", true, Duration::from_nanos(1));
            metrics.record("tool", false, Duration::from_nanos(1));
            let snapshot = &metrics.snapshot()[0];
            assert_eq!(
                snapshot.calls,
                u64::MAX,
                "calls must saturate from {start:#x}"
            );
            assert_eq!(snapshot.failures, 1);
        }
    }

    #[test]
    fn duration_counter_saturates_at_max() {
        let metrics = ToolMetrics::new(8);
        metrics.record("tool", true, Duration::from_nanos(1));
        seed_duration(&metrics, "tool", u64::MAX - 1);
        // Adding any nonzero duration must pin at MAX, not wrap.
        metrics.record("tool", true, Duration::from_nanos(2));
        let stats = metrics.stats.lock().unwrap_or_else(|e| e.into_inner());
        let entry = stats.get("tool").expect("tracked");
        assert_eq!(entry.total_duration_ns.load(Ordering::Relaxed), u64::MAX);
    }

    #[test]
    fn totals_aggregate_saturates_at_max() {
        let metrics = ToolMetrics::new(8);
        metrics.record("a", true, Duration::from_nanos(1));
        metrics.record("b", true, Duration::from_nanos(1));
        seed_counters(&metrics, "a", u64::MAX, 0);
        seed_counters(&metrics, "b", u64::MAX, 0);
        // The aggregate must saturate rather than wrap past MAX.
        let (tools, calls, _failures) = metrics.totals();
        assert_eq!(tools, 2);
        assert_eq!(calls, u64::MAX);
    }

    #[test]
    fn concurrent_increments_at_the_boundary_never_wrap() {
        // Many threads increment a counter already at u64::MAX-2. Every
        // increment is a CAS that either saturates or retries; the final
        // value must be exactly MAX — a value BELOW the starting point
        // (or below MAX) would prove a wrap/lost-update race.
        let metrics = ToolMetrics::new(8);
        metrics.record("tool", true, Duration::from_nanos(1));
        seed_counters(&metrics, "tool", 0, u64::MAX - 2);
        let metrics = Arc::new(metrics);
        let threads: Vec<_> = (0..16)
            .map(|_| {
                let metrics = Arc::clone(&metrics);
                std::thread::spawn(move || {
                    for _ in 0..1_000 {
                        metrics.record("tool", false, Duration::from_nanos(1));
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().expect("thread panicked");
        }
        let snapshot = &metrics.snapshot()[0];
        assert_eq!(
            snapshot.failures,
            u64::MAX,
            "concurrent increments must saturate at MAX, not wrap: {}",
            snapshot.failures
        );
    }

    #[test]
    fn hooks_observe_events_in_order() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let hook_seen = Arc::clone(&seen);
        let hooks = McpHooks::new();
        hooks
            .register(Arc::new(move |event: &McpEvent<'_>| {
                let mut seen = hook_seen.lock().unwrap();
                match event {
                    McpEvent::NotificationReceived { method } => {
                        seen.push(format!("note:{method}"))
                    }
                    McpEvent::ToolCallCompleted { name, ok, .. } => {
                        seen.push(format!("tool:{name}:{}", ok))
                    }
                    _ => seen.push("other".to_string()),
                }
            }))
            .unwrap();

        hooks.fire(&McpEvent::NotificationReceived {
            method: "notifications/initialized",
        });
        hooks.fire(&McpEvent::ToolCallCompleted {
            name: "memory.store",
            ok: true,
            duration: Duration::from_millis(2),
        });
        hooks.fire(&McpEvent::ToolCallCompleted {
            name: "memory.store",
            ok: false,
            duration: Duration::from_millis(1),
        });

        let seen = seen.lock().unwrap();
        assert_eq!(
            *seen,
            vec![
                "note:notifications/initialized",
                "tool:memory.store:true",
                "tool:memory.store:false",
            ]
        );
    }

    #[test]
    fn hook_registry_is_bounded() {
        let hooks = McpHooks::new();
        let noop: McpHook = Arc::new(|_: &McpEvent<'_>| {});
        for _ in 0..MAX_HOOKS {
            hooks.register(Arc::clone(&noop)).unwrap();
        }
        assert_eq!(hooks.len(), MAX_HOOKS);
        assert!(hooks.register(noop).is_err());
    }

    #[test]
    fn hooks_cannot_modify_or_veto() {
        // Hooks receive &McpEvent — read-only by construction. This test
        // pins that a registered hook cannot change dispatch outcomes; the
        // events carry no mutable access to arguments or results.
        let hooks = McpHooks::new();
        let _ = hooks.register(Arc::new(|event: &McpEvent<'_>| {
            // Observe only; nothing here can alter `event`.
            let _ = matches!(event, McpEvent::ResourceRead { uri: _ });
        }));
        hooks.fire(&McpEvent::ResourceRead {
            uri: "awh://context",
        });
        assert_eq!(hooks.len(), 1);
    }

    /// Panic containment, per hook: a panicking hook is contained by
    /// `fire` itself — the CALLER must not need catch_unwind, later hooks
    /// still run, and the registry stays usable. (This replaces the earlier
    /// test that wrongly asserted the panic escapes to the caller.)
    #[test]
    fn panicking_hook_does_not_terminate_process() {
        let hooks = McpHooks::new();
        hooks
            .register(Arc::new(|_: &McpEvent<'_>| panic!("hook bug")))
            .unwrap();
        // No catch_unwind around `fire`: if the panic escaped, this test
        // would abort instead of completing.
        hooks.fire(&McpEvent::PromptRequested { name: "x" });
    }

    #[test]
    fn panicking_hook_does_not_prevent_later_hooks() {
        let hooks = McpHooks::new();
        hooks
            .register(Arc::new(|_: &McpEvent<'_>| panic!("hook A bug")))
            .unwrap();
        let observed = Arc::new(AtomicUsize::new(0));
        let observed_hook = Arc::clone(&observed);
        hooks
            .register(Arc::new(move |_: &McpEvent<'_>| {
                observed_hook.fetch_add(1, Ordering::SeqCst);
            }))
            .unwrap();
        // Hook A panics; hook B must still observe the event.
        hooks.fire(&McpEvent::PromptRequested { name: "x" });
        assert_eq!(observed.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn panicking_hook_does_not_break_registry() {
        let hooks = McpHooks::new();
        hooks
            .register(Arc::new(|_: &McpEvent<'_>| panic!("hook bug")))
            .unwrap();
        hooks.fire(&McpEvent::PromptRequested { name: "x" });
        // The registry survived the contained panic: entries are intact and
        // firing again is safe.
        assert_eq!(hooks.len(), 1);
        hooks.fire(&McpEvent::PromptRequested { name: "x" });
    }

    /// The full request-path invariant: the dispatch path itself (metrics
    /// recording here) must complete even though an observer-side panic
    /// happened. Tool calls keep returning results with panicking hooks
    /// registered; that is pinned end-to-end by the dispatcher test
    /// `panicking_hook_does_not_break_dispatch` in tests/mcp_protocol.rs.
    #[test]
    fn hook_failure_is_observable_through_surviving_registry() {
        let hooks = McpHooks::new();
        hooks
            .register(Arc::new(|_: &McpEvent<'_>| panic!("hook bug")))
            .unwrap();
        let observed = Arc::new(AtomicUsize::new(0));
        let observed_hook = Arc::clone(&observed);
        hooks
            .register(Arc::new(move |_: &McpEvent<'_>| {
                observed_hook.fetch_add(1, Ordering::SeqCst);
            }))
            .unwrap();
        hooks.fire(&McpEvent::NotificationReceived { method: "x" });
        // The surviving hook observed the event; the registry did not shrink
        // or reorder as a side effect of the contained panic.
        assert_eq!(observed.load(Ordering::SeqCst), 1);
        assert_eq!(hooks.len(), 2);
    }

    #[test]
    fn metrics_record_and_snapshot() {
        let metrics = ToolMetrics::new(8);
        metrics.record("memory.store", true, Duration::from_millis(10));
        metrics.record("memory.store", false, Duration::from_millis(20));
        metrics.record("skills.list", true, Duration::from_millis(5));

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot[0].name, "memory.store");
        assert_eq!(snapshot[0].calls, 2);
        assert_eq!(snapshot[0].failures, 1);
        assert_eq!(snapshot[0].avg_duration, Duration::from_millis(15));
        assert_eq!(snapshot[1].name, "skills.list");
        assert_eq!(snapshot[1].calls, 1);

        let (tools, calls, failures) = metrics.totals();
        assert_eq!((tools, calls, failures), (2, 3, 1));
    }

    #[test]
    fn metrics_registry_is_bounded() {
        let metrics = ToolMetrics::new(2);
        metrics.record("a", true, Duration::ZERO);
        metrics.record("b", true, Duration::ZERO);
        metrics.record("c", true, Duration::ZERO); // beyond the cap: ignored
        assert_eq!(metrics.snapshot().len(), 2);
        let names: Vec<String> = metrics.snapshot().into_iter().map(|s| s.name).collect();
        assert!(!names.contains(&"c".to_string()));
    }

    #[test]
    fn client_info_reduction_is_safe() {
        let full = serde_json::json!({"name": "opencode", "version": "1.2.3", "extra": "x"});
        assert_eq!(
            client_name_version(Some(&full)),
            Some(("opencode", "1.2.3"))
        );
        assert_eq!(client_name_version(None), None);
        let bare = serde_json::json!({"name": "anon"});
        assert_eq!(client_name_version(Some(&bare)), Some(("anon", "")));
    }
}
