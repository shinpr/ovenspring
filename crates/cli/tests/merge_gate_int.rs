//! @lane: integration
//! Area: merge execution by the daemon (gate decision -> serialized merge or owner notice).

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
use std::sync::atomic::{AtomicBool, Ordering};

use ovenspring_core::flow::Stage;
use ovenspring_core::output::Approval;
use ovenspring_integrations::channel::NoticeKind;
use ovenspring_integrations::github::{BranchRules, CheckState, ReviewState};
use serde_json::{Value, json};
use support::fake_github::FakeGitHub;
use support::recording::RecordingTracker;
use support::recording::{RecordingChannel, Sent};
use support::scripts::{always_passing, filed_steps, requiring_test, stage, straight};
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

// AC: docs/design/factory-core-design.md § Acceptance Criteria > Issue: "The gate shall judge the
//     pull request's current head, whoever pushed it; if the base moved after checks, then the system
//     shall return the issue to push, which brings the base in (AC-026)";
//     "When the merge setting is `factory`, the system shall merge only the head commit whose
//     required checks passed, when the branch contains the base head, every required approval is
//     present and every owner approval for that commit is recorded (AC-026, AC-028)".
//     Integration Verification Point: "Two issues pass the gate on the same base; the first merges;
//     the second, re-checked immediately before its merge call, returns to bringing in the base and
//     CI instead of merging."
// Behavior: ACME-123 and ACME-124 both pass the gate on the same `develop` head -> the daemon
//     merges ACME-123 -> immediately before ACME-124's merge call it re-reads the base head, sees it
//     moved, and returns ACME-124 to push, which brings the base in, and CI -> ACME-124 merges later
//     with its new head.
// @lane: integration
// @dependency: daemon merge execution, gate, flow, store, fake agent, FakeGitHub
#[test]
fn second_issue_returns_to_push_and_ci_when_the_base_moved_before_its_merge_call() {
    let env = Env::new();
    env.configure(CONFIG);
    env.add_branch("develop");
    let mut script = straight("ACME-123");
    script.extend(straight("ACME-124"));
    env.script_agents(&Value::Object(script));
    // Checks stay pending until both issues have pushed, so both reach the gate on one base.
    let release = Arc::new(AtomicBool::new(false));
    let open = release.clone();
    let gh = Arc::new(FakeGitHub::new(
        env.origin.clone(),
        BranchRules {
            required_checks: ["e2e".to_owned()].into(),
        },
        Box::new(move |_| {
            let state = if open.load(Ordering::SeqCst) {
                CheckState::Success
            } else {
                CheckState::Pending
            };
            vec![("e2e".to_owned(), state)]
        }),
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
    d.begin_issue("ACME-124", "Import", vec![]).unwrap();
    run_until(
        &mut d,
        60,
        |_| {},
        |d| stage(d, "ACME-123") == Stage::Ci && stage(d, "ACME-124") == Stage::Ci,
    );
    let base_before = git(&env.origin, &["rev-parse", "develop"]);
    let head_123 = git(&env.origin, &["rev-parse", "ovenspring/ACME-123"]);
    release.store(true, Ordering::SeqCst);

    run_until(
        &mut d,
        30,
        |_| {},
        |d| stage(d, "ACME-123") == Stage::Merged,
    );
    assert_eq!(
        merges(&gh),
        [format!("merge {} {head_123}", pr_of(&d, "ACME-123"))],
        "one merge, of ACME-123's checked head"
    );
    assert_ne!(stage(&d, "ACME-124"), Stage::Merged);
    let merge_commit = git(&env.origin, &["rev-parse", "develop"]);
    assert_ne!(merge_commit, base_before);

    run_until(
        &mut d,
        60,
        |_| {},
        |d| stage(d, "ACME-124") == Stage::Merged,
    );
    assert!(!filed_steps(&env, "ACME-124").contains(&"merge-base".to_owned()));
    let all = merges(&gh);
    assert_eq!(all.len(), 2);
    let merged_head = all[1].rsplit(' ').next().unwrap();
    git(
        &env.origin,
        &["merge-base", "--is-ancestor", &merge_commit, merged_head],
    );
}

// Behavior: the gate passes ACME-123 -> a commit lands on `develop` from outside the factory
//     between the gate's look and the merge call -> the last look before merging sees it -> no
//     merge call; ACME-123 goes back to push, which brings in the base, and merges later with a
//     head that contains it.
#[test]
fn a_base_that_moves_after_the_gate_looked_is_brought_in_before_merging() {
    let env = Env::new();
    env.configure(CONFIG);
    env.add_branch("develop");
    env.script_agents(&Value::Object(straight("ACME-123")));
    let gh = always_passing(&env, requiring_test());
    let repo = env.repo.clone();
    *gh.before_merge_look.lock().unwrap() = Some(Box::new(move || {
        git(&repo, &["checkout", "-q", "develop"]);
        std::fs::write(repo.join("HOTFIX.md"), "hotfix\n").unwrap();
        git(&repo, &["add", "HOTFIX.md"]);
        git(
            &repo,
            &[
                "-c",
                "user.name=dev",
                "-c",
                "user.email=dev@example.com",
                "commit",
                "-q",
                "-m",
                "Hotfix",
            ],
        );
        git(&repo, &["push", "-q", "origin", "develop"]);
    }));
    let gh = Arc::new(gh);
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
        |d| stage(d, "ACME-123") == Stage::Merged,
    );
    let hotfix = git(&env.repo, &["rev-parse", "develop"]);
    // The one merge call is of a head that holds the hotfix: none was made on the earlier view.
    let all = merges(&gh);
    assert_eq!(all.len(), 1);
    git(
        &env.origin,
        &[
            "merge-base",
            "--is-ancestor",
            &hotfix,
            all[0].rsplit(' ').next().unwrap(),
        ],
    );
}

// AC: docs/design/factory-core-design.md § Acceptance Criteria > Issue: "When the merge setting is
//     `owner` and the gate's conditions other than owner approvals hold, the system shall notify the
//     owner that the pull request is ready and mark the issue merged once GitHub reports it merged
//     (AC-027)".
//     PRD AC-011: "`owner` (the factory stops when the pull request is ready to merge and the owner
//     merges)". PRD AC-027: "the factory never merges; it tells the owner in Slack that the pull
//     request is ready, and it continues the issue as merged once the owner merges it."
// Behavior: `merge.by = "owner"`; ACME-123 passes the gate and ACME-124 depends on it -> the daemon
//     posts one ready-to-merge notice and waits -> FakeGitHub reports the pull request merged by the
//     owner -> ACME-123 becomes `merged` and ACME-124 leaves `queued`.
// @lane: integration
// @dependency: daemon merge execution, gate, flow, store, FakeGitHub, RecordingChannel
#[test]
fn owner_merge_setting_never_merges_and_marks_merged_when_github_reports_it() {
    let env = Env::new();
    env.configure(&format!("{CONFIG}[merge]\nby = \"owner\"\n"));
    env.add_branch("develop");
    let mut script = straight("ACME-123");
    script.extend(straight("ACME-124"));
    env.script_agents(&Value::Object(script));
    let gh = Arc::new(always_passing(&env, requiring_test()));
    let channel = Arc::new(RecordingChannel::default());
    let mut d = daemon(
        &env,
        services(
            gh.clone(),
            channel.clone(),
            Arc::new(RecordingTracker::default()),
        ),
    );
    d.begin_issue("ACME-123", "Export", vec![]).unwrap();
    d.begin_issue("ACME-124", "Import", vec!["ACME-123".into()])
        .unwrap();
    run_until(
        &mut d,
        60,
        |_| {},
        |d| stage(d, "ACME-123") == Stage::ReadyForOwner,
    );
    for _ in 0..10 {
        d.tick().unwrap();
    }
    assert_eq!(stage(&d, "ACME-123"), Stage::ReadyForOwner);
    assert_eq!(stage(&d, "ACME-124"), Stage::Queued);
    assert_eq!(filed_steps(&env, "ACME-124"), [] as [String; 0]);
    let ready: Vec<String> = channel
        .sent()
        .into_iter()
        .filter_map(|m| match m {
            Sent::Notice(n) if n.kind == NoticeKind::ReadyToMerge => Some(n.text),
            Sent::Notice(_)
            | Sent::Decision(_)
            | Sent::Reply { .. }
            | Sent::Attached { .. }
            | Sent::Resolved { .. }
            | Sent::Told { .. } => None,
        })
        .collect();
    let pr = pr_of(&d, "ACME-123");
    assert_eq!(ready.len(), 1);
    assert!(
        ready[0].contains("ACME-123") && ready[0].contains(&format!("/pull/{pr}")),
        "{}",
        ready[0]
    );

    gh.merge_as_owner(pr);
    run_until(
        &mut d,
        30,
        |_| {},
        |_| filed_steps(&env, "ACME-124").first().map(String::as_str) == Some("requirements"),
    );
    assert_eq!(stage(&d, "ACME-123"), Stage::Merged);
    assert!(merges(&gh).is_empty(), "the factory never merges");
}

// AC: PRD AC-024: "When the branch rules require approving reviews, then the factory waits for
//     them"; AC-027: the owner is told the pull request is ready.
// Behavior: `merge.by = "owner"`; GitHub requires one approval the factory's token cannot read
//     from the branch rules -> no ready notice while the approval is missing -> a reviewer approves
//     -> one ready notice.
// @lane: integration
// @dependency: daemon gate, FakeGitHub review decision, RecordingChannel
#[test]
fn a_required_approval_the_branch_rules_do_not_show_holds_the_ready_notice_until_given() {
    let env = Env::new();
    env.configure(&format!("{CONFIG}[merge]\nby = \"owner\"\n"));
    env.add_branch("develop");
    env.script_agents(&Value::Object(straight("ACME-123")));
    let mut gh = always_passing(&env, requiring_test());
    gh.approvals_required = 1;
    let gh = Arc::new(gh);
    let channel = Arc::new(RecordingChannel::default());
    let mut d = daemon(
        &env,
        services(
            gh.clone(),
            channel.clone(),
            Arc::new(RecordingTracker::default()),
        ),
    );
    d.begin_issue("ACME-123", "Export", vec![]).unwrap();
    let ready = |channel: &RecordingChannel| {
        channel
            .sent()
            .into_iter()
            .filter(|m| matches!(m, Sent::Notice(n) if n.kind == NoticeKind::ReadyToMerge))
            .count()
    };
    run_until(
        &mut d,
        60,
        |_| {},
        |d| stage(d, "ACME-123") == Stage::WaitingReview,
    );
    for _ in 0..5 {
        d.tick().unwrap();
    }
    assert_eq!(stage(&d, "ACME-123"), Stage::WaitingReview);
    assert_eq!(ready(&channel), 0);

    gh.review(pr_of(&d, "ACME-123"), "acme-lead", ReviewState::Approved);
    run_until(
        &mut d,
        30,
        |_| {},
        |d| stage(d, "ACME-123") == Stage::ReadyForOwner,
    );
    assert_eq!(ready(&channel), 1);
}

// AC: "When a pull request changes `.ovenspring/` or the repository's agent instructions file
//     (`AGENTS.md`, `CLAUDE.md`), the system shall hold it for the owner whatever its design
//     declared" — factory-core design § Acceptance Criteria > Issue; PRD AC-014.
// Behavior: ACME-123's implementation edits `AGENTS.md` and its design declares nothing -> checks
//     pass -> the gate holds it and asks the owner, naming the factory's configuration, instead of
//     merging.
#[test]
fn a_change_to_the_agent_instructions_waits_for_the_owner_whatever_the_design_declared() {
    let env = Env::new();
    env.configure(CONFIG);
    env.add_branch("develop");
    let mut script = straight("ACME-123");
    script.insert(
        "implement@ACME-123".into(),
        json!([support::scripts::implement_run("AGENTS.md")]),
    );
    env.script_agents(&Value::Object(script));
    let gh = Arc::new(always_passing(&env, requiring_test()));
    let channel = Arc::new(RecordingChannel::default());
    let mut d = daemon(
        &env,
        services(
            gh.clone(),
            channel.clone(),
            Arc::new(RecordingTracker::default()),
        ),
    );
    d.begin_issue("ACME-123", "Export", vec![]).unwrap();
    run_until(
        &mut d,
        60,
        |_| {},
        |d| stage(d, "ACME-123") == Stage::HeldForOwner,
    );
    let decision = d
        .decisions()
        .unwrap()
        .into_iter()
        .find(|x| x.issue.as_deref() == Some("ACME-123"))
        .unwrap();
    assert_eq!(
        decision.categories,
        [ovenspring_core::output::Approval::FactoryConfiguration]
    );
    assert_eq!(merges(&gh), [] as [String; 0]);
}

// AC: PRD AC-014: a pull request that changes the working agreements is held for the owner, in the
//     folder the configuration names.
// Behavior: `agreements.path = "docs/factory/agreements"`; ACME-123's implementation edits a file
//     there and its design declares nothing -> checks pass -> the gate holds it, naming the
//     factory's configuration, instead of merging.
#[test]
fn a_change_to_the_agreements_folder_the_owner_set_waits_for_the_owner() {
    let env = Env::new();
    env.configure(&format!(
        "{CONFIG}[agreements]\npath = \"docs/factory/agreements\"\n"
    ));
    env.add_branch("develop");
    let mut script = straight("ACME-123");
    script.insert(
        "implement@ACME-123".into(),
        json!([support::scripts::implement_run(
            "docs/factory/agreements/executor.md"
        )]),
    );
    env.script_agents(&Value::Object(script));
    let gh = Arc::new(always_passing(&env, requiring_test()));
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
        |d| stage(d, "ACME-123") == Stage::HeldForOwner,
    );
    let decision = d
        .decisions()
        .unwrap()
        .into_iter()
        .find(|x| x.issue.as_deref() == Some("ACME-123"))
        .unwrap();
    assert_eq!(
        decision.categories,
        [ovenspring_core::output::Approval::FactoryConfiguration]
    );
    assert_eq!(merges(&gh), [] as [String; 0]);
}

// AC: PRD AC-027: the owner is told the pull request is ready; AC-026: only once the checks the
//     base requires have passed.
// Behavior: `merge.by = "owner"`; classic protection requires `e2e`, which the factory's token
//     cannot read, and `e2e` has not reported yet while `test` passed -> the issue waits with GitHub
//     blocking it and no ready notice -> `e2e` reports success -> one ready notice.
// @lane: integration
// @dependency: daemon gate, FakeGitHub standing, RecordingChannel
#[test]
fn a_required_check_the_branch_rules_do_not_show_holds_the_ready_notice_until_it_passes() {
    let env = Env::new();
    env.configure(&format!("{CONFIG}[merge]\nby = \"owner\"\n"));
    env.add_branch("develop");
    env.script_agents(&Value::Object(straight("ACME-123")));
    let reported = Arc::new(AtomicBool::new(false));
    let e2e = reported.clone();
    let mut gh = FakeGitHub::new(
        env.origin.clone(),
        requiring_test(),
        Box::new(move |_| {
            let mut checks = vec![("test".to_owned(), CheckState::Success)];
            if e2e.load(Ordering::SeqCst) {
                checks.push(("e2e".to_owned(), CheckState::Success));
            }
            checks
        }),
    );
    gh.unreadable_required = vec!["e2e".into()];
    let gh = Arc::new(gh);
    let channel = Arc::new(RecordingChannel::default());
    let mut d = daemon(
        &env,
        services(
            gh.clone(),
            channel.clone(),
            Arc::new(RecordingTracker::default()),
        ),
    );
    d.begin_issue("ACME-123", "Export", vec![]).unwrap();
    run_until(
        &mut d,
        60,
        |_| {},
        |d| stage(d, "ACME-123") == Stage::BlockedOnGitHub,
    );
    for _ in 0..5 {
        d.tick().unwrap();
    }
    assert_eq!(stage(&d, "ACME-123"), Stage::BlockedOnGitHub);
    assert_eq!(notices_of(&channel, NoticeKind::ReadyToMerge).len(), 0);

    reported.store(true, Ordering::SeqCst);
    run_until(
        &mut d,
        30,
        |_| {},
        |d| stage(d, "ACME-123") == Stage::ReadyForOwner,
    );
    assert_eq!(notices_of(&channel, NoticeKind::ReadyToMerge).len(), 1);
}

const PAYMENTS: &str = "[person_reviews]\npayments = \"How customers are charged or refunded\"\n";

/// ACME-123 is a medium issue whose design says it touches payments, a kind of change the owner
/// named for a person to review.
fn touching_payments(
    env: &Env,
    merge: &str,
) -> (
    ovenspring_daemon::Daemon,
    Arc<FakeGitHub>,
    Arc<RecordingChannel>,
) {
    env.configure(&format!("{CONFIG}{merge}{PAYMENTS}"));
    env.add_branch("develop");
    let mut steps = support::scripts::designed("ACME-123", &[], &[("T1", "none")]);
    steps.insert(
        "design@ACME-123".into(),
        json!([{"change": {"path": "docs/design/ACME-123.md", "content": "# ACME-123\n"},
                "output": {"status": "completed", "documentType": "DesignDoc",
                           "path": "docs/design/ACME-123.md", "approvals": ["payments"]}}]),
    );
    env.script_agents(&Value::Object(steps));
    let gh = Arc::new(always_passing(env, requiring_test()));
    let channel = Arc::new(RecordingChannel::default());
    let mut d = daemon(
        env,
        services(
            gh.clone(),
            channel.clone(),
            Arc::new(RecordingTracker::default()),
        ),
    );
    d.begin_issue("ACME-123", "Refund an order", vec![])
        .unwrap();
    (d, gh, channel)
}

fn notices_of(channel: &RecordingChannel, kind: NoticeKind) -> Vec<String> {
    channel
        .sent()
        .into_iter()
        .filter_map(|m| match m {
            Sent::Notice(n) if n.kind == kind => Some(n.text),
            Sent::Notice(_)
            | Sent::Decision(_)
            | Sent::Reply { .. }
            | Sent::Attached { .. }
            | Sent::Resolved { .. }
            | Sent::Told { .. } => None,
        })
        .collect()
}

// AC: PRD AC-069: "The owner names kinds of change a person reviews, each described in their own
//     words; the design and the code review judge which a change falls under. When the factory
//     merges, such a change waits for the owner's approval of its commit; when people merge, the
//     ready-to-merge message names each kind with its words. Neither asks to approve the design."
// Behavior: `[person_reviews] payments` and a design declaring `payments`, the factory merging ->
//     the design and code review prompts list the kind -> no design approval is asked -> the gate
//     holds the merge for the owner, naming payments -> nothing merges.
// @lane: integration
// @dependency: daemon, flow, gate, prompt assembly, fake agent, FakeGitHub, RecordingChannel
#[test]
fn a_change_a_person_reviews_waits_for_the_owner_before_the_factory_merges_it() {
    let env = Env::new();
    let (mut d, gh, _) = touching_payments(&env, "");
    run_until(
        &mut d,
        90,
        |_| {},
        |d| stage(d, "ACME-123") == Stage::HeldForOwner,
    );

    let decisions = d.decisions().unwrap();
    assert!(
        decisions
            .iter()
            .all(|x| x.kind != ovenspring_integrations::channel::DecisionKind::Design),
        "a kind a person reviews asks nothing of the design"
    );
    let merge = decisions
        .iter()
        .find(|x| x.kind == ovenspring_integrations::channel::DecisionKind::Merge)
        .unwrap();
    assert_eq!(merge.categories, [Approval::Named("payments".into())]);
    assert!(
        merge.text.contains("payments, which a person reviews"),
        "{}",
        merge.text
    );
    assert!(merges(&gh).is_empty(), "the owner has not approved it");
    let judging: Vec<String> = env
        .records()
        .iter()
        .map(|r| r["prompt"].as_str().unwrap_or_default().to_owned())
        .filter(|p| p.contains("# Step: design\n") || p.contains("# Step: code-review\n"))
        .collect();
    assert!(judging.len() >= 2, "{}", judging.len());
    for p in judging {
        assert!(
            p.contains("- `payments`: How customers are charged or refunded"),
            "{p}"
        );
    }
}

// AC: PRD AC-069 (as above), when people merge.
// Behavior: the same issue with `merge.by = "owner"` -> one ready-to-merge notice that names
//     payments with the owner's words.
// @lane: integration
// @dependency: daemon, flow, gate, fake agent, FakeGitHub, RecordingChannel
#[test]
fn when_people_merge_the_ready_message_names_what_a_person_reviews() {
    let env = Env::new();
    let (mut d, _, channel) = touching_payments(&env, "[merge]\nby = \"owner\"\n");
    run_until(
        &mut d,
        90,
        |_| {},
        |d| stage(d, "ACME-123") == Stage::ReadyForOwner,
    );

    let ready = notices_of(&channel, NoticeKind::ReadyToMerge);
    assert_eq!(ready.len(), 1);
    assert!(
        ready[0].ends_with(
            "\nIt includes what a person reviews:\n- payments: How customers are charged or refunded"
        ),
        "{}",
        ready[0]
    );
}
