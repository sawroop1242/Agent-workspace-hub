//! The `awh` command-line surface. Each module is a thin adapter over
//! the canonical application services — the CLI owns argument parsing,
//! output formatting, and exit codes only.

pub mod fs_edit;
