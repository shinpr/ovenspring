//! One git worktree and branch per issue, under the state directory.

use std::path::{Path, PathBuf};
use std::process::Command;

use ovenspring_store::Store;

#[derive(Debug, thiserror::Error)]
#[error("git {args}: {message}")]
pub struct GitError {
    pub args: String,
    pub message: String,
}

pub fn git(dir: &Path, args: &[&str]) -> Result<String, GitError> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| GitError {
            args: args.join(" "),
            message: e.to_string(),
        })?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        // A hook's standard output reaches git's standard output, so what explains a refusal can
        // be on either stream; git's own error closes standard error.
        let message = [&out.stdout, &out.stderr]
            .map(|stream| String::from_utf8_lossy(stream).trim().to_string())
            .into_iter()
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        Err(GitError {
            args: args.join(" "),
            message,
        })
    }
}

pub fn branch_for(issue: &str) -> String {
    format!("ovenspring/{issue}")
}

/// The issue's worktree, created from the base branch on first use.
pub fn ensure(repo: &Path, store: &Store, issue: &str, base: &str) -> Result<PathBuf, GitError> {
    let path = store.worktree(issue);
    if path.join(".git").exists() {
        return Ok(path);
    }
    prune(repo)?;
    let branch = branch_for(issue);
    let path_str = path.display().to_string();
    let branch_exists = git(
        repo,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
    )
    .is_ok();
    if branch_exists {
        git(repo, &["worktree", "add", &path_str, &branch])?;
    } else {
        let start = if git(
            repo,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/remotes/origin/{base}"),
            ],
        )
        .is_ok()
        {
            format!("origin/{base}")
        } else {
            base.to_string()
        };
        git(repo, &["worktree", "add", "-b", &branch, &path_str, &start])?;
    }
    Ok(path)
}

/// The detached checkout of the base branch that launches for no issue work in.
pub const BASE: &str = "base";

/// A detached checkout of `origin/<base>` under the state directory, refreshed on each call.
pub fn ensure_detached(
    repo: &Path,
    store: &Store,
    name: &str,
    base: &str,
) -> Result<PathBuf, GitError> {
    let path = store.worktree(name);
    git(repo, &["fetch", "-q", "origin", base])?;
    let start = format!("origin/{base}");
    if path.join(".git").exists() {
        git(&path, &["checkout", "-q", "--detach", &start])?;
    } else {
        prune(repo)?;
        git(
            repo,
            &[
                "worktree",
                "add",
                "-q",
                "--detach",
                &path.display().to_string(),
                &start,
            ],
        )?;
    }
    Ok(path)
}

/// Forgets worktrees whose directory is gone, such as those of a state directory that was moved
/// or deleted; git refuses to add a worktree at a path it still has registered.
fn prune(repo: &Path) -> Result<(), GitError> {
    git(repo, &["worktree", "prune"]).map(|_| ())
}
