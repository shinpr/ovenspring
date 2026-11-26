//! Every step's output, as `ovenspring item set` and the daemon read it: the outputs a step may
//! leave parse, the ones that break a rule of its shape are refused, and every step file has an
//! output type and names skills that exist.
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

use std::collections::BTreeSet;
use std::path::PathBuf;

use ovenspring_core::output::StepOutput;
use ovenspring_core::step::Step;
use serde_json::{Value, json};

fn factory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../factory")
}

type Samples = Vec<(&'static str, Vec<Value>, Vec<Value>)>;

/// For each step: outputs it must accept (minimal and full) and outputs it must refuse.
fn samples() -> Samples {
    let mut all = issue_samples();
    all.extend(sprint_samples());
    all
}

fn sprint_samples() -> Samples {
    vec![
        (
            "sprint-plan",
            vec![
                json!({"purpose": "p", "key_results": [], "issues": [], "message": "m"}),
                json!({"purpose": "p", "key_results": ["k"], "issues": [{"key": "ACME-1"}, {"key": "ACME-2", "depends_on": ["ACME-1"]}], "message": "m"}),
                // A dependency outside the plan holds nothing up.
                json!({"purpose": "p", "key_results": ["k"], "issues": [{"key": "ACME-2", "depends_on": ["ACME-9"]}], "message": "m"}),
            ],
            vec![
                json!({"purpose": "p", "key_results": [], "issues": [{"depends_on": []}], "message": "m"}),
                json!({"purpose": "p", "key_results": ["k"], "issues": [{"key": "ACME-1", "depends_on": ["ACME-2"]}, {"key": "ACME-2", "depends_on": ["ACME-1"]}], "message": "m"}),
                json!({"purpose": "p", "key_results": ["k"], "issues": [{"key": "ACME-1", "depends_on": ["ACME-1"]}], "message": "m"}),
                json!({"purpose": "p", "key_results": ["k"], "issues": [{"key": "ACME-1"}, {"key": "ACME-1"}], "message": "m"}),
            ],
        ),
        (
            "facts",
            vec![json!({"facts_path": "/s/sprints/1-facts.md"})],
            vec![json!({})],
        ),
        (
            "sprint-review",
            vec![
                json!({"key_results": [], "message": "m"}),
                json!({"key_results": [{"text": "k", "met": false, "evidence": "e"}],
                       "agreements": [{"file": "common.md", "content": "- rule\n"}], "message": "m"}),
                json!({"key_results": [], "message": "m",
                       "knowledge": [{"file": "billing/refunds-settle-next-day.md", "content": "# Refunds settle the next day\n"},
                                     {"file": "stale.md", "content": ""}]}),
            ],
            vec![
                json!({"key_results": [], "agreements": [{"file": "../config.toml", "content": "x"}], "message": "m"}),
                json!({"key_results": [], "message": "m", "knowledge": [{"file": "../AGENTS.md", "content": "x"}]}),
                json!({"key_results": [], "message": "m", "knowledge": [{"file": "/etc/notes.md", "content": "x"}]}),
                json!({"key_results": [], "message": "m", "knowledge": [{"file": "notes.txt", "content": "x"}]}),
            ],
        ),
        (
            "agreements-check",
            vec![json!({"findings": []})],
            vec![json!({})],
        ),
        (
            "conversation",
            vec![json!({"reply": "r"})],
            vec![json!({"reply": ""})],
        ),
        (
            "run-commands",
            vec![
                json!({"runs": "commands", "start": "pnpm dev --port $PORT", "reason": "r"}),
                json!({"runs": "commands", "setup": "pnpm i", "start": "pnpm dev", "ready": "http://localhost:$PORT/health", "reason": "r"}),
                json!({"runs": "nothing", "reason": "r"}),
                json!({"runs": "unknown", "reason": "r"}),
            ],
            vec![
                json!({"runs": "commands", "setup": "pnpm i", "reason": "r"}),
                json!({"runs": "nothing", "start": "pnpm dev", "reason": "r"}),
                json!({"runs": "unknown", "ready": "http://localhost:$PORT", "reason": "r"}),
                json!({"runs": "commands", "start": "pnpm dev", "reason": " "}),
            ],
        ),
    ]
}

fn issue_samples() -> Samples {
    let mut all = spec_samples();
    all.extend(design_samples());
    all.extend(code_samples());
    all
}

fn document(kind: &str) -> (Vec<Value>, Vec<Value>) {
    (
        vec![
            json!({"status": "completed", "documentType": kind, "path": "docs/a.md"}),
            json!({"status": "blocked", "reason": "the issue contradicts AC-3"}),
        ],
        vec![
            json!({"status": "completed"}),
            json!({"status": "blocked"}),
            json!({"status": "done", "path": "docs/a.md"}),
        ],
    )
}

fn document_review() -> (Vec<Value>, Vec<Value>) {
    (
        vec![
            json!({"metadata": {"doc_type": "DesignDoc"}, "verdict": {"decision": "pass"}, "issues": []}),
            json!({"verdict": {"decision": "needs_revision"}, "issues": [{"id": "I001", "correction": "c"}]}),
            json!({"verdict": {"decision": "pass"}, "prior_feedback_reconciliation":
                   [{"id": "I001", "prior_disposition": "apply", "status": "resolved", "evidence": "e"}]}),
        ],
        vec![
            json!({"issues": []}),
            json!({"verdict": {"decision": "approved"}}),
        ],
    )
}

/// The steps before any code: requirements, documents and their checks, planning.
fn spec_samples() -> Samples {
    let document_review = document_review();
    vec![
        (
            "requirements",
            vec![
                json!({"scopeEvidence": {}, "costEvidence": {}, "convergence": {"outcome": "o"},
                       "structuralScale": "small", "decision": "proceed"}),
                json!({"convergence": {"outcome": "o"}, "structuralScale": "unresolved",
                       "decision": "close", "evidence": "src/a.ts:3 already does it"}),
                json!({"convergence": {"outcome": "o"}, "structuralScale": "medium", "decision": "ask", "question": "q"}),
            ],
            vec![
                json!({"convergence": {"outcome": "o"}, "structuralScale": "small", "decision": "close"}),
                json!({"convergence": {"outcome": "o"}, "structuralScale": "small", "decision": "ask"}),
                json!({"convergence": {"outcome": ""}, "structuralScale": "small", "decision": "proceed"}),
                json!({"decision": "proceed", "outcome": "o"}),
            ],
        ),
        (
            "scope-check",
            vec![
                json!({"decision": "proceed", "structuralScale": "medium", "uiSpec": true,
                       "adrDecisionPoints": [{"question": "q", "answer": "a"}]}),
                json!({"decision": "ask", "structuralScale": "medium", "question": "q"}),
            ],
            vec![
                json!({"decision": "proceed", "structuralScale": "unresolved"}),
                json!({"decision": "close", "structuralScale": "small"}),
            ],
        ),
        (
            "prd",
            {
                let (mut good, _) = document("PRD");
                good.push(json!({"status": "completed", "question": "Should archived orders be exported?"}));
                good
            },
            document("PRD").1,
        ),
        (
            "prd-review",
            document_review.0.clone(),
            document_review.1.clone(),
        ),
        (
            "codebase-analysis",
            vec![json!({}), json!({"analysisScope": {}, "focusAreas": []})],
            vec![json!([])],
        ),
        (
            "prototype",
            vec![
                json!({"path": "docs/ui-spec/assets/orders/prototype.html", "referenceStrength": "reference", "basis": "no design tokens in the repository"}),
            ],
            vec![json!({})],
        ),
        ("ui-analysis", vec![json!({})], vec![json!("text")]),
        ("ui-spec", document("UISpec").0, document("UISpec").1),
        (
            "ui-spec-review",
            document_review.0.clone(),
            document_review.1.clone(),
        ),
        (
            "adr-review",
            document_review.0.clone(),
            document_review.1.clone(),
        ),
        (
            "adr",
            vec![
                json!({"status": "completed", "documentType": "ADRBatch", "paths": ["docs/adr/ADR-0008-x.md"]}),
            ],
            document("ADRBatch").1,
        ),
    ]
}

/// The Design Doc, its checks, and the planning that follows it.
fn design_samples() -> Samples {
    let document_review = document_review();
    vec![
        (
            "design",
            vec![
                json!({"status": "completed", "documentType": "DesignDoc", "path": "docs/design/a.md"}),
                json!({"status": "completed", "documentType": "DesignDoc", "path": "docs/design/a.md",
                       "approvals": ["schema", "external-connection", "infrastructure", "ci", "external-contract"]}),
                // A name of the owner's own: a kind of change a person reviews.
                json!({"status": "completed", "documentType": "DesignDoc", "path": "docs/design/a.md",
                       "approvals": ["payments"]}),
                json!({"status": "completed", "path": "docs/design/a.md", "dispositions": [
                    {"id": "I001", "disposition": "apply", "reason": "AC-2 has no row in the table"},
                    {"id": "I002", "disposition": "decline", "reason": "the section it asks for repeats AC-3"}
                ]}),
            ],
            vec![
                json!({"status": "completed", "path": "docs/design/a.md", "dispositions": [{"id": "I001", "disposition": "apply", "reason": ""}]}),
                json!({"status": "completed", "path": "docs/design/a.md", "approvals": [""]}),
                json!({"status": "completed", "path": "docs/design/a.md", "approvals": ["schema", "schema"]}),
                json!({"status": "completed", "path": "docs/design/a.md", "approvals": ["factory-configuration"]}),
                json!({"status": "completed", "path": ""}),
            ],
        ),
        (
            "design-verify",
            vec![
                json!({"summary": {"docType": "design-doc", "status": "consistent"}, "discrepancies": [], "limitations": []}),
                json!({"summary": {"status": "needs_review"}, "discrepancies": [{"id": "D001", "status": "drift"}]}),
            ],
            vec![json!({"summary": {"status": "fine"}})],
        ),
        (
            "design-review",
            document_review.0.clone(),
            document_review.1.clone(),
        ),
        (
            "design-sync",
            vec![
                json!({"sync_status": "SKIPPED"}),
                json!({"sync_status": "CONFLICTS_FOUND", "total_conflicts": 2}),
            ],
            vec![json!({"sync_status": "skipped"})],
        ),
        (
            "test-design",
            vec![
                json!({"status": "completed", "feature": "f", "generatedFiles": {"integration": "a.int.test.ts", "fixtureE2e": null, "serviceE2e": null}}),
                json!({"status": "blocked", "reason": "r"}),
            ],
            vec![json!({"generatedFiles": {}})],
        ),
        ("work-plan", document("WorkPlan").0, document("WorkPlan").1),
        ("plan-review", document_review.0, document_review.1),
        (
            "decompose",
            vec![json!({"status": "completed", "paths": ["/s/plans/tasks/a-task-01.md"]})],
            vec![json!({"status": "completed", "paths": []})],
        ),
    ]
}

/// The steps from implementation to the pull request.
fn code_samples() -> Samples {
    let executor = (
        vec![
            json!({"status": "completed", "taskName": "t", "changeSummary": "c", "requiresTestReview": true}),
            json!({"status": "escalation_needed", "reason": "r", "requiredDecision": "d"}),
            json!({"status": "completed", "changeSummary": "c", "requiresTestReview": false, "dispositions": [
                {"id": "F001", "review": "code-review/1", "disposition": "apply", "reason": "a.ts:1 breaks AC-1"},
                {"id": "F002", "review": "code-review/1", "disposition": "apply", "owner": "author", "reason": "the Design Doc names five columns; AC-1 six"},
                {"id": "S001", "review": "security-review", "disposition": "decline", "reason": "optional hardening"}
            ]}),
        ],
        vec![
            json!({"taskName": "t"}),
            json!({"status": "done"}),
            json!({"status": "completed", "dispositions": [{"id": "F001", "disposition": "decline"}]}),
            json!({"status": "completed", "dispositions": [{"id": "F001", "review": "code-review/0", "disposition": "apply", "reason": "r"}]}),
        ],
    );
    let quality = (
        vec![
            json!({"status": "pass", "summary": "s", "checksPerformed": {}, "fixesApplied": []}),
            json!({"status": "stub_detected", "incompleteImplementations": [{"file": "a.ts", "location": "f", "description": "d"}]}),
            json!({"status": "verification_incomplete", "missingPrerequisites": [{"type": "library", "description": "d", "affectedTests": ["t"]}]}),
        ],
        vec![json!({"status": "passed"})],
    );
    let code_review = (
        vec![
            json!({"verdict": "pass", "findings": [], "limitations": []}),
            json!({"verdict": "needs-improvement", "findings": [{"id": "F001", "location": "a.ts:1"}]}),
        ],
        vec![json!({"findings": []}), json!({"verdict": "findings"})],
    );
    vec![
        ("implement", executor.0.clone(), executor.1.clone()),
        (
            "test-review",
            vec![
                json!({"status": "pass", "testFiles": [], "reviewBasis": "skeleton", "qualityIssues": []}),
                json!({"status": "needs_revision", "qualityIssues": [{"id": "T001"}]}),
            ],
            vec![json!({"status": "ok"})],
        ),
        ("quality", quality.0.clone(), quality.1.clone()),
        ("code-review", code_review.0.clone(), code_review.1.clone()),
        (
            "security-review",
            vec![
                json!({"status": "pass", "summary": "s", "findings": []}),
                json!({"status": "needs_revision", "summary": "s", "findings": [{"id": "S001"}]}),
            ],
            vec![json!({"status": "needs-improvement"})],
        ),
        (
            "rerun-check",
            vec![
                json!({"rerun": []}),
                json!({"rerun": [{"review": "security-review", "implementationFiles": ["a.ts"]}]}),
            ],
            vec![json!({"rerun": [{"review": "lint"}]})],
        ),
        ("review-response", executor.0, executor.1),
        ("fix-ci", quality.0.clone(), quality.1.clone()),
        ("merge-base", quality.0, quality.1),
        (
            "exception",
            vec![
                json!({"action": "retry", "note": "n"}),
                json!({"action": "ask", "note": "n", "question": "q"}),
                json!({"action": "recover", "note": "n", "procedure": "git push -f", "summary": "s"}),
            ],
            vec![
                json!({"action": "ask", "note": "n"}),
                json!({"action": "recover", "note": "n", "summary": "s"}),
                json!({"action": "recover", "note": "n", "procedure": "git push -f"}),
            ],
        ),
    ]
}

/// Both kinds of verification limit quality's prompt reports reach the commit.
#[test]
fn a_quality_result_keeps_each_verification_limit_its_prompt_reports() {
    let output = StepOutput::parse(
        Step::Quality,
        json!({
            "status": "verification_incomplete",
            "missingPrerequisites": [{"type": "running_service", "description": "No test database",
                "affectedTests": ["e2e/orders"], "resolutionSteps": []}],
            "separateResponsibilityFailures": [{"command": "pnpm lint", "file": "src/legacy.ts",
                "evidence": "lint fails on src/legacy.ts:4", "owningBoundary": "the legacy module"}]
        }),
    )
    .unwrap();
    let StepOutput::Quality(q) = output else {
        panic!("{output:?}");
    };
    assert_eq!(
        q.limitations(),
        [
            ("No test database".to_owned(), "e2e/orders".to_owned()),
            (
                "lint fails on src/legacy.ts:4 (owned by the legacy module)".to_owned(),
                "pnpm lint".to_owned()
            ),
        ]
    );
}

/// An implementation result that does not say whether it touched integration or E2E tests is
/// sent to the test review.
#[test]
fn an_implementation_that_does_not_say_whether_it_needs_a_test_review_gets_one() {
    let output = StepOutput::parse(Step::Implement, json!({"status": "completed"})).unwrap();
    let StepOutput::Executor(e) = output else {
        panic!("{output:?}");
    };
    assert!(e.requires_test_review);
}

#[test]
fn each_step_accepts_its_outputs_and_refuses_those_that_break_its_rules() {
    for (step, good, bad) in samples() {
        for value in good {
            StepOutput::parse(step.parse().unwrap(), value.clone())
                .unwrap_or_else(|e| panic!("{step} refused {value}: {e}"));
        }
        for value in bad {
            assert!(
                StepOutput::parse(step.parse().unwrap(), value.clone()).is_err(),
                "{step} accepted {value}"
            );
        }
    }
}

#[test]
fn every_step_file_has_samples_and_names_skills_that_exist() {
    let files: BTreeSet<String> = std::fs::read_dir(factory().join("steps"))
        .unwrap()
        .filter_map(|e| {
            let p = e.unwrap().path();
            (p.extension()? == "md").then(|| p.file_stem().unwrap().to_string_lossy().into_owned())
        })
        .collect();

    let sampled: BTreeSet<String> = samples().iter().map(|(s, _, _)| (*s).to_owned()).collect();
    let named: BTreeSet<String> = Step::ALL.iter().map(Step::to_string).collect();
    assert_eq!(
        sampled, named,
        "every step the factory files needs samples here, and every sample a step"
    );
    let texts: BTreeSet<String> = Step::ALL
        .iter()
        .map(|s| s.text_file().to_string())
        .collect();
    assert_eq!(
        files, texts,
        "every step's text needs its file, and every step file is a step's text"
    );

    for step in &files {
        let text = std::fs::read_to_string(factory().join(format!("steps/{step}.md"))).unwrap();
        assert!(
            text.contains("## Required skills\n\nLoad and read "),
            "steps/{step}.md has no required skills"
        );
        for (skill, reference) in skill_mentions(&text) {
            let dir = factory().join(format!("skills/{skill}"));
            assert!(
                dir.join("SKILL.md").is_file(),
                "steps/{step}.md names missing skill {skill}"
            );
            if let Some(r) = reference {
                assert!(
                    dir.join(&r).is_file(),
                    "steps/{step}.md names missing {r} in {skill}"
                );
            }
        }
    }
}

/// Every `` `ovenspring-<name>` `` a text names, with the `` `references/<file>` `` read in it
/// when the text says so (`` `references/x.md` in `ovenspring-y` ``).
fn skill_mentions(text: &str) -> Vec<(String, Option<String>)> {
    let tick = |s: &str| {
        s.split('`')
            .skip(1)
            .step_by(2)
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let mut out = Vec::new();
    for line in text.lines() {
        let quoted = tick(line);
        for (n, q) in quoted.iter().enumerate() {
            if q.starts_with("ovenspring-") {
                out.push((q.clone(), None));
            }
            if q.starts_with("references/")
                && let Some(skill) = quoted.get(n + 1).filter(|s| s.starts_with("ovenspring-"))
            {
                out.push((skill.clone(), Some(q.clone())));
            }
        }
    }
    out
}

/// The document reviews share their source agent's gate, review order and resolution sections;
/// a correction to one copy only would leave them following different rules.
#[test]
fn document_reviews_keep_their_source_agents_shared_sections_alike() {
    let reviews = [
        "prd-review",
        "ui-spec-review",
        "adr-review",
        "design-review",
        "plan-review",
    ];
    let shared = [
        "Execution Gate",
        "Review Order and Boundary",
        "Findings and Rechecks",
    ];
    let first = paragraphs(reviews[0], &shared);
    for review in &reviews[1..] {
        let missing: Vec<String> = first
            .difference(&paragraphs(review, &shared))
            .cloned()
            .collect();
        assert!(
            missing.is_empty(),
            "steps/{review}.md lacks what steps/{}.md says: {missing:#?}",
            reviews[0]
        );
    }
}

/// A step file's paragraphs in the `shared` sections, apart from `## In the factory`'s bullets.
fn paragraphs(step: &str, shared: &[&str]) -> BTreeSet<String> {
    let text = std::fs::read_to_string(factory().join(format!("steps/{step}.md"))).unwrap();
    let mut out = BTreeSet::new();
    let mut section = "";
    let mut kept = String::new();
    for line in text.lines().skip(1) {
        if let Some(heading) = line.strip_prefix("## ") {
            section = heading;
            continue;
        }
        // `## In the factory` holds the factory's bullets; the source's text may follow them
        // before the next heading.
        let factory_bullet = section == "In the factory" && line.starts_with("- ");
        let keep = !factory_bullet && shared.contains(&section);
        if keep {
            kept.push_str(line);
        }
        kept.push('\n');
    }
    for paragraph in kept.split("\n\n") {
        let paragraph = paragraph.trim();
        if !paragraph.is_empty() {
            out.insert(paragraph.to_owned());
        }
    }
    out
}
