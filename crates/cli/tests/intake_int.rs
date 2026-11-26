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
use ovenspring_integrations::tracker::{TrackerComment, TrackerIssue};
use serde_json::json;
use support::recording::{RecordingChannel, RecordingTracker};
use support::{Env, daemon, daemon_timed, item as new_item};

// @lane: integration
// Shared setup: Env supplies a temporary owner repository and state directory; RecordingTracker
// substitutes only tracker I/O; filter scripts are real sh -c commands in the repository.
// @real-dependency: Store, sh -c filter process
//
// Planned test: a dropped candidate is removed before any agent can read it.
// AC: Issue Filter Design Document, Acceptance Criteria > AC-072: "When the command drops a
//     candidate, the system shall remove its snapshot, leave it out of Daemon::open_issues, fire no
//     IssueOpened for it, and record one filter-dropped event with issue (key), url and reason, the
//     reason recorded as the command wrote it except that each credential the daemon held at any
//     moment from the command's start until its result was taken, and each value in a token format
//     the downloads mask, is replaced by [masked]"; PRD AC-072: "Nothing about a drop or a failure
//     is posted to Slack or the tracker."
// Source: docs/design/issue-filter-design.md, Acceptance Criteria > The command's contract and Intake.
// Behavior: a new candidate's command exits 2 with a plain reason -> intake applies the result ->
//     its snapshot and open-list entry are absent, no IssueOpened is recorded for it, and exactly
//     one filter-dropped event contains its key, URL, and reason.
// @lane: integration
// @dependency: Daemon::tick, intake publication, sprint facts, Store, RecordingTracker, RecordingChannel
// @real-dependency: Store, sh -c filter process
// Primary failure mode: a dropped issue is written to sources/issues, appears in open_issues, or
//     emits IssueOpened before or after the result is applied.
// Proof obligation: drive Daemon::tick until the gated run is taken, then observe the real source
//     file, Daemon::open_issues, sprint-fact events, and events.jsonl; require no snapshot, no key,
//     no IssueOpened, no channel message or tracker mutation, and one filter-dropped event with
//     the exact unmasked reason.
// Permissible substitutes: RecordingTracker for external tracker I/O; deterministic filter script
//     in the temporary repository. Keep Store and the command process real.

// Planned test: a drop reason containing a held credential is masked without changing surrounding text.
// AC: Issue Filter Design Document, Acceptance Criteria > Intake: "If the command's reason contains
//     a credential the daemon held at any moment from the command's start until its result was
//     taken, then the system shall still drop the candidate and record its filter-dropped event,
//     and events.jsonl shall not contain that credential, even when the credential is replaced
//     while the command runs or before the event is recorded"
// Source: docs/design/issue-filter-design.md, Acceptance Criteria > Intake and Security Considerations.
// Behavior: a candidate's command exits 2 with ordinary words surrounding a generated secret held
//     in Services -> the issue is dropped and its event masks only the secret as [masked].
// @lane: integration
// @dependency: Daemon::tick, Services::secrets, Store, RecordingTracker
// @real-dependency: Store, sh -c filter process
// Primary failure mode: the drop reason is logged verbatim or masking changes the verdict and
//     admits the issue.
// Proof obligation: plant a random value with no token prefix in Services::secrets; return
//     "before <secret> after" on stdout; require a dropped snapshot, event reason "before [masked]
//     after", and no occurrence of the secret in the raw events.jsonl text.
// Permissible substitutes: generated test-only secret and RecordingTracker. Keep masking, Store,
//     and the command process real.

// Planned test: credentials replaced while a filter run is held remain masked.
// AC: Issue Filter Design Document, Acceptance Criteria > Intake: "If the command's reason contains
//     a credential the daemon held at any moment from the command's start until its result was
//     taken, then the system shall still drop the candidate and record its filter-dropped event,
//     and events.jsonl shall not contain that credential, even when the credential is replaced
//     while the command runs or before the event is recorded"
// Source: docs/design/issue-filter-design.md, Test Boundaries > Integration Verification Points
//     (credentials replaced during the run).
// Behavior: a gated run starts with secret A, Services are replaced with B and then C, and the
//     command exits 2 with A and B in its reason -> both held values are masked and the drop applies.
// @lane: integration
// @dependency: Daemon::tick, Daemon::replace_services, Services::secrets, Store, RecordingTracker
// @real-dependency: Store, sh -c filter process
// Primary failure mode: an earlier credential leaks after Services is replaced while the command
//     is running.
// Proof obligation: gate the shell script; start with only generated A, replace Services with only
//     B and then only C, release the gate, and emit A and B beside ordinary words. Require the drop,
//     both values replaced by [masked], surrounding words preserved, and neither value in events.jsonl.
// Permissible substitutes: generated test-only values and RecordingTracker. Keep service replacement,
//     masking, Store, and the command process real.

// Planned test: credentials remain masked when a drop waits for the sprint proposal to end.
// AC: Issue Filter Design Document, Acceptance Criteria > Intake: "If the command's reason contains
//     a credential the daemon held at any moment from the command's start until its result was
//     taken, then the system shall still drop the candidate and record its filter-dropped event,
//     and events.jsonl shall not contain that credential, even when the credential is replaced
//     while the command runs or before the event is recorded"
// Source: docs/design/issue-filter-design.md, Test Boundaries > Integration Verification Points
//     (credential replaced before the drop is logged).
// Behavior: while the sprint is Proposing, a gated command returns a drop reason containing secret
//     A; Services then changes to secret B and Proposing ends -> the deferred event masks A.
// @lane: integration
// @dependency: Daemon::tick, Daemon::replace_services, Daemon::sprint_fact, Services::secrets, Store
// @real-dependency: Store, sh -c filter process
// Primary failure mode: the verdict stores raw command output and writes a formerly held secret to
//     events.jsonl after a later service replacement.
// Proof obligation: gate X's exit-2 run while Proposing, add Y, and wait until Y's command starts
//     to prove X's result was taken; replace Services only then, require no event until Proposing
//     ends, and verify one dropped event with A masked and absent from the raw log.
// Permissible substitutes: generated test-only values and RecordingTracker. Keep Store, command,
//     Services replacement, and the sprint transition real.
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
    let script = "input=$(cat)\nn=$(cat .ovenspring/count 2>/dev/null || echo 0)\nn=$((n + 1))\nprintf '%s' \"$input\" > .ovenspring/input-$n.json\nprintf '%s\\n' \"$n\" > .ovenspring/count\nprintf '%s\\n' \"$n\" >> .ovenspring/calls\nif [ \"$n\" -eq 1 ]; then while [ ! -f .ovenspring/gate-1 ]; do sleep 0.01; done; cat .ovenspring/reason; exit 2; fi\nexit 0\n";
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

// Planned test: exit 0 admits a candidate.
// AC: Issue Filter Design Document, Acceptance Criteria > The command's contract: "If the command
//     exits 0, then the system shall admit the issue"
// Source: docs/design/issue-filter-design.md, Acceptance Criteria > The command's contract.
// Behavior: a new candidate's filter exits 0 -> the publication pass writes its snapshot and
//     includes it in Daemon::open_issues; with a sprint waiting for issues, it fires IssueOpened
//     and files one sprint-plan item.
// @lane: integration
// @dependency: Daemon::tick, Store, RecordingTracker
// @real-dependency: Store, sh -c filter process
// Primary failure mode: an exit-0 verdict is treated as dropped, failed, or left unpublished.
// Proof obligation: wait for the real filter process to finish and require the rendered snapshot,
//     the open-list entry, one IssueOpened and sprint-plan filing, and no filter-dropped or
//     filter-failed event.
// Permissible substitutes: RecordingTracker and a deterministic exit-0 script. Keep Store and the
//     command process real.

// Planned test: a failed command lets the candidate go on and records one failure.
// AC: Issue Filter Design Document, Acceptance Criteria > Intake: "When the command fails on a
//     candidate, the system shall publish the candidate as without a filter and record one
//     filter-failed event with issue (key) and error"
// Source: docs/design/issue-filter-design.md, Test Boundaries > Integration Verification Points.
// Behavior: a candidate's command exits 1 -> the publication pass writes its snapshot, publishes it,
//     and records one filter-failed event.
// @lane: integration
// @dependency: Daemon::tick, Store, RecordingTracker
// @real-dependency: Store, sh -c filter process
// Primary failure mode: a command failure blocks intake or is published without its failure event.
// Proof obligation: compare the full snapshot with independently written expected content, require
//     the open-list entry, and require exactly one filter-failed event naming the key and exit status.
// Permissible substitutes: RecordingTracker and a deterministic exit-1 script. Keep Store and the
//     command process real.

// Planned test: unchanged admitted and dropped inputs are judged once; changed content is judged again.
// AC: Issue Filter Design Document, Acceptance Criteria > Intake: "When a candidate's stdin JSON is
//     identical to the one last judged by the configured command, the system shall not run the
//     command for it again, whatever the last verdict was"
// Source: docs/design/issue-filter-design.md, Test Boundaries > Integration Verification Points.
// Behavior: two candidates receive exit 0 and exit 2; another poll with identical inputs and then
//     an edit to one candidate -> no repeated run for unchanged inputs, the earlier snapshot remains
//     while changed content is being judged, and one new run decides that edit.
// @lane: integration
// @dependency: Daemon::tick, Store, RecordingTracker, command invocation counter
// @real-dependency: Store, sh -c filter process
// Primary failure mode: an unchanged issue reruns every poll, or an edited issue reuses a stale verdict.
// Proof obligation: use a counter file to distinguish invocations per key; after two identical
//     polls require one run for each verdict, then change one field in the command JSON and gate the
//     new run. Require the earlier snapshot to remain unchanged until that run's result, then
//     require exactly one additional run for that key and publication matching its new verdict.
// Permissible substitutes: RecordingTracker and a deterministic counter script. Keep hashing,
//     verdict state, Store, and the command process real.

// Planned test: a new steady-state candidate stays unpublished until its first verdict.
// AC: Issue Filter Design Document, Acceptance Criteria > Intake: "While a filter is configured and
//     a candidate is not awaiting its first judgment after a change of command (the command was
//     already set when the daemon started, the candidate first appeared after the change, or the
//     new command has judged it), the system shall leave the candidate's snapshot as it was until
//     its new or changed content is judged: absent when it had none, the earlier content when it
//     had one"
// Source: docs/design/issue-filter-design.md, Test Boundaries > Integration Verification Points.
// Behavior: a new candidate begins a gated filter run -> at each tick before its result it has no
//     snapshot and is absent from the open list; after exit 0 it is published.
// @lane: integration
// @dependency: Daemon::tick, Store, RecordingTracker
// @real-dependency: Store, sh -c filter process
// Primary failure mode: a newly seen issue reaches sources/issues or open_issues before a steady
//     filter has judged it.
// Proof obligation: gate the real script, inspect the source file and open list on multiple ticks
//     before releasing the gate, then require publication only after the exit-0 result is taken.
// Permissible substitutes: RecordingTracker, a temporary gate file, and a deterministic script.
//     Keep Store and the command process real.

// Planned test: setting or changing a command rechecks every candidate and publishes provisionally.
// AC: Issue Filter Design Document, Acceptance Criteria > Intake: "When the owner first sets the
//     filter or changes its command, the system shall judge every open candidate once under the new
//     command, and until a candidate is judged publish its current content as without a filter, an
//     issue the earlier command dropped included"
// Source: docs/design/issue-filter-design.md, Test Boundaries > Integration Verification Points
//     (transition).
// Behavior: X and Y are published without a filter; setting gated command A -> both unchanged
//     candidates are checked while their snapshots stay available; A drops X and admits Y; changing
//     to gated command B -> X is provisionally republished, Y remains published, and B is run once
//     for both candidates before its results decide their snapshots.
// @lane: integration
// @dependency: Daemon::reload_config, Daemon::tick, Store, RecordingTracker
// @real-dependency: Store, sh -c filter process
// Primary failure mode: the old verdict survives a changed command, an old dropped issue stays
//     absent before its new judgment, or an unchanged candidate is skipped.
// Proof obligation: count A and B invocations by key; setting A must check both open candidates
//     despite unchanged input; while gated, keep the earlier snapshots; after A drops X and admits
//     Y, changing to B must provisionally republish X, retain Y, and check both keys once. Require
//     the snapshots after each command's verdict to match its outcome.
// Permissible substitutes: RecordingTracker and deterministic A/B scripts with gate files. Keep
//     configuration reload, Store, and the command processes real.

// Planned test: removing the filter publishes every current candidate.
// AC: Issue Filter Design Document, Acceptance Criteria > Intake: "When the owner removes the
//     filter, the system shall publish every candidate"
// Source: docs/design/issue-filter-design.md, Test Boundaries > Integration Verification Points.
// Behavior: a configured command drops a candidate -> the owner removes the command -> the
//     publication pass restores its current snapshot and open-list entry without starting a command.
// @lane: integration
// @dependency: Daemon::reload_config, Daemon::tick, Store, RecordingTracker
// @real-dependency: Store
// Primary failure mode: a dropped candidate remains absent or a command continues to run after
//     filter removal.
// Proof obligation: after reload, require the current rendered snapshot and key in open_issues,
//     and no additional filter invocation or filter event.
// Permissible substitutes: RecordingTracker. Keep configuration reload and Store real.

// Planned test: a result from a command changed away and back is discarded.
// AC: Issue Filter Design Document, Acceptance Criteria > Intake: "If the configured filter
//     changed, the issue left the tracker's open list (even if reopened), or its content changed
//     while a command judged it, or the issue is selected when the result arrives, then the system
//     shall discard that result: no verdict, no publication change and no event come from it"
// Source: docs/design/issue-filter-design.md, Test Boundaries > Integration Verification Points
//     (void).
// Behavior: command A is gated for X; configuration changes A to B and back to A before the gate
//     opens -> A's old result is discarded and X is judged by a fresh run.
// @lane: integration
// @dependency: Daemon::reload_config, Daemon::tick, Store, RecordingTracker
// @real-dependency: Store, sh -c filter process
// Primary failure mode: a result from an earlier command instance creates a verdict, snapshot
//     change, or event after the current configuration returns to the same string.
// Proof obligation: gate A, reload B then A, release the first A, and require no state or event
//     change from it; require a second invocation to judge the still-current candidate.
// Permissible substitutes: RecordingTracker and deterministic gated A/B scripts. Keep reload,
//     Store, and command processes real.

// Planned test: a filter change made while paused takes effect on the first tick after resume.
// AC: PRD AC-034 (pause and resume) and AC-073 (a changed command rechecks candidates; removing it
//     admits candidates).
// Source: docs/design/issue-filter-design.md, Test Boundaries > Integration Verification Points
//     (change while paused).
// Behavior: command A drops X; while HALT exists, changing to command B and then removing the
//     filter starts no command and publishes nothing; after resume, an unreachable tracker still
//     lets the first tick publish X from the last successful read.
// @lane: integration
// @dependency: Daemon::reload_config, Daemon::tick, Daemon::halt_file, RecordingTracker::unreachable
// @real-dependency: Store, sh -c filter process
// Primary failure mode: config reload runs intake before the HALT check, or failed tracker reads
//     skip the pass that applies the filter removal.
// Proof obligation: drop X with A; create HALT and reload B across several ticks; require no B run,
//     no X snapshot or open-list entry, and no sprint-plan filing; remove the filter, make tracker
//     reads fail, remove HALT, and on exactly one tick require X's last successful snapshot and
//     open-list entry while B's counter remains empty.
// Permissible substitutes: RecordingTracker for external tracker I/O; real config reload, HALT,
//     Store and filter process.

// Planned test: a result for content that changed during judgment is discarded.
// AC: Issue Filter Design Document, Acceptance Criteria > Intake: "If the configured filter
//     changed, the issue left the tracker's open list (even if reopened), or its content changed
//     while a command judged it, or the issue is selected when the result arrives, then the system
//     shall discard that result: no verdict, no publication change and no event come from it"
// Source: docs/design/issue-filter-design.md, Acceptance Criteria > Intake.
// Behavior: a gated run reads X's old input; the tracker edits X before the run returns exit 2 ->
//     the old result changes no snapshot or event, and the edited input receives a fresh run.
// @lane: integration
// @dependency: Daemon::tick, Store, RecordingTracker
// @real-dependency: Store, sh -c filter process
// Primary failure mode: a stale drop removes the updated issue or logs a reason for content the
//     current command never judged.
// Proof obligation: gate the first invocation, change a serialized input field, release it with
//     exit 2, and require the new content to remain unjudged and the old result to emit no event;
//     then require a fresh invocation whose captured stdin contains the edited body to decide it.
// Permissible substitutes: RecordingTracker and deterministic gated scripts. Keep Store and the
//     command process real.

// Planned test: a run for a closed and reopened candidate is discarded and the reopened key is judged again.
// AC: Issue Filter Design Document, Acceptance Criteria > Intake: "When the tracker no longer
//     reports an issue open, the system shall forget its verdict, so an issue reported open again
//     is judged again"; and "If the configured filter changed, the issue left the tracker's open
//     list (even if reopened), or its content changed while a command judged it, or the issue is
//     selected when the result arrives, then the system shall discard that result: no verdict, no
//     publication change and no event come from it"
// Source: docs/design/issue-filter-design.md, Acceptance Criteria > Intake.
// Behavior: X's gated run begins; the tracker closes X and then reports it open again before the
//     result arrives -> the old result has no effect and the reopened X receives a fresh judgment.
// @lane: integration
// @dependency: Daemon::tick, Store, RecordingTracker
// @real-dependency: Store, sh -c filter process
// Primary failure mode: a pre-close run is reused as the reopened issue's verdict.
// Proof obligation: require the first run to produce no verdict, publication change, or event;
//     observe a second command invocation for the same content after reopening and its result
//     applying to the current candidate.
// Permissible substitutes: RecordingTracker and a deterministic gate file. Keep Store and the
//     command process real.

// Planned test: selection while a filter run is in flight discards the result.
// AC: Issue Filter Design Document, Acceptance Criteria > End of intake: "While an issue is
//     selected (its key is in the current sprint state's plan, or an issue record exists for it),
//     the system shall neither rewrite nor remove its snapshot because of tracker edits and shall
//     not run the filter for it, with or without a filter configured"
// Source: docs/design/issue-filter-design.md, Test Boundaries > Integration Verification Points
//     (selection during a run).
// Behavior: X's gated run is taken while the plan selects X; Y's later command proves that point;
//     after a revised plan omits X, X's snapshot and open-list entry survive until a fresh gated
//     judgment for the unchanged input completes.
// @lane: integration
// @dependency: Daemon::tick, Daemon::sprint_fact, Store, RecordingTracker, RecordingChannel
// @real-dependency: Store, sh -c filter process
// Primary failure mode: a late filter drop deletes content the lead selected or logs an event for
//     an issue that left intake.
// Proof obligation: release X's gated run while selected and wait for Y's command to start; revise
//     the proposal to omit X, then require its snapshot and open-list entry to survive until a fresh
//     gated X invocation starts and supplies the drop. No X event may come from the first run.
// Permissible substitutes: RecordingTracker and deterministic sprint facts. Keep Store, sprint
//     selection, and the command process real.
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
        "input=$(cat)\nn=$(cat .ovenspring/count 2>/dev/null || echo 0)\nn=$((n + 1))\nprintf '%s' \"$input\" > .ovenspring/input-$n.json\nprintf '%s\\n' \"$n\" > .ovenspring/count\nprintf '%s\\n' \"$n\" >> .ovenspring/calls\ncase \"$n\" in\n1) while [ ! -f .ovenspring/gate-1 ]; do sleep 0.01; done; printf 'selected issue denied\\n'; exit 2;;\n2) while [ ! -f .ovenspring/gate-2 ]; do sleep 0.01; done; printf 'other candidate denied\\n'; exit 2;;\n3) while [ ! -f .ovenspring/gate-3 ]; do sleep 0.01; done; printf 'selected issue denied\\n'; exit 2;;\nesac\n",
    );
    env.set_tracker_filter(Some(&command));
    support::run_until(
        &mut daemon,
        5,
        |_| {},
        |_| invocation_count(&env, "calls") == 1,
    );
    let proposal = set_proposing(&daemon, Some(plan(&["ACME-131"])));
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

    complete_sprint_plan(&mut daemon, &proposal, &plan(&["ACME-131"]));
    reply_to_sprint_proposal(&mut daemon, &channel);
    complete_active_sprint_plan(&mut daemon, &plan(&[]));
    assert_eq!(
        std::fs::read_to_string(&snapshot_path).unwrap(),
        selected_snapshot
    );
    assert!(
        daemon
            .open_issues()
            .iter()
            .any(|candidate| candidate.key == "ACME-131")
    );
    assert!(
        !filter_events(&env, "filter-dropped")
            .iter()
            .any(|event| event["issue"] == "ACME-131")
    );

    std::fs::write(env.filter_file("gate-2"), "go").unwrap();
    support::run_until(
        &mut daemon,
        5,
        |daemon| assert_candidate_retained(&env, daemon, "ACME-131", &selected_snapshot),
        |_| invocation_count(&env, "calls") == 3,
    );
    let third_input: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(env.filter_file("input-3.json")).unwrap())
            .unwrap();
    assert_eq!(third_input, first_input);
    assert_eq!(invocation_count(&env, "calls"), 3);
    assert_eq!(filter_events(&env, "filter-dropped").len(), 1);
    assert_eq!(
        filter_events(&env, "filter-dropped")[0]["issue"],
        "ACME-132"
    );

    std::fs::write(env.filter_file("gate-3"), "go").unwrap();
    support::run_until(
        &mut daemon,
        5,
        |_| {},
        |_| filter_events(&env, "filter-dropped").len() == 2,
    );

    assert_eq!(invocation_count(&env, "calls"), 3);
    assert_no_filter_events(&env, "filter-failed");
    let dropped = filter_events(&env, "filter-dropped");
    assert_eq!(dropped[1]["issue"], "ACME-131");
    assert_eq!(dropped[1]["reason"], "selected issue denied");
    assert_not_published(&env, &daemon, "ACME-131");
}

// Planned test: a selected issue left out of a revised proposal becomes a candidate again.
// AC: Issue Filter Design Document, Acceptance Criteria > End of intake: "When a revised proposal
//     leaves out an issue the earlier proposal included and no issue record exists for it, the
//     system shall treat it as a candidate again"
// Source: docs/design/issue-filter-design.md, Test Boundaries > Integration Verification Points
//     (selected then left out during a run).
// Behavior: a gated run for X spans a PlanDone that selects X, the owner's reply, and a revised
//     PlanDone that leaves X out -> the unchanged input's result applies once it is a candidate.
// @lane: integration
// @dependency: Daemon::tick, Daemon::sprint_fact, Store, RecordingTracker, RecordingChannel
// @real-dependency: Store, sh -c filter process
// Primary failure mode: the selected-key state remains stuck after a revised proposal removes X.
// Proof obligation: preserve X's content while selected, return the same-content gated exit-2
//     result after the revised PlanDone, and require the snapshot removed, one drop event applied,
//     and no second command invocation.
// Permissible substitutes: RecordingTracker and deterministic sprint facts. Keep Store, selection
//     state, and the command process real.
#[test]
fn a_selected_issue_left_out_of_a_revised_proposal_becomes_a_candidate_again() {
    let env = Env::new();
    env.configure("");
    let tracker = Arc::new(RecordingTracker::default());
    *tracker.issues.lock().unwrap() = vec![issue("ACME-131")];
    let channel = Arc::new(RecordingChannel::default());
    let mut daemon = daemon(&env, services(tracker, channel.clone(), Vec::new()));

    daemon.tick().unwrap();
    let selected_snapshot =
        std::fs::read_to_string(env.state().join("sources/issues/ACME-131.md")).unwrap();
    let command = named_filter_command(
        &env,
        "filter.sh",
        "printf 'x\\n' >> .ovenspring/calls\necho $$ > .ovenspring/filter-pid\nwhile [ ! -f .ovenspring/gate ]; do sleep 0.01; done\nprintf 'still denied after revision\\n'\nexit 2\n",
    );
    env.set_tracker_filter(Some(&command));
    daemon.tick().unwrap();
    assert!(support::wait_until(3, || env
        .filter_file("filter-pid")
        .exists()));

    let first_proposal = set_proposing(&daemon, None);
    complete_sprint_plan(&mut daemon, &first_proposal, &plan(&["ACME-131"]));
    assert_eq!(
        std::fs::read_to_string(env.state().join("sources/issues/ACME-131.md")).unwrap(),
        selected_snapshot
    );
    reply_to_sprint_proposal(&mut daemon, &channel);
    complete_active_sprint_plan(&mut daemon, &plan(&[]));
    assert_eq!(
        std::fs::read_to_string(env.state().join("sources/issues/ACME-131.md")).unwrap(),
        selected_snapshot
    );
    assert_no_filter_events(&env, "filter-dropped");

    std::fs::write(env.filter_file("gate"), "go").unwrap();
    assert!(support::wait_until(3, || filter_process_exited(
        &env,
        "filter-pid"
    )));
    support::run_until(
        &mut daemon,
        5,
        |_| {},
        |_| filter_events(&env, "filter-dropped").len() == 1,
    );

    assert_eq!(invocation_count(&env, "calls"), 1);
    assert_not_published(&env, &daemon, "ACME-131");
    assert_eq!(
        filter_events(&env, "filter-dropped")[0]["reason"],
        "still denied after revision"
    );
}

// Planned test: a restart rechecks a dropped candidate before publishing it.
// AC: Issue Filter Design Document, Acceptance Criteria > Intake: "When the daemon starts, the
//     system shall judge every open candidate once before publishing any candidate that has no
//     snapshot"; and "When the tracker no longer reports an issue open, the system shall forget its
//     verdict"; PRD AC-073: "After a restart, each dropped issue is checked once more"
// Source: docs/design/issue-filter-design.md, Test Boundaries > Integration Verification Points.
// Behavior: a first daemon drops X and admits Y; a new daemon uses the same state directory with a
//     changed command B -> X has no snapshot before B judges it, Y keeps its earlier snapshot while
//     B judges it, and both are checked again.
// @lane: integration
// @dependency: Daemon construction and startup tick, Store, RecordingTracker
// @real-dependency: Store, sh -c filter process
// Primary failure mode: a dropped verdict survives restart or the issue is provisionally published
//     before it is rechecked.
// Proof obligation: construct a second daemon over the first daemon's state path after changing
//     the configured command; gate B, require X absent and Y's earlier snapshot unchanged while
//     each awaits judgment, then observe one new run for each and the results applied.
// Permissible substitutes: RecordingTracker and a temporary state directory. Keep Store and the
//     command process real.

// Planned test: a proposal freezes its selected snapshot and the saved issue title and URL.
// AC: Issue Filter Design Document, Acceptance Criteria > End of intake: "While an issue is selected
//     (its key is in the current sprint state's plan, or an issue record exists for it), the system
//     shall neither rewrite nor remove its snapshot because of tracker edits and shall not run the
//     filter for it, with or without a filter configured"; "While the sprint is in Proposing, the
//     system shall neither write nor remove any snapshot"; and "When the owner approves a proposal,
//     the system shall take each issue's title and URL for its issue record, and so for its
//     requirements request, from the snapshot the lead selected"
// Source: docs/design/issue-filter-design.md, Test Boundaries > Integration Verification Points
//     (proposal then tracker edit; proposed issue closes before approval).
// Behavior: while Proposing, the tracker edits still-open X, closes selected Z, and edits
//     unselected Y; neither selected snapshot changes; approval uses the original snapshot title
//     and URL for both X and Z, including X's requirements request.
// @lane: integration
// @dependency: Daemon::tick, Daemon::sprint_fact, Store, RecordingTracker, RecordingChannel
// @real-dependency: Store
// Primary failure mode: polling rewrites or removes selected content, or sprint start reads the
//     latest tracker title/URL instead of the snapshot the lead saw.
// Proof obligation: keep X open with edited metadata through approval and Z closed before approval;
//     assert both selected snapshots stay unchanged during Proposing, then require both issue
//     records to use their original title and URL and X's requirements request to use its title.
// Permissible substitutes: RecordingTracker and deterministic sprint facts. Keep Store, selection,
//     and sprint start real.
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

// Planned test: a selected candidate is not rejudged while proposed, then is judged when a revised proposal leaves it out.
// AC: Issue Filter Design Document, Acceptance Criteria > End of intake: "When a revised proposal
//     leaves out an issue the earlier proposal included and no issue record exists for it, the
//     system shall treat it as a candidate again"; and "While an issue is selected (its key is in
//     the current sprint state's plan, or an issue record exists for it), the system shall neither
//     rewrite nor remove its snapshot because of tracker edits and shall not run the filter for it,
//     with or without a filter configured"
// Source: docs/design/issue-filter-design.md, Test Boundaries > Integration Verification Points
//     (proposal then tracker edit).
// Behavior: the first PlanDone selects X and waits for approval; after X is edited and Y becomes a
//     candidate, Y is judged while X is never sent to the command; a revised proposal leaves X out
//     -> its changed input is judged once through a fresh gated invocation.
// @lane: integration
// @dependency: Daemon::tick, Daemon::sprint_fact, Store, RecordingTracker, RecordingChannel
// @real-dependency: Store, sh -c filter process
// Primary failure mode: X is rejudged while selected or stays excluded after the revised proposal
//     removes it.
// Proof obligation: complete the first PlanDone before editing X and adding Y; require the second
//     command input is Y while X is selected and awaiting approval; keep X's snapshot through the
//     revised empty proposal, then require one gated X input with the edited body and apply its result.
// Permissible substitutes: RecordingTracker and deterministic sprint facts. Keep Store, selection,
//     and the command process real.
#[test]
fn a_selected_candidate_is_not_rejudged_while_awaiting_approval_then_is_judged_when_left_out() {
    let env = Env::new();
    let script = "input=$(cat)\nn=$(cat .ovenspring/count 2>/dev/null || echo 0)\nn=$((n + 1))\nprintf '%s' \"$input\" > .ovenspring/input-$n.json\nprintf '%s\\n' \"$n\" > .ovenspring/count\nprintf '%s\\n' \"$n\" >> .ovenspring/calls\ncase \"$n\" in\n1) exit 0;;\n2) while [ ! -f .ovenspring/gate-2 ]; do sleep 0.01; done; printf 'other candidate denied\\n'; exit 2;;\n3) while [ ! -f .ovenspring/gate-3 ]; do sleep 0.01; done; printf 'edited candidate denied\\n'; exit 2;;\nesac\n";
    let (mut daemon, tracker, channel) =
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
        |_| invocation_count(&env, "calls") == 2,
    );
    let second_input: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(env.filter_file("input-2.json")).unwrap())
            .unwrap();
    assert_eq!(second_input["key"], "ACME-132");
    assert_eq!(invocation_count(&env, "calls"), 2);
    assert_candidate_retained(&env, &daemon, "ACME-131", &selected_snapshot);

    reply_to_sprint_proposal(&mut daemon, &channel);
    complete_active_sprint_plan(&mut daemon, &plan(&[]));
    assert_candidate_retained(&env, &daemon, "ACME-131", &selected_snapshot);

    std::fs::write(env.filter_file("gate-2"), "go").unwrap();
    support::run_until(
        &mut daemon,
        5,
        |daemon| assert_candidate_retained(&env, daemon, "ACME-131", &selected_snapshot),
        |_| invocation_count(&env, "calls") == 3,
    );
    let third_input: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(env.filter_file("input-3.json")).unwrap())
            .unwrap();
    assert_eq!(third_input["key"], "ACME-131");
    assert_eq!(third_input["body"], "edited requirement after proposal");
    assert_eq!(invocation_count(&env, "calls"), 3);
    assert_eq!(
        issue_filter_events(&env, "filter-dropped", "ACME-132").len(),
        1
    );
    assert_eq!(
        std::fs::read_to_string(&snapshot_path).unwrap(),
        selected_snapshot
    );
    assert_no_filter_events(&env, "filter-failed");

    std::fs::write(env.filter_file("gate-3"), "go").unwrap();
    support::run_until(
        &mut daemon,
        5,
        |_| {},
        |_| filter_events(&env, "filter-dropped").len() == 2,
    );
    assert_eq!(invocation_count(&env, "calls"), 3);
    assert_not_published(&env, &daemon, "ACME-131");
    let dropped = issue_filter_events(&env, "filter-dropped", "ACME-131");
    assert_eq!(dropped.len(), 1);
    assert_eq!(dropped[0]["reason"], "edited candidate denied");
}

// Planned test: an issue admitted during Proposing is published afterward and triggers a proposal once.
// AC: Issue Filter Design Document, Acceptance Criteria > End of intake: "When a proposal with no
//     issue ends and intake admitted an issue while the lead was proposing, the system shall
//     publish that issue after the proposal ends and file a new proposal; an issue published before
//     the proposal began shall not file another one"
// Source: docs/design/issue-filter-design.md, Test Boundaries > Integration Verification Points
//     (empty proposal).
// Behavior: a candidate is admitted and its result is taken while the sprint is Proposing, with no
//     snapshot before the stage ends; an empty PlanDone ends the proposal -> the next pass publishes
//     it and files a new sprint-plan; a later empty proposal with no new admission files no duplicate.
// @lane: integration
// @dependency: Daemon::tick, Daemon::sprint_fact, Store, RecordingTracker
// @real-dependency: Store, sh -c filter process
// Primary failure mode: the issue is silently stranded because IssueOpened happened too early, or
//     the same admission repeatedly files sprint-plan work.
// Proof obligation: set tracker_poll beyond the test, gate X's exit-0 result while Proposing, and
//     wait for Y's command to start as proof X was taken; require no snapshot while Proposing, then
//     finish an empty PlanDone and immediately require X's snapshot, open-list entry and exactly
//     one new sprint-plan filing before another tracker poll can run.
// Permissible substitutes: RecordingTracker and deterministic sprint facts. Keep Store, the
//     publication pass, and the command process real.
#[test]
fn an_issue_admitted_during_proposing_is_published_afterward_and_triggers_one_proposal() {
    let env = Env::new();
    env.script_agents(&json!({"sprint-plan": [{"hang": true}]}));
    let script = "input=$(cat)\nn=$(cat .ovenspring/count 2>/dev/null || echo 0)\nn=$((n + 1))\nprintf '%s' \"$input\" > .ovenspring/input-$n.json\nprintf '%s\\n' \"$n\" > .ovenspring/count\nprintf '%s\\n' \"$n\" >> .ovenspring/calls\nif [ \"$n\" -eq 1 ]; then while [ ! -f .ovenspring/gate-1 ]; do sleep 0.01; done; exit 0; fi\nwhile [ ! -f .ovenspring/gate-2 ]; do sleep 0.01; done\nexit 2\n";
    let command = env.write_filter_script(script);
    env.configure_filter(&command);
    let tracker = Arc::new(RecordingTracker::default());
    *tracker.issues.lock().unwrap() = vec![issue("ACME-131"), issue("ACME-132")];
    let channel = Arc::new(RecordingChannel::default());
    let mut timing = support::QUICK;
    timing.tracker_poll = std::time::Duration::from_secs(60);
    let mut daemon = daemon_timed(&env, services(tracker, channel, Vec::new()), timing);
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

// Planned test: normal daemon stop terminates the filter process group.
// AC: Issue Filter Design Document, Acceptance Criteria > The command's contract: "When the daemon
//     stops normally while a filter command runs, the system shall stop that command's process group"
// Source: docs/design/issue-filter-design.md, Test Boundaries > Integration Verification Points.
// Behavior: a gated filter spawns a child that remains alive; normal daemon stop -> both the filter
//     and its child are gone.
// @lane: integration
// @dependency: Daemon::stop_launches, process-group lifecycle, Store
// @real-dependency: sh -c filter process and child process
// Primary failure mode: the daemon stops while the owner's command or its child outlives it.
// Proof obligation: set the filter timeout beyond the full stop-observation window, start the
//     command and child, call the normal stop path, and verify neither process remains well before
//     the automatic timeout.
// Permissible substitutes: deterministic shell script and temporary pid/gate files. Keep process
//     group handling real.

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
            tracker: Some(tracker),
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
        tracker: Some(tracker),
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

fn filter_process_exited(env: &Env, pid_file: &str) -> bool {
    let Ok(pid) = std::fs::read_to_string(env.filter_file(pid_file)) else {
        return false;
    };
    let pid = pid.trim();
    !pid.is_empty() && !process_exists(pid)
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

#[test]
fn setting_or_changing_a_filter_rechecks_every_candidate_and_publishes_provisionally() {
    let env = Env::new();
    env.configure("");
    let tracker = Arc::new(RecordingTracker::default());
    *tracker.issues.lock().unwrap() = vec![issue("ACME-131"), issue("ACME-132")];
    let channel = Arc::new(RecordingChannel::default());
    let mut daemon = daemon(&env, services(tracker, channel, Vec::new()));

    daemon.tick().unwrap();
    let initial_131 =
        std::fs::read_to_string(env.state().join("sources/issues/ACME-131.md")).unwrap();
    let initial_132 =
        std::fs::read_to_string(env.state().join("sources/issues/ACME-132.md")).unwrap();
    assert_eq!(
        daemon
            .open_issues()
            .iter()
            .map(|issue| issue.key.as_str())
            .collect::<Vec<_>>(),
        ["ACME-131", "ACME-132"]
    );

    run_command_a_transition(&env, &mut daemon, &initial_131, &initial_132);
    run_command_b_transition(&env, &mut daemon, &initial_131, &initial_132);

    assert_eq!(invocation_count(&env, "a-131"), 1);
    assert_eq!(invocation_count(&env, "a-132"), 1);
    assert_eq!(invocation_count(&env, "b-131"), 1);
    assert_eq!(invocation_count(&env, "b-132"), 1);
    assert!(!env.state().join("sources/issues/ACME-131.md").exists());
    assert_eq!(
        std::fs::read_to_string(env.state().join("sources/issues/ACME-132.md")).unwrap(),
        initial_132
    );
    assert_eq!(
        filter_events(&env, "filter-dropped")
            .iter()
            .map(|event| event["issue"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["ACME-131", "ACME-131"]
    );
}

fn run_command_a_transition(
    env: &Env,
    daemon: &mut ovenspring_daemon::Daemon,
    initial_131: &str,
    initial_132: &str,
) {
    let command_a = named_filter_command(
        env,
        "filter-a.sh",
        r#"input=$(cat)
case "$input" in
  *ACME-131*) printf 'x\n' >> .ovenspring/a-131; while [ ! -f .ovenspring/a-gate-131 ]; do sleep 0.01; done; printf 'denied by command A\n'; exit 2;;
  *ACME-132*) printf 'x\n' >> .ovenspring/a-132; echo $$ > .ovenspring/a-pid-132; while [ ! -f .ovenspring/a-gate-132 ]; do sleep 0.01; done; exit 0;;
esac
exit 1
"#,
    );
    env.set_tracker_filter(Some(&command_a));
    daemon.tick().unwrap();
    assert!(support::wait_until(3, || invocation_count(env, "a-131") == 1));
    assert_eq!(
        std::fs::read_to_string(env.state().join("sources/issues/ACME-131.md")).unwrap(),
        initial_131
    );
    assert_eq!(
        std::fs::read_to_string(env.state().join("sources/issues/ACME-132.md")).unwrap(),
        initial_132
    );
    std::fs::write(env.filter_file("a-gate-131"), "go").unwrap();
    support::run_until(daemon, 5, |_| {}, |_| invocation_count(env, "a-132") == 1);
    assert!(!env.state().join("sources/issues/ACME-131.md").exists());
    assert_eq!(
        std::fs::read_to_string(env.state().join("sources/issues/ACME-132.md")).unwrap(),
        initial_132
    );
    assert_eq!(filter_events(env, "filter-dropped").len(), 1);

    std::fs::write(env.filter_file("a-gate-132"), "go").unwrap();
    assert!(support::wait_until(3, || filter_process_exited(
        env,
        "a-pid-132"
    )));
    daemon.tick().unwrap();
}

fn run_command_b_transition(
    env: &Env,
    daemon: &mut ovenspring_daemon::Daemon,
    initial_131: &str,
    initial_132: &str,
) {
    let command_b = named_filter_command(
        env,
        "filter-b.sh",
        r#"input=$(cat)
case "$input" in
  *ACME-131*) printf 'x\n' >> .ovenspring/b-131; while [ ! -f .ovenspring/b-gate-131 ]; do sleep 0.01; done; printf 'denied by command B\n'; exit 2;;
  *ACME-132*) printf 'x\n' >> .ovenspring/b-132; echo $$ > .ovenspring/b-pid-132; while [ ! -f .ovenspring/b-gate-132 ]; do sleep 0.01; done; exit 0;;
esac
exit 1
"#,
    );
    env.set_tracker_filter(Some(&command_b));
    daemon.tick().unwrap();
    assert!(support::wait_until(3, || invocation_count(env, "b-131") == 1));
    assert_eq!(
        std::fs::read_to_string(env.state().join("sources/issues/ACME-131.md")).unwrap(),
        initial_131,
        "a dropped issue is provisionally republished before command B decides it"
    );
    assert_eq!(
        std::fs::read_to_string(env.state().join("sources/issues/ACME-132.md")).unwrap(),
        initial_132
    );
    assert!(
        daemon
            .open_issues()
            .iter()
            .any(|issue| issue.key == "ACME-131"),
        "the previously dropped issue is available while command B judges it"
    );
    std::fs::write(env.filter_file("b-gate-131"), "go").unwrap();
    support::run_until(daemon, 5, |_| {}, |_| invocation_count(env, "b-132") == 1);
    assert_not_published(env, daemon, "ACME-131");
    assert_eq!(
        std::fs::read_to_string(env.state().join("sources/issues/ACME-132.md")).unwrap(),
        initial_132
    );
    std::fs::write(env.filter_file("b-gate-132"), "go").unwrap();
    assert!(support::wait_until(3, || filter_process_exited(
        env,
        "b-pid-132"
    )));
    daemon.tick().unwrap();
}

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

#[test]
fn a_result_from_a_command_changed_away_and_back_is_discarded() {
    let env = Env::new();
    env.configure("");
    let tracker = Arc::new(RecordingTracker::default());
    *tracker.issues.lock().unwrap() = vec![issue("ACME-131")];
    let mut daemon = daemon(
        &env,
        services(tracker, Arc::new(RecordingChannel::default()), Vec::new()),
    );
    daemon.tick().unwrap();
    let prior_snapshot =
        std::fs::read_to_string(env.state().join("sources/issues/ACME-131.md")).unwrap();

    let command_a = named_filter_command(
        &env,
        "filter-a.sh",
        "printf 'x\\n' >> .ovenspring/calls\nif [ ! -f .ovenspring/first ]; then touch .ovenspring/first; while [ ! -f .ovenspring/first-gate ]; do sleep 0.01; done; printf 'stale rejection\\n'; exit 2; fi\ntouch .ovenspring/second\nwhile [ ! -f .ovenspring/second-gate ]; do sleep 0.01; done\nprintf 'fresh rejection\\n'\nexit 2\n",
    );
    env.set_tracker_filter(Some(&command_a));
    daemon.tick().unwrap();
    assert!(support::wait_until(3, || env.filter_file("first").exists()));
    assert_eq!(
        std::fs::read_to_string(env.state().join("sources/issues/ACME-131.md")).unwrap(),
        prior_snapshot
    );

    env.set_tracker_filter(Some("exit 0"));
    daemon.tick().unwrap();
    env.set_tracker_filter(Some(&command_a));
    daemon.tick().unwrap();
    assert_eq!(
        std::fs::read_to_string(env.state().join("sources/issues/ACME-131.md")).unwrap(),
        prior_snapshot
    );
    assert_no_filter_events(&env, "filter-dropped");
    assert_no_filter_events(&env, "filter-failed");

    std::fs::write(env.filter_file("first-gate"), "go").unwrap();
    support::run_until(
        &mut daemon,
        5,
        |daemon| {
            assert_eq!(
                std::fs::read_to_string(env.state().join("sources/issues/ACME-131.md")).unwrap(),
                prior_snapshot
            );
            assert_no_filter_events(&env, "filter-dropped");
            assert_no_filter_events(&env, "filter-failed");
            assert!(
                daemon
                    .open_issues()
                    .iter()
                    .any(|issue| issue.key == "ACME-131")
            );
        },
        |_| env.filter_file("second").exists(),
    );
    assert_eq!(invocation_count(&env, "calls"), 2);

    std::fs::write(env.filter_file("second-gate"), "go").unwrap();
    support::run_until(
        &mut daemon,
        5,
        |_| {},
        |_| filter_events(&env, "filter-dropped").len() == 1,
    );
    assert_not_published(&env, &daemon, "ACME-131");
    assert_eq!(
        filter_events(&env, "filter-dropped")[0]["reason"],
        "fresh rejection"
    );
    assert_no_filter_events(&env, "filter-failed");
}

#[test]
fn a_filter_change_while_paused_waits_for_resume_and_reconciles_after_a_failed_read() {
    let env = Env::new();
    env.configure("");
    let command_a = named_filter_command(
        &env,
        "filter-a.sh",
        "printf 'x\\n' >> .ovenspring/a-calls\nprintf 'rejected by command A\\n'\nexit 2\n",
    );
    env.set_tracker_filter(Some(&command_a));
    let tracker = Arc::new(RecordingTracker::default());
    *tracker.issues.lock().unwrap() = vec![issue("ACME-131")];
    let mut timing = support::QUICK;
    timing.tracker_poll = std::time::Duration::from_secs(60);
    let mut daemon = daemon_timed(
        &env,
        services(
            tracker.clone(),
            Arc::new(RecordingChannel::default()),
            Vec::new(),
        ),
        timing,
    );
    set_waiting_for_issues(&daemon);
    daemon.tick().unwrap();
    support::run_until(
        &mut daemon,
        5,
        |daemon| assert_unpublished(&env, daemon, "ACME-131"),
        |_| filter_events(&env, "filter-dropped").len() == 1,
    );

    let halt = daemon.halt_file();
    std::fs::write(&halt, "paused").unwrap();
    let command_b = named_filter_command(
        &env,
        "filter-b.sh",
        "printf 'x\\n' >> .ovenspring/b-calls\nexit 0\n",
    );
    env.set_tracker_filter(Some(&command_b));
    for _ in 0..4 {
        daemon.tick().unwrap();
        assert_unpublished(&env, &daemon, "ACME-131");
        assert_eq!(invocation_count(&env, "b-calls"), 0);
        assert_eq!(
            env.events()
                .iter()
                .filter(|event| event["event"] == "filed" && event["step"] == "sprint-plan")
                .count(),
            0
        );
    }

    env.set_tracker_filter(None);
    tracker
        .unreachable
        .store(true, std::sync::atomic::Ordering::SeqCst);
    std::fs::remove_file(&halt).unwrap();
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
    assert_eq!(invocation_count(&env, "b-calls"), 0);
    assert_eq!(filter_events(&env, "filter-dropped").len(), 1);
    assert!(env.events().iter().any(|event| {
        event["event"] == "service-unreachable" && event["operation"] == "reading the tracker"
    }));
}

#[test]
fn a_dropped_candidate_is_removed_before_any_agent_can_read_it() {
    let env = Env::new();
    let script = "printf '%s\\n' $$ > .ovenspring/filter.pid\nwhile [ ! -f .ovenspring/gate ]; do sleep 0.01; done\nprintf 'rejected by owner policy\\n'\nexit 2\n";
    let (mut daemon, tracker, channel) =
        filtered_daemon(&env, script, vec![issue("ACME-131")], Vec::new());
    set_waiting_for_issues(&daemon);

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
    let script =
        "while [ ! -f .ovenspring/gate ]; do sleep 0.01; done\ncat .ovenspring/reason\nexit 2\n";
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

#[test]
fn unchanged_admitted_and_dropped_inputs_run_once_and_changed_content_runs_again() {
    let env = Env::new();
    let script = "input=$(cat)\ncase \"$input\" in\n  *ACME-131*)\n    printf 'x\\n' >> .ovenspring/calls-131\n    case \"$input\" in\n      *updated*) while [ ! -f .ovenspring/rejudge-gate ]; do sleep 0.01; done; printf 'updated candidate denied\\n'; exit 2;;\n      *) exit 0;;\n    esac;;\n  *ACME-132*) printf 'x\\n' >> .ovenspring/calls-132; printf 'second candidate denied\\n'; exit 2;;\nesac\nexit 1\n";
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
        |daemon| {
            if filter_events(&env, "filter-dropped").len() < 2 {
                assert_eq!(
                    std::fs::read_to_string(env.state().join("sources/issues/ACME-131.md"))
                        .unwrap(),
                    old_snapshot
                );
                assert!(
                    daemon
                        .open_issues()
                        .iter()
                        .any(|issue| issue.key == "ACME-131")
                );
            }
        },
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

#[test]
fn a_new_steady_state_candidate_stays_unpublished_until_its_first_verdict() {
    let env = Env::new();
    let script = "while [ ! -f .ovenspring/gate ]; do sleep 0.01; done\nexit 0\n";
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

#[test]
fn a_result_for_changed_content_is_discarded_and_the_current_content_is_judged() {
    let env = Env::new();
    let script = "input=$(cat)\nprintf '%s\\n' \"$input\" >> .ovenspring/calls\nif [ ! -f .ovenspring/first ]; then printf '%s' \"$input\" > .ovenspring/first-input.json; touch .ovenspring/first; while [ ! -f .ovenspring/gate ]; do sleep 0.01; done; printf 'stale rejection\\n'; exit 2; fi\nprintf '%s' \"$input\" > .ovenspring/replacement-input.json\nexit 0\n";
    let (mut daemon, tracker, _) =
        filtered_daemon(&env, script, vec![issue("ACME-131")], Vec::new());

    daemon.tick().unwrap();
    assert!(support::wait_until(3, || env.filter_file("first").exists()));
    tracker.issues.lock().unwrap()[0].body = "current edited requirement".into();
    daemon.tick().unwrap();
    assert_unpublished(&env, &daemon, "ACME-131");
    assert_no_filter_events(&env, "filter-dropped");
    std::fs::write(env.filter_file("gate"), "go").unwrap();

    support::run_until(
        &mut daemon,
        5,
        |daemon| {
            assert_no_filter_events(&env, "filter-dropped");
            if env.state().join("sources/issues/ACME-131.md").exists() {
                let snapshot =
                    std::fs::read_to_string(env.state().join("sources/issues/ACME-131.md"))
                        .unwrap();
                assert!(snapshot.contains("current edited requirement"));
            }
            if !daemon.open_issues().is_empty() {
                assert_eq!(daemon.open_issues()[0].body, "current edited requirement");
            }
        },
        |daemon| {
            daemon
                .open_issues()
                .iter()
                .any(|issue| issue.key == "ACME-131")
        },
    );
    assert_eq!(
        std::fs::read_to_string(env.filter_file("calls"))
            .unwrap()
            .lines()
            .count(),
        2
    );
    let replacement: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(env.filter_file("replacement-input.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(replacement["body"], "current edited requirement");
}

#[test]
fn a_closed_and_reopened_candidate_discards_its_old_run_and_is_judged_again() {
    let env = Env::new();
    let script = "printf 'x\\n' >> .ovenspring/calls\nif [ ! -f .ovenspring/first ]; then touch .ovenspring/first; while [ ! -f .ovenspring/gate ]; do sleep 0.01; done; printf 'pre-close rejection\\n'; exit 2; fi\nexit 0\n";
    let candidate = issue("ACME-131");
    let (mut daemon, tracker, _) =
        filtered_daemon(&env, script, vec![candidate.clone()], Vec::new());

    daemon.tick().unwrap();
    assert!(support::wait_until(3, || env.filter_file("first").exists()));
    tracker.issues.lock().unwrap().clear();
    daemon.tick().unwrap();
    assert_unpublished(&env, &daemon, "ACME-131");
    tracker.issues.lock().unwrap().push(candidate);
    daemon.tick().unwrap();
    assert_unpublished(&env, &daemon, "ACME-131");
    assert_no_filter_events(&env, "filter-dropped");
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
    assert_eq!(
        std::fs::read_to_string(env.filter_file("calls"))
            .unwrap()
            .lines()
            .count(),
        2
    );
    assert_no_filter_events(&env, "filter-dropped");
}

#[test]
fn a_restart_rechecks_a_dropped_candidate_before_publishing_it() {
    let env = Env::new();
    let (tracker, channel, admitted_snapshot) = prepare_restart_candidates(&env);

    let restart_script = r#"input=$(cat)
case "$input" in
  *ACME-131*)
    printf 'x\n' >> .ovenspring/restart-131
    while [ ! -f .ovenspring/restart-gate-131 ]; do sleep 0.01; done
    printf 'still rejected after restart\n'
    exit 2;;
  *ACME-132*)
    printf 'x\n' >> .ovenspring/restart-132
    while [ ! -f .ovenspring/restart-gate-132 ]; do sleep 0.01; done
    printf 'rejected after changed command\n'
    exit 2;;
esac
exit 1
"#;
    std::fs::write(env.filter_file("filter-restart.sh"), restart_script).unwrap();
    env.set_tracker_filter(Some("exec sh .ovenspring/filter-restart.sh"));

    let mut restarted = daemon(&env, services(tracker, channel, Vec::new()));
    restarted.tick().unwrap();
    assert!(support::wait_until(3, || invocation_count(
        &env,
        "restart-131"
    ) == 1));
    assert_not_published(&env, &restarted, "ACME-131");
    assert_eq!(
        std::fs::read_to_string(env.state().join("sources/issues/ACME-132.md")).unwrap(),
        admitted_snapshot
    );
    for _ in 0..4 {
        restarted.tick().unwrap();
        assert_not_published(&env, &restarted, "ACME-131");
        assert_eq!(
            std::fs::read_to_string(env.state().join("sources/issues/ACME-132.md")).unwrap(),
            admitted_snapshot
        );
    }
    std::fs::write(env.filter_file("restart-gate-131"), "go").unwrap();
    support::run_until(
        &mut restarted,
        5,
        |daemon| {
            assert_not_published(&env, daemon, "ACME-131");
            assert_eq!(
                std::fs::read_to_string(env.state().join("sources/issues/ACME-132.md")).unwrap(),
                admitted_snapshot
            );
        },
        |_| {
            invocation_count(&env, "restart-132") == 1
                && filter_events(&env, "filter-dropped").len() == 2
        },
    );
    std::fs::write(env.filter_file("restart-gate-132"), "go").unwrap();
    support::run_until(
        &mut restarted,
        5,
        |daemon| {
            assert_not_published(&env, daemon, "ACME-131");
            if filter_events(&env, "filter-dropped").len() < 3 {
                assert_eq!(
                    std::fs::read_to_string(env.state().join("sources/issues/ACME-132.md"))
                        .unwrap(),
                    admitted_snapshot
                );
            }
        },
        |daemon| {
            filter_events(&env, "filter-dropped").len() == 3
                && !daemon
                    .open_issues()
                    .iter()
                    .any(|issue| issue.key == "ACME-132")
        },
    );
    assert_eq!(
        (
            invocation_count(&env, "restart-131"),
            invocation_count(&env, "restart-132")
        ),
        (1, 1)
    );
    assert!(!env.state().join("sources/issues/ACME-131.md").exists());
    assert!(!env.state().join("sources/issues/ACME-132.md").exists());
    assert_eq!(
        filter_events(&env, "filter-dropped")
            .iter()
            .map(|event| event["issue"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["ACME-131", "ACME-131", "ACME-132"]
    );
}

fn prepare_restart_candidates(env: &Env) -> (Arc<RecordingTracker>, Arc<RecordingChannel>, String) {
    let first_script = r#"input=$(cat)
case "$input" in
  *ACME-131*) printf 'x\n' >> .ovenspring/initial-131; printf 'still rejected\n'; exit 2;;
  *ACME-132*) printf 'x\n' >> .ovenspring/initial-132; exit 0;;
esac
exit 1
"#;
    let (mut first, tracker, channel) = filtered_daemon(
        env,
        first_script,
        vec![issue("ACME-131"), issue("ACME-132")],
        Vec::new(),
    );

    first.tick().unwrap();
    support::run_until(
        &mut first,
        5,
        |daemon| assert_not_published(env, daemon, "ACME-131"),
        |daemon| {
            invocation_count(env, "initial-131") == 1
                && invocation_count(env, "initial-132") == 1
                && filter_events(env, "filter-dropped").len() == 1
                && daemon
                    .open_issues()
                    .iter()
                    .any(|issue| issue.key == "ACME-132")
        },
    );
    assert_not_published(env, &first, "ACME-131");
    let admitted_snapshot =
        std::fs::read_to_string(env.state().join("sources/issues/ACME-132.md")).unwrap();
    (tracker, channel, admitted_snapshot)
}

fn process_exists(pid: &str) -> bool {
    std::process::Command::new("kill")
        .args(["-0", "--", pid])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[test]
fn normal_daemon_stop_terminates_the_filter_process_group() {
    const STOP_OBSERVATION_SECS: u64 = 3;
    const FILTER_TIMEOUT_SECS: u64 = 10;

    let env = Env::new();
    let script = "echo $$ > .ovenspring/filter-pid\nsleep 30 &\nchild=$!\necho $child > .ovenspring/child-pid\nwhile [ ! -f .ovenspring/gate ]; do sleep 0.01; done\nwait $child\n";
    let command = env.write_filter_script(script);
    env.configure_filter(&command);
    let tracker = Arc::new(RecordingTracker::default());
    *tracker.issues.lock().unwrap() = vec![issue("ACME-131")];
    let channel = Arc::new(RecordingChannel::default());
    let timing = ovenspring_daemon::Timing {
        filter_timeout: std::time::Duration::from_secs(FILTER_TIMEOUT_SECS),
        ..support::QUICK
    };
    let mut daemon = support::daemon_timed(&env, services(tracker, channel, Vec::new()), timing);

    daemon.tick().unwrap();
    assert!(support::wait_until(3, || {
        env.filter_file("filter-pid").exists() && env.filter_file("child-pid").exists()
    }));
    let filter_pid = std::fs::read_to_string(env.filter_file("filter-pid")).unwrap();
    let child_pid = std::fs::read_to_string(env.filter_file("child-pid")).unwrap();
    assert!(process_exists(filter_pid.trim()));
    assert!(process_exists(child_pid.trim()));

    let stop_started = std::time::Instant::now();
    daemon.stop_launches();

    let stopped = support::wait_until(STOP_OBSERVATION_SECS, || {
        !process_exists(filter_pid.trim()) && !process_exists(child_pid.trim())
    });
    let stop_elapsed = stop_started.elapsed();
    if !stopped {
        let process_group = format!("-{}", filter_pid.trim());
        let _ = std::process::Command::new("kill")
            .args(["-KILL", "--", process_group.as_str()])
            .status();
    }
    assert!(
        stopped && stop_elapsed < std::time::Duration::from_secs(STOP_OBSERVATION_SECS),
        "the command process group must stop within {STOP_OBSERVATION_SECS} seconds, before its {FILTER_TIMEOUT_SECS}-second automatic timeout (elapsed: {stop_elapsed:?})"
    );
}
