//! @lane: integration
//! Area: what a person does to a pull request outside the factory, and the base moving under it.

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

use ovenspring_daemon::owner::{DecisionRecord, DecisionStatus};
use ovenspring_integrations::channel::{Button, ChannelEvent, DecisionKind};

use ovenspring_core::flow::{Cause, Stage};
use ovenspring_integrations::github::{BranchRules, CheckState, Permission, ReviewState};
use serde_json::{Value, json};
use support::fake_github::FakeGitHub;
use support::recording::{RecordingChannel, RecordingTracker};
use support::scripts::{
    always_passing, filed_steps, quality_pass, requiring_test, stage, straight,
};
use support::{Env, daemon, git, run_until, services};

const CONFIG: &str = r#"
[repo]
base_branch = "develop"
[review]
"#;

fn merges(gh: &FakeGitHub) -> Vec<String> {
    gh.calls()
        .into_iter()
        .filter(|c| c.starts_with("merge "))
        .collect()
}

fn pr_of(d: &ovenspring_daemon::Daemon, key: &str) -> u64 {
    d.issue(key).unwrap().unwrap().flow.pr.unwrap()
}

/// A person commits `path` on `branch` from the main checkout and pushes it to `origin`.
fn push_as_person(env: &Env, branch: &str, path: &str, content: &str) -> String {
    git(&env.repo, &["fetch", "-q", "origin", branch]);
    git(&env.repo, &["checkout", "-q", "-B", "person", "FETCH_HEAD"]);
    std::fs::create_dir_all(env.repo.join(path).parent().unwrap()).unwrap();
    std::fs::write(env.repo.join(path), content).unwrap();
    git(&env.repo, &["add", path]);
    git(
        &env.repo,
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
        &env.repo,
        &["push", "-q", "origin", &format!("HEAD:refs/heads/{branch}")],
    );
    git(&env.repo, &["rev-parse", "HEAD"])
}

fn requiring_one_approval(env: &Env) -> FakeGitHub {
    let mut gh = always_passing(
        env,
        BranchRules {
            required_checks: ["test".to_owned()].into(),
        },
    );
    gh.approvals_required = 1;
    gh
}

fn in_daemon(env: &Env, gh: &Arc<FakeGitHub>) -> ovenspring_daemon::Daemon {
    daemon(
        env,
        services(
            gh.clone(),
            Arc::new(RecordingChannel::default()),
            Arc::new(RecordingTracker::default()),
        ),
    )
}

// AC: docs/design/factory-core-design.md § Acceptance Criteria > Observation, exceptions and
//     recovery: "When the daemon is about to work on an issue branch that has a pull request (an
//     agent step, a push), it shall first bring the branch's current head into the worktree, so
//     commits a person pushed are carried along; what the factory takes in from others is neither
//     checked nor reviewed by the factory (AC-063)"; § Issue: "The gate shall judge the pull
//     request's current head, whoever pushed it".
// Behavior: ACME-123 waits for a required approval -> a person pushes a commit to its branch ->
//     the daemon takes that head as it is -> the reviewer approves the new head -> the gate merges
//     that head.
// @lane: integration
// @dependency: daemon, flow, gate, FakeGitHub, fake agent
#[test]
fn a_persons_commit_pushed_while_the_issue_waits_is_judged_by_the_gate_and_merged() {
    let env = Env::new();
    env.configure(CONFIG);
    env.add_branch("develop");
    env.script_agents(&Value::Object(straight("ACME-123")));
    let gh = Arc::new(requiring_one_approval(&env));
    let mut d = in_daemon(&env, &gh);
    d.begin_issue("ACME-123", "Export", vec![]).unwrap();
    run_until(
        &mut d,
        60,
        |_| {},
        |d| stage(d, "ACME-123") == Stage::WaitingReview,
    );
    let steps_before = filed_steps(&env, "ACME-123");

    let person = push_as_person(&env, "ovenspring/ACME-123", "docs/NOTE.md", "note\n");
    run_until(
        &mut d,
        30,
        |_| {},
        |d| d.issue("ACME-123").unwrap().unwrap().pushed_sha.as_deref() == Some(person.as_str()),
    );
    let worktree = env.state().join("worktrees/ACME-123");
    assert_eq!(git(&worktree, &["rev-parse", "HEAD"]), person);

    gh.review(pr_of(&d, "ACME-123"), "acme-lead", ReviewState::Approved);
    run_until(
        &mut d,
        30,
        |_| {},
        |d| stage(d, "ACME-123") == Stage::Merged,
    );
    assert_eq!(
        merges(&gh),
        [format!("merge {} {person}", pr_of(&d, "ACME-123"))]
    );
    assert_eq!(filed_steps(&env, "ACME-123"), steps_before);
}

// AC: docs/design/factory-core-design.md § Acceptance Criteria > Issue: "When reviews pass, the
//     system shall bring the branch's current head and the base into the worktree, push the issue
//     branch and open or update its pull request; if bringing them in conflicts, then it shall file
//     `merge-base` for the executor instead (AC-026)".
// Behavior: ACME-123 is under review -> `develop` gains a file of the same name with other content
//     -> at the push the base conflicts -> `merge-base` resolves it -> the push carries the base ->
//     the gate merges a head that contains the base's commit.
// @lane: integration
// @dependency: daemon, flow, gate, FakeGitHub, fake agent
#[test]
fn a_conflict_with_the_base_at_the_push_goes_to_the_executor_then_merges() {
    let env = Env::new();
    env.configure(CONFIG);
    env.add_branch("develop");
    let mut script = straight("ACME-123");
    script.insert(
        "merge-base@ACME-123".into(),
        json!([{"git": [["fetch", "-q", "origin"], ["merge", "-q", "--no-edit", "-X", "ours", "origin/develop"]],
                "output": {"status": "pass", "summary": "s", "checksPerformed": {}, "fixesApplied": []}}]),
    );
    env.script_agents(&Value::Object(script));
    let gh = Arc::new(always_passing(&env, requiring_test()));
    let mut d = in_daemon(&env, &gh);
    d.begin_issue("ACME-123", "Export", vec![]).unwrap();
    run_until(
        &mut d,
        60,
        |_| {},
        |d| stage(d, "ACME-123") == Stage::Reviewing,
    );
    let base = push_as_person(&env, "develop", "src/ACME-123.ts", "the base's own\n");

    run_until(
        &mut d,
        60,
        |_| {},
        |d| stage(d, "ACME-123") == Stage::Merged,
    );
    let events = env.events();
    let conflict = events
        .iter()
        .position(|e| e["event"] == "push-conflict" && e["issue"] == "ACME-123")
        .expect("the push met the conflict");
    let first_push = events
        .iter()
        .position(|e| e["event"] == "pushed" && e["issue"] == "ACME-123")
        .unwrap();
    assert!(conflict < first_push);
    assert_eq!(
        filed_steps(&env, "ACME-123")
            .iter()
            .filter(|s| *s == "merge-base")
            .count(),
        1
    );
    let all = merges(&gh);
    assert_eq!(all.len(), 1);
    git(
        &env.origin,
        &[
            "merge-base",
            "--is-ancestor",
            &base,
            all[0].rsplit(' ').next().unwrap(),
        ],
    );
}

// AC: docs/design/factory-core-design.md § Acceptance Criteria > Observation, exceptions and
//     recovery: "On every GitHub poll, for each issue with a pull request in any state ... merged →
//     the merged path; closed without merging → `exception`" (AC-063).
// Behavior: ACME-123's checks never finish -> a person merges its pull request -> the issue ends
//     merged; ACME-124's pull request is closed by a person -> the lead gets an `exception`.
// @lane: integration
// @dependency: daemon, flow, FakeGitHub, fake agent
#[test]
fn a_pull_request_merged_or_closed_by_a_person_while_ci_runs_moves_the_issue_on() {
    let env = Env::new();
    env.configure(CONFIG);
    env.add_branch("develop");
    let mut script = straight("ACME-123");
    script.extend(straight("ACME-124"));
    env.script_agents(&Value::Object(script));
    let gh = Arc::new(FakeGitHub::new(
        env.origin.clone(),
        requiring_test(),
        Box::new(|_| vec![("test".to_owned(), CheckState::Pending)]),
    ));
    let mut d = in_daemon(&env, &gh);
    d.begin_issue("ACME-123", "Export", vec![]).unwrap();
    d.begin_issue("ACME-124", "Import", vec![]).unwrap();
    run_until(
        &mut d,
        60,
        |_| {},
        |d| stage(d, "ACME-123") == Stage::Ci && stage(d, "ACME-124") == Stage::Ci,
    );

    gh.merge_as_owner(pr_of(&d, "ACME-123"));
    gh.close_as_owner(pr_of(&d, "ACME-124"));
    run_until(
        &mut d,
        30,
        |_| {},
        |d| {
            stage(d, "ACME-123") == Stage::Merged
                && matches!(stage(d, "ACME-124"), Stage::Exception { .. })
        },
    );
    assert_eq!(merges(&gh), Vec::<String>::new());
    assert!(
        gh.calls()
            .contains(&"delete_branch ovenspring/ACME-123".to_owned())
    );
    assert_eq!(
        stage(&d, "ACME-124"),
        Stage::Exception {
            cause: Cause::PullClosed,
            resume: None
        }
    );
    // Later looks at the closed pull request file nothing more while the lead has it.
    for _ in 0..3 {
        d.tick().unwrap();
    }
    assert_eq!(
        filed_steps(&env, "ACME-124")
            .iter()
            .filter(|s| *s == "exception")
            .count(),
        1
    );
}

type Held = (
    ovenspring_daemon::Daemon,
    Arc<FakeGitHub>,
    Arc<RecordingChannel>,
    Arc<RecordingTracker>,
);

/// ACME-123 changes `AGENTS.md`, so the gate holds its merge for the owner.
fn held_for_owner(env: &Env) -> Held {
    env.configure(CONFIG);
    env.add_branch("develop");
    let mut script = straight("ACME-123");
    script.insert(
        "implement@ACME-123".into(),
        json!([support::scripts::implement_run("AGENTS.md")]),
    );
    env.script_agents(&Value::Object(script));
    let gh = Arc::new(always_passing(env, requiring_test()));
    let channel = Arc::new(RecordingChannel::default());
    let tracker = Arc::new(RecordingTracker::default());
    let mut d = daemon(env, services(gh.clone(), channel.clone(), tracker.clone()));
    d.begin_issue("ACME-123", "Export", vec![]).unwrap();
    run_until(
        &mut d,
        60,
        |_| {},
        |d| stage(d, "ACME-123") == Stage::HeldForOwner,
    );
    (d, gh, channel, tracker)
}

fn pending_merge(d: &ovenspring_daemon::Daemon) -> Vec<DecisionRecord> {
    d.decisions()
        .unwrap()
        .into_iter()
        .filter(|x| x.kind == DecisionKind::Merge && x.status == DecisionStatus::Pending)
        .collect()
}

// AC: "An owner approval is tied to the sha shown in its request; a later push requests it
//     again" — docs/design/factory-core-design.md § Merge gate.
// Behavior: ACME-123's merge is held for the owner -> a person pushes a commit to its branch -> the
//     request for the earlier head is superseded and the owner is asked for the new head.
// @dependency: daemon, gate, owner channel, FakeGitHub
#[test]
fn a_head_a_person_pushes_while_the_merge_is_held_is_asked_for_again() {
    let env = Env::new();
    let (mut d, _gh, _, _) = held_for_owner(&env);
    assert_eq!(pending_merge(&d).len(), 1);

    let person = push_as_person(&env, "ovenspring/ACME-123", "docs/NOTE.md", "note\n");
    run_until(
        &mut d,
        30,
        |_| {},
        |d| {
            pending_merge(d)
                .iter()
                .any(|x| x.sha.as_deref() == Some(person.as_str()))
        },
    );
    assert_eq!(pending_merge(&d).len(), 1);
}

// AC: "for each issue with a pull request in any state ... merged -> the merged path" (AC-063)
//     — docs/design/factory-core-design.md § Acceptance Criteria > Observation.
// Behavior: the owner holds ACME-123's merge for the sprint review -> they merge its pull request
//     on GitHub themselves -> the issue ends merged: closed on the tracker and its branch deleted.
// @dependency: daemon, gate, owner channel, FakeGitHub, RecordingTracker
#[test]
fn a_held_pull_request_the_owner_merges_takes_the_merged_path() {
    let env = Env::new();
    let (mut d, gh, channel, tracker) = held_for_owner(&env);
    let request = pending_merge(&d).remove(0);
    channel.push_event(ChannelEvent::Action {
        decision: request.id,
        button: Button::Hold,
        user: "U0123456789".into(),
    });
    run_until(&mut d, 30, |_| {}, |d| stage(d, "ACME-123") == Stage::Held);

    gh.merge_as_owner(pr_of(&d, "ACME-123"));
    run_until(
        &mut d,
        30,
        |_| {},
        |d| stage(d, "ACME-123") == Stage::Merged,
    );
    assert!(
        tracker
            .calls()
            .contains(&("ACME-123".to_owned(), "state: Done".to_owned())),
        "{:?}",
        tracker.calls()
    );
    assert!(
        gh.calls()
            .contains(&"delete_branch ovenspring/ACME-123".to_owned())
    );
}

// AC: "While the issue waits on the pull request, a head someone else pushed is taken" — and only
//     that — docs/design/factory-core-design.md § Observation.
// Behavior: ACME-123 waits for an approval after the factory's push -> GitHub still reports the
//     head before that push -> the factory keeps its own head and takes nothing.
// @dependency: daemon, FakeGitHub
#[test]
fn a_head_github_has_not_caught_up_with_is_not_taken_as_someone_elses() {
    let env = Env::new();
    env.configure(CONFIG);
    env.add_branch("develop");
    env.script_agents(&Value::Object(straight("ACME-123")));
    let gh = Arc::new(requiring_one_approval(&env));
    let mut d = in_daemon(&env, &gh);
    d.begin_issue("ACME-123", "Export", vec![]).unwrap();
    run_until(
        &mut d,
        60,
        |_| {},
        |d| stage(d, "ACME-123") == Stage::WaitingReview,
    );
    let pushed = d.issue("ACME-123").unwrap().unwrap().pushed_sha.unwrap();
    let worktree = env.state().join("worktrees/ACME-123");
    let before = git(&worktree, &["rev-parse", &format!("{pushed}^")]);

    *gh.stale_head.lock().unwrap() = Some(before);
    for _ in 0..3 {
        d.tick().unwrap();
    }
    assert_eq!(
        d.issue("ACME-123").unwrap().unwrap().pushed_sha.as_deref(),
        Some(pushed.as_str())
    );
    assert_eq!(git(&worktree, &["rev-parse", "HEAD"]), pushed);
    assert!(!env.events().iter().any(|e| e["event"] == "head-taken"));
}

// AC: "When the daemon is about to work on an issue branch that has a pull request (an agent step,
//     a push), it shall first bring the branch's current head into the worktree" (AC-063) —
//     docs/design/factory-core-design.md § Acceptance Criteria > Observation.
// Behavior: a reviewer's comment on ACME-123 goes to the executor -> while the executor judges and
//     corrects it, a person pushes a commit to the branch -> the quality run after the correction
//     starts with that commit already in its worktree.
// @dependency: daemon, launcher, fake agent, FakeGitHub
#[test]
fn a_commit_pushed_during_an_agent_step_is_in_the_next_steps_worktree() {
    let env = Env::new();
    env.configure(CONFIG);
    env.add_branch("develop");
    let mut script = straight("ACME-123");
    script.insert(
        "review-response@ACME-123".into(),
        json!([{"wait": true, "change": {"path": "src/ACME-123.ts", "content": "refunds\n"},
                "output": {"status": "completed", "taskName": "comments", "changeSummary": "Count refunds", "requiresTestReview": false,
                           "dispositions": [{"id": "$COMMENT_1", "disposition": "apply", "reason": "the total must count refunds (AC 1)"}]}}]),
    );
    let mut correction_quality = quality_pass();
    correction_quality["wait"] = json!(true);
    script.insert(
        "quality@ACME-123".into(),
        json!([quality_pass(), correction_quality]),
    );
    env.script_agents(&Value::Object(script));
    let mut gh = requiring_one_approval(&env);
    gh.permissions.insert("acme-dev".into(), Permission::Write);
    let gh = Arc::new(gh);
    let mut d = in_daemon(&env, &gh);
    d.begin_issue("ACME-123", "Export", vec![]).unwrap();
    run_until(
        &mut d,
        60,
        |_| {},
        |d| stage(d, "ACME-123") == Stage::WaitingReview,
    );
    gh.comment(
        pr_of(&d, "ACME-123"),
        "acme-dev",
        "The total ignores refunds.",
    );
    let root = env.fake_bin.parent().unwrap().to_path_buf();
    let launched = |step: &str| {
        env.records().iter().any(|r| {
            r["prompt"]
                .as_str()
                .unwrap_or_default()
                .contains(&format!("-{step}-ACME-123"))
        })
    };
    run_until(&mut d, 30, |_| {}, |_| launched("review-response"));

    let person = push_as_person(&env, "ovenspring/ACME-123", "docs/NOTE.md", "note\n");
    std::fs::write(root.join("release-ACME-123-review-response"), "").unwrap();
    let quality_runs = || {
        env.records()
            .iter()
            .filter(|r| {
                r["prompt"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("-quality-ACME-123")
            })
            .count()
    };
    run_until(&mut d, 30, |_| {}, |_| quality_runs() == 2);

    let worktree = env.state().join("worktrees/ACME-123");
    git(&worktree, &["merge-base", "--is-ancestor", &person, "HEAD"]);
    std::fs::write(root.join("release-ACME-123-quality"), "").unwrap();
}

// AC: "When the owner approves the sprint proposal, the system shall file `requirements` for every
//     proposed issue whose dependencies are met" (AC-016), and a held issue returns "only through
//     the owner's reply" (`factory/steps/sprint-plan.md`).
// Behavior: the owner holds ACME-123's merge -> a later sprint the owner approves takes it up
//     again -> it goes back to the gate, which asks the owner for its head, instead of staying
//     held and being counted as settled.
// @dependency: daemon, sprint flow, gate, owner channel, fake agent
#[test]
fn a_held_issue_a_sprint_takes_up_again_goes_back_to_the_gate() {
    let env = Env::new();
    let (mut d, _gh, channel, _) = held_for_owner(&env);
    let request = pending_merge(&d).remove(0);
    channel.push_event(ChannelEvent::Action {
        decision: request.id,
        button: Button::Hold,
        user: "U0123456789".into(),
    });
    run_until(&mut d, 30, |_| {}, |d| stage(d, "ACME-123") == Stage::Held);

    env.script_agents(&json!({"sprint-plan": [{"output": {
        "purpose": "Exports are complete", "key_results": ["Orders export with refunds"],
        "issues": [{"key": "ACME-123", "depends_on": []}], "message": "Sprint: exports"}}]}));
    let (sprint, _) = ovenspring_core::sprint::begin("s2");
    d.store().write_record("sprint.json", &sprint).unwrap();
    let proposal = |d: &ovenspring_daemon::Daemon| {
        d.decisions()
            .unwrap()
            .into_iter()
            .find(|x| x.kind == DecisionKind::Sprint && x.status == DecisionStatus::Pending)
    };
    run_until(&mut d, 30, |_| {}, |d| proposal(d).is_some());
    channel.push_event(ChannelEvent::Action {
        decision: proposal(&d).unwrap().id,
        button: Button::Approve,
        user: "U0123456789".into(),
    });
    run_until(
        &mut d,
        30,
        |_| {},
        |d| stage(d, "ACME-123") == Stage::HeldForOwner,
    );
    assert_eq!(pending_merge(&d).len(), 1);
}
