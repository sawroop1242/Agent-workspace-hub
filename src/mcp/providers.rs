use anyhow::{bail, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor { pub name: String, pub description: String, pub input_schema: Value }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult { pub content: Vec<ToolContent> }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ToolContent { Text { text: String }, Json { json: Value } }

#[async_trait]
pub trait ConnectorProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    async fn list_tools(&self) -> Result<Vec<ToolDescriptor>>;
    async fn invoke(&self, tool: &str, arguments: Value) -> Result<ToolCallResult>;
}

#[derive(Default)]
pub struct ProviderRegistry { providers: HashMap<String, Box<dyn ConnectorProvider>> }
impl ProviderRegistry {
    pub fn register(&mut self, provider: Box<dyn ConnectorProvider>) { self.providers.insert(provider.provider_id().to_string(), provider); }
    pub fn providers(&self) -> Vec<String> { self.providers.keys().cloned().collect() }
    pub async fn tools(&self, provider: &str) -> Result<Vec<ToolDescriptor>> { self.providers.get(provider).ok_or_else(|| anyhow::anyhow!("provider not registered: {provider}"))?.list_tools().await }
    pub async fn invoke(&self, provider: &str, tool: &str, arguments: Value) -> Result<ToolCallResult> { if tool.is_empty(){bail!("tool name is required")} self.providers.get(provider).ok_or_else(|| anyhow::anyhow!("provider not registered: {provider}"))?.invoke(tool, arguments).await }
}

pub struct UnconfiguredProvider { id: String }
impl UnconfiguredProvider { pub fn new(id: impl Into<String>) -> Self { Self { id: id.into() } } }
#[async_trait]
impl ConnectorProvider for UnconfiguredProvider {
    fn provider_id(&self) -> &str { &self.id }
    async fn list_tools(&self) -> Result<Vec<ToolDescriptor>> { Ok(Vec::new()) }
    async fn invoke(&self, _tool: &str, _arguments: Value) -> Result<ToolCallResult> { bail!("provider '{}' is not configured", self.id) }
}

pub struct CustomMcpProvider { id: String, client: std::sync::Arc<crate::mcp::StdioMcpClient> }
impl CustomMcpProvider { pub fn new(id: impl Into<String>, client: std::sync::Arc<crate::mcp::StdioMcpClient>) -> Self { Self { id: id.into(), client } } }
#[async_trait]
impl ConnectorProvider for CustomMcpProvider {
    fn provider_id(&self) -> &str { &self.id }
    async fn list_tools(&self) -> Result<Vec<ToolDescriptor>> {
        let value = self.client.tools_list().await?;
        let tools = value.get("tools").cloned().unwrap_or_else(|| Value::Array(vec![]));
        Ok(serde_json::from_value(tools)?)
    }
    async fn invoke(&self, tool: &str, arguments: Value) -> Result<ToolCallResult> {
        Ok(serde_json::from_value(self.client.tools_call(tool, arguments).await?)?)
    }
}

/// Generic provider adapter for an external tool gateway such as Composio.
pub struct GatewayProvider<F, G> where F: Fn() -> Result<Vec<ToolDescriptor>> + Send + Sync, G: Fn(&str, Value) -> Result<ToolCallResult> + Send + Sync {
    id: String, list_fn: F, invoke_fn: G,
}
impl<F, G> GatewayProvider<F, G> where F: Fn() -> Result<Vec<ToolDescriptor>> + Send + Sync, G: Fn(&str, Value) -> Result<ToolCallResult> + Send + Sync {
    pub fn new(id: impl Into<String>, list_fn: F, invoke_fn: G) -> Self { Self { id: id.into(), list_fn, invoke_fn } }
}
impl<F, G> ConnectorProvider for GatewayProvider<F, G> where F: Fn() -> Result<Vec<ToolDescriptor>> + Send + Sync, G: Fn(&str, Value) -> Result<ToolCallResult> + Send + Sync {
    fn provider_id(&self) -> &str { &self.id }
    async fn list_tools(&self) -> Result<Vec<ToolDescriptor>> { (self.list_fn)() }
    async fn invoke(&self, tool: &str, arguments: Value) -> Result<ToolCallResult> { (self.invoke_fn)(tool, arguments) }
}
