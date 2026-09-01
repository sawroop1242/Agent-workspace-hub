use anyhow::{bail, Result};
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
    /// Completed.
    Done,
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
struct TaskStore {
    tasks: Vec<Task>,
}

/// Project-scoped MCP task store persisted to `.agent/tasks.json`.
pub struct TasksMcp {
    path: PathBuf,
}

impl TasksMcp {
    /// Creates a task store backed by `.agent/tasks.json` under the project root.
    pub fn new(project_root: impl Into<PathBuf>) -> Result<Self> {
        let root = project_root.into();
        fs::create_dir_all(root.join(".agent"))?;
        Ok(Self {
            path: root.join(".agent").join("tasks.json"),
        })
    }

    fn load(&self) -> Result<TaskStore> {
        if !self.path.exists() {
            return Ok(TaskStore::default());
        }
        Ok(serde_json::from_str(&fs::read_to_string(&self.path)?)?)
    }

    fn save(&self, store: &TaskStore) -> Result<()> {
        fs::write(&self.path, serde_json::to_string_pretty(store)?)?;
        Ok(())
    }

    /// Creates (or replaces) a task, starting in the `Todo` state.
    ///
    /// Fails closed when the task would exceed the store's enforced size
    /// limits (task count, id/title/description length, tag count/length),
    /// so a misbehaving client cannot grow the on-disk store without bound.
    pub fn create(
        &self,
        id: String,
        title: String,
        description: String,
        priority: TaskPriority,
        tags: Vec<String>,
    ) -> Result<Task> {
        validate_task_input(&id, &title, &description, &tags)?;
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
        let mut store = self.load()?;
        let exists = store.tasks.iter().any(|t| t.id == task.id);
        if !exists && store.tasks.len() >= MAX_TASKS {
            bail!("task store is full (max {MAX_TASKS} tasks)");
        }
        store.tasks.retain(|t| t.id != task.id);
        store.tasks.push(task.clone());
        self.save(&store)?;
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

    /// Updates a task's status, priority, and/or assignee, returning the updated task.
    pub fn update(
        &self,
        id: &str,
        status: Option<TaskStatus>,
        priority: Option<TaskPriority>,
        assignee: Option<Option<String>>,
    ) -> Result<Option<Task>> {
        let mut store = self.load()?;
        let task = match store.tasks.iter_mut().find(|t| t.id == id) {
            Some(t) => t,
            None => return Ok(None),
        };
        if let Some(v) = status {
            task.status = v;
        }
        if let Some(v) = priority {
            task.priority = v;
        }
        if let Some(v) = assignee {
            task.assignee = v;
        }
        task.updated_at = Utc::now().to_rfc3339();
        let result = task.clone();
        self.save(&store)?;
        Ok(Some(result))
    }

    /// Deletes the task with the given `id`, returning whether it existed.
    pub fn delete(&self, id: &str) -> Result<bool> {
        let mut store = self.load()?;
        let before = store.tasks.len();
        store.tasks.retain(|t| t.id != id);
        self.save(&store)?;
        Ok(before != store.tasks.len())
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

    fn temp_store() -> (TasksMcp, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = TasksMcp::new(dir.path()).unwrap();
        (store, dir)
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
            serde_json::to_string(&TaskStore { tasks }).unwrap(),
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
