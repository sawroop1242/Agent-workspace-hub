//! Regression tests for the nested-Tokio-runtime hazard in dispatcher
//! construction (master prompt §20).
//!
//! Reproduction intent: `McpDispatcher::new` historically created an internal
//! `tokio::runtime::Runtime` and called `block_on` when spawning trusted
//! custom MCP servers. Calling `block_on` from within an already-running
//! runtime panics ("Cannot start a runtime from within a runtime"). These
//! tests pin the fail-closed contract that replaced that behavior.

use agent_workspace_hub::mcp::McpDispatcher;
use tempfile::tempdir;

/// Constructing the dispatcher from inside a runtime context must fail closed
/// with an actionable error (pointing at the async constructor) instead of
/// panicking when a trusted custom MCP server later forces an internal
/// `block_on`. The hazard is deterministic: any caller on a tokio worker
/// thread hits it the moment a trusted custom stdio server is configured.
#[tokio::test]
async fn sync_dispatcher_construction_fails_closed_inside_async_context() {
    let dir = tempdir().expect("tempdir");
    let error = match McpDispatcher::new(dir.path().to_path_buf()) {
        Ok(_) => panic!("sync construction must fail closed inside a runtime"),
        Err(error) => error,
    };
    assert!(
        error.to_string().contains("new_async"),
        "error must point at the async constructor: {error}"
    );
}

/// The async constructor is the supported path inside a runtime: it performs
/// the identical provider wiring (Composio, custom MCP servers, GitHub) with
/// plain awaits and never creates a nested runtime.
#[tokio::test]
async fn async_dispatcher_construction_works_inside_runtime() {
    let dir = tempdir().expect("tempdir");
    let dispatcher = McpDispatcher::new_async(dir.path().to_path_buf())
        .await
        .expect("async construction must succeed inside a runtime");
    // A trivial dispatch proves the dispatcher is fully operational.
    let response = dispatcher
        .dispatch(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#)
        .await;
    match response {
        agent_workspace_hub::mcp::DispatchResult::Response(r) => {
            assert!(r.result.is_some(), "ping must produce a result");
        }
        agent_workspace_hub::mcp::DispatchResult::NoResponse => {
            panic!("ping is a request and must be answered");
        }
    }
}

/// The sync constructor remains the supported path from plain threads (the
/// stdio transport and the CLI are synchronous at construction time).
#[test]
fn sync_dispatcher_construction_still_works_on_plain_threads() {
    let dir = tempdir().expect("tempdir");
    McpDispatcher::new(dir.path().to_path_buf()).expect("sync construction on a plain thread");
}
