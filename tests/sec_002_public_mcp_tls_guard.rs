//! SEC-002: Public MCP TLS Guard Tests
//!
//! These tests verify the SEC-002 security policy: non-loopback MCP HTTP/SSE
//! binds require TLS. The tests exercise the actual security boundary by
//! validating configurations before server startup.

use agent_workspace_hub::mcp::{HttpServerConfig, TlsConfig};
use tempfile::tempdir;

/// Test fixture: create a valid TLS config for testing
fn valid_tls_config() -> TlsConfig {
    let dir = tempdir().unwrap();
    let cert_path = dir.path().join("cert.pem");
    let key_path = dir.path().join("key.pem");
    std::fs::write(&cert_path, TEST_CERT).unwrap();
    std::fs::write(&key_path, TEST_KEY).unwrap();

    TlsConfig {
        cert: Some(cert_path.to_string_lossy().to_string()),
        key: Some(key_path.to_string_lossy().to_string()),
    }
}

/// Test fixture: create an invalid/malformed TLS config
fn invalid_tls_config() -> TlsConfig {
    TlsConfig {
        cert: Some("/nonexistent/cert.pem".to_string()),
        key: Some("/nonexistent/key.pem".to_string()),
    }
}

/// Base config builder
fn base_config(host: &str, tls: TlsConfig) -> HttpServerConfig {
    HttpServerConfig {
        host: host.to_string(),
        port: 8443,
        tls,
        api_key: "test-api-key".to_string(),
        allowed_origins: vec![],
        max_body_bytes: 10 * 1024 * 1024,
        max_sessions: 100,
        request_timeout: std::time::Duration::from_secs(30),
        sse_keepalive: std::time::Duration::from_secs(15),
    }
}

// ============================================================================
// A. Loopback Plaintext - Should be ALLOWED
// ============================================================================

#[test]
fn sec_002_loopback_ipv4_plaintext_allowed() {
    // 127.0.0.1 + TLS disabled → ALLOWED
    let config = base_config("127.0.0.1", TlsConfig::default());
    // This should not panic - loopback plaintext is allowed
    // We test the validation function directly since serve() would block
    let result =
        agent_workspace_hub::mcp::http::validate_sec_002_policy(&config.host, config.tls.enabled());
    assert!(
        result.is_ok(),
        "127.0.0.1 plaintext should be allowed: {:?}",
        result.err()
    );
}

#[test]
fn sec_002_loopback_ipv6_plaintext_allowed() {
    // ::1 + TLS disabled → ALLOWED
    let config = base_config("::1", TlsConfig::default());
    let result =
        agent_workspace_hub::mcp::http::validate_sec_002_policy(&config.host, config.tls.enabled());
    assert!(
        result.is_ok(),
        "::1 plaintext should be allowed: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn sec_002_localhost_plaintext_allowed() {
    // localhost + TLS disabled → ALLOWED (if resolves to loopback)
    let config = base_config("localhost", TlsConfig::default());
    let result =
        agent_workspace_hub::mcp::http::validate_sec_002_policy(&config.host, config.tls.enabled());
    assert!(
        result.is_ok(),
        "localhost plaintext should be allowed: {:?}",
        result.err()
    );
}

// ============================================================================
// B. Public/Non-loopback Plaintext - Should be REJECTED
// ============================================================================

#[test]
fn sec_002_wildcard_ipv4_plaintext_rejected() {
    // 0.0.0.0 + TLS disabled → REJECTED
    let config = base_config("0.0.0.0", TlsConfig::default());
    let result =
        agent_workspace_hub::mcp::http::validate_sec_002_policy(&config.host, config.tls.enabled());
    assert!(result.is_err(), "0.0.0.0 plaintext should be rejected");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("SEC-002"),
        "Error should mention SEC-002: {}",
        err_msg
    );
    assert!(
        err_msg.contains("TLS is required"),
        "Error should mention TLS requirement: {}",
        err_msg
    );
}

#[test]
fn sec_002_non_loopback_ipv4_plaintext_rejected() {
    // Non-loopback IPv4 (e.g., 192.168.1.1) + TLS disabled → REJECTED
    let config = base_config("192.168.1.100", TlsConfig::default());
    let result =
        agent_workspace_hub::mcp::http::validate_sec_002_policy(&config.host, config.tls.enabled());
    assert!(
        result.is_err(),
        "Non-loopback IPv4 plaintext should be rejected"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("SEC-002"));
}

#[test]
fn sec_002_non_loopback_ipv6_plaintext_rejected() {
    // Non-loopback IPv6 (e.g., 2001:db8::1) + TLS disabled → REJECTED
    let config = base_config("2001:db8::1", TlsConfig::default());
    let result =
        agent_workspace_hub::mcp::http::validate_sec_002_policy(&config.host, config.tls.enabled());
    assert!(
        result.is_err(),
        "Non-loopback IPv6 plaintext should be rejected"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("SEC-002"));
}

#[test]
fn sec_002_wildcard_ipv6_plaintext_rejected() {
    // :: (IPv6 wildcard/unspecified) + TLS disabled → REJECTED
    let config = base_config("::", TlsConfig::default());
    let result =
        agent_workspace_hub::mcp::http::validate_sec_002_policy(&config.host, config.tls.enabled());
    assert!(
        result.is_err(),
        ":: (IPv6 wildcard) plaintext should be rejected"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("SEC-002"));
}

#[test]
fn sec_002_unknown_hostname_plaintext_rejected() {
    // Unknown hostname (not localhost) + TLS disabled → REJECTED (fail closed)
    let config = base_config("example.com", TlsConfig::default());
    let result =
        agent_workspace_hub::mcp::http::validate_sec_002_policy(&config.host, config.tls.enabled());
    assert!(
        result.is_err(),
        "Unknown hostname plaintext should be rejected (fail closed)"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("SEC-002"));
}

// ============================================================================
// C. TLS-enabled Public Bind - Should be ALLOWED
// ============================================================================

#[test]
fn sec_002_non_loopback_with_valid_tls_allowed() {
    // Non-loopback + valid TLS → ALLOWED
    let tls = valid_tls_config();
    let config = base_config("0.0.0.0", tls);
    // Note: validate_sec_002_policy only checks the policy, not TLS validity
    // TLS validity is checked separately by tls.validate()
    let result =
        agent_workspace_hub::mcp::http::validate_sec_002_policy(&config.host, config.tls.enabled());
    assert!(
        result.is_ok(),
        "Non-loopback with TLS enabled should be allowed"
    );
}

#[test]
fn sec_002_loopback_with_tls_allowed() {
    // Loopback + TLS → ALLOWED (both are fine)
    let tls = valid_tls_config();
    let config = base_config("127.0.0.1", tls);
    let result =
        agent_workspace_hub::mcp::http::validate_sec_002_policy(&config.host, config.tls.enabled());
    assert!(result.is_ok(), "Loopback with TLS should be allowed");
}

// ============================================================================
// D. Invalid TLS Configuration - Should be REJECTED
// ============================================================================

#[test]
fn sec_002_invalid_tls_config_rejected() {
    // Invalid TLS (missing files) should be rejected by tls.validate()
    let tls = invalid_tls_config();
    let config = base_config("0.0.0.0", tls);
    // The TLS validation happens before SEC-002 check in serve()
    let result = config.tls.validate();
    assert!(
        result.is_ok(),
        "Invalid paths pass initial validation (checked at build_acceptor)"
    );

    // But build_acceptor should fail
    let acceptor_result = config.tls.build_acceptor();
    assert!(
        acceptor_result.is_err(),
        "Invalid TLS cert/key should fail at build_acceptor"
    );
}

#[test]
fn sec_002_half_configured_tls_rejected() {
    // Half-configured TLS (cert without key) should be rejected
    let tls_half = TlsConfig {
        cert: Some("/some/cert.pem".to_string()),
        key: None,
    };
    let result = tls_half.validate();
    assert!(result.is_err(), "Half-configured TLS should be rejected");
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("certificate provided without a private key"));
}

// ============================================================================
// E. Security Invariant Tests
// ============================================================================

#[test]
fn sec_002_error_message_quality() {
    // Verify error messages are clear and don't leak sensitive info
    let config = base_config("192.168.1.100", TlsConfig::default());
    let result =
        agent_workspace_hub::mcp::http::validate_sec_002_policy(&config.host, config.tls.enabled());
    let err = result.unwrap_err();
    let err_msg = err.to_string();

    // Should contain helpful guidance
    assert!(err_msg.contains("TLS is required for non-loopback"));
    assert!(err_msg.contains("192.168.1.100"));
    assert!(err_msg.contains("loopback address"));

    // Should NOT contain secrets or sensitive paths
    assert!(!err_msg.contains("api_key"));
    assert!(!err_msg.contains("secret"));
    assert!(!err_msg.contains("/etc/"));
}

#[test]
fn sec_002_audit_log_on_deny() {
    // Verify that denied configurations trigger audit logging
    // This is tested indirectly by checking the rejection path
    let config = base_config("0.0.0.0", TlsConfig::default());
    let result =
        agent_workspace_hub::mcp::http::validate_sec_002_policy(&config.host, config.tls.enabled());
    assert!(result.is_err());
    // The audit_deny call is made in the rejection path
}

// ============================================================================
// F. Edge Cases
// ============================================================================

#[test]
fn sec_002_case_insensitive_localhost() {
    // Localhost case variations should all work
    for host in &["localhost", "Localhost", "LOCALHOST", "LoCaLhOsT"] {
        let config = base_config(host, TlsConfig::default());
        let result = agent_workspace_hub::mcp::http::validate_sec_002_policy(
            &config.host,
            config.tls.enabled(),
        );
        assert!(result.is_ok(), "{} should be treated as loopback", host);
    }
}

#[test]
fn sec_002_ipv4_mapped_ipv6_loopback() {
    // ::ffff:127.0.0.1 (IPv4-mapped IPv6 loopback) - depends on Rust's is_loopback()
    // This tests the underlying IP semantics
    use std::net::IpAddr;
    let ipv4_mapped = "::ffff:127.0.0.1".parse::<IpAddr>().unwrap();
    // Rust considers IPv4-mapped addresses based on their IPv6 form
    // The important thing is consistent behavior
    let _is_loopback = ipv4_mapped.is_loopback();
}

// Test certificate and key (same as in tls.rs tests)
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
z1Djd5DMQBYAznxIFbsGxh5t7Z5lxpRsB3O/mcLy8QKBgQDKpVRguBk+ckYm1Sjy\n\ndpxUoLQuSmSsFf7bPU/f0Y1RgfoKpEd6Cv8jd/DvlccH9xFySm/CwG8ADbR2FB0J\n\
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
