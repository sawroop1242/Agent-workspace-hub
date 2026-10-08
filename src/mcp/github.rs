//! First-class GitHub MCP provider.
//!
//! `github.*` tools call the GitHub REST API directly — the same way `git.*`
//! tools call the local `git` binary — rather than hiding behind the generic
//! `connector.invoke` indirection. The provider is optional and fails closed:
//! without `GITHUB_TOKEN` it is never registered, and the `github.*` tools
//! then don't appear in `tools/list` at all.

use crate::services::git::GitService;
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Map, Value};
use std::path::Path;

const DEFAULT_BASE_URL: &str = "https://api.github.com";

/// A GitHub `owner/repo` pair, resolved from tool arguments, the local
/// `origin` git remote, or the `GITHUB_DEFAULT_OWNER`/`GITHUB_DEFAULT_REPO`
/// environment variables — in that precedence order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoTarget {
    pub owner: String,
    pub repo: String,
}

impl RepoTarget {
    /// Extracts `owner/repo` from a git remote URL, supporting the HTTPS form
    /// (`https://github.com/o/r.git`, with or without leading userinfo), the
    /// SCP-like SSH form (`git@github.com:o/r.git`), `ssh://git@github.com/o/r`,
    /// and `git://github.com/o/r`. Returns `None` for other hosts or shapes —
    /// the remote is an optional hint, never a hard requirement.
    pub fn from_remote_url(url: &str) -> Option<Self> {
        let trimmed = url.trim();
        let no_git = trimmed.strip_suffix(".git").unwrap_or(trimmed);
        let path = if let Some(rest) = no_git.strip_prefix("git@github.com:") {
            rest.to_string()
        } else {
            let after_scheme = no_git
                .strip_prefix("https://")
                .or_else(|| no_git.strip_prefix("http://"))
                .or_else(|| no_git.strip_prefix("ssh://"))
                .or_else(|| no_git.strip_prefix("git://"))
                .unwrap_or(no_git);
            // Drop userinfo (`token@github.com/...`) so credential-bearing
            // clones resolve the same target as plain ones.
            let host_path = match after_scheme.split_once('@') {
                Some((_, rest)) => rest,
                None => after_scheme,
            };
            host_path.strip_prefix("github.com/")?.to_string()
        };
        let path = path.trim_end_matches('/');
        let mut segments = path.split('/');
        let owner = segments.next()?.trim();
        let repo = segments.next()?.trim();
        if segments.next().is_some() || !is_valid_github_name(owner) || !is_valid_github_name(repo)
        {
            return None;
        }
        Some(Self {
            owner: owner.to_string(),
            repo: repo.to_string(),
        })
    }

    /// Parses an `owner`/`repo` argument pair (the tool-argument form, also
    /// used by `GITHUB_DEFAULT_OWNER`/`GITHUB_DEFAULT_REPO`).
    pub fn from_str_pair(owner: &str, repo: &str) -> Option<Self> {
        let owner = owner.trim();
        let repo = repo.trim();
        if !is_valid_github_name(owner) || !is_valid_github_name(repo) {
            return None;
        }
        Some(Self {
            owner: owner.to_string(),
            repo: repo.to_string(),
        })
    }
}

impl std::fmt::Display for RepoTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.owner, self.repo)
    }
}

/// GitHub restricts owner and repository names to ASCII alphanumerics plus
/// `-`, `_`, and `.`. Enforcing the same charset before a name is spliced
/// into a URL path keeps arguments from reshaping the path or query (e.g. a
/// `repo` of `r?per_page=1`).
fn is_valid_github_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Resolves the target repository for a `github.*` tool call: explicit
/// `owner`/`repo` arguments first, then the workspace's `origin` git remote,
/// then the `GITHUB_DEFAULT_OWNER`/`GITHUB_DEFAULT_REPO` environment
/// variables. Every precedence level resolves *both* halves together, so a
/// half-specified target never mixes sources (an explicit `owner` never
/// pairs with an environment `repo`, which would silently target the wrong
/// repository).
pub async fn resolve_repo_target(
    owner_arg: Option<&str>,
    repo_arg: Option<&str>,
    workspace_root: &Path,
) -> Result<RepoTarget> {
    if let Some(target) = RepoTarget::from_str_pair(owner_arg.unwrap_or(""), repo_arg.unwrap_or(""))
    {
        return Ok(target);
    }
    let remote = if workspace_root.join(".git").exists() {
        match GitService::open(workspace_root) {
            Ok(git) => git.remote_url("origin").await,
            Err(_) => None,
        }
    } else {
        None
    };
    resolve_repo_target_with_remote(owner_arg, repo_arg, remote.as_deref())
}

/// Resolution against a pre-fetched remote URL, so the pure precedence
/// logic is unit-testable without spawning git.
pub fn resolve_repo_target_with_remote(
    owner_arg: Option<&str>,
    repo_arg: Option<&str>,
    remote_url: Option<&str>,
) -> Result<RepoTarget> {
    if let Some(target) = RepoTarget::from_str_pair(owner_arg.unwrap_or(""), repo_arg.unwrap_or(""))
    {
        return Ok(target);
    }
    if let Some(url) = remote_url {
        if let Some(target) = RepoTarget::from_remote_url(url) {
            return Ok(target);
        }
    }
    if let (Ok(owner), Ok(repo)) = (
        std::env::var("GITHUB_DEFAULT_OWNER"),
        std::env::var("GITHUB_DEFAULT_REPO"),
    ) {
        if let Some(target) = RepoTarget::from_str_pair(&owner, &repo) {
            return Ok(target);
        }
    }
    bail!("owner/repo not specified and could not be inferred from git remote 'origin'")
}

/// Validates an optional enum argument against its allowed values, mirroring
/// the fail-closed enum handling of `tasks.update`'s status field.
pub fn validate_enum<'a>(
    value: Option<&'a str>,
    allowed: &[&str],
    name: &str,
) -> Result<Option<&'a str>> {
    match value {
        None => Ok(None),
        Some(v) if allowed.contains(&v) => Ok(Some(v)),
        Some(v) => bail!(
            "invalid {name} '{v}': expected one of {}",
            allowed.join(", ")
        ),
    }
}

/// The `github.*` MCP tool surface, backed by one HTTP client and one token.
pub struct GithubProvider {
    token: String,
    base_url: String,
    client: reqwest::Client,
}

impl GithubProvider {
    /// Builds a provider from `GITHUB_TOKEN` (required) and `GITHUB_API_URL`
    /// (optional; GitHub Enterprise Server deployments point this at their
    /// own host, so the API host is never hardcoded). Fails closed exactly
    /// like every other optional provider: a missing or empty token is an
    /// `Err`, which the dispatcher turns into "not registered".
    pub fn from_env() -> Result<Self> {
        let token = std::env::var("GITHUB_TOKEN")
            .map_err(|_| anyhow!("GITHUB_TOKEN is required to enable the github.* tools"))?;
        if token.trim().is_empty() {
            bail!("GITHUB_TOKEN is empty");
        }
        let base_url = std::env::var("GITHUB_API_URL")
            .ok()
            .map(|url| url.trim().trim_end_matches('/').to_string())
            .filter(|url| !url.is_empty())
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        Ok(Self {
            token,
            base_url,
            client: super::config::build_http_client()?,
        })
    }

    /// Creates a provider with an explicit token and base URL — the seam
    /// tests use to point the provider at a local stub server.
    ///
    /// Fails only if the shared HTTP client cannot be constructed —
    /// fail-closed, not panic (§20).
    pub fn new(token: impl Into<String>, base_url: impl Into<String>) -> Result<Self> {
        Ok(Self {
            token: token.into(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client: super::config::build_http_client()?,
        })
    }

    /// The API root this provider talks to.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Sends one request carrying the full GitHub auth/header set and
    /// returns the response status plus raw body.
    async fn send(
        &self,
        method: reqwest::Method,
        url: &str,
        body: Option<Value>,
    ) -> Result<(reqwest::StatusCode, String)> {
        let mut builder = self
            .client
            .request(method, url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(body) = body {
            builder = builder.json(&body);
        }
        let response = builder.send().await.context("GitHub HTTP request failed")?;
        let status = response.status();
        let text = response
            .text()
            .await
            .context("GitHub returned a non-UTF-8 body")?;
        Ok((status, text))
    }

    /// Sends one JSON request and returns the parsed body, failing with the
    /// status and body GitHub sent on any non-2xx response.
    async fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        query: Option<String>,
        body: Option<Value>,
    ) -> Result<Value> {
        let mut url = format!("{}{}", self.base_url, path);
        if let Some(query) = query {
            url.push('?');
            url.push_str(&query);
        }
        let (status, text) = self.send(method, &url, body).await?;
        let value: Value = serde_json::from_str(&text)
            .with_context(|| format!("GitHub returned invalid JSON: {text}"))?;
        if !status.is_success() {
            bail!("GitHub API returned {status}: {value}");
        }
        Ok(value)
    }

    /// Lists pull requests, filtered by state and optionally by base branch.
    pub async fn pr_list(
        &self,
        target: &RepoTarget,
        state: Option<&str>,
        base: Option<&str>,
    ) -> Result<Value> {
        let query = build_query(&[("state", state), ("base", base)]);
        self.request(reqwest::Method::GET, &pulls_path(target), query, None)
            .await
    }

    /// Fetches one pull request by number.
    pub async fn pr_get(&self, target: &RepoTarget, number: u64) -> Result<Value> {
        self.request(
            reqwest::Method::GET,
            &format!("{}/{number}", pulls_path(target)),
            None,
            None,
        )
        .await
    }

    /// Creates a pull request.
    pub async fn pr_create(
        &self,
        target: &RepoTarget,
        title: &str,
        head: &str,
        base: &str,
        body: Option<&str>,
        draft: Option<bool>,
    ) -> Result<Value> {
        let payload = build_body(&[
            ("title", Some(json!(title))),
            ("head", Some(json!(head))),
            ("base", Some(json!(base))),
            ("body", body.map(|v| json!(v))),
            ("draft", draft.map(|v| json!(v))),
        ]);
        self.request(
            reqwest::Method::POST,
            &pulls_path(target),
            None,
            Some(payload),
        )
        .await
    }

    /// Merges a pull request by number.
    pub async fn pr_merge(
        &self,
        target: &RepoTarget,
        number: u64,
        merge_method: Option<&str>,
    ) -> Result<Value> {
        let payload = build_body(&[("merge_method", merge_method.map(|v| json!(v)))]);
        self.request(
            reqwest::Method::PUT,
            &format!("{}/{number}/merge", pulls_path(target)),
            None,
            Some(payload),
        )
        .await
    }

    /// Submits a pull request review.
    pub async fn pr_review(
        &self,
        target: &RepoTarget,
        number: u64,
        event: &str,
        body: Option<&str>,
    ) -> Result<Value> {
        let payload = build_body(&[
            ("event", Some(json!(event))),
            ("body", body.map(|v| json!(v))),
        ]);
        self.request(
            reqwest::Method::POST,
            &format!("{}/{number}/reviews", pulls_path(target)),
            None,
            Some(payload),
        )
        .await
    }

    /// Lists issues, filtered by state and optionally by labels. GitHub's
    /// issues endpoint also returns pull requests, so entries carrying a
    /// `pull_request` key are filtered out — callers that want PRs have
    /// `github.pr_list` for that.
    pub async fn issue_list(
        &self,
        target: &RepoTarget,
        state: Option<&str>,
        labels: Option<&str>,
    ) -> Result<Value> {
        let query = build_query(&[("state", state), ("labels", labels)]);
        let value = self
            .request(
                reqwest::Method::GET,
                &format!("/repos/{target}/issues"),
                query,
                None,
            )
            .await?;
        Ok(filter_pull_requests(value))
    }

    /// Fetches one issue (or pull request) by number.
    pub async fn issue_get(&self, target: &RepoTarget, number: u64) -> Result<Value> {
        self.request(
            reqwest::Method::GET,
            &format!("/repos/{target}/issues/{number}"),
            None,
            None,
        )
        .await
    }

    /// Creates an issue.
    pub async fn issue_create(
        &self,
        target: &RepoTarget,
        title: &str,
        body: Option<&str>,
        labels: Option<Vec<String>>,
    ) -> Result<Value> {
        let payload = build_body(&[
            ("title", Some(json!(title))),
            ("body", body.map(|v| json!(v))),
            ("labels", labels.map(|v| json!(v))),
        ]);
        self.request(
            reqwest::Method::POST,
            &format!("/repos/{target}/issues"),
            None,
            Some(payload),
        )
        .await
    }

    /// Comments on an issue or pull request (one endpoint serves both).
    pub async fn issue_comment(
        &self,
        target: &RepoTarget,
        number: u64,
        body: &str,
    ) -> Result<Value> {
        self.request(
            reqwest::Method::POST,
            &format!("/repos/{target}/issues/{number}/comments"),
            None,
            Some(json!({"body": body})),
        )
        .await
    }

    /// Combined CI picture for a ref: the legacy commit status *and* the
    /// GitHub Actions check-runs view, since repositories use either or
    /// both. Both views are returned whenever present. If the first
    /// (status) request fails the error propagates — a soft-null result
    /// would hide auth failures behind an apparent success.
    pub async fn checks_status(&self, target: &RepoTarget, ref_: &str) -> Result<Value> {
        if ref_.trim().is_empty() {
            bail!("ref must not be empty");
        }
        let status = self
            .request(
                reqwest::Method::GET,
                &format!("/repos/{target}/commits/{ref_}/status"),
                None,
                None,
            )
            .await?;
        let check_runs = self
            .request(
                reqwest::Method::GET,
                &format!("/repos/{target}/commits/{ref_}/check-runs"),
                Some("per_page=100".to_string()),
                None,
            )
            .await;
        Ok(json!({
            "status": status,
            "check_runs": check_runs.ok().unwrap_or(Value::Null),
        }))
    }

    /// Triggers a `workflow_dispatch` run on a ref. GitHub answers 204 with
    /// no body, so this call bypasses the JSON-parsing request helper.
    pub async fn workflow_dispatch(
        &self,
        target: &RepoTarget,
        workflow_file: &str,
        ref_: &str,
    ) -> Result<Value> {
        if workflow_file.trim().is_empty() || ref_.trim().is_empty() {
            bail!("workflow_file and ref must not be empty");
        }
        if !is_valid_github_name(workflow_file.trim()) {
            bail!("invalid workflow_file '{workflow_file}'");
        }
        let url = format!(
            "{}/repos/{target}/actions/workflows/{workflow_file}/dispatches",
            self.base_url
        );
        let (status, text) = self
            .send(reqwest::Method::POST, &url, Some(json!({"ref": ref_})))
            .await?;
        if !status.is_success() {
            let value = serde_json::from_str::<Value>(&text).unwrap_or(json!({"message": text}));
            bail!("GitHub API returned {status}: {value}")
        }
        Ok(json!({"dispatched": true}))
    }

    /// Creates a release.
    pub async fn release_create(
        &self,
        target: &RepoTarget,
        tag_name: &str,
        name: Option<&str>,
        body: Option<&str>,
        draft: Option<bool>,
        prerelease: Option<bool>,
    ) -> Result<Value> {
        let payload = build_body(&[
            ("tag_name", Some(json!(tag_name))),
            ("name", name.map(|v| json!(v))),
            ("body", body.map(|v| json!(v))),
            ("draft", draft.map(|v| json!(v))),
            ("prerelease", prerelease.map(|v| json!(v))),
        ]);
        self.request(
            reqwest::Method::POST,
            &format!("/repos/{target}/releases"),
            None,
            Some(payload),
        )
        .await
    }
}

/// The `/repos/{owner}/{repo}/pulls` path prefix.
fn pulls_path(target: &RepoTarget) -> String {
    format!("/repos/{target}/pulls")
}

/// Builds a percent-encoded query string from optional key/value pairs;
/// keys with `None` values are skipped entirely so GitHub's own defaults
/// apply.
fn build_query(parts: &[(&str, Option<&str>)]) -> Option<String> {
    let encoded: Vec<String> = parts
        .iter()
        .filter_map(|(key, value)| {
            value.map(|v| format!("{}={}", percent_encode(key), percent_encode(v)))
        })
        .collect();
    if encoded.is_empty() {
        None
    } else {
        Some(encoded.join("&"))
    }
}

/// Minimal RFC 3986 percent-encoding for query components, so label strings
/// like `help wanted` travel correctly.
fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Assembles a JSON request body, skipping unset keys so GitHub's defaults
/// apply instead of being explicitly overridden with null.
fn build_body(parts: &[(&str, Option<Value>)]) -> Value {
    let mut map = Map::new();
    for (key, value) in parts {
        if let Some(value) = value {
            map.insert((*key).to_string(), value.clone());
        }
    }
    Value::Object(map)
}

/// Removes entries that are pull requests from an issues-list response.
fn filter_pull_requests(value: Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .filter(|item| item.get("pull_request").is_none())
                .collect(),
        ),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- remote URL parsing ----------------------------------------------

    #[test]
    fn remote_url_parsing_supports_all_common_forms() {
        for (url, expected) in [
            ("https://github.com/owner/repo.git", Some(("owner", "repo"))),
            ("https://github.com/owner/repo", Some(("owner", "repo"))),
            (
                "https://user:token@github.com/owner/repo.git",
                Some(("owner", "repo")),
            ),
            (
                "https://token@github.com/owner/repo.git",
                Some(("owner", "repo")),
            ),
            ("git@github.com:owner/repo.git", Some(("owner", "repo"))),
            ("git@github.com:owner/repo", Some(("owner", "repo"))),
            (
                "ssh://git@github.com/owner/repo.git",
                Some(("owner", "repo")),
            ),
            ("git://github.com/owner/repo.git", Some(("owner", "repo"))),
            ("http://github.com/owner/repo.git", Some(("owner", "repo"))),
            ("https://github.com/owner/repo/", Some(("owner", "repo"))),
            ("  https://github.com/o/r.git  \n", Some(("o", "r"))),
            ("https://gitlab.com/owner/repo.git", None),
            ("https://github.com/owner", None),
            ("https://github.com/owner/", None),
            ("https://github.com/owner/repo/extra", None),
            ("https://github.com/o/r?per_page=1", None),
            ("", None),
        ] {
            let got = RepoTarget::from_remote_url(url).map(|t| (t.owner.clone(), t.repo.clone()));
            let expected = expected.map(|(o, r)| (o.to_string(), r.to_string()));
            assert_eq!(got, expected, "url: {url:?}");
        }
    }

    // ---- target resolution precedence ------------------------------------

    #[test]
    fn resolution_precedence_and_fail_closed_error() {
        // All env manipulation lives in this single test so parallel tests
        // never observe a half-set pair of GITHUB_DEFAULT_* variables.
        std::env::set_var("GITHUB_DEFAULT_OWNER", "env-o");
        std::env::set_var("GITHUB_DEFAULT_REPO", "env-r");

        // 1. Explicit arguments win over everything.
        let target = resolve_repo_target_with_remote(
            Some("arg-o"),
            Some("arg-r"),
            Some("https://github.com/remote-o/remote-r.git"),
        )
        .unwrap();
        assert_eq!(
            (target.owner.as_str(), target.repo.as_str()),
            ("arg-o", "arg-r")
        );

        // 2. Half-explicit arguments resolve from the remote *as a pair* — an
        // explicit `owner` must never be silently paired with an environment
        // `repo`, which would target the wrong repository.
        let target =
            resolve_repo_target_with_remote(Some("arg-o"), None, Some("git@github.com:ro/rr.git"))
                .unwrap();
        assert_eq!((target.owner.as_str(), target.repo.as_str()), ("ro", "rr"));

        // 3. The remote beats the env fallback.
        let target = resolve_repo_target_with_remote(
            None,
            None,
            Some("https://github.com/remote-o/remote-r.git"),
        )
        .unwrap();
        assert_eq!(
            (target.owner.as_str(), target.repo.as_str()),
            ("remote-o", "remote-r")
        );

        // 4. A non-GitHub remote falls through to the env fallback.
        let target =
            resolve_repo_target_with_remote(None, None, Some("https://gitlab.com/a/b.git"))
                .unwrap();
        assert_eq!(
            (target.owner.as_str(), target.repo.as_str()),
            ("env-o", "env-r")
        );

        std::env::remove_var("GITHUB_DEFAULT_OWNER");
        std::env::remove_var("GITHUB_DEFAULT_REPO");

        // With env cleared, the remote alone resolves...
        let target = resolve_repo_target_with_remote(
            None,
            None,
            Some("https://github.com/remote-o/remote-r.git"),
        )
        .unwrap();
        assert_eq!(
            (target.owner.as_str(), target.repo.as_str()),
            ("remote-o", "remote-r")
        );

        // ...and nothing resolving fails closed with the documented error.
        let error = resolve_repo_target_with_remote(None, None, None).unwrap_err();
        assert!(
            error.to_string().contains("owner/repo not specified"),
            "unexpected: {error}"
        );
    }

    #[test]
    fn invalid_name_characters_are_rejected_in_str_pairs() {
        // These would splice into the request URL path, so they must be
        // rejected rather than reshaped into a different endpoint.
        for (owner, repo) in [
            ("../escape", "repo"),
            ("owner", "repo?per_page=1"),
            ("owner", "re#po"),
            ("owner", "repo/extra"),
            ("own er", "repo"),
            ("", "repo"),
            ("owner", ""),
        ] {
            assert!(
                RepoTarget::from_str_pair(owner, repo).is_none(),
                "{owner:?}/{repo:?} must not resolve"
            );
        }
    }

    #[test]
    fn remote_url_read_from_a_real_git_repo() {
        let temp = tempfile::tempdir().unwrap();
        init_git_repo(temp.path());
        let output = std::process::Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "https://github.com/live-o/live-r.git",
            ])
            .current_dir(temp.path())
            .output()
            .expect("git remote add");
        assert!(output.status.success(), "git remote add failed");
        let root = temp.path().to_path_buf();
        let target = block_on_resolve(resolve_repo_target(None, None, &root));
        assert_eq!(
            (target.owner.as_str(), target.repo.as_str()),
            ("live-o", "live-r")
        );
    }

    #[test]
    fn from_env_fails_closed_without_token() {
        // A token legitimately present in the ambient environment (an agent
        // sandbox with GITHUB_TOKEN exported) must not fail this test; the
        // skip is the same pattern the audit tests use for ambient config.
        if std::env::var("GITHUB_TOKEN").is_ok() {
            return;
        }
        assert!(GithubProvider::from_env().is_err());
    }

    fn block_on_resolve(fut: impl std::future::Future<Output = Result<RepoTarget>>) -> RepoTarget {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(fut).expect("resolve target")
    }

    fn init_git_repo(dir: &Path) {
        for args in [
            ["init", "--quiet"].as_slice(),
            ["config", "user.email", "test@example.com"].as_slice(),
            ["config", "user.name", "Test"].as_slice(),
        ] {
            let output = std::process::Command::new("git")
                .args(args)
                .current_dir(dir)
                .output()
                .expect("git");
            assert!(output.status.success(), "git {args:?} failed");
        }
    }

    // ---- request shaping --------------------------------------------------

    #[test]
    fn query_building_encodes_and_skips_unset() {
        assert_eq!(build_query(&[]), None);
        assert_eq!(
            build_query(&[("state", Some("open")), ("base", None)]),
            Some("state=open".to_string())
        );
        assert_eq!(
            build_query(&[("labels", Some("help wanted"))]),
            Some("labels=help%20wanted".to_string())
        );
    }

    #[test]
    fn body_building_skips_unset_keys() {
        let body = build_body(&[("title", Some(json!("t"))), ("draft", None), ("body", None)]);
        assert_eq!(body, json!({"title": "t"}));
    }

    #[test]
    fn issue_list_filters_out_pull_requests() {
        let value = json!([
            {"number": 1, "title": "an issue"},
            {"number": 2, "title": "a pr", "pull_request": {"url": "x"}},
            {"number": 3, "title": "another issue"}
        ]);
        let filtered = filter_pull_requests(value);
        assert_eq!(
            filtered,
            json!([
                {"number": 1, "title": "an issue"},
                {"number": 3, "title": "another issue"}
            ])
        );
    }

    #[test]
    fn enum_validation_rejects_unknown_values() {
        assert_eq!(
            validate_enum(Some("open"), &["open", "closed", "all"], "state").unwrap(),
            Some("open")
        );
        assert!(validate_enum(None, &["open"], "state").unwrap().is_none());
        let error = validate_enum(Some("bogus"), &["open", "closed", "all"], "state").unwrap_err();
        assert!(
            error.to_string().contains("invalid state 'bogus'"),
            "unexpected: {error}"
        );
    }

    // ---- HTTP stub tests --------------------------------------------------

    /// One request recorded by the stub server.
    #[derive(Clone, Debug)]
    struct Recorded {
        method: String,
        path: String,
        query: String,
        body: String,
        authorization: String,
        accept: String,
        api_version: String,
    }

    /// Spawns a stub GitHub API on an ephemeral loopback port. Every request
    /// is recorded; exact `"METHOD /path"` keys in `overrides` answer with
    /// the given status (a `None` body yields an empty body, as 204
    /// requires); everything else answers `{"stub": true}` with 200.
    async fn spawn_stub(
        overrides: Vec<(&'static str, reqwest::StatusCode, Option<Value>)>,
    ) -> (String, std::sync::Arc<std::sync::Mutex<Vec<Recorded>>>) {
        use axum::response::IntoResponse;
        use std::sync::{Arc, Mutex};

        let recorded: Arc<Mutex<Vec<Recorded>>> = Arc::new(Mutex::new(Vec::new()));
        let overrides = Arc::new(overrides);
        let recorded_for_test = std::sync::Arc::clone(&recorded);
        let router = axum::Router::new().fallback(move |req: axum::extract::Request| {
            let recorded = recorded.clone();
            let overrides = overrides.clone();
            async move {
                let method = req.method().to_string();
                let authorization = req
                    .headers()
                    .get("authorization")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or_default()
                    .to_string();
                let accept = req
                    .headers()
                    .get("accept")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or_default()
                    .to_string();
                let api_version = req
                    .headers()
                    .get("x-github-api-version")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or_default()
                    .to_string();
                let (parts, body) = req.into_parts();
                let bytes = axum::body::to_bytes(body, 1024 * 1024)
                    .await
                    .unwrap_or_default();
                let query = parts.uri.query().unwrap_or_default().to_string();
                let path = parts.uri.path().to_string();
                recorded.lock().unwrap().push(Recorded {
                    method: method.clone(),
                    path: path.clone(),
                    query,
                    body: String::from_utf8_lossy(&bytes).to_string(),
                    authorization,
                    accept,
                    api_version,
                });
                let key = format!("{method} {path}");
                for (k, status, value) in overrides.iter() {
                    if *k == key {
                        return match value {
                            Some(v) => (*status, axum::Json(v.clone())).into_response(),
                            None => (*status).into_response(),
                        };
                    }
                }
                axum::Json(json!({"stub": true})).into_response()
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        (format!("http://{addr}"), recorded_for_test)
    }

    fn target() -> RepoTarget {
        RepoTarget::from_str_pair("o", "r").unwrap()
    }

    #[tokio::test]
    async fn pr_list_hits_expected_endpoint_query_and_headers() {
        let (base, recorded) = spawn_stub(vec![]).await;
        let provider = GithubProvider::new("token", &base).expect("provider");
        let value = provider
            .pr_list(&target(), Some("open"), Some("main"))
            .await
            .unwrap();
        assert_eq!(value, json!({"stub": true}));
        let recorded = recorded.lock().unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].method, "GET");
        assert_eq!(recorded[0].path, "/repos/o/r/pulls");
        assert_eq!(recorded[0].query, "state=open&base=main");
        assert_eq!(recorded[0].authorization, "Bearer token");
        assert_eq!(recorded[0].accept, "application/vnd.github+json");
        assert_eq!(recorded[0].api_version, "2022-11-28");
    }

    #[tokio::test]
    async fn pr_list_without_filters_sends_no_query() {
        let (base, recorded) = spawn_stub(vec![]).await;
        let provider = GithubProvider::new("token", &base).expect("provider");
        provider.pr_list(&target(), None, None).await.unwrap();
        let recorded = recorded.lock().unwrap();
        assert_eq!(recorded[0].query, "");
    }

    #[tokio::test]
    async fn pr_create_posts_required_fields_only() {
        let (base, recorded) = spawn_stub(vec![]).await;
        let provider = GithubProvider::new("token", &base).expect("provider");
        provider
            .pr_create(&target(), "T", "feature", "main", None, None)
            .await
            .unwrap();
        let recorded = recorded.lock().unwrap();
        assert_eq!(recorded[0].method, "POST");
        assert_eq!(recorded[0].path, "/repos/o/r/pulls");
        let body: Value = serde_json::from_str(&recorded[0].body).unwrap();
        assert_eq!(
            body,
            json!({"title": "T", "head": "feature", "base": "main"})
        );
    }

    #[tokio::test]
    async fn pr_merge_puts_merge_method() {
        let (base, recorded) = spawn_stub(vec![]).await;
        let provider = GithubProvider::new("token", &base).expect("provider");
        provider
            .pr_merge(&target(), 7, Some("squash"))
            .await
            .unwrap();
        let recorded = recorded.lock().unwrap();
        assert_eq!(recorded[0].method, "PUT");
        assert_eq!(recorded[0].path, "/repos/o/r/pulls/7/merge");
        let body: Value = serde_json::from_str(&recorded[0].body).unwrap();
        assert_eq!(body, json!({"merge_method": "squash"}));
    }

    #[tokio::test]
    async fn issue_comment_posts_body_to_issue_endpoint() {
        let (base, recorded) = spawn_stub(vec![]).await;
        let provider = GithubProvider::new("token", &base).expect("provider");
        provider
            .issue_comment(&target(), 5, "looks good")
            .await
            .unwrap();
        let recorded = recorded.lock().unwrap();
        assert_eq!(recorded[0].method, "POST");
        assert_eq!(recorded[0].path, "/repos/o/r/issues/5/comments");
        let body: Value = serde_json::from_str(&recorded[0].body).unwrap();
        assert_eq!(body, json!({"body": "looks good"}));
    }

    #[tokio::test]
    async fn checks_status_combines_status_and_check_runs() {
        let (base, recorded) = spawn_stub(vec![]).await;
        let provider = GithubProvider::new("token", &base).expect("provider");
        let value = provider.checks_status(&target(), "main").await.unwrap();
        assert_eq!(
            value,
            json!({"status": {"stub": true}, "check_runs": {"stub": true}})
        );
        let recorded = recorded.lock().unwrap();
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[0].path, "/repos/o/r/commits/main/status");
        assert_eq!(recorded[1].path, "/repos/o/r/commits/main/check-runs");
        assert_eq!(recorded[1].query, "per_page=100");
    }

    #[tokio::test]
    async fn checks_status_fails_closed_on_status_error() {
        let (base, _recorded) = spawn_stub(vec![(
            "GET /repos/o/r/commits/main/status",
            reqwest::StatusCode::UNAUTHORIZED,
            Some(json!({"message": "Bad credentials"})),
        )])
        .await;
        let provider = GithubProvider::new("token", &base).expect("provider");
        let error = provider.checks_status(&target(), "main").await.unwrap_err();
        assert!(error.to_string().contains("401"), "unexpected: {error}");
    }

    #[tokio::test]
    async fn workflow_dispatch_handles_204_no_content() {
        let (base, recorded) = spawn_stub(vec![(
            "POST /repos/o/r/actions/workflows/ci.yml/dispatches",
            reqwest::StatusCode::NO_CONTENT,
            None,
        )])
        .await;
        let provider = GithubProvider::new("token", &base).expect("provider");
        let value = provider
            .workflow_dispatch(&target(), "ci.yml", "main")
            .await
            .unwrap();
        assert_eq!(value, json!({"dispatched": true}));
        let recorded = recorded.lock().unwrap();
        assert_eq!(recorded[0].method, "POST");
        assert_eq!(
            recorded[0].path,
            "/repos/o/r/actions/workflows/ci.yml/dispatches"
        );
        let body: Value = serde_json::from_str(&recorded[0].body).unwrap();
        assert_eq!(body, json!({"ref": "main"}));
    }

    #[tokio::test]
    async fn workflow_dispatch_rejects_path_shaping_arguments() {
        let (base, _recorded) = spawn_stub(vec![]).await;
        let provider = GithubProvider::new("token", &base).expect("provider");
        // A workflow_file carrying URL metacharacters could reshape the
        // endpoint; it must be rejected before any request is sent.
        let error = provider
            .workflow_dispatch(&target(), "../evil.yml", "main")
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("invalid workflow_file"),
            "unexpected: {error}"
        );
    }

    #[tokio::test]
    async fn non_2xx_responses_bail_with_status_and_body() {
        let (base, _recorded) = spawn_stub(vec![(
            "GET /repos/o/r/pulls/999",
            reqwest::StatusCode::NOT_FOUND,
            Some(json!({"message": "Not Found"})),
        )])
        .await;
        let provider = GithubProvider::new("token", &base).expect("provider");
        let error = provider.pr_get(&target(), 999).await.unwrap_err();
        let text = error.to_string();
        assert!(text.contains("404"), "unexpected: {text}");
        assert!(text.contains("Not Found"), "unexpected: {text}");
    }

    #[test]
    fn base_url_has_trailing_slash_stripped() {
        let provider = GithubProvider::new("token", "http://127.0.0.1:1/").expect("provider");
        assert_eq!(provider.base_url(), "http://127.0.0.1:1");
    }
}
