//! The goal and the sprint around the issues: proposing a sprint toward the goal, putting its
//! issues on the board as slots free up, reviewing it, and applying the working agreements the
//! owner approved before the next sprint starts.

use std::collections::BTreeSet;
use std::fs;

use ovenspring_core::config::MergeBy;
use ovenspring_core::flow::{Fact, Stage};
use ovenspring_core::gate::{Return, Wait};
use ovenspring_core::item::{Front, Item, Status};
use ovenspring_core::output::{Approval, StepOutput};
use ovenspring_core::sprint::{
    self, SprintAction, SprintFact, SprintStage, SprintState, SprintStep,
};
use ovenspring_core::step::Step;
use ovenspring_integrations::channel::DecisionKind;
use ovenspring_integrations::github::{NewPull, PullState, ReviewDecision};
use ovenspring_integrations::tracker::TrackerIssue;
use ovenspring_store::{Store, StoreError, item_name};
use serde_json::json;

use crate::issues::IssueRecord;
use crate::owner::{DecisionRecord, DecisionStatus, Proposal};
use crate::pulls::{Gated, OwnerApprovals, Subject};
use crate::worktree::{git, git_as, git_remote};
use crate::{Daemon, DaemonError, records};

impl Daemon {
    /// The product goal, trimmed; empty until the owner sets one.
    pub fn goal(&self) -> Result<String, DaemonError> {
        records::goal(&self.store)
    }

    pub fn set_goal(&self, text: &str) -> Result<(), DaemonError> {
        records::set_goal(&self.store, text)
    }

    pub fn sprint(&self) -> Result<Option<SprintState>, DaemonError> {
        records::sprint(&self.store)
    }

    /// Saves the sprint; one that has just started running gets its start time.
    fn save_sprint(&self, s: &SprintState) -> Result<(), DaemonError> {
        let mut s = s.clone();
        if s.stage == SprintStage::Running && s.started_at.is_none() {
            s.started_at = Some(jiff::Timestamp::now().to_string());
        }
        self.store.write_record(records::SPRINT, &s)?;
        Ok(())
    }

    /// The running sprint's purpose, for every launch's prompt.
    pub(crate) fn sprint_purpose(&self) -> Option<String> {
        let s = self.sprint().ok()??;
        s.plan
            .map(|p| p.purpose)
            .filter(|_| s.stage != SprintStage::Proposing)
    }

    pub(crate) fn maybe_begin_sprint(&mut self) -> Result<(), DaemonError> {
        // No work starts that needs a missing connection: every issue ends in a push.
        if self.sprint()?.is_some()
            || self.goal()?.is_empty()
            || !self.config.app.settled()
            || self.services.github.is_none()
        {
            return Ok(());
        }
        let id = jiff::ZonedDateTime::now()
            .strftime("%Y%m%d-%H%M%S")
            .to_string();
        let (state, actions) = sprint::begin(&id);
        self.save_sprint(&state)?;
        self.event(&json!({"event": "sprint-begun", "sprint": id}))?;
        self.sprint_actions(&actions)
    }

    pub(crate) fn sprint_fact(&mut self, fact: &SprintFact) -> Result<(), DaemonError> {
        let Some(state) = self.sprint()? else {
            return Ok(());
        };
        let (next, actions) = sprint::next(&state, fact);
        let from = state.stage.clone();
        self.save_sprint(&next)?;
        self.event(
            &json!({"event": "sprint-fact", "sprint": next.id, "fact": crate::records::fact_name(fact), "from": from, "to": next.stage}),
        )?;
        let leaves_proposing =
            from == SprintStage::Proposing && next.stage != SprintStage::Proposing;
        self.sprint_actions(&actions)?;
        if leaves_proposing {
            self.intake_pass()?;
        }
        Ok(())
    }

    fn sprint_actions(&mut self, actions: &[SprintAction]) -> Result<(), DaemonError> {
        for action in actions {
            match action {
                SprintAction::File(step) => self.file_sprint_step(*step)?,
                SprintAction::PostProposal => self.post_sprint_decision(DecisionKind::Sprint)?,
                SprintAction::PostWaiting => self.post_waiting()?,
                SprintAction::PostReview => self.post_sprint_decision(DecisionKind::Review)?,
                SprintAction::StartIssues => self.start_sprint_issues()?,
                SprintAction::OpenAgreementsPr => {
                    // A failure here is retried by `poll_agreements`; the sprint stays applying.
                    if let Err(e) = self.open_agreements_pr() {
                        self.event(&json!({"event": "agreements-error", "error": e.to_string()}))?;
                    }
                }
                SprintAction::EndSprint => self.end_sprint()?,
                SprintAction::AskQuestion(question) => self.ask_about_the_sprint(question)?,
                SprintAction::RequestRecovery => self.request_recovery_for(None)?,
                SprintAction::ReportStopped => self.report_stopped_for(None)?,
            }
        }
        Ok(())
    }

    fn file_sprint_step(&self, filed: SprintStep) -> Result<(), DaemonError> {
        let Some(state) = self.sprint()? else {
            return Ok(());
        };
        let step = filed.step();
        let name = self.new_item_name(&format!("{step}-sprint-{}", state.id));
        let root = self.store.root();
        let mut refs = vec![self.store.source("issues").display().to_string()];
        if step == Step::SprintPlan {
            // Issues the factory already holds or dropped are on record here.
            refs.push(root.join("issues").display().to_string());
            refs.extend(self.last_review_and_its_replies()?);
        }
        if matches!(step, Step::SprintReview | Step::AgreementsCheck) {
            // Earlier agreement checks of this sprint: what the review must answer and the check
            // must follow up.
            refs.extend(self.sprint_outputs(&state.id, Step::AgreementsCheck).pop());
        }
        // The owner's answers to this sprint's questions, and what its failed recovery procedures
        // printed.
        let ours: Vec<String> = self
            .decisions()?
            .into_iter()
            .filter(|d| d.sprint.as_deref() == Some(state.id.as_str()))
            .map(|d| d.id)
            .collect();
        let sources = |dir: &str, extension: &str| -> Vec<String> {
            ours.iter()
                .map(|id| self.store.source(&format!("{dir}/{id}.{extension}")))
                .filter(|p| p.exists())
                .map(|p| p.display().to_string())
                .collect()
        };
        refs.extend(sources("answers", "md"));
        refs.push(root.join(records::SPRINT).display().to_string());
        if step == Step::Facts {
            // The owner's settings explain choices made outside the sprint, such as its base
            // branch; the base's worktree holds the file only when the owner committed it.
            let settings = self.paths.repo.join(ovenspring_core::config::CONFIG_FILE);
            let text = fs::read_to_string(&settings).map_err(StoreError::io(&settings))?;
            refs.push(self.write_source("config.toml", &text)?);
        }
        let facts = root.join("sprints").join(format!("{}-facts.md", state.id));
        if step != Step::Facts && facts.exists() {
            refs.push(facts.display().to_string());
        }
        if step == Step::Exception {
            // What a failed recovery procedure printed, and the lead's earlier ways forward.
            refs.extend(sources("recoveries", "log"));
            refs.extend(self.sprint_outputs(&state.id, Step::Exception));
        }
        let request = match filed {
            SprintStep::SprintPlan => {
                "Propose the next sprint toward the product goal from the open issues.".to_owned()
            }
            SprintStep::Facts => format!(
                "Gather the facts of sprint {} into `{}`.",
                state.id,
                facts.display()
            ),
            SprintStep::SprintReview => {
                format!("Review sprint {} against its key results.", state.id)
            }
            SprintStep::AgreementsCheck => format!(
                "Check the working agreements sprint {}'s review proposes.",
                state.id
            ),
            SprintStep::Exception => format!(
                "Sprint {} stopped: {} could not go on: {}. Find the cause and choose the way forward: retry, recover or ask (replan, redesign and drop are for issues).",
                state.id,
                state.failed.as_deref().unwrap_or("an operation"),
                state.failure.as_deref().unwrap_or("no message")
            ),
        };
        let item = Item {
            front: Front {
                step,
                to: step.role(),
                status: Status::Open,
                issue: None,
                sprint: Some(state.id.clone()),
                round: None,
                unit: None,
                refs,
                out: self.store.output_path(&name).display().to_string(),
                prepare: false,
                resume: None,
            },
            body: format!("{request}\n\n## Result\n"),
        };
        self.store.file_item(&name, &item)?;
        self.event(&json!({"event": "filed", "sprint": state.id, "item": name, "step": step}))
    }

    /// Files the item the sprint's stage calls for when none is open or doing: after a restart, or
    /// when the sprint returns to a stage whose step could not finish.
    pub(crate) fn reconcile_sprint(&mut self) -> Result<(), DaemonError> {
        let Some(state) = self.sprint()? else {
            return Ok(());
        };
        if state.stage == SprintStage::Running {
            self.start_sprint_issues()?;
        }
        let Some(called) = state.stage.calls_for() else {
            return Ok(());
        };
        for path in self.store.active_items()? {
            let front = self.store.read_item(&path)?.front;
            if front.sprint.as_deref() == Some(state.id.as_str()) && front.step == called.step() {
                return Ok(());
            }
        }
        self.file_sprint_step(called)?;
        self.event(&json!({"event": "reconciled", "sprint": state.id, "step": called.step()}))
    }

    /// The outputs of the sprint's finished `exception` items, oldest first.
    pub(crate) fn sprint_exception_outputs(&self, sprint: &str) -> Vec<String> {
        self.sprint_outputs(sprint, Step::Exception)
    }

    fn ask_about_the_sprint(&mut self, question: &str) -> Result<(), DaemonError> {
        let Some(state) = self.sprint()? else {
            return Ok(());
        };
        self.decide(DecisionRecord {
            text: question.to_owned(),
            ..self.new_decision(DecisionKind::Spec, None, format!("Sprint {}", state.id))?
        })
    }

    /// What stopped the sprint was cleared: the lead's open work on it ends, open questions about
    /// it are superseded, and it returns to the stage it stopped in.
    pub(crate) fn resume_sprint(&mut self) -> Result<(), DaemonError> {
        let Some(state) = self.sprint()? else {
            return Ok(());
        };
        if state.resume.is_none() {
            return Ok(());
        }
        for path in self.store.active_items()? {
            let item = self.store.read_item(&path)?;
            if item.front.sprint.as_deref() == Some(state.id.as_str())
                && item.front.step == Step::Exception
            {
                self.stop_launch(&item_name(&path))?;
                self.store.archive_item(&path)?;
            }
        }
        for mut d in self.decisions()? {
            let open = matches!(d.status, DecisionStatus::Pending | DecisionStatus::Unsent);
            let about_the_stop = matches!(
                d.kind,
                DecisionKind::Spec | DecisionKind::Recovery | DecisionKind::Stopped
            );
            if open && about_the_stop && d.sprint.as_deref() == Some(state.id.as_str()) {
                self.supersede(&mut d)?;
            }
        }
        self.event(&json!({"event": "resumed", "sprint": state.id}))?;
        self.sprint_fact(&SprintFact::Resume)
    }

    /// The outputs of this sprint's finished `step` items, oldest first.
    fn sprint_outputs(&self, sprint: &str, step: Step) -> Vec<String> {
        self.store
            .archived_items()
            .unwrap_or_default()
            .iter()
            .filter_map(|p| self.store.read_item(p).ok())
            .filter(|i| i.front.step == step && i.front.sprint.as_deref() == Some(sprint))
            .map(|i| i.front.out)
            .filter(|out| std::path::Path::new(out).exists())
            .collect()
    }

    pub(crate) fn consume_sprint_item(
        &mut self,
        path: &std::path::Path,
        item: &Item,
    ) -> Result<(), DaemonError> {
        let step = item.front.step;
        let fact = if item.front.status == Status::Done {
            match self.read_output(item)? {
                Some(StepOutput::SprintPlan(p)) => SprintFact::PlanDone(p),
                Some(StepOutput::Facts(_)) => SprintFact::FactsDone,
                Some(StepOutput::SprintReview(r)) => SprintFact::ReviewDone(r),
                Some(StepOutput::AgreementsCheck(c)) => SprintFact::CheckDone {
                    findings: !c.findings.is_empty(),
                },
                Some(StepOutput::Exception(e)) => SprintFact::ExceptionDone(e),
                _ => SprintFact::Stuck { step },
            }
        } else {
            SprintFact::Stuck { step }
        };
        self.store.archive_item(path)?;
        self.event(&json!({"event": "sprint-item-ended", "item": item_name(path)}))?;
        self.sprint_fact(&fact)
    }

    /// Tells the owner that no open issue serves the goal, in the lead's words, so the wait is not
    /// silent.
    fn post_waiting(&self) -> Result<(), DaemonError> {
        let Some(plan) = self.sprint()?.and_then(|s| s.plan) else {
            return Ok(());
        };
        let notice = ovenspring_integrations::channel::Notice {
            kind: ovenspring_integrations::channel::NoticeKind::WaitingForIssues,
            issue: None,
            text: format!("Goal: {}\n\n{}", self.goal()?, plan.message),
            preview: false,
            marker: None,
        };
        match self.services.channel.post_notice(&notice) {
            Ok(message) => crate::records::set_told_waiting(
                &self.store,
                Some(&crate::records::Told {
                    at: jiff::Timestamp::now().to_string(),
                    link: message.permalink,
                }),
            ),
            Err(e) => {
                crate::records::set_told_waiting(&self.store, None)?;
                self.event(&json!({"event": "channel-error", "error": e.0}))
            }
        }
    }

    fn post_sprint_decision(&mut self, kind: DecisionKind) -> Result<(), DaemonError> {
        let Some(state) = self.sprint()? else {
            return Ok(());
        };
        let text = match kind {
            DecisionKind::Sprint => state.plan.as_ref().map(|p| p.message.clone()),
            DecisionKind::Review => state.review.as_ref().map(|r| r.message.clone()),
            DecisionKind::Spec
            | DecisionKind::Design
            | DecisionKind::Prototype
            | DecisionKind::Prd
            | DecisionKind::Merge
            | DecisionKind::Recovery
            | DecisionKind::Stopped => None,
        }
        .unwrap_or_else(|| {
            format!(
                "A step of sprint {} could not finish. Reply in this thread to run it again.",
                state.id
            )
        });
        // Every proposal and review opens with the goal it serves.
        let text = format!("Goal: {}\n\n{text}", self.goal()?);
        let title = match kind {
            DecisionKind::Review => "Sprint review".to_owned(),
            DecisionKind::Sprint
            | DecisionKind::Spec
            | DecisionKind::Design
            | DecisionKind::Prototype
            | DecisionKind::Prd
            | DecisionKind::Merge
            | DecisionKind::Recovery
            | DecisionKind::Stopped => "Sprint proposal".to_owned(),
        };
        // What a proposal proposes, fixed with the decision so a later delivery tells the same.
        let proposal = match (kind, &state.plan) {
            (DecisionKind::Sprint, Some(plan)) => {
                let secrets: Vec<&str> = self.services.secrets.iter().map(String::as_str).collect();
                Some(Proposal {
                    issues: plan.issues.len(),
                    purpose: crate::download::mask(&plan.purpose, &secrets),
                })
            }
            _ => None,
        };
        self.decide(DecisionRecord {
            text,
            proposal,
            ..self.new_decision(kind, None, title)?
        })
    }

    /// Puts the plan's issues not yet in the sprint on the board; `schedule` starts them.
    fn start_sprint_issues(&mut self) -> Result<(), DaemonError> {
        let Some(state) = self.sprint()? else {
            return Ok(());
        };
        let Some(plan) = state.plan.clone() else {
            return Ok(());
        };
        let keys: Vec<String> = plan.issues.iter().map(|i| i.key.clone()).collect();
        for planned in &plan.issues {
            if self
                .issue(&planned.key)?
                .is_some_and(|r| r.sprint.as_deref() == Some(state.id.as_str()))
            {
                continue;
            }
            let snapshot = self.store.source(&format!("issues/{}.md", planned.key));
            let selected_heading = fs::read_to_string(snapshot)
                .ok()
                .and_then(|snapshot| TrackerIssue::heading(&planned.key, &snapshot));
            let (title, url) = selected_heading.map_or_else(
                || (planned.key.clone(), None),
                |(title, url)| (title, Some(url)),
            );
            // Only issues in this sprint can hold it up; the lead's plan already ordered the rest.
            let deps = planned
                .depends_on
                .iter()
                .filter(|d| keys.contains(d))
                .cloned()
                .collect();
            self.begin_issue(&planned.key, &title, deps)?;
            let id = state.id.clone();
            let record = self.update_issue(&planned.key, |r| {
                r.sprint = Some(id);
                r.url = url;
            })?;
            if record.flow.stage == Stage::Held {
                self.apply(&planned.key, Fact::TakenUp)?;
            }
        }
        Ok(())
    }

    /// Starts queued issues whose dependencies are merged, while fewer than
    /// `limits.issues_at_once` are under way.
    pub(crate) fn schedule(&mut self) -> Result<(), DaemonError> {
        let records = self.issues()?;
        let limit = usize::try_from(self.config.limits.issues_at_once).unwrap_or(usize::MAX);
        let mut active = under_way(&records).count();
        // Every issue ends in a push: none starts while GitHub is not connected.
        let startable = self.services.github.is_some();
        for r in records
            .iter()
            .filter(|r| startable && r.flow.stage == Stage::Queued)
        {
            if active >= limit {
                break;
            }
            if unmerged_dependencies(r, &records).is_empty() {
                let started = self.apply(r.key(), Fact::DependenciesMerged);
                self.settle(r.key(), "starting it", started)?;
                active += 1;
            }
        }
        // Every issue of the running sprint settled: review it.
        if let Some(s) = self.sprint()?
            && s.stage == SprintStage::Running
            && plan_settled(&s, &self.issues()?)
        {
            self.sprint_fact(&SprintFact::IssuesSettled)?;
        }
        Ok(())
    }

    /// Writes the review's agreements and knowledge on a branch from the base and opens their pull
    /// request.
    fn open_agreements_pr(&mut self) -> Result<(), DaemonError> {
        let Some(state) = self.sprint()? else {
            return Ok(());
        };
        let Some(review) = &state.review else {
            return Ok(());
        };
        let key = format!("agreements-{}", state.id);
        let base = self.config.repo.base_branch.clone();
        let worktree = crate::worktree::ensure(&self.paths.repo, &self.store, &key, &base)?;
        let agreements = worktree.join(&self.config.agreements.path);
        for a in &review.agreements {
            write_whole(&agreements.join(a.file.as_str()), &a.content)?;
        }
        let knowledge = worktree.join(&self.config.knowledge.path);
        for k in &review.knowledge {
            write_whole(&knowledge.join(&k.file), &k.content)?;
        }
        // A folder the review never wrote to is not there for git to add.
        let written: Vec<&str> = [
            self.config.agreements.path.as_str(),
            self.config.knowledge.path.as_str(),
        ]
        .into_iter()
        .filter(|dir| worktree.join(dir).exists())
        .collect();
        if !written.is_empty() {
            let mut add = vec!["add", "-A", "--"];
            add.extend(written);
            git(&worktree, &add)?;
        }
        // A retry after a failure further on finds the change already committed.
        let staged = git(&worktree, &["diff", "--cached", "--quiet"]).is_err();
        if staged {
            let account = self.github()?.viewer()?;
            git_as(
                &worktree,
                &account,
                &[
                    "commit",
                    "-q",
                    "-m",
                    &format!(
                        "Update working agreements and knowledge after sprint {}",
                        state.id
                    ),
                ],
            )?;
        }
        let branch = crate::worktree::branch_for(&key);
        if !self.push_agreements(&worktree, &branch)? {
            return Ok(());
        }
        let gh = self
            .services
            .github
            .clone()
            .ok_or_else(|| DaemonError::Launch("no GitHub connection is configured".into()))?;
        let pull = match gh.pull_for_branch(&branch)? {
            Some(p) if p.state == PullState::Open => p,
            _ => gh.open_pull(&NewPull {
                head: &branch,
                base: &base,
                title: &format!("Working agreements and knowledge after sprint {}", state.id),
                body: "What the sprint review changed in how the factory works and what it knows.\n\nOpened by Oven Spring.",
            })?,
        };
        let needs_approval = match gh.standing(pull.number) {
            Ok(standing) => Some(standing.review == ReviewDecision::Required),
            Err(e) => {
                self.event(&json!({"event": "github-error", "error": e.to_string()}))?;
                None
            }
        };
        let mut next = state.clone();
        next.agreements_pr = Some(pull.number);
        next.agreements_url = Some(pull.url.clone());
        next.agreements_need_approval = needs_approval == Some(true);
        next.agreements_pushed_at = Some(jiff::Timestamp::now().to_string());
        self.save_sprint(&next)?;
        self.tell_agreements_pr(&state.id, &pull.url, needs_approval)?;
        self.event(&json!({"event": "agreements-pr", "sprint": state.id, "pr": pull.number}))
    }

    /// Tells the owner, in the sprint review's thread, what the agreements pull request waits on
    /// from them. `needs_approval` is `None` when GitHub could not say.
    fn tell_agreements_pr(
        &mut self,
        sprint: &str,
        url: &str,
        needs_approval: Option<bool>,
    ) -> Result<(), DaemonError> {
        let thread = self
            .decisions()?
            .into_iter()
            .rev()
            .find(|d| d.kind == DecisionKind::Review && d.sprint.as_deref() == Some(sprint))
            .and_then(|d| d.message);
        let Some(m) = thread else {
            return Ok(());
        };
        // The owner's part comes first, or the pull request waits on them unnoticed. The address
        // ends the reply: Slack links punctuation that follows it.
        let note = match (self.config.merge.by, needs_approval) {
            (MergeBy::Factory, Some(true)) => {
                "To finish the sprint, approve the working agreements and knowledge on GitHub. This repository requires an approving review, so they stay unmerged until you approve. The factory then merges them and proposes the next sprint."
            }
            (MergeBy::Factory, Some(false)) => {
                "The factory merges the working agreements and knowledge once their checks pass, then proposes the next sprint. Nothing is needed from you."
            }
            (MergeBy::Factory, None) => {
                "The factory merges the working agreements and knowledge once GitHub allows it, then proposes the next sprint. If GitHub asks for an approving review, approve them there."
            }
            (MergeBy::Owner, Some(true)) => {
                "To finish the sprint, approve and merge the working agreements and knowledge on GitHub. This repository requires an approving review before they can merge. The next sprint is proposed once they are merged."
            }
            (MergeBy::Owner, Some(false) | None) => {
                "To finish the sprint, merge the working agreements and knowledge on GitHub. The next sprint is proposed once they are merged."
            }
        };
        let note = format!("{note}\n{url}");
        let at = ovenspring_integrations::channel::MessageRef {
            channel: m.channel,
            ts: m.ts,
            permalink: m.permalink,
        };
        if let Err(e) = self.services.channel.reply(&at, &note) {
            self.event(&json!({"event": "channel-error", "error": e.0}))?;
        }
        Ok(())
    }

    /// Puts the agreements pull request through the merge gate (`merge.by = factory`), or waits
    /// for the owner to merge it (`owner`); either way the next sprint starts once it is merged.
    /// The owner approved its changes with the sprint review, before the pull request existed. It
    /// is read as an issue's is: merged by anyone ends the wait, and what keeps it from merging that
    /// no agent can fix in an agreements file (failed checks, a close, a conflict) goes to the lead.
    pub(crate) fn poll_agreements(&mut self) -> Result<(), DaemonError> {
        let Some(state) = self.sprint()? else {
            return Ok(());
        };
        let stopped_applying = state.resume.as_deref() == Some(&SprintStage::ApplyingAgreements);
        if state.stage != SprintStage::ApplyingAgreements && !stopped_applying {
            return Ok(());
        }
        let read = self.poll_agreements_pr(&state, stopped_applying);
        self.settle_sprint("applying the working agreements", read)
    }

    fn poll_agreements_pr(
        &mut self,
        state: &SprintState,
        stopped: bool,
    ) -> Result<(), DaemonError> {
        let Some(pr) = state.agreements_pr else {
            // Opening the pull request failed earlier: try again.
            return if stopped {
                Ok(())
            } else {
                self.open_agreements_pr()
            };
        };
        let Some(gh) = self.services.github.clone() else {
            return Ok(());
        };
        let branch = crate::worktree::branch_for(&format!("agreements-{}", state.id));
        let pull = gh.pull(pr)?;
        match pull.state {
            PullState::Merged => {
                if stopped {
                    self.resume_sprint()?;
                }
                self.delete_branch(("sprint", &state.id), &branch)?;
                return self.sprint_fact(&SprintFact::AgreementsMerged);
            }
            PullState::Closed if !stopped => {
                return self.sprint_fact(&SprintFact::Stopped {
                    operation: "the agreements pull request".into(),
                    error: format!("{} was closed without being merged", pull.url),
                });
            }
            PullState::Closed | PullState::Open => {}
        }
        let refreshed;
        let need_approval = gh.standing(pr)?.review == ReviewDecision::Required;
        let state = if need_approval == state.agreements_need_approval {
            state
        } else {
            refreshed = SprintState {
                agreements_need_approval: need_approval,
                ..state.clone()
            };
            self.save_sprint(&refreshed)?;
            &refreshed
        };
        if stopped || self.config.merge.by == MergeBy::Owner {
            return Ok(());
        }
        let name = format!("agreements-{}", state.id);
        let subject = Subject {
            worktree: self.store.worktree(&name),
            name,
            log: ("sprint", state.id.clone()),
            pr,
            branch: branch.clone(),
            pushed_at: state.agreements_pushed_at.clone(),
            declared: BTreeSet::new(),
            owner: OwnerApprovals::Given(BTreeSet::from([Approval::FactoryConfiguration])),
        };
        match self.gate(&subject)? {
            Some(Gated::Merged) => {
                self.delete_branch(("sprint", &state.id), &branch)?;
                self.sprint_fact(&SprintFact::AgreementsMerged)
            }
            Some(Gated::Waiting(Wait::Return(Return::Push))) => {
                if self.push_agreements(&subject.worktree, &branch)? {
                    let mut next = state.clone();
                    next.agreements_pushed_at = Some(jiff::Timestamp::now().to_string());
                    self.save_sprint(&next)?;
                }
                Ok(())
            }
            Some(Gated::Waiting(Wait::Return(Return::Ci)))
                if let Some(failed) = self.failed_checks(&pull.head_sha)? =>
            {
                self.sprint_fact(&SprintFact::Stopped {
                    operation: "the agreements pull request's checks".into(),
                    error: format!("failed on {}: {}", pull.head_sha, failed.join(", ")),
                })
            }
            Some(Gated::Waiting(wait)) => {
                let waiting = format!("{wait:?}");
                if self.agreements_waiting.as_deref() == Some(waiting.as_str()) {
                    return Ok(());
                }
                self.event(
                    &json!({"event": "agreements-gate", "sprint": state.id, "decision": waiting}),
                )?;
                self.agreements_waiting = Some(waiting);
                Ok(())
            }
            None => Ok(()),
        }
    }

    /// Pushes the agreements branch after bringing in its current head and the base, as an
    /// issue's push does. A conflict, which no agent resolves in an agreements file, goes to the
    /// lead; `Ok(false)` then.
    fn push_agreements(
        &mut self,
        worktree: &std::path::Path,
        branch: &str,
    ) -> Result<bool, DaemonError> {
        if !self.bring_in(worktree, branch)? {
            self.sprint_fact(&SprintFact::Stopped {
                operation: "bringing the base into the agreements".into(),
                error: "the merge conflicted".into(),
            })?;
            return Ok(false);
        }
        git_remote(
            worktree,
            &self.github()?.git_remote(),
            &["push", "-q", "origin", &format!("HEAD:refs/heads/{branch}")],
        )?;
        Ok(true)
    }

    /// Records the sprint as ended and begins the next one, which the lead proposes.
    /// The record of the sprint that ended last, which holds its review, and the owner's replies
    /// to that review: what the owner said about the result is the next plan's to answer.
    fn last_review_and_its_replies(&self) -> Result<Vec<String>, DaemonError> {
        let Some(last) = self.store.record_names("sprints")?.into_iter().max() else {
            return Ok(Vec::new());
        };
        let mut refs = vec![
            self.store
                .root()
                .join(Store::sprint_record(&last))
                .display()
                .to_string(),
        ];
        refs.extend(
            self.decisions()?
                .into_iter()
                .filter(|d| d.kind == DecisionKind::Review && d.sprint.as_deref() == Some(&last))
                .map(|d| self.store.source(&format!("answers/{}.md", d.id)))
                .filter(|p| p.exists())
                .map(|p| p.display().to_string()),
        );
        Ok(refs)
    }

    fn end_sprint(&mut self) -> Result<(), DaemonError> {
        let Some(state) = self.sprint()? else {
            return Ok(());
        };
        let mut ended = state.clone();
        ended.ended_at = Some(jiff::Timestamp::now().to_string());
        self.store
            .write_record(&Store::sprint_record(&state.id), &ended)?;
        let id = jiff::ZonedDateTime::now()
            .strftime("%Y%m%d-%H%M%S")
            .to_string();
        let (next, actions) = sprint::begin(&id);
        self.save_sprint(&next)?;
        self.event(&json!({"event": "sprint-begun", "sprint": id, "after": state.id}))?;
        self.sprint_actions(&actions)
    }
}

/// Writes a file the review gives whole, or deletes it when the review leaves it empty.
fn write_whole(path: &std::path::Path, content: &str) -> Result<(), DaemonError> {
    if content.trim().is_empty() {
        if path.exists() {
            fs::remove_file(path).map_err(StoreError::io(path))?;
        }
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(StoreError::io(dir))?;
    }
    fs::write(path, content).map_err(StoreError::io(path))?;
    Ok(())
}

/// Whether every planned issue is in the sprint and has ended. The plan decides, since putting
/// its issues on the board can stop partway.
fn plan_settled(sprint: &SprintState, issues: &[IssueRecord]) -> bool {
    sprint.plan.as_ref().is_some_and(|plan| {
        plan.issues.iter().all(|planned| {
            issues.iter().any(|r| {
                r.key() == planned.key
                    && r.sprint.as_deref() == Some(sprint.id.as_str())
                    && r.flow.stage.is_finished()
            })
        })
    })
}

/// The issues that have started and not ended: each holds one of `limits.issues_at_once`.
pub(crate) fn under_way(issues: &[IssueRecord]) -> impl Iterator<Item = &IssueRecord> {
    issues
        .iter()
        .filter(|r| r.flow.stage != Stage::Queued && !r.flow.stage.is_finished())
}

/// The issues `record` depends on that are not merged; it starts once there are none.
pub(crate) fn unmerged_dependencies(record: &IssueRecord, issues: &[IssueRecord]) -> Vec<String> {
    record
        .depends_on
        .iter()
        .filter(|dep| {
            !issues
                .iter()
                .any(|r| r.key() == dep.as_str() && r.flow.stage == Stage::Merged)
        })
        .cloned()
        .collect()
}
