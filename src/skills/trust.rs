use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// Trust level for a skill source or package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrustLevel {
    Official,
    Verified,
    Community,
    Untrusted,
}

impl TrustLevel {
    /// Whether installing this skill requires explicit user confirmation.
    pub fn requires_confirmation(&self) -> bool {
        matches!(self, Self::Community | Self::Untrusted)
    }
}

/// Validates a registry-provided SHA-256 digest against a computed one.
pub fn validate_sha256(expected: Option<&str>, actual: &str) -> Result<()> {
    if let Some(expected) = expected {
        if expected.len() != 64 || !expected.chars().all(|c| c.is_ascii_hexdigit()) {
            bail!("invalid SHA-256 digest in registry manifest");
        }
        if !expected.eq_ignore_ascii_case(actual) {
            bail!("skill integrity check failed");
        }
    }
    Ok(())
}
