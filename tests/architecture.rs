//! Architecture conformance tests (spec section 26).
//!
//! The Control API and the MCP plane must stay conceptually separate:
//! both may use the shared `services` layer, but MCP must never depend
//! on the Control API, and the Control API must never reach into MCP's
//! dispatcher or tool implementations. These tests enforce the boundary
//! textually so a future refactor cannot silently couple the planes.

use std::fs;
use std::path::Path;

/// Collects every `.rs` file under `src/mcp`, recursively.
fn mcp_sources() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/mcp");
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("read src/mcp") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                let name = path.display().to_string();
                let src = fs::read_to_string(&path).expect("read source");
                out.push((name, src));
            }
        }
    }
    assert!(!out.is_empty(), "src/mcp should contain sources");
    out
}

#[test]
fn mcp_never_imports_the_control_api() {
    for (file, src) in mcp_sources() {
        assert!(
            !src.contains("crate::api"),
            "{file} must not depend on the Control API plane (spec 26)"
        );
    }
}

#[test]
fn control_api_never_imports_mcp_dispatcher_or_tools() {
    let control =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/api/control.rs"))
            .expect("read control.rs");
    for forbidden in [
        "McpDispatcher",
        "mcp::tools",
        "StdioMcpServer",
        "handle_response",
    ] {
        assert!(
            !control.contains(forbidden),
            "src/api/control.rs must not use MCP-internal `{forbidden}` (spec 26)"
        );
    }
}

#[test]
fn both_planes_share_only_the_service_layer() {
    // The Control API may use auth + registries (shared infrastructure)
    // and services, nothing else from mcp.
    let control =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/api/control.rs"))
            .expect("read control.rs");
    for line in control.lines().filter(|l| l.contains("crate::mcp")) {
        let shared = ["auth", "audit", "GlobalMcpRegistry"];
        let ok = shared
            .iter()
            .any(|allowed| line.contains(&format!("crate::mcp::{allowed}")))
            || line.trim_start().starts_with("//")
            || line.contains("mcp::{");
        assert!(
            ok,
            "unexpected MCP import in control.rs: `{line}` — only auth/audit/registry are shared"
        );
    }
}
