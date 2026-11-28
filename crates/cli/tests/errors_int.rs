//! @lane: integration
//! Area: errors (start, repeat, clear), their Slack notices (M-13, M-14, and M-08 for an agent CLI
//! error), and the events read (`GET /api/v1/events`), over the real daemon, store and server.
//!
//! Skeleton (GH-9, test-design). A block still written as comments only is a planned test whose
//! boundary is not executable yet; the implementing task replaces it with the test.
//!
//! Governing sources: `docs/design/events-and-error-notices-design.md` (cited below as "Design"),
//! § Acceptance Criteria, § Selected Design, § Test Boundaries > Integration Verification Points;
//! `docs/prd/oven-spring-prd.md` AC-072, AC-076, AC-077, AC-078.
//!
//! Proof contract shared by every daemon test here (Design § Test Boundaries > Mock Boundary
//! Decisions):
//! - Real: `ovenspring_daemon::Daemon` built with `support::daemon` and driven by `tick()` /
//!   `support::run_until`; the store and `events.jsonl` in the `support::Env` temporary state
//!   directory; a restart is a new `Daemon` over the same `Env`; the filter command is a real
//!   `sh -c` script (`Env::write_filter_script`, `Env::configure_filter`); git `origin` is a real
//!   local repository (an unreachable path, then a reachable bare repository).
//! - Slack: the real `Slack` adapter over `support::slack_stub::SlackStub` (`SlackStub::channel()`),
//!   never `RecordingChannel`, because once-delivery depends on the adapter's marked post and
//!   history lookup. The stub gains (Design § Implementation Path Mapping): `conversations.history`
//!   returning posted messages with their metadata, honoring `oldest` and paging by `cursor`; a
//!   mode that stores a post and then fails the transport (a lost answer).
//! - Substituted outside the claim: GitHub HTTP (`support::fake_github::FakeGitHub`, its
//!   `unreachable` switch, and a way to fail pull reads while `viewer` answers), tracker HTTP
//!   (`support::recording::RecordingTracker`, `unreachable` and a refusing mode), agent CLIs
//!   (`Env::script_agents`).
//! - Counting oracle: messages in the stub whose metadata is
//!   `{event_type: "ovenspring_notice", event_payload: {id: "<key>@<since>/<started|cleared>"}}`,
//!   and lines in `events.jsonl` (`Env::events()`): start lines (`failure.start`), `error-cleared`,
//!   `error-notice` by `outcome`. Expected counts are literals from the AC, never read back from
//!   the daemon's own `Errors`.
//!
//! Consolidation: the per-line rules (fold order, attribution order, told state, window, masking
//! of served strings) are owned by the single `GET /events` test at the end; the daemon tests assert
//! only counts, order and the lines a transition writes. The adapter's request shape (history
//! read before a marked post, `oldest` an hour before the line's time, metadata, a second history
//! page after a short first page with `next_cursor`) is owned by
//! `crates/integrations/tests/slack_contract.rs`. `append_line` ending a torn line, `records::lines`
//! decoding per line, the `[notices]` default, the notices section saving without a connection
//! check, and the `config-reloaded.changes` diff are owned by their unit tests (Design § Test
//! Boundaries). The existing `resilience_int.rs` test
//! `a_service_out_of_reach_is_recorded_from_its_first_failure_until_it_answers` keeps its
//! behavior and reads `Errors::unreachable()` once `records::unreachable` is removed.

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

use std::sync::Arc;
use std::sync::atomic::Ordering;

use ovenspring_core::flow::Stage;
use ovenspring_core::item::Status;
use ovenspring_core::output::SprintPlanOutput;
use ovenspring_core::sprint::SprintStage;
use ovenspring_core::step::Step;
use ovenspring_daemon::Daemon;
use ovenspring_integrations::github::{CheckState, GitHub as _};
use ovenspring_integrations::tracker::TrackerIssue;
use serde_json::{Value, json};
use support::dashboard::Dashboard;
use support::fake_github::FakeGitHub;
use support::git_http::GitHttp;
use support::recording::{RecordingChannel, RecordingTracker};
use support::scripts::requiring_test;
use support::slack_stub::SlackStub;
use support::{Env, daemon, git, run_until};

fn passing_checks() -> support::fake_github::CheckScript {
    Box::new(|_| vec![("test".to_owned(), CheckState::Success)])
}

fn passing_github(env: &Env) -> Arc<FakeGitHub> {
    Arc::new(FakeGitHub::new(
        env.origin.clone(),
        requiring_test(),
        passing_checks(),
    ))
}

/// The commit `origin`'s bare repository holds for ACME-123's branch; empty when it has none.
fn issue_head_on_origin(env: &Env) -> String {
    git(
        &env.origin,
        &[
            "for-each-ref",
            "--format=%(objectname)",
            "refs/heads/ovenspring/ACME-123",
        ],
    )
}

/// Services with Slack as the real adapter over `slack`.
fn on_slack(
    github: &Arc<FakeGitHub>,
    slack: &SlackStub,
    tracker: Option<Arc<RecordingTracker>>,
) -> ovenspring_daemon::Services {
    ovenspring_daemon::Services {
        github: Some(github.clone()),
        channel: slack.channel(),
        tracker: tracker.map(support::linear),
        secrets: Vec::new(),
    }
}

/// A daemon over `env` as `ovenspring daemon` starts it, which reads back what it recorded.
fn started(env: &Env, services: ovenspring_daemon::Services) -> Daemon {
    let mut d = daemon(env, services);
    d.recover().unwrap();
    d
}

/// The marker of each error notice in the channel, in the order they were posted.
fn notices(slack: &SlackStub, key: &str) -> Vec<String> {
    slack
        .posted()
        .iter()
        .filter(|m| m["metadata"]["event_type"] == "ovenspring_notice")
        .filter_map(|m| m["metadata"]["event_payload"]["id"].as_str())
        .filter(|id| id.starts_with(&format!("{key}@")))
        .map(str::to_owned)
        .collect()
}

fn of_key(key: &str) -> impl Fn(&Value) -> bool + '_ {
    move |e: &Value| e["failure"]["key"] == key
}

fn starts(env: &Env, key: &str) -> Vec<Value> {
    env.events()
        .into_iter()
        .filter(of_key(key))
        .filter(|e| e["failure"]["start"] == true)
        .collect()
}

fn failures(env: &Env, key: &str) -> usize {
    env.events()
        .into_iter()
        .filter(of_key(key))
        .filter(|e| e["event"] != "error-cleared" && e["event"] != "error-notice")
        .count()
}

fn cleared(env: &Env, key: &str) -> usize {
    env.events()
        .into_iter()
        .filter(of_key(key))
        .filter(|e| e["event"] == "error-cleared")
        .count()
}

fn told(env: &Env, key: &str, phase: &str) -> usize {
    env.events()
        .into_iter()
        .filter(of_key(key))
        .filter(|e| e["event"] == "error-notice" && e["phase"] == phase && e["outcome"] == "told")
        .count()
}

/// Now's "Can't reach" status, as `GET /api/v1/overview` serves it.
fn unreachable_now(d: &Daemon) -> Value {
    let o = ovenspring_daemon::overview::overview(d.store(), None, Some(d.config())).unwrap();
    serde_json::to_value(o.status.unreachable).unwrap()
}

/// Takes the bare repository behind `origin` away, so `origin` does not answer while its URL, the
/// one the GitHub connection was built for, stays as it is.
fn take_origin_away(env: &Env) {
    std::fs::rename(&env.origin, env.dir.path().join("origin-away.git")).unwrap();
}

fn bring_origin_back(env: &Env) {
    std::fs::rename(env.dir.path().join("origin-away.git"), &env.origin).unwrap();
}

// ---------------------------------------------------------------------------------------------
// 1. Early verification point (Design § Verification Strategy > Early Verification Point).
// AC: "While notices are on, when an error starts, the system shall post one M-13. Repeats shall
//     post nothing. When the error clears, the system shall post one M-14. A daemon restart while
//     it lasts shall post nothing more." and "If Slack accepts a notice and its answer is lost, or
//     the daemon stops before recording that it was told, then the next delivery shall find that
//     message in the channel in force and record it as told without posting it again." (Design §
//     Acceptance Criteria > Error notices; PRD AC-077)
// Behavior: GitHub unreachable (`FakeGitHub.unreachable`) for three passes -> the stub stores the
//     M-13 and loses its answer -> the daemon is dropped before another pass and a new `Daemon` is
//     built over the same `Env` -> GitHub fails once more -> GitHub answers.
// Observable result:
//     - the stub holds exactly one message marked `github-unreachable@<since>/started` and exactly
//       one marked `github-unreachable@<since>/cleared`;
//     - `events.jsonl` has exactly one start line for `github-unreachable`, one `error-cleared`
//       for it, and one `error-notice` with `outcome: "told"` per phase;
//     - `GET /api/v1/overview` (or `overview::status`) `status.unreachable` names GitHub with the
//       first failure's `since` after the first failure, after the restart and after the last
//       failure, and no longer names it after the clear.
// @lane: integration
// @dependency: daemon (restart included), store, real `Slack` over `SlackStub` (lost-answer mode,
//     history with metadata), FakeGitHub
// Name: github_out_of_reach_is_told_once_when_it_starts_and_once_when_it_clears_across_a_lost_answer_and_a_restart
#[test]
fn github_out_of_reach_is_told_once_when_it_starts_and_once_when_it_clears_across_a_lost_answer_and_a_restart()
 {
    let env = Env::new();
    env.configure("");
    env.script_agents(&Value::Object(support::scripts::straight("ACME-123")));
    let gh = passing_github(&env);
    gh.unreachable.store(true, Ordering::SeqCst);
    let slack = SlackStub::default();
    slack.lose_answer("chat.postMessage", true);
    let mut d = started(&env, on_slack(&gh, &slack, None));
    d.begin_issue("ACME-123", "Export", vec![]).unwrap();

    // The M-13 is posted and its answer lost; the daemon stops before another pass.
    run_until(
        &mut d,
        60,
        |_| {},
        |_| {
            failures(&env, "github-unreachable") >= 3
                && !notices(&slack, "github-unreachable").is_empty()
        },
    );
    let since = starts(&env, "github-unreachable")[0]["failure"]["since"].clone();
    let now = json!({"service": "github", "since": since, "why": "no-connection"});
    assert_eq!(unreachable_now(&d), now);
    assert_eq!(told(&env, "github-unreachable", "started"), 0);
    drop(d);

    slack.lose_answer("chat.postMessage", false);
    let mut d = started(&env, on_slack(&gh, &slack, None));
    assert_eq!(
        unreachable_now(&d),
        now,
        "a restart keeps the first failure's time"
    );
    let before = failures(&env, "github-unreachable");
    run_until(
        &mut d,
        60,
        |_| {},
        |_| {
            failures(&env, "github-unreachable") > before
                && told(&env, "github-unreachable", "started") == 1
        },
    );
    assert_eq!(
        unreachable_now(&d),
        now,
        "a failure after the restart is a repeat"
    );

    gh.unreachable.store(false, Ordering::SeqCst);
    run_until(
        &mut d,
        60,
        |_| {},
        |_| told(&env, "github-unreachable", "cleared") == 1,
    );

    let since = since.as_str().unwrap();
    assert_eq!(
        notices(&slack, "github-unreachable"),
        vec![
            format!("github-unreachable@{since}/started"),
            format!("github-unreachable@{since}/cleared"),
        ]
    );
    assert_eq!(starts(&env, "github-unreachable").len(), 1);
    assert_eq!(cleared(&env, "github-unreachable"), 1);
    assert_eq!(told(&env, "github-unreachable", "started"), 1);
    assert_eq!(told(&env, "github-unreachable", "cleared"), 1);
    assert_eq!(unreachable_now(&d), Value::Null);
}

// ---------------------------------------------------------------------------------------------
// 2.
// AC: "If Slack does not accept a notice, then the system shall post it once Slack accepts again,
//     in the order the notices were recorded. A waiting M-13 shall go before its M-14." (Design §
//     Acceptance Criteria > Error notices; UI Spec Slack Messages, Delivery)
// Behavior: the stub refuses `chat.postMessage` (`SlackStub::fail`) -> the tracker becomes
//     unreachable and then answers again over several passes -> the stub accepts again.
// Observable result: the stub's posted messages, in order, are exactly the
//     `tracker-unreachable@<since>/started` message and then its `/cleared` message; `events.jsonl`
//     has exactly one `channel-error` line, for the `/started` notice, however many passes were
//     refused: a refusal stops the pass's delivery, so the waiting `/cleared` notice is never
//     attempted while the one before it is refused (Design § Selected Design > Delivery; finding
//     I001 of the work plan).
// @lane: integration
// @dependency: daemon, store, real `Slack` over `SlackStub` (refusing mode), RecordingTracker
// Name: notices_slack_refused_go_out_in_order_once_slack_accepts
#[test]
fn notices_slack_refused_go_out_in_order_once_slack_accepts() {
    let env = Env::new();
    env.configure("");
    let gh = passing_github(&env);
    let tracker = Arc::new(RecordingTracker::default());
    tracker.unreachable.store(true, Ordering::SeqCst);
    let slack = SlackStub::default();
    slack.fail("chat.postMessage", true);
    let mut d = started(&env, on_slack(&gh, &slack, Some(tracker.clone())));

    run_until(
        &mut d,
        60,
        |_| {},
        |_| failures(&env, "tracker-unreachable") >= 3,
    );
    tracker.unreachable.store(false, Ordering::SeqCst);
    run_until(
        &mut d,
        60,
        |_| {},
        |_| cleared(&env, "tracker-unreachable") == 1,
    );
    for _ in 0..3 {
        d.tick().unwrap();
    }
    assert_eq!(notices(&slack, "tracker-unreachable"), Vec::<String>::new());
    slack.fail("chat.postMessage", false);
    run_until(
        &mut d,
        60,
        |_| {},
        |_| told(&env, "tracker-unreachable", "cleared") == 1,
    );

    let since = starts(&env, "tracker-unreachable")[0]["failure"]["since"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        notices(&slack, "tracker-unreachable"),
        vec![
            format!("tracker-unreachable@{since}/started"),
            format!("tracker-unreachable@{since}/cleared"),
        ]
    );
    let refusals: Vec<Value> = env
        .events()
        .into_iter()
        .filter(|e| {
            e["event"] == "channel-error"
                && e["marker"]
                    .as_str()
                    .is_some_and(|m| m.starts_with("tracker-unreachable@"))
        })
        .collect();
    assert_eq!(refusals.len(), 1, "{refusals:?}");
    assert_eq!(
        refusals[0]["marker"],
        format!("tracker-unreachable@{since}/started")
    );
}

// ---------------------------------------------------------------------------------------------
// 3.
// AC: "While notices are off, the system shall post no M-13 or M-14 and drop the waiting ones, and
//     the Events page shall still list every line. M-08 is unaffected." and "The daemon shall apply
//     it on its next pass and record 'Settings changed: Error notices are now off/on.'" (Design §
//     Acceptance Criteria > Error notices; PRD AC-078)
// Behavior: notices on, the stub refusing -> the tracker becomes unreachable (an M-13 waits) ->
//     `config.toml` is rewritten with `[notices]\nerrors = false` (`Env::configure`) -> next pass ->
//     the stub accepts again -> the tracker fails again, then answers.
// Observable result:
//     - the stub holds no message marked `tracker-unreachable@…`;
//     - `events.jsonl` has the start line, its repeat lines and the `error-cleared` line, an
//       `error-notice` with `outcome: "dropped"` for the waiting start, and the clear's
//       `failure.notice` is `"off"`;
//     - `events.jsonl` has one `config-reloaded` line whose `changes` holds
//       `{"setting": "notices.errors", "value": false}`.
// Not here: "M-08 is unaffected" is a case of test 6.
// @lane: integration
// @dependency: daemon (`reload_config`), store, `config.toml`, real `Slack` over `SlackStub`,
//     RecordingTracker
#[test]
fn with_error_notices_turned_off_the_waiting_notice_is_dropped_and_nothing_is_posted() {
    let env = Env::new();
    env.configure("");
    let gh = passing_github(&env);
    let tracker = Arc::new(RecordingTracker::default());
    tracker.unreachable.store(true, Ordering::SeqCst);
    let slack = SlackStub::default();
    slack.fail("chat.postMessage", true);
    let mut d = started(&env, on_slack(&gh, &slack, Some(tracker.clone())));
    run_until(
        &mut d,
        60,
        |_| {},
        |_| failures(&env, "tracker-unreachable") >= 1,
    );

    env.configure("[notices]\nerrors = false\n");
    d.tick().unwrap();
    slack.fail("chat.postMessage", false);
    let failed_before = failures(&env, "tracker-unreachable");
    run_until(
        &mut d,
        60,
        |_| {},
        |_| failures(&env, "tracker-unreachable") > failed_before,
    );
    tracker.unreachable.store(false, Ordering::SeqCst);
    run_until(
        &mut d,
        60,
        |_| {},
        |_| cleared(&env, "tracker-unreachable") == 1,
    );
    for _ in 0..3 {
        d.tick().unwrap();
    }

    assert_eq!(notices(&slack, "tracker-unreachable"), Vec::<String>::new());
    assert_eq!(starts(&env, "tracker-unreachable").len(), 1);
    assert!(
        failures(&env, "tracker-unreachable") >= 2,
        "the repeat stays listed"
    );
    let lines: Vec<Value> = env
        .events()
        .into_iter()
        .filter(|e| e["event"] == "config-reloaded" || of_key("tracker-unreachable")(e))
        .collect();
    let dropped: Vec<&Value> = lines
        .iter()
        .filter(|e| e["event"] == "error-notice" && e["outcome"] == "dropped")
        .collect();
    assert_eq!(dropped.len(), 1, "{dropped:?}");
    assert_eq!(dropped[0]["phase"], "started");
    let clear = lines
        .iter()
        .find(|e| e["event"] == "error-cleared")
        .unwrap();
    assert_eq!(clear["failure"]["notice"], "off");
    let reloads: Vec<&Value> = lines
        .iter()
        .filter(|e| e["event"] == "config-reloaded")
        .collect();
    assert_eq!(reloads.len(), 1, "{reloads:?}");
    assert_eq!(
        reloads[0]["changes"],
        json!([{"setting": "notices.errors", "value": false}])
    );
}

// ---------------------------------------------------------------------------------------------
// 4.
// AC: "When a GitHub operation keeps failing as unreachable while another way to GitHub, or another
//     GitHub request, answers, the error shall stay open with no clear and no new start. It shall
//     clear once a pass finds every way that failed answering and no GitHub operation failing as
//     unreachable." (Design § Acceptance Criteria > Error notices; § Selected Design > Errors,
//     GitHub clear rule)
// Cases (reported distinctly; each starts from a fresh `Env`):
//     a. `via: git`: `origin` is the token-required Git endpoint fixture, whose bare repository is
//        taken away while the API (`viewer`) answers -> several passes -> it is brought back; only
//        the token-authenticated probe sees it answer.
//     b. `via: api`: pull reads fail as unreachable (HTTP 502) while `viewer` answers -> several
//        passes -> pull reads answer.
// Observable result (each case): while the failing way still fails, `events.jsonl` has exactly one
//     start line for `github-unreachable` and no `error-cleared`, and the stub one `/started`
//     message; after the way answers, exactly one `error-cleared` and one `/cleared` message.
// @lane: integration
// @dependency: daemon, store, real git `origin` over the GitHttp fixture (case a), FakeGitHub, real
//     `Slack` over `SlackStub`
// Name: github_stays_one_open_error_until_every_failing_way_answers

/// GitHub's error while one way to it keeps failing, then after `answer` makes that way answer:
/// one start and one `/started` message while it fails, one clear and one `/cleared` message after.
fn stays_open_until_it_answers(
    env: &Env,
    slack: &SlackStub,
    d: &mut Daemon,
    answer: impl FnOnce(),
) {
    let first = failures(env, "github-unreachable");
    run_until(
        d,
        60,
        |_| {},
        |_| {
            failures(env, "github-unreachable") >= first + 3
                && notices(slack, "github-unreachable").len() == 1
        },
    );
    assert_eq!(starts(env, "github-unreachable").len(), 1);
    assert_eq!(cleared(env, "github-unreachable"), 0);

    answer();
    run_until(
        d,
        60,
        |_| {},
        |_| notices(slack, "github-unreachable").len() == 2,
    );
    for _ in 0..3 {
        d.tick().unwrap();
    }
    assert_eq!(starts(env, "github-unreachable").len(), 1);
    assert_eq!(cleared(env, "github-unreachable"), 1);
    assert!(notices(slack, "github-unreachable")[1].ends_with("/cleared"));
}

#[test]
fn github_stays_one_open_error_until_every_failing_way_answers_by_git() {
    let env = Env::new();
    env.configure("");
    env.script_agents(&Value::Object(support::scripts::straight("ACME-123")));
    // `origin` answers only the token, so the probe that clears the error must carry it.
    let fixture = GitHttp::serve(&env.origin);
    git(&env.repo, &["remote", "set-url", "origin", &fixture.url]);
    let mut gh = FakeGitHub::new(env.origin.clone(), requiring_test(), passing_checks());
    gh.remote.clone_from(&fixture.url);
    let gh = Arc::new(gh);
    let slack = SlackStub::default();
    let mut d = started(&env, on_slack(&gh, &slack, None));
    d.begin_issue("ACME-123", "Export", vec![]).unwrap();
    take_origin_away(&env);
    run_until(
        &mut d,
        60,
        |_| {},
        |_| failures(&env, "github-unreachable") >= 1,
    );
    assert_eq!(
        starts(&env, "github-unreachable")[0]["failure"]["via"],
        json!(["git"])
    );

    assert!(gh.viewer().is_ok(), "the API answers meanwhile");
    stays_open_until_it_answers(&env, &slack, &mut d, || bring_origin_back(&env));
}

// AC: GH-34 AC5 "... it shall clear once that probe answers." A refused probe is not an answer.
// Behavior: GitHub is out of reach by Git -> `origin` is changed to another repository, so the
//     probe is refused locally -> the open error stays open with no recovery notice.
#[test]
fn an_origin_changed_during_a_git_outage_does_not_clear_it() {
    let env = Env::new();
    env.configure("");
    env.script_agents(&Value::Object(support::scripts::straight("ACME-123")));
    let gh = passing_github(&env);
    let slack = SlackStub::default();
    let mut d = started(&env, on_slack(&gh, &slack, None));
    d.begin_issue("ACME-123", "Export", vec![]).unwrap();
    take_origin_away(&env);
    run_until(
        &mut d,
        60,
        |_| {},
        |_| failures(&env, "github-unreachable") >= 1,
    );
    let elsewhere = env.dir.path().join("elsewhere.git");
    git(
        env.dir.path(),
        &["init", "-q", "--bare", elsewhere.to_str().unwrap()],
    );
    git(
        &env.repo,
        &["remote", "set-url", "origin", elsewhere.to_str().unwrap()],
    );

    for _ in 0..3 {
        d.tick().unwrap();
    }

    assert_eq!(starts(&env, "github-unreachable").len(), 1);
    assert_eq!(cleared(&env, "github-unreachable"), 0);
    assert!(
        notices(&slack, "github-unreachable")
            .iter()
            .all(|n| !n.ends_with("/cleared")),
        "{:?}",
        notices(&slack, "github-unreachable")
    );
}

// AC: GH-34 AC5 "When a factory Git command fails while `origin` passes the check in AC4 and its
//     token-authenticated probe also fails, ... it shall be retried as `github-unreachable` with
//     `via: git`, and it shall clear once that probe answers." (docs/design/github-git-identity-design.md
//     § Tests, "AC5 push after a successful fetch")
// Behavior: the repository's `pre-push` hook takes the bare repository away and exits 1, so the
//     pass's pre-push fetch succeeds, Git aborts the push without sending anything, and the probe
//     after it fails -> the hook is removed and the repository brought back.
// @lane: integration
// @dependency: daemon, flow, stops (`probe_git`), real Git push and hook, FakeGitHub
#[test]
fn a_push_that_fails_after_its_fetch_succeeded_opens_github_unreachable_on_that_pass() {
    let env = Env::new();
    env.configure("");
    env.script_agents(&Value::Object(support::scripts::straight("ACME-123")));
    let hook = env.repo.join(".git/hooks/pre-push");
    let away = env.dir.path().join("origin-away.git");
    std::fs::write(
        &hook,
        format!(
            "#!/bin/sh\nmv '{}' '{}'\nexit 1\n",
            env.origin.display(),
            away.display()
        ),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&hook).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
    std::fs::set_permissions(&hook, permissions).unwrap();
    let gh = passing_github(&env);
    let slack = SlackStub::default();
    let mut d = started(&env, on_slack(&gh, &slack, None));
    d.begin_issue("ACME-123", "Export", vec![]).unwrap();

    run_until(&mut d, 60, |_| {}, |_| away.exists());

    let opened = starts(&env, "github-unreachable");
    assert_eq!(opened.len(), 1, "on the pass whose push failed: {opened:?}");
    assert_eq!(opened[0]["failure"]["via"], json!(["git"]));
    assert!(
        opened[0]["error"]
            .as_str()
            .unwrap()
            .starts_with("git push "),
        "{}",
        opened[0]["error"]
    );
    assert!(
        env.events().iter().all(|e| e["event"] != "push-failed"),
        "{:?}",
        env.events()
    );

    std::fs::remove_file(&hook).unwrap();
    bring_origin_back(&env);
    assert_eq!(issue_head_on_origin(&env), "", "nothing was pushed yet");
    run_until(
        &mut d,
        60,
        |_| {},
        |d| d.issue("ACME-123").unwrap().unwrap().pushed_sha.is_some(),
    );
    assert_eq!(
        issue_head_on_origin(&env),
        d.issue("ACME-123").unwrap().unwrap().pushed_sha.unwrap(),
        "the later pass published the issue branch"
    );
    for _ in 0..3 {
        d.tick().unwrap();
    }
    assert_eq!(starts(&env, "github-unreachable").len(), 1);
    assert_eq!(cleared(&env, "github-unreachable"), 1);
}

#[test]
fn github_stays_one_open_error_until_every_failing_way_answers_by_the_api() {
    let env = Env::new();
    env.configure("");
    env.script_agents(&Value::Object(support::scripts::straight("ACME-123")));
    let gh = passing_github(&env);
    let slack = SlackStub::default();
    let mut d = started(&env, on_slack(&gh, &slack, None));
    d.begin_issue("ACME-123", "Export", vec![]).unwrap();
    run_until(
        &mut d,
        60,
        |_| {},
        |d| d.issue("ACME-123").unwrap().unwrap().pr_url.is_some(),
    );
    gh.failing_pull_reads.store(true, Ordering::SeqCst);
    run_until(
        &mut d,
        60,
        |_| {},
        |_| failures(&env, "github-unreachable") >= 1,
    );
    let start = &starts(&env, "github-unreachable")[0];
    assert_eq!(start["failure"]["via"], json!(["api"]));
    assert_eq!(start["failure"]["cause"], "HTTP 502");

    assert!(gh.viewer().is_ok(), "the API answers meanwhile");
    stays_open_until_it_answers(&env, &slack, &mut d, || {
        gh.failing_pull_reads.store(false, Ordering::SeqCst);
    });
    run_until(
        &mut d,
        60,
        |_| {},
        |d| d.issue("ACME-123").unwrap().unwrap().flow.stage == Stage::Merged,
    );
}

// ---------------------------------------------------------------------------------------------
// 5. Upgrade from `services-unanswered.json` (Design § Migration Strategy).
// AC: "When the daemon starts with a `services-unanswered.json` from the previous version, Now
//     shall keep that service's 'since' and reason, and the next confirmed answer shall clear it
//     once." (Design § Acceptance Criteria > Error notices)
// Behavior: the state directory is seeded with `services-unanswered.json` holding GitHub failing
//     since a literal time T with reason no connection -> a daemon is built (recover imports) ->
//     before any poll, a second daemon is built over the same `Env` with the file put back (an
//     interruption before the delete) -> `origin` unreachable while `viewer` answers for a pass ->
//     `origin` reachable.
// Observable result:
//     - before any poll, `GET /api/v1/overview` `status.unreachable` is GitHub, `since` T, the
//       no-connection reason (the same JSON the overview returned before this change);
//     - `services-unanswered.json` is gone and `events.jsonl` has exactly one start line with
//       `failure.imported: true` and `failure.via` `["api", "git"]`;
//     - after the pass with `origin` unreachable, no `error-cleared`; after `origin` answers,
//       exactly one `error-cleared` and one `/cleared` message in the stub.
// @lane: integration
// @dependency: daemon `recover`, store, seeded `services-unanswered.json`, real git `origin`,
//     FakeGitHub, real `Slack` over `SlackStub`, dashboard server for `GET /overview`
// Name: an_outage_recorded_by_the_previous_version_keeps_its_since_and_clears_once
#[test]
fn an_outage_recorded_by_the_previous_version_keeps_its_since_and_clears_once() {
    let env = Env::new();
    env.configure("");
    let gh = passing_github(&env);
    let slack = SlackStub::default();
    let old = env.state().join("services-unanswered.json");
    let since = "2026-11-20T08:00:00Z";
    let seeded = json!({"github": {"since": since, "why": "no-connection"}}).to_string();
    std::fs::create_dir_all(env.state()).unwrap();
    std::fs::write(&old, &seeded).unwrap();

    let d = started(&env, on_slack(&gh, &slack, None));
    assert_eq!(
        unreachable_now(&d),
        json!({"service": "github", "since": since, "why": "no-connection"})
    );
    assert!(!old.exists());
    drop(d);
    // An import interrupted before the delete runs again.
    std::fs::write(&old, &seeded).unwrap();
    let mut d = started(&env, on_slack(&gh, &slack, None));
    assert!(!old.exists());
    let imported = starts(&env, "github-unreachable");
    assert_eq!(imported.len(), 1, "{imported:?}");
    assert_eq!(imported[0]["failure"]["imported"], true);
    assert_eq!(imported[0]["failure"]["via"], json!(["api", "git"]));
    assert_eq!(imported[0]["failure"]["since"], since);

    take_origin_away(&env);
    for _ in 0..3 {
        d.tick().unwrap();
    }
    assert_eq!(cleared(&env, "github-unreachable"), 0, "origin still fails");
    bring_origin_back(&env);
    run_until(
        &mut d,
        60,
        |_| {},
        |_| notices(&slack, "github-unreachable").len() == 2,
    );
    for _ in 0..3 {
        d.tick().unwrap();
    }
    assert_eq!(cleared(&env, "github-unreachable"), 1);
    assert_eq!(
        notices(&slack, "github-unreachable"),
        vec![
            format!("github-unreachable@{since}/started"),
            format!("github-unreachable@{since}/cleared"),
        ]
    );
    assert_eq!(unreachable_now(&d), Value::Null);
}

// ---------------------------------------------------------------------------------------------
// 6. Agent CLI errors (Design § Selected Design > Errors, `cli-*`; Transitions).
// AC: "When an error that pauses the factory starts (the agent CLI fails a run), the system shall
//     post one M-08 that names that error's cause. Its start line shall read 'You were told in
//     Slack.' once that M-08 is accepted. A restart before it is accepted shall still post it. If
//     the daemon stops after that start line is recorded and before the factory is paused, then
//     the restart shall pause the factory with the recorded reason before it launches anything."
//     and "While notices are off, … M-08 is unaffected." (Design § Acceptance Criteria > Error
//     notices)
// Cases (reported distinctly):
//     a. A launch exits with a fatal failure (`{"fail": "fatal"}`, as
//        `resilience_int.rs` `fatal_launch_failure_pauses_with_the_cause_until_resume`) while the
//        stub refuses -> the halt file exists -> restart -> the stub accepts -> `!resume` -> a later
//        launch of the same CLI succeeds. Result: exactly one M-08 in the stub, naming the
//        failure's cause and marked `cli-<cli>@<since>/started`; one `error-notice told` for that
//        start only; one `error-cleared` for `cli-<cli>` and one `/cleared` message.
//     b. Same failure with `[notices] errors = false`. Result: exactly one M-08; no `/cleared`
//        message after the later good launch.
//     c. `events.jsonl` seeded to end with a `launch-error` start line for `cli-claude` (with
//        `failure.reason`) and no `paused` line, and no halt file -> a daemon is built and ticked.
//        Result: the halt file exists before any launch is recorded in `launches.jsonl`; exactly
//        one M-08, naming the recorded reason.
//     d. A launch of one CLI fails while another of the same CLI, begun before that failure, still
//        runs and then ends well. Result: no `error-cleared` while paused; after `!resume` a launch
//        begun after the failure clears it: exactly one M-08, one `/cleared` message and one
//        `error-cleared`.
//     e. With notices off, a waiting M-08 Slack refuses is followed by an ordinary waiting
//        notice. Result: that notice is dropped in the same pass; once notices are on and Slack
//        accepts, only the M-08 is posted.
// Block 7 makes the spawn fail on its log, a directory, rather than on a program missing from
//     `PATH`: an agent CLI installed on the machine would otherwise run.
// @lane: integration
// @dependency: daemon (`finished`, `pause`, restart), store, halt file, scripted agent CLIs, real
//     `Slack` over `SlackStub`
// Name: an_agent_cli_error_pauses_once_and_is_told_by_one_m08
fn ok_requirements() -> Value {
    json!({"output": {"convergence": {"outcome": "o"}, "structuralScale": "medium", "decision": "proceed"}})
}

/// The M-08 messages in the channel, marked or not.
fn m08s(slack: &SlackStub) -> Vec<Value> {
    slack
        .posted()
        .into_iter()
        .filter(|m| m.to_string().contains("Oven Spring paused because"))
        .collect()
}

/// Start lines in `launches.jsonl` for items whose name contains `part`.
fn launches_of(env: &Env, part: &str) -> Vec<Value> {
    std::fs::read_to_string(env.state().join("launches.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .filter(|l| l.get("pid").is_some() && l["item"].as_str().unwrap().contains(part))
        .collect()
}

fn paused_lines(env: &Env) -> Vec<Value> {
    env.events()
        .into_iter()
        .filter(|e| e["event"] == "paused")
        .collect()
}

/// The error key and `since` of the one agent CLI error started so far.
fn cli_start(env: &Env) -> (String, String) {
    let starts: Vec<Value> = env
        .events()
        .into_iter()
        .filter(|e| e["event"] == "launch-error" && e["failure"]["start"] == true)
        .collect();
    assert_eq!(starts.len(), 1, "one agent CLI error started");
    let failure = &starts[0]["failure"];
    (
        failure["key"].as_str().unwrap().to_owned(),
        failure["since"].as_str().unwrap().to_owned(),
    )
}

const REASON: &str =
    "the agent on ACME-123 (requirements) stopped with an error: Claude AI usage limit reached";

/// `events.jsonl` as an earlier run left it: ACME-123 begun, then a `cli-claude` start line
/// recorded and the daemon stopped before the factory was paused.
fn seed_unpaused_cli_start(env: &Env, slack: &SlackStub) -> String {
    let gh = passing_github(env);
    started(env, on_slack(&gh, slack, None))
        .begin_issue("ACME-123", "Export", vec![])
        .unwrap();
    let since = jiff::Timestamp::now().to_string();
    let store = ovenspring_store::Store::open(env.state()).unwrap();
    store
        .append_event(&json!({
            "event": "launch-error", "item": "requirements-ACME-123", "cli": "claude",
            "error": "Claude AI usage limit reached", "at": since,
            "failure": {"key": "cli-claude", "since": since, "cause": "Claude AI usage limit reached",
                        "start": true, "notice": "pause", "reason": REASON},
        }))
        .unwrap();
    since
}

#[test]
fn an_agent_cli_error_pauses_once_and_is_told_by_one_m08_across_a_refusal_and_a_restart() {
    let env = Env::new();
    env.configure("");
    env.script_agents(&json!({
        "requirements@ACME-123": [{"fail": "fatal"}, ok_requirements()],
        "codebase-analysis": [{"hang": true}],
    }));
    let gh = passing_github(&env);
    let slack = SlackStub::default();
    slack.fail("chat.postMessage", true);
    let mut d = started(&env, on_slack(&gh, &slack, None));
    d.begin_issue("ACME-123", "Export", vec![]).unwrap();
    run_until(&mut d, 30, |_| {}, |d| d.halt_file().exists());
    for _ in 0..3 {
        d.tick().unwrap();
    }
    let (key, since) = cli_start(&env);
    assert!(m08s(&slack).is_empty(), "Slack refuses every post");
    assert_eq!(told(&env, &key, "started"), 0);
    drop(d);

    slack.fail("chat.postMessage", false);
    let mut d = started(&env, on_slack(&gh, &slack, None));
    run_until(&mut d, 30, |_| {}, |_| told(&env, &key, "started") == 1);
    let m08 = m08s(&slack);
    assert_eq!(m08.len(), 1, "{m08:?}");
    assert_eq!(
        m08[0]["metadata"]["event_payload"]["id"],
        format!("{key}@{since}/started")
    );
    assert!(m08[0].to_string().contains(REASON), "{}", m08[0]);

    d.resume().unwrap();
    run_until(&mut d, 30, |_| {}, |_| told(&env, &key, "cleared") == 1);
    assert_eq!(
        notices(&slack, &key),
        vec![
            format!("{key}@{since}/started"),
            format!("{key}@{since}/cleared")
        ]
    );
    assert_eq!(m08s(&slack).len(), 1);
    assert_eq!(told(&env, &key, "started"), 1);
    assert_eq!(cleared(&env, &key), 1);
    assert_eq!(paused_lines(&env).len(), 1);
}

#[test]
fn an_agent_cli_error_whose_text_holds_a_credential_pauses_once_with_the_masked_reason() {
    let env = Env::new();
    env.configure("");
    env.script_agents(&json!({
        "requirements@ACME-123": [{"fail": "fatal"}, ok_requirements()],
        "codebase-analysis": [{"hang": true}],
    }));
    let gh = passing_github(&env);
    let slack = SlackStub::default();
    let mut services = on_slack(&gh, &slack, None);
    // A held credential that the fake CLI's error text ("Claude AI usage limit reached") contains.
    services.secrets = vec!["usage limit".to_owned()];
    let mut d = started(&env, services);
    d.begin_issue("ACME-123", "Export", vec![]).unwrap();
    run_until(&mut d, 30, |_| {}, |d| d.halt_file().exists());
    for _ in 0..2 {
        d.tick().unwrap();
    }

    let starts: Vec<Value> = env
        .events()
        .into_iter()
        .filter(|e| e["event"] == "launch-error" && e["failure"]["start"] == true)
        .collect();
    assert_eq!(starts.len(), 1);
    let paused = paused_lines(&env);
    assert_eq!(paused.len(), 1, "{paused:?}");
    assert_eq!(paused[0]["reason"], starts[0]["failure"]["reason"]);
    let halt = std::fs::read_to_string(d.halt_file()).unwrap();
    assert!(!halt.contains("usage limit"), "{halt}");
    assert!(
        !paused[0].to_string().contains("usage limit"),
        "{}",
        paused[0]
    );
}

#[test]
fn an_agent_cli_error_pauses_once_and_is_told_by_one_m08_while_error_notices_are_off() {
    let env = Env::new();
    env.configure("[notices]\nerrors = false\n");
    env.script_agents(&json!({
        "requirements@ACME-123": [{"fail": "fatal"}, ok_requirements()],
        "codebase-analysis": [{"hang": true}],
    }));
    let gh = passing_github(&env);
    let slack = SlackStub::default();
    let mut d = started(&env, on_slack(&gh, &slack, None));
    d.begin_issue("ACME-123", "Export", vec![]).unwrap();
    run_until(&mut d, 30, |_| {}, |d| d.halt_file().exists());
    let (key, since) = cli_start(&env);
    run_until(&mut d, 30, |_| {}, |_| told(&env, &key, "started") == 1);

    d.resume().unwrap();
    run_until(&mut d, 30, |_| {}, |_| cleared(&env, &key) == 1);
    for _ in 0..3 {
        d.tick().unwrap();
    }
    assert_eq!(
        notices(&slack, &key),
        vec![format!("{key}@{since}/started")]
    );
    assert_eq!(m08s(&slack).len(), 1);
    let dropped = env
        .events()
        .into_iter()
        .filter(|e| e["event"] == "error-notice" && e["outcome"] == "dropped")
        .count();
    assert_eq!(
        dropped, 0,
        "an M-08 is never dropped, and the clear waits for no notice"
    );
}

/// With notices off, an M-08 Slack refuses holds back no ordinary notice behind it: that one is
/// dropped in the same pass, so once notices are on and Slack accepts, only the M-08 goes out.
#[test]
fn an_agent_cli_error_pauses_once_and_is_told_by_one_m08_while_notices_are_off_and_slack_refuses_it()
 {
    let env = Env::new();
    env.configure("[notices]\nerrors = false\n");
    env.script_agents(&json!({"codebase-analysis": [{"hang": true}]}));
    let slack = SlackStub::default();
    let since = seed_unpaused_cli_start(&env, &slack);
    let store = ovenspring_store::Store::open(env.state()).unwrap();
    store
        .append_event(&json!({
            "event": "service-unreachable", "why": "no-connection", "at": since,
            "failure": {"key": "tracker-unreachable", "since": since, "cause": "no connection",
                        "start": true, "notice": "waiting"},
        }))
        .unwrap();
    let gh = passing_github(&env);
    let tracker = Arc::new(RecordingTracker::default());
    tracker.unreachable.store(true, Ordering::SeqCst);
    slack.fail("chat.postMessage", true);
    let mut d = started(&env, on_slack(&gh, &slack, Some(tracker)));

    d.tick().unwrap();
    let dropped: Vec<Value> = env
        .events()
        .into_iter()
        .filter(of_key("tracker-unreachable"))
        .filter(|e| e["event"] == "error-notice" && e["outcome"] == "dropped")
        .collect();
    assert_eq!(dropped.len(), 1, "{dropped:?}");
    assert_eq!(told(&env, "cli-claude", "started"), 0);

    env.configure("");
    slack.fail("chat.postMessage", false);
    run_until(
        &mut d,
        30,
        |_| {},
        |_| told(&env, "cli-claude", "started") == 1,
    );
    for _ in 0..3 {
        d.tick().unwrap();
    }
    assert_eq!(
        notices(&slack, "cli-claude"),
        vec![format!("cli-claude@{since}/started")]
    );
    assert_eq!(notices(&slack, "tracker-unreachable"), Vec::<String>::new());
    assert_eq!(m08s(&slack).len(), 1);
}

#[test]
fn an_agent_cli_error_pauses_once_and_is_told_by_one_m08_when_the_daemon_stopped_before_pausing() {
    let env = Env::new();
    env.configure("");
    env.script_agents(
        &json!({"requirements": [ok_requirements()], "codebase-analysis": [{"hang": true}]}),
    );
    let slack = SlackStub::default();
    let since = seed_unpaused_cli_start(&env, &slack);
    let gh = passing_github(&env);
    let mut d = started(&env, on_slack(&gh, &slack, None));

    d.tick().unwrap();
    assert!(d.halt_file().exists());
    assert!(
        launches_of(&env, "ACME-123").is_empty(),
        "nothing launches under the pause"
    );
    run_until(
        &mut d,
        30,
        |_| {},
        |_| told(&env, "cli-claude", "started") == 1,
    );
    let m08 = m08s(&slack);
    assert_eq!(m08.len(), 1, "{m08:?}");
    assert_eq!(
        m08[0]["metadata"]["event_payload"]["id"],
        format!("cli-claude@{since}/started")
    );
    assert!(m08[0].to_string().contains(REASON), "{}", m08[0]);
    let paused = paused_lines(&env);
    assert_eq!(paused.len(), 1);
    assert_eq!(paused[0]["reason"], REASON);

    // What the pause held back launches once the owner resumes.
    d.resume().unwrap();
    run_until(
        &mut d,
        30,
        |_| {},
        |_| !launches_of(&env, "ACME-123").is_empty(),
    );
}

#[test]
fn an_agent_cli_error_pauses_once_and_stays_open_when_a_launch_of_its_cli_begun_before_it_ends_well()
 {
    let env = Env::new();
    env.configure("");
    env.script_agents(&json!({
        "prepare:requirements@ACME-123": [{"fail": "fatal"}, {"output": {"message": null, "question": false}}],
        "prepare:requirements@ACME-124": [{"wait": true, "output": {"message": null, "question": false}}],
        "codebase-analysis": [{"hang": true}],
    }));
    let gh = passing_github(&env);
    let slack = SlackStub::default();
    let mut d = started(&env, on_slack(&gh, &slack, None));
    d.begin_issue("ACME-123", "Export", vec![]).unwrap();
    d.begin_issue("ACME-124", "Import", vec![]).unwrap();
    d.tick().unwrap();
    assert_eq!(
        launches_of(&env, "ACME-123").len(),
        1,
        "both issues' steps launch in one pass"
    );
    assert_eq!(launches_of(&env, "ACME-124").len(), 1);

    // ACME-123's launch fails while ACME-124's, begun before that failure, still runs.
    run_until(&mut d, 30, |_| {}, |d| d.halt_file().exists());
    let (key, since) = cli_start(&env);
    let ended = |item: &str| {
        std::fs::read_to_string(env.state().join("launches.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str::<Value>(l).unwrap())
            .any(|l| l["ended"] == true && l["item"].as_str().unwrap().contains(item))
    };
    assert!(!ended("ACME-124"));
    let fake = env.fake_bin.parent().unwrap().to_path_buf();
    std::fs::write(fake.join("release-ACME-124-requirements"), "").unwrap();
    run_until(&mut d, 30, |_| {}, |_| ended("ACME-124"));
    for _ in 0..3 {
        d.tick().unwrap();
    }
    assert_eq!(
        cleared(&env, &key),
        0,
        "a launch begun before the failure clears nothing"
    );
    assert_eq!(told(&env, &key, "started"), 1);
    assert_eq!(
        launches_of(&env, "ACME-").len(),
        2,
        "nothing launches under the pause"
    );

    // A launch of that CLI begun after the failure clears it, once.
    d.resume().unwrap();
    run_until(&mut d, 30, |_| {}, |_| told(&env, &key, "cleared") == 1);
    for _ in 0..3 {
        d.tick().unwrap();
    }
    assert_eq!(m08s(&slack).len(), 1);
    assert_eq!(
        notices(&slack, &key),
        vec![
            format!("{key}@{since}/started"),
            format!("{key}@{since}/cleared")
        ]
    );
    assert_eq!(cleared(&env, &key), 1);
}

// ---------------------------------------------------------------------------------------------
// 7. A CLI that cannot be spawned is not an error (Design § Selected Design > Errors, CLI spawn).
// AC: "When the agent CLI cannot be spawned, the system shall launch the step again on the next
//     pass and record no error line for it. If it fails on two passes in a row, then the system
//     shall stop the issue or sprint that owns the item and file `exception`, or pause the factory
//     with M-08 when no issue or sprint owns it, as today." (Design § Acceptance Criteria > Error
//     notices)
// Cases (reported distinctly): the configured CLI program does not exist on `PATH`
//     a. for an issue's item: after the first failed pass the item is launched again; after the
//        second, the issue is stopped with an `exception` item and no halt file exists;
//     b. for an item no issue or sprint owns: the second failed pass writes the halt file and the
//        stub holds exactly one M-08 (from `announce_pause`).
// Observable result (both): `events.jsonl` has no line with a `failure` field.
// @lane: integration
// @dependency: daemon (`start_process`, `settle`, `settle_part`), store, real process spawn,
//     real `Slack` over `SlackStub`
// Name: a_cli_that_cannot_be_spawned_is_retried_and_records_no_error
/// An open `requirements` item for ACME-123 (`issue`) or for nothing, whose launches cannot be
/// spawned: the log each of its first attempts writes to is a directory. A program missing from
/// `PATH` fails the same spawn, but an agent CLI installed on the machine would then run.
fn unspawnable_item(env: &Env, issue: Option<&str>) {
    let store = ovenspring_store::Store::open(env.state()).unwrap();
    let name = "requirements-unspawnable";
    if let Some(key) = issue {
        let mut r = ovenspring_daemon::issues::IssueRecord::new(key, "Export", vec![]);
        r.flow.stage = Stage::running(ovenspring_core::step::Step::Requirements);
        store
            .write_record(&format!("issues/{key}.json"), &r)
            .unwrap();
    }
    let item = support::item(
        &store,
        name,
        ovenspring_core::step::Step::Requirements,
        issue,
    );
    store.file_item(name, &item).unwrap();
    for attempt in 1..=3 {
        std::fs::create_dir_all(env.state().join(format!("launches/{name}.{attempt}.log")))
            .unwrap();
    }
}

fn launch_failures(env: &Env) -> Vec<Value> {
    env.events()
        .into_iter()
        .filter(|e| e["event"] == "operation-failed")
        .filter(|e| {
            e["error"]
                .as_str()
                .unwrap_or_default()
                .starts_with("starting ")
        })
        .collect()
}

fn failure_lines(env: &Env) -> usize {
    env.events()
        .into_iter()
        .filter(|e| e.get("failure").is_some())
        .count()
}

#[test]
fn a_cli_that_cannot_be_spawned_is_retried_and_records_no_error_for_an_issues_item() {
    let env = Env::new();
    env.configure("");
    let gh = passing_github(&env);
    let slack = SlackStub::default();
    let mut d = started(&env, on_slack(&gh, &slack, None));
    unspawnable_item(&env, Some("ACME-123"));

    d.tick().unwrap();
    assert_eq!(launch_failures(&env).len(), 1);
    assert!(d.issue("ACME-123").unwrap().unwrap().stopped.is_none());

    d.tick().unwrap();
    let failed = launch_failures(&env);
    assert_eq!(failed.len(), 2, "the next pass launches the step again");
    assert_eq!(failed[1]["issue"], "ACME-123");
    assert!(d.issue("ACME-123").unwrap().unwrap().stopped.is_some());
    let exception = env
        .events()
        .into_iter()
        .filter(|e| e["event"] == "filed" && e["issue"] == "ACME-123" && e["step"] == "exception")
        .count();
    assert_eq!(exception, 1);
    assert!(!d.halt_file().exists());
    assert_eq!(failure_lines(&env), 0);
}

#[test]
fn a_cli_that_cannot_be_spawned_is_retried_and_records_no_error_for_an_item_nothing_owns() {
    let env = Env::new();
    env.configure("");
    let gh = passing_github(&env);
    let slack = SlackStub::default();
    let mut d = started(&env, on_slack(&gh, &slack, None));
    unspawnable_item(&env, None);

    d.tick().unwrap();
    assert_eq!(launch_failures(&env).len(), 1);
    assert!(!d.halt_file().exists());
    for _ in 0..3 {
        d.tick().unwrap();
    }
    assert_eq!(launch_failures(&env).len(), 2);
    assert!(d.halt_file().exists());
    let m08 = m08s(&slack);
    assert_eq!(m08.len(), 1, "{m08:?}");
    assert!(
        m08[0]["metadata"].is_null(),
        "announced as any pause: {}",
        m08[0]
    );
    assert_eq!(failure_lines(&env), 0);
}

// ---------------------------------------------------------------------------------------------
// 8. Filter error lifecycle (Design § Selected Design > Errors, `filter`; PRD AC-072, AC-077).
// AC: "A filter failure is such an error." and "When the filter judges an issue freshly (admitted
//     or dropped) while the filter's error is open, the system shall clear it once, naming that
//     issue. A verdict reapplied from the cache shall not clear it." (Design § Acceptance Criteria
//     > Error notices)
// Behavior: a filter script admits ACME-1 -> the script is changed to exit 1 with a stderr line ->
//     ACME-2 and ACME-3 arrive and fail -> several intake passes reapply ACME-1's cached verdict ->
//     the script is changed to admit -> ACME-4 arrives and is judged freshly.
// Observable result:
//     - after ACME-2 and ACME-3: exactly one start line for `filter` (issue ACME-2, `command`, and
//       `output` = the stderr line) and one repeat line (ACME-3); one `/started` message in the
//       stub;
//     - after the cached passes: no `error-cleared`;
//     - after ACME-4: exactly one `error-cleared` with `issue: "ACME-4"` and one `/cleared` message.
// @lane: integration
// @dependency: daemon intake (`judge`, `take_filter_run`, `intake_pass`), real `sh -c` filter,
//     RecordingTracker, store, real `Slack` over `SlackStub`
// Name: a_failing_filter_is_one_error_until_an_issue_is_judged_freshly
// The clear that follows the fresh judgment is proved twice: appended at once, and appended by a
//     later pass after its first append failed (`events.jsonl` read-only for that pass), when the
//     fresh verdict is cached and the command never runs again.
#[test]
fn a_failing_filter_is_one_error_until_an_issue_is_judged_freshly() {
    a_failing_filter_is_one_error_until(false);
}

#[test]
fn a_failing_filter_is_one_error_until_an_issue_is_judged_freshly_even_when_the_clear_append_fails()
{
    a_failing_filter_is_one_error_until(true);
}

const ADMIT: &str = "cat >/dev/null\nexit 0\n";
const FAIL: &str = "cat >/dev/null\necho 'token expired' >&2\nexit 1\n";
/// Admits once the test creates `.ovenspring/gate`, after telling it has started.
const GATED_ADMIT: &str = "cat >/dev/null\ntouch .ovenspring/running\nwaited=0; while [ ! -f .ovenspring/gate ]; do waited=$((waited + 1)); [ \"$waited\" -le 3000 ] || exit 1; sleep 0.01; done\nexit 0\n";
const FILTER_COMMAND: &str = "exec sh .ovenspring/filter.sh";

fn tracker_issue(key: &str) -> TrackerIssue {
    TrackerIssue {
        key: key.into(),
        title: format!("Title of {key}"),
        body: format!("Body of {key}"),
        url: format!("https://linear.app/acme/issue/{key}"),
        state: "Todo".into(),
        ..TrackerIssue::default()
    }
}

fn reports(tracker: &RecordingTracker, keys: &[&str]) {
    *tracker.issues.lock().unwrap() = keys.iter().map(|k| tracker_issue(k)).collect();
}

/// The filter's start and repeat lines.
fn filter_lines(env: &Env) -> Vec<Value> {
    env.events()
        .into_iter()
        .filter(|e| e["event"] == "filter-failed")
        .collect()
}

fn cleared_lines(env: &Env, key: &str) -> Vec<Value> {
    env.events()
        .into_iter()
        .filter(of_key(key))
        .filter(|e| e["event"] == "error-cleared")
        .collect()
}

/// A daemon over Slack whose filter command fails on ACME-1, with that error's M-13 told.
fn failing_filter(env: &Env, slack: &SlackStub) -> Daemon {
    let command = env.write_filter_script(FAIL);
    env.configure_filter(&command);
    let tracker = Arc::new(RecordingTracker::default());
    reports(&tracker, &["ACME-1"]);
    let gh = passing_github(env);
    let mut d = started(env, on_slack(&gh, slack, Some(tracker)));
    run_until(&mut d, 10, |_| {}, |_| told(env, "filter", "started") == 1);
    d
}

fn a_failing_filter_is_one_error_until(clear_append_fails: bool) {
    let env = Env::new();
    let command = env.write_filter_script(ADMIT);
    env.configure_filter(&command);
    let tracker = Arc::new(RecordingTracker::default());
    reports(&tracker, &["ACME-1"]);
    let gh = passing_github(&env);
    let slack = SlackStub::default();
    let mut d = started(&env, on_slack(&gh, &slack, Some(tracker.clone())));
    run_until(
        &mut d,
        10,
        |_| {},
        |d| d.open_issues().iter().any(|i| i.key == "ACME-1"),
    );

    // ACME-2 and ACME-3 fail.
    env.write_filter_script(FAIL);
    reports(&tracker, &["ACME-1", "ACME-2"]);
    run_until(&mut d, 10, |_| {}, |_| failures(&env, "filter") == 1);
    reports(&tracker, &["ACME-1", "ACME-2", "ACME-3"]);
    run_until(&mut d, 10, |_| {}, |_| failures(&env, "filter") == 2);
    run_until(&mut d, 10, |_| {}, |_| told(&env, "filter", "started") == 1);
    let since = one_start_and_one_repeat(&env);
    assert_eq!(
        notices(&slack, "filter"),
        [format!("filter@{since}/started")]
    );

    // ACME-1's admission is reapplied from the cache on every pass.
    for _ in 0..5 {
        d.tick().unwrap();
    }
    assert_eq!(cleared(&env, "filter"), 0);
    assert_eq!(failures(&env, "filter"), 2);

    // ACME-4 is judged freshly.
    env.write_filter_script(GATED_ADMIT);
    reports(&tracker, &["ACME-1", "ACME-2", "ACME-3", "ACME-4"]);
    run_until(&mut d, 10, |_| {}, |_| env.filter_file("running").exists());
    let events = env.state().join("events.jsonl");
    if clear_append_fails {
        set_read_only(&events, true);
        std::fs::write(env.filter_file("gate"), "go").unwrap();
        assert!(
            support::wait_until(10, || d.tick().is_err()),
            "the pass taking the fresh judgment fails on its append"
        );
        set_read_only(&events, false);
        assert_eq!(cleared(&env, "filter"), 0);
    } else {
        std::fs::write(env.filter_file("gate"), "go").unwrap();
    }
    run_until(&mut d, 10, |_| {}, |_| told(&env, "filter", "cleared") == 1);
    for _ in 0..3 {
        d.tick().unwrap();
    }

    let clears = cleared_lines(&env, "filter");
    assert_eq!(clears.len(), 1, "{clears:?}");
    assert_eq!(
        (
            &clears[0]["issue"],
            &clears[0]["count"],
            &clears[0]["removed"]
        ),
        (&json!("ACME-4"), &json!(2), &Value::Null)
    );
    assert_eq!(
        notices(&slack, "filter"),
        [
            format!("filter@{since}/started"),
            format!("filter@{since}/cleared")
        ]
    );
}

/// The filter's lines after ACME-2 and ACME-3 failed: ACME-2's start and ACME-3's repeat, with
/// the command, stderr's line and the cause. Returns the error's `since`.
fn one_start_and_one_repeat(env: &Env) -> String {
    let lines = filter_lines(env);
    let since = lines[0]["failure"]["since"].as_str().unwrap().to_owned();
    let shape = |line: &Value| {
        json!([
            line["event"],
            line["issue"],
            line["error"],
            line["command"],
            line["output"],
            line["failure"]["cause"],
            line["failure"]["start"]
        ])
    };
    assert_eq!(
        shape(&lines[0]),
        json!([
            "filter-failed",
            "ACME-2",
            "exited with status 1",
            FILTER_COMMAND,
            "token expired",
            "exited with status 1 (token expired)",
            true
        ])
    );
    assert_eq!(
        shape(&lines[1]),
        json!([
            "filter-failed",
            "ACME-3",
            "exited with status 1",
            FILTER_COMMAND,
            "token expired",
            "exited with status 1 (token expired)",
            null
        ])
    );
    assert_eq!(lines[1]["failure"]["since"], since.as_str());
    since
}

fn set_read_only(path: &std::path::Path, read_only: bool) {
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    permissions.set_readonly(read_only);
    std::fs::set_permissions(path, permissions).unwrap();
}

// ---------------------------------------------------------------------------------------------
// 9.
// AC: "When the filter command is absent while the filter's error is open, whether the daemon sees
//     it on reload or at start, the system shall clear the error once with 'The issue filter was
//     removed.'" (Design § Acceptance Criteria > Error notices)
// Cases (reported distinctly):
//     a. on reload: a failing filter opens the error -> `Env::set_tracker_filter(None)` -> passes.
//     b. at start: a failing filter opens the error -> the daemon is dropped -> the filter is
//        removed from `config.toml` -> a new daemon is built and ticked.
// Observable result (each): exactly one `error-cleared` for `filter` with `removed: true`, after
//     any number of passes.
// @lane: integration
// @dependency: daemon (`reload_config`, `clear_removed_filter`, `recover`), real `sh -c` filter,
//     store
// Name: removing_the_filter_clears_its_error_once
#[test]
fn removing_the_filter_clears_its_error_once_on_reload() {
    let env = Env::new();
    let slack = SlackStub::default();
    let mut d = failing_filter(&env, &slack);

    env.set_tracker_filter(None);
    for _ in 0..5 {
        d.tick().unwrap();
    }

    assert_removed_once(&env);
}

#[test]
fn removing_the_filter_clears_its_error_once_at_start() {
    let env = Env::new();
    let slack = SlackStub::default();
    let d = failing_filter(&env, &slack);
    drop(d);

    env.set_tracker_filter(None);
    let gh = passing_github(&env);
    let tracker = Arc::new(RecordingTracker::default());
    reports(&tracker, &["ACME-1"]);
    let mut d = started(&env, on_slack(&gh, &slack, Some(tracker)));
    for _ in 0..5 {
        d.tick().unwrap();
    }

    assert_removed_once(&env);
}

fn assert_removed_once(env: &Env) {
    let clears = cleared_lines(env, "filter");
    assert_eq!(clears.len(), 1, "{clears:?}");
    assert_eq!(
        (&clears[0]["removed"], &clears[0]["issue"]),
        (&json!(true), &Value::Null)
    );
}

// ---------------------------------------------------------------------------------------------
// 10. Masking (Design § Security Considerations; § Selected Design > Filter output).
// AC: "No held credential and no value in a common token format shall appear in a new event field,
//     the endpoint's response or a Slack error notice, including when the 300-byte cut of the
//     filter's stderr would fall inside a credential." (Design § Acceptance Criteria > Events page;
//     PRD AC-047)
// Cases (reported distinctly):
//     a. a held credential (planted in the credentials file) is printed on the filter's stderr at
//        a position where the 300-byte cut falls inside it -> the filter error starts and its M-13
//        is posted. Result: no substring of 8 or more of the credential's characters appears in
//        `events.jsonl`, in the `GET /api/v1/events` response body, or in any stub
//        `chat.postMessage` payload.
//     b. the same credential printed by a failing command while the sprint is Proposing (as
//        `intake_int.rs` `credentials_remain_masked_when_a_drop_waits_for_the_sprint_proposal_to_end`
//        sets it up), then replaced in the credentials file before the proposal ends. Result: the
//        `filter-failed` line written after the proposal does not contain it.
// @lane: integration
// @dependency: daemon intake, real `sh -c` filter, credentials file, store, dashboard server
//     (`GET /events`), real `Slack` over `SlackStub`
// Name: a_credential_in_the_filters_output_never_reaches_events_the_page_or_slack
#[test]
fn a_credential_in_the_filters_output_never_reaches_events_the_page_or_slack_across_the_cut() {
    let env = Env::new();
    let dash = Dashboard::start(&env);
    dash.hold(CREDENTIAL);
    // The 300-byte cut falls 10 bytes into the credential.
    let pad = "x".repeat(290);
    let command = env.write_filter_script(&format!(
        "cat >/dev/null\necho '{pad}{CREDENTIAL} tail' >&2\nexit 1\n"
    ));
    env.configure_filter(&command);
    let tracker = Arc::new(RecordingTracker::default());
    reports(&tracker, &["ACME-1"]);
    let gh = passing_github(&env);
    let slack = SlackStub::default();
    let mut services = on_slack(&gh, &slack, Some(tracker));
    services.secrets = vec![CREDENTIAL.to_owned()];
    let mut d = started(&env, services);

    run_until(&mut d, 10, |_| {}, |_| told(&env, "filter", "started") == 1);

    let start = &filter_lines(&env)[0];
    assert_eq!(start["output"], format!("{pad}[masked] t"));
    let raw = std::fs::read_to_string(env.state().join("events.jsonl")).unwrap();
    let page = dash.get("/api/v1/events?show=filter");
    assert_eq!(page.status, 200);
    assert_eq!(
        page.json()["lines"][0]["kind"],
        "filter-failed",
        "{}",
        page.body
    );
    let posted = Value::Array(slack.calls("chat.postMessage")).to_string();
    for (place, text) in [
        ("events.jsonl", &raw),
        ("GET /events", &page.body),
        ("Slack", &posted),
    ] {
        assert_no_part_of_credential(place, text);
    }
}

#[test]
fn a_credential_in_the_filters_output_never_reaches_events_once_replaced_while_the_sprint_is_proposing()
 {
    let env = Env::new();
    let replaced = "held-replacement-55c1e0d2";
    let command = env.write_filter_script(&format!(
        "cat >/dev/null\nn=$(cat .ovenspring/count 2>/dev/null || echo 0)\nn=$((n + 1))\nprintf '%s\\n' \"$n\" > .ovenspring/count\nif [ \"$n\" -eq 1 ]; then echo 'before {CREDENTIAL} after' >&2; exit 1; fi\nexit 0\n"
    ));
    env.configure_filter(&command);
    let tracker = Arc::new(RecordingTracker::default());
    reports(&tracker, &["ACME-131"]);
    let channel = Arc::new(RecordingChannel::default());
    let services = |secret: &str| ovenspring_daemon::Services {
        github: None,
        channel: channel.clone(),
        tracker: Some(support::linear(tracker.clone())),
        secrets: vec![secret.to_owned()],
    };
    let mut d = daemon(&env, services(CREDENTIAL));
    let proposal = set_proposing(&d);

    d.tick().unwrap();
    // One run at a time: ACME-132's run starting shows ACME-131's verdict was taken.
    reports(&tracker, &["ACME-131", "ACME-132"]);
    run_until(
        &mut d,
        10,
        |_| assert_eq!(failures(&env, "filter"), 0, "held while Proposing"),
        |_| std::fs::read_to_string(env.filter_file("count")).is_ok_and(|n| n.trim() == "2"),
    );
    d.replace_services(services(replaced));
    complete_sprint_plan(&mut d, &proposal);

    let lines = filter_lines(&env);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(
        (&lines[0]["issue"], &lines[0]["output"]),
        (&json!("ACME-131"), &json!("before [masked] after"))
    );
    let raw = std::fs::read_to_string(env.state().join("events.jsonl")).unwrap();
    assert_no_part_of_credential("events.jsonl", &raw);
}

/// A credential the factory holds, printed by the filter.
const CREDENTIAL: &str = "Qx7vLm3Kp0Zt9d41b7e2WcRj";

/// No run of 8 or more of the credential's characters is in `text`.
fn assert_no_part_of_credential(place: &str, text: &str) {
    for part in CREDENTIAL.as_bytes().windows(8) {
        let part = std::str::from_utf8(part).unwrap();
        assert!(!text.contains(part), "{place} holds {part:?}: {text}");
    }
}

/// A sprint waiting for its plan, with the plan's item running.
fn set_proposing(d: &Daemon) -> std::path::PathBuf {
    let (mut sprint, _) = ovenspring_core::sprint::begin("filter-masking");
    sprint.stage = SprintStage::Proposing;
    d.store().write_record("sprint.json", &sprint).unwrap();
    let name = format!("sprint-plan-{}", sprint.id);
    let mut item = support::item(d.store(), &name, Step::SprintPlan, None);
    item.front.sprint = Some(sprint.id);
    item.front.status = Status::Doing;
    d.store().file_item(&name, &item).unwrap()
}

/// Ends Proposing with a plan of no issue.
fn complete_sprint_plan(d: &mut Daemon, path: &std::path::Path) {
    let plan = SprintPlanOutput {
        purpose: "Nothing yet".into(),
        key_results: vec![],
        issues: vec![],
        message: "No issue is ready.".into(),
    };
    let item = d.store().read_item(path).unwrap();
    std::fs::write(&item.front.out, serde_json::to_vec(&plan).unwrap()).unwrap();
    d.store()
        .update_item(path, |item| item.front.status = Status::Done)
        .unwrap();
    d.tick().unwrap();
}

// ---------------------------------------------------------------------------------------------
// 11. Events read (Design § Selected Design > Events read; § Data Contracts > GET /api/v1/events).
// AC: the Events page criteria (Design § Acceptance Criteria > Events page; PRD AC-076):
//     newest 15 matching lines newest first with time, kind and subject; attribution of `item`
//     lines through `launches.jsonl` and of an `issue` naming only a sprint record; drop and filter
//     Error lines; folding of a repeat with the same issue and cause, an Error repeat line
//     otherwise; "Show older events" adds the next 50; `recorded` tells "No event matches" from
//     "Nothing has been recorded yet."; masking of served strings.
// Behavior: a state directory seeded (as `dashboard_int.rs` seeds records) with issue records,
//     a current sprint and an ended sprint, `launches.jsonl`, and an `events.jsonl` written as
//     literal lines, including: lines naming `issue`, a `sprint` id, `sprint: true`, a sprint id in
//     `issue` (`github-error`), and only an `item`; a `filter-dropped` and a `filter` start, a
//     folded repeat and an unfolded repeat (other issue), an `error-cleared`; `error-notice` lines
//     (`told`, `dropped`); a start with `notice: "off"`; a kind with no sentence; a `replay-reset`
//     line written with other JSON spacing; a torn last line; a held credential inside an old
//     line's string; more than 65 lines -> the server (`ovenspring_server::Server` over the
//     `Context`, as `dashboard_int.rs` builds it) is requested over HTTP.
// Cases (reported distinctly; each compared with literal expected JSON):
//     a. `GET /api/v1/events`: 15 lines newest first with `seq`, `kind`, masked `fields`,
//        `issue`/`sprint` subjects as Design § Events read step 2 orders them, `error` roles
//        (`started` with `times`/`last`/`open`/`told`, `repeat` with `startedAt`, `cleared` with
//        `removed`), told states `told {at}` / `waiting` / `off`; no `error-notice` line; the
//        unknown kind listed; `older` set; `errorsNow` holding the open errors oldest first;
//        `notices`; `issues` sorted with titles.
//     b. `?from=<older>`: every matching line with `seq ≥ from`, `older` for the next 50, then null.
//     c. `?show=errors`, `?show=filter`, `?issue=ACME-2`: only the lines Design step 5 keeps.
//     d. `?issue=<a key with no line>`: `lines` empty, `recorded: true`; a state directory with no
//        `events.jsonl`: `recorded: false`.
//     e. an open `github-unreachable` start seeded: `errorsNow[0].since` equals
//        `GET /api/v1/overview` `status.unreachable` `since` for GitHub.
//     f. `events.jsonl` present but unreadable (a directory at its path, or no read permission):
//        200 with `unreadable: true` and `lines` empty.
//     g. `?show=other` and `?from=abc`: 400 with `ApiErrors` naming `show` / `from`.
//     h. the planted credential: absent from the whole response body.
// @lane: integration
// @dependency: dashboard server router, `events::page`, store, seeded `events.jsonl` and
//     `launches.jsonl`, credentials file
// Name: the_events_endpoint_lists_the_recorded_lines_with_their_subjects_errors_and_told_states
#[test]
fn the_events_endpoint_lists_the_recorded_lines_with_their_subjects_errors_and_told_states() {
    let env = Env::new();
    env.configure("");
    let dash = Dashboard::start(&env);
    dash.hold(HELD);
    seed_events_records(&dash.store);
    seed_event_lines(&dash.store);

    // a. The newest 15 lines.
    let first = dash.get("/api/v1/events");
    assert_eq!(first.status, 200, "a: {}", first.body);
    let page = first.json();
    assert_eq!(page["lines"], newest_lines(), "a: lines");
    assert_eq!(
        (
            &page["older"],
            &page["recorded"],
            &page["unreadable"],
            &page["notices"]
        ),
        (&json!(18), &json!(true), &json!(false), &json!(true)),
        "a: frame"
    );
    assert_eq!(
        page["errorsNow"],
        json!([
            {"key": "tracker-unreachable", "since": at(11), "last": at(11), "times": 1, "firstIssue": null,
             "cause": "no answer", "tracker": "linear", "cli": null, "command": null},
            {"key": "github-unreachable", "since": at(12), "last": at(12), "times": 1,
             "firstIssue": issue("ACME-1"), "cause": "HTTP 502", "tracker": null, "cli": null, "command": null},
        ]),
        "a: errors now"
    );
    assert_eq!(
        page["issues"],
        json!([{"key": "ACME-1", "title": "Export orders"}, {"key": "ACME-2", "title": "Refund flow"},
               {"key": "ACME-9", "title": null}]),
        "a: issues"
    );

    assert_windows_and_filters(&dash);
    assert_empty_and_unreadable(&dash);

    // e. Now and Events read one since.
    let overview = dash.get("/api/v1/overview").json();
    assert_eq!(
        (
            &overview["status"]["unreachable"]["service"],
            &overview["status"]["unreachable"]["since"]
        ),
        (&json!("github"), &page["errorsNow"][1]["since"]),
        "e: since"
    );

    // g. Parameters the page never sends.
    for (query, field) in [("show=other", "show"), ("from=abc", "from")] {
        let refused = dash.get(&format!("/api/v1/events?{query}"));
        assert_eq!(refused.status, 400, "g: {query}");
        let errors = refused.json()["errors"].clone();
        assert_eq!(
            errors.as_object().unwrap().keys().collect::<Vec<_>>(),
            [field],
            "g: {query}"
        );
    }

    // h. The held credential is never served.
    for body in [&first.body, &dash.get("/api/v1/events?from=0").body] {
        assert!(!body.contains(HELD), "h: {body}");
    }
}

/// A credential the factory holds, planted in an old line.
const HELD: &str = "plain-held-7f3a9c";

fn at(minute: u32) -> String {
    format!("2026-11-27T04:{minute:02}:00Z")
}

fn issue(key: &str) -> Value {
    json!({"key": key, "known": key != "ACME-9"})
}

/// The seq of the older filter failure `seed_event_lines` records without a `failure`.
const OLD_FILTER_FAILURE: u64 = 60;

/// `events.jsonl` with 70 older lines, then one line of each case the page reads, then a torn
/// line.
fn seed_event_lines(store: &ovenspring_store::Store) {
    let filter_since = at(6);
    // All on ACME-1, so the windows below are counted in them; one is a filter failure as the
    // version before error tracking recorded it, with no `failure`.
    let mut lines: Vec<String> = (0..70)
        .map(|i| {
            let at = format!("2026-11-26T{:02}:{:02}:00Z", 9 + i / 60, i % 60);
            if i == OLD_FILTER_FAILURE {
                json!({"event": "filter-failed", "issue": "ACME-1", "error": "exit 2", "at": at})
            } else {
                json!({"event": "pushed", "issue": "ACME-1", "n": i, "at": at})
            }
            .to_string()
        })
        .collect();
    lines.extend(
        [
        json!({"event": "sprint-begun", "sprint": "20261127-0900", "at": at(1)}),
        json!({"event": "github-error", "issue": "20261120-0900", "error": "branch gone", "at": at(2)}),
        json!({"event": "operation-failed", "operation": "pushing", "sprint": true, "at": at(3)}),
        json!({"event": "step-done", "item": "implement-ACME-2", "at": at(4)}),
        json!({"event": "filter-dropped", "issue": "ACME-9", "url": "https://linear.app/acme/issue/ACME-9",
               "reason": "Not ours", "at": at(5)}),
        json!({"event": "filter-failed", "issue": "ACME-2", "error": "exit 1", "command": "./judge",
               "output": "boom", "at": at(6),
               "failure": {"key": "filter", "since": filter_since, "cause": "exit 1 (boom)", "start": true,
                           "notice": "waiting"}}),
        json!({"event": "filter-failed", "issue": "ACME-2", "error": "exit 1", "command": "./judge",
               "output": "boom", "at": at(7),
               "failure": {"key": "filter", "since": filter_since, "cause": "exit 1 (boom)"}}),
        json!({"event": "filter-failed", "issue": "ACME-1", "error": "exit 1", "command": "./judge",
               "output": "boom", "at": at(8),
               "failure": {"key": "filter", "since": filter_since, "cause": "exit 1 (boom)"}}),
        json!({"event": "error-notice", "failure": {"key": "filter", "since": filter_since},
               "phase": "started", "outcome": "told", "at": "2026-11-27T04:09:30Z"}),
        json!({"event": "error-cleared", "issue": "ACME-1", "count": 3, "at": at(10),
               "failure": {"key": "filter", "since": filter_since, "notice": "waiting"}}),
        json!({"event": "service-unreachable", "operation": "reading the tracker", "why": "no-connection",
               "tracker": "linear", "at": at(11),
               "failure": {"key": "tracker-unreachable", "since": at(11), "cause": "no answer", "start": true,
                           "notice": "off"}}),
        json!({"event": "service-unreachable", "operation": "reading its pull request", "why": "server-error",
               "issue": "ACME-1", "at": at(12),
               "failure": {"key": "github-unreachable", "since": at(12), "cause": "HTTP 502", "via": ["api"],
                           "start": true, "notice": "waiting"}}),
        json!({"event": "error-notice", "failure": {"key": "github-unreachable", "since": at(12)},
               "phase": "started", "outcome": "dropped", "at": at(13)}),
        ]
        .iter()
        .map(Value::to_string),
    );
    // Another writer's spacing, a kind with no sentence, an old line holding a credential, and a
    // last line torn by a crash.
    lines.push(format!(
        "{{\"event\": \"replay-reset\", \"at\": \"{}\", \"from\": 3}}",
        at(14)
    ));
    lines.push(json!({"event": "mystery-kind", "issue": "ACME-2", "at": at(15)}).to_string());
    lines.push(
        json!({"event": "tracker-error", "issue": "ACME-2", "error": format!("refused with {HELD}"),
               "at": at(16)})
        .to_string(),
    );
    lines.push("{\"event\":\"half".to_owned());
    std::fs::write(store.root().join("events.jsonl"), lines.join("\n")).unwrap();
}

/// What `seed_event_lines` serves first: its newest 15 lines, newest first.
fn newest_lines() -> Value {
    let filter_since = at(6);
    let pushed = |seq: u64, minute: u64| {
        json!({"seq": seq, "at": format!("2026-11-26T10:{minute:02}:00Z"), "kind": "pushed",
               "fields": {"issue": "ACME-1", "n": seq}, "issue": issue("ACME-1"), "sprint": null, "error": null})
    };
    json!([
        {"seq": 85, "at": at(16), "kind": "tracker-error",
         "fields": {"issue": "ACME-2", "error": "refused with [masked]"},
         "issue": issue("ACME-2"), "sprint": null, "error": null},
        {"seq": 84, "at": at(15), "kind": "mystery-kind", "fields": {"issue": "ACME-2"},
         "issue": issue("ACME-2"), "sprint": null, "error": null},
        {"seq": 83, "at": at(14), "kind": "replay-reset", "fields": {"from": 3},
         "issue": null, "sprint": null, "error": null},
        {"seq": 81, "at": at(12), "kind": "service-unreachable",
         "fields": {"operation": "reading its pull request", "why": "server-error", "issue": "ACME-1",
                    "failure": {"key": "github-unreachable", "since": at(12), "cause": "HTTP 502",
                                "via": ["api"], "start": true, "notice": "waiting"}},
         "issue": issue("ACME-1"), "sprint": null,
         "error": {"role": "started", "key": "github-unreachable", "cause": "HTTP 502", "times": 1,
                   "last": at(12), "open": true, "told": {"state": "off"}}},
        {"seq": 80, "at": at(11), "kind": "service-unreachable",
         "fields": {"operation": "reading the tracker", "why": "no-connection", "tracker": "linear",
                    "failure": {"key": "tracker-unreachable", "since": at(11), "cause": "no answer",
                                "start": true, "notice": "off"}},
         "issue": null, "sprint": null,
         "error": {"role": "started", "key": "tracker-unreachable", "cause": "no answer", "times": 1,
                   "last": at(11), "open": true, "told": {"state": "off"}}},
        {"seq": 79, "at": at(10), "kind": "error-cleared",
         "fields": {"issue": "ACME-1", "count": 3,
                    "failure": {"key": "filter", "since": filter_since, "notice": "waiting"}},
         "issue": issue("ACME-1"), "sprint": null,
         "error": {"role": "cleared", "key": "filter", "startedAt": filter_since, "times": 3,
                   "removed": false, "told": {"state": "waiting"}}},
        {"seq": 77, "at": at(8), "kind": "filter-failed",
         "fields": {"issue": "ACME-1", "error": "exit 1", "command": "./judge", "output": "boom",
                    "failure": {"key": "filter", "since": filter_since, "cause": "exit 1 (boom)"}},
         "issue": issue("ACME-1"), "sprint": null,
         "error": {"role": "repeat", "key": "filter", "cause": "exit 1 (boom)", "times": 1,
                   "last": at(8), "startedAt": filter_since}},
        {"seq": 75, "at": at(6), "kind": "filter-failed",
         "fields": {"issue": "ACME-2", "error": "exit 1", "command": "./judge", "output": "boom",
                    "failure": {"key": "filter", "since": filter_since, "cause": "exit 1 (boom)",
                                "start": true, "notice": "waiting"}},
         "issue": issue("ACME-2"), "sprint": null,
         "error": {"role": "started", "key": "filter", "cause": "exit 1 (boom)", "times": 2,
                   "last": at(7), "open": false,
                   "told": {"state": "told", "at": "2026-11-27T04:09:30Z"}}},
        {"seq": 74, "at": at(5), "kind": "filter-dropped",
         "fields": {"issue": "ACME-9", "url": "https://linear.app/acme/issue/ACME-9", "reason": "Not ours"},
         "issue": issue("ACME-9"), "sprint": null, "error": null},
        {"seq": 73, "at": at(4), "kind": "step-done", "fields": {"item": "implement-ACME-2"},
         "issue": issue("ACME-2"), "sprint": null, "error": null},
        {"seq": 72, "at": at(3), "kind": "operation-failed",
         "fields": {"operation": "pushing", "sprint": true},
         "issue": null, "sprint": null, "error": null},
        {"seq": 71, "at": at(2), "kind": "github-error",
         "fields": {"issue": "20261120-0900", "error": "branch gone"},
         "issue": null, "sprint": {"id": "20261120-0900", "purpose": "Refunds", "listed": true},
         "error": null},
        {"seq": 70, "at": at(1), "kind": "sprint-begun", "fields": {"sprint": "20261127-0900"},
         "issue": null,
         "sprint": {"id": "20261127-0900", "purpose": "Customers export orders", "listed": true},
         "error": null},
        pushed(69, 9),
        pushed(68, 8),
    ])
}

fn seqs(page: &Value) -> Vec<u64> {
    page["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["seq"].as_u64().unwrap())
        .collect()
}

/// b. Older lines, 50 at a time; c. the lines each filter keeps.
fn assert_windows_and_filters(dash: &Dashboard) {
    let shown = |from: u64| -> Vec<u64> {
        let mut seqs: Vec<u64> = vec![85, 84, 83, 81, 80, 79, 77, 75, 74, 73, 72, 71, 70];
        seqs.extend((from..70).rev());
        seqs
    };
    let more = dash.get("/api/v1/events?from=18").json();
    assert_eq!(
        (seqs(&more), &more["older"]),
        (shown(18), &json!(0)),
        "b: from 18"
    );
    let all = dash.get("/api/v1/events?from=0").json();
    assert_eq!(
        (seqs(&all), &all["older"]),
        (shown(0), &Value::Null),
        "b: from 0"
    );

    for (query, expected) in [
        ("show=errors", vec![81, 80, 79, 77, 75]),
        ("show=filter", vec![79, 77, 75, 74, OLD_FILTER_FAILURE]),
        ("issue=ACME-2", vec![85, 84, 75, 73]),
    ] {
        let narrowed = dash.get(&format!("/api/v1/events?{query}")).json();
        assert_eq!(
            (seqs(&narrowed), &narrowed["older"]),
            (expected, &Value::Null),
            "c: {query}"
        );
    }
    let filter = dash.get("/api/v1/events?show=filter").json();
    let old = filter["lines"].as_array().unwrap().last().unwrap();
    assert_eq!(
        (&old["kind"], &old["fields"], &old["issue"]),
        (
            &json!("filter-failed"),
            &json!({"issue": "ACME-1", "error": "exit 2"}),
            &issue("ACME-1")
        ),
        "c: an older filter failure with its issue and cause"
    );
}

/// d. Nothing matches, against nothing recorded; f. a file that cannot be read.
fn assert_empty_and_unreadable(dash: &Dashboard) {
    let none = dash.get("/api/v1/events?issue=ACME-404").json();
    assert_eq!(
        (&none["lines"], &none["recorded"]),
        (&json!([]), &json!(true)),
        "d: no match"
    );
    let empty_env = Env::new();
    let empty = Dashboard::start(&empty_env).get("/api/v1/events").json();
    assert_eq!(
        (&empty["lines"], &empty["recorded"], &empty["unreadable"]),
        (&json!([]), &json!(false), &json!(false)),
        "d: nothing recorded"
    );

    let unreadable_env = Env::new();
    let unreadable_dash = Dashboard::start(&unreadable_env);
    std::fs::create_dir_all(unreadable_dash.store.root().join("events.jsonl")).unwrap();
    let unreadable = unreadable_dash.get("/api/v1/events");
    assert_eq!(unreadable.status, 200, "f: {}", unreadable.body);
    let unreadable = unreadable.json();
    assert_eq!(
        (&unreadable["unreadable"], &unreadable["lines"]),
        (&json!(true), &json!([])),
        "f"
    );
}

/// Two issues, the running sprint and an ended one, and the launch of an item of ACME-2.
fn seed_events_records(store: &ovenspring_store::Store) {
    use ovenspring_core::sprint::{SprintStage, SprintState};
    let sprint = |id: &str, purpose: &str| SprintState {
        id: id.into(),
        stage: SprintStage::Running,
        plan: Some(ovenspring_core::output::SprintPlanOutput {
            purpose: purpose.into(),
            key_results: vec![],
            issues: vec![],
            message: "m".into(),
        }),
        review: None,
        agreements_pr: None,
        agreements_url: None,
        agreements_need_approval: false,
        agreements_pushed_at: None,
        started_at: Some("2026-11-20T09:00:00Z".into()),
        ended_at: None,
        failed: None,
        failure: None,
        resume: None,
    };
    store
        .write_record(
            "sprint.json",
            &sprint("20261127-0900", "Customers export orders"),
        )
        .unwrap();
    store
        .write_record(
            "sprints/20261120-0900.json",
            &sprint("20261120-0900", "Refunds"),
        )
        .unwrap();
    for (key, title) in [("ACME-1", "Export orders"), ("ACME-2", "Refund flow")] {
        let r = ovenspring_daemon::issues::IssueRecord::new(key, title, vec![]);
        store
            .write_record(&format!("issues/{key}.json"), &r)
            .unwrap();
    }
    store
        .append_launch(&json!({"at": "2026-11-27T04:03:30Z", "role": "executor",
                               "item": "implement-ACME-2", "step": "implement", "issue": "ACME-2",
                               "sprint": "20261127-0900", "cli": "claude", "model": null,
                               "effort": null, "permissions": "auto", "attempt": 1, "pid": 999_999,
                               "log": "x.log"}))
        .unwrap();
}
