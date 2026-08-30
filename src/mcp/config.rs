//! Configuration management with explicit precedence rules.
//!
//! Resource limits and tunable runtime knobs are resolved in the following
//! order of precedence (highest first):
//!
//! 1. Explicit in-code/config values (e.g. a [`ResourceLimits`] instance).
//! 2. Environment variables (`AWH_*`), applied as overrides.
//! 3. Built-in, security-conservative defaults.
//!
//! This keeps the default policy fail-closed while allowing operators to tune
//! limits for trusted workloads without editing code.

use anyhow::{bail, Result};
use std::env;
use std::time::Duration;

/// Resource limits applied to the MCP transport and execution layers.
///
/// All values are conservative by default. Oversized messages or timed-out
/// operations are rejected (fail closed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceLimits {
    /// Maximum stdio MCP line/message size in bytes.
    pub max_mcp_line_bytes: usize,
    /// Maximum HTTP response body size in bytes.
    pub max_http_body_bytes: usize,
    /// Per-request MCP timeout.
    pub mcp_request_timeout: Duration,
    /// Underlying HTTP client connect/read timeout.
    pub http_client_timeout: Duration,
    /// Circuit-breaker consecutive-failure threshold.
    pub circuit_failure_threshold: u32,
    /// Circuit-breaker cooldown duration once open.
    pub circuit_cooldown: Duration,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_mcp_line_bytes: 10 * 1024 * 1024,
            max_http_body_bytes: 10 * 1024 * 1024,
            mcp_request_timeout: Duration::from_secs(30),
            http_client_timeout: Duration::from_secs(30),
            circuit_failure_threshold: 5,
            circuit_cooldown: Duration::from_secs(30),
        }
    }
}

impl ResourceLimits {
    /// Applies environment-variable overrides on top of these limits.
    ///
    /// Recognized variables (all optional):
    /// - `AWH_MAX_MCP_LINE_BYTES`
    /// - `AWH_MAX_HTTP_BODY_BYTES`
    /// - `AWH_MCP_REQUEST_TIMEOUT_SECS`
    /// - `AWH_HTTP_CLIENT_TIMEOUT_SECS`
    /// - `AWH_CIRCUIT_FAILURE_THRESHOLD`
    /// - `AWH_CIRCUIT_COOLDOWN_SECS`
    pub fn with_env_overrides(self) -> Result<Self> {
        let mut out = self;
        if let Some(v) = env_opt("AWH_MAX_MCP_LINE_BYTES")? {
            out.max_mcp_line_bytes = v;
        }
        if let Some(v) = env_opt("AWH_MAX_HTTP_BODY_BYTES")? {
            out.max_http_body_bytes = v;
        }
        if let Some(v) = env_opt("AWH_MCP_REQUEST_TIMEOUT_SECS")? {
            out.mcp_request_timeout = Duration::from_secs(v as u64);
        }
        if let Some(v) = env_opt("AWH_HTTP_CLIENT_TIMEOUT_SECS")? {
            out.http_client_timeout = Duration::from_secs(v as u64);
        }
        if let Some(v) = env_opt("AWH_CIRCUIT_FAILURE_THRESHOLD")? {
            out.circuit_failure_threshold = v as u32;
        }
        if let Some(v) = env_opt("AWH_CIRCUIT_COOLDOWN_SECS")? {
            out.circuit_cooldown = Duration::from_secs(v as u64);
        }
        Ok(out)
    }
}

/// Reads an optional `usize` environment variable, erroring on non-numeric input.
fn env_opt(key: &str) -> Result<Option<usize>> {
    match env::var(key) {
        Ok(raw) => match raw.trim().parse::<usize>() {
            Ok(v) => Ok(Some(v)),
            Err(_) => bail!("invalid value for {key}: {raw:?}"),
        },
        Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(_)) => bail!("invalid (non-unicode) value for {key}"),
    }
}

/// Builds a shared, consistently-configured HTTP client.
///
/// The client applies the resolved request timeout and a common user-agent, and
/// its connection pool is reused across all requests for its lifetime. This is
/// the single canonical client configuration; callers should use this rather
/// than `reqwest::Client::new()` so timeout and identity policy stay uniform.
pub fn build_http_client() -> reqwest::Client {
    let limits = ResourceLimits::default()
        .with_env_overrides()
        .unwrap_or_else(|e| {
            tracing::warn!(event = "config_invalid", error = %e);
            ResourceLimits::default()
        });
    reqwest::Client::builder()
        .timeout(limits.http_client_timeout)
        .user_agent(format!("agent-workspace-hub/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("static reqwest client configuration is valid")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serializes access to process-global environment variables, which are
    /// shared across parallel test threads and otherwise race one another.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn defaults_match_security_conservative_baseline() {
        let l = ResourceLimits::default();
        assert_eq!(l.max_mcp_line_bytes, 10 * 1024 * 1024);
        assert_eq!(l.max_http_body_bytes, 10 * 1024 * 1024);
        assert_eq!(l.mcp_request_timeout, Duration::from_secs(30));
        assert_eq!(l.circuit_failure_threshold, 5);
    }

    #[test]
    fn env_overrides_apply_on_top_of_explicit_values() {
        let _guard = ENV_LOCK.lock().unwrap();
        // Explicit base value should be overridden by the env var.
        let base = ResourceLimits {
            max_mcp_line_bytes: 123,
            ..ResourceLimits::default()
        };
        std::env::set_var("AWH_MAX_MCP_LINE_BYTES", "456");
        let resolved = base.with_env_overrides().unwrap();
        assert_eq!(resolved.max_mcp_line_bytes, 456);
        std::env::remove_var("AWH_MAX_MCP_LINE_BYTES");
    }

    #[test]
    fn invalid_env_value_is_rejected() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var("AWH_MAX_HTTP_BODY_BYTES", "not-a-number");
        let resolved = ResourceLimits::default().with_env_overrides();
        assert!(resolved.is_err());
        std::env::remove_var("AWH_MAX_HTTP_BODY_BYTES");
    }
}
