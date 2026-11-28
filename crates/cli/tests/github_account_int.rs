//! @lane: integration
//! Area: the GitHub account the factory's own Git commands act as: commit identity, the token a
//! push and its reads authenticate with, and the `origin` they may reach.
//!
//! Skeleton (GH-34, test-design). A block still written as comments only is a planned test whose
//! boundary is not executable yet; the implementing task replaces it with the test, adding
//! `mod support;`, imports and the crate-level `cfg_attr` lint allowance the other `*_int.rs`
//! files carry.
//!
//! Governing source: `docs/design/github-git-identity-design.md` (cited below as "Design"),
//! § Acceptance Criteria AC1-AC5, § Selected Design items 3-6, § Test Boundaries and Verification.
//!
//! Proof contract shared by every test here (Design § Test Boundaries > Mock Boundary Decisions):
//! - Real: `ovenspring_daemon::Daemon` built with `support::daemon` and driven by `tick()` /
//!   `support::run_until`; the dashboard server through `support::dashboard::Dashboard`; Git as a
//!   child process (`worktree::git_as`, `worktree::git_remote`) over the `support::Env` repository,
//!   its issue worktrees and a bare `origin`. Identity, credential and config effects are Git's
//!   behavior, so Git is never substituted.
//! - Substituted outside the claim: GitHub REST by `support::fake_github::FakeGitHub` (`viewer`
//!   returns `Account { login: "factory-bot", id: 12345678 }`, `git_remote()` returns its `remote`
//!   and `"test-token"`); agent CLIs by `Env::script_agents`; GitHub's Git endpoint, in tests 1 and
//!   2 only, by a new plain-HTTP localhost fixture in `crates/cli/tests/support` that runs
//!   `git http-backend` as CGI over the test's bare `origin`, answers 401 with a Basic challenge
//!   unless `Authorization` is `Basic base64(x-access-token:test-token)`, records every
//!   `Authorization` value it receives, and records each accepted request (upload-pack /
//!   receive-pack).
//! - Isolation: owner Git settings go into the repository's own `.git/config` only, never into the
//!   process environment, because tests in this binary run concurrently.
//! - Oracles are literals: `factory-bot <12345678+factory-bot@users.noreply.github.com>`, the
//!   fixture's recorded values, and the config bytes read before the Act.
//!
//! Proof owned elsewhere (not repeated here):
//! - `viewer()` decoding `id` and `login` and `Account::email()`: extend
//!   `crates/integrations/tests/github_contract.rs`
//!   `every_request_carries_the_token_and_api_version` (Design § Tests, AC1 mapping).
//! - The connection guard's refusals (SSH `origin`, a `pushurl` elsewhere, `origin` changed, an
//!   inherited `insteadOf` rewrite), each without contacting the other destination: unit tests in
//!   `crates/daemon/src/worktree.rs` against `git_remote` / `remote_with` (Design § Tests, AC4 a-d).
//!
//! Existing tests this change revises in place (no skeleton; the implementing task edits them):
//! - `crates/cli/tests/sprint_int.rs`, the assertion at line ~1416 that the agreements commit is
//!   `Storefront Dev <dev@storefront.test>`: becomes `%an <%ae> | %cn <%ce>` showing the fake
//!   account in both roles (AC1, agreements commit).
//! - `crates/cli/tests/errors_int.rs` `github_stays_one_open_error_until_every_failing_way_answers_by_git`
//!   and `an_outage_recorded_by_the_previous_version_keeps_its_since_and_clears_once`: replace
//!   `set_origin(gone.git)` and its restore with renaming the bare repository at `env.origin` to a
//!   sibling path and back, so `origin`'s URL stays the connected one and the guard admits it;
//!   assertions unchanged (AC5 outage, Events AC-077/078).
//! - `crates/daemon/tests/worktree.rs`: the `ensure_detached` tests fetch the base before calling
//!   it, since `ensure_detached` no longer fetches.
//! - Unchanged and still passing: the pre-push refusal tests in `issue_flow_int.rs` and the outage
//!   cases in `resilience_int.rs` (file-path `origin`s that the guard admits because `FakeGitHub`'s
//!   `remote` is that path).

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
// 1. Early verification point (Design § Early Verification Point). Implement this first; if the
//    fixture does not receive `test-token`, or the owner's helper is called, stop and revisit the
//    remote-access design before routing the other callers (Design § Implementation Plan, step 2).
// AC: AC1 "When the factory creates an issue commit, a local merge of the base or the branch
//     head, or the agreements commit, `git log --format='%an <%ae> | %cn <%ce>'` on that commit
//     shall show `<login> <<id>+<login>@users.noreply.github.com>` in both roles. Commits it did
//     not create keep their metadata."; AC2 "When the factory pushes, or reads `origin` for the
//     work that leads to a push ..., the command shall authenticate with the configured token and
//     succeed. The owner's credential helper is never called, and an inherited `Authorization`
//     header is never sent."; AC3 "... the repository's and the owner's Git config files and
//     stored credentials shall keep their bytes."
// Behavior: `origin` is the fixture's `http://127.0.0.1:<port>/origin.git` (actual port) and
//     `FakeGitHub.remote` is that URL -> the repository's `.git/config` holds an owner
//     `credential.http://127.0.0.1:<port>.helper` (a script that appends to a marker file and
//     answers `owner`/`owner-secret`) and an owner
//     `http.http://127.0.0.1:<port>/origin.git.extraHeader` =
//     `Authorization: Basic base64(owner:owner-secret)`; both keys name the actual port, since a
//     key without it does not match the endpoint -> ACME-123 runs through review with the flow of
//     `outside_change_int.rs` `a_persons_commit_pushed_while_the_issue_waits_is_judged_by_the_gate_and_merged`
//     -> before the factory's push, a person's commit to a different file is added to the base
//     from a scratch clone whose `origin` is the fixture's backing bare repository by path, so
//     `bring_in` makes a real two-parent merge -> the factory commits, merges the base and pushes
//     the issue branch through the fixture.
// Assert (one behavior, several assertions):
//     - Control, before the Act: in the repository, `git config --get-urlmatch credential.helper
//       <fixture URL>` returns the marker script and `git config --get-urlmatch http.extraHeader
//       <fixture URL>` returns the owner's header, so plain Git would use both.
//     - On the pushed branch in the backing bare repository: the issue commit and the base merge
//       (two parents) each show `factory-bot <12345678+factory-bot@users.noreply.github.com>` in
//       both roles of `%an <%ae> | %cn <%ce>`; the person's commit keeps its own name and email.
//     - The fixture accepted receive-pack with `Basic base64(x-access-token:test-token)` and never
//       received the owner's `Authorization` value.
//     - The marker file does not exist.
//     - The repository's `.git/config` has the same bytes as once the issue worktree and its branch
//       tracking were set up, before the factory's first commit.
// @lane: integration
// @dependency: daemon, flow, real Git (commit, merge, fetch, ls-remote, push), git http-backend
//     fixture (new), FakeGitHub, fake agent
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
// 2. The dashboard's base fetch, a separate production route (`setup::settle_app`), which reads
//    the token from the credentials file through `github::selected_token` instead of the daemon's
//    `Services.github`.
// AC: AC2 "... reads `origin` for the work that leads to a push (including the base fetch of the
//     dashboard's Run the product save), the command shall authenticate with the configured token
//     and succeed. The owner's credential helper is never called ..."
// Behavior: a variant of `dashboard_int.rs`
//     `run_the_product_writes_commands_only_once_they_start_then_the_goal_finishes_setup`:
//     `[app]` unsettled, `origin` is the fixture URL of test 1, the dashboard's credentials file
//     (`Dashboard.credentials`) holds the GitHub token `test-token`, and the repository's
//     `.git/config` holds the same port-scoped owner marker helper -> the owner saves commands
//     that start (`PUT /api/v1/setup/run`).
// Assert: the save returns 200 and `[app]` is written; the fixture accepted upload-pack with
//     `Basic base64(x-access-token:test-token)`; the marker file does not exist.
// @lane: integration
// @dependency: dashboard server, setup, configuration, base worktree, credentials file, real Git
//     fetch, git http-backend fixture (new)
#[test]
fn saving_how_the_product_runs_fetches_the_base_with_the_configured_token() {
    let env = Env::new();
    env.configure("[app]\n");
    let fixture = GitHttp::serve(&env.origin);
    let marker = env.dir.path().join("owner-helper-called");
    owners_settings(&env, &fixture.url, &marker);
    let dash = Dashboard::start(&env);
    dash.hold_github_token("test-token");

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

// ---------------------------------------------------------------------------------------------
// 3. AC4 failure path: classification only; that nothing reaches the other URL is owned by the
//    `worktree.rs` unit tests named in the header.
// AC: AC4 "If ... any fetch or push URL of `origin` differs from the URL the GitHub connection was
//     built from (including after `origin` changes while the daemon runs), then the factory's
//     remote Git command ... shall fail. ... The failure shall count as an operation failure
//     (Factory Core AC-062), so two passes in a row stop the issue or sprint with `exception`, and
//     it shall never be recorded as `github-unreachable`."
// Behavior: a daemon over `Env` with `FakeGitHub.remote` = `env.origin` -> `begin_issue("ACME-123")`
//     -> `origin` is set (`git remote set-url`) to a second, reachable bare repository -> the
//     daemon ticks until the issue stops.
// Assert: each pass records `operation-failed` for ACME-123, never `service-unreachable`; the
//     second pass stops the issue on that operation with `exception` filed; the stop or exception
//     names both URLs; `events.jsonl` (`Env::events()`) has no `github-unreachable` start.
// @lane: integration
// @dependency: daemon, flow, stops (`settle_for`, `git_out_of_reach`), real Git, store,
//     FakeGitHub, fake agent
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
// 4. AC5 refused token.
// AC: AC5 "When a factory Git command fails while `origin` passes the check in AC4 and its
//     token-authenticated probe also fails, ... If GitHub refuses the token (`viewer` gets an
//     answer with an error status), it shall count as an operation failure and stop after two
//     passes."
// Behavior: as test 3, but `origin` stays the connected path; after `begin_issue` the bare
//     repository at `env.origin` is renamed to a sibling path so the probe fails, and
//     `FakeGitHub.refuse_viewer` (new flag, like `refuse_replies`) makes `viewer` answer HTTP 401.
// Assert: `operation-failed` on two passes, ACME-123 stopped with `exception` filed, and no
//     `github-unreachable` start in `events.jsonl`. (The opposite branch, `viewer` answering, is the
//     revised `errors_int.rs` outage test named in the header.)
// @lane: integration
// @dependency: daemon, flow, stops (`git_out_of_reach`, `errors::out_of_reach`), real Git, store,
//     FakeGitHub (`refuse_viewer`), fake agent
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
