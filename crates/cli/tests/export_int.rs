//! @lane: integration
//! Area: the download of a sprint's or an issue's records through the dashboard server.
//!
//! The secrets the download must mask are generated at test time, never committed as literals.

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
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use ovenspring_core::item::Status;
use ovenspring_daemon::issues::IssueRecord;
use ovenspring_daemon::owner::{
    Answer, AnswerKind, ApprovalRecord, DecisionRecord, DecisionStatus,
};
use ovenspring_integrations::channel::DecisionKind;
use ovenspring_server::Context;
use ovenspring_server::setup::{GitHubAccount, SlackSetup, TrackerSetup};
use ovenspring_store::Store;
use ovenspring_store::credentials::{self, Credentials};
use serde_json::{Value, json};
use support::Env;
use support::http::request;

// AC: "When the owner requests a download, the system shall produce the records of the chosen scope
//     in one form, with secrets masked (AC-046, AC-047)" — docs/design/owner-interface-design.md
//     § Acceptance Criteria > Dashboard; § Download: "Masking runs on every downloaded byte".
//     Integration Verification Point: "Download: the records of a fixture sprint, with every
//     credential the factory holds and every listed token format masked."
// Behavior: `GET /api/v1/export?scope=sprint:20261121-0905` and `scope=issue:ACME-123` -> the server
//     builds one JSON Lines file from the records -> the downloaded bytes hold the scoped records in
//     time order with every planted secret replaced by `[masked]`.
// Cases (reported distinctly): sprint scope; issue scope.
// @lane: integration
// @dependency: dashboard server, download builder, masking, store, credentials file
#[test]
fn the_download_masks_every_held_credential_and_token_pattern() {
    let env = Env::new();
    let unique = format!(
        "{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let held = Credentials {
        slack: credentials::Slack {
            bot_token: Some(format!("xoxb-{unique}")),
            app_token: Some(format!("xapp-{unique}")),
        },
        linear: credentials::Linear {
            api_key: Some(format!("plain{unique}held")),
        },
        github: credentials::GitHub {
            token: Some(format!("ghp_{unique}")),
        },
    };
    let patterns = [
        format!("xoxp-{unique}"),
        format!("github_pat_{unique}"),
        format!("gho_{unique}"),
        format!("lin_api_{unique}"),
        format!("sk-{unique}"),
        format!("AKIA{}", &format!("{unique:0>16}").to_uppercase()[..16]),
        format!("-----BEGIN OPENSSH PRIVATE KEY-----\n{unique}\n-----END OPENSSH PRIVATE KEY-----"),
    ];
    let planted: Vec<String> = held
        .values()
        .into_iter()
        .map(str::to_owned)
        .chain(patterns)
        .collect();
    let all = planted.join(" ");
    let credentials_file = env.dir.path().join("config/credentials.toml");
    credentials::write(&credentials_file, &held).unwrap();

    let store = Store::open(env.state()).unwrap();
    write_records(&store, &all);
    let server = ovenspring_server::start(
        Context {
            repo: env.repo.clone(),
            store: Store::open(env.state()).unwrap(),
            credentials: credentials_file,
            checks: Arc::new(NoChecks),
            home: env.dir.path().join("home"),
            codex_home: env.dir.path().join("home/.codex"),
            version: support::no_new_version(),
        },
        0,
    )
    .unwrap();

    let sprint = request(server.addr, "GET /api/v1/export?scope=sprint:S1", None);

    assert_eq!(sprint.status, 200);
    assert_eq!(
        sprint.header("content-disposition"),
        Some("attachment; filename=\"ovenspring-sprint-S1.jsonl\"")
    );
    for value in &planted {
        assert!(!sprint.body.contains(value.as_str()), "{value} survived");
    }
    let lines: Vec<Value> = sprint
        .body
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let kinds: BTreeSet<&str> = lines.iter().map(|l| l["type"].as_str().unwrap()).collect();
    assert_eq!(
        kinds,
        BTreeSet::from(["approval", "decision", "event", "item", "launch"])
    );
    let times: Vec<&str> = lines.iter().map(|l| l["at"].as_str().unwrap()).collect();
    assert!(
        times.windows(2).all(|w| w[0] <= w[1]),
        "time order: {times:?}"
    );
    for kind in &kinds {
        assert!(
            lines
                .iter()
                .any(|l| l["type"] == *kind && l.to_string().contains("[masked]")),
            "{kind}: the planted value is masked where it was"
        );
    }

    let issue = request(server.addr, "GET /api/v1/export?scope=issue:ACME-123", None);
    assert_eq!(issue.status, 200);
    assert!(!issue.body.contains("ACME-124"), "{}", issue.body);
    assert!(issue.body.contains("ACME-123"));
}

// AC: "A newly recorded scoped line, such as `filter-failed` with `command`, `output` and `failure`,
//     is exported whole with its added fields" (docs/design/events-and-error-notices-design.md
//     § Change Impact Map, § Output Comparison).
// Behavior: an issue's events include a `filter-failed` start whose command and output hold a held
//     credential -> `GET /api/v1/export?scope=issue:ACME-123` -> the record is exported whole,
//     masked, between the issue's earlier and later lines.
// @lane: integration
// @dependency: dashboard server, download builder, masking, store, credentials file
#[test]
fn an_issues_download_holds_its_filter_failure_whole_and_masked() {
    let env = Env::new();
    let held = Credentials {
        linear: credentials::Linear {
            api_key: Some("plainHeld4c2e9f17".into()),
        },
        ..Credentials::default()
    };
    let credentials_file = env.dir.path().join("config/credentials.toml");
    credentials::write(&credentials_file, &held).unwrap();
    let store = Store::open(env.state()).unwrap();
    store
        .write_record(
            "issues/ACME-123.json",
            &IssueRecord::new("ACME-123", "ACME-123", vec![]),
        )
        .unwrap();
    for line in [
        json!({"event": "fact", "issue": "ACME-123", "at": "2026-11-21T12:01:00Z"}),
        json!({"event": "filter-failed", "issue": "ACME-123", "at": "2026-11-21T12:02:00Z",
               "error": "exited with status 1", "command": "check --key plainHeld4c2e9f17",
               "output": "denied for plainHeld4c2e9f17",
               "failure": {"key": "filter", "since": "2026-11-21T12:02:00Z",
                           "cause": "exited with status 1 (denied for plainHeld4c2e9f17)",
                           "start": true, "notice": "waiting"}}),
        json!({"event": "fact", "issue": "ACME-123", "at": "2026-11-21T12:03:00Z"}),
        json!({"event": "filter-failed", "issue": "ACME-124", "at": "2026-11-21T12:04:00Z",
               "error": "exited with status 1", "failure": {"key": "filter",
               "since": "2026-11-21T12:02:00Z", "cause": "exited with status 1"}}),
    ] {
        store.append_event(&line).unwrap();
    }
    let server = ovenspring_server::start(
        Context {
            repo: env.repo.clone(),
            store: Store::open(env.state()).unwrap(),
            credentials: credentials_file,
            checks: Arc::new(NoChecks),
            home: env.dir.path().join("home"),
            codex_home: env.dir.path().join("home/.codex"),
            version: support::no_new_version(),
        },
        0,
    )
    .unwrap();

    let issue = request(server.addr, "GET /api/v1/export?scope=issue:ACME-123", None);

    assert_eq!(issue.status, 200);
    let lines: Vec<Value> = issue
        .body
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(
        lines,
        [
            json!({"type": "event", "event": "fact", "issue": "ACME-123", "at": "2026-11-21T12:01:00Z"}),
            json!({"type": "event", "event": "filter-failed", "issue": "ACME-123",
                   "at": "2026-11-21T12:02:00Z", "error": "exited with status 1",
                   "command": "check --key [masked]", "output": "denied for [masked]",
                   "failure": {"key": "filter", "since": "2026-11-21T12:02:00Z",
                               "cause": "exited with status 1 (denied for [masked])",
                               "start": true, "notice": "waiting"}}),
            json!({"type": "event", "event": "fact", "issue": "ACME-123", "at": "2026-11-21T12:03:00Z"}),
        ]
    );
}

struct NoChecks;

impl ovenspring_server::SetupChecks for NoChecks {
    fn tracker(&self, _: &Path, _: &TrackerSetup, _: Option<&str>) -> Result<String, String> {
        Err("unused".into())
    }
    fn slack(&self, _: &SlackSetup) -> Result<String, String> {
        Err("unused".into())
    }
    fn github(&self, _: &Path, _: Option<&str>, _: &str) -> Result<GitHubAccount, String> {
        Err("unused".into())
    }
}

/// Sprint S1 with ACME-123 and ACME-124, each record carrying `secret`: an event, a launch's start
/// and end, an item with its result and output, a decision and its approval.
fn write_records(store: &Store, secret: &str) {
    for (n, key) in ["ACME-123", "ACME-124"].into_iter().enumerate() {
        let mut r = IssueRecord::new(key, key, vec![]);
        r.sprint = Some("S1".into());
        store
            .write_record(&format!("issues/{key}.json"), &r)
            .unwrap();
        let at = |minute: usize| format!("2026-11-21T12:{:02}:00Z", minute + n * 10);
        store
            .append_event(&json!({"event": "fact", "issue": key, "at": at(1), "note": secret}))
            .unwrap();
        let item = format!("20261121-1200{n}0-implement-{key}");
        store
            .append_launch(&json!({"at": at(2), "item": item, "issue": key, "step": "implement", "role": "executor", "cli": "claude", "attempt": 1}))
            .unwrap();
        store
            .append_launch(
                &json!({"at": at(3), "item": item, "ended": true, "attempt": 1, "failure": secret}),
            )
            .unwrap();
        let out = store.output_path(&item);
        std::fs::write(&out, json!({"status": "completed", "taskName": "1", "changeSummary": secret, "requiresTestReview": false}).to_string()).unwrap();
        let mut filed = support::item(store, &item, Step::Implement, Some(key));
        filed.front.status = Status::Done;
        filed.body = format!("Carry out task 1.\n\n## Result\n\nDone with {secret}.\n");
        store.file_item(&item, &filed).unwrap();
        let decision = DecisionRecord {
            id: format!("{key}-merge-1"),
            kind: DecisionKind::Merge,
            issue: Some(key.into()),
            sprint: None,
            sha: Some("abcdef1".into()),
            categories: vec![],
            title: "Merge?".into(),
            text: secret.into(),
            status: DecisionStatus::Answered,
            message: None,
            answer: Some(Answer {
                kind: AnswerKind::Approve,
                text: Some(secret.into()),
                user: "U0123456789".into(),
                at: at(4),
            }),
            procedure: None,
            asked_by: None,
            proposal: None,
        };
        store
            .write_isolated(&format!("decisions/{}.json", decision.id), &decision)
            .unwrap();
        let approval = ApprovalRecord {
            decision: decision.id.clone(),
            kind: DecisionKind::Merge,
            issue: Some(key.into()),
            sha: Some("abcdef1".into()),
            categories: vec![],
            user: secret.into(),
            at: at(5),
            permalink: Some("https://acme.slack.test/archives/C0123456789/p1".into()),
        };
        store
            .write_isolated(&format!("approvals/{}.json", decision.id), &approval)
            .unwrap();
    }
}
