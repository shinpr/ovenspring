//! The issue flow: given an issue's state and one new fact, the next state and the actions the
//! daemon carries out. No I/O and no judgment: every judgment is an agent's, and arrives here as
//! the validated output of its step.
//!
//! The steps and their order are the sources' implementation workflow by Structural Scale:
//! routing that an output fixes is decided here, judgment is a lead launch.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::config::Role;
use crate::output::{
    self, Approval, CheckResult, Decision, Disposition, DocumentStatus, ExceptionAction,
    ExecutorStatus, FindingDisposition, QualityStatus, Review, Scale, StepOutput,
};
pub use crate::step::Step;
use crate::step::{Check, Document};

/// A task file `decompose` wrote, in the order its dependencies allow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedTask {
    pub path: String,
}

/// Where an issue is. A waiting stage has no item on the board; every other stage has the items
/// `calls_for` names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "stage", rename_all = "kebab-case")]
pub enum Stage {
    Queued,
    /// The item of `step` is open or doing.
    Running {
        step: Step,
    },
    /// Waiting for the owner's answer; `step` runs again with the answer.
    Asking {
        step: Step,
    },
    /// The lead asked the owner about `cause`; it takes the cause again with the answer.
    LeadAsking {
        cause: Cause,
        resume: Option<Box<Stage>>,
    },
    /// A daemon operation stopped the issue while the lead or the owner already had `cause`, so
    /// the lead cannot take the stop: waiting for the owner to resolve it.
    Stopped {
        cause: Cause,
        resume: Option<Box<Stage>>,
    },
    /// Waiting for the owner on the lead's recovery procedure for `cause`.
    AwaitingRecovery {
        cause: Cause,
        resume: Option<Box<Stage>>,
    },
    /// Waiting for the owner to approve the PRD, the prototype or what a reviewed design adds.
    AwaitingApproval {
        of: Approve,
    },
    /// The post-implementation review round: its pending reviews run in parallel.
    Reviewing,
    /// The daemon commits the work that passed `after`; what follows waits for the commit.
    Committing {
        after: CommitPoint,
    },
    Pushing,
    Ci,
    WaitingBots,
    WaitingReview,
    Gate,
    /// The gate holds the merge until the owner approves categories for this head.
    HeldForOwner,
    /// Waiting for the owner to merge (`merge.by = owner`).
    ReadyForOwner,
    /// Everything the factory checks holds, but GitHub still stops the merge.
    BlockedOnGitHub,
    /// Held for the owner at the end of the sprint.
    Held,
    Merged,
    Dropped,
    /// The lead is finding a way past `cause`. `resume` is the state a stopped daemon operation
    /// returns to, kept while the lead or the owner has the issue.
    Exception {
        cause: Cause,
        resume: Option<Box<Stage>>,
    },
}

/// What the owner approves before the issue goes on. Serialized by the step that wrote it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Approve {
    Prd,
    Prototype,
    Design,
}

impl Approve {
    /// The step that writes what is approved, run again with the owner's reply.
    pub fn step(self) -> Step {
        match self {
            Approve::Prd => Step::Prd,
            Approve::Prototype => Step::Prototype,
            Approve::Design => Step::Design,
        }
    }
}

/// Where a commit falls in the flow: after the work that passed a document's checks or a quality
/// run, or before a push the repository refused, once the quality run has corrected what it
/// reported. Serialized by the name of the step whose work passed, or `push`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CommitPoint {
    PrdReview,
    UiSpecReview,
    AdrReview,
    DesignSync,
    Quality,
    FixCi,
    MergeBase,
    Push,
}

impl CommitPoint {
    /// The step whose work passed; none for a push.
    pub fn step(self) -> Option<Step> {
        match self {
            CommitPoint::PrdReview => Some(Step::PrdReview),
            CommitPoint::UiSpecReview => Some(Step::UiSpecReview),
            CommitPoint::AdrReview => Some(Step::AdrReview),
            CommitPoint::DesignSync => Some(Step::DesignSync),
            CommitPoint::Quality => Some(Step::Quality),
            CommitPoint::FixCi => Some(Step::FixCi),
            CommitPoint::MergeBase => Some(Step::MergeBase),
            CommitPoint::Push => None,
        }
    }

    fn after_quality(step: Step) -> Option<CommitPoint> {
        match step {
            Step::Quality => Some(CommitPoint::Quality),
            Step::FixCi => Some(CommitPoint::FixCi),
            Step::MergeBase => Some(CommitPoint::MergeBase),
            Step::Requirements
            | Step::Prd
            | Step::PrdReview
            | Step::CodebaseAnalysis
            | Step::ScopeCheck
            | Step::Prototype
            | Step::UiAnalysis
            | Step::UiSpec
            | Step::UiSpecReview
            | Step::Adr
            | Step::AdrReview
            | Step::Design
            | Step::DesignVerify
            | Step::DesignReview
            | Step::DesignSync
            | Step::TestDesign
            | Step::WorkPlan
            | Step::PlanReview
            | Step::Decompose
            | Step::Implement
            | Step::TestReview
            | Step::CodeReview
            | Step::SecurityReview
            | Step::RerunCheck
            | Step::ReviewResponse
            | Step::Exception
            | Step::SprintPlan
            | Step::Facts
            | Step::SprintReview
            | Step::AgreementsCheck
            | Step::Conversation
            | Step::RunCommands => None,
        }
    }
}

impl fmt::Display for CommitPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            CommitPoint::PrdReview => "prd-review",
            CommitPoint::UiSpecReview => "ui-spec-review",
            CommitPoint::AdrReview => "adr-review",
            CommitPoint::DesignSync => "design-sync",
            CommitPoint::Quality => "quality",
            CommitPoint::FixCi => "fix-ci",
            CommitPoint::MergeBase => "merge-base",
            CommitPoint::Push => "push",
        })
    }
}

/// What the lead's `exception` finds a way past.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "cause", rename_all = "kebab-case")]
pub enum Cause {
    /// A step could not finish.
    Step { step: Step },
    /// Reviews still had findings when their loop ended: an applied finding maintained twice, or
    /// the owner's round limit.
    Unsettled { step: Step },
    /// An issue this one depends on was held or dropped.
    DependencyLost,
    /// Someone closed the pull request without merging it.
    PullClosed,
    /// The repository refused the push, and the quality runs could not correct it.
    Push,
    /// A daemon operation failed on two passes in a row; the issue returns to the state it
    /// stopped in.
    Operation { operation: String },
}

impl fmt::Display for Cause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Cause::Step { step } | Cause::Unsettled { step } => step.fmt(f),
            Cause::DependencyLost => f.write_str("an issue it depends on"),
            Cause::PullClosed => f.write_str("its pull request"),
            Cause::Push => f.write_str("the push"),
            Cause::Operation { operation } => f.write_str(operation),
        }
    }
}

impl Stage {
    pub fn running(step: Step) -> Stage {
        Stage::Running { step }
    }

    pub fn is_finished(&self) -> bool {
        matches!(self, Stage::Merged | Stage::Dropped | Stage::Held)
    }
}

/// Whose findings are being corrected, and so who receives them and what rechecks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Origin {
    /// A document check (`prd-review`, `ui-spec-review`, `adr-review`, `design-verify`,
    /// `design-review`, `plan-review`): its author corrects, the same check rechecks.
    Document,
    /// `test-review`: the executor corrects, the test review rechecks.
    TestReview,
    /// The post-implementation review round.
    Round,
    /// Accepted pull request comments: the executor corrects, and the pull request is pushed.
    PrComments,
}

/// What follows a converged `test-review`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum After {
    /// The next task, or the final quality run.
    NextTask,
    /// The quality run after a correction of the review round or of pull request comments.
    /// Retained for issues saved by earlier versions: a correction no longer passes a test review.
    Correction(Origin),
    /// The review round, after the Design Doc its author corrected passed its own checks.
    ReturnToRound,
}

/// The post-implementation review round.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Round {
    /// The round's number: 1, then one more for each recheck after a correction.
    pub number: u32,
    /// Reviews filed and not yet ended.
    pub pending: BTreeSet<Review>,
    /// Code reviews waiting for one of the three parallel places.
    pub queued: Vec<Review>,
    /// Whether `rerun-check` is filed and not yet ended.
    pub rerun_check: bool,
    /// Reviews that ended with new findings.
    pub findings: BTreeSet<Review>,
    /// Reviews whose correction re-review maintained an applied finding.
    pub maintained: BTreeSet<Review>,
    /// Reviews whose last result passed; retained unless `rerun-check` names them.
    pub passed: BTreeSet<Review>,
    /// Reviews that own a finding under correction: they recheck after it.
    pub correcting: BTreeSet<Review>,
    /// Finish an author handoff already scheduled by a pre-upgrade review round.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub author_follows: bool,
    /// A step of the round that could not finish: a review, or `rerun-check`.
    pub stuck: Option<Step>,
    /// Consecutive corrections whose applied finding a re-review maintained.
    #[serde(default)]
    pub kept: u32,
}

/// The documents an issue's scope calls for, beyond the Design Doc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Documents {
    /// The scope check found a UI Spec applies.
    pub ui_spec: bool,
    /// The scope check found decision points the owner answered, for an ADR batch.
    pub adr: bool,
    /// The owner approved this issue's PRD.
    pub prd_approved: bool,
}

/// What the daemon keeps about an issue between facts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IssueState {
    pub key: String,
    pub stage: Stage,
    pub scale: Option<Scale>,
    pub documents: Documents,
    pub approvals: Vec<Approval>,
    pub tasks: Vec<PlannedTask>,
    /// The task being implemented (index into `tasks`).
    pub task: usize,
    /// Tasks the quality run found incomplete, to implement again before it reruns.
    pub redo: Vec<usize>,
    /// Implementing `redo`: the quality run follows the last of them.
    pub redoing: bool,
    /// Number of post-implementation review units (1 unless a large change is split).
    pub units: usize,
    /// Whose findings the current correction belongs to.
    pub origin: Option<Origin>,
    /// The check whose findings are being corrected (a document check or `test-review`).
    pub check: Option<Check>,
    /// What follows a converged `test-review`.
    pub after: Option<After>,
    /// Consecutive corrections of `check` whose applied finding its re-review maintained.
    pub attempts: u32,
    /// A refused commit or push being corrected: the commit point it returns to, and how many
    /// quality runs it has had.
    pub refused: Option<(CommitPoint, u32)>,
    pub round: Round,
    pub review_round: u32,
    pub fix_ci_round: u32,
    pub pr: Option<u64>,
}

impl IssueState {
    /// An author runs again for applied findings, or a check rechecks after a correction.
    pub fn correcting(&self) -> bool {
        self.check.is_some() || self.origin == Some(Origin::Round)
    }

    pub fn new(key: &str) -> IssueState {
        IssueState {
            key: key.into(),
            stage: Stage::Queued,
            scale: None,
            documents: Documents::default(),
            approvals: vec![],
            tasks: vec![],
            task: 0,
            redo: vec![],
            redoing: false,
            units: 1,
            origin: None,
            check: None,
            after: None,
            attempts: 0,
            refused: None,
            round: Round::default(),
            review_round: 0,
            fix_ci_round: 0,
            pr: None,
        }
    }

    /// The state a stopped daemon operation returns to, while the lead or the owner has the issue.
    pub fn resume(&self) -> Option<&Stage> {
        match &self.stage {
            Stage::Exception { resume, .. }
            | Stage::LeadAsking { resume, .. }
            | Stage::Stopped { resume, .. }
            | Stage::AwaitingRecovery { resume, .. } => resume.as_deref(),
            Stage::Queued
            | Stage::Running { .. }
            | Stage::Asking { .. }
            | Stage::AwaitingApproval { .. }
            | Stage::Reviewing
            | Stage::Committing { .. }
            | Stage::Pushing
            | Stage::Ci
            | Stage::WaitingBots
            | Stage::WaitingReview
            | Stage::Gate
            | Stage::BlockedOnGitHub
            | Stage::HeldForOwner
            | Stage::ReadyForOwner
            | Stage::Held
            | Stage::Merged
            | Stage::Dropped => None,
        }
    }
}

/// Something that happened to an issue.
#[derive(Debug, Clone, PartialEq)]
pub enum Fact {
    /// Every issue this one depends on is merged.
    DependenciesMerged,
    /// An issue this one depends on was held or dropped.
    DependencyLost {
        dependency: String,
    },
    /// A step's item ended `done` with this validated output; `unit` names a code review's unit.
    Done {
        step: Step,
        unit: Option<usize>,
        output: StepOutput,
    },
    /// `decompose` is done: its task files in dependency order and the review units the daemon
    /// built from their `Dependencies` (each unit lists task indexes; empty for one unit).
    Planned {
        tasks: Vec<PlannedTask>,
        units: Vec<Vec<usize>>,
    },
    /// The quality run found incomplete implementation owned by these tasks (empty for the
    /// direct scope).
    Stubs {
        tasks: Vec<usize>,
    },
    /// A design-phase step's preparation turn ended: `asks` when its message asks the owner
    /// something the work needs answered; otherwise its work turn follows at once.
    Prepared {
        step: Step,
        asks: bool,
    },
    /// A step's item ended `blocked`, or its process died twice.
    Stuck {
        step: Step,
        unit: Option<usize>,
    },
    /// The owner answered the question or approved the request this issue waits on.
    OwnerAnswered,
    /// The owner replied to the PRD, design or prototype they were asked to approve, instead of
    /// approving it.
    OwnerReplied,
    /// The owner held the merge the gate asked them to approve, for the sprint review.
    OwnerHeld,
    /// A sprint the owner approved takes the held issue up again.
    TakenUp,
    /// The factory pushed the issue branch at this commit (and opened or updated the pull request).
    Pushed,
    ChecksPassed,
    ChecksFailed,
    /// The wait for review bots ended.
    BotWaitEnded {
        comments: bool,
    },
    /// New accepted comments arrived on the pull request.
    CommentsArrived,
    /// Why the gate does not merge the current head (see `gate`).
    Gate(crate::gate::Wait),
    /// The pull request is merged: the daemon merged it, or GitHub reports it merged.
    MergedOnGitHub,
    /// The daemon committed the work that passed `after`, or found nothing to commit.
    Committed {
        after: CommitPoint,
    },
    /// The repository refused the commit of the work that passed `after` (a hook, most often).
    CommitFailed {
        after: CommitPoint,
    },
    /// The push reached `origin` and was refused (a hook, or the remote).
    PushRefused,
    /// Bringing the branch's current head or the base into the worktree before a push conflicted.
    PushConflict,
    /// Someone closed the pull request without merging it.
    PullClosed,
    /// The same daemon operation failed for this issue on two passes in a row.
    Stopped {
        operation: String,
    },
    /// What stopped the issue was cleared (the owner resolved it, a recovery ran, the daemon
    /// restarted): it returns to the state it stopped in.
    Resume,
    /// The recovery procedure the owner approved exited with an error.
    RecoveryFailed,
    /// The owner canceled the issue: whatever it was doing, it ends here.
    Canceled,
}

/// An item the daemon files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filing {
    pub step: Step,
    pub role: Role,
    /// The check's round, for a review or verification (a recheck is round 2 and later).
    pub round: Option<u32>,
    /// A code review's unit.
    pub unit: Option<usize>,
    /// The task file `implement` carries out; `None` for the direct scope.
    pub task: Option<usize>,
}

/// What the daemon does next.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    File(Filing),
    /// Commit the worktree: the work before it passed its review or quality check. `after` names
    /// that step.
    Commit {
        after: CommitPoint,
    },
    /// Stop the issue's app.
    StopApp,
    Push,
    /// Put this question to the owner and wait (the lead's output carries the text).
    AskOwner,
    /// Ask the owner to approve the PRD, the prototype, what the design adds, or a held merge.
    RequestApproval,
    WaitForBots,
    /// Reply in the threads of comments judged without applying any. A push answers a batch that
    /// applied some, once it carries the commit that applied them.
    AnswerComments,
    RunGate,
    /// Tell the owner the pull request is ready to merge.
    NotifyReadyForOwner,
    /// Comment the lead's evidence on the tracker issue and close it.
    CloseOnTracker,
    /// Close the issue's pull request, when it has one; its branch stays.
    ClosePull,
    /// Assign the issue to the factory's account and mark it started on the tracker.
    StartOnTracker,
    /// Wake the issues that depend on this one.
    NotifyDependents,
    /// The issue stopped and the lead cannot take it: tell the owner what stopped it.
    ReportStopped,
    /// Ask the owner to approve the lead's recovery procedure.
    RequestRecovery,
    /// Record that review round `number` completed: its findings go to correction, or it
    /// converged.
    RecordReviewRound {
        number: u32,
    },
}

/// The limits that change the flow; `None` means no limit (the default).
#[derive(Debug, Clone, Copy, Default)]
pub struct Limits {
    pub review_rounds: Option<u32>,
    pub fix_ci_rounds: Option<u32>,
}

/// Code reviews of one issue that run at once (Post-Implementation Review Units).
const PARALLEL_CODE_REVIEWS: usize = 3;

/// The item `step` runs as in this state: its check round and task.
pub fn filing(s: &IssueState, step: Step, unit: Option<usize>) -> Filing {
    let task = (step == Step::Implement)
        .then(|| s.tasks.get(s.task).map(|_| s.task))
        .flatten();
    Filing {
        step,
        role: step.role(),
        round: if matches!(step, Step::CodeReview | Step::SecurityReview) {
            Some(s.round.number.max(1))
        } else {
            step.is_check().then_some(s.review_round.max(1))
        },
        unit,
        task,
    }
}

/// The items an issue in this state must have open or doing: what the daemon files again when one
/// is missing.
pub fn calls_for(s: &IssueState) -> Vec<Filing> {
    match &s.stage {
        Stage::Running { step } => vec![filing(s, *step, None)],
        Stage::Exception { .. } => vec![filing(s, Step::Exception, None)],
        Stage::Reviewing => {
            let mut items: Vec<Filing> = s
                .round
                .pending
                .iter()
                .map(|r| filing(s, r.step(), r.unit()))
                .collect();
            if s.round.rerun_check {
                items.push(filing(s, Step::RerunCheck, None));
            }
            items
        }
        Stage::Queued
        | Stage::Asking { .. }
        | Stage::LeadAsking { .. }
        | Stage::Stopped { .. }
        | Stage::AwaitingRecovery { .. }
        | Stage::AwaitingApproval { .. }
        | Stage::Committing { .. }
        | Stage::Pushing
        | Stage::Ci
        | Stage::WaitingBots
        | Stage::WaitingReview
        | Stage::Gate
        | Stage::BlockedOnGitHub
        | Stage::HeldForOwner
        | Stage::ReadyForOwner
        | Stage::Held
        | Stage::Merged
        | Stage::Dropped => vec![],
    }
}

fn run(s: &mut IssueState, step: Step) -> Vec<Action> {
    s.stage = Stage::running(step);
    vec![Action::File(filing(s, step, None))]
}

fn exception(s: &mut IssueState, cause: Cause) -> Vec<Action> {
    to_lead(s, cause, None)
}

fn to_lead(s: &mut IssueState, cause: Cause, resume: Option<Box<Stage>>) -> Vec<Action> {
    s.stage = Stage::Exception { cause, resume };
    vec![Action::File(filing(s, Step::Exception, None))]
}

/// The lead finds the cause of what `step` could not finish.
fn step_failed(s: &mut IssueState, step: Step) -> Vec<Action> {
    exception(s, Cause::Step { step })
}

/// A daemon operation stopped the issue: the lead takes it, with the state kept to return to. When
/// the lead or the owner already has the issue, the owner is told instead and the issue waits for
/// them.
fn stopped(s: &mut IssueState, operation: &str) -> Vec<Action> {
    let operation = Cause::Operation {
        operation: operation.into(),
    };
    let (cause, resume) = match s.stage.clone() {
        Stage::Exception { cause, resume }
        | Stage::LeadAsking { cause, resume }
        | Stage::AwaitingRecovery { cause, resume } => (cause, resume),
        Stage::Stopped { .. } => return vec![Action::ReportStopped],
        // The step's own question waits on the owner: the stop goes to them with it.
        asking @ Stage::Asking { .. } => (operation, Some(Box::new(asking))),
        stage @ (Stage::Queued
        | Stage::Running { .. }
        | Stage::AwaitingApproval { .. }
        | Stage::Reviewing
        | Stage::Committing { .. }
        | Stage::Pushing
        | Stage::Ci
        | Stage::WaitingBots
        | Stage::WaitingReview
        | Stage::Gate
        | Stage::BlockedOnGitHub
        | Stage::HeldForOwner
        | Stage::ReadyForOwner
        | Stage::Held
        | Stage::Merged
        | Stage::Dropped) => return to_lead(s, operation, Some(Box::new(stage))),
    };
    s.stage = Stage::Stopped { cause, resume };
    vec![Action::ReportStopped]
}

/// What stopped the issue was cleared. A stopped daemon operation returns to the state it stopped
/// in; a recovered failure runs again, as `retry` would run it; a stop the owner resolved returns
/// the failure to the lead.
fn resume(s: &mut IssueState) -> Vec<Action> {
    if let Some(stage) = s.resume().cloned() {
        return return_to(s, stage);
    }
    match s.stage.clone() {
        Stage::AwaitingRecovery { cause, .. } => {
            s.stage = Stage::Exception {
                cause,
                resume: None,
            };
            after_exception(s, ExceptionAction::Retry)
        }
        Stage::Stopped { cause, .. } => exception(s, cause),
        Stage::Queued
        | Stage::Running { .. }
        | Stage::Asking { .. }
        | Stage::LeadAsking { .. }
        | Stage::AwaitingApproval { .. }
        | Stage::Reviewing
        | Stage::Committing { .. }
        | Stage::Pushing
        | Stage::Ci
        | Stage::WaitingBots
        | Stage::WaitingReview
        | Stage::Gate
        | Stage::BlockedOnGitHub
        | Stage::HeldForOwner
        | Stage::ReadyForOwner
        | Stage::Held
        | Stage::Merged
        | Stage::Dropped
        | Stage::Exception { .. } => vec![],
    }
}

/// The issue returns to the state a stopped operation left. A step's question was superseded
/// when the issue resumed: the step runs again and asks again.
fn return_to(s: &mut IssueState, stage: Stage) -> Vec<Action> {
    if let Stage::Asking { step } = stage {
        return run(s, step);
    }
    s.stage = stage;
    vec![]
}

/// A design-phase step's preparation ended: the owner answers first when it asked, otherwise the
/// work turn follows.
fn prepared(s: &mut IssueState, step: Step, asks: bool) -> Vec<Action> {
    if s.stage != Stage::running(step) {
        return vec![];
    }
    if asks { ask(s, step) } else { run(s, step) }
}

fn ask(s: &mut IssueState, step: Step) -> Vec<Action> {
    s.stage = Stage::Asking { step };
    vec![Action::AskOwner]
}

fn await_approval(s: &mut IssueState, of: Approve) -> Vec<Action> {
    s.stage = Stage::AwaitingApproval { of };
    vec![Action::RequestApproval]
}

/// The first round of a check.
fn check(s: &mut IssueState, check: Check) -> Vec<Action> {
    s.review_round = 1;
    s.attempts = 0;
    s.check = Some(check);
    run(s, check.step())
}

/// The next state and actions for one fact. Facts that do not apply to the current stage change
/// nothing; the daemon logs them.
pub fn next(state: &IssueState, fact: &Fact, limits: Limits) -> (IssueState, Vec<Action>) {
    let mut s = state.clone();
    let actions = match fact {
        Fact::Canceled => canceled(&mut s),
        Fact::DependenciesMerged if s.stage == Stage::Queued => {
            let mut actions = vec![Action::StartOnTracker];
            actions.extend(run(&mut s, Step::Requirements));
            actions
        }
        Fact::DependencyLost { .. } if s.stage == Stage::Queued => {
            exception(&mut s, Cause::DependencyLost)
        }
        Fact::Done { .. } | Fact::Planned { .. } | Fact::Stubs { .. } | Fact::Stuck { .. } => {
            item_ended(&mut s, fact, limits)
        }
        Fact::Committed { after } if s.stage == (Stage::Committing { after: *after }) => {
            after_commit(&mut s, *after, limits)
        }
        Fact::CommitFailed { after } if s.stage == (Stage::Committing { after: *after }) => {
            refused(&mut s, *after)
        }
        Fact::PushRefused if s.stage == Stage::Pushing => refused(&mut s, CommitPoint::Push),
        Fact::PushConflict if s.stage == Stage::Pushing => run(&mut s, Step::MergeBase),
        // Once the lead or the owner has the closed pull request, the next look changes nothing.
        // A held issue's pull request stays open; a close by anyone reaches the lead like any other.
        Fact::PullClosed
            if !matches!(s.stage, Stage::Merged | Stage::Dropped)
                && !matches!(
                    s.stage,
                    Stage::Exception { .. }
                        | Stage::Asking { .. }
                        | Stage::LeadAsking { .. }
                        | Stage::Stopped { .. }
                        | Stage::AwaitingRecovery { .. }
                ) =>
        {
            exception(&mut s, Cause::PullClosed)
        }
        Fact::Stopped { operation } if !s.stage.is_finished() => stopped(&mut s, operation),
        Fact::Resume => resume(&mut s),
        Fact::RecoveryFailed => recovery_failed(&mut s),
        Fact::OwnerAnswered => answered(&mut s),
        Fact::Prepared { step, asks } => prepared(&mut s, *step, *asks),
        // The author takes the reply into the document or prototype it asked approval for.
        Fact::OwnerReplied => {
            if let Stage::AwaitingApproval { of } = s.stage {
                run(&mut s, of.step())
            } else {
                vec![]
            }
        }
        Fact::OwnerHeld if s.stage == Stage::HeldForOwner => {
            s.stage = Stage::Held;
            vec![Action::NotifyDependents]
        }
        Fact::TakenUp if s.stage == Stage::Held => {
            s.stage = Stage::Gate;
            vec![Action::RunGate]
        }
        Fact::DependenciesMerged
        | Fact::DependencyLost { .. }
        | Fact::OwnerHeld
        | Fact::TakenUp
        | Fact::Pushed
        | Fact::ChecksPassed
        | Fact::ChecksFailed
        | Fact::BotWaitEnded { .. }
        | Fact::CommentsArrived
        | Fact::Gate(_)
        | Fact::MergedOnGitHub
        | Fact::Committed { .. }
        | Fact::CommitFailed { .. }
        | Fact::PushRefused
        | Fact::PushConflict
        | Fact::PullClosed
        | Fact::Stopped { .. } => pull_request(&mut s, fact, limits),
    };
    (s, actions)
}

/// An item of the issue ended: its output, the task files `decompose` wrote, the stubs the quality
/// run found, or a step that could not finish.
fn item_ended(s: &mut IssueState, fact: &Fact, limits: Limits) -> Vec<Action> {
    let reviewing = s.stage == Stage::Reviewing;
    let in_exception = matches!(s.stage, Stage::Exception { .. });
    match fact {
        Fact::Done { step, unit, output } if reviewing => match (step, Review::of(*step, *unit)) {
            (Step::RerunCheck, _) => rerun_checked(s, output),
            (_, Some(review)) => reviewed(s, review, output),
            _ => vec![],
        },
        Fact::Done { step, output, .. }
            if s.stage == Stage::running(*step) || (in_exception && *step == Step::Exception) =>
        {
            done(s, *step, output, limits)
        }
        Fact::Planned { tasks, units } if s.stage == Stage::running(Step::Decompose) => {
            s.tasks.clone_from(tasks);
            s.units = units.len().max(1);
            s.task = 0;
            if s.tasks.is_empty() {
                step_failed(s, Step::Decompose)
            } else {
                run(s, Step::Implement)
            }
        }
        Fact::Stubs { tasks } if s.stage == Stage::running(Step::Quality) => {
            if s.tasks.is_empty() {
                return run(s, Step::Implement);
            }
            s.redo = tasks.iter().rev().copied().collect();
            s.redoing = true;
            next_task_or_redo(s)
        }
        Fact::Stuck { step, unit } if reviewing => {
            if let Some(review) = Review::of(*step, *unit) {
                s.round.pending.remove(&review);
                s.round.correcting.insert(review);
            } else if *step == Step::RerunCheck {
                s.round.rerun_check = false;
                s.round.stuck = Some(*step);
            }
            s.round.stuck.get_or_insert(*step);
            round_progress(s)
        }
        // The lead could not find a way forward either: the way out is the owner's.
        Fact::Stuck { .. } if in_exception => lead_asks(s),
        Fact::Stuck { step, .. } if s.stage == Stage::running(*step) => step_failed(s, *step),
        Fact::Done { .. }
        | Fact::Planned { .. }
        | Fact::Stubs { .. }
        | Fact::Stuck { .. }
        | Fact::DependenciesMerged
        | Fact::DependencyLost { .. }
        | Fact::OwnerAnswered
        | Fact::Prepared { .. }
        | Fact::OwnerReplied
        | Fact::OwnerHeld
        | Fact::TakenUp
        | Fact::Pushed
        | Fact::ChecksPassed
        | Fact::ChecksFailed
        | Fact::BotWaitEnded { .. }
        | Fact::CommentsArrived
        | Fact::Gate(_)
        | Fact::MergedOnGitHub
        | Fact::Committed { .. }
        | Fact::CommitFailed { .. }
        | Fact::PushRefused
        | Fact::PushConflict
        | Fact::PullClosed
        | Fact::Stopped { .. }
        | Fact::Resume
        | Fact::RecoveryFailed
        | Fact::Canceled => vec![],
    }
}

fn done(s: &mut IssueState, step: Step, output: &StepOutput, limits: Limits) -> Vec<Action> {
    if let Some(result) = output.check_result() {
        return checked(s, step, result);
    }
    match output {
        StepOutput::Requirements(r) => requirements_done(s, r),
        StepOutput::ScopeCheck(c) => scope_checked(s, c),
        StepOutput::Document(d) => document_done(s, step, d, limits),
        StepOutput::Analysis(_) if step == Step::CodebaseAnalysis => run(s, Step::ScopeCheck),
        StepOutput::Analysis(_) => run(s, Step::UiSpec),
        StepOutput::Prototype(_) => await_approval(s, Approve::Prototype),
        StepOutput::TestDesign(t) if t.status == DocumentStatus::Blocked => step_failed(s, step),
        StepOutput::TestDesign(_) => run(s, Step::WorkPlan),
        StepOutput::Executor(e) => executed(s, step, e, limits),
        StepOutput::Quality(q) => match q.status {
            // Its owning tasks arrive as `Fact::Stubs`.
            QualityStatus::StubDetected if step == Step::Quality => vec![],
            QualityStatus::Blocked | QualityStatus::StubDetected => step_failed(s, step),
            QualityStatus::Pass | QualityStatus::VerificationIncomplete => {
                // The refusal it corrected: commit again, or push again after committing.
                match s
                    .refused
                    .map(|(after, _)| after)
                    .or(CommitPoint::after_quality(step))
                {
                    Some(after) => commit_then(s, after),
                    None => vec![],
                }
            }
        },
        StepOutput::Exception(e) => after_exception(s, e.action),
        StepOutput::DocumentReview(_)
        | StepOutput::Verifier(_)
        | StepOutput::DesignSync(_)
        | StepOutput::TestReview(_)
        | StepOutput::CodeReview(_)
        | StepOutput::SecurityReview(_)
        | StepOutput::RerunCheck(_)
        | StepOutput::AgreementsCheck(_)
        | StepOutput::SprintPlan(_)
        | StepOutput::Facts(_)
        | StepOutput::SprintReview(_)
        | StepOutput::Conversation(_)
        | StepOutput::RunCommands(_) => vec![],
    }
}

fn requirements_done(s: &mut IssueState, r: &crate::output::RequirementsOutput) -> Vec<Action> {
    match r.decision {
        Decision::Ask => ask(s, Step::Requirements),
        Decision::Close => {
            s.stage = Stage::Dropped;
            vec![Action::CloseOnTracker, Action::NotifyDependents]
        }
        Decision::Proceed => {
            s.scale = Some(r.structural_scale);
            match r.structural_scale {
                Scale::Large => run(s, Step::Prd),
                // A change that alters what an existing PRD states updates it first.
                Scale::Small | Scale::Medium | Scale::Unresolved if r.prd_update.is_some() => {
                    run(s, Step::Prd)
                }
                Scale::Small => run(s, Step::Implement),
                Scale::Medium | Scale::Unresolved => run(s, Step::CodebaseAnalysis),
            }
        }
    }
}

fn scope_checked(s: &mut IssueState, c: &crate::output::ScopeCheckOutput) -> Vec<Action> {
    if c.decision == Decision::Ask {
        return ask(s, Step::ScopeCheck);
    }
    s.scale = Some(c.structural_scale);
    s.documents.ui_spec = c.ui_spec;
    s.documents.adr = !c.adr_decision_points.is_empty();
    match c.structural_scale {
        Scale::Small => run(s, Step::Implement),
        Scale::Large if !s.documents.prd_approved => run(s, Step::Prd),
        Scale::Medium | Scale::Large | Scale::Unresolved => before_design(s),
    }
}

fn document_done(
    s: &mut IssueState,
    step: Step,
    d: &crate::output::DocumentOutput,
    limits: Limits,
) -> Vec<Action> {
    if d.status == DocumentStatus::Blocked {
        return step_failed(s, step);
    }
    if d.question.is_some() {
        return ask(s, step);
    }
    if step == Step::Design {
        s.approvals.clone_from(&d.approvals);
    }
    // An author run again for applied findings: the check that raised them rechecks.
    if s.correcting() {
        return corrected_by_author(s, &d.dispositions, limits);
    }
    // `decompose` writes no document: it ends through `Fact::Planned`, built from its task files.
    match step.document() {
        Some(document) => check(s, document.first_check()),
        None => vec![],
    }
}

fn executed(
    s: &mut IssueState,
    step: Step,
    e: &crate::output::ExecutorOutput,
    limits: Limits,
) -> Vec<Action> {
    if e.status != ExecutorStatus::Completed {
        return step_failed(s, step);
    }
    if step == Step::ReviewResponse {
        return corrected_by_executor(s, e, limits);
    }
    if test_review_follows(s, e) {
        s.after = Some(After::NextTask);
        return check(s, Check::TestReview);
    }
    next_task_or_redo(s)
}

/// Whether the executor's work passes the test review: when it asks for one, outside a review
/// round. A round's reviews recheck the tests of its correction themselves, the completion of
/// what that correction left incomplete included.
fn test_review_follows(s: &IssueState, e: &crate::output::ExecutorOutput) -> bool {
    e.requires_test_review && s.origin != Some(Origin::Round)
}

/// After the scope check, or after the PRD's analysis: the UI Spec, ADR and design documents.
fn before_design(s: &mut IssueState) -> Vec<Action> {
    if s.documents.ui_spec {
        run(s, Step::Prototype)
    } else if s.documents.adr {
        run(s, Step::Adr)
    } else {
        run(s, Step::Design)
    }
}

fn next_task_or_redo(s: &mut IssueState) -> Vec<Action> {
    if let Some(task) = s.redo.pop() {
        s.task = task;
        return run(s, Step::Implement);
    }
    if s.redoing {
        s.redoing = false;
        s.task = s.tasks.len().saturating_sub(1);
    } else if s.task + 1 < s.tasks.len() {
        s.task += 1;
        return run(s, Step::Implement);
    }
    // A round correction's origin carries its pending rechecks past the quality run.
    if s.origin != Some(Origin::Round) {
        s.origin = None;
    }
    run(s, Step::Quality)
}

/// Commits the work that passed `after`; what follows runs once the commit is made.
fn commit_then(s: &mut IssueState, after: CommitPoint) -> Vec<Action> {
    s.stage = Stage::Committing { after };
    vec![Action::Commit { after }]
}

/// A refused commit or push: the quality run corrects what the repository's checks reported, at
/// most twice in a row before the lead takes it.
fn refused(s: &mut IssueState, after: CommitPoint) -> Vec<Action> {
    let runs = s.refused.as_ref().map_or(0, |(_, n)| *n) + 1;
    if runs > 2 {
        s.refused = None;
        return match after.step() {
            Some(step) => step_failed(s, step),
            None => exception(s, Cause::Push),
        };
    }
    s.refused = Some((after, runs));
    run(s, Step::Quality)
}

/// What follows the commit of the work that passed `after`.
fn after_commit(s: &mut IssueState, after: CommitPoint, limits: Limits) -> Vec<Action> {
    // A refused commit is over once a commit is made; a refused push, once a push is.
    if after != CommitPoint::Push {
        s.refused = None;
    }
    match after {
        CommitPoint::PrdReview => await_approval(s, Approve::Prd),
        CommitPoint::UiSpecReview if s.documents.adr => run(s, Step::Adr),
        CommitPoint::UiSpecReview | CommitPoint::AdrReview => run(s, Step::Design),
        CommitPoint::DesignSync if s.after == Some(After::ReturnToRound) => {
            s.after = None;
            return_to_round(s, limits)
        }
        // A category a person reviews before it merges asks nothing of the design.
        CommitPoint::DesignSync
            if !s
                .approvals
                .iter()
                .any(Approval::is_approved_with_the_design) =>
        {
            run(s, Step::TestDesign)
        }
        CommitPoint::DesignSync => await_approval(s, Approve::Design),
        CommitPoint::Quality => match s.origin {
            Some(Origin::Round) if s.round.author_follows => {
                s.round.author_follows = false;
                run(s, Step::Design)
            }
            Some(Origin::Round) => recheck_round(s, limits),
            Some(Origin::PrComments) => {
                s.origin = None;
                s.stage = Stage::Pushing;
                vec![Action::Push]
            }
            Some(Origin::Document | Origin::TestReview) | None => start_round(s),
        },
        CommitPoint::FixCi | CommitPoint::MergeBase | CommitPoint::Push => {
            s.stage = Stage::Pushing;
            vec![Action::Push]
        }
    }
}

/// A check's result. Findings, and a maintained applied finding, go to the step that wrote what
/// the check reviewed; a clear check moves on.
fn checked(s: &mut IssueState, step: Step, result: CheckResult) -> Vec<Action> {
    let Some(check) = step.check() else {
        return vec![];
    };
    match result {
        CheckResult::Blocked => step_failed(s, step),
        CheckResult::Findings => {
            s.origin = Some(if check == Check::TestReview {
                Origin::TestReview
            } else {
                Origin::Document
            });
            correct(s, check)
        }
        CheckResult::MaintainedApply => {
            s.attempts += 1;
            if s.attempts >= 2 {
                return exception(s, Cause::Unsettled { step });
            }
            correct(s, check)
        }
        CheckResult::Clear => check_cleared(s, check),
    }
}

/// The receiver of the current check's findings, who judges and corrects them: the document's
/// author, run again in update mode, or the executor.
fn correct(s: &mut IssueState, check: Check) -> Vec<Action> {
    run(
        s,
        check
            .document()
            .map_or(Step::ReviewResponse, Document::step),
    )
}

/// The step after a check that found nothing more to correct.
fn check_cleared(s: &mut IssueState, cleared: Check) -> Vec<Action> {
    s.check = None;
    s.attempts = 0;
    match cleared {
        // A document is committed once its checks pass.
        Check::PrdReview => commit_then(s, CommitPoint::PrdReview),
        Check::UiSpecReview => commit_then(s, CommitPoint::UiSpecReview),
        Check::AdrReview => commit_then(s, CommitPoint::AdrReview),
        Check::DesignSync => commit_then(s, CommitPoint::DesignSync),
        // DesignVerify is retained for issues saved by earlier versions.
        Check::DesignReview | Check::DesignVerify => check(s, Check::DesignSync),
        Check::PlanReview => run(s, Step::Decompose),
        Check::TestReview => match s.after.take() {
            Some(After::Correction(origin)) => {
                s.origin = Some(origin);
                run(s, Step::Quality)
            }
            Some(After::NextTask | After::ReturnToRound) | None => next_task_or_redo(s),
        },
    }
}

/// The current check again, as a correction re-review, unless the owner's round limit is reached.
fn recheck(s: &mut IssueState, limits: Limits) -> Vec<Action> {
    let Some(check) = s.check else {
        return vec![];
    };
    s.review_round += 1;
    if limits.review_rounds.is_some_and(|l| s.review_round > l) {
        return exception(s, Cause::Unsettled { step: check.step() });
    }
    run(s, check.step())
}

/// Design sync ends with the author’s judgment. Other checks revisit applied findings.
fn after_judgment(
    s: &mut IssueState,
    dispositions: &[FindingDisposition],
    limits: Limits,
) -> Vec<Action> {
    match s.check {
        Some(Check::DesignSync) => check_cleared(s, Check::DesignSync),
        Some(check) if !output::applies(dispositions) => check_cleared(s, check),
        _ => recheck(s, limits),
    }
}

fn corrected_by_author(
    s: &mut IssueState,
    dispositions: &[FindingDisposition],
    limits: Limits,
) -> Vec<Action> {
    if s.origin != Some(Origin::Round) {
        return after_judgment(s, dispositions, limits);
    }
    if !output::applies(dispositions) {
        return return_to_round(s, limits);
    }
    // The corrected Design Doc passes its own review and design sync before the round goes on.
    s.after = Some(After::ReturnToRound);
    check(s, Check::DesignReview)
}

/// The round after its author correction passed the Design Doc's checks: the reviews recheck.
fn return_to_round(s: &mut IssueState, limits: Limits) -> Vec<Action> {
    s.origin = Some(Origin::Round);
    recheck_round(s, limits)
}

fn corrected_by_executor(
    s: &mut IssueState,
    e: &crate::output::ExecutorOutput,
    limits: Limits,
) -> Vec<Action> {
    let Some(origin @ (Origin::Round | Origin::PrComments)) = s.origin else {
        return after_judgment(s, &e.dispositions, limits);
    };
    // An executor launched before the upgrade could leave edits for the document author.
    // Refile its original findings before clearing them or acknowledging PR comments.
    if output::author_applies(&e.dispositions) {
        return run(s, Step::ReviewResponse);
    }
    let applied = output::applies(&e.dispositions);
    if origin == Origin::Round {
        s.round.correcting = owning_reviews(&s.round, &e.dispositions);
        s.round.findings.clear();
        s.round.maintained.clear();
        if !applied {
            return round_converged(s);
        }
    } else if !applied {
        s.origin = None;
        s.stage = Stage::Gate;
        return vec![Action::AnswerComments, Action::RunGate];
    }
    // Whoever raised the findings reviews the correction again, its tests included.
    run(s, Step::Quality)
}

/// The round's reviews that raised a finding the executor applied: they recheck after it. A
/// disposition that names no review belongs to every review that raised a finding.
fn owning_reviews(round: &Round, dispositions: &[FindingDisposition]) -> BTreeSet<Review> {
    let raised: BTreeSet<Review> = round.findings.union(&round.maintained).copied().collect();
    let mut owning = BTreeSet::new();
    for d in dispositions
        .iter()
        .filter(|d| d.disposition == Disposition::Apply)
    {
        match d.review {
            Some(review) => {
                owning.insert(review);
            }
            None => owning.extend(raised.iter().copied()),
        }
    }
    owning
}

/// Files every review of a fresh round: a code review per unit (three at once) and the security
/// review.
fn start_round(s: &mut IssueState) -> Vec<Action> {
    s.origin = Some(Origin::Round);
    s.round = Round {
        number: 1,
        ..Round::default()
    };
    let reviews: Vec<Review> = (1..=s.units.max(1))
        .map(|unit| Review::CodeReview { unit })
        .chain([Review::SecurityReview])
        .collect();
    file_reviews(s, reviews)
}

fn file_reviews(s: &mut IssueState, reviews: Vec<Review>) -> Vec<Action> {
    s.stage = Stage::Reviewing;
    let mut actions = Vec::new();
    for review in reviews {
        let running_code = s
            .round
            .pending
            .iter()
            .filter(|r| matches!(r, Review::CodeReview { .. }))
            .count();
        if matches!(review, Review::CodeReview { .. }) && running_code >= PARALLEL_CODE_REVIEWS {
            s.round.queued.push(review);
        } else {
            s.round.pending.insert(review);
            actions.push(Action::File(filing(s, review.step(), review.unit())));
        }
    }
    actions
}

/// After a round's correction passed `quality`: the reviews that own a corrected finding recheck,
/// and `rerun-check` runs when a passed review is retained.
fn recheck_round(s: &mut IssueState, limits: Limits) -> Vec<Action> {
    s.round.number += 1;
    s.round.rerun_check = !s.round.passed.is_empty();
    if limits.review_rounds.is_some_and(|l| s.round.number > l) {
        return exception(
            s,
            Cause::Unsettled {
                step: Step::CodeReview,
            },
        );
    }
    file_rechecks(s)
}

fn file_rechecks(s: &mut IssueState) -> Vec<Action> {
    let reviews = std::mem::take(&mut s.round.correcting)
        .into_iter()
        .collect();
    let mut actions = file_reviews(s, reviews);
    if s.round.rerun_check {
        actions.push(Action::File(filing(s, Step::RerunCheck, None)));
    }
    actions
}

fn reviewed(s: &mut IssueState, review: Review, output: &StepOutput) -> Vec<Action> {
    if !s.round.pending.remove(&review) {
        return vec![];
    }
    // What the code shows a change falls under joins what its design declared, for the gate.
    if let StepOutput::CodeReview(r) = output {
        for a in &r.approvals {
            if !s.approvals.contains(a) {
                s.approvals.push(a.clone());
            }
        }
    }
    match output.check_result() {
        Some(CheckResult::Clear) => {
            s.round.passed.insert(review);
        }
        Some(CheckResult::Findings) => {
            s.round.passed.remove(&review);
            s.round.findings.insert(review);
        }
        Some(CheckResult::MaintainedApply) => {
            s.round.maintained.insert(review);
        }
        Some(CheckResult::Blocked) | None => {
            s.round.correcting.insert(review);
            s.round.stuck.get_or_insert(review.step());
        }
    }
    let mut actions = Vec::new();
    if matches!(review, Review::CodeReview { .. }) && !s.round.queued.is_empty() {
        let waiting = s.round.queued.remove(0);
        actions.extend(file_reviews(s, vec![waiting]));
    }
    actions.extend(round_progress(s));
    actions
}

fn rerun_checked(s: &mut IssueState, output: &StepOutput) -> Vec<Action> {
    if !s.round.rerun_check {
        return vec![];
    }
    s.round.rerun_check = false;
    let StepOutput::RerunCheck(r) = output else {
        return vec![];
    };
    let reviews: Vec<Review> = r
        .rerun
        .iter()
        .map(|rerun| rerun.review)
        .filter(|review| s.round.passed.remove(review))
        .collect();
    let mut actions = file_reviews(s, reviews);
    actions.extend(round_progress(s));
    actions
}

/// When every review of the round has ended: findings, and a maintained applied finding, to the
/// executor, or the round is complete.
fn round_progress(s: &mut IssueState) -> Vec<Action> {
    if !s.round.pending.is_empty() || !s.round.queued.is_empty() || s.round.rerun_check {
        return vec![];
    }
    if let Some(step) = s.round.stuck.take() {
        return step_failed(s, step);
    }
    let ended = Action::RecordReviewRound {
        number: s.round.number.max(1),
    };
    if !s.round.findings.is_empty() {
        return [vec![ended], run(s, Step::ReviewResponse)].concat();
    }
    if !s.round.maintained.is_empty() {
        s.round.kept += 1;
        if s.round.kept >= 2 {
            return exception(
                s,
                Cause::Unsettled {
                    step: Step::CodeReview,
                },
            );
        }
        return [vec![ended], run(s, Step::ReviewResponse)].concat();
    }
    [vec![ended], round_converged(s)].concat()
}

fn round_converged(s: &mut IssueState) -> Vec<Action> {
    s.origin = None;
    s.round = Round::default();
    s.stage = Stage::Pushing;
    vec![Action::Push]
}

fn answered(s: &mut IssueState) -> Vec<Action> {
    match s.stage.clone() {
        // The owner's answer to the lead's question, or their reply to a recovery request or a
        // stop: the lead takes the same failure again, with the answer.
        Stage::LeadAsking { cause, resume }
        | Stage::AwaitingRecovery { cause, resume }
        | Stage::Stopped { cause, resume } => to_lead(s, cause, resume),
        Stage::Asking { step } => run(s, step),
        Stage::AwaitingApproval { of } => match of {
            Approve::Prd => {
                s.documents.prd_approved = true;
                if s.scale == Some(Scale::Small) {
                    run(s, Step::Implement)
                } else {
                    run(s, Step::CodebaseAnalysis)
                }
            }
            Approve::Prototype => run(s, Step::UiAnalysis),
            // The categories the design declared stay held: the gate asks the owner again for the
            // commit it would merge.
            Approve::Design => run(s, Step::TestDesign),
        },
        Stage::Queued
        | Stage::Running { .. }
        | Stage::Reviewing
        | Stage::Committing { .. }
        | Stage::Pushing
        | Stage::Ci
        | Stage::WaitingBots
        | Stage::WaitingReview
        | Stage::Gate
        | Stage::BlockedOnGitHub
        | Stage::HeldForOwner
        | Stage::ReadyForOwner
        | Stage::Held
        | Stage::Merged
        | Stage::Dropped
        | Stage::Exception { .. } => vec![],
    }
}

fn recovery_failed(s: &mut IssueState) -> Vec<Action> {
    if let Stage::AwaitingRecovery { cause, resume } = s.stage.clone() {
        return to_lead(s, cause, resume);
    }
    vec![]
}

/// The lead's question, or a lead that could not run: the owner answers, and the lead takes the
/// cause again.
fn lead_asks(s: &mut IssueState) -> Vec<Action> {
    let Stage::Exception { cause, resume } = s.stage.clone() else {
        return vec![];
    };
    s.stage = Stage::LeadAsking { cause, resume };
    vec![Action::AskOwner]
}

fn after_exception(s: &mut IssueState, action: ExceptionAction) -> Vec<Action> {
    let Stage::Exception {
        cause: failed,
        resume,
    } = s.stage.clone()
    else {
        return vec![];
    };
    if let (ExceptionAction::Retry, Some(stage)) = (&action, resume.clone()) {
        // The stopped daemon operation runs again from the state it stopped in.
        return return_to(s, *stage);
    }
    match action {
        ExceptionAction::Retry => retry(s, &failed),
        ExceptionAction::Replan => {
            redirect_author(s, Document::WorkPlan);
            s.tasks.clear();
            s.redo.clear();
            s.redoing = false;
            s.task = 0;
            run(s, Step::WorkPlan)
        }
        ExceptionAction::Redesign => {
            redirect_author(s, Document::Design);
            run(s, Step::Design)
        }
        ExceptionAction::Drop => drop_issue(s),
        ExceptionAction::Ask => lead_asks(s),
        ExceptionAction::Recover => {
            s.stage = Stage::AwaitingRecovery {
                cause: failed,
                resume,
            };
            vec![Action::RequestRecovery]
        }
    }
}

/// Returning to the author of the open check carries the correction and its continuation.
fn redirect_author(s: &mut IssueState, target: Document) {
    if s.origin == Some(Origin::Document) && s.check.and_then(Check::document) == Some(target) {
        s.attempts = 0;
        s.review_round = 0;
        return;
    }
    s.origin = None;
    s.check = None;
    s.after = None;
    s.attempts = 0;
    s.review_round = 0;
    s.round = Round::default();
}

fn retry(s: &mut IssueState, failed: &Cause) -> Vec<Action> {
    match failed {
        Cause::DependencyLost => run(s, Step::Requirements),
        // The branch is pushed again, and a new pull request opened for it if it was closed.
        Cause::PullClosed | Cause::Push => {
            s.stage = Stage::Pushing;
            vec![Action::Push]
        }
        Cause::Step {
            step: step @ (Step::RerunCheck | Step::CodeReview | Step::SecurityReview),
        }
        | Cause::Unsettled {
            step: step @ (Step::RerunCheck | Step::CodeReview | Step::SecurityReview),
        } => {
            s.round.rerun_check |= *step == Step::RerunCheck;
            if s.round.correcting.is_empty() && *step != Step::RerunCheck {
                s.round.correcting = std::mem::take(&mut s.round.maintained);
                // Older snapshots recorded only the failed step; completed siblings keep their results.
                if s.round.correcting.is_empty() {
                    s.round.correcting = (1..=s.units.max(1))
                        .map(|unit| Review::CodeReview { unit })
                        .chain([Review::SecurityReview])
                        .filter(|r| {
                            r.step() == *step
                                && !s.round.passed.contains(r)
                                && !s.round.findings.contains(r)
                        })
                        .collect();
                }
            }
            s.round.kept = 0;
            file_rechecks(s)
        }
        Cause::Step { step } | Cause::Unsettled { step } => {
            if *step == Step::FixCi {
                s.fix_ci_round = 0;
            }
            run(s, *step)
        }
        // An operation stops the issue with the state to return to, which `Retry` resumes.
        Cause::Operation { .. } => vec![],
    }
}

/// New pull request comments go to the executor to judge, each as a finding, like a review's.
fn judge_comments(s: &mut IssueState) -> Vec<Action> {
    s.origin = Some(Origin::PrComments);
    run(s, Step::ReviewResponse)
}

/// Facts about the pushed branch and its pull request.
fn pull_request(s: &mut IssueState, fact: &Fact, limits: Limits) -> Vec<Action> {
    match fact {
        Fact::Pushed if s.stage == Stage::Pushing => {
            s.refused = None;
            s.stage = Stage::Ci;
            vec![]
        }
        Fact::ChecksPassed if s.stage == Stage::Ci => {
            s.stage = Stage::WaitingBots;
            vec![Action::WaitForBots]
        }
        Fact::ChecksFailed if s.stage == Stage::Ci => {
            s.fix_ci_round += 1;
            if limits.fix_ci_rounds.is_some_and(|l| s.fix_ci_round > l) {
                step_failed(s, Step::FixCi)
            } else {
                run(s, Step::FixCi)
            }
        }
        Fact::BotWaitEnded { comments: true } if s.stage == Stage::WaitingBots => judge_comments(s),
        Fact::BotWaitEnded { comments: false } if s.stage == Stage::WaitingBots => {
            s.stage = Stage::Gate;
            vec![Action::RunGate]
        }
        Fact::CommentsArrived
            if matches!(
                s.stage,
                Stage::WaitingReview
                    | Stage::Gate
                    | Stage::HeldForOwner
                    | Stage::ReadyForOwner
                    | Stage::BlockedOnGitHub
            ) =>
        {
            judge_comments(s)
        }
        Fact::Gate(wait)
            if matches!(
                s.stage,
                Stage::Gate | Stage::WaitingReview | Stage::HeldForOwner | Stage::BlockedOnGitHub
            ) =>
        {
            gate(s, wait)
        }
        // The owner may merge a held issue's pull request themselves.
        Fact::MergedOnGitHub if !matches!(s.stage, Stage::Merged | Stage::Dropped) => {
            s.stage = Stage::Merged;
            vec![Action::NotifyDependents]
        }
        Fact::DependenciesMerged
        | Fact::DependencyLost { .. }
        | Fact::Done { .. }
        | Fact::Planned { .. }
        | Fact::Stubs { .. }
        | Fact::Stuck { .. }
        | Fact::OwnerAnswered
        | Fact::Prepared { .. }
        | Fact::OwnerReplied
        | Fact::OwnerHeld
        | Fact::TakenUp
        | Fact::Pushed
        | Fact::ChecksPassed
        | Fact::ChecksFailed
        | Fact::BotWaitEnded { .. }
        | Fact::CommentsArrived
        | Fact::Gate(_)
        | Fact::MergedOnGitHub
        | Fact::Committed { .. }
        | Fact::CommitFailed { .. }
        | Fact::PushRefused
        | Fact::PushConflict
        | Fact::PullClosed
        | Fact::Stopped { .. }
        | Fact::Resume
        | Fact::RecoveryFailed
        | Fact::Canceled => vec![],
    }
}

/// The issue ends without merging: its app stops, its pull request closes with its branch kept,
/// the tracker records why, and the issues that wait on it learn of it.
fn drop_issue(s: &mut IssueState) -> Vec<Action> {
    s.stage = Stage::Dropped;
    vec![
        Action::StopApp,
        Action::ClosePull,
        Action::CloseOnTracker,
        Action::NotifyDependents,
    ]
}

/// The owner's cancel ends any issue that has not ended, a held one included.
fn canceled(s: &mut IssueState) -> Vec<Action> {
    if matches!(s.stage, Stage::Merged | Stage::Dropped) {
        return vec![];
    }
    drop_issue(s)
}

fn gate(s: &mut IssueState, wait: &crate::gate::Wait) -> Vec<Action> {
    use crate::gate::{Return, Wait as G};
    match wait {
        G::Return(Return::Push) => {
            s.stage = Stage::Pushing;
            vec![Action::Push]
        }
        G::Return(Return::Ci) => {
            s.stage = Stage::Ci;
            vec![]
        }
        G::Return(Return::Approvals) => {
            s.stage = Stage::WaitingReview;
            vec![]
        }
        G::HoldForOwner(_) => {
            s.stage = Stage::HeldForOwner;
            vec![Action::RequestApproval]
        }
        G::ReadyForOwner => {
            s.stage = Stage::ReadyForOwner;
            vec![Action::NotifyReadyForOwner]
        }
        G::GitHubBlocks => {
            s.stage = Stage::BlockedOnGitHub;
            vec![]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate::{Return, Wait as G};
    use serde_json::{Value, json};

    fn step_named(name: &str) -> Step {
        name.parse().unwrap()
    }

    fn running(name: &str) -> Stage {
        Stage::running(step_named(name))
    }

    fn output(step: &str, value: Value) -> StepOutput {
        StepOutput::parse(step_named(step), value).unwrap()
    }

    fn done(step: &str, value: Value) -> Fact {
        Fact::Done {
            step: step_named(step),
            unit: None,
            output: output(step, value),
        }
    }

    fn reviewed(review: &str, value: Value) -> Fact {
        let r = Review::try_from(review.to_owned()).unwrap();
        Fact::Done {
            step: r.step(),
            unit: r.unit(),
            output: StepOutput::parse(r.step(), value).unwrap(),
        }
    }

    /// The actions as short names: filed steps (a code review with its unit), commits, the rest.
    fn names(actions: &[Action]) -> Vec<String> {
        actions
            .iter()
            .map(|a| match a {
                Action::File(f) => match f.unit {
                    Some(u) => format!("{}/{u}", f.step),
                    None => f.step.to_string(),
                },
                Action::Commit { after } => format!("commit after {after}"),
                Action::RecordReviewRound { number } => format!("round {number} ended"),
                Action::StopApp
                | Action::Push
                | Action::AskOwner
                | Action::RequestApproval
                | Action::WaitForBots
                | Action::AnswerComments
                | Action::RunGate
                | Action::NotifyReadyForOwner
                | Action::CloseOnTracker
                | Action::ClosePull
                | Action::StartOnTracker
                | Action::NotifyDependents
                | Action::ReportStopped
                | Action::RequestRecovery => format!("{a:?}"),
            })
            .collect()
    }

    /// The next state and actions for `fact`, with a commit the actions ask for made at once, as
    /// the daemon makes it, and what follows it.
    fn step(s: &IssueState, fact: &Fact, limits: Limits) -> (IssueState, Vec<Action>) {
        let (n, mut actions) = next(s, fact, limits);
        if let Some(Action::Commit { after }) = actions.last().cloned() {
            let (n, more) = next(&n, &Fact::Committed { after }, limits);
            actions.extend(more);
            return (n, actions);
        }
        (n, actions)
    }

    /// Applies each fact in turn and checks the actions it produces.
    fn drive(mut s: IssueState, steps: Vec<(Fact, Vec<&str>)>, limits: Limits) -> IssueState {
        for (i, (fact, expected)) in steps.into_iter().enumerate() {
            let (n, actions) = step(&s, &fact, limits);
            assert_eq!(
                names(&actions),
                expected,
                "fact {i}: {fact:?} at {:?}",
                s.stage
            );
            s = n;
        }
        s
    }

    fn requirements(scale: &str) -> Fact {
        done(
            "requirements",
            json!({
                "scopeEvidence": {"affectedFiles": ["src/export.ts"], "affectedLayers": ["backend"],
                    "executionRoute": {"status": "evident"}},
                "costEvidence": {"drivers": [], "unknowns": []},
                "convergence": {"outcome": "A customer downloads every order as one CSV file.",
                    "requirements": [], "nonGoals": []},
                "structuralScale": scale,
                "decision": "proceed"
            }),
        )
    }

    fn scope_check(scale: &str, ui_spec: bool, adr: bool) -> Fact {
        let points = if adr {
            json!([{"question": "Queue or cron", "answer": "cron"}])
        } else {
            json!([])
        };
        done(
            "scope-check",
            json!({"decision": "proceed", "structuralScale": scale, "uiSpec": ui_spec,
                   "adrDecisionPoints": points}),
        )
    }

    fn document(step: &str) -> Fact {
        done(
            step,
            json!({"status": "completed", "documentType": "DesignDoc", "path": "docs/design/x.md"}),
        )
    }

    fn doc_pass(step: &str) -> Fact {
        done(
            step,
            json!({"metadata": {"doc_type": "DesignDoc"}, "verdict": {"decision": "pass"}, "issues": []}),
        )
    }

    fn doc_issues(step: &str) -> Fact {
        done(
            step,
            json!({"verdict": {"decision": "needs_revision"},
                   "issues": [{"id": "I001", "description": "d", "correction": "c"}]}),
        )
    }

    fn doc_reconciled(step: &str, status: &str) -> Fact {
        done(
            step,
            json!({"verdict": {"decision": "pass"}, "prior_feedback_reconciliation":
                   [{"id": "I001", "prior_disposition": "apply", "status": status, "evidence": "e"}]}),
        )
    }

    fn quality_of(step: &str) -> Fact {
        done(
            step,
            json!({"status": "pass", "summary": "s", "checksPerformed": {}, "fixesApplied": []}),
        )
    }

    fn synced(status: &str) -> Fact {
        done(
            "design-sync",
            json!({"sync_status": status, "total_conflicts": 0}),
        )
    }

    fn implemented(test_review: bool) -> Fact {
        done(
            "implement",
            json!({"status": "completed", "taskName": "t", "changeSummary": "Export orders as CSV",
                   "testsAdded": [], "requiresTestReview": test_review, "newTestsPassed": true}),
        )
    }

    fn quality(status: &str) -> Fact {
        done(
            "quality",
            json!({"status": status, "summary": "s", "checksPerformed": {}, "fixesApplied": []}),
        )
    }

    fn code_pass() -> Value {
        json!({"verdict": "pass", "findings": [], "limitations": []})
    }

    fn code_findings() -> Value {
        json!({"verdict": "needs-improvement", "findings": [{"id": "F001", "location": "a.ts:1"}]})
    }

    fn security_pass() -> Value {
        json!({"status": "pass", "summary": "s", "findings": []})
    }

    fn security_findings() -> Value {
        json!({"status": "needs_revision", "summary": "s", "findings": [{"id": "S001"}]})
    }

    /// The document's author, in update mode, with its judgment of the findings it was handed.
    fn revised(step: &str, dispositions: &Value) -> Fact {
        done(
            step,
            json!({"status": "completed", "documentType": "DesignDoc", "path": "docs/design/x.md",
                   "dispositions": dispositions}),
        )
    }

    /// The executor's `review-response`, with its judgment of the findings it was handed.
    fn answered(dispositions: &Value) -> Fact {
        done(
            "review-response",
            json!({"status": "completed", "taskName": "t", "changeSummary": "Escape commas",
                   "requiresTestReview": false, "dispositions": dispositions}),
        )
    }

    fn apply(id: &str, review: Option<&str>) -> Value {
        let mut d = json!({"id": id, "disposition": "apply", "reason": "a.ts:1 breaks AC-1"});
        if let Some(r) = review {
            d["review"] = json!(r);
        }
        d
    }

    fn decline(id: &str, review: &str) -> Value {
        json!({"id": id, "disposition": "decline", "reason": "optional hardening", "review": review})
    }

    fn planned_tasks(tasks: usize) -> Vec<PlannedTask> {
        (1..=tasks)
            .map(|i| PlannedTask {
                path: format!("tasks/plan-task-{i:02}.md"),
            })
            .collect()
    }

    fn planned(tasks: usize, units: usize) -> Fact {
        Fact::Planned {
            tasks: planned_tasks(tasks),
            units: (0..units).map(|u| vec![u]).collect(),
        }
    }

    fn at(stage: Stage) -> IssueState {
        IssueState {
            stage,
            ..IssueState::new("ACME-1")
        }
    }

    const ROUND: [&str; 3] = ["commit after quality", "code-review/1", "security-review"];

    #[test]
    fn a_small_issue_goes_from_requirements_to_implementation_and_both_reviews_in_parallel() {
        let s = drive(
            at(Stage::Queued),
            vec![
                (
                    Fact::DependenciesMerged,
                    vec!["StartOnTracker", "requirements"],
                ),
                (requirements("small"), vec!["implement"]),
                (implemented(false), vec!["quality"]),
                (quality("pass"), ROUND.to_vec()),
                (reviewed("code-review/1", code_pass()), vec![]),
                (
                    reviewed("security-review", security_pass()),
                    vec!["round 1 ended", "Push"],
                ),
            ],
            Limits::default(),
        );
        assert_eq!(s.stage, Stage::Pushing);
    }

    #[test]
    fn a_small_issue_whose_evidence_gate_is_unresolved_is_analyzed_and_scoped_first() {
        drive(
            at(running("requirements")),
            vec![
                (requirements("unresolved"), vec!["codebase-analysis"]),
                (
                    done("codebase-analysis", json!({"analysisScope": {}})),
                    vec!["scope-check"],
                ),
                (scope_check("small", false, false), vec!["implement"]),
            ],
            Limits::default(),
        );
    }

    #[test]
    fn a_medium_issue_runs_design_planning_and_each_task_before_one_quality_run() {
        let s = drive(
            at(running("requirements")),
            vec![
                (requirements("medium"), vec!["codebase-analysis"]),
                (done("codebase-analysis", json!({})), vec!["scope-check"]),
                (scope_check("medium", false, false), vec!["design"]),
                (document("design"), vec!["design-review"]),
                (doc_pass("design-review"), vec!["design-sync"]),
                (
                    synced("SKIPPED"),
                    vec!["commit after design-sync", "test-design"],
                ),
                (
                    done(
                        "test-design",
                        json!({"status": "completed", "feature": "f",
                        "generatedFiles": {"integration": "tests/export.int.test.ts", "fixtureE2e": null, "serviceE2e": null}}),
                    ),
                    vec!["work-plan"],
                ),
                (document("work-plan"), vec!["plan-review"]),
                (doc_pass("plan-review"), vec!["decompose"]),
                (document("decompose"), vec![]),
                (planned(2, 1), vec!["implement"]),
                (implemented(false), vec!["implement"]),
                (implemented(false), vec!["quality"]),
                (quality("verification_incomplete"), ROUND.to_vec()),
            ],
            Limits::default(),
        );
        assert_eq!(s.task, 1);
    }

    #[test]
    fn a_large_issue_has_its_prd_reviewed_and_approved_before_analysis() {
        let s = drive(
            at(running("requirements")),
            vec![
                (requirements("large"), vec!["prd"]),
                (
                    done(
                        "prd",
                        json!({"status": "completed", "question": "Should archived orders be exported?"}),
                    ),
                    vec!["AskOwner"],
                ),
                (Fact::OwnerAnswered, vec!["prd"]),
                (document("prd"), vec!["prd-review"]),
                (
                    doc_pass("prd-review"),
                    vec!["commit after prd-review", "RequestApproval"],
                ),
                (Fact::OwnerAnswered, vec!["codebase-analysis"]),
                (done("codebase-analysis", json!({})), vec!["scope-check"]),
                (scope_check("large", false, false), vec!["design"]),
            ],
            Limits::default(),
        );
        assert!(s.documents.prd_approved);
    }

    #[test]
    fn a_small_change_to_what_a_prd_states_updates_the_prd_and_waits_for_its_approval() {
        let updating = done(
            "requirements",
            json!({
                "scopeEvidence": {"affectedFiles": ["src/export.ts"], "affectedLayers": ["backend"],
                    "executionRoute": {"status": "evident"}},
                "costEvidence": {"drivers": [], "unknowns": []},
                "convergence": {"outcome": "Exports include archived orders.",
                    "requirements": [], "nonGoals": []},
                "structuralScale": "small",
                "decision": "proceed",
                "prdUpdate": "docs/prd/export-prd.md"
            }),
        );
        let s = drive(
            at(running("requirements")),
            vec![
                (updating, vec!["prd"]),
                (document("prd"), vec!["prd-review"]),
                (
                    doc_pass("prd-review"),
                    vec!["commit after prd-review", "RequestApproval"],
                ),
                (Fact::OwnerAnswered, vec!["implement"]),
            ],
            Limits::default(),
        );
        assert!(s.documents.prd_approved);
    }

    #[test]
    fn a_large_change_is_reviewed_by_unit_three_at_a_time_with_one_security_review() {
        let s = IssueState {
            units: 5,
            ..at(running("quality"))
        };
        let s = drive(
            s,
            vec![
                (
                    quality("pass"),
                    vec![
                        "commit after quality",
                        "code-review/1",
                        "code-review/2",
                        "code-review/3",
                        "security-review",
                    ],
                ),
                (
                    reviewed("code-review/2", code_pass()),
                    vec!["code-review/4"],
                ),
                (reviewed("security-review", security_pass()), vec![]),
                (
                    reviewed("code-review/1", code_pass()),
                    vec!["code-review/5"],
                ),
                (reviewed("code-review/3", code_pass()), vec![]),
                (reviewed("code-review/4", code_pass()), vec![]),
            ],
            Limits::default(),
        );
        let (_, actions) = next(
            &s,
            &reviewed("code-review/5", code_pass()),
            Limits::default(),
        );
        assert_eq!(names(&actions), ["round 1 ended", "Push"]);
    }

    #[test]
    fn a_new_screen_and_an_owner_answered_decision_add_the_ui_spec_and_adr_before_design() {
        drive(
            at(running("scope-check")),
            vec![
                (scope_check("medium", true, true), vec!["prototype"]),
                (
                    done(
                        "prototype",
                        json!({"path": "docs/ui-spec/assets/orders/prototype.html", "referenceStrength": "reference", "basis": "no design tokens in the repository"}),
                    ),
                    vec!["RequestApproval"],
                ),
                (Fact::OwnerAnswered, vec!["ui-analysis"]),
                (done("ui-analysis", json!({})), vec!["ui-spec"]),
                (document("ui-spec"), vec!["ui-spec-review"]),
                (
                    doc_pass("ui-spec-review"),
                    vec!["commit after ui-spec-review", "adr"],
                ),
                (document("adr"), vec!["adr-review"]),
                (
                    doc_pass("adr-review"),
                    vec!["commit after adr-review", "design"],
                ),
            ],
            Limits::default(),
        );
    }

    #[test]
    fn a_design_that_adds_an_approval_category_waits_for_the_owner_and_keeps_it_held_for_the_merge()
    {
        let s = drive(
            at(running("design")),
            vec![
                (
                    done(
                        "design",
                        json!({"status": "completed", "documentType": "DesignDoc",
                        "path": "docs/design/x.md", "approvals": ["schema"]}),
                    ),
                    vec!["design-review"],
                ),
                (doc_pass("design-review"), vec!["design-sync"]),
                (
                    synced("NO_CONFLICTS"),
                    vec!["commit after design-sync", "RequestApproval"],
                ),
                (Fact::OwnerAnswered, vec!["test-design"]),
            ],
            Limits::default(),
        );
        assert_eq!(s.approvals, vec![Approval::Schema]);
    }

    #[test]
    fn a_kind_a_person_reviews_the_code_review_names_joins_what_the_design_declared() {
        let mut start = at(running("quality"));
        start.approvals = vec![Approval::Schema];
        let s = drive(
            start,
            vec![
                (quality("pass"), ROUND.to_vec()),
                (
                    reviewed(
                        "code-review/1",
                        json!({"verdict": "pass", "findings": [], "approvals": ["payments", "schema"]}),
                    ),
                    vec![],
                ),
            ],
            Limits::default(),
        );
        assert_eq!(
            s.approvals,
            vec![Approval::Schema, Approval::Named("payments".into())]
        );
    }

    #[test]
    fn a_design_that_touches_only_what_a_person_reviews_goes_on_and_keeps_it_held_for_the_merge() {
        let s = drive(
            at(running("design")),
            vec![
                (
                    done(
                        "design",
                        json!({"status": "completed", "documentType": "DesignDoc",
                        "path": "docs/design/x.md", "approvals": ["payments"]}),
                    ),
                    vec!["design-review"],
                ),
                (doc_pass("design-review"), vec!["design-sync"]),
                (
                    synced("NO_CONFLICTS"),
                    vec!["commit after design-sync", "test-design"],
                ),
            ],
            Limits::default(),
        );
        assert_eq!(s.approvals, vec![Approval::Named("payments".into())]);
    }

    #[test]
    fn the_executor_judges_the_whole_round_and_only_the_owner_of_an_applied_finding_rechecks() {
        let s = drive(
            at(running("quality")),
            vec![
                (quality("pass"), ROUND.to_vec()),
                (reviewed("code-review/1", code_findings()), vec![]),
                (
                    reviewed("security-review", security_pass()),
                    vec!["round 1 ended", "review-response"],
                ),
                (
                    answered(&json!([apply("F001", Some("code-review/1"))])),
                    vec!["quality"],
                ),
                // The security review passed and is retained: rerun-check judges the diff.
                (
                    quality("pass"),
                    vec!["commit after quality", "code-review/1", "rerun-check"],
                ),
                (
                    reviewed(
                        "code-review/1",
                        json!({"verdict": "pass", "prior_feedback_reconciliation":
                        [{"id": "F001", "prior_disposition": "apply", "status": "resolved", "evidence": "e"}]}),
                    ),
                    vec![],
                ),
                (
                    done("rerun-check", json!({"rerun": []})),
                    vec!["round 2 ended", "Push"],
                ),
            ],
            Limits::default(),
        );
        assert_eq!(s.round, Round::default());
    }

    #[test]
    fn findings_from_both_reviews_get_one_correction_and_both_recheck() {
        drive(
            at(running("quality")),
            vec![
                (quality("pass"), ROUND.to_vec()),
                (reviewed("security-review", security_findings()), vec![]),
                (
                    reviewed("code-review/1", code_findings()),
                    vec!["round 1 ended", "review-response"],
                ),
                (
                    answered(&json!([
                        apply("F001", Some("code-review/1")),
                        apply("S001", Some("security-review"))
                    ])),
                    vec!["quality"],
                ),
                (
                    quality("pass"),
                    vec!["commit after quality", "code-review/1", "security-review"],
                ),
            ],
            Limits::default(),
        );
    }

    #[test]
    fn a_review_whose_findings_are_all_declined_is_not_corrected_or_rerun() {
        drive(
            at(running("quality")),
            vec![
                (quality("pass"), ROUND.to_vec()),
                (reviewed("security-review", security_findings()), vec![]),
                (
                    reviewed("code-review/1", code_findings()),
                    vec!["round 1 ended", "review-response"],
                ),
                (
                    answered(&json!([
                        decline("F001", "code-review/1"),
                        apply("S001", Some("security-review"))
                    ])),
                    vec!["quality"],
                ),
                (
                    quality("pass"),
                    vec!["commit after quality", "security-review"],
                ),
            ],
            Limits::default(),
        );
        let (s, actions) = next(
            &IssueState {
                origin: Some(Origin::Round),
                ..at(running("review-response"))
            },
            &answered(&json!([decline("F001", "code-review/1")])),
            Limits::default(),
        );
        assert_eq!(
            (names(&actions), s.stage),
            (vec!["Push".to_owned()], Stage::Pushing)
        );
    }

    #[test]
    fn a_pre_upgrade_delegation_is_completed_by_the_executor_before_pr_comments_are_answered() {
        drive(
            at(Stage::WaitingReview),
            vec![
                (Fact::CommentsArrived, vec!["review-response"]),
                (
                    answered(
                        &json!([{"id": "doc", "disposition": "apply", "owner": "author",
                    "reason": "left for the author"}]),
                    ),
                    vec!["review-response"],
                ),
                (answered(&json!([apply("doc", None)])), vec!["quality"]),
                (quality("pass"), vec!["commit after quality", "Push"]),
            ],
            Limits::default(),
        );
    }

    #[test]
    fn rerun_check_reruns_only_the_passed_reviews_it_names() {
        let s = IssueState {
            origin: Some(Origin::Round),
            stage: Stage::Reviewing,
            round: Round {
                rerun_check: true,
                passed: [Review::SecurityReview].into(),
                ..Round::default()
            },
            ..IssueState::new("ACME-1")
        };
        drive(
            s,
            vec![
                (
                    done(
                        "rerun-check",
                        json!({"rerun": [{"review": "security-review", "implementationFiles": ["src/auth.ts"]}]}),
                    ),
                    vec!["security-review"],
                ),
                (
                    reviewed("security-review", security_pass()),
                    vec!["round 1 ended", "Push"],
                ),
            ],
            Limits::default(),
        );
    }

    #[test]
    fn an_applied_finding_maintained_through_two_corrections_goes_to_the_lead() {
        let s = drive(
            at(running("design")),
            vec![
                (document("design"), vec!["design-review"]),
                (doc_issues("design-review"), vec!["design"]),
                (
                    revised("design", &json!([apply("I001", None)])),
                    vec!["design-review"],
                ),
                (
                    doc_reconciled("design-review", "maintained"),
                    vec!["design"],
                ),
                (
                    revised("design", &json!([apply("I001", None)])),
                    vec!["design-review"],
                ),
                (
                    doc_reconciled("design-review", "maintained"),
                    vec!["exception"],
                ),
            ],
            Limits::default(),
        );
        assert_unsettled(&s, "design-review", &["design-review"]);
    }

    /// The lead has the issue because reviews still had findings, which `feedback` names.
    fn assert_unsettled(s: &IssueState, step: &str, feedback: &[&str]) {
        assert_eq!(
            s.stage,
            Stage::Exception {
                cause: Cause::Unsettled {
                    step: step_named(step),
                },
                resume: None,
            }
        );
        assert_eq!(
            s.feedback(),
            feedback.iter().map(|f| step_named(f)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_document_review_past_the_owners_round_limit_goes_to_the_lead() {
        let s = drive(
            at(running("design")),
            vec![
                (document("design"), vec!["design-review"]),
                (doc_issues("design-review"), vec!["design"]),
                (
                    revised("design", &json!([apply("I001", None)])),
                    vec!["exception"],
                ),
            ],
            Limits {
                review_rounds: Some(0),
                fix_ci_rounds: None,
            },
        );
        assert_unsettled(&s, "design-review", &["design-review"]);
    }

    #[test]
    fn a_blocked_check_still_goes_to_the_lead_as_a_step_that_could_not_finish() {
        let s = drive(
            at(running("decompose")),
            vec![
                (planned(1, 1), vec!["implement"]),
                (implemented(true), vec!["test-review"]),
                (
                    done(
                        "test-review",
                        json!({"status": "blocked", "testFiles": [], "reviewBasis": "skeleton"}),
                    ),
                    vec!["exception"],
                ),
            ],
            Limits::default(),
        );
        assert_eq!(
            s.stage,
            Stage::Exception {
                cause: Cause::Step {
                    step: step_named("test-review"),
                },
                resume: None,
            }
        );
        assert_eq!(s.feedback(), []);
    }

    #[test]
    fn an_exception_saved_before_reviews_could_be_unsettled_loads_as_a_step_that_could_not_finish()
    {
        let stage: Stage = serde_json::from_value(json!(
            {"cause": {"cause": "step", "step": "design-review"}, "resume": null, "stage": "exception"}
        ))
        .unwrap();
        assert_eq!(
            stage,
            Stage::Exception {
                cause: Cause::Step {
                    step: Step::DesignReview,
                },
                resume: None,
            }
        );
    }

    #[test]
    fn a_document_check_is_complete_when_its_findings_are_resolved_or_declined() {
        for disposition in ["apply", "decline"] {
            drive(
                at(running("design")),
                vec![
                    (document("design"), vec!["design-review"]),
                    (doc_issues("design-review"), vec!["design"]),
                    (
                        revised("design", &json!([apply("I001", None)])),
                        vec!["design-review"],
                    ),
                    (
                        doc_reconciled("design-review", "resolved"),
                        vec!["design-sync"],
                    ),
                    (synced("CONFLICTS_FOUND"), vec!["design"]),
                    (
                        revised(
                            "design",
                            &json!([{"id": "C001", "disposition": disposition, "reason": "the documents now agree"}]),
                        ),
                        vec!["commit after design-sync", "test-design"],
                    ),
                ],
                Limits::default(),
            );
        }
    }

    #[test]
    fn a_task_that_changed_integration_tests_has_them_reviewed_before_the_next_task() {
        let s = drive(
            at(running("decompose")),
            vec![
                (planned(2, 1), vec!["implement"]),
                (implemented(true), vec!["test-review"]),
                (
                    done(
                        "test-review",
                        json!({"status": "needs_revision", "testFiles": ["a.int.test.ts"],
                        "reviewBasis": "skeleton", "qualityIssues": [{"id": "T001"}]}),
                    ),
                    vec!["review-response"],
                ),
                (answered(&json!([apply("T001", None)])), vec!["test-review"]),
                (
                    done(
                        "test-review",
                        json!({"status": "pass", "testFiles": [], "reviewBasis": "skeleton",
                        "prior_feedback_reconciliation": [{"id": "T001", "prior_disposition": "apply", "status": "resolved", "evidence": "e"}]}),
                    ),
                    vec!["implement"],
                ),
            ],
            Limits::default(),
        );
        assert_eq!(s.task, 1);
    }

    #[test]
    fn stubs_send_their_owning_tasks_back_and_the_quality_run_follows_them() {
        drive(
            at(running("decompose")),
            vec![
                (planned(3, 1), vec!["implement"]),
                (implemented(false), vec!["implement"]),
                (implemented(false), vec!["implement"]),
                (implemented(false), vec!["quality"]),
                (quality("stub_detected"), vec![]),
                (Fact::Stubs { tasks: vec![0] }, vec!["implement"]),
                (implemented(false), vec!["quality"]),
            ],
            Limits::default(),
        );
        let (_, actions) = next(
            &at(running("quality")),
            &Fact::Stubs { tasks: vec![] },
            Limits::default(),
        );
        assert_eq!(names(&actions), ["implement"]);
    }

    #[test]
    fn stubs_after_a_round_correction_are_completed_in_review_and_the_rechecks_follow() {
        // A small issue (no task files) and a planned one: the incomplete work is implemented
        // again without a test review, and the quality run leads to the round's rechecks.
        for tasks in [0, 2] {
            let mut s = IssueState {
                tasks: planned_tasks(tasks),
                ..at(running("quality"))
            };
            let steps = vec![
                (quality("pass"), ROUND.to_vec()),
                (reviewed("code-review/1", code_findings()), vec![]),
                (
                    reviewed("security-review", security_pass()),
                    vec!["round 1 ended", "review-response"],
                ),
                (
                    answered(&json!([apply("F001", Some("code-review/1"))])),
                    vec!["quality"],
                ),
                (quality("stub_detected"), vec![]),
                (Fact::Stubs { tasks: vec![0] }, vec!["implement"]),
                (implemented(true), vec!["quality"]),
                (
                    quality("pass"),
                    vec!["commit after quality", "code-review/1", "rerun-check"],
                ),
            ];
            for (i, (fact, expected)) in steps.into_iter().enumerate() {
                let (n, actions) = step(&s, &fact, Limits::default());
                assert_eq!(
                    names(&actions),
                    expected,
                    "{tasks} tasks, fact {i}: {fact:?}"
                );
                if i >= 3 {
                    assert_eq!(
                        n.phase(),
                        Some(crate::phase::Phase::Review),
                        "{tasks} tasks, fact {i}"
                    );
                }
                s = n;
            }
        }
    }

    #[test]
    fn a_review_that_could_not_run_waits_for_the_round_then_goes_to_the_lead_who_may_rerun_it() {
        let s = drive(
            at(running("quality")),
            vec![
                (quality("pass"), ROUND.to_vec()),
                (
                    Fact::Stuck {
                        step: step_named("security-review"),
                        unit: None,
                    },
                    vec![],
                ),
                (reviewed("code-review/1", code_pass()), vec!["exception"]),
            ],
            Limits::default(),
        );
        assert_eq!(
            s.stage,
            Stage::Exception {
                cause: Cause::Step {
                    step: step_named("security-review"),
                },
                resume: None,
            }
        );
        assert_eq!(s.feedback(), []);
        let s = drive(
            s,
            vec![(
                done(
                    "exception",
                    json!({"action": "retry", "note": "the reviewer's CLI was signed out"}),
                ),
                vec!["security-review"],
            )],
            Limits::default(),
        );
        assert_eq!(s.stage, Stage::Reviewing);
    }

    #[test]
    fn the_owners_answer_to_the_leads_question_returns_to_the_lead_for_the_same_failure() {
        let s = drive(
            at(running("implement")),
            vec![
                (
                    Fact::Stuck {
                        step: step_named("implement"),
                        unit: None,
                    },
                    vec!["exception"],
                ),
                (
                    done(
                        "exception",
                        json!({"action": "ask", "note": "n", "question": "May the factory add a dependency?"}),
                    ),
                    vec!["AskOwner"],
                ),
                (Fact::OwnerAnswered, vec!["exception"]),
                (
                    done(
                        "exception",
                        json!({"action": "retry", "note": "the owner allowed it"}),
                    ),
                    vec!["implement"],
                ),
            ],
            Limits::default(),
        );
        assert_eq!(s.stage, running("implement"));
    }

    #[test]
    fn a_stopped_operation_goes_to_the_lead_and_retry_returns_to_the_state_it_stopped_in() {
        let s = drive(
            at(Stage::Pushing),
            vec![
                (
                    Fact::Stopped {
                        operation: "pushing".into(),
                    },
                    vec!["exception"],
                ),
                (
                    done(
                        "exception",
                        json!({"action": "ask", "note": "n", "question": "May the factory push?"}),
                    ),
                    vec!["AskOwner"],
                ),
                (Fact::OwnerAnswered, vec!["exception"]),
                (
                    done("exception", json!({"action": "retry", "note": "fixed"})),
                    vec![],
                ),
            ],
            Limits::default(),
        );
        assert_eq!(
            (s.stage.clone(), s.resume().cloned()),
            (Stage::Pushing, None)
        );
    }

    #[test]
    fn the_owners_cancel_ends_an_issue_wherever_it_is_and_leaves_an_ended_one_alone() {
        let ending = vec![
            Action::StopApp,
            Action::ClosePull,
            Action::CloseOnTracker,
            Action::NotifyDependents,
        ];
        for stage in [
            Stage::Queued,
            Stage::running(Step::Implement),
            Stage::Ci,
            Stage::HeldForOwner,
            Stage::Held,
        ] {
            let (n, actions) = next(&at(stage.clone()), &Fact::Canceled, Limits::default());
            assert_eq!(
                (n.stage, actions),
                (Stage::Dropped, ending.clone()),
                "{stage:?}"
            );
        }
        for stage in [Stage::Merged, Stage::Dropped] {
            let (n, actions) = next(&at(stage.clone()), &Fact::Canceled, Limits::default());
            assert_eq!((n.stage, actions), (stage, vec![]));
        }
    }

    #[test]
    fn a_stopped_issue_the_lead_drops_forgets_the_state_it_stopped_in() {
        let s = drive(
            at(Stage::Gate),
            vec![
                (
                    Fact::Stopped {
                        operation: "reading its pull request".into(),
                    },
                    vec!["exception"],
                ),
                (
                    done("exception", json!({"action": "drop", "note": "abandoned"})),
                    vec!["StopApp", "ClosePull", "CloseOnTracker", "NotifyDependents"],
                ),
            ],
            Limits::default(),
        );
        assert_eq!(
            (s.stage.clone(), s.resume().cloned()),
            (Stage::Dropped, None)
        );
    }

    #[test]
    fn when_the_leads_own_exception_cannot_run_the_owner_is_told_and_resolving_resumes() {
        let s = drive(
            at(Stage::Ci),
            vec![
                (
                    Fact::Stopped {
                        operation: "reading its pull request".into(),
                    },
                    vec!["exception"],
                ),
                (
                    Fact::Stopped {
                        operation: "launching its step".into(),
                    },
                    vec!["ReportStopped"],
                ),
            ],
            Limits::default(),
        );
        assert!(matches!(s.stage, Stage::Stopped { .. }), "{:?}", s.stage);
        let (s, actions) = next(&s, &Fact::Resume, Limits::default());
        assert_eq!(
            (s.stage.clone(), s.resume().cloned(), actions),
            (Stage::Ci, None, vec![])
        );
    }

    fn recover() -> Fact {
        done(
            "exception",
            json!({"action": "recover", "note": "n", "procedure": "git push -f", "summary": "s"}),
        )
    }

    #[test]
    fn a_step_the_lead_recovers_runs_again_once_the_procedure_succeeds_or_is_resolved() {
        let s = drive(
            at(running("implement")),
            vec![
                (
                    Fact::Stuck {
                        step: step_named("implement"),
                        unit: None,
                    },
                    vec!["exception"],
                ),
                (recover(), vec!["RequestRecovery"]),
                (Fact::Resume, vec!["implement"]),
            ],
            Limits::default(),
        );
        assert_eq!(s.stage, running("implement"));
    }

    #[test]
    fn a_failed_procedure_and_the_owners_reply_go_back_to_the_lead_keeping_the_stopped_state() {
        let s = drive(
            at(Stage::Gate),
            vec![
                (
                    Fact::Stopped {
                        operation: "reading its pull request".into(),
                    },
                    vec!["exception"],
                ),
                (recover(), vec!["RequestRecovery"]),
                (Fact::RecoveryFailed, vec!["exception"]),
                (recover(), vec!["RequestRecovery"]),
                (Fact::OwnerAnswered, vec!["exception"]),
                (recover(), vec!["RequestRecovery"]),
                (Fact::Resume, vec![]),
            ],
            Limits::default(),
        );
        assert_eq!((s.stage.clone(), s.resume().cloned()), (Stage::Gate, None));
    }

    #[test]
    fn a_closed_pull_request_goes_to_the_lead_once_and_retry_opens_another() {
        let s = drive(
            at(Stage::Ci),
            vec![
                (Fact::PullClosed, vec!["exception"]),
                (Fact::PullClosed, vec![]),
                (
                    done("exception", json!({"action": "retry", "note": "reopen"})),
                    vec!["Push"],
                ),
            ],
            Limits::default(),
        );
        assert_eq!(s.stage, Stage::Pushing);
    }

    #[test]
    fn new_pull_request_comments_go_to_the_executor_and_applied_ones_are_corrected_and_pushed() {
        drive(
            at(Stage::WaitingBots),
            vec![
                (
                    Fact::BotWaitEnded { comments: true },
                    vec!["review-response"],
                ),
                // The commenter reviews the correction's tests: no test review comes between.
                (
                    done(
                        "review-response",
                        json!({"status": "completed", "taskName": "t",
                               "changeSummary": "Escape commas", "testsAdded": ["a.int.test.ts"],
                               "requiresTestReview": true, "newTestsPassed": true,
                               "dispositions": [apply("review-comment-1", None)]}),
                    ),
                    vec!["quality"],
                ),
                (quality("pass"), vec!["commit after quality", "Push"]),
            ],
            Limits::default(),
        );
    }

    #[test]
    fn pull_request_comments_all_declined_are_answered_and_go_to_the_gate() {
        drive(
            at(Stage::WaitingReview),
            vec![
                (Fact::CommentsArrived, vec!["review-response"]),
                (
                    answered(&json!([decline("review-comment-1", "code-review/1")])),
                    vec!["AnswerComments", "RunGate"],
                ),
            ],
            Limits::default(),
        );
    }

    #[test]
    fn rounds_have_no_limit_unless_the_owner_sets_one() {
        let corrected = IssueState {
            origin: Some(Origin::Round),
            round: Round {
                number: 2,
                correcting: [Review::CodeReview { unit: 1 }].into(),
                ..Round::default()
            },
            ..at(running("quality"))
        };
        let (_, actions) = step(&corrected, &quality("pass"), Limits::default());
        assert_eq!(names(&actions), ["commit after quality", "code-review/1"]);
        let (s, actions) = step(
            &corrected,
            &quality("pass"),
            Limits {
                review_rounds: Some(2),
                fix_ci_rounds: None,
            },
        );
        assert_eq!(names(&actions), ["commit after quality", "exception"]);
        assert_unsettled(&s, "code-review", &["code-review"]);
    }

    #[test]
    fn a_round_past_the_owners_limit_names_the_reviews_whose_findings_are_under_correction() {
        let limit = Limits {
            review_rounds: Some(2),
            fix_ci_rounds: None,
        };
        for (correcting, feedback) in [
            (vec![Review::SecurityReview], vec!["security-review"]),
            (
                vec![Review::CodeReview { unit: 1 }, Review::SecurityReview],
                vec!["code-review", "security-review"],
            ),
        ] {
            let corrected = IssueState {
                origin: Some(Origin::Round),
                round: Round {
                    number: 2,
                    correcting: correcting.into_iter().collect(),
                    ..Round::default()
                },
                ..at(running("quality"))
            };
            let (s, _) = step(&corrected, &quality("pass"), limit);
            assert_unsettled(&s, "code-review", &feedback);
        }
    }

    #[test]
    fn a_stop_taken_while_the_lead_or_the_owner_had_the_issue_returns_it_to_them_on_resume() {
        let failure = Cause::Step {
            step: step_named("implement"),
        };
        for waiting in [
            Stage::Exception {
                cause: failure.clone(),
                resume: None,
            },
            Stage::LeadAsking {
                cause: failure.clone(),
                resume: None,
            },
            Stage::AwaitingRecovery {
                cause: failure.clone(),
                resume: None,
            },
        ] {
            let s = at(waiting.clone());
            let (s, actions) = next(
                &s,
                &Fact::Stopped {
                    operation: "pushing".into(),
                },
                Limits::default(),
            );
            assert_eq!(names(&actions), ["ReportStopped"], "{waiting:?}");
            let (s, actions) = next(&s, &Fact::Resume, Limits::default());
            assert_eq!(names(&actions), ["exception"], "{waiting:?}");
            assert_eq!(
                s.stage,
                Stage::Exception {
                    cause: failure.clone(),
                    resume: None
                }
            );
        }
        let (s, _) = next(
            &at(Stage::Asking {
                step: step_named("requirements"),
            }),
            &Fact::Stopped {
                operation: "pushing".into(),
            },
            Limits::default(),
        );
        let (s, actions) = next(&s, &Fact::Resume, Limits::default());
        assert_eq!(
            (s.stage, names(&actions)),
            (running("requirements"), vec!["requirements".to_owned()])
        );
    }

    #[test]
    fn a_new_plan_after_a_failed_review_round_goes_through_its_own_checks_to_a_fresh_round() {
        let s = IssueState {
            origin: Some(Origin::Round),
            check: Some(Check::DesignReview),
            after: Some(After::ReturnToRound),
            round: Round {
                number: 3,
                ..Round::default()
            },
            tasks: vec![PlannedTask {
                path: "t1.md".into(),
            }],
            ..at(Stage::Exception {
                cause: Cause::Step {
                    step: step_named("code-review"),
                },
                resume: None,
            })
        };
        drive(
            s,
            vec![
                (
                    done("exception", json!({"action": "replan", "note": "n"})),
                    vec!["work-plan"],
                ),
                (document("work-plan"), vec!["plan-review"]),
                (doc_pass("plan-review"), vec!["decompose"]),
                (planned(1, 1), vec!["implement"]),
                (implemented(false), vec!["quality"]),
                (quality("pass"), ROUND.to_vec()),
            ],
            Limits::default(),
        );
    }

    #[test]
    fn a_round_correction_asking_for_a_test_review_goes_to_quality_then_the_rechecks() {
        let s = drive(
            at(running("quality")),
            vec![
                (quality("pass"), ROUND.to_vec()),
                (reviewed("code-review/1", code_findings()), vec![]),
                (
                    reviewed("security-review", security_pass()),
                    vec!["round 1 ended", "review-response"],
                ),
                (
                    done(
                        "review-response",
                        json!({"status": "completed", "taskName": "t",
                               "changeSummary": "Escape commas", "testsAdded": ["a.int.test.ts"],
                               "requiresTestReview": true, "newTestsPassed": true,
                               "dispositions": [apply("F001", Some("code-review/1"))]}),
                    ),
                    vec!["quality"],
                ),
            ],
            Limits::default(),
        );
        assert_eq!(s.phase(), Some(crate::phase::Phase::Review));
        let s = drive(
            s,
            vec![(
                quality("pass"),
                vec!["commit after quality", "code-review/1", "rerun-check"],
            )],
            Limits::default(),
        );
        assert_eq!(s.phase(), Some(crate::phase::Phase::Review));
    }

    #[test]
    fn a_finding_kept_through_two_corrections_goes_to_the_lead() {
        let kept = || {
            reviewed(
                "code-review/1",
                json!({"verdict": "pass", "findings": [], "limitations": [],
                       "prior_feedback_reconciliation":
                       [{"id": "F001", "prior_disposition": "apply", "status": "maintained", "evidence": "e"}]}),
            )
        };
        let corrected = || answered(&json!([apply("F001", Some("code-review/1"))]));
        let s = drive(
            at(running("quality")),
            vec![
                (quality("pass"), ROUND.to_vec()),
                (reviewed("security-review", security_pass()), vec![]),
                (kept(), vec!["round 1 ended", "review-response"]),
                (corrected(), vec!["quality"]),
                (
                    quality("pass"),
                    vec!["commit after quality", "code-review/1", "rerun-check"],
                ),
                (done("rerun-check", json!({"rerun": []})), vec![]),
                (kept(), vec!["exception"]),
            ],
            Limits::default(),
        );
        assert_unsettled(&s, "code-review", &["code-review"]);
    }

    #[test]
    fn a_security_finding_kept_twice_is_the_security_reviews_feedback_and_retry_rechecks_it() {
        let kept = || {
            reviewed(
                "security-review",
                json!({"status": "pass", "summary": "s", "findings": [],
                       "prior_feedback_reconciliation":
                       [{"id": "S001", "prior_disposition": "apply", "status": "maintained", "evidence": "e"}]}),
            )
        };
        let corrected = || answered(&json!([apply("S001", Some("security-review"))]));
        let s = drive(
            at(running("quality")),
            vec![
                (quality("pass"), ROUND.to_vec()),
                (reviewed("code-review/1", code_pass()), vec![]),
                (kept(), vec!["round 1 ended", "review-response"]),
                (corrected(), vec!["quality"]),
                (
                    quality("pass"),
                    vec!["commit after quality", "security-review", "rerun-check"],
                ),
                (done("rerun-check", json!({"rerun": []})), vec![]),
                (kept(), vec!["exception"]),
            ],
            Limits::default(),
        );
        assert_unsettled(&s, "code-review", &["security-review"]);
        let s = drive(
            s,
            vec![(
                done("exception", json!({"action": "retry", "note": "n"})),
                vec!["security-review"],
            )],
            Limits::default(),
        );
        assert_eq!(s.stage, Stage::Reviewing);
    }

    #[test]
    fn a_rerun_check_that_could_not_run_goes_to_the_lead_who_may_retry_the_check() {
        let s = IssueState {
            origin: Some(Origin::Round),
            round: Round {
                number: 2,
                rerun_check: true,
                passed: [Review::SecurityReview].into(),
                ..Round::default()
            },
            ..at(Stage::Reviewing)
        };
        drive(
            s,
            vec![
                (
                    Fact::Stuck {
                        step: Step::RerunCheck,
                        unit: None,
                    },
                    vec!["exception"],
                ),
                (
                    done("exception", json!({"action": "retry", "note": "n"})),
                    vec!["rerun-check"],
                ),
            ],
            Limits::default(),
        );
    }

    #[test]
    fn ci_fixes_stop_at_the_owners_limit_and_the_leads_retry_starts_the_count_again() {
        let limits = Limits {
            review_rounds: None,
            fix_ci_rounds: Some(1),
        };
        drive(
            at(Stage::Ci),
            vec![
                (Fact::ChecksFailed, vec!["fix-ci"]),
                (quality_of("fix-ci"), vec!["commit after fix-ci", "Push"]),
                (Fact::Pushed, vec![]),
                (Fact::ChecksFailed, vec!["exception"]),
                (
                    done("exception", json!({"action": "retry", "note": "n"})),
                    vec!["fix-ci"],
                ),
                (quality_of("fix-ci"), vec!["commit after fix-ci", "Push"]),
                (Fact::Pushed, vec![]),
                (Fact::ChecksFailed, vec!["fix-ci"]),
            ],
            limits,
        );
    }

    #[test]
    fn a_held_issue_merged_or_closed_on_github_takes_the_merged_path_or_goes_to_the_lead() {
        let (s, actions) = next(&at(Stage::Held), &Fact::MergedOnGitHub, Limits::default());
        assert_eq!(
            (s.stage, names(&actions)),
            (Stage::Merged, vec!["NotifyDependents".to_owned()])
        );
        let (s, actions) = next(&at(Stage::Held), &Fact::PullClosed, Limits::default());
        assert_eq!(names(&actions), ["exception"]);
        assert!(matches!(s.stage, Stage::Exception { .. }));
    }

    #[test]
    fn a_held_issue_taken_up_again_goes_back_to_the_gate() {
        let (s, actions) = next(&at(Stage::Held), &Fact::TakenUp, Limits::default());
        assert_eq!(
            (s.stage, names(&actions)),
            (Stage::Gate, vec!["RunGate".to_owned()])
        );
    }

    #[test]
    fn the_items_an_issue_must_have_follow_its_state() {
        let s = IssueState {
            round: Round {
                pending: [Review::CodeReview { unit: 2 }, Review::SecurityReview].into(),
                rerun_check: true,
                ..Round::default()
            },
            ..at(Stage::Reviewing)
        };
        let items: Vec<String> = calls_for(&s)
            .iter()
            .map(|f| {
                format!(
                    "{}{}",
                    f.step,
                    f.unit.map(|u| format!("/{u}")).unwrap_or_default()
                )
            })
            .collect();
        assert_eq!(items, ["code-review/2", "security-review", "rerun-check"]);
        assert_eq!(calls_for(&at(Stage::Ci)), vec![]);
        assert_eq!(
            calls_for(&at(Stage::Exception {
                cause: Cause::Step {
                    step: step_named("design"),
                },
                resume: None,
            }))[0]
                .step,
            Step::Exception
        );
    }

    #[test]
    fn a_closed_issue_is_dropped_on_the_tracker_and_wakes_its_dependents() {
        let (s, actions) = next(
            &at(running("requirements")),
            &done(
                "requirements",
                json!({"decision": "close", "evidence": "src/export.ts:12 already does it",
                "structuralScale": "small", "convergence": {"outcome": "o"}}),
            ),
            Limits::default(),
        );
        assert_eq!(s.stage, Stage::Dropped);
        assert_eq!(
            actions,
            vec![Action::CloseOnTracker, Action::NotifyDependents]
        );
    }

    #[test]
    fn a_lost_dependency_goes_to_the_lead_and_never_drops_the_issue_by_itself() {
        let (s, actions) = next(
            &at(Stage::Queued),
            &Fact::DependencyLost {
                dependency: "ACME-0".into(),
            },
            Limits::default(),
        );
        assert_eq!(names(&actions), ["exception"]);
        let (s, _) = next(
            &s,
            &done("exception", json!({"action": "retry", "note": "n"})),
            Limits::default(),
        );
        assert_eq!(s.stage, running("requirements"));
    }

    #[test]
    fn the_leads_way_forward_takes_the_issue_to_the_level_it_names() {
        let failed = IssueState {
            tasks: vec![PlannedTask { path: "t".into() }],
            ..at(Stage::Exception {
                cause: Cause::Step {
                    step: step_named("implement"),
                },
                resume: None,
            })
        };
        for (action, step) in [
            ("replan", "work-plan"),
            ("redesign", "design"),
            ("retry", "implement"),
        ] {
            let (s, _) = next(
                &failed,
                &done("exception", json!({"action": action, "note": "n"})),
                Limits::default(),
            );
            assert_eq!(s.stage, running(step), "{action}");
        }
        let (s, actions) = next(
            &failed,
            &done("exception", json!({"action": "drop", "note": "n"})),
            Limits::default(),
        );
        assert_eq!(s.stage, Stage::Dropped);
        assert_eq!(
            actions,
            vec![
                Action::StopApp,
                Action::ClosePull,
                Action::CloseOnTracker,
                Action::NotifyDependents
            ]
        );
    }

    #[test]
    fn a_reply_to_an_approval_request_goes_back_to_the_author_of_that_document() {
        for of in [Approve::Prd, Approve::Prototype, Approve::Design] {
            let (s, _) = next(
                &at(Stage::AwaitingApproval { of }),
                &Fact::OwnerReplied,
                Limits::default(),
            );
            assert_eq!(s.stage, Stage::running(of.step()));
        }
    }

    #[test]
    fn a_ci_failure_is_fixed_committed_and_pushed_then_waits_for_the_bots_and_the_gate() {
        drive(
            at(Stage::Ci),
            vec![
                (Fact::ChecksFailed, vec!["fix-ci"]),
                (
                    done(
                        "fix-ci",
                        json!({"status": "pass", "summary": "s", "checksPerformed": {}, "fixesApplied": []}),
                    ),
                    vec!["commit after fix-ci", "Push"],
                ),
                (Fact::Pushed, vec![]),
                (Fact::ChecksPassed, vec!["WaitForBots"]),
                (Fact::BotWaitEnded { comments: false }, vec!["RunGate"]),
            ],
            Limits::default(),
        );
    }

    #[test]
    fn each_wait_of_the_gate_takes_the_issue_to_the_stage_that_can_meet_it() {
        let cases = [
            (G::Return(Return::Push), Stage::Pushing),
            (G::Return(Return::Ci), Stage::Ci),
            (G::Return(Return::Approvals), Stage::WaitingReview),
            (
                G::HoldForOwner([Approval::Schema].into()),
                Stage::HeldForOwner,
            ),
            (G::ReadyForOwner, Stage::ReadyForOwner),
        ];
        for (decision, stage) in cases {
            let (n, _) = next(
                &at(Stage::Gate),
                &Fact::Gate(decision.clone()),
                Limits::default(),
            );
            assert_eq!(n.stage, stage, "{decision:?}");
        }
    }

    #[test]
    fn a_merge_the_owner_makes_on_github_ends_the_issue() {
        let (n, _) = next(
            &at(Stage::ReadyForOwner),
            &Fact::MergedOnGitHub,
            Limits::default(),
        );
        assert_eq!(n.stage, Stage::Merged);
    }

    // Only the gate moves a merge it holds; the owner's Hold keeps it for the sprint review.
    #[test]
    fn a_held_merge_moves_only_on_the_gate_or_the_owners_hold() {
        let (n, actions) = next(
            &at(Stage::HeldForOwner),
            &Fact::OwnerAnswered,
            Limits::default(),
        );
        assert_eq!((n.stage, actions), (Stage::HeldForOwner, vec![]));
        let (n, actions) = next(
            &at(Stage::HeldForOwner),
            &Fact::OwnerHeld,
            Limits::default(),
        );
        assert_eq!(
            (n.stage, actions),
            (Stage::Held, vec![Action::NotifyDependents])
        );
    }

    #[test]
    fn a_refused_commit_is_corrected_by_the_quality_run_before_anything_follows() {
        let (s, actions) = next(&at(running("quality")), &quality("pass"), Limits::default());
        assert_eq!(names(&actions), ["commit after quality"]);
        // The daemon reports the hook's refusal instead of the commit.
        let (s, actions) = next(
            &s,
            &Fact::CommitFailed {
                after: CommitPoint::Quality,
            },
            Limits::default(),
        );
        assert_eq!(names(&actions), ["quality"]);
        // The corrected work is committed, then the review round starts as it would have.
        drive(
            s,
            vec![(quality("pass"), ROUND.to_vec())],
            Limits::default(),
        );
    }

    #[test]
    fn a_refusal_the_quality_run_cannot_clear_twice_goes_to_the_lead() {
        let mut s = at(Stage::Committing {
            after: CommitPoint::DesignSync,
        });
        for _ in 0..2 {
            let (n, actions) = next(
                &s,
                &Fact::CommitFailed {
                    after: CommitPoint::DesignSync,
                },
                Limits::default(),
            );
            assert_eq!(names(&actions), ["quality"]);
            let (n, actions) = next(&n, &quality("pass"), Limits::default());
            assert_eq!(names(&actions), ["commit after design-sync"]);
            s = n;
        }
        let (s, actions) = next(
            &s,
            &Fact::CommitFailed {
                after: CommitPoint::DesignSync,
            },
            Limits::default(),
        );
        assert_eq!(names(&actions), ["exception"]);
        assert_eq!(
            s.stage,
            Stage::Exception {
                cause: Cause::Step {
                    step: step_named("design-sync"),
                },
                resume: None,
            }
        );
    }

    #[test]
    fn a_refused_push_is_corrected_committed_and_pushed_again() {
        let s = at(Stage::Pushing);
        let (s, actions) = next(&s, &Fact::PushRefused, Limits::default());
        assert_eq!(names(&actions), ["quality"]);
        let (s, actions) = step(&s, &quality("pass"), Limits::default());
        assert_eq!(names(&actions), ["commit after push", "Push"]);
        assert_eq!(s.stage, Stage::Pushing);
    }

    // A push the quality runs could not get accepted goes to the lead; its retry pushes again
    // rather than filing a step named after the push.
    #[test]
    fn a_push_refused_past_the_quality_runs_goes_to_the_lead_whose_retry_pushes_again() {
        let s = drive(
            at(Stage::Pushing),
            vec![
                (Fact::PushRefused, vec!["quality"]),
                (quality("pass"), vec!["commit after push", "Push"]),
                (Fact::PushRefused, vec!["quality"]),
                (quality("pass"), vec!["commit after push", "Push"]),
                (Fact::PushRefused, vec!["exception"]),
                (
                    done(
                        "exception",
                        json!({"action": "retry", "note": "the hook is fixed"}),
                    ),
                    vec!["Push"],
                ),
            ],
            Limits::default(),
        );
        assert_eq!(s.stage, Stage::Pushing);
    }
}
