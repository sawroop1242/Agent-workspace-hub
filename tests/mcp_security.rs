use agent_workspace_hub::mcp::{
    authorize_mcp_execution, is_blocked_environment, is_valid_env_name, CustomMcpRegistry,
    CustomMcpServerConfig, McpExecutionRequest, McpPermissions, McpTransport, PersistentTrustStore,
    TrustLevel, TrustStore,
};
use tempfile::tempdir;

fn permissions(
    network: bool,
    process: bool,
    filesystem: &[&str],
    environment: &[&str],
    secrets: &[&str],
) -> McpPermissions {
    McpPermissions {
        network,
        process,
        filesystem: filesystem.iter().map(|s| s.to_string()).collect(),
        environment: environment.iter().map(|s| s.to_string()).collect(),
        secrets: secrets.iter().map(|s| s.to_string()).collect(),
    }
}

#[test]
fn unknown_mcp_is_denied() {
    let trust = TrustStore::default();
    let requested = permissions(false, false, &[], &[], &[]);
    let request = McpExecutionRequest {
        id: "unknown",
        version: "1.0.0",
        permissions: &requested,
    };
    assert!(authorize_mcp_execution(&request, &trust).is_err());
}

#[test]
fn blocked_mcp_is_denied() {
    let mut trust = TrustStore::default();
    trust
        .approve(
            "github",
            TrustLevel::Blocked,
            McpPermissions::default(),
            "1.0.0",
        )
        .unwrap();
    let requested = permissions(false, false, &[], &[], &[]);
    let request = McpExecutionRequest {
        id: "github",
        version: "1.0.0",
        permissions: &requested,
    };
    assert!(authorize_mcp_execution(&request, &trust).is_err());
}

#[test]
fn wrong_version_is_denied() {
    let mut trust = TrustStore::default();
    trust
        .approve(
            "github",
            TrustLevel::Reviewed,
            McpPermissions::default(),
            "1.0.0",
        )
        .unwrap();
    let requested = McpPermissions::default();
    let request = McpExecutionRequest {
        id: "github",
        version: "2.0.0",
        permissions: &requested,
    };
    assert!(authorize_mcp_execution(&request, &trust).is_err());
}

#[test]
fn extra_permission_is_denied() {
    let mut trust = TrustStore::default();
    trust
        .approve(
            "github",
            TrustLevel::Reviewed,
            permissions(false, false, &[], &[], &[]),
            "1.0.0",
        )
        .unwrap();
    let requested = permissions(true, false, &[], &[], &[]);
    let request = McpExecutionRequest {
        id: "github",
        version: "1.0.0",
        permissions: &requested,
    };
    assert!(authorize_mcp_execution(&request, &trust).is_err());
}

#[test]
fn approved_mcp_is_allowed() {
    let mut trust = TrustStore::default();
    let approved = permissions(
        true,
        false,
        &["project"],
        &["GITHUB_TOKEN"],
        &["GITHUB_TOKEN"],
    );
    trust
        .approve("github", TrustLevel::Reviewed, approved.clone(), "1.0.0")
        .unwrap();
    let request = McpExecutionRequest {
        id: "github",
        version: "1.0.0",
        permissions: &approved,
    };
    assert!(authorize_mcp_execution(&request, &trust).is_ok());
}

#[test]
fn revoked_mcp_is_denied() {
    let mut trust = TrustStore::default();
    trust
        .approve(
            "github",
            TrustLevel::Reviewed,
            McpPermissions::default(),
            "1.0.0",
        )
        .unwrap();
    assert!(trust.revoke("github"));
    let requested = McpPermissions::default();
    let request = McpExecutionRequest {
        id: "github",
        version: "1.0.0",
        permissions: &requested,
    };
    assert!(authorize_mcp_execution(&request, &trust).is_err());
}

#[test]
fn persistent_trust_round_trip() {
    let dir = tempdir().unwrap();
    let mut store = PersistentTrustStore::new(dir.path()).unwrap();
    store
        .approve(
            "github",
            TrustLevel::Reviewed,
            McpPermissions::default(),
            "1.0.0",
        )
        .unwrap();
    store.save(dir.path()).unwrap();
    let loaded = PersistentTrustStore::new(dir.path()).unwrap();
    assert_eq!(loaded.approvals.len(), 1);
    assert_eq!(loaded.approvals[0].id, "github");
    assert_eq!(loaded.approvals[0].approved_version, "1.0.0");
}

#[test]
fn corrupted_trust_store_fails_closed() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("trust.json"), "{ not valid json").unwrap();
    // A corrupt trust file must fail to load (surfacing an error) rather than
    // silently defaulting to an empty trust store, which would fail open.
    assert!(PersistentTrustStore::new(dir.path()).is_err());
}

#[test]
fn empty_trust_store_defaults_to_no_approvals() {
    let dir = tempdir().unwrap();
    // A missing trust file is the legitimate "no approvals yet" state.
    let store = PersistentTrustStore::new(dir.path()).unwrap();
    assert!(store.approvals.is_empty());
}

#[test]
fn environment_names_require_safe_identifier_syntax() {
    assert!(is_valid_env_name("GITHUB_TOKEN"));
    assert!(is_valid_env_name("_A1"));
    assert!(!is_valid_env_name("BAD-NAME"));
    assert!(!is_valid_env_name("1BAD"));
    assert!(!is_valid_env_name("BAD.NAME"));
}

#[test]
fn dangerous_environment_names_are_blocked() {
    assert!(is_blocked_environment("PATH"));
    assert!(is_blocked_environment("LD_PRELOAD"));
    assert!(is_blocked_environment("dyld_insert_libraries"));
    assert!(!is_blocked_environment("GITHUB_TOKEN"));
}

#[test]
fn dangerous_environment_permission_is_rejected() {
    let permissions = permissions(false, false, &[], &["PATH"], &[]);
    assert!(permissions.validate().is_err());
}

#[test]
fn secret_requires_environment_permission() {
    let permissions = permissions(false, false, &[], &[], &["GITHUB_TOKEN"]);
    assert!(permissions.validate().is_err());
}

#[test]
fn dangerous_secret_permission_is_rejected() {
    let permissions = permissions(false, false, &[], &["LD_PRELOAD"], &["LD_PRELOAD"]);
    assert!(permissions.validate().is_err());
}

/// §14: a malformed permission set (empty filesystem path) must be rejected
/// at the registry level — the server cannot even be stored, so it can never
/// be spawned.
#[test]
fn malformed_permissions_are_rejected_at_registration() {
    let dir = tempdir().expect("tempdir");
    let registry = CustomMcpRegistry::new(dir.path()).expect("registry");
    let mut server = server_config("malformed-perm-server");
    server.permissions.filesystem.push("  ".to_string());
    assert!(registry.add(server).is_err(), "empty path must be rejected");
    // The store must remain empty — nothing was persisted.
    assert!(registry
        .list()
        .expect("list")
        .iter()
        .all(|entry| entry.id != "malformed-perm-server"));
}

/// §14: trust is enforced, not merely stored. An enabled custom MCP server
/// with no matching approval must NOT be registered by the dispatcher, so
/// its tools remain invisible to agents. The id is deliberately unique so
/// it cannot appear in any pre-existing trust store on the host.
#[test]
fn untrusted_custom_server_is_not_registered_by_dispatcher() {
    use agent_workspace_hub::mcp::McpDispatcher;

    let dir = tempdir().expect("tempdir");
    let registry = CustomMcpRegistry::new(dir.path()).expect("registry");
    registry
        .add(server_config("awh-never-approved-c39f1d"))
        .expect("add valid config");

    let dispatcher = McpDispatcher::new(dir.path().to_path_buf()).expect("dispatcher");
    let registered = dispatcher.provider_registry().blocking_read().providers();
    assert!(
        !registered
            .iter()
            .any(|id| id.contains("awh-never-approved-c39f1d")),
        "untrusted server must not be registered as a provider, got: {registered:?}"
    );
}

/// Shared fixture for custom-server registration tests: an enabled stdio
/// server asking for no permissions.
fn server_config(id: &str) -> CustomMcpServerConfig {
    CustomMcpServerConfig {
        id: id.to_string(),
        name: format!("test server {id}"),
        transport: McpTransport::Stdio,
        command: Some("cat".to_string()),
        args: Vec::new(),
        url: None,
        env: Default::default(),
        headers: Default::default(),
        permissions: McpPermissions::default(),
        enabled: true,
    }
}
