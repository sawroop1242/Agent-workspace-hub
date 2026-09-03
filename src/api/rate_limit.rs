//! In-process sliding-window rate limiter for the Control API
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
pub struct RateLimiter {
    limit: usize,
    window: Duration,
    hits: Mutex<HashMap<String, Vec<Instant>>>,
}

impl RateLimiter {
    pub fn new(limit: usize, window: Duration) -> Self {
        Self {
            limit,
            window,
            hits: Mutex::new(HashMap::new()),
        }
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
}
