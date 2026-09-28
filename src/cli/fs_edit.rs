//! AWE-014: the `awh fs` editing/recovery command family — a thin CLI
//! adapter over the canonical `EditService` (and the snapshot store's
//! provenance read boundary).
//!
//! The CLI owns argument parsing, workspace resolution, output
//! formatting, and exit codes ONLY. Every mutation constructs the
//! canonical [`EditTransaction`] vocabulary and flows through the
//! authorized service surface (`*_as` with an explicit workspace-bound
//! operator principal — the same trusted-operator contract the MCP
//! global session uses), so policy deny rules, durable snapshot capture,
//! provenance, and correlated audit all apply unchanged. No CLI-local
//! editor, parser, snapshot store, rollback engine, or audit store.
//!
//! Exit codes (the minimum stable set, §23):
//!
//! ```text
//! 0  success
//! 1  general service failure (also anyhow's default)
//! 2  usage/parse failure (clap's default)
//! 3  authorization failure
//! 4  conflict (stale state, context, rollback target changed)
//! 5  recovery failure (snapshot/provenance/verification)
//! ```

use crate::services::authorization::AuthorizingPrincipal;
use crate::services::edit::{EditOperation, EditService, EditTransaction, ExpectedState};
use crate::services::init::load_workspace_manifest;
use clap::Subcommand;
use serde_json::json;
use std::path::Path;

/// The `awh fs` subcommand tree (editing/recovery family only — the
/// basic `fs read/write/stat/search/hash` commands are a separate
/// milestone and are NOT claimed here).
#[derive(Debug, Subcommand)]
pub enum FsCommand {
    /// Replace one exact text occurrence in a workspace file.
    Replace {
        /// Workspace-relative target path.
        path: String,
        /// Exact text to match (byte-for-byte).
        old: String,
        /// Replacement text.
        new: String,
        /// 1-based occurrence to replace; without it the match must be
        /// unique in the file.
        #[arg(long)]
        occurrence: Option<usize>,
        /// SHA-256 the file must have before the edit (stale guard).
        #[arg(long)]
        expected_hash: Option<String>,
        /// Byte size the file must have before the edit.
        #[arg(long)]
        expected_size: Option<u64>,
        /// Line count the file must have before the edit.
        #[arg(long)]
        expected_lines: Option<usize>,
        /// Text that must still occur exactly once at the edit location.
        #[arg(long)]
        expected_context: Option<String>,
        /// Emit a bounded machine-readable JSON result.
        #[arg(long)]
        json: bool,
    },
    /// Insert text at a line boundary (0 = beginning, N = before logical
    /// line N, N+1 = end).
    Insert {
        path: String,
        line: usize,
        content: String,
        #[arg(long)]
        expected_hash: Option<String>,
        #[arg(long)]
        expected_size: Option<u64>,
        #[arg(long)]
        expected_lines: Option<usize>,
        #[arg(long)]
        expected_context: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Delete an inclusive 1-based line range.
    DeleteRange {
        path: String,
        #[arg(long = "from")]
        start_line: usize,
        #[arg(long = "to")]
        end_line: usize,
        #[arg(long)]
        expected_hash: Option<String>,
        #[arg(long)]
        expected_size: Option<u64>,
        #[arg(long)]
        expected_lines: Option<usize>,
        #[arg(long)]
        expected_context: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Apply a multi-operation patch from a JSON file whose `operations`
    /// array uses the exact same object shape as the `filesystem.patch`
    /// MCP tool (lossless conversion, §11).
    Patch {
        /// Path to a JSON file containing {"operations": [...]} in the
        /// canonical patch schema.
        file: String,
        #[arg(long)]
        expected_hash: Option<String>,
        #[arg(long)]
        expected_size: Option<u64>,
        #[arg(long)]
        expected_lines: Option<usize>,
        #[arg(long)]
        expected_context: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Apply a unified diff from a file.
    ApplyDiff {
        /// Path to a file containing the unified diff (paths in diff
        /// headers stay workspace-relative).
        file: String,
        #[arg(long)]
        json: bool,
    },
    /// Verify the durable recovery chain for one edit id, read-only.
    Verify {
        /// The edit id returned by a prior edit.
        edit_id: String,
        #[arg(long)]
        json: bool,
    },
    /// List recent edits from the canonical provenance records.
    History {
        /// Maximum number of entries (bounded; default 20).
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Roll back one completed edit by its exact id.
    Rollback {
        edit_id: String,
        #[arg(long)]
        json: bool,
    },
}

/// The CLI-facing error vocabulary, mapped from canonical failures to
/// the stable exit-code categories.
enum FsError {
    Usage(String),
    Workspace(String),
    Authorization(String),
    Conflict(String),
    Recovery(String),
    Service(String),
}

impl FsError {
    fn exit_code(&self) -> i32 {
        match self {
            FsError::Usage(_) | FsError::Workspace(_) => 2,
            FsError::Authorization(_) => 3,
            FsError::Conflict(_) => 4,
            FsError::Recovery(_) => 5,
            FsError::Service(_) => 1,
        }
    }

    fn print(&self) {
        let (kind, message) = match self {
            FsError::Usage(message) => ("usage", message),
            FsError::Workspace(message) => ("workspace", message),
            FsError::Authorization(message) => ("authorization", message),
            FsError::Conflict(message) => ("conflict", message),
            FsError::Recovery(message) => ("recovery", message),
            FsError::Service(message) => ("service", message),
        };
        // Human diagnostics go to stderr; stdout stays clean for data.
        eprintln!("awh fs: {kind} error: {message}");
    }
}

/// Resolves the workspace root and the operator principal bound to it.
/// The CLI is the trusted in-process operator surface — the principal is
/// the *documented operator context* (the same contract the MCP global
/// session uses), never a fabricated agent identity. An uninitialized
/// workspace fails closed: without the canonical manifest there is no
/// workspace identity to bind authority or audit to.
fn workspace_principal(root: &Path) -> Result<AuthorizingPrincipal, FsError> {
    let manifest =
        load_workspace_manifest(root).map_err(|error| FsError::Workspace(format!("{error:#}")))?;
    Ok(AuthorizingPrincipal::operator(
        manifest.workspace_id.as_str(),
    ))
}

/// Builds the canonical expected-state precondition from CLI options, or
/// `None` when the caller supplied no expected-state flags — the model's
/// cardinality contract (expected.len() == operations.len() when
/// non-empty) forbids pushing an empty entry alongside real operations.
fn expected_state(
    hash: Option<String>,
    size: Option<u64>,
    lines: Option<usize>,
    context: Option<String>,
) -> Option<ExpectedState> {
    if hash.is_none() && size.is_none() && lines.is_none() && context.is_none() {
        return None;
    }
    Some(ExpectedState {
        hash,
        size,
        line_count: lines,
        context,
    })
}

/// Attaches the CLI expected-state options to the transaction when any
/// were supplied.
fn apply_expected(transaction: &mut EditTransaction, expected: Option<ExpectedState>) {
    if let Some(expected) = expected {
        transaction.expected.push(expected);
    }
}

/// A successful edit outcome, formatted per the output mode.
fn print_edit_result(json: bool, edit_id: &str, path: &str, operation: &str) {
    if json {
        // Bounded stable fields only (§22): never file contents, never
        // raw payloads.
        println!(
            "{}",
            json!({
                "command": format!("fs {operation}"),
                "status": "committed",
                "edit_id": edit_id,
                "path": path,
            })
        );
    } else {
        println!("edit {edit_id} committed ({operation}: {path})");
    }
}

/// Runs one `awh fs` subcommand against `root`, returning the process
/// exit code (0 on success).
pub fn run(command: FsCommand, root: &Path) -> i32 {
    match run_inner(command, root) {
        Ok(()) => 0,
        Err(error) => {
            error.print();
            error.exit_code()
        }
    }
}

fn run_inner(command: FsCommand, root: &Path) -> Result<(), FsError> {
    let principal = workspace_principal(root)?;
    let service = EditService::new(root.to_path_buf());

    match command {
        FsCommand::Replace {
            path,
            old,
            new,
            occurrence,
            expected_hash,
            expected_size,
            expected_lines,
            expected_context,
            json,
        } => {
            let mut transaction = EditTransaction::single(EditOperation::Replace {
                path: path.clone(),
                old,
                new,
                occurrence,
            });
            apply_expected(
                &mut transaction,
                expected_state(
                    expected_hash,
                    expected_size,
                    expected_lines,
                    expected_context,
                ),
            );
            let result = service
                .replace_as(&principal, transaction)
                .map_err(map_edit_error)?;
            print_edit_result(json, result.id.to_string().as_str(), &path, "replace");
            Ok(())
        }
        FsCommand::Insert {
            path,
            line,
            content,
            expected_hash,
            expected_size,
            expected_lines,
            expected_context,
            json,
        } => {
            let mut transaction = EditTransaction::single(EditOperation::Insert {
                path: path.clone(),
                line,
                content,
            });
            apply_expected(
                &mut transaction,
                expected_state(
                    expected_hash,
                    expected_size,
                    expected_lines,
                    expected_context,
                ),
            );
            let result = service
                .insert_as(&principal, transaction)
                .map_err(map_edit_error)?;
            print_edit_result(json, result.id.to_string().as_str(), &path, "insert");
            Ok(())
        }
        FsCommand::DeleteRange {
            path,
            start_line,
            end_line,
            expected_hash,
            expected_size,
            expected_lines,
            expected_context,
            json,
        } => {
            let mut transaction = EditTransaction::single(EditOperation::DeleteRange {
                path: path.clone(),
                start_line,
                end_line,
            });
            apply_expected(
                &mut transaction,
                expected_state(
                    expected_hash,
                    expected_size,
                    expected_lines,
                    expected_context,
                ),
            );
            let result = service
                .delete_range_as(&principal, transaction)
                .map_err(map_edit_error)?;
            print_edit_result(json, result.id.to_string().as_str(), &path, "delete-range");
            Ok(())
        }
        FsCommand::Patch {
            file,
            expected_hash,
            expected_size,
            expected_lines,
            expected_context,
            json,
        } => {
            let operations = parse_patch_file(&file)?;
            let mut transaction = EditTransaction::new(operations);
            apply_expected(
                &mut transaction,
                expected_state(
                    expected_hash,
                    expected_size,
                    expected_lines,
                    expected_context,
                ),
            );
            let result = service
                .patch_as(&principal, transaction)
                .map_err(map_edit_error)?;
            print_edit_result(
                json,
                result.transaction_id.to_string().as_str(),
                "(multi-file)",
                "patch",
            );
            Ok(())
        }
        FsCommand::ApplyDiff { file, json } => {
            let diff = std::fs::read_to_string(&file).map_err(|error| {
                FsError::Usage(format!("cannot read diff file {file}: {error}"))
            })?;
            let transaction = EditTransaction::single(EditOperation::ApplyDiff { diff });
            let result = service
                .patch_as(&principal, transaction)
                .map_err(map_edit_error)?;
            print_edit_result(
                json,
                result.transaction_id.to_string().as_str(),
                "(diff)",
                "apply-diff",
            );
            Ok(())
        }
        FsCommand::Verify { edit_id, json } => {
            let store = crate::services::snapshot::SnapshotStore::new(root.to_path_buf());
            let provenance = store
                .provenance(&edit_id)
                .map_err(|error| FsError::Recovery(error.to_string()))?;
            // Read-only integrity check of the whole recovery chain:
            // the manifest load verifies schema, ids, sequence, and every
            // content blob's length + SHA-256 without mutating anything.
            store
                .load(&provenance.snapshot_id)
                .map_err(|error| FsError::Recovery(error.to_string()))?;
            if json {
                println!(
                    "{}",
                    json!({
                        "command": "fs verify",
                        "status": "verified",
                        "edit_id": edit_id,
                        "snapshot_id": provenance.snapshot_id.to_string().as_str(),
                        "entries": provenance.paths.len(),
                    })
                );
            } else {
                println!(
                    "edit {edit_id} recovery material verified ({} path(s))",
                    provenance.paths.len()
                );
            }
            Ok(())
        }
        FsCommand::History { limit, json } => {
            let store = crate::services::snapshot::SnapshotStore::new(root.to_path_buf());
            // Bounded read over the canonical provenance records — no
            // CLI-local store. A corrupt record fails closed rather than
            // being silently skipped or fabricated.
            let entries = store
                .list_provenance(limit)
                .map_err(|error| FsError::Recovery(error.to_string()))?;
            if json {
                let items: Vec<_> = entries
                    .iter()
                    .map(|record| {
                        let status = match &record.outcome {
                            crate::services::snapshot::ProvenanceOutcome::Committed => "committed",
                            crate::services::snapshot::ProvenanceOutcome::RolledBack { .. } => {
                                "rolled_back"
                            }
                            crate::services::snapshot::ProvenanceOutcome::Failed { .. } => "failed",
                        };
                        json!({
                            "edit_id": record.edit_id,
                            "status": status,
                            "paths": record.paths,
                            "agent_id": record.agent_id,
                            "workspace_id": record.workspace_id,
                            "created_at": record.created_at,
                        })
                    })
                    .collect();
                println!("{}", json!({ "command": "fs history", "edits": items }));
            } else {
                for record in entries {
                    let status = match &record.outcome {
                        crate::services::snapshot::ProvenanceOutcome::Committed => "committed",
                        crate::services::snapshot::ProvenanceOutcome::RolledBack { .. } => {
                            "rolled_back"
                        }
                        crate::services::snapshot::ProvenanceOutcome::Failed { .. } => "failed",
                    };
                    println!("{}  {}  {}", record.created_at, status, record.edit_id);
                }
            }
            Ok(())
        }
        FsCommand::Rollback { edit_id, json } => {
            let status = service
                .rollback_edit(&principal, &edit_id)
                .map_err(map_edit_error)?;
            let (label, conflict) = match &status {
                crate::services::edit::EditRollbackStatus::Restored => ("restored", false),
                crate::services::edit::EditRollbackStatus::AlreadyRolledBack => {
                    ("already_rolled_back", false)
                }
                crate::services::edit::EditRollbackStatus::Conflict { .. } => ("conflict", true),
                crate::services::edit::EditRollbackStatus::Failed { .. } => ("failed", true),
            };
            if json {
                println!(
                    "{}",
                    json!({
                        "command": "fs rollback",
                        "status": label,
                        "edit_id": edit_id,
                    })
                );
            } else {
                println!("edit {edit_id}: {label}");
            }
            // §30: a rollback that could not restore (conflict, or the
            // restoration itself failed) is a non-zero outcome — the
            // canonical status is reported honestly on stdout first, the
            // diagnostic goes to stderr, and the newer live state is
            // preserved untouched. The idempotent already-rolled-back
            // result IS a success (§59 "repeated rollback: canonical
            // behavior").
            if conflict {
                eprintln!(
                    "awh fs: rollback did not restore; the target no longer matches the edit's produced state"
                );
                return Err(FsError::Conflict(label.to_owned()));
            }
            Ok(())
        }
    }
}

/// Parses the patch operations JSON file. The array uses the exact same
/// object shape as the `filesystem.patch` MCP tool, so the conversion
/// is lossless with respect to the canonical contract (§11).
fn parse_patch_file(file: &str) -> Result<Vec<EditOperation>, FsError> {
    let text = std::fs::read_to_string(file)
        .map_err(|error| FsError::Usage(format!("cannot read patch file {file}: {error}")))?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|error| FsError::Usage(format!("patch file is not valid JSON: {error}")))?;
    let array = value
        .get("operations")
        .and_then(|operations| operations.as_array())
        .ok_or_else(|| FsError::Usage("patch file must contain an \"operations\" array".into()))?;
    let mut operations = Vec::with_capacity(array.len());
    for (index, op) in array.iter().enumerate() {
        let path = op
            .get("path")
            .and_then(|p| p.as_str())
            .ok_or_else(|| FsError::Usage(format!("operation {index}: missing 'path'")))?;
        if let (Some(old), Some(new)) = (
            op.get("old").and_then(|v| v.as_str()),
            op.get("new").and_then(|v| v.as_str()),
        ) {
            operations.push(EditOperation::Replace {
                path: path.to_owned(),
                old: old.to_owned(),
                new: new.to_owned(),
                occurrence: op
                    .get("occurrence")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as usize),
            });
        } else if let (Some(line), Some(content)) = (
            op.get("line").and_then(|v| v.as_u64()),
            op.get("content").and_then(|v| v.as_str()),
        ) {
            operations.push(EditOperation::Insert {
                path: path.to_owned(),
                line: line as usize,
                content: content.to_owned(),
            });
        } else if let (Some(start_line), Some(end_line)) = (
            op.get("start_line").and_then(|v| v.as_u64()),
            op.get("end_line").and_then(|v| v.as_u64()),
        ) {
            operations.push(EditOperation::DeleteRange {
                path: path.to_owned(),
                start_line: start_line as usize,
                end_line: end_line as usize,
            });
        } else {
            return Err(FsError::Usage(format!(
                "operation {index}: must be a replace (old/new), insert (line/content), or delete-range (start_line/end_line) object"
            )));
        }
    }
    if operations.is_empty() {
        return Err(FsError::Usage("patch file contains no operations".into()));
    }
    Ok(operations)
}

/// Maps a canonical edit error to the stable CLI exit categories (§25):
/// authorization → 3, stale-state/context → 4, recovery corruption → 5,
/// everything else → 1. Messages are the canonical, already-sanitized
/// `Display` text — no backtraces, no host paths, no file contents.
fn map_edit_error(error: crate::services::edit::EditError) -> FsError {
    use crate::services::edit::EditError;
    match &error {
        EditError::AuthorizationDenied { .. } => FsError::Authorization(error.to_string()),
        EditError::ExpectedStateConflict(_) | EditError::ContextConflict { .. } => {
            FsError::Conflict(error.to_string())
        }
        EditError::HunkContextMismatch { .. } => FsError::Conflict(error.to_string()),
        EditError::PatchPreparationFailure { .. } => FsError::Recovery(error.to_string()),
        EditError::FileNotFound { .. } => FsError::Usage(error.to_string()),
        _ => FsError::Service(error.to_string()),
    }
}
