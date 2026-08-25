use agent_workspace_hub::mcp::{authorize_mcp_execution, McpExecutionRequest, McpPermissions, PersistentTrustStore, TrustLevel, TrustStore};
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
    let request = McpExecutionRequest { id: "unknown", version: "1.0.0", permissions: &requested };
    assert!(authorize_mcp_execution(&request, &trust).is_err());
}

#[test]
fn blocked_mcp_is_denied() {
    let mut trust = TrustStore::default();
    trust
        .approve("github", TrustLevel::Blocked, McpPermissions::default(), "1.0.0")
        .unwrap();
    let requested = permissions(false, false, &[], &[], &[]);
    let request = McpExecutionRequest { id: "github", version: "1.0.0", permissions: &requested };
    assert!(authorize_mcp_execution(&request, &trust).is_err());
}

#[test]
fn wrong_version_is_denied() {
    let mut trust = TrustStore::default();
    trust
        .approve("github", TrustLevel::Reviewed, McpPermissions::default(), "1.0.0")
        .unwrap();
    let requested = McpPermissions::default();
    let request = McpExecutionRequest { id: "github", version: "2.0.0", permissions: &requested };
    assert!(authorize_mcp_execution(&request, &trust).is_err());
}

#[test]
fn extra_permission_is_denied() {
    let mut trust = TrustStore::default();
    trust
        .approve("github", TrustLevel::Reviewed, permissions(false, false, &[], &[], &[]), "1.0.0")
        .unwrap();
    let requested = permissions(true, false, &[], &[], &[]);
    let request = McpExecutionRequest { id: "github", version: "1.0.0", permissions: &requested };
    assert!(authorize_mcp_execution(&request, &trust).is_err());
}

#[test]
fn approved_mcp_is_allowed() {
    let mut trust = TrustStore::default();
    let approved = permissions(true, false, &["project"], &["GITHUB_TOKEN"], &["GITHUB_TOKEN"]);
    trust
        .approve("github", TrustLevel::Reviewed, approved.clone(), "1.0.0")
        .unwrap();
    let request = McpExecutionRequest { id: "github", version: "1.0.0", permissions: &approved };
    assert!(authorize_mcp_execution(&request, &trust).is_ok());
}

#[test]
fn revoked_mcp_is_denied() {
    let mut trust = TrustStore::default();
    trust
        .approve("github", TrustLevel::Reviewed, McpPermissions::default(), "1.0.0")
        .unwrap();
    assert!(trust.revoke("github"));
    let requested = McpPermissions::default();
    let request = McpExecutionRequest { id: "github", version: "1.0.0", permissions: &requested };
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
