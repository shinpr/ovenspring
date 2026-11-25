//! The overview the dashboard's Now shows: the factory's one state, the running sprint, what waits
//! for the owner, and where each issue of the sprint is. Every value is a code; the dashboard
//! words it.

use ovenspring_core::config::{Config, MergeBy, TrackerKind};
use ovenspring_core::flow::{Cause, IssueState, Stage};
use ovenspring_core::output::{Decision, StepOutput};
use ovenspring_core::phase::{Activity, Phase};
use ovenspring_core::sprint::SprintStage;
use ovenspring_core::step::Step;
use ovenspring_integrations::channel::DecisionKind;
use ovenspring_store::Store;
use serde::Serialize;
use ts_rs::TS;

use crate::DaemonError;
use crate::issues::IssueRecord;
use crate::owner::{DecisionRecord, DecisionStatus};
use crate::prerequisites::Missing;
use crate::records::{self, FactEvent, LaunchLine, Service};

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub goal: String,
    pub status: Status,
    pub sprint: Option<SprintLine>,
    /// Every decision waiting on the owner, sprint decisions first, then by issue.
    pub decisions: Vec<Waiting>,
    /// The running sprint's issues, by key.
    pub issues: Vec<IssueRow>,
    /// Where the issues come from, as the configuration names it.
    pub tracker: Option<TrackerKind>,
    /// The open tracker issues the factory last read.
    #[ts(type = "number")]
    pub open_issues: usize,
    pub last_sprint_ended_at: Option<String>,
    /// Where the lead told the owner that no open issue serves the goal, while that is the state.
    pub told_waiting: Option<records::Told>,
    /// The working agreements' pull request, while the sprint waits for it to merge.
    pub agreements: Option<String>,
    /// What the owner types after `@` to mention the factory in Slack.
    pub mention: String,
}

/// The factory's one state: the first of these that holds, in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum State {
    Paused,
    Prerequisites,
    Unreachable,
    SettingsNotApplied,
    Proposing,
    WaitingForIssues,
    ProposalWithOwner,
    /// A sprint step could not finish and the lead is finding the way forward.
    SprintProblem,
    /// A sprint step waits for the owner's answer.
    SprintWithOwner,
    Reviewing,
    ReviewWithOwner,
    ApplyingAgreements,
    Working,
}

const ORDER: [State; 13] = [
    State::Paused,
    State::Prerequisites,
    State::Unreachable,
    State::SettingsNotApplied,
    State::Proposing,
    State::WaitingForIssues,
    State::ProposalWithOwner,
    State::SprintProblem,
    State::SprintWithOwner,
    State::Reviewing,
    State::ReviewWithOwner,
    State::ApplyingAgreements,
    State::Working,
];

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub state: State,
    /// Why the factory paused, when it did.
    pub reason: Option<String>,
    pub prerequisites: Vec<Missing>,
    /// A service failing to answer and since when.
    pub unreachable: Option<Unreachable>,
    /// Why `.ovenspring/config.toml` cannot be used, while it cannot.
    pub config_error: Option<String>,
    /// Which part of the sprint review runs, while it does.
    pub review_part: Option<ReviewPart>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Unreachable {
    pub service: Service,
    pub since: String,
    #[ts(type = r#""no-connection" | "server-error" | "rate-limited""#)]
    pub why: ovenspring_integrations::http::Unanswered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewPart {
    /// The analyzer gathers the sprint's facts.
    Facts,
    /// The lead writes the review.
    Review,
    /// The reviewer checks the proposed working agreements.
    Agreements,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SprintLine {
    pub id: String,
    pub purpose: String,
    pub started_at: Option<String>,
    /// Issues that ended: merged, closed, dropped or held.
    #[ts(type = "number")]
    pub done: usize,
    #[ts(type = "number")]
    pub total: usize,
}

/// What the owner is asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Ask {
    AnswerQuestion,
    ApprovePrd,
    ApprovePrototype,
    ApproveDesign,
    ApproveMerge,
    /// `merge.by = owner`: the pull request is ready for the owner to merge.
    MergeOnGithub,
    /// The pull request the factory merges waits for the owner's approving review on GitHub.
    ApproveOnGithub,
    ApproveRecovery,
    ResolveStop,
    ApproveSprintProposal,
    /// The review is with the owner: read it, reply, or close the sprint.
    CloseSprint,
}

impl From<DecisionKind> for Ask {
    fn from(kind: DecisionKind) -> Ask {
        match kind {
            DecisionKind::Sprint => Ask::ApproveSprintProposal,
            DecisionKind::Review => Ask::CloseSprint,
            DecisionKind::Spec => Ask::AnswerQuestion,
            DecisionKind::Design => Ask::ApproveDesign,
            DecisionKind::Prototype => Ask::ApprovePrototype,
            DecisionKind::Prd => Ask::ApprovePrd,
            DecisionKind::Merge => Ask::ApproveMerge,
            DecisionKind::Recovery => Ask::ApproveRecovery,
            DecisionKind::Stopped => Ask::ResolveStop,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Waiting {
    pub issue: Option<String>,
    pub ask: Ask,
    /// The issue's title, or the decision's for a sprint decision.
    pub title: String,
    /// Where the owner acts: the Slack message once delivered, or the pull request to merge.
    pub link: Option<String>,
    pub delivered: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct IssueRow {
    pub key: String,
    pub title: String,
    pub now: Position,
}

/// Where an issue is: its section, and by section its phase and activity, why it has not
/// started, or how it ended.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub section: Section,
    pub phase: Option<Phase>,
    pub activity: Option<Activity>,
    /// The task being implemented, when the change has task files.
    pub task: Option<TaskCount>,
    /// What the lead is finding a way past.
    pub problem: Option<Cause>,
    /// In progress while the factory is paused.
    pub on_hold: bool,
    /// When the issue entered its current activity (in progress only).
    pub since: Option<String>,
    pub wait: Option<Wait>,
    pub ended: Option<Ending>,
    pub ended_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Section {
    You,
    InProgress,
    WaitingToStart,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskCount {
    #[ts(type = "number")]
    pub number: usize,
    #[ts(type = "number")]
    pub of: usize,
}

/// Why an issue waits to start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(
    tag = "reason",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum Wait {
    /// Issues it depends on are not merged yet.
    After { keys: Vec<String> },
    /// As many issues as the limit allows are under way.
    Place { held_by: Vec<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Ending {
    Merged,
    /// The lead closed it at its requirements, with evidence that it was already done.
    Closed,
    /// The lead dropped it from a problem it could not get past.
    Dropped,
    /// The owner canceled it.
    Canceled,
    /// Held for the sprint review.
    Held,
}

/// What the overview reads besides the store: why the configuration file cannot be used, which
/// only the caller that loads it knows.
pub fn overview(
    store: &Store,
    config_error: Option<String>,
    config: Option<&Config>,
) -> Result<Overview, DaemonError> {
    let tracker = config.map(|c| c.tracker.kind);
    let goal = records::goal(store)?;
    let sprint = records::sprint(store)?;
    let all = records::issues(store)?;
    let pending: Vec<DecisionRecord> = records::decisions(store)?
        .into_iter()
        .filter(|d| matches!(d.status, DecisionStatus::Unsent | DecisionStatus::Pending))
        .collect();
    let stage = sprint.as_ref().map(|s| &s.stage);
    let status = status(store, stage, config_error)?;
    // A sprint runs from the owner's approval of its proposal; before that it has no issues and
    // no purpose to show as running.
    let running = sprint.as_ref().filter(|s| s.started_at.is_some());
    let purpose = sprint
        .as_ref()
        .and_then(|s| s.plan.as_ref())
        .map(|p| p.purpose.as_str());
    let mine: Vec<&IssueRecord> = match running {
        Some(s) => all
            .iter()
            .filter(|r| r.sprint.as_deref() == Some(s.id.as_str()))
            .collect(),
        _ => Vec::new(),
    };
    let around = Around {
        issues: &all,
        pending: &pending,
        paused: status.reason.is_some(),
        times: Times::read(store)?,
    };
    let told_waiting = if status.state == State::WaitingForIssues {
        records::told_waiting(store)?
    } else {
        None
    };
    let mut issues: Vec<IssueRow> = mine
        .iter()
        .map(|r| IssueRow {
            key: r.key().to_owned(),
            title: r.title.clone(),
            now: position(r, &around),
        })
        .collect();
    issues.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(Overview {
        goal,
        status,
        sprint: running.map(|s| SprintLine {
            id: s.id.clone(),
            purpose: s
                .plan
                .as_ref()
                .map(|p| p.purpose.clone())
                .unwrap_or_default(),
            started_at: s.started_at.clone(),
            done: mine.iter().filter(|r| r.flow.stage.is_finished()).count(),
            total: mine.len(),
        }),
        decisions: merge_agreements(sprint.as_ref(), config)
            .into_iter()
            .chain(waiting(&pending, &all, purpose))
            .collect(),
        issues,
        tracker,
        open_issues: open_issues(store),
        last_sprint_ended_at: crate::history::ended_sprints(store)?
            .into_iter()
            .filter_map(|s| s.ended_at)
            .max(),
        told_waiting,
        agreements: sprint
            .as_ref()
            .filter(|s| s.stage == SprintStage::ApplyingAgreements)
            .and_then(|s| s.agreements_url.clone()),
        mention: crate::prerequisites::mention(store)?,
    })
}

/// The factory's one state, with what explains it.
fn status(
    store: &Store,
    stage: Option<&SprintStage>,
    config_error: Option<String>,
) -> Result<Status, DaemonError> {
    let reason = records::pause_reason(store)?;
    let prerequisites = crate::prerequisites::missing(store)?;
    let unreachable = records::unreachable(store)?
        .into_iter()
        .next()
        .map(|(service, f)| Unreachable {
            service,
            since: f.since,
            why: f.why,
        });
    Ok(Status {
        state: first_holding(|state| match state {
            State::Paused => reason.is_some(),
            State::Prerequisites => !prerequisites.is_empty(),
            State::Unreachable => unreachable.is_some(),
            State::SettingsNotApplied => config_error.is_some(),
            State::Proposing => stage.is_none_or(|s| *s == SprintStage::Proposing),
            State::WaitingForIssues => stage == Some(&SprintStage::WaitingForIssues),
            State::ProposalWithOwner => stage == Some(&SprintStage::AwaitingApproval),
            State::SprintProblem => stage == Some(&SprintStage::Exception),
            State::SprintWithOwner => {
                matches!(stage, Some(SprintStage::Asking { .. }))
            }
            State::Reviewing => review_part(stage).is_some(),
            State::ReviewWithOwner => stage == Some(&SprintStage::AwaitingReviewApproval),
            State::ApplyingAgreements => stage == Some(&SprintStage::ApplyingAgreements),
            State::Working => true,
        }),
        reason,
        prerequisites,
        unreachable,
        config_error,
        review_part: review_part(stage),
    })
}

/// The working agreements' pull request, when it waits on the owner: to merge it under
/// `merge.by = owner`, or to approve it where GitHub requires a review before the factory merges.
fn merge_agreements(
    sprint: Option<&ovenspring_core::sprint::SprintState>,
    config: Option<&Config>,
) -> Option<Waiting> {
    let s = sprint.filter(|s| s.stage == SprintStage::ApplyingAgreements)?;
    let ask = match config?.merge.by {
        MergeBy::Owner => Ask::MergeOnGithub,
        MergeBy::Factory if s.agreements_need_approval => Ask::ApproveOnGithub,
        MergeBy::Factory => return None,
    };
    Some(Waiting {
        issue: None,
        ask,
        title: "Working agreements and knowledge".into(),
        link: s.agreements_url.clone(),
        delivered: s.agreements_url.is_some(),
    })
}

fn open_issues(store: &Store) -> usize {
    std::fs::read_dir(store.root().join("sources/issues")).map_or(0, |entries| {
        entries
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "md"))
            .count()
    })
}

fn first_holding(holds: impl Fn(State) -> bool) -> State {
    ORDER
        .into_iter()
        .find(|state| holds(*state))
        .unwrap_or(State::Working)
}

fn review_part(stage: Option<&SprintStage>) -> Option<ReviewPart> {
    match stage? {
        SprintStage::Collecting => Some(ReviewPart::Facts),
        SprintStage::Reviewing => Some(ReviewPart::Review),
        SprintStage::CheckingAgreements => Some(ReviewPart::Agreements),
        SprintStage::Proposing
        | SprintStage::AwaitingApproval
        | SprintStage::Running
        | SprintStage::AwaitingReviewApproval
        | SprintStage::ApplyingAgreements
        | SprintStage::WaitingForIssues
        | SprintStage::Exception
        | SprintStage::Asking { .. } => None,
    }
}

/// The pending decisions, with each issue ready for the owner to merge on GitHub, which has no
/// decision of its own. A decision about the sprint goes by `purpose`, the purpose its sprint
/// proposes or reached for.
pub(crate) fn waiting(
    pending: &[DecisionRecord],
    issues: &[IssueRecord],
    purpose: Option<&str>,
) -> Vec<Waiting> {
    let title_of = |key: &str| {
        issues
            .iter()
            .find(|r| r.key() == key)
            .map(|r| r.title.clone())
    };
    let mut out: Vec<Waiting> = pending
        .iter()
        .map(|d| {
            let delivered = d.status == DecisionStatus::Pending;
            Waiting {
                issue: d.issue.clone(),
                ask: d.kind.into(),
                title: d
                    .issue
                    .as_deref()
                    .and_then(title_of)
                    .or_else(|| purpose.map(str::to_owned))
                    .unwrap_or_else(|| d.title.clone()),
                link: d
                    .message
                    .as_ref()
                    .and_then(|m| m.permalink.clone())
                    .filter(|_| delivered),
                delivered,
            }
        })
        .chain(
            issues
                .iter()
                .filter(|r| r.flow.stage == Stage::ReadyForOwner)
                .map(|r| Waiting {
                    issue: Some(r.key().to_owned()),
                    ask: Ask::MergeOnGithub,
                    title: r.title.clone(),
                    link: r.pr_url.clone(),
                    delivered: r.pr_url.is_some(),
                }),
        )
        .collect();
    out.sort_by(|a, b| a.issue.cmp(&b.issue));
    out
}

/// When each issue entered its current stage, and when its launches started.
pub(crate) struct Times {
    events: Vec<FactEvent>,
    launches: Vec<(String, Step, String)>,
}

impl Times {
    pub(crate) fn read(store: &Store) -> Result<Times, DaemonError> {
        let launches = records::launch_lines(store)?
            .into_iter()
            .filter_map(|l| match l {
                LaunchLine::Start(s) => Some((s.issue?, s.step, s.at)),
                LaunchLine::End(_) => None,
            })
            .collect();
        Ok(Times {
            events: records::fact_events(store)?,
            launches,
        })
    }

    /// When the issue last changed stage.
    fn entered(&self, key: &str) -> Option<&str> {
        self.events
            .iter()
            .rev()
            .find(|e| e.issue == key && e.from != e.to)
            .map(|e| e.at.as_str())
    }

    /// When the issue entered what it is doing now: for a running step, its latest launch.
    fn since(&self, key: &str, state: &IssueState) -> Option<String> {
        let entered = self.entered(key);
        let step = if let Stage::Running { step } = state.stage {
            Some(step)
        } else if matches!(state.stage, Stage::Exception { .. }) {
            Some(Step::Exception)
        } else {
            None
        };
        let launched = step.and_then(|step| {
            self.launches
                .iter()
                .rev()
                .find(|(k, s, _)| k == key && *s == step)
                .map(|(_, _, at)| at.as_str())
        });
        // RFC 3339 times in one zone order as text.
        launched.max(entered).map(str::to_owned)
    }
}

/// What placing an issue reads besides its own record.
pub(crate) struct Around<'a> {
    /// Every issue, for what a queued one waits on.
    pub(crate) issues: &'a [IssueRecord],
    pub(crate) pending: &'a [DecisionRecord],
    pub(crate) paused: bool,
    pub(crate) times: Times,
}

/// Where `record` is.
pub(crate) fn position(record: &IssueRecord, around: &Around<'_>) -> Position {
    let Around {
        issues: all,
        pending,
        paused,
        times,
    } = around;
    let paused = *paused;
    let key = record.key();
    let flow = &record.flow;
    let asked = pending.iter().any(|d| d.issue.as_deref() == Some(key));
    let section = section(&flow.stage, asked);
    let in_progress = section == Section::InProgress;
    let ended = ending(record);
    Position {
        section,
        phase: flow.phase().filter(|_| section != Section::Done),
        activity: flow.activity().filter(|_| in_progress),
        task: flow
            .task_of()
            .filter(|_| in_progress)
            .map(|(number, of)| TaskCount { number, of }),
        problem: if let Stage::Exception { cause, .. } = &flow.stage
            && in_progress
        {
            Some(cause.clone())
        } else {
            None
        },
        on_hold: paused && in_progress,
        since: in_progress.then(|| times.since(key, flow)).flatten(),
        wait: (section == Section::WaitingToStart).then(|| wait(record, all)),
        ended_at: ended.and_then(|e| match e {
            Ending::Merged => record.merged_at.clone(),
            Ending::Closed | Ending::Dropped | Ending::Canceled | Ending::Held => {
                times.entered(key).map(str::to_owned)
            }
        }),
        ended,
    }
}

fn section(stage: &Stage, asked: bool) -> Section {
    match stage {
        Stage::Merged | Stage::Dropped | Stage::Held => Section::Done,
        Stage::Queued => Section::WaitingToStart,
        Stage::Asking { .. }
        | Stage::LeadAsking { .. }
        | Stage::Stopped { .. }
        | Stage::AwaitingRecovery { .. }
        | Stage::AwaitingApproval { .. }
        | Stage::HeldForOwner
        | Stage::ReadyForOwner => Section::You,
        Stage::Running { .. }
        | Stage::Reviewing
        | Stage::Committing { .. }
        | Stage::Pushing
        | Stage::Ci
        | Stage::WaitingBots
        | Stage::WaitingReview
        | Stage::Gate
        | Stage::BlockedOnGitHub
        | Stage::Exception { .. } => {
            if asked {
                Section::You
            } else {
                Section::InProgress
            }
        }
    }
}

fn ending(record: &IssueRecord) -> Option<Ending> {
    Some(match record.flow.stage {
        Stage::Merged => Ending::Merged,
        Stage::Held => Ending::Held,
        Stage::Dropped if record.canceled.is_some() => Ending::Canceled,
        Stage::Dropped => match record.output(Step::Requirements) {
            Some(StepOutput::Requirements(r)) if r.decision == Decision::Close => Ending::Closed,
            _ => Ending::Dropped,
        },
        Stage::Queued
        | Stage::Running { .. }
        | Stage::Asking { .. }
        | Stage::LeadAsking { .. }
        | Stage::Stopped { .. }
        | Stage::AwaitingRecovery { .. }
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
        | Stage::Exception { .. } => return None,
    })
}

/// Why a queued issue has not started: the issues it depends on that are not merged, or else the
/// issues holding the places.
fn wait(record: &IssueRecord, all: &[IssueRecord]) -> Wait {
    let keys = crate::sprint::unmerged_dependencies(record, all);
    if keys.is_empty() {
        Wait::Place {
            held_by: crate::sprint::under_way(all)
                .map(|r| r.key().to_owned())
                .collect(),
        }
    } else {
        Wait::After { keys }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_state_is_the_first_that_holds_in_order() {
        let state = |holding: &[State]| first_holding(|s| holding.contains(&s));
        assert_eq!(
            state(&[State::Working, State::Proposing, State::Paused]),
            State::Paused
        );
        assert_eq!(
            state(&[State::Proposing, State::Unreachable]),
            State::Unreachable
        );
        assert_eq!(state(&[]), State::Working);
    }

    #[test]
    fn an_issue_with_a_decision_waits_for_the_owner_whatever_it_runs() {
        assert_eq!(section(&Stage::Ci, false), Section::InProgress);
        assert_eq!(section(&Stage::Ci, true), Section::You);
        assert_eq!(section(&Stage::ReadyForOwner, false), Section::You);
        assert_eq!(section(&Stage::Merged, true), Section::Done);
        assert_eq!(section(&Stage::Queued, false), Section::WaitingToStart);
    }
}
