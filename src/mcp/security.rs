use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageIntegrity { pub sha256: String }

pub fn validate_id(id: &str) -> Result<()> {
    if id.is_empty() || id.len() > 128 { bail!("MCP id must contain 1-128 characters"); }
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')) { bail!("MCP id contains unsupported characters"); }
    Ok(())
}

pub fn validate_command(command: Option<&str>) -> Result<()> {
    if let Some(command) = command {
        let command = command.trim();
        if command.is_empty() { bail!("MCP command cannot be empty"); }
        if command.contains(['\n', '\r', '\0']) { bail!("MCP command contains invalid control characters"); }
    }
    Ok(())
}

pub fn validate_url(url: Option<&str>) -> Result<()> {
    if let Some(url) = url {
        let parsed = reqwest::Url::parse(url)?;
        match parsed.scheme() { "http" | "https" => {}, other => bail!("unsupported MCP URL scheme: {other}") }
    }
    Ok(())
}

pub fn sha256_file(path: impl AsRef<Path>) -> Result<String> {
    let bytes = std::fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn verify_sha256(path: impl AsRef<Path>, expected: &str) -> Result<()> {
    let actual = sha256_file(path)?;
    if !actual.eq_ignore_ascii_case(expected) { bail!("MCP integrity check failed: SHA-256 mismatch"); }
    Ok(())
}
