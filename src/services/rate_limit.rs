//! In-process sliding-window rate limiter shared by both API planes
//! (spec section 25 chain: Auth -> Authz -> Rate Limit -> Audit ->
//! Services).
//!
//! Small on purpose: a `HashMap<key, Vec<Instant>>` behind a `Mutex`.
//! Every authenticated request is recorded against the caller's IP;
//! requests older than the window are dropped. No new dependencies.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Default sustained rate for authenticated callers.
pub const DEFAULT_WINDOW: Duration = Duration::from_secs(60);
pub const DEFAULT_MAX_PER_WINDOW: usize = 120;

/// Sliding-window counter. `limit` requests per `window`.
///
/// Bounded memory: keys whose windows have fully expired are pruned
/// on access, and the key set itself is capped — a client spraying
/// random `X-Forwarded-For` values cannot grow the map unboundedly.
pub struct RateLimiter {
    limit: usize,
    window: Duration,
    max_keys: usize,
    hits: Mutex<HashMap<String, Vec<Instant>>>,
}

/// Upper bound on distinct tracked keys. Deliberately generous for
/// legitimate deployments, small enough that exhaustion is impossible.
const MAX_TRACKED_KEYS: usize = 10_000;

impl RateLimiter {
    pub fn new(limit: usize, window: Duration) -> Self {
        Self {
            limit,
            window,
            max_keys: MAX_TRACKED_KEYS,
            hits: Mutex::new(HashMap::new()),
        }
    }

    pub fn with_max_keys(mut self, max_keys: usize) -> Self {
        self.max_keys = max_keys;
        self
    }

    pub fn default_limiter() -> Arc<Self> {
        Arc::new(Self::new(DEFAULT_MAX_PER_WINDOW, DEFAULT_WINDOW))
    }

    pub fn limit(&self) -> usize {
        self.limit
    }

    pub fn window(&self) -> Duration {
        self.window
    }

    /// Records a request for `key`. Returns `Ok(remaining)` when the
    /// caller is under the limit, or `Err(retry_after_secs)` when the
    /// limit is exceeded (the request is NOT recorded in that case).
    pub fn check(&self, key: &str) -> Result<usize, u64> {
        let mut hits = self.hits.lock().expect("rate limiter lock poisoned");
        let now = Instant::now();
        let cutoff = now - self.window;
        // Prune stale keys FIRST so expired windows free their slots —
        // otherwise a capped map could never admit a new key after its
        // spammed entries expired.
        if hits.len() >= self.max_keys {
            let mut stale: Option<String> = None;
            for (k, v) in hits.iter() {
                if k.as_str() != key && v.iter().all(|t| *t <= cutoff) {
                    stale = Some(k.clone());
                    break;
                }
            }
            if let Some(stale) = stale {
                hits.remove(&stale);
            }
        }
        if hits.len() >= self.max_keys && !hits.contains_key(key) {
            // Key set saturated (only reachable under adversarial key
            // spraying): refuse unseen keys rather than grow memory.
            return Err(self.window.as_secs().max(1));
        }
        let entry = hits.entry(key.to_string()).or_default();
        entry.retain(|t| *t > cutoff);
        if entry.len() >= self.limit {
            let oldest = entry.first().copied().unwrap_or(now);
            let retry = self.window - now.duration_since(oldest);
            Err(retry.as_secs().max(1))
        } else {
            entry.push(now);
            Ok(self.limit - entry.len())
        }
    }

    /// Number of recorded hits currently inside the window for `key`
    /// (diagnostics/tests only).
    pub fn current(&self, key: &str) -> usize {
        let mut hits = self.hits.lock().expect("rate limiter lock poisoned");
        let cutoff = Instant::now() - self.window;
        hits.get_mut(key)
            .map(|v| {
                v.retain(|t| *t > cutoff);
                v.len()
            })
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn under_limit_requests_pass_and_count_down() {
        let rl = RateLimiter::new(3, Duration::from_secs(60));
        assert_eq!(rl.check("ip").unwrap(), 2);
        assert_eq!(rl.check("ip").unwrap(), 1);
        assert_eq!(rl.check("ip").unwrap(), 0);
    }

    #[test]
    fn over_limit_returns_retry_after() {
        let rl = RateLimiter::new(2, Duration::from_secs(60));
        assert!(rl.check("ip").is_ok());
        assert!(rl.check("ip").is_ok());
        let retry = rl.check("ip").unwrap_err();
        assert!((1..=60).contains(&retry), "retry {retry}");
        // A denied request must not extend the penalty.
        assert_eq!(rl.current("ip"), 2);
        assert!(rl.check("ip").is_err());
        assert_eq!(rl.current("ip"), 2);
    }

    #[test]
    fn keys_are_independent() {
        let rl = RateLimiter::new(1, Duration::from_secs(60));
        assert!(rl.check("a").is_ok());
        assert!(rl.check("b").is_ok());
        assert!(rl.check("a").is_err());
    }

    #[test]
    fn window_slides_so_old_hits_expire() {
        // A one-millisecond window lets hits fall out quickly; loop to
        // avoid a flaky single-sleep assertion.
        let rl = RateLimiter::new(1, Duration::from_millis(1));
        assert!(rl.check("ip").is_ok());
        assert!(rl.check("ip").is_err());
        let mut expired = false;
        for _ in 0..50 {
            std::thread::sleep(Duration::from_millis(2));
            if rl.check("ip").is_ok() {
                expired = true;
                break;
            }
        }
        assert!(expired, "hit never expired from the window");
    }

    #[test]
    fn zero_limit_denies_everything() {
        let rl = RateLimiter::new(0, Duration::from_secs(60));
        assert!(rl.check("x").is_err());
        assert_eq!(rl.current("x"), 0);
    }

    #[test]
    fn key_spray_is_capped_fail_closed() {
        // Adversarial key spraying must not grow the map unboundedly:
        // once the key cap is reached, unseen keys are refused (fail
        // closed) while known keys keep their windows.
        let rl = RateLimiter::new(2, Duration::from_secs(60)).with_max_keys(2);
        assert!(rl.check("k1").is_ok());
        assert!(rl.check("k2").is_ok());
        assert!(
            rl.check("sprayed-key-1").is_err(),
            "unseen key must be refused at cap"
        );
        assert!(rl.check("sprayed-key-2").is_err());
        assert_eq!(
            rl.current("sprayed-key-1"),
            0,
            "refused key must not be recorded"
        );
        assert!(rl.check("k1").is_ok(), "known keys unaffected");
    }

    #[test]
    fn stale_keys_are_pruned_so_the_cap_decays() {
        let rl = RateLimiter::new(1, Duration::from_millis(1)).with_max_keys(1);
        assert!(rl.check("old").is_ok());
        // Wait for `old`'s window to fully expire, then a new key must
        // be admitted again: pruning reclaimed the expired slot.
        let mut admitted = false;
        for _ in 0..50 {
            std::thread::sleep(Duration::from_millis(2));
            if rl.check("new").is_ok() {
                admitted = true;
                break;
            }
        }
        assert!(admitted, "stale key was never pruned from the cap");
    }
}
