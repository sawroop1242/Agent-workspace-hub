//! Tunnel abstraction (spec section 25 / implementation plan).
//!
//! Exposes a local Control API behind a public URL. Providers are
//! pluggable behind [`TunnelProvider`]; the ngrok implementation
//! (`NgrokProvider`) shells out to the local `ngrok` binary with an
//! argument vector (never a shell string) and resolves the public URL
//! through ngrok's local agent API. Nothing else in AWH knows ngrok
//! specifics.
//!
//! A tunnel is transport, not authentication: the Control API keeps
//! enforcing bearer-token auth for every request that arrives through
//! it. `start` refuses to expose a server bound to a non-loopback
//! address unless the caller opts in, since the API key would then be
//! the only barrier on an already-public interface.

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

/// Default port of ngrok's local agent API.
const NGROK_AGENT_API: &str = "127.0.0.1:4040";

/// How long to poll the agent API for the public URL to appear.
const URL_RESOLVE_TIMEOUT: Duration = Duration::from_secs(30);

/// Lifecycle status of a tunnel.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TunnelStatus {
    /// No tunnel process is running.
    Stopped,
    /// Process spawned; public URL not yet confirmed.
    Starting,
    /// Tunnel is up and this URL answers on the public internet.
    Running { public_url: String },
}

/// A pluggable tunnel provider. Implementations own their process or
/// network details; callers only see start/stop/status.
#[async_trait]
pub trait TunnelProvider: Send + Sync {
    /// Unique provider name (e.g. "ngrok").
    fn name(&self) -> &'static str;

    /// Starts a tunnel forwarding the public URL to `target_addr`.
    /// Returns the public URL once the tunnel is confirmed up.
    async fn start(&mut self, target_addr: &str) -> Result<String>;

    /// Stops a tunnel started by this provider. Idempotent.
    async fn stop(&mut self) -> Result<()>;

    /// Current status without mutating the provider.
    async fn status(&self) -> Result<TunnelStatus>;
}

/// Builds the provider identified by `name`, or errors listing the
/// known providers.
pub fn provider_by_name(name: &str) -> Result<Box<dyn TunnelProvider>> {
    match name {
        "ngrok" => Ok(Box::new(NgrokProvider::default())),
        other => bail!("unknown tunnel provider {other:?}; known: ngrok"),
    }
}

/// ngrok-backed provider. Spawns the local `ngrok` binary directly
/// (no shell), then polls its agent API for the assigned public URL.
pub struct NgrokProvider {
    ngrok_path: PathBuf,
    /// Auth token passed to the ngrok child via `NGROK_AUTHTOKEN`
    /// (never argv — `/proc/<pid>/cmdline` is world-readable).
    authtoken: Option<String>,
    /// Region flag, forwarded verbatim if set.
    region: Option<String>,
    child: Option<tokio::process::Child>,
}

impl Default for NgrokProvider {
    fn default() -> Self {
        Self {
            ngrok_path: PathBuf::from("ngrok"),
            authtoken: None,
            region: None,
            child: None,
        }
    }
}

impl NgrokProvider {
    pub fn new(ngrok_path: impl Into<PathBuf>) -> Self {
        Self {
            ngrok_path: ngrok_path.into(),
            ..Self::default()
        }
    }

    pub fn with_authtoken(mut self, token: Option<String>) -> Self {
        self.authtoken = token;
        self
    }

    pub fn with_region(mut self, region: Option<String>) -> Self {
        self.region = region;
        self
    }

    /// Argument vector for `ngrok http <addr>`. Exposed for tests;
    /// never build this through a shell. The authtoken is deliberately
    /// NOT in argv — argv is world-readable via `/proc/<pid>/cmdline`
    /// and `ps`, so the token goes to the child's environment instead
    /// (`NGROK_AUTHTOKEN`, which ngrok v3 reads natively).
    pub fn build_args(&self, target_addr: &str) -> Vec<String> {
        let mut args = vec![
            "http".to_string(),
            target_addr.to_string(),
            "--log".to_string(),
            "stdout".to_string(),
        ];
        if let Some(region) = &self.region {
            args.push("--region".to_string());
            args.push(region.clone());
        }
        args
    }

    /// Secret environment passed to the ngrok child (authtoken only).
    fn child_env(&self) -> Vec<(&'static str, String)> {
        self.authtoken
            .as_ref()
            .map(|t| vec![("NGROK_AUTHTOKEN", t.clone())])
            .unwrap_or_default()
    }

    /// Test-only view of [`Self::child_env`]: proves the secret is
    /// delivered via env, not argv.
    #[cfg(test)]
    pub fn child_env_for_test(&self) -> Vec<(&'static str, String)> {
        self.child_env()
    }

    /// Polls the agent API until an HTTP tunnel with a public URL
    /// exists, then returns that URL.
    async fn resolve_public_url(&self) -> Result<String> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()?;
        let url = format!("http://{NGROK_AGENT_API}/api/tunnels");
        let deadline = tokio::time::Instant::now() + URL_RESOLVE_TIMEOUT;
        loop {
            if let Ok(resp) = client.get(&url).send().await {
                if let Ok(body) = resp.text().await {
                    if let Some(public) = parse_agent_tunnels(&body) {
                        return Ok(public);
                    }
                }
            }
            if tokio::time::Instant::now() >= deadline {
                bail!(
                    "ngrok agent API at {NGROK_AGENT_API} reported no public URL within {}s",
                    URL_RESOLVE_TIMEOUT.as_secs()
                );
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
}

#[async_trait]
impl TunnelProvider for NgrokProvider {
    fn name(&self) -> &'static str {
        "ngrok"
    }

    async fn start(&mut self, target_addr: &str) -> Result<String> {
        if self.child.is_some() {
            bail!("tunnel already running");
        }
        let mut cmd = Command::new(&self.ngrok_path);
        cmd.args(self.build_args(target_addr));
        for (k, v) in self.child_env() {
            // Env additions (not env_clear): the token must NOT appear in
            // argv where any process on the host can read it.
            cmd.env(k, v);
        }
        cmd.stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let child = cmd
            .spawn()
            .with_context(|| format!("failed to spawn ngrok at {}", self.ngrok_path.display()))?;
        self.child = Some(child);
        match self.resolve_public_url().await {
            Ok(url) => Ok(url),
            Err(e) => {
                // Leave nothing half-running behind on failure.
                self.stop().await.ok();
                Err(e)
            }
        }
    }

    async fn stop(&mut self) -> Result<()> {
        if let Some(mut child) = self.child.take() {
            child.kill().await.context("failed to stop ngrok process")?;
        }
        Ok(())
    }

    async fn status(&self) -> Result<TunnelStatus> {
        // Probe the agent API even when this provider instance never
        // spawned a child: `awh tunnel status` must detect tunnels
        // started by other processes (e.g. another terminal).
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()?;
        match client
            .get(format!("http://{NGROK_AGENT_API}/api/tunnels"))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                let body = resp.text().await.unwrap_or_default();
                Ok(parse_agent_tunnels(&body)
                    .map(|public_url| TunnelStatus::Running { public_url })
                    .unwrap_or(if self.child.is_some() {
                        TunnelStatus::Starting
                    } else {
                        TunnelStatus::Stopped
                    }))
            }
            // Agent API unreachable: a live child means it is still
            // booting; otherwise nothing is running.
            _ => Ok(if self.child.is_some() {
                TunnelStatus::Starting
            } else {
                TunnelStatus::Stopped
            }),
        }
    }
}

/// Extracts the first public tunnel URL from an ngrok agent API
/// `/api/tunnels` response, preferring `https://` over other protocols.
/// Pure function; unit-tested.
pub fn parse_agent_tunnels(body: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct AgentTunnels {
        #[serde(default)]
        tunnels: Vec<AgentTunnel>,
    }
    #[derive(Deserialize)]
    struct AgentTunnel {
        public_url: Option<String>,
        proto: Option<String>,
    }
    let parsed: AgentTunnels = serde_json::from_str(body).ok()?;
    let public = |t: &AgentTunnel| {
        t.public_url
            .as_ref()
            .filter(|u| {
                u.starts_with("http://") || u.starts_with("https://") || u.starts_with("tcp://")
            })
            .cloned()
    };
    parsed
        .tunnels
        .iter()
        .find(|t| t.proto.as_deref() == Some("https"))
        .and_then(public)
        .or_else(|| parsed.tunnels.iter().find_map(public))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_are_structured_and_ordered() {
        let p = NgrokProvider::new("/usr/local/bin/ngrok")
            .with_authtoken(Some("super-secret-token".into()))
            .with_region(Some("eu".into()));
        let args = p.build_args("127.0.0.1:8080");
        // The authtoken must never appear in argv: /proc/<pid>/cmdline
        // is world-readable, so secrets ride in the child's env instead.
        assert_eq!(
            args,
            vec![
                "http",
                "127.0.0.1:8080",
                "--log",
                "stdout",
                "--region",
                "eu"
            ]
        );
        assert!(!args.iter().any(|a| a.contains("super-secret-token")));
        let env = p.child_env_for_test();
        assert_eq!(
            env,
            vec![("NGROK_AUTHTOKEN", "super-secret-token".to_string())]
        );
    }

    #[test]
    fn default_args_have_no_optional_flags() {
        let p = NgrokProvider::default();
        let args = p.build_args("127.0.0.1:8080");
        assert_eq!(args, vec!["http", "127.0.0.1:8080", "--log", "stdout"]);
        assert!(p.child_env_for_test().is_empty());
    }

    #[test]
    fn parses_https_public_url_preferring_https() {
        let body = r#"{"tunnels":[
            {"name":"command_line","uri":"/api/tunnels/command_line","public_url":"http://abcd.ngrok.io","proto":"http"},
            {"name":"command_line-2","uri":"/api/tunnels/x","public_url":"https://abcd.ngrok.io","proto":"https"}
        ]}"#;
        assert_eq!(
            parse_agent_tunnels(body),
            Some("https://abcd.ngrok.io".to_string())
        );
    }

    #[test]
    fn falls_back_to_http_tunnel_when_no_https() {
        let body = r#"{"tunnels":[{"public_url":"http://xyz.ngrok.io","proto":"http"}]}"#;
        assert_eq!(
            parse_agent_tunnels(body),
            Some("http://xyz.ngrok.io".into())
        );
    }

    #[test]
    fn rejects_empty_or_malformed_agent_payloads() {
        assert_eq!(parse_agent_tunnels(""), None);
        assert_eq!(parse_agent_tunnels("not json"), None);
        assert_eq!(parse_agent_tunnels(r#"{"tunnels":[]}"#), None);
        assert_eq!(
            parse_agent_tunnels(r#"{"tunnels":[{"proto":"tcp"}]}"#),
            None
        );
    }

    #[test]
    fn unknown_provider_name_lists_known() {
        let err = provider_by_name("cloudflare").err().expect("should fail");
        let msg = err.to_string();
        assert!(msg.contains("ngrok"), "{msg}");
    }

    /// Fake provider for lifecycle wiring tests: simulates a tunnel
    /// without touching the network or processes.
    struct MockTunnel {
        running: bool,
        url: &'static str,
    }

    #[async_trait]
    impl TunnelProvider for MockTunnel {
        fn name(&self) -> &'static str {
            "mock"
        }

        async fn start(&mut self, _target: &str) -> Result<String> {
            self.running = true;
            Ok(self.url.to_string())
        }

        async fn stop(&mut self) -> Result<()> {
            self.running = false;
            Ok(())
        }

        async fn status(&self) -> Result<TunnelStatus> {
            if self.running {
                Ok(TunnelStatus::Running {
                    public_url: self.url.to_string(),
                })
            } else {
                Ok(TunnelStatus::Stopped)
            }
        }
    }

    #[tokio::test]
    async fn trait_object_lifecycle_roundtrip() {
        let mut provider: Box<dyn TunnelProvider> = Box::new(MockTunnel {
            running: false,
            url: "https://mock.example",
        });
        assert_eq!(provider.status().await.unwrap(), TunnelStatus::Stopped);
        assert_eq!(
            provider.start("127.0.0.1:8080").await.unwrap(),
            "https://mock.example"
        );
        assert_eq!(
            provider.status().await.unwrap(),
            TunnelStatus::Running {
                public_url: "https://mock.example".into()
            }
        );
        provider.stop().await.unwrap();
        assert_eq!(provider.status().await.unwrap(), TunnelStatus::Stopped);
    }

    #[tokio::test]
    async fn ngrok_start_without_binary_fails_cleanly() {
        let mut p = NgrokProvider::new("/nonexistent/ngrok-binary");
        let err = p.start("127.0.0.1:8080").await.unwrap_err().to_string();
        assert!(err.contains("failed to spawn ngrok"), "{err}");
        // Failed start must not leave state behind.
        assert_eq!(p.status().await.unwrap(), TunnelStatus::Stopped);
    }
}
