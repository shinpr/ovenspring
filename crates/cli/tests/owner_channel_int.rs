//! @lane: integration
//! Area: the owner channel wired into the daemon: the real `Slack` channel against an in-memory stub
//! of the Web API, fed the Socket Mode envelopes Slack sends.

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

use ovenspring_core::flow::Stage;
use ovenspring_daemon::owner::{AnswerKind, ApprovalRecord, DecisionRecord, DecisionStatus};
use ovenspring_daemon::{Daemon, Services};
use ovenspring_integrations::channel::DecisionKind;
use ovenspring_integrations::channel::slack::Slack;
use ovenspring_integrations::github::CommentKind;
use ovenspring_integrations::github::Permission;
use ovenspring_integrations::tracker::TrackerIssue;
use serde_json::{Map, Value, json};
use support::fake_github::{FakeGitHub, Planned};
use support::recording::RecordingTracker;
use support::scripts::{always_passing, implement_run, requiring_test, stage, straight};
use support::slack_stub::{SlackStub, deliver, mention, message, press_on};
use support::{Env, daemon, run_until, wait_until};

const OWNER: &str = "U0123456789";
const STRANGER: &str = "U0999999999";

const CONFIG: &str = r#"
[repo]
base_branch = "develop"
[review]
[held.schema]
paths = ["db/migrations/**"]
"#;

fn slack_services(
    gh: Arc<FakeGitHub>,
    slack: Arc<Slack<SlackStub>>,
    tracker: Arc<RecordingTracker>,
) -> Services {
    Services {
        github: Some(gh),
        channel: slack,
        tracker: Some(tracker),
    }
}

fn merges(gh: &FakeGitHub) -> Vec<String> {
    gh.calls()
        .into_iter()
        .filter(|c| c.starts_with("merge "))
        .collect()
}

fn approvals(env: &Env) -> Vec<ApprovalRecord> {
    std::fs::read_dir(env.isolated().join("approvals"))
        .into_iter()
        .flatten()
        .map(|e| {
            serde_json::from_str(&std::fs::read_to_string(e.unwrap().path()).unwrap()).unwrap()
        })
        .collect()
}

fn merge_requests(d: &Daemon) -> Vec<DecisionRecord> {
    d.decisions()
        .unwrap()
        .into_iter()
        .filter(|x| x.kind == DecisionKind::Merge)
        .collect()
}

fn pending_merge_request(d: &Daemon) -> Option<DecisionRecord> {
    merge_requests(d)
        .into_iter()
        .find(|x| x.status == DecisionStatus::Pending)
}

fn ticks(d: &mut Daemon, n: usize) {
    for _ in 0..n {
        d.tick().unwrap();
    }
}

/// ACME-125 adds a migration, so the gate holds its merge for the owner; `extra` is appended to
/// the configuration and `script` extends the issue's straight path.
fn held_merge(
    env: &Env,
    extra: &str,
    script: Map<String, Value>,
) -> (Daemon, Arc<FakeGitHub>, SlackStub, Arc<Slack<SlackStub>>) {
    held_merge_told(env, extra, script, Arc::new(RecordingTracker::default()))
}

/// `held_merge`, with the tracker the test reads afterwards.
fn held_merge_told(
    env: &Env,
    extra: &str,
    script: Map<String, Value>,
    tracker: Arc<RecordingTracker>,
) -> (Daemon, Arc<FakeGitHub>, SlackStub, Arc<Slack<SlackStub>>) {
    env.configure(&format!("{CONFIG}{extra}"));
    env.add_branch("develop");
    let mut steps = straight("ACME-125");
    steps.insert(
        "implement@ACME-125".into(),
        json!([implement_run("db/migrations/0002_add_index.sql")]),
    );
    steps.extend(script);
    env.script_agents(&Value::Object(steps));
    let mut gh = always_passing(env, requiring_test());
    gh.permissions.insert("acme-dev".into(), Permission::Write);
    let gh = Arc::new(gh);
    let stub = SlackStub::default();
    let slack = stub.channel();
    let mut d = daemon(env, slack_services(gh.clone(), slack.clone(), tracker));
    d.begin_issue("ACME-125", "Add an index", vec![]).unwrap();
    run_until(
        &mut d,
        60,
        |_| {},
        |d| stage(d, "ACME-125") == Stage::HeldForOwner && pending_merge_request(d).is_some(),
    );
    (d, gh, stub, slack)
}

/// The executor applies a writer's comment on ACME-125 and the fix is committed.
fn comment_applied_script() -> Map<String, Value> {
    let mut script = Map::new();
    script.insert(
        "review-response@ACME-125".into(),
        json!([{"change": {"path": "db/migrations/0002_add_index.sql", "content": "create index concurrently\n"},
                "output": {"status": "completed", "taskName": "comments", "changeSummary": "Create the index concurrently", "requiresTestReview": false,
                           "dispositions": [{"id": "review-comment-1", "disposition": "apply", "reason": "db/migrations/0002_add_index.sql:1 locks the table (AC 2)"}]}}]),
    );
    script
}

/// Whether `message` was updated to say `says`.
fn updated(stub: &SlackStub, message: &Value, says: &str) -> bool {
    stub.calls("chat.update")
        .iter()
        .any(|u| u["ts"] == message["ts"] && u["blocks"].to_string().contains(says))
}

/// `user`, and only they, was told why their press did nothing.
fn assert_told_privately(stub: &SlackStub, user: &str) {
    let told = stub.calls("chat.postEphemeral");
    assert_eq!(told.len(), 1);
    assert_eq!(told[0]["user"], user);
    assert!(
        told[0]["text"]
            .as_str()
            .unwrap()
            .contains("Only the owners")
    );
}

fn assert_nothing_counted(env: &Env, gh: &FakeGitHub, why: &str) {
    assert_eq!(approvals(env), [] as [ApprovalRecord; 0], "{why}");
    assert_eq!(merges(gh), [] as [String; 0], "{why}");
}

// AC: docs/design/owner-interface-design.md § Acceptance Criteria > Channel: "When the core requests
//     an owner decision, the system shall post its message (UI Spec M-01 to M-07) and record a
//     pending decision with the message's link (AC-029)"; "When an owner presses
//     Approve, the system shall record the approval with the Slack user, the time, the message link
//     and, for a merge, the commit sha shown, and update the message to show who approved (AC-032)";
//     "If someone who is not an owner presses a button, then the system shall record
//     nothing and tell that person privately why (AC-032)".
//     Integration Verification Point: "A merge decision: posted, a press by someone who is not an
//     owner refused, an owner's press recorded with sha, superseded by a new push, re-requested."
//     docs/design/factory-core-design.md § Merge gate: "An owner approval is tied to the sha shown in
//     its request; a later push requests it again."
// Behavior: ACME-125 is held (schema path) with head sha A -> merge request M-05 posted -> U0999999999
//     presses Approve -> a PR comment is applied and pushed (head sha B) -> the first message is
//     marked superseded and a new request for B is posted -> U0123456789 presses Approve on the old
//     message -> U0123456789 presses Approve on the new message -> merge with sha B.
// @lane: integration
// @dependency: Slack channel, decisions and approvals store, daemon, gate, fake agent, FakeGitHub
#[test]
fn merge_approval_counts_only_from_an_owner_for_the_sha_shown_and_is_rerequested_after_a_push() {
    let env = Env::new();
    let (mut d, gh, stub, slack) = held_merge(&env, "", comment_applied_script());
    let first = pending_merge_request(&d).unwrap();
    let sha_a = d.issue("ACME-125").unwrap().unwrap().pushed_sha.unwrap();
    assert_eq!(first.sha.as_deref(), Some(sha_a.as_str()));
    assert!(
        first.message.clone().unwrap().permalink.is_some(),
        "recorded with its link"
    );
    let first_message = stub.message_of(&first.id);
    let press_first = |user| press_on(&first_message, "approve", user);
    assert_eq!(
        press_first(OWNER)["payload"]["actions"][0]["value"],
        first.id
    );

    deliver(&slack, &[press_first(STRANGER)]);
    ticks(&mut d, 3);
    assert_nothing_counted(&env, &gh, "a stranger's press");
    assert_told_privately(&stub, STRANGER);

    let pr = d.issue("ACME-125").unwrap().unwrap().flow.pr.unwrap();
    gh.comment(pr, "acme-dev", "Create the index concurrently.");
    run_until(
        &mut d,
        60,
        |_| {},
        |d| pending_merge_request(d).is_some_and(|x| x.id != first.id),
    );
    let second = pending_merge_request(&d).unwrap();
    let sha_b = d.issue("ACME-125").unwrap().unwrap().pushed_sha.unwrap();
    assert_ne!(sha_a, sha_b);
    assert_eq!(second.sha.as_deref(), Some(sha_b.as_str()));
    let superseded = merge_requests(&d)
        .into_iter()
        .find(|x| x.id == first.id)
        .unwrap();
    assert_eq!(superseded.status, DecisionStatus::Superseded);
    assert!(updated(
        &stub,
        &first_message,
        "The branch changed after this was asked"
    ));

    deliver(&slack, &[press_first(OWNER)]);
    ticks(&mut d, 3);
    assert_nothing_counted(&env, &gh, "the superseded message counts nothing");

    let second_message = stub.message_of(&second.id);
    deliver(&slack, &[press_on(&second_message, "approve", OWNER)]);
    run_until(
        &mut d,
        30,
        |_| {},
        |d| stage(d, "ACME-125") == Stage::Merged,
    );
    let recorded = approvals(&env);
    assert_eq!(recorded.len(), 1);
    assert_eq!(
        (recorded[0].user.as_str(), recorded[0].sha.as_deref()),
        (OWNER, Some(sha_b.as_str()))
    );
    assert_ne!(recorded[0].at, "");
    assert_eq!(recorded[0].permalink, second.message.unwrap().permalink);
    assert!(updated(
        &stub,
        &second_message,
        &format!("Approved by <@{OWNER}>")
    ));
    assert_eq!(merges(&gh), [format!("merge {pr} {sha_b}")]);
}

// AC: docs/design/owner-interface-design.md § Decisions and approvals: "A decision is written
//     `unsent`, with its content, before anything is posted. ... Posting fails -> it stays `unsent`
//     and is retried on every loop until delivered. ... An action or reply resolves a decision by the
//     id the message carries, so the owner answers once and the answer counts once."
//     Integration Verification Point: "Restart while a post fails: the decision stays `unsent`, is
//     delivered after restart, and the owner's one answer resolves it."
//     PRD AC-029: "Every decision that belongs to the owner reaches them as a Slack message they can
//     answer in place".
// Behavior: a sprint proposal decision is requested while the stub fails `chat.postMessage` -> the
//     daemon instance is dropped -> the stub accepts posts again and a new instance starts on the same
//     state directory -> the proposal is posted once -> U0123456789 presses Approve once.
// @lane: integration
// @dependency: Slack channel, decisions and approvals store, daemon
#[test]
fn unsent_decision_is_delivered_after_restart_and_resolved_by_one_answer() {
    let env = Env::new();
    env.configure(CONFIG);
    env.add_branch("develop");
    let purpose = "Returning customers check out without retyping their address";
    env.script_agents(&json!({
        "sprint-plan": [{"output": {"purpose": purpose, "key_results": ["Saved addresses fill in"],
                         "issues": [{"key": "ACME-123", "depends_on": []}], "message": format!("Sprint: {purpose}")}}],
        "requirements": [{"hang": true}]
    }));
    let gh = Arc::new(always_passing(&env, requiring_test()));
    let tracker = Arc::new(RecordingTracker::default());
    *tracker.issues.lock().unwrap() = vec![TrackerIssue {
        key: "ACME-123".into(),
        title: "Saved addresses".into(),
        body: "Fill in the saved address.".into(),
        url: "https://linear.app/acme/issue/ACME-123".into(),
        state: "Todo".into(),
        ..TrackerIssue::default()
    }];
    let stub = SlackStub::default();
    stub.fail("chat.postMessage", true);
    let mut d = daemon(
        &env,
        slack_services(gh.clone(), stub.channel(), tracker.clone()),
    );
    d.set_goal("Make checkout reliable for returning customers")
        .unwrap();
    run_until(
        &mut d,
        30,
        |_| {},
        |d| {
            d.decisions()
                .unwrap()
                .iter()
                .any(|x| x.kind == DecisionKind::Sprint)
        },
    );
    ticks(&mut d, 3);
    let unsent = d.decisions().unwrap().remove(0);
    assert_eq!(unsent.status, DecisionStatus::Unsent);
    assert!(unsent.text.contains(purpose));
    drop(d);

    stub.fail("chat.postMessage", false);
    let slack = stub.channel();
    let mut d = daemon(&env, slack_services(gh, slack.clone(), tracker));
    d.recover().unwrap();
    run_until(
        &mut d,
        30,
        |_| {},
        |d| d.decisions().unwrap()[0].status == DecisionStatus::Pending,
    );
    let posted: Vec<Value> = stub
        .posted()
        .into_iter()
        .filter(|m| m["metadata"]["event_payload"]["id"] == unsent.id)
        .collect();
    assert_eq!(posted.len(), 1);
    assert!(
        d.decisions().unwrap()[0]
            .message
            .as_ref()
            .unwrap()
            .permalink
            .is_some()
    );

    deliver(&slack, &[press_on(&posted[0], "approve", OWNER)]);
    run_until(
        &mut d,
        30,
        |_| {},
        |_| {
            env.events()
                .iter()
                .any(|e| e["event"] == "filed" && e["issue"] == "ACME-123")
        },
    );
    ticks(&mut d, 3);
    assert_eq!(approvals(&env).len(), 1);
    assert_eq!(d.decisions().unwrap()[0].status, DecisionStatus::Answered);
    let events = env.events();
    let count = |name: &str| events.iter().filter(|e| e["event"] == name).count();
    assert_eq!(count("decision-answered"), 1);
    assert_eq!(
        events
            .iter()
            .filter(|e| e["event"] == "sprint-fact" && e["to"]["stage"] == "running")
            .map(|e| e["fact"].clone())
            .collect::<Vec<_>>(),
        [json!("Approved")],
        "the sprint starts once, recorded with the fact that started it"
    );
}

// AC: docs/design/owner-interface-design.md § Acceptance Criteria > Channel: "When an owner mentions
//     the bot with `!pause` or `!resume` right after the mention, the system shall pause or resume
//     and confirm in the command's thread (AC-034)"; "A message that does not mention the bot shall
//     never be taken as a command (AC-034)"; "Messages and presses from users not in `slack.owners`
//     shall be ignored (AC-012)".
// Behavior: recorded `app_mention` envelopes (each with its `message` twin) with each command arrive
//     from U0123456789 (an owner) and from U0999999999 (not one), and a plain `!pause` without a
//     mention -> the Slack channel turns them into facts or drops them -> the daemon pauses or
//     resumes.
// Cases (reported distinctly): owner `@bot !pause`; owner `@bot !resume` after it; owner `!pause`
//     without a mention; `@bot !pause` from U0999999999.
// @lane: integration
// @dependency: Slack channel, daemon, store
#[test]
fn commands_from_owners_through_a_mention_take_effect_and_nothing_else_does() {
    let env = Env::new();
    env.configure(CONFIG);
    env.add_branch("develop");
    env.script_agents(&json!({"requirements": [{"hang": true}]}));
    let stub = SlackStub::default();
    let slack = stub.channel();
    let mut d = daemon(
        &env,
        slack_services(
            Arc::new(always_passing(&env, requiring_test())),
            slack.clone(),
            Arc::new(RecordingTracker::default()),
        ),
    );
    let halt = env.state().join("HALT");
    let replies_in = |ts: &str| {
        stub.calls("chat.postMessage")
            .into_iter()
            .filter(|m| m["thread_ts"] == ts)
            .collect::<Vec<_>>()
    };

    deliver(
        &slack,
        &mention(
            OWNER,
            "!pause the release is today",
            "1800000001.000001",
            None,
        ),
    );
    ticks(&mut d, 2);
    assert!(halt.exists(), "owner !pause");
    d.begin_issue("ACME-123", "Export", vec![]).unwrap();
    ticks(&mut d, 3);
    assert_eq!(env.records(), [] as [Value; 0], "no launch while paused");
    let paused = replies_in("1800000001.000001");
    assert_eq!(paused.len(), 1);
    assert!(paused[0]["text"].as_str().unwrap().starts_with("Paused."));

    deliver(
        &slack,
        &mention(OWNER, "!resume", "1800000002.000001", None),
    );
    ticks(&mut d, 2);
    assert!(!halt.exists(), "owner !resume");
    assert!(
        wait_until(10, || !env.records().is_empty()),
        "the open item launches"
    );
    assert_eq!(replies_in("1800000002.000001").len(), 1);

    let posts = stub.calls("chat.postMessage").len();
    deliver(
        &slack,
        &mention(STRANGER, "!pause", "1800000004.000001", None),
    );
    ticks(&mut d, 3);
    assert!(!halt.exists(), "a stranger's !pause pauses nothing");
    assert_eq!(stub.calls("chat.postMessage").len(), posts);
    assert_eq!(stub.calls("chat.postEphemeral"), [] as [Value; 0]);
}

// AC: the owner finds the dashboard from Slack: the port is the one its daemon serves on, which the
//     owner cannot see from Slack (owner, 2026-11-24).
// Behavior: an owner mentions the bot with `!dashboard` -> the reply in its thread gives the
//     dashboard's address.
// @lane: integration
// @dependency: Slack channel, daemon
#[test]
fn an_owner_asking_for_the_dashboard_gets_its_address() {
    let env = Env::new();
    env.configure(CONFIG);
    let stub = SlackStub::default();
    let slack = stub.channel();
    let mut d = daemon(
        &env,
        slack_services(
            Arc::new(always_passing(&env, requiring_test())),
            slack.clone(),
            Arc::new(RecordingTracker::default()),
        ),
    );

    deliver(
        &slack,
        &mention(OWNER, "!dashboard", "1800000005.000001", None),
    );
    ticks(&mut d, 2);

    let replies: Vec<Value> = stub
        .calls("chat.postMessage")
        .into_iter()
        .filter(|m| m["thread_ts"] == "1800000005.000001")
        .collect();
    assert_eq!(
        replies
            .iter()
            .map(|m| m["text"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["The dashboard: http://127.0.0.1:7171/"]
    );
}

// AC: "A restart reconstructs everything from the directory" (AC-039): what Slack received while
//     the factory was stopped is read from the last record on, so a command already answered is
//     not answered again.
// Behavior: an owner's `!dashboard` is answered -> it is recorded, so the next start's catch-up
//     begins after it.
// @lane: integration
// @dependency: Slack channel, daemon
#[test]
fn an_answered_command_is_recorded_so_a_restart_does_not_answer_it_again() {
    let env = Env::new();
    env.configure(CONFIG);
    let stub = SlackStub::default();
    let slack = stub.channel();
    let mut d = daemon(
        &env,
        slack_services(
            Arc::new(always_passing(&env, requiring_test())),
            slack.clone(),
            Arc::new(RecordingTracker::default()),
        ),
    );

    deliver(
        &slack,
        &mention(OWNER, "!dashboard", "1800000005.000001", None),
    );
    ticks(&mut d, 2);

    assert!(
        env.events()
            .iter()
            .any(|e| e["event"] == "command" && e["command"] == "dashboard"),
        "{:#?}",
        env.events()
    );
}

// AC: docs/design/owner-interface-design.md § Acceptance Criteria > Channel: "When Preview is
//     pressed on a merge request, the system shall start the issue's app and reply in the thread
//     with its local address (AC-031)".
//     PRD AC-031: "A merge request in Slack lets the owner start the issue's preview and receive its
//     local address in the thread, and approve or hold."
// Behavior: ACME-125 waits on a posted merge request; `[app] start` runs a test script that serves
//     the `[app] ready` URL on `$PORT` -> U0123456789 presses Preview -> the daemon starts the app in
//     ACME-125's worktree -> a thread reply gives the local address.
// @lane: integration
// @dependency: Slack channel, daemon, worktree app control, store
#[test]
fn preview_press_starts_the_issue_app_and_replies_with_its_local_address() {
    let env = Env::new();
    let start = format!(
        r#"printf "%s %s %s" "$PWD" "$PORT" "$OVENSPRING_DATA_DIR" > "$OVENSPRING_DATA_DIR/started"; {}"#,
        support::serve("\"$OVENSPRING_DATA_DIR\"")
    );
    let app = format!(
        "\n[app]\nstart = {}\nready = \"http://127.0.0.1:$PORT/started\"\n",
        serde_json::to_string(&start).unwrap()
    );
    let (mut d, gh, stub, slack) = held_merge(&env, &app, Map::new());
    let request = stub.message_of(&pending_merge_request(&d).unwrap().id);
    let ts = request["ts"].as_str().unwrap().to_owned();
    let replies = || {
        stub.calls("chat.postMessage")
            .into_iter()
            .filter(|m| {
                m["thread_ts"] == ts.as_str() && m["text"].as_str().unwrap().contains("preview")
            })
            .map(|m| m["text"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };

    deliver(&slack, &[press_on(&request, "preview", OWNER)]);
    d.tick().unwrap();
    assert!(wait_until(30, || replies().len() == 1), "{:?}", replies());
    let apps = env.state().join("apps/ACME-125");
    let port = std::fs::read_to_string(apps.join("port")).unwrap();
    let port = port.trim();
    assert_eq!(
        replies()[0],
        format!("ACME-125's preview: [http://localhost:{port}](http://localhost:{port})")
    );
    let started = std::fs::read_to_string(apps.join("data/started")).unwrap();
    let worktree = env.state().join("worktrees/ACME-125");
    assert_eq!(
        started,
        format!(
            "{} {port} {}",
            worktree.display(),
            apps.join("data").display()
        )
    );
    assert!(answers(&format!("http://127.0.0.1:{port}/started")));
    let pid = std::fs::read_to_string(apps.join("app.pid")).unwrap();

    deliver(&slack, &[press_on(&request, "preview", OWNER)]);
    d.tick().unwrap();
    assert!(wait_until(30, || replies().len() == 2), "{:?}", replies());
    assert_eq!(replies()[1], replies()[0]);
    assert_eq!(std::fs::read_to_string(apps.join("app.pid")).unwrap(), pid);
    assert_nothing_counted(&env, &gh, "Preview approves nothing");
    assert_eq!(stage(&d, "ACME-125"), Stage::HeldForOwner);
}

fn answers(url: &str) -> bool {
    std::process::Command::new("curl")
        .args(["-sf", "-o", "/dev/null", url])
        .status()
        .is_ok_and(|s| s.success())
}

// AC: UI Spec M-05: "Hold keeps it for the sprint review"; PRD AC-031: "a merge request also lets
//     the owner approve or hold."
// Behavior: ACME-125 waits on a posted merge request -> U0123456789 presses Hold -> the issue ends
//     the sprint held, its pull request unmerged, and the message shows who held it.
// @lane: integration
// @dependency: Slack channel, daemon, flow, store
#[test]
fn hold_keeps_the_merge_for_the_sprint_review() {
    let env = Env::new();
    let (mut d, gh, stub, slack) = held_merge(&env, "", Map::new());
    let request = pending_merge_request(&d).unwrap();
    let message = stub.message_of(&request.id);

    deliver(&slack, &[press_on(&message, "hold", OWNER)]);
    ticks(&mut d, 3);

    assert_eq!(stage(&d, "ACME-125"), Stage::Held);
    assert_nothing_counted(&env, &gh, "Hold approves nothing");
    let answered = merge_requests(&d).remove(0);
    let answer = answered.answer.unwrap();
    assert_eq!(
        (answer.kind, answer.user.as_str()),
        (AnswerKind::Hold, OWNER)
    );
    assert!(updated(
        &stub,
        &message,
        &format!("Held for the sprint review by <@{OWNER}>")
    ));
}

fn tracker_said(tracker: &RecordingTracker, key: &str) -> Vec<String> {
    tracker
        .calls()
        .into_iter()
        .filter(|(k, w)| k == key && w != "start")
        .map(|(_, w)| w)
        .collect()
}

// AC: PRD AC-067: "The owner can cancel an issue that has not ended, from its page on the dashboard
//     or from any Slack message it waits on them with; the factory stops its work, closes its pull
//     request and keeps its branch, and the tracker records it canceled with the owner's words."
// Behavior: ACME-125's merge is held for the owner -> the owner cancels it on the dashboard with a
//     reason -> the issue is dropped, its pull request closed and its branch kept, the merge request
//     settled, nothing merged, and the tracker told the owner's words.
// @lane: integration
// @dependency: dashboard cancel request, daemon, flow, FakeGitHub, Slack channel, RecordingTracker
#[test]
fn a_cancel_from_the_dashboard_ends_a_held_issue_and_settles_its_merge_request() {
    let env = Env::new();
    let tracker = Arc::new(RecordingTracker::default());
    let (mut d, gh, stub, _) = held_merge_told(&env, "", Map::new(), tracker.clone());
    let request = pending_merge_request(&d).unwrap();
    let message = stub.message_of(&request.id);
    let pr = d.issue("ACME-125").unwrap().unwrap().flow.pr.unwrap();

    ovenspring_daemon::records::request_cancel(
        d.store(),
        "ACME-125",
        "The index moves to the billing service.",
    )
    .unwrap();
    ticks(&mut d, 2);

    assert_eq!(stage(&d, "ACME-125"), Stage::Dropped);
    assert!(gh.calls().contains(&format!("close_pull {pr}")));
    assert!(!gh.calls().iter().any(|c| c.starts_with("delete_branch")));
    assert!(merges(&gh).is_empty(), "a canceled issue never merges");
    assert_eq!(merge_requests(&d)[0].status, DecisionStatus::Superseded);
    assert!(updated(&stub, &message, "Settled another way"));
    assert_eq!(
        tracker_said(&tracker, "ACME-125"),
        [
            "comment: Canceled by the owner: The index moves to the billing service.",
            "state: Canceled"
        ]
    ); // The issue page says the owner canceled it, in their words.
    let detail = ovenspring_daemon::history::issue_detail(d.store(), "ACME-125")
        .unwrap()
        .unwrap();
    assert_eq!(
        (detail.now.ended, detail.canceled.as_deref()),
        (
            Some(ovenspring_daemon::overview::Ending::Canceled),
            Some("The index moves to the billing service.")
        )
    );
}

// AC: PRD AC-067 (as above), from Slack.
// Behavior: ACME-125's executor escalates a writer's comment on its pull request and the lead asks
//     the owner about it, and ACME-126 waits on ACME-125 -> the owner presses Cancel issue on the
//     question -> ACME-125 is
//     dropped with its pull request closed, the tracker records it canceled, the question says who
//     canceled it, and ACME-126's lead learns it lost its dependency.
// @lane: integration
// @dependency: Slack channel, daemon, flow, FakeGitHub, RecordingTracker, fake agent
#[test]
fn cancel_on_a_question_the_issue_waits_on_ends_it_and_its_dependent_learns_of_it() {
    let env = Env::new();
    env.configure(CONFIG);
    env.add_branch("develop");
    let mut script = straight("ACME-125");
    script.extend(straight("ACME-126"));
    script.insert(
        "review-response@ACME-125".into(),
        json!([{"output": {"status": "escalation_needed",
                           "reason": "Including archived orders would widen the issue's outcome."}}]),
    );
    script.insert(
        "exception@ACME-125".into(),
        json!([
            {"output": {"action": "ask", "note": "A comment asks for more than the issue",
                        "question": "Should the export include archived orders? The comment asks for them; the issue does not."}},
            {"hang": true}
        ]),
    );
    env.script_agents(&Value::Object(script));
    let mut gh = always_passing(&env, requiring_test());
    gh.permissions.insert("acme-dev".into(), Permission::Write);
    gh.on_open = vec![Planned {
        kind: CommentKind::ReviewComment,
        author: "acme-dev".into(),
        bot: false,
        body: "Include archived orders too.".into(),
        branch: Some("ovenspring/ACME-125".into()),
    }];
    let gh = Arc::new(gh);
    let stub = SlackStub::default();
    let slack = stub.channel();
    let tracker = Arc::new(RecordingTracker::default());
    let mut d = daemon(
        &env,
        slack_services(gh.clone(), slack.clone(), tracker.clone()),
    );
    d.begin_issue("ACME-125", "Export", vec![]).unwrap();
    d.begin_issue("ACME-126", "Import", vec!["ACME-125".into()])
        .unwrap();
    let question = |d: &Daemon| {
        d.decisions().unwrap().into_iter().find(|x| {
            x.kind == DecisionKind::Spec
                && x.issue.as_deref() == Some("ACME-125")
                && x.status == DecisionStatus::Pending
        })
    };
    run_until(&mut d, 60, |_| {}, |d| question(d).is_some());
    let message = stub.message_of(&question(&d).unwrap().id);
    let pr = d.issue("ACME-125").unwrap().unwrap().flow.pr.unwrap();

    deliver(&slack, &[press_on(&message, "cancel", OWNER)]);
    ticks(&mut d, 3);

    assert_eq!(stage(&d, "ACME-125"), Stage::Dropped);
    assert!(gh.calls().contains(&format!("close_pull {pr}")));
    assert_eq!(
        tracker_said(&tracker, "ACME-125"),
        ["comment: Canceled by the owner.", "state: Canceled"]
    );
    assert!(updated(
        &stub,
        &message,
        &format!("Issue canceled by <@{OWNER}>")
    ));
    assert!(
        support::scripts::filed_steps(&env, "ACME-126").contains(&"exception".to_owned()),
        "{:?}",
        support::scripts::filed_steps(&env, "ACME-126")
    );
}

// AC: docs/design/owner-interface-design.md § Acceptance Criteria > Channel: "When an owner mentions
//     the bot outside a decision thread with anything but a command, the system shall start or
//     continue a `conversation` with the lead in that thread (AC-033)".
//     PRD AC-033: "...the lead is the only agent that speaks to the owner."
// Behavior: a recorded `app_mention` from U0123456789 "why is ACME-124 waiting?" outside any decision
//     thread -> a `conversation` item for the lead -> its `{reply}` output is posted in that thread ->
//     a follow-up message in the same thread continues the conversation.
// @lane: integration
// @dependency: Slack channel, daemon, flow, launcher, store, fake agent
#[test]
fn mention_starts_a_lead_conversation_answered_in_the_same_thread() {
    let env = Env::new();
    env.configure(CONFIG);
    env.add_branch("develop");
    let first_reply = "ACME-124 waits for ACME-123, whose pull request is in review.";
    let second_reply = "No: it starts as soon as ACME-123 merges.";
    env.script_agents(&json!({"conversation": [
        {"output": {"reply": first_reply}},
        {"output": {"reply": second_reply}}
    ]}));
    let stub = SlackStub::default();
    let slack = stub.channel();
    let mut d = daemon(
        &env,
        slack_services(
            Arc::new(always_passing(&env, requiring_test())),
            slack.clone(),
            Arc::new(RecordingTracker::default()),
        ),
    );
    let thread = "1800000010.000001";
    let lead_replies = || {
        stub.calls("chat.postMessage")
            .into_iter()
            .filter(|m| m["thread_ts"] == thread)
            .map(|m| m["text"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };

    deliver(
        &slack,
        &mention(OWNER, "why is ACME-124 waiting?", thread, None),
    );
    run_until(&mut d, 30, |_| {}, |_| lead_replies().len() == 1);
    assert_eq!(lead_replies(), [first_reply]);

    deliver(
        &slack,
        &[
            message(STRANGER, "me too", "1800000011.000001", Some(thread)),
            message(
                OWNER,
                "Does it need anything from me?",
                "1800000012.000001",
                Some(thread),
            ),
        ],
    );
    run_until(&mut d, 30, |_| {}, |_| lead_replies().len() == 2);
    ticks(&mut d, 3);
    assert_eq!(lead_replies(), [first_reply, second_reply]);

    let mut paths: Vec<_> = std::fs::read_dir(env.state().join("board/archive"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().contains("-conversation-"))
        .collect();
    paths.sort();
    let items: Vec<String> = paths
        .iter()
        .map(|p| std::fs::read_to_string(p).unwrap())
        .collect();
    assert_eq!(items.len(), 2);
    let (first, second) = (&items[0], &items[1]);
    assert!(first.contains("to: lead") && first.contains("why is ACME-124 waiting?"));
    for said in [
        "why is ACME-124 waiting?",
        first_reply,
        "Does it need anything from me?",
    ] {
        assert!(second.contains(said), "{said} in\n{second}");
    }
    assert!(!second.contains("me too"));
    let launched: Vec<(String, String)> =
        std::fs::read_to_string(env.state().join("launches.jsonl"))
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str::<Value>(l).unwrap())
            .filter(|l| l.get("role").is_some())
            .map(|l| {
                (
                    l["role"].as_str().unwrap().to_owned(),
                    l["step"].as_str().unwrap().to_owned(),
                )
            })
            .collect();
    assert_eq!(
        launched,
        [
            ("lead".to_owned(), "conversation".to_owned()),
            ("lead".to_owned(), "conversation".to_owned())
        ]
    );
}

// AC: docs/design/owner-interface-design.md § Acceptance Criteria > Channel: "When an owner replies
//     in a decision's thread, the system shall pass the reply to the waiting step as the answer
//     (AC-029, AC-017)"; UI Spec M-03: "Approve, or reply".
// Behavior: ACME-125's design declares a schema change -> the design approval M-03 is posted ->
//     U0123456789 replies in its thread instead of approving -> the lead's `design` runs again with
//     the reply as a source.
// @lane: integration
// @dependency: Slack channel, daemon, flow, store, fake agent
#[test]
fn a_reply_to_a_design_approval_goes_back_to_the_lead_as_the_answer() {
    let env = Env::new();
    env.configure(CONFIG);
    env.add_branch("develop");
    let mut steps = support::scripts::designed("ACME-125", &["schema"], &[("T1", "none")]);
    steps.insert(
        "design@ACME-125".into(),
        json!([{"change": {"path": "docs/design/ACME-125.md", "content": "# ACME-125\n"},
                "output": {"status": "completed", "documentType": "DesignDoc",
                           "path": "docs/design/ACME-125.md", "approvals": ["schema"]}},
               {"hang": true}]),
    );
    env.script_agents(&Value::Object(steps));
    let stub = SlackStub::default();
    let slack = stub.channel();
    let mut d = daemon(
        &env,
        slack_services(
            Arc::new(always_passing(&env, requiring_test())),
            slack.clone(),
            Arc::new(RecordingTracker::default()),
        ),
    );
    d.begin_issue("ACME-125", "Add an index", vec![]).unwrap();
    run_until(
        &mut d,
        60,
        |_| {},
        |d| {
            d.decisions()
                .unwrap()
                .iter()
                .any(|x| x.kind == DecisionKind::Design && x.status == DecisionStatus::Pending)
        },
    );
    let request = d.decisions().unwrap().remove(0);
    let posted = stub.message_of(&request.id);
    let upload = stub.calls("files.getUploadURLExternal");
    assert_eq!(
        (upload.len(), &upload[0]["filename"]),
        (1, &json!("ACME-125.md"))
    );
    assert_eq!(
        stub.calls("https://files.slack.test/upload/v1/F0FILE0001"),
        [json!("# ACME-125\n")]
    );
    let shared = stub.calls("files.completeUploadExternal");
    assert_eq!(
        (&shared[0]["thread_ts"], &shared[0]["channel_id"]),
        (&posted["ts"], &posted["channel"])
    );
    let reply = "Use a partial index on active orders only.";

    deliver(
        &slack,
        &[message(
            OWNER,
            reply,
            "1800000020.000001",
            posted["ts"].as_str(),
        )],
    );
    run_until(
        &mut d,
        30,
        |_| {},
        |_| {
            support::scripts::filed_steps(&env, "ACME-125")
                .iter()
                .filter(|s| *s == "design")
                .count()
                == 2
        },
    );

    let answered = d.decisions().unwrap().remove(0);
    assert_eq!(answered.status, DecisionStatus::Answered);
    assert_eq!(answered.answer.unwrap().text.as_deref(), Some(reply));
    assert_eq!(approvals(&env), [] as [ApprovalRecord; 0]);
    let item = d
        .store()
        .active_items()
        .unwrap()
        .into_iter()
        .map(|p| std::fs::read_to_string(p).unwrap())
        .find(|t| t.contains("step: design\n"))
        .expect("the design runs again");
    let answer = env
        .state()
        .join(format!("sources/answers/{}.md", request.id));
    assert!(item.contains(&answer.display().to_string()), "{item}");
    assert!(std::fs::read_to_string(answer).unwrap().contains(reply));
}

// AC: "When the merge setting is `factory`, the system shall merge only the head commit whose
//     required checks passed ... and every owner approval for that commit is recorded" (AC-026,
//     AC-028); new comments are judged before the gate (AC-022).
// Behavior: a merge request waits; a write-access reviewer comments and the issue goes back to
//     judging the comments -> the owner presses Approve merge on the earlier message -> nothing
//     merges while the comments are judged.
#[test]
fn approving_a_merge_while_new_comments_are_judged_merges_nothing() {
    let env = Env::new();
    let mut script = Map::new();
    script.insert("review-response@ACME-125".into(), json!([{"hang": true}]));
    let (mut d, gh, stub, slack) = held_merge(&env, "", script);
    let request = stub.message_of(&pending_merge_request(&d).unwrap().id);
    let pr = d.issue("ACME-125").unwrap().unwrap().flow.pr.unwrap();
    gh.comment(pr, "acme-dev", "Create the index concurrently.");
    run_until(
        &mut d,
        30,
        |_| {},
        |d| stage(d, "ACME-125") == Stage::running(Step::ReviewResponse),
    );

    deliver(&slack, &[press_on(&request, "approve", OWNER)]);
    ticks(&mut d, 5);

    assert_eq!(merges(&gh), [] as [String; 0]);
    assert_eq!(stage(&d, "ACME-125"), Stage::running(Step::ReviewResponse));
}

// AC: "Held categories are the union of what the design declared and what path rules in
//     `held.<category>.paths` match in the diff from base" — factory-core design § Merge gate.
// Behavior: the change adds a migration whose name is not ASCII -> the gate matches it to the held
//     path as stored, not as git quotes it for display -> the merge waits for the owner.
#[test]
fn a_held_path_with_a_name_git_would_quote_is_still_held() {
    let env = Env::new();
    let mut script = Map::new();
    script.insert(
        "implement@ACME-125".into(),
        json!([implement_run("db/migrations/0002_索引.sql")]),
    );
    let (d, gh, _, _) = held_merge(&env, "", script);
    assert_eq!(stage(&d, "ACME-125"), Stage::HeldForOwner);
    assert_eq!(merges(&gh), [] as [String; 0]);
}

// AC: "Each part of the daemon's pass (... the owner's answers ...) handles each issue and the
//     sprint on its own: an error is recorded ... and the pass goes on with the others" —
//     docs/design/factory-core-design.md § Exceptions, recovery and resilience.
// Behavior: the owner's reply to ACME-1's question and their `!pause` arrive together -> taking in
//     the reply fails (its answer cannot be written) -> the pause still takes effect, instead of
//     the rest of the batch Slack already handed over being dropped.
// @lane: integration
// @dependency: daemon, owner channel, RecordingChannel
#[test]
fn one_owner_event_that_fails_does_not_drop_the_others_taken_in_with_it() {
    use ovenspring_integrations::channel::ChannelEvent;
    use support::recording::RecordingChannel;

    let env = Env::new();
    env.configure("");
    env.script_agents(&json!({"requirements@ACME-1": [{"output": {
        "decision": "ask", "question": "Should the export include archived orders?",
        "structuralScale": "small", "convergence": {"outcome": "Orders export as CSV"}, "evidence": "README.md"}}]}));
    let channel = Arc::new(RecordingChannel::default());
    let mut d = daemon(
        &env,
        Services {
            github: Some(Arc::new(always_passing(&env, requiring_test()))),
            channel: channel.clone(),
            tracker: Some(Arc::new(RecordingTracker::default())),
        },
    );
    d.begin_issue("ACME-1", "Export", vec![]).unwrap();
    let question = |d: &Daemon| {
        d.decisions()
            .unwrap()
            .into_iter()
            .find(|x| x.kind == DecisionKind::Spec && x.status == DecisionStatus::Pending)
    };
    run_until(&mut d, 30, |_| {}, |d| question(d).is_some());
    let asked = question(&d).unwrap();
    let answers = env.state().join("sources/answers");
    let _ = std::fs::remove_dir_all(&answers);
    std::fs::write(&answers, "not a directory").unwrap();

    channel.push_event(ChannelEvent::Reply {
        decision: asked.id,
        text: "Active orders only.".into(),
        user: OWNER.into(),
    });
    channel.push_event(ChannelEvent::Command {
        name: "pause".into(),
        args: String::new(),
        user: OWNER.into(),
        at: None,
    });
    let _ = d.tick();
    assert!(question(&d).is_some(), "the reply could not be taken in");
    assert!(
        d.halt_file().exists(),
        "the pause was taken in with the failed reply"
    );
}
