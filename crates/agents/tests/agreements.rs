#![cfg_attr(
    test,
    allow(
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used,
        clippy::expect_used,
        reason = "Test helpers use concise assertions; production paths remain strict."
    )
)]

use std::path::Path;
use std::process::Command;

use ovenspring_agents::prompt::read_agreements;
use ovenspring_core::config::Role;

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

#[test]
fn agreements_come_from_the_base_branch_not_the_working_tree() {
    let dir = tempfile::tempdir().unwrap();
    let origin = dir.path().join("origin.git");
    let work = dir.path().join("work");
    git(
        dir.path(),
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
        dir.path(),
        &["init", "-q", "-b", "main", work.to_str().unwrap()],
    );
    std::fs::create_dir_all(work.join(".ovenspring/agreements")).unwrap();
    std::fs::write(
        work.join(".ovenspring/agreements/common.md"),
        "- Name the precedent a design follows.\n",
    )
    .unwrap();
    git(&work, &["add", "."]);
    git(
        &work,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "commit",
            "-q",
            "-m",
            "agreements",
        ],
    );
    git(
        &work,
        &["remote", "add", "origin", origin.to_str().unwrap()],
    );
    git(&work, &["push", "-q", "-u", "origin", "main"]);

    std::fs::write(
        work.join(".ovenspring/agreements/common.md"),
        "- An uncommitted edit.\n",
    )
    .unwrap();
    std::fs::write(
        work.join(".ovenspring/agreements/executor.md"),
        "- An unmerged role file.\n",
    )
    .unwrap();

    let agreements = read_agreements(&work, "main", Role::Executor).unwrap();
    assert_eq!(
        agreements.common.as_deref(),
        Some("- Name the precedent a design follows.\n")
    );
    assert_eq!(agreements.role, None);
    assert!(
        read_agreements(&work, "release", Role::Executor).is_err(),
        "a base branch that cannot be read is an error, not an empty agreement"
    );
}
