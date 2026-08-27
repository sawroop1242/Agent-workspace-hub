use crate::models::{Task, TaskStatus};
use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

/// Persistent task storage under `.agent/tasks` as per-task JSON files.
pub struct TaskStore {
    root: PathBuf,
}

impl TaskStore {
    /// Creates a `TaskStore` rooted at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn tasks_dir(&self) -> PathBuf {
        self.root.join(".agent").join("tasks")
    }

    /// Creates (or overwrites) a task, keyed by its `id`.
    pub fn create(&self, task: &Task) -> Result<()> {
        fs::create_dir_all(self.tasks_dir())?;
        let path = self.tasks_dir().join(format!("{}.json", task.id));
        let data = serde_json::to_string_pretty(task)?;
        fs::write(path, data)?;
        Ok(())
    }

    /// Returns the task with the given `id`, or `None` if it does not exist.
    pub fn get(&self, id: &str) -> Result<Option<Task>> {
        let path = self.tasks_dir().join(format!("{}.json", id));
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(&fs::read_to_string(path)?)?))
    }

    /// Lists all stored tasks sorted by `id`.
    pub fn list(&self) -> Result<Vec<Task>> {
        let dir = self.tasks_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut tasks = Vec::new();
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            let task: Task = serde_json::from_str(&fs::read_to_string(path)?)?;
            tasks.push(task);
        }
        tasks.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(tasks)
    }

    /// Updates a task's status, returning `false` if the task does not exist.
    pub fn set_status(&self, id: &str, status: TaskStatus) -> Result<bool> {
        let Some(mut task) = self.get(id)? else {
            return Ok(false);
        };
        task.status = status;
        self.create(&task)?;
        Ok(true)
    }
}

/// Returns whether `id` is safe to use as a task filename (no path separators or traversal).
pub fn is_safe_task_id(id: &str) -> bool {
    !id.is_empty()
        && id != "."
        && id != ".."
        && !id.contains('/')
        && !id.contains('\\')
        && Path::new(id).file_name().and_then(|x| x.to_str()) == Some(id)
}
