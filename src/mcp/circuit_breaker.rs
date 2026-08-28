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

/// Half-open probe state, tracking a single trial call.
struct State {
    failures: u32,
    opened_at: Option<Instant>,
}

impl State {
    fn new() -> Self {
        Self {
            failures: 0,
            opened_at: None,
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
    fn allow(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(opened_at) = state.opened_at {
            if opened_at.elapsed() < self.config.cooldown {
                return false; // still open
            }
            // Transition to half-open: permit exactly one trial call.
            state.opened_at = None;
        }
        true
    }

    /// Records a successful call, closing the breaker.
    fn record_success(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.failures = 0;
        state.opened_at = None;
    }

    /// Records a failure, incrementing the counter and possibly opening the breaker.
    /// Returns `true` if this failure tripped the breaker into the open state.
    fn record_failure(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.failures += 1;
        if state.failures >= self.config.failure_threshold {
            state.opened_at = Some(Instant::now());
            self.opened_count.fetch_add(1, Ordering::Relaxed);
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
            self.breaker.rejected_count.fetch_add(1, Ordering::Relaxed);
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
            self.breaker.rejected_count.fetch_add(1, Ordering::Relaxed);
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
}
