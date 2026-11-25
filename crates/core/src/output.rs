//! What each step's agent writes to its item's `out`, as the daemon reads it.
//!
//! An issue step's output is its source agent's result as the source defines it. The types here
//! hold only the fields the flow routes on and the daemon uses; every other field stays in the
//! output file and reaches the steps that consume it as written. `ovenspring item set` refuses
//! `done` until the output parses here.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::step::Step;

/// Structural Scale; `unresolved` while the Small evidence gate is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Scale {
    Small,
    Medium,
    Large,
    Unresolved,
}

/// The lead's decision at the requirements stop: go on, close the issue with evidence, or ask
/// the owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    Proceed,
    Close,
    Ask,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Convergence {
    pub outcome: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeEvidence {
    #[serde(default)]
    pub affected_layers: Vec<String>,
}

/// `requirements`: requirement-analyzer's evidence with the orchestrator's convergence record and
/// Structural Scale; `decision`, `evidence` (the tracker comment on close) and `question` are the
/// factory's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequirementsOutput {
    pub decision: Decision,
    pub structural_scale: Scale,
    pub convergence: Convergence,
    #[serde(default)]
    pub scope_evidence: ScopeEvidence,
    #[serde(default)]
    pub evidence: Option<String>,
    #[serde(default)]
    pub question: Option<String>,
    /// An existing PRD whose stated behavior a Small or Medium change alters: it is updated, and
    /// approved by the owner, before the work goes on.
    #[serde(default)]
    pub prd_update: Option<String>,
}

/// `scope-check`: the orchestrator's decisions after the first codebase analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeCheckOutput {
    pub decision: Decision,
    pub structural_scale: Scale,
    #[serde(default)]
    pub ui_spec: bool,
    #[serde(default)]
    pub adr_decision_points: Vec<Value>,
    #[serde(default)]
    pub question: Option<String>,
}

/// Kinds of change the owner approves before the factory goes on.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, TS)]
#[ts(type = "string")]
pub enum Approval {
    Schema,
    ExternalConnection,
    Infrastructure,
    Ci,
    ExternalContract,
    /// The factory's configuration, working agreements, knowledge or the repository's agent
    /// instructions: held by path whatever a design declares.
    FactoryConfiguration,
    /// A kind of change the owner named in `[person_reviews]`, with the words that describe it: a
    /// person reviews it before it merges.
    Named(String),
}

/// The categories the factory knows, by the names the configuration and step outputs use.
const BUILT_IN: [(Approval, &str); 6] = [
    (Approval::Schema, "schema"),
    (Approval::ExternalConnection, "external-connection"),
    (Approval::Infrastructure, "infrastructure"),
    (Approval::Ci, "ci"),
    (Approval::ExternalContract, "external-contract"),
    (Approval::FactoryConfiguration, "factory-configuration"),
];

impl Approval {
    /// The name the configuration and step outputs use.
    pub fn as_str(&self) -> &str {
        match self {
            Approval::Named(name) => name,
            Approval::Schema
            | Approval::ExternalConnection
            | Approval::Infrastructure
            | Approval::Ci
            | Approval::ExternalContract
            | Approval::FactoryConfiguration => BUILT_IN
                .iter()
                .find(|(a, _)| a == self)
                .map_or("", |(_, name)| name),
        }
    }

    /// The category of `name`: one the factory knows, or one the owner named.
    pub fn of(name: &str) -> Approval {
        BUILT_IN
            .iter()
            .find(|(_, n)| *n == name)
            .map_or_else(|| Approval::Named(name.to_owned()), |(a, _)| a.clone())
    }

    /// Whether the owner approves the design that adds it, before it is built; a named category
    /// is reviewed only before it merges.
    pub fn is_approved_with_the_design(&self) -> bool {
        !matches!(self, Approval::Named(_))
    }
}

impl Serialize for Approval {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Approval {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Approval, D::Error> {
        let name = String::deserialize(d)?;
        if name.trim().is_empty() {
            return Err(serde::de::Error::custom("a category needs a name"));
        }
        Ok(Approval::of(&name))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocumentStatus {
    Completed,
    Blocked,
}

/// A document author's result (technical-designer's shape, also used for the authors that return
/// only a path): `prd`, `ui-spec`, `adr`, `design`, `work-plan`, `decompose`.
/// `question` (prd-creator's extracted questions) and `approvals` (the held categories a design
/// adds) are the factory's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentOutput {
    pub status: DocumentStatus,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub question: Option<String>,
    #[serde(default)]
    pub approvals: Vec<Approval>,
}

/// How far the implementation follows a prototype, as the UI Spec records it: `binding`, its
/// rendering is the implementation's target except where the UI Spec differs; `reference`, only
/// what the UI Spec records reaches the implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum ReferenceStrength {
    Binding,
    Reference,
}

impl ReferenceStrength {
    pub fn as_str(self) -> &'static str {
        match self {
            ReferenceStrength::Binding => "binding",
            ReferenceStrength::Reference => "reference",
        }
    }
}

/// `prototype`: the one HTML file in the worktree, its reference strength and why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrototypeOutput {
    pub path: String,
    pub reference_strength: ReferenceStrength,
    pub basis: String,
}

/// An analyst's result (`codebase-analysis`, `ui-analysis`): nothing in it routes the flow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalysisOutput(pub serde_json::Map<String, Value>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReconciliationStatus {
    Resolved,
    Withdrawn,
    Maintained,
}

/// One entry of a correction re-review (`prior_feedback_reconciliation`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reconciliation {
    pub id: String,
    pub prior_disposition: Disposition,
    pub status: ReconciliationStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentVerdict {
    Pass,
    NeedsRevision,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerdictDecision {
    pub decision: DocumentVerdict,
}

/// document-reviewer's result: `prd-review`, `ui-spec-review`, `adr-review`, `design-review`,
/// `plan-review`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentReviewOutput {
    pub verdict: VerdictDecision,
    #[serde(default)]
    pub issues: Vec<Value>,
    #[serde(default)]
    pub prior_feedback_reconciliation: Vec<Reconciliation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerifierStatus {
    Consistent,
    NeedsReview,
    Inconsistent,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerifierSummary {
    pub status: VerifierStatus,
}

/// code-verifier's result: `design-verify`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerifierOutput {
    pub summary: VerifierSummary,
    #[serde(default)]
    pub discrepancies: Vec<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SyncStatus {
    ConflictsFound,
    NoConflicts,
    /// No other Design Doc to compare with.
    Skipped,
}

/// design-sync's `[SUMMARY]` fields: `design-sync`. The report itself is written under
/// `## Result`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesignSyncOutput {
    pub sync_status: SyncStatus,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedFiles {
    #[serde(default)]
    pub integration: Option<String>,
    #[serde(default)]
    pub fixture_e2e: Option<String>,
    #[serde(default)]
    pub service_e2e: Option<String>,
}

/// acceptance-test-generator's result: `test-design`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestDesignOutput {
    pub status: DocumentStatus,
    #[serde(default)]
    pub generated_files: GeneratedFiles,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutorStatus {
    Completed,
    Blocked,
    EscalationNeeded,
}

/// task-executor's result: `implement`, `review-response`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutorOutput {
    pub status: ExecutorStatus,
    #[serde(default)]
    pub change_summary: Option<String>,
    /// Whether the change touched integration or E2E tests. A result that does not say runs the
    /// test review: an extra review costs one launch, a skipped one lets those tests go unreviewed.
    #[serde(default = "unsaid_means_review")]
    pub requires_test_review: bool,
}

fn unsaid_means_review() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    Pass,
    NeedsRevision,
    Blocked,
}

/// integration-test-reviewer's result: `test-review`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestReviewOutput {
    pub status: ReviewStatus,
    #[serde(default)]
    pub quality_issues: Vec<Value>,
    #[serde(default, rename = "prior_feedback_reconciliation")]
    pub prior_feedback_reconciliation: Vec<Reconciliation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityStatus {
    Pass,
    StubDetected,
    VerificationIncomplete,
    Blocked,
}

/// quality-fixer's result: `quality`, `fix-ci`, `merge-base`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityOutput {
    pub status: QualityStatus,
    #[serde(default)]
    pub incomplete_implementations: Vec<Value>,
    #[serde(default)]
    pub missing_prerequisites: Vec<Value>,
    #[serde(default)]
    pub separate_responsibility_failures: Vec<Value>,
}

impl QualityOutput {
    /// Each retained verification limit, as its reason and what it affects.
    pub fn limitations(&self) -> Vec<(String, String)> {
        let text = |v: &Value, field: &str| v.get(field).and_then(Value::as_str).map(str::to_owned);
        let missing = self.missing_prerequisites.iter().map(|m| {
            let affected = m
                .get("affectedTests")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            (
                text(m, "description").unwrap_or_else(|| "unavailable prerequisite".to_owned()),
                affected,
            )
        });
        let separate = self.separate_responsibility_failures.iter().map(|f| {
            let evidence = text(f, "evidence").unwrap_or_else(|| "failure".to_owned());
            let reason = match text(f, "owningBoundary") {
                Some(owner) => format!("{evidence} (owned by {owner})"),
                None => evidence,
            };
            (reason, text(f, "command").unwrap_or_default())
        });
        missing.chain(separate).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CodeVerdict {
    Pass,
    NeedsImprovement,
    NeedsRedesign,
    Blocked,
}

/// code-reviewer's result: `code-review`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeReviewOutput {
    pub verdict: CodeVerdict,
    #[serde(default)]
    pub findings: Vec<Value>,
    #[serde(default)]
    pub prior_feedback_reconciliation: Vec<Reconciliation>,
    /// The kinds of change a person reviews that the change falls under, as the code shows them.
    #[serde(default)]
    pub approvals: Vec<Approval>,
}

/// security-reviewer's result: `security-review`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SecurityReviewOutput {
    pub status: ReviewStatus,
    #[serde(default)]
    pub findings: Vec<Value>,
    #[serde(default)]
    pub prior_feedback_reconciliation: Vec<Reconciliation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Disposition {
    Apply,
    Decline,
}

impl Disposition {
    pub fn as_str(self) -> &'static str {
        match self {
            Disposition::Apply => "apply",
            Disposition::Decline => "decline",
        }
    }
}

/// Who corrects an applied finding: the document's author when the
/// implementation already satisfies the value boundaries, else the executor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CorrectionOwner {
    Author,
    Executor,
}

/// One finding's record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FindingDisposition {
    pub id: String,
    pub disposition: Disposition,
    pub basis: String,
    pub evidence: String,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub owner: Option<CorrectionOwner>,
    /// The review of the post-implementation round that raised the finding.
    #[serde(default)]
    pub review: Option<Review>,
}

/// `resolution`: the orchestrator's dispositions; `decision` and `question` carry Requirement
/// Change Detection to the owner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolutionOutput {
    pub decision: Decision,
    #[serde(default)]
    pub dispositions: Vec<FindingDisposition>,
    #[serde(default)]
    pub question: Option<String>,
}

impl ResolutionOutput {
    pub fn applies(&self) -> bool {
        self.dispositions
            .iter()
            .any(|d| d.disposition == Disposition::Apply)
    }

    /// Whether an applied finding is the document author's to correct.
    pub fn author_applies(&self) -> bool {
        self.dispositions.iter().any(|d| {
            d.disposition == Disposition::Apply && d.owner == Some(CorrectionOwner::Author)
        })
    }

    /// Whether an applied finding is the executor's to correct (the default owner).
    pub fn executor_applies(&self) -> bool {
        self.dispositions.iter().any(|d| {
            d.disposition == Disposition::Apply && d.owner != Some(CorrectionOwner::Author)
        })
    }
}

/// A review of the post-implementation round: `code-review/<unit>` (units counted from 1) or
/// `security-review`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum Review {
    CodeReview { unit: usize },
    SecurityReview,
}

impl Review {
    pub fn step(self) -> Step {
        match self {
            Review::CodeReview { .. } => Step::CodeReview,
            Review::SecurityReview => Step::SecurityReview,
        }
    }

    pub fn unit(self) -> Option<usize> {
        match self {
            Review::CodeReview { unit } => Some(unit),
            Review::SecurityReview => None,
        }
    }

    /// The review `step` names, with `unit` for a code review.
    pub fn of(step: Step, unit: Option<usize>) -> Option<Review> {
        match (step, unit) {
            (Step::CodeReview, Some(unit)) => Some(Review::CodeReview { unit }),
            (Step::SecurityReview, None) => Some(Review::SecurityReview),
            _ => None,
        }
    }
}

/// The key an item's output and findings are kept under: its step, with a code review's unit.
pub fn output_key(step: Step, unit: Option<usize>) -> String {
    unit.map_or_else(|| step.to_string(), |u| format!("{step}/{u}"))
}

impl From<Review> for String {
    fn from(r: Review) -> String {
        output_key(r.step(), r.unit())
    }
}

impl TryFrom<String> for Review {
    type Error = String;

    fn try_from(text: String) -> Result<Review, String> {
        if text == Step::SecurityReview.as_str() {
            return Ok(Review::SecurityReview);
        }
        text.strip_prefix(Step::CodeReview.as_str())
            .and_then(|rest| rest.strip_prefix('/'))
            .and_then(|n| n.parse().ok())
            .filter(|n| *n >= 1)
            .map(|unit| Review::CodeReview { unit })
            .ok_or_else(|| format!("{text:?} is not code-review/<unit> or security-review"))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rerun {
    pub review: Review,
    #[serde(default)]
    pub implementation_files: Vec<String>,
}

/// `rerun-check`: the passed reviews whose evidence a correction invalidated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RerunCheckOutput {
    #[serde(default)]
    pub rerun: Vec<Rerun>,
}

/// `agreements-check`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    pub id: String,
    pub location: String,
    pub basis: String,
    pub effect: String,
    pub correction: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgreementsCheckOutput {
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExceptionAction {
    Retry,
    Replan,
    Redesign,
    Drop,
    Ask,
    /// The way forward needs what agents may not do: the owner approves `procedure`, which only
    /// the factory runs.
    Recover,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExceptionOutput {
    pub action: ExceptionAction,
    pub note: String,
    #[serde(default)]
    pub question: Option<String>,
    /// For `recover`: the shell commands the factory is to run.
    #[serde(default)]
    pub procedure: Option<String>,
    /// For `recover`: what happened, what the procedure changes, whether it can be undone.
    #[serde(default)]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedIssue {
    pub key: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SprintPlanOutput {
    pub purpose: String,
    pub key_results: Vec<String>,
    /// In dependency order; empty when no open issue serves the goal now.
    pub issues: Vec<PlannedIssue>,
    /// The proposal as the owner reads it.
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsOutput {
    pub facts_path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct KeyResult {
    pub text: String,
    pub met: bool,
    pub evidence: String,
}

/// The working-agreements files launches read: the common one and one per role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgreementName {
    #[serde(rename = "common.md")]
    Common,
    #[serde(rename = "lead.md")]
    Lead,
    #[serde(rename = "executor.md")]
    Executor,
    #[serde(rename = "reviewer.md")]
    Reviewer,
    #[serde(rename = "analyzer.md")]
    Analyzer,
}

impl AgreementName {
    /// The file a role's launches read beside the common one.
    pub fn of(role: crate::config::Role) -> AgreementName {
        match role {
            crate::config::Role::Lead => AgreementName::Lead,
            crate::config::Role::Executor => AgreementName::Executor,
            crate::config::Role::Reviewer => AgreementName::Reviewer,
            crate::config::Role::Analyzer => AgreementName::Analyzer,
        }
    }

    /// The file's name in the working agreements folder (`[agreements] path`).
    pub fn as_str(self) -> &'static str {
        match self {
            AgreementName::Common => "common.md",
            AgreementName::Lead => "lead.md",
            AgreementName::Executor => "executor.md",
            AgreementName::Reviewer => "reviewer.md",
            AgreementName::Analyzer => "analyzer.md",
        }
    }
}

/// A working-agreements file as it should read after the review, `<agreements folder>/<file>`; a
/// file left with nothing (empty `content`) is deleted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgreementFile {
    pub file: AgreementName,
    pub content: String,
}

/// One knowledge file: its full new content, or empty to delete it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeFile {
    pub file: String,
    pub content: String,
}

impl SprintReviewOutput {
    /// Whether the review changes what later launches read: agreements or knowledge.
    pub fn changes_what_launches_read(&self) -> bool {
        !self.agreements.is_empty() || !self.knowledge.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SprintReviewOutput {
    pub key_results: Vec<KeyResult>,
    #[serde(default)]
    pub agreements: Vec<AgreementFile>,
    /// Knowledge files to write, rewrite or delete, by their path under the knowledge folder.
    #[serde(default)]
    pub knowledge: Vec<KnowledgeFile>,
    /// The review as the owner reads it.
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationOutput {
    pub reply: String,
}

/// How the lead found the product runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Runs {
    Commands,
    /// There is nothing to start (a library, a CLI).
    Nothing,
    /// The repository does not say how it starts.
    Unknown,
}

/// `run-commands`: how the product starts, for `[app]`, with the repository's evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RunCommandsOutput {
    pub runs: Runs,
    #[serde(default)]
    pub setup: Option<String>,
    #[serde(default)]
    pub start: Option<String>,
    #[serde(default)]
    pub ready: Option<String>,
    /// What in the repository says so, as the owner reads it.
    pub reason: String,
}

/// What a check (a review or verification) found, read from its source's result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckResult {
    /// Nothing actionable: the check is complete.
    Clear,
    /// Findings for `resolution`.
    Findings,
    /// A correction re-review maintained an applied finding: it returns to its author or executor.
    MaintainedApply,
    /// The check could not run.
    Blocked,
}

fn reconciled(entries: &[Reconciliation]) -> CheckResult {
    if entries.iter().any(|r| {
        r.prior_disposition == Disposition::Apply && r.status == ReconciliationStatus::Maintained
    }) {
        CheckResult::MaintainedApply
    } else {
        CheckResult::Clear
    }
}

fn found(blocked: bool, recheck: &[Reconciliation], findings: bool) -> CheckResult {
    if blocked {
        CheckResult::Blocked
    } else if !recheck.is_empty() {
        reconciled(recheck)
    } else if findings {
        CheckResult::Findings
    } else {
        CheckResult::Clear
    }
}

/// A validated output, by step.
#[derive(Debug, Clone, PartialEq)]
pub enum StepOutput {
    Requirements(RequirementsOutput),
    ScopeCheck(ScopeCheckOutput),
    Document(DocumentOutput),
    Prototype(PrototypeOutput),
    Analysis(AnalysisOutput),
    DocumentReview(DocumentReviewOutput),
    Verifier(VerifierOutput),
    DesignSync(DesignSyncOutput),
    TestDesign(TestDesignOutput),
    Executor(ExecutorOutput),
    TestReview(TestReviewOutput),
    Quality(QualityOutput),
    CodeReview(CodeReviewOutput),
    SecurityReview(SecurityReviewOutput),
    Resolution(ResolutionOutput),
    RerunCheck(RerunCheckOutput),
    AgreementsCheck(AgreementsCheckOutput),
    Exception(ExceptionOutput),
    SprintPlan(SprintPlanOutput),
    Facts(FactsOutput),
    SprintReview(SprintReviewOutput),
    Conversation(ConversationOutput),
    RunCommands(RunCommandsOutput),
}

/// A design-phase launch's preparation turn: what the owner must answer or know before the work,
/// as one message (`None` when nothing), and whether it asks something the work needs answered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrepareOutput {
    pub message: Option<String>,
    pub question: bool,
}

impl PrepareOutput {
    /// Refused when it asks without a message, or carries an empty one.
    pub fn parse(value: serde_json::Value) -> Result<PrepareOutput, String> {
        let p: PrepareOutput =
            serde_json::from_value(value).map_err(|e| format!("preparation output: {e}"))?;
        match &p.message {
            Some(m) if m.trim().is_empty() => {
                Err("preparation output: `message` is empty; use null when there is nothing".into())
            }
            None if p.question => {
                Err("preparation output: `question` is true but there is no `message`".into())
            }
            _ => Ok(p),
        }
    }
}

impl StepOutput {
    /// The output of `step`, refused unless it has the step's shape and every rule that shape
    /// states: text fields the daemon uses are not empty, and fields one choice needs are there.
    pub fn parse(step: Step, value: serde_json::Value) -> Result<StepOutput, String> {
        let output = Self::shape(step, value)?;
        output
            .check()
            .map_err(|problem| format!("{step} output: {problem}"))?;
        Ok(output)
    }

    fn shape(step: Step, value: serde_json::Value) -> Result<StepOutput, String> {
        fn read<T: serde::de::DeserializeOwned>(step: Step, value: Value) -> Result<T, String> {
            serde_json::from_value(value).map_err(|err| format!("{step} output: {err}"))
        }
        Ok(match step {
            Step::Requirements => StepOutput::Requirements(read(step, value)?),
            Step::ScopeCheck => StepOutput::ScopeCheck(read(step, value)?),
            Step::Prd
            | Step::UiSpec
            | Step::Adr
            | Step::Design
            | Step::WorkPlan
            | Step::Decompose => StepOutput::Document(read(step, value)?),
            Step::Prototype => StepOutput::Prototype(read(step, value)?),
            Step::CodebaseAnalysis | Step::UiAnalysis => StepOutput::Analysis(read(step, value)?),
            Step::PrdReview
            | Step::UiSpecReview
            | Step::AdrReview
            | Step::DesignReview
            | Step::PlanReview => StepOutput::DocumentReview(read(step, value)?),
            Step::DesignVerify => StepOutput::Verifier(read(step, value)?),
            Step::DesignSync => StepOutput::DesignSync(read(step, value)?),
            Step::TestDesign => StepOutput::TestDesign(read(step, value)?),
            Step::Implement | Step::ReviewResponse => StepOutput::Executor(read(step, value)?),
            Step::TestReview => StepOutput::TestReview(read(step, value)?),
            Step::Quality | Step::FixCi | Step::MergeBase => {
                StepOutput::Quality(read(step, value)?)
            }
            Step::CodeReview => StepOutput::CodeReview(read(step, value)?),
            Step::SecurityReview => StepOutput::SecurityReview(read(step, value)?),
            Step::Resolution => StepOutput::Resolution(read(step, value)?),
            Step::RerunCheck => StepOutput::RerunCheck(read(step, value)?),
            Step::AgreementsCheck => StepOutput::AgreementsCheck(read(step, value)?),
            Step::Exception => StepOutput::Exception(read(step, value)?),
            Step::SprintPlan => StepOutput::SprintPlan(read(step, value)?),
            Step::Facts => StepOutput::Facts(read(step, value)?),
            Step::SprintReview => StepOutput::SprintReview(read(step, value)?),
            Step::Conversation => StepOutput::Conversation(read(step, value)?),
            Step::RunCommands => StepOutput::RunCommands(read(step, value)?),
        })
    }

    fn check(&self) -> Result<(), String> {
        match self {
            StepOutput::Requirements(r) => {
                filled("convergence.outcome", &r.convergence.outcome)?;
                optional("evidence", r.evidence.as_deref())?;
                optional("question", r.question.as_deref())?;
                match r.decision {
                    Decision::Close => needed("evidence", r.evidence.as_deref(), "close"),
                    Decision::Ask => needed("question", r.question.as_deref(), "ask"),
                    Decision::Proceed => Ok(()),
                }
            }
            StepOutput::ScopeCheck(c) => {
                optional("question", c.question.as_deref())?;
                match c.decision {
                    Decision::Ask => needed("question", c.question.as_deref(), "ask"),
                    Decision::Close => Err("decision: close is decided at requirements".into()),
                    Decision::Proceed if c.structural_scale == Scale::Unresolved => {
                        Err("structuralScale: the scope check settles the scale".into())
                    }
                    Decision::Proceed => Ok(()),
                }
            }
            StepOutput::Document(d) => check_document(d),
            StepOutput::CodeReview(r) => declared(&r.approvals),
            StepOutput::Prototype(p) => {
                filled("path", &p.path)?;
                filled("basis", &p.basis)
            }
            StepOutput::Resolution(r) => {
                for d in &r.dispositions {
                    filled("dispositions.id", &d.id)?;
                    filled("dispositions.basis", &d.basis)?;
                    filled("dispositions.evidence", &d.evidence)?;
                    if d.disposition == Disposition::Decline {
                        needed("dispositions.reason", d.reason.as_deref(), "decline")?;
                    }
                }
                optional("question", r.question.as_deref())?;
                match r.decision {
                    Decision::Ask => needed("question", r.question.as_deref(), "ask"),
                    Decision::Close => Err("decision: close is decided at requirements".into()),
                    Decision::Proceed => Ok(()),
                }
            }
            StepOutput::AgreementsCheck(r) => r.findings.iter().try_for_each(|f| {
                filled("findings.id", &f.id)?;
                filled("findings.location", &f.location)?;
                filled("findings.basis", &f.basis)?;
                filled("findings.effect", &f.effect)?;
                filled("findings.correction", &f.correction)
            }),
            StepOutput::Exception(e) => {
                filled("note", &e.note)?;
                optional("question", e.question.as_deref())?;
                if e.action == ExceptionAction::Ask {
                    needed("question", e.question.as_deref(), "ask")?;
                }
                if e.action == ExceptionAction::Recover {
                    needed("procedure", e.procedure.as_deref(), "recover")?;
                    needed("summary", e.summary.as_deref(), "recover")?;
                }
                Ok(())
            }
            StepOutput::SprintPlan(p) => check_plan(p),
            StepOutput::Facts(f) => filled("facts_path", &f.facts_path),
            StepOutput::SprintReview(r) => {
                for k in &r.key_results {
                    filled("key_results.text", &k.text)?;
                    filled("key_results.evidence", &k.evidence)?;
                }
                for k in &r.knowledge {
                    let file = std::path::Path::new(&k.file);
                    let inside = file
                        .components()
                        .all(|c| matches!(c, std::path::Component::Normal(_)));
                    if !inside || file.extension().is_none_or(|x| x != "md") {
                        return Err(format!(
                            "knowledge.file: {} is not a Markdown file under the knowledge folder",
                            k.file
                        ));
                    }
                }
                filled("message", &r.message)
            }
            StepOutput::Conversation(c) => filled("reply", &c.reply),
            StepOutput::RunCommands(r) => check_run_commands(r),
            StepOutput::Analysis(_)
            | StepOutput::DocumentReview(_)
            | StepOutput::Verifier(_)
            | StepOutput::DesignSync(_)
            | StepOutput::TestDesign(_)
            | StepOutput::Executor(_)
            | StepOutput::TestReview(_)
            | StepOutput::Quality(_)
            | StepOutput::SecurityReview(_)
            | StepOutput::RerunCheck(_) => Ok(()),
        }
    }

    /// For a check (a review or verification), what it found; `None` for other steps.
    pub fn check_result(&self) -> Option<CheckResult> {
        Some(match self {
            // An empty actionable issue set completes the review whatever the verdict says; a
            // rejection goes to resolution, which settles its source conflict.
            StepOutput::DocumentReview(r) => found(
                false,
                &r.prior_feedback_reconciliation,
                !r.issues.is_empty() || r.verdict.decision == DocumentVerdict::Rejected,
            ),
            StepOutput::Verifier(v) => found(
                v.summary.status == VerifierStatus::Blocked,
                &[],
                !v.discrepancies.is_empty(),
            ),
            StepOutput::DesignSync(d) => {
                found(false, &[], d.sync_status == SyncStatus::ConflictsFound)
            }
            StepOutput::TestReview(r) => found(
                r.status == ReviewStatus::Blocked,
                &r.prior_feedback_reconciliation,
                !r.quality_issues.is_empty() || r.status == ReviewStatus::NeedsRevision,
            ),
            StepOutput::CodeReview(r) => found(
                r.verdict == CodeVerdict::Blocked,
                &r.prior_feedback_reconciliation,
                !r.findings.is_empty() || r.verdict != CodeVerdict::Pass,
            ),
            StepOutput::SecurityReview(r) => found(
                r.status == ReviewStatus::Blocked,
                &r.prior_feedback_reconciliation,
                !r.findings.is_empty() || r.status == ReviewStatus::NeedsRevision,
            ),
            StepOutput::Requirements(_)
            | StepOutput::ScopeCheck(_)
            | StepOutput::Document(_)
            | StepOutput::Prototype(_)
            | StepOutput::Analysis(_)
            | StepOutput::TestDesign(_)
            | StepOutput::Executor(_)
            | StepOutput::Quality(_)
            | StepOutput::Resolution(_)
            | StepOutput::RerunCheck(_)
            | StepOutput::AgreementsCheck(_)
            | StepOutput::Exception(_)
            | StepOutput::SprintPlan(_)
            | StepOutput::Facts(_)
            | StepOutput::SprintReview(_)
            | StepOutput::Conversation(_)
            | StepOutput::RunCommands(_) => return None,
        })
    }
}

fn check_run_commands(r: &RunCommandsOutput) -> Result<(), String> {
    filled("reason", &r.reason)?;
    let commands = [
        ("setup", &r.setup),
        ("start", &r.start),
        ("ready", &r.ready),
    ];
    for (field, text) in commands {
        optional(field, text.as_deref())?;
    }
    match r.runs {
        Runs::Commands => needed("start", r.start.as_deref(), "commands"),
        Runs::Nothing | Runs::Unknown => match commands.iter().find(|(_, t)| t.is_some()) {
            Some((field, _)) => Err(format!("{field}: only `commands` gives commands")),
            None => Ok(()),
        },
    }
}

fn check_document(d: &DocumentOutput) -> Result<(), String> {
    optional("question", d.question.as_deref())?;
    match d.status {
        DocumentStatus::Blocked => needed("reason", d.reason.as_deref(), "blocked")?,
        DocumentStatus::Completed if d.question.is_none() => {
            if d.path.as_deref().is_none_or(|p| p.trim().is_empty()) && d.paths.is_empty() {
                return Err("path or paths: a completed document names what it wrote".into());
            }
            every("paths", &d.paths)?;
        }
        DocumentStatus::Completed => {}
    }
    declared(&d.approvals)
}

/// The categories a step declares: each once, and never the factory's configuration, which is
/// held by path.
fn declared(approvals: &[Approval]) -> Result<(), String> {
    let mut seen = std::collections::BTreeSet::new();
    for a in approvals {
        if *a == Approval::FactoryConfiguration {
            return Err("approvals: factory-configuration is held by path, not declared".into());
        }
        if !seen.insert(a) {
            return Err(format!("approvals: {} is listed twice", a.as_str()));
        }
    }
    Ok(())
}

fn filled(field: &str, text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        Err(format!("{field} must not be empty"))
    } else {
        Ok(())
    }
}

fn optional(field: &str, text: Option<&str>) -> Result<(), String> {
    text.map_or(Ok(()), |t| filled(field, t))
}

fn needed(field: &str, text: Option<&str>, choice: &str) -> Result<(), String> {
    match text {
        Some(t) => filled(field, t),
        None => Err(format!("{field} is required with {choice}")),
    }
}

fn every(field: &str, texts: &[String]) -> Result<(), String> {
    texts.iter().try_for_each(|t| filled(field, t))
}

fn check_plan(p: &SprintPlanOutput) -> Result<(), String> {
    filled("purpose", &p.purpose)?;
    every("key_results", &p.key_results)?;
    for i in &p.issues {
        filled("issues.key", &i.key)?;
        every("issues.depends_on", &i.depends_on)?;
    }
    runnable(&p.issues)?;
    filled("message", &p.message)
}

/// Whether the plan can run to its end: each key once, and no planned issues waiting on each
/// other. Dependencies outside the plan hold nothing up.
fn runnable(issues: &[PlannedIssue]) -> Result<(), String> {
    let keys: Vec<&str> = issues.iter().map(|i| i.key.as_str()).collect();
    if let Some(key) = keys
        .iter()
        .enumerate()
        .find_map(|(n, k)| keys.get(..n)?.contains(k).then_some(k))
    {
        return Err(format!("issues: {key} is planned twice"));
    }
    let mut started: Vec<&str> = Vec::new();
    while started.len() < issues.len() {
        let ready = issues.iter().find(|i| {
            !started.contains(&i.key.as_str())
                && i.depends_on
                    .iter()
                    .all(|d| started.contains(&d.as_str()) || !keys.contains(&d.as_str()))
        });
        let Some(next) = ready else {
            let waiting: Vec<&str> = keys
                .iter()
                .copied()
                .filter(|k| !started.contains(k))
                .collect();
            return Err(format!(
                "issues.depends_on: {} wait on each other",
                waiting.join(", ")
            ));
        };
        started.push(&next.key);
    }
    Ok(())
}
