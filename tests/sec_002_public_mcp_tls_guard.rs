//! SEC-002: Public MCP TLS Guard — refuse non-loopback binds without valid TLS.
//!
//! Contract:
//! - loopback + plaintext: allowed for local development
//! - non-loopback + no TLS: reject before serving
//! - non-loopback + invalid TLS: reject with actionable error
//! - non-loopback + valid TLS: serve
//!
//! These tests verify the startup rejection logic in `mcp::http::serve`.

use agent_workspace_hub::mcp::{HttpServerConfig, TlsConfig};

use tempfile::tempdir;

/// Test certificate and key (self-signed, for testing only).
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

const TEST_KEY: &str = "-----BEGIN PRIVATE KEY-----\n\
MIIEvAIBADANBgkqhkiG9w0BAQEFAASCBKYwggSiAgEAAoIBAQCct5g2nzpN0h0Y\n\
cNVvEnDMdx8f1555EIEuc/uwl1hHMtwFc15emNtT8PHMlKqxpQuNVog91HMCWgbB\n\
ZGeXBguZHSZ3WQSy7PPz12uASuOlZoPWJFNr+9IOdzgYSMcYADz5jcmcFgSGgfdf\n\
g+FJf4Bq/44XEJRhRbCmfUqnEn0lTRqp5SFC4ItBScLUAK1wZn/o19CvKv3s0tgy\n\
BAWPvkSZOwMJ+AIGbGBuw3Es0xsTvfwm2xhBt6QOKFIHfFYEtOzKUdoDrhgQ7Nzo\n\
fZLIgifMgLFr+2ZS+DOlcZOAYqM6C6sfkpqbO3/mR260srX8AQhChSLFSQNz8+11\n\neSkwfDsRAgMBAAECggEAJ0iuUyLezpsYyAOgvNL2i4pgtu6pvtcwSqCwOrf1XQOW\n\
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
JozYFTheMM32SoOZiQvw5HRm4PV99buM6T02psO0rJrKrJAvUbpMuuWJ48YX9/Pe\n\
JbQaOC3/zAqse33f1+PchHDecCsH2f7aK+tofc6Ff5v+pSzJzaYHtj55AoGAZgC1\n\
QDpCe4ZMx6nB8VRd/J+mFwWYc/rkT+K7/5+ukQHyhR4Zn7AtT5gnwDTmQ+TYoV46\n\
4Mv4x5ptnc/3Sq6TIpD2v5rWsq1fFL8VL83FIePHvtiD9RopvzYseClNObXHja9S\n\
BEkOa3q2Fewd0sVxQmm38QQFXN1sN724PKZhb50CgYB0UwbszoEnoxkV/GY9kJmZ\n\
3n6mnafh6sJKD7/TseKsL0Z7N1/Yjks6xtgafV37mreXpTyxbqcH8GJeyDz/EG4p\n\
qK57RiqkzW3rwnkAlE265qxQMbsNHlDCMca3UgJjQUIbdQ1nWV/RpHf4I+cC6NJX\n\
s/dv5yLIxbY+S/qlUKRnuw==\n\
-----END PRIVATE KEY-----\n";

fn write_test_certs(dir: &tempfile::TempDir) -> (String, String) {
    let cert_path = dir.path().join("cert.pem");
    let key_path = dir.path().join("key.pem");
    std::fs::write(&cert_path, TEST_CERT).unwrap();
    std::fs::write(&key_path, TEST_KEY).unwrap();
    (
        cert_path.to_string_lossy().to_string(),
        key_path.to_string_lossy().to_string(),
    )
}

#[test]
fn loopback_plaintext_is_allowed() {
    // Loopback + plaintext must be allowed for local development.
    let config = HttpServerConfig {
        host: "127.0.0.1".to_string(),
        port: 0,
        tls: TlsConfig {
            cert: None,
            key: None,
        },
        api_key: "test-key".to_string(),
        ..HttpServerConfig::default()
    };
    // Validation should pass (no TLS error for loopback).
    // The actual bind/serve is tested in integration tests; here we just
    // verify the config validation doesn't reject loopback+plaintext.
    assert!(config.tls.validate().is_ok());
    assert!(!config.tls.enabled());
}

#[test]
fn localhost_plaintext_is_allowed() {
    let config = HttpServerConfig {
        host: "localhost".to_string(),
        port: 0,
        tls: TlsConfig {
            cert: None,
            key: None,
        },
        api_key: "test-key".to_string(),
        ..HttpServerConfig::default()
    };
    assert!(config.tls.validate().is_ok());
    assert!(!config.tls.enabled());
}

#[test]
fn ipv6_loopback_plaintext_is_allowed() {
    let config = HttpServerConfig {
        host: "::1".to_string(),
        port: 0,
        tls: TlsConfig {
            cert: None,
            key: None,
        },
        api_key: "test-key".to_string(),
        ..HttpServerConfig::default()
    };
    assert!(config.tls.validate().is_ok());
    assert!(!config.tls.enabled());
}

#[test]
fn non_loopback_without_tls_is_rejected() {
    // Non-loopback (0.0.0.0) without TLS must be rejected.
    let config = HttpServerConfig {
        host: "0.0.0.0".to_string(),
        port: 8443,
        tls: TlsConfig {
            cert: None,
            key: None,
        },
        api_key: "test-key".to_string(),
        ..HttpServerConfig::default()
    };
    // TLS validation passes (none configured is valid), but the serve
    // function must reject non-loopback + no-TLS.
    assert!(config.tls.validate().is_ok());
    assert!(!config.tls.enabled());
    // The rejection happens in serve(), not in validate().
    // This test documents the expected behavior; integration test verifies
    // the actual serve() rejection.
}

#[test]
fn non_loopback_with_valid_tls_is_allowed() {
    let dir = tempdir().unwrap();
    let (cert_path, key_path) = write_test_certs(&dir);

    let config = HttpServerConfig {
        host: "0.0.0.0".to_string(),
        port: 8443,
        tls: TlsConfig {
            cert: Some(cert_path),
            key: Some(key_path),
        },
        api_key: "test-key".to_string(),
        ..HttpServerConfig::default()
    };
    assert!(config.tls.validate().is_ok());
    assert!(config.tls.enabled());
    assert!(config.tls.build_acceptor().unwrap().is_some());
}

#[test]
fn non_loopback_with_invalid_tls_is_rejected() {
    let dir = tempdir().unwrap();
    let invalid_cert = dir.path().join("invalid.pem");
    let invalid_key = dir.path().join("invalid.key");
    std::fs::write(&invalid_cert, b"not a valid cert").unwrap();
    std::fs::write(&invalid_key, b"not a valid key").unwrap();

    let config = HttpServerConfig {
        host: "0.0.0.0".to_string(),
        port: 8443,
        tls: TlsConfig {
            cert: Some(invalid_cert.to_string_lossy().to_string()),
            key: Some(invalid_key.to_string_lossy().to_string()),
        },
        api_key: "test-key".to_string(),
        ..HttpServerConfig::default()
    };
    // Config validation passes (both cert and key are present),
    // but building the acceptor must fail with invalid material.
    assert!(config.tls.validate().is_ok());
    assert!(config.tls.enabled());
    assert!(config.tls.build_acceptor().is_err());
}

#[tokio::test]
async fn serve_rejects_non_loopback_without_tls() {
    // Integration test: verify that serve() actually rejects non-loopback binds without TLS.
    use agent_workspace_hub::mcp::McpDispatcher;
    use std::sync::Arc;
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let dispatcher = Arc::new(
        McpDispatcher::new_async(dir.path().to_path_buf())
            .await
            .expect("build dispatcher"),
    );

    // Non-loopback (0.0.0.0) without TLS must be rejected by serve().
    let config = HttpServerConfig {
        host: "0.0.0.0".to_string(),
        port: 0, // Use port 0 to avoid conflicts; rejection happens before bind anyway
        tls: TlsConfig {
            cert: None,
            key: None,
        },
        api_key: "test-key".to_string(),
        ..HttpServerConfig::default()
    };

    let result = agent_workspace_hub::mcp::http::serve(config, dispatcher).await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("non-loopback"));
    assert!(err_msg.contains("without TLS"));
}

#[tokio::test]
async fn serve_allows_loopback_without_tls() {
    // Integration test: verify that loopback + plaintext is allowed (but will fail on bind due to port 0).
    use agent_workspace_hub::mcp::McpDispatcher;
    use std::sync::Arc;
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let dispatcher = Arc::new(
        McpDispatcher::new_async(dir.path().to_path_buf())
            .await
            .expect("build dispatcher"),
    );

    // Loopback without TLS should pass the guard (may fail on bind due to port 0 or address in use).
    let config = HttpServerConfig {
        host: "127.0.0.1".to_string(),
        port: 0,
        tls: TlsConfig {
            cert: None,
            key: None,
        },
        api_key: "test-key".to_string(),
        ..HttpServerConfig::default()
    };

    // The serve call will attempt to bind; we expect it to either:
    // 1. Start successfully (if port 0 assigns a free port), or
    // 2. Fail with a bind error (not our TLS guard error).
    // We use tokio::time::timeout to avoid hanging.
    let serve_fut = agent_workspace_hub::mcp::http::serve(config, dispatcher);

    // Give it a short time to start or fail; if it's still running after 100ms,
    // the server started successfully (we cancel it).
    match tokio::time::timeout(std::time::Duration::from_millis(100), serve_fut).await {
        Ok(result) => {
            // Server terminated quickly; check it wasn't our TLS guard error.
            if let Err(e) = result {
                let err_msg = e.to_string();
                // Must NOT be the non-loopback TLS error.
                assert!(
                    !err_msg.contains("non-loopback"),
                    "loopback should not trigger non-loopback TLS guard"
                );
            }
            // If Ok(()), server shut down cleanly (e.g., cancellation).
        }
        Err(_) => {
            // Timeout means server is running; this is success for loopback+plaintext.
            // Test passes by not panicking.
        }
    }
}
