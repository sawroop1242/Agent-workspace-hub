//! Circuit breaker for MCP providers.
//!
//! Repeated, consecutive tool/list failures from an MCP provider trip the
//! breaker open, after which calls are rejected immediately (fail fast) until a
//! cooldown elapses. This prevents an unhealthy MCP server from degrading the
//! whole runtime with repeated timeouts.

use crate::mcp::audit::audit_circuit_open;
use crate::mcp::providers::McpClient;
use anyhow::{anyhow, Result};
use serde_json::Value;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Configuration for a [`CircuitBreaker`].
#[derive(Debug, Clone, Copy)]
pub struct CircuitBreakerConfig {
    /// Consecutive failures that must accumulate before the breaker opens.
    pub failure_threshold: u32,
    /// How long the breaker stays open before allowing a half-open probe.
    pub cooldown: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            cooldown: Duration::from_secs(30),
        }
    }
}

/// Saturating metric increment: counters must pin at `u64::MAX`, never
/// wrap to 0 (which would report a healthy provider). The CAS loop in
/// `fetch_update` keeps concurrent increments correct.
fn saturating_increment_u64(counter: &AtomicU64) {
    counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            Some(current.saturating_add(1))
        })
        .ok();
}

/// Same, for the u32 open-counter.
fn saturating_increment_u32(counter: &AtomicU32) {
    counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            Some(current.saturating_add(1))
        })
        .ok();
}

/// Upper bound on how long a half-open probe may stay in flight before it
/// is considered lost (e.g. the caller's future was dropped mid-call) and
/// a fresh probe may be admitted. Keeps the breaker self-healing without
/// manual intervention.
const PROBE_TIMEOUT: Duration = Duration::from_secs(60);

/// The breaker's phase within the classic three-state machine.
enum Phase {
    /// Calls pass through; consecutive failures increment a counter.
    Closed,
    /// The failure threshold was exceeded; calls are rejected until the
    /// cooldown elapses.
    Open { opened_at: Instant },
    /// Cooldown has elapsed: exactly one trial call is admitted at a time.
    /// `Some(started)` marks an in-flight probe; `None` means the next call
    /// becomes the probe.
    HalfOpen { probe: Option<Instant> },
}

/// Circuit breaker state: failure counter plus current phase.
struct State {
    failures: u32,
    phase: Phase,
}

impl State {
    fn new() -> Self {
        Self {
            failures: 0,
            phase: Phase::Closed,
        }
    }
}

/// A fail-fast circuit breaker guarding MCP provider calls.
///
/// The breaker is in one of three states:
/// - **Closed**: calls pass through; consecutive failures increment a counter.
/// - **Open**: the failure threshold was exceeded; calls are rejected until the
///   cooldown elapses.
/// - **Half-open**: after cooldown, one trial call is permitted; success closes
///   the breaker and resets the counter, failure re-opens it.
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: Mutex<State>,
    opened_count: AtomicU32,
    rejected_count: AtomicU64,
}

impl CircuitBreaker {
    /// Creates a breaker with the given configuration.
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            state: Mutex::new(State::new()),
            opened_count: AtomicU32::new(0),
            rejected_count: AtomicU64::new(0),
        }
    }

    /// Whether a call should be permitted right now.
    ///
    /// Deterministic transitions (§17): after the breaker opens and the
    /// cooldown elapses, exactly ONE half-open probe is admitted. Concurrent
    /// callers are rejected until the probe resolves (success closes the
    /// breaker, failure re-opens it), so recovery traffic can never stampede
    /// a recovering provider. A probe stuck longer than [`PROBE_TIMEOUT`]
    /// (dropped caller) is considered lost and a fresh probe is admitted.
    fn allow(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        // Open → HalfOpen when the cooldown has fully elapsed.
        if let Phase::Open { opened_at } = state.phase {
            if opened_at.elapsed() < self.config.cooldown {
                return false; // still open
            }
            state.phase = Phase::HalfOpen { probe: None };
        }
        match state.phase {
            Phase::Closed => true,
            // A probe is already in flight and has not timed out: reject.
            Phase::HalfOpen {
                probe: Some(started),
            } if started.elapsed() < PROBE_TIMEOUT => false,
            // No probe yet (or a lost one): this caller becomes the probe.
            Phase::HalfOpen { .. } => {
                state.phase = Phase::HalfOpen {
                    probe: Some(Instant::now()),
                };
                true
            }
            Phase::Open { .. } => false, // re-checked above; unreachable guard
        }
    }

    /// Records a successful call, closing the breaker.
    fn record_success(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.failures = 0;
        state.phase = Phase::Closed;
    }

    /// Records a failure, incrementing the counter and possibly opening the breaker.
    /// Returns `true` if this failure tripped the breaker into the open state.
    fn record_failure(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.failures += 1;
        // A failed half-open probe re-opens immediately regardless of the
        // counter (it already reached the threshold to open originally).
        let tripped = match state.phase {
            Phase::HalfOpen { .. } => true,
            _ => state.failures >= self.config.failure_threshold,
        };
        if tripped {
            state.phase = Phase::Open {
                opened_at: Instant::now(),
            };
            saturating_increment_u32(&self.opened_count);
            true
        } else {
            false
        }
    }

    /// The number of times the breaker has transitioned to the open state.
    pub fn opened_count(&self) -> u32 {
        self.opened_count.load(Ordering::Relaxed)
    }

    /// The number of calls rejected while the breaker was open.
    pub fn rejected_count(&self) -> u64 {
        self.rejected_count.load(Ordering::Relaxed)
    }
}

/// Wraps an MCP client so that repeated failures trip a [`CircuitBreaker`].
pub struct CircuitBreakerMcpClient<C> {
    id: String,
    client: std::sync::Arc<C>,
    breaker: std::sync::Arc<CircuitBreaker>,
}

impl<C> CircuitBreakerMcpClient<C> {
    /// Wraps `client` behind a breaker owned by the returned handle's `Arc`.
    pub fn new(
        id: impl Into<String>,
        client: std::sync::Arc<C>,
        config: CircuitBreakerConfig,
    ) -> Self {
        Self {
            id: id.into(),
            client,
            breaker: std::sync::Arc::new(CircuitBreaker::new(config)),
        }
    }

    /// Returns a shared reference to the underlying breaker.
    pub fn breaker(&self) -> std::sync::Arc<CircuitBreaker> {
        std::sync::Arc::clone(&self.breaker)
    }
}

/// Reuses the raw MCP trait so the breaker can wrap any [`McpClient`].
#[async_trait::async_trait]
impl<C: McpClient> McpClient for CircuitBreakerMcpClient<C> {
    async fn tools_list(&self) -> Result<Value> {
        let result = if self.breaker.allow() {
            self.client.tools_list().await
        } else {
            saturating_increment_u64(&self.breaker.rejected_count);
            return Err(anyhow!(
                "MCP provider '{}' is unavailable (circuit breaker open)",
                self.id
            ));
        };
        self.record(result)
    }

    async fn tools_call(&self, tool: &str, args: Value) -> Result<Value> {
        let result = if self.breaker.allow() {
            self.client.tools_call(tool, args).await
        } else {
            saturating_increment_u64(&self.breaker.rejected_count);
            return Err(anyhow!(
                "MCP provider '{}' is unavailable (circuit breaker open)",
                self.id
            ));
        };
        self.record(result)
    }
}

impl<C> CircuitBreakerMcpClient<C> {
    fn record(&self, result: Result<Value>) -> Result<Value> {
        match &result {
            Ok(_) => {
                self.breaker.record_success();
            }
            Err(_) => {
                if self.breaker.record_failure() {
                    audit_circuit_open(&self.id);
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;
    use std::sync::Arc;

    #[test]
    fn rejected_count_saturates_at_max() {
        // The breaker's rejection metric must pin at u64::MAX near the
        // boundary (MAX-2, MAX-1, MAX) rather than wrap to 0 — a wrapped
        // rejection count would report a healthy provider.
        for start in [u64::MAX - 2, u64::MAX - 1, u64::MAX] {
            let breaker = CircuitBreaker::new(CircuitBreakerConfig {
                failure_threshold: 1,
                cooldown: Duration::from_secs(3600),
            });
            breaker.rejected_count.store(start, Ordering::Relaxed);
            for _ in 0..3 {
                saturating_increment_u64(&breaker.rejected_count);
            }
            assert_eq!(
                breaker.rejected_count(),
                u64::MAX,
                "rejected_count must saturate from {start:#x}"
            );
        }
    }

    #[test]
    fn rejected_count_concurrent_increments_never_wrap() {
        // Concurrent CAS increments at the boundary must converge on MAX.
        let breaker = Arc::new(CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            cooldown: Duration::from_secs(3600),
        }));
        breaker
            .rejected_count
            .store(u64::MAX - 2, Ordering::Relaxed);
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let breaker = Arc::clone(&breaker);
                std::thread::spawn(move || {
                    for _ in 0..500 {
                        saturating_increment_u64(&breaker.rejected_count);
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().expect("thread panicked");
        }
        assert_eq!(breaker.rejected_count(), u64::MAX);
    }

    #[test]
    fn opened_count_saturates_at_max() {
        let breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            cooldown: Duration::from_secs(3600),
        });
        breaker.opened_count.store(u32::MAX - 1, Ordering::Relaxed);
        saturating_increment_u32(&breaker.opened_count);
        saturating_increment_u32(&breaker.opened_count);
        assert_eq!(breaker.opened_count(), u32::MAX);
    }

    /// A test client whose behavior is driven by atomic flags.
    struct MockClient {
        failures_remaining: AtomicU32,
    }

    #[async_trait::async_trait]
    impl McpClient for MockClient {
        async fn tools_list(&self) -> Result<Value> {
            self.maybe_fail().await
        }
        async fn tools_call(&self, _tool: &str, _args: Value) -> Result<Value> {
            self.maybe_fail().await
        }
    }

    impl MockClient {
        async fn maybe_fail(&self) -> Result<Value> {
            if self.failures_remaining.load(Ordering::SeqCst) > 0 {
                self.failures_remaining.fetch_sub(1, Ordering::SeqCst);
                anyhow::bail!("simulated failure");
            }
            Ok(serde_json::json!({"ok": true}))
        }
    }

    #[tokio::test]
    async fn opens_after_threshold_and_rejects() {
        // Configure a low threshold and long cooldown for deterministic behavior.
        let cfg = CircuitBreakerConfig {
            failure_threshold: 2,
            cooldown: Duration::from_secs(3600),
        };
        let client = Arc::new(MockClient {
            failures_remaining: AtomicU32::new(100),
        });
        let breaker = CircuitBreakerMcpClient::new("mock", client, cfg);

        assert!(breaker
            .tools_call("x", serde_json::json!({}))
            .await
            .is_err());
        assert!(breaker
            .tools_call("x", serde_json::json!({}))
            .await
            .is_err());
        // Third call is rejected (breaker open).
        let err = breaker
            .tools_call("x", serde_json::json!({}))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("circuit breaker open"), "got: {err}");
        assert_eq!(breaker.breaker().opened_count(), 1);
    }

    #[tokio::test]
    async fn success_resets_failure_counter() {
        let cfg = CircuitBreakerConfig {
            failure_threshold: 2,
            cooldown: Duration::from_secs(3600),
        };
        let client = Arc::new(MockClient {
            failures_remaining: AtomicU32::new(1),
        });
        let breaker = CircuitBreakerMcpClient::new("mock", client, cfg);

        // One failure, then a success resets the counter.
        assert!(breaker
            .tools_call("x", serde_json::json!({}))
            .await
            .is_err());
        assert!(breaker.tools_call("x", serde_json::json!({})).await.is_ok());
        assert_eq!(breaker.breaker().opened_count(), 0);
    }

    /// After the breaker opens and cooldown elapses, exactly ONE half-open
    /// probe is admitted; concurrent callers are rejected until the probe
    /// resolves. This pins the deterministic single-probe transition (§17).
    #[tokio::test]
    async fn half_open_admits_exactly_one_probe() {
        // Zero cooldown so the breaker is immediately half-open after tripping.
        let cfg = CircuitBreakerConfig {
            failure_threshold: 1,
            cooldown: Duration::from_millis(0),
        };

        /// Call-scripted mock: the first call fails (trips the breaker),
        /// the second blocks until released (the in-flight half-open probe),
        /// every later call succeeds immediately.
        struct ScriptedClient {
            calls: std::sync::atomic::AtomicU32,
            started: tokio::sync::mpsc::Sender<()>,
            release: tokio::sync::Mutex<tokio::sync::mpsc::Receiver<()>>,
        }
        #[async_trait::async_trait]
        impl McpClient for ScriptedClient {
            async fn tools_list(&self) -> Result<Value> {
                self.scripted().await
            }
            async fn tools_call(&self, _tool: &str, _args: Value) -> Result<Value> {
                self.scripted().await
            }
        }
        impl ScriptedClient {
            async fn scripted(&self) -> Result<Value> {
                match self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) {
                    0 => Err(anyhow::anyhow!("boom")),
                    1 => {
                        // The half-open probe: announce and hold until released.
                        self.started.send(()).await.unwrap();
                        self.release.lock().await.recv().await.unwrap();
                        Ok(serde_json::json!({"ok": true}))
                    }
                    _ => Ok(serde_json::json!({"ok": true})),
                }
            }
        }

        let (started_tx, mut started_rx) = tokio::sync::mpsc::channel(1);
        let (release_tx, release_rx) = tokio::sync::mpsc::channel(1);
        let client = std::sync::Arc::new(ScriptedClient {
            calls: std::sync::atomic::AtomicU32::new(0),
            started: started_tx,
            release: tokio::sync::Mutex::new(release_rx),
        });
        let breaker = std::sync::Arc::new(CircuitBreakerMcpClient::new(
            "mock",
            client as std::sync::Arc<ScriptedClient>,
            cfg,
        ));

        // Call 1 fails and trips the breaker open (threshold 1).
        assert!(breaker
            .tools_call("trip", serde_json::json!({}))
            .await
            .is_err());

        // Call 2 is the half-open probe (cooldown is zero): it goes to the
        // client and blocks there, holding the single probe slot.
        let probe = {
            let breaker = breaker.clone();
            tokio::spawn(async move {
                breaker
                    .tools_call("probe", serde_json::json!({}))
                    .await
                    .expect("probe call succeeds once released")
            })
        };
        started_rx.recv().await.unwrap();

        // While the probe is in flight, a concurrent call must be rejected
        // by the breaker — it must never reach the client.
        let err = breaker
            .tools_call("concurrent", serde_json::json!({}))
            .await
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("circuit breaker open"),
            "concurrent call during half-open probe must be rejected: {err}"
        );

        // Release the probe; it succeeds and closes the breaker.
        release_tx.send(()).await.unwrap();
        probe.await.unwrap();

        // Post-recovery calls flow normally again.
        let ok = breaker.tools_call("recovered", serde_json::json!({})).await;
        assert!(ok.is_ok(), "breaker must close after a successful probe");
    }
}
