#![cfg_attr(
    test,
    allow(
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used,
        clippy::expect_used,
        reason = "Integration tests use concise assertions; production paths remain strict."
    )
)]

mod support;

use std::sync::Arc;

use ovenspring_core::flow::Fact;
use ovenspring_core::item::Status;
use ovenspring_core::output::{PlannedIssue, SprintPlanOutput};
use ovenspring_core::sprint::SprintStage;
use ovenspring_core::step::Step;
use ovenspring_daemon::owner::DecisionStatus;
use ovenspring_integrations::channel::{Button, ChannelEvent, DecisionKind};
use ovenspring_integrations::github::BranchRules;
use ovenspring_integrations::tracker::{TrackerComment, TrackerIssue};
use serde_json::json;
use support::recording::{RecordingChannel, RecordingTracker};
use support::scripts::always_passing;
use support::{Env, daemon, daemon_timed, item as new_item};

// AC: "If the command's reason contains a credential the daemon held at any moment from the
//     command's start until its result was taken, then the system shall still drop the candidate
//     and record its filter-dropped event, and events.jsonl shall not contain that credential, even
//     when the credential is replaced while the command runs or before the event is recorded"
// Behavior: while the sprint is Proposing, a gated command returns a drop reason containing secret
//     A; Services then changes to secret B and Proposing ends -> the deferred event masks A.
// @lane: integration
// @dependency: Daemon::tick, Daemon::replace_services, Daemon::sprint_fact, Services::secrets,
//     Store
#[test]
fn credentials_remain_masked_when_a_drop_waits_for_the_sprint_proposal_to_end() {
    let env = Env::new();
    let secret_a = format!(
        "held-a-{}",
        env.dir.path().file_name().unwrap().to_string_lossy()
    );
    let secret_b = format!(
        "held-b-{}",
        env.dir.path().file_name().unwrap().to_string_lossy()
    );
    let script = &format!(
        "input=$(cat)\nn=$(cat .ovenspring/count 2>/dev/null || echo 0)\nn=$((n + 1))\nprintf '%s' \"$input\" > .ovenspring/input-$n.json\nprintf '%s\\n' \"$n\" > .ovenspring/count\nprintf '%s\\n' \"$n\" >> .ovenspring/calls\nif [ \"$n\" -eq 1 ]; then {gate_1}; cat .ovenspring/reason; exit 2; fi\nexit 0\n",
        gate_1 = wait_for_gate("gate-1")
    );
    let (mut daemon, tracker, channel) = filtered_daemon(
        &env,
        script,
        vec![issue("ACME-131")],
        vec![secret_a.clone()],
    );
    std::fs::write(
        env.filter_file("reason"),
        format!("before {secret_a} after\n"),
    )
    .unwrap();
    let proposal = set_proposing(&daemon, None);

    daemon.tick().unwrap();
    assert!(support::wait_until(3, || invocation_count(&env, "calls") == 1));
    *tracker.issues.lock().unwrap() = vec![issue("ACME-131"), issue("ACME-132")];
    std::fs::write(env.filter_file("gate-1"), "go").unwrap();
    support::run_until(
        &mut daemon,
        5,
        |daemon| {
            assert_unpublished(&env, daemon, "ACME-131");
            assert_no_filter_events(&env, "filter-dropped");
        },
        |_| invocation_count(&env, "calls") == 2,
    );
    let second_input: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(env.filter_file("input-2.json")).unwrap())
            .unwrap();
    assert_eq!(second_input["key"], "ACME-132");
    assert_eq!(invocation_count(&env, "calls"), 2);
    assert_no_filter_events(&env, "filter-dropped");
    daemon.replace_services(services(tracker, channel, vec![secret_b]));
    assert_no_filter_events(&env, "filter-dropped");

    complete_sprint_plan(&mut daemon, &proposal, &plan(&[]));

    let dropped = filter_events(&env, "filter-dropped");
    assert_eq!(dropped.len(), 1);
    assert_eq!(dropped[0]["reason"], "before [masked] after");
    let raw_events = std::fs::read_to_string(env.state().join("events.jsonl")).unwrap();
    assert!(!raw_events.contains(&secret_a));
}

// AC: "While an issue is selected (its key is in the current sprint state's plan, or an issue
//     record exists for it), the system shall neither rewrite nor remove its snapshot because of
//     tracker edits and shall not run the filter for it, with or without a filter configured"; and
//     "If the configured filter changed, the issue left the tracker's open list (even if reopened),
//     or its content when the result is taken differs from the content the command judged, or the
//     issue is selected when the result arrives, then the system shall discard that result: no
//     verdict, no publication change and no event come from it"
// Behavior: X's gated run is taken while the plan selects X, shown by Y's command starting -> X's
//     snapshot, open-list entry and the filter events are unchanged.
// @lane: integration
// @dependency: Daemon::tick, Store, RecordingTracker, RecordingChannel
#[test]
fn selection_while_a_filter_run_is_in_flight_discards_the_result() {
    let env = Env::new();
    env.configure("");
    let tracker = Arc::new(RecordingTracker::default());
    *tracker.issues.lock().unwrap() = vec![issue("ACME-131")];
    let channel = Arc::new(RecordingChannel::default());
    let mut daemon = daemon(&env, services(tracker.clone(), channel.clone(), Vec::new()));

    daemon.tick().unwrap();
    let snapshot_path = env.state().join("sources/issues/ACME-131.md");
    let selected_snapshot = std::fs::read_to_string(&snapshot_path).unwrap();
    let command = named_filter_command(
        &env,
        "filter.sh",
        &format!(
            "input=$(cat)\nn=$(cat .ovenspring/count 2>/dev/null || echo 0)\nn=$((n + 1))\nprintf '%s' \"$input\" > .ovenspring/input-$n.json\nprintf '%s\\n' \"$n\" > .ovenspring/count\nprintf '%s\\n' \"$n\" >> .ovenspring/calls\ncase \"$n\" in\n1) {gate_1}; printf 'selected issue denied\\n'; exit 2;;\n2) {gate_2}; printf 'other candidate denied\\n'; exit 2;;\nesac\n",
            gate_1 = wait_for_gate("gate-1"),
            gate_2 = wait_for_gate("gate-2")
        ),
    );
    env.set_tracker_filter(Some(&command));
    support::run_until(
        &mut daemon,
        5,
        |_| {},
        |_| invocation_count(&env, "calls") == 1,
    );
    set_proposing(&daemon, Some(plan(&["ACME-131"])));
    *tracker.issues.lock().unwrap() = vec![issue("ACME-131"), issue("ACME-132")];
    std::fs::write(env.filter_file("gate-1"), "go").unwrap();
    support::run_until(
        &mut daemon,
        5,
        |daemon| assert_candidate_retained(&env, daemon, "ACME-131", &selected_snapshot),
        |_| invocation_count(&env, "calls") == 2,
    );
    let first_input: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(env.filter_file("input-1.json")).unwrap())
            .unwrap();
    let second_input: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(env.filter_file("input-2.json")).unwrap())
            .unwrap();
    assert_eq!(first_input["key"], "ACME-131");
    assert_eq!(second_input["key"], "ACME-132");
    assert_eq!(invocation_count(&env, "calls"), 2);
    assert_eq!(
        std::fs::read_to_string(&snapshot_path).unwrap(),
        selected_snapshot
    );
    assert!(has_open_issue(&daemon, "ACME-131"));
    assert_no_filter_events(&env, "filter-dropped");
    assert_no_filter_events(&env, "filter-failed");
}

// AC: "When a revised proposal leaves out an issue the earlier proposal included and no issue
//     record exists for it, the system shall treat it as a candidate again"
// Behavior: X is admitted, a proposal selects X, the tracker edits X, and a revised proposal leaves
//     X out -> the command judges X's edited content and X's snapshot shows it.
// @lane: integration
// @dependency: Daemon::tick, Daemon::sprint_fact, Store, RecordingTracker, RecordingChannel
#[test]
fn a_selected_issue_left_out_of_a_revised_proposal_becomes_a_candidate_again() {
    let env = Env::new();
    let script = "input=$(cat)\nprintf '%s\\n' \"$input\" >> .ovenspring/calls\nexit 0\n";
    let (mut daemon, tracker, channel) =
        filtered_daemon(&env, script, vec![issue("ACME-131")], Vec::new());
    support::run_until(
        &mut daemon,
        5,
        |_| {},
        |daemon| has_open_issue(daemon, "ACME-131"),
    );
    let first_proposal = set_proposing(&daemon, None);
    complete_sprint_plan(&mut daemon, &first_proposal, &plan(&["ACME-131"]));
    let mut edited = issue("ACME-131");
    edited.body = "Edited while selected".into();
    tracker.issues.lock().unwrap()[0].body = edited.body.clone();

    reply_to_sprint_proposal(&mut daemon, &channel);
    complete_active_sprint_plan(&mut daemon, &plan(&[]));
    support::run_until(
        &mut daemon,
        5,
        |_| {},
        |_| {
            std::fs::read_to_string(env.state().join("sources/issues/ACME-131.md"))
                .is_ok_and(|snapshot| snapshot == edited.render())
        },
    );

    let bodies: Vec<String> = std::fs::read_to_string(env.filter_file("calls"))
        .unwrap()
        .lines()
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line).unwrap()["body"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert_eq!(bodies, ["Body of ACME-131", "Edited while selected"]);
    assert!(has_open_issue(&daemon, "ACME-131"));
}

// AC: "While an issue is selected (its key is in the current sprint state's plan, or an issue
//     record exists for it), the system shall neither rewrite nor remove its snapshot because of
//     tracker edits and shall not run the filter for it, with or without a filter configured";
//     "While the sprint is in Proposing, the system shall neither write nor remove any snapshot";
//     and "When the owner approves a proposal, the system shall take each issue's title and URL for
//     its issue record, and so for its requirements request, from the snapshot the lead selected"
// Behavior: while Proposing, the tracker edits still-open X, closes selected Z, and edits
//     unselected Y; neither selected snapshot changes; approval uses the original snapshot title
//     and URL for both X and Z, including X's requirements request.
// @lane: integration
// @dependency: Daemon::tick, Daemon::sprint_fact, Store, RecordingTracker, RecordingChannel
#[test]
fn a_proposal_freezes_its_selected_snapshot_and_saved_title_and_url() {
    let env = Env::new();
    env.configure("");
    let tracker = Arc::new(RecordingTracker::default());
    let mut selected = issue("ACME-131");
    selected.title = "Selected title: from the proposal".into();
    selected.url = "https://tracker.test/issues/selected-title".into();
    let mut selected_closed = issue("ACME-133");
    selected_closed.title = "Selected title closed before approval".into();
    selected_closed.url = "https://tracker.test/issues/selected-closed".into();
    let mut revised = issue("ACME-132");
    *tracker.issues.lock().unwrap() =
        vec![selected.clone(), selected_closed.clone(), revised.clone()];
    let channel = Arc::new(RecordingChannel::default());
    let mut daemon = daemon(&env, services(tracker.clone(), channel.clone(), Vec::new()));

    daemon.tick().unwrap();
    let sources = env.state().join("sources/issues");
    let selected_snapshot = std::fs::read_to_string(sources.join("ACME-131.md")).unwrap();
    let closed_snapshot = std::fs::read_to_string(sources.join("ACME-133.md")).unwrap();
    let revised_snapshot = std::fs::read_to_string(sources.join("ACME-132.md")).unwrap();
    set_proposing(&daemon, None);

    selected.title = "Latest tracker title".into();
    selected.url = "https://tracker.test/issues/latest-title".into();
    revised.title = "Latest candidate title".into();
    revised.body = "Latest candidate body".into();
    revised.url = "https://tracker.test/issues/latest-candidate".into();
    *tracker.issues.lock().unwrap() = vec![selected.clone(), revised.clone()];
    daemon.tick().unwrap();
    daemon.tick().unwrap();

    assert_eq!(
        std::fs::read_to_string(sources.join("ACME-131.md")).unwrap(),
        selected_snapshot
    );
    assert_eq!(
        std::fs::read_to_string(sources.join("ACME-133.md")).unwrap(),
        closed_snapshot
    );
    assert_eq!(
        std::fs::read_to_string(sources.join("ACME-132.md")).unwrap(),
        revised_snapshot
    );

    complete_active_sprint_plan(&mut daemon, &plan(&["ACME-131", "ACME-133"]));
    assert_eq!(
        std::fs::read_to_string(sources.join("ACME-131.md")).unwrap(),
        selected_snapshot
    );
    assert_eq!(
        std::fs::read_to_string(sources.join("ACME-133.md")).unwrap(),
        closed_snapshot
    );
    assert_eq!(
        std::fs::read_to_string(sources.join("ACME-132.md")).unwrap(),
        "# ACME-132: Latest candidate title\n\n- URL: https://tracker.test/issues/latest-candidate\n- State: Todo\n\n## Description\n\nLatest candidate body\n"
    );

    approve_sprint_proposal(&mut daemon, &channel);
    let record = daemon.issue("ACME-131").unwrap().unwrap();
    assert_eq!(record.title, "Selected title: from the proposal");
    assert_eq!(
        record.url.as_deref(),
        Some("https://tracker.test/issues/selected-title")
    );
    let closed_record = daemon.issue("ACME-133").unwrap().unwrap();
    assert_eq!(closed_record.title, "Selected title closed before approval");
    assert_eq!(
        closed_record.url.as_deref(),
        Some("https://tracker.test/issues/selected-closed")
    );

    daemon.apply("ACME-131", Fact::DependenciesMerged).unwrap();
    let requirements = daemon
        .store()
        .active_items()
        .unwrap()
        .into_iter()
        .map(|path| daemon.store().read_item(&path).unwrap())
        .find(|item| {
            item.front.issue.as_deref() == Some("ACME-131") && item.front.step == Step::Requirements
        })
        .expect("the selected issue reaches requirements");
    assert!(
        requirements
            .body
            .contains("Settle the requirements of ACME-131: Selected title: from the proposal.")
    );
}

// AC: "While an issue is selected (its key is in the current sprint state's plan, or an issue
//     record exists for it), the system shall neither rewrite nor remove its snapshot because of
//     tracker edits and shall not run the filter for it, with or without a filter configured"
// Behavior: the first PlanDone selects X and waits for approval; X is edited and Y becomes a
//     candidate -> Y is judged while X is never sent to the command and keeps its snapshot.
// @lane: integration
// @dependency: Daemon::tick, Daemon::sprint_fact, Store, RecordingTracker
#[test]
fn a_selected_candidate_is_not_rejudged_while_awaiting_approval() {
    let env = Env::new();
    let script = "input=$(cat)\nn=$(cat .ovenspring/count 2>/dev/null || echo 0)\nn=$((n + 1))\nprintf '%s' \"$input\" > .ovenspring/input-$n.json\nprintf '%s\\n' \"$n\" > .ovenspring/count\nprintf '%s\\n' \"$n\" >> .ovenspring/calls\ncase \"$n\" in\n1) exit 0;;\n2) printf 'other candidate denied\\n'; exit 2;;\nesac\n";
    let (mut daemon, tracker, _) =
        filtered_daemon(&env, script, vec![issue("ACME-131")], Vec::new());

    support::run_until(
        &mut daemon,
        5,
        |_| {},
        |daemon| has_open_issue(daemon, "ACME-131"),
    );
    let snapshot_path = env.state().join("sources/issues/ACME-131.md");
    let selected_snapshot = std::fs::read_to_string(&snapshot_path).unwrap();
    assert_eq!(invocation_count(&env, "calls"), 1);

    let first_proposal = set_proposing(&daemon, Some(plan(&["ACME-131"])));
    complete_sprint_plan(&mut daemon, &first_proposal, &plan(&["ACME-131"]));
    let pending_state = daemon.sprint().unwrap().unwrap();
    assert_eq!(pending_state.stage, SprintStage::AwaitingApproval);
    assert_eq!(pending_state.plan.unwrap().issues[0].key, "ACME-131");
    assert_candidate_retained(&env, &daemon, "ACME-131", &selected_snapshot);
    assert!(daemon.decisions().unwrap().iter().any(|decision| {
        decision.kind == DecisionKind::Sprint && decision.status == DecisionStatus::Pending
    }));

    tracker.issues.lock().unwrap()[0].body = "edited requirement after proposal".into();
    tracker.issues.lock().unwrap().push(issue("ACME-132"));
    support::run_until(
        &mut daemon,
        5,
        |daemon| assert_candidate_retained(&env, daemon, "ACME-131", &selected_snapshot),
        |_| issue_filter_events(&env, "filter-dropped", "ACME-132").len() == 1,
    );

    let second_input: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(env.filter_file("input-2.json")).unwrap())
            .unwrap();
    assert_eq!(second_input["key"], "ACME-132");
    assert_eq!(invocation_count(&env, "calls"), 2);
    assert_candidate_retained(&env, &daemon, "ACME-131", &selected_snapshot);
}

// AC: "When a proposal with no issue ends and intake admitted an issue while the lead was
//     proposing, the system shall publish that issue after the proposal ends and file a new
//     proposal; an issue published before the proposal began shall not file another one"
// Behavior: a candidate is admitted while the sprint is Proposing, with no snapshot before the
//     stage ends; an empty PlanDone ends the proposal -> the next pass publishes it and files a new
//     sprint-plan; a later empty proposal with no new admission files no duplicate.
// @lane: integration
// @dependency: Daemon::tick, Daemon::sprint_fact, Store, RecordingTracker
#[test]
fn an_issue_admitted_during_proposing_is_published_afterward_and_triggers_one_proposal() {
    let env = Env::new();
    env.script_agents(&json!({"sprint-plan": [{"hang": true}]}));
    let script = &format!(
        "input=$(cat)\nn=$(cat .ovenspring/count 2>/dev/null || echo 0)\nn=$((n + 1))\nprintf '%s' \"$input\" > .ovenspring/input-$n.json\nprintf '%s\\n' \"$n\" > .ovenspring/count\nprintf '%s\\n' \"$n\" >> .ovenspring/calls\nif [ \"$n\" -eq 1 ]; then {gate_1}; exit 0; fi\n{gate_2}\nexit 2\n",
        gate_1 = wait_for_gate("gate-1"),
        gate_2 = wait_for_gate("gate-2")
    );
    let command = env.write_filter_script(script);
    env.configure_filter(&command);
    let tracker = Arc::new(RecordingTracker::default());
    *tracker.issues.lock().unwrap() = vec![issue("ACME-131"), issue("ACME-132")];
    let channel = Arc::new(RecordingChannel::default());
    let mut timing = support::QUICK;
    timing.tracker_poll = std::time::Duration::from_secs(60);
    let mut services = services(tracker, channel, Vec::new());
    // The sprint plan launches in the base checkout, which is fetched with the GitHub connection.
    services.github = Some(Arc::new(always_passing(&env, BranchRules::default())));
    let mut daemon = daemon_timed(&env, services, timing);
    let proposal = set_proposing(&daemon, None);

    daemon.tick().unwrap();
    assert!(support::wait_until(3, || invocation_count(&env, "calls") == 1));
    assert_unpublished(&env, &daemon, "ACME-131");
    std::fs::write(env.filter_file("gate-1"), "go").unwrap();
    support::run_until(
        &mut daemon,
        5,
        |daemon| assert_unpublished(&env, daemon, "ACME-131"),
        |_| invocation_count(&env, "calls") == 2,
    );
    let second_input: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(env.filter_file("input-2.json")).unwrap())
            .unwrap();
    assert_eq!(second_input["key"], "ACME-132");
    assert_eq!(invocation_count(&env, "calls"), 2);

    complete_sprint_plan(&mut daemon, &proposal, &plan(&[]));
    assert!(env.state().join("sources/issues/ACME-131.md").exists());
    assert!(
        daemon
            .open_issues()
            .iter()
            .any(|candidate| candidate.key == "ACME-131")
    );
    let issue_opened = env
        .events()
        .iter()
        .filter(|event| event["event"] == "sprint-fact" && event["fact"] == "IssueOpened")
        .count();
    assert_eq!(issue_opened, 1);
    assert_eq!(sprint_plan_filed(&env), 1);
    assert!(support::wait_until(3, || env
        .dir
        .path()
        .join("pids-none-sprint-plan.json")
        .exists()));

    daemon.stop_launches();
    complete_active_sprint_plan(&mut daemon, &plan(&[]));

    assert_eq!(
        env.events()
            .iter()
            .filter(|event| event["event"] == "sprint-fact" && event["fact"] == "IssueOpened")
            .count(),
        1
    );
    assert_eq!(sprint_plan_filed(&env), 1);
}

// AC: "While no filter is configured, the system shall publish every candidate without running
//     anything"
// Behavior: with no filter configured, one tick -> each tracker issue's snapshot equals its
//     expected text, Daemon::open_issues lists every open key, and one tracker-issues event lists
//     them.
// @lane: integration
// @dependency: Daemon::tick, Store, RecordingTracker
#[test]
fn no_filter_poll_preserves_tracker_snapshots_and_publication_event() {
    let env = Env::new();
    env.configure("");
    let issues = vec![
        TrackerIssue {
            key: "ACME-131".into(),
            title: "Title of ACME-131".into(),
            body: "What ACME-131 asks for.".into(),
            url: "https://linear.app/acme/issue/ACME-131".into(),
            state: "Todo".into(),
            labels: vec!["checkout".into(), "urgent".into()],
            comments: vec![TrackerComment {
                author: "Reviewer".into(),
                body: "Keep the current route.".into(),
            }],
            ..TrackerIssue::default()
        },
        TrackerIssue {
            key: "ACME-132".into(),
            title: "Title of ACME-132".into(),
            body: "What ACME-132 asks for.".into(),
            url: "https://linear.app/acme/issue/ACME-132".into(),
            state: "Todo".into(),
            ..TrackerIssue::default()
        },
    ];
    let tracker = Arc::new(RecordingTracker::default());
    *tracker.issues.lock().unwrap() = issues;
    let mut daemon = daemon(
        &env,
        ovenspring_daemon::Services {
            github: None,
            channel: Arc::new(RecordingChannel::default()),
            tracker: Some(support::linear(tracker)),
            secrets: Vec::new(),
        },
    );

    daemon.tick().unwrap();

    let sources = env.state().join("sources/issues");
    assert_eq!(
        std::fs::read_to_string(sources.join("ACME-131.md")).unwrap(),
        "# ACME-131: Title of ACME-131\n\n- URL: https://linear.app/acme/issue/ACME-131\n- State: Todo\n- Labels: checkout, urgent\n\n## Description\n\nWhat ACME-131 asks for.\n\n## Comments\n\n### Reviewer\n\nKeep the current route.\n"
    );
    assert_eq!(
        std::fs::read_to_string(sources.join("ACME-132.md")).unwrap(),
        "# ACME-132: Title of ACME-132\n\n- URL: https://linear.app/acme/issue/ACME-132\n- State: Todo\n\n## Description\n\nWhat ACME-132 asks for.\n"
    );
    let keys: Vec<&str> = daemon
        .open_issues()
        .iter()
        .map(|issue| issue.key.as_str())
        .collect();
    assert_eq!(keys, ["ACME-131", "ACME-132"]);
    let tracker_events: Vec<_> = env
        .events()
        .into_iter()
        .filter(|event| event["event"] == "tracker-issues")
        .collect();
    assert_eq!(tracker_events.len(), 1);
    assert_eq!(tracker_events[0]["open"], json!(["ACME-131", "ACME-132"]));
}

// AC: Regression for closed-snapshot cleanup: the exception in "If an issue is proposed but not yet
//     begun and the tracker no longer reports it open, then the system shall keep its snapshot"
//     does not extend to an issue whose work is finished
// Behavior: an issue whose work is finished closes on the tracker -> its intake snapshot and
//     open-list entry are removed, since only a proposed issue not yet begun keeps its snapshot.
// @lane: integration
// @dependency: Daemon::tick, issue records, Store, RecordingTracker
#[test]
fn a_closed_finished_issue_is_removed_from_intake_sources() {
    let env = Env::new();
    env.configure("");
    let tracker = Arc::new(RecordingTracker::default());
    *tracker.issues.lock().unwrap() = vec![issue("ACME-131"), issue("ACME-132")];
    let channel = Arc::new(RecordingChannel::default());
    let mut daemon = daemon(&env, services(tracker.clone(), channel.clone(), Vec::new()));

    daemon.tick().unwrap();
    let snapshot = env.state().join("sources/issues/ACME-131.md");
    let other_snapshot = env.state().join("sources/issues/ACME-132.md");
    assert!(snapshot.exists());
    assert!(other_snapshot.exists());

    let proposal = set_proposing(&daemon, None);
    complete_sprint_plan(&mut daemon, &proposal, &plan(&["ACME-131", "ACME-132"]));
    approve_sprint_proposal(&mut daemon, &channel);
    assert_eq!(
        daemon
            .sprint()
            .unwrap()
            .unwrap()
            .plan
            .unwrap()
            .issues
            .iter()
            .map(|planned| planned.key.as_str())
            .collect::<Vec<_>>(),
        ["ACME-131", "ACME-132"]
    );

    daemon.apply("ACME-131", Fact::MergedOnGitHub).unwrap();
    assert!(
        daemon
            .issue("ACME-131")
            .unwrap()
            .unwrap()
            .flow
            .stage
            .is_finished()
    );
    assert!(snapshot.exists());
    assert!(has_open_issue(&daemon, "ACME-131"));
    assert!(other_snapshot.exists());
    assert!(has_open_issue(&daemon, "ACME-132"));

    tracker
        .issues
        .lock()
        .unwrap()
        .retain(|issue| issue.key != "ACME-131");

    daemon.tick().unwrap();

    assert!(!snapshot.exists());
    assert!(!has_open_issue(&daemon, "ACME-131"));
    assert!(other_snapshot.exists());
    assert!(has_open_issue(&daemon, "ACME-132"));
}

fn issue(key: &str) -> TrackerIssue {
    TrackerIssue {
        key: key.into(),
        title: format!("Title of {key}"),
        author: format!("Author of {key}"),
        body: format!("Body of {key}"),
        url: format!("https://linear.app/acme/issue/{key}"),
        state: "Todo".into(),
        ..TrackerIssue::default()
    }
}

fn services(
    tracker: Arc<RecordingTracker>,
    channel: Arc<RecordingChannel>,
    secrets: Vec<String>,
) -> ovenspring_daemon::Services {
    ovenspring_daemon::Services {
        github: None,
        channel,
        tracker: Some(support::linear(tracker)),
        secrets,
    }
}

fn filtered_daemon(
    env: &Env,
    script: &str,
    issues: Vec<TrackerIssue>,
    secrets: Vec<String>,
) -> (
    ovenspring_daemon::Daemon,
    Arc<RecordingTracker>,
    Arc<RecordingChannel>,
) {
    let command = env.write_filter_script(script);
    env.configure_filter(&command);
    let tracker = Arc::new(RecordingTracker::default());
    *tracker.issues.lock().unwrap() = issues;
    let channel = Arc::new(RecordingChannel::default());
    let daemon = daemon(env, services(tracker.clone(), channel.clone(), secrets));
    (daemon, tracker, channel)
}

/// A shell loop that waits until the test creates `.ovenspring/<name>`. It gives up after 3,000
/// checks (at least 30 seconds), so a test that fails before opening its gate leaves no command
/// running once its repository is removed.
fn wait_for_gate(name: &str) -> String {
    format!(
        "waited=0; while [ ! -f .ovenspring/{name} ]; do waited=$((waited + 1)); [ \"$waited\" -le 3000 ] || exit 1; sleep 0.01; done"
    )
}

fn named_filter_command(env: &Env, name: &str, script: &str) -> String {
    std::fs::write(env.filter_file(name), script).unwrap();
    format!("exec sh .ovenspring/{name}")
}

fn filter_events(env: &Env, kind: &str) -> Vec<serde_json::Value> {
    env.events()
        .into_iter()
        .filter(|event| event["event"] == kind)
        .collect()
}

fn issue_filter_events(env: &Env, kind: &str, issue: &str) -> Vec<serde_json::Value> {
    filter_events(env, kind)
        .into_iter()
        .filter(|event| event["issue"] == issue)
        .collect()
}

fn has_open_issue(daemon: &ovenspring_daemon::Daemon, key: &str) -> bool {
    daemon.open_issues().iter().any(|issue| issue.key == key)
}

fn assert_no_filter_events(env: &Env, kind: &str) {
    assert_eq!(filter_events(env, kind), Vec::<serde_json::Value>::new());
}

fn invocation_count(env: &Env, file: &str) -> usize {
    let path = env.filter_file(file);
    if !path.exists() {
        return 0;
    }
    std::fs::read_to_string(path).unwrap().lines().count()
}

fn assert_not_published(env: &Env, daemon: &ovenspring_daemon::Daemon, key: &str) {
    assert!(
        !env.state()
            .join(format!("sources/issues/{key}.md"))
            .exists()
    );
    assert!(!daemon.open_issues().iter().any(|issue| issue.key == key));
}

fn assert_unpublished(env: &Env, daemon: &ovenspring_daemon::Daemon, key: &str) {
    assert_not_published(env, daemon, key);
    assert!(
        !env.events()
            .iter()
            .any(|event| { event["event"] == "sprint-fact" && event["fact"] == "IssueOpened" }),
        "an unpublished candidate must not fire IssueOpened"
    );
}

fn assert_candidate_retained(
    env: &Env,
    daemon: &ovenspring_daemon::Daemon,
    key: &str,
    snapshot: &str,
) {
    assert_eq!(
        std::fs::read_to_string(env.state().join(format!("sources/issues/{key}.md"))).unwrap(),
        snapshot
    );
    assert!(has_open_issue(daemon, key));
    assert_eq!(
        issue_filter_events(env, "filter-dropped", key),
        Vec::<serde_json::Value>::new()
    );
}

fn set_waiting_for_issues(daemon: &ovenspring_daemon::Daemon) {
    let (mut sprint, _) = ovenspring_core::sprint::begin("filter-test");
    sprint.stage = ovenspring_core::sprint::SprintStage::WaitingForIssues;
    sprint.plan = Some(ovenspring_core::output::SprintPlanOutput {
        purpose: "Wait for admitted work".into(),
        key_results: vec![],
        issues: vec![],
        message: "No work is admitted yet.".into(),
    });
    daemon.store().write_record("sprint.json", &sprint).unwrap();
}

fn plan(issues: &[&str]) -> SprintPlanOutput {
    SprintPlanOutput {
        purpose: "Keep the selected issue's request intact".into(),
        key_results: vec!["The selected request reaches requirements unchanged".into()],
        issues: issues
            .iter()
            .map(|key| PlannedIssue {
                key: (*key).into(),
                depends_on: Vec::new(),
            })
            .collect(),
        message: "This proposal keeps the selected issue's request intact.".into(),
    }
}

fn set_proposing(
    daemon: &ovenspring_daemon::Daemon,
    current_plan: Option<SprintPlanOutput>,
) -> std::path::PathBuf {
    let (mut sprint, _) = ovenspring_core::sprint::begin("intake-test");
    sprint.stage = SprintStage::Proposing;
    sprint.plan = current_plan;
    daemon.store().write_record("sprint.json", &sprint).unwrap();

    let name = format!("sprint-plan-{}", sprint.id);
    let mut item = new_item(daemon.store(), &name, Step::SprintPlan, None);
    item.front.sprint = Some(sprint.id);
    item.front.status = Status::Doing;
    daemon.store().file_item(&name, &item).unwrap()
}

fn complete_sprint_plan(
    daemon: &mut ovenspring_daemon::Daemon,
    path: &std::path::Path,
    output: &SprintPlanOutput,
) {
    let item = daemon.store().read_item(path).unwrap();
    std::fs::write(&item.front.out, serde_json::to_vec(output).unwrap()).unwrap();
    daemon
        .store()
        .update_item(path, |item| item.front.status = Status::Done)
        .unwrap();
    daemon.tick().unwrap();
}

fn complete_active_sprint_plan(daemon: &mut ovenspring_daemon::Daemon, output: &SprintPlanOutput) {
    let path = daemon
        .store()
        .active_items()
        .unwrap()
        .into_iter()
        .find(|path| {
            daemon
                .store()
                .read_item(path)
                .is_ok_and(|item| item.front.step == Step::SprintPlan)
        })
        .expect("an active sprint-plan item exists");
    complete_sprint_plan(daemon, &path, output);
}

fn reply_to_sprint_proposal(daemon: &mut ovenspring_daemon::Daemon, channel: &RecordingChannel) {
    let decision = daemon
        .decisions()
        .unwrap()
        .into_iter()
        .find(|decision| {
            decision.kind == DecisionKind::Sprint && decision.status == DecisionStatus::Pending
        })
        .expect("the sprint proposal waits for the owner");
    let halt = daemon.halt_file();
    std::fs::write(&halt, "hold this pass while the reply is taken").unwrap();
    channel.push_event(ChannelEvent::Reply {
        decision: decision.id,
        text: "Revise the proposal without this issue.".into(),
        user: "U0123456789".into(),
    });
    daemon.tick().unwrap();
    std::fs::remove_file(halt).unwrap();
}

fn approve_sprint_proposal(daemon: &mut ovenspring_daemon::Daemon, channel: &RecordingChannel) {
    let decision = daemon
        .decisions()
        .unwrap()
        .into_iter()
        .find(|decision| {
            decision.kind == DecisionKind::Sprint && decision.status == DecisionStatus::Pending
        })
        .expect("the sprint proposal waits for the owner");
    channel.push_event(ChannelEvent::Action {
        decision: decision.id,
        button: Button::Approve,
        user: "U0123456789".into(),
    });
    daemon.tick().unwrap();
}

fn sprint_plan_filed(env: &Env) -> usize {
    env.events()
        .iter()
        .filter(|event| event["event"] == "filed" && event["step"] == "sprint-plan")
        .count()
}

// AC: "When the owner first sets the filter or changes its command, the system shall judge every
//     open candidate once under the new command, and until a candidate is judged publish its
//     current content as without a filter, an issue the earlier command dropped included"
// Behavior: command A has dropped X and admitted Y; changing to gated command B -> X is republished
//     and Y keeps its snapshot until B judges each, B checks both once, and B's results (X admitted,
//     Y dropped) decide the final snapshots.
// @lane: integration
// @dependency: Daemon::reload_config, Daemon::tick, Store, RecordingTracker
#[test]
fn changing_the_filter_command_rechecks_every_candidate_and_publishes_provisionally() {
    let env = Env::new();
    env.configure("");
    let command_a = named_filter_command(
        &env,
        "filter-a.sh",
        r#"input=$(cat)
case "$input" in
  *ACME-131*) printf 'denied by command A\n'; exit 2;;
esac
exit 0
"#,
    );
    env.set_tracker_filter(Some(&command_a));
    let tracker = Arc::new(RecordingTracker::default());
    *tracker.issues.lock().unwrap() = vec![issue("ACME-131"), issue("ACME-132")];
    let channel = Arc::new(RecordingChannel::default());
    let mut daemon = daemon(&env, services(tracker, channel, Vec::new()));
    support::run_until(
        &mut daemon,
        5,
        |_| {},
        |daemon| has_open_issue(daemon, "ACME-132"),
    );
    assert_not_published(&env, &daemon, "ACME-131");
    let snapshot_131 = issue("ACME-131").render();
    let snapshot_132 =
        std::fs::read_to_string(env.state().join("sources/issues/ACME-132.md")).unwrap();

    let command_b = named_filter_command(
        &env,
        "filter-b.sh",
        &format!(
            r#"input=$(cat)
case "$input" in
  *ACME-131*) printf 'x\n' >> .ovenspring/b-131; {b_gate_131}; exit 0;;
  *ACME-132*) printf 'x\n' >> .ovenspring/b-132; {b_gate_132}; printf 'denied by command B\n'; exit 2;;
esac
exit 1
"#,
            b_gate_131 = wait_for_gate("b-gate-131"),
            b_gate_132 = wait_for_gate("b-gate-132")
        ),
    );
    env.set_tracker_filter(Some(&command_b));
    support::run_until(
        &mut daemon,
        3,
        |_| {},
        |_| invocation_count(&env, "b-131") == 1,
    );
    assert_eq!(
        std::fs::read_to_string(env.state().join("sources/issues/ACME-131.md")).unwrap(),
        snapshot_131,
        "the issue command A dropped is republished until command B judges it"
    );
    assert!(has_open_issue(&daemon, "ACME-131"));
    assert_candidate_retained(&env, &daemon, "ACME-132", &snapshot_132);
    std::fs::write(env.filter_file("b-gate-131"), "go").unwrap();
    support::run_until(
        &mut daemon,
        5,
        |_| {},
        |_| invocation_count(&env, "b-132") == 1,
    );
    assert_eq!(
        std::fs::read_to_string(env.state().join("sources/issues/ACME-131.md")).unwrap(),
        snapshot_131,
        "command B's admission keeps the issue command A dropped"
    );
    assert!(has_open_issue(&daemon, "ACME-131"));
    std::fs::write(env.filter_file("b-gate-132"), "go").unwrap();
    support::run_until(
        &mut daemon,
        5,
        |_| {},
        |_| !issue_filter_events(&env, "filter-dropped", "ACME-132").is_empty(),
    );

    assert_eq!(invocation_count(&env, "b-131"), 1);
    assert_eq!(invocation_count(&env, "b-132"), 1);
    assert_not_published(&env, &daemon, "ACME-132");
    assert_eq!(
        std::fs::read_to_string(env.state().join("sources/issues/ACME-131.md")).unwrap(),
        snapshot_131
    );
    assert!(has_open_issue(&daemon, "ACME-131"));
    assert_eq!(
        filter_events(&env, "filter-dropped")
            .iter()
            .map(|event| (
                event["issue"].as_str().unwrap(),
                event["reason"].as_str().unwrap()
            ))
            .collect::<Vec<_>>(),
        [
            ("ACME-131", "denied by command A"),
            ("ACME-132", "denied by command B")
        ]
    );
    assert_no_filter_events(&env, "filter-failed");
}

// AC: "When the owner removes the filter, the system shall publish every candidate"
// Behavior: a configured command drops a candidate -> the owner removes the command -> the
//     publication pass restores its current snapshot and open-list entry without starting a
//     command.
// @lane: integration
// @dependency: Daemon::reload_config, Daemon::tick, Store, RecordingTracker
#[test]
fn removing_a_filter_publishes_every_current_candidate() {
    let env = Env::new();
    env.configure("");
    let command = named_filter_command(
        &env,
        "filter.sh",
        "printf 'x\\n' >> .ovenspring/calls\nprintf 'not admitted\\n'\nexit 2\n",
    );
    env.set_tracker_filter(Some(&command));
    let tracker = Arc::new(RecordingTracker::default());
    *tracker.issues.lock().unwrap() = vec![issue("ACME-131")];
    let mut daemon = daemon(
        &env,
        services(tracker, Arc::new(RecordingChannel::default()), Vec::new()),
    );

    daemon.tick().unwrap();
    support::run_until(
        &mut daemon,
        5,
        |daemon| assert_unpublished(&env, daemon, "ACME-131"),
        |_| filter_events(&env, "filter-dropped").len() == 1,
    );

    env.set_tracker_filter(None);
    daemon.tick().unwrap();

    assert_eq!(
        std::fs::read_to_string(env.state().join("sources/issues/ACME-131.md")).unwrap(),
        "# ACME-131: Title of ACME-131\n\n- URL: https://linear.app/acme/issue/ACME-131\n- State: Todo\n\n## Description\n\nBody of ACME-131\n"
    );
    assert!(
        daemon
            .open_issues()
            .iter()
            .any(|issue| issue.key == "ACME-131")
    );
    assert_eq!(invocation_count(&env, "calls"), 1);
    assert_eq!(filter_events(&env, "filter-dropped").len(), 1);
    assert_no_filter_events(&env, "filter-failed");
}

// AC: "If the configured filter changed, the issue left the tracker's open list (even if reopened),
//     or its content when the result is taken differs from the content the command judged, or the
//     issue is selected when the result arrives, then the system shall discard that result: no
//     verdict, no publication change and no event come from it"
// Behavior: command A is gated for X; the configuration changes to command B before A's result
//     arrives -> A's result is discarded and B's judgment decides X.
// @lane: integration
// @dependency: Daemon::reload_config, Daemon::tick, Store, RecordingTracker
#[test]
fn a_result_from_a_command_changed_while_it_ran_is_discarded() {
    let env = Env::new();
    env.configure("");
    let tracker = Arc::new(RecordingTracker::default());
    *tracker.issues.lock().unwrap() = vec![issue("ACME-131")];
    let mut daemon = daemon(
        &env,
        services(tracker, Arc::new(RecordingChannel::default()), Vec::new()),
    );
    daemon.tick().unwrap();

    let command_a = named_filter_command(
        &env,
        "filter-a.sh",
        &format!(
            "touch .ovenspring/a-started\n{}\nprintf 'stale rejection\\n'\nexit 2\n",
            wait_for_gate("a-gate")
        ),
    );
    env.set_tracker_filter(Some(&command_a));
    daemon.tick().unwrap();
    assert!(support::wait_until(3, || env
        .filter_file("a-started")
        .exists()));
    let command_b = named_filter_command(
        &env,
        "filter-b.sh",
        "printf 'x\\n' >> .ovenspring/b-calls\nprintf 'fresh rejection\\n'\nexit 2\n",
    );
    env.set_tracker_filter(Some(&command_b));
    daemon.tick().unwrap();

    std::fs::write(env.filter_file("a-gate"), "go").unwrap();
    support::run_until(
        &mut daemon,
        5,
        |_| {},
        |_| filter_events(&env, "filter-dropped").len() == 1,
    );
    assert_eq!(
        filter_events(&env, "filter-dropped")[0]["reason"],
        "fresh rejection"
    );
    assert_eq!(invocation_count(&env, "b-calls"), 1);
    assert_not_published(&env, &daemon, "ACME-131");
    assert_no_filter_events(&env, "filter-failed");
}

// AC: "When the command drops a candidate, the system shall remove its snapshot, leave it out of
//     Daemon::open_issues, fire no IssueOpened for it, and record one filter-dropped event with
//     issue (key), url and reason, the reason recorded as the command wrote it except that each
//     credential the daemon held at any moment from the command's start until its result was taken,
//     and each value in a token format the downloads mask, is replaced by [masked]"; and AC-072
//     "Nothing about a drop or a failure is posted to Slack or the tracker."
// Behavior: a new candidate's command exits 2 with a plain reason -> it has no snapshot, no
//     open-list entry and no IssueOpened, exactly one filter-dropped event holds its key, URL and
//     reason, and nothing is sent to Slack or the tracker.
// @lane: integration
// @dependency: Daemon::tick, Store, RecordingTracker, RecordingChannel
#[test]
fn a_dropped_candidate_is_removed_before_any_agent_can_read_it() {
    let env = Env::new();
    let script = "printf 'rejected by owner policy\\n'\nexit 2\n";
    let (mut daemon, tracker, channel) =
        filtered_daemon(&env, script, vec![issue("ACME-131")], Vec::new());
    set_waiting_for_issues(&daemon);

    support::run_until(
        &mut daemon,
        5,
        |daemon| assert_unpublished(&env, daemon, "ACME-131"),
        |_| filter_events(&env, "filter-dropped").len() == 1,
    );

    let dropped = filter_events(&env, "filter-dropped");
    assert_eq!(dropped.len(), 1);
    assert_eq!(dropped[0]["issue"], "ACME-131");
    assert_eq!(dropped[0]["url"], "https://linear.app/acme/issue/ACME-131");
    assert_eq!(dropped[0]["reason"], "rejected by owner policy");
    assert_eq!(tracker.calls(), Vec::new());
    assert_eq!(channel.sent(), Vec::new());
}

// AC: "When the filter runs for an issue, the system shall run sh -c <filter> with the owner's
//     checkout as working directory and write to its stdin one JSON object with exactly key, title,
//     body, url, labels, author, and comments (each {author, body})"; and "If the command exits 0,
//     then the system shall admit the issue"
// Behavior: a new candidate's command records its stdin and working directory and exits 0 -> it
//     receives the documented JSON from the repository root, and the candidate is published with
//     one IssueOpened and one sprint-plan filing.
// @lane: integration
// @dependency: Daemon::tick, Store, RecordingTracker
#[test]
fn exit_zero_admits_and_receives_the_documented_json_from_the_repository_root() {
    let env = Env::new();
    let script = "cat > .ovenspring/filter-input.json\npwd -P > .ovenspring/filter-pwd\nexit 0\n";
    let mut candidate = issue("ACME-131");
    candidate.title = "Check the selected route".into();
    candidate.author = "owner-login".into();
    candidate.body = "Use the selected route.".into();
    candidate.labels = vec!["checkout".into(), "filter".into()];
    candidate.comments = vec![TrackerComment {
        author: "reviewer-login".into(),
        body: "Keep this route.".into(),
    }];
    let (mut daemon, _, _) = filtered_daemon(&env, script, vec![candidate], Vec::new());
    set_waiting_for_issues(&daemon);

    support::run_until(
        &mut daemon,
        5,
        |_| {},
        |daemon| {
            daemon
                .open_issues()
                .iter()
                .any(|issue| issue.key == "ACME-131")
                && env
                    .events()
                    .iter()
                    .any(|event| event["event"] == "filed" && event["step"] == "sprint-plan")
        },
    );

    let input: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(env.filter_file("filter-input.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        input,
        json!({
            "key": "ACME-131",
            "title": "Check the selected route",
            "body": "Use the selected route.",
            "url": "https://linear.app/acme/issue/ACME-131",
            "labels": ["checkout", "filter"],
            "author": "owner-login",
            "comments": [{"author": "reviewer-login", "body": "Keep this route."}]
        })
    );
    let pwd = std::fs::read_to_string(env.filter_file("filter-pwd")).unwrap();
    assert_eq!(
        pwd.trim(),
        env.repo.canonicalize().unwrap().display().to_string()
    );
    assert!(env.state().join("sources/issues/ACME-131.md").exists());
    let issue_opened: Vec<_> = env
        .events()
        .into_iter()
        .filter(|event| event["event"] == "sprint-fact" && event["fact"] == "IssueOpened")
        .collect();
    assert_eq!(issue_opened.len(), 1);
    let sprint_plans: Vec<_> = env
        .events()
        .into_iter()
        .filter(|event| event["event"] == "filed" && event["step"] == "sprint-plan")
        .collect();
    assert_eq!(sprint_plans.len(), 1);
    assert_no_filter_events(&env, "filter-dropped");
    assert_no_filter_events(&env, "filter-failed");
}

// AC: "If the command's reason contains a credential the daemon held at any moment from the
//     command's start until its result was taken, then the system shall still drop the candidate
//     and record its filter-dropped event, and events.jsonl shall not contain that credential, even
//     when the credential is replaced while the command runs or before the event is recorded"
// Behavior: a candidate's command exits 2 with ordinary words around a secret held in Services ->
//     the issue is dropped and its event masks only the secret as [masked].
// @lane: integration
// @dependency: Daemon::tick, Services::secrets, Store, RecordingTracker
#[test]
fn a_drop_reason_containing_a_held_credential_is_masked_without_changing_surrounding_text() {
    let env = Env::new();
    let secret = format!(
        "held-secret-{}-{}",
        std::process::id(),
        env.dir.path().file_name().unwrap().to_string_lossy()
    );
    let (mut daemon, _, _) = filtered_daemon(
        &env,
        "cat .ovenspring/reason\nexit 2\n",
        vec![issue("ACME-131")],
        vec![secret.clone()],
    );
    std::fs::write(
        env.filter_file("reason"),
        format!("before {secret} after\n"),
    )
    .unwrap();

    daemon.tick().unwrap();
    support::run_until(
        &mut daemon,
        5,
        |daemon| assert_unpublished(&env, daemon, "ACME-131"),
        |_| filter_events(&env, "filter-dropped").len() == 1,
    );

    let dropped = filter_events(&env, "filter-dropped");
    assert_eq!(dropped.len(), 1);
    assert_eq!(dropped[0]["reason"], "before [masked] after");
    let raw_events = std::fs::read_to_string(env.state().join("events.jsonl")).unwrap();
    assert!(!raw_events.contains(&secret));
}

// AC: "If the command's reason contains a credential the daemon held at any moment from the
//     command's start until its result was taken, then the system shall still drop the candidate
//     and record its filter-dropped event, and events.jsonl shall not contain that credential, even
//     when the credential is replaced while the command runs or before the event is recorded"
// Behavior: a gated run starts with secret A, Services are replaced with B and then C, and the
//     command exits 2 with A and B in its reason -> both are masked and the drop applies.
// @lane: integration
// @dependency: Daemon::tick, Daemon::replace_services, Services::secrets, Store, RecordingTracker
#[test]
fn credentials_replaced_while_a_filter_run_is_held_remain_masked() {
    let env = Env::new();
    let secret_a = format!(
        "held-a-{}",
        env.dir.path().file_name().unwrap().to_string_lossy()
    );
    let secret_b = format!(
        "held-b-{}",
        env.dir.path().file_name().unwrap().to_string_lossy()
    );
    let secret_c = format!(
        "held-c-{}",
        env.dir.path().file_name().unwrap().to_string_lossy()
    );
    let script = &format!(
        "{gate}\ncat .ovenspring/reason\nexit 2\n",
        gate = wait_for_gate("gate")
    );
    let (mut daemon, tracker, channel) = filtered_daemon(
        &env,
        script,
        vec![issue("ACME-131")],
        vec![secret_a.clone()],
    );
    std::fs::write(
        env.filter_file("reason"),
        format!("before {secret_a} between {secret_b} after\n"),
    )
    .unwrap();

    daemon.tick().unwrap();
    daemon.replace_services(services(
        tracker.clone(),
        channel.clone(),
        vec![secret_b.clone()],
    ));
    daemon.replace_services(services(tracker, channel, vec![secret_c]));
    std::fs::write(env.filter_file("gate"), "go").unwrap();
    support::run_until(
        &mut daemon,
        5,
        |daemon| assert_unpublished(&env, daemon, "ACME-131"),
        |_| filter_events(&env, "filter-dropped").len() == 1,
    );

    let dropped = filter_events(&env, "filter-dropped");
    assert_eq!(dropped.len(), 1);
    assert_eq!(
        dropped[0]["reason"],
        "before [masked] between [masked] after"
    );
    let raw_events = std::fs::read_to_string(env.state().join("events.jsonl")).unwrap();
    assert!(!raw_events.contains(&secret_a));
    assert!(!raw_events.contains(&secret_b));
}

// AC: "When the command fails on a candidate, the system shall publish the candidate as without a
//     filter and record one filter-failed event with issue (key) and error"
// Behavior: a candidate's command exits 1 -> its snapshot is written, it is published, and one
//     filter-failed event records it.
// @lane: integration
// @dependency: Daemon::tick, Store, RecordingTracker
#[test]
fn a_failed_command_publishes_the_candidate_and_records_one_failure() {
    let env = Env::new();
    let (mut daemon, _, _) = filtered_daemon(&env, "exit 1\n", vec![issue("ACME-131")], Vec::new());

    daemon.tick().unwrap();
    support::run_until(
        &mut daemon,
        5,
        |_| {},
        |_| filter_events(&env, "filter-failed").len() == 1,
    );

    assert_eq!(
        std::fs::read_to_string(env.state().join("sources/issues/ACME-131.md")).unwrap(),
        "# ACME-131: Title of ACME-131\n\n- URL: https://linear.app/acme/issue/ACME-131\n- State: Todo\n\n## Description\n\nBody of ACME-131\n"
    );
    assert!(
        daemon
            .open_issues()
            .iter()
            .any(|issue| issue.key == "ACME-131")
    );
    let failed = filter_events(&env, "filter-failed");
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0]["issue"], "ACME-131");
    assert_eq!(failed[0]["error"], "exited with status 1");
}

// AC: "When a candidate's stdin JSON is identical to the one last judged by the configured command,
//     the system shall not run the command for it again, whatever the last verdict was"
// Behavior: two candidates receive exit 0 and exit 2; polls with identical inputs and then an edit
//     to one candidate -> no repeated run for unchanged inputs, the earlier snapshot remains while
//     the edit is judged, and one new run decides it.
// @lane: integration
// @dependency: Daemon::tick, Store, RecordingTracker
#[test]
fn unchanged_admitted_and_dropped_inputs_run_once_and_changed_content_runs_again() {
    let env = Env::new();
    let script = &format!(
        "input=$(cat)\ncase \"$input\" in\n  *ACME-131*)\n    printf 'x\\n' >> .ovenspring/calls-131\n    case \"$input\" in\n      *updated*) {rejudge_gate}; printf 'updated candidate denied\\n'; exit 2;;\n      *) exit 0;;\n    esac;;\n  *ACME-132*) printf 'x\\n' >> .ovenspring/calls-132; printf 'second candidate denied\\n'; exit 2;;\nesac\nexit 1\n",
        rejudge_gate = wait_for_gate("rejudge-gate")
    );
    let (mut daemon, tracker, _) = filtered_daemon(
        &env,
        script,
        vec![issue("ACME-131"), issue("ACME-132")],
        Vec::new(),
    );

    daemon.tick().unwrap();
    support::run_until(
        &mut daemon,
        5,
        |_| {},
        |daemon| {
            daemon
                .open_issues()
                .iter()
                .any(|issue| issue.key == "ACME-131")
                && filter_events(&env, "filter-dropped").len() == 1
        },
    );
    assert_eq!(invocation_count(&env, "calls-131"), 1);
    assert_eq!(invocation_count(&env, "calls-132"), 1);
    for _ in 0..3 {
        daemon.tick().unwrap();
    }
    assert_eq!(invocation_count(&env, "calls-131"), 1);
    assert_eq!(invocation_count(&env, "calls-132"), 1);

    let old_snapshot =
        std::fs::read_to_string(env.state().join("sources/issues/ACME-131.md")).unwrap();
    tracker.issues.lock().unwrap()[0].body = "updated requirement".into();
    daemon.tick().unwrap();
    assert_eq!(
        std::fs::read_to_string(env.state().join("sources/issues/ACME-131.md")).unwrap(),
        old_snapshot
    );
    assert!(support::wait_until(3, || invocation_count(
        &env,
        "calls-131"
    ) == 2));
    for _ in 0..4 {
        daemon.tick().unwrap();
        assert_eq!(
            std::fs::read_to_string(env.state().join("sources/issues/ACME-131.md")).unwrap(),
            old_snapshot
        );
        assert!(
            daemon
                .open_issues()
                .iter()
                .any(|issue| issue.key == "ACME-131")
        );
    }
    std::fs::write(env.filter_file("rejudge-gate"), "go").unwrap();
    support::run_until(
        &mut daemon,
        5,
        |_| {},
        |daemon| {
            filter_events(&env, "filter-dropped").len() == 2
                && !daemon
                    .open_issues()
                    .iter()
                    .any(|issue| issue.key == "ACME-131")
        },
    );
    assert_eq!(invocation_count(&env, "calls-131"), 2);
    assert_eq!(invocation_count(&env, "calls-132"), 1);
}

// AC: "If the configured filter changed, the issue left the tracker's open list (even if reopened),
//     or its content when the result is taken differs from the content the command judged, or the
//     issue is selected when the result arrives, then the system shall discard that result: no
//     verdict, no publication change and no event come from it"
// Behavior: a gated drop of content A is taken while the tracker shows edited content B, shown by
//     B's command starting; restoring A then applies no drop, and a fresh run judges A.
// @lane: integration
// @dependency: Daemon::tick, Store, RecordingTracker
#[test]
fn a_result_for_changed_content_is_discarded_and_the_current_content_is_judged() {
    let env = Env::new();
    let script = &format!(
        r#"input=$(cat)
printf '%s\n' "$input" >> .ovenspring/calls
case $(( $(wc -l < .ovenspring/calls) )) in
  1) {gate_1}; printf 'stale rejection\n'; exit 2;;
  2) {gate_2}; exit 0;;
esac
exit 0
"#,
        gate_1 = wait_for_gate("gate-1"),
        gate_2 = wait_for_gate("gate-2")
    );
    let original = issue("ACME-131");
    let (mut daemon, tracker, _) =
        filtered_daemon(&env, script, vec![original.clone()], Vec::new());

    support::run_until(
        &mut daemon,
        3,
        |_| {},
        |_| invocation_count(&env, "calls") == 1,
    );
    tracker.issues.lock().unwrap()[0].body = "edited requirement".into();
    daemon.tick().unwrap();
    std::fs::write(env.filter_file("gate-1"), "go").unwrap();
    support::run_until(
        &mut daemon,
        5,
        |_| {},
        |_| invocation_count(&env, "calls") == 2,
    );
    tracker.issues.lock().unwrap()[0].body = original.body.clone();
    daemon.tick().unwrap();
    assert_no_filter_events(&env, "filter-dropped");
    assert_unpublished(&env, &daemon, "ACME-131");

    std::fs::write(env.filter_file("gate-2"), "go").unwrap();
    support::run_until(
        &mut daemon,
        5,
        |_| {},
        |daemon| has_open_issue(daemon, "ACME-131"),
    );
    let inputs: Vec<serde_json::Value> = std::fs::read_to_string(env.filter_file("calls"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        inputs
            .iter()
            .map(|input| input["body"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            original.body.as_str(),
            "edited requirement",
            original.body.as_str()
        ]
    );
    assert_eq!(
        std::fs::read_to_string(env.state().join("sources/issues/ACME-131.md")).unwrap(),
        original.render()
    );
    assert_no_filter_events(&env, "filter-dropped");
}

// AC: "While a filter is configured and a candidate is not awaiting its first judgment after a
//     change of command (the command was already set when the daemon started, the candidate first
//     appeared after the change, or the new command has judged it), the system shall leave the
//     candidate's snapshot as it was until its new or changed content is judged: absent when it had
//     none, the earlier content when it had one"
// Behavior: a new candidate begins a gated filter run -> at each tick before its result it has no
//     snapshot and is absent from the open list; after exit 0 it is published.
// @lane: integration
// @dependency: Daemon::tick, Store, RecordingTracker
#[test]
fn a_new_steady_state_candidate_stays_unpublished_until_its_first_verdict() {
    let env = Env::new();
    let script = &format!("{gate}\nexit 0\n", gate = wait_for_gate("gate"));
    let (mut daemon, _, _) = filtered_daemon(&env, script, vec![issue("ACME-131")], Vec::new());

    daemon.tick().unwrap();
    assert_unpublished(&env, &daemon, "ACME-131");
    for _ in 0..5 {
        daemon.tick().unwrap();
        assert_unpublished(&env, &daemon, "ACME-131");
    }
    std::fs::write(env.filter_file("gate"), "go").unwrap();
    support::run_until(
        &mut daemon,
        5,
        |_| {},
        |daemon| {
            daemon
                .open_issues()
                .iter()
                .any(|issue| issue.key == "ACME-131")
        },
    );
    assert!(env.state().join("sources/issues/ACME-131.md").exists());
    assert_no_filter_events(&env, "filter-dropped");
}

// AC: "When the tracker no longer reports an issue open, the system shall forget its verdict, so an
//     issue reported open again is judged again"
// Behavior: X is admitted; the tracker closes X and then reports it open again with the same
//     content -> the command judges X a second time before X is published again.
// @lane: integration
// @dependency: Daemon::tick, Store, RecordingTracker
#[test]
fn a_closed_and_reopened_candidate_is_judged_again() {
    let env = Env::new();
    let script = "printf 'x\\n' >> .ovenspring/calls\nexit 0\n";
    let candidate = issue("ACME-131");
    let (mut daemon, tracker, _) =
        filtered_daemon(&env, script, vec![candidate.clone()], Vec::new());
    support::run_until(
        &mut daemon,
        5,
        |_| {},
        |daemon| has_open_issue(daemon, "ACME-131"),
    );

    tracker.issues.lock().unwrap().clear();
    daemon.tick().unwrap();
    assert_not_published(&env, &daemon, "ACME-131");
    tracker.issues.lock().unwrap().push(candidate);
    support::run_until(
        &mut daemon,
        5,
        |_| {},
        |daemon| has_open_issue(daemon, "ACME-131"),
    );

    assert_eq!(invocation_count(&env, "calls"), 2);
}

// AC: "When the daemon starts, the system shall judge every open candidate once before publishing
//     any candidate that has no snapshot"; and AC-073 "After a restart, each dropped issue is
//     checked once more"
// Behavior: a first daemon drops X; a new daemon over the same state and command -> X stays
//     unpublished while the command judges it again, and the second drop keeps it out.
// @lane: integration
// @dependency: Daemon construction and startup tick, Store, RecordingTracker
#[test]
fn a_restart_rechecks_a_dropped_candidate_before_publishing_it() {
    let env = Env::new();
    let script = format!(
        "printf 'x\\n' >> .ovenspring/calls\nif [ -f .ovenspring/restarted ]; then {}; fi\nprintf 'still rejected\\n'\nexit 2\n",
        wait_for_gate("gate")
    );
    let (mut first, tracker, channel) =
        filtered_daemon(&env, &script, vec![issue("ACME-131")], Vec::new());
    support::run_until(
        &mut first,
        5,
        |daemon| assert_not_published(&env, daemon, "ACME-131"),
        |_| filter_events(&env, "filter-dropped").len() == 1,
    );
    drop(first);

    std::fs::write(env.filter_file("restarted"), "").unwrap();
    let mut restarted = daemon(&env, services(tracker, channel, Vec::new()));
    restarted.tick().unwrap();
    assert!(support::wait_until(3, || invocation_count(&env, "calls") == 2));
    for _ in 0..4 {
        restarted.tick().unwrap();
        assert_not_published(&env, &restarted, "ACME-131");
    }
    std::fs::write(env.filter_file("gate"), "go").unwrap();
    support::run_until(
        &mut restarted,
        5,
        |daemon| assert_not_published(&env, daemon, "ACME-131"),
        |_| filter_events(&env, "filter-dropped").len() == 2,
    );

    assert_eq!(invocation_count(&env, "calls"), 2);
}

fn process_exists(pid: &str) -> bool {
    pid.parse::<u32>().is_ok_and(ovenspring_daemon::app::alive)
}

// AC: "When the daemon stops normally while a filter command runs, the system shall stop that
//     command's process group"
// Behavior: a gated filter spawns a child that remains alive; normal daemon stop -> both the filter
//     and its child are gone before the filter timeout.
// @lane: integration
// @dependency: Daemon::stop_launches, Store, sh -c process group
#[test]
fn normal_daemon_stop_terminates_the_filter_process_group() {
    assert_filter_group_stops(|mut daemon| daemon.stop_launches());
}

// AC: "When the daemon stops normally or is dropped while a filter command runs, the system shall
//     stop that command's process group"
// Behavior: a gated filter spawns a child that remains alive; the daemon is dropped, as when a
//     test ends or panics -> both the filter and its child are gone before the filter timeout.
// @lane: integration
// @dependency: Daemon drop, Store, sh -c process group
#[test]
fn dropping_the_daemon_terminates_the_filter_process_group() {
    assert_filter_group_stops(drop);
}

/// Starts a gated filter that leaves a child running, ends the daemon with `end`, and asserts the
/// filter and its child are gone well before the filter timeout.
fn assert_filter_group_stops(end: impl FnOnce(ovenspring_daemon::Daemon)) {
    const STOP_OBSERVATION_SECS: u64 = 3;

    let env = Env::new();
    let script = &format!(
        "echo $$ > .ovenspring/filter-pid\nsleep 30 &\nchild=$!\necho $child > .ovenspring/child-pid\ntouch .ovenspring/pids-ready\n{gate}\nwait $child\n",
        gate = wait_for_gate("gate")
    );
    let (mut daemon, _, _) = filtered_daemon(&env, script, vec![issue("ACME-131")], Vec::new());

    daemon.tick().unwrap();
    let pids_ready = env.filter_file("pids-ready");
    assert!(support::wait_until(3, || pids_ready.exists()));
    let filter_pid = std::fs::read_to_string(env.filter_file("filter-pid")).unwrap();
    let child_pid = std::fs::read_to_string(env.filter_file("child-pid")).unwrap();
    assert!(process_exists(filter_pid.trim()));
    assert!(process_exists(child_pid.trim()));

    end(daemon);

    let stopped = support::wait_until(STOP_OBSERVATION_SECS, || {
        !process_exists(filter_pid.trim()) && !process_exists(child_pid.trim())
    });
    if !stopped {
        let process_group = format!("-{}", filter_pid.trim());
        let _ = std::process::Command::new("kill")
            .args(["-KILL", "--", process_group.as_str()])
            .status();
    }
    assert!(
        stopped,
        "the command process group must stop within {STOP_OBSERVATION_SECS} seconds"
    );
}
