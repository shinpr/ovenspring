//! What each issue step receives: the input entries its source agent declares, as `field: value`
//! lines extracted mechanically from the records, and the review handoffs the sources define:
//! `prior_feedback` for a check that rechecks, `correction_findings` for the author or executor
//! that judges and corrects.

use std::path::Path;

use ovenspring_core::flow::{Filing, Origin};
use ovenspring_core::output::{
    CheckResult, CorrectionOwner, Disposition, FindingDisposition, PrototypeOutput, Review,
    StepOutput, output_key,
};
use ovenspring_core::step::{Check, Document, Step};
use serde_json::{Value, json};

use crate::issues::IssueRecord;

/// An input entry: a value written in place, or a JSON handoff written to a file the entry names.
pub(crate) enum Entry {
    Text(String),
    Json(Value),
}

/// The findings key of the pull request comments the lead judges.
pub(crate) const COMMENTS: &str = "comments";

/// The output key of the executor's judgment of the test review's findings. A test review can
/// check the correction of the review round or of the pull request comments, so its judgment is
/// kept apart from the correction's, under `review-response`: the round's recheck, the quality run
/// after the correction and the comment replies read the correction's judgment after it.
pub(crate) const TEST_REVIEW_JUDGMENT: &str = "test-review-judgment";

/// The output key a finished step's output is kept under, given the origin of the correction it
/// ran in.
pub(crate) fn kept_under(origin: Option<Origin>, step: Step, unit: Option<usize>) -> String {
    match (step, origin) {
        (Step::ReviewResponse, Some(Origin::TestReview)) => TEST_REVIEW_JUDGMENT.to_owned(),
        _ => output_key(step, unit),
    }
}

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

/// The step a check's findings are handed to: the author of the document it reviewed, or the
/// executor (the test review, the review round, the pull request's comments).
fn receiver(key: &str) -> Step {
    key.parse::<Step>()
        .ok()
        .and_then(Step::check)
        .and_then(Check::document)
        .map_or(Step::ReviewResponse, Document::step)
}

/// The output key of the receiver's judgment of the findings kept under `key`.
fn judgment_key(key: &str) -> String {
    if key == Step::TestReview.as_str() {
        TEST_REVIEW_JUDGMENT.to_owned()
    } else {
        receiver(key).to_string()
    }
}

/// The judgments `step` recorded, in its output kept under `key`, for the findings it was handed.
fn judgments(record: &IssueRecord, step: Step, key: &str) -> Vec<FindingDisposition> {
    match record.output_at(step, key) {
        Some(StepOutput::Document(d)) => d.dispositions,
        Some(StepOutput::Executor(e)) => e.dispositions,
        _ => vec![],
    }
}

/// The judgments of `key`'s findings by the step they were handed to.
fn dispositions_for(record: &IssueRecord, key: &str) -> Vec<FindingDisposition> {
    judgments(record, receiver(key), &judgment_key(key))
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

/// The findings handed to a step that it leaves without a disposition.
pub(crate) fn undisposed(record: &IssueRecord, dispositions: &[FindingDisposition]) -> Vec<String> {
    handed(record)
        .into_iter()
        .filter_map(|(key, f)| {
            let id = f.get("id").and_then(Value::as_str)?.to_owned();
            (!dispositions
                .iter()
                .any(|d| d.id == id && belongs_to(d, &key)))
            .then(|| format!("{key} {id}"))
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

/// `prior_feedback` for a check that rechecks after a correction: one entry per judgment the
/// receiver of its findings recorded, the original finding carried in `evidence`.
fn prior_feedback(record: &IssueRecord, key: &str) -> Value {
    let outstanding = maintained_findings(record, key);
    // An exception can correct the document itself. Its output records the lead's
    // decision; an exception from an earlier round is unrelated to this correction.
    let correction = latest_exception(record).map_or_else(|| correction_ref(record), str::to_owned);
    let entries: Vec<Value> = dispositions_for(record, key)
        .iter()
        .filter_map(|d| {
            let original = match &outstanding {
                Some(findings) => findings
                    .iter()
                    .find(|f| f["id"].as_str() == Some(&d.id))?
                    .clone(),
                None => finding(record, key, &d.id),
            };
            Some(json!({
                "id": d.id,
                "disposition": d.disposition.as_str(),
                "reason": d.reason,
                "evidence": {
                    "finding": original,
                    "correction": correction,
                },
            }))
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

fn latest_exception(record: &IssueRecord) -> Option<&str> {
    let item = Path::new(record.last_item.as_ref()?);
    let exception = record.outputs.get(Step::Exception.as_str())?;
    (item.file_stem() == Path::new(exception).file_stem()).then_some(exception.as_str())
}

fn correction_ref(record: &IssueRecord) -> String {
    match &record.last_commit {
        Some(sha) => format!("the changes since commit {sha}"),
        None => "the changes in the worktree".into(),
    }
}

/// The checks whose findings the current correction answers, by output key.
fn corrected_checks(record: &IssueRecord) -> Vec<String> {
    let flow = &record.flow;
    match flow.origin {
        Some(Origin::Round) => flow
            .round
            .findings
            .union(&flow.round.maintained)
            .map(|r| String::from(*r))
            .collect(),
        Some(Origin::PrComments) => vec![COMMENTS.to_owned()],
        _ => flow.check.iter().map(|c| c.step().to_string()).collect(),
    }
}

/// The still-open applied findings when the last result was a reconciliation.
fn maintained_findings(record: &IssueRecord, key: &str) -> Option<Vec<Value>> {
    let maintained: Vec<Value> = record
        .raw_output(key)
        .and_then(|o| o.get("prior_feedback_reconciliation").cloned())
        .and_then(|r| r.as_array().cloned())
        .unwrap_or_default();
    if maintained.is_empty() {
        return None;
    }
    Some(
        maintained
            .into_iter()
            .filter(|m| m.get("status").and_then(Value::as_str) == Some("maintained"))
            .filter(|m| m.get("prior_disposition").and_then(Value::as_str) == Some("apply"))
            .map(|m| json!({"id": m.get("id"), "remaining": m.get("evidence")}))
            .collect(),
    )
}

/// The findings handed to the step that judges them, each as its check returned it, by output
/// key: a check's new findings, or the findings its correction re-review maintained with the
/// reconciliation entry. A round's findings name their review, since its code reviews number
/// their findings alike.
pub(crate) fn handed(record: &IssueRecord) -> Vec<(String, Value)> {
    let round = record.flow.origin == Some(Origin::Round);
    let mut out = Vec::new();
    for key in corrected_checks(record) {
        let found = maintained_findings(record, &key)
            .unwrap_or_else(|| record.findings.get(&key).cloned().unwrap_or_default());
        for mut f in found {
            if round && let Some(obj) = f.as_object_mut() {
                obj.insert("review".into(), json!(key));
            }
            out.push((key.clone(), f));
        }
    }
    out
}

/// The findings the executor applied in `review-response` and kept for `owner`, each as its check
/// returned it with the executor's judgment: the quality run's correction scope, or the Design
/// Doc's author's share of a round.
fn executor_applied(record: &IssueRecord, owner: CorrectionOwner) -> Value {
    let check = record.flow.check.map(|c| c.step().to_string());
    let keys: Vec<String> = record.findings.keys().cloned().collect();
    let out: Vec<Value> = judgments(record, Step::ReviewResponse, Step::ReviewResponse.as_str())
        .into_iter()
        .filter(|d| d.disposition == Disposition::Apply)
        .filter(|d| d.owner.unwrap_or(CorrectionOwner::Executor) == owner)
        .map(|d| {
            let key = match (d.review, record.flow.origin) {
                (Some(review), _) => Some(String::from(review)),
                (None, Some(Origin::PrComments)) => Some(COMMENTS.to_owned()),
                (None, Some(Origin::Round)) => keys
                    .iter()
                    .find(|k| {
                        Review::try_from((*k).clone()).is_ok()
                            && record.findings.get(*k).is_some_and(|found| {
                                found
                                    .iter()
                                    .any(|f| f.get("id").and_then(Value::as_str) == Some(&d.id))
                            })
                    })
                    .cloned(),
                (None, _) => check.clone(),
            };
            let mut f = key.map_or_else(|| json!({"id": d.id}), |k| finding(record, &k, &d.id));
            if let Some(obj) = f.as_object_mut() {
                obj.insert(
                    "executor".into(),
                    json!({"disposition": "apply", "reason": d.reason}),
                );
            }
            f
        })
        .collect();
    Value::Array(out)
}

/// `correction_findings`: the findings a step judges and corrects.
fn correction_findings(record: &IssueRecord) -> Value {
    Value::Array(handed(record).into_iter().map(|(_, f)| f).collect())
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
        text(if document(self.record, self.filing.step).is_some() {
            "update"
        } else {
            "create"
        })
    }

    fn author_findings(&self) -> Option<Entry> {
        let r = self.record;
        r.flow.correcting().then(|| {
            Entry::Json(if r.flow.origin == Some(Origin::Round) {
                executor_applied(r, CorrectionOwner::Author)
            } else {
                correction_findings(r)
            })
        })
    }

    /// document-reviewer's entries.
    fn review(&self, doc_type: &str, step: Step) -> Entries {
        let r = self.record;
        vec![
            ("doc_type", Some(text(doc_type))),
            ("target", document(r, step).map(text)),
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
                ("Existing PRD", document(r, Step::Prd).map(text)),
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
                ("existing_document", document(r, step).map(text)),
                ("correction_findings", self.author_findings()),
            ],
            Document::WorkPlan => vec![
                ("mode", Some(self.mode())),
                ("designDoc", document(r, Step::Design).map(text)),
                ("uiSpec", document(r, Step::UiSpec).map(text)),
                ("prd", document(r, Step::Prd).map(text)),
                ("adr", document(r, Step::Adr).map(text)),
                ("testSkeletons", output(r, Step::TestDesign)),
                ("correction_findings", self.author_findings()),
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
        } else if step == Step::ReviewResponse {
            v.push((
                "correction_findings",
                Some(Entry::Json(correction_findings(r))),
            ));
        } else if step == Step::Quality && r.flow.origin.is_some() {
            v.push((
                "correction_findings",
                Some(Entry::Json(executor_applied(r, CorrectionOwner::Executor))),
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

    /// Use the previous judgment of this subject even when routing the correction restarted a stage.
    fn review_scope(&self, v: &mut Entries) {
        let step = self.filing.step;
        let r = self.record;
        if step == Step::TestReview && r.flow.origin != Some(Origin::TestReview) {
            return; // A new task's tests are a different review subject.
        }
        let key = output_key(step, self.filing.unit);
        let Some(previous) = r.outputs.get(&key) else {
            return;
        };
        let result = r
            .raw_output(&key)
            .and_then(|raw| StepOutput::parse(step, raw).ok())
            .and_then(|out| out.check_result());
        match result {
            Some(CheckResult::Findings | CheckResult::MaintainedApply) => {
                let feedback = prior_feedback(r, &key);
                if !feedback.as_array().is_some_and(Vec::is_empty) {
                    v.push(("prior_feedback", Some(Entry::Json(feedback))));
                    return;
                }
            }
            Some(CheckResult::Clear) => {}
            Some(CheckResult::Blocked) | None => return,
        }
        v.push(("previous_review", Some(text(previous))));
        v.push(("change_to_review", r.last_item.clone().map(text)));
        if let Some(review) = Review::of(step, self.filing.unit)
            && let Some(StepOutput::RerunCheck(decision)) = r.output(Step::RerunCheck)
            && let Some(affected) = decision.rerun.iter().find(|entry| entry.review == review)
            && r.last_item.as_ref().and_then(|p| Path::new(p).file_stem())
                == r.outputs
                    .get(Step::RerunCheck.as_str())
                    .and_then(|p| Path::new(p).file_stem())
        {
            v.push((
                "affected_files",
                Some(Entry::Json(json!(affected.implementation_files))),
            ));
        }
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
            Step::DesignReview => self.review("DesignDoc", Step::Design),
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
            | Step::RerunCheck
            | Step::Exception
            | Step::SprintPlan
            | Step::Facts
            | Step::SprintReview
            | Step::AgreementsCheck
            | Step::Conversation
            | Step::RunCommands => vec![],
        };
        if self.filing.step.is_check() {
            self.review_scope(&mut v);
        } else if self.filing.step.document().is_some()
            && let Some(decision) = latest_exception(r)
        {
            v.push(("lead_decision", Some(text(decision))));
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

#[cfg(test)]
mod tests {
    use ovenspring_core::flow::Origin;
    use ovenspring_core::output::Review;
    use serde_json::json;

    use super::handed;
    use crate::issues::IssueRecord;

    /// A round's findings as `review-response` receives them: one entry per finding of each
    /// review, an ID two reviews both use counted for each, and an applied finding a re-review
    /// maintained handed again as still open.
    #[test]
    fn a_round_hands_each_reviews_findings_and_each_maintained_applied_finding() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("security-review.json");
        std::fs::write(
            &output,
            json!({"status": "pass", "findings": [], "prior_feedback_reconciliation": [
                {"id": "F009", "prior_disposition": "apply", "status": "maintained", "evidence": "still open"},
                {"id": "F010", "prior_disposition": "apply", "status": "resolved", "evidence": "fixed"},
                {"id": "F011", "prior_disposition": "decline", "status": "maintained", "evidence": "e"}
            ]})
            .to_string(),
        )
        .unwrap();
        let mut record = IssueRecord::new("ACME-1", "Export orders", vec![]);
        record.flow.origin = Some(Origin::Round);
        record.flow.round.findings = [
            Review::CodeReview { unit: 1 },
            Review::CodeReview { unit: 2 },
        ]
        .into();
        record.flow.round.maintained = [Review::SecurityReview].into();
        record.findings.insert(
            "code-review/1".into(),
            vec![json!({"id": "F001"}), json!({"id": "F002"})],
        );
        record
            .findings
            .insert("code-review/2".into(), vec![json!({"id": "F001"})]);
        record
            .outputs
            .insert("security-review".into(), output.display().to_string());

        let handed: Vec<(String, String)> = handed(&record)
            .into_iter()
            .map(|(key, f)| (key, f["id"].as_str().unwrap().to_owned()))
            .collect();
        assert_eq!(
            handed,
            [
                ("code-review/1", "F001"),
                ("code-review/1", "F002"),
                ("code-review/2", "F001"),
                ("security-review", "F009"),
            ]
            .map(|(k, id)| (k.to_owned(), id.to_owned()))
        );
    }
}
