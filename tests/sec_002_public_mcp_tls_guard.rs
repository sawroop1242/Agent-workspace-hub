//! SEC-002 regression tests for public MCP HTTP/SSE binds.
//!
//! The security boundary is tested at three layers:
//! 1. the centralized policy helper (`validate_sec_002_policy`);
//! 2. the `serve()` startup boundary — every rejected combination must
//!    fail *before* a TCP listener exists (proven by holding the target
//!    port: an `AddrInUse` would mean the bind ran first);
//! 3. the allow path — a non-loopback bind with valid TLS must actually
//!    serve, proven by an end-to-end TLS round trip.

use agent_workspace_hub::mcp::{
    serve, validate_sec_002_policy, HttpServerConfig, McpDispatcher, TlsConfig,
};
use rcgen::CertifiedKey;
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
fn wildcard_ipv4_plaintext_is_rejected_with_actionable_guidance() {
    let error = validate_sec_002_policy("0.0.0.0", false).unwrap_err();
    let message = error.to_string();
    assert!(
        message.contains("TLS is required for non-loopback"),
        "must state the violation: {message}"
    );
    // SEC-002 acceptance: the error must explain how to fix it — configure
    // TLS or move to a loopback bind.
    assert!(
        message.contains("Enable TLS"),
        "must explain the TLS option: {message}"
    );
    assert!(
        message.contains("loopback address (127.0.0.1, ::1)"),
        "must name concrete loopback addresses: {message}"
    );
    assert!(
        message.contains("0.0.0.0"),
        "must name the offending bind address: {message}"
    );
}

#[test]
fn wildcard_ipv6_plaintext_is_rejected_with_actionable_guidance() {
    let error = validate_sec_002_policy("::", false).unwrap_err();
    let message = error.to_string();
    assert!(
        message.contains("TLS is required for non-loopback"),
        "must state the violation: {message}"
    );
    assert!(
        message.contains("Enable TLS"),
        "must explain the TLS option: {message}"
    );
    assert!(
        message.contains("loopback address (127.0.0.1, ::1)"),
        "must name concrete loopback addresses: {message}"
    );
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
    assert!(
        result.is_ok(),
        "localhost must resolve entirely to loopback on a conforming host"
    );
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
            .expect("dispatcher"),
    );

    let error = serve(plaintext_config("0.0.0.0"), dispatcher)
        .await
        .expect_err("public plaintext MCP must be rejected before bind");

    let message = error.to_string();
    assert!(message.contains("TLS is required for non-loopback"));
    assert!(message.contains("SEC-002") || message.contains("plaintext"));
    // The actionable guidance must survive to the serve() boundary.
    assert!(message.contains("Enable TLS"));
    assert!(message.contains("loopback address (127.0.0.1, ::1)"));
}

#[tokio::test]
async fn serve_rejects_non_loopback_ipv4_before_binding() {
    let dir = tempdir().expect("tempdir");
    let dispatcher = Arc::new(
        McpDispatcher::new_async(dir.path().to_path_buf())
            .await
            .expect("dispatcher"),
    );

    let error = serve(plaintext_config("192.0.2.1"), dispatcher)
        .await
        .expect_err("non-loopback plaintext MCP must be rejected before bind");

    assert!(error
        .to_string()
        .contains("TLS is required for non-loopback"));
}

/// Reserves an ephemeral port by binding it, and returns it still bound
/// (plus the listener that owns it). While the returned listener is held,
/// a `serve()` call that reaches its bind step fails with `AddrInUse` —
/// so an error *other* than a bind failure proves `serve()` rejected the
/// configuration before ever attempting to bind.
fn held_public_port() -> (u16, std::net::TcpListener) {
    let listener = std::net::TcpListener::bind(("0.0.0.0", 0)).expect("reserve port");
    let port = listener.local_addr().expect("local addr").port();
    (port, listener)
}

/// Proves the plaintext rejection happens *before* the bind attempt: the
/// port is held by this test, so if `serve()` had reached `bind` the error
/// would be `failed to bind ... AddrInUse`. Receiving the SEC-002 policy
/// error instead is the no-bind proof.
#[tokio::test]
async fn serve_rejects_public_plaintext_without_attempting_the_bind() {
    let (port, _held) = held_public_port();
    let dir = tempdir().expect("tempdir");
    let dispatcher = Arc::new(
        McpDispatcher::new_async(dir.path().to_path_buf())
            .await
            .expect("dispatcher"),
    );

    let mut config = plaintext_config("0.0.0.0");
    config.port = port;

    let error = serve(config, dispatcher)
        .await
        .expect_err("public plaintext must be rejected");

    let message = error.to_string();
    assert!(
        message.contains("TLS is required for non-loopback"),
        "must be the SEC-002 rejection, got: {message}"
    );
    assert!(
        !message.contains("failed to bind") && !message.contains("AddrInUse"),
        "the rejection must occur before any bind attempt, got: {message}"
    );
}

/// A non-loopback bind with TLS *enabled* but unusable material must fail
/// clearly — and (SEC-002 hardening) before any socket is bound. The port
/// is held by this test: an `AddrInUse` would mean the bind ran first.
#[tokio::test]
async fn serve_rejects_invalid_tls_material_before_binding() {
    let (port, _held) = held_public_port();
    let dir = tempdir().expect("tempdir");
    let cert = dir.path().join("cert.pem");
    let key = dir.path().join("key.pem");
    std::fs::write(&cert, "not a certificate").expect("write cert");
    std::fs::write(&key, "not a key").expect("write key");

    let dispatcher = Arc::new(
        McpDispatcher::new_async(dir.path().to_path_buf())
            .await
            .expect("dispatcher"),
    );

    let config = HttpServerConfig {
        host: "0.0.0.0".to_string(),
        port,
        tls: TlsConfig {
            cert: Some(cert.to_string_lossy().into_owned()),
            key: Some(key.to_string_lossy().into_owned()),
        },
        api_key: "test-secret".to_string(),
        ..Default::default()
    };

    let error = serve(config, dispatcher)
        .await
        .expect_err("invalid TLS material must be rejected");

    let message = error.to_string();
    // The error must be the TLS failure, not a bind failure.
    assert!(
        message.contains("failed to parse TLS certificate")
            || message.contains("TLS certificate file")
            || message.contains("contained no certificates"),
        "must fail clearly on the TLS material, got: {message}"
    );
    assert!(
        !message.contains("failed to bind") && !message.contains("AddrInUse"),
        "invalid TLS must fail before the bind attempt, got: {message}"
    );
}

/// A half-configured TLS setup (cert without key) on a non-loopback bind
/// is rejected by `validate()` with a clear, actionable message naming the
/// missing environment variable — before any bind attempt.
#[tokio::test]
async fn serve_rejects_half_configured_tls_with_actionable_error() {
    let (port, _held) = held_public_port();
    let dir = tempdir().expect("tempdir");

    let dispatcher = Arc::new(
        McpDispatcher::new_async(dir.path().to_path_buf())
            .await
            .expect("dispatcher"),
    );

    let config = HttpServerConfig {
        host: "0.0.0.0".to_string(),
        port,
        tls: TlsConfig {
            cert: Some("/tmp/sec-002-cert.pem".to_string()),
            key: None,
        },
        api_key: "test-secret".to_string(),
        ..Default::default()
    };

    let error = serve(config, dispatcher)
        .await
        .expect_err("half-configured TLS must be rejected");

    let message = error.to_string();
    assert!(
        message.contains("certificate provided without a private key"),
        "must explain the half-configured state, got: {message}"
    );
    assert!(
        message.contains("AWH_TLS_KEY"),
        "must name the missing variable, got: {message}"
    );
}

/// Generates a self-signed certificate (and matching key) valid for
/// loopback clients, using the same rustls toolchain the server parses.
fn generate_loopback_cert(dir: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let CertifiedKey { cert, key_pair } =
        rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_string(), "localhost".to_string()])
            .expect("generate test certificate");
    let cert_path = dir.join("valid-cert.pem");
    let key_path = dir.join("valid-key.pem");
    std::fs::write(&cert_path, cert.pem()).expect("write cert pem");
    std::fs::write(&key_path, key_pair.serialize_pem()).expect("write key pem");
    (cert_path, key_path)
}

/// The allow cell of the SEC-002 matrix at the real serve() boundary: a
/// NON-LOOPBACK bind (`0.0.0.0`) with valid TLS material must actually
/// serve. Proven end-to-end: a rustls client that trusts only the test
/// certificate completes the TLS handshake and receives an HTTP response
/// from the MCP health route.
#[tokio::test]
async fn serve_starts_on_non_loopback_with_valid_tls_and_serves_https() {
    let dir = tempdir().expect("tempdir");
    let (cert_path, key_path) = generate_loopback_cert(dir.path());

    // Reserve a port on loopback, then release it for serve() to take.
    let probe = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("reserve port");
    let port = probe.local_addr().expect("local addr").port();
    drop(probe);

    let dispatcher = Arc::new(
        McpDispatcher::new_async(dir.path().to_path_buf())
            .await
            .expect("dispatcher"),
    );

    let config = HttpServerConfig {
        host: "0.0.0.0".to_string(),
        port,
        tls: TlsConfig {
            cert: Some(cert_path.to_string_lossy().into_owned()),
            key: Some(key_path.to_string_lossy().into_owned()),
        },
        api_key: "test-secret".to_string(),
        ..Default::default()
    };

    let server = tokio::spawn(serve(config, dispatcher));

    // Trust only the generated certificate.
    let mut roots = rustls::RootCertStore::empty();
    let cert_pem = std::fs::read(&cert_path).expect("read cert pem");
    let mut chain = rustls_pemfile::certs(&mut cert_pem.as_slice())
        .collect::<Result<Vec<_>, _>>()
        .expect("parse cert pem");
    for cert in chain.drain(..) {
        roots.add(cert).expect("add root");
    }
    let client_config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(client_config));

    // Give serve() a moment to bind, then complete a full TLS + HTTP
    // round trip against the health route.
    let response = tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            if let Ok(tcp) = tokio::net::TcpStream::connect(("127.0.0.1", port)).await {
                let name =
                    rustls::pki_types::ServerName::from(std::net::IpAddr::from([127u8, 0, 0, 1]));
                let mut tls = connector.connect(name, tcp).await.expect("TLS handshake");
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                tls.write_all(
                    b"GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
                )
                .await
                .expect("send request");
                let mut buf = Vec::new();
                tls.read_to_end(&mut buf).await.expect("read response");
                let text = String::from_utf8_lossy(&buf);
                if !text.is_empty() {
                    return text.into_owned();
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("TLS round trip within timeout");

    server.abort();

    assert!(
        response.starts_with("HTTP/1.1 200"),
        "valid TLS on a non-loopback bind must serve: {response}"
    );
    assert!(
        response.contains("\"status\":\"ok\""),
        "the health route must answer over TLS: {response}"
    );
}

#[test]
fn invalid_tls_material_is_not_accepted_by_tls_validation() {
    let config = TlsConfig {
        cert: Some("/definitely/missing/sec-002-cert.pem".to_string()),
        key: Some("/definitely/missing/sec-002-key.pem".to_string()),
    };
    // `validate()` checks cert/key pairing only; the TLS material itself is
    // rejected at the `build_acceptor()` boundary that `serve()` runs
    // before binding any socket — invalid TLS material fails closed
    // during startup without ever creating a listener.
    assert!(config.build_acceptor().is_err());
}
