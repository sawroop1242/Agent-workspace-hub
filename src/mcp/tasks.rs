use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

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
    pub fn create(
        &self,
        id: String,
        title: String,
        description: String,
        priority: TaskPriority,
        tags: Vec<String>,
    ) -> Result<Task> {
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
