//! The five phases every issue moves through and what it is doing within one, as the owner reads
//! them at a glance. The dashboard words each value; this module decides which applies.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::flow::{After, Cause, CommitPoint, IssueState, Origin, Stage};
use crate::output::Scale;
use crate::step::Step;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    Requirements,
    Design,
    Build,
    Review,
    Merge,
}

impl Phase {
    pub const ALL: [Phase; 5] = [
        Phase::Requirements,
        Phase::Design,
        Phase::Build,
        Phase::Review,
        Phase::Merge,
    ];

    /// Whether an issue of `scale` passes through the phase: a small change goes from its
    /// requirements straight to building.
    pub fn taken_at(self, scale: Option<Scale>) -> bool {
        !(self == Phase::Design && scale == Some(Scale::Small))
    }
}

/// What an issue in progress is doing: one per step it runs, and one per wait in its merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Activity {
    ReadingTheIssue,
    WritingThePrd,
    ReviewingThePrd,
    ReadingTheCode,
    ChoosingDocuments,
    MakingAPrototype,
    ReadingTheScreens,
    WritingTheUiSpec,
    ReviewingTheUiSpec,
    RecordingDecisions,
    ReviewingDecisions,
    WritingTheDesign,
    CheckingTheDesign,
    ReviewingTheDesign,
    ComparingDesigns,
    WritingTestPlans,
    Planning,
    ReviewingThePlan,
    SplittingTasks,
    Implementing,
    ReviewingTests,
    QualityChecks,
    Committing,
    ReviewsRunning,
    ApplyingFindings,
    Rechecking,
    Pushing,
    WaitingForCi,
    FixingCi,
    ResolvingAConflict,
    WaitingForBots,
    WaitingForApproval,
    CheckingMerge,
    /// GitHub still stops the merge for a rule the factory cannot see.
    BlockedOnGitHub,
}

impl Step {
    /// The phase of the work the step does. A correction (`review-response`) and
    /// the lead's `exception` take the phase of what they answer, which only the issue's state
    /// knows; the sprint's steps belong to no issue.
    fn own_phase(self) -> Option<Phase> {
        match self {
            Step::Requirements | Step::Prd | Step::PrdReview => Some(Phase::Requirements),
            Step::CodebaseAnalysis
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
            | Step::Decompose => Some(Phase::Design),
            Step::Implement | Step::TestReview | Step::Quality => Some(Phase::Build),
            Step::CodeReview | Step::SecurityReview | Step::RerunCheck => Some(Phase::Review),
            Step::FixCi | Step::MergeBase => Some(Phase::Merge),
            Step::ReviewResponse
            | Step::Exception
            | Step::SprintPlan
            | Step::Facts
            | Step::SprintReview
            | Step::AgreementsCheck
            | Step::Conversation
            | Step::RunCommands => None,
        }
    }

    /// What an issue running the step is doing; none for the lead's `exception` (a problem, not
    /// an activity) and the sprint's steps.
    pub fn activity(self) -> Option<Activity> {
        Some(match self {
            Step::Requirements => Activity::ReadingTheIssue,
            Step::Prd => Activity::WritingThePrd,
            Step::PrdReview => Activity::ReviewingThePrd,
            Step::CodebaseAnalysis => Activity::ReadingTheCode,
            Step::ScopeCheck => Activity::ChoosingDocuments,
            Step::Prototype => Activity::MakingAPrototype,
            Step::UiAnalysis => Activity::ReadingTheScreens,
            Step::UiSpec => Activity::WritingTheUiSpec,
            Step::UiSpecReview => Activity::ReviewingTheUiSpec,
            Step::Adr => Activity::RecordingDecisions,
            Step::AdrReview => Activity::ReviewingDecisions,
            Step::Design => Activity::WritingTheDesign,
            Step::DesignVerify => Activity::CheckingTheDesign,
            Step::DesignReview => Activity::ReviewingTheDesign,
            Step::DesignSync => Activity::ComparingDesigns,
            Step::TestDesign => Activity::WritingTestPlans,
            Step::WorkPlan => Activity::Planning,
            Step::PlanReview => Activity::ReviewingThePlan,
            Step::Decompose => Activity::SplittingTasks,
            Step::Implement => Activity::Implementing,
            Step::TestReview => Activity::ReviewingTests,
            Step::Quality => Activity::QualityChecks,
            Step::CodeReview | Step::SecurityReview => Activity::ReviewsRunning,
            Step::RerunCheck => Activity::Rechecking,
            Step::ReviewResponse => Activity::ApplyingFindings,
            Step::FixCi => Activity::FixingCi,
            Step::MergeBase => Activity::ResolvingAConflict,
            Step::Exception
            | Step::SprintPlan
            | Step::Facts
            | Step::SprintReview
            | Step::AgreementsCheck
            | Step::Conversation
            | Step::RunCommands => return None,
        })
    }
}

impl IssueState {
    /// The phase the issue is in; none while it waits to start and once it has ended. A
    /// correction belongs to the phase of the check whose findings it answers, and a stop or a
    /// problem keeps the phase it happened in.
    pub fn phase(&self) -> Option<Phase> {
        self.phase_at(&self.stage)
    }

    fn phase_at(&self, stage: &Stage) -> Option<Phase> {
        match stage {
            Stage::Queued | Stage::Held | Stage::Merged | Stage::Dropped => None,
            Stage::Running { step } | Stage::Asking { step } => Some(self.phase_of(*step)),
            Stage::AwaitingApproval { of } => Some(self.phase_of(of.step())),
            Stage::Committing { after } => match after.step() {
                Some(step) => Some(self.phase_of(step)),
                None => Some(Phase::Merge),
            },
            Stage::Reviewing => Some(Phase::Review),
            Stage::Pushing
            | Stage::Ci
            | Stage::WaitingBots
            | Stage::WaitingReview
            | Stage::Gate
            | Stage::BlockedOnGitHub
            | Stage::HeldForOwner
            | Stage::ReadyForOwner => Some(Phase::Merge),
            Stage::Exception { cause, resume }
            | Stage::LeadAsking { cause, resume }
            | Stage::Stopped { cause, resume }
            | Stage::AwaitingRecovery { cause, resume } => {
                self.stopped_phase(cause, resume.as_deref())
            }
        }
    }

    /// The phase a step runs in, given what the issue is correcting.
    fn phase_of(&self, step: Step) -> Phase {
        // The Design Doc corrected for the review round and its checks before the round goes on,
        // and the implementation a round correction left incomplete.
        let round_correction =
            self.origin == Some(Origin::Round) || self.after == Some(After::ReturnToRound);
        if round_correction
            && matches!(
                step,
                Step::Design | Step::DesignReview | Step::DesignSync | Step::Implement
            )
        {
            return Phase::Review;
        }
        if step == Step::Quality {
            return match self.refused {
                Some((CommitPoint::Push, _)) => Phase::Merge,
                Some((after, _)) if after != CommitPoint::Quality => after
                    .step()
                    .map_or(Phase::Merge, |step| self.phase_of(step)),
                _ => self.correction_phase().unwrap_or(Phase::Build),
            };
        }
        step.own_phase()
            .or_else(|| self.correction_phase())
            .unwrap_or(Phase::Build)
    }

    /// The phase of the check whose findings are being answered.
    fn correction_phase(&self) -> Option<Phase> {
        match self.origin? {
            Origin::Document => self.check.and_then(|c| c.step().own_phase()),
            Origin::TestReview => Some(Phase::Build),
            Origin::Round => Some(Phase::Review),
            Origin::PrComments => Some(Phase::Merge),
        }
    }

    /// The phase the issue stopped in: the state it returns to, or what the lead is finding a
    /// way past.
    fn stopped_phase(&self, cause: &Cause, resume: Option<&Stage>) -> Option<Phase> {
        if let Some(stage) = resume {
            return self.phase_at(stage);
        }
        match cause {
            Cause::Step { step } => Some(self.phase_of(*step)),
            // The issue had not started: a dependency it waited for was lost.
            Cause::DependencyLost => Some(Phase::Requirements),
            Cause::PullClosed | Cause::Push | Cause::Operation { .. } => Some(Phase::Merge),
        }
    }

    /// What the issue is doing in its phase; none when it is waiting on the owner, waiting to
    /// start, ended, or the lead has a problem with it.
    pub fn activity(&self) -> Option<Activity> {
        match &self.stage {
            Stage::Running { step } => step.activity(),
            Stage::Reviewing => Some(Activity::ReviewsRunning),
            Stage::Committing { .. } => Some(Activity::Committing),
            Stage::Pushing => Some(Activity::Pushing),
            Stage::Ci => Some(Activity::WaitingForCi),
            Stage::WaitingBots => Some(Activity::WaitingForBots),
            Stage::WaitingReview => Some(Activity::WaitingForApproval),
            Stage::Gate => Some(Activity::CheckingMerge),
            Stage::BlockedOnGitHub => Some(Activity::BlockedOnGitHub),
            Stage::Queued
            | Stage::Asking { .. }
            | Stage::LeadAsking { .. }
            | Stage::Stopped { .. }
            | Stage::AwaitingRecovery { .. }
            | Stage::AwaitingApproval { .. }
            | Stage::HeldForOwner
            | Stage::ReadyForOwner
            | Stage::Held
            | Stage::Merged
            | Stage::Dropped
            | Stage::Exception { .. } => None,
        }
    }

    /// The task being implemented, counted from one, and how many there are; none for a change
    /// implemented without task files.
    pub fn task_of(&self) -> Option<(usize, usize)> {
        (self.stage == Stage::running(Step::Implement) && !self.tasks.is_empty())
            .then(|| (self.task + 1, self.tasks.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::step::Check;

    fn at(stage: Stage) -> IssueState {
        IssueState {
            stage,
            ..IssueState::new("ACME-1")
        }
    }

    #[test]
    fn each_work_step_runs_in_its_own_phase() {
        let cases = [
            (Step::Requirements, Phase::Requirements),
            (Step::PrdReview, Phase::Requirements),
            (Step::CodebaseAnalysis, Phase::Design),
            (Step::Decompose, Phase::Design),
            (Step::Implement, Phase::Build),
            (Step::Quality, Phase::Build),
            (Step::RerunCheck, Phase::Review),
            (Step::FixCi, Phase::Merge),
            (Step::MergeBase, Phase::Merge),
        ];
        for (step, phase) in cases {
            assert_eq!(at(Stage::running(step)).phase(), Some(phase), "{step}");
        }
    }

    #[test]
    fn a_correction_takes_the_phase_of_the_check_it_answers() {
        let document = IssueState {
            origin: Some(Origin::Document),
            check: Some(Check::PrdReview),
            ..at(Stage::running(Step::ReviewResponse))
        };
        assert_eq!(document.phase(), Some(Phase::Requirements));
        let design_check = IssueState {
            check: Some(Check::DesignVerify),
            ..document.clone()
        };
        assert_eq!(design_check.phase(), Some(Phase::Design));
        let tests = IssueState {
            origin: Some(Origin::TestReview),
            check: Some(Check::TestReview),
            ..at(Stage::running(Step::ReviewResponse))
        };
        assert_eq!(tests.phase(), Some(Phase::Build));
        let round = IssueState {
            origin: Some(Origin::Round),
            ..at(Stage::running(Step::ReviewResponse))
        };
        assert_eq!(round.phase(), Some(Phase::Review));
        let round_quality = IssueState {
            origin: Some(Origin::Round),
            ..at(Stage::running(Step::Quality))
        };
        assert_eq!(round_quality.phase(), Some(Phase::Review));
        let round_design = IssueState {
            origin: Some(Origin::Round),
            ..at(Stage::running(Step::Design))
        };
        assert_eq!(round_design.phase(), Some(Phase::Review));
        let round_implement = IssueState {
            origin: Some(Origin::Round),
            ..at(Stage::running(Step::Implement))
        };
        assert_eq!(round_implement.phase(), Some(Phase::Review));
        let comments = IssueState {
            origin: Some(Origin::PrComments),
            ..at(Stage::running(Step::ReviewResponse))
        };
        assert_eq!(comments.phase(), Some(Phase::Merge));
        let refused_push = IssueState {
            refused: Some((CommitPoint::Push, 1)),
            ..at(Stage::running(Step::Quality))
        };
        assert_eq!(refused_push.phase(), Some(Phase::Merge));
    }

    #[test]
    fn a_problem_or_a_stop_keeps_the_phase_it_happened_in() {
        let failed_step = at(Stage::Exception {
            cause: Cause::Step {
                step: Step::DesignVerify,
            },
            resume: None,
        });
        assert_eq!(failed_step.phase(), Some(Phase::Design));
        let stopped_in_ci = at(Stage::Stopped {
            cause: Cause::Operation {
                operation: "push".into(),
            },
            resume: Some(Box::new(Stage::Ci)),
        });
        assert_eq!(stopped_in_ci.phase(), Some(Phase::Merge));
        let stopped_in_build = at(Stage::Exception {
            cause: Cause::Operation {
                operation: "commit".into(),
            },
            resume: Some(Box::new(Stage::running(Step::Implement))),
        });
        assert_eq!(stopped_in_build.phase(), Some(Phase::Build));
    }

    #[test]
    fn waiting_to_start_and_ended_issues_are_in_no_phase() {
        for stage in [Stage::Queued, Stage::Merged, Stage::Dropped, Stage::Held] {
            assert_eq!(at(stage.clone()).phase(), None, "{stage:?}");
        }
        assert_eq!(at(Stage::ReadyForOwner).phase(), Some(Phase::Merge));
        assert_eq!(
            at(Stage::AwaitingApproval {
                of: crate::flow::Approve::Prd
            })
            .phase(),
            Some(Phase::Requirements)
        );
    }

    #[test]
    fn an_issue_waiting_on_the_owner_or_with_a_problem_has_no_activity() {
        assert_eq!(at(Stage::Ci).activity(), Some(Activity::WaitingForCi));
        assert_eq!(
            at(Stage::running(Step::ReviewResponse)).activity(),
            Some(Activity::ApplyingFindings)
        );
        assert_eq!(at(Stage::HeldForOwner).activity(), None);
        assert_eq!(
            at(Stage::Exception {
                cause: Cause::PullClosed,
                resume: None
            })
            .activity(),
            None
        );
    }

    #[test]
    fn the_task_counts_from_one_only_when_task_files_exist() {
        let planned = IssueState {
            tasks: vec![
                crate::flow::PlannedTask { path: "a".into() },
                crate::flow::PlannedTask { path: "b".into() },
            ],
            task: 1,
            ..at(Stage::running(Step::Implement))
        };
        assert_eq!(planned.task_of(), Some((2, 2)));
        assert_eq!(at(Stage::running(Step::Implement)).task_of(), None);
    }

    #[test]
    fn a_small_change_skips_design() {
        assert!(!Phase::Design.taken_at(Some(Scale::Small)));
        assert!(Phase::Design.taken_at(Some(Scale::Medium)));
        assert!(Phase::Design.taken_at(None));
        assert!(Phase::Build.taken_at(Some(Scale::Small)));
    }
}
