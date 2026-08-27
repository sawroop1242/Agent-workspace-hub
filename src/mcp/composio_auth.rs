use anyhow::{anyhow, bail, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const BASE_URL: &str = "https://backend.composio.dev/api/v3.1";

/// An authorization link returned by the Composio Auth Link API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthLink {
    pub redirect_url: String,
    pub connected_account_id: Option<String>,
}

/// A connected Composio account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectedAccount {
    pub id: String,
    pub status: Option<String>,
    pub toolkit: Option<Value>,
    pub user_id: Option<String>,
}

/// Client for Composio connected-account authentication.
pub struct ComposioAuth {
    client: Client,
    api_key: String,
    base_url: String,
}

impl ComposioAuth {
    /// Builds a client from the `COMPOSIO_API_KEY` env var.
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("COMPOSIO_API_KEY")
            .map_err(|_| anyhow!("COMPOSIO_API_KEY is not configured"))?;
        Ok(Self {
            client: Client::new(),
            api_key,
            base_url: BASE_URL.to_string(),
        })
    }

    /// Creates a client with an explicit API key and base URL.
    pub fn with_base_url(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            base_url: base_url.into(),
        }
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        self.client
            .request(method, format!("{}{}", self.base_url, path))
            .header("x-api-key", &self.api_key)
            .header("Content-Type", "application/json")
    }

    /// Creates a link for connecting an account and returns the redirect URL.
    pub async fn create_link(
        &self,
        auth_config_id: &str,
        user_id: &str,
        alias: Option<&str>,
        callback_url: Option<&str>,
    ) -> Result<AuthLink> {
        if auth_config_id.is_empty() || user_id.is_empty() {
            bail!("auth_config_id and user_id are required");
        }
        let mut body = serde_json::json!({ "auth_config": auth_config_id, "user_id": user_id });
        if let Some(v) = alias {
            body["alias"] = Value::String(v.to_string());
        }
        if let Some(v) = callback_url {
            body["callback_url"] = Value::String(v.to_string());
        }
        let response: Value = self
            .request(reqwest::Method::POST, "/connected_accounts/link")
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let redirect_url = response
            .get("redirect_url")
            .or_else(|| response.get("data").and_then(|d| d.get("redirect_url")))
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("Composio response did not contain redirect_url"))?
            .to_string();
        let connected_account_id = response
            .get("connected_account_id")
            .or_else(|| {
                response
                    .get("data")
                    .and_then(|d| d.get("connected_account_id"))
            })
            .and_then(Value::as_str)
            .map(str::to_string);
        Ok(AuthLink {
            redirect_url,
            connected_account_id,
        })
    }

    /// Lists connected accounts, optionally filtered by user and toolkit.
    pub async fn list_accounts(
        &self,
        user_id: Option<&str>,
        toolkit: Option<&str>,
    ) -> Result<Vec<ConnectedAccount>> {
        let mut query = Vec::new();
        if let Some(v) = user_id {
            query.push(("user_id", v));
        }
        if let Some(v) = toolkit {
            query.push(("toolkit", v));
        }
        let response: Value = self
            .request(reqwest::Method::GET, "/connected_accounts")
            .query(&query)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let items = response
            .get("items")
            .or_else(|| response.get("data"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(items
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect())
    }

    /// Fetches a single connected account by id.
    pub async fn get_account(&self, account_id: &str) -> Result<ConnectedAccount> {
        if account_id.is_empty() {
            bail!("account_id is required");
        }
        Ok(self
            .request(
                reqwest::Method::GET,
                &format!("/connected_accounts/{account_id}"),
            )
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    /// Deletes a connected account by id.
    pub async fn delete_account(&self, account_id: &str) -> Result<()> {
        if account_id.is_empty() {
            bail!("account_id is required");
        }
        self.request(
            reqwest::Method::DELETE,
            &format!("/connected_accounts/{account_id}"),
        )
        .send()
        .await?
        .error_for_status()?;
        Ok(())
    }
}
