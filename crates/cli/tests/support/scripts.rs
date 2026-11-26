//! Scripts and lookups the issue tests share.

use ovenspring_core::flow::Stage;
use ovenspring_integrations::github::{BranchRules, CheckState};
use serde_json::{Value, json};

use super::Env;
use super::fake_github::FakeGitHub;

/// An executor's run that changes `path` and reports the change as task-executor does.
pub fn implement_run(path: &str) -> Value {
    json!({
        "change": {"path": path, "content": "export\n"},
        "output": {"status": "completed", "taskName": "export", "changeSummary": format!("Change {path}"),
                   "testsAdded": [], "requiresTestReview": false, "newTestsPassed": true}
    })
}

/// The requirements output of an issue of `scale`.
pub fn requirements(key: &str, scale: &str) -> Value {
    json!({"output": {
        "scopeEvidence": {"affectedFiles": [], "affectedLayers": ["backend"], "executionRoute": {"status": "evident"}},
        "costEvidence": {"drivers": [], "unknowns": []},
        "convergence": {"outcome": format!("{key} works"), "requirements": [], "nonGoals": []},
        "structuralScale": scale, "decision": "proceed"
    }})
}

/// A quality run that passes.
pub fn quality_pass() -> Value {
    json!({"output": {"status": "pass", "summary": "All checks passed.", "checksPerformed": {}, "fixesApplied": []}})
}

/// A code review that passes.
pub fn code_pass() -> Value {
    json!({"output": {"verdict": "pass", "findings": [], "limitations": []}, "verdict": "pass"})
}

/// A security review that passes.
pub fn security_pass() -> Value {
    json!({"output": {"status": "pass", "summary": "No risk found.", "findings": []}, "verdict": "pass"})
}

/// A document review that passes.
pub fn document_pass() -> Value {
    json!({"output": {"metadata": {"doc_type": "DesignDoc"}, "verdict": {"decision": "pass"}, "issues": []}, "verdict": "pass"})
}

/// An author's run that writes `path` in the worktree and reports it.
pub fn document_run(path: &str, kind: &str) -> Value {
    json!({
        "change": {"path": path, "content": format!("# {kind}\n")},
        "output": {"status": "completed", "documentType": kind, "path": path}
    })
}

/// A task file as task-decomposer writes it.
pub fn task_file(id: &str, deps: &str) -> String {
    format!(
        "# Task: {id}\n\nMetadata:\n- Source Work Plan Task: {id}\n- Dependencies: {deps}\n\n## Implementation Outcome\n\nBuild {id}.\n"
    )
}

/// Script entries (`step@key`) for a small issue that passes every step: requirements, one
/// implementation, the quality run and both reviews.
pub fn straight(key: &str) -> serde_json::Map<String, Value> {
    let entries = [
        ("requirements", json!([requirements(key, "small")])),
        (
            "implement",
            json!([implement_run(&format!("src/{key}.ts"))]),
        ),
        ("quality", json!([quality_pass()])),
        ("code-review", json!([code_pass()])),
        ("security-review", json!([security_pass()])),
    ];
    entries
        .into_iter()
        .map(|(step, runs)| (format!("{step}@{key}"), runs))
        .collect()
}

/// Script entries (`step@key`) for a medium issue that passes every step, with a Design Doc that
/// declares `approvals` and one task per `(id, dependencies)`.
pub fn designed(
    key: &str,
    approvals: &[&str],
    tasks: &[(&str, &str)],
) -> serde_json::Map<String, Value> {
    let doc = format!("docs/design/{key}.md");
    let files: Vec<Value> = tasks
        .iter()
        .map(|(id, deps)| json!({"path": format!("$STATE/plans/tasks/{key}-task-{id}.md"), "content": task_file(id, deps)}))
        .collect();
    let paths: Vec<String> = tasks
        .iter()
        .map(|(id, _)| format!("$STATE/plans/tasks/{key}-task-{id}.md"))
        .collect();
    let implement: Vec<Value> = tasks
        .iter()
        .map(|(id, _)| implement_run(&format!("src/{key}-{id}.ts")))
        .collect();
    let entries = [
        ("requirements", json!([requirements(key, "medium")])),
        (
            "codebase-analysis",
            json!([{"output": {"analysisScope": {}, "focusAreas": []}}]),
        ),
        (
            "scope-check",
            json!([{"output": {"decision": "proceed", "structuralScale": "medium", "uiSpec": false, "adrDecisionPoints": []}}]),
        ),
        (
            "design",
            json!([{
                "change": {"path": doc, "content": format!("# {key}\n")},
                "output": {"status": "completed", "documentType": "DesignDoc", "path": doc, "approvals": approvals}
            }]),
        ),
        ("design-review", json!([document_pass()])),
        (
            "design-sync",
            json!([{"output": {"sync_status": "SKIPPED"}, "verdict": "pass"}]),
        ),
        (
            "test-design",
            json!([{"output": {"status": "completed", "feature": key, "generatedFiles": {"integration": null, "fixtureE2e": null, "serviceE2e": null}}}]),
        ),
        (
            "work-plan",
            json!([{"files": [{"path": format!("$STATE/plans/{key}.md"), "content": "# Work Plan\n"}],
                               "output": {"status": "completed", "documentType": "WorkPlan", "path": format!("$STATE/plans/{key}.md")}}]),
        ),
        ("plan-review", json!([document_pass()])),
        (
            "decompose",
            json!([{"files": files, "output": {"status": "completed", "paths": paths}}]),
        ),
        ("implement", Value::Array(implement)),
        ("quality", json!([quality_pass()])),
        ("code-review", json!([code_pass()])),
        ("security-review", json!([security_pass()])),
    ];
    entries
        .into_iter()
        .map(|(step, runs)| (format!("{step}@{key}"), runs))
        .collect()
}

/// Branch rules that require the `test` check and nothing else.
pub fn requiring_test() -> BranchRules {
    BranchRules {
        required_checks: ["test".to_owned()].into(),
    }
}

pub fn always_passing(env: &Env, rules: BranchRules) -> FakeGitHub {
    FakeGitHub::new(
        env.origin.clone(),
        rules,
        Box::new(|_| vec![("test".to_owned(), CheckState::Success)]),
    )
}

/// The issue's stage, `queued` before it begins.
pub fn stage(d: &ovenspring_daemon::Daemon, key: &str) -> Stage {
    d.issue(key)
        .unwrap()
        .map_or(Stage::Queued, |r| r.flow.stage)
}

/// The steps filed for an issue, in order.
pub fn filed_steps(env: &Env, issue: &str) -> Vec<String> {
    env.events()
        .iter()
        .filter(|e| e["event"] == "filed" && e["issue"] == issue && e["turn"] != "prepare")
        .map(|e| e["step"].as_str().unwrap().to_owned())
        .collect()
}
