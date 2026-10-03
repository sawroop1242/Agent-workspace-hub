use anyhow::{bail, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;

/// Default per-provider cap on tool listing inside `aggregate_tools`.
/// The error-isolation below covers providers that *fail*; the timeout
/// covers providers that *hang*. Timed-out providers take the same skip
/// path as failing ones; the direct per-provider listing
/// (`connector.tools`) stays un-timed for diagnosis.
const PROVIDER_LIST_TIMEOUT: Duration = Duration::from_secs(20);

/// At most this many provider listings are in flight at once inside
/// `aggregate_tools`. Bounded so a registry of N providers does not
/// burst N simultaneous outbound HTTP requests on every `tools/list`;
/// 8 covers realistic SaaS/connector fan-out. Together with the
/// aggregate budget (the effective cap + `PROVIDER_LIST_BUDGET_SLACK`,
/// see below) the wall-clock cost of a full aggregation stays ~one cap
/// regardless of N.
const PROVIDER_LIST_CONCURRENCY: usize = 8;

/// The aggregate budget is the effective per-provider cap
/// (`effective_list_timeout`) plus a small fixed collection slack: the
/// whole listing phase — not each future — must fit inside ONE cap.
/// Without an aggregate bound, ceil(N/8) hung providers under the
/// concurrency window would stretch a single `tools/list` to
/// ceil(N/8) x cap (about 140 s at 50 providers) — and stdio
/// `handle()` has NO outer request deadline to clip it (the HTTP
/// plane's `TimeoutLayer` never sees stdio). The slack keeps the two
/// timeouts from racing: a provider whose own cap just fired is
/// still COLLECTED (and audited `list_timeout`) instead of losing to
/// the budget; only providers still listing when the slack elapses
/// take the `list_budget` skip path. Either way the advertisement
/// stays live, the ring records why, and total aggregation latency
/// is bounded by one cap + slack, whatever N is.
const PROVIDER_LIST_BUDGET_SLACK: Duration = Duration::from_millis(100);

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
    /// Per-provider listing timeout for `aggregate_tools`. `None`
    /// selects the default (`PROVIDER_LIST_TIMEOUT`); `Some(cap)` uses
    /// the cap verbatim (the test constructor rejects zero, which
    /// would time every provider out instantly and silently empty the
    /// catalog). Instance-scoped (not a process-global/env override):
    /// test registries can shrink the cap without any release-build
    /// surface and without racing parallel tests; production always
    /// runs the default.
    list_timeout_override: Option<Duration>,
}
/// One provider's `aggregate_tools` listing outcome: the outer layer
/// distinguishes cap-timeout (Err) from completion; the inner Result is
/// the provider's own listing result.
type ProviderListing = (
    String,
    Result<Result<Vec<ToolDescriptor>, anyhow::Error>, tokio::time::error::Elapsed>,
);

impl ProviderRegistry {
    /// Registers a provider.
    pub fn register(&mut self, p: Box<dyn ConnectorProvider>) {
        self.providers.insert(p.provider_id().to_string(), p);
    }

    /// Test/bench constructor: a registry with a non-default
    /// per-provider listing cap (non-zero — a zero cap would skip
    /// every provider and silently empty the dynamic catalog).
    /// Production code never calls this — `Default` keeps the
    /// documented 20 s cap.
    #[cfg(test)]
    pub fn with_list_timeout(cap: Duration) -> Self {
        assert!(
            !cap.is_zero(),
            "test listing cap must be non-zero (zero empties the catalog)"
        );
        Self {
            list_timeout_override: Some(cap),
            ..Default::default()
        }
    }

    /// The effective per-provider listing cap for this registry.
    fn effective_list_timeout(&self) -> Duration {
        self.list_timeout_override.unwrap_or(PROVIDER_LIST_TIMEOUT)
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
        let providers = self.providers();
        // Fail closed per provider, not per plane: an unhealthy provider
        // (bad credentials, unreachable backend, or a backend that never
        // answers) must not take down the whole tools/list advertisement
        // — and must not stall it either. Listings run concurrently
        // under a bounded window (each future gets its own cap, at most
        // PROVIDER_LIST_CONCURRENCY in flight at once), so the
        // wall-clock cost is one cap, not N × cap, and a registry of N
        // providers does not burst N simultaneous outbound requests on
        // every tools/list. Unhealthy providers are skipped; their
        // tools are simply absent and a deny audit event names them
        // (direct per-provider listing via `connector.tools` still
        // surfaces the real error for diagnosis).
        //
        // The audit reasons are deliberately SHORT (<16 chars):
        // `AuditLog::record` runs `redact_token_like` over the detail,
        // which masks >=16-char base62 runs, so longer slugs
        // (`provider_list_timeout`) would persist as `[redacted]` and
        // the ring could not distinguish a hang from a failure.
        let cap = self.effective_list_timeout();
        let provider_ids = providers;
        let mut listings = futures_util::stream::iter(
            provider_ids
                .iter()
                .map(|provider: &String| async move {
                    (
                        provider.clone(),
                        tokio::time::timeout(cap, self.tools(provider)).await,
                    )
                })
                .collect::<Vec<_>>(),
        )
        .buffer_unordered(PROVIDER_LIST_CONCURRENCY);
        // Aggregate budget: the whole listing phase — not each future —
        // must fit inside ONE effective cap (see the const doc above),
        // so a registry of N hung providers can never stretch one
        // tools/list to ceil(N/window) caps (stdio has no outer request
        // deadline).
        let mut results: Vec<ProviderListing> = Vec::new();
        let budget = tokio::time::timeout(cap + PROVIDER_LIST_BUDGET_SLACK, async {
            while let Some((provider, listed)) = listings.next().await {
                // Results arrive in completion order; re-sort below
                // keeps the advertised catalog stable.
                results.push((provider, listed));
            }
        })
        .await;
        if let Err(_elapsed) = budget {
            // Budget exhausted: everything not yet collected is skipped
            // and audited exactly like a per-future timeout, so the
            // advertisement still returns — live, bounded, and truthful
            // about what was skipped.
            let listed_ids: Vec<&str> = results.iter().map(|(id, _)| id.as_str()).collect();
            for provider in &provider_ids {
                if !listed_ids.contains(&provider.as_str()) {
                    tracing::warn!(
                        provider = %provider,
                        budget_ms = cap.as_millis() as u64,
                        "dynamic provider listing budget exhausted; skipping provider"
                    );
                    crate::mcp::audit::audit_deny(
                        "dynamic_provider_rejected",
                        "list_budget",
                        provider,
                    );
                }
            }
        }
        results.sort_by(|a, b| a.0.cmp(&b.0));
        for (provider, listed) in results {
            let listed = match listed {
                Ok(Ok(listed)) => listed,
                Ok(Err(error)) => {
                    tracing::warn!(
                        provider = %provider,
                        error = %error,
                        "dynamic provider tool listing failed; skipping provider"
                    );
                    crate::mcp::audit::audit_deny(
                        "dynamic_provider_rejected",
                        "list_failed",
                        &provider,
                    );
                    continue;
                }
                Err(_) => {
                    tracing::warn!(
                        provider = %provider,
                        timeout_secs = cap.as_secs(),
                        "dynamic provider tool listing timed out; skipping provider"
                    );
                    crate::mcp::audit::audit_deny(
                        "dynamic_provider_rejected",
                        "list_timeout",
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

        // The skip is observable: a deny audit event names the provider,
        // with the `list_failed` reason (kept under the 16-char
        // redaction threshold so it persists verbatim in the record).
        let denied = crate::services::audit::global()
            .recent(200)
            .into_iter()
            .any(|e| {
                e.action == "dynamic_provider_rejected"
                    && e.subject == "bad"
                    && e.detail.contains("list_failed")
            });
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

        // 150ms cap per provider (well under the multi-second HTTP
        // client default) — set on THIS registry instance, so no
        // process-global state can race parallel tests.
        let mut registry = ProviderRegistry::with_list_timeout(Duration::from_millis(150));
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

        // The healthy provider's tools still ship; the hanging one is
        // absent, not fatal.
        let names: Vec<&str> = aggregated.iter().map(|t| t.name.as_str()).collect();
        assert!(
            names.contains(&"good.ping".to_string().as_str()),
            "{names:?}"
        );
        assert!(!names.iter().any(|n| n.starts_with("hung.")), "{names:?}");
        // Bounded wait: the cap (150ms in this test) plus a small margin —
        // certainly not the multi-second HTTP client default.
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "hanging provider bounded to the per-provider cap, took {elapsed:?}"
        );

        // The timeout skip is audited with the `list_timeout` reason
        // (short enough to survive the redaction pass verbatim, so the
        // ring can distinguish a hang from a failure).
        let denied = crate::services::audit::global()
            .recent(200)
            .into_iter()
            .any(|e| {
                e.action == "dynamic_provider_rejected"
                    && e.subject == "hung"
                    && e.detail.contains("list_timeout")
            });
        assert!(
            denied,
            "expected a dynamic_provider_rejected/list_timeout audit event"
        );
    }

    /// N hanging providers must never stretch ONE aggregation past the
    /// aggregate budget: listings run under a bounded window
    /// (PROVIDER_LIST_CONCURRENCY) and the whole listing phase is
    /// capped by the effective cap + budget slack, whatever N is.
    #[tokio::test]
    async fn aggregate_tools_bounds_n_hanging_providers_to_the_budget() {
        struct HangingProvider(&'static str);
        #[async_trait]
        impl ConnectorProvider for HangingProvider {
            fn provider_id(&self) -> &str {
                self.0
            }
            async fn list_tools(&self) -> Result<Vec<ToolDescriptor>> {
                std::future::pending::<()>().await;
                unreachable!()
            }
            async fn invoke(&self, _tool: &str, _args: Value) -> Result<ToolCallResult> {
                std::future::pending::<()>().await;
                unreachable!()
            }
        }

        // 10 hanging providers under a 200ms per-provider cap with a
        // concurrency window of 8: wave 1 (8 hangs) hits its cap at
        // ~200ms, wave 2 (2 hangs) starts and would hit its own cap at
        // ~400ms — BUT the aggregate budget is also 200ms, so the
        // budget fires first: wave-2 providers never get listed and
        // are skipped+audited (`list_budget`). Total ~one budget
        // (~200ms), INDEPENDENT of N. (The old serial loop: 10 x 200ms
        // = 2s. Unbounded concurrency: one cap. Neither bound is
        // sufficient alone — that is why both exist.)
        let mut registry = ProviderRegistry::with_list_timeout(Duration::from_millis(200));
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
        for id in [
            "hang-a", "hang-b", "hang-c", "hang-d", "hang-e", "hang-f", "hang-g", "hang-h",
            "hang-i", "hang-j",
        ] {
            registry.register(Box::new(HangingProvider(id)));
        }

        let started = std::time::Instant::now();
        let aggregated = registry
            .aggregate_tools()
            .await
            .expect("aggregate listing must survive five hanging providers");
        let elapsed = started.elapsed();

        let names: Vec<&str> = aggregated.iter().map(|t| t.name.as_str()).collect();
        assert!(
            names.contains(&"good.ping".to_string().as_str()),
            "{names:?}"
        );
        for id in [
            "hang-a", "hang-b", "hang-c", "hang-d", "hang-e", "hang-f", "hang-g", "hang-h",
            "hang-i", "hang-j",
        ] {
            assert!(
                !names.iter().any(|n| n.starts_with(&format!("{id}."))),
                "{names:?}"
            );
        }
        // The aggregate budget (equal to the per-provider cap here:
        // 200ms) bounds the WHOLE listing phase regardless of N: one
        // budget + scheduling margin, not ceil(N/8) caps, not N caps.
        assert!(
            elapsed < std::time::Duration::from_millis(1200),
            "aggregate budget must bound N hangs to one budget, took {elapsed:?}"
        );
        // Wave-2 providers hit the budget path specifically: they are
        // audited with `list_budget`, distinct from wave-1's
        // `list_timeout`.
        let budget_denied = crate::services::audit::global()
            .recent(200)
            .into_iter()
            .any(|e| e.action == "dynamic_provider_rejected" && e.detail.contains("list_budget"));
        assert!(
            budget_denied,
            "expected a dynamic_provider_rejected/list_budget audit event"
        );
    }
}
