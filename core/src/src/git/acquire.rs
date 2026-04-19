//! Acquire a Mule source tree from a local path or remote Git URL.

use std::hash::{Hash, Hasher};
use std::path::Path;

use crate::git::{AcquiredInput, InputSource};
use crate::Result;

pub fn acquire(source: &InputSource) -> Result<AcquiredInput> {
    match source {
        InputSource::LocalPath(path) => acquire_local(path),
        InputSource::RemoteUrl { url, branch } => acquire_remote(url, branch.as_deref()),
    }
}

fn acquire_local(path: &Path) -> Result<AcquiredInput> {
    let source_commit = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(path)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string());

    Ok(AcquiredInput {
        working_dir: path.to_path_buf(),
        source_description: format!("local path: {}", path.display()),
        source_commit,
        is_temporary: false,
    })
}

fn acquire_remote(url: &str, branch: Option<&str>) -> Result<AcquiredInput> {
    // Derive a stable temp directory name from the URL so repeated runs reuse it.
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut hasher);
    let hash = hasher.finish();
    let dir_name = format!("muleforge-{:016x}", hash);
    let tmp_path = std::env::temp_dir().join(dir_name);

    let mut cmd = std::process::Command::new("git");
    cmd.arg("clone").arg("--depth=1");
    if let Some(b) = branch {
        cmd.arg("--branch").arg(b);
    }
    cmd.arg(url).arg(&tmp_path);

    let status = cmd
        .status()
        .map_err(|e| crate::MuleForgeError::Git(format!("failed to run git: {}", e)))?;

    if !status.success() {
        return Err(crate::MuleForgeError::Git(format!(
            "git clone failed for {}",
            url
        )));
    }

    let source_commit = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&tmp_path)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string());

    Ok(AcquiredInput {
        working_dir: tmp_path,
        source_description: format!("remote: {}", url),
        source_commit,
        is_temporary: true,
    })
}
