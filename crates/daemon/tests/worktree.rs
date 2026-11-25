//! Worktrees under the state directory, against a real git repository.
#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, reason = "tests fail loudly")
)]

use std::path::Path;
use std::process::Command;

use ovenspring_daemon::worktree::{ensure, ensure_detached};
use ovenspring_store::Store;

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A repository with a bare `origin` holding one commit on `main`.
fn repo(dir: &Path) -> std::path::PathBuf {
    let origin = dir.join("origin.git");
    let repo = dir.join("repo");
    git(
        dir,
        &[
            "init",
            "-q",
            "--bare",
            "-b",
            "main",
            origin.to_str().unwrap(),
        ],
    );
    git(
        dir,
        &[
            "clone",
            "-q",
            origin.to_str().unwrap(),
            repo.to_str().unwrap(),
        ],
    );
    git(
        &repo,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "start",
        ],
    );
    git(&repo, &["push", "-q", "origin", "HEAD:main"]);
    repo
}

// Behavior: the state directory that held the worktrees is moved aside (or deleted) while git
//     still has them registered -> the next launch creates them again at the same paths.
// @lane: integration
#[test]
fn worktrees_of_a_state_directory_moved_aside_are_created_again() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    let state = dir.path().join("state");
    let store = Store::open(&state).unwrap();
    ensure_detached(&repo, &store, "base", "main").unwrap();
    ensure(&repo, &store, "GH-1", "main").unwrap();

    std::fs::rename(&state, dir.path().join("state.aside")).unwrap();

    let store = Store::open(&state).unwrap();
    let base = ensure_detached(&repo, &store, "base", "main").unwrap();
    let issue = ensure(&repo, &store, "GH-1", "main").unwrap();
    assert!(base.join(".git").exists() && issue.join(".git").exists());
}
