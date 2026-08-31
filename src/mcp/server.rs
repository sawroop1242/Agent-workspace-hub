use crate::mcp::dispatcher::{DispatchResult, McpDispatcher};
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
pub struct StdioMcpServer {
    dispatcher: McpDispatcher,
    runtime: Runtime,
}

impl StdioMcpServer {
    /// Builds the server for a project, wiring the Composio and custom MCP providers.
    pub fn new(project_root: PathBuf) -> Result<Self> {
        let runtime = Runtime::new()?;
        let dispatcher = McpDispatcher::new(project_root)?;
        Ok(Self {
            dispatcher,
            runtime,
        })
    }

    /// Returns the shared provider registry used to dispatch tool calls.
    pub fn provider_registry(&self) -> Arc<RwLock<ProviderRegistry>> {
        self.dispatcher.provider_registry()
    }

    /// Handles a single JSON-RPC request line, returning the JSON response.
    ///
    /// This method is a thin synchronous wrapper over the async dispatcher. It
    /// returns [`Err`] for protocol-level or structural failures (e.g. an
    /// unsupported JSON-RPC version) so callers that need to distinguish hard
    /// failures can do so; [`Self::handle_response`] is the resilient variant.
    pub fn handle(&self, input: &str) -> Result<String> {
        match self
            .runtime
            .block_on(self.dispatcher.dispatch_strict(input))?
        {
            DispatchResult::Response(response) => Ok(serde_json::to_string(&response)?),
            DispatchResult::NoResponse => Ok(String::new()),
        }
    }

    /// Handles a request line and always returns a JSON-RPC response string,
    /// turning any parse/dispatch failure into a JSON-RPC error object so the
    /// serve loop survives malformed or unknown requests instead of exiting.
    pub fn handle_response(&self, input: &str) -> String {
        match self.runtime.block_on(self.dispatcher.dispatch(input)) {
            DispatchResult::Response(response) => {
                serde_json::to_string(&response).unwrap_or_else(|_| {
                    r#"{"jsonrpc":"2.0","id":null,"result":null,"error":{"code":-32600,"message":"internal error"}}"#.to_string()
                })
            }
            DispatchResult::NoResponse => String::new(),
        }
    }
}
