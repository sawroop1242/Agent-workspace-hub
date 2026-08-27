use anyhow::{bail, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "inputSchema", alias = "input_schema")]
    pub input_schema: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub content: Vec<ToolContent>,
    #[serde(default, rename = "isError")]
    pub is_error: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ToolContent {
    Text { text: String },
    Json { json: Value },
}

#[async_trait]
pub trait ConnectorProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    async fn list_tools(&self) -> Result<Vec<ToolDescriptor>>;
    async fn invoke(&self, tool: &str, arguments: Value) -> Result<ToolCallResult>;
}
#[derive(Default)]
pub struct ProviderRegistry {
    providers: HashMap<String, Box<dyn ConnectorProvider>>,
}
impl ProviderRegistry {
    pub fn register(&mut self, p: Box<dyn ConnectorProvider>) {
        self.providers.insert(p.provider_id().to_string(), p);
    }
    pub fn providers(&self) -> Vec<String> {
        let mut v: Vec<_> = self.providers.keys().cloned().collect();
        v.sort();
        v
    }
    pub async fn tools(&self, provider: &str) -> Result<Vec<ToolDescriptor>> {
        self.providers
            .get(provider)
            .ok_or_else(|| anyhow::anyhow!("provider not registered: {provider}"))?
            .list_tools()
            .await
    }
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
    pub async fn aggregate_tools(&self) -> Result<Vec<ToolDescriptor>> {
        let mut out = Vec::new();
        for provider in self.providers() {
            for mut tool in self.tools(&provider).await? {
                tool.name = format!("{}.{}", provider, tool.name);
                if tool.description.is_empty() {
                    tool.description = format!("Tool provided by {provider}");
                }
                out.push(tool);
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }
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

pub struct UnconfiguredProvider {
    id: String,
}
impl UnconfiguredProvider {
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

pub struct CustomMcpProvider<C> {
    id: String,
    client: std::sync::Arc<C>,
}
impl<C> CustomMcpProvider<C> {
    pub fn new(id: impl Into<String>, client: std::sync::Arc<C>) -> Self {
        Self {
            id: id.into(),
            client,
        }
    }
}
#[async_trait]
pub trait McpClient: Send + Sync {
    async fn tools_list(&self) -> Result<Value>;
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
