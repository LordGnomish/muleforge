//! Emit the output working tree as a Git repository.

use std::path::Path;

use crate::git::{CommitStrategy, GitEmitOptions};
use crate::{MuleForgeError, Result};

pub fn emit(output_dir: &Path, opts: &GitEmitOptions) -> Result<()> {
    if !opts.init {
        tracing::info!("skipping git init (--no-git)");
        return Ok(());
    }

    init_repo(output_dir, &opts.default_branch)?;

    match opts.strategy {
        CommitStrategy::Single => commit_single(output_dir, opts)?,
        CommitStrategy::Incremental => commit_incremental(output_dir, opts)?,
    }

    if let Some(remote) = &opts.push_to {
        add_remote_and_push(output_dir, remote, &opts.default_branch)?;
    }

    Ok(())
}

fn init_repo(dir: &Path, default_branch: &str) -> Result<()> {
    // Try modern -b flag (git ≥ 2.28); fall back for older installations.
    let ok = std::process::Command::new("git")
        .args(["init", "-b", default_branch])
        .current_dir(dir)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !ok {
        let status = std::process::Command::new("git")
            .arg("init")
            .current_dir(dir)
            .status()
            .map_err(|e| MuleForgeError::Git(format!("failed to run git init: {}", e)))?;
        if !status.success() {
            return Err(MuleForgeError::Git("git init failed".into()));
        }
    }

    Ok(())
}

fn git_add_all(dir: &Path) -> Result<()> {
    let status = std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(dir)
        .status()
        .map_err(|e| MuleForgeError::Git(format!("failed to run git add: {}", e)))?;

    if !status.success() {
        return Err(MuleForgeError::Git("git add failed".into()));
    }
    Ok(())
}

fn git_commit(dir: &Path, message: &str, opts: &GitEmitOptions) -> Result<bool> {
    let status = std::process::Command::new("git")
        .args(["commit", "-m", message])
        .env("GIT_AUTHOR_NAME", &opts.author.name)
        .env("GIT_AUTHOR_EMAIL", &opts.author.email)
        .env("GIT_COMMITTER_NAME", &opts.author.name)
        .env("GIT_COMMITTER_EMAIL", &opts.author.email)
        .current_dir(dir)
        .status()
        .map_err(|e| MuleForgeError::Git(format!("failed to run git commit: {}", e)))?;

    Ok(status.success())
}

fn has_staged_changes(dir: &Path) -> bool {
    // exit code 1 means there are staged changes; 0 means clean
    std::process::Command::new("git")
        .args(["diff", "--cached", "--quiet"])
        .current_dir(dir)
        .status()
        .map(|s| !s.success())
        .unwrap_or(false)
}

fn has_unstaged_files(dir: &Path) -> bool {
    std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(dir)
        .output()
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false)
}

fn commit_single(dir: &Path, opts: &GitEmitOptions) -> Result<()> {
    git_add_all(dir)?;
    git_commit(dir, "chore: initial migration via MuleForge", opts)?;
    Ok(())
}

fn commit_incremental(dir: &Path, opts: &GitEmitOptions) -> Result<()> {
    // Logical stages matching the pipeline output.  Paths that don't exist
    // are silently skipped so sparse migrations still produce clean histories.
    let stages: &[(&str, &[&str])] = &[
        (
            "chore: scaffold Quarkus project (MuleForge)",
            &["pom.xml", ".gitignore", "Dockerfile"],
        ),
        (
            "feat: add migrated Camel routes",
            &["src/main/java/generated/routes/"],
        ),
        (
            "feat: add transformation beans",
            &["src/main/java/generated/beans/"],
        ),
        (
            "feat: add application configuration",
            &["src/main/resources/"],
        ),
        ("test: add route smoke tests", &["src/test/"]),
        ("ops: add Kubernetes manifests", &["k8s/"]),
        ("ops: add API gateway config", &["kong/"]),
        ("ci: add GitHub Actions workflows", &[".github/"]),
        (
            "docs: add project documentation",
            &["docs/", "README.md", "CONTRIBUTING.md"],
        ),
        ("chore: add migration report", &["MIGRATION_REPORT.md"]),
    ];

    for (message, paths) in stages {
        for p in *paths {
            let full = dir.join(p);
            if full.exists() {
                let _ = std::process::Command::new("git")
                    .args(["add", p])
                    .current_dir(dir)
                    .status();
            }
        }
        if has_staged_changes(dir) {
            git_commit(dir, message, opts)?;
        }
    }

    // Catch any files not covered by the stage list above.
    if has_unstaged_files(dir) {
        git_add_all(dir)?;
        if has_staged_changes(dir) {
            git_commit(dir, "chore: remaining migrated files", opts)?;
        }
    }

    Ok(())
}

fn add_remote_and_push(dir: &Path, remote_url: &str, branch: &str) -> Result<()> {
    let status = std::process::Command::new("git")
        .args(["remote", "add", "origin", remote_url])
        .current_dir(dir)
        .status()
        .map_err(|e| MuleForgeError::Git(format!("failed to run git remote add: {}", e)))?;

    if !status.success() {
        return Err(MuleForgeError::Git("git remote add failed".into()));
    }

    let status = std::process::Command::new("git")
        .args(["push", "-u", "origin", branch])
        .current_dir(dir)
        .status()
        .map_err(|e| MuleForgeError::Git(format!("failed to run git push: {}", e)))?;

    if !status.success() {
        return Err(MuleForgeError::Git(
            "git push failed — check remote URL and credentials".into(),
        ));
    }

    Ok(())
}
