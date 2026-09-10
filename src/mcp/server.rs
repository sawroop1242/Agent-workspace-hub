use crate::mcp::dispatcher::{DispatchResult, McpDispatcher, SessionLifecycle};
use crate::mcp::ProviderRegistry;
use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::RwLock;

/// The stdio JSON-RPC MCP server.
///
/// This is a thin transport adapter over the shared [`McpDispatcher`]: it owns a
/// dedicated [`Runtime`] to drive the async dispatcher from the synchronous
/// stdio read loop. All tool dispatches are forwarded to the dispatcher, so the
/// tool implementations live in exactly one place regardless of transport.
///
/// The server enforces the MCP initialization lifecycle for its single client
/// (one stdio connection per process): requests other than `initialize` and
/// `ping` are answered with `-32002` (server not initialized) until the
/// `initialize` exchange completes.
pub struct StdioMcpServer {
    dispatcher: McpDispatcher,
    runtime: Runtime,
    lifecycle: SessionLifecycle,
}

impl StdioMcpServer {
    /// Builds the server for a project, wiring the Composio and custom MCP providers.
    pub fn new(project_root: PathBuf) -> Result<Self> {
        let runtime = Runtime::new()?;
        // Build the dispatcher on this server's own runtime — never a nested
        // one (see McpDispatcher::new_async).
        let dispatcher = runtime.block_on(McpDispatcher::new_async(project_root))?;
        Ok(Self {
            dispatcher,
            runtime,
            lifecycle: SessionLifecycle::default(),
        })
    }

    /// Returns the shared provider registry used to dispatch tool calls.
    pub fn provider_registry(&self) -> Arc<RwLock<ProviderRegistry>> {
        self.dispatcher.provider_registry()
    }

    /// Whether this server's session has completed the `initialize` exchange.
    pub fn is_initialized(&self) -> bool {
        self.lifecycle.is_initialized()
    }

    /// Handles a single JSON-RPC request line, returning the JSON response.
    ///
    /// This method is a thin synchronous wrapper over the async dispatcher. It
    /// returns [`Err`] for protocol-level or structural failures (e.g. an
    /// unsupported JSON-RPC version) so callers that need to distinguish hard
    /// failures can do so; [`Self::handle_response`] is the resilient variant.
    ///
    /// Lifecycle outcomes (pre-initialization requests, duplicate
    /// initialization) are *known* JSON-RPC failure modes and therefore come
    /// back as well-formed error responses, not [`Err`].
    pub fn handle(&self, input: &str) -> Result<String> {
        match self.runtime.block_on(
            self.dispatcher
                .dispatch_strict_with_lifecycle(input, &self.lifecycle),
        )? {
            DispatchResult::Response(response) => Ok(serde_json::to_string(&response)?),
            DispatchResult::NoResponse => Ok(String::new()),
        }
    }

    /// Handles a request line and always returns a JSON-RPC response string,
    /// turning any parse/dispatch failure into a JSON-RPC error object so the
    /// serve loop survives malformed or unknown requests instead of exiting.
    pub fn handle_response(&self, input: &str) -> String {
        match self.runtime.block_on(
            self.dispatcher
                .dispatch_with_lifecycle(input, &self.lifecycle),
        ) {
            DispatchResult::Response(response) => {
                serde_json::to_string(&response).unwrap_or_else(|_| {
                    r#"{"jsonrpc":"2.0","id":null,"result":null,"error":{"code":-32600,"message":"internal error"}}"#.to_string()
                })
            }
            DispatchResult::NoResponse => String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The lifecycle gate must reject tool execution before the initialize
    /// exchange with the documented -32002 code, and admit it after.
    #[test]
    fn stdio_session_gates_tools_until_initialized() {
        let temp = tempfile::tempdir().unwrap();
        let server = StdioMcpServer::new(temp.path().to_path_buf()).unwrap();

        let pre =
            server.handle_response(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#);
        let response: serde_json::Value = serde_json::from_str(&pre).unwrap();
        assert_eq!(response["error"]["code"], -32002, "got: {response}");
        assert!(!server.lifecycle.is_initialized());

        // ping is the liveness probe and stays available pre-initialize.
        let ping = server.handle_response(r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#);
        let response: serde_json::Value = serde_json::from_str(&ping).unwrap();
        assert!(response.get("error").is_none(), "got: {response}");

        // A notification never gets a response and does not flip state.
        let note =
            server.handle_response(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
        assert!(note.is_empty());
        assert!(!server.lifecycle.is_initialized());

        // The initialize exchange opens the session.
        let init = server.handle_response(
            r#"{"jsonrpc":"2.0","id":3,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
        );
        let response: serde_json::Value = serde_json::from_str(&init).unwrap();
        assert!(response.get("error").is_none(), "got: {response}");
        assert_eq!(response["result"]["protocolVersion"], "2025-06-18");
        assert!(server.lifecycle.is_initialized());

        // Now tools/list succeeds.
        let tools =
            server.handle_response(r#"{"jsonrpc":"2.0","id":4,"method":"tools/list","params":{}}"#);
        let response: serde_json::Value = serde_json::from_str(&tools).unwrap();
        assert!(response.get("error").is_none(), "got: {response}");
        assert!(response["result"]["tools"].as_array().is_some());

        // Duplicate initialization is a deterministic protocol error.
        let dup =
            server.handle_response(r#"{"jsonrpc":"2.0","id":5,"method":"initialize","params":{}}"#);
        let response: serde_json::Value = serde_json::from_str(&dup).unwrap();
        assert_eq!(response["error"]["code"], -32600, "got: {response}");
    }

    /// Invalid initialize parameters keep the session uninitialized (the
    /// client may retry with corrected parameters).
    #[test]
    fn invalid_initialize_params_do_not_open_the_session() {
        let temp = tempfile::tempdir().unwrap();
        let server = StdioMcpServer::new(temp.path().to_path_buf()).unwrap();

        let bad = server.handle_response(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":42}}"#,
        );
        let response: serde_json::Value = serde_json::from_str(&bad).unwrap();
        assert_eq!(response["error"]["code"], -32602, "got: {response}");
        assert!(!server.lifecycle.is_initialized());

        let bad = server
            .handle_response(r#"{"jsonrpc":"2.0","id":2,"method":"initialize","params":[1,2]}"#);
        let response: serde_json::Value = serde_json::from_str(&bad).unwrap();
        assert_eq!(response["error"]["code"], -32602, "got: {response}");
        assert!(!server.lifecycle.is_initialized());

        // A corrected retry still initializes the session.
        let ok =
            server.handle_response(r#"{"jsonrpc":"2.0","id":3,"method":"initialize","params":{}}"#);
        let response: serde_json::Value = serde_json::from_str(&ok).unwrap();
        assert!(response.get("error").is_none(), "got: {response}");
        assert!(server.lifecycle.is_initialized());
    }

    /// Structural errors keep their precise codes regardless of session state:
    /// a pre-initialize message that is not even valid JSON-RPC reports the
    /// structural problem, not "not initialized".
    #[test]
    fn structural_errors_precede_the_lifecycle_gate() {
        let temp = tempfile::tempdir().unwrap();
        let server = StdioMcpServer::new(temp.path().to_path_buf()).unwrap();

        let bad = server.handle_response("{ not json");
        let response: serde_json::Value = serde_json::from_str(&bad).unwrap();
        assert_eq!(response["error"]["code"], -32700, "got: {response}");

        let bad =
            server.handle_response(r#"{"jsonrpc":"1.0","id":1,"method":"tools/list","params":{}}"#);
        let response: serde_json::Value = serde_json::from_str(&bad).unwrap();
        assert_eq!(response["error"]["code"], -32600, "got: {response}");
    }
}
