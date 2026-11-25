//! @lane: integration
//! Area: `ovenspring app run` (an agent's command in its worktree -> the issue's app on its port ->
//! the command's result).
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

mod support;

use ovenspring_core::step::Step;
use std::path::Path;

use ovenspring_core::item::Status;
use ovenspring_store::Store;
use support::Env;

fn app() -> String {
    let start = support::serve("\"$OVENSPRING_DATA_DIR\"");
    format!(
        "\n[app]\nstart = {}\nready = \"http://127.0.0.1:$PORT/\"\n",
        serde_json::to_string(&start).unwrap()
    )
}

/// Fetches `/` from the app on `$PORT` and prints the status.
const PROBE: &str = "import os, urllib.request; print(urllib.request.urlopen('http://127.0.0.1:' + os.environ['PORT'] + '/').status)";

fn file_item(store: &Store, name: &str, issue: &str) {
    let mut item = support::item(store, name, Step::Implement, Some(issue));
    item.front.status = Status::Doing;
    store.file_item(name, &item).unwrap();
}

fn in_worktree(
    env: &Env,
    worktree: &Path,
    launch: Option<&str>,
    args: &[&str],
) -> std::process::Output {
    let mut cmd = env.ovenspring(args);
    cmd.current_dir(worktree.join("src"));
    match launch {
        Some(l) => cmd.env(ovenspring_store::LAUNCH_ENV, l),
        None => cmd.env_remove(ovenspring_store::LAUNCH_ENV),
    };
    cmd.output().unwrap()
}

// AC: "Each issue runs on its own branch and worktree" (AC-016), with the app an agent checks its
// work against running from that worktree on the issue's own port, as the owner set it in Settings
// (AC-059).
// Behavior: the owner's `[app]` is saved in their working tree only -> agent runs
// `ovenspring app run -- <cmd>` in its worktree -> the app starts on the issue's port -> the command
// reaches it -> the command's exit status is returned
#[test]
fn an_agent_runs_its_check_against_its_own_issues_app() {
    let env = Env::new();
    env.configure("");
    let config = env.repo.join(".ovenspring/config.toml");
    let text = std::fs::read_to_string(&config)
        .unwrap()
        .replace("[app]\nnothing_to_run = true\n", "");
    std::fs::write(&config, format!("{text}\n{}", app())).unwrap();
    let state = env.state();
    let store = Store::open(&state).unwrap();
    let worktree =
        ovenspring_daemon::worktree::ensure(&env.repo, &store, "ACME-1", "main").unwrap();
    std::fs::create_dir_all(worktree.join("src")).unwrap();
    file_item(&store, "20261121-120000-implement-ACME-1", "ACME-1");
    file_item(&store, "20261121-120000-implement-ACME-2", "ACME-2");

    let out = in_worktree(
        &env,
        &worktree,
        Some("20261121-120000-implement-ACME-1"),
        &["app", "run", "--", "python3", "-c", PROBE],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "200");

    let out = in_worktree(
        &env,
        &worktree,
        Some("20261121-120000-implement-ACME-1"),
        &["app", "run", "--", "sh", "-c", "exit 7"],
    );
    assert_eq!(out.status.code(), Some(7), "the command's own exit status");
}

// Behavior: a command from outside a launch, or from another issue's launch -> refused -> no app
#[test]
fn only_a_launch_of_the_issue_runs_its_app() {
    let env = Env::new();
    env.configure(&app());
    let state = env.state();
    let store = Store::open(&state).unwrap();
    let worktree =
        ovenspring_daemon::worktree::ensure(&env.repo, &store, "ACME-1", "main").unwrap();
    std::fs::create_dir_all(worktree.join("src")).unwrap();
    file_item(&store, "20261121-120000-implement-ACME-2", "ACME-2");

    for (launch, says) in [
        (None, "is for factory launches"),
        (
            Some("20261121-120000-implement-ACME-2"),
            "is not an item of ACME-1",
        ),
        (
            Some("20261121-120000-implement-ACME-9"),
            "not an active item",
        ),
    ] {
        let out = in_worktree(&env, &worktree, launch, &["app", "run", "--", "true"]);
        assert!(!out.status.success());
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains(says), "{err}");
    }
    assert!(
        !state.join("apps/ACME-1/app.pid").exists(),
        "no app was started"
    );
}

// AC: "`ovenspring stop`: Stops the repository's daemon after its current pass, ending the agents
//     it runs and the issues' apps; their items run again on the next start" (distribution design).
// Behavior: an agent's `ovenspring app run` leaves ACME-1's app running -> the daemon stops -> the
//     app no longer runs and its pid file is gone.
// @lane: integration
// @dependency: daemon, worktree app control, store
#[test]
fn the_daemons_stop_ends_the_apps_it_started() {
    let env = Env::new();
    env.configure(&app());
    let store = Store::open(env.state()).unwrap();
    let worktree =
        ovenspring_daemon::worktree::ensure(&env.repo, &store, "ACME-1", "main").unwrap();
    std::fs::create_dir_all(worktree.join("src")).unwrap();
    file_item(&store, "20261121-120000-implement-ACME-1", "ACME-1");
    let out = in_worktree(
        &env,
        &worktree,
        Some("20261121-120000-implement-ACME-1"),
        &["app", "run", "--", "true"],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let pid_file = env.state().join("apps/ACME-1/app.pid");
    let pid: u32 = std::fs::read_to_string(&pid_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(ovenspring_daemon::app::alive(pid));
    let mut d = support::daemon(
        &env,
        support::services(
            std::sync::Arc::new(support::scripts::always_passing(
                &env,
                support::scripts::requiring_test(),
            )),
            std::sync::Arc::new(support::recording::RecordingChannel::default()),
            std::sync::Arc::new(support::recording::RecordingTracker::default()),
        ),
    );

    d.stop_launches();

    assert!(support::wait_until(15, || !ovenspring_daemon::app::alive(
        pid
    )));
    assert!(!pid_file.exists());
}
