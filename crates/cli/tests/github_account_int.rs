//! @lane: integration
//! Area: the GitHub account the factory's own Git commands act as: commit identity, the token a
//! push and its reads authenticate with, and the `origin` they may reach.
//!
//! GitHub's Git endpoint is `support::git_http`, which demands the test token. Owner Git settings
//! go into the repository's own `.git/config`, never the process environment, because tests in
//! this binary run concurrently. What the connection guard keeps from reaching another destination
//! is proven by the unit tests in `crates/daemon/src/worktree.rs`; the tests here prove the
//! production routes and how a refusal is classified.

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

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use ovenspring_core::config::Config;
use ovenspring_core::flow::Stage;
use ovenspring_integrations::github::BranchRules;
use serde_json::{Value, json};
use support::dashboard::Dashboard;
use support::git_http::{GitHttp, TEST_TOKEN_AUTHORIZATION};
use support::recording::{RecordingChannel, RecordingTracker};
use support::scripts::{always_passing, filed_steps, stage, straight};
use support::{Env, daemon, git, run_until, services};

const CONFIG: &str = r#"
[repo]
base_branch = "develop"
[review]
"#;

/// `FakeGitHub`'s account, in both roles of `%an <%ae> | %cn <%ce>`.
const ACCOUNT_IN_BOTH_ROLES: &str = "factory-bot <12345678+factory-bot@users.noreply.github.com> | \
     factory-bot <12345678+factory-bot@users.noreply.github.com>";

/// Both roles of a commit.
const IDENTITY: &str = "--format=%an <%ae> | %cn <%ce>";

/// `Authorization` with the owner's own login, `owner` / `owner-secret`.
const OWNER_AUTHORIZATION: &str = "Basic b3duZXI6b3duZXItc2VjcmV0";

fn marker_helper(env: &Env) -> PathBuf {
    env.dir.path().join("owner-helper")
}

/// The owner's Git settings for the fixture's endpoint, in the repository's own `.git/config`: a
/// credential helper that leaves `marker` behind and answers the owner's login, and an extra
/// header carrying that login. Both keys name the fixture's port, without which they would not
/// match it.
fn owners_settings(env: &Env, url: &str, marker: &Path) {
    let helper = marker_helper(env);
    std::fs::write(
        &helper,
        format!(
            "#!/bin/sh\necho \"$1\" >> '{}'\ntest \"$1\" = get && echo username=owner && echo password=owner-secret\nexit 0\n",
            marker.display()
        ),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&helper).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
    std::fs::set_permissions(&helper, permissions).unwrap();
    let endpoint = url.rsplit_once('/').unwrap().0;
    git(&env.repo, &["remote", "set-url", "origin", url]);
    git(
        &env.repo,
        &[
            "config",
            &format!("credential.{endpoint}.helper"),
            helper.to_str().unwrap(),
        ],
    );
    git(
        &env.repo,
        &[
            "config",
            &format!("http.{url}.extraHeader"),
            &format!("Authorization: {OWNER_AUTHORIZATION}"),
        ],
    );
}

/// A daemon whose GitHub connection was built for `url`, with a check that always passes.
fn daemon_connected_to(env: &Env, url: &str) -> ovenspring_daemon::Daemon {
    let mut gh = always_passing(
        env,
        BranchRules {
            required_checks: ["test".to_owned()].into(),
        },
    );
    url.clone_into(&mut gh.remote);
    daemon(
        env,
        services(
            Arc::new(gh),
            Arc::new(RecordingChannel::default()),
            Arc::new(RecordingTracker::default()),
        ),
    )
}

/// The value of `key` plain Git in the repository uses for `url`.
fn effective(env: &Env, key: &str, url: &str) -> String {
    git(&env.repo, &["config", "--get-urlmatch", key, url])
}

/// The operation `key` is stopped on and its error, if it is stopped.
fn stopped(d: &ovenspring_daemon::Daemon, key: &str) -> Option<(String, String)> {
    d.issue(key)
        .unwrap()
        .unwrap()
        .stopped
        .map(|s| (s.operation, s.error))
}

/// `operation-failed` lines for `key`, in order.
fn operation_failures(env: &Env, key: &str) -> Vec<Value> {
    env.events()
        .into_iter()
        .filter(|e| e["event"] == "operation-failed" && e["issue"] == key)
        .collect()
}

/// The count each `operation-failed` line of `operation` recorded, in order.
fn counts_of(failed: &[Value], operation: &str) -> Vec<Value> {
    failed
        .iter()
        .filter(|e| e["operation"] == operation)
        .map(|e| e["count"].clone())
        .collect()
}

/// The operation a failure while an issue waits to push, commit or step is retried under.
const RECONCILING: &str = "filing its step, committing or pushing";

fn no_github_unreachable_start(env: &Env) -> bool {
    !env.events()
        .iter()
        .any(|e| e["failure"]["key"] == "github-unreachable" && e["failure"]["start"] == true)
}

/// The fixture accepted `service` with the test token and received no other credential, and the
/// owner's helper left no `marker`.
fn assert_only_the_token_reached(fixture: &GitHttp, service: &str, marker: &Path) {
    assert!(
        fixture.accepted().iter().any(|s| s == service),
        "{:?}",
        fixture.accepted()
    );
    let authorizations = fixture.authorizations();
    assert!(
        authorizations.iter().all(|a| a == TEST_TOKEN_AUTHORIZATION),
        "{authorizations:?}"
    );
    assert!(!marker.exists(), "the owner's helper was called");
}

/// A person's commit to `path` on the base, pushed from a scratch clone of the bare repository
/// behind the fixture, so the factory's push has a real base to merge.
fn commit_to_base_as_person(env: &Env, path: &str) -> String {
    let scratch = env.dir.path().join("person");
    git(
        env.dir.path(),
        &[
            "clone",
            "-q",
            "-b",
            "develop",
            env.origin.to_str().unwrap(),
            scratch.to_str().unwrap(),
        ],
    );
    let file = scratch.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, "note\n").unwrap();
    git(&scratch, &["add", path]);
    git(
        &scratch,
        &[
            "-c",
            "user.name=dev",
            "-c",
            "user.email=dev@example.com",
            "commit",
            "-q",
            "-m",
            "A person's change",
        ],
    );
    git(
        &scratch,
        &["push", "-q", "origin", "HEAD:refs/heads/develop"],
    );
    git(&scratch, &["rev-parse", "HEAD"])
}

// ---------------------------------------------------------------------------------------------
// Both owner keys name the fixture's port, without which they would not match its endpoint, so
// plain Git would use them (the control below).
#[test]
fn the_factorys_commits_and_push_carry_the_token_account_while_the_owners_helper_and_header_stay_unused()
 {
    let env = Env::new();
    env.configure(CONFIG);
    env.add_branch("develop");
    env.script_agents(&Value::Object(straight("ACME-123")));
    let fixture = GitHttp::serve(&env.origin);
    let marker = env.dir.path().join("owner-helper-called");
    owners_settings(&env, &fixture.url, &marker);
    let mut d = daemon_connected_to(&env, &fixture.url);
    d.begin_issue("ACME-123", "Export", vec![]).unwrap();
    let worktree = d.store().worktree("ACME-123");
    run_until(&mut d, 60, |_| {}, |_| worktree.join(".git").exists());
    assert_eq!(
        git(
            &env.repo,
            &["rev-list", "--count", "ovenspring/ACME-123", "^develop"]
        ),
        "0",
        "the snapshot precedes the factory's first commit"
    );
    let config_before = std::fs::read(env.repo.join(".git/config")).unwrap();
    run_until(
        &mut d,
        60,
        |_| {},
        |d| stage(d, "ACME-123") == Stage::Reviewing,
    );
    let person = commit_to_base_as_person(&env, "docs/NOTE.md");
    assert_eq!(
        effective(&env, "credential.helper", &fixture.url),
        marker_helper(&env).display().to_string(),
        "plain Git would ask the owner's helper"
    );
    assert_eq!(
        effective(&env, "http.extraHeader", &fixture.url),
        format!("Authorization: {OWNER_AUTHORIZATION}"),
        "plain Git would send the owner's header"
    );

    run_until(
        &mut d,
        60,
        |_| {},
        |d| d.issue("ACME-123").unwrap().unwrap().pushed_sha.is_some(),
    );

    let pushed = d.issue("ACME-123").unwrap().unwrap().pushed_sha.unwrap();
    let parents = git(&env.origin, &["log", "-1", "--format=%P", &pushed]);
    assert_eq!(
        parents.split(' ').nth(1),
        Some(person.as_str()),
        "the pushed head is the merge of the base: {parents}"
    );
    let factorys = git(
        &env.origin,
        &["log", IDENTITY, &pushed, &format!("^{person}")],
    );
    let issue_commits = git(
        &env.origin,
        &[
            "rev-list",
            "--count",
            "--no-merges",
            &pushed,
            &format!("^{person}"),
        ],
    );
    assert!(issue_commits != "0", "the issue's own commit was pushed");
    assert!(
        factorys
            .lines()
            .all(|identity| identity == ACCOUNT_IN_BOTH_ROLES),
        "{factorys}"
    );
    assert_eq!(
        git(&env.origin, &["log", "-1", IDENTITY, &person]),
        "dev <dev@example.com> | dev <dev@example.com>"
    );
    assert_only_the_token_reached(&fixture, "git-receive-pack", &marker);
    assert_eq!(
        std::fs::read(env.repo.join(".git/config")).unwrap(),
        config_before
    );
}

// ---------------------------------------------------------------------------------------------
#[test]
fn saving_how_the_product_runs_fetches_the_base_with_the_configured_token() {
    let env = Env::new();
    env.configure("[app]\n");
    let fixture = GitHttp::serve(&env.origin);
    let marker = env.dir.path().join("owner-helper-called");
    owners_settings(&env, &fixture.url, &marker);
    let dash = Dashboard::start(&env);
    dash.connect_github("test-token", &fixture.url);

    let saved = dash.put(
        "/api/v1/setup/run",
        &json!({
            "runs": "commands",
            "setup": "mkdir -p \"$OVENSPRING_DATA_DIR/www\" && touch \"$OVENSPRING_DATA_DIR/www/ready\"",
            "start": support::serve("\"$OVENSPRING_DATA_DIR/www\""),
            "ready": "http://127.0.0.1:$PORT/ready"
        }),
    );

    assert_eq!(saved.status, 200, "{}", saved.body);
    assert!(Config::load(&env.repo).unwrap().app.settled());
    assert_only_the_token_reached(&fixture, "git-upload-pack", &marker);
}

// Both save routes reach the base through the same fetch; this one goes through Settings. A
// command that fails its check shows whether the fetch before it ran: a refused fetch is refused
// under `request`, and a fetch that passed lets the check refuse under `check`.
#[test]
fn a_save_after_origin_changed_since_the_github_check_is_refused_before_any_contact() {
    let env = Env::new();
    env.configure("");
    let fixture = GitHttp::serve(&env.origin);
    let dash = Dashboard::start(&env);
    let connected = env.origin.to_str().unwrap();
    dash.connect_github("test-token", connected);
    git(&env.repo, &["remote", "set-url", "origin", &fixture.url]);
    let failing = json!({"runs": "commands", "setup": "", "start": "exit 3", "ready": ""});

    let saved = dash.put("/api/v1/settings/app", &failing);

    assert_eq!(saved.status, 400, "{}", saved.body);
    let why = &saved.json()["errors"]["request"];
    let why = why.as_str().unwrap_or_else(|| panic!("{}", saved.body));
    assert!(
        why.contains(&fixture.url) && why.contains(connected),
        "{why}"
    );
    assert_eq!(
        fixture.requests(),
        0,
        "the replacement endpoint was contacted"
    );
    git(&env.repo, &["remote", "set-url", "origin", connected]);
    let restored = dash.put("/api/v1/settings/app", &failing);
    assert_eq!(restored.status, 400, "control: {}", restored.body);
    assert!(
        restored.json()["errors"]["check"].is_string(),
        "control: {}",
        restored.body
    );
}

#[test]
fn an_origin_changed_while_the_daemon_runs_stops_the_issue_instead_of_retrying_as_unreachable() {
    let env = Env::new();
    env.configure(CONFIG);
    env.add_branch("develop");
    env.script_agents(&Value::Object(straight("ACME-123")));
    let mut d = daemon_connected_to(&env, env.origin.to_str().unwrap());
    d.begin_issue("ACME-123", "Export", vec![]).unwrap();
    let elsewhere = env.dir.path().join("elsewhere.git");
    git(
        env.dir.path(),
        &["init", "-q", "--bare", elsewhere.to_str().unwrap()],
    );
    git(
        &env.repo,
        &["remote", "set-url", "origin", elsewhere.to_str().unwrap()],
    );

    run_until(&mut d, 60, |_| {}, |d| stopped(d, "ACME-123").is_some());

    let failed = operation_failures(&env, "ACME-123");
    let (operation, error) = stopped(&d, "ACME-123").unwrap();
    assert_eq!(
        counts_of(&failed, &operation),
        [json!(1), json!(2)],
        "{failed:?}"
    );
    assert!(
        env.events()
            .iter()
            .all(|e| e["event"] != "service-unreachable"),
        "{:?}",
        env.events()
    );
    assert!(
        error.contains(env.origin.to_str().unwrap()) && error.contains(elsewhere.to_str().unwrap()),
        "{error}"
    );
    assert!(filed_steps(&env, "ACME-123").contains(&"exception".to_owned()));
    assert!(no_github_unreachable_start(&env), "{:?}", env.events());
}

// ---------------------------------------------------------------------------------------------
#[test]
fn a_token_github_refuses_stops_the_issue_instead_of_retrying_as_unreachable() {
    let env = Env::new();
    env.configure(CONFIG);
    env.add_branch("develop");
    env.script_agents(&Value::Object(straight("ACME-123")));
    let gh = Arc::new(always_passing(
        &env,
        BranchRules {
            required_checks: ["test".to_owned()].into(),
        },
    ));
    let mut d = daemon(
        &env,
        services(
            gh.clone(),
            Arc::new(RecordingChannel::default()),
            Arc::new(RecordingTracker::default()),
        ),
    );
    d.begin_issue("ACME-123", "Export", vec![]).unwrap();
    run_until(
        &mut d,
        60,
        |_| {},
        |d| stage(d, "ACME-123") == Stage::Reviewing,
    );
    assert_ne!(
        git(
            &env.repo,
            &["rev-list", "--count", "ovenspring/ACME-123", "^develop"]
        ),
        "0",
        "the issue's commit is on its branch before its push"
    );
    std::fs::rename(&env.origin, env.dir.path().join("origin-away.git")).unwrap();
    gh.refuse_viewer.store(true, Ordering::SeqCst);

    run_until(&mut d, 60, |_| {}, |d| stopped(d, "ACME-123").is_some());

    let failed = operation_failures(&env, "ACME-123");
    let (operation, error) = stopped(&d, "ACME-123").unwrap();
    assert_eq!(operation, RECONCILING);
    assert_eq!(
        d.issue("ACME-123").unwrap().unwrap().flow.resume(),
        Some(&Stage::Pushing),
        "it stopped publishing"
    );
    assert_eq!(
        counts_of(&failed, RECONCILING),
        [json!(1), json!(2)],
        "{failed:?}"
    );
    assert!(error.starts_with("git "), "{error}");
    assert!(filed_steps(&env, "ACME-123").contains(&"exception".to_owned()));
    assert!(no_github_unreachable_start(&env), "{:?}", env.events());
}
