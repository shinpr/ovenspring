//! The sprint flow: given a sprint's state and one new fact, the next state and the actions the
//! daemon carries out. Like the issue flow, it holds no judgment.

use serde::{Deserialize, Serialize};

use crate::output::{ExceptionAction, ExceptionOutput, SprintPlanOutput, SprintReviewOutput};
use crate::step::Step;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "stage", rename_all = "kebab-case")]
pub enum SprintStage {
    /// The lead writes the proposal (`sprint-plan`).
    Proposing,
    /// The proposal is with the owner.
    AwaitingApproval,
    Running,
    /// The analyzer gathers the sprint's facts.
    Collecting,
    /// The lead writes the review (`sprint-review`).
    Reviewing,
    /// The reviewer checks the proposed working agreements.
    CheckingAgreements,
    /// The review is with the owner.
    AwaitingReviewApproval,
    /// The agreements pull request is open; the next sprint waits for its merge.
    ApplyingAgreements,
    /// The proposal held no issue: no sprint runs until an open issue appears and the lead proposes
    /// again.
    WaitingForIssues,
    /// The lead is finding the way forward for what stopped the sprint (`failed`).
    Exception,
    /// The owner is asked about what stopped the sprint.
    Asking {
        what: Asked,
    },
}

/// What the owner is asked about a stopped sprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Asked {
    /// The lead's question.
    Question,
    /// The lead's recovery procedure.
    Recovery,
    /// A stop the lead could not be launched to take.
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SprintState {
    pub id: String,
    pub stage: SprintStage,
    pub plan: Option<SprintPlanOutput>,
    pub review: Option<SprintReviewOutput>,
    /// The agreements pull request, once opened.
    pub agreements_pr: Option<u64>,
    /// Its address, for the owner.
    pub agreements_url: Option<String>,
    /// Whether GitHub waits for an approving review on it, as last read.
    #[serde(default)]
    pub agreements_need_approval: bool,
    /// When it was pushed (RFC 3339).
    pub agreements_pushed_at: Option<String>,
    /// When the sprint started running (RFC 3339).
    pub started_at: Option<String>,
    /// When the next sprint took its place (RFC 3339).
    pub ended_at: Option<String>,
    /// What stopped the sprint and what it answered, while the lead or the owner has it.
    pub failed: Option<String>,
    pub failure: Option<String>,
    /// The stage the sprint stopped in; it returns there when it resumes.
    pub resume: Option<Box<SprintStage>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SprintFact {
    PlanDone(SprintPlanOutput),
    /// The owner approved what waits on them: the proposal or the review.
    Approved,
    /// The owner replied without approving; the step that waits on them runs again with it.
    Replied,
    /// Every issue of the sprint is merged, held or dropped.
    IssuesSettled,
    FactsDone,
    ReviewDone(SprintReviewOutput),
    CheckDone {
        findings: bool,
    },
    AgreementsMerged,
    /// A sprint step ended blocked or its process died.
    Stuck {
        step: Step,
    },
    /// An issue the last proposal did not see is open on the tracker.
    IssueOpened,
    /// A daemon operation for the sprint failed on two passes in a row, the agreements pull
    /// request's checks failed, or it was closed: the lead takes `operation` and what it answered.
    Stopped {
        operation: String,
        error: String,
    },
    /// The lead's `exception` for the sprint.
    ExceptionDone(ExceptionOutput),
    /// What stopped the sprint was cleared: it returns to the stage it stopped in.
    Resume,
    /// The recovery procedure the owner approved exited with an error.
    RecoveryFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SprintStep {
    SprintPlan,
    Facts,
    SprintReview,
    AgreementsCheck,
    Exception,
}

impl SprintStep {
    pub fn step(self) -> Step {
        match self {
            SprintStep::SprintPlan => Step::SprintPlan,
            SprintStep::Facts => Step::Facts,
            SprintStep::SprintReview => Step::SprintReview,
            SprintStep::AgreementsCheck => Step::AgreementsCheck,
            SprintStep::Exception => Step::Exception,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SprintAction {
    File(SprintStep),
    PostProposal,
    /// Tell the owner the proposal held no issue and what is missing; nothing waits on a reply.
    PostWaiting,
    /// Put the plan's issues on the board.
    StartIssues,
    PostReview,
    OpenAgreementsPr,
    /// End this sprint; the next one starts with its own proposal.
    EndSprint,
    /// Put the lead's question about what stopped the sprint to the owner.
    AskQuestion(String),
    /// Ask the owner to approve the lead's recovery procedure.
    RequestRecovery,
    /// The lead could not be launched to take the stop: tell the owner what stopped.
    ReportStopped,
}

/// A new sprint: the lead proposes it.
pub fn begin(id: &str) -> (SprintState, Vec<SprintAction>) {
    (
        SprintState {
            id: id.into(),
            stage: SprintStage::Proposing,
            plan: None,
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
        vec![SprintAction::File(SprintStep::SprintPlan)],
    )
}

/// The next state and actions for one fact. Facts that do not apply change nothing.
pub fn next(state: &SprintState, fact: &SprintFact) -> (SprintState, Vec<SprintAction>) {
    use SprintAction as A;
    use SprintStage as S;
    let mut s = state.clone();
    let actions = match (&state.stage, fact) {
        (S::Proposing, SprintFact::PlanDone(plan)) if plan.issues.is_empty() => {
            s.plan = Some(plan.clone());
            s.stage = S::WaitingForIssues;
            vec![A::PostWaiting]
        }
        (S::Proposing, SprintFact::PlanDone(plan)) => {
            s.plan = Some(plan.clone());
            s.stage = S::AwaitingApproval;
            vec![A::PostProposal]
        }
        (S::AwaitingApproval, SprintFact::Approved) => {
            s.stage = S::Running;
            vec![A::StartIssues]
        }
        // The owner's reply to the proposal, or a new open issue after an empty proposal.
        (S::AwaitingApproval, SprintFact::Replied)
        | (S::WaitingForIssues, SprintFact::IssueOpened) => {
            s.stage = S::Proposing;
            vec![A::File(SprintStep::SprintPlan)]
        }
        (S::Running, SprintFact::IssuesSettled) => {
            s.stage = S::Collecting;
            vec![A::File(SprintStep::Facts)]
        }
        // Facts gathered, agreements to revise, or the owner's reply on the review: the lead writes it.
        (S::Collecting, SprintFact::FactsDone)
        | (S::CheckingAgreements, SprintFact::CheckDone { findings: true })
        | (S::AwaitingReviewApproval, SprintFact::Replied) => {
            s.stage = S::Reviewing;
            vec![A::File(SprintStep::SprintReview)]
        }
        (S::Reviewing, SprintFact::ReviewDone(review)) => {
            let changes = review.changes_what_launches_read();
            s.review = Some(review.clone());
            if changes {
                s.stage = S::CheckingAgreements;
                vec![A::File(SprintStep::AgreementsCheck)]
            } else {
                s.stage = S::AwaitingReviewApproval;
                vec![A::PostReview]
            }
        }
        (S::CheckingAgreements, SprintFact::CheckDone { findings: false }) => {
            s.stage = S::AwaitingReviewApproval;
            vec![A::PostReview]
        }
        (S::AwaitingReviewApproval, SprintFact::Approved) => {
            if s.review
                .as_ref()
                .is_some_and(SprintReviewOutput::changes_what_launches_read)
            {
                s.stage = S::ApplyingAgreements;
                vec![A::OpenAgreementsPr]
            } else {
                vec![A::EndSprint]
            }
        }
        (S::ApplyingAgreements, SprintFact::AgreementsMerged) => vec![A::EndSprint],
        // A sprint step that could not finish goes to the lead, as a stopped operation does; the
        // step runs again when the sprint returns to its stage.
        (
            S::Proposing | S::Collecting | S::Reviewing | S::CheckingAgreements,
            SprintFact::Stuck { step },
        ) => stopped(
            &mut s,
            &format!("its {step} step"),
            "the step ended blocked or its agent's process died; its run is in Activity",
        ),
        (_, SprintFact::Stopped { operation, error }) => stopped(&mut s, operation, error),
        (S::Exception, SprintFact::ExceptionDone(out)) => after_exception(&mut s, out),
        (S::Exception, SprintFact::Stuck { step }) if *step == Step::Exception => {
            s.stage = S::Asking {
                what: Asked::Stopped,
            };
            vec![A::ReportStopped]
        }
        (S::Asking { what }, SprintFact::Replied | SprintFact::RecoveryFailed)
            if fact == &SprintFact::Replied || *what == Asked::Recovery =>
        {
            s.stage = S::Exception;
            vec![A::File(SprintStep::Exception)]
        }
        (_, SprintFact::Resume) => {
            if let Some(stage) = s.resume.take() {
                s.stage = *stage;
                s.failed = None;
                s.failure = None;
            }
            vec![]
        }
        _ => vec![],
    };
    (s, actions)
}

impl SprintStage {
    /// The step whose item a sprint in this stage must have open or doing: what the daemon files
    /// again when it is missing.
    pub fn calls_for(&self) -> Option<SprintStep> {
        match self {
            SprintStage::Proposing => Some(SprintStep::SprintPlan),
            SprintStage::Collecting => Some(SprintStep::Facts),
            SprintStage::Reviewing => Some(SprintStep::SprintReview),
            SprintStage::CheckingAgreements => Some(SprintStep::AgreementsCheck),
            SprintStage::Exception => Some(SprintStep::Exception),
            SprintStage::AwaitingApproval
            | SprintStage::Running
            | SprintStage::AwaitingReviewApproval
            | SprintStage::ApplyingAgreements
            | SprintStage::WaitingForIssues
            | SprintStage::Asking { .. } => None,
        }
    }
}

/// The sprint stopped on `operation`: the lead takes it with the stage kept to return to. When the
/// lead's own `exception` is what could not run, the owner is told instead.
fn stopped(s: &mut SprintState, operation: &str, error: &str) -> Vec<SprintAction> {
    let with_lead_or_owner = matches!(s.stage, SprintStage::Exception | SprintStage::Asking { .. });
    if with_lead_or_owner && s.failed.as_deref() == Some(operation) {
        return vec![];
    }
    if with_lead_or_owner {
        s.stage = SprintStage::Asking {
            what: Asked::Stopped,
        };
        return vec![SprintAction::ReportStopped];
    }
    s.resume = Some(Box::new(s.stage.clone()));
    s.failed = Some(operation.into());
    s.failure = Some(error.into());
    s.stage = SprintStage::Exception;
    vec![SprintAction::File(SprintStep::Exception)]
}

/// The lead's way forward for the sprint: run the stopped operation again, have the owner approve
/// a recovery, or ask the owner. The issue-only actions reach the owner as the lead's note.
fn after_exception(s: &mut SprintState, out: &ExceptionOutput) -> Vec<SprintAction> {
    match out.action {
        ExceptionAction::Retry => {
            if let Some(stage) = s.resume.take() {
                s.stage = *stage;
            }
            s.failed = None;
            s.failure = None;
            vec![]
        }
        ExceptionAction::Recover => {
            s.stage = SprintStage::Asking {
                what: Asked::Recovery,
            };
            vec![SprintAction::RequestRecovery]
        }
        ExceptionAction::Ask
        | ExceptionAction::Replan
        | ExceptionAction::Redesign
        | ExceptionAction::Drop => {
            s.stage = SprintStage::Asking {
                what: Asked::Question,
            };
            vec![SprintAction::AskQuestion(
                out.question.clone().unwrap_or_else(|| out.note.clone()),
            )]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{AgreementFile, KeyResult, PlannedIssue};

    fn plan() -> SprintPlanOutput {
        SprintPlanOutput {
            purpose: "Owners can export their data".into(),
            key_results: vec!["Orders export as CSV".into()],
            issues: vec![PlannedIssue {
                key: "ACME-1".into(),
                depends_on: vec![],
            }],
            message: "Proposal".into(),
        }
    }

    fn review(agreements: bool) -> SprintReviewOutput {
        SprintReviewOutput {
            key_results: vec![KeyResult {
                text: "Orders export as CSV".into(),
                met: true,
                evidence: "ACME-1 merged".into(),
            }],
            agreements: if agreements {
                vec![AgreementFile {
                    file: crate::output::AgreementName::Common,
                    content: "- Name the precedent.\n".into(),
                }]
            } else {
                vec![]
            },
            knowledge: vec![],
            message: "Review".into(),
        }
    }

    fn at(stage: SprintStage) -> SprintState {
        SprintState {
            id: "s1".into(),
            stage,
            plan: Some(plan()),
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
        }
    }

    fn filed(actions: &[SprintAction]) -> Vec<String> {
        actions
            .iter()
            .filter_map(|a| match a {
                SprintAction::File(step) => Some(step.step().to_string()),
                SprintAction::PostProposal
                | SprintAction::PostWaiting
                | SprintAction::StartIssues
                | SprintAction::PostReview
                | SprintAction::OpenAgreementsPr
                | SprintAction::EndSprint
                | SprintAction::AskQuestion(_)
                | SprintAction::RequestRecovery
                | SprintAction::ReportStopped => None,
            })
            .collect()
    }

    #[test]
    fn a_sprint_runs_from_proposal_through_review_without_agreement_changes() {
        let (s, a) = begin("s1");
        assert_eq!(filed(&a), ["sprint-plan"]);
        let (s, a) = next(&s, &SprintFact::PlanDone(plan()));
        assert_eq!(
            (s.stage.clone(), a),
            (
                SprintStage::AwaitingApproval,
                vec![SprintAction::PostProposal]
            )
        );
        let (s, a) = next(&s, &SprintFact::Approved);
        assert_eq!(
            (s.stage.clone(), a),
            (SprintStage::Running, vec![SprintAction::StartIssues])
        );
        let (s, a) = next(&s, &SprintFact::IssuesSettled);
        assert_eq!(filed(&a), ["facts"]);
        let (s, a) = next(&s, &SprintFact::FactsDone);
        assert_eq!(filed(&a), ["sprint-review"]);
        let (s, a) = next(&s, &SprintFact::ReviewDone(review(false)));
        assert_eq!(
            (s.stage.clone(), a),
            (
                SprintStage::AwaitingReviewApproval,
                vec![SprintAction::PostReview]
            )
        );
        let (_, a) = next(&s, &SprintFact::Approved);
        assert_eq!(a, vec![SprintAction::EndSprint]);
    }

    #[test]
    fn agreement_changes_are_checked_and_the_next_sprint_waits_for_their_merge() {
        let (s, a) = next(
            &at(SprintStage::Reviewing),
            &SprintFact::ReviewDone(review(true)),
        );
        assert_eq!(filed(&a), ["agreements-check"]);
        let (back, a) = next(&s, &SprintFact::CheckDone { findings: true });
        assert_eq!(
            (back.stage, filed(&a)),
            (SprintStage::Reviewing, vec!["sprint-review".to_owned()])
        );
        let (s, a) = next(&s, &SprintFact::CheckDone { findings: false });
        assert_eq!(a, vec![SprintAction::PostReview]);
        let (s, a) = next(&s, &SprintFact::Approved);
        assert_eq!(
            (s.stage.clone(), a),
            (
                SprintStage::ApplyingAgreements,
                vec![SprintAction::OpenAgreementsPr]
            )
        );
        assert_eq!(
            next(&s, &SprintFact::Approved).1,
            vec![],
            "approval again does not skip the merge"
        );
        assert_eq!(
            next(&s, &SprintFact::AgreementsMerged).1,
            vec![SprintAction::EndSprint]
        );
    }

    #[test]
    fn a_reply_sends_the_proposal_or_review_back_to_the_lead() {
        let (s, a) = next(&at(SprintStage::AwaitingApproval), &SprintFact::Replied);
        assert_eq!(
            (s.stage, filed(&a)),
            (SprintStage::Proposing, vec!["sprint-plan".to_owned()])
        );
        let (s, a) = next(
            &at(SprintStage::AwaitingReviewApproval),
            &SprintFact::Replied,
        );
        assert_eq!(
            (s.stage, filed(&a)),
            (SprintStage::Reviewing, vec!["sprint-review".to_owned()])
        );
    }

    #[test]
    fn a_stuck_sprint_step_goes_to_the_lead_and_its_stage_calls_for_it_again_on_retry() {
        let (s, a) = next(
            &at(SprintStage::Collecting),
            &SprintFact::Stuck { step: Step::Facts },
        );
        assert_eq!(
            (s.stage.clone(), a, s.failed.clone()),
            (
                SprintStage::Exception,
                vec![SprintAction::File(SprintStep::Exception)],
                Some("its facts step".to_owned())
            )
        );
        let (s, a) = next(
            &s,
            &SprintFact::ExceptionDone(exception_output(ExceptionAction::Retry)),
        );
        assert_eq!((s.stage.clone(), a), (SprintStage::Collecting, vec![]));
        assert_eq!(s.stage.calls_for(), Some(SprintStep::Facts));
    }

    #[test]
    fn facts_out_of_place_change_nothing() {
        for stage in [
            SprintStage::Running,
            SprintStage::Proposing,
            SprintStage::ApplyingAgreements,
        ] {
            let s = at(stage);
            assert_eq!(next(&s, &SprintFact::Approved), (s.clone(), vec![]));
        }
    }

    #[test]
    fn a_proposal_with_no_issue_starts_no_sprint_until_an_issue_opens() {
        let empty = SprintPlanOutput {
            issues: vec![],
            ..plan()
        };
        let (s, actions) = next(&begin("1").0, &SprintFact::PlanDone(empty));
        assert_eq!(
            (s.stage.clone(), actions),
            (
                SprintStage::WaitingForIssues,
                vec![SprintAction::PostWaiting]
            )
        );
        // Approving does nothing while there is no proposal to approve.
        assert_eq!(next(&s, &SprintFact::Approved).1, vec![]);
        let (s, actions) = next(&s, &SprintFact::IssueOpened);
        assert_eq!(s.stage, SprintStage::Proposing);
        assert_eq!(actions, vec![SprintAction::File(SprintStep::SprintPlan)]);
    }

    fn exception_output(action: ExceptionAction) -> ExceptionOutput {
        ExceptionOutput {
            action,
            note: "n".into(),
            question: Some("q".into()),
            procedure: Some("p".into()),
            summary: Some("s".into()),
        }
    }

    #[test]
    fn a_stopped_sprint_goes_to_the_lead_once_and_returns_to_where_it_stopped() {
        let stopped = |operation: &str| SprintFact::Stopped {
            operation: operation.into(),
            error: "e".into(),
        };
        let (s, a) = next(
            &at(SprintStage::ApplyingAgreements),
            &stopped("the agreements pull request's checks"),
        );
        assert_eq!(
            (s.stage.clone(), filed(&a)),
            (SprintStage::Exception, vec!["exception".to_owned()])
        );
        // The same stop again changes nothing while the lead has it.
        let (s, a) = next(&s, &stopped("the agreements pull request's checks"));
        assert_eq!((s.stage.clone(), a), (SprintStage::Exception, vec![]));

        let (s, a) = next(
            &s,
            &SprintFact::ExceptionDone(exception_output(ExceptionAction::Recover)),
        );
        let asking = |what: Asked| SprintStage::Asking { what };
        assert_eq!(
            (s.stage.clone(), a),
            (asking(Asked::Recovery), vec![SprintAction::RequestRecovery])
        );
        let (s, a) = next(&s, &SprintFact::RecoveryFailed);
        assert_eq!(
            (s.stage.clone(), filed(&a)),
            (SprintStage::Exception, vec!["exception".to_owned()])
        );
        let (s, a) = next(
            &s,
            &SprintFact::ExceptionDone(exception_output(ExceptionAction::Ask)),
        );
        assert_eq!(
            (s.stage.clone(), a),
            (
                asking(Asked::Question),
                vec![SprintAction::AskQuestion("q".into())]
            )
        );
        let (s, a) = next(&s, &SprintFact::Replied);
        assert_eq!(
            (s.stage.clone(), filed(&a)),
            (SprintStage::Exception, vec!["exception".to_owned()])
        );
        let (s, a) = next(
            &s,
            &SprintFact::ExceptionDone(exception_output(ExceptionAction::Retry)),
        );
        assert_eq!(
            (s.stage.clone(), a),
            (SprintStage::ApplyingAgreements, vec![])
        );
        assert_eq!((s.failed, s.resume), (None, None));
    }

    #[test]
    fn when_the_leads_exception_cannot_run_the_owner_is_told_and_resolving_resumes() {
        let (s, _) = next(
            &at(SprintStage::ApplyingAgreements),
            &SprintFact::Stopped {
                operation: "applying the working agreements".into(),
                error: "e".into(),
            },
        );
        let (s, a) = next(
            &s,
            &SprintFact::Stopped {
                operation: "launching its step".into(),
                error: "e".into(),
            },
        );
        assert_eq!(
            (s.stage.clone(), a),
            (
                SprintStage::Asking {
                    what: Asked::Stopped
                },
                vec![SprintAction::ReportStopped]
            )
        );
        let (s, a) = next(&s, &SprintFact::Resume);
        assert_eq!((s.stage, a), (SprintStage::ApplyingAgreements, vec![]));
    }
}
