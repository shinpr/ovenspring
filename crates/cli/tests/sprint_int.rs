//! @lane: integration
//! Area: the sprint loop (proposal, approval, scheduling, review, agreements, next sprint) and the
//! product goal's reach into launches.

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

use ovenspring_core::sprint::Asked;
use ovenspring_core::step::Step;
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use ovenspring_core::flow::Stage;
use ovenspring_core::sprint::SprintStage;
use ovenspring_daemon::Daemon;
use ovenspring_daemon::owner::{DecisionRecord, DecisionStatus};
use ovenspring_integrations::channel::slack::Slack;
use ovenspring_integrations::channel::{Button, Channel, ChannelEvent, DecisionKind};
use ovenspring_integrations::tracker::TrackerIssue;
use serde_json::{Map, Value, json};
use support::fake_github::FakeGitHub;
use support::recording::RecordingTracker;
use support::recording::{RecordingChannel, Sent};
use support::scripts::{always_passing, implement_run, requiring_test, stage, straight};
use support::slack_stub::{SlackStub, deliver, message, press_on};
use support::{Env, daemon, git, run_until};

const GOAL: &str = "Make checkout reliable for returning customers";
const PURPOSE: &str = "Returning customers check out without retyping their address";
const OWNER: &str = "U0123456789";

const CONFIG: &str = r#"
[repo]
base_branch = "develop"
[review]
[limits]
issues_at_once = 3
[held.schema]
paths = ["db/migrations/**"]
"#;

fn issue(key: &str, depends_on: &[&str]) -> TrackerIssue {
    TrackerIssue {
        key: key.into(),
        title: format!("Title of {key}"),
        body: format!("What {key} asks for."),
        url: format!("https://linear.app/acme/issue/{key}"),
        state: "Todo".into(),
        depends_on: depends_on.iter().map(|d| (*d).to_owned()).collect(),
        ..TrackerIssue::default()
    }
}

fn plan(issues: &[(&str, &[&str])]) -> Value {
    let issues: Vec<Value> = issues
        .iter()
        .map(|(k, deps)| json!({"key": k, "depends_on": deps}))
        .collect();
    json!({"purpose": PURPOSE, "key_results": ["Saved addresses fill in at checkout"], "issues": issues,
           "message": format!("Sprint: {PURPOSE}")})
}

fn review(agreements: bool) -> Value {
    let agreements = if agreements {
        json!([{"file": "common.md", "content": "# Working agreements\n- Name the precedent a design follows.\n- Run migrations in the e2e lane.\n"}])
    } else {
        json!([])
    };
    json!({"output": {
        "key_results": [{"text": "Saved addresses fill in at checkout", "met": true, "evidence": "ACME-123 merged"}],
        "agreements": agreements, "message": "Review: the key result is met."
    }})
}

/// Sprint steps that pass: facts, a review, an agreements check that passes, and `next` as the lead's
/// proposal for the sprint that follows (call after the sprint's own `sprint-plan` runs are set).
fn sprint_steps(script: &mut Map<String, Value>, agreements: bool, next: &Value) {
    script.insert(
        "facts".into(),
        json!([{"output": {"facts_path": "facts.md"}}]),
    );
    script.insert("sprint-review".into(), json!([review(agreements)]));
    script
        .entry("sprint-plan")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .unwrap()
        .push(json!({"output": next}));
    script.insert(
        "agreements-check".into(),
        json!([{"output": {"findings": []}, "verdict": "pass"}]),
    );
}

/// Plays the owner: answers each newly pending decision with `answer(decision)`, once.
fn owner(
    channel: Arc<RecordingChannel>,
    answer: impl Fn(&DecisionRecord) -> ChannelEvent,
) -> impl FnMut(&Daemon) {
    let seen = Mutex::new(BTreeSet::new());
    move |d: &Daemon| {
        for decision in d.decisions().unwrap() {
            if decision.status == DecisionStatus::Pending
                && seen.lock().unwrap().insert(decision.id.clone())
            {
                channel.push_event(answer(&decision));
            }
        }
    }
}

fn approve(d: &DecisionRecord) -> ChannelEvent {
    ChannelEvent::Action {
        decision: d.id.clone(),
        button: Button::Approve,
        user: OWNER.into(),
    }
}

fn sprint_filed(env: &Env) -> Vec<String> {
    env.events()
        .iter()
        .filter(|e| e["event"] == "filed" && e.get("sprint").is_some())
        .map(|e| e["step"].as_str().unwrap().to_owned())
        .collect()
}

fn event_index(env: &Env, pred: impl Fn(&Value) -> bool) -> Option<usize> {
    env.events().iter().position(pred)
}

/// A first sprint that settles at once: its one issue, ACME-120, is closed at requirements as
/// already delivered. A proposal with no issue would start no sprint.
fn settling_sprint(script: &mut Map<String, Value>) {
    script.insert(
        "sprint-plan".into(),
        json!([{"output": plan(&[("ACME-120", &[])])}]),
    );
    script.insert(
        "requirements@ACME-120".into(),
        json!([{"output": {"decision": "close", "structuralScale": "small",
                           "convergence": {"outcome": "o"}, "evidence": "src/a.rs:1 already does it"}}]),
    );
}

fn setup(
    env: &Env,
    extra: &str,
    script: &Map<String, Value>,
    issues: Vec<TrackerIssue>,
) -> (
    Daemon,
    Arc<FakeGitHub>,
    Arc<RecordingChannel>,
    Arc<RecordingTracker>,
) {
    prepare(env, extra, script);
    let channel = Arc::new(RecordingChannel::default());
    let (d, gh, tracker) = start(env, issues, channel.clone());
    (d, gh, channel, tracker)
}

/// The repository configured with `extra` on `develop`, and the agents following `script`.
fn prepare(env: &Env, extra: &str, script: &Map<String, Value>) {
    env.configure(&format!("{CONFIG}{extra}"));
    env.add_branch("develop");
    env.script_agents(&Value::Object(script.clone()));
}

/// The daemon over the tracker's open `issues`, talking to the owner through `channel`, with the
/// goal set.
fn start(
    env: &Env,
    issues: Vec<TrackerIssue>,
    channel: Arc<dyn Channel>,
) -> (Daemon, Arc<FakeGitHub>, Arc<RecordingTracker>) {
    let gh = Arc::new(always_passing(env, requiring_test()));
    let tracker = Arc::new(RecordingTracker::default());
    *tracker.issues.lock().unwrap() = issues;
    let d = daemon(
        env,
        ovenspring_daemon::Services {
            github: Some(gh.clone()),
            channel,
            tracker: Some(tracker.clone()),
            secrets: Vec::new(),
        },
    );
    d.set_goal(GOAL).unwrap();
    (d, gh, tracker)
}

fn whole_sprint_script() -> Map<String, Value> {
    let mut script = straight("ACME-123");
    script.extend(straight("ACME-124"));
    script.extend(straight("ACME-125"));
    script.insert(
        "implement@ACME-125".into(),
        json!([implement_run("db/migrations/0002_add_index.sql")]),
    );
    for key in ["ACME-123", "ACME-124", "ACME-125"] {
        script.insert(
            format!("merge-base@{key}"),
            json!([{"git": [["fetch", "-q", "origin"], ["merge", "-q", "--no-edit", "origin/develop"]],
                    "output": {"status": "pass", "summary": "s", "checksPerformed": {}, "fixesApplied": []}}]),
        );
    }
    script.insert(
        "sprint-plan".into(),
        json!([
            {"output": plan(&[("ACME-123", &[]), ("ACME-124", &["ACME-123"]), ("ACME-125", &[]), ("ACME-126", &[])])},
            {"output": plan(&[("ACME-123", &[]), ("ACME-124", &["ACME-123"]), ("ACME-125", &[])])}
        ]),
    );
    sprint_steps(&mut script, true, &plan(&[("ACME-126", &[])]));
    script.insert("requirements@ACME-126".into(), json!([{"hang": true}]));
    script
}

/// The owner, in Slack: replies in the first proposal's thread, then presses Approve on every
/// other decision's message, once each.
fn replies_once_then_approves_in_slack(
    stub: SlackStub,
    slack: Arc<Slack<SlackStub>>,
) -> impl FnMut(&Daemon) {
    let seen = Mutex::new(BTreeSet::new());
    move |d: &Daemon| {
        for decision in d.decisions().unwrap() {
            if decision.status != DecisionStatus::Pending
                || !seen.lock().unwrap().insert(decision.id.clone())
            {
                continue;
            }
            let posted = stub.message_of(&decision.id);
            let first_proposal =
                decision.kind == DecisionKind::Sprint && decision.id.ends_with("-sprint-1");
            let answer = if first_proposal {
                message(
                    OWNER,
                    "Leave ACME-126 for the next sprint.",
                    "1800000100.000001",
                    posted["ts"].as_str(),
                )
            } else {
                press_on(&posted, "approve", OWNER)
            };
            deliver(&slack, &[answer]);
        }
    }
}

/// The content of each sprint proposal and review as posted in Slack.
fn posted_proposals_and_reviews(d: &Daemon, stub: &SlackStub) -> Vec<String> {
    d.decisions()
        .unwrap()
        .into_iter()
        .filter(|r| matches!(r.kind, DecisionKind::Sprint | DecisionKind::Review))
        .map(|r| {
            stub.message_of(&r.id)["blocks"][1]["text"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect()
}

fn assert_the_goal_and_purpose_reach_the_owner_and_every_launch(env: &Env, posted: &[String]) {
    assert_eq!(
        posted.len(),
        4,
        "two proposals, one review, and the next sprint's own proposal"
    );
    assert!(
        posted
            .iter()
            .all(|t| t.starts_with(&format!("Goal: {GOAL}")))
    );
    let prompts: Vec<String> = env
        .records()
        .iter()
        .map(|r| r["prompt"].as_str().unwrap().to_owned())
        .collect();
    // A resumed session's next turn names only its item; the session holds the rest.
    assert!(
        prompts
            .iter()
            .filter(|p| !p.starts_with("# Your work turn"))
            .all(|p| p.contains(GOAL))
    );
    let issue_prompts: Vec<&String> = prompts
        .iter()
        .filter(|p| p.contains("- Issue: ACME-"))
        .collect();
    assert_ne!(issue_prompts.len(), 0);
    assert!(
        issue_prompts.iter().all(|p| p.contains(PURPOSE)),
        "issue launches carry the sprint's purpose"
    );
}

fn assert_dependencies_and_the_held_merge(env: &Env, d: &Daemon, gh: &FakeGitHub) {
    let merged_123 =
        event_index(env, |e| e["event"] == "merged" && e["issue"] == "ACME-123").unwrap();
    let begun_124 =
        event_index(env, |e| e["event"] == "filed" && e["issue"] == "ACME-124").unwrap();
    let begun_125 =
        event_index(env, |e| e["event"] == "filed" && e["issue"] == "ACME-125").unwrap();
    assert!(
        begun_124 > merged_123,
        "ACME-124 starts after ACME-123 merges"
    );
    assert!(begun_125 < merged_123, "ACME-125 runs alongside ACME-123");
    // ACME-125's head changes when ACME-123 merges while it waits, which asks for approval again.
    let approved: Vec<String> = d
        .decisions()
        .unwrap()
        .into_iter()
        .filter(|x| {
            x.kind == DecisionKind::Merge
                && x.issue.as_deref() == Some("ACME-125")
                && x.status == DecisionStatus::Answered
        })
        .filter_map(|x| x.sha)
        .collect();
    let pr = d.issue("ACME-125").unwrap().unwrap().flow.pr.unwrap();
    let merges: Vec<String> = gh
        .calls()
        .into_iter()
        .filter(|c| c.starts_with(&format!("merge {pr} ")))
        .collect();
    let [merge] = merges.as_slice() else {
        panic!("ACME-125 merges once: {merges:?}");
    };
    assert!(
        approved
            .iter()
            .any(|sha| *merge == format!("merge {pr} {sha}")),
        "{merge} is a commit the owner approved: {approved:?}"
    );
}

// AC: docs/design/factory-core-design.md § Acceptance Criteria > Sprint: "When no sprint is running,
//     a product goal is set and the factory is not paused, the system shall file a `sprint-plan` item
//     for the lead with the goal and a snapshot of every open issue (AC-015)"; "When a `sprint-plan`
//     item is done with a valid proposal, the system shall post the proposal through the channel and
//     wait for the owner (AC-015)"; "When the owner approves a sprint proposal, the system shall
//     start the sprint and file `requirements` for every proposed issue whose dependencies are met,
//     up to the configured number of issues at once (AC-016)"; "If the owner replies to a proposal
//     without approving, then the system shall file a `sprint-plan` item that names the reply
//     (AC-017)"; "When every issue of the sprint is merged, held or dropped, the system shall file
//     `facts` for the analyzer, then `sprint-review` for the lead, then `agreements-check` for the
//     reviewer when agreements change, and post the review (AC-041)"; "When the owner approves a
//     sprint review that changes working agreements, the system shall push them as a pull request
//     that follows the merge setting (merged by the factory under `factory`, by the owner under
//     `owner`), and shall file no item of the next sprint until GitHub reports that pull request
//     merged (AC-027, AC-042)"; "Every launch's prompt shall contain the product goal and, during a
//     sprint, the sprint's purpose (AC-057)".
//     PRD AC-016: "...its independent issues run in parallel, each on its own branch and worktree."
//     PRD AC-028: "...in the `factory` setting the merge waits for the owner's approval in Slack."
//     Integration Verification Point: "A whole sprint with the fake agent, fake GitHub, fake tracker
//     and fake channel: proposal, approval, two issues with a dependency, one review round, CI
//     failure and fix, a held merge approved, review, agreements merged."
// Consolidation: the review round and the CI failure and fix run on the same daemon composition in
//     issue_flow_int.rs (one_issue_runs_from_requirements_to_the_merge_of_its_checked_sha); here the
//     issues take the shortest passing path so this test owns the sprint-level wiring.
// Behavior: RecordingTracker lists ACME-123, ACME-124 (blocked by ACME-123), ACME-125 (its change adds
//     `db/migrations/0002_add_index.sql`) and ACME-126 -> sprint-plan, proposal posted -> the owner
//     replies "Leave ACME-126 for the next sprint" -> revised proposal -> approval -> ACME-123 and
//     ACME-125 run in parallel, ACME-124 after ACME-123 merges, ACME-125 held and approved for its
//     sha -> facts, sprint-review, agreements-check -> review posted and approved -> the agreements
//     pull request merges -> the next sprint is proposed and approved on its own -> its first items
//     are filed.
// @lane: integration
// @dependency: daemon, flow (issue and sprint), gate, launcher, worktrees, store, fake agent, RecordingTracker, FakeGitHub, Slack channel
fn assert_the_facts_read_the_owners_settings(env: &Env) {
    // The facts step reads the owner's settings, which the base's worktree need not hold.
    let settings = env.state().join("sources/config.toml");
    let facts_item = std::fs::read_dir(env.state().join("board/archive"))
        .unwrap()
        .map(|e| std::fs::read_to_string(e.unwrap().path()).unwrap())
        .find(|t| t.contains("step: facts"))
        .unwrap();
    assert!(facts_item.contains(&settings.display().to_string()));
    assert_eq!(
        std::fs::read_to_string(&settings).unwrap(),
        std::fs::read_to_string(env.repo.join(".ovenspring/config.toml")).unwrap()
    );
}

fn assert_the_next_plan_reads_the_last_review(env: &Env, d: &Daemon, next: &str) {
    // The next plan reads the review it follows and the owner's answer to it.
    let ended = std::fs::read_dir(env.state().join("sprints"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|x| x == "json"))
        .unwrap();
    let review_answer = d
        .decisions()
        .unwrap()
        .into_iter()
        .find(|r| r.kind == DecisionKind::Review)
        .map(|r| env.state().join(format!("sources/answers/{}.md", r.id)))
        .unwrap();
    let next_plan = std::fs::read_dir(env.state().join("board/archive"))
        .unwrap()
        .map(|e| std::fs::read_to_string(e.unwrap().path()).unwrap())
        .find(|t| t.contains("step: sprint-plan") && t.contains(&format!("sprint: {next}")))
        .expect("the next sprint's proposal");
    assert!(
        next_plan.contains(&ended.display().to_string()),
        "{next_plan}"
    );
    assert!(
        next_plan.contains(&review_answer.display().to_string()),
        "{next_plan}"
    );
}

#[test]
fn whole_sprint_runs_from_proposal_to_the_start_of_the_next_sprint() {
    let env = Env::new();
    let issues = vec![
        issue("ACME-123", &[]),
        issue("ACME-124", &["ACME-123"]),
        issue("ACME-125", &[]),
        issue("ACME-126", &[]),
    ];
    let stub = SlackStub::default();
    let slack = stub.channel();
    prepare(&env, "", &whole_sprint_script());
    let (mut d, gh, _) = start(&env, issues, slack.clone());
    run_until(
        &mut d,
        120,
        replies_once_then_approves_in_slack(stub.clone(), slack),
        |_| {
            env.events()
                .iter()
                .any(|e| e["event"] == "filed" && e["issue"] == "ACME-126")
        },
    );

    assert_eq!(
        sprint_filed(&env),
        [
            "sprint-plan",
            "sprint-plan",
            "facts",
            "sprint-review",
            "agreements-check",
            "sprint-plan"
        ]
    );
    let second_plan = std::fs::read_dir(env.state().join("board/archive"))
        .unwrap()
        .map(|e| std::fs::read_to_string(e.unwrap().path()).unwrap())
        .filter(|t| t.contains("step: sprint-plan"))
        .find(|t| t.contains("/sources/answers/"))
        .expect("the second proposal names the owner's reply");
    assert!(second_plan.contains("sprint-"));
    assert_the_facts_read_the_owners_settings(&env);
    for key in ["ACME-123", "ACME-124", "ACME-125"] {
        assert_eq!(stage(&d, key), Stage::Merged, "{key}");
    }
    assert_the_goal_and_purpose_reach_the_owner_and_every_launch(
        &env,
        &posted_proposals_and_reviews(&d, &stub),
    );
    assert_dependencies_and_the_held_merge(&env, &d, &gh);
    // The issue's page links to the issue on the tracker it came from.
    let detail = ovenspring_daemon::history::issue_detail(d.store(), "ACME-125")
        .unwrap()
        .unwrap();
    assert_eq!(
        detail.tracker.as_deref(),
        Some("https://linear.app/acme/issue/ACME-125")
    );
    assert!(
        git(
            &env.origin,
            &["show", "develop:.ovenspring/agreements/common.md"]
        )
        .contains("Run migrations in the e2e lane.")
    );
    let next = d.sprint().unwrap().unwrap();
    assert_the_next_plan_reads_the_last_review(&env, &d, &next.id);
    assert_eq!(next.stage, SprintStage::Running);
    assert_eq!(next.plan.unwrap().issues[0].key, "ACME-126");
    let next_proposal_approved = event_index(&env, |e| {
        e["event"] == "decision-answered"
            && e["id"]
                .as_str()
                .is_some_and(|id| id.starts_with(&format!("sprint-{}-", next.id)))
    })
    .expect("the next sprint's proposal is answered");
    let begun_126 =
        event_index(&env, |e| e["event"] == "filed" && e["issue"] == "ACME-126").unwrap();
    assert!(
        next_proposal_approved < begun_126,
        "the next sprint's issues start only after its own proposal is approved"
    );
}

// AC: "When a `requirements` output decides `close`, the system shall comment the evidence on the
//     tracker issue, close it, and mark the issue dropped in the sprint (AC-018)" —
//     docs/design/factory-core-design.md § Acceptance Criteria > Issue; § Issue states: "queued -> a
//     dependency held or dropped -> exception (the lead proceeds, replans, or drops this issue)".
//     Integration Verification Point: "A dependency dropped while its dependent is queued files
//     `exception` for the dependent".
// Behavior: a running sprint with ACME-123 and ACME-124 (blocked by ACME-123); ACME-123's
//     `requirements` output decides `close` with evidence "Already delivered by src/checkout/retry.rs"
//     -> the daemon comments the evidence and closes ACME-123 in RecordingTracker and marks it dropped ->
//     ACME-124 gets an `exception` item for the lead instead of staying queued.
// @lane: integration
// @dependency: daemon, flow, store, fake agent, RecordingTracker
#[test]
fn dependency_closed_as_already_delivered_files_exception_for_its_queued_dependent() {
    let env = Env::new();
    let mut script = Map::new();
    script.insert(
        "sprint-plan".into(),
        json!([{"output": plan(&[("ACME-123", &[]), ("ACME-124", &["ACME-123"])])}]),
    );
    script.insert(
        "requirements@ACME-123".into(),
        json!([{"output": {"decision": "close", "structuralScale": "small",
                           "convergence": {"outcome": "Checkout retries a failed payment"},
                           "evidence": "Already delivered by src/checkout/retry.rs"}}]),
    );
    script.insert("exception@ACME-124".into(), json!([{"hang": true}]));
    let (mut d, _, channel, tracker) = setup(
        &env,
        "",
        &script,
        vec![issue("ACME-123", &[]), issue("ACME-124", &["ACME-123"])],
    );
    run_until(&mut d, 60, owner(channel, approve), |d| {
        matches!(stage(d, "ACME-124"), Stage::Exception { .. })
    });

    let calls = tracker.calls();
    let comment = calls
        .iter()
        .position(|(k, c)| {
            k == "ACME-123" && c.contains("Already delivered by src/checkout/retry.rs")
        })
        .unwrap();
    let closed = calls
        .iter()
        .position(|(k, c)| k == "ACME-123" && c == "state: Canceled")
        .unwrap();
    assert!(comment < closed);
    assert_eq!(stage(&d, "ACME-123"), Stage::Dropped);
    let filed: Vec<String> = support::scripts::filed_steps(&env, "ACME-124");
    assert_eq!(
        filed,
        ["exception"],
        "the lead decides; ACME-124 does not start on its own"
    );
}

// AC: docs/design/owner-interface-design.md § Data Contracts > Goal: "goal.md replaced; events.jsonl
//     gains {event: goal-set, at, old, new}"; "Guarantees: next sprint-plan and every later launch
//     read the new text". docs/design/factory-core-design.md § Sprint: "Every launch's prompt shall
//     contain the product goal and, during a sprint, the sprint's purpose (AC-057)".
//     PRD AC-056: "...the next sprint proposal uses the goal as it stands, and the running sprint
//     continues."
// Behavior: a sprint with purpose "Returning customers check out without retyping their address"
//     runs; ACME-123's `requirements` launch is in progress -> the owner saves "Make checkout fast on
//     mobile" as the goal (the dashboard's route) -> the goal is replaced -> the running launch and
//     sprint continue, and later launches carry the new goal.
// @lane: integration
// @dependency: daemon, goal store, prompt assembly, launcher, fake agent
#[test]
fn goal_rewritten_mid_sprint_reaches_later_launches_without_stopping_the_sprint() {
    let env = Env::new();
    let mut script = Map::new();
    script.insert(
        "sprint-plan".into(),
        json!([{"output": plan(&[("ACME-123", &[])])}]),
    );
    script.insert(
        "requirements@ACME-123".into(),
        json!([{"wait": true, "output": {"convergence": {"outcome": "o"}, "structuralScale": "medium", "decision": "proceed"}}]),
    );
    script.insert("codebase-analysis@ACME-123".into(), json!([{"hang": true}]));
    let (mut d, _, channel, _) = setup(&env, "", &script, vec![issue("ACME-123", &[])]);
    let root = env.fake_bin.parent().unwrap().to_path_buf();
    run_until(&mut d, 60, owner(channel.clone(), approve), |_| {
        root.join("pids-ACME-123-requirements.json").exists()
    });

    let new_goal = "Make checkout fast on mobile";
    d.set_goal(new_goal).unwrap();
    assert!(d.set_goal("  ").is_err());
    for _ in 0..5 {
        d.tick().unwrap();
    }
    assert_eq!(
        d.goal().unwrap(),
        new_goal,
        "the empty goal changed nothing"
    );
    let changes: Vec<Value> = env
        .events()
        .into_iter()
        .filter(|e| e["event"] == "goal-set")
        .collect();
    assert_eq!(changes.len(), 2, "the first goal and the change");
    assert_eq!(
        (changes[1]["old"].as_str(), changes[1]["new"].as_str()),
        (Some(GOAL), Some(new_goal))
    );
    assert_eq!(
        d.sprint().unwrap().unwrap().stage,
        SprintStage::Running,
        "the sprint goes on"
    );
    assert_eq!(
        stage(&d, "ACME-123"),
        Stage::running(Step::Requirements),
        "the running launch goes on"
    );

    std::fs::write(root.join("release-ACME-123-requirements"), "").unwrap();
    run_until(
        &mut d,
        30,
        |_| {},
        |_| root.join("pids-ACME-123-codebase-analysis.json").exists(),
    );
    let prompts: Vec<String> = env
        .records()
        .iter()
        .map(|r| r["prompt"].as_str().unwrap().to_owned())
        .collect();
    let requirements = prompts
        .iter()
        .find(|p| p.contains("requirements-ACME-123"))
        .unwrap();
    let analysis = prompts
        .iter()
        .find(|p| p.contains("codebase-analysis-ACME-123"))
        .unwrap();
    assert!(requirements.contains(GOAL) && !requirements.contains(new_goal));
    assert!(analysis.contains(new_goal) && analysis.contains(PURPOSE));
}

// AC: "When the owner approves a sprint review that changes working agreements, the system shall
//     push them as a pull request that follows the merge setting (merged by the factory under
//     `factory`, by the owner under `owner`), and shall file no item of the next sprint until GitHub
//     reports that pull request merged (AC-027, AC-042)" — docs/design/factory-core-design.md
//     § Acceptance Criteria > Sprint.
//     UI Spec M-07: "under `owner`, the thread links it for the owner to merge".
// Behavior: `merge.by = "owner"`; the sprint is `awaiting-review-approval` with an agreements patch
//     -> the approver approves the review -> the agreements pull request is opened and linked in the
//     thread -> FakeGitHub later reports it merged by the owner -> the next sprint's first items are
//     filed.
// @lane: integration
// @dependency: daemon, sprint flow, store, FakeGitHub, RecordingChannel
#[test]
fn owner_merge_setting_starts_the_next_sprint_only_after_github_reports_the_agreements_pr_merged() {
    let env = Env::new();
    let mut script = Map::new();
    settling_sprint(&mut script);
    sprint_steps(&mut script, true, &plan(&[("ACME-126", &[])]));
    script.insert("requirements@ACME-126".into(), json!([{"hang": true}]));
    let (mut d, gh, channel, _) = setup(
        &env,
        "[merge]\nby = \"owner\"\n",
        &script,
        vec![issue("ACME-120", &[]), issue("ACME-126", &[])],
    );
    run_until(&mut d, 60, owner(channel.clone(), approve), |d| {
        d.sprint()
            .unwrap()
            .is_some_and(|s| s.agreements_pr.is_some())
    });
    for _ in 0..10 {
        d.tick().unwrap();
    }
    let pr = d.sprint().unwrap().unwrap().agreements_pr.unwrap();
    assert!(
        d.sprint()
            .unwrap()
            .unwrap()
            .agreements_url
            .is_some_and(|u| u.ends_with(&format!("/pull/{pr}"))),
        "the sprint keeps the pull request's address for the owner"
    );
    assert!(
        !gh.calls().iter().any(|c| c.starts_with("merge ")),
        "the factory never merges under `owner`"
    );
    assert!(
        support::scripts::filed_steps(&env, "ACME-126").is_empty(),
        "no item of the next sprint yet"
    );
    let branch = git(
        &env.origin,
        &[
            "for-each-ref",
            "--format=%(refname:short)",
            "refs/heads/ovenspring/agreements-*",
        ],
    );
    let touched = git(
        &env.origin,
        &["diff", "--name-only", &format!("develop...{branch}")],
    );
    assert_eq!(touched, ".ovenspring/agreements/common.md");
    assert!(
        channel.sent().iter().any(
            |m| matches!(m, Sent::Reply { text, .. } if text.ends_with(&format!("/pull/{pr}")))
        )
    );

    gh.merge_as_owner(pr);
    // The next sprint is proposed on its own; the owner approves that proposal.
    run_until(&mut d, 30, owner(channel, approve), |_| {
        support::scripts::filed_steps(&env, "ACME-126")
            .first()
            .map(String::as_str)
            == Some("requirements")
    });
}

// AC: "When the owner approves a sprint review that changes working agreements, the system shall
//     push them as a pull request that follows the merge setting" (AC-027, AC-042); UI Spec M-07:
//     the thread links it under either setting.
// Behavior: `merge.by = "factory"`; the owner closes a review that changes the agreements -> the
//     pull request opens -> the review's thread says what the owner does: approve it on GitHub
//     when the base branch requires an approving review, nothing otherwise.
// Cases (reported distinctly): one approving review required; none required.
// @lane: integration
// @dependency: daemon, sprint flow, FakeGitHub, RecordingChannel
#[test]
fn the_agreements_reply_asks_the_owner_to_approve_only_when_github_requires_it() {
    for (approvals_required, expected) in [
        (
            1,
            "To finish the sprint, approve the working agreements and knowledge on GitHub.",
        ),
        (
            0,
            "The factory merges the working agreements and knowledge once their checks pass, then proposes the next sprint. Nothing is needed from you.",
        ),
    ] {
        let env = Env::new();
        let mut script = Map::new();
        settling_sprint(&mut script);
        sprint_steps(&mut script, true, &plan(&[("ACME-126", &[])]));
        script.insert("requirements@ACME-126".into(), json!([{"hang": true}]));
        prepare(&env, "", &script);
        let mut gh = always_passing(&env, requiring_test());
        gh.approvals_required = approvals_required;
        let gh = Arc::new(gh);
        let channel = Arc::new(RecordingChannel::default());
        let tracker = Arc::new(RecordingTracker::default());
        *tracker.issues.lock().unwrap() = vec![issue("ACME-120", &[]), issue("ACME-126", &[])];
        let mut d = daemon(
            &env,
            ovenspring_daemon::Services {
                github: Some(gh.clone()),
                channel: channel.clone(),
                tracker: Some(tracker),
                secrets: Vec::new(),
            },
        );
        d.set_goal(GOAL).unwrap();

        let agreements_reply = |channel: &RecordingChannel| {
            channel.sent().into_iter().find_map(|m| {
                if let Sent::Reply { text, .. } = m
                    && text.contains("working agreements and knowledge")
                {
                    Some(text)
                } else {
                    None
                }
            })
        };
        let watched = channel.clone();
        run_until(&mut d, 60, owner(channel.clone(), approve), move |_| {
            agreements_reply(&watched).is_some()
        });

        let reply = agreements_reply(&channel).unwrap_or_default();
        assert!(
            reply.starts_with(expected),
            "{approvals_required} approvals required: {reply}"
        );
        if approvals_required > 0 {
            assert!(
                d.sprint().unwrap().unwrap().agreements_need_approval,
                "the dashboard shows it waiting for the owner's approval"
            );
        }
    }
}

// Behavior: the approved review leaves `common.md` with nothing (empty content) -> the agreements
//     pull request deletes the file rather than committing it empty.
// @lane: integration
// @dependency: daemon, sprint flow, FakeGitHub
#[test]
fn an_agreements_file_the_review_leaves_empty_is_deleted() {
    let env = Env::new();
    let mut script = Map::new();
    settling_sprint(&mut script);
    sprint_steps(&mut script, false, &plan(&[]));
    let mut emptied = review(false);
    emptied["output"]["agreements"] = json!([{"file": "common.md", "content": ""}]);
    script.insert("sprint-review".into(), json!([emptied]));
    let (mut d, _, channel, _) = setup(
        &env,
        "[merge]\nby = \"owner\"\n",
        &script,
        vec![issue("ACME-120", &[])],
    );
    run_until(&mut d, 60, owner(channel.clone(), approve), |d| {
        d.sprint()
            .unwrap()
            .is_some_and(|s| s.agreements_pr.is_some())
    });
    let branch = git(
        &env.origin,
        &[
            "for-each-ref",
            "--format=%(refname:short)",
            "refs/heads/ovenspring/agreements-*",
        ],
    );
    assert_eq!(
        git(
            &env.origin,
            &["diff", "--name-status", &format!("develop...{branch}")]
        ),
        "D\t.ovenspring/agreements/common.md"
    );
}

// AC: PRD AC-070: "The sprint review keeps the factory's knowledge ... its changes reach the base
//     branch through the same pull request as the working agreements, in the folder the owner set,
//     and the next sprint starts once it merges."
// Behavior: `agreements.path = "docs/factory/agreements"` and `knowledge.path = "docs/knowledge"`,
//     with the lead's agreement merged in the first -> the lead's launches read it; the closed review
//     changes the shared agreement and adds one knowledge file -> the review's pull request adds
//     those two files in those folders and nothing else.
// @lane: integration
// @dependency: daemon, sprint flow, FakeGitHub
#[test]
fn agreements_and_knowledge_are_read_and_written_in_the_folders_the_owner_set() {
    let env = Env::new();
    std::fs::create_dir_all(env.repo.join("docs/factory/agreements")).unwrap();
    std::fs::write(
        env.repo.join("docs/factory/agreements/lead.md"),
        "- Plan the smallest sprint that reaches the purpose.\n",
    )
    .unwrap();
    let mut script = Map::new();
    settling_sprint(&mut script);
    sprint_steps(&mut script, true, &plan(&[]));
    let mut learned = review(true);
    learned["output"]["knowledge"] = json!([{
        "file": "checkout/saved-addresses.md",
        "content": "# Saved addresses come from the account service\n"
    }]);
    script.insert("sprint-review".into(), json!([learned]));
    let (mut d, _, channel, _) = setup(
        &env,
        "[merge]\nby = \"owner\"\n[agreements]\npath = \"docs/factory/agreements\"\n[knowledge]\npath = \"docs/knowledge\"\n",
        &script,
        vec![issue("ACME-120", &[])],
    );
    run_until(&mut d, 60, owner(channel.clone(), approve), |d| {
        d.sprint()
            .unwrap()
            .is_some_and(|s| s.agreements_pr.is_some())
    });
    let branch = git(
        &env.origin,
        &[
            "for-each-ref",
            "--format=%(refname:short)",
            "refs/heads/ovenspring/agreements-*",
        ],
    );
    assert_eq!(
        git(
            &env.origin,
            &["diff", "--name-status", &format!("develop...{branch}")]
        ),
        "A\tdocs/factory/agreements/common.md\nA\tdocs/knowledge/checkout/saved-addresses.md"
    );
    let plan_prompt = env
        .records()
        .into_iter()
        .find_map(|r| {
            let prompt = r["prompt"].as_str()?.to_owned();
            prompt.contains("# Step: sprint-plan").then_some(prompt)
        })
        .unwrap();
    assert!(
        plan_prompt.contains("- Plan the smallest sprint that reaches the purpose."),
        "{plan_prompt}"
    );
}

// AC: "While a prerequisite is missing (... the tracker or Slack connection), ... the factory shall
//     start no work that needs it (AC-060)" — docs/design/owner-interface-design.md.
// Behavior: a goal is set but no tracker is connected -> the daemon runs -> no sprint is proposed,
//     since a proposal is made from the open issues the tracker holds.
// @lane: integration
// @dependency: daemon, sprint flow
#[test]
fn no_sprint_is_proposed_while_no_tracker_is_connected() {
    let env = Env::new();
    env.configure(CONFIG);
    env.script_agents(&json!({"sprint-plan": [{"output": plan(&[])}]}));
    let gh: Arc<dyn ovenspring_integrations::github::GitHub> =
        Arc::new(always_passing(&env, requiring_test()));
    let mut d = daemon(
        &env,
        ovenspring_daemon::Services {
            github: Some(gh),
            channel: Arc::new(RecordingChannel::default()),
            tracker: None,
            secrets: Vec::new(),
        },
    );
    d.set_goal(GOAL).unwrap();
    for _ in 0..10 {
        d.tick().unwrap();
    }
    assert_eq!(d.sprint().unwrap(), None);
    assert!(env.records().is_empty(), "no launch");
}

// AC: "While a prerequisite is missing (the GitHub, tracker or Slack connection), ... the factory
//     shall start no work that needs it (AC-060)" — docs/design/owner-interface-design.md.
// Behavior: a goal and an open issue, but no GitHub connection -> the daemon runs -> no sprint is
//     proposed and nothing is launched, since every issue ends in a push.
// @lane: integration
// @dependency: daemon, sprint flow
#[test]
fn no_sprint_is_proposed_while_github_is_not_connected() {
    let env = Env::new();
    env.configure(CONFIG);
    env.add_branch("develop");
    env.script_agents(&json!({"sprint-plan": [{"output": plan(&[("ACME-1", &[])])}]}));
    let tracker = Arc::new(RecordingTracker::default());
    tracker.issues.lock().unwrap().push(issue("ACME-1", &[]));
    let mut d = daemon(
        &env,
        ovenspring_daemon::Services {
            github: None,
            channel: Arc::new(RecordingChannel::default()),
            tracker: Some(tracker),
            secrets: Vec::new(),
        },
    );
    d.set_goal(GOAL).unwrap();
    for _ in 0..10 {
        d.tick().unwrap();
    }
    assert_eq!(d.sprint().unwrap(), None);
    assert!(env.records().is_empty(), "no launch");
}

// AC: PRD AC-015: "A proposal with no issue starts no sprint; the owner is told what is missing, and
//     the lead proposes again once an open issue appears."
// Behavior: no open issue serves the goal, so the lead's proposal holds none -> the owner receives
//     the lead's message as a notice, with nothing to approve, and the overview links to it -> an
//     issue opens on the tracker -> `sprint-plan` is filed again.
// @lane: integration
// @dependency: daemon, sprint flow, store, fake agent, RecordingTracker, RecordingChannel
#[test]
fn a_proposal_with_no_issue_tells_the_owner_what_is_missing_and_proposes_again_when_one_opens() {
    let env = Env::new();
    let mut empty = plan(&[]);
    empty["message"] = json!("No open issue serves the goal yet: saved addresses need an issue.");
    let mut script = Map::new();
    script.insert(
        "sprint-plan".into(),
        json!([{"output": empty}, {"output": plan(&[("ACME-126", &[])])}]),
    );
    script.insert("requirements@ACME-126".into(), json!([{"hang": true}]));
    let (mut d, _, channel, tracker) = setup(&env, "", &script, vec![]);
    run_until(
        &mut d,
        30,
        |_| {},
        |d| {
            d.sprint()
                .unwrap()
                .is_some_and(|s| s.stage == SprintStage::WaitingForIssues)
        },
    );
    assert!(channel.sent().iter().any(|m| matches!(m, Sent::Notice(n)
        if n.kind == ovenspring_integrations::channel::NoticeKind::WaitingForIssues
            && n.text.starts_with(&format!("Goal: {GOAL}"))
            && n.text.contains("saved addresses need an issue"))));
    assert!(d.decisions().unwrap().is_empty(), "nothing to approve");
    let told = ovenspring_daemon::overview::overview(d.store(), None, None)
        .unwrap()
        .told_waiting
        .unwrap();
    assert!(
        told.link
            .is_some_and(|l| l.starts_with("https://slack.test/archives/C-TEST/")),
        "the overview links to the lead's message"
    );

    *tracker.issues.lock().unwrap() = vec![issue("ACME-126", &[])];
    run_until(
        &mut d,
        30,
        |_| {},
        |_| {
            sprint_filed(&env)
                .iter()
                .filter(|s| *s == "sprint-plan")
                .count()
                == 2
        },
    );
}

// AC: PRD AC-015: the lead proposes a sprint "from the product goal and every open issue"; an issue
//     someone is already working on (Linear's `started` states) is theirs, not open work.
// Behavior: the tracker lists ACME-130, started by a person, and ACME-131, not started -> only
//     ACME-131 reaches the lead's sources and the sprint -> the factory starts ACME-131 and the
//     tracker then reports it started -> it stays among the factory's issues; ACME-130 never enters.
// @lane: integration
// @dependency: daemon, sprint flow, store, fake agent, RecordingTracker, RecordingChannel
#[test]
fn an_issue_someone_else_started_is_left_to_them_and_the_factorys_own_stay() {
    let env = Env::new();
    let mut script = Map::new();
    script.insert(
        "sprint-plan".into(),
        json!([{"output": plan(&[("ACME-131", &[])])}]),
    );
    script.insert("requirements@ACME-131".into(), json!([{"hang": true}]));
    let theirs = TrackerIssue {
        state: "In Progress".into(),
        started: true,
        ..issue("ACME-130", &[])
    };
    let (mut d, _, channel, tracker) =
        setup(&env, "", &script, vec![theirs, issue("ACME-131", &[])]);
    run_until(&mut d, 60, owner(channel, approve), |_| {
        support::scripts::filed_steps(&env, "ACME-131")
            .first()
            .map(String::as_str)
            == Some("requirements")
    });
    assert!(!env.state().join("sources/issues/ACME-130.md").exists());

    for i in tracker.issues.lock().unwrap().iter_mut() {
        i.started = true;
    }
    d.tick().unwrap();
    let open: Vec<&str> = d.open_issues().iter().map(|i| i.key.as_str()).collect();
    assert_eq!(open, ["ACME-131"]);
}

// AC: PRD AC-015: the lead proposes "from the product goal and every open issue".
// Behavior: ACME-131 is in the sprint and ACME-132 is open outside it -> the tracker stops
//     reporting both -> ACME-132's snapshot is gone, so no reader takes its old state as current;
//     ACME-131's stays while the factory still works on it.
// @lane: integration
// @dependency: daemon, store, fake agent, RecordingTracker, RecordingChannel
#[test]
fn an_issue_the_tracker_no_longer_reports_open_leaves_the_sources_unless_the_factory_holds_it() {
    let env = Env::new();
    let mut script = Map::new();
    script.insert(
        "sprint-plan".into(),
        json!([{"output": plan(&[("ACME-131", &[])])}]),
    );
    script.insert("requirements@ACME-131".into(), json!([{"hang": true}]));
    let (mut d, _, channel, tracker) = setup(
        &env,
        "",
        &script,
        vec![issue("ACME-131", &[]), issue("ACME-132", &[])],
    );
    run_until(&mut d, 60, owner(channel, approve), |_| {
        support::scripts::filed_steps(&env, "ACME-131")
            .first()
            .map(String::as_str)
            == Some("requirements")
    });
    let sources = env.state().join("sources/issues");
    assert!(sources.join("ACME-132.md").exists());

    tracker.issues.lock().unwrap().clear();
    d.tick().unwrap();
    assert!(!sources.join("ACME-132.md").exists());
    assert!(sources.join("ACME-131.md").exists());
}

// AC: "When the owner approves a sprint proposal in Slack, then the sprint starts and its
//     independent issues run in parallel" (AC-016).
// Behavior: the sprint was saved running and the factory stopped before putting its plan's issues
//     on the board -> one tick -> both issues are in the sprint, and the sprint is not reviewed
//     while they have not ended.
// @lane: integration
// @dependency: daemon, sprint flow, RecordingTracker
#[test]
fn a_running_sprint_puts_the_plans_issues_on_the_board_that_are_not_there() {
    let env = Env::new();
    let mut script = Map::new();
    script.insert(
        "requirements".into(),
        json!([{"hang": true}, {"hang": true}]),
    );
    let (mut d, _, _, _) = setup(
        &env,
        "",
        &script,
        vec![issue("ACME-131", &[]), issue("ACME-132", &[])],
    );
    let (mut state, _) = ovenspring_core::sprint::begin("s1");
    state.stage = SprintStage::Running;
    state.plan = Some(ovenspring_core::output::SprintPlanOutput {
        purpose: "Shoppers find products by what they do.".into(),
        key_results: vec!["A search by attribute returns the product.".into()],
        issues: ["ACME-131", "ACME-132"]
            .map(|key| ovenspring_core::output::PlannedIssue {
                key: key.into(),
                depends_on: vec![],
            })
            .into(),
        message: "The sprint.".into(),
    });
    d.store().write_record("sprint.json", &state).unwrap();

    d.tick().unwrap();

    for key in ["ACME-131", "ACME-132"] {
        let record = d.issue(key).unwrap().unwrap();
        assert_eq!(record.sprint.as_deref(), Some("s1"), "{key}");
    }
    assert_eq!(d.sprint().unwrap().unwrap().stage, SprintStage::Running);
}

// Behavior: a done `sprint-plan` item whose output file is missing -> one tick -> the reason is
//     logged as `unreadable-output` and the lead takes the stopped step -> the lead retries -> the
//     sprint returns to proposing and its step is filed again.
// @lane: integration
// @dependency: daemon, board, sprint flow, RecordingChannel
#[test]
fn unreadable_sprint_output_is_logged_and_the_lead_runs_the_step_again() {
    use ovenspring_core::item::Status;

    let env = Env::new();
    let mut script = Map::new();
    script.insert(
        "exception".into(),
        json!([{"output": {"action": "retry", "note": "The output was lost; write it again."}}]),
    );
    let (mut d, _, _, _) = setup(&env, "", &script, vec![]);
    let (state, _) = ovenspring_core::sprint::begin("s1");
    d.store().write_record("sprint.json", &state).unwrap();
    let name = "20260101-000000-sprint-plan-sprint-s1";
    let mut item = support::item(d.store(), name, Step::SprintPlan, None);
    item.front.status = Status::Done;
    item.front.sprint = Some("s1".into());
    d.store().file_item(name, &item).unwrap();

    d.tick().unwrap();

    assert!(
        event_index(&env, |e| e["event"] == "unreadable-output"
            && e["step"] == "sprint-plan")
        .is_some(),
        "the unreadable output is logged"
    );
    let stopped = d.sprint().unwrap().unwrap();
    assert_eq!(
        (stopped.stage, stopped.failed.as_deref()),
        (SprintStage::Exception, Some("its sprint-plan step"))
    );

    // The lead chooses to run the step again: the sprint returns to proposing and files it.
    run_until(
        &mut d,
        30,
        |_| {},
        |d| d.sprint().unwrap().unwrap().stage == SprintStage::Proposing,
    );
    assert_eq!(
        sprint_filed(&env).last().map(String::as_str),
        Some("sprint-plan")
    );
}

// Behavior: opening the agreements pull request fails once (GitHub unreachable) -> the failure is
//     logged and the sprint keeps applying agreements -> the next poll opens it -> it merges and
//     the next sprint starts.
#[test]
fn an_agreements_pull_request_that_failed_to_open_is_opened_on_a_later_poll() {
    let env = Env::new();
    let mut script = Map::new();
    settling_sprint(&mut script);
    sprint_steps(&mut script, true, &plan(&[("ACME-126", &[])]));
    script.insert("requirements@ACME-126".into(), json!([{"hang": true}]));
    let (mut d, gh, channel, _) = setup(
        &env,
        "",
        &script,
        vec![issue("ACME-120", &[]), issue("ACME-126", &[])],
    );
    gh.fail_next_open
        .store(true, std::sync::atomic::Ordering::SeqCst);
    run_until(&mut d, 60, owner(channel, approve), |_| {
        support::scripts::filed_steps(&env, "ACME-126")
            .first()
            .map(String::as_str)
            == Some("requirements")
    });
    assert!(event_index(&env, |e| e["event"] == "agreements-error").is_some());
    assert_eq!(
        gh.calls()
            .iter()
            .filter(|c| c.starts_with("open_pull ovenspring/agreements-"))
            .count(),
        2
    );
}

/// A sprint whose agreements pull request's required check fails until `fixed` is set, with the
/// lead's `exception` proposing a recovery; run until that recovery reaches the owner, the owner
/// approving the proposal and the review but leaving the recovery.
fn agreements_failing_their_check() -> (
    Env,
    Daemon,
    Arc<FakeGitHub>,
    Arc<RecordingChannel>,
    Arc<std::sync::atomic::AtomicBool>,
) {
    let env = Env::new();
    let mut script = Map::new();
    settling_sprint(&mut script);
    sprint_steps(&mut script, true, &plan(&[("ACME-126", &[])]));
    script.insert("requirements@ACME-126".into(), json!([{"hang": true}]));
    script.insert(
        "exception".into(),
        json!([{"output": {"action": "recover", "note": "The deploy check refuses the commit's author",
            "procedure": "#!/bin/sh\necho rerun the deploy check\n", "summary": "The deploy check failed for a reason outside the code."}}]),
    );
    prepare(&env, "", &script);
    let fixed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let passing = fixed.clone();
    let gh = Arc::new(FakeGitHub::new(
        env.origin.clone(),
        requiring_test(),
        Box::new(move |_| {
            let state = if passing.load(std::sync::atomic::Ordering::SeqCst) {
                ovenspring_integrations::github::CheckState::Success
            } else {
                ovenspring_integrations::github::CheckState::Failure
            };
            vec![("test".to_owned(), state)]
        }),
    ));
    let tracker = Arc::new(RecordingTracker::default());
    *tracker.issues.lock().unwrap() = vec![issue("ACME-120", &[]), issue("ACME-126", &[])];
    let channel = Arc::new(RecordingChannel::default());
    let mut d = daemon(
        &env,
        ovenspring_daemon::Services {
            github: Some(gh.clone()),
            channel: channel.clone(),
            tracker: Some(tracker),
            secrets: Vec::new(),
        },
    );
    d.set_goal(GOAL).unwrap();
    // The owner approves the proposal and the review, and leaves the recovery for later.
    let seen = Mutex::new(BTreeSet::new());
    let presses = channel.clone();
    let owner_approves = move |d: &Daemon| {
        for x in d.decisions().unwrap() {
            if x.status == DecisionStatus::Pending
                && x.kind != DecisionKind::Recovery
                && seen.lock().unwrap().insert(x.id.clone())
            {
                presses.push_event(approve(&x));
            }
        }
    };
    run_until(&mut d, 60, owner_approves, |d| {
        d.decisions()
            .unwrap()
            .iter()
            .any(|x| x.kind == DecisionKind::Recovery && x.issue.is_none())
    });
    (env, d, gh, channel, fixed)
}

// AC: docs/design/factory-core-design.md § Acceptance Criteria > Observation, exceptions and
//     recovery: "If the agreements pull request's checks fail, then the system shall file
//     `exception` for the lead on the sprint (AC-062)"; "When an owner approves M-09 ... or when an
//     owner presses Resolved, it shall clear the stop, read the current state and run the stopped
//     operation again (AC-064)". Sprint states: "The agreements commit takes the repository's git
//     identity, as every commit the factory makes".
// Behavior: the review is approved and the agreements pull request is opened -> its required
//     check fails in a way no agent can fix -> the sprint stops and the lead's `exception` proposes
//     a recovery (M-09) -> the owner fixes the check and presses Resolved -> the factory merges the
//     agreements and the next sprint starts.
// @lane: integration
// @dependency: daemon, sprint flow, gate, owner channel, fake agent, FakeGitHub
#[test]
fn a_failed_check_on_the_agreements_reaches_the_lead_and_the_next_sprint_starts_once_resolved() {
    let (env, mut d, gh, channel, fixed) = agreements_failing_their_check();
    assert_eq!(
        d.sprint().unwrap().unwrap().stage,
        SprintStage::Asking {
            what: Asked::Recovery
        }
    );
    assert_eq!(
        d.sprint().unwrap().unwrap().failed.as_deref(),
        Some("the agreements pull request's checks")
    );
    assert!(sprint_filed(&env).contains(&"exception".to_owned()));
    assert_eq!(
        support::scripts::filed_steps(&env, "ACME-126"),
        Vec::<String>::new()
    );
    let branch = git(
        &env.origin,
        &[
            "for-each-ref",
            "--format=%(refname:short)",
            "refs/heads/ovenspring/agreements-*",
        ],
    );
    assert_eq!(
        git(&env.origin, &["log", "-1", "--format=%an <%ae>", &branch]),
        "Storefront Dev <dev@storefront.test>"
    );

    fixed.store(true, std::sync::atomic::Ordering::SeqCst);
    let recovery = d
        .decisions()
        .unwrap()
        .into_iter()
        .find(|x| x.kind == DecisionKind::Recovery)
        .unwrap();
    channel.push_event(ChannelEvent::Action {
        decision: recovery.id,
        button: Button::Resolved,
        user: OWNER.into(),
    });
    // The next sprint is proposed on its own; the owner approves that proposal.
    run_until(&mut d, 60, owner(channel.clone(), approve), |_| {
        support::scripts::filed_steps(&env, "ACME-126")
            .first()
            .map(String::as_str)
            == Some("requirements")
    });
    let merged = d
        .store()
        .read_record::<ovenspring_core::sprint::SprintState>(&format!(
            "sprints/{}.json",
            recovery_sprint(&env)
        ))
        .unwrap()
        .unwrap();
    let pr = merged.agreements_pr.unwrap();
    assert!(
        gh.calls()
            .iter()
            .any(|c| c.starts_with(&format!("merge {pr} "))),
        "the factory merged the agreements"
    );
}

// AC: "When an owner approves M-09, the system shall run the copied procedure" (AC-064) —
//     docs/design/factory-core-design.md § Acceptance Criteria > Observation, exceptions and
//     recovery.
// Behavior: the lead proposes a recovery for the stopped sprint -> `origin` cannot be fetched when
//     the owner approves it -> the procedure still runs, in the base worktree as it is, instead of
//     the approval being recorded and the procedure never started.
// @lane: integration
// @dependency: daemon, sprint flow, owner channel, recovery
#[test]
fn an_approved_sprint_recovery_runs_even_when_origin_cannot_be_fetched() {
    let (env, mut d, _gh, channel, _fixed) = agreements_failing_their_check();
    let origin = git(&env.repo, &["remote", "get-url", "origin"]);
    git(
        &env.repo,
        &["remote", "set-url", "origin", "/nonexistent/origin.git"],
    );
    let recovery = d
        .decisions()
        .unwrap()
        .into_iter()
        .find(|x| x.kind == DecisionKind::Recovery)
        .unwrap();
    let log = d
        .store()
        .isolated()
        .join("recoveries")
        .join(recovery.procedure.clone().unwrap())
        .with_extension("log");
    channel.push_event(approve(&recovery));
    run_until(
        &mut d,
        30,
        |_| {},
        |_| std::fs::read_to_string(&log).is_ok_and(|t| t.contains("rerun the deploy check")),
    );
    git(&env.repo, &["remote", "set-url", "origin", &origin]);
}

/// The id of the sprint whose agreements stopped, from its `exception` filing.
fn recovery_sprint(env: &Env) -> String {
    env.events()
        .into_iter()
        .find(|e| e["event"] == "filed" && e["step"] == "exception" && e.get("sprint").is_some())
        .and_then(|e| e["sprint"].as_str().map(str::to_owned))
        .unwrap()
}

// AC: "While `[app]` has neither `start` nor `nothing_to_run`, the daemon files one `run-commands`
//     item for the lead in the base branch's worktree ... For commands, the daemon starts the
//     product once in that worktree ... and records `{started: {port, seconds}}` or `{failed:
//     {output}}`. Nothing is written to `[app]` by the daemon ... No sprint begins while `[app]` is
//     unsettled." — docs/design/factory-core-design.md § Worktrees and apps (PRD AC-066).
// Behavior: `[app]` unsettled, a goal and an open issue -> the lead answers with commands -> the
//     factory starts the product once and records that it answered -> no sprint was proposed and
//     `[app]` is unchanged.
// @lane: integration
// @dependency: daemon, launcher, fake agents, the product started from the base worktree
#[test]
fn how_the_product_runs_is_found_and_started_once_before_any_sprint() {
    let env = Env::new();
    let script = json!({"run-commands": [{"output": {
        "runs": "commands",
        "setup": "mkdir -p \"$OVENSPRING_DATA_DIR/www\" && touch \"$OVENSPRING_DATA_DIR/www/ready\"",
        "start": support::serve("\"$OVENSPRING_DATA_DIR/www\""),
        "ready": "http://127.0.0.1:$PORT/ready",
        "reason": "README.md says the shop runs with its dev server."
    }}]});
    let Value::Object(script) = script else {
        unreachable!()
    };
    let (mut d, _, _, _) = setup(&env, "[app]\n", &script, vec![issue("ACME-1", &[])]);

    run_until(
        &mut d,
        60,
        |d| {
            assert!(
                ovenspring_daemon::records::sprint(d.store())
                    .unwrap()
                    .is_none(),
                "no sprint while how the product runs is unsettled"
            );
        },
        |d| {
            ovenspring_daemon::product_run::read(d.store())
                .unwrap()
                .check
                .is_some()
        },
    );
    let run = ovenspring_daemon::product_run::read(d.store()).unwrap();
    assert_eq!(
        run.found.map(|f| f.runs),
        Some(ovenspring_core::output::Runs::Commands)
    );
    assert!(
        matches!(
            run.check,
            Some(ovenspring_daemon::product_run::RunCheck::Started { .. })
        ),
        "{:?}",
        run.check
    );
    assert!(
        !ovenspring_core::config::Config::load(&env.repo)
            .unwrap()
            .app
            .settled(),
        "the owner settles `[app]`"
    );
}

// Behavior: the daemon stopped while it was starting the product with the lead's commands (an
//     answer is recorded, its start is not) -> a new daemon starts it once and records how it
//     ended.
// @lane: integration
// @dependency: daemon, the product started from the base worktree
#[test]
fn a_start_check_the_daemon_stopped_during_runs_again_on_the_next_start() {
    let env = Env::new();
    let (mut d, _, _, _) = setup(&env, "[app]\n", &Map::new(), vec![]);
    let found: ovenspring_core::output::RunCommandsOutput = serde_json::from_value(json!({
        "runs": "commands",
        "setup": "mkdir -p \"$OVENSPRING_DATA_DIR/www\"",
        "start": support::serve("\"$OVENSPRING_DATA_DIR/www\""),
        "ready": null,
        "reason": "README.md says the shop runs with its dev server."
    }))
    .unwrap();
    d.store()
        .write_record(
            "product-run.json",
            &ovenspring_daemon::product_run::ProductRun {
                item: None,
                found: Some(found),
                check: None,
            },
        )
        .unwrap();
    run_until(
        &mut d,
        60,
        |_| {},
        |d| {
            ovenspring_daemon::product_run::read(d.store())
                .unwrap()
                .check
                .is_some()
        },
    );
    let check = ovenspring_daemon::product_run::read(d.store())
        .unwrap()
        .check;
    assert!(
        matches!(
            check,
            Some(ovenspring_daemon::product_run::RunCheck::Started { .. })
        ),
        "{check:?}"
    );
}
