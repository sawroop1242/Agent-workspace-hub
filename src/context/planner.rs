//! Lightweight deterministic planning.
//!
//! Given a task, the planner identifies the context likely needed next:
//! relevant workspace files (by lexical overlap with the task, discovered via
//! a bounded walk — never a full unbounded repository scan), referenced
//! skills, relevant memories, and items that are currently offloaded. The
//! interface is a trait so an LLM planner can slot in later without changing
//! call sites; the default implementation needs no model at all.

use crate::context::item::ContextSource;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

/// Default maximum number of directory entries examined per walk (bounded).
const MAX_WALK_ENTRIES: usize = 1_000;
/// Default maximum files returned by the planner.
const MAX_FILE_HINTS: usize = 20;
/// Default maximum memory hints.
const MAX_MEMORY_HINTS: usize = 10;

/// One hint about context likely relevant to a task.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlanHint {
    /// What kind of hint this is.
    pub kind: PlanHintKind,
    /// The hint target (file path, skill name, memory id, item id, …).
    pub target: String,
    /// Deterministic confidence in `[0.0, 1.0]`.
    pub score: f32,
    /// Why the planner surfaced this hint.
    pub reason: String,
}

/// The kinds of hints the planner can produce.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PlanHintKind {
    /// A workspace file likely required.
    RequiredFile,
    /// A skill referenced by the project that lexically matches.
    RelevantSkill,
    /// A memory entry lexically matching the task.
    RelevantMemory,
    /// An offloaded item likely worth restoring.
    OffloadedContext,
    /// Tool output the planner expects (informational).
    LikelyToolOutput,
}

/// The planner abstraction.
pub trait ContextPlanner: Send + Sync {
    /// Plans the likely-needed context for `task`.
    fn plan(&self, task: &str) -> Result<Vec<PlanHint>, anyhow::Error>;
}

/// The deterministic default planner.
pub struct DeterministicPlanner {
    project_root: PathBuf,
}

impl DeterministicPlanner {
    /// Creates a planner bound to a project root.
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: project_root.into(),
        }
    }

    /// Lexical overlap score between task text and candidate text.
    fn overlap(task_words: &HashSet<String>, candidate: &str) -> f32 {
        Self::overlap_owned(task_words, &candidate.to_lowercase())
    }

    fn task_words(task: &str) -> HashSet<String> {
        task.to_lowercase()
            .split_whitespace()
            .filter(|w| w.len() > 2)
            .map(str::to_string)
            .collect()
    }

    /// Shared-word overlap over owned word sets.
    fn overlap_owned(task_words: &HashSet<String>, candidate_lower: &str) -> f32 {
        if task_words.is_empty() {
            return 0.0;
        }
        let candidate_words: HashSet<String> = candidate_lower
            .split_whitespace()
            .filter(|w| w.len() > 2)
            .map(str::to_string)
            .collect();
        if candidate_words.is_empty() {
            return 0.0;
        }
        let shared = task_words.intersection(&candidate_words).count();
        shared as f32 / task_words.len().max(candidate_words.len()) as f32
    }

    /// Bounded walk collecting candidate files (paths only, no content reads).
    fn candidate_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(self.project_root.clone());
        let mut examined = 0usize;
        while let Some(dir) = queue.pop_front() {
            if examined >= MAX_WALK_ENTRIES {
                break;
            }
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                examined += 1;
                if examined > MAX_WALK_ENTRIES {
                    break;
                }
                let path = entry.path();
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                // Skip hidden and state/dependency directories; the engine
                // never walks into them.
                if path.is_dir() {
                    if !name.starts_with('.')
                        && !matches!(name, "target" | "node_modules" | "vendor")
                    {
                        queue.push_back(path);
                    }
                    continue;
                }
                files.push(path);
            }
        }
        files
    }
}

impl ContextPlanner for DeterministicPlanner {
    fn plan(&self, task: &str) -> Result<Vec<PlanHint>, anyhow::Error> {
        let mut file_hints = Vec::new();
        let mut other_hints = Vec::new();
        let task_words = Self::task_words(task);

        // 1. Likely-required files: lexical overlap between the task and the
        // relative file path. Uses the directory listing only — no file
        // content is read here. The candidate text contains both the
        // separator-split path words ("data md" from "data.md") and the full
        // filename as one token ("data.md"), so tasks that reference either
        // form score, while pure-extension words like "md" (stopword-length)
        // never dominate the match.
        for path in self.candidate_files() {
            let relative = path
                .strip_prefix(&self.project_root)
                .unwrap_or(path.as_path())
                .display()
                .to_string();
            let mut lexical = relative.replace(['/', '\\', '.', '-', '_'], " ");
            if let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) {
                if !name.is_empty() {
                    lexical.push(' ');
                    lexical.push_str(&name);
                }
            }
            let score = Self::overlap(&task_words, &lexical);
            if score > 0.0 {
                file_hints.push(PlanHint {
                    kind: PlanHintKind::RequiredFile,
                    target: relative,
                    score,
                    reason: format!("lexical overlap with task ({score:.2})"),
                });
            }
        }
        file_hints.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.target.cmp(&b.target))
        });
        file_hints.truncate(MAX_FILE_HINTS);

        // 2. Relevant skills: referenced skill names that lexically overlap.
        if let Ok(referenced) =
            crate::skills::ProjectSkillReferences::new(self.project_root.clone())
                .resolve(&crate::skills::GlobalSkillRegistry::discover()?)
        {
            for skill in referenced {
                let score = Self::overlap(
                    &task_words,
                    &format!("{} {}", skill.name, skill.description),
                );
                if score > 0.0 {
                    other_hints.push(PlanHint {
                        kind: PlanHintKind::RelevantSkill,
                        target: skill.name.clone(),
                        score,
                        reason: "referenced skill matches task lexically".to_string(),
                    });
                }
            }
        }

        // 3. Relevant memories: search the existing memory store (the
        // project-scoped `.agent/memory.json`), reusing — not duplicating —
        // the MCP memory store.
        let memory = crate::mcp::MemoryMcp::new(self.project_root.clone())?;
        for entry in memory.search(task, None)? {
            let score = Self::overlap(&task_words, &entry.content);
            if score > 0.0 || !task_words.is_empty() {
                other_hints.push(PlanHint {
                    kind: PlanHintKind::RelevantMemory,
                    target: entry.id.clone(),
                    score: score.max(0.1),
                    reason: "memory matches task lexically".to_string(),
                });
            }
        }

        // 4. Offloaded context: items whose ids or content suggest relevance.
        let offloads = crate::context::offload::OffloadStore::new(&self.project_root)?;
        for id in offloads.list_ids()? {
            if let Some(item) = offloads.get(&id)? {
                let score = Self::overlap(&task_words, &item.content);
                if score > 0.0 {
                    other_hints.push(PlanHint {
                        kind: PlanHintKind::OffloadedContext,
                        target: id.clone(),
                        score,
                        reason: "offloaded item may be worth restoring".to_string(),
                    });
                }
            }
        }
        other_hints.truncate(MAX_MEMORY_HINTS * 3);

        let mut hints = other_hints;
        hints.extend(file_hints);
        Ok(hints)
    }
}

/// Informational: maps hint kinds for the MCP surface.
pub fn hint_kind_label(kind: PlanHintKind) -> &'static str {
    match kind {
        PlanHintKind::RequiredFile => "required_file",
        PlanHintKind::RelevantSkill => "relevant_skill",
        PlanHintKind::RelevantMemory => "relevant_memory",
        PlanHintKind::OffloadedContext => "offloaded_context",
        PlanHintKind::LikelyToolOutput => "likely_tool_output",
    }
}

/// Convenience for tests and tooling: the source used for file hints.
pub fn file_hint_source() -> ContextSource {
    ContextSource::File
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn project() -> (tempfile::TempDir, DeterministicPlanner) {
        let dir = tempfile::tempdir().unwrap();
        let planner = DeterministicPlanner::new(dir.path().to_path_buf());
        (dir, planner)
    }

    #[test]
    fn finds_lexically_relevant_files() {
        let (dir, planner) = project();
        fs::write(dir.path().join("auth.rs"), "fn login() {}").unwrap();
        fs::write(dir.path().join("random_notes.txt"), "nothing").unwrap();
        let hints = planner.plan("fix the auth login bug").unwrap();
        let file_targets: Vec<&str> = hints
            .iter()
            .filter(|h| h.kind == PlanHintKind::RequiredFile)
            .map(|h| h.target.as_str())
            .collect();
        assert!(
            file_targets.iter().any(|t| t.contains("auth.rs")),
            "auth.rs should be hinted: {file_targets:?}"
        );
    }

    #[test]
    fn hints_skip_hidden_and_state_dirs() {
        let (dir, planner) = project();
        fs::create_dir_all(dir.path().join(".git")).unwrap();
        fs::write(dir.path().join(".git").join("config"), "git stuff").unwrap();
        fs::write(dir.path().join(".env"), "secret").unwrap();
        let hints = planner.plan("find the config secret").unwrap();
        let targets: Vec<&str> = hints.iter().map(|h| h.target.as_str()).collect();
        assert!(!targets.iter().any(|t| t.contains(".git")), "{targets:?}");
        assert!(!targets.iter().any(|t| t.contains(".env")), "{targets:?}");
    }

    #[test]
    fn empty_task_yields_file_hints_but_no_overlap_scores() {
        let (dir, planner) = project();
        fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
        let hints = planner.plan("").unwrap();
        // With an empty task no file can lexically overlap.
        assert!(!hints.iter().any(|h| h.kind == PlanHintKind::RequiredFile));
    }

    #[test]
    fn planner_does_not_read_file_contents() {
        let (dir, planner) = project();
        fs::write(dir.path().join("data.md"), "irrelevant content words").unwrap();
        // The task matches the filename, not content; content never leaks
        // into hints because the planner only reads directory listings.
        let hints = planner.plan("read data.md").unwrap();
        let data_hints: Vec<&PlanHint> = hints
            .iter()
            .filter(|h| h.target.contains("data.md"))
            .collect();
        assert!(!data_hints.is_empty());
        for hint in data_hints {
            assert!(!hint.reason.contains("irrelevant content"));
        }
    }

    #[test]
    fn walk_is_bounded() {
        let (dir, planner) = project();
        // Create a deep tree larger than the walk bound; planning must still
        // terminate quickly and succeed.
        for i in 0..50 {
            let sub = dir.path().join(format!("d{i}"));
            fs::create_dir_all(&sub).unwrap();
            for j in 0..50 {
                fs::write(sub.join(format!("f{j}.txt")), "x").unwrap();
            }
        }
        let start = std::time::Instant::now();
        let result = planner.plan("find file");
        assert!(result.is_ok());
        assert!(start.elapsed().as_secs() < 10);
    }

    #[test]
    fn offloaded_items_can_be_hinted() {
        let (dir, planner) = project();
        let store = crate::context::offload::OffloadStore::new(dir.path()).unwrap();
        let mut item = crate::context::item::ContextItem::new(
            "off-auth",
            ContextSource::Tool,
            "auth login details",
            5,
        );
        item.state = crate::context::item::ContextState::Offloaded;
        let record = crate::context::offload::OffloadRecord {
            item,
            offloaded_at: chrono::Utc::now().to_rfc3339(),
            reason: None,
        };
        store.put(&record).unwrap();
        let hints = planner.plan("fix auth login").unwrap();
        assert!(
            hints
                .iter()
                .any(|h| h.kind == PlanHintKind::OffloadedContext && h.target == "off-auth"),
            "{hints:?}"
        );
    }
}
