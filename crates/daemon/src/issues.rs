//! Issues on the board: each issue's record, the facts its finished items produce, and the
//! actions `flow::next` returns, carried out one fact at a time.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use ovenspring_core::config::Role;
use ovenspring_core::flow::{self, Action, Cause, CommitPoint, Fact, Filing, IssueState, Stage};
use ovenspring_core::item::{Front, Item, Status};
use ovenspring_core::output::{
    Disposition, DocumentStatus, ExecutorStatus, PrepareOutput, QualityStatus, Scale, StepOutput,
};
use ovenspring_core::plan;
use ovenspring_core::step::{Document, Step};
use ovenspring_store::{StoreError, item_name};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::worktree::{git, git_as};
use crate::{Daemon, DaemonError};

/// What the daemon keeps about an issue: the flow's state and what the daemon's own actions need.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IssueRecord {
    pub flow: IssueState,
    pub title: String,
    /// The issue on the tracker.
    #[serde(default)]
    pub url: Option<String>,
    pub depends_on: Vec<String>,
    /// The latest output of each step, by step.
    pub outputs: BTreeMap<String, String>,
    /// The item that last ended, for the lead's `exception`.
    pub last_item: Option<String>,
    pub pushed_sha: Option<String>,
    /// When `pushed_sha` was pushed (RFC 3339).
    pub pushed_at: Option<String>,
    pub pr_url: Option<String>,
    /// Keys of pull request comments already given to an agent.
    pub comments_seen: BTreeSet<String>,
    /// The latest `sources/comments/` file.
    pub comments_source: Option<String>,
    /// The replies owed to the judged comments, each kept until it is posted or its failure is
    /// recorded.
    #[serde(default)]
    pub replies: Vec<DueReply>,
    /// When the wait for review bots began (RFC 3339).
    pub bots_since: Option<String>,
    /// The owner's answers, as source files the next item names.
    pub answers: Vec<String>,
    /// The sprint that put the issue on the board.
    pub sprint: Option<String>,
    /// When it merged (RFC 3339).
    pub merged_at: Option<String>,
    /// The review units of a large change: task indexes, from `decompose`'s task files.
    #[serde(default)]
    pub units: Vec<Vec<usize>>,
    /// The worktree's tree before the first task, then after each `implement` (with the task it
    /// carried out) and each `review-response` (with none): a task's changed files, and a
    /// correction's, are the difference from the tree before it.
    #[serde(default)]
    pub trees: Vec<(Option<usize>, String)>,
    /// The latest `implement` or `review-response` change: what a test review after it reviews.
    #[serde(default)]
    pub change: Option<Change>,
    /// The `changeSummary` of each implementation since the last commit, for its message.
    #[serde(default)]
    pub summaries: Vec<String>,
    /// The factory's latest commit on the issue branch.
    pub last_commit: Option<String>,
    /// What the repository said when it last refused the factory's commit or push.
    #[serde(default)]
    pub refusal: Option<String>,
    /// The findings each check last reported, by output key, as its source wrote them: what
    /// `prior_feedback` and `correction_findings` carry.
    #[serde(default)]
    pub findings: BTreeMap<String, Vec<serde_json::Value>>,
    /// The daemon operation that stopped the issue, while it is stopped.
    #[serde(default)]
    pub stopped: Option<crate::stops::StopRecord>,
    /// What a failed recovery procedure printed, as a source the lead's next `exception` reads.
    #[serde(default)]
    pub recovery_log: Option<String>,
    /// The design-phase session the next run of its step resumes.
    #[serde(default)]
    pub resume: Option<Resume>,
    /// Why the owner canceled the issue, as the tracker is told.
    #[serde(default)]
    pub canceled: Option<String>,
}

/// The worktree's trees around an executor's change. `before` is the tree when its launch was
/// placed, moved past what a merge from the branch brought in while it was open; `after` is the
/// tree when it ended, `None` while it is open (a blocked executor's retry continues it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Change {
    pub before: String,
    pub after: Option<String>,
}

/// A reply owed to a judged pull request comment. An applied comment is answered with the first
/// commit made after its judgment, once a push carries it; a declined one, with its reason.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DueReply {
    pub comment: String,
    pub disposition: Disposition,
    pub reason: String,
    #[serde(default)]
    pub commit: Option<String>,
}

/// A design-phase launch's session, for the next run of the same step to resume: the preparation
/// turn's, or the work turn's that asked the owner. `session` is `None` when its launch recorded
/// none, and the next run starts fresh; `question` is the preparation's message when it asks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Resume {
    pub step: Step,
    pub session: Option<String>,
    pub question: Option<String>,
}

impl IssueRecord {
    pub fn new(key: &str, title: &str, depends_on: Vec<String>) -> IssueRecord {
        IssueRecord {
            flow: IssueState::new(key),
            title: title.into(),
            url: None,
            depends_on,
            outputs: BTreeMap::new(),
            last_item: None,
            pushed_sha: None,
            pushed_at: None,
            pr_url: None,
            comments_seen: BTreeSet::new(),
            comments_source: None,
            replies: Vec::new(),
            bots_since: None,
            answers: Vec::new(),
            sprint: None,
            merged_at: None,
            units: Vec::new(),
            trees: Vec::new(),
            summaries: Vec::new(),
            last_commit: None,
            refusal: None,
            findings: BTreeMap::new(),
            stopped: None,
            recovery_log: None,
            resume: None,
            canceled: None,
            change: None,
        }
    }

    /// Whether a launch of `step` is in the design phase's dialogue: a lead's, before the issue's
    /// execution starts (test design, the plan, or a small issue's implementation). `exception`
    /// is the escalation route of both phases, not a step of the design.
    pub fn in_dialogue(&self, step: Step) -> bool {
        step.role() == Role::Lead
            && step != Step::Exception
            && ![Step::TestDesign, Step::WorkPlan, Step::Implement]
                .iter()
                .any(|s| self.outputs.contains_key(s.as_str()))
    }

    pub fn key(&self) -> &str {
        &self.flow.key
    }

    pub fn branch(&self) -> String {
        crate::worktree::branch_for(self.key())
    }

    /// Where the Design Doc is, as the design step reported it.
    pub fn design_path(&self) -> Option<String> {
        match self.output(Step::Design) {
            Some(StepOutput::Document(d)) => d.path,
            _ => None,
        }
    }

    /// Owes a reply to each accepted comment the latest `review-response` judged. A disposition
    /// naming no comment of the accepted batch gets none.
    pub(crate) fn owe_replies(&mut self) {
        let Some(StepOutput::Executor(judged)) = self.output(Step::ReviewResponse) else {
            return;
        };
        let accepted = self
            .findings
            .get(crate::handoff::COMMENTS)
            .map(Vec::as_slice)
            .unwrap_or_default();
        self.replies = judged
            .dispositions
            .into_iter()
            .filter(|d| {
                accepted
                    .iter()
                    .any(|f| f.get("id").and_then(serde_json::Value::as_str) == Some(&d.id))
            })
            .map(|d| DueReply {
                comment: d.id,
                disposition: d.disposition,
                reason: d.reason,
                commit: None,
            })
            .collect();
    }

    /// A commit point was passed with `commit` at the branch's head: the first commit after a
    /// comment's judgment is the one that applied it.
    fn committed(&mut self, commit: Option<&str>) {
        for reply in &mut self.replies {
            if reply.disposition == Disposition::Apply && reply.commit.is_none() {
                reply.commit = commit.map(str::to_owned);
            }
        }
    }

    pub fn output(&self, step: Step) -> Option<StepOutput> {
        self.output_at(step, step.as_str())
    }

    /// The output of `step` kept under the output `key`.
    pub fn output_at(&self, step: Step, key: &str) -> Option<StepOutput> {
        let path = self.outputs.get(key)?;
        let value = serde_json::from_str(&fs::read_to_string(path).ok()?).ok()?;
        StepOutput::parse(step, value).ok()
    }

    /// The latest output of `key` as its agent wrote it.
    pub fn raw_output(&self, key: &str) -> Option<serde_json::Value> {
        let path = self.outputs.get(key)?;
        serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
    }

    /// The files each task changed, from the trees recorded around it.
    pub(crate) fn task_files(&self, worktree: &Path) -> BTreeMap<usize, BTreeSet<String>> {
        let mut files: BTreeMap<usize, BTreeSet<String>> = BTreeMap::new();
        for pair in self.trees.windows(2) {
            let [(_, before), (Some(task), after)] = pair else {
                continue;
            };
            let changed = git(
                worktree,
                &["diff-tree", "-r", "-z", "--name-only", before, after],
            )
            .unwrap_or_default();
            files.entry(*task).or_default().extend(
                changed
                    .split('\0')
                    .filter(|f| !f.is_empty())
                    .map(String::from),
            );
        }
        files
    }
}

/// Steps that are the orchestrator's or the factory's own judgment: they read the issue's records.
const JUDGING: [Step; 7] = [
    Step::Requirements,
    Step::ScopeCheck,
    Step::RerunCheck,
    Step::Exception,
    Step::Prototype,
    Step::FixCi,
    Step::MergeBase,
];

pub(crate) fn record_path(key: &str) -> String {
    format!("issues/{key}.json")
}

impl Daemon {
    pub fn issue(&self, key: &str) -> Result<Option<IssueRecord>, DaemonError> {
        crate::records::issue(&self.store, key)
    }

    /// The issue's record, which the caller knows exists.
    pub(crate) fn record_of(&self, key: &str) -> Result<IssueRecord, DaemonError> {
        self.issue(key)?
            .ok_or_else(|| DaemonError::NoRecord(key.to_owned()))
    }

    pub fn issues(&self) -> Result<Vec<IssueRecord>, DaemonError> {
        crate::records::issues(&self.store)
    }

    /// Saves the record; one that has just merged gets its merge time.
    pub(crate) fn save_issue(&self, record: &IssueRecord) -> Result<(), DaemonError> {
        let mut record = record.clone();
        if record.flow.stage == Stage::Merged && record.merged_at.is_none() {
            record.merged_at = Some(jiff::Timestamp::now().to_string());
        }
        self.store
            .write_record(&record_path(record.key()), &record)?;
        Ok(())
    }

    /// Puts an issue on the board. It starts once every issue it depends on is merged and fewer
    /// than `limits.issues_at_once` are under way.
    pub fn begin_issue(
        &mut self,
        key: &str,
        title: &str,
        depends_on: Vec<String>,
    ) -> Result<(), DaemonError> {
        if self.issue(key)?.is_some() {
            return Ok(());
        }
        let record = IssueRecord::new(key, title, depends_on);
        self.save_issue(&record)?;
        self.event(&json!({"event": "issue-begun", "issue": key}))
    }

    /// Applies one fact to an issue and carries out the actions it returns, including the facts
    /// those actions produce, until none is left.
    pub fn apply(&mut self, key: &str, fact: Fact) -> Result<(), DaemonError> {
        let mut queue = VecDeque::from([fact]);
        while let Some(fact) = queue.pop_front() {
            let Some(mut record) = self.issue(key)? else {
                return Ok(());
            };
            let (state, actions) = flow::next(&record.flow, &fact, self.flow_limits());
            let from = record.flow.stage.clone();
            record.flow = state;
            if record.flow.resume().is_none() {
                record.stopped = None;
            }
            self.save_issue(&record)?;
            if record.flow.stage.is_finished() && !from.is_finished() {
                self.end_work(key)?;
                self.supersede_open_decisions(key)?;
            }
            self.event(&json!(crate::records::FactEvent {
                issue: key.to_owned(),
                fact: crate::records::fact_name(&fact),
                from,
                to: record.flow.stage.clone(),
                at: String::new(),
            }))?;
            for action in actions {
                if let Some(next) = self.act(key, &action)? {
                    queue.push_back(next);
                }
            }
        }
        Ok(())
    }

    /// An issue that ended while work for it was open (merged or dropped by someone, or with a
    /// step under way): its launches stop and its items are archived.
    pub(crate) fn end_work(&mut self, key: &str) -> Result<(), DaemonError> {
        for path in self.store.active_items()? {
            if self.store.read_item(&path)?.front.issue.as_deref() != Some(key) {
                continue;
            }
            let name = item_name(&path);
            self.stop_launch(&name)?;
            self.store.archive_item(&path)?;
            self.event(&json!({"event": "item-ended-with-issue", "issue": key, "item": name}))?;
        }
        Ok(())
    }

    fn flow_limits(&self) -> flow::Limits {
        flow::Limits {
            review_rounds: self.config.limits.review_rounds,
            fix_ci_rounds: self.config.limits.fix_ci_rounds,
        }
    }

    fn act(&mut self, key: &str, action: &Action) -> Result<Option<Fact>, DaemonError> {
        match action {
            Action::File(filing) => {
                self.file_step(key, filing)?;
                Ok(None)
            }
            Action::Commit { after } => self.commit(key, *after).map(Some),
            Action::StopApp => {
                self.stop_app(key);
                Ok(None)
            }
            Action::ClosePull => {
                self.close_pull(key)?;
                Ok(None)
            }
            Action::Push => self.push(key),
            Action::AskOwner => {
                self.ask_owner(key)?;
                Ok(None)
            }
            Action::RequestApproval => {
                self.request_approval(key)?;
                Ok(None)
            }
            Action::WaitForBots => {
                self.update_issue(key, |r| {
                    r.bots_since = Some(jiff::Timestamp::now().to_string());
                })?;
                Ok(None)
            }
            Action::AnswerComments => {
                self.answer_comments(key, false)?;
                Ok(None)
            }
            Action::RunGate => self.run_gate(key),
            Action::NotifyReadyForOwner => {
                self.notify_ready(key)?;
                Ok(None)
            }
            Action::CloseOnTracker => {
                self.close_on_tracker(key)?;
                Ok(None)
            }
            Action::StartOnTracker => {
                if let Some(tracker) = &self.services.tracker {
                    match tracker.client.start(key) {
                        Ok(()) => self.event(&json!({"event": "started-on-tracker", "issue": key}))?,
                        Err(e) => self.event(
                            &json!({"event": "tracker-error", "issue": key, "error": e.to_string()}),
                        )?,
                    }
                }
                Ok(None)
            }
            Action::NotifyDependents => {
                self.notify_dependents(key)?;
                Ok(None)
            }
            Action::ReportStopped => {
                self.report_stopped_for(Some(key))?;
                Ok(None)
            }
            Action::RequestRecovery => {
                self.request_recovery_for(Some(key))?;
                Ok(None)
            }
            Action::RecordReviewRound { number } => {
                self.record_review_round(key, *number)?;
                Ok(None)
            }
        }
    }

    /// The line that review round `number` ended, with the count of findings it hands to
    /// correction: each entry `review-response` receives, none when the round converged.
    fn record_review_round(&self, key: &str, number: u32) -> Result<(), DaemonError> {
        let record = self.record_of(key)?;
        let findings = if record.flow.correction == Some(ovenspring_core::flow::Correction::Round) {
            crate::handoff::handed(&record).len()
        } else {
            0
        };
        self.event(&json!({
            "event": "review-round-ended", "issue": key, "round": number, "findings": findings,
        }))
    }

    pub(crate) fn update_issue(
        &self,
        key: &str,
        change: impl FnOnce(&mut IssueRecord),
    ) -> Result<IssueRecord, DaemonError> {
        let mut record = self.record_of(key)?;
        change(&mut record);
        self.save_issue(&record)?;
        Ok(record)
    }

    /// After an issue ends: a merged one is closed on the tracker and its branch and app are
    /// cleared; the issues waiting on a dropped one learn of it. Issues a merge unblocks start from
    /// `schedule`, within `limits.issues_at_once`.
    fn notify_dependents(&mut self, key: &str) -> Result<(), DaemonError> {
        let Some(me) = self.issue(key)? else {
            return Ok(());
        };
        if me.flow.stage == Stage::Merged {
            if let Some(tracker) = &self.services.tracker {
                match tracker
                    .client
                    .set_state(key, ovenspring_integrations::tracker::TrackerState::Done)
                {
                    Ok(()) => self.event(&json!({"event": "closed-on-tracker", "issue": key,
                                                 "state": "done", "tracker": tracker.kind}))?,
                    Err(e) => self.event(
                        &json!({"event": "tracker-error", "issue": key, "error": e.to_string()}),
                    )?,
                }
            }
            self.delete_branch(("issue", key), &me.branch())?;
            self.stop_app(key);
            return Ok(());
        }
        for other in self.issues()? {
            if other.flow.stage == Stage::Queued && other.depends_on.iter().any(|d| d == key) {
                self.apply(
                    other.key(),
                    Fact::DependencyLost {
                        dependency: key.into(),
                    },
                )?;
            }
        }
        Ok(())
    }

    /// Deletes a merged branch from GitHub; one the repository already deleted is left as it is.
    /// A failure is logged under its owner: `("issue", key)` or `("sprint", id)`.
    pub(crate) fn delete_branch(
        &self,
        (field, name): (&str, &str),
        branch: &str,
    ) -> Result<(), DaemonError> {
        let Some(gh) = &self.services.github else {
            return Ok(());
        };
        match gh.delete_branch(branch) {
            Ok(()) => self.event(&json!({"event": "branch-deleted", "branch": branch})),
            Err(e) if matches!(e.status(), Some(404 | 422)) => Ok(()),
            Err(e) => {
                self.event(&json!({"event": "github-error", field: name, "error": e.to_string()}))
            }
        }
    }

    /// Cancels each issue the owner asked the dashboard to cancel.
    pub(crate) fn take_cancels(&mut self) -> Result<(), DaemonError> {
        for (key, request) in crate::records::take_cancel_requests(&self.store)? {
            self.cancel_issue(&key, &request.words)?;
        }
        Ok(())
    }

    /// The owner's cancel ends the issue wherever it is; the tracker is told their words.
    pub(crate) fn cancel_issue(&mut self, key: &str, words: &str) -> Result<(), DaemonError> {
        if self.issue(key)?.is_none() {
            return Ok(());
        }
        self.update_issue(key, |r| r.canceled = Some(words.to_owned()))?;
        self.event(&json!({"event": "canceled", "issue": key}))?;
        self.apply(key, Fact::Canceled)
    }

    /// Closes the issue's pull request without merging it, keeping its branch for whoever takes
    /// the work up again.
    fn close_pull(&self, key: &str) -> Result<(), DaemonError> {
        let (Some(gh), Some(pr)) = (&self.services.github, self.record_of(key)?.flow.pr) else {
            return Ok(());
        };
        match gh.close_pull(pr) {
            Ok(()) => self.event(&json!({"event": "pull-closed", "issue": key, "pr": pr})),
            Err(e) => {
                self.event(&json!({"event": "github-error", "issue": key, "error": e.to_string()}))
            }
        }
    }

    fn close_on_tracker(&self, key: &str) -> Result<(), DaemonError> {
        let Some(record) = self.issue(key)? else {
            return Ok(());
        };
        let evidence = if let Some(words) = &record.canceled {
            if words.is_empty() {
                "Canceled by the owner.".to_owned()
            } else {
                format!("Canceled by the owner: {words}")
            }
        } else {
            match (
                record.output(Step::Exception),
                record.output(Step::Requirements),
            ) {
                (Some(StepOutput::Exception(e)), _)
                    if record.flow.stage == Stage::Dropped
                        && record.outputs.contains_key(Step::Exception.as_str()) =>
                {
                    e.note
                }
                (_, Some(StepOutput::Requirements(r))) => {
                    r.evidence.unwrap_or(r.convergence.outcome)
                }
                _ => "Closed by the factory.".into(),
            }
        };
        let Some(tracker) = &self.services.tracker else {
            return self.event(&json!({"event": "close-on-tracker", "issue": key, "note": "no tracker configured"}));
        };
        let result = tracker.client.comment(key, &evidence).and_then(|()| {
            tracker.client.set_state(
                key,
                ovenspring_integrations::tracker::TrackerState::Canceled,
            )
        });
        match result {
            Ok(()) => self.event(&json!({"event": "closed-on-tracker", "issue": key,
                                         "state": "canceled", "tracker": tracker.kind})),
            Err(e) => {
                self.event(&json!({"event": "tracker-error", "issue": key, "error": e.to_string()}))
            }
        }
    }

    fn file_step(&self, key: &str, filing: &Filing) -> Result<PathBuf, DaemonError> {
        let step = filing.step;
        let record = self.record_of(key)?;
        let name = self.new_item_name(&format!("{step}-{key}"));
        let issue = self
            .store
            .source(&format!("issues/{key}.md"))
            .display()
            .to_string();
        let mut refs = vec![issue.clone()];
        refs.extend(record.answers.iter().cloned());
        // The orchestrator's and the factory's own steps judge from the issue's records; a source
        // agent's step receives its inputs as entries below.
        if JUDGING.contains(&step) {
            refs.extend(record.outputs.values().cloned());
        }
        if record.flow.corrected() == Some(ovenspring_core::flow::Corrected::PrComments)
            && step == Step::ReviewResponse
        {
            refs.extend(record.comments_source.iter().cloned());
        }
        if step == Step::Exception {
            refs.extend(record.last_item.iter().cloned());
            refs.extend(record.recovery_log.iter().cloned());
        }
        let resumed = record.resume.as_ref().filter(|r| r.step == step);
        let prepare = resumed.is_none() && record.in_dialogue(step);
        let resume = resumed.and_then(|r| r.session.clone());
        if record.resume.is_some() {
            self.update_issue(key, |r| r.resume = None)?;
        }
        let worktree = self.worktree_of(key);
        let mut request = self.request_text(&record, filing)?;
        let entries = crate::handoff::Ctx {
            record: &record,
            filing,
            issue: &issue,
            base: &self.config.repo.base_branch,
            worktree: &worktree,
        }
        .inputs();
        if !entries.is_empty() {
            request.push('\n');
        }
        for (field, entry) in entries {
            let value = match entry {
                crate::handoff::Entry::Text(t) => t,
                crate::handoff::Entry::Json(v) => self.write_source(
                    &format!(
                        "handoffs/{name}-{}.json",
                        field.to_lowercase().replace(' ', "-")
                    ),
                    &serde_json::to_string_pretty(&v).unwrap_or_default(),
                )?,
            };
            let _ = write!(request, "\n{field}: {value}");
        }
        let item = Item {
            front: Front {
                step,
                to: filing.role,
                status: Status::Open,
                issue: Some(key.into()),
                sprint: None,
                round: filing.round,
                unit: filing.unit,
                refs,
                out: self.store.output_path(&name).display().to_string(),
                prepare,
                resume,
            },
            body: format!("{request}\n\n## Result\n"),
        };
        let path = self.store.file_item(&name, &item)?;
        let filed = if prepare {
            json!({"event": "filed", "issue": key, "item": name, "step": step, "turn": "prepare"})
        } else {
            json!({"event": "filed", "issue": key, "item": name, "step": step})
        };
        self.event(&filed)?;
        Ok(path)
    }

    /// A name for a new item about `what`: the local time it is filed, then `what`, made unique
    /// on the board and in the archive.
    pub(crate) fn new_item_name(&self, what: &str) -> String {
        let stamp = jiff::ZonedDateTime::now().strftime("%Y%m%d-%H%M%S");
        self.store.unused_item_name(&format!("{stamp}-{what}"))
    }

    /// The request an item carries: what to do, in one or two sentences, with what the step
    /// needs that no source file holds.
    fn request_text(&self, record: &IssueRecord, filing: &Filing) -> Result<String, DaemonError> {
        let key = record.key();
        let round = filing
            .round
            .map(|r| format!(" (round {r})"))
            .unwrap_or_default();
        let plans = self.store.root().join("plans").display().to_string();
        let correcting = record.flow.correcting();
        let document = filing.step.document().map(Document::name);
        if let (true, Some(document)) = (correcting, document) {
            return Ok(format!(
                "Judge the findings on the {document} of {key}, and update it (update mode) for those you apply."
            ));
        }
        Ok(match filing.step {
            Step::Requirements => format!("Settle the requirements of {key}: {}.", record.title),
            Step::Prd => match record.output(Step::Requirements) {
                Some(StepOutput::Requirements(r)) if r.prd_update.is_some() => format!(
                    "Update the PRD {} (update mode) for what {key} changes in the behavior it states.",
                    r.prd_update.unwrap_or_default()
                ),
                _ => format!("Write the PRD of {key}."),
            },
            Step::PrdReview => format!("Review the PRD of {key}{round}."),
            Step::CodebaseAnalysis => format!("Analyze the code {key} concerns."),
            Step::ScopeCheck => format!("Settle {key}'s scope with its codebase analysis."),
            Step::Prototype => format!("Build the prototype of the screen {key} adds."),
            Step::UiAnalysis => format!("Gather the UI facts for {key}'s UI Spec."),
            Step::UiSpec => format!("Write the UI Spec of {key} from its approved prototype."),
            Step::UiSpecReview => format!("Review the UI Spec of {key}{round}."),
            Step::Adr => format!("Record the technical decisions the owner answered for {key}."),
            Step::AdrReview => format!("Review the ADRs of {key}{round}."),
            Step::Design => format!("Write the Design Doc of {key}."),
            Step::DesignVerify => {
                format!("Verify the Design Doc of {key} against the code{round}.")
            }
            Step::DesignReview => format!("Review the Design Doc of {key}{round}."),
            Step::DesignSync => format!(
                "Check the Design Doc of {key} against the repository's other Design Docs{round}."
            ),
            Step::TestDesign => format!("Generate the test skeletons of {key}."),
            Step::WorkPlan => format!("Write the Work Plan of {key} in {plans}/{key}.md."),
            Step::PlanReview => format!("Review the Work Plan of {key}{round}."),
            Step::Decompose => {
                format!("Decompose the Work Plan of {key} into task files in {plans}/tasks/.")
            }
            Step::Implement => match filing.task.and_then(|t| record.flow.tasks.get(t)) {
                Some(task) => format!("Carry out the task in {}.", task.path),
                None => format!("Implement {key} within its requirements."),
            },
            Step::TestReview => format!(
                "Review the integration and E2E tests {key}'s change added or updated{round}."
            ),
            Step::Quality => {
                format!("Run the quality checks on {key}'s change and fix what fails.")
            }
            Step::CodeReview => match filing.unit.filter(|_| record.units.len() > 1) {
                Some(u) => format!("Review unit {u} of {key}'s change{round}."),
                None => format!("Review the code of {key}{round}."),
            },
            Step::SecurityReview => format!("Review the security of {key}'s change{round}."),
            Step::RerunCheck => format!(
                "Judge whether {key}'s correction changed the evidence the reviews that passed relied on."
            ),
            Step::ReviewResponse
                if record.flow.corrected()
                    == Some(ovenspring_core::flow::Corrected::PrComments) =>
            {
                format!(
                    "Judge the new comments on {key}'s pull request {}, each as a finding, and correct the implementation for those you apply.",
                    record.pr_url.as_deref().unwrap_or_default()
                )
            }
            Step::ReviewResponse => {
                format!("Judge the findings on {key} and complete each correction you apply.")
            }
            Step::FixCi => self.fix_ci_request(record)?,
            Step::MergeBase => format!(
                "First bring `origin/{}` and `origin/{}` into {key}'s branch with `git merge` and resolve each conflict within the issue's outcome: merging them before the push conflicted.",
                record.branch(),
                self.config.repo.base_branch
            ),
            Step::Exception => exception_request(record),
            // The sprint's and the conversation's steps are filed for no issue.
            Step::SprintPlan
            | Step::Facts
            | Step::SprintReview
            | Step::AgreementsCheck
            | Step::Conversation
            | Step::RunCommands => format!("Run {} for {key}.", filing.step),
        })
    }

    /// fix-ci's request, naming the checks that failed on the pushed head.
    fn fix_ci_request(&self, record: &IssueRecord) -> Result<String, DaemonError> {
        let key = record.key();
        let failed = match &record.pushed_sha {
            Some(sha) => self.failed_checks(sha)?.unwrap_or_default(),
            None => vec![],
        };
        let at = record.pushed_sha.as_deref().unwrap_or("its head");
        let which = if failed.is_empty() {
            String::new()
        } else {
            format!("; these failed: {}", failed.join(", "))
        };
        Ok(format!(
            "Make the checks pass on {key}'s branch at {at}{which}. Fix what the failed checks and their logs show."
        ))
    }

    /// Turns each finished issue item into its fact, archiving the item first.
    pub(crate) fn consume_finished(&mut self) -> Result<(), DaemonError> {
        for path in self.store.active_items()? {
            let name = item_name(&path);
            if self.running.contains_key(&name) {
                continue;
            }
            let item = self.store.read_item(&path)?;
            let Some(key) = item.front.issue.clone() else {
                if item.front.step == Step::Conversation {
                    self.consume_conversation_item(&path, &item)?;
                } else if item.front.step == Step::RunCommands {
                    self.consume_run_commands_item(&path, &item)?;
                } else if item.front.sprint.is_some()
                    && matches!(
                        item.front.status,
                        Status::Done | Status::Blocked | Status::Failed
                    )
                {
                    self.consume_sprint_item(&path, &item)?;
                }
                continue;
            };
            if self.issue(&key)?.is_none() {
                // An item filed outside the flow (by hand or a probe) ends where it is.
                continue;
            }
            if matches!(item.front.status, Status::Open | Status::Doing) {
                continue;
            }
            let taken = self.take_finished(&path, &item, &key);
            self.settle(&key, "taking in its finished step", taken)?;
        }
        Ok(())
    }

    fn take_finished(&mut self, path: &Path, item: &Item, key: &str) -> Result<(), DaemonError> {
        if item.front.prepare {
            return self.take_prepared(path, item, key);
        }
        let key = key.to_owned();
        let session = self.session_of(&item_name(path))?;
        let dialogue = self
            .issue(&key)?
            .is_some_and(|r| r.in_dialogue(item.front.step));
        let unit = item.front.unit;
        let fact = match item.front.status {
            Status::Done => self.done_fact(&key, item)?,
            Status::Blocked | Status::Failed => Fact::Stuck {
                step: item.front.step,
                unit,
            },
            Status::Open | Status::Doing => return Ok(()),
        };
        let archived = self.store.archive_item(path)?;
        let step = item.front.step;
        self.update_issue(&key, |r| {
            r.last_item = Some(archived.display().to_string());
            // The archive retains failed attempts. Only a usable result replaces the
            // review evidence a retry will receive.
            let failed_check = step.is_check()
                && !matches!(&fact, Fact::Done {output, ..}
                if output.check_result() != Some(ovenspring_core::output::CheckResult::Blocked));
            if item.front.status == Status::Done && !failed_check {
                let key = crate::handoff::kept_under(r.flow.corrected(), step, unit);
                let found = fs::read_to_string(&item.front.out)
                    .ok()
                    .and_then(|t| serde_json::from_str(&t).ok())
                    .and_then(|v| crate::handoff::findings_of(&v));
                if let Some(found) = found {
                    r.findings.insert(key.clone(), found);
                }
                r.outputs.insert(key, item.front.out.clone());
                if step == Step::ReviewResponse
                    && r.flow.corrected() == Some(ovenspring_core::flow::Corrected::PrComments)
                {
                    r.owe_replies();
                }
                if dialogue {
                    r.resume = Some(Resume {
                        step,
                        session: session.clone(),
                        question: None,
                    });
                }
            }
        })?;
        let fact = self.after_done(&key, fact)?;
        self.apply(&key, fact)?;
        Ok(())
    }

    /// A preparation turn ended: its message reaches the owner (as a question that waits, or as
    /// word that does not), and its session is kept for the work turn.
    fn take_prepared(&mut self, path: &Path, item: &Item, key: &str) -> Result<(), DaemonError> {
        let step = item.front.step;
        let prepared = prepared(item);
        let session = self.session_of(&item_name(path))?;
        let archived = self.store.archive_item(path)?.display().to_string();
        let Some(prepared) = prepared else {
            self.update_issue(key, |r| r.last_item = Some(archived))?;
            return self.apply(
                key,
                Fact::Stuck {
                    step,
                    unit: item.front.unit,
                },
            );
        };
        let asks = prepared.question;
        if let (Some(message), false) = (&prepared.message, asks) {
            self.tell_owner(key, message)?;
        }
        self.update_issue(key, |r| {
            r.last_item = Some(archived);
            r.resume = Some(Resume {
                step,
                session,
                question: prepared.message.filter(|_| asks),
            });
        })?;
        self.apply(key, Fact::Prepared { step, asks })
    }

    /// The session the latest launch of `item` recorded.
    fn session_of(&self, item: &str) -> Result<Option<String>, DaemonError> {
        Ok(crate::records::launch_lines(&self.store)?
            .into_iter()
            .rev()
            .find_map(|line| match line {
                crate::records::LaunchLine::End(end) if end.item == item => end.session,
                crate::records::LaunchLine::End(_) | crate::records::LaunchLine::Start(_) => None,
            }))
    }

    fn done_fact(&self, key: &str, item: &Item) -> Result<Fact, DaemonError> {
        let step = item.front.step;
        let unit = item.front.unit;
        let Some(output) = self.read_output(item)? else {
            return Ok(Fact::Stuck { step, unit });
        };
        // Only a finished judgment must name every finding: an escalation or a question is not one.
        let judged = if let StepOutput::Executor(e) = &output
            && step == Step::ReviewResponse
            && e.status == ExecutorStatus::Completed
        {
            Some(&e.dispositions)
        } else if let StepOutput::Document(d) = &output
            && d.question.is_none()
            && d.status == DocumentStatus::Completed
        {
            Some(&d.dispositions)
        } else {
            None
        };
        let record = self.record_of(key)?;
        if let Some(dispositions) =
            judged.filter(|_| step == Step::ReviewResponse || record.flow.correcting())
        {
            let undisposed = crate::handoff::undisposed(&record, dispositions);
            if !undisposed.is_empty() {
                let error = format!("no disposition for {}", undisposed.join(", "));
                self.event(&json!({"event": "unreadable-output", "step": step, "error": error}))?;
                return Ok(Fact::Stuck { step, unit });
            }
        }
        Ok(Fact::Done { step, unit, output })
    }

    /// What the daemon learns from a finished item beyond its output: an implementation's tree and
    /// summary, the task files `decompose` wrote, the tasks that own the quality run's stubs.
    fn after_done(&self, key: &str, fact: Fact) -> Result<Fact, DaemonError> {
        let Fact::Done { step, output, .. } = &fact else {
            return Ok(fact);
        };
        match (step, output) {
            (Step::Implement | Step::ReviewResponse, StepOutput::Executor(e)) => {
                let tree = self.tree_of(key);
                let task = (*step == Step::Implement)
                    .then(|| self.issue(key).ok().flatten())
                    .flatten()
                    .filter(|r| !r.flow.tasks.is_empty())
                    .map(|r| r.flow.task);
                self.update_issue(key, |r| {
                    r.summaries.extend(e.change_summary.iter().cloned());
                    // A correction's tree too, so a test review after it is given what it changed.
                    if let Some(tree) = tree {
                        if let Some(change) = r.change.as_mut() {
                            change.after = Some(tree.clone());
                        }
                        r.trees.push((task, tree));
                    }
                })?;
                Ok(fact)
            }
            (Step::Decompose, StepOutput::Document(d)) => {
                let paths: Vec<String> = d.paths.iter().chain(d.path.iter()).cloned().collect();
                self.planned(key, &paths)
            }
            (Step::Quality, StepOutput::Quality(q)) if q.status == QualityStatus::StubDetected => {
                Ok(self.stubs(key, &q.incomplete_implementations)?)
            }
            _ => Ok(fact),
        }
    }

    /// `decompose`'s task files in dependency order, with the review units of a large change.
    fn planned(&self, key: &str, paths: &[String]) -> Result<Fact, DaemonError> {
        let worktree = self.worktree_of(key);
        let read = |p: &String| -> Result<(String, plan::TaskMeta), String> {
            let path = worktree.join(p).display().to_string();
            let text = fs::read_to_string(&path).map_err(|e| format!("{p}: {e}"))?;
            Ok((
                path,
                plan::read_task(&text).map_err(|e| format!("{p}: {e}"))?,
            ))
        };
        let ordered = paths
            .iter()
            .map(read)
            .collect::<Result<Vec<_>, String>>()
            .and_then(|tasks| plan::order(&tasks));
        let (tasks, deps) = match ordered {
            Ok(o) => o,
            Err(e) => {
                self.event(&json!({"event": "task-files-unreadable", "issue": key, "error": e}))?;
                return Ok(Fact::Stuck {
                    step: Step::Decompose,
                    unit: None,
                });
            }
        };
        let large = self
            .issue(key)?
            .is_some_and(|r| r.flow.scale == Some(Scale::Large));
        let units = if large {
            plan::review_units(&deps)
        } else {
            vec![]
        };
        let tree = self.tree_of(key);
        self.update_issue(key, |r| {
            r.units.clone_from(&units);
            r.trees = tree.map(|t| vec![(None, t)]).unwrap_or_default();
            r.change = None;
        })?;
        Ok(Fact::Planned { tasks, units })
    }

    /// The tasks whose recorded changes hold the files the quality run found incomplete; the last
    /// task when no recorded change does.
    fn stubs(&self, key: &str, incomplete: &[serde_json::Value]) -> Result<Fact, DaemonError> {
        let Some(record) = self.issue(key)? else {
            return Ok(Fact::Stubs { tasks: vec![] });
        };
        if record.flow.tasks.is_empty() {
            return Ok(Fact::Stubs { tasks: vec![] });
        }
        let changed = record.task_files(&self.worktree_of(key));
        let mut tasks = BTreeSet::new();
        for i in incomplete {
            let file = i.get("file").and_then(serde_json::Value::as_str);
            let owner = file.and_then(|f| {
                changed
                    .iter()
                    .rev()
                    .find(|(_, files)| files.contains(f))
                    .map(|(t, _)| *t)
            });
            tasks.insert(owner.unwrap_or(record.flow.tasks.len() - 1));
        }
        Ok(Fact::Stubs {
            tasks: tasks.into_iter().collect(),
        })
    }

    /// The worktree's tree as it stands, without committing: every file `git add -A` would take.
    fn tree_of(&self, key: &str) -> Option<String> {
        self.tree_index(key)?.tree()
    }

    /// The issue's worktree with the temporary index its trees are written through.
    fn tree_index(&self, key: &str) -> Option<TreeIndex> {
        let index = self.store.root().join(format!("trees/{key}.index"));
        fs::create_dir_all(index.parent()?).ok()?;
        Some(TreeIndex {
            worktree: self.worktree_of(key),
            index,
        })
    }

    /// An executor's launch on the issue opens its change at the worktree as placed, unless an
    /// earlier attempt's change is still open.
    pub(crate) fn open_change(&self, key: &str) -> Result<(), DaemonError> {
        let Some(record) = self.issue(key)? else {
            return Ok(()); // An item filed outside the issue flow has no change to record.
        };
        if record.change.is_some_and(|c| c.after.is_none()) {
            return Ok(());
        }
        let Some(before) = self.tree_of(key) else {
            return Ok(()); // The test review falls back to the recorded trees.
        };
        self.update_issue(key, |r| {
            r.change = Some(Change {
                before,
                after: None,
            });
        })?;
        Ok(())
    }

    /// Runs `merge`, a merge from the branch into the issue's worktree. While a change is open,
    /// what the merge brought in is moved into the change's start, so it is not listed as the
    /// change's own.
    pub(crate) fn merged_into(
        &self,
        key: &str,
        merge: impl FnOnce() -> Result<bool, DaemonError>,
    ) -> Result<bool, DaemonError> {
        let open = self
            .issue(key)?
            .and_then(|r| r.change)
            .filter(|c| c.after.is_none());
        let Some(change) = open else {
            return merge();
        };
        let pre = self.tree_of(key);
        let merged = merge()?;
        let post = self.tree_of(key);
        let (Some(pre), Some(post)) = (pre, post) else {
            return Ok(merged);
        };
        if pre == post {
            return Ok(merged);
        }
        let moved = self
            .tree_index(key)
            .and_then(|trees| trees.moved_start(&change.before, &pre, &post));
        if let Some(before) = moved {
            self.update_issue(key, |r| {
                if let Some(c) = r.change.as_mut() {
                    c.before = before;
                }
            })?;
        }
        Ok(merged)
    }

    /// Commits the worktree at one of the flow's fixed points, with a message composed from the
    /// outputs that led to it: `Committed`, also when there is nothing to commit, or `CommitFailed`
    /// with what the repository said kept for the quality run that corrects it.
    fn commit(&self, key: &str, after: CommitPoint) -> Result<Fact, DaemonError> {
        let committed = Fact::Committed { after };
        let Some(record) = self.issue(key)? else {
            return Ok(committed);
        };
        let worktree = self.worktree_of(key);
        let staged = git(&worktree, &["add", "-A"])
            .and_then(|_| git(&worktree, &["diff", "--cached", "--name-only"]));
        let made = match staged {
            Ok(s) if s.is_empty() => {
                // The branch head, which a merge brought in before a push may have moved past
                // the factory's last commit.
                let head = git(&worktree, &["rev-parse", "HEAD"]).ok();
                self.update_issue(key, |r| r.committed(head.as_deref()))?;
                self.event(&json!({"event": "nothing-to-commit", "issue": key, "after": after.to_string()}))?;
                return Ok(committed);
            }
            Ok(_) => {
                let account = self.github()?.viewer()?;
                let message = commit_message(&record, after, &self.config.repo.base_branch);
                git_as(&worktree, &account, &["commit", "-q", "-m", &message])
                    .and_then(|_| git(&worktree, &["rev-parse", "HEAD"]))
            }
            Err(e) => Err(e),
        };
        match made {
            Ok(sha) => {
                self.update_issue(key, |r| {
                    r.summaries.clear();
                    r.last_commit = Some(sha.clone());
                    r.refusal = None;
                    r.committed(Some(&sha));
                })?;
                self.event(
                    &json!({"event": "committed", "issue": key, "after": after.to_string(), "sha": sha}),
                )?;
                Ok(committed)
            }
            Err(e) => {
                self.refused(key, "commit", &e.to_string())?;
                Ok(Fact::CommitFailed { after })
            }
        }
    }

    /// Keeps what the repository said when it refused the factory's `what` (commit or push).
    pub(crate) fn refused(&self, key: &str, what: &str, error: &str) -> Result<(), DaemonError> {
        self.event(&json!({"event": format!("{what}-refused"), "issue": key, "error": error}))?;
        self.update_issue(key, |r| {
            r.refusal = Some(format!(
                "The repository refused the factory's {what}:\n{error}"
            ));
        })?;
        Ok(())
    }

    /// A finished item's output parsed for its step; `None` once the reason it could not be read
    /// is logged as an `unreadable-output` event.
    pub(crate) fn read_output(&self, item: &Item) -> Result<Option<StepOutput>, DaemonError> {
        let step = item.front.step;
        let parsed = fs::read_to_string(&item.front.out)
            .map_err(|e| e.to_string())
            .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))
            .and_then(|raw: serde_json::Value| {
                let output = StepOutput::parse(step, raw.clone())?;
                if step.is_check()
                    && output.check_result() != Some(ovenspring_core::output::CheckResult::Blocked)
                {
                    self.check_reconciliation(item, &raw)?;
                }
                Ok(output)
            });
        match parsed {
            Ok(output) => Ok(Some(output)),
            Err(e) => {
                self.event(&json!({"event": "unreadable-output", "step": step, "error": e}))?;
                Ok(None)
            }
        }
    }

    /// A scoped recheck answers exactly its filed questions. Failure to answer preserves
    /// the previous judgment for recovery instead of accepting a new review as its answer.
    fn check_reconciliation(&self, item: &Item, raw: &serde_json::Value) -> Result<(), String> {
        if !item
            .body
            .lines()
            .take_while(|line| *line != "## Result")
            .any(|line| line.starts_with("prior_feedback: "))
        {
            return Ok(());
        }
        let name = item_name(Path::new(&item.front.out));
        let file = self
            .store
            .source(&format!("handoffs/{name}-prior_feedback.json"));
        let expected: serde_json::Value =
            serde_json::from_slice(&fs::read(file).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        // Cross-document sync and legacy verification carry their own conflict report schema.
        let Some(entries) = expected.as_array() else {
            return Ok(());
        };
        let mut expected: Vec<_> = entries
            .iter()
            .map(|e| (&e["id"], &e["disposition"]))
            .collect();
        let answers = raw["prior_feedback_reconciliation"]
            .as_array()
            .ok_or("missing reconciliation for prior feedback")?;
        for answer in answers {
            let pair = (&answer["id"], &answer["prior_disposition"]);
            let index = expected
                .iter()
                .position(|entry| *entry == pair)
                .ok_or("unexpected or repeated reconciliation entry")?;
            expected.swap_remove(index);
            if answer["status"] == "maintained"
                && answer["evidence"]
                    .as_str()
                    .is_none_or(|e| e.trim().is_empty())
            {
                return Err("maintained finding has no remaining condition".into());
            }
        }
        if !expected.is_empty()
            || crate::handoff::FINDING_KEYS
                .iter()
                .any(|k| raw[*k].as_array().is_some_and(|a| !a.is_empty()))
        {
            return Err("a recheck must answer only and all of its filed findings".into());
        }
        Ok(())
    }

    /// Repairs any issue missing an item its state calls for (after a restart, a failed write, or
    /// an action that could not complete), and retries pushes that did not happen.
    pub(crate) fn reconcile(&mut self) -> Result<(), DaemonError> {
        let mut present: BTreeSet<(String, Step, Option<usize>)> = BTreeSet::new();
        for path in self.store.active_items()? {
            let front = self.store.read_item(&path)?.front;
            if let Some(issue) = front.issue {
                present.insert((issue, front.step, front.unit));
            }
        }
        for record in self.issues()? {
            let key = record.key().to_owned();
            let repaired = self.reconcile_issue(&record, &present);
            self.settle(&key, "filing its step, committing or pushing", repaired)?;
        }
        Ok(())
    }

    fn reconcile_issue(
        &mut self,
        record: &IssueRecord,
        present: &BTreeSet<(String, Step, Option<usize>)>,
    ) -> Result<(), DaemonError> {
        let key = record.key().to_owned();
        for filing in flow::calls_for(&record.flow) {
            if present.contains(&(key.clone(), filing.step, filing.unit)) {
                continue;
            }
            self.file_step(&key, &filing)?;
            self.event(&json!({"event": "reconciled", "issue": key, "step": filing.step}))?;
        }
        if record.flow.stage == Stage::Pushing
            && let Some(fact) = self.push(&key)?
        {
            self.apply(&key, fact)?;
        }
        // A commit the daemon had not made when it stopped is made now.
        if let Stage::Committing { after } = &record.flow.stage {
            let fact = self.commit(&key, *after)?;
            self.apply(&key, fact)?;
        }
        Ok(())
    }

    pub(crate) fn stop_app(&self, key: &str) {
        let worktree = self.store.worktree(key);
        let app = crate::app::App {
            state: self.store.root(),
            issue: key,
            worktree: &worktree,
            config: &self.config.app,
        };
        if let Err(e) = app.stop() {
            // Stopping is best effort; the record of a failure is what the owner can act on.
            let _ = self
                .event(&json!({"event": "app-stop-error", "issue": key, "error": e.to_string()}));
        }
    }

    pub(crate) fn worktree_of(&self, key: &str) -> PathBuf {
        self.store.worktree(key)
    }

    /// The open issues as the tracker last reported them.
    pub fn open_issues(&self) -> &[ovenspring_integrations::tracker::TrackerIssue] {
        &self.open_issues
    }

    pub(crate) fn write_source(&self, rel: &str, text: &str) -> Result<String, DaemonError> {
        let path: &Path = &self.store.source(rel);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(StoreError::io(dir))?;
        }
        fs::write(path, text).map_err(StoreError::io(path))?;
        Ok(path.display().to_string())
    }
}

/// What a finished preparation item came to: its output, when its agent set it done with one
/// that parses; `None` sends the step down its failure path. The daemon acts on this alone, and
/// history reads a preparation run by it, so the two cannot disagree.
pub(crate) fn prepared(item: &Item) -> Option<PrepareOutput> {
    if item.front.status != Status::Done {
        return None;
    }
    let text = fs::read_to_string(&item.front.out).ok()?;
    PrepareOutput::parse(serde_json::from_str(&text).ok()?).ok()
}

/// A commit message for one of the flow's fixed points: what passed, and for an implementation
/// the summaries of its changes and the verification limits it carries.
fn commit_message(record: &IssueRecord, after: CommitPoint, base: &str) -> String {
    let key = record.key();
    let subject = match after {
        CommitPoint::PrdReview => format!("{key}: add the PRD"),
        CommitPoint::UiSpecReview => format!("{key}: add the UI Spec"),
        CommitPoint::AdrReview => format!("{key}: record the technical decisions"),
        CommitPoint::DesignSync => format!("{key}: add the design"),
        CommitPoint::FixCi => format!("{key}: make the checks pass"),
        CommitPoint::MergeBase => format!("{key}: resolve the conflict with origin/{base}"),
        CommitPoint::Quality | CommitPoint::Push => format!("{key}: {}", record.title),
    };
    let mut message = subject;
    if !record.summaries.is_empty() {
        message.push_str("\n\n");
        message.push_str(
            &record
                .summaries
                .iter()
                .map(|s| format!("- {s}"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
    let step = match after {
        CommitPoint::FixCi => Step::FixCi,
        CommitPoint::MergeBase => Step::MergeBase,
        CommitPoint::PrdReview
        | CommitPoint::UiSpecReview
        | CommitPoint::AdrReview
        | CommitPoint::DesignSync
        | CommitPoint::Quality
        | CommitPoint::Push => Step::Quality,
    };
    if let Some(StepOutput::Quality(q)) = record.output(step)
        && q.status == QualityStatus::VerificationIncomplete
    {
        let mut trailers = String::new();
        for (reason, affected) in q.limitations() {
            let _ = write!(
                trailers,
                "\nVerification-Limitation: {reason}\nVerification-Affected: {affected}"
            );
        }
        if !trailers.is_empty() {
            message.push('\n');
            message.push_str(&trailers);
        }
    }
    message
}

/// What the lead's `exception` is asked: the failure it takes, in the words that name it.
fn exception_request(record: &IssueRecord) -> String {
    let key = record.key();
    let failed_recovery = if record.recovery_log.is_some() {
        " The recovery procedure the owner approved did not succeed; what it printed is in your refs."
    } else {
        ""
    };
    let cause = if let Stage::Exception { cause, .. } = &record.flow.stage {
        Some(cause)
    } else {
        None
    };
    if cause == Some(&Cause::PullClosed) {
        return format!(
            "{key}'s pull request {} was closed without being merged, by someone outside the factory. Choose the way forward.",
            record.pr_url.as_deref().unwrap_or_default()
        );
    }
    if let Some(stop) = &record.stopped {
        return format!(
            "{key} stopped: the factory's own operation, {}, failed on two passes in a row with: {}.{failed_recovery} Find the cause and choose the way forward.",
            stop.operation, stop.error
        );
    }
    let what = match cause {
        Some(Cause::DependencyLost) => {
            format!("{key} cannot start as planned: an issue it depends on was held or dropped")
        }
        Some(Cause::Push) => format!(
            "the repository refused the push of {key}'s branch, and the quality runs could not correct what it reported"
        ),
        Some(cause @ (Cause::Step { .. } | Cause::Operation { .. } | Cause::PullClosed)) => {
            format!("{cause} of {key} could not go on")
        }
        None => format!("a step of {key} could not go on"),
    };
    format!("Find why {what}, and choose the way forward.{failed_recovery}")
}

/// An issue's worktree written through a temporary index, so trees are recorded without touching
/// the worktree's own index.
struct TreeIndex {
    worktree: PathBuf,
    index: PathBuf,
}

impl TreeIndex {
    /// Runs `git` against the temporary index, with `input` on its standard input: its trimmed
    /// output when it succeeds.
    fn git(&self, args: &[&str], input: &str) -> Option<String> {
        use std::io::Write as _;
        use std::process::{Command, Stdio};

        let mut child = Command::new("git")
            .args(args)
            .current_dir(&self.worktree)
            .env("GIT_INDEX_FILE", &self.index)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let mut stdin = child.stdin.take()?;
        stdin.write_all(input.as_bytes()).ok()?;
        drop(stdin);
        let out = child.wait_with_output().ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
    }

    /// Runs `write` on a fresh temporary index, which is removed afterwards.
    fn written(&self, write: impl FnOnce() -> Option<String>) -> Option<String> {
        let _ = fs::remove_file(&self.index);
        let tree = write();
        let _ = fs::remove_file(&self.index);
        tree
    }

    /// The worktree's tree: every file `git add -A` would take.
    fn tree(&self) -> Option<String> {
        self.written(|| {
            self.git(&["add", "-A"], "")?;
            self.git(&["write-tree"], "")
        })
    }

    /// The start of a change that a merge moved from `pre` to `post` while it was open: `post`,
    /// with each path the change had already changed (between `before` and `pre`) set back to its
    /// version at `before`. What the merge brought in is then part of the start, and the change's
    /// own edits are not. `None` when Git fails.
    fn moved_start(&self, before: &str, pre: &str, post: &str) -> Option<String> {
        self.written(|| {
            self.git(&["read-tree", post], "")?;
            let changed = self.git(&["diff-tree", "-r", "--no-renames", "-z", before, pre], "")?;
            // Each entry is `:<old mode> <new mode> <old blob> <new blob> <status>` and its path;
            // the old mode is 000000 for a path the change added, which removes it.
            let fields: Vec<&str> = changed.split('\0').filter(|f| !f.is_empty()).collect();
            let mut info = String::new();
            for entry in fields.chunks(2) {
                let [meta, path] = entry else {
                    return None;
                };
                let mut meta = meta.trim_start_matches(':').split(' ');
                let (mode, _, blob) = (meta.next()?, meta.next()?, meta.next()?);
                let _ = write!(info, "{mode} {blob}\t{path}\0");
            }
            self.git(&["update-index", "-z", "--index-info"], &info)?;
            self.git(&["write-tree"], "")
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::TreeIndex;

    // Behavior: a change open across a merge (a blocked attempt, then its retry) -> its moved
    //     start lists both attempts' edits, additions and deletions, and not the merged path.
    #[test]
    fn moved_start_keeps_both_attempts_and_leaves_out_what_the_merge_brought_in() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        crate::worktree::git(dir.path(), &["init", "-q", repo.to_str().unwrap()]).unwrap();
        let trees = TreeIndex {
            worktree: repo.clone(),
            index: dir.path().join("trees.index"),
        };
        for file in ["edited", "removed", "keep"] {
            fs::write(repo.join(file), "start\n").unwrap();
        }
        let before = trees.tree().unwrap();

        fs::write(repo.join("edited"), "first attempt\n").unwrap();
        fs::write(repo.join("added"), "first attempt\n").unwrap();
        fs::remove_file(repo.join("removed")).unwrap();
        let pre = trees.tree().unwrap();
        fs::write(repo.join("pushed"), "merged from the branch\n").unwrap();
        let post = trees.tree().unwrap();
        fs::write(repo.join("keep"), "second attempt\n").unwrap();
        let after = trees.tree().unwrap();

        let start = trees.moved_start(&before, &pre, &post).unwrap();
        let listed =
            crate::worktree::git(&repo, &["diff-tree", "-r", "--name-only", &start, &after])
                .unwrap();
        assert_eq!(
            listed.lines().collect::<Vec<_>>(),
            ["added", "edited", "keep", "removed"]
        );
        assert!(!trees.index.exists());
    }
}
