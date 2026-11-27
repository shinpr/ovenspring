//! @lane: integration
//! Area: exceptions and resilience of the running daemon (retry, pause, restart, stop).
//!
//! No clock is injected: the daemon's waits are shortened (`support::QUICK`) and the tests read real
//! timestamps.

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
use std::sync::Arc;

use ovenspring_core::flow::{IssueState, Stage};
use ovenspring_daemon::issues::IssueRecord;
use ovenspring_daemon::owner::{DecisionRecord, DecisionStatus};
use ovenspring_integrations::channel::DecisionKind;
use ovenspring_integrations::channel::{Button, ChannelEvent, NoticeKind};
use ovenspring_integrations::github::CheckState;
use serde_json::{Value, json};
use support::fake_github::FakeGitHub;
use support::recording::RecordingTracker;
use support::recording::{RecordingChannel, Sent};
use support::scripts::requiring_test;
use support::{Env, daemon, git, run_until, services, wait_until};

fn ok_requirements() -> Value {
    json!({"output": {"convergence": {"outcome": "o"}, "structuralScale": "medium", "decision": "proceed"}})
}

fn passing_github(env: &Env) -> Arc<FakeGitHub> {
    Arc::new(FakeGitHub::new(
        env.origin.clone(),
        requiring_test(),
        Box::new(|_| vec![("test".to_owned(), CheckState::Success)]),
    ))
}

fn setup(
    env: &Env,
    extra: &str,
    script: &Value,
) -> (ovenspring_daemon::Daemon, Arc<RecordingChannel>) {
    env.configure(extra);
    env.script_agents(script);
    let channel = Arc::new(RecordingChannel::default());
    let d = daemon(
        env,
        services(
            passing_github(env),
            channel.clone(),
            Arc::new(RecordingTracker::default()),
        ),
    );
    (d, channel)
}

/// Start lines in `launches.jsonl` for items whose name contains `part`.
fn launches_of(env: &Env, part: &str) -> Vec<Value> {
    std::fs::read_to_string(env.state().join("launches.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .filter(|l| {
            l.get("pid").is_some()
                && l["prepare"] != true
                && l["item"].as_str().unwrap().contains(part)
        })
        .collect()
}

fn paused_notices(channel: &RecordingChannel) -> Vec<String> {
    channel
        .sent()
        .into_iter()
        .filter_map(|s| match s {
            Sent::Notice(n) if n.kind == NoticeKind::Paused => Some(n.text),
            Sent::Notice(_)
            | Sent::Decision(_)
            | Sent::Reply { .. }
            | Sent::Attached { .. }
            | Sent::Resolved { .. }
            | Sent::Told { .. } => None,
        })
        .collect()
}

fn pids(env: &Env, issue: &str, step: &str) -> Option<Value> {
    let path = env
        .fake_bin
        .parent()
        .unwrap()
        .join(format!("pids-{issue}-{step}.json"));
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

// AC: "A git command that fails on two passes in a row ... stops the issue and files `exception`"
//     — docs/design/factory-core-design.md § Exceptions, recovery and resilience (AC-062).
// Behavior: `origin` answers but fetching into the worktree keeps failing ->
//     the push cannot bring the branch in -> after the second pass the issue stops and the lead
//     takes it, instead of the push being retried quietly forever.
// @lane: integration
// @dependency: daemon, flow, fake agent
#[test]
fn a_push_that_cannot_bring_the_branch_in_stops_the_issue_for_the_lead() {
    let env = Env::new();
    let mut script = support::scripts::straight("ACME-123");
    script.insert(
        "quality@ACME-123".into(),
        json!([{"wait": true, "output": {"status": "pass", "summary": "s", "checksPerformed": {}, "fixesApplied": []}}]),
    );
    let (mut d, _) = setup(&env, "", &Value::Object(script));
    d.begin_issue("ACME-123", "Export", vec![]).unwrap();
    let root = env.fake_bin.parent().unwrap().to_path_buf();
    run_until(
        &mut d,
        30,
        |_| {},
        |d| d.issue("ACME-123").unwrap().unwrap().flow.stage == Stage::running(Step::Quality),
    );
    // The worktree's FETCH_HEAD cannot be written: every fetch into it fails, though `origin`
    // answers.
    let worktree = env.state().join("worktrees/ACME-123");
    let gitdir = git(&worktree, &["rev-parse", "--absolute-git-dir"]);
    let fetch_head = std::path::Path::new(&gitdir).join("FETCH_HEAD");
    let _ = std::fs::remove_file(&fetch_head);
    std::fs::create_dir_all(fetch_head.join("blocked")).unwrap();
    std::fs::write(root.join("release-ACME-123-quality"), "").unwrap();

    run_until(
        &mut d,
        30,
        |_| {},
        |d| {
            matches!(
                d.issue("ACME-123").unwrap().unwrap().flow.stage,
                Stage::Exception { .. }
            )
        },
    );
    let record = d.issue("ACME-123").unwrap().unwrap();
    assert!(record.stopped.is_some(), "{:?}", record.flow.stage);
}

// AC: "On start, items `doing` whose process is gone return to `open`" — docs/design/
//     factory-core-design.md § Exceptions, recovery and resilience (AC-039).
// Behavior: the daemon ends without stopping its launches (a crash) while an agent works -> a new
//     daemon starts -> it stops the agent the old one left before the item runs again, and the
//     history records that launch as ended.
// @lane: integration
// @dependency: daemon, launcher, fake agent
#[test]
fn a_restart_after_a_crash_stops_the_agent_the_old_daemon_left_before_relaunching() {
    let env = Env::new();
    let (mut first, _) = setup(&env, "", &json!({"requirements": [{"hang": true}]}));
    first.begin_issue("ACME-123", "Export", vec![]).unwrap();
    run_until(
        &mut first,
        30,
        |_| {},
        |_| pids(&env, "ACME-123", "requirements").is_some(),
    );
    let agent = u32::try_from(
        pids(&env, "ACME-123", "requirements").unwrap()["agent"]
            .as_u64()
            .unwrap(),
    )
    .unwrap();
    // No `stop_launches`: the old daemon is gone and its agent still runs.
    drop(first);
    assert!(ovenspring_daemon::app::alive(agent));

    let mut second = daemon(
        &env,
        services(
            passing_github(&env),
            Arc::new(RecordingChannel::default()),
            Arc::new(RecordingTracker::default()),
        ),
    );
    second.recover().unwrap();
    assert!(
        wait_until(15, || !ovenspring_daemon::app::alive(agent)),
        "the agent the old daemon left still runs"
    );
    let rows = ovenspring_daemon::history::launch_rows(second.store(), None, None).unwrap();
    assert!(
        rows.iter()
            .all(|r| r.result != ovenspring_daemon::history::RunResult::Running),
        "{rows:?}"
    );
}

// AC: "When an agent CLI fails because its provider is overloaded, the factory retries it a bounded
//     number of times; any other failure pauses the factory" (AC-037); factory-core design
//     § Exceptions and resilience: the kind the CLI records decides.
// Behavior: the lead runs on Codex, whose session records `server_overloaded` -> relaunch after the
//     wait, three times -> (a) after the bound, pause with a notice, and after the owner resumes,
//     three retries again; (b) the relaunch succeeds.
const CODEX_LEAD: &str = "[roles.lead]\ncli = \"codex\"\n";

// AC: PRD AC-044, AC-045: the owner sees each launch's cost, and an issue's.
// Behavior: the lead works on Codex -> the factory stops it mid-turn, before its stream reports any
//     tokens -> the launch is recorded with the model and tokens its session recorded so far, and
//     their cost.
// @lane: integration
// @dependency: daemon, launcher, backend adapters, price table, store, fake agent
#[test]
fn a_launch_the_factory_stops_is_priced_by_what_its_session_used_until_then() {
    let env = Env::new();
    let (mut d, _) = setup(&env, CODEX_LEAD, &json!({"requirements": [{"hang": true}]}));
    d.begin_issue("ACME-123", "Export", vec![]).unwrap();
    run_until(
        &mut d,
        30,
        |_| {},
        |_| pids(&env, "ACME-123", "requirements").is_some(),
    );

    d.stop_launches();

    let end = std::fs::read_to_string(env.state().join("launches.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .find(|l| l["failure"] == "stopped: the factory ended the launch")
        .unwrap();
    assert_eq!(end["model"], "gpt-6.2-sol", "{end}");
    // gpt-6.2-sol per million: 500,000 uncached input x 2.00 + 400,000 cache reads x 0.10
    // + 100,000 cache writes x 2.50 + 100,000 output x 10.00.
    let cost = end["cost_usd"].as_f64().unwrap();
    assert!((cost - 2.29).abs() < 1e-9, "{end}");
}

#[test]
fn a_launch_overloaded_every_time_is_retried_three_times_after_the_wait_then_pauses() {
    let env = Env::new();
    let (mut d, channel) = setup(
        &env,
        CODEX_LEAD,
        &json!({"requirements": [{"fail": "overloaded"}]}),
    );
    d.begin_issue("ACME-123", "Export", vec![]).unwrap();
    run_until(&mut d, 30, |_| {}, |d| d.halt_file().exists());
    let starts = launches_of(&env, "requirements-ACME-123");
    assert_eq!(starts.len(), 4, "the first launch and three retries");
    let at = |v: &Value| {
        v["at"]
            .as_str()
            .unwrap()
            .parse::<jiff::Timestamp>()
            .unwrap()
    };
    for pair in starts.windows(2) {
        assert!(
            at(&pair[1]).duration_since(at(&pair[0])).as_secs_f64() >= 1.0,
            "relaunch waits"
        );
    }
    for _ in 0..3 {
        d.tick().unwrap();
    }
    assert_eq!(
        launches_of(&env, "requirements-ACME-123").len(),
        4,
        "no fifth launch"
    );
    let notices = paused_notices(&channel);
    assert_eq!(notices.len(), 1);
    assert!(
        notices[0].contains("high demand") && notices[0].contains("!resume"),
        "{}",
        notices[0]
    );

    // The owner resumes while the provider is still overloaded: the bound counts again from
    // there, so the launch gets its three retries before the factory pauses again.
    d.resume().unwrap();
    run_until(&mut d, 30, |_| {}, |d| d.halt_file().exists());
    assert_eq!(
        launches_of(&env, "requirements-ACME-123").len(),
        8,
        "after the resume, one launch and three retries again"
    );
}

#[test]
fn a_launch_overloaded_once_is_relaunched_after_the_wait_and_goes_on() {
    let env = Env::new();
    let script = json!({"requirements": [{"fail": "overloaded"}, ok_requirements()], "codebase-analysis": [{"hang": true}]});
    let (mut d, channel) = setup(&env, CODEX_LEAD, &script);
    d.begin_issue("ACME-123", "Export", vec![]).unwrap();
    run_until(
        &mut d,
        30,
        |_| {},
        |d| {
            d.issue("ACME-123").unwrap().unwrap().flow.stage
                == Stage::running(Step::CodebaseAnalysis)
        },
    );
    assert_eq!(launches_of(&env, "requirements-ACME-123").len(), 2);
    assert!(!d.halt_file().exists());
    assert_eq!(paused_notices(&channel), [] as [std::string::String; 0]);
}

// AC: "...any other failure pauses the factory and tells the owner the cause and how to resume
//     (AC-037)".
// Behavior: ACME-123's launch exits with the usage-limit output -> pause and notify; ACME-124's
//     item does not launch -> the owner sends `!resume` in Slack -> both run.
#[test]
fn fatal_launch_failure_pauses_with_the_cause_until_resume() {
    let env = Env::new();
    let script = json!({
        "requirements@ACME-123": [{"fail": "fatal"}, ok_requirements()],
        "requirements": [ok_requirements()],
        "codebase-analysis": [{"hang": true}]
    });
    let (mut d, channel) = setup(&env, "", &script);
    d.begin_issue("ACME-123", "Export", vec![]).unwrap();
    run_until(&mut d, 30, |_| {}, |d| d.halt_file().exists());
    assert!(
        std::fs::read_to_string(d.halt_file())
            .unwrap()
            .contains("usage limit")
    );

    d.begin_issue("ACME-124", "Import", vec![]).unwrap();
    for _ in 0..3 {
        d.tick().unwrap();
    }
    assert!(
        launches_of(&env, "ACME-124").is_empty(),
        "nothing launches while paused"
    );
    let notices = paused_notices(&channel);
    assert_eq!(notices.len(), 1);
    assert!(
        notices[0].contains("the agent on ACME-123 (requirements) stopped with an error")
            && notices[0].contains("usage limit")
            && notices[0].contains("!resume"),
        "{}",
        notices[0]
    );

    channel.push_event(ChannelEvent::Command {
        name: "resume".into(),
        args: String::new(),
        user: "U0123456789".into(),
        at: Some(ovenspring_integrations::channel::MessageRef {
            channel: "C0123456789".into(),
            ts: "5.0".into(),
            permalink: None,
        }),
    });
    d.tick().unwrap();
    assert!(!d.halt_file().exists());
    // ACME-123's item runs again and finishes; ACME-124's launches.
    run_until(
        &mut d,
        30,
        |_| {},
        |d| {
            ["ACME-123", "ACME-124"].iter().all(|k| {
                d.issue(k).unwrap().unwrap().flow.stage == Stage::running(Step::CodebaseAnalysis)
            })
        },
    );
    assert_eq!(launches_of(&env, "requirements-ACME-123").len(), 2);
    let events = env.events();
    assert!(events.iter().any(|e| e["event"] == "paused"));
    assert!(channel.sent().iter().any(
        |s| matches!(s, Sent::Reply { thread, text } if thread == "5.0" && text == "Resumed.")
    ));
}

// AC: "The owner can pause and resume the factory from Slack" (AC-034); UI Spec § Slack Messages:
//     "Commands: `!pause` (stops new launches; running ones finish) and `!resume`; the factory
//     confirms each in the command's thread."
// Behavior: an owner sends `!pause` in a thread -> no new launch starts, the thread gets the
//     confirmation and the channel gets no pause notice -> someone who is not an owner sends
//     `!resume` -> nothing changes -> the owner sends `!resume` -> launches start again.
#[test]
fn an_owner_pauses_and_resumes_from_slack_and_hears_back_in_the_thread() {
    let env = Env::new();
    let (mut d, channel) = setup(
        &env,
        "",
        &json!({"requirements": [ok_requirements()], "codebase-analysis": [{"hang": true}]}),
    );
    let at = |ts: &str| {
        Some(ovenspring_integrations::channel::MessageRef {
            channel: "C0123456789".into(),
            ts: ts.into(),
            permalink: None,
        })
    };
    let command = |name: &str, user: &str, ts: &str| ChannelEvent::Command {
        name: name.into(),
        args: String::new(),
        user: user.into(),
        at: at(ts),
    };
    channel.push_event(command("pause", "U0123456789", "1.0"));
    d.begin_issue("ACME-123", "Export", vec![]).unwrap();
    for _ in 0..3 {
        d.tick().unwrap();
    }
    assert!(d.halt_file().exists());
    assert!(
        launches_of(&env, "ACME-123").is_empty(),
        "nothing launches while paused"
    );
    assert_eq!(
        paused_notices(&channel),
        [] as [String; 0],
        "the owner's own pause posts no notice"
    );

    channel.push_event(command("resume", "U0123456789", "3.0"));
    run_until(
        &mut d,
        30,
        |_| {},
        |_| !launches_of(&env, "requirements-ACME-123").is_empty(),
    );
    let replies: Vec<(String, String)> = channel
        .sent()
        .into_iter()
        .filter_map(|s| match s {
            Sent::Reply { thread, text } => Some((thread, text)),
            Sent::Notice(_)
            | Sent::Decision(_)
            | Sent::Attached { .. }
            | Sent::Resolved { .. }
            | Sent::Told { .. } => None,
        })
        .collect();
    assert_eq!(replies.len(), 2, "{replies:?}");
    assert!(
        replies[0].0 == "1.0" && replies[0].1.starts_with("Paused."),
        "{replies:?}"
    );
    assert_eq!(replies[1], ("3.0".to_owned(), "Resumed.".to_owned()));
}

fn write_record(env: &Env, record: &IssueRecord) {
    let store = ovenspring_store::Store::open(env.state()).unwrap();
    store
        .write_record(&format!("issues/{}.json", record.key()), record)
        .unwrap();
}

/// ACME-124 pushed and waits for checks on the pull request it opened; ACME-125 is planning and
/// its item is gone.
fn seed_ci_and_planning(env: &Env) -> Arc<FakeGitHub> {
    let state = env.state();
    let worktree = ovenspring_daemon::worktree::ensure(
        &env.repo,
        &ovenspring_store::Store::open(&state).unwrap(),
        "ACME-124",
        "main",
    )
    .unwrap();
    git(
        &worktree,
        &[
            "-c",
            "user.name=a",
            "-c",
            "user.email=a@example.com",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "Import",
        ],
    );
    git(
        &worktree,
        &[
            "push",
            "-q",
            "origin",
            "HEAD:refs/heads/ovenspring/ACME-124",
        ],
    );
    let gh = passing_github(env);
    let number = gh.add_pull("ovenspring/ACME-124", "main");
    let mut ci = IssueRecord::new("ACME-124", "Import", vec![]);
    ci.flow = IssueState {
        stage: Stage::Ci,
        pr: Some(number),
        ..IssueState::new("ACME-124")
    };
    ci.pushed_sha = Some(git(&worktree, &["rev-parse", "HEAD"]));
    write_record(env, &ci);
    let mut planning = IssueRecord::new("ACME-125", "Search", vec![]);
    planning.flow.stage = Stage::running(Step::WorkPlan);
    write_record(env, &planning);

    gh
}

// AC: "When the daemon starts, items left `doing` shall run again (AC-039)"; "When the daemon
//     finds an issue in a non-waiting state with no item open or doing, it shall file the item
//     that state calls for (AC-039)"; PRD AC-039: "...keeps track of the pull requests it opened."
// Behavior: a first instance leaves ACME-123's item `doing`; ACME-124 is in `ci` with its pull
//     request open; ACME-125 is `planning` with no item -> a new instance recovers each.
#[test]
fn restart_relaunches_doing_items_repairs_missing_items_and_keeps_open_pull_requests() {
    let env = Env::new();
    let script = json!({
        "requirements": [{"hang": true}, ok_requirements()],
        "codebase-analysis": [{"hang": true}],
        "work-plan": [{"hang": true}]
    });
    let (mut first, _) = setup(&env, "", &script);
    first.begin_issue("ACME-123", "Export", vec![]).unwrap();
    run_until(
        &mut first,
        30,
        |_| {},
        |_| pids(&env, "ACME-123", "requirements").is_some(),
    );
    // The daemon's own stop ends the running agent and what it started, so the relaunch after
    // the restart is the only one at work.
    let p = pids(&env, "ACME-123", "requirements").unwrap();
    first.stop_launches();
    drop(first);
    for pid in [p["agent"].as_u64().unwrap(), p["child"].as_u64().unwrap()] {
        assert!(
            wait_until(15, || !ovenspring_daemon::app::alive(
                u32::try_from(pid).unwrap()
            )),
            "{pid} still runs after the stop"
        );
    }
    std::fs::remove_file(
        env.fake_bin
            .parent()
            .unwrap()
            .join("pids-ACME-123-requirements.json"),
    )
    .unwrap();

    let gh = seed_ci_and_planning(&env);
    let mut second = daemon(
        &env,
        services(
            gh.clone(),
            Arc::new(RecordingChannel::default()),
            Arc::new(RecordingTracker::default()),
        ),
    );
    second.recover().unwrap();
    run_until(
        &mut second,
        60,
        |d| {
            for issue in ["ACME-123", "ACME-124", "ACME-125"] {
                let n = d
                    .store()
                    .active_items()
                    .unwrap()
                    .iter()
                    .filter(|p| {
                        d.store().read_item(p).unwrap().front.issue.as_deref() == Some(issue)
                    })
                    .count();
                assert!(n <= 1, "{issue} has {n} items");
            }
        },
        |d| {
            d.issue("ACME-123").unwrap().unwrap().flow.stage
                == Stage::running(Step::CodebaseAnalysis)
                && d.issue("ACME-124").unwrap().unwrap().flow.stage == Stage::Merged
                && pids(&env, "ACME-125", "work-plan").is_some()
        },
    );
    assert_eq!(
        launches_of(&env, "requirements-ACME-123").len(),
        2,
        "relaunched after restart"
    );
    let logs: Vec<std::path::PathBuf> = launches_of(&env, "requirements-ACME-123")
        .iter()
        .map(|l| std::path::PathBuf::from(l["log"].as_str().unwrap()))
        .filter(|p| p.exists())
        .collect();
    assert_eq!(logs.len(), 2, "the first launch's log is kept: {logs:?}");
    assert!(env.events().iter().any(|e| e["event"] == "reconciled"
        && e["issue"] == "ACME-125"
        && e["step"] == "work-plan"));
    assert!(
        !gh.calls().iter().any(|c| c.starts_with("open_pull")),
        "the open pull request is kept"
    );
}

// AC: docs/design/factory-core-design.md § Acceptance Criteria > Observation, exceptions and
//     recovery: "If the same daemon operation ... fails for the same issue or sprint on two passes
//     in a row, then the system shall record the issue or sprint as stopped on that operation, keep
//     its state, and file `exception` for the lead with the operation and its error (AC-062)"; "An
//     error for one issue or the sprint shall not keep the daemon's pass from handling the others
//     (AC-062)"; "When the daemon starts, each stopped issue or sprint shall run its stopped
//     operation again (AC-039, AC-062)".
// Behavior: GitHub refuses to show ACME-123's pull request (403) on every poll -> after two polls
//     ACME-123 stops on reading it, keeps the state it was in, and the lead's `exception` asks the
//     owner -> ACME-124 merges meanwhile -> the refusal ends and the daemon restarts -> ACME-123
//     returns to its state and merges.
// @lane: integration
// @dependency: daemon, flow, gate, store, fake agent, FakeGitHub
#[test]
fn a_failure_that_repeats_for_one_issue_stops_only_it_and_a_restart_resumes_it() {
    let env = Env::new();
    env.configure("[repo]\nbase_branch = \"develop\"\n[review]\n");
    env.add_branch("develop");
    let mut script = support::scripts::straight("ACME-123");
    script.extend(support::scripts::straight("ACME-124"));
    script.insert(
        "exception@ACME-123".into(),
        json!([{"output": {"action": "ask", "note": "GitHub refuses the pull request", "question": "Can the factory's token read ACME-123's pull request?"}}]),
    );
    env.script_agents(&Value::Object(script));
    let gh = passing_github(&env);
    *gh.refuse_pull_of.lock().unwrap() = Some("ovenspring/ACME-123".into());
    let channel = Arc::new(RecordingChannel::default());
    let mut first = daemon(
        &env,
        services(
            gh.clone(),
            channel.clone(),
            Arc::new(RecordingTracker::default()),
        ),
    );
    first.begin_issue("ACME-123", "Export", vec![]).unwrap();
    first.begin_issue("ACME-124", "Import", vec![]).unwrap();
    let asking = |stage: &Stage| matches!(stage, Stage::LeadAsking { .. });
    run_until(
        &mut first,
        60,
        |_| {},
        |d| {
            asking(&d.issue("ACME-123").unwrap().unwrap().flow.stage)
                && d.issue("ACME-124").unwrap().unwrap().flow.stage == Stage::Merged
        },
    );
    let record = first.issue("ACME-123").unwrap().unwrap();
    let stopped = record.stopped.clone().unwrap();
    assert_eq!(stopped.operation, "reading its pull request");
    assert!(stopped.error.contains("403"), "{}", stopped.error);
    assert!(record.flow.resume().is_some());
    assert_eq!(
        support::scripts::filed_steps(&env, "ACME-123")
            .iter()
            .filter(|s| *s == "exception")
            .count(),
        1
    );
    let questions = channel
        .sent()
        .into_iter()
        .filter(|s| matches!(s, Sent::Decision(d) if d.issue.as_deref() == Some("ACME-123")))
        .count();
    assert_eq!(questions, 1);

    *gh.refuse_pull_of.lock().unwrap() = None;
    drop(first);
    let mut second = daemon(
        &env,
        services(
            gh.clone(),
            Arc::new(RecordingChannel::default()),
            Arc::new(RecordingTracker::default()),
        ),
    );
    second.recover().unwrap();
    run_until(
        &mut second,
        60,
        |_| {},
        |d| d.issue("ACME-123").unwrap().unwrap().flow.stage == Stage::Merged,
    );
    let record = second.issue("ACME-123").unwrap().unwrap();
    assert_eq!(
        (record.stopped, record.flow.resume().cloned()),
        (None, None)
    );
    assert!(
        env.events()
            .iter()
            .any(|e| e["event"] == "resumed" && e["issue"] == "ACME-123")
    );
}

fn pending(d: &ovenspring_daemon::Daemon, kind: DecisionKind) -> Option<DecisionRecord> {
    d.decisions()
        .unwrap()
        .into_iter()
        .find(|x| x.kind == kind && x.status == DecisionStatus::Pending)
}

fn press(channel: &RecordingChannel, decision: &str, button: Button, user: &str) {
    channel.push_event(ChannelEvent::Action {
        decision: decision.into(),
        button,
        user: user.into(),
    });
}

fn recover(procedure: &str) -> Value {
    json!({"output": {"action": "recover", "note": "GitHub refuses the pull request",
        "procedure": procedure, "summary": "The factory's token cannot read the pull request."}})
}

/// An issue whose pull request GitHub refuses to show, with its lead's recovery scripted.
fn stopping_on_its_pull_request(
    exceptions: &[Value],
) -> (
    Env,
    Arc<FakeGitHub>,
    Arc<RecordingChannel>,
    ovenspring_daemon::Daemon,
) {
    let env = Env::new();
    env.configure("[repo]\nbase_branch = \"develop\"\n[review]\n");
    env.add_branch("develop");
    let mut script = support::scripts::straight("ACME-123");
    script.insert("exception@ACME-123".into(), json!(exceptions));
    env.script_agents(&Value::Object(script));
    let gh = passing_github(&env);
    *gh.refuse_pull_of.lock().unwrap() = Some("ovenspring/ACME-123".into());
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
        |d| pending(d, DecisionKind::Recovery).is_some(),
    );
    (env, gh, channel, d)
}

fn merged(d: &mut ovenspring_daemon::Daemon) {
    run_until(
        d,
        60,
        |_| {},
        |d| d.issue("ACME-123").unwrap().unwrap().flow.stage == Stage::Merged,
    );
}

// AC: docs/design/factory-core-design.md § Acceptance Criteria > Observation, exceptions and
//     recovery: "When an `exception` output is `recover`, the system shall copy its procedure to the
//     isolated directory and post M-09 with the procedure as copied (AC-064)"; "When an owner
//     approves M-09, the system shall run the copied procedure and nothing else; on success, or when
//     an owner presses Resolved, it shall clear the stop, read the current state and run the stopped
//     operation again; on failure it shall file `exception` with the procedure's output (AC-064)".
// Behavior: ACME-123 stops on reading its pull request -> the lead writes a procedure that fails
//     -> the owner approves it -> it fails, and the lead gets its output and writes another -> the
//     lead's output file is edited after the request was posted, and someone who is not an owner
//     presses Approve -> the owner approves -> the copy runs, ACME-123 resumes and merges. In a
//     second case the owner fixes it and presses Resolved -> it resumes and merges, and the
//     procedure never runs.
// @lane: integration
// @dependency: daemon, flow, owner channel, store, fake agent, FakeGitHub
#[test]
fn a_recovery_runs_only_the_copy_an_owner_approved_and_a_failure_returns_to_the_lead() {
    let (env, gh, channel, mut d) = stopping_on_its_pull_request(&[
        recover("#!/bin/sh\necho first >> RECOVERY.txt\nexit 3\n"),
        // Runs only with the interpreter its first line names, not with `sh`.
        recover(
            "#!/usr/bin/env python3\nwith open('RECOVERY.txt', 'a') as f:\n    f.write('second\\n')\n",
        ),
    ]);
    let first = pending(&d, DecisionKind::Recovery).unwrap();
    let copy = env
        .isolated()
        .join("recoveries")
        .join(first.procedure.clone().unwrap());
    assert_eq!(
        std::fs::read_to_string(&copy).unwrap(),
        "#!/bin/sh\necho first >> RECOVERY.txt\nexit 3\n"
    );
    assert!(
        first.text.contains("echo first >> RECOVERY.txt"),
        "{}",
        first.text
    );
    press(&channel, &first.id, Button::Approve, "U0123456789");
    run_until(
        &mut d,
        30,
        |_| {},
        |d| pending(d, DecisionKind::Recovery).is_some_and(|x| x.id != first.id),
    );
    let exception_items: Vec<Value> = env
        .events()
        .into_iter()
        .filter(|e| e["event"] == "filed" && e["issue"] == "ACME-123" && e["step"] == "exception")
        .collect();
    assert_eq!(exception_items.len(), 2);
    let second_item = exception_items[1]["item"].as_str().unwrap().to_owned();
    let archived = std::fs::read_to_string(
        env.state()
            .join("board/archive")
            .join(format!("{second_item}.md")),
    )
    .unwrap();
    assert!(archived.contains("recoveries/"), "{archived}");
    assert!(archived.contains("failed"), "{archived}");

    let second = pending(&d, DecisionKind::Recovery).unwrap();
    // An agent's output edited after the request was posted changes nothing that runs.
    std::fs::write(
        env.state()
            .join("outputs")
            .join(format!("{second_item}.json")),
        recover("#!/bin/sh\ntouch TAMPERED\n")["output"].to_string(),
    )
    .unwrap();
    *gh.refuse_pull_of.lock().unwrap() = None;
    press(&channel, &second.id, Button::Approve, "U0123456789");
    merged(&mut d);
    let worktree = env.state().join("worktrees/ACME-123");
    assert_eq!(
        std::fs::read_to_string(worktree.join("RECOVERY.txt")).unwrap(),
        "first\nsecond\n"
    );
    assert!(!worktree.join("TAMPERED").exists());
}

// AC: "When an owner approves M-09, the system shall run the copied procedure ...; on failure it
//     shall file `exception` with the procedure's output (AC-064)"; "A restart reconstructs
//     everything from the directory" (AC-039).
// Behavior: the owner approves a procedure that is still running when the factory stops -> the
//     factory starts again -> the procedure is not run again; the lead gets it as failed, with its
//     output saying its result is unknown.
// @lane: integration
// @dependency: daemon, flow, owner channel, store, fake agent, FakeGitHub
#[test]
fn a_procedure_still_running_when_the_factory_stops_returns_to_the_lead_unknown() {
    let (env, gh, channel, mut d) = stopping_on_its_pull_request(&[
        recover("#!/bin/sh\necho ran >> RAN.txt\nsleep 30\n"),
        json!({"hang": true}),
    ]);
    let request = pending(&d, DecisionKind::Recovery).unwrap();
    press(&channel, &request.id, Button::Approve, "U0123456789");
    run_until(
        &mut d,
        30,
        |_| {},
        |_| {
            env.events()
                .iter()
                .any(|e| e["event"] == "recovery-started")
        },
    );
    drop(d);

    let mut restarted = daemon(
        &env,
        services(
            gh.clone(),
            channel.clone(),
            Arc::new(RecordingTracker::default()),
        ),
    );
    run_until(
        &mut restarted,
        30,
        |_| {},
        |_| {
            env.events()
                .iter()
                .filter(|e| {
                    e["event"] == "filed" && e["issue"] == "ACME-123" && e["step"] == "exception"
                })
                .count()
                == 2
        },
    );

    let ended: Vec<Value> = env
        .events()
        .into_iter()
        .filter(|e| e["event"] == "recovery-ended")
        .collect();
    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0]["succeeded"], false);
    let log = std::fs::read_to_string(
        env.isolated()
            .join("recoveries")
            .join(format!("{}.log", request.id)),
    )
    .unwrap();
    assert!(log.contains("its result is unknown"), "{log}");
    let started = env
        .events()
        .iter()
        .filter(|e| e["event"] == "recovery-started")
        .count();
    assert_eq!(started, 1, "the procedure is not run again");
}

#[test]
fn resolved_resumes_the_issue_without_running_the_procedure() {
    let (env, gh, channel, mut d) =
        stopping_on_its_pull_request(&[recover("#!/bin/sh\necho never > NEVER.txt\n")]);
    let request = pending(&d, DecisionKind::Recovery).unwrap();
    *gh.refuse_pull_of.lock().unwrap() = None;
    press(&channel, &request.id, Button::Resolved, "U0123456789");
    merged(&mut d);
    assert!(!env.state().join("worktrees/ACME-123/NEVER.txt").exists());
    assert!(
        !env.events()
            .iter()
            .any(|e| e["event"] == "recovery-started")
    );
}

// AC: "Each launch runs in its own process group, which ends with the agent: what the agent left
//     running in the background stops when its launch ends" (factory core design, Launcher).
// Behavior: ACME-123's requirements agent starts a background process and finishes -> the daemon
//     takes the finished launch -> the background process no longer runs.
// @lane: integration
// @dependency: daemon, launcher, fake agent
#[test]
fn what_a_finished_launch_left_running_ends_with_it() {
    let env = Env::new();
    let mut requirements = ok_requirements();
    requirements["leave"] = json!(true);
    let script = json!({"requirements": [requirements], "codebase-analysis": [{"hang": true}]});
    let (mut d, _) = setup(&env, "", &script);
    d.begin_issue("ACME-123", "Export", vec![]).unwrap();

    run_until(
        &mut d,
        30,
        |_| {},
        |d| {
            d.issue("ACME-123").unwrap().unwrap().flow.stage
                == Stage::running(Step::CodebaseAnalysis)
        },
    );

    let child = pids(&env, "ACME-123", "requirements").unwrap()["child"]
        .as_u64()
        .unwrap();
    assert!(wait_until(15, || !ovenspring_daemon::app::alive(
        u32::try_from(child).unwrap()
    )));
}

// AC: "the overview shows the service as unreachable since the first failure, until a call
//     succeeds" — docs/design/factory-core-design.md § Exceptions, recovery and resilience; UI Spec
//     § FactoryStatus "Service out of reach".
// Behavior: GitHub stops answering while an issue opens its pull request, and the tracker while it
//     is read -> each is recorded with the time of its first failure and why it fails (no
//     connection), and the tracker's is not a prerequisite to fix -> both answer again -> both
//     records are removed and the issue merges.
// @lane: integration
// @dependency: daemon, store, FakeGitHub, RecordingTracker
#[test]
fn a_service_out_of_reach_is_recorded_from_its_first_failure_until_it_answers() {
    let env = Env::new();
    env.configure("[repo]\nbase_branch = \"develop\"\n[review]\n");
    env.add_branch("develop");
    env.script_agents(&Value::Object(support::scripts::straight("ACME-123")));
    let gh = passing_github(&env);
    gh.unreachable
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let tracker = Arc::new(RecordingTracker::default());
    tracker
        .unreachable
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut d = daemon(
        &env,
        services(
            gh.clone(),
            Arc::new(RecordingChannel::default()),
            tracker.clone(),
        ),
    );
    d.begin_issue("ACME-123", "Export", vec![]).unwrap();
    let failing = |d: &ovenspring_daemon::Daemon| {
        ovenspring_daemon::errors::Errors::read(d.store())
            .unwrap()
            .unreachable()
    };
    run_until(&mut d, 60, |_| {}, |d| failing(d).len() == 2);
    let first = failing(&d);
    for _ in 0..3 {
        d.tick().unwrap();
    }
    assert_eq!(failing(&d), first, "the time of the first failure is kept");
    assert!(
        first
            .values()
            .all(|f| f.why == ovenspring_integrations::http::Unanswered::NoConnection),
        "{first:?}"
    );
    assert!(
        ovenspring_daemon::prerequisites::missing(d.store())
            .unwrap()
            .is_empty(),
        "a tracker out of reach is not a connection to fix"
    );

    gh.unreachable
        .store(false, std::sync::atomic::Ordering::SeqCst);
    tracker
        .unreachable
        .store(false, std::sync::atomic::Ordering::SeqCst);
    run_until(
        &mut d,
        60,
        |_| {},
        |d| d.issue("ACME-123").unwrap().unwrap().flow.stage == Stage::Merged,
    );
    assert!(failing(&d).is_empty());
}
