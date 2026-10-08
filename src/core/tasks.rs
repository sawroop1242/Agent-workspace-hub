//! Canonical AWH task store (ARCH-001).
//!
//! One authoritative task representation per project scope, persisted at
//! `.agent/tasks.json` as a JSON object holding one JSON array of [`Task`]
//! records. The MCP task tools are thin translations over this store; no
//! interface plane may persist a second task file. (The previous
//! one-file-per-task store under `.agent/tasks/<id>.json` had no
//! production callers and was removed by ARCH-001.)

use crate::core::agents::AgentStore;
use crate::core::sessions::SessionStore;
use crate::mcp::store_lock::StoreLock;
use crate::services::worktree::WorktreeStore;
use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Maximum number of tasks a single project task store may hold.
const MAX_TASKS: usize = 10_000;
/// Maximum length of a task id.
const MAX_TASK_ID_LEN: usize = 256;
/// Maximum length of a task title.
const MAX_TASK_TITLE_LEN: usize = 1024;
/// Maximum length of a task description.
const MAX_TASK_DESCRIPTION_LEN: usize = 64 * 1024;
/// Maximum number of tags per task.
const MAX_TASK_TAGS: usize = 64;
/// Maximum length of a single tag.
const MAX_TASK_TAG_LEN: usize = 128;

/// A single tracked task with status, priority, and optional assignee.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique task id.
    pub id: String,
    /// Short human-readable summary.
    pub title: String,
    /// Longer description of the work.
    pub description: String,
    /// Current lifecycle state.
    pub status: TaskStatus,
    /// Relative importance.
    pub priority: TaskPriority,
    /// Optional owner/assignee.
    pub assignee: Option<String>,
    /// Arbitrary tags for categorization.
    pub tags: Vec<String>,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
    /// RFC 3339 last-update timestamp.
    pub updated_at: String,
}

/// Lifecycle state of a [`Task`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus {
    /// Not yet started.
    Todo,
    /// Currently in progress.
    InProgress,
    /// Blocked by a dependency.
    Blocked,
    /// Completed (terminal).
    Done,
    /// Abandoned without completing (terminal). The CLI `task cancel`
    /// verb lands here; a cancelled task is never silently reopened —
    /// create a new task instead.
    Cancelled,
}

impl TaskStatus {
    /// The canonical wire name (identical to the serde representation),
    /// used in stable error messages.
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Todo => "Todo",
            TaskStatus::InProgress => "InProgress",
            TaskStatus::Blocked => "Blocked",
            TaskStatus::Done => "Done",
            TaskStatus::Cancelled => "Cancelled",
        }
    }

    /// Terminal states have no outgoing transitions.
    pub fn is_terminal(&self) -> bool {
        matches!(self, TaskStatus::Done | TaskStatus::Cancelled)
    }
}

/// The task state machine: `Done` and `Cancelled` are terminal — every
/// transition out of them is invalid, so a completed or abandoned task
/// can never be silently reopened (create a new task instead). All
/// moves among the live states (`Todo`/`InProgress`/`Blocked`) are
/// legal, and a live task may be completed or cancelled at any time.
pub fn is_valid_task_transition(from: &TaskStatus, _to: &TaskStatus) -> bool {
    !from.is_terminal()
}

/// Relative importance of a [`Task`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskPriority {
    /// Lowest importance.
    Low,
    /// Default importance.
    Normal,
    /// Important.
    High,
    /// Most important.
    Critical,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct TaskFile {
    tasks: Vec<Task>,
}

/// Canonical project-scoped task store persisted to `.agent/tasks.json`.
pub struct TaskStore {
    path: PathBuf,
    root: PathBuf,
}

impl TaskStore {
    /// Creates a task store backed by `.agent/tasks.json` under the project root.
    pub fn new(project_root: impl Into<PathBuf>) -> Result<Self> {
        let root = project_root.into();
        fs::create_dir_all(root.join(".agent"))?;
        Ok(Self {
            path: root.join(".agent").join("tasks.json"),
            root,
        })
    }

    fn load(&self) -> Result<TaskFile> {
        if !self.path.exists() {
            return Ok(TaskFile::default());
        }
        Ok(serde_json::from_str(&fs::read_to_string(&self.path)?)?)
    }

    /// Writes `file` atomically (temp file + rename) so a sibling agent
    /// process reading `.agent/tasks.json` never observes a torn write.
    fn save(&self, file: &TaskFile) -> Result<()> {
        let parent = self
            .path
            .parent()
            .context("task store path has no parent directory")?;
        let mut temp =
            tempfile::NamedTempFile::new_in(parent).context("failed to create temp file")?;
        std::io::Write::write_all(&mut temp, serde_json::to_string_pretty(file)?.as_bytes())?;
        temp.as_file().sync_all()?;
        temp.persist(&self.path)
            .map_err(|error| error.error)
            .context("failed to atomically write task store")?;
        Ok(())
    }

    /// Creates (or replaces) a task, starting in the `Todo` state.
    ///
    /// Fails closed when the task would exceed the store's enforced size
    /// limits (task count, id/title/description length, tag count/length),
    /// so a misbehaving client cannot grow the on-disk store without bound.
    ///
    /// Holds a [`StoreLock`] across the load-modify-save cycle so a second
    /// agent process working the same project at the same time serializes
    /// behind it instead of racing (see `crate::mcp::store_lock`).
    pub fn create(
        &self,
        id: String,
        title: String,
        description: String,
        priority: TaskPriority,
        tags: Vec<String>,
    ) -> Result<Task> {
        validate_task_input(&id, &title, &description, &tags)?;
        let _lock = StoreLock::acquire(&self.path)?;
        let now = Utc::now().to_rfc3339();
        let task = Task {
            id,
            title,
            description,
            status: TaskStatus::Todo,
            priority,
            assignee: None,
            tags,
            created_at: now.clone(),
            updated_at: now,
        };
        let mut file = self.load()?;
        let exists = file.tasks.iter().any(|t| t.id == task.id);
        if !exists && file.tasks.len() >= MAX_TASKS {
            bail!("task store is full (max {MAX_TASKS} tasks)");
        }
        file.tasks.retain(|t| t.id != task.id);
        file.tasks.push(task.clone());
        self.save(&file)?;
        Ok(task)
    }

    /// Lists tasks, optionally filtered by status.
    pub fn list(&self, status: Option<TaskStatus>) -> Result<Vec<Task>> {
        Ok(self
            .load()?
            .tasks
            .into_iter()
            .filter(|t| status.as_ref().is_none_or(|s| &t.status == s))
            .collect())
    }

    /// Returns the task with the given `id`, if present — the single-item
    /// counterpart to [`Self::list`], so a caller doesn't have to list every
    /// task and filter client-side.
    pub fn get(&self, id: &str) -> Result<Option<Task>> {
        Ok(self.load()?.tasks.into_iter().find(|t| t.id == id))
    }

    /// Updates a task's status, priority, and/or assignee, returning the updated task.
    ///
    /// Status changes run through the canonical state machine
    /// ([`is_valid_task_transition`]): terminal states (`Done`,
    /// `Cancelled`) have no outgoing transitions, so a completed or
    /// abandoned task can never be silently reopened. Assignment
    /// targets must exist in this workspace as a canonical agent,
    /// session, or worktree id — assignment is metadata only and never
    /// grants the assignee capabilities.
    pub fn update(
        &self,
        id: &str,
        status: Option<TaskStatus>,
        priority: Option<TaskPriority>,
        assignee: Option<Option<String>>,
    ) -> Result<Option<Task>> {
        if let Some(target) = assignee.as_ref().and_then(|a| a.as_deref()) {
            if !target.is_empty() {
                self.validate_assignee(target)?;
            }
        }
        let _lock = StoreLock::acquire(&self.path)?;
        let mut file = self.load()?;
        let task = match file.tasks.iter_mut().find(|t| t.id == id) {
            Some(t) => t,
            None => return Ok(None),
        };
        if status.is_none() && priority.is_none() && assignee.is_none() {
            // Same error class the CLI surfaces for a fieldless `awh task
            // update`: a mutation call must name what it changes, on
            // every interface plane.
            bail!("no changes requested: provide at least one of status, priority, or assignee");
        }
        if let Some(new_status) = &status {
            if &task.status != new_status && !is_valid_task_transition(&task.status, new_status) {
                bail!(
                    "invalid task transition {} -> {}: terminal tasks cannot be reopened; create a new task instead",
                    task.status.as_str(),
                    new_status.as_str()
                );
            }
            task.status = new_status.clone();
        }
        if let Some(v) = priority {
            task.priority = v;
        }
        if let Some(v) = assignee {
            task.assignee = v;
        }
        task.updated_at = Utc::now().to_rfc3339();
        let result = task.clone();
        self.save(&file)?;
        Ok(Some(result))
    }

    /// Cancels a task: the live-state → `Cancelled` transition with the
    /// same validation and locking as [`Self::update`]. Cancelling an
    /// already-terminal task is a stable conflict error (never a silent
    /// success), and an unknown id fails closed.
    pub fn cancel(&self, id: &str) -> Result<Task> {
        let _lock = StoreLock::acquire(&self.path)?;
        let mut file = self.load()?;
        let task = file
            .tasks
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or_else(|| anyhow::anyhow!("task not found: {id}"))?;
        if task.status.is_terminal() {
            bail!(
                "invalid task transition {} -> Cancelled: terminal tasks cannot be reopened; create a new task instead",
                task.status.as_str()
            );
        }
        task.status = TaskStatus::Cancelled;
        task.updated_at = Utc::now().to_rfc3339();
        let result = task.clone();
        self.save(&file)?;
        Ok(result)
    }

    /// Assignment targets must be canonical ids that exist in this
    /// workspace: a runtime session (`sess-…`), a managed worktree
    /// record (`wt-…`), or an agent profile id (free-form safe ids like
    /// `writer`; the agent store fails closed on unsafe ids, so an
    /// arbitrary string simply resolves to "not found"). Free-text
    /// targets that match nothing are rejected — assignment is a
    /// reference, not a capability grant, and a dangling reference is
    /// a lie about who owns the work.
    fn validate_assignee(&self, target: &str) -> Result<()> {
        let found = if target.starts_with("sess-") {
            SessionStore::new(&self.root)
                .get(target)
                .map(|s| s.is_some())?
        } else if target.starts_with("wt-") {
            WorktreeStore::new(self.root.clone())
                .list()
                .map_err(anyhow::Error::from)
                .map(|records| records.iter().any(|r| r.worktree_id == target))?
        } else {
            AgentStore::new(&self.root)
                .get(target)
                .map(|a| a.is_some())?
        };
        if !found {
            bail!(
                "assignee not found: {target} (must be an existing agent profile, sess-, or wt- id in this workspace)"
            );
        }
        Ok(())
    }

    /// Deletes the task with the given `id`, returning whether it existed.
    pub fn delete(&self, id: &str) -> Result<bool> {
        let _lock = StoreLock::acquire(&self.path)?;
        let mut file = self.load()?;
        let before = file.tasks.len();
        file.tasks.retain(|t| t.id != id);
        self.save(&file)?;
        Ok(before != file.tasks.len())
    }
}

/// Rejects task inputs that would violate the store's size limits.
fn validate_task_input(id: &str, title: &str, description: &str, tags: &[String]) -> Result<()> {
    if id.trim().is_empty() {
        bail!("task id must not be empty");
    }
    if id.len() > MAX_TASK_ID_LEN {
        bail!("task id exceeds {MAX_TASK_ID_LEN} bytes");
    }
    if title.trim().is_empty() {
        bail!("task title must not be empty");
    }
    if title.len() > MAX_TASK_TITLE_LEN {
        bail!("task title exceeds {MAX_TASK_TITLE_LEN} bytes");
    }
    if description.len() > MAX_TASK_DESCRIPTION_LEN {
        bail!("task description exceeds {MAX_TASK_DESCRIPTION_LEN} bytes");
    }
    if tags.len() > MAX_TASK_TAGS {
        bail!("task exceeds {MAX_TASK_TAGS} tags");
    }
    if let Some(tag) = tags.iter().find(|t| t.len() > MAX_TASK_TAG_LEN) {
        bail!("task tag exceeds {MAX_TASK_TAG_LEN} bytes: {tag:?}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (TaskStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = TaskStore::new(dir.path()).unwrap();
        (store, dir)
    }

    #[test]
    fn terminal_states_have_no_outgoing_transitions() {
        // live states may move freely and may finish or cancel
        for from in [
            TaskStatus::Todo,
            TaskStatus::InProgress,
            TaskStatus::Blocked,
        ] {
            for to in [
                TaskStatus::Todo,
                TaskStatus::InProgress,
                TaskStatus::Blocked,
                TaskStatus::Done,
                TaskStatus::Cancelled,
            ] {
                assert!(
                    is_valid_task_transition(&from, &to),
                    "{from:?} -> {to:?} must be legal"
                );
            }
        }
        // terminal states are terminal
        for from in [TaskStatus::Done, TaskStatus::Cancelled] {
            for to in [
                TaskStatus::Todo,
                TaskStatus::InProgress,
                TaskStatus::Blocked,
                TaskStatus::Done,
                TaskStatus::Cancelled,
            ] {
                assert!(
                    !is_valid_task_transition(&from, &to),
                    "{from:?} -> {to:?} must be rejected"
                );
            }
        }
    }

    #[test]
    fn update_rejects_reopening_terminal_tasks() {
        let (store, _dir) = temp_store();
        let (id, title, description, tags) = make_task("t1");
        store
            .create(id, title, description, TaskPriority::Normal, tags)
            .unwrap();

        // Todo -> Done is legal
        store
            .update("t1", Some(TaskStatus::Done), None, None)
            .unwrap()
            .expect("task exists");

        // Done -> InProgress is rejected, and the store did not mutate
        let err = store
            .update("t1", Some(TaskStatus::InProgress), None, None)
            .unwrap_err();
        assert!(err
            .to_string()
            .contains("invalid task transition Done -> InProgress"));
        assert_eq!(store.get("t1").unwrap().unwrap().status, TaskStatus::Done);

        // same-status update stays a permitted no-op (idempotent)
        store
            .update("t1", Some(TaskStatus::Done), None, None)
            .unwrap()
            .expect("task exists");
    }

    #[test]
    fn cancel_is_terminal_and_repeated_cancel_fails_closed() {
        let (store, _dir) = temp_store();
        let (id, title, description, tags) = make_task("t1");
        store
            .create(id, title, description, TaskPriority::Normal, tags)
            .unwrap();

        let cancelled = store.cancel("t1").unwrap();
        assert_eq!(cancelled.status, TaskStatus::Cancelled);

        // replayed cancel: stable conflict error, never a silent success
        let err = store.cancel("t1").unwrap_err();
        assert!(err
            .to_string()
            .contains("invalid task transition Cancelled -> Cancelled"));

        // cancel of a Done task is rejected too
        let (id, title, description, tags) = make_task("t2");
        store
            .create(id, title, description, TaskPriority::Normal, tags)
            .unwrap();
        store
            .update("t2", Some(TaskStatus::Done), None, None)
            .unwrap();
        assert!(store
            .cancel("t2")
            .unwrap_err()
            .to_string()
            .contains("invalid task transition Done -> Cancelled"));

        // unknown id fails closed
        assert!(store.cancel("ghost").is_err());
    }

    #[test]
    fn assignee_must_exist_in_this_workspace() {
        let (store, dir) = temp_store();
        let (id, title, description, tags) = make_task("t1");
        store
            .create(id, title, description, TaskPriority::Normal, tags)
            .unwrap();

        // unknown free-text target rejected
        let err = store
            .update("t1", None, None, Some(Some("nobody".to_string())))
            .unwrap_err();
        assert!(err.to_string().contains("assignee not found: nobody"));

        // unknown session-shaped id rejected (not merely prefix-trusted)
        let err = store
            .update("t1", None, None, Some(Some("sess-ghost".to_string())))
            .unwrap_err();
        assert!(err.to_string().contains("assignee not found: sess-ghost"));

        // a real agent profile is accepted
        crate::core::agents::AgentStore::new(dir.path())
            .create(&crate::models::agent::Agent {
                id: "writer".to_string(),
                name: "Writer".to_string(),
                role: "writer".to_string(),
                status: crate::models::agent::AgentStatus::Active,
                enabled: true,
                created_at: chrono::Utc::now().to_rfc3339(),
            })
            .unwrap();
        store
            .update("t1", None, None, Some(Some("writer".to_string())))
            .unwrap()
            .expect("task exists");
        assert_eq!(
            store.get("t1").unwrap().unwrap().assignee.as_deref(),
            Some("writer")
        );

        // clearing the assignee needs no validation
        store
            .update("t1", None, None, Some(None))
            .unwrap()
            .expect("task exists");
        assert_eq!(store.get("t1").unwrap().unwrap().assignee, None);
    }

    #[test]
    fn legacy_records_without_cancelled_still_parse() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".agent")).unwrap();
        std::fs::write(
            dir.path().join(".agent/tasks.json"),
            r#"{"tasks":[{"id":"old-1","title":"t","description":"d","status":"Done","priority":"Normal","assignee":null,"tags":[],"created_at":"2024-01-01T00:00:00+00:00","updated_at":"2024-01-01T00:00:00+00:00"}]}"#,
        )
        .unwrap();
        let store = TaskStore::new(dir.path()).unwrap();
        let task = store.get("old-1").unwrap().expect("legacy record");
        assert_eq!(task.status, TaskStatus::Done);
        // and it stays terminal
        assert!(store
            .update("old-1", Some(TaskStatus::Todo), None, None)
            .is_err());
    }

    fn make_task(id: &str) -> (String, String, String, Vec<String>) {
        (
            id.to_string(),
            "title".to_string(),
            "description".to_string(),
            vec![],
        )
    }

    #[test]
    fn create_rejects_empty_id_and_title() {
        let (store, _dir) = temp_store();
        let (id, title, description, tags) = make_task("");
        assert!(store
            .create(id, title, description, TaskPriority::Normal, tags)
            .is_err());

        // Empty title is rejected even with a valid id.
        let (_, _, description, tags) = make_task("id");
        assert!(store
            .create(
                "id".into(),
                String::new(),
                description,
                TaskPriority::Normal,
                tags
            )
            .is_err());
    }

    #[test]
    fn create_rejects_oversized_fields() {
        let (store, _dir) = temp_store();
        let oversized_title = "t".repeat(MAX_TASK_TITLE_LEN + 1);
        assert!(store
            .create(
                "id".into(),
                oversized_title,
                "d".into(),
                TaskPriority::Normal,
                vec![]
            )
            .is_err());

        let oversized_desc = "d".repeat(MAX_TASK_DESCRIPTION_LEN + 1);
        assert!(store
            .create(
                "id2".into(),
                "title".into(),
                oversized_desc,
                TaskPriority::Normal,
                vec![]
            )
            .is_err());

        let oversized_id = "i".repeat(MAX_TASK_ID_LEN + 1);
        assert!(store
            .create(
                oversized_id,
                "title".into(),
                "d".into(),
                TaskPriority::Normal,
                vec![]
            )
            .is_err());
    }

    #[test]
    fn create_rejects_too_many_tags_and_oversized_tags() {
        let (store, _dir) = temp_store();
        let many: Vec<String> = (0..MAX_TASK_TAGS + 1).map(|i| i.to_string()).collect();
        assert!(store
            .create(
                "id".into(),
                "title".into(),
                "d".into(),
                TaskPriority::Normal,
                many
            )
            .is_err());

        let long_tag = vec!["t".repeat(MAX_TASK_TAG_LEN + 1)];
        assert!(store
            .create(
                "id2".into(),
                "title".into(),
                "d".into(),
                TaskPriority::Normal,
                long_tag
            )
            .is_err());
    }

    #[test]
    fn create_enforces_task_count_limit() {
        let (store, _dir) = temp_store();
        // Seed the store at capacity via pre-serialized state.
        let tasks: Vec<Task> = (0..MAX_TASKS as u64)
            .map(|i| Task {
                id: format!("task-{i}"),
                title: "t".into(),
                description: String::new(),
                status: TaskStatus::Todo,
                priority: TaskPriority::Normal,
                assignee: None,
                tags: vec![],
                created_at: String::new(),
                updated_at: String::new(),
            })
            .collect();
        fs::write(
            store.path.clone(),
            serde_json::to_string(&TaskFile { tasks }).unwrap(),
        )
        .unwrap();

        // Creating one more must fail.
        assert!(store
            .create(
                "overflow".into(),
                "title".into(),
                "d".into(),
                TaskPriority::Normal,
                vec![]
            )
            .is_err());

        // Replacing an existing task must still succeed.
        assert!(store
            .create(
                "task-0".into(),
                "updated".into(),
                "d".into(),
                TaskPriority::High,
                vec![]
            )
            .is_ok());
    }

    #[test]
    fn update_without_changes_fails_closed_on_every_plane() {
        let (store, _dir) = temp_store();
        let (id, title, description, tags) = make_task("t1");
        store
            .create(id, title, description, TaskPriority::Normal, tags)
            .unwrap();

        // Fieldless update: same error class the CLI's argument guard
        // produces — the store is the authority so MCP callers hit it too.
        let err = store.update("t1", None, None, None).unwrap_err();
        assert!(
            err.to_string().contains(
                "no changes requested: provide at least one of status, priority, or assignee"
            ),
            "got: {err}"
        );

        // An unknown id still resolves to `Ok(None)` (missing → null, not
        // an error) — the guard fires only when a real task would be
        // pointlessly rewritten.
        assert!(store.update("missing", None, None, None).unwrap().is_none());
    }

    #[test]
    fn update_validates_status_transition_and_missing_task() {
        let (store, _dir) = temp_store();
        assert!(store
            .update("missing", Some(TaskStatus::Done), None, None)
            .unwrap()
            .is_none());

        let (id, title, description, tags) = make_task("t1");
        store
            .create(id, title, description, TaskPriority::Normal, tags)
            .unwrap();
        let updated = store
            .update("t1", Some(TaskStatus::InProgress), None, None)
            .unwrap()
            .unwrap();
        assert_eq!(updated.status, TaskStatus::InProgress);
    }

    #[test]
    fn delete_removes_task_and_reports_existence() {
        let (store, _dir) = temp_store();
        let (id, title, description, tags) = make_task("t1");
        store
            .create(id, title, description, TaskPriority::Normal, tags)
            .unwrap();
        assert!(store.delete("t1").unwrap());
        assert!(!store.delete("t1").unwrap());
    }

    #[test]
    fn get_returns_matching_task_and_none_for_missing() {
        let (store, _dir) = temp_store();
        let (id, title, description, tags) = make_task("t1");
        store
            .create(id, title, description, TaskPriority::Normal, tags)
            .unwrap();
        let task = store.get("t1").unwrap().expect("task t1 should exist");
        assert_eq!(task.id, "t1");
        assert_eq!(task.title, "title");
        assert!(store.get("missing").unwrap().is_none());
        // A missing id is distinct from an empty one: both are lookups, not
        // errors — the caller distinguishes by the Option.
        assert!(store.get("").unwrap().is_none());
    }

    #[test]
    fn corrupted_store_fails_closed() {
        let (store, _dir) = temp_store();
        fs::write(&store.path, "not json {").unwrap();
        assert!(store
            .create(
                "id".into(),
                "title".into(),
                "d".into(),
                TaskPriority::Normal,
                vec![]
            )
            .is_err());
        assert!(store.list(None).is_err());
    }
}
