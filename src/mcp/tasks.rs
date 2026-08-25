use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub priority: TaskPriority,
    pub assignee: Option<String>,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus { Todo, InProgress, Blocked, Done }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskPriority { Low, Normal, High, Critical }

#[derive(Debug, Default, Serialize, Deserialize)]
struct TaskStore { tasks: Vec<Task> }

pub struct TasksMcp { path: PathBuf }

impl TasksMcp {
    pub fn new(project_root: impl Into<PathBuf>) -> Result<Self> {
        let root = project_root.into();
        fs::create_dir_all(root.join(".agent"))?;
        Ok(Self { path: root.join(".agent").join("tasks.json") })
    }

    fn load(&self) -> Result<TaskStore> {
        if !self.path.exists() { return Ok(TaskStore::default()); }
        Ok(serde_json::from_str(&fs::read_to_string(&self.path)?)?)
    }

    fn save(&self, store: &TaskStore) -> Result<()> {
        fs::write(&self.path, serde_json::to_string_pretty(store)?)?;
        Ok(())
    }

    pub fn create(&self, id: String, title: String, description: String, priority: TaskPriority, tags: Vec<String>) -> Result<Task> {
        let now = Utc::now().to_rfc3339();
        let task = Task { id, title, description, status: TaskStatus::Todo, priority, assignee: None, tags, created_at: now.clone(), updated_at: now };
        let mut store = self.load()?;
        store.tasks.retain(|t| t.id != task.id);
        store.tasks.push(task.clone());
        self.save(&store)?;
        Ok(task)
    }

    pub fn list(&self, status: Option<TaskStatus>) -> Result<Vec<Task>> {
        Ok(self.load()?.tasks.into_iter().filter(|t| status.as_ref().map_or(true, |s| &t.status == s)).collect())
    }

    pub fn update(&self, id: &str, status: Option<TaskStatus>, priority: Option<TaskPriority>, assignee: Option<Option<String>>) -> Result<Option<Task>> {
        let mut store = self.load()?;
        let task = match store.tasks.iter_mut().find(|t| t.id == id) { Some(t) => t, None => return Ok(None) };
        if let Some(v) = status { task.status = v; }
        if let Some(v) = priority { task.priority = v; }
        if let Some(v) = assignee { task.assignee = v; }
        task.updated_at = Utc::now().to_rfc3339();
        let result = task.clone();
        self.save(&store)?;
        Ok(Some(result))
    }

    pub fn delete(&self, id: &str) -> Result<bool> {
        let mut store = self.load()?;
        let before = store.tasks.len();
        store.tasks.retain(|t| t.id != id);
        self.save(&store)?;
        Ok(before != store.tasks.len())
    }
}
