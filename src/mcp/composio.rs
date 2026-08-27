use crate::mcp::providers::{ConnectorProvider, ToolCallResult, ToolContent, ToolDescriptor};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use reqwest::blocking::Client;
use serde_json::{json, Value};

const BASE_URL: &str = "https://backend.composio.dev/api/v3.1";

/// Composio REST provider. The API key is read from the environment and is never
/// persisted in the workspace registry. Connected-account credentials remain in Composio.
pub struct ComposioProvider {
    api_key: String,
    connected_account_id: Option<String>,
    toolkit: Option<String>,
    client: Client,
}

impl ComposioProvider {
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("COMPOSIO_API_KEY")
            .context("COMPOSIO_API_KEY is required to enable the Composio provider")?;
        if api_key.trim().is_empty() {
            bail!("COMPOSIO_API_KEY is empty");
        }
        Ok(Self::new(
            api_key,
            std::env::var("COMPOSIO_CONNECTED_ACCOUNT_ID").ok(),
            std::env::var("COMPOSIO_TOOLKIT").ok(),
        ))
    }

    pub fn new(
        api_key: String,
        connected_account_id: Option<String>,
        toolkit: Option<String>,
    ) -> Self {
        Self {
            api_key,
            connected_account_id,
            toolkit,
            client: Client::new(),
        }
    }

    fn request(&self, builder: reqwest::blocking::RequestBuilder) -> Result<Value> {
        let response = builder
            .header("x-api-key", &self.api_key)
            .header("content-type", "application/json")
            .send()
            .context("Composio HTTP request failed")?;
        let status = response.status();
        let body: Value = response
            .json()
            .context("Composio returned invalid JSON")?;
        if !status.is_success() {
            bail!("Composio API returned {status}: {body}");
        }
        Ok(body)
    }
}

#[async_trait]
impl ConnectorProvider for ComposioProvider {
    fn provider_id(&self) -> &str {
        "composio"
    }

    fn list_tools(&self) -> Result<Vec<ToolDescriptor>> {
        let mut req = self.client.get(format!("{BASE_URL}/tools"));
        if let Some(toolkit) = &self.toolkit {
            req = req.query(&[("toolkit", toolkit)]);
        }
        let body = self.request(req)?;
        let items = body
            .get("items")
            .or_else(|| body.get("data"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(items
            .into_iter()
            .filter_map(|item| {
                let name = item
                    .get("slug")
                    .or_else(|| item.get("tool_slug"))
                    .and_then(Value::as_str)?
                    .to_string();
                Some(ToolDescriptor {
                    name,
                    description: item
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    input_schema: item
                        .get("input_parameters")
                        .or_else(|| item.get("input_schema"))
                        .cloned()
                        .unwrap_or_else(|| {
                            json!({"type": "object", "properties": {}})
                        }),
                })
            })
            .collect())
    }

    fn invoke(&self, tool: &str, arguments: Value) -> Result<ToolCallResult> {
        if tool.trim().is_empty() {
            bail!("Composio tool slug is required");
        }
        let mut payload = json!({"arguments": arguments, "version": "latest"});
        if let Some(account) = &self.connected_account_id {
            payload["connected_account_id"] = json!(account);
        }
        let body = self.request(
            self.client
                .post(format!("{BASE_URL}/tools/execute/{tool}"))
                .json(&payload),
        )?;
        Ok(ToolCallResult {
            content: vec![ToolContent::Json { json: body }],
            is_error: false,
        })
    }
}
