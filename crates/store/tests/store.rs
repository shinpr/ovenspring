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

use std::path::{Path, PathBuf};

use ovenspring_core::config::Role;
use ovenspring_core::item::{Front, Item, Status};
use ovenspring_store::{SetRequest, Store, StoreError};

fn file(store: &Store, name: &str, (to, step): (Role, &str), status: Status) -> PathBuf {
    let item = Item {
        front: Front {
            step: step.parse().unwrap(),
            to,
            status,
            issue: Some("ACME-142".into()),
            sprint: None,
            round: None,
            unit: None,
            refs: vec![],
            out: store.output_path(name).display().to_string(),
            prepare: false,
            resume: None,
        },
        body: "Carry out task 2 of the plan.\n\n## Result\nDone: the export streams rows.\n".into(),
    };
    store.file_item(name, &item).unwrap()
}

fn set(store: &Store, path: &Path, kv: &[(&str, &str)], launch: &str) -> Result<Item, StoreError> {
    let kv: Vec<(String, String)> = kv
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    store.agent_set(&SetRequest {
        path,
        assignments: &kv,
        launch: Some(launch),
    })
}

fn refused(r: Result<Item, StoreError>) -> String {
    match r {
        Err(StoreError::Refused(m)) => m,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn done_is_refused_until_a_valid_output_exists_then_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("state")).unwrap();
    let name = "20261121-120400-implement-ACME-142";
    let path = file(&store, name, (Role::Executor, "implement"), Status::Doing);

    let m = refused(set(&store, &path, &[("status", "done")], name));
    assert!(m.contains("write the step's output"), "{m}");

    std::fs::write(store.output_path(name), r#"{"taskName": "2"}"#).unwrap();
    let m = refused(set(&store, &path, &[("status", "done")], name));
    assert!(m.contains("is not a valid output"), "{m}");

    std::fs::write(
        store.output_path(name),
        r#"{"status": "completed", "taskName": "2", "changeSummary": "Stream the export rows", "requiresTestReview": false}"#,
    )
    .unwrap();
    let item = set(&store, &path, &[("status", "done")], name).unwrap();
    assert_eq!(item.front.status, Status::Done);
    assert!(
        store
            .read_item(&path)
            .unwrap()
            .body
            .ends_with("Done: the export streams rows.\n"),
        "the result is kept"
    );
}

#[test]
fn only_the_owning_launch_may_change_an_item() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("state")).unwrap();
    let path = file(
        &store,
        "a-implement-ACME-142",
        (Role::Executor, "implement"),
        Status::Doing,
    );
    let m = refused(set(
        &store,
        &path,
        &[("status", "blocked")],
        "b-implement-ACME-147",
    ));
    assert!(m.contains("only the launch that owns"), "{m}");
    assert_eq!(store.read_item(&path).unwrap().front.status, Status::Doing);
}

#[test]
fn an_item_that_is_not_doing_cannot_be_changed() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("state")).unwrap();
    let path = file(&store, "x", (Role::Executor, "implement"), Status::Open);
    assert!(refused(set(&store, &path, &[("status", "blocked")], "x")).contains("not doing"));
}

#[test]
fn blocked_needs_no_output() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("state")).unwrap();
    let path = file(&store, "x", (Role::Executor, "implement"), Status::Doing);
    assert_eq!(
        set(&store, &path, &[("status", "blocked")], "x")
            .unwrap()
            .front
            .status,
        Status::Blocked
    );
}

#[test]
fn agents_set_only_the_status_and_only_to_done_or_blocked() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("state")).unwrap();
    let path = file(&store, "x", (Role::Executor, "implement"), Status::Doing);
    assert!(refused(set(&store, &path, &[("status", "open")], "x")).contains("done or blocked"));
    assert!(refused(set(&store, &path, &[("issue", "ACME-1")], "x")).contains("cannot be set"));
    assert!(refused(set(&store, &path, &[("verdict", "pass")], "x")).contains("cannot be set"));
}

// Behavior: a preparation turn's item is checked against the preparation's output, not the
//     step's -> a question without its text and an empty message are refused, a valid one is
//     accepted.
#[test]
fn a_preparation_turn_is_done_only_with_a_valid_preparation_output() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("state")).unwrap();
    let name = "20261121-120400-design-ACME-142";
    let path = file(&store, name, (Role::Lead, "design"), Status::Doing);
    store
        .update_item(&path, |i| i.front.prepare = true)
        .unwrap();
    for (output, refusal) in [
        (r#"{"message": null, "question": true}"#, "no `message`"),
        (
            r#"{"message": " ", "question": false}"#,
            "`message` is empty",
        ),
    ] {
        std::fs::write(store.output_path(name), output).unwrap();
        let m = refused(set(&store, &path, &[("status", "done")], name));
        assert!(m.contains(refusal), "{m}");
    }
    std::fs::write(
        store.output_path(name),
        r#"{"message": "Which payment service?", "question": true}"#,
    )
    .unwrap();
    assert_eq!(
        set(&store, &path, &[("status", "done")], name)
            .unwrap()
            .front
            .status,
        Status::Done
    );
}

#[test]
fn records_kept_from_agents_live_in_the_isolated_directory_beside_the_state() {
    let base = tempfile::tempdir().unwrap();
    let repo = base.path().join("storefront");
    std::fs::create_dir_all(&repo).unwrap();
    let state = ovenspring_store::state_dir_in(base.path(), &repo);
    let store = Store::open(&state).unwrap();
    assert_eq!(store.isolated(), ovenspring_store::isolated_of(&state));
    assert_eq!(state.file_name().unwrap(), "state");
    assert_eq!(store.isolated().parent(), state.parent());

    // Opening the state, as an agent's own `ovenspring` call does, writes nothing there.
    assert!(!store.isolated().exists());
    assert_eq!(
        store.isolated_names("decisions").unwrap(),
        Vec::<String>::new()
    );

    store
        .write_isolated("decisions/d-1.json", &serde_json::json!({"id": "d-1"}))
        .unwrap();
    assert!(store.isolated().join("decisions/d-1.json").exists());
    assert!(!state.join("decisions").exists());
    assert_eq!(store.isolated_names("decisions").unwrap(), ["d-1"]);
    let read: Option<serde_json::Value> = store.read_isolated("decisions/d-1.json").unwrap();
    assert_eq!(read.unwrap()["id"], "d-1");
}

// Behavior: a new item is named while items of the same step and second sit on the board and in
//     the archive -> the name taken is one neither holds, so the new item never replaces an older
//     one or its output.
#[test]
fn a_new_item_name_is_one_no_item_on_the_board_or_in_the_archive_has() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("state")).unwrap();
    let base = "20261121-120400-sprint-plan-sprint-20261121-0905";
    let archived = file(&store, base, (Role::Lead, "sprint-plan"), Status::Done);
    store.archive_item(&archived).unwrap();
    file(
        &store,
        &format!("{base}-2"),
        (Role::Lead, "sprint-plan"),
        Status::Open,
    );

    assert_eq!(store.unused_item_name(base), format!("{base}-3"));
    assert_eq!(
        store.unused_item_name("20261121-120401-sprint-plan"),
        "20261121-120401-sprint-plan"
    );
}

// Behavior: a JSON Lines file ends in part of a line with no newline, as an interrupted write
//     leaves it, then a line is appended -> the part stays a line of its own and the appended
//     line is a whole line after it.
#[test]
fn an_appended_line_starts_its_own_line_after_a_part_left_with_no_newline() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("state")).unwrap();
    let path = store.root().join("events.jsonl");
    std::fs::write(&path, "{\"event\":\"first\"}\n{\"event\":\"cu").unwrap();

    store
        .append_line("events.jsonl", &serde_json::json!({"event": "after"}))
        .unwrap();

    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "{\"event\":\"first\"}\n{\"event\":\"cu\n{\"event\":\"after\"}\n"
    );
}
