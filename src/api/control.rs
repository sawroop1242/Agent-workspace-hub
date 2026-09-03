//! Versioned HTTP Control API (spec section 16).
//!
//! Serves `/api/v1/` for TUI/CLI/admin operations: projects, files,
//! git, terminal, status, and audit. Kept conceptually separate from
//! the MCP server (section 26): this plane is for humans and AWH
//! clients, not AI-agent tools. Both share the application services.
//!
//! All errors are structured JSON `{"error": {"code", "message"}}`;
//! internal implementation details (paths, tool names) stay private.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;

use crate::api::rate_limit::RateLimiter;
use crate::mcp::audit::{audit_allow, audit_deny};
use crate::mcp::auth;
use crate::services::{
    files::FilesService, git::GitService, projects::ProjectsService, terminal::TerminalService,
};
use crate::skills::registry::GlobalSkillRegistry;

/// Shared application state for the Control API.
pub struct ControlState {
    pub root: PathBuf,
    pub api_key: String,
    pub started: std::time::Instant,
    pub version: &'static str,
    /// Sliding-window limiter applied after authentication.
    pub rate_limiter: Arc<RateLimiter>,
    /// Test seam: overrides the global skills-registry root so tests
    /// can exercise registry endpoints without mutating the real
    /// user home (which `GlobalSkillRegistry::discover()` would use,
    /// and which on Windows ignores `HOME` entirely — it resolves via
    /// the known-folders API). `None` in production.
    pub global_skills_root: Option<PathBuf>,
}

impl ControlState {
    pub fn new(root: impl Into<PathBuf>, api_key: String) -> Self {
        Self {
            root: root.into(),
            api_key,
            started: std::time::Instant::now(),
            version: env!("CARGO_PKG_VERSION"),
            rate_limiter: RateLimiter::default_limiter(),
            global_skills_root: None,
        }
    }

    /// The global skills registry: the injected test root when set,
    /// the discovered user home otherwise.
    fn global_skill_registry(&self) -> Result<GlobalSkillRegistry, ApiError> {
        match &self.global_skills_root {
            Some(root) => Ok(GlobalSkillRegistry::new(root)),
            None => GlobalSkillRegistry::discover().map_err(|e| ApiError::internal(&e)),
        }
    }

    fn files(&self) -> FilesService {
        FilesService::new(&self.root)
    }

    fn projects(&self) -> ProjectsService {
        ProjectsService::new(&self.root)
    }
}

/// Structured error envelope. Internal details are never leaked.
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }

    fn not_found(what: &str) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("{what} not found"),
        )
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "bad_request", message)
    }

    fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "invalid or missing token",
        )
    }

    fn internal(e: &anyhow::Error) -> Self {
        // Log full detail server-side; the client sees a generic code.
        tracing::error!(target: "awh::api", "control api error: {e:#}");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "internal error",
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({"error": {"code": self.code, "message": self.message}})),
        )
            .into_response()
    }
}

/// Builds the `/api/v1` router. Health stays unauthenticated; every
/// other route requires a bearer token checked with a constant-time
/// comparison.
pub fn build_router(state: Arc<ControlState>) -> Router {
    // Public health endpoint lives under the versioned prefix but is
    // exempt from authentication so load balancers can probe it.
    let api = Router::new()
        .route("/status", get(status))
        .route("/projects", get(list_projects).post(create_project))
        .route("/projects/{name}", get(project).delete(delete_project))
        .route("/files", get(list_files))
        .route("/files/content", get(read_file).put(write_file))
        .route("/files/search", get(search_files))
        .route(
            "/files/entry",
            delete(delete_entry).post(rename_entry).put(create_dir),
        )
        .route("/git/status", get(git_status))
        .route("/git/log", get(git_log))
        .route("/git/diff", get(git_diff))
        .route("/git/stage", post(git_stage))
        .route("/git/unstage", post(git_unstage))
        .route("/git/commit", post(git_commit))
        .route("/terminal/run", post(run_command))
        .route("/context", get(read_context).put(write_context))
        .route("/memory", get(list_memory).post(append_memory))
        .route("/skills", get(list_skills))
        .route(
            "/skills/project",
            get(list_project_skills)
                .post(add_project_skill)
                .delete(remove_project_skill),
        )
        .route("/mcp", get(list_mcp))
        .route("/audit", get(audit))
        .route("/logs", get(logs))
        // Rate limiting sits after authentication in the spec §25
        // chain: added before the auth layer, so `authenticate` (the
        // outermost layer) runs first and only valid tokens reach the
        // limiter. Health (public router below) is never throttled.
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            rate_limit_guard,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            authenticate,
        ));

    // Health is added without the auth layer but under the same prefix.
    let public = Router::new()
        .route("/healthz", get(health))
        .with_state(state.clone());

    let routes = api
        .merge(public)
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            std::time::Duration::from_secs(30),
        ))
        .layer(RequestBodyLimitLayer::new(1024 * 1024));

    Router::new().nest("/api/v1", routes).with_state(state)
}
async fn authenticate(
    State(state): State<Arc<ControlState>>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| auth::bearer_token(Some(s)));
    match token {
        Some(t) if auth::verify_token(&state.api_key, t) => {
            audit_allow("control_auth", "remote", "success");
            next.run(request).await
        }
        _ => {
            audit_deny("control_auth", "invalid_or_missing_token", "remote");
            ApiError::unauthorized().into_response()
        }
    }
}

/// Spec §25 rate-limit step: counts authenticated requests per client
/// key. The key prefers `X-Forwarded-For` (set by tunnels/proxies like
/// ngrok, which is the deployment this API expects); direct connections
/// share the `"direct"` bucket. Auth runs before this layer, so the
/// header cannot be used to dodge authentication — only to split
/// buckets, which is the desired behavior behind a proxy.
async fn rate_limit_guard(
    State(state): State<Arc<ControlState>>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let key = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(',').next().unwrap_or("direct").trim().to_string())
        .unwrap_or_else(|| "direct".to_string());
    match state.rate_limiter.check(&key) {
        Ok(remaining) => {
            let mut response = next.run(request).await;
            response
                .headers_mut()
                .insert("x-ratelimit-remaining", remaining.into());
            response
        }
        Err(retry_after_secs) => {
            // Signature is (action, reason, subject): the client key is
            // the subject of the denial, "rate_limited" the reason.
            audit_deny("api_rate_limit", "rate_limited", &key);
            let mut response = ApiError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                format!("rate limit exceeded; retry in {retry_after_secs}s"),
            )
            .into_response();
            response
                .headers_mut()
                .insert("retry-after", retry_after_secs.into());
            response
        }
    }
}

// ----- System -----

async fn health(State(state): State<Arc<ControlState>>) -> Json<serde_json::Value> {
    let uptime = state.started.elapsed().as_secs();
    Json(json!({"status": "ok", "uptime_secs": uptime}))
}

async fn status(State(state): State<Arc<ControlState>>) -> Json<serde_json::Value> {
    let project_count = state.projects().list().map(|l| l.len()).unwrap_or_default();
    let is_repo = GitService::open(&state.root)
        .map(|g| g.is_repo_blocking())
        .unwrap_or(false);
    Json(json!({
        "version": state.version,
        "uptime_secs": state.started.elapsed().as_secs(),
        "projects": project_count,
        "git_repo": is_repo,
        "root_name": state.root.file_name().and_then(|n| n.to_str()).unwrap_or(""),
    }))
}

// ----- Projects -----

async fn list_projects(
    State(state): State<Arc<ControlState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let projects = state
        .projects()
        .list()
        .map_err(|e| ApiError::internal(&e))?;
    Ok(Json(json!({"projects": projects})))
}

#[derive(Deserialize)]
struct CreateProjectBody {
    name: String,
}

async fn create_project(
    State(state): State<Arc<ControlState>>,
    Json(body): Json<CreateProjectBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    match state.projects().create(&body.name) {
        Ok(_) => {
            audit_allow("api_project_create", &body.name, "remote");
        }
        Err(e) => {
            audit_deny("api_project_create", "rejected", &body.name);
            return Err(ApiError::bad_request(format!("{e:#}")));
        }
    }
    Ok(Json(json!({"created": body.name})))
}

async fn project(
    State(state): State<Arc<ControlState>>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let projects = state
        .projects()
        .list()
        .map_err(|e| ApiError::internal(&e))?;
    if projects.iter().any(|p| p.name == name) {
        Ok(Json(json!({"name": name})))
    } else {
        Err(ApiError::not_found("project"))
    }
}

async fn delete_project(
    State(state): State<Arc<ControlState>>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let projects = state
        .projects()
        .list()
        .map_err(|e| ApiError::internal(&e))?;
    if !projects.iter().any(|p| p.name == name) {
        audit_deny("api_project_delete", "not_found", &name);
        return Err(ApiError::not_found("project"));
    }
    state
        .projects()
        .delete(&name)
        .map_err(|e| ApiError::internal(&e))?;
    audit_allow("api_project_delete", &name, "remote");
    Ok(Json(json!({"deleted": name})))
}

// ----- Files -----

#[derive(Deserialize)]
struct ListParams {
    dir: Option<String>,
}

async fn list_files(
    State(state): State<Arc<ControlState>>,
    Query(params): Query<ListParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let dir = params.dir.as_deref().unwrap_or("");
    let entries = state
        .files()
        .list(dir)
        .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    Ok(Json(json!({"dir": dir, "entries": entries})))
}

async fn read_file(
    State(state): State<Arc<ControlState>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let path = params
        .get("path")
        .ok_or_else(|| ApiError::bad_request("path query parameter is required"))?;
    let content = state
        .files()
        .read(path)
        .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    Ok(Json(json!({"path": path, "content": content})))
}

#[derive(Deserialize)]
struct WriteBody {
    path: String,
    content: String,
}

async fn write_file(
    State(state): State<Arc<ControlState>>,
    Json(body): Json<WriteBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .files()
        .write(&body.path, &body.content)
        .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    Ok(Json(json!({"written": body.path})))
}

#[derive(Deserialize)]
struct SearchParams {
    q: String,
}

async fn search_files(
    State(state): State<Arc<ControlState>>,
    Query(params): Query<SearchParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let hits = state
        .files()
        .search(&params.q, 100)
        .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    Ok(Json(json!({"query": params.q, "hits": hits})))
}

#[derive(Deserialize)]
struct EntryBody {
    path: String,
    new_name: Option<String>,
}

async fn delete_entry(
    State(state): State<Arc<ControlState>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let path = params
        .get("path")
        .ok_or_else(|| ApiError::bad_request("path query parameter is required"))?;
    state
        .files()
        .delete(path)
        .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    Ok(Json(json!({"deleted": path})))
}

async fn rename_entry(
    State(state): State<Arc<ControlState>>,
    Json(body): Json<EntryBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let new_name = body
        .new_name
        .as_deref()
        .ok_or_else(|| ApiError::bad_request("new_name is required"))?;
    state
        .files()
        .rename(&body.path, new_name)
        .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    Ok(Json(json!({"renamed": body.path, "to": new_name})))
}

async fn create_dir(
    State(state): State<Arc<ControlState>>,
    Json(body): Json<EntryBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .files()
        .create_dir(&body.path)
        .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    Ok(Json(json!({"created": body.path})))
}

// ----- Git -----

/// Opens the workspace git repo or returns a client-facing error
/// when the workspace is not a repository.
fn open_repo(state: &ControlState) -> Result<GitService, ApiError> {
    let git = GitService::open(&state.root).map_err(|e| ApiError::internal(&e))?;
    if !git.is_repo_blocking() {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "not_a_git_repo",
            "workspace is not a git repository",
        ));
    }
    Ok(git)
}

async fn git_status(
    State(state): State<Arc<ControlState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let git = open_repo(&state)?;
    let out = git.status().await.map_err(|e| ApiError::internal(&e))?;
    Ok(Json(
        serde_json::to_value(out).unwrap_or_else(|_| json!({})),
    ))
}

async fn git_log(
    State(state): State<Arc<ControlState>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let limit = params
        .get("limit")
        .and_then(|l| l.parse::<usize>().ok())
        .unwrap_or(50)
        .min(200);
    let git = GitService::open(&state.root).map_err(|e| ApiError::internal(&e))?;
    let out = git.log(limit).await.map_err(|e| ApiError::internal(&e))?;
    Ok(Json(
        serde_json::to_value(out).unwrap_or_else(|_| json!({})),
    ))
}

#[derive(Deserialize)]
struct GitDiffParams {
    path: Option<String>,
    staged: Option<bool>,
}

async fn git_diff(
    State(state): State<Arc<ControlState>>,
    Query(params): Query<GitDiffParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let git = open_repo(&state)?;
    let out = if params.staged.unwrap_or(false) {
        git.diff_staged(params.path.as_deref())
            .await
            .map_err(|e| ApiError::internal(&e))?
    } else {
        git.diff(params.path.as_deref())
            .await
            .map_err(|e| ApiError::internal(&e))?
    };
    Ok(Json(
        serde_json::to_value(out).unwrap_or_else(|_| json!({})),
    ))
}

#[derive(Deserialize)]
struct GitPathBody {
    path: String,
}

async fn git_stage(
    State(state): State<Arc<ControlState>>,
    Json(body): Json<GitPathBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let git = open_repo(&state)?;
    let out = git
        .stage(&body.path)
        .await
        .map_err(|e| ApiError::internal(&e))?;
    Ok(Json(
        serde_json::to_value(out).unwrap_or_else(|_| json!({})),
    ))
}

async fn git_unstage(
    State(state): State<Arc<ControlState>>,
    Json(body): Json<GitPathBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let git = open_repo(&state)?;
    let out = git
        .unstage(&body.path)
        .await
        .map_err(|e| ApiError::internal(&e))?;
    Ok(Json(
        serde_json::to_value(out).unwrap_or_else(|_| json!({})),
    ))
}

#[derive(Deserialize)]
struct CommitBody {
    message: String,
}

async fn git_commit(
    State(state): State<Arc<ControlState>>,
    Json(body): Json<CommitBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if body.message.trim().is_empty() {
        audit_deny("api_git_commit", "empty_message", "remote");
        return Err(ApiError::bad_request("commit message cannot be empty"));
    }
    let git = open_repo(&state)?;
    let out = git
        .commit(&body.message)
        .await
        .map_err(|e| ApiError::internal(&e))?;
    audit_allow(
        "api_git_commit",
        "remote",
        &truncate_detail(&body.message, 60),
    );
    Ok(Json(
        serde_json::to_value(out).unwrap_or_else(|_| json!({})),
    ))
}

// ----- Terminal -----

#[derive(Deserialize)]
struct RunBody {
    program: String,
    #[serde(default)]
    args: Vec<String>,
}

async fn run_command(
    State(state): State<Arc<ControlState>>,
    Json(body): Json<RunBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if body.program.trim().is_empty() {
        audit_deny("api_terminal_run", "empty_program", "remote");
        return Err(ApiError::bad_request("program is required"));
    }
    let terminal = TerminalService::new(&state.root);
    let outcome = terminal.run(&body.program, &body.args).await.map_err(|e| {
        audit_deny("api_terminal_run", "spawn_failed", &body.program);
        ApiError::internal(&e)
    })?;
    // Spec 22: terminal execution is high-risk and must be auditable.
    // The program name is the subject; exit status is the detail. Args
    // are deliberately omitted — they can carry secrets.
    audit_allow(
        "api_terminal_run",
        &body.program,
        &format!(
            "exit={}{}",
            outcome
                .exit_code
                .map(|c| c.to_string())
                .unwrap_or_else(|| "timeout".into()),
            if outcome.truncated { " truncated" } else { "" }
        ),
    );
    Ok(Json(
        serde_json::to_value(outcome).unwrap_or_else(|_| json!({})),
    ))
}

/// Truncates free-form audit detail on a char boundary.
fn truncate_detail(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let cut: String = s.chars().take(max - 1).collect();
        format!("{cut}\u{2026}")
    }
}

// ----- Context / Memory / Skills (spec sections 12-14) -----

/// Query scope for context/memory/project-skill reads and writes.
#[derive(Deserialize, Default)]
struct ScopeParams {
    project: Option<String>,
}

/// Resolves the store root for a scope: the named project (validated,
/// traversal-safe) or the workspace root when unset.
fn store_scope(
    state: &ControlState,
    project: Option<&str>,
) -> Result<std::path::PathBuf, ApiError> {
    match project {
        Some(name) => {
            crate::services::projects::validate_project_name(name)
                .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
            let path = crate::core::workspace::Workspace::new(&state.root).project_path(name);
            if !path.is_dir() {
                return Err(ApiError::not_found("project"));
            }
            Ok(path)
        }
        None => Ok(state.root.clone()),
    }
}

/// Safety cap matching the TUI editor's large-file posture.
const MAX_CONTEXT_BYTES: usize = 512 * 1024;

async fn read_context(
    State(state): State<Arc<ControlState>>,
    Query(params): Query<ScopeParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let root = store_scope(&state, params.project.as_deref())?;
    let content = crate::core::context::ContextStore::for_project(&root)
        .read()
        .map_err(|e| ApiError::internal(&e))?;
    Ok(Json(json!({
        "project": params.project,
        "content": content,
    })))
}

#[derive(Deserialize)]
struct WriteContextBody {
    content: String,
}

async fn write_context(
    State(state): State<Arc<ControlState>>,
    Query(params): Query<ScopeParams>,
    Json(body): Json<WriteContextBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let scope = scope_name(&params);
    if body.content.len() > MAX_CONTEXT_BYTES {
        audit_deny("api_context_write", "too_large", &scope);
        return Err(ApiError::bad_request("context exceeds 512 KiB cap"));
    }
    let root = store_scope(&state, params.project.as_deref())?;
    crate::core::context::ContextStore::for_project(&root)
        .write(&body.content)
        .map_err(|e| ApiError::internal(&e))?;
    audit_allow("api_context_write", &scope, "remote");
    Ok(Json(json!({"written": true})))
}

async fn list_memory(
    State(state): State<Arc<ControlState>>,
    Query(params): Query<ScopeParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let root = store_scope(&state, params.project.as_deref())?;
    let entries = crate::core::memory::MemoryStore::for_project(&root)
        .read_all()
        .map_err(|e| ApiError::internal(&e))?;
    let total = entries.len();
    Ok(Json(json!({
        "project": params.project,
        "total": total,
        "entries": entries,
    })))
}

#[derive(Deserialize)]
struct AppendMemoryBody {
    content: String,
}

async fn append_memory(
    State(state): State<Arc<ControlState>>,
    Query(params): Query<ScopeParams>,
    Json(body): Json<AppendMemoryBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if body.content.trim().is_empty() {
        return Err(ApiError::bad_request("content is required"));
    }
    let root = store_scope(&state, params.project.as_deref())?;
    let entry = crate::models::MemoryEntry {
        timestamp: chrono::Utc::now().to_rfc3339(),
        content: body.content,
    };
    crate::core::memory::MemoryStore::for_project(&root)
        .append(&entry)
        .map_err(|e| ApiError::internal(&e))?;
    audit_allow("api_memory_append", scope_name(&params).as_str(), "remote");
    Ok(Json(json!({"appended": true})))
}

/// Scope label for audit subjects (never includes paths).
fn scope_name(params: &ScopeParams) -> String {
    params
        .project
        .clone()
        .unwrap_or_else(|| "(root)".to_owned())
}

async fn list_project_skills(
    State(state): State<Arc<ControlState>>,
    Query(params): Query<ScopeParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let root = store_scope(&state, params.project.as_deref())?;
    let registry = state.global_skill_registry()?;
    let skills = crate::skills::ProjectSkillReferences::new(&root)
        .resolve(&registry)
        .map_err(|e| ApiError::internal(&e))?;
    let items: Vec<serde_json::Value> = skills
        .iter()
        .map(|s| {
            json!({
                "name": s.name,
                "description": s.description,
                "version": s.version.as_deref().unwrap_or("unknown"),
            })
        })
        .collect();
    Ok(Json(json!({"project": params.project, "skills": items})))
}

#[derive(Deserialize)]
struct ProjectSkillBody {
    project: Option<String>,
    name: String,
}

async fn add_project_skill(
    State(state): State<Arc<ControlState>>,
    Json(body): Json<ProjectSkillBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let root = store_scope(&state, body.project.as_deref())?;
    let registry = state.global_skill_registry()?;
    crate::skills::ProjectSkillReferences::new(&root)
        .add(&body.name, &registry)
        .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    audit_allow("api_skill_add", &body.name, &scope_name_body(&body));
    Ok(Json(json!({"added": body.name})))
}

async fn remove_project_skill(
    State(state): State<Arc<ControlState>>,
    Query(body): Query<ProjectSkillQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let root = store_scope(&state, body.project.as_deref())?;
    crate::skills::ProjectSkillReferences::new(&root)
        .remove(&body.name)
        .map_err(|e| ApiError::internal(&e))?;
    audit_allow("api_skill_remove", &body.name, &scope_name_body_q(&body));
    Ok(Json(json!({"removed": body.name})))
}

#[derive(Deserialize, Default)]
struct ProjectSkillQuery {
    project: Option<String>,
    name: String,
}

fn scope_name_body(body: &ProjectSkillBody) -> String {
    body.project.clone().unwrap_or_else(|| "(root)".to_owned())
}

fn scope_name_body_q(body: &ProjectSkillQuery) -> String {
    body.project.clone().unwrap_or_else(|| "(root)".to_owned())
}

// ----- Skills & MCP (read-only listings) -----

/// Lists globally installed skills. Project-level skill references are
/// managed via the CLI/TUI, which are the trust-owning planes.
async fn list_skills(
    State(state): State<Arc<ControlState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let registry = state.global_skill_registry()?;
    let skills = registry.list().map_err(|e| ApiError::internal(&e))?;
    let items: Vec<serde_json::Value> = skills
        .iter()
        .map(|s| {
            json!({
                "name": s.name,
                "description": s.description,
                "version": s.version.as_deref().unwrap_or("unknown"),
            })
        })
        .collect();
    Ok(Json(json!({"skills": items})))
}

/// Lists configured MCP servers (id, transport, enabled). Secrets,
/// commands, and environments are intentionally omitted from the
/// Control API response.
async fn list_mcp(
    State(_state): State<Arc<ControlState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let registry = crate::mcp::GlobalMcpRegistry::new().map_err(|e| ApiError::internal(&e))?;
    let servers = registry.list().map_err(|e| ApiError::internal(&e))?;
    let items: Vec<serde_json::Value> = servers
        .iter()
        .map(|s| {
            json!({
                "id": s.config.id,
                "transport": format!("{:?}", s.config.transport),
                "enabled": s.config.enabled,
                "version": s.version,
            })
        })
        .collect();
    Ok(Json(json!({"servers": items})))
}

// ----- Audit & logs (spec sections 16, 25) -----

#[derive(Deserialize)]
struct AuditParams {
    /// Maximum entries to return (default 100, capped at 1000).
    limit: Option<usize>,
    /// Filter: `allow`, `deny`, or omitted for all.
    kind: Option<String>,
}

/// Security-relevant events from the shared audit ring. Entries never
/// contain secret values; only actor/action identifiers are recorded.
async fn audit(Query(params): Query<AuditParams>) -> Result<Json<serde_json::Value>, ApiError> {
    let limit = params.limit.unwrap_or(100).min(1000);
    let entries = crate::services::audit::global().recent(limit);
    let entries: Vec<_> = entries
        .into_iter()
        .filter(|e| params.kind.as_deref().is_none_or(|k| e.kind == k))
        .collect();
    Ok(Json(json!({
        "entries": entries,
        "total": crate::services::audit::global().len(),
    })))
}

/// Operational log view over the same bounded ring; this plane records
/// security events, so it serves both `/logs` and `/audit` consumers.
async fn logs(Query(params): Query<AuditParams>) -> Result<Json<serde_json::Value>, ApiError> {
    let limit = params.limit.unwrap_or(50).min(1000);
    let entries = crate::services::audit::global().recent(limit);
    Ok(Json(json!({
        "entries": entries,
        "total": crate::services::audit::global().len(),
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::util::ServiceExt;

    fn state(root: &std::path::Path) -> Arc<ControlState> {
        Arc::new(ControlState::new(root, "test-key".to_string()))
    }

    fn get(path: &str, token: Option<&str>) -> Request<Body> {
        authed(Request::get(path).body(Body::empty()).unwrap(), token)
    }

    fn post_json(path: &str, token: Option<&str>, body: serde_json::Value) -> Request<Body> {
        let mut req = Request::post(path)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        if let Some(t) = token {
            req.headers_mut()
                .insert("authorization", format!("Bearer {t}").parse().unwrap());
        }
        req
    }

    fn authed(mut req: Request<Body>, token: Option<&str>) -> Request<Body> {
        if let Some(t) = token {
            req.headers_mut()
                .insert("authorization", format!("Bearer {t}").parse().unwrap());
        }
        req
    }

    #[tokio::test]
    async fn health_is_public() {
        let tmp = tempfile::tempdir().unwrap();
        let app = build_router(state(tmp.path()));
        let res = app
            .clone()
            .oneshot(get("/api/v1/healthz", None))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn api_requires_valid_token() {
        let tmp = tempfile::tempdir().unwrap();
        let app = build_router(state(tmp.path()));
        let res = app
            .clone()
            .oneshot(get("/api/v1/status", None))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

        let res = app
            .clone()
            .oneshot(get("/api/v1/status", Some("wrong")))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn valid_token_grants_access() {
        let tmp = tempfile::tempdir().unwrap();
        let app = build_router(state(tmp.path()));
        let res = app
            .clone()
            .oneshot(get("/api/v1/status", Some("test-key")))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn project_lifecycle_over_http() {
        let tmp = tempfile::tempdir().unwrap();
        let app = build_router(state(tmp.path()));

        let res = app
            .clone()
            .oneshot(post_json(
                "/api/v1/projects",
                Some("test-key"),
                json!({"name": "alpha"}),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let res = app
            .clone()
            .clone()
            .oneshot(get("/api/v1/projects", Some("test-key")))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let res = app
            .clone()
            .oneshot(
                Request::delete("/api/v1/projects/alpha")
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn files_roundtrip_requires_auth() {
        let tmp = tempfile::tempdir().unwrap();
        let app = build_router(state(tmp.path()));

        let res = app
            .clone()
            .oneshot(get("/api/v1/files?dir=", None))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

        let res = app
            .clone()
            .oneshot(get("/api/v1/files?dir=", Some("test-key")))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn traversal_paths_are_rejected_by_services() {
        let tmp = tempfile::tempdir().unwrap();
        let app = build_router(state(tmp.path()));
        let res = app
            .clone()
            .oneshot(post_json(
                "/api/v1/projects",
                Some("test-key"),
                json!({"name": "../escape"}),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn git_on_non_repo_returns_structured_conflict() {
        let tmp = tempfile::tempdir().unwrap();
        let app = build_router(state(tmp.path()));
        let res = app
            .oneshot(get("/api/v1/git/status", Some("test-key")))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CONFLICT);
        let body = axum::body::to_bytes(res.into_body(), 4096).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["error"]["code"], "not_a_git_repo");
    }

    #[tokio::test]
    async fn skills_and_mcp_listings_are_read_only() {
        let tmp = tempfile::tempdir().unwrap();
        let app = build_router(state(tmp.path()));

        let res = app
            .clone()
            .oneshot(get("/api/v1/skills", Some("test-key")))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let res = app
            .oneshot(get("/api/v1/mcp", Some("test-key")))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn unknown_api_version_route_is_404() {
        let tmp = tempfile::tempdir().unwrap();
        let app = build_router(state(tmp.path()));
        let res = app
            .oneshot(get("/api/v2/status", Some("test-key")))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn auth_denials_are_persisted_to_the_audit_store() {
        // Global store: prove the deny event from a 401 lands in it.
        crate::services::audit::record_deny("probe_reset", "test_start", "test");
        let before = crate::services::audit::global().len();

        let tmp = tempfile::tempdir().unwrap();
        let app = build_router(state(tmp.path()));
        let res = app
            .clone()
            .oneshot(get("/api/v1/status", Some("wrong-key")))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        assert!(crate::services::audit::global().len() > before);

        // And the endpoint itself serves the ring, requiring auth.
        let res = app
            .oneshot(get("/api/v1/audit?kind=deny&limit=10", Some("test-key")))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1 << 20)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let entries = v["entries"].as_array().unwrap();
        assert!(!entries.is_empty());
        // Newest first, and the filter kept only denials.
        assert_eq!(entries[0]["kind"], "deny");
        assert!(entries.iter().all(|e| e["kind"] == "deny"));

        // The bearer token itself must never appear in any entry.
        let raw = serde_json::to_string(&entries).unwrap();
        assert!(!raw.contains("test-key"));
        assert!(!raw.contains("wrong-key"));
    }

    #[tokio::test]
    async fn logs_endpoint_serves_ring_with_limit() {
        let tmp = tempfile::tempdir().unwrap();
        let app = build_router(state(tmp.path()));
        let res = app
            .oneshot(get("/api/v1/logs?limit=5", Some("test-key")))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1 << 20)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(v["entries"].as_array().unwrap().len() <= 5);
    }

    #[tokio::test]
    async fn terminal_run_records_high_risk_audit_event_without_args() {
        let tmp = tempfile::tempdir().unwrap();
        let app = build_router(state(tmp.path()));
        let res = app
            .oneshot(post_json(
                "/api/v1/terminal/run",
                Some("test-key"),
                json!({"program": "echo", "args": ["leak-attempt-xyz"]}),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let recent = crate::services::audit::global().recent(50);
        let entry = recent
            .iter()
            .find(|e| e.action == "api_terminal_run")
            .expect("terminal run audited");
        assert_eq!(entry.subject, "echo");
        assert!(entry.detail.starts_with("exit="));
        // Arguments are deliberately never recorded.
        assert!(!format!("{entry:?}").contains("leak-attempt-xyz"));
    }

    #[tokio::test]
    async fn terminal_run_denials_are_audited() {
        let tmp = tempfile::tempdir().unwrap();
        let app = build_router(state(tmp.path()));
        let res = app
            .oneshot(post_json(
                "/api/v1/terminal/run",
                Some("test-key"),
                json!({"program": "   ", "args": []}),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let recent = crate::services::audit::global().recent(50);
        assert!(recent.iter().any(|e| {
            e.action == "api_terminal_run" && e.kind == "deny" && e.detail == "empty_program"
        }));
    }

    #[tokio::test]
    async fn project_create_and_delete_are_audited() {
        let tmp = tempfile::tempdir().unwrap();
        let app = build_router(state(tmp.path()));
        let app = {
            let res = app
                .oneshot(post_json(
                    "/api/v1/projects",
                    Some("test-key"),
                    json!({"name": "audited-proj"}),
                ))
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::OK);
            build_router(state(tmp.path()))
        };
        let res = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("DELETE")
                    .uri("/api/v1/projects/audited-proj")
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let recent = crate::services::audit::global().recent(50);
        assert!(recent
            .iter()
            .any(|e| e.action == "api_project_create" && e.subject == "audited-proj"));
        assert!(recent
            .iter()
            .any(|e| e.action == "api_project_delete" && e.subject == "audited-proj"));
    }

    // ----- Context / Memory / Skills -----

    fn put_json(path: &str, token: Option<&str>, body: serde_json::Value) -> Request<Body> {
        let req = Request::put(path)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        authed(req, token)
    }

    fn delete(path: &str, token: Option<&str>) -> Request<Body> {
        authed(
            Request::builder()
                .method("DELETE")
                .uri(path)
                .body(Body::empty())
                .unwrap(),
            token,
        )
    }

    async fn body_json(res: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn context_roundtrip_and_scope_isolation() {
        let tmp = tempfile::tempdir().unwrap();
        let state_ = state(tmp.path());
        let app = build_router(state_.clone());

        let res = app
            .clone()
            .oneshot(post_json(
                "/api/v1/projects",
                Some("test-key"),
                json!({"name": "scoped"}),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let res = app
            .clone()
            .oneshot(put_json(
                "/api/v1/context?project=scoped",
                Some("test-key"),
                json!({"content": "scoped conventions"}),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let res = app
            .clone()
            .oneshot(get("/api/v1/context?project=scoped", Some("test-key")))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_json(res).await;
        assert_eq!(body["content"], "scoped conventions");

        // Root scope must not see the project's context.
        let res = app
            .clone()
            .oneshot(get("/api/v1/context", Some("test-key")))
            .await
            .unwrap();
        let body = body_json(res).await;
        assert_eq!(body["content"], "");

        // Oversized write is refused and audited as a deny.
        let big = "x".repeat(MAX_CONTEXT_BYTES + 1);
        let res = app
            .clone()
            .oneshot(put_json(
                "/api/v1/context?project=scoped",
                Some("test-key"),
                json!({"content": big}),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let recent = crate::services::audit::global().recent(500);
        assert!(recent
            .iter()
            .any(|e| e.action == "api_context_write" && e.kind == "deny"));
    }

    #[tokio::test]
    async fn context_write_rejects_traversal_and_missing_project() {
        let tmp = tempfile::tempdir().unwrap();
        let app = build_router(state(tmp.path()));
        let res = app
            .clone()
            .oneshot(put_json(
                "/api/v1/context?project=..%2F..%2Fetc",
                Some("test-key"),
                json!({"content": "escape"}),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);

        let res = app
            .oneshot(get("/api/v1/context?project=nope", Some("test-key")))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn memory_append_lists_and_audits() {
        let tmp = tempfile::tempdir().unwrap();
        let app = build_router(state(tmp.path()));

        let res = app
            .clone()
            .oneshot(post_json(
                "/api/v1/memory",
                Some("test-key"),
                json!({"content": "first fact"}),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let res = app
            .clone()
            .oneshot(post_json(
                "/api/v1/memory",
                Some("test-key"),
                json!({"content": "second fact"}),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let res = app
            .clone()
            .oneshot(get("/api/v1/memory", Some("test-key")))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_json(res).await;
        assert_eq!(body["total"], 2);
        assert_eq!(body["entries"][0]["content"], "first fact");
        assert_eq!(body["entries"][1]["content"], "second fact");
        assert!(!body["entries"][0]["timestamp"].as_str().unwrap().is_empty());

        let recent = crate::services::audit::global().recent(500);
        assert!(recent
            .iter()
            .any(|e| e.action == "api_memory_append" && e.subject == "(root)"));

        // Empty content is rejected.
        let res = app
            .oneshot(post_json(
                "/api/v1/memory",
                Some("test-key"),
                json!({"content": "   "}),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn project_skill_references_managed_via_api() {
        let tmp = tempfile::tempdir().unwrap();
        // Global registry discovery uses the user home; point it at a
        // temp root via the ControlState seam so this test cannot see
        // or mutate the real user registry on ANY platform (on
        // Windows, `dirs` ignores `HOME` entirely and resolves via the
        // known-folders API, so env mutation would leak into the real
        // home there).
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join("skills/demo")).unwrap();
        std::fs::write(
            home.path().join("skills/demo/SKILL.md"),
            "---\nname: demo\ndescription: Demo skill\nversion: 0.1.0\n---\n\n# demo\n",
        )
        .unwrap();

        let state = {
            let mut s = ControlState::new(tmp.path(), "test-key".to_string());
            s.global_skills_root = Some(home.path().join("skills"));
            Arc::new(s)
        };
        let app = build_router(state);

        let res = app
            .clone()
            .oneshot(post_json(
                "/api/v1/skills/project",
                Some("test-key"),
                json!({"name": "demo"}),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let res = app
            .clone()
            .oneshot(get("/api/v1/skills/project", Some("test-key")))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_json(res).await;
        assert_eq!(body["skills"][0]["name"], "demo");

        let res = app
            .clone()
            .oneshot(delete("/api/v1/skills/project?name=demo", Some("test-key")))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let res = app
            .oneshot(get("/api/v1/skills/project", Some("test-key")))
            .await
            .unwrap();
        let body = body_json(res).await;
        assert_eq!(body["skills"].as_array().unwrap().len(), 0);

        let recent = crate::services::audit::global().recent(500);
        assert!(recent
            .iter()
            .any(|e| e.action == "api_skill_add" && e.subject == "demo"));
        assert!(recent
            .iter()
            .any(|e| e.action == "api_skill_remove" && e.subject == "demo"));
    }

    #[tokio::test]
    async fn adding_uninstalled_global_skill_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let state = {
            let mut s = ControlState::new(tmp.path(), "test-key".to_string());
            s.global_skills_root = Some(home.path().join("skills"));
            Arc::new(s)
        };
        let app = build_router(state);
        let res = app
            .oneshot(post_json(
                "/api/v1/skills/project",
                Some("test-key"),
                json!({"name": "ghost"}),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    // ----- Rate limiting (spec section 25) -----

    /// State with a tiny 5-per-minute bucket so tests trip the limit fast.
    fn tiny_state(root: &std::path::Path) -> Arc<ControlState> {
        let mut s = Arc::new(ControlState::new(root, "test-key".to_string()));
        Arc::get_mut(&mut s).unwrap().rate_limiter = Arc::new(
            crate::api::rate_limit::RateLimiter::new(5, std::time::Duration::from_secs(60)),
        );
        s
    }

    fn get_with_ip(path: &str, ip: &str) -> Request<Body> {
        authed(
            Request::get(path)
                .header("x-forwarded-for", ip)
                .body(Body::empty())
                .unwrap(),
            Some("test-key"),
        )
    }

    /// The limiter must trip only for the offending client, respond 429
    /// with a `Retry-After` header, audit the deny, and leave health
    /// probes throttling-free.
    #[tokio::test]
    async fn rate_limit_trips_429_audits_and_scopes_to_client() {
        let tmp = tempfile::tempdir().unwrap();
        let app = build_router(tiny_state(tmp.path()));

        for _ in 0..5 {
            let res = app
                .clone()
                .oneshot(get_with_ip("/api/v1/status", "9.9.9.9"))
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::OK);
            assert!(res.headers().contains_key("x-ratelimit-remaining"));
        }

        // 6th request from the same IP: refused with Retry-After.
        let res = app
            .clone()
            .oneshot(get_with_ip("/api/v1/status", "9.9.9.9"))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
        let retry = res
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .expect("retry-after header");
        assert!((1..=60).contains(&retry));
        let body = body_json(res).await;
        assert_eq!(body["error"]["code"], "rate_limited");

        // A different client is unaffected.
        let res = app
            .clone()
            .oneshot(get_with_ip("/api/v1/status", "8.8.8.8"))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // Denials are audited.
        let recent = crate::services::audit::global().recent(500);
        assert!(recent
            .iter()
            .any(|e| e.action == "api_rate_limit" && e.subject == "9.9.9.9" && e.kind == "deny"));

        // Health probes stay unthrottled even for the exhausted client.
        let res = app
            .oneshot(get("/api/v1/healthz", Some("test-key")))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    /// Requests without `X-Forwarded-For` share the `direct` bucket.
    #[tokio::test]
    async fn rate_limit_counts_direct_connections_together() {
        let tmp = tempfile::tempdir().unwrap();
        let state_ = state(tmp.path());
        let app = build_router(state_.clone());

        assert_eq!(state_.rate_limiter.current("direct"), 0);
        for _ in 0..3 {
            let res = app
                .clone()
                .oneshot(get("/api/v1/status", Some("test-key")))
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::OK);
        }
        assert_eq!(state_.rate_limiter.current("direct"), 3);
    }

    /// An invalid token is rejected by auth before the limiter, and
    /// never consumes quota.
    #[tokio::test]
    async fn invalid_tokens_do_not_consume_rate_quota() {
        let tmp = tempfile::tempdir().unwrap();
        let state_ = state(tmp.path());
        let limiter_count = state_.rate_limiter.current("direct");
        let app = build_router(state_.clone());

        for _ in 0..3 {
            let res = app
                .clone()
                .oneshot(get("/api/v1/status", None))
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        }
        assert_eq!(
            state_.rate_limiter.current("direct"),
            limiter_count,
            "auth failures must not touch the rate limiter"
        );
    }
}
