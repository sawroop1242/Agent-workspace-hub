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

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str) -> Task {
        Task {
            id: id.into(),
            title: format!("task {id}"),
            status: TaskStatus::Pending,
        }
    }

    #[test]
    fn create_persists_task_json_and_get_round_trips() {
        let temp = tempfile::tempdir().unwrap();
        let store = TaskStore::new(temp.path());
        store.create(&task("t1")).unwrap();
        let loaded = store.get("t1").unwrap().expect("task must exist");
        assert_eq!(loaded.id, "t1");
        assert_eq!(loaded.title, "task t1");
        assert_eq!(loaded.status, TaskStatus::Pending);
    }

    #[test]
    fn get_returns_none_for_unknown_task() {
        let temp = tempfile::tempdir().unwrap();
        let store = TaskStore::new(temp.path());
        assert!(store.get("missing").unwrap().is_none());
    }

    #[test]
    fn list_on_missing_dir_is_empty_and_sorted_when_present() {
        let temp = tempfile::tempdir().unwrap();
        let store = TaskStore::new(temp.path());
        assert!(store.list().unwrap().is_empty());
        store.create(&task("b-task")).unwrap();
        store.create(&task("a-task")).unwrap();
        let listed = store.list().unwrap();
        let ids: Vec<&str> = listed.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["a-task", "b-task"]);
    }

    #[test]
    fn list_ignores_non_json_files_in_tasks_dir() {
        let temp = tempfile::tempdir().unwrap();
        let store = TaskStore::new(temp.path());
        store.create(&task("t1")).unwrap();
        let tasks_dir = temp.path().join(".agent").join("tasks");
        fs::write(tasks_dir.join("notes.txt"), "not a task").unwrap();
        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "t1");
    }

    #[test]
    fn set_status_updates_and_reports_existence() {
        let temp = tempfile::tempdir().unwrap();
        let store = TaskStore::new(temp.path());
        assert!(!store.set_status("missing", TaskStatus::Completed).unwrap());
        store.create(&task("t1")).unwrap();
        assert!(store.set_status("t1", TaskStatus::InProgress).unwrap());
        assert_eq!(
            store.get("t1").unwrap().unwrap().status,
            TaskStatus::InProgress
        );
        assert!(store.set_status("t1", TaskStatus::Completed).unwrap());
        assert_eq!(
            store.get("t1").unwrap().unwrap().status,
            TaskStatus::Completed
        );
    }

    #[test]
    fn create_overwrites_task_with_same_id() {
        let temp = tempfile::tempdir().unwrap();
        let store = TaskStore::new(temp.path());
        store.create(&task("t1")).unwrap();
        let mut updated = task("t1");
        updated.title = "renamed".into();
        updated.status = TaskStatus::Cancelled;
        store.create(&updated).unwrap();
        let loaded = store.get("t1").unwrap().unwrap();
        assert_eq!(loaded.title, "renamed");
        assert_eq!(loaded.status, TaskStatus::Cancelled);
        assert_eq!(store.list().unwrap().len(), 1);
    }

    #[test]
    fn task_status_serde_uses_snake_case() {
        assert_eq!(
            serde_json::to_string(&TaskStatus::InProgress).unwrap(),
            "\"in_progress\""
        );
        let back: TaskStatus = serde_json::from_str("\"in_progress\"").unwrap();
        assert_eq!(back, TaskStatus::InProgress);
    }

    #[test]
    fn safe_task_ids_are_accepted() {
        for id in ["t1", "task-2", "task_3", "TASK", "a.b.c", "t-1_x.2"] {
            assert!(is_safe_task_id(id), "{id} should be safe");
        }
    }

    #[test]
    fn traversal_and_separator_ids_are_rejected() {
        for id in [
            "",
            ".",
            "..",
            "a/b",
            "a\\b",
            "../escape",
            "..\\escape",
            "/absolute",
            "trailing/",
        ] {
            assert!(!is_safe_task_id(id), "{id:?} must be rejected");
        }
    }
}
