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
    /// Round-robin start offset for `aggregate_tools` listings (see the
    /// rotation comment inside `aggregate_tools`). Per-instance counter,
    /// not security-sensitive.
    start_rotation: std::sync::atomic::AtomicU64,
    /// Last successful listing per provider id, served when the
    /// aggregate budget truncates a provider that never got to answer
    /// (budget truncation is OUR scheduling artifact, not a provider
    /// health signal — a hung/failed provider is still dropped, only
    /// budget-truncated ones fall back to cache). Keeps advertised
    /// tool MEMBERSHIP stable across calls: once a tool has been
    /// advertised, it stays advertised. Std Mutex, never held across
    /// an await.
    last_good: std::sync::Mutex<HashMap<String, Vec<ToolDescriptor>>>,
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
        // An upsert replaces the instance but the last-good cache
        // must NOT survive it: re-registering an id that is already
        // live (composio re-registration writes `composio:{label}`
        // straight into a live registry) must never serve the
        // previous instance's listing for the new one.
        self.last_good_guard().remove(p.provider_id());
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
        // The last-good cache must be purged with the registration
        // (see the mirror comment in `register`).
        self.last_good_guard().remove(provider_id);
        self.providers.remove(provider_id).is_some()
    }

    /// Access the last-good cache. A poisoned mutex still yields the
    /// map (it is structurally valid — the guard is never held across
    /// an await and only HashMap ops run under it), so a panic in one
    /// caller can never turn cache serve/purge into a silent fail-open
    /// skip.
    fn last_good_guard(&self) -> std::sync::MutexGuard<'_, HashMap<String, Vec<ToolDescriptor>>> {
        self.last_good
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
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
        // The budget truncates whatever has not finished when it fires;
        // starting futures in the registry's sorted id order would
        // deterministically pre-empt the alphabetically LAST providers
        // on EVERY call. Rotate the START order by a per-call counter
        // (round-robin) so, over successive calls, every provider gets
        // an early slot equally often — budget truncation is spread
        // fairly across the id space instead of always falling on the
        // same names. Results are re-sorted below; the audit loop still
        // walks the sorted list.
        // (An empty registry would make `% n` a division by zero;
        // an empty rotation list is simply empty — nothing to rotate.)
        let n = provider_ids.len();
        let offset = if n == 0 {
            0
        } else {
            self.start_rotation
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed) as usize
                % n
        };
        let start_order: Vec<String> = (0..n)
            .map(|i| provider_ids[(i + offset) % n].clone())
            .collect();
        let mut listings = futures_util::stream::iter(
            start_order
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
                // Results arrive in completion order; the re-sort
                // below keeps advertised ORDER stable. Membership is
                // stable too once a provider has succeeded once
                // (last-good cache on budget truncation) — but a
                // never-yet-listed provider can be absent from a
                // given call while the budget is under pressure.
                results.push((provider, listed));
            }
        })
        .await;
        if let Err(_elapsed) = budget {
            // Budget exhausted. A provider that never got to answer is
            // NOT unhealthy — truncation is our scheduling artifact — so
            // its LAST GOOD listing is served instead (audited
            // `list_stale`): advertised tool membership stays stable
            // across calls, and only a provider with no cached
            // listing is actually skipped (`list_budget`).
            // Owned copies: `results` is pushed to inside the loop,
            // so the id list must not borrow it.
            let listed_ids: Vec<String> = results.iter().map(|(id, _)| id.clone()).collect();
            for provider in &provider_ids {
                if listed_ids.contains(provider) {
                    continue;
                }
                let cached = self.last_good_guard().get(provider).cloned();
                if let Some(cached) = cached {
                    tracing::warn!(
                        provider = %provider,
                        budget_ms = (cap + PROVIDER_LIST_BUDGET_SLACK).as_millis() as u64,
                        "dynamic provider listing budget exhausted; serving last good listing"
                    );
                    crate::mcp::audit::audit_deny(
                        "dynamic_provider_rejected",
                        "list_stale",
                        provider,
                    );
                    results.push((provider.clone(), Ok(Ok(cached))));
                } else {
                    tracing::warn!(
                        provider = %provider,
                        budget_ms = (cap + PROVIDER_LIST_BUDGET_SLACK).as_millis() as u64,
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
                Ok(Ok(listed)) => {
                    // Successful listings refresh the per-provider
                    // last-good cache (served on later budget truncation).
                    self.last_good_guard()
                        .insert(provider.clone(), listed.clone());
                    listed
                }
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

    /// Async provider whose listing never resolves (a true hang - a
    /// sync closure that blocked would freeze the current-thread
    /// runtime's timer wheel so the per-future cap could never fire).
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

    /// Async provider that lists one tool instantly.
    struct InstantProvider {
        id: &'static str,
        tool: &'static str,
    }
    #[async_trait]
    impl ConnectorProvider for InstantProvider {
        fn provider_id(&self) -> &str {
            self.id
        }
        async fn list_tools(&self) -> Result<Vec<ToolDescriptor>> {
            Ok(vec![descriptor(self.tool)])
        }
        async fn invoke(&self, _tool: &str, _args: Value) -> Result<ToolCallResult> {
            Ok(ToolCallResult {
                content: vec![],
                is_error: false,
            })
        }
    }

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
        // concurrency window of 8: wave 1 (8 hangs) resolves at its cap
        // (~200ms) and is collected as `list_timeout`; wave 2 (2 hangs)
        // starts next and would resolve at ~400ms — BUT the aggregate
        // budget fires first (cap 200ms + 100ms slack = 300ms), so the
        // wave-2 providers are skipped and audited `list_budget`.
        // Total ~300ms, INDEPENDENT of N. (The old serial loop: 10 x
        // 200ms = 2s. Neither bound alone is sufficient — that is why
        // both exist.)
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
            .expect("aggregate listing must survive ten hanging providers");
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
        // The aggregate budget (cap 200ms + 100ms slack = 300ms
        // here) bounds the WHOLE listing phase regardless of N. This
        // bound is deliberately loose (it pins the serial-loop
        // regression, 10 x 200ms = 2s); the DISCRIMINATING pin for
        // the budget is the `list_budget` audit below — a 2-wave
        // completion would finish with no `list_budget` event.
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

    /// Advertised MEMBERSHIP must stay stable across calls: once a
    /// provider has been successfully listed, a later call whose
    /// aggregate budget truncates that provider must serve its last
    /// good listing (`list_stale`) instead of silently dropping the
    /// tool — an MCP client caches `tools/list` to build its tool
    /// prompt, so a tool vanishing between identical calls against
    /// an unchanged registry is a correctness break, not just latency.
    #[tokio::test]
    async fn aggregate_tools_serves_last_good_listing_when_budget_truncates() {
        struct SlowGoodProvider;
        #[async_trait]
        impl ConnectorProvider for SlowGoodProvider {
            fn provider_id(&self) -> &str {
                "good"
            }
            async fn list_tools(&self) -> Result<Vec<ToolDescriptor>> {
                // Slow enough that it can only finish when it starts
                // early (wave 1): started late (wave 2 begins ~200ms
                // in), it would finish at ~350ms — past the 300ms
                // budget — so truncation, not completion, is the
                // deterministic outcome for a late start.
                tokio::time::sleep(Duration::from_millis(150)).await;
                Ok(vec![descriptor("ping")])
            }
            async fn invoke(&self, _tool: &str, _args: Value) -> Result<ToolCallResult> {
                Ok(ToolCallResult {
                    content: vec![],
                    is_error: false,
                })
            }
        }
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

        // "good" sorts first, so call 1 (rotation offset 0) starts it
        // in wave 1: it completes at ~150ms and enters the last-good
        // cache. Call 2 (offset 1) rotates it to start position 9 —
        // wave 2, starting at ~200ms — so the 300ms budget truncates
        // it and the cache must serve it. Ten providers total keep
        // the rotation arithmetic (0 and 1) deterministic.
        let mut registry = ProviderRegistry::with_list_timeout(Duration::from_millis(200));
        registry.register(Box::new(SlowGoodProvider));
        for id in [
            "hang-a", "hang-b", "hang-c", "hang-d", "hang-e", "hang-f", "hang-g", "hang-h",
            "hang-i",
        ] {
            registry.register(Box::new(HangingProvider(id)));
        }

        // Call 1: good completes (wave 1) and is cached.
        let first = registry
            .aggregate_tools()
            .await
            .expect("first aggregation must list the slow provider");
        let names: Vec<&str> = first.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"good.ping"), "{names:?}");

        // Call 2: good is rotated into wave 2 and truncated, but its
        // cached listing keeps it advertised.
        let second = registry
            .aggregate_tools()
            .await
            .expect("second aggregation must survive truncation");
        let names: Vec<&str> = second.iter().map(|t| t.name.as_str()).collect();
        assert!(
            names.contains(&"good.ping"),
            "budget truncation must serve the last good listing, not drop the tool: {names:?}"
        );
        // And the fallback is observable: the provider was NOT
        // silently dropped — it was audited `list_stale`.
        let stale = crate::services::audit::global()
            .recent(200)
            .into_iter()
            .any(|e| {
                e.action == "dynamic_provider_rejected"
                    && e.subject == "good"
                    && e.detail.contains("list_stale")
            });
        assert!(stale, "expected a list_stale audit event for 'good'");
    }

    /// CONTRACT: after unregister + re-register of the same id, the
    /// old instance's listing is never served - exercised on the
    /// BUDGET path, the only path that consults the cache. Call 1
    /// lists the instant provider (wave 1) and caches it; call 2
    /// rotates the re-registered hanging id into wave 2 where the
    /// budget truncates it, so a surviving entry WOULD have served
    /// the old tool (audited list_stale). Mutation note, kept honest:
    /// re-registering goes through `register`, whose own purge is the
    /// load-bearing line on this path (see the upsert test); the
    /// unregister purge's unique job is remove-without-re-add, where
    /// it prevents the entry from lingering in the map forever
    /// (unreachable from the listing loop, but an unbounded leak
    /// under remove/re-add churn).
    #[tokio::test]
    async fn unregister_purges_last_good_cache() {
        let mut registry = ProviderRegistry::with_list_timeout(Duration::from_millis(200));
        registry.register(Box::new(InstantProvider {
            id: "rebind",
            tool: "old",
        }));
        for id in [
            "hang-a", "hang-b", "hang-c", "hang-d", "hang-e", "hang-f", "hang-g", "hang-h",
            "hang-i",
        ] {
            registry.register(Box::new(HangingProvider(id)));
        }

        let first = registry
            .aggregate_tools()
            .await
            .expect("first aggregation lists the instant provider");
        assert!(first.iter().any(|t| t.name == "rebind.old"));

        assert!(registry.unregister("rebind"));
        registry.register(Box::new(HangingProvider("rebind")));

        // Offset 1 rotates `good` to start position 9 (wave 2): the
        // budget truncates it; the purge (if present) leaves the cache
        // empty for it.
        let second = registry
            .aggregate_tools()
            .await
            .expect("second aggregation survives");
        let names: Vec<&str> = second.iter().map(|t| t.name.as_str()).collect();
        assert!(
            !names.contains(&"rebind.old"),
            "unregister must purge the last-good cache; got: {names:?}"
        );
        let (stale, skipped) = crate::services::audit::global()
            .recent(200)
            .into_iter()
            .fold((false, false), |acc, e| {
                if e.action == "dynamic_provider_rejected" && e.subject == "rebind" {
                    (
                        acc.0 || e.detail.contains("list_stale"),
                        acc.1 || e.detail.contains("list_budget"),
                    )
                } else {
                    acc
                }
            });
        assert!(skipped, "expected a list_budget audit for 'good'");
        assert!(
            !stale,
            "a stale serve means the unregister purge is missing"
        );
    }

    /// The OTHER replacement path: register is an UPSERT -
    /// connector.composio_register writes composio:{label} straight
    /// into a live registry without unregistering first. The new
    /// instance must not inherit the previous instance's cache entry.
    /// Same discriminator, exercised through a bare re-register.
    #[tokio::test]
    async fn register_upsert_purges_last_good_cache() {
        let mut registry = ProviderRegistry::with_list_timeout(Duration::from_millis(200));
        registry.register(Box::new(InstantProvider {
            id: "rebind",
            tool: "old",
        }));
        for id in [
            "hang-a", "hang-b", "hang-c", "hang-d", "hang-e", "hang-f", "hang-g", "hang-h",
            "hang-i",
        ] {
            registry.register(Box::new(HangingProvider(id)));
        }

        let first = registry
            .aggregate_tools()
            .await
            .expect("first aggregation lists the instant provider");
        assert!(first.iter().any(|t| t.name == "rebind.old"));

        // Bare re-register (upsert) with a hanging instance - no
        // unregister call at all.
        registry.register(Box::new(HangingProvider("rebind")));

        let second = registry
            .aggregate_tools()
            .await
            .expect("second aggregation survives");
        let names: Vec<&str> = second.iter().map(|t| t.name.as_str()).collect();
        assert!(
            !names.contains(&"rebind.old"),
            "re-register must purge the previous instance's cache entry; got: {names:?}"
        );
        let stale = crate::services::audit::global()
            .recent(200)
            .into_iter()
            .any(|e| {
                e.action == "dynamic_provider_rejected"
                    && e.subject == "rebind"
                    && e.detail.contains("list_stale")
            });
        assert!(!stale, "a stale serve means the upsert purge is missing");
    }
}
