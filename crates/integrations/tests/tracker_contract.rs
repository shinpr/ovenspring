//! The tracker adapters against recorded Linear and GitHub responses: open issues with their
//! blockers, comments, and state changes.
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        reason = "tests fail loudly"
    )
)]

mod common;

use common::Recorded;
use ovenspring_integrations::github::{GitHubApi, RepoId};
use ovenspring_integrations::http::{HttpError, Method, Unanswered};
use ovenspring_integrations::tracker::linear::Linear;
use ovenspring_integrations::tracker::{Tracker, TrackerComment, TrackerState};
use serde_json::json;

const LINEAR: &str = "https://linear.test/graphql";
const REPO: &str = "https://api.test/repos/acme/storefront";

fn linear(t: &Recorded) -> Linear<&Recorded> {
    Linear::with_api(t, LINEAR, "lin_api_fixture0001".into(), "ACME".into())
}

fn linear_issue(key: &str, blockers: &serde_json::Value) -> serde_json::Value {
    json!({
        "identifier": key, "title": format!("Title of {key}"), "description": "As written.",
        "url": format!("https://linear.app/acme/issue/{key}"), "priority": 2,
        "creator": {"name": format!("Creator of {key}")},
        "state": {"name": "Todo", "type": "unstarted"},
        "labels": {"nodes": [{"name": "Feature"}]},
        "comments": {"nodes": [{"body": "Also on mobile.", "user": {"name": "Dana"}}]},
        "inverseRelations": {"nodes": blockers}
    })
}

#[test]
fn linear_reads_open_issues_across_pages_with_their_open_blockers() {
    let mut second = linear_issue(
        "ACME-2",
        &json!([
            {"type": "blocks", "issue": {"identifier": "ACME-1", "state": {"type": "unstarted"}}},
            {"type": "blocks", "issue": {"identifier": "ACME-0", "state": {"type": "completed"}}},
            {"type": "related", "issue": {"identifier": "ACME-9", "state": {"type": "started"}}}
        ]),
    );
    second["creator"] = serde_json::Value::Null;
    second["state"] = json!({"name": "In Progress", "type": "started"});
    let t = Recorded::new()
        .on_body(
            LINEAR,
            "\"after\":null",
            &json!({"data": {"issues": {
                "nodes": [linear_issue("ACME-1", &json!([]))],
                "pageInfo": {"hasNextPage": true, "endCursor": "c1"}
            }}}),
        )
        .on_body(
            LINEAR,
            "\"after\":\"c1\"",
            &json!({"data": {"issues": {
                "nodes": [second],
                "pageInfo": {"hasNextPage": false, "endCursor": "c2"}
            }}}),
        );
    let issues = linear(&t).open_issues().unwrap();
    assert_eq!(issues.len(), 2);
    assert_eq!(issues[0].key, "ACME-1");
    assert_eq!(issues[0].author, "Creator of ACME-1");
    assert_eq!(issues[1].author, "");
    assert_eq!(
        (
            issues[0].started,
            issues[1].started,
            issues[1].state.as_str()
        ),
        (false, true, "In Progress"),
        "a started issue is marked as one someone already works on"
    );
    assert_eq!(issues[0].depends_on, [] as [String; 0]);
    assert_eq!(
        issues[1].depends_on,
        ["ACME-1"],
        "only an open `blocks` relation is a dependency"
    );
    assert_eq!(issues[1].labels, ["Feature"]);
    assert_eq!(
        issues[1].comments,
        [TrackerComment {
            author: "Dana".into(),
            body: "Also on mobile.".into()
        }]
    );

    let sent = t.sent();
    assert_eq!(
        sent[0].headers,
        [("authorization".to_owned(), "lin_api_fixture0001".to_owned())]
    );
    let query = sent[0].body.as_ref().unwrap()["query"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(query.contains(r#"in: ["unstarted", "started"]"#));
    assert!(query.contains("creator { name }"));
    assert_eq!(sent[0].body.as_ref().unwrap()["variables"]["team"], "ACME");

    let rendered = issues[1].render();
    assert!(
        rendered.contains("# ACME-2: Title of ACME-2")
            && rendered.contains("- Blocked by: ACME-1")
            && rendered.contains("As written.")
    );
}

#[test]
fn linear_moves_an_issue_to_the_teams_first_state_of_the_kind_and_comments() {
    let t = Recorded::new()
        .on_body(
            LINEAR,
            "query States",
            &json!({"data": {"workflowStates": {"nodes": [
                {"id": "st-done-late", "position": 3.0}, {"id": "st-done", "position": 1.0}
            ]}}}),
        )
        .on_body(
            LINEAR,
            "query IssueId",
            &json!({"data": {"issue": {"id": "uuid-1"}}}),
        )
        .on_body(
            LINEAR,
            "mutation UpdateState",
            &json!({"data": {"issueUpdate": {"success": true}}}),
        )
        .on_body(
            LINEAR,
            "mutation Comment",
            &json!({"data": {"commentCreate": {"success": true}}}),
        );
    let l = linear(&t);
    l.set_state("ACME-1", TrackerState::Done).unwrap();
    l.comment("ACME-1", "Already done: see tests/export.test.ts.")
        .unwrap();
    let bodies: Vec<serde_json::Value> = t.sent().into_iter().map(|r| r.body.unwrap()).collect();
    let variables = |operation: &str| {
        bodies
            .iter()
            .find(|b| b["query"].as_str().unwrap().contains(operation))
            .unwrap()["variables"]
            .clone()
    };
    assert_eq!(
        variables("query States"),
        json!({"team": "ACME", "type": "completed"})
    );
    assert_eq!(
        variables("mutation UpdateState"),
        json!({"id": "uuid-1", "state": "st-done"})
    );
    assert_eq!(
        variables("mutation Comment"),
        json!({"id": "uuid-1", "body": "Already done: see tests/export.test.ts."})
    );
}

#[test]
fn linear_errors_are_reported_and_its_rate_limit_is_transient() {
    let t = Recorded::new().on_body(LINEAR, "OpenIssues", &json!({"errors": [{"message": "Rate limit exceeded", "extensions": {"code": "RATELIMITED"}}]}));
    assert!(matches!(
        linear(&t).open_issues(),
        Err(HttpError::Transient {
            why: Unanswered::RateLimited,
            ..
        })
    ));
    let t = Recorded::new().on_body(
        LINEAR,
        "OpenIssues",
        &json!({"errors": [{"message": "Team not found"}]}),
    );
    let err = linear(&t).open_issues().unwrap_err().to_string();
    assert!(err.contains("Team not found"), "{err}");
}

// Behavior: Linear answers an unknown team key with no team -> the team check names the key, so a
//     mistyped key is not taken for an empty backlog.
#[test]
fn linear_names_a_team_key_it_does_not_know() {
    let t = Recorded::new().on_body(
        LINEAR,
        "query Team",
        &json!({"data": {"teams": {"nodes": []}}}),
    );
    let err = linear(&t).team_name().unwrap_err().to_string();
    assert!(err.contains("no team with the key"), "{err}");
    let t = Recorded::new().on_body(
        LINEAR,
        "query Team",
        &json!({"data": {"teams": {"nodes": [{"name": "Storefront"}]}}}),
    );
    assert_eq!(linear(&t).team_name().unwrap(), "Storefront");
}

#[test]
fn linear_mutation_that_does_not_succeed_is_an_error() {
    let t = Recorded::new()
        .on_body(
            LINEAR,
            "query IssueId",
            &json!({"data": {"issue": {"id": "uuid-1"}}}),
        )
        .on_body(
            LINEAR,
            "mutation Comment",
            &json!({"data": {"commentCreate": {"success": false}}}),
        );
    let err = linear(&t)
        .comment("ACME-1", "Already done.")
        .unwrap_err()
        .to_string();
    assert!(err.contains("commentCreate did not succeed"), "{err}");
}

fn github(t: &Recorded) -> GitHubApi<&Recorded> {
    GitHubApi::with_base(
        t,
        "https://api.test",
        RepoId {
            owner: "acme".into(),
            name: "storefront".into(),
        },
        "tok".into(),
    )
}

#[test]
fn github_issues_reads_open_issues_without_pull_requests_and_with_open_blockers() {
    let t = Recorded::new()
        .on((Method::Get, &format!("{REPO}/issues?state=open&per_page=100")), 200, &json!([
            {"number": 12, "title": "Export", "body": "CSV please", "html_url": "https://github.com/acme/storefront/issues/12",
             "user": {"login": "octocat"}, "state": "open", "labels": [{"name": "feature"}], "comments": 1,
             "issue_dependencies_summary": {"blocked_by": 1, "blocking": 0}},
            {"number": 13, "title": "Import", "body": null, "html_url": "https://github.com/acme/storefront/issues/13",
             "state": "open", "labels": [], "comments": 0, "issue_dependencies_summary": {"blocked_by": 0, "blocking": 1}},
            {"number": 14, "title": "A pull request", "html_url": "https://github.com/acme/storefront/pull/14",
             "state": "open", "pull_request": {"url": "x"}}
        ]))
        .on((Method::Get, &format!("{REPO}/issues/12/comments?per_page=100")), 200, &json!([
            {"id": 1, "user": {"login": "dana"}, "body": "Also on mobile."}
        ]))
        .on((Method::Get, &format!("{REPO}/issues/12/dependencies/blocked_by")), 200, &json!([
            {"number": 13, "state": "open"}, {"number": 9, "state": "closed"}
        ]));
    let issues = github(&t).open_issues().unwrap();
    let keys: Vec<&str> = issues.iter().map(|i| i.key.as_str()).collect();
    assert_eq!(keys, ["GH-12", "GH-13"]);
    assert_eq!(issues[0].depends_on, ["GH-13"]);
    assert_eq!(issues[0].author, "octocat");
    assert_eq!(issues[1].author, "");
    assert_eq!(issues[0].comments[0].author, "dana");
    assert_eq!(issues[1].body, "");
    assert!(
        !t.sent().iter().any(|r| r.url.contains("/issues/13/")),
        "nothing asked about an issue with no comments or blockers"
    );
}

#[test]
fn github_issues_closes_as_completed_or_not_planned_and_comments() {
    let t = Recorded::new()
        .on(
            (Method::Patch, &format!("{REPO}/issues/12")),
            200,
            &json!({"number": 12}),
        )
        .on(
            (Method::Post, &format!("{REPO}/issues/12/comments")),
            201,
            &json!({"id": 5}),
        );
    let gh = github(&t);
    gh.set_state("GH-12", TrackerState::Done).unwrap();
    gh.set_state("GH-12", TrackerState::Canceled).unwrap();
    gh.comment("GH-12", "Done in #20.").unwrap();
    let bodies: Vec<serde_json::Value> = t.sent().into_iter().map(|r| r.body.unwrap()).collect();
    assert_eq!(
        bodies,
        [
            json!({"state": "closed", "state_reason": "completed"}),
            json!({"state": "closed", "state_reason": "not_planned"}),
            json!({"body": "Done in #20."})
        ]
    );
    assert!(
        gh.comment("ACME-1", "x").is_err(),
        "a key that is not GH-<n>"
    );
}

#[test]
fn linear_starts_an_issue_in_the_teams_first_started_state_assigned_to_the_key_owner() {
    let t = Recorded::new()
        .on_body(
            LINEAR,
            "query States",
            &json!({"data": {"workflowStates": {"nodes": [
                {"id": "st-review", "position": 2.0}, {"id": "st-progress", "position": 1.0}
            ]}}}),
        )
        .on_body(
            LINEAR,
            "query Viewer",
            &json!({"data": {"viewer": {"id": "user-1"}}}),
        )
        .on_body(
            LINEAR,
            "query IssueId",
            &json!({"data": {"issue": {"id": "uuid-1"}}}),
        )
        .on_body(
            LINEAR,
            "mutation Start",
            &json!({"data": {"issueUpdate": {"success": true}}}),
        );
    linear(&t).start("ACME-1").unwrap();
    let bodies: Vec<serde_json::Value> = t.sent().into_iter().map(|r| r.body.unwrap()).collect();
    assert_eq!(
        bodies[0]["variables"],
        json!({"team": "ACME", "type": "started"})
    );
    assert_eq!(
        bodies[3]["variables"],
        json!({"id": "uuid-1", "state": "st-progress", "assignee": "user-1"})
    );
}

#[test]
fn github_issues_starts_an_issue_by_assigning_the_token_owner() {
    let t = Recorded::new()
        .on(
            (Method::Get, "https://api.test/user"),
            200,
            &json!({"login": "shin"}),
        )
        .on(
            (Method::Post, &format!("{REPO}/issues/12/assignees")),
            201,
            &json!({"number": 12}),
        );
    github(&t).start("GH-12").unwrap();
    let sent = t.sent();
    assert_eq!(
        sent.last().unwrap().body,
        Some(json!({"assignees": ["shin"]}))
    );
}
