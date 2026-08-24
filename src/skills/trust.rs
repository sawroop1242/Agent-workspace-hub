use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrustLevel {
    Official,
    Verified,
    Community,
    Untrusted,
}

impl TrustLevel {
    pub fn requires_confirmation(&self) -> bool {
        matches!(self, Self::Community | Self::Untrusted)
    }
}

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
