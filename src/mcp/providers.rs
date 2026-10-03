use anyhow::{bail, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;

/// Per-provider cap on tool listing inside `aggregate_tools`. The
/// error-isolation below covers providers that *fail*; this covers
/// providers that *hang* — without it, one black-holed backend (or N of
/// them) stalls the whole `tools/list` advertisement for up to N × its
/// own client timeout on the client's connect path. Timed-out providers
/// take the same skip path as failing ones; the direct per-provider
/// listing (`connector.tools`) stays un-timed for diagnosis.
fn provider_list_timeout() -> Duration {
    // Test-only override so the hang-isolation regression test can run in
    // milliseconds (unit tests read this per call, in-process).
    if let Ok(millis) = std::env::var("AWH_TEST_PROVIDER_LIST_TIMEOUT_MS") {
        if let Ok(millis) = millis.parse::<u64>() {
            return Duration::from_millis(millis);
        }
    }
    Duration::from_secs(20)
}

/// Describes a tool exposed by a connector provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor {
    /// Tool name.
    pub name: String,
    /// Human-readable description.
    #[serde(default)]
    pub description: String,
    /// JSON Schema describing the tool's input arguments.
    #[serde(rename = "inputSchema", alias = "input_schema")]
    pub input_schema: Value,
}

/// The result of invoking a tool, carrying content and an error flag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    /// Output content pieces produced by the tool.
    pub content: Vec<ToolContent>,
    /// Whether the invocation reported an error.
    #[serde(default, rename = "isError")]
    pub is_error: bool,
}

/// A single piece of tool output.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ToolContent {
    /// Plain-text output.
    Text {
        /// The text.
        text: String,
    },
    /// Structured JSON output.
    Json {
        /// The JSON value.
        json: Value,
    },
}

/// A connector provider exposing tools that can be listed and invoked.
#[async_trait]
pub trait ConnectorProvider: Send + Sync {
    /// The provider's unique identifier.
    fn provider_id(&self) -> &str;
    /// Lists the tools this provider exposes.
    async fn list_tools(&self) -> Result<Vec<ToolDescriptor>>;
    /// Invokes a tool with the given arguments.
    async fn invoke(&self, tool: &str, arguments: Value) -> Result<ToolCallResult>;
}

/// Registry of registered connector providers, keyed by provider id.
#[derive(Default)]
pub struct ProviderRegistry {
    providers: HashMap<String, Box<dyn ConnectorProvider>>,
}
impl ProviderRegistry {
    /// Registers a provider.
    pub fn register(&mut self, p: Box<dyn ConnectorProvider>) {
        self.providers.insert(p.provider_id().to_string(), p);
    }

    /// Unregisters a provider by id, returning whether it was present.
    pub fn unregister(&mut self, provider_id: &str) -> bool {
        self.providers.remove(provider_id).is_some()
    }

    /// Returns the sorted list of registered provider ids.
    pub fn providers(&self) -> Vec<String> {
        let mut v: Vec<_> = self.providers.keys().cloned().collect();
        v.sort();
        v
    }

    /// Lists the tools exposed by one provider.
    pub async fn tools(&self, provider: &str) -> Result<Vec<ToolDescriptor>> {
        self.providers
            .get(provider)
            .ok_or_else(|| anyhow::anyhow!("provider not registered: {provider}"))?
            .list_tools()
            .await
    }

    /// Invokes a tool on one provider.
    pub async fn invoke(&self, provider: &str, tool: &str, args: Value) -> Result<ToolCallResult> {
        if tool.is_empty() {
            bail!("tool name is required")
        }
        self.providers
            .get(provider)
            .ok_or_else(|| anyhow::anyhow!("provider not registered: {provider}"))?
            .invoke(tool, args)
            .await
    }

    /// Collects tools from all providers, prefixing names with the provider id.
    pub async fn aggregate_tools(&self) -> Result<Vec<ToolDescriptor>> {
        let mut out = Vec::new();
        for provider in self.providers() {
            // Fail closed per provider, not per plane: one unhealthy
            // provider (bad credentials, unreachable backend, or a
            // backend that simply never answers) must not take down the
            // whole tools/list advertisement. Its tools are simply
            // absent; direct per-provider listing (`connector.tools`)
            // still surfaces the real error for diagnosis.
            let listed =
                match tokio::time::timeout(provider_list_timeout(), self.tools(&provider)).await {
                    Ok(Ok(listed)) => listed,
                    Ok(Err(error)) => {
                        tracing::warn!(
                            provider = %provider,
                            error = %error,
                            "dynamic provider tool listing failed; skipping provider"
                        );
                        crate::mcp::audit::audit_deny(
                            "dynamic_provider_rejected",
                            "provider_list_failed",
                            &provider,
                        );
                        continue;
                    }
                    Err(_) => {
                        tracing::warn!(
                            provider = %provider,
                            timeout_ms = provider_list_timeout().as_millis(),
                            "dynamic provider tool listing timed out; skipping provider"
                        );
                        crate::mcp::audit::audit_deny(
                            "dynamic_provider_rejected",
                            "provider_list_timeout",
                            &provider,
                        );
                        continue;
                    }
                };
            for mut tool in listed {
                tool.name = format!("{}.{}", provider, tool.name);
                if tool.description.is_empty() {
                    tool.description = format!("Tool provided by {provider}");
                }
                // Exposure gate: a dynamic tool whose advertised input
                // schema is malformed (or uses unsupported keywords) is
                // NOT exposed — clients would otherwise see a tool whose
                // arguments AWH cannot validate, leaving provider-side
                // checks as the only defense. Fail closed per tool, not
                // per provider: one bad advertisement must not hide its
                // healthy siblings.
                if let Err(error) =
                    crate::mcp::schema::validate_schema_syntax(&tool.input_schema, "#")
                {
                    tracing::warn!(
                        tool = %tool.name,
                        error = %error,
                        "dynamic tool rejected: malformed input schema"
                    );
                    crate::mcp::audit::audit_deny(
                        "dynamic_tool_rejected",
                        "malformed_tool_schema",
                        &tool.name,
                    );
                    continue;
                }
                out.push(tool);
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }
    /// Invokes a tool addressed as `provider.tool`.
    pub async fn invoke_qualified(
        &self,
        qualified_tool: &str,
        args: Value,
    ) -> Result<ToolCallResult> {
        let (provider, tool) = qualified_tool
            .split_once('.')
            .ok_or_else(|| anyhow::anyhow!("tool must use provider.tool format"))?;
        self.invoke(provider, tool, args).await
    }
}

/// A placeholder provider that reports itself as unconfigured.
pub struct UnconfiguredProvider {
    id: String,
}
impl UnconfiguredProvider {
    /// Creates an unconfigured provider for a given id.
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}
#[async_trait]
impl ConnectorProvider for UnconfiguredProvider {
    fn provider_id(&self) -> &str {
        &self.id
    }
    async fn list_tools(&self) -> Result<Vec<ToolDescriptor>> {
        Ok(Vec::new())
    }
    async fn invoke(&self, _: &str, _: Value) -> Result<ToolCallResult> {
        bail!("provider '{}' is not configured", self.id)
    }
}

/// Adapts a custom MCP server into a [`ConnectorProvider`] over an [`McpClient`].
pub struct CustomMcpProvider<C> {
    id: String,
    client: std::sync::Arc<C>,
}
impl<C> CustomMcpProvider<C> {
    /// Creates a provider around an MCP client.
    pub fn new(id: impl Into<String>, client: std::sync::Arc<C>) -> Self {
        Self {
            id: id.into(),
            client,
        }
    }
}
/// Minimal MCP client interface listing and calling tools.
#[async_trait]
pub trait McpClient: Send + Sync {
    /// Lists tools (returns the raw MCP response value).
    async fn tools_list(&self) -> Result<Value>;
    /// Calls a tool (returns the raw MCP response value).
    async fn tools_call(&self, tool: &str, args: Value) -> Result<Value>;
}
#[async_trait]
impl McpClient for crate::mcp::StdioMcpClient {
    async fn tools_list(&self) -> Result<Value> {
        self.tools_list().await
    }
    async fn tools_call(&self, t: &str, a: Value) -> Result<Value> {
        self.tools_call(t, a).await
    }
}
#[async_trait]
impl McpClient for crate::mcp::StreamableHttpMcpClient {
    async fn tools_list(&self) -> Result<Value> {
        self.tools_list().await
    }
    async fn tools_call(&self, t: &str, a: Value) -> Result<Value> {
        self.tools_call(t, a).await
    }
}
#[async_trait]
impl<C: McpClient> ConnectorProvider for CustomMcpProvider<C> {
    fn provider_id(&self) -> &str {
        &self.id
    }
    async fn list_tools(&self) -> Result<Vec<ToolDescriptor>> {
        let v = self.client.tools_list().await?;
        Ok(serde_json::from_value(
            v.get("tools")
                .cloned()
                .unwrap_or_else(|| Value::Array(vec![])),
        )?)
    }
    async fn invoke(&self, t: &str, a: Value) -> Result<ToolCallResult> {
        let v = self.client.tools_call(t, a).await?;
        match serde_json::from_value(v.clone()) {
            Ok(r) => Ok(r),
            Err(_) => Ok(ToolCallResult {
                content: vec![ToolContent::Json { json: v }],
                is_error: false,
            }),
        }
    }
}

/// A provider backed by closures, bridging non-async tool sources.
pub struct GatewayProvider<F, G>
where
    F: Fn() -> Result<Vec<ToolDescriptor>> + Send + Sync,
    G: Fn(&str, Value) -> Result<ToolCallResult> + Send + Sync,
{
    id: String,
    list_fn: F,
    invoke_fn: G,
}
impl<F, G> GatewayProvider<F, G>
where
    F: Fn() -> Result<Vec<ToolDescriptor>> + Send + Sync,
    G: Fn(&str, Value) -> Result<ToolCallResult> + Send + Sync,
{
    /// Creates a gateway provider from list and invoke closures.
    pub fn new(id: impl Into<String>, list_fn: F, invoke_fn: G) -> Self {
        Self {
            id: id.into(),
            list_fn,
            invoke_fn,
        }
    }
}
#[async_trait]
impl<F, G> ConnectorProvider for GatewayProvider<F, G>
where
    F: Fn() -> Result<Vec<ToolDescriptor>> + Send + Sync,
    G: Fn(&str, Value) -> Result<ToolCallResult> + Send + Sync,
{
    fn provider_id(&self) -> &str {
        &self.id
    }
    async fn list_tools(&self) -> Result<Vec<ToolDescriptor>> {
        (self.list_fn)()
    }
    async fn invoke(&self, t: &str, a: Value) -> Result<ToolCallResult> {
        (self.invoke_fn)(t, a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor(name: &str) -> ToolDescriptor {
        ToolDescriptor {
            name: name.into(),
            description: String::new(),
            input_schema: serde_json::json!({"type": "object", "properties": {}}),
        }
    }

    #[tokio::test]
    async fn register_then_unregister_removes_provider() {
        let mut registry = ProviderRegistry::default();
        registry.register(Box::new(GatewayProvider::new(
            "demo",
            || Ok(vec![descriptor("ping")]),
            |_t, _a| {
                Ok(ToolCallResult {
                    content: vec![],
                    is_error: false,
                })
            },
        )));
        assert_eq!(registry.providers(), vec!["demo".to_string()]);
        assert!(registry.tools("demo").await.is_ok());

        assert!(registry.unregister("demo"));
        assert!(registry.providers().is_empty());
        assert!(registry.tools("demo").await.is_err());
        // Unregistering an id that was never (or no longer) present reports
        // false rather than erroring, mirroring the *Mcp stores' remove().
        assert!(!registry.unregister("demo"));
    }

    #[tokio::test]
    async fn aggregate_tools_isolates_failing_providers() {
        // Regression: an unhealthy provider (bad credentials / unreachable
        // backend) used to poison the WHOLE tools/list advertisement via `?`.
        // One external SaaS outage must not hide the 73 static tools.
        let mut registry = ProviderRegistry::default();
        registry.register(Box::new(GatewayProvider::new(
            "good",
            || Ok(vec![descriptor("ping")]),
            |_t, _a| {
                Ok(ToolCallResult {
                    content: vec![],
                    is_error: false,
                })
            },
        )));
        registry.register(Box::new(GatewayProvider::new(
            "bad",
            || Err(anyhow::anyhow!("Composio API returned 401 Unauthorized")),
            |_t, _a| {
                Ok(ToolCallResult {
                    content: vec![],
                    is_error: false,
                })
            },
        )));

        let aggregated = registry
            .aggregate_tools()
            .await
            .expect("aggregate listing must survive one failing provider");
        let names: Vec<&str> = aggregated.iter().map(|t| t.name.as_str()).collect();
        assert!(
            names.contains(&"good.ping".to_string().as_str()),
            "{names:?}"
        );
        assert!(!names.iter().any(|n| n.starts_with("bad.")), "{names:?}");

        // The skip is observable: a deny audit event names the provider.
        let denied = crate::services::audit::global()
            .recent(200)
            .into_iter()
            .any(|e| e.action == "dynamic_provider_rejected" && e.subject == "bad");
        assert!(denied, "expected a dynamic_provider_rejected audit event");
    }

    /// A provider whose backend never answers must be skipped after the
    /// per-provider cap instead of stalling the whole advertisement —
    /// error isolation alone does not cover hangs (Kilo review finding).
    #[tokio::test]
    async fn aggregate_tools_isolates_hanging_providers() {
        struct HangingProvider;
        #[async_trait]
        impl ConnectorProvider for HangingProvider {
            fn provider_id(&self) -> &str {
                "hung"
            }
            async fn list_tools(&self) -> Result<Vec<ToolDescriptor>> {
                // Never resolves: a black-holed backend.
                std::future::pending::<()>().await;
                unreachable!()
            }
            async fn invoke(&self, _tool: &str, _args: Value) -> Result<ToolCallResult> {
                std::future::pending::<()>().await;
                unreachable!()
            }
        }

        std::env::set_var("AWH_TEST_PROVIDER_LIST_TIMEOUT_MS", "250");
        let mut registry = ProviderRegistry::default();
        registry.register(Box::new(GatewayProvider::new(
            "good",
            || Ok(vec![descriptor("ping")]),
            |_t, _a| {
                Ok(ToolCallResult {
                    content: vec![],
                    is_error: false,
                })
            },
        )));
        registry.register(Box::new(HangingProvider));

        let started = std::time::Instant::now();
        let aggregated = registry
            .aggregate_tools()
            .await
            .expect("aggregate listing must survive one hanging provider");
        let elapsed = started.elapsed();
        std::env::remove_var("AWH_TEST_PROVIDER_LIST_TIMEOUT_MS");

        // The healthy provider's tools still ship; the hanging one is
        // absent, not fatal.
        let names: Vec<&str> = aggregated.iter().map(|t| t.name.as_str()).collect();
        assert!(
            names.contains(&"good.ping".to_string().as_str()),
            "{names:?}"
        );
        assert!(!names.iter().any(|n| n.starts_with("hung.")), "{names:?}");
        // Bounded wait: the cap (250ms in this test) plus a small margin —
        // certainly not the multi-second HTTP client default.
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "hanging provider bounded to the per-provider cap, took {elapsed:?}"
        );

        // The timeout skip is audited, with the timeout reason.
        // The skip is observable: a deny audit event names the hung
        // provider. (The reason string itself is redacted in the record
        // -- `provider_list_timeout` is a 21-char base62 run, masked by
        // the documented >=16-char redaction trade-off -- so the event
        // is asserted by action + subject, like the failing-provider
        // test above.)
        let denied = crate::services::audit::global()
            .recent(200)
            .into_iter()
            .any(|e| e.action == "dynamic_provider_rejected" && e.subject == "hung");
        assert!(denied, "expected a dynamic_provider_rejected audit event");
    }
}
