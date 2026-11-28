//! The five phases every issue moves through and what it is doing within one, as the owner reads
//! them at a glance. The dashboard words each value; this module decides which applies.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::flow::{
    Cause, CommitPoint, Correction, IssueState, JudgmentRoute, QualityRoute, Stage, TaskRoute,
    judgment_route, quality_route, task_route,
};
use crate::output::Scale;
use crate::step::{Check, Step};

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

/// What follows the running step, as the owner reads it after "Next:".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Upcoming {
    RemainingTasks,
    QualityChecks,
    Reviews,
    QualityThenRechecks,
    Rechecks,
    DesignThenRechecks,
    QualityThenPush,
    PushAndReplies,
    TestRecheck,
    Push,
    RepliesAndGate,
    CommitAgain,
    PushAgain,
}

/// What follows the running step once its work holds; for a judgment of findings whose applied
/// correction leads somewhere else, also what follows if one is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Following {
    pub follows: Upcoming,
    pub if_applied: Option<Upcoming>,
}

impl Following {
    fn to(follows: Upcoming) -> Following {
        Following {
            follows,
            if_applied: None,
        }
    }
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

impl Correction {
    /// The phase every step run inside this correction belongs to.
    pub fn phase(self) -> Phase {
        match self {
            Correction::Round => Phase::Review,
            Correction::PrComments => Phase::Merge,
        }
    }
}

impl IssueState {
    /// The phase the issue is in; none while it waits to start and once it has ended. A
    /// correction raised after the build keeps its phase for every step inside it, a correction
    /// of a check belongs to the phase of that check, and a stop or a problem keeps the phase it
    /// happened in.
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

    /// The phase a step runs in, given what the issue is correcting: a step run inside a
    /// correction raised after the build belongs to that correction's phase; otherwise a step
    /// has its own phase, or the phase of the check whose findings it answers.
    fn phase_of(&self, step: Step) -> Phase {
        if step == Step::Quality {
            match self.refused {
                Some((CommitPoint::Push, _)) => return Phase::Merge,
                Some((after, _)) if after != CommitPoint::Quality => {
                    return after
                        .step()
                        .map_or(Phase::Merge, |step| self.phase_of(step));
                }
                _ => {}
            }
        }
        if let Some(correction) = self.correction {
            return correction.phase();
        }
        step.own_phase()
            .or_else(|| self.check.and_then(|check| check.step().own_phase()))
            .unwrap_or(Phase::Build)
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

    /// What follows the running step once its work holds, read from the routes the flow applies;
    /// none where the step alone decides what follows (the dashboard words its activity).
    pub fn following(&self) -> Option<Following> {
        let Stage::Running { step } = self.stage else {
            return None;
        };
        if step == Step::Implement {
            return Some(Following::to(self.after_tasks()));
        }
        if step == Step::Quality {
            return Some(Following::to(self.after_quality()));
        }
        if step == Step::ReviewResponse || step.document().is_some() {
            let follows = self.after_judgment(judgment_route(self, false)?)?;
            let applied = self.after_judgment(judgment_route(self, true)?)?;
            return Some(Following {
                follows,
                if_applied: (applied != follows).then_some(applied),
            });
        }
        step.check()
            .and_then(|check| self.after_check(check, false))
            .map(Following::to)
    }

    /// What follows the current task: the remaining tasks, or the quality run.
    fn after_tasks(&self) -> Upcoming {
        match task_route(self) {
            TaskRoute::Implement(_) => Upcoming::RemainingTasks,
            TaskRoute::Quality => match quality_route(self) {
                QualityRoute::Round => Upcoming::QualityChecks,
                QualityRoute::Rechecks | QualityRoute::Design => Upcoming::QualityThenRechecks,
                QualityRoute::Push => Upcoming::QualityThenPush,
            },
        }
    }

    /// What follows a passing quality run: the commit or push it corrected, or what follows the
    /// commit.
    fn after_quality(&self) -> Upcoming {
        match self.refused {
            Some((CommitPoint::Push, _)) => return Upcoming::PushAgain,
            Some((after, _)) if after != CommitPoint::Quality => return Upcoming::CommitAgain,
            _ => {}
        }
        match quality_route(self) {
            QualityRoute::Round => Upcoming::Reviews,
            QualityRoute::Rechecks => Upcoming::Rechecks,
            QualityRoute::Design => Upcoming::DesignThenRechecks,
            QualityRoute::Push => Upcoming::PushAndReplies,
        }
    }

    fn after_judgment(&self, route: JudgmentRoute) -> Option<Upcoming> {
        match route {
            JudgmentRoute::Recheck(check) => self.after_check(check, true),
            JudgmentRoute::Cleared(check) => self.after_check(check, false),
            JudgmentRoute::Quality => Some(match quality_route(self) {
                QualityRoute::Push => Upcoming::QualityThenPush,
                QualityRoute::Round | QualityRoute::Rechecks | QualityRoute::Design => {
                    Upcoming::QualityThenRechecks
                }
            }),
            JudgmentRoute::DesignChecks => Some(Upcoming::DesignThenRechecks),
            JudgmentRoute::Rechecks => Some(Upcoming::Rechecks),
            JudgmentRoute::Converged => Some(Upcoming::Push),
            JudgmentRoute::Answer => Some(Upcoming::RepliesAndGate),
        }
    }

    /// What follows `check` once it rechecks a correction, or once it is clear. Outside a round,
    /// a document's checks lead on to its own next step, which the activity already words.
    fn after_check(&self, check: Check, recheck: bool) -> Option<Upcoming> {
        let in_round = self.correction == Some(Correction::Round);
        match check {
            Check::TestReview if recheck => Some(Upcoming::TestRecheck),
            Check::TestReview => Some(self.after_tasks()),
            // A design review checks again, or the design sync follows it: either way the
            // design's checks run before the round's.
            Check::DesignReview | Check::DesignVerify if in_round => {
                Some(Upcoming::DesignThenRechecks)
            }
            Check::DesignSync if in_round => Some(Upcoming::Rechecks),
            Check::DesignReview
            | Check::DesignVerify
            | Check::DesignSync
            | Check::PrdReview
            | Check::UiSpecReview
            | Check::AdrReview
            | Check::PlanReview => None,
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
            check: Some(Check::TestReview),
            ..at(Stage::running(Step::ReviewResponse))
        };
        assert_eq!(tests.phase(), Some(Phase::Build));
        let round = IssueState {
            correction: Some(Correction::Round),
            ..at(Stage::running(Step::ReviewResponse))
        };
        assert_eq!(round.phase(), Some(Phase::Review));
        let round_quality = IssueState {
            correction: Some(Correction::Round),
            ..at(Stage::running(Step::Quality))
        };
        assert_eq!(round_quality.phase(), Some(Phase::Review));
        let round_design = IssueState {
            correction: Some(Correction::Round),
            ..at(Stage::running(Step::Design))
        };
        assert_eq!(round_design.phase(), Some(Phase::Review));
        let round_implement = IssueState {
            correction: Some(Correction::Round),
            ..at(Stage::running(Step::Implement))
        };
        assert_eq!(round_implement.phase(), Some(Phase::Review));
        let comments = IssueState {
            correction: Some(Correction::PrComments),
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
    fn a_task_redone_for_pull_request_comments_and_its_test_review_run_in_merge() {
        for step in [Step::Implement, Step::TestReview, Step::ReviewResponse] {
            let redone = IssueState {
                correction: Some(Correction::PrComments),
                check: (step != Step::Implement).then_some(Check::TestReview),
                redoing: true,
                ..at(Stage::running(step))
            };
            assert_eq!(redone.phase(), Some(Phase::Merge), "{step}");
        }
        let built = IssueState {
            check: Some(Check::TestReview),
            ..at(Stage::running(Step::TestReview))
        };
        assert_eq!(built.phase(), Some(Phase::Build));
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

    fn two_tasks_at(step: Step) -> IssueState {
        IssueState {
            tasks: vec![
                crate::flow::PlannedTask { path: "a".into() },
                crate::flow::PlannedTask { path: "b".into() },
            ],
            task: 1,
            ..at(Stage::running(step))
        }
    }

    #[test]
    fn following_the_last_task_s_test_review_are_the_quality_checks_alone() {
        let last = IssueState {
            check: Some(Check::TestReview),
            ..two_tasks_at(Step::TestReview)
        };
        assert_eq!(
            last.following(),
            Some(Following::to(Upcoming::QualityChecks))
        );
        let first = IssueState {
            task: 0,
            ..last.clone()
        };
        assert_eq!(
            first.following(),
            Some(Following::to(Upcoming::RemainingTasks))
        );
    }

    #[test]
    fn following_a_comment_correction_s_redone_tasks_are_the_remaining_tasks_then_the_push() {
        let pending = IssueState {
            correction: Some(Correction::PrComments),
            redo: vec![0],
            redoing: true,
            ..two_tasks_at(Step::Implement)
        };
        assert_eq!(
            pending.following(),
            Some(Following::to(Upcoming::RemainingTasks))
        );
        let last = IssueState {
            redo: vec![],
            ..pending.clone()
        };
        assert_eq!(
            last.following(),
            Some(Following::to(Upcoming::QualityThenPush))
        );
        let last_reviewed = IssueState {
            check: Some(Check::TestReview),
            stage: Stage::running(Step::TestReview),
            ..last.clone()
        };
        assert_eq!(
            last_reviewed.following(),
            Some(Following::to(Upcoming::QualityThenPush))
        );
        let quality = IssueState {
            redoing: false,
            stage: Stage::running(Step::Quality),
            ..last
        };
        assert_eq!(
            quality.following(),
            Some(Following::to(Upcoming::PushAndReplies))
        );
    }

    #[test]
    fn following_a_judgment_names_what_an_applied_correction_leads_to() {
        let comments = IssueState {
            correction: Some(Correction::PrComments),
            ..at(Stage::running(Step::ReviewResponse))
        };
        assert_eq!(
            comments.following(),
            Some(Following {
                follows: Upcoming::RepliesAndGate,
                if_applied: Some(Upcoming::QualityThenPush),
            })
        );
        let tests = IssueState {
            check: Some(Check::TestReview),
            ..two_tasks_at(Step::ReviewResponse)
        };
        assert_eq!(
            tests.following(),
            Some(Following {
                follows: Upcoming::QualityChecks,
                if_applied: Some(Upcoming::TestRecheck),
            })
        );
        let round = IssueState {
            correction: Some(Correction::Round),
            ..at(Stage::running(Step::ReviewResponse))
        };
        assert_eq!(
            round.following(),
            Some(Following {
                follows: Upcoming::Push,
                if_applied: Some(Upcoming::QualityThenRechecks),
            })
        );
    }

    #[test]
    fn following_the_round_s_design_correction_are_its_checks_then_the_rechecks() {
        let author = IssueState {
            correction: Some(Correction::Round),
            ..at(Stage::running(Step::Design))
        };
        assert_eq!(
            author.following(),
            Some(Following {
                follows: Upcoming::Rechecks,
                if_applied: Some(Upcoming::DesignThenRechecks),
            })
        );
        let reviewed = IssueState {
            check: Some(Check::DesignReview),
            ..author.clone()
        };
        assert_eq!(
            reviewed.following(),
            Some(Following::to(Upcoming::DesignThenRechecks))
        );
        let synced = IssueState {
            check: Some(Check::DesignSync),
            ..author.clone()
        };
        assert_eq!(synced.following(), Some(Following::to(Upcoming::Rechecks)));
        let sync_running = IssueState {
            check: Some(Check::DesignSync),
            stage: Stage::running(Step::DesignSync),
            ..author
        };
        assert_eq!(
            sync_running.following(),
            Some(Following::to(Upcoming::Rechecks))
        );
    }

    #[test]
    fn following_quality_are_the_reviews_or_the_refused_commit_again() {
        let built = at(Stage::running(Step::Quality));
        assert_eq!(built.following(), Some(Following::to(Upcoming::Reviews)));
        let round = IssueState {
            correction: Some(Correction::Round),
            ..built.clone()
        };
        assert_eq!(round.following(), Some(Following::to(Upcoming::Rechecks)));
        let push = IssueState {
            refused: Some((CommitPoint::Push, 1)),
            ..built.clone()
        };
        assert_eq!(push.following(), Some(Following::to(Upcoming::PushAgain)));
        let commit = IssueState {
            refused: Some((CommitPoint::FixCi, 1)),
            ..built
        };
        assert_eq!(
            commit.following(),
            Some(Following::to(Upcoming::CommitAgain))
        );
    }

    #[test]
    fn nothing_following_is_named_where_the_activity_decides_or_outside_a_running_step() {
        let document = IssueState {
            check: Some(Check::PrdReview),
            ..at(Stage::running(Step::Prd))
        };
        assert_eq!(document.following(), None);
        assert_eq!(at(Stage::running(Step::Design)).following(), None);
        assert_eq!(at(Stage::running(Step::CodeReview)).following(), None);
        assert_eq!(at(Stage::Ci).following(), None);
    }

    #[test]
    fn a_small_change_skips_design() {
        assert!(!Phase::Design.taken_at(Some(Scale::Small)));
        assert!(Phase::Design.taken_at(Some(Scale::Medium)));
        assert!(Phase::Design.taken_at(None));
        assert!(Phase::Build.taken_at(Some(Scale::Small)));
    }
}
