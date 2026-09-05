//! Remote TUI backend: drives every screen over the versioned HTTP
//! Control API (`/api/v1`) instead of touching the local filesystem.
//!
//! [`RemoteBackend`] implements [`WorkspaceBackend`] with `reqwest`
//! blocking calls. The TUI event loop is synchronous, so a blocking
//! client with a request timeout matches the loop's contract exactly
//! the way [`LocalBackend`]'s owned tokio runtime does. Every operation
//! runs with the operator's bearer token, so the server-side auth,
//! rate-limit, audit, and traversal rules apply to all of it.

use anyhow::{anyhow, Context, Result};
use std::path::PathBuf;
use std::time::Duration;

use crate::models::MemoryEntry;
use crate::services::files::{FileMeta, ListEntry, SearchHit};
use crate::services::git::GitOutput;
use crate::services::terminal::ExecOutcome;
use crate::skills::Skill;
use crate::tui::backend::{DashboardSnapshot, WorkspaceBackend};

use serde::Deserialize;

/// Upper bound for one remote request before the TUI reports failure.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// One MCP server row for the MCP screen.
#[derive(Debug, Clone, Deserialize)]
pub struct McpInfo {
    pub id: String,
    pub transport: String,
    pub enabled: bool,
    #[serde(default)]
    pub version: String,
}

/// Outcome of a connection attempt against a remote AWH API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    /// No connection attempt has been made yet.
    Local,
    /// Currently performing the handshake.
    Connecting,
    /// Handshake succeeded; the server reports this version.
    Connected { version: String, uptime: u64 },
    /// The server is reachable but rejected the API key.
    AuthFailed,
    /// No HTTP response at all (DNS, refused connection, timeout, TLS).
    Unavailable { reason: String },
    /// The server answered but its protocol version is incompatible.
    Incompatible { server_version: String },
}

/// Client for the AWH Control API over HTTP, used by the TUI.
pub struct RemoteBackend {
    /// Base URL, e.g. `https://host:8080` (no trailing slash, no path).
    base: String,
    api_key: String,
    client: reqwest::blocking::Client,
    current_project: Option<String>,
}

impl RemoteBackend {
    /// Builds the client without contacting the server.
    pub fn new(base: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            base: base.into().trim_end_matches('/').to_owned(),
            api_key: api_key.into(),
            client: reqwest::blocking::Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .build()
                .expect("reqwest blocking client"),
            current_project: None,
        }
    }

    /// Base URL this backend talks to.
    pub fn base(&self) -> &str {
        &self.base
    }

    /// Runs the connection handshake:
    /// `/healthz` (public liveness) then `/status` (authenticated, and
    /// the compatibility signal). Returns the live connection state.
    pub fn probe(&self) -> ConnectionState {
        // Liveness first: distinguishes "server down" from "wrong key".
        let health = self.client.get(self.url("/api/v1/healthz")).send();
        match health {
            Err(e) => {
                return ConnectionState::Unavailable {
                    reason: describe_reqwest(&e),
                }
            }
            Ok(resp) if !resp.status().is_success() => {
                return ConnectionState::Unavailable {
                    reason: format!("health check returned HTTP {}", resp.status()),
                };
            }
            Ok(_) => {}
        }
        match self.get_json::<serde_json::Value>("/api/v1/status") {
            Ok(status) => {
                let server_version = status
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_owned();
                let uptime = status
                    .get("uptime_secs")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                if version_compatible(&server_version, env!("CARGO_PKG_VERSION")) {
                    ConnectionState::Connected {
                        version: server_version,
                        uptime,
                    }
                } else {
                    ConnectionState::Incompatible { server_version }
                }
            }
            Err(e) if is_auth_error(&e) => ConnectionState::AuthFailed,
            Err(e) => ConnectionState::Unavailable {
                reason: e.to_string(),
            },
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    /// Authenticated GET expecting a JSON body.
    fn get_json<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T> {
        let resp = self
            .client
            .get(self.url(path))
            .bearer_auth(&self.api_key)
            .send()
            .map_err(|e| anyhow!("connection failed: {}", describe_reqwest(&e)))?;
        Self::decode(resp)
    }

    /// Authenticated request with a JSON body, expecting a JSON response.
    fn send_json<T: for<'de> Deserialize<'de>>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<T> {
        let resp = self
            .client
            .request(method, self.url(path))
            .bearer_auth(&self.api_key)
            .json(body)
            .send()
            .map_err(|e| anyhow!("connection failed: {}", describe_reqwest(&e)))?;
        Self::decode(resp)
    }

    /// Authenticated request with no body (POST/PUT/DELETE), JSON response.
    fn send_empty<T: for<'de> Deserialize<'de>>(
        &self,
        method: reqwest::Method,
        path: &str,
    ) -> Result<T> {
        let resp = self
            .client
            .request(method, self.url(path))
            .bearer_auth(&self.api_key)
            .send()
            .map_err(|e| anyhow!("connection failed: {}", describe_reqwest(&e)))?;
        Self::decode(resp)
    }

    /// Turns an HTTP response into `T`, mapping API errors to readable
    /// anyhow errors that carry the server's own message.
    fn decode<T: for<'de> Deserialize<'de>>(resp: reqwest::blocking::Response) -> Result<T> {
        let status = resp.status();
        let text = resp
            .text()
            .map_err(|e| anyhow!("reading response body failed: {e}"))?;
        if !status.is_success() {
            let message = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| {
                    v.get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|m| m.as_str())
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| format!("HTTP {status}"));
            return Err(RemoteError {
                status: status.as_u16(),
                message,
            }
            .into());
        }
        serde_json::from_str(&text)
            .with_context(|| format!("malformed response from server: {status}"))
    }
}

/// Transport-level error carrying the HTTP status, so callers can
/// classify 401 (auth) and 429 (rate limited) without string matching.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
struct RemoteError {
    status: u16,
    message: String,
}

impl RemoteError {
    fn is_auth(&self) -> bool {
        self.status == 401
    }
}

/// Whether an error chain contains an auth rejection (HTTP 401).
fn is_auth_error(e: &anyhow::Error) -> bool {
    e.chain()
        .filter_map(|c| c.downcast_ref::<RemoteError>())
        .any(RemoteError::is_auth)
}

/// Human-friendly one-line description of a reqwest failure without
/// leaking URL credentials (the API key travels in a header, not the
/// URL, and reqwest errors never echo headers).
fn describe_reqwest(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "request timed out".to_owned()
    } else if e.is_connect() {
        "server unreachable".to_owned()
    } else {
        e.to_string()
    }
}

/// Compatibility check: the client and server must share the first
/// two version components (x.y). Patch/RC drift is tolerated.
fn version_compatible(server: &str, client: &str) -> bool {
    let head = |s: &str| s.split(['.', '-']).take(2).collect::<Vec<_>>().join(".");
    head(server) == head(client)
}

#[derive(Deserialize)]
struct StatusResponse {
    #[serde(default)]
    version: String,
    #[serde(default)]
    projects: usize,
    #[serde(default)]
    git_repo: bool,
    #[serde(default)]
    root_name: String,
}

#[derive(Deserialize)]
struct ProjectsResponse {
    projects: Vec<String>,
}

#[derive(Deserialize)]
struct EntriesResponse {
    entries: Vec<ListEntry>,
}

#[derive(Deserialize)]
struct ContentResponse {
    content: String,
}

#[derive(Deserialize)]
struct MetaResponse {
    kind: String,
    size: u64,
}

#[derive(Deserialize)]
struct SearchResponse {
    hits: Vec<SearchHit>,
}

#[derive(Deserialize)]
struct ContextResponse {
    content: String,
}

#[derive(Deserialize)]
struct MemoryResponse {
    entries: Vec<MemoryEntry>,
}

#[derive(Deserialize)]
struct SkillsResponse {
    skills: Vec<SkillWire>,
}

/// The API serializes skills without the on-disk path; the remote
/// screen only needs the descriptive fields.
#[derive(Debug, Clone, Deserialize)]
struct SkillWire {
    name: String,
    description: String,
    version: Option<String>,
}

#[derive(Deserialize)]
struct McpResponse {
    servers: Vec<McpInfo>,
}

#[derive(Deserialize)]
struct AuditResponse {
    events: Vec<AuditWire>,
}

#[derive(Debug, Clone, Deserialize)]
struct AuditWire {
    kind: String,
    action: String,
    subject: String,
    detail: String,
}

fn err_remote(op: &str, e: anyhow::Error) -> anyhow::Error {
    anyhow!("remote {op} failed: {e:#}")
}

impl WorkspaceBackend for RemoteBackend {
    fn dashboard(&self) -> Result<DashboardSnapshot> {
        let status: StatusResponse = self
            .get_json("/api/v1/status")
            .map_err(|e| err_remote("status", e))?;
        let (branch, dirty_entries) = self
            .get_json::<GitOutput>("/api/v1/git/branch")
            .ok()
            .zip(self.get_json::<GitOutput>("/api/v1/git/status").ok())
            .map(|(b, s)| {
                (
                    (!b.stdout.trim().is_empty()).then(|| b.stdout.trim().to_owned()),
                    s.porcelain_entries().len(),
                )
            })
            .unwrap_or((None, 0));
        let recent = self
            .get_json::<AuditResponse>("/api/v1/audit?limit=5")
            .map(|a| {
                a.events
                    .into_iter()
                    .map(|e| format!("{} {} {} ({})", e.kind, e.action, e.subject, e.detail))
                    .collect()
            })
            .unwrap_or_default();
        Ok(DashboardSnapshot {
            root: PathBuf::from(&status.root_name),
            project_count: status.projects,
            is_git_repo: status.git_repo,
            branch,
            dirty_entries,
            warnings: Vec::new(),
            current_project: self.current_project.clone(),
            running_sessions: 0,
            mcp_status: format!("remote API {}", status.version),
            api_status: format!("connected to {}", self.base),
            recent_activity: recent,
        })
    }

    fn list_projects(&self) -> Result<Vec<String>> {
        self.get_json::<ProjectsResponse>("/api/v1/projects")
            .map(|r| r.projects)
            .map_err(|e| err_remote("list projects", e))
    }

    fn create_project(&self, name: &str) -> Result<()> {
        self.send_json::<serde_json::Value>(
            reqwest::Method::POST,
            "/api/v1/projects",
            &serde_json::json!({ "name": name }),
        )
        .map_err(|e| err_remote("create project", e))?;
        Ok(())
    }

    fn delete_project(&self, name: &str) -> Result<()> {
        let path = format!("/api/v1/projects/{}", urlencode(name));
        self.send_empty::<serde_json::Value>(reqwest::Method::DELETE, &path)
            .map_err(|e| err_remote("delete project", e))?;
        Ok(())
    }

    fn open_project(&mut self, name: &str) -> Result<()> {
        let path = format!("/api/v1/projects/{}", urlencode(name));
        self.get_json::<serde_json::Value>(&path)
            .map_err(|e| err_remote("open project", e))?;
        self.current_project = Some(name.to_owned());
        Ok(())
    }

    fn list_dir(&self, relative: &str) -> Result<Vec<ListEntry>> {
        let path = format!("/api/v1/files?dir={}", urlencode(relative));
        self.get_json::<EntriesResponse>(&path)
            .map(|r| r.entries)
            .map_err(|e| err_remote("list files", e))
    }

    fn read_file(&self, relative: &str) -> Result<String> {
        let path = format!("/api/v1/files/content?path={}", urlencode(relative));
        self.get_json::<ContentResponse>(&path)
            .map(|r| r.content)
            .map_err(|e| err_remote("read file", e))
    }

    fn write_file(&self, relative: &str, content: &str) -> Result<bool> {
        self.send_json::<serde_json::Value>(
            reqwest::Method::PUT,
            "/api/v1/files/content",
            &serde_json::json!({ "path": relative, "content": content }),
        )
        .map_err(|e| err_remote("write file", e))?;
        Ok(true)
    }

    fn delete_path(&self, relative: &str) -> Result<()> {
        let path = format!("/api/v1/files/entry?path={}", urlencode(relative));
        self.send_empty::<serde_json::Value>(reqwest::Method::DELETE, &path)
            .map_err(|e| err_remote("delete file", e))?;
        Ok(())
    }

    fn rename_path(&self, from: &str, to: &str) -> Result<()> {
        self.send_json::<serde_json::Value>(
            reqwest::Method::POST,
            "/api/v1/files/entry",
            &serde_json::json!({ "path": from, "new_name": to }),
        )
        .map_err(|e| err_remote("rename", e))?;
        Ok(())
    }

    fn create_dir(&self, relative: &str) -> Result<()> {
        self.send_json::<serde_json::Value>(
            reqwest::Method::PUT,
            "/api/v1/files/entry",
            &serde_json::json!({ "path": relative }),
        )
        .map_err(|e| err_remote("create directory", e))?;
        Ok(())
    }

    fn meta(&self, relative: &str) -> Result<FileMeta> {
        let path = format!("/api/v1/files/meta?path={}", urlencode(relative));
        let m = self
            .get_json::<MetaResponse>(&path)
            .map_err(|e| err_remote("file meta", e))?;
        Ok(FileMeta {
            kind: parse_kind(&m.kind),
            size: m.size,
        })
    }

    fn search_files(&self, needle: &str, limit: usize) -> Result<Vec<SearchHit>> {
        // The API caps search at 100 hits; respect the caller's cap.
        let capped = format!("/api/v1/files/search?q={}", urlencode(needle));
        let hits = self
            .get_json::<SearchResponse>(&capped)
            .map(|r| r.hits)
            .map_err(|e| err_remote("search", e))?;
        Ok(hits.into_iter().take(limit).collect())
    }

    fn git_status(&self) -> Result<GitOutput> {
        self.get_json("/api/v1/git/status")
            .map_err(|e| err_remote("git status", e))
    }

    fn git_log(&self, limit: usize) -> Result<GitOutput> {
        self.get_json(&format!("/api/v1/git/log?limit={limit}"))
            .map_err(|e| err_remote("git log", e))
    }

    fn git_commit(&self, message: &str) -> Result<GitOutput> {
        self.send_json::<GitOutput>(
            reqwest::Method::POST,
            "/api/v1/git/commit",
            &serde_json::json!({ "message": message }),
        )
        .map_err(|e| err_remote("git commit", e))
    }

    fn git_stage(&self, path: &str) -> Result<GitOutput> {
        self.send_json::<GitOutput>(
            reqwest::Method::POST,
            "/api/v1/git/stage",
            &serde_json::json!({ "path": path }),
        )
        .map_err(|e| err_remote("git stage", e))
    }

    fn git_unstage(&self, path: &str) -> Result<GitOutput> {
        self.send_json::<GitOutput>(
            reqwest::Method::POST,
            "/api/v1/git/unstage",
            &serde_json::json!({ "path": path }),
        )
        .map_err(|e| err_remote("git unstage", e))
    }

    fn git_diff(&self, staged: bool, path: Option<&str>) -> Result<GitOutput> {
        let mut query = format!("/api/v1/git/diff?staged={staged}");
        if let Some(p) = path {
            query.push_str("&path=");
            query.push_str(&urlencode(p));
        }
        self.get_json(&query).map_err(|e| err_remote("git diff", e))
    }

    fn git_branches(&self) -> Result<GitOutput> {
        self.get_json("/api/v1/git/branches")
            .map_err(|e| err_remote("git branches", e))
    }

    fn git_push(&self, remote: &str, branch: &str) -> Result<GitOutput> {
        self.send_json::<GitOutput>(
            reqwest::Method::POST,
            &format!(
                "/api/v1/git/push?remote={}&branch={}",
                urlencode(remote),
                urlencode(branch)
            ),
            &serde_json::json!({}),
        )
        .map_err(|e| err_remote("git push", e))
    }

    fn git_pull(&self, remote: &str, branch: &str) -> Result<GitOutput> {
        self.send_json::<GitOutput>(
            reqwest::Method::POST,
            &format!(
                "/api/v1/git/pull?remote={}&branch={}",
                urlencode(remote),
                urlencode(branch)
            ),
            &serde_json::json!({}),
        )
        .map_err(|e| err_remote("git pull", e))
    }

    fn terminal_run(&self, program: &str, args: &[String]) -> Result<ExecOutcome> {
        self.send_json::<ExecOutcome>(
            reqwest::Method::POST,
            "/api/v1/terminal/run",
            &serde_json::json!({ "program": program, "args": args }),
        )
        .map_err(|e| err_remote("terminal", e))
    }

    fn read_context(&self, project: Option<&str>) -> Result<String> {
        let path = format!("/api/v1/context?{}", scope_query(project));
        self.get_json::<ContextResponse>(&path)
            .map(|r| r.content)
            .map_err(|e| err_remote("read context", e))
    }

    fn write_context(&self, project: Option<&str>, content: &str) -> Result<()> {
        self.send_json::<serde_json::Value>(
            reqwest::Method::PUT,
            &format!("/api/v1/context?{}", scope_query(project)),
            &serde_json::json!({ "content": content }),
        )
        .map_err(|e| err_remote("write context", e))?;
        Ok(())
    }

    fn list_memory(&self, project: Option<&str>) -> Result<Vec<MemoryEntry>> {
        self.get_json::<MemoryResponse>(&format!("/api/v1/memory?{}", scope_query(project)))
            .map(|r| r.entries)
            .map_err(|e| err_remote("list memory", e))
    }

    fn append_memory(&self, project: Option<&str>, content: &str) -> Result<()> {
        self.send_json::<serde_json::Value>(
            reqwest::Method::POST,
            &format!("/api/v1/memory?{}", scope_query(project)),
            &serde_json::json!({ "content": content }),
        )
        .map_err(|e| err_remote("append memory", e))?;
        Ok(())
    }

    fn list_global_skills(&self) -> Result<Vec<Skill>> {
        self.list_skills_wire("/api/v1/skills")
    }

    fn list_project_skills(&self, project: Option<&str>) -> Result<Vec<Skill>> {
        self.list_skills_wire(&format!("/api/v1/skills/project?{}", scope_query(project)))
    }

    fn toggle_project_skill(&self, project: Option<&str>, name: &str) -> Result<bool> {
        // The API splits add/remove into distinct endpoints; "toggle"
        // is resolved by inspecting the current reference set.
        let listed = self.list_project_skills(project)?;
        let enabled = listed.iter().any(|s| s.name == name);
        if enabled {
            self.send_empty::<serde_json::Value>(
                reqwest::Method::DELETE,
                &format!(
                    "/api/v1/skills/project?{}&name={}",
                    scope_query(project),
                    urlencode(name)
                ),
            )
            .map_err(|e| err_remote("remove project skill", e))?;
        } else {
            self.send_json::<serde_json::Value>(
                reqwest::Method::POST,
                "/api/v1/skills/project",
                &serde_json::json!({
                    "project": project.map(str::to_owned),
                    "name": name,
                }),
            )
            .map_err(|e| err_remote("add project skill", e))?;
        }
        Ok(!enabled)
    }

    fn current_project_hint(&self) -> Option<String> {
        self.current_project.clone()
    }

    fn list_mcp_servers(&self) -> Result<Vec<crate::tui::remote::McpInfo>> {
        self.list_mcp()
    }

    fn mode(&self) -> crate::tui::backend::BackendMode {
        crate::tui::backend::BackendMode::Remote
    }

    fn remote_base(&self) -> Option<String> {
        Some(self.base.clone())
    }
}

impl RemoteBackend {
    fn list_skills_wire(&self, path: &str) -> Result<Vec<Skill>> {
        let skills = self
            .get_json::<SkillsResponse>(path)
            .map_err(|e| err_remote("list skills", e))?;
        Ok(skills
            .skills
            .into_iter()
            .map(|s| Skill {
                name: s.name,
                description: s.description,
                version: s.version,
                path: PathBuf::new(),
            })
            .collect())
    }

    /// MCP registry rows for the MCP screen.
    pub fn list_mcp(&self) -> Result<Vec<McpInfo>> {
        self.get_json::<McpResponse>("/api/v1/mcp")
            .map(|r| r.servers)
            .map_err(|e| err_remote("list mcp", e))
    }
}

/// Minimal percent-encoding for query components: everything outside
/// the unreserved set is escaped, so paths with `?`, `&`, `=`, spaces,
/// or UTF-8 survive the round trip.
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn scope_query(project: Option<&str>) -> String {
    match project {
        Some(p) => format!("project={}", urlencode(p)),
        None => String::new(),
    }
}

fn parse_kind(s: &str) -> crate::services::files::PathKind {
    match s {
        "Directory" => crate::services::files::PathKind::Directory,
        "BinaryFile" => crate::services::files::PathKind::BinaryFile,
        _ => crate::services::files::PathKind::TextFile,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urlencode_escapes_reserved_characters() {
        assert_eq!(urlencode("a b&c=d"), "a%20b%26c%3Dd");
        assert_eq!(urlencode("ünïcode"), "%C3%BCn%C3%AFcode");
        assert_eq!(urlencode("plain.txt"), "plain.txt");
    }

    #[test]
    fn version_compatibility_matches_major_minor() {
        assert!(version_compatible("0.1.0", "0.1.5"));
        assert!(version_compatible("0.1.0", "0.1.0"));
        assert!(!version_compatible("0.2.0", "0.1.5"));
        assert!(!version_compatible("1.0.0", "0.1.5"));
    }

    #[test]
    fn probe_reports_unreachable_servers() {
        let backend = RemoteBackend::new("http://127.0.0.1:1", "key");
        match backend.probe() {
            ConnectionState::Unavailable { .. } => {}
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }
}
