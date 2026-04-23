//! Git integration: acquire input and emit output as Git repositories.
//!
//! Input side: clone remote URLs into a temp directory, or use local paths
//! as-is. Record the source commit SHA in the migration report for
//! traceability.
//!
//! Output side: initialize a Git repository in the output directory, commit
//! the generated files (single commit or logical incremental commits), and
//! optionally push to a remote.

pub mod acquire;
pub mod emit;

use std::path::PathBuf;

/// Where the Mule source lives.
#[derive(Debug, Clone)]
pub enum InputSource {
    /// Use this local path directly; do not clone.
    LocalPath(PathBuf),
    /// Clone this URL into a temp directory.
    RemoteUrl { url: String, branch: Option<String> },
}

/// How to package the output.
#[derive(Debug, Clone)]
pub struct GitEmitOptions {
    /// If false, skip `git init` entirely (leave a plain directory).
    pub init: bool,
    /// Write a single commit vs. logical incremental commits.
    pub strategy: CommitStrategy,
    /// If set, add this as origin and push after committing.
    pub push_to: Option<String>,
    /// Commit author.
    pub author: CommitAuthor,
    /// Default branch name.
    pub default_branch: String,
}

impl Default for GitEmitOptions {
    fn default() -> Self {
        Self {
            init: true,
            strategy: CommitStrategy::Single,
            push_to: None,
            author: CommitAuthor::default(),
            default_branch: "main".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum CommitStrategy {
    /// One commit containing everything.
    Single,
    /// Multiple commits in a sensible order: scaffold, routes, docs, CI, etc.
    Incremental,
}

#[derive(Debug, Clone)]
pub struct CommitAuthor {
    pub name: String,
    pub email: String,
}

impl Default for CommitAuthor {
    fn default() -> Self {
        Self {
            name: "MuleForge".into(),
            email: "noreply@muleforge.dev".into(),
        }
    }
}

/// Acquire result: a working directory plus metadata for the report.
pub struct AcquiredInput {
    pub working_dir: PathBuf,
    pub source_description: String,
    pub source_commit: Option<String>,
    /// If true, the working dir is a temp dir that should be removed on drop.
    pub is_temporary: bool,
}
