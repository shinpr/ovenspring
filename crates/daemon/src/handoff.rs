//! What each issue step receives: the input entries its source agent declares, as `field: value`
//! lines extracted mechanically from the records, and the review handoffs the sources define:
//! `prior_feedback` for a check that rechecks, `correction_findings` for the author or executor
//! that corrects.

use std::path::Path;

use ovenspring_core::flow::{Filing, Origin};
use ovenspring_core::output::{
    CorrectionOwner, Disposition, FindingDisposition, PrototypeOutput, ResolutionOutput, Review,
    StepOutput, output_key,
};
use ovenspring_core::step::{Document, Step};
use serde_json::{Value, json};

use crate::issues::IssueRecord;

/// An input entry: a value written in place, or a JSON handoff written to a file the entry names.
pub(crate) enum Entry {
    Text(String),
    Json(Value),
}

/// The findings key of the pull request comments the lead judges.
pub(crate) const COMMENTS: &str = "comments";

/// The keys under which a check's source reports its findings.
pub(crate) const FINDING_KEYS: [&str; 4] = ["findings", "issues", "discrepancies", "qualityIssues"];

/// A check output's findings, when it reports new ones rather than a correction re-review.
pub(crate) fn findings_of(output: &Value) -> Option<Vec<Value>> {
    if output
        .get("prior_feedback_reconciliation")
        .and_then(Value::as_array)
        .is_some_and(|r| !r.is_empty())
    {
        return None;
    }
    FINDING_KEYS
        .iter()
        .find_map(|k| output.get(*k).and_then(Value::as_array))
        .filter(|f| !f.is_empty())
        .cloned()
}

fn text(v: impl Into<String>) -> Entry {
    Entry::Text(v.into())
}

/// The path an output or document is at, for an entry.
fn output(record: &IssueRecord, step: Step) -> Option<Entry> {
    record.outputs.get(step.as_str()).cloned().map(Entry::Text)
}

/// The prototype the owner approved: its HTML file and reference strength.
fn prototype(record: &IssueRecord) -> Option<PrototypeOutput> {
    match record.output(Step::Prototype) {
        Some(StepOutput::Prototype(p)) => Some(p),
        _ => None,
    }
}

fn document(record: &IssueRecord, step: Step) -> Option<String> {
    match record.output(step) {
        Some(StepOutput::Document(d)) => d
            .path
            .or_else(|| (!d.paths.is_empty()).then(|| d.paths.join(", "))),
        _ => None,
    }
}

/// The source the design and its reviews take the requirements from: the approved PRD, or the
/// requirements output with its convergence record.
fn requirement_context(record: &IssueRecord) -> Option<Entry> {
    if record.flow.documents.prd_approved {
        document(record, Step::Prd).map(text)
    } else {
        output(record, Step::Requirements)
    }
}

/// Dispositions of the latest resolution that belong to `key`'s findings.
fn dispositions_for(record: &IssueRecord, key: &str) -> Vec<FindingDisposition> {
    let Some(StepOutput::Resolution(r)) = record.output(Step::Resolution) else {
        return vec![];
    };
    r.dispositions
        .into_iter()
        .filter(|d| belongs_to(d, key))
        .collect()
}

/// Whether `d` judges a finding of the check kept under `key`. Code reviews of one round number
/// their findings alike, so a disposition that names its review belongs to that review alone.
fn belongs_to(d: &FindingDisposition, key: &str) -> bool {
    let review = Review::try_from(key.to_owned()).ok();
    review.is_none() || d.review.is_none() || d.review == review
}

/// The findings the running `resolution` was filed for that it leaves without a disposition.
pub(crate) fn undisposed(record: &IssueRecord, resolution: &ResolutionOutput) -> Vec<String> {
    let flow = &record.flow;
    let keys: Vec<String> = match flow.origin {
        Some(Origin::Round) => flow
            .round
            .findings
            .iter()
            .map(|r| String::from(*r))
            .collect(),
        Some(Origin::Document | Origin::TestReview) => flow
            .check
            .map(|c| output_key(c.step(), None))
            .into_iter()
            .collect(),
        Some(Origin::PrComments) => vec![COMMENTS.to_owned()],
        None => vec![],
    };
    keys.iter()
        .flat_map(|key| {
            record
                .findings
                .get(key)
                .into_iter()
                .flatten()
                .filter_map(|f| f.get("id").and_then(Value::as_str))
                .filter(|id| {
                    !resolution
                        .dispositions
                        .iter()
                        .any(|d| d.id == *id && belongs_to(d, key))
                })
                .map(move |id| format!("{key} {id}"))
        })
        .collect()
}

fn finding(record: &IssueRecord, key: &str, id: &str) -> Value {
    record
        .findings
        .get(key)
        .and_then(|f| {
            f.iter()
                .find(|x| x.get("id").and_then(Value::as_str) == Some(id))
        })
        .cloned()
        .unwrap_or_else(|| json!({"id": id}))
}

/// `prior_feedback` for a check that rechecks after a correction: one entry per disposition the
/// resolution recorded for its findings, the original finding carried in `evidence`.
fn prior_feedback(record: &IssueRecord, key: &str) -> Value {
    let entries: Vec<Value> = dispositions_for(record, key)
        .iter()
        .map(|d| {
            json!({
                "id": d.id,
                "disposition": d.disposition.as_str(),
                "reason": d.reason,
                "evidence": {
                    "finding": finding(record, key, &d.id),
                    "basis": d.basis,
                    "evidence": d.evidence,
                    "correction": correction_ref(record),
                },
            })
        })
        .collect();
    if [Step::DesignVerify, Step::DesignSync]
        .iter()
        .any(|s| s.as_str() == key)
    {
        // A bounded verifier receives its previous complete result with the dispositions.
        return json!({
            "previous_result": record.outputs.get(key),
            "dispositions": entries,
            "correction": correction_ref(record),
        });
    }
    Value::Array(entries)
}

fn correction_ref(record: &IssueRecord) -> String {
    match &record.last_commit {
        Some(sha) => format!("the changes since commit {sha}"),
        None => "the changes in the worktree".into(),
    }
}

/// The check whose applied findings a correction answers, by origin.
fn corrected_checks(record: &IssueRecord) -> Vec<String> {
    match record.flow.origin {
        Some(Origin::Round) => record
            .findings
            .keys()
            .filter(|k| Review::try_from((*k).clone()).is_ok())
            .cloned()
            .collect(),
        Some(Origin::PrComments) => vec![COMMENTS.to_owned()],
        _ => record
            .flow
            .check
            .iter()
            .map(|c| c.step().to_string())
            .collect(),
    }
}

/// `correction_findings` for `owner`: each applied finding as its check returned it, with its
/// disposition, and the reconciliation entry of one its check maintained.
fn correction_findings(record: &IssueRecord, owner: CorrectionOwner) -> Value {
    let mut out = Vec::new();
    for key in corrected_checks(record) {
        let maintained: Vec<Value> = record
            .raw_output(&key)
            .and_then(|o| o.get("prior_feedback_reconciliation").cloned())
            .and_then(|r| r.as_array().cloned())
            .unwrap_or_default();
        for d in dispositions_for(record, &key) {
            let mine = d.owner.unwrap_or(CorrectionOwner::Executor) == owner
                || record.flow.origin != Some(Origin::Round);
            if d.disposition != Disposition::Apply || !mine {
                continue;
            }
            let entry = maintained
                .iter()
                .find(|m| m.get("id").and_then(Value::as_str) == Some(d.id.as_str()))
                .cloned();
            if !maintained.is_empty()
                && entry
                    .as_ref()
                    .and_then(|e| e.get("status"))
                    .and_then(Value::as_str)
                    != Some("maintained")
            {
                continue;
            }
            let mut f = finding(record, &key, &d.id);
            if let Some(obj) = f.as_object_mut() {
                obj.insert("disposition".into(), json!("apply"));
                if let Some(e) = entry {
                    obj.insert("reconciliation".into(), e);
                }
            }
            out.push(f);
        }
    }
    Value::Array(out)
}

/// The work an executor's scope is: the task file, or a small issue's direct scope.
fn executor_scope(
    record: &IssueRecord,
    filing: &Filing,
    issue: &str,
) -> Vec<(&'static str, Entry)> {
    if let Some(task) = filing.task.and_then(|t| record.flow.tasks.get(t)) {
        return vec![("task_file", text(task.path.clone()))];
    }
    if record.flow.tasks.is_empty() {
        let mut v = vec![];
        if let Some(r) = output(record, Step::Requirements) {
            v.push(("direct_scope", r));
        }
        v.push(("governing_sources", text(issue.to_owned())));
        return v;
    }
    let mut sources: Vec<String> = document(record, Step::WorkPlan).into_iter().collect();
    sources.extend(record.flow.tasks.iter().map(|t| t.path.clone()));
    vec![("governing_sources", text(sources.join(", ")))]
}

/// The documents an implementation is reviewed against.
fn governing_documents(record: &IssueRecord, issue: &str) -> Entry {
    let docs: Vec<String> = [Step::Prd, Step::Design, Step::WorkPlan]
        .into_iter()
        .filter_map(|s| document(record, s))
        .collect();
    if docs.is_empty() {
        let mut v = vec![issue.to_owned()];
        v.extend(record.outputs.get(Step::Requirements.as_str()).cloned());
        return text(v.join(", "));
    }
    text(docs.join(", "))
}

/// What the input entries of one item are drawn from.
pub(crate) struct Ctx<'a> {
    pub record: &'a IssueRecord,
    pub filing: &'a Filing,
    /// The issue as written (its source file).
    pub issue: &'a str,
    pub base: &'a str,
    pub worktree: &'a Path,
}

type Entries = Vec<(&'static str, Option<Entry>)>;

impl Ctx<'_> {
    fn mode(&self) -> Entry {
        text(if self.record.flow.correcting() {
            "update"
        } else {
            "create"
        })
    }

    fn author_findings(&self) -> Option<Entry> {
        self.record
            .flow
            .correcting()
            .then(|| Entry::Json(correction_findings(self.record, CorrectionOwner::Author)))
    }

    /// document-reviewer's entries.
    fn review(&self, doc_type: &str, step: Step) -> Entries {
        let r = self.record;
        vec![
            ("doc_type", Some(text(doc_type))),
            ("target", document(r, step).map(text)),
            ("review_context", Some(text("creation"))),
            ("requirements_verbatim", Some(text(self.issue))),
            ("requirement_context", requirement_context(r)),
            ("codebase_analysis", output(r, Step::CodebaseAnalysis)),
            ("ui_analysis", output(r, Step::UiAnalysis)),
        ]
    }

    fn author(&self, written: Document) -> Entries {
        let r = self.record;
        let step = written.step();
        match written {
            Document::Prd => vec![
                ("Operation Mode", Some(self.mode())),
                (
                    "Requirements Analysis Results",
                    output(r, Step::Requirements),
                ),
                ("Convergence Result", output(r, Step::Requirements)),
                (
                    "Existing PRD",
                    self.record
                        .flow
                        .correcting()
                        .then(|| document(r, Step::Prd))
                        .flatten()
                        .map(text),
                ),
                ("correction_findings", self.author_findings()),
            ],
            Document::UiSpec => vec![
                ("requirement_context", requirement_context(r)),
                ("ui_analysis", output(r, Step::UiAnalysis)),
                ("codebase_analysis", output(r, Step::CodebaseAnalysis)),
                ("prototype_path", prototype(r).map(|p| text(p.path))),
                (
                    "prototype_reference_strength",
                    prototype(r).map(|p| text(p.reference_strength.as_str())),
                ),
                ("correction_findings", self.author_findings()),
            ],
            Document::Adr | Document::Design => vec![
                ("Operation Mode", Some(self.mode())),
                ("requirement_context", requirement_context(r)),
                (
                    "structural_scale",
                    r.flow.scale.map(|s| text(format!("{s:?}"))),
                ),
                ("codebase_analysis", output(r, Step::CodebaseAnalysis)),
                (
                    "decision_points",
                    (step == Step::Adr)
                        .then(|| output(r, Step::ScopeCheck))
                        .flatten(),
                ),
                (
                    "ui_spec",
                    (step == Step::Design)
                        .then(|| document(r, Step::UiSpec))
                        .flatten()
                        .map(text),
                ),
                (
                    "ui_analysis",
                    (step == Step::Design)
                        .then(|| output(r, Step::UiAnalysis))
                        .flatten(),
                ),
                (
                    "adr_paths",
                    (step == Step::Design)
                        .then(|| document(r, Step::Adr))
                        .flatten()
                        .map(text),
                ),
                (
                    "existing_document",
                    self.record
                        .flow
                        .correcting()
                        .then(|| document(r, step))
                        .flatten()
                        .map(text),
                ),
                ("correction_findings", self.author_findings()),
            ],
            Document::WorkPlan => vec![
                ("mode", Some(self.mode())),
                ("designDoc", document(r, Step::Design).map(text)),
                ("uiSpec", document(r, Step::UiSpec).map(text)),
                ("prd", document(r, Step::Prd).map(text)),
                ("adr", document(r, Step::Adr).map(text)),
                ("testSkeletons", output(r, Step::TestDesign)),
                ("updateContext", self.author_findings()),
            ],
        }
    }

    /// task-executor's and quality-fixer's entries.
    fn executor(&self) -> Entries {
        let r = self.record;
        let step = self.filing.step;
        let mut v: Entries = executor_scope(r, self.filing, self.issue)
            .into_iter()
            .map(|(k, e)| (k, Some(e)))
            .collect();
        let stubs = matches!(r.output(Step::Quality), Some(StepOutput::Quality(q))
            if q.status == ovenspring_core::output::QualityStatus::StubDetected);
        if step == Step::Implement && stubs {
            v.push(("incompleteImplementations", output(r, Step::Quality)));
        }
        if let Some(refusal) = &r.refusal
            && step == Step::Quality
            && r.flow.refused.is_some()
        {
            // What the repository's checks reported when they refused the commit or push.
            v.push((
                "correction_findings",
                Some(Entry::Json(json!([{
                    "id": "refused",
                    "source": "the repository's git hooks or remote",
                    "description": refusal,
                }]))),
            ));
        } else if step != Step::Implement && r.flow.origin.is_some() {
            v.push((
                "correction_findings",
                Some(Entry::Json(correction_findings(
                    r,
                    CorrectionOwner::Executor,
                ))),
            ));
        }
        v
    }

    /// code-reviewer's and security-reviewer's entries: a large change's unit, or the whole change.
    fn code_review(&self) -> Entries {
        let r = self.record;
        let mut v: Entries = vec![(
            "governingDocuments",
            Some(governing_documents(r, self.issue)),
        )];
        let unit = self
            .filing
            .unit
            .filter(|_| r.units.len() > 1)
            .and_then(|u| r.units.get(u.saturating_sub(1)));
        let Some(tasks) = unit else {
            v.push((
                "implementationFiles",
                Some(text(format!("the change from origin/{}", self.base))),
            ));
            return v;
        };
        let changed = r.task_files(self.worktree);
        let mut files = std::collections::BTreeSet::new();
        let mut scope: Vec<String> = document(r, Step::WorkPlan).into_iter().collect();
        for t in tasks {
            scope.extend(r.flow.tasks.get(*t).map(|x| x.path.clone()));
            files.extend(changed.get(t).into_iter().flatten().cloned());
        }
        v.push(("Work Plan and task scope", Some(text(scope.join(", ")))));
        v.push((
            "implementationFiles",
            Some(text(files.into_iter().collect::<Vec<_>>().join(", "))),
        ));
        v
    }

    /// integration-test-reviewer's entries.
    fn test_review(&self) -> Entries {
        let r = self.record;
        let base = r
            .last_commit
            .clone()
            .unwrap_or_else(|| format!("origin/{}", self.base));
        let changed = changed_files(r, self.worktree, &base);
        vec![
            (
                "changedFiles",
                (!changed.is_empty()).then(|| text(changed.join(", "))),
            ),
            ("diffBase", Some(text(base))),
            (
                "taskFile",
                self.filing
                    .task
                    .and_then(|t| r.flow.tasks.get(t))
                    .map(|t| text(t.path.clone())),
            ),
            ("skeletonFiles", output(r, Step::TestDesign)),
        ]
    }

    /// The input entries of the item's step, in its source's terms. Steps that are the
    /// orchestrator's or the factory's own take their sources from the item's references instead.
    pub(crate) fn inputs(&self) -> Vec<(&'static str, Entry)> {
        let r = self.record;
        let mut v: Entries = match self.filing.step {
            Step::Prd | Step::UiSpec | Step::Adr | Step::Design | Step::WorkPlan => self
                .filing
                .step
                .document()
                .map_or_else(Vec::new, |written| self.author(written)),
            Step::PrdReview => self.review("PRD", Step::Prd),
            Step::UiSpecReview => self.review("UISpec", Step::UiSpec),
            Step::AdrReview => self.review("ADRBatch", Step::Adr),
            Step::PlanReview => self.review("WorkPlan", Step::WorkPlan),
            Step::DesignReview => {
                let mut v = self.review("DesignDoc", Step::Design);
                v.push(("verification_evidence", output(r, Step::DesignVerify)));
                v
            }
            Step::CodebaseAnalysis | Step::UiAnalysis => vec![
                (
                    "prd_path",
                    r.flow
                        .documents
                        .prd_approved
                        .then(|| document(r, Step::Prd))
                        .flatten()
                        .map(text),
                ),
                (
                    "requirements",
                    (!r.flow.documents.prd_approved)
                        .then(|| output(r, Step::Requirements))
                        .flatten(),
                ),
                (
                    "prototype_path",
                    (self.filing.step == Step::UiAnalysis)
                        .then(|| prototype(r).map(|p| text(p.path)))
                        .flatten(),
                ),
            ],
            Step::DesignVerify => vec![
                ("doc_type", Some(text("design-doc"))),
                ("document_path", document(r, Step::Design).map(text)),
            ],
            Step::DesignSync => vec![("source_design", document(r, Step::Design).map(text))],
            Step::TestDesign => vec![
                ("design_docs", document(r, Step::Design).map(text)),
                ("ui_spec", document(r, Step::UiSpec).map(text)),
                ("requirement_context", requirement_context(r)),
            ],
            Step::Decompose => vec![("Work Plan", document(r, Step::WorkPlan).map(text))],
            Step::Implement | Step::ReviewResponse | Step::Quality => self.executor(),
            Step::TestReview => self.test_review(),
            Step::CodeReview | Step::SecurityReview => self.code_review(),
            Step::FixCi | Step::MergeBase => {
                vec![(
                    "governingDocuments",
                    Some(governing_documents(r, self.issue)),
                )]
            }
            // The orchestrator's and the factory's own steps take their sources from the item's
            // references.
            Step::Requirements
            | Step::ScopeCheck
            | Step::Prototype
            | Step::Resolution
            | Step::RerunCheck
            | Step::Exception
            | Step::SprintPlan
            | Step::Facts
            | Step::SprintReview
            | Step::AgreementsCheck
            | Step::Conversation
            | Step::RunCommands => vec![],
        };
        if self.filing.round.is_some_and(|n| n > 1) {
            v.push((
                "prior_feedback",
                Some(Entry::Json(prior_feedback(
                    r,
                    &output_key(self.filing.step, self.filing.unit),
                ))),
            ));
        }
        v.into_iter()
            .filter_map(|(k, e)| e.map(|e| (k, e)))
            .collect()
    }
}

/// The files the last task changed: test-review picks the integration and E2E tests among them by
/// the repository's own conventions, which no fixed file name pattern covers.
/// A task is measured from the tree before it; a change with no task files, from `diff_base`.
fn changed_files(record: &IssueRecord, worktree: &Path, diff_base: &str) -> Vec<String> {
    let (before, after) = match record.trees.as_slice() {
        [.., (_, before), (_, after)] => (before.as_str(), after),
        [(_, after)] => (diff_base, after),
        [] => return vec![],
    };
    crate::worktree::git(
        worktree,
        &["diff-tree", "-r", "-z", "--name-only", before, after],
    )
    .unwrap_or_default()
    .split('\0')
    .filter(|f| !f.is_empty())
    .map(String::from)
    .collect()
}
