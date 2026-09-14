//! SEC-002 regression tests for public MCP HTTP/SSE binds.
//!
//! The security boundary is intentionally tested at the policy helper and at
//! the `serve()` startup boundary: a rejected plaintext public bind must fail
//! before a TCP listener is created.

use agent_workspace_hub::mcp::{serve, validate_sec_002_policy, HttpServerConfig, McpDispatcher, TlsConfig};
use std::sync::Arc;
use tempfile::tempdir;

fn plaintext_config(host: &str) -> HttpServerConfig {
    HttpServerConfig {
        host: host.to_string(),
        port: 0,
        tls: TlsConfig::default(),
        api_key: "test-secret".to_string(),
        ..Default::default()
    }
}

#[test]
fn loopback_ipv4_plaintext_is_allowed() {
    assert!(validate_sec_002_policy("127.0.0.1", false).is_ok());
    assert!(validate_sec_002_policy("127.0.0.2", false).is_ok());
}

#[test]
fn loopback_ipv6_plaintext_is_allowed() {
    assert!(validate_sec_002_policy("::1", false).is_ok());
}

#[test]
fn wildcard_ipv4_plaintext_is_rejected() {
    let error = validate_sec_002_policy("0.0.0.0", false).unwrap_err();
    assert!(error.to_string().contains("TLS is required for non-loopback"));
}

#[test]
fn wildcard_ipv6_plaintext_is_rejected() {
    let error = validate_sec_002_policy("::", false).unwrap_err();
    assert!(error.to_string().contains("TLS is required for non-loopback"));
}

#[test]
fn non_loopback_ipv4_and_ipv6_plaintext_are_rejected() {
    assert!(validate_sec_002_policy("192.0.2.1", false).is_err());
    assert!(validate_sec_002_policy("2001:db8::1", false).is_err());
}

#[test]
fn unknown_hostname_fails_closed() {
    assert!(validate_sec_002_policy("mcp.example.invalid", false).is_err());
}

#[test]
fn localhost_is_allowed_only_when_resolution_is_entirely_loopback() {
    let result = validate_sec_002_policy("localhost", false);
    assert!(result.is_ok(), "localhost must resolve entirely to loopback on a conforming host");
}

#[test]
fn tls_enabled_allows_non_loopback_policy() {
    assert!(validate_sec_002_policy("0.0.0.0", true).is_ok());
    assert!(validate_sec_002_policy("192.0.2.1", true).is_ok());
    assert!(validate_sec_002_policy("2001:db8::1", true).is_ok());
}

#[tokio::test]
async fn serve_rejects_public_plaintext_before_binding() {
    let dir = tempdir().expect("tempdir");
    let dispatcher = Arc::new(
        McpDispatcher::new_async(dir.path().to_path_buf())
            .await
            .expect("dispatcher");
    );

    let error = serve(plaintext_config("0.0.0.0"), dispatcher)
        .await
        .expect_err("public plaintext MCP must be rejected before bind");

    let message = error.to_string();
    assert!(message.contains("TLS is required for non-loopback"));
    assert!(message.contains("SEC-002") || message.contains("plaintext"));
}

#[tokio::test]
async fn serve_rejects_non_loopback_ipv4_before_binding() {
    let dir = tempdir().expect("tempdir");
    let dispatcher = Arc::new(
        McpDispatcher::new_async(dir.path().to_path_buf())
            .await
            .expect("dispatcher");
    );

    let error = serve(plaintext_config("192.0.2.1"), dispatcher)
        .await
        .expect_err("non-loopback plaintext MCP must be rejected before bind");

    assert!(error.to_string().contains("TLS is required for non-loopback"));
}

#[test]
fn invalid_tls_material_is_not_accepted_by_tls_validation() {
    let config = TlsConfig {
        cert: Some("/definitely/missing/sec-002-cert.pem".to_string()),
        key: Some("/definitely/missing/sec-002-key.pem".to_string()),
    };
    assert!(config.validate().is_err());
}
