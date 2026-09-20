use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// Trust level for a skill source or package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrustLevel {
    /// Published by an official source.
    Official,
    /// Independently verified.
    Verified,
    /// Community-contributed.
    Community,
    /// Unvetted and untrusted.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_and_verified_do_not_require_confirmation() {
        assert!(!TrustLevel::Official.requires_confirmation());
        assert!(!TrustLevel::Verified.requires_confirmation());
    }

    #[test]
    fn community_and_untrusted_require_confirmation() {
        assert!(TrustLevel::Community.requires_confirmation());
        assert!(TrustLevel::Untrusted.requires_confirmation());
    }

    #[test]
    fn trust_level_serde_round_trips() {
        for level in [
            TrustLevel::Official,
            TrustLevel::Verified,
            TrustLevel::Community,
            TrustLevel::Untrusted,
        ] {
            let json = serde_json::to_string(&level).unwrap();
            let back: TrustLevel = serde_json::from_str(&json).unwrap();
            assert_eq!(
                serde_json::to_string(&back).unwrap(),
                json,
                "trust level must round trip through its wire label"
            );
        }
    }

    const ACTUAL: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    #[test]
    fn validate_sha256_accepts_matching_digest() {
        assert!(validate_sha256(Some(ACTUAL), ACTUAL).is_ok());
        // comparison is case-insensitive
        assert!(validate_sha256(Some(ACTUAL), &ACTUAL.to_uppercase()).is_ok());
        assert!(validate_sha256(Some(&ACTUAL.to_uppercase()), ACTUAL).is_ok());
    }

    #[test]
    fn validate_sha256_accepts_missing_expected_digest() {
        // `None` means the registry did not publish a digest; nothing to check.
        assert!(validate_sha256(None, ACTUAL).is_ok());
        assert!(validate_sha256(None, "").is_ok());
    }

    #[test]
    fn validate_sha256_rejects_mismatching_digest() {
        let other = "f".repeat(64);
        let error = validate_sha256(Some(ACTUAL), &other).unwrap_err();
        assert!(error.to_string().contains("integrity check failed"));
    }

    #[test]
    fn validate_sha256_rejects_malformed_digest() {
        // wrong length
        assert!(validate_sha256(Some("e3b0c442"), ACTUAL).is_err());
        // non-hex characters
        let non_hex = "z".repeat(64);
        assert!(validate_sha256(Some(&non_hex), ACTUAL).is_err());
        // empty string is also malformed, not "missing"
        assert!(validate_sha256(Some(""), ACTUAL).is_err());
    }
}
