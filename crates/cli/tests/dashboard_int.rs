//! @lane: integration
//! Area: the dashboard server's HTTP boundary over the real state directory.
//!
//! Setup's connection checks are a stand-in here; the checks against the real services are their
//! contract tests'.

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
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use ovenspring_core::config::{Cli, Config, MergeBy, Role, TrackerKind};
use ovenspring_core::flow::Stage;
use ovenspring_core::output::SprintPlanOutput;
use ovenspring_core::sprint::{SprintStage, SprintState};
use ovenspring_daemon::issues::IssueRecord;
use ovenspring_daemon::owner::{DecisionRecord, DecisionStatus};
use ovenspring_daemon::records;
use ovenspring_integrations::channel::{DecisionKind, MessageRef};
use ovenspring_server::Context;
use ovenspring_server::setup::{
    GitHubAccount, GitHubConnection, SetupChecks, SlackSetup, TrackerSetup,
};
use ovenspring_store::Store;
use serde_json::{Value, json};
use support::dashboard::Dashboard;
use support::http::{request, request_to};
use support::{Env, git, wait_until};

/// Setup's connection checks, answering as the test sets: a section named in `refuse` fails.
struct Checks {
    refuse: Option<&'static str>,
}

impl Checks {
    fn answer(&self, section: &str, found: &str) -> Result<String, String> {
        if self.refuse == Some(section) {
            Err(format!("{section} refused the connection."))
        } else {
            Ok(found.to_owned())
        }
    }
}

impl SetupChecks for Checks {
    fn tracker(&self, _: &Path, _: &TrackerSetup, _: Option<&str>) -> Result<String, String> {
        self.answer("tracker", "Team ACME, 14 open issues.")
    }
    fn slack(&self, _: &SlackSetup) -> Result<String, String> {
        self.answer("slack", "Connected to channel C0123456789, as Kai Moreno.")
    }
    fn github(&self, repo: &Path, _: Option<&str>, _: &str) -> Result<GitHubConnection, String> {
        self.answer("github", "Signed in as kai.")
            .map(|_| GitHubConnection {
                account: GitHubAccount {
                    login: "kai".into(),
                    from_gh: false,
                },
                remote: git(repo, &["remote", "get-url", "origin"]),
            })
    }
}

fn goal_events(store: &Store) -> usize {
    std::fs::read_to_string(store.root().join("events.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter(|l| l.contains("\"goal-set\""))
        .count()
}

// AC: "The dashboard answers only on 127.0.0.1 and only requests addressed to this machine
//     (AC-055)" —
//     docs/design/owner-interface-design.md § Dashboard server: another site's page cannot read or
//     change anything through the owner's browser, including one that reaches the port under its
//     own host name (DNS rebinding); a write takes only a JSON body.
// Behavior: requests to the running server -> it checks each one's `Host` and body -> data is
//     served and changed only for requests addressed to the dashboard itself.
// Cases (reported distinctly): (a) the listener's address; (b) `GET /api/v1/overview` naming
//     another host; (c) a client route `GET /issues/ACME-123`; (d) `GET /api/v1/overview` at
//     `localhost` on another port, as an SSH port forward sends; (e) a cancel posted as a form, as
//     another site's page can send.
// @lane: integration
// @dependency: dashboard server (host check, JSON bodies, embedded assets), store
#[test]
fn only_requests_addressed_to_the_dashboard_reach_it() {
    let env = Env::new();
    let dash = Dashboard::with_checks(&env, Checks { refuse: None });
    records::set_goal(&dash.store, "Shop customers manage their own orders").unwrap();
    write_overview_records(&dash.store);
    let addr = dash.server.addr;

    assert_eq!(addr.ip(), std::net::Ipv4Addr::LOCALHOST);

    let rebound = request_to(
        addr,
        &format!("attacker.example:{}", addr.port()),
        "GET /api/v1/overview",
        None,
    );
    assert_eq!(rebound.status, 403);
    assert!(!rebound.body.contains("Shop customers"), "{}", rebound.body);

    let deep = dash.get("/issues/ACME-123");
    assert_eq!(deep.status, 200);
    assert_eq!(
        deep.header("content-type"),
        Some("text/html; charset=utf-8")
    );

    let overview = request_to(addr, "localhost:8080", "GET /api/v1/overview", None);
    assert_eq!(overview.status, 200);
    assert_eq!(
        overview.json()["goal"],
        "Shop customers manage their own orders"
    );

    let form = form_post(addr, "/api/v1/issues/ACME-156/cancel");
    assert_eq!(form, 415);
    assert!(
        !dash.store.isolated().join("cancels/ACME-156.json").exists(),
        "a form leaves no cancel"
    );
}

/// Posts to `path` as an HTML form would, and returns the status. No body: one the server never
/// reads makes the connection reset, which can lose the answer.
fn form_post(addr: std::net::SocketAddr, path: &str) -> u16 {
    use std::io::{Read as _, Write as _};
    let mut stream = std::net::TcpStream::connect(addr).unwrap();
    write!(
        stream,
        "POST {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: 0\r\n\r\n"
    )
    .unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).unwrap();
    raw.split(' ').nth(1).unwrap().parse().unwrap()
}

// AC: "The goal band shall show the current goal on every screen, and when the owner saves a new
//     goal, the system shall store it and record the change (AC-056)" —
//     docs/design/owner-interface-design.md § Acceptance Criteria > Dashboard. § Data Contracts >
//     Goal: "{text} trimmed, non-empty ... goal.md replaced; events.jsonl gains {event: goal-set,
//     at, old, new}".
// Behavior: `PUT /api/v1/goal` with "  Make checkout fast on mobile  " -> the server validates and
//     stores it -> `GET /api/v1/overview` serves the new goal.
// Cases (reported distinctly): valid text; whitespace-only text.
// @lane: integration
// @dependency: dashboard server, goal store
#[test]
fn goal_saved_through_the_api_is_stored_logged_and_served_back() {
    let env = Env::new();
    let dash = Dashboard::with_checks(&env, Checks { refuse: None });

    let saved = dash.put(
        "/api/v1/goal",
        &json!({"text": "  Make checkout fast on mobile  "}),
    );
    assert_eq!(saved.status, 200, "{}", saved.body);
    assert_eq!(saved.json()["goal"], "Make checkout fast on mobile");
    assert_eq!(
        std::fs::read_to_string(dash.store.root().join("goal.md")).unwrap(),
        "Make checkout fast on mobile\n"
    );
    assert_eq!(goal_events(&dash.store), 1);
    assert_eq!(
        dash.get("/api/v1/overview").json()["goal"],
        "Make checkout fast on mobile"
    );

    let refused = dash.put("/api/v1/goal", &json!({"text": "   "}));
    assert_eq!(refused.status, 400, "{}", refused.body);
    assert_eq!(refused.json()["errors"]["text"], "a goal cannot be empty");
    assert_eq!(
        records::goal(&dash.store).unwrap(),
        "Make checkout fast on mobile"
    );
    assert_eq!(goal_events(&dash.store), 1, "a refused goal logs nothing");
}

fn decision(id: &str, issue: &str, title: &str, status: DecisionStatus) -> DecisionRecord {
    DecisionRecord {
        id: id.into(),
        kind: DecisionKind::Spec,
        issue: Some(issue.into()),
        sprint: None,
        sha: None,
        categories: vec![],
        title: title.into(),
        text: title.into(),
        status,
        message: (status == DecisionStatus::Pending).then(|| MessageRef {
            channel: "C0123456789".into(),
            ts: format!("{id}.0"),
            permalink: Some(format!("https://acme.slack.test/p/{id}")),
        }),
        answer: None,
        procedure: None,
        asked_by: None,
        proposal: None,
    }
}

/// A paused factory with three decisions waiting and a running sprint of eight issues, two merged.
fn write_overview_records(store: &Store) {
    std::fs::write(
        store.root().join("HALT"),
        "the agent on ACME-142 (implement) stopped with an error: You've hit your usage limit\n",
    )
    .unwrap();
    for d in [
        decision(
            "d1",
            "ACME-153",
            "Queue or refuse a second export?",
            DecisionStatus::Pending,
        ),
        decision(
            "d2",
            "ACME-149",
            "Merge the new export_jobs table?",
            DecisionStatus::Pending,
        ),
        decision(
            "d3",
            "ACME-160",
            "Which date format?",
            DecisionStatus::Unsent,
        ),
    ] {
        store
            .write_isolated(&format!("decisions/{}.json", d.id), &d)
            .unwrap();
    }
    store
        .write_record(
            "sprint.json",
            &SprintState {
                id: "20261121-0905".into(),
                stage: SprintStage::Running,
                plan: Some(SprintPlanOutput {
                    purpose: "Customers export their order history".into(),
                    key_results: vec![],
                    issues: vec![],
                    message: "m".into(),
                }),
                review: None,
                agreements_pr: None,
                agreements_url: None,
                agreements_need_approval: false,
                agreements_pushed_at: None,
                started_at: Some("2026-11-21T09:05:00Z".into()),
                ended_at: None,
                failed: None,
                failure: None,
                resume: None,
            },
        )
        .unwrap();
    for (key, stage) in [
        ("ACME-142", Stage::Merged),
        ("ACME-143", Stage::Merged),
        ("ACME-149", Stage::Gate),
        (
            "ACME-153",
            Stage::Asking {
                step: Step::Requirements,
            },
        ),
        ("ACME-156", Stage::running(Step::FixCi)),
        ("ACME-160", Stage::running(Step::Design)),
        ("ACME-161", Stage::Queued),
        ("ACME-162", Stage::ReadyForOwner),
    ] {
        let depends_on = if key == "ACME-161" {
            vec!["ACME-143".to_owned(), "ACME-160".to_owned()]
        } else {
            vec![]
        };
        let mut r = IssueRecord::new(key, &format!("Title of {key}"), depends_on);
        r.flow.stage = stage;
        r.sprint = Some("20261121-0905".into());
        if key == "ACME-162" {
            r.pr_url = Some("https://github.test/acme/storefront/pull/7".into());
        }
        store
            .write_record(&format!("issues/{key}.json"), &r)
            .unwrap();
    }
}

// AC: "The overview shall return the factory's one state in the UI Spec's FactoryStatus order, the
//     sprint with its done count, every pending decision with its kind, and each issue of the
//     sprint with its section ... (AC-043)" — docs/design/owner-interface-design.md § Acceptance
//     Criteria > Dashboard. "A `merge.by = owner` issue ready to merge is a decision of kind
//     `merge-on-github` with the pull request as its link" — § Dashboard server.
// Behavior: the state directory holds the records of the case -> `GET /api/v1/overview` -> the
//     response carries the state, the decisions, the sprint's done count and each issue's place.
// Cases (reported distinctly): (a) `HALT` with a usage-limit cause; (b) two `pending` decisions with
//     permalinks, one `unsent`, and an issue ready for the owner to merge; (c) a running sprint
//     with 2 of 8 issues done, one running while paused, one waiting on a dependency.
// @lane: integration
// @dependency: dashboard server, store, overview
#[test]
fn overview_reports_pause_cause_pending_decisions_and_sprint_progress_from_the_store() {
    let env = Env::new();
    let dash = Dashboard::with_checks(&env, Checks { refuse: None });
    write_overview_records(&dash.store);

    let o = dash.get("/api/v1/overview").json();

    assert_eq!(o["status"]["state"], "paused");
    assert_eq!(
        o["status"]["reason"],
        "the agent on ACME-142 (implement) stopped with an error: You've hit your usage limit"
    );
    assert_eq!(
        o["decisions"],
        json!([
            {"issue": "ACME-149", "ask": "answer-question", "title": "Title of ACME-149",
             "link": "https://acme.slack.test/p/d2", "delivered": true},
            {"issue": "ACME-153", "ask": "answer-question", "title": "Title of ACME-153",
             "link": "https://acme.slack.test/p/d1", "delivered": true},
            {"issue": "ACME-160", "ask": "answer-question", "title": "Title of ACME-160",
             "link": null, "delivered": false},
            {"issue": "ACME-162", "ask": "merge-on-github", "title": "Title of ACME-162",
             "link": "https://github.test/acme/storefront/pull/7", "delivered": true},
        ])
    );
    assert_eq!(
        o["sprint"],
        json!({"id": "20261121-0905", "purpose": "Customers export their order history",
               "startedAt": "2026-11-21T09:05:00Z", "done": 2, "total": 8})
    );
    let place = |key: &str| {
        o["issues"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["key"] == key)
            .unwrap()["now"]
            .clone()
    };
    assert_eq!(place("ACME-142")["section"], "done");
    assert_eq!(place("ACME-142")["ended"], "merged");
    for key in ["ACME-149", "ACME-153", "ACME-160", "ACME-162"] {
        assert_eq!(place(key)["section"], "you", "{key}");
    }
    let running = place("ACME-156");
    assert_eq!(
        (
            &running["section"],
            &running["phase"],
            &running["activity"],
            &running["onHold"]
        ),
        (
            &json!("in-progress"),
            &json!("merge"),
            &json!("fixing-ci"),
            &json!(true)
        )
    );
    assert_eq!(
        place("ACME-161")["wait"],
        json!({"reason": "after", "keys": ["ACME-160"]})
    );
}

// AC: "When the daemon starts with a `services-unanswered.json` from the previous version, Now
//     shall keep that service's 'since' and reason" — docs/design/events-and-error-notices-design.md
//     § Acceptance Criteria > Error notices; `status.unreachable` comes from the open errors in
//     `events.jsonl` (§ Selected Design > Error state). The upgrade itself is errors_int's.
// Behavior: `events.jsonl` holds a `github-unreachable` start and a repeat with a later reason, and
//     a `tracker-unreachable` start and its clear -> `GET /api/v1/overview`.
// Observable result: `status.unreachable` is GitHub since its start, with the latest reason; the
//     cleared tracker is not named.
// @lane: integration
// @dependency: dashboard server, store, overview
#[test]
fn overview_names_the_service_out_of_reach_from_the_errors_in_the_event_log() {
    let env = Env::new();
    let dash = Dashboard::with_checks(&env, Checks { refuse: None });
    for line in [
        json!({"event": "service-unreachable", "operation": "reading the tracker", "why": "no-connection",
               "at": "2026-11-27T04:30:00Z",
               "failure": {"key": "tracker-unreachable", "since": "2026-11-27T04:30:00Z", "cause": "no answer",
                           "start": true, "notice": "waiting"}}),
        json!({"event": "service-unreachable", "operation": "reading its pull request", "why": "server-error",
               "issue": "ACME-142", "at": "2026-11-27T04:41:00Z",
               "failure": {"key": "github-unreachable", "since": "2026-11-27T04:41:00Z", "cause": "HTTP 502",
                           "via": ["api"], "start": true, "notice": "waiting"}}),
        json!({"event": "error-cleared", "count": 1, "at": "2026-11-27T04:42:00Z",
               "failure": {"key": "tracker-unreachable", "since": "2026-11-27T04:30:00Z", "notice": "waiting"}}),
        json!({"event": "service-unreachable", "operation": "pushing its branch", "why": "rate-limited",
               "issue": "ACME-142", "at": "2026-11-27T04:43:00Z",
               "failure": {"key": "github-unreachable", "since": "2026-11-27T04:41:00Z", "cause": "HTTP 429",
                           "via": ["api"]}}),
    ] {
        dash.store.append_event(&line).unwrap();
    }

    let o = dash.get("/api/v1/overview").json();

    assert_eq!(o["status"]["state"], "unreachable");
    assert_eq!(
        o["status"]["unreachable"],
        json!({"service": "github", "since": "2026-11-27T04:41:00Z", "why": "rate-limited"})
    );
}

// AC: UI Spec S-04 Activity: runs newest first, each at the time it ended.
// Behavior: a code review starts before a security review and ends after it, and a third run is
//     still running -> the list reads running, then the code review, then the security review.
// @lane: integration
// @dependency: dashboard API, launch records
#[test]
fn activity_lists_runs_by_when_they_ended_with_a_running_one_first() {
    let env = Env::new();
    let dash = Dashboard::with_checks(&env, Checks { refuse: None });
    let start = |at: &str, item: &str, step: &str| {
        json!({"at": at, "role": "reviewer", "item": item, "step": step, "issue": "ACME-1",
               "sprint": "S1", "cli": "claude", "model": null, "effort": null,
               "permissions": "auto", "attempt": 1, "pid": 999_999, "log": "x.log"})
    };
    let end = |at: &str, item: &str| {
        json!({"at": at, "item": item, "ended": true, "code": 0, "attempt": 1, "failure": null,
               "error_kind": null, "session": null,
               "usage": {"input": 0, "output": 0, "cache_read": 0, "cache_write": 0},
               "cost_usd": null})
    };
    for line in [
        start("2026-11-24T12:18:00Z", "code", "code-review"),
        start("2026-11-24T12:18:30Z", "security", "security-review"),
        end("2026-11-24T12:19:00Z", "security"),
        start("2026-11-24T12:20:00Z", "fix", "review-response"),
        end("2026-11-24T12:21:00Z", "code"),
    ] {
        dash.store.append_launch(&line).unwrap();
    }

    let rows = dash.get("/api/v1/launches?sprint=S1").json();

    let steps: Vec<&str> = rows
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["step"].as_str().unwrap())
        .collect();
    assert_eq!(steps, ["review-response", "code-review", "security-review"]);
}

// AC: UI Spec GlanceRow, Waiting for you: each row names what the owner answers.
// Behavior: a sprint proposal waits on the owner -> its row is titled by the sprint's purpose, not
//     by the kind of decision.
// @lane: integration
// @dependency: dashboard API, overview, store
#[test]
fn a_sprint_decision_waiting_on_the_owner_is_titled_by_the_sprints_purpose() {
    let env = Env::new();
    let dash = Dashboard::with_checks(&env, Checks { refuse: None });
    dash.store
        .write_record(
            "sprint.json",
            &SprintState {
                id: "20261124-2113".into(),
                stage: SprintStage::AwaitingApproval,
                plan: Some(SprintPlanOutput {
                    purpose: "Each card states small values truthfully".into(),
                    key_results: vec![],
                    issues: vec![],
                    message: "m".into(),
                }),
                review: None,
                agreements_pr: None,
                agreements_url: None,
                agreements_need_approval: false,
                agreements_pushed_at: None,
                started_at: None,
                ended_at: None,
                failed: None,
                failure: None,
                resume: None,
            },
        )
        .unwrap();
    let proposal = DecisionRecord {
        kind: DecisionKind::Sprint,
        issue: None,
        sprint: Some("20261124-2113".into()),
        title: "Sprint proposal".into(),
        ..decision("sprint-1", "unused", "unused", DecisionStatus::Pending)
    };
    dash.store
        .write_isolated("decisions/sprint-1.json", &proposal)
        .unwrap();

    let o = dash.get("/api/v1/overview").json();

    assert_eq!(
        o["decisions"][0]["title"],
        "Each card states small values truthfully"
    );
}

/// A sprint whose working agreements' pull request is open and waits to merge, with GitHub
/// requiring an approving review on it when `need_approval`.
fn applying_agreements(store: &Store, need_approval: bool) {
    store
        .write_record(
            "sprint.json",
            &SprintState {
                id: "20261124-2113".into(),
                stage: SprintStage::ApplyingAgreements,
                plan: None,
                review: None,
                agreements_pr: Some(9),
                agreements_url: Some("https://github.com/acme/storefront/pull/9".into()),
                agreements_need_approval: need_approval,
                agreements_pushed_at: None,
                started_at: Some("2026-11-24T12:13:00Z".into()),
                ended_at: None,
                failed: None,
                failure: None,
                resume: None,
            },
        )
        .unwrap();
}

// AC: UI Spec FactoryStatus, Merging the working agreements: Now says what the sprint waits for.
// Behavior: the factory merges and the agreements' pull request is open -> the overview links it,
//     and nothing waits on the owner.
// @lane: integration
// @dependency: dashboard API, overview, configuration, store
#[test]
fn agreements_the_factory_merges_are_linked_while_they_wait_to_merge() {
    let env = Env::new();
    env.configure("");
    let dash = Dashboard::with_checks(&env, Checks { refuse: None });
    applying_agreements(&dash.store, false);

    let o = dash.get("/api/v1/overview").json();

    assert_eq!(
        (o["agreements"].clone(), o["decisions"].clone()),
        (
            json!("https://github.com/acme/storefront/pull/9"),
            json!([])
        )
    );
}

// AC: UI Spec GlanceRow, Waiting for you: the agreements' pull request waits on the owner when
//     GitHub requires their approving review, even though the factory merges it.
// Behavior: `merge.by = factory`, the pull request is open and GitHub requires an approving review
//     -> it waits for the owner as Approve on GitHub, linked to the pull request.
// @lane: integration
// @dependency: dashboard API, overview, configuration, store
#[test]
fn agreements_waiting_for_an_approving_review_wait_for_the_owner() {
    let env = Env::new();
    env.configure("");
    let dash = Dashboard::with_checks(&env, Checks { refuse: None });
    applying_agreements(&dash.store, true);

    let o = dash.get("/api/v1/overview").json();

    assert_eq!(
        (
            o["decisions"][0]["ask"].clone(),
            o["decisions"][0]["link"].clone()
        ),
        (
            json!("approve-on-github"),
            json!("https://github.com/acme/storefront/pull/9")
        )
    );
}

// AC: UI Spec GlanceRow, Waiting for you: a pull request the owner merges is theirs to act on.
// Behavior: `merge.by = owner` and the agreements' pull request is open -> it waits for the owner,
//     linked to the pull request.
// @lane: integration
// @dependency: dashboard API, overview, configuration, store
#[test]
fn agreements_the_owner_merges_wait_for_the_owner() {
    let env = Env::new();
    env.configure("[merge]\nby = \"owner\"\n");
    let dash = Dashboard::with_checks(&env, Checks { refuse: None });
    applying_agreements(&dash.store, false);

    let o = dash.get("/api/v1/overview").json();

    assert_eq!(
        (
            o["decisions"][0]["ask"].clone(),
            o["decisions"][0]["link"].clone()
        ),
        (
            json!("merge-on-github"),
            json!("https://github.com/acme/storefront/pull/9")
        )
    );
}

// AC: PRD AC-067: the owner cancels an issue that has not ended from its page on the dashboard.
// Behavior: a cancel for an issue in progress is accepted and left, out of the agents' reach, for
//     the daemon; one for a merged issue or for no issue of the factory is refused.
// @lane: integration
// @dependency: dashboard API, store
#[test]
fn a_cancel_is_left_for_the_daemon_only_for_an_issue_that_has_not_ended() {
    let env = Env::new();
    let dash = Dashboard::with_checks(&env, Checks { refuse: None });
    write_overview_records(&dash.store);

    let accepted = dash.post(
        "/api/v1/issues/ACME-156/cancel",
        &json!({"words": "Billing takes this over."}),
    );
    let merged = dash.post("/api/v1/issues/ACME-142/cancel", &json!({"words": ""}));
    let unknown = dash.post("/api/v1/issues/ACME-999/cancel", &json!({"words": ""}));

    assert_eq!(accepted.status, 202, "{}", accepted.body);
    let left: Value = serde_json::from_str(
        &std::fs::read_to_string(dash.store.isolated().join("cancels/ACME-156.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(left["words"], "Billing takes this over.");
    assert!(!dash.store.root().join("cancels").exists());
    for refused in [merged, unknown] {
        assert_eq!(refused.status, 409, "{}", refused.body);
    }
}

// AC: UI Spec Now: "No sprint running" while the next sprint is proposed, waits on the owner, or
//     waits for an issue (prototype situations `approve-sprint` and `noissues`).
// Behavior: a proposal with a purpose that has not started running -> the overview has no sprint
//     line and keeps the last ended sprint's time.
// @lane: integration
// @dependency: dashboard API, overview, store
#[test]
fn a_sprint_not_yet_running_has_no_sprint_line_whatever_its_proposal_says() {
    let env = Env::new();
    let dash = Dashboard::with_checks(&env, Checks { refuse: None });
    for stage in [SprintStage::AwaitingApproval, SprintStage::WaitingForIssues] {
        dash.store
            .write_record(
                "sprint.json",
                &SprintState {
                    id: "20261124-1800".into(),
                    stage: stage.clone(),
                    plan: Some(SprintPlanOutput {
                        purpose: "No sprint: no open issue moves the goal".into(),
                        key_results: vec![],
                        issues: vec![],
                        message: "m".into(),
                    }),
                    review: None,
                    agreements_pr: None,
                    agreements_url: None,
                    agreements_need_approval: false,
                    agreements_pushed_at: None,
                    started_at: None,
                    ended_at: None,
                    failed: None,
                    failure: None,
                    resume: None,
                },
            )
            .unwrap();

        let o = dash.get("/api/v1/overview").json();

        assert_eq!(o["sprint"], Value::Null, "{stage:?}");
    }
}

fn setup_request() -> Value {
    json!({
        "cli": "codex",
        "tracker": {"kind": "linear", "team": "ACME", "apiKey": "lin_api_fixture0001"},
        "slack": {"botToken": "xoxb-fixture-0001", "appToken": "xapp-fixture-0001", "channel": "C0123456789", "owner": "U0123456789"},
        "repo": {"baseBranch": "main", "mergeBy": "owner"},
        "github": {"token": null}
    })
}

/// A product that starts: it answers `/ready` on `$PORT` once up.
fn serving() -> Value {
    json!({
        "runs": "commands",
        "setup": "mkdir -p \"$OVENSPRING_DATA_DIR/www\" && touch \"$OVENSPRING_DATA_DIR/www/ready\"",
        "start": support::serve("\"$OVENSPRING_DATA_DIR/www\""),
        "ready": "http://127.0.0.1:$PORT/ready"
    })
}

fn changed(repo: &Path) -> String {
    git(repo, &["status", "--porcelain", "--untracked-files=all"])
}

fn assert_connected(env: &Env, dash: &Dashboard) {
    let config = Config::load(&env.repo).unwrap();
    assert_eq!(
        (
            config.tracker.kind,
            config.tracker.team.as_deref(),
            config.merge.by,
            config.slack.owners.clone()
        ),
        (
            TrackerKind::Linear,
            Some("ACME"),
            MergeBy::Owner,
            vec!["U0123456789".to_owned()]
        )
    );
    assert!(
        Role::ALL
            .into_iter()
            .all(|r| config.roles.get(r).cli == Cli::Codex),
        "every role on the agent chosen: {:?}",
        config.roles
    );
    assert!(!config.app.settled());
    let text = std::fs::read_to_string(env.repo.join(".ovenspring/config.toml")).unwrap();
    assert!(
        !text.contains("fixture"),
        "no token in the repository: {text}"
    );
    let held = std::fs::read_to_string(&dash.credentials).unwrap();
    assert!(
        held.contains("xoxb-fixture-0001") && held.contains("lin_api_fixture0001"),
        "{held}"
    );
    assert_eq!(
        std::fs::metadata(&dash.credentials)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        dash.get("/api/v1/settings").json()["githubAccount"],
        json!({"login": "kai", "fromGh": false}),
        "the account the GitHub check answered as, for Settings"
    );
}

// AC: "While setup is not finished, the dashboard shall open on its first step not done: Connect
//     while there is no configuration ..."; "When the owner continues from Connect, the system shall
//     check GitHub, the tracker and Slack, then write `.ovenspring/config.toml` with every role on
//     the agent the owner chose and store the credentials outside the repository" —
//     docs/design/owner-interface-design.md § Acceptance Criteria > Dashboard; "...and change
//     nothing else in the repository" — docs/design/distribution-design.md.
// Behavior: a block check -> Connect refused by Slack -> Connect saved -> a second save.
// Cases (reported distinctly): (a) Connect first; (b) one block is checked without writing; (c) a
//     failing check writes nothing; (d) a passing save writes only `.ovenspring/` and the
//     credentials, every role on the chosen agent, and moves to Run the product; (e) a second save
//     is refused.
// @lane: integration
// @dependency: dashboard server, setup, configuration, credentials file
#[test]
fn connect_checks_each_block_and_writes_the_connections_with_the_chosen_agent() {
    let env = Env::new();
    let refused = Dashboard::with_checks(
        &env,
        Checks {
            refuse: Some("slack"),
        },
    );
    let state = refused.get("/api/v1/setup").json();
    assert_eq!(
        (state["step"].clone(), state["repository"].clone()),
        (json!("connect"), json!("storefront"))
    );

    let github = refused.post(
        "/api/v1/setup/check",
        &json!({"section": "github", "token": null, "baseBranch": "main"}),
    );
    assert_eq!(github.status, 200, "{}", github.body);
    assert_eq!(github.json()["found"], "Signed in as kai.");
    let slack = refused.post(
        "/api/v1/setup/check",
        &json!({"section": "slack", "botToken": "xoxb-1", "appToken": "xapp-1", "channel": "C0123456789", "owner": "U0123456789"}),
    );
    assert_eq!(slack.status, 400);
    assert_eq!(
        slack.json()["errors"]["slack"],
        "slack refused the connection."
    );

    let failed = refused.put("/api/v1/setup", &setup_request());
    assert_eq!(failed.status, 400, "{}", failed.body);
    assert_eq!(
        failed.json()["errors"]["slack"],
        "slack refused the connection."
    );
    assert_eq!(changed(&env.repo), "");
    assert!(!refused.credentials.exists());

    let dash = Dashboard::with_checks(&env, Checks { refuse: None });
    let saved = dash.put("/api/v1/setup", &setup_request());
    assert_eq!(saved.status, 200, "{}", saved.body);
    assert_eq!(saved.json()["step"], "run");
    assert_eq!(changed(&env.repo), "?? .ovenspring/config.toml");
    assert_connected(&env, &dash);
    assert_eq!(dash.put("/api/v1/setup", &setup_request()).status, 409);
}

// AC: "When the owner keeps or changes the product's commands, the system shall start the product
//     once with them and write `[app]` only when it answered"; "...Set the goal while there is no
//     goal" — docs/design/owner-interface-design.md § Acceptance Criteria > Dashboard.
// Behavior: a configuration with `[app]` unsettled -> commands that do not start -> commands that
//     start -> the goal -> setup is done.
// Cases (reported distinctly): (a) Run the product is the step; (b) commands that do not start are
//     refused with their output and not written; (c) commands that start are written and setup
//     moves to the goal; (d) the goal finishes it.
// @lane: integration
// @dependency: dashboard server, setup, configuration, base worktree, goal store
#[test]
fn run_the_product_writes_commands_only_once_they_start_then_the_goal_finishes_setup() {
    let env = Env::new();
    env.configure("[app]\n");
    let dash = Dashboard::with_checks(&env, Checks { refuse: None });
    dash.connect_github("test-token", env.origin.to_str().unwrap());
    assert_eq!(dash.get("/api/v1/setup").json()["step"], "run");

    let broken = dash.put(
        "/api/v1/setup/run",
        &json!({"runs": "commands", "setup": null, "start": "echo 'cannot find module next' >&2; exit 1", "ready": null}),
    );
    assert_eq!(broken.status, 400, "{}", broken.body);
    let output = broken.json()["errors"]["check"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(output.contains("cannot find module next"), "{output}");
    assert!(!Config::load(&env.repo).unwrap().app.settled());

    let started = dash.put("/api/v1/setup/run", &serving());
    assert_eq!(started.status, 200, "{}", started.body);
    assert_eq!(started.json()["step"], "goal");
    let app = Config::load(&env.repo).unwrap().app;
    assert_eq!(app.ready.as_deref(), Some("http://127.0.0.1:$PORT/ready"));

    let goal = dash.put(
        "/api/v1/goal",
        &json!({"text": "Shop customers manage their own orders"}),
    );
    assert_eq!(goal.status, 200, "{}", goal.body);
    assert_eq!(dash.get("/api/v1/setup").json()["step"], "done");
}

// AC: "When the owner records that there is nothing to run, the system shall write that" and
//     `PUT /setup/run` starts only "commands other than the lead's that already started"
//     (AC-066) — docs/design/owner-interface-design.md.
// Behavior: the owner keeps the lead's commands, already started -> `[app]` is written without
//     starting them again; in another repository the owner says there is nothing to run ->
//     `nothing_to_run` is written; both go on to the goal.
// @lane: integration
// @dependency: dashboard server, product-run record
#[test]
fn keeping_the_leads_started_commands_or_nothing_to_run_settles_how_the_product_runs() {
    let env = Env::new();
    env.configure("[app]\n");
    // Would fail if it ran again: the recorded start is the check that passed.
    let found = json!({"runs": "commands", "setup": null, "start": "exit 1", "ready": null,
                       "reason": "README.md"});
    std::fs::create_dir_all(env.state()).unwrap();
    std::fs::write(
        env.state().join("product-run.json"),
        json!({"item": null, "found": found, "check": {"started": {"port": 4100, "seconds": 3}}})
            .to_string(),
    )
    .unwrap();
    let dash = Dashboard::with_checks(&env, Checks { refuse: None });
    let kept = dash.put(
        "/api/v1/setup/run",
        &json!({"runs": "commands", "setup": null, "start": "exit 1", "ready": null}),
    );
    assert_eq!(kept.status, 200, "{}", kept.body);
    assert_eq!(kept.json()["step"], "goal");
    assert_eq!(
        Config::load(&env.repo).unwrap().app.start.as_deref(),
        Some("exit 1")
    );

    let other = Env::new();
    other.configure("[app]\n");
    let dash = Dashboard::with_checks(&other, Checks { refuse: None });
    let nothing = dash.put("/api/v1/setup/run", &json!({"runs": "nothing"}));
    assert_eq!(nothing.status, 200, "{}", nothing.body);
    assert_eq!(nothing.json()["step"], "goal");
    let app = Config::load(&other.repo).unwrap().app;
    assert!(app.nothing_to_run && app.start.is_none(), "{app:?}");
}

// AC: "While the repository has no `.ovenspring/config.toml`, the dashboard shall open on setup and
//     the factory shall start no work (AC-002, AC-060)" — docs/design/distribution-design.md.
// Behavior: `ovenspring daemon` in a repository without configuration -> it serves the dashboard and
//     records its address -> setup, at that address, reports the repository not
//     set up, and nothing is launched.
// @lane: integration
// @dependency: the executable's daemon command, dashboard server
#[test]
fn a_first_start_serves_setup_and_starts_no_work() {
    let env = Env::new();
    env.install_fake_agents();
    let mut daemon = env
        .ovenspring(&["daemon"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let record = env.isolated().join("dashboard.json");
    assert!(
        wait_until(10, || record.exists()),
        "no dashboard address recorded"
    );
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&record).unwrap()).unwrap();
    let url = v["url"].as_str().unwrap().strip_prefix("http://").unwrap();
    let addr = url.split_once('/').unwrap().0.parse().unwrap();

    let setup = request(addr, "GET /api/v1/setup", None).json();
    assert_eq!(setup["step"], "connect");
    std::thread::sleep(std::time::Duration::from_secs(1));
    assert!(env.records().is_empty(), "no agent was launched");
    assert_eq!(changed(&env.repo), "");

    daemon.kill().unwrap();
    daemon.wait().unwrap();
}

/// Stops the daemon `ovenspring start` left, even when an assertion failed first.
struct Started(PathBuf);

impl Drop for Started {
    fn drop(&mut self) {
        if let Ok(pid) = std::fs::read_to_string(&self.0) {
            let _ = std::process::Command::new("kill")
                .args(["-TERM", pid.trim()])
                .status();
        }
    }
}

/// Puts `open` and `xdg-open` on PATH, recording each address they are asked to open; the
/// returned function lists them in order.
fn record_opened_addresses(env: &Env) -> impl Fn() -> Vec<String> {
    let opened = env.dir.path().join("opened");
    for name in ["open", "xdg-open"] {
        let browser = env.fake_bin.join(name);
        std::fs::write(
            &browser,
            format!("#!/bin/sh\necho \"$1\" >> '{}'\n", opened.display()),
        )
        .unwrap();
        std::fs::set_permissions(&browser, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    move || {
        std::fs::read_to_string(&opened)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

/// The socket address of `http://<addr>/...`.
fn address_of(url: &str) -> std::net::SocketAddr {
    url.strip_prefix("http://")
        .unwrap()
        .split_once('/')
        .unwrap()
        .0
        .parse()
        .unwrap()
}

/// The pid of the daemon `start` started, once its output says it started one.
fn started_daemon(start: &std::process::Output, pid_file: &Path) -> String {
    assert!(start.status.success(), "{start:?}");
    let pid = std::fs::read_to_string(pid_file).unwrap().trim().to_owned();
    assert_eq!(
        String::from_utf8_lossy(&start.stdout).trim(),
        format!("Oven Spring started (pid {pid}). The dashboard is open in your browser.")
    );
    pid
}

// AC: docs/design/distribution-design.md § Acceptance Criteria: "When the owner runs `ovenspring
//     start` in a repository, the system shall start that repository's daemon detached from the
//     terminal and open its dashboard in the browser; when the daemon already runs, it shall only
//     open the dashboard (AC-059)"; "When the owner runs `ovenspring stop` in a repository, the
//     system shall stop its daemon after the daemon's current pass (AC-059)".
// Behavior: `ovenspring start` -> the daemon runs detached and the browser is asked to open the
//     dashboard's address -> `start` again -> the same daemon, the address opened again ->
//     `ovenspring stop` -> `start` at once -> a new daemon at the same address, so the owner's
//     address keeps working, and the stopped daemon's process ends.
// @lane: integration
// @dependency: the executable's start, stop and daemon commands, dashboard server
// Substitutes: `open` and `xdg-open` on PATH record the address instead of opening a browser.
#[test]
fn start_runs_one_daemon_and_opens_its_dashboard_and_stop_ends_it() {
    let env = Env::new();
    env.install_fake_agents();
    let opened_lines = record_opened_addresses(&env);
    let pid_file = env.state().join("daemon.pid");
    let _cleanup = Started(pid_file.clone());

    // A pid file left by a daemon that ended (a crash, a reboot), naming a process that is alive
    // but is not a daemon: here, this test's own process.
    std::fs::create_dir_all(env.state()).unwrap();
    std::fs::write(&pid_file, format!("{}\n", std::process::id())).unwrap();
    let not_running = env.run(&["stop"]);
    assert_eq!(
        String::from_utf8_lossy(&not_running.stdout).trim(),
        "Oven Spring is not running"
    );

    let pid = started_daemon(&env.run(&["start"]), &pid_file);
    let url = opened_lines()[0].clone();
    assert_eq!(
        request(address_of(&url), "GET /api/v1/setup", None).status,
        200,
        "the address opens the dashboard"
    );

    let again = env.run(&["start"]);
    assert!(again.status.success(), "{again:?}");
    assert!(
        String::from_utf8_lossy(&again.stdout)
            .starts_with(&format!("Oven Spring is already running (pid {pid}).")),
        "{again:?}"
    );
    assert_eq!(opened_lines(), [url.clone(), url.clone()]);
    assert_eq!(std::fs::read_to_string(&pid_file).unwrap().trim(), pid);

    let stopped = env.run(&["stop"]);
    assert!(stopped.status.success(), "{stopped:?}");
    assert_eq!(
        String::from_utf8_lossy(&stopped.stdout).trim(),
        format!("Oven Spring stopped (pid {pid})")
    );

    // Started at once: `stop` returned only after the daemon let the repository go.
    started_daemon(&env.run(&["start"]), &pid_file);
    assert_eq!(
        opened_lines().last(),
        Some(&url),
        "a new daemon serves at the same address"
    );
    // Reaping the exited process is the operating system's, so its end is awaited.
    let stopped_pid = pid.parse().unwrap();
    assert!(
        support::wait_until(15, || !ovenspring_daemon::app::alive(stopped_pid)),
        "the stopped daemon's process has ended"
    );
    let stopped_again = env.run(&["stop"]);
    assert!(stopped_again.status.success(), "{stopped_again:?}");
}

/// A medium issue whose design review finds one thing its author applies, stopped at its Work
/// Plan.
fn recorded_issue_script() -> Value {
    json!({
        "requirements": [{"output": {"convergence": {"outcome": "Owners export their orders as CSV.", "nonGoals": ["XLSX"]},
                                     "costEvidence": {"drivers": [{"kind": "observed", "fact": "The export touches the orders API and a new job table.", "source": "src/orders.ts"}]},
                                     "structuralScale": "medium", "decision": "proceed"}}],
        "codebase-analysis": [{"output": {}}],
        "scope-check": [{"output": {"decision": "proceed", "structuralScale": "medium"}}],
        "design": [
            support::scripts::document_run("docs/design/ACME-123.md", "DesignDoc"),
            {"change": {"path": "docs/design/ACME-123.md", "content": "Test the header row.\n"},
             "output": {"status": "completed", "documentType": "DesignDoc", "path": "docs/design/ACME-123.md",
                        "dispositions": [{"id": "I001", "disposition": "apply", "reason": "A test now reads the header."}]}}
        ],
        "design-review": [
            {"output": {"verdict": {"decision": "needs_revision"}, "issues": [{"id": "I001", "location": "s 2", "description": "The header row is untested.", "correction": "Test it."}]}, "verdict": "findings"},
            {"output": {"verdict": {"decision": "pass"}, "prior_feedback_reconciliation": [{"id": "I001", "prior_disposition": "apply", "status": "resolved", "evidence": "e"}]}, "verdict": "pass"}
        ],
        "design-sync": [{"output": {"sync_status": "SKIPPED"}, "verdict": "pass"}],
        "test-design": [{"output": {"status": "completed", "feature": "f", "generatedFiles": {}}}],
        "work-plan": [{"hang": true}]
    })
}

// AC: "The issue screen shows the issue's outcome, progress, findings with how each was answered,
//     links, launches and cost (AC-044)" — UI Spec S-02, IssueHeader; owner-interface design
//     § Dashboard server `GET /issues/{key}`.
// Behavior: ACME-123 runs through the factory with fake agents: requirements, design, a design
//     review with one finding, the author's answer, a passing review, and a plan still running ->
//     `GET /api/v1/issues/ACME-123` reads the records the run left.
// @lane: integration
// @dependency: daemon, launcher, fake agents, dashboard server, history
#[test]
fn an_issue_is_read_back_from_the_records_its_run_left() {
    let env = Env::new();
    env.configure("");
    env.script_agents(&recorded_issue_script());
    let tracker = Arc::new(support::recording::RecordingTracker::default());
    *tracker.issues.lock().unwrap() = vec![ovenspring_integrations::tracker::TrackerIssue {
        key: "ACME-123".into(),
        title: "Export order history".into(),
        author: String::new(),
        body: "As CSV.".into(),
        url: "https://linear.app/acme/issue/ACME-123".into(),
        state: "Todo".into(),
        started: false,
        labels: vec![],
        comments: vec![],
        depends_on: vec![],
    }];
    let gh = Arc::new(support::scripts::always_passing(
        &env,
        support::scripts::requiring_test(),
    ));
    let mut d = support::daemon(
        &env,
        support::services(
            gh,
            Arc::new(support::recording::RecordingChannel::default()),
            tracker,
        ),
    );
    // The tracker is read before an issue begins, as it is before any sprint plans one.
    d.tick().unwrap();
    d.begin_issue("ACME-123", "Export order history", vec![])
        .unwrap();
    // This test begins the issue directly; sprint start copies this link from its selected snapshot.
    let mut record = d.issue("ACME-123").unwrap().unwrap();
    record.url = Some("https://linear.app/acme/issue/ACME-123".into());
    d.store()
        .write_record(&format!("issues/{}.json", record.key()), &record)
        .unwrap();
    support::run_until(
        &mut d,
        60,
        |_| {},
        |d| d.issue("ACME-123").unwrap().unwrap().flow.stage == Stage::running(Step::WorkPlan),
    );
    let dash = Dashboard::with_checks(&env, Checks { refuse: None });
    assert!(wait_until(10, || dash
        .get("/api/v1/issues/ACME-123")
        .json()["now"]["since"]
        .is_string()));

    let issue = dash.get("/api/v1/issues/ACME-123").json();

    assert_eq!(
        (issue["outcome"].clone(), issue["nonGoals"].clone()),
        (json!("Owners export their orders as CSV."), json!(["XLSX"]))
    );
    assert_eq!(
        (issue["scale"].clone(), issue["scaleEvidence"].clone()),
        (
            json!("medium"),
            json!(["The export touches the orders API and a new job table."])
        )
    );
    assert_eq!(issue["tracker"], "https://linear.app/acme/issue/ACME-123");
    assert_eq!(
        issue["findings"],
        json!([{"source": {"kind": "review", "review": "design-review", "round": 1}, "text": "The header row is untested. Test it.", "disposition": "apply", "answer": "A test now reads the header."}])
    );
    assert_eq!(
        (
            issue["now"]["section"].clone(),
            issue["now"]["phase"].clone(),
            issue["now"]["activity"].clone()
        ),
        (json!("in-progress"), json!("design"), json!("planning"))
    );
    let path: Vec<(String, String)> = issue["path"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["phase"].as_str().unwrap().into(),
                p["state"].as_str().unwrap().into(),
            )
        })
        .collect();
    assert_eq!(
        path,
        [
            ("requirements", "done"),
            ("design", "now"),
            ("build", "ahead"),
            ("review", "ahead"),
            ("merge", "ahead")
        ]
        .map(|(p, s)| (p.to_owned(), s.to_owned()))
    );
    assert!(issue["path"][0]["endedAt"].is_string(), "{issue}");
    assert_eq!(dash.get("/api/v1/issues/NOPE-1").status, 404);
}

// AC: "Earlier sprints lists the sprints that ended, each with its key results and agreement
//     changes" — UI Spec S-03; owner-interface design § Dashboard server `GET /sprints`.
// Behavior: two ended sprints are archived with their reviews and issues -> `GET /api/v1/sprints`.
// @lane: integration
// @dependency: dashboard server, history, store
#[test]
fn ended_sprints_come_back_newest_first_with_their_results() {
    let env = Env::new();
    let dash = Dashboard::with_checks(&env, Checks { refuse: None });
    let review = |met: bool, agreements: Value| {
        serde_json::from_value::<ovenspring_core::output::SprintReviewOutput>(json!({
            "key_results": [{"text": "A delayed order shows its reason.", "met": met, "evidence": "ACME-120 merged"}],
            "agreements": agreements,
            "message": "m"
        }))
        .unwrap()
    };
    for (id, met, agreements) in [
        ("20261117-0900", false, json!([])),
        (
            "20261119-0900",
            true,
            json!([{"file": "common.md", "content": "- Name the precedent.\n"}]),
        ),
    ] {
        dash.store
            .write_record(
                &format!("sprints/{id}.json"),
                &SprintState {
                    id: id.into(),
                    stage: SprintStage::ApplyingAgreements,
                    plan: Some(SprintPlanOutput {
                        purpose: format!("Purpose {id}"),
                        key_results: vec![],
                        issues: vec![],
                        message: "m".into(),
                    }),
                    review: Some(review(met, agreements)),
                    agreements_pr: None,
                    agreements_url: None,
                    agreements_need_approval: false,
                    agreements_pushed_at: None,
                    started_at: Some(format!(
                        "{}-{}-{}T09:00:00Z",
                        &id[..4],
                        &id[4..6],
                        &id[6..8]
                    )),
                    ended_at: Some("2026-11-20T18:00:00Z".into()),
                    failed: None,
                    failure: None,
                    resume: None,
                },
            )
            .unwrap();
    }
    let mut merged = IssueRecord::new("ACME-120", "Delay reasons", vec![]);
    merged.flow.stage = Stage::Merged;
    merged.sprint = Some("20261119-0900".into());
    dash.store
        .write_record("issues/ACME-120.json", &merged)
        .unwrap();

    let sprints = dash.get("/api/v1/sprints").json();

    let ids: Vec<&str> = sprints
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["20261119-0900", "20261117-0900"]);
    assert_eq!(sprints[0]["keyResults"][0]["met"], true);
    assert_eq!(
        (
            sprints[0]["agreements"].clone(),
            sprints[0]["merged"].clone()
        ),
        (json!(["common.md"]), json!(["ACME-120"]))
    );
    assert_eq!(
        (
            sprints[1]["agreements"].clone(),
            sprints[1]["keyResults"][0]["met"].clone()
        ),
        (json!([]), json!(false))
    );
}

// AC: PRD AC-045: "The owner can list the factory's launches filtered by sprint and issue, and open
//     each one's model, duration, cost and log." docs/design/owner-interface-design.md § Dashboard
//     server: `GET /launches?sprint=&issue=`, `GET /launches/{id}`.
//     PRD AC-044 (part): "...its launches and cost."
// Behavior: the daemon launches the fake agent for ACME-123 and ACME-124 of sprint S1, the agent
//     printing the CLI's result with its cost and last message -> the launches API lists and
//     details what the launcher logged, with what each run's output names (the judged scale).
// @lane: integration
// @dependency: daemon launcher, launch log, dashboard server, fake agent
#[test]
fn launches_recorded_by_the_launcher_are_listed_and_detailed_by_the_api() {
    let env = Env::new();
    env.configure("");
    env.script_agents(&json!({
        "requirements": [{"output": {"convergence": {"outcome": "o"}, "structuralScale": "small", "decision": "proceed"}}],
        "implement": [support::scripts::implement_run("src/d.ts")],
        "quality": [{"hang": true}]
    }));
    let gh = Arc::new(support::scripts::always_passing(
        &env,
        support::scripts::requiring_test(),
    ));
    let mut d = support::daemon(
        &env,
        support::services(
            gh,
            Arc::new(support::recording::RecordingChannel::default()),
            Arc::new(support::recording::RecordingTracker::default()),
        ),
    );
    for key in ["ACME-123", "ACME-124"] {
        d.begin_issue(key, key, vec![]).unwrap();
    }
    support::run_until(
        &mut d,
        60,
        |_| {},
        |d| {
            ["ACME-123", "ACME-124"]
                .iter()
                .all(|k| d.issue(k).unwrap().unwrap().flow.stage == Stage::running(Step::Quality))
        },
    );
    for key in ["ACME-123", "ACME-124"] {
        let mut r = records::issue(d.store(), key).unwrap().unwrap();
        r.sprint = Some("S1".into());
        d.store()
            .write_record(&format!("issues/{key}.json"), &r)
            .unwrap();
    }
    let dash = Dashboard::with_checks(&env, Checks { refuse: None });

    let rows = dash.get("/api/v1/launches?sprint=S1&issue=ACME-123").json();

    let seen: Vec<(String, String, String)> = rows
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["step"].as_str().unwrap().into(),
                r["phase"].as_str().unwrap().into(),
                r["result"].as_str().unwrap().into(),
            )
        })
        .collect();
    assert_eq!(
        seen,
        [
            ("quality", "build", "running"),
            ("implement", "build", "done"),
            // The requirements step's work turn, then its preparation turn.
            ("requirements", "requirements", "done"),
            ("requirements", "requirements", "done")
        ]
        .map(|(s, p, r)| (s.to_owned(), p.to_owned(), r.to_owned()))
    );
    assert!(
        rows.as_array()
            .unwrap()
            .iter()
            .all(|r| r["issue"] == "ACME-123")
    );
    assert_eq!(rows[2]["detail"]["scale"], "small");
    let id = rows[2]["id"].as_str().unwrap();
    let detail = dash.get(&format!("/api/v1/launches/{id}")).json();
    assert_eq!(
        (detail["cli"].clone(), detail["costUsd"].clone()),
        (json!("claude"), json!(0.01))
    );
    assert!(detail["endedAt"].is_string(), "{detail}");
    assert!(
        detail["messages"]
            .as_array()
            .unwrap()
            .last()
            .and_then(Value::as_str)
            .unwrap()
            .starts_with("Finished requirements for "),
        "{detail}"
    );
    assert_eq!(
        // Requirements' preparation and work turns, and the implementation.
        dash.get("/api/v1/issues/ACME-123").json()["costUsd"],
        json!(0.03)
    );
    assert_eq!(dash.get("/api/v1/launches/..%2F..%2Fgoal.md").status, 404);
}

// AC: "Credentials shall be stored only in the OS credential store or the explicit file fallback,
//     never in the repository (AC-013)" — docs/design/distribution-design.md § Acceptance Criteria.
//     docs/design/owner-interface-design.md § Dashboard server: "`GET /settings` / `PUT
//     /settings/{section}` ... which credentials are held, each by its last four characters
//     (values are write-only)".
//     PRD AC-012: "The owner can set ... the Slack credentials and channel, and the Slack users who
//     are owners."
// Behavior: `PUT /api/v1/settings/slack` with bot token `xoxb-fixture-0001`, app token
//     `xapp-fixture-0001`, channel and owners -> the server splits settings from
//     credentials -> `GET /api/v1/settings` names each credential by its last four characters.
// @lane: integration
// @dependency: dashboard server, configuration writer, credential store (file fallback)
#[test]
fn credentials_saved_through_settings_are_write_only_and_stay_out_of_config_toml() {
    let env = Env::new();
    env.configure("");
    let dash = Dashboard::with_checks(&env, Checks { refuse: None });

    let saved = dash.put(
        "/api/v1/settings/slack",
        &json!({"channel": "C0987654321", "owners": ["U0123456789", "U0234567890"],
                "botToken": "xoxb-fixture-0001", "appToken": "xapp-fixture-0002"}),
    );

    assert_eq!(saved.status, 200, "{}", saved.body);
    let config = Config::load(&env.repo).unwrap();
    assert_eq!(
        (config.slack.channel.as_str(), config.slack.owners.len()),
        ("C0987654321", 2)
    );
    let text = std::fs::read_to_string(env.repo.join(".ovenspring/config.toml")).unwrap();
    assert!(!text.contains("fixture"), "{text}");
    let held = std::fs::read_to_string(&dash.credentials).unwrap();
    assert!(
        held.contains("xoxb-fixture-0001") && held.contains("xapp-fixture-0002"),
        "{held}"
    );
    assert_eq!(
        std::fs::metadata(&dash.credentials)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let read = dash.get("/api/v1/settings");
    assert_eq!(
        read.json()["held"],
        json!([
            {"credential": "slack-bot-token", "ending": "0001"},
            {"credential": "slack-app-token", "ending": "0002"},
        ])
    );
    for body in [&saved.body, &read.body] {
        assert!(!body.contains("fixture"), "{body}");
    }
    assert_eq!(changed(&env.repo), "M .ovenspring/config.toml");
}

// AC: UI Spec "Settings not applied": change it in Settings or fix the file; distribution-design
//     § Error Handling (Validation): the owner fixes it in the dashboard.
// Behavior: a selected skill is missing from the repository -> `GET /settings` serves the file
//     with why the factory cannot use it -> a save that unchecks the skill is written, and the
//     problem is gone.
// @lane: integration
// @dependency: dashboard server, configuration
#[test]
fn settings_the_factory_refuses_are_shown_and_corrected_in_settings() {
    let env = Env::new();
    env.configure("[roles.lead]\ncli = \"claude\"\nskills = [\"payments-domain\"]\n");
    let dash = Dashboard::with_checks(&env, Checks { refuse: None });
    let read = dash.get("/api/v1/settings");
    assert_eq!(read.status, 200, "{}", read.body);
    let problem = read.json()["problem"].as_str().unwrap().to_owned();
    assert!(problem.contains("roles.lead.skills"), "{problem}");

    let saved = dash.put(
        "/api/v1/settings/roles",
        &json!({"role": "lead", "cli": "claude", "model": null, "effort": null, "skills": [], "append": ""}),
    );
    assert_eq!(saved.status, 200, "{}", saved.body);
    assert_eq!(saved.json()["problem"], Value::Null);
    assert!(Config::load(&env.repo).is_ok());
}

// AC: PRD AC-059: "After `ovenspring start`, the owner works with the factory only through Slack and
//     the dashboard." How the product runs in a worktree (AC-031), the commands agents must not run
//     (AC-054) and the review bots whose comments count (AC-022, AC-023) are set in Settings.
// Behavior: `PUT /api/v1/settings/{app,guard,review}` -> `.ovenspring/config.toml` holds them, blank
//     lines and fields left out; changed commands that do not start are refused.
#[test]
fn app_guard_and_review_settings_are_saved_to_the_configuration() {
    let env = Env::new();
    env.configure("");
    let dash = Dashboard::with_checks(&env, Checks { refuse: None });
    dash.connect_github("test-token", env.origin.to_str().unwrap());

    let broken = dash.put(
        "/api/v1/settings/app",
        &json!({"runs": "commands", "setup": "", "start": "exit 3", "ready": ""}),
    );
    assert_eq!(broken.status, 400, "{}", broken.body);
    assert!(broken.json()["errors"]["check"].is_string());
    let mut serving = serving();
    serving["ready"] = json!("");
    let start = serving["start"].clone();
    for (section, body) in [
        ("app", serving),
        ("guard", json!({"deny": ["terraform apply *", " "]})),
        ("review", json!({"bots": ["ai-reviewer[bot]"]})),
    ] {
        let saved = dash.put(&format!("/api/v1/settings/{section}"), &body);
        assert_eq!(saved.status, 200, "{section}: {}", saved.body);
    }

    let config = Config::load(&env.repo).unwrap();
    assert!(config.app.setup.is_some() && !config.app.nothing_to_run);
    assert_eq!(config.app.start.as_deref(), start.as_str());
    assert_eq!(config.app.ready, None);
    assert_eq!(config.guard.deny, ["terraform apply *"]);
    assert_eq!(config.review.bots, ["ai-reviewer[bot]"]);
}

// AC: PRD AC-069 (the owner names kinds of change a person reviews) and AC-070 (the working
//     agreements and knowledge folders follow the repository's layout), both set in Settings.
// Behavior: `PUT /api/v1/settings/general` with a kind of change and the folders -> the
//     configuration holds them, each folder without its trailing slash; a folder outside the
//     repository and a kind with no words are refused with the field to correct.
// @lane: integration
// @dependency: dashboard API, settings, configuration validation
#[test]
fn what_a_person_reviews_and_the_agreements_and_knowledge_folders_are_saved_from_settings() {
    let env = Env::new();
    env.configure("");
    let dash = Dashboard::with_checks(&env, Checks { refuse: None });
    let general = |reviews: Value, folder: &str| {
        json!({"baseBranch": "main", "mergeBy": "factory", "held": {}, "githubToken": null,
               "personReviews": reviews, "agreementsPath": "docs/factory/agreements/",
               "knowledgePath": folder})
    };

    let saved = dash.put(
        "/api/v1/settings/general",
        &general(
            json!({"payments": " How customers are charged "}),
            "docs/knowledge/",
        ),
    );
    let outside = dash.put("/api/v1/settings/general", &general(json!({}), "../notes"));
    let wordless = dash.put(
        "/api/v1/settings/general",
        &general(json!({"payments": ""}), "docs/knowledge"),
    );

    assert_eq!(saved.status, 200, "{}", saved.body);
    let config = Config::load(&env.repo).unwrap();
    assert_eq!(config.agreements.path, "docs/factory/agreements");
    assert_eq!(config.knowledge.path, "docs/knowledge");
    assert_eq!(
        config.person_reviews.get("payments").map(String::as_str),
        Some("How customers are charged")
    );
    assert_eq!(outside.status, 400, "{}", outside.body);
    assert!(
        outside.json()["errors"]["knowledge.path"].is_string(),
        "{}",
        outside.body
    );
    assert_eq!(wordless.status, 400, "{}", wordless.body);
    assert!(
        wordless.json()["errors"]["person_reviews"].is_string(),
        "{}",
        wordless.body
    );
}

// AC: docs/design/owner-interface-design.md: "When a prototype is ready, its approval message shall
//     link to the issue's prototype page in the dashboard and state its reference strength and
//     why; the page shows the lead's HTML in a sandbox without the dashboard's origin (AC-031)"
// Behavior: an issue's `prototype` output names an HTML file in its worktree ->
//     `GET /api/v1/issues/<key>/prototype` answers the page under a sandbox policy, and the issue
//     carries the strength; an output naming a file outside the worktree is not served.
// @lane: integration
// @dependency: dashboard server, history, store
#[test]
fn a_prototype_is_served_sandboxed_from_its_issue_worktree_only() {
    let env = Env::new();
    let dash = Dashboard::with_checks(&env, Checks { refuse: None });
    let page = "<!doctype html><title>Orders</title><p>Every order, newest first.</p>";
    let assets = dash
        .store
        .worktree("ACME-130")
        .join("docs/ui-spec/assets/orders");
    std::fs::create_dir_all(&assets).unwrap();
    std::fs::write(assets.join("prototype.html"), page).unwrap();
    std::fs::write(dash.store.root().join("outside.html"), "not the prototype").unwrap();
    std::fs::create_dir_all(dash.store.worktree("ACME-131")).unwrap();
    let issue = |key: &str, path: &str| {
        let out = dash.store.root().join(format!("{key}-prototype.json"));
        std::fs::write(
            &out,
            json!({"path": path, "referenceStrength": "reference", "basis": "no design tokens"})
                .to_string(),
        )
        .unwrap();
        let mut record = IssueRecord::new(key, "Orders", vec![]);
        record
            .outputs
            .insert("prototype".into(), out.display().to_string());
        dash.store
            .write_record(&format!("issues/{key}.json"), &record)
            .unwrap();
    };
    issue("ACME-130", "docs/ui-spec/assets/orders/prototype.html");
    issue("ACME-131", "../../outside.html");

    let served = dash.get("/api/v1/issues/ACME-130/prototype");
    let detail = dash.get("/api/v1/issues/ACME-130").json();
    let escaped = dash.get("/api/v1/issues/ACME-131/prototype");

    assert_eq!(
        (
            served.status,
            served.header("content-type"),
            served.header("content-security-policy"),
            served.body.as_str(),
        ),
        (
            200,
            Some("text/html; charset=utf-8"),
            Some("sandbox allow-scripts"),
            page
        ),
        "the page runs in an opaque origin, away from the dashboard's API"
    );
    assert_eq!(detail["prototype"], "reference");
    assert_eq!(escaped.status, 404, "{}", escaped.body);
}

/// Puts `content` at `path` as the install script does: written beside it, then moved over it in
/// one step.
fn install(path: &Path, content: &[u8]) {
    let next = path.with_file_name(".ovenspring.new");
    std::fs::write(&next, content).unwrap();
    std::fs::set_permissions(&next, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::rename(&next, path).unwrap();
}

// AC: "The dashboard's header shows the version the repository's factory runs; when a newer
//     release is out it says so with how to update, and when a newer executable is installed
//     but not yet running it says a restart runs it (AC-071)" —
//     docs/design/distribution-design.md § Version and updates.
// Behavior: the daemon started from the installed executable and a newer release is out -> the
//     version answer offers the release -> the install script puts that release in place ->
//     the answer asks for a restart instead.
// Cases (reported distinctly): (a) the built executable installed, so its own `--version` is
//     what is read; (b) a newer executable moved over it.
// @lane: integration
// @dependency: dashboard server (version answer, release watch), installed executable
#[test]
fn the_version_answer_offers_a_newer_release_then_a_restart_once_it_is_installed() {
    let env = Env::new();
    let installed = env.dir.path().join("bin/ovenspring");
    std::fs::create_dir_all(installed.parent().unwrap()).unwrap();
    install(
        &installed,
        &std::fs::read(env!("CARGO_BIN_EXE_ovenspring")).unwrap(),
    );
    let running = env!("CARGO_PKG_VERSION");
    let server = ovenspring_server::start(
        Context {
            repo: env.repo.clone(),
            store: Store::open(env.state()).unwrap(),
            credentials: env.dir.path().join("config/credentials.toml"),
            checks: Arc::new(Checks { refuse: None }),
            home: env.dir.path().join("home"),
            codex_home: env.dir.path().join("home/.codex"),
            version: support::versions(installed.clone(), "v99.0.0"),
        },
        0,
    )
    .unwrap();
    let version = || request(server.addr, "GET /api/v1/version", None).json();

    let offered = wait_until(10, || !version()["update"].is_null());
    assert!(offered, "no update offered: {}", version());
    assert_eq!(
        version(),
        json!({"running": running, "update": {"kind": "available", "version": "99.0.0"}}),
        "(a) the installed executable is the running version, and the release is newer"
    );

    install(&installed, b"#!/bin/sh\necho 'ovenspring 99.0.0'\n");

    assert_eq!(
        version(),
        json!({"running": running, "update": {"kind": "restart", "version": "99.0.0"}}),
        "(b) the release is installed and waits for a restart"
    );
}

// AC: Design `docs/design/events-and-error-notices-design.md` § Risks and Mitigation:
//     `events.jsonl` grows without rotation (about 360 lines a day), and the Events page reads it
//     whole on each 5-second poll; "Measure on a large seeded file". § Non-Functional Requirements:
//     the page reflects a new event at its next 5 s poll.
// Behavior: a year of lines at that rate, with errors among them, is seeded -> `GET /api/v1/events`
//     is asked for the newest lines and for every line -> each answers well within one poll.
// @lane: integration
// @dependency: dashboard server, `events::page`, store
/// A year of `events.jsonl` at about 360 lines a day.
const YEAR_OF_LINES: usize = 130_000;

#[test]
fn the_events_page_answers_within_a_poll_over_a_year_of_lines() {
    let env = Env::new();
    let dash = Dashboard::with_checks(&env, Checks { refuse: None });
    let mut lines = Vec::with_capacity(YEAR_OF_LINES);
    for i in 0..YEAR_OF_LINES {
        let line = if i % 100 == 0 {
            json!({"event": "service-unreachable", "operation": "reading the tracker", "why": "no-connection",
                   "tracker": "linear", "at": "2026-11-27T04:00:00Z",
                   "failure": {"key": "tracker-unreachable", "since": "2026-11-27T04:00:00Z",
                               "cause": "no answer", "start": i == 0, "notice": "waiting"}})
        } else {
            json!({"event": "pushed", "issue": format!("ACME-{}", i % 400), "sha": format!("{i:040x}"),
                   "at": "2026-11-27T04:00:00Z"})
        };
        lines.push(line.to_string());
    }
    let text = lines.join("\n");
    std::fs::write(dash.store.root().join("events.jsonl"), &text).unwrap();

    for path in [
        "/api/v1/events",
        "/api/v1/events?show=errors",
        "/api/v1/events?from=0",
    ] {
        let started = std::time::Instant::now();
        let answer = dash.get(path);
        let took = started.elapsed();
        eprintln!(
            "{path}: {took:?} over {YEAR_OF_LINES} lines ({} bytes)",
            text.len()
        );
        assert_eq!(answer.status, 200, "{path}");
        assert!(
            took < std::time::Duration::from_secs(5),
            "{path} took {took:?}, longer than the page's poll"
        );
    }
}
