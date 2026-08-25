use anyhow::{bail, Result};
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

pub trait ConnectorProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    fn list_tools(&self) -> Result<Vec<ToolDescriptor>>;
    fn invoke(&self, tool: &str, arguments: Value) -> Result<ToolCallResult>;
}

#[derive(Default)]
pub struct ProviderRegistry { providers: HashMap<String, Box<dyn ConnectorProvider>> }
impl ProviderRegistry {
    pub fn register(&mut self, provider: Box<dyn ConnectorProvider>) { self.providers.insert(provider.provider_id().to_string(), provider); }
    pub fn providers(&self) -> Vec<String> { self.providers.keys().cloned().collect() }
    pub fn tools(&self, provider: &str) -> Result<Vec<ToolDescriptor>> { self.providers.get(provider).map(|p| p.list_tools()).transpose()?.ok_or_else(|| anyhow::anyhow!("provider not registered: {provider}")) }
    pub fn invoke(&self, provider: &str, tool: &str, arguments: Value) -> Result<ToolCallResult> { let p=self.providers.get(provider).ok_or_else(|| anyhow::anyhow!("provider not registered: {provider}"))?; if tool.is_empty(){bail!("tool name is required")} p.invoke(tool, arguments) }
}

pub struct UnconfiguredProvider { id: String }
impl UnconfiguredProvider { pub fn new(id: impl Into<String>) -> Self { Self { id: id.into() } } }
impl ConnectorProvider for UnconfiguredProvider {
    fn provider_id(&self) -> &str { &self.id }
    fn list_tools(&self) -> Result<Vec<ToolDescriptor>> { Ok(Vec::new()) }
    fn invoke(&self, _tool: &str, _arguments: Value) -> Result<ToolCallResult> { bail!("provider '{}' is not configured", self.id) }
}

/// Generic provider adapter for an external tool gateway such as Composio.
/// The transport is injected by the application; this layer never stores credentials.
pub struct GatewayProvider<F, G> where F: Fn() -> Result<Vec<ToolDescriptor>> + Send + Sync, G: Fn(&str, Value) -> Result<ToolCallResult> + Send + Sync {
    id: String,
    list_fn: F,
    invoke_fn: G,
}
impl<F, G> GatewayProvider<F, G> where F: Fn() -> Result<Vec<ToolDescriptor>> + Send + Sync, G: Fn(&str, Value) -> Result<ToolCallResult> + Send + Sync {
    pub fn new(id: impl Into<String>, list_fn: F, invoke_fn: G) -> Self { Self { id: id.into(), list_fn, invoke_fn } }
}
impl<F, G> ConnectorProvider for GatewayProvider<F, G> where F: Fn() -> Result<Vec<ToolDescriptor>> + Send + Sync, G: Fn(&str, Value) -> Result<ToolCallResult> + Send + Sync {
    fn provider_id(&self) -> &str { &self.id }
    fn list_tools(&self) -> Result<Vec<ToolDescriptor>> { (self.list_fn)() }
    fn invoke(&self, tool: &str, arguments: Value) -> Result<ToolCallResult> { (self.invoke_fn)(tool, arguments) }
}
