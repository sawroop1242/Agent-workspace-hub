//! Bearer-token authentication for the remote MCP transport.
//!
//! Remote (HTTP/SSE) access **must** be authenticated. The expected API key is
//! supplied out-of-band (never embedded in source) and read from an environment
//! variable at startup. Comparisons use a constant-time routine to resist
//! timing side channels, and the key value is never logged or echoed.

use anyhow::{bail, Context, Result};
use subtle::ConstantTimeEq;

/// Reads and validates the remote-transport API key from an environment
/// variable.
///
/// The variable name is configurable (default `AWH_API_KEY`). A missing or
/// empty key is an error — the remote server fails closed rather than serving
/// unauthenticated traffic.
pub fn load_api_key(env_name: &str) -> Result<String> {
    let raw = std::env::var(env_name)
        .with_context(|| format!("missing API key: environment variable {env_name} is not set"))?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("API key environment variable {env_name} is empty");
    }
    Ok(trimmed.to_string())
}

/// Verifies a presented bearer token against the expected API key using a
/// constant-time comparison.
pub fn verify_token(expected: &str, presented: &str) -> bool {
    // Constant-time comparison on the raw bytes; the length check is not
    // secret, but the comparison of the overlapping bytes is.
    if expected.len() != presented.len() {
        return false;
    }
    expected.as_bytes().ct_eq(presented.as_bytes()).into()
}

/// Extracts the bearer token from an `Authorization` header value, if present.
///
/// Only the `Bearer` scheme is recognized. Returns `None` for a missing or
/// malformed header so callers can fail closed with a 401.
pub fn bearer_token(authorization: Option<&str>) -> Option<&str> {
    let header = authorization?;
    let (scheme, token) = header.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = token.trim();
    if token.is_empty() {
        return None;
    }
    Some(token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn bearer_token_parses_bearer_scheme() {
        assert_eq!(bearer_token(Some("Bearer abc123")), Some("abc123"));
        assert_eq!(bearer_token(Some("bearer abc123")), Some("abc123"));
        assert_eq!(bearer_token(Some("BEARER   abc123  ")), Some("abc123"));
    }

    #[test]
    fn bearer_token_rejects_non_bearer_and_malformed() {
        assert_eq!(bearer_token(None), None);
        assert_eq!(bearer_token(Some("Basic abc123")), None);
        assert_eq!(bearer_token(Some("Bearer")), None);
        assert_eq!(bearer_token(Some("Bearer ")), None);
        assert_eq!(bearer_token(Some("")), None);
    }

    #[test]
    fn verify_token_uses_constant_time_comparison() {
        assert!(verify_token("secret", "secret"));
        assert!(!verify_token("secret", "Secret"));
        assert!(!verify_token("secret", "secret2"));
        assert!(!verify_token("secret", ""));
        // Equal empty strings match (length 0, trivial), but an empty key is
        // rejected at load time by `load_api_key`, so this never authorizes.
        assert!(verify_token("", ""));
    }

    #[test]
    fn load_api_key_reads_configured_variable() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var("AWH_TEST_API_KEY", "  topsecret  ");
        let key = load_api_key("AWH_TEST_API_KEY").unwrap();
        assert_eq!(key, "topsecret");
        std::env::remove_var("AWH_TEST_API_KEY");
    }

    #[test]
    fn load_api_key_fails_closed_when_missing_or_empty() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::remove_var("AWH_TEST_API_KEY_MISSING");
        assert!(load_api_key("AWH_TEST_API_KEY_MISSING").is_err());

        std::env::set_var("AWH_TEST_API_KEY_EMPTY", "   ");
        assert!(load_api_key("AWH_TEST_API_KEY_EMPTY").is_err());
        std::env::remove_var("AWH_TEST_API_KEY_EMPTY");
    }
}
