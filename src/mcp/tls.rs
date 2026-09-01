//! TLS configuration for the remote MCP transport.
//!
//! The remote server supports TLS 1.2+. Certificate and key material is read
//! from files at startup (paths supplied via CLI flags or `AWH_TLS_CERT` /
//! `AWH_TLS_KEY`). Insecure TLS configurations — a certificate without a key,
//! a key without a certificate, or unreadable material — are rejected. Private
//! key bytes are never logged.

use anyhow::{bail, Context, Result};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio_rustls::TlsAcceptor;

/// An optional TLS configuration: `None` means plain HTTP, `Some` enables TLS.
#[derive(Debug, Clone, Default)]
pub struct TlsConfig {
    /// Path to the PEM-encoded certificate chain.
    pub cert: Option<String>,
    /// Path to the PEM-encoded private key.
    pub key: Option<String>,
}

impl TlsConfig {
    /// Validates the configuration: both cert and key must be present, or both
    /// absent. A half-configured TLS setup is rejected (fail closed).
    pub fn validate(&self) -> Result<()> {
        match (&self.cert, &self.key) {
            (None, None) => Ok(()),
            (Some(_), Some(_)) => Ok(()),
            (Some(_), None) => {
                bail!("TLS certificate provided without a private key (AWH_TLS_KEY)")
            }
            (None, Some(_)) => {
                bail!("TLS private key provided without a certificate (AWH_TLS_CERT)")
            }
        }
    }

    /// Returns whether TLS is enabled.
    pub fn enabled(&self) -> bool {
        self.cert.is_some() && self.key.is_some()
    }

    /// Builds a [`TlsAcceptor`] from the configured certificate and key.
    ///
    /// Returns `Ok(None)` when TLS is fully unconfigured. A half-configured
    /// state (cert without key or vice versa) is a typed error, mirroring
    /// [`TlsConfig::validate`], rather than silently treating TLS as off.
    pub fn build_acceptor(&self) -> Result<Option<TlsAcceptor>> {
        let (Some(cert_path), Some(key_path)) = (self.cert.as_deref(), self.key.as_deref()) else {
            if self.cert.is_none() && self.key.is_none() {
                return Ok(None);
            }
            bail!("TLS enabled with incomplete certificate/key configuration");
        };

        ensure_regular_files(cert_path, key_path)?;

        let certs = load_certs(cert_path)?;
        let key = load_key(key_path)?;

        let config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .context("failed to build TLS server configuration")?;

        Ok(Some(TlsAcceptor::from(Arc::new(config))))
    }
}

/// Loads a PEM certificate chain from `path`.
fn load_certs(path: &str) -> Result<Vec<CertificateDer<'static>>> {
    let data =
        fs::read(path).with_context(|| format!("failed to read TLS certificate file {}", path))?;
    let certs = rustls_pemfile::certs(&mut data.as_slice())
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("failed to parse TLS certificate file {path}"))?;
    if certs.is_empty() {
        bail!("TLS certificate file {path} contained no certificates");
    }
    Ok(certs)
}

/// Loads a PEM private key from `path`.
fn load_key(path: &str) -> Result<PrivateKeyDer<'static>> {
    let data =
        fs::read(path).with_context(|| format!("failed to read TLS private key file {path}"))?;
    let key = rustls_pemfile::private_key(&mut data.as_slice())
        .with_context(|| format!("failed to parse TLS private key file {path}"))?
        .context("TLS private key file contained no private key")?;
    // Never log the key contents; the error paths above reference only the path.
    Ok(key)
}

/// Rejects non-regular files (e.g. directories) before any key material is
/// read, failing closed with only the path (never the contents) in the error.
fn ensure_regular_files(cert: &str, key: &str) -> Result<()> {
    for path in [cert, key] {
        match std::fs::metadata(Path::new(path)) {
            Ok(meta) => {
                if !meta.is_file() {
                    bail!("TLS material is not a regular file: {path}");
                }
            }
            Err(e) => bail!("cannot access TLS material {path}: {e}"),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn tls_config_requires_both_or_neither() {
        assert!(TlsConfig {
            cert: None,
            key: None
        }
        .validate()
        .is_ok());
        assert!(TlsConfig {
            cert: Some("c".into()),
            key: Some("k".into())
        }
        .validate()
        .is_ok());
        assert!(TlsConfig {
            cert: Some("c".into()),
            key: None
        }
        .validate()
        .is_err());
        assert!(TlsConfig {
            cert: None,
            key: Some("k".into())
        }
        .validate()
        .is_err());
    }

    #[test]
    fn missing_cert_or_key_is_an_error() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("nope.pem").to_string_lossy().to_string();
        assert!(load_certs(&missing).is_err());
        assert!(load_key(&missing).is_err());
    }

    #[test]
    fn half_configured_acceptor_fails_with_typed_error_not_panic() {
        // Half-configured TLS (cert without key or vice versa) must produce a
        // typed error, never a panic and never a silent "TLS off" acceptor.
        // Regression: this path previously relied on `expect` + `enabled()`
        // coupling, or worse, silently returned Ok(None).
        for config in [
            TlsConfig {
                cert: Some("cert.pem".into()),
                key: None,
            },
            TlsConfig {
                cert: None,
                key: Some("key.pem".into()),
            },
        ] {
            let result = std::panic::catch_unwind(|| config.build_acceptor());
            assert!(
                result.is_ok_and(|r| r.is_err()),
                "half-configured TLS must fail with a typed error, not panic or return Ok(None)"
            );
        }

        // Fully unconfigured still yields Ok(None) (TLS off).
        let off = TlsConfig {
            cert: None,
            key: None,
        };
        assert!(off.build_acceptor().unwrap().is_none());
    }

    #[test]
    fn invalid_cert_and_key_are_rejected() {
        let dir = tempdir().unwrap();
        let bad = dir.path().join("bad.pem");
        std::fs::write(&bad, b"this is not a pem").unwrap();
        let bads = bad.to_string_lossy().to_string();
        assert!(load_certs(&bads).is_err());
        assert!(load_key(&bads).is_err());
    }

    #[test]
    fn ensure_regular_files_rejects_directories() {
        let dir = tempdir().unwrap();
        let d = dir.path().to_string_lossy().to_string();
        let f = dir.path().join("f").to_string_lossy().to_string();
        std::fs::write(&f, b"x").unwrap();
        assert!(ensure_regular_files(&d, &f).is_err());
    }

    #[test]
    fn valid_cert_and_key_build_an_acceptor() {
        let dir = tempdir().unwrap();
        let cert_path = dir.path().join("cert.pem");
        let key_path = dir.path().join("key.pem");
        std::fs::write(&cert_path, TEST_CERT).unwrap();
        std::fs::write(&key_path, TEST_KEY).unwrap();

        let config = TlsConfig {
            cert: Some(cert_path.to_string_lossy().to_string()),
            key: Some(key_path.to_string_lossy().to_string()),
        };
        assert!(config.validate().is_ok());
        assert!(config.enabled());
        let acceptor = config.build_acceptor().unwrap();
        assert!(acceptor.is_some(), "valid cert/key must yield an acceptor");
    }

    /// A self-signed test-only certificate (do not use for production secrets).
    const TEST_CERT: &str = "-----BEGIN CERTIFICATE-----\n\
MIIDCTCCAfGgAwIBAgIUPxZ8dsRScrPu7PKq3bp6FgtvcKowDQYJKoZIhvcNAQEL\n\
BQAwFDESMBAGA1UEAwwJbG9jYWxob3N0MB4XDTI2MDgzMTEwMDMzOFoXDTI3MDgz\n\
MTEwMDMzOFowFDESMBAGA1UEAwwJbG9jYWxob3N0MIIBIjANBgkqhkiG9w0BAQEF\n\
AAOCAQ8AMIIBCgKCAQEAnLeYNp86TdIdGHDVbxJwzHcfH9eeeRCBLnP7sJdYRzLc\n\
BXNeXpjbU/DxzJSqsaULjVaIPdRzAloGwWRnlwYLmR0md1kEsuzz89drgErjpWaD\n\
1iRTa/vSDnc4GEjHGAA8+Y3JnBYEhoH3X4PhSX+Aav+OFxCUYUWwpn1KpxJ9JU0a\n\
qeUhQuCLQUnC1ACtcGZ/6NfQryr97NLYMgQFj75EmTsDCfgCBmxgbsNxLNMbE738\n\
JtsYQbekDihSB3xWBLTsylHaA64YEOzc6H2SyIInzICxa/tmUvgzpXGTgGKjOgur\n\
H5Kamzt/5kdutLK1/AEIQoUixUkDc/PtdXkpMHw7EQIDAQABo1MwUTAdBgNVHQ4E\n\
FgQUC/Cb+aYLYdjmN8Kkw6xnIP8+TsswHwYDVR0jBBgwFoAUC/Cb+aYLYdjmN8Kk\n\
w6xnIP8+TsswDwYDVR0TAQH/BAUwAwEB/zANBgkqhkiG9w0BAQsFAAOCAQEAI7ZQ\n\
v9TbBhMPQH5ExMr30QIa4i/GkSD/0JtqxiJkrET5pN9gC1L0IdLZ++WdSv8xAuCD\n\
28GOPc9f7xmTIzDPYCMlwVJVYCGvC2m2bembJPyBfD0z0nKHk3EjK4H2ZmEUf4y3\n\
PxR6xY+DUGM8mWoa6UyvuRexg/Xl9kL26sb4XhurK+U+PaCNYGe2xjyGqc8H9VgW\n\
99V5fvM1FgTTqw0afHorkBZvqGpaVvKmm7tjyT8gx1o0ecqSe+dY4LLT7amY2rS0\n\
2t47Lg8TYwMsjkhoNcPLr+q+A6kbDmpFjpX4kvS7XHCooqREc2VHEEMflXy3Fnvm\n\
8JDG+akBaT3Ai5zlCw==\n\
-----END CERTIFICATE-----\n";

    /// A self-signed test-only private key (do not use for production secrets).
    const TEST_KEY: &str = "-----BEGIN PRIVATE KEY-----\n\
MIIEvAIBADANBgkqhkiG9w0BAQEFAASCBKYwggSiAgEAAoIBAQCct5g2nzpN0h0Y\n\
cNVvEnDMdx8f1555EIEuc/uwl1hHMtwFc15emNtT8PHMlKqxpQuNVog91HMCWgbB\n\
ZGeXBguZHSZ3WQSy7PPz12uASuOlZoPWJFNr+9IOdzgYSMcYADz5jcmcFgSGgfdf\n\
g+FJf4Bq/44XEJRhRbCmfUqnEn0lTRqp5SFC4ItBScLUAK1wZn/o19CvKv3s0tgy\n\
BAWPvkSZOwMJ+AIGbGBuw3Es0xsTvfwm2xhBt6QOKFIHfFYEtOzKUdoDrhgQ7Nzo\n\
fZLIgifMgLFr+2ZS+DOlcZOAYqM6C6sfkpqbO3/mR260srX8AQhChSLFSQNz8+11\n\
eSkwfDsRAgMBAAECggEAJ0iuUyLezpsYyAOgvNL2i4pgtu6pvtcwSqCwOrf1XQOW\n\
u5cL1NKkSAph0lKB5z3kA23pgPY8Th6bCudMQEM3rQ3tkoUx9FgJXtplDCe5oMBt\n\
08QPVUYuhYnE+fFkVtPYdQXhv8qVH9J8W+kHFBFt82RUDdwOFcQOX+2QRQkRbcPd\n\
uloDfRMFgVzCdrTsIvS7BygLp40gaaCYmvIKBGKD9lBV8DKFliiy6/BJvo8Z9+ic\n\
IBvGyxepgzvGOQN1FjXi/5hhJIiCnp6cpoZfJlaGC9w4I/KbrgLbI8QftvSUAOFy\n\
z1Djd5DMQBYAznxIFbsGxh5t7Z5lxpRsB3O/mcLy8QKBgQDKpVRguBk+ckYm1Sjy\n\
dpxUoLQuSmSsFf7bPU/f0Y1RgfoKpEd6Cv8jd/DvlccH9xFySm/CwG8ADbR2FB0J\n\
RpzDaImNF6AV5R1PEugYwC04AxrVxjox+ivgHNKnQoEVd1QkwiqZ6xg/GAodM1eB\n\
ESkvIekZhB5DNOP2pf4wOEVs4wKBgQDF+pijwjclmHASH8ONeY7cHwOdKfudV+c2\n\
zsOnO3iwYcyHcTbEjw6ZtINBV9QncqcddBN2T0E2sAYBpk2QiNDlin7MgLE1XBlt\n\
9vJzagsRCQEQ8HLbs0P8qdsFAIqNvOW32FSy09mkJmewzeI9zLAiaaUC5ViTKiWN\n\
iFzBiEmOewKBgGxu4yON3xQnGZqV3P9AsI4oH8HVVOEwM9skh6UAAFpo7l7bYNPR\n\
JozYFTheMM32SoOZiQvw5HRm4PV99buM6T02psO0rJiKrJAvUbpMuuWJ48YX9/Pe\n\
JbQaOC3/zAqse33f1+PchHDecCsH2f7aK+tofc6Ff5v+pSzJzaYHtj55AoGAZgC1\n\
QDpCe4ZMx6nB8VRd/J+mFwWYc/rkT+K7/5+ukQHyhR4Zn7AtT5gnwDTmQ+TYoV46\n\
4Mv4x5ptnc/3Sq6TIpD2v5rWsq1fFL8VL83FIePHvtiD9RopvzYseClNObXHja9S\n\
BEkOa3q2Fewd0sVxQmm38QQFXN1sN724PKZhb50CgYB0UwbszoEnoxkV/GY9kJmZ\n\
3n6mnafh6sJKD7/TseKsL0Z7N1/Yjks6xtgafV37mreXpTyxbqcH8GJeyDz/EG4p\n\
qK57RiqkzW3rwnkAlE265qxQMbsNHlDCMca3UgJjQUIbdQ1nWV/RpHf4I+cC6NJX\n\
s/dv5yLIxbY+S/qlUKRnuw==\n\
-----END PRIVATE KEY-----\n";
}
