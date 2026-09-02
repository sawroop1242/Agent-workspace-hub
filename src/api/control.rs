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
}

impl ControlState {
    pub fn new(root: impl Into<PathBuf>, api_key: String) -> Self {
        Self {
            root: root.into(),
            api_key,
            started: std::time::Instant::now(),
            version: env!("CARGO_PKG_VERSION"),
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
        .route("/skills", get(list_skills))
        .route("/mcp", get(list_mcp))
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
    state
        .projects()
        .create(&body.name)
        .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
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
        return Err(ApiError::not_found("project"));
    }
    state
        .projects()
        .delete(&name)
        .map_err(|e| ApiError::internal(&e))?;
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
        return Err(ApiError::bad_request("commit message cannot be empty"));
    }
    let git = open_repo(&state)?;
    let out = git
        .commit(&body.message)
        .await
        .map_err(|e| ApiError::internal(&e))?;
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
        return Err(ApiError::bad_request("program is required"));
    }
    let terminal = TerminalService::new(&state.root);
    let outcome = terminal
        .run(&body.program, &body.args)
        .await
        .map_err(|e| ApiError::internal(&e))?;
    Ok(Json(
        serde_json::to_value(outcome).unwrap_or_else(|_| json!({})),
    ))
}

// ----- Skills & MCP (read-only listings) -----

/// Lists globally installed skills. Project-level skill references are
/// managed via the CLI/TUI, which are the trust-owning planes.
async fn list_skills(
    State(_state): State<Arc<ControlState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let registry = GlobalSkillRegistry::discover().map_err(|e| ApiError::internal(&e))?;
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
}
