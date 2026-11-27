//! The issue's branch and pull request: pushing, waiting for checks and review bots, taking in
//! comments, answering them, and the merge gate.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use ovenspring_core::flow::{Fact, Stage};
use ovenspring_core::gate::{self, Return, Reviews, Verdict, View, Wait};
use ovenspring_core::output::{Approval, Disposition, StepOutput};
use ovenspring_core::step::Step;
use ovenspring_integrations::channel::DecisionKind;
use ovenspring_integrations::comments::{self, Trust, is_configured_bot};
use ovenspring_integrations::github::{
    BranchRules, Check, CheckState, Comment, CommentKind, GitHub, MergeOutcome, MergeState,
    NewPull, PullState, Review, ReviewDecision, ReviewState,
};
use ovenspring_integrations::http::Unanswered;
use serde_json::json;

use crate::errors::{ErrorKey, Failure, Via, out_of_reach, unanswered_cause};
use crate::issues::IssueRecord;
use crate::owner::{DecisionRecord, DecisionStatus};
use crate::worktree::git;
use crate::{Daemon, DaemonError};

/// Whether the checks on a commit passed: every check reported on it finished and succeeded, and
/// every check the base branch requires is among them. `None` while any is still to report. A
/// commit with no check at all passes only once `quiet` says CI had time to show itself, since
/// checks appear a few seconds after a push. A review bot's check is not CI.
pub fn checks_result(checks: &[Check], rules: &BranchRules, quiet: bool) -> Option<bool> {
    // A review bot's check is waited for with the bots, under their own limit.
    let checks: Vec<&Check> = checks
        .iter()
        .filter(|c| c.name != CLAUDE_REVIEW_CHECK)
        .collect();
    if checks.iter().any(|c| c.state == CheckState::Failure) {
        return Some(false);
    }
    if checks.iter().any(|c| c.state == CheckState::Pending)
        || !rules
            .required_checks
            .iter()
            .all(|r| checks.iter().any(|c| &c.name == r))
    {
        return None;
    }
    (!checks.is_empty() || quiet).then_some(true)
}

/// How far one review bot has got on the pushed commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BotProgress {
    /// Its review of this commit is under way.
    Pending,
    Done,
    /// It has left no trace on the pull request.
    Absent,
}

/// The check run Claude Code Review reports under.
pub const CLAUDE_REVIEW_CHECK: &str = "Claude Code Review";

/// Claude Code Review, from its check run on the commit.
pub fn claude_progress(checks: &[Check]) -> BotProgress {
    match checks.iter().find(|c| c.name == CLAUDE_REVIEW_CHECK) {
        None => BotProgress::Absent,
        Some(c) if c.state == CheckState::Pending => BotProgress::Pending,
        Some(_) => BotProgress::Done,
    }
}

/// The Codex connector keeps its progress in a pull request comment as
/// `codex-review-state:v1 {json} -->`; it is done when every review it started has completed on
/// this commit. Only a configured bot's comment counts, as for the comments taken in: anyone can
/// comment on a pull request, and the progress decides how long the merge waits.
pub fn codex_progress(comments: &[Comment], sha: &str, bots: &[String]) -> BotProgress {
    let Some(state) = comments
        .iter()
        .filter(|c| c.kind == CommentKind::IssueComment && is_configured_bot(c, bots))
        .filter_map(|c| codex_state(&c.body))
        .next_back()
    else {
        return BotProgress::Absent;
    };
    let done = state
        .as_object()
        .into_iter()
        .flat_map(serde_json::Map::values)
        .filter(|review| review.get("status").is_some())
        .all(|review| {
            review.get("commit_sha").and_then(serde_json::Value::as_str) == Some(sha)
                && review.get("status").and_then(serde_json::Value::as_str) == Some("completed")
        });
    if done {
        BotProgress::Done
    } else {
        BotProgress::Pending
    }
}

fn codex_state(body: &str) -> Option<serde_json::Value> {
    const MARK: &str = "codex-review-state:v1 ";
    let start = body.find(MARK)? + MARK.len();
    let rest = body.get(start..)?;
    let end = rest.find(" -->")?;
    serde_json::from_str(rest.get(..end)?).ok()
}

/// The pull request's description: the issue's outcome, its key and its design document.
fn pull_body(record: &IssueRecord, decided: &[DecisionRecord]) -> String {
    let outcome = match record.output(Step::Requirements) {
        Some(StepOutput::Requirements(r)) => r.convergence.outcome,
        _ => record.title.clone(),
    };
    let design = record
        .design_path()
        .map(|p| format!("\nDesign: `{p}`"))
        .unwrap_or_default();
    // Links, not the words: what the owner wrote stays in Slack, where it was said.
    let links: String = decided
        .iter()
        .filter_map(|d| {
            let link = d.message.as_ref()?.permalink.as_deref()?;
            Some(format!("\n- {}: {link}", exchange_name(d)?))
        })
        .collect();
    let decided = if links.is_empty() {
        String::new()
    } else {
        format!("\n\nDecided with the owner in Slack:{links}")
    };
    format!(
        "{outcome}\n\nIssue: {}{design}{decided}\n\nOpened by Oven Spring.",
        record.key()
    )
}

/// What an answered exchange with the owner was, when no document records its answer: the
/// questions of a failure or a review finding, a recovery, a stop. A question from requirements or
/// design is answered in the documents the pull request carries.
fn exchange_name(d: &DecisionRecord) -> Option<&'static str> {
    if d.status != DecisionStatus::Answered {
        return None;
    }
    match (d.kind, d.asked_by) {
        (DecisionKind::Spec, Some(Step::Exception)) => {
            Some("The lead's question when the work could not go on")
        }
        (DecisionKind::Recovery, _) => Some("A recovery the lead proposed"),
        (DecisionKind::Stopped, _) => Some("A stop the owner resolved"),
        _ => None,
    }
}

/// Whether changes are requested, and whom to ask again, by each reviewer's latest deciding review.
fn review_standing(reviews: &[Review]) -> (bool, BTreeSet<String>) {
    let mut latest: BTreeMap<&str, ReviewState> = BTreeMap::new();
    for r in reviews {
        if matches!(
            r.state,
            ReviewState::Approved | ReviewState::ChangesRequested | ReviewState::Dismissed
        ) {
            latest.insert(&r.author, r.state);
        }
    }
    let changes = latest.values().any(|s| *s == ReviewState::ChangesRequested);
    // Reviewers to ask again after a push: who requested changes, and whose approval was dismissed.
    let again = latest
        .iter()
        .filter(|(_, s)| matches!(s, ReviewState::ChangesRequested | ReviewState::Dismissed))
        .map(|(a, _)| (*a).to_owned())
        .collect();
    (changes, again)
}

impl Daemon {
    fn github(&self) -> Result<&dyn GitHub, DaemonError> {
        self.services
            .github
            .as_deref()
            .ok_or_else(|| DaemonError::Launch("no GitHub connection is configured".into()))
    }

    /// Pushes the issue branch, opens its pull request on the first push, and asks reviewers to
    /// look again. A push that fails leaves the issue in `pushing`; reconciliation retries it.
    pub(crate) fn push(&mut self, key: &str) -> Result<Option<Fact>, DaemonError> {
        let record = self.record_of(key)?;
        let worktree = self.worktree_of(key);
        let branch = record.branch();
        if !self.bring_in(&worktree, &branch)? {
            self.event(&json!({"event": "push-conflict", "issue": key}))?;
            return Ok(Some(Fact::PushConflict));
        }
        let pushed = git(
            &worktree,
            &["push", "-q", "origin", &format!("HEAD:refs/heads/{branch}")],
        )
        .and_then(|_| git(&worktree, &["rev-parse", "HEAD"]));
        let sha = match pushed {
            Ok(sha) => sha,
            // `origin` answered and refused the push (a hook, most often): the quality run corrects
            // what it said. Otherwise `origin` was out of reach, and the next pass pushes again.
            Err(e)
                if git(
                    &worktree,
                    &["ls-remote", "-q", "--exit-code", "origin", "HEAD"],
                )
                .is_ok() =>
            {
                self.refused(key, "push", &e.to_string())?;
                return Ok(Some(Fact::PushRefused));
            }
            Err(e) => {
                self.event(&json!({"event": "push-failed", "issue": key, "error": e.to_string()}))?;
                return Ok(None);
            }
        };
        let decided: Vec<DecisionRecord> = self
            .decisions()?
            .into_iter()
            .filter(|d| d.issue.as_deref() == Some(key))
            .collect();
        let body = pull_body(&record, &decided);
        let gh = self.github()?;
        let existing = match record.flow.pr {
            Some(n) => Some(gh.pull(n)),
            None => gh.pull_for_branch(&branch).transpose(),
        };
        let pull = match existing {
            Some(Ok(p)) if p.state == PullState::Open => {
                // What was decided with the owner since the last push reaches the body.
                if let Err(e) = gh.set_pull_body(p.number, &body) {
                    self.github_error(key, &e)?;
                }
                p
            }
            Some(Err(e)) => return Err(e.into()),
            _ => gh.open_pull(&NewPull {
                head: &branch,
                base: &self.config.repo.base_branch,
                title: &format!("{key}: {}", record.title),
                body: &body,
            })?,
        };
        // Reviewers are asked again on the next push when their reviews cannot be read now.
        let again = match gh.reviews(pull.number) {
            Ok(reviews) => review_standing(&reviews).1,
            Err(e) => {
                self.github_error(key, &e)?;
                BTreeSet::new()
            }
        };
        if !again.is_empty() && record.pushed_sha.is_some() {
            let logins: Vec<String> = again.into_iter().collect();
            if let Err(e) = gh.request_reviewers(pull.number, &logins) {
                self.github_error(key, &e)?;
            } else {
                self.event(
                    &json!({"event": "review-requested", "issue": key, "reviewers": logins}),
                )?;
            }
        }
        self.update_issue(key, |r| {
            r.flow.pr = Some(pull.number);
            r.pr_url = Some(pull.url.clone());
            r.pushed_sha = Some(sha.clone());
            r.pushed_at = Some(jiff::Timestamp::now().to_string());
        })?;
        self.event(&json!({"event": "pushed", "issue": key, "sha": sha, "pr": pull.number}))?;
        self.answer_comments(key, true)?;
        Ok(Some(Fact::Pushed))
    }

    /// The names of the checks that failed on `sha`, once every check finished and one failed.
    pub(crate) fn failed_checks(&self, sha: &str) -> Result<Option<Vec<String>>, DaemonError> {
        let gh = self.github()?;
        let rules = gh.branch_rules(&self.config.repo.base_branch)?;
        let checks = gh.checks(sha)?;
        Ok((checks_result(&checks, &rules, true) == Some(false)).then(|| failures(&checks)))
    }

    /// Merges the branch's current head on `origin` (what anyone pushed to it) and the base into
    /// the worktree before a push, so CI and the bots run once, on the current base. These merges
    /// carry no code of the factory's own and take the repository's git identity. `Ok(false)` when
    /// one conflicts: the merge is undone and the executor resolves it.
    pub(crate) fn bring_in(
        &self,
        worktree: &std::path::Path,
        branch: &str,
    ) -> Result<bool, DaemonError> {
        let base = &self.config.repo.base_branch;
        let pushed_before = git(
            worktree,
            &[
                "ls-remote",
                "-q",
                "--exit-code",
                "--heads",
                "origin",
                branch,
            ],
        )
        .is_ok();
        let mut refs = vec![base.as_str()];
        if pushed_before {
            refs.push(branch);
        }
        merge_from_origin(worktree, &refs)
    }

    pub(crate) fn github_error(
        &self,
        key: &str,
        e: &ovenspring_integrations::http::HttpError,
    ) -> Result<(), DaemonError> {
        self.event(&json!({"event": "github-error", "issue": key, "error": e.to_string()}))
    }

    /// One look at GitHub for every issue with a pull request; what fails for one issue is that
    /// issue's (`settle`), and the others are still read.
    pub(crate) fn poll_pull_requests(&mut self) -> Result<(), DaemonError> {
        let Some(gh) = self.services.github.clone() else {
            return Ok(());
        };
        // While GitHub is out of reach, each way that failed is asked whether it answers again;
        // they all answering only lets the error clear at the pass's end, since a read below can
        // still fail as out of reach.
        if let Some(open) = self.errors.state.open().get(&ErrorKey::GithubUnreachable) {
            let via = open.via.clone();
            let mut answered = true;
            if via.contains(&Via::Api)
                && let Err(e) = gh.viewer()
                && let Some((why, cause)) = out_of_reach(&e)
            {
                self.failed(
                    ErrorKey::GithubUnreachable,
                    Failure {
                        event: json!({"event": "service-unreachable", "operation": "checking that GitHub answers",
                                      "error": e.to_string(), "why": why}),
                        cause,
                        via: Some(Via::Api),
                        reason: None,
                    },
                )?;
                // The reads below would fail the same way.
                return Ok(());
            }
            if via.contains(&Via::Git) && !self.origin_answers() {
                let why = Unanswered::NoConnection;
                self.failed(
                    ErrorKey::GithubUnreachable,
                    Failure {
                        event: json!({"event": "service-unreachable", "operation": "checking that origin answers",
                                      "error": "origin does not answer", "why": why}),
                        cause: unanswered_cause(why, None),
                        via: Some(Via::Git),
                        reason: None,
                    },
                )?;
                answered = false;
            }
            self.errors.github_clear = answered;
        }
        for record in self.issues()? {
            let key = record.key().to_owned();
            let read = self.poll_pull_request(&record);
            self.settle(&key, "reading its pull request", read)?;
        }
        Ok(())
    }

    fn poll_pull_request(&mut self, record: &IssueRecord) -> Result<(), DaemonError> {
        let key = record.key().to_owned();
        let record = match self.observe_pull(record)? {
            Observed::Fact(f) => return self.apply(&key, f),
            Observed::Waiting(r) => *r,
            Observed::Nothing => return Ok(()),
        };
        let fact = match record.flow.stage {
            Stage::Ci => self.poll_checks(&record)?,
            Stage::WaitingBots => self.poll_bots(&record)?,
            Stage::WaitingReview | Stage::Gate | Stage::HeldForOwner | Stage::BlockedOnGitHub => {
                if self.new_comments(&record)? {
                    Some(Fact::CommentsArrived)
                } else {
                    self.run_gate(&key)?
                }
            }
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
            | Stage::ReadyForOwner
            | Stage::Held
            | Stage::Merged
            | Stage::Dropped
            | Stage::Exception { .. } => None,
        };
        match fact {
            Some(f) => self.apply(&key, f),
            None => Ok(()),
        }
    }

    /// Reads an issue's pull request, whatever the issue's state: merged or closed by anyone, it
    /// says so at once. While the issue waits on the pull request, a head someone else pushed is
    /// taken as it is: the worktree is moved to it and the record names it, so checks, bots and the
    /// gate are read for it and the next step works on it. The factory does not check or review
    /// what others push.
    fn observe_pull(&mut self, record: &IssueRecord) -> Result<Observed, DaemonError> {
        let Some(pr) = record.flow.pr else {
            return Ok(Observed::Nothing);
        };
        // A held issue's pull request stays open; the owner may merge or close it.
        if matches!(record.flow.stage, Stage::Merged | Stage::Dropped) {
            return Ok(Observed::Nothing);
        }
        let pull = self.github()?.pull(pr)?;
        match pull.state {
            PullState::Merged => return Ok(Observed::Fact(Fact::MergedOnGitHub)),
            PullState::Closed => return Ok(Observed::Fact(Fact::PullClosed)),
            PullState::Open => {}
        }
        let waiting = matches!(
            record.flow.stage,
            Stage::Ci
                | Stage::WaitingBots
                | Stage::WaitingReview
                | Stage::Gate
                | Stage::BlockedOnGitHub
                | Stage::HeldForOwner
                | Stage::ReadyForOwner
        );
        if !waiting {
            return Ok(Observed::Nothing);
        }
        if record.pushed_sha.as_deref() == Some(pull.head_sha.as_str()) {
            return Ok(Observed::Waiting(Box::new(record.clone())));
        }
        // GitHub has not caught up with the factory's own push yet: its head is one the push
        // contains.
        let worktree = self.worktree_of(record.key());
        if let Some(pushed) = &record.pushed_sha
            && git(
                &worktree,
                &["merge-base", "--is-ancestor", &pull.head_sha, pushed],
            )
            .is_ok()
        {
            return Ok(Observed::Waiting(Box::new(record.clone())));
        }
        // Nothing uncommitted is in the worktree while the issue waits: every commit was pushed.
        git(&worktree, &["fetch", "-q", "origin", &record.branch()])?;
        git(&worktree, &["reset", "-q", "--hard", &pull.head_sha])?;
        let key = record.key().to_owned();
        let taken = self.update_issue(&key, |r| {
            r.pushed_sha = Some(pull.head_sha.clone());
            r.pushed_at = Some(jiff::Timestamp::now().to_string());
        })?;
        self.event(&json!({"event": "head-taken", "issue": key, "sha": pull.head_sha}))?;
        Ok(Observed::Waiting(Box::new(taken)))
    }

    /// Whether a commit pushed at `pushed_at` has had the time CI takes to show itself: as long
    /// as the wait for a review bot that has left no trace.
    fn quiet_since(&self, pushed_at: Option<&str>) -> bool {
        pushed_at
            .and_then(|t| t.parse::<jiff::Timestamp>().ok())
            .is_none_or(|t| {
                jiff::Timestamp::now().duration_since(t).unsigned_abs() >= self.timing.bot_absent
            })
    }

    fn poll_checks(&self, record: &IssueRecord) -> Result<Option<Fact>, DaemonError> {
        let Some(sha) = &record.pushed_sha else {
            return Ok(None);
        };
        let gh = self.github()?;
        let rules = gh.branch_rules(&self.config.repo.base_branch)?;
        let checks = gh.checks(sha)?;
        let quiet = self.quiet_since(record.pushed_at.as_deref());
        Ok(match checks_result(&checks, &rules, quiet) {
            Some(true) => Some(Fact::ChecksPassed),
            Some(false) => {
                let failed = failures(&checks);
                self.event(&json!({"event": "checks-failed", "issue": record.key(), "sha": sha, "failed": failed}))?;
                Some(Fact::ChecksFailed)
            }
            None => None,
        })
    }

    /// Waits for the review bots, then takes in the comments left on the pull request. A bot's
    /// comments are taken in only when `review.bots` names it, so with none named nothing is
    /// waited for.
    fn poll_bots(&mut self, record: &IssueRecord) -> Result<Option<Fact>, DaemonError> {
        if !self.config.review.bots.is_empty() && self.bots_under_way(record)? {
            return Ok(None);
        }
        let comments = self.new_comments(record)?;
        Ok(Some(Fact::BotWaitEnded { comments }))
    }

    /// Whether a review bot whose progress shows on the pull request is still to be waited for:
    /// one under way, up to `timing.bot_pending`; one that has left no trace, up to
    /// `timing.bot_absent`, after which it is taken as not running on this push.
    fn bots_under_way(&self, record: &IssueRecord) -> Result<bool, DaemonError> {
        let (Some(pr), Some(sha)) = (record.flow.pr, record.pushed_sha.as_deref()) else {
            return Ok(true);
        };
        let since = record
            .bots_since
            .as_deref()
            .ok_or_else(|| {
                DaemonError::Launch(format!("{}: no bot wait start recorded", record.key()))
            })?
            .parse::<jiff::Timestamp>()
            .map_err(|e| DaemonError::Launch(format!("{}: bot wait start: {e}", record.key())))?;
        let waited = jiff::Timestamp::now().duration_since(since).unsigned_abs();
        let gh = self.github()?;
        let progress = [
            claude_progress(&gh.checks(sha)?),
            codex_progress(&gh.comments(pr)?, sha, &self.config.review.bots),
        ];
        let waiting =
            |p: BotProgress, limit: std::time::Duration| progress.contains(&p) && waited < limit;
        Ok(waiting(BotProgress::Pending, self.timing.bot_pending)
            || waiting(BotProgress::Absent, self.timing.bot_absent))
    }

    /// Takes in comments not yet seen from trusted authors, writing them to a source file and
    /// keeping each as a finding for the executor to judge. Returns whether any arrived.
    fn new_comments(&mut self, record: &IssueRecord) -> Result<bool, DaemonError> {
        let Some(pr) = record.flow.pr else {
            return Ok(false);
        };
        let gh = self.github()?;
        let all = gh.comments(pr)?;
        let seen = &record.comments_seen;
        let accepted = comments::accept(
            gh,
            all,
            &Trust {
                bots: &self.config.review.bots,
            },
            &|k| seen.contains(k),
        )?;
        if accepted.is_empty() {
            return Ok(false);
        }
        let n = record.comments_seen.len() + 1;
        let text = comments::render(
            record.key(),
            record.pr_url.as_deref().unwrap_or_default(),
            &accepted,
        );
        let source = self.write_source(&format!("comments/{}-{n}.md", record.key()), &text)?;
        let keys: Vec<String> = accepted.iter().map(Comment::key).collect();
        let findings = comments::as_findings(&accepted);
        self.update_issue(record.key(), |r| {
            r.comments_seen.extend(keys.iter().cloned());
            r.comments_source = Some(source.clone());
            r.findings
                .insert(crate::handoff::COMMENTS.to_owned(), findings);
        })?;
        self.event(
            &json!({"event": "comments-accepted", "issue": record.key(), "comments": keys}),
        )?;
        Ok(true)
    }

    /// Replies in each judged comment's thread: the commit that applied it, or the reason it was
    /// declined. A batch is answered at once when it applies nothing, and otherwise once `pushed`
    /// says a push carried the commits that applied it. A reply that cannot be posted is recorded
    /// as a `reply-failed` event naming the comment and why. Each reply is dropped from the record
    /// once posted or recorded, so a stop in between leaves only the rest owed.
    pub(crate) fn answer_comments(&mut self, key: &str, pushed: bool) -> Result<(), DaemonError> {
        let record = self.record_of(key)?;
        let Some(pr) = record.flow.pr else {
            return Ok(());
        };
        let due = record.replies;
        let ready = pushed || due.iter().all(|r| r.disposition == Disposition::Decline);
        if due.is_empty() || !ready {
            return Ok(());
        }
        let gh = self.github()?;
        let all = gh.comments(pr).map_err(|e| e.to_string());
        for reply in &due {
            let text = match (reply.disposition, &reply.commit) {
                (Disposition::Apply, Some(commit)) => Ok(format!("Applied in {commit}.")),
                (Disposition::Apply, None) => {
                    Err("no commit of the correction is recorded".to_owned())
                }
                (Disposition::Decline, _) => Ok(format!("Not applied: {}", reply.reason)),
            };
            let posted = text.and_then(|text| {
                let comment = all
                    .as_ref()
                    .map_err(Clone::clone)?
                    .iter()
                    .find(|c| c.key() == reply.comment)
                    .ok_or_else(|| "no comment on the pull request has this key".to_owned())?;
                gh.reply(pr, comment, &comments::factory_reply(&text))
                    .map(|()| text)
                    .map_err(|e| e.to_string())
            });
            self.update_issue(key, |r| r.replies.retain(|r| r.comment != reply.comment))?;
            match posted {
                Ok(text) => self.event(
                    &json!({"event": "replied", "issue": key, "comment": reply.comment, "text": text}),
                )?,
                Err(error) => self.event(
                    &json!({"event": "reply-failed", "issue": key, "comment": reply.comment, "error": error}),
                )?,
            }
        }
        Ok(())
    }

    /// Decides the merge from GitHub's current state, and merges when the gate allows it.
    pub(crate) fn run_gate(&mut self, key: &str) -> Result<Option<Fact>, DaemonError> {
        let record = self.record_of(key)?;
        let Some(pr) = record.flow.pr else {
            return Ok(None);
        };
        let subject = Subject {
            name: key.to_owned(),
            log: ("issue", key.to_owned()),
            pr,
            branch: record.branch(),
            pushed_at: record.pushed_at.clone(),
            worktree: self.worktree_of(key),
            declared: record.flow.approvals.iter().cloned().collect(),
            owner: OwnerApprovals::Recorded,
        };
        let wait = match self.gate(&subject)? {
            None => return Ok(None),
            Some(Gated::Merged) => return Ok(Some(Fact::MergedOnGitHub)),
            Some(Gated::Waiting(wait)) => wait,
        };
        // A wait the issue's stage already reflects changes nothing.
        let unchanged = matches!(
            (&wait, &record.flow.stage),
            (Wait::Return(Return::Approvals), Stage::WaitingReview)
                | (Wait::HoldForOwner(_), Stage::HeldForOwner)
                | (Wait::GitHubBlocks, Stage::BlockedOnGitHub)
        );
        if unchanged {
            // A new head while held asks again, for the head the gate would merge.
            if record.flow.stage == Stage::HeldForOwner {
                self.request_approval(key)?;
            }
            return Ok(None);
        }
        self.event(&json!({"event": "gate", "issue": key, "decision": format!("{wait:?}")}))?;
        Ok(Some(Fact::Gate(wait)))
    }

    /// The gate's decision on one pull request the factory pushed, after merging it when every
    /// condition holds. `None` while GitHub has not yet worked
    /// out whether it merges cleanly, or when GitHub refused the merge call.
    pub(crate) fn gate(&mut self, subject: &Subject) -> Result<Option<Gated>, DaemonError> {
        let Some(view) = self.gate_view(subject)? else {
            return Ok(None);
        };
        match gate::decide(&view) {
            Verdict::Merge(sha) => self.merge(subject, &sha),
            Verdict::Wait(wait) => Ok(Some(Gated::Waiting(wait))),
        }
    }

    fn gate_view(&self, subject: &Subject) -> Result<Option<View>, DaemonError> {
        let gh = self.github()?;
        let base = &self.config.repo.base_branch;
        let pull = gh.pull(subject.pr)?;
        let Some(mergeable) = pull.mergeable else {
            return Ok(None);
        };
        let rules = gh.branch_rules(base)?;
        let checks = gh.checks(&pull.head_sha)?;
        let (changes_requested, _) = review_standing(&gh.reviews(subject.pr)?);
        let standing = gh.standing(subject.pr)?;
        let github_blocks = match standing.merge {
            // GitHub has not worked it out yet: the gate looks again on the next poll.
            MergeState::Computing => return Ok(None),
            MergeState::Blocked => true,
            MergeState::Allowed => false,
        };
        let reviews = if changes_requested {
            Reviews::ChangesRequested
        } else if standing.review == ReviewDecision::Required {
            Reviews::ApprovalMissing
        } else {
            Reviews::Settled
        };
        // The head is judged whoever pushed it, so it is fetched with the base before its diff.
        git(
            &subject.worktree,
            &["fetch", "-q", "origin", base, &subject.branch],
        )?;
        let contains_base = git(
            &subject.worktree,
            &[
                "merge-base",
                "--is-ancestor",
                &format!("origin/{base}"),
                &pull.head_sha,
            ],
        )
        .is_ok();
        let owner_approved = match &subject.owner {
            OwnerApprovals::Recorded => self.owner_approved(&subject.name, &pull.head_sha)?,
            OwnerApprovals::Given(given) => given.clone(),
        };
        Ok(Some(View {
            head_sha: pull.head_sha.clone(),
            checks_passed: checks_result(
                &checks,
                &rules,
                self.quiet_since(subject.pushed_at.as_deref()),
            ),
            contains_base,
            mergeable,
            reviews,
            github_blocks,
            held: self.held_categories(&subject.worktree, &pull.head_sha, &subject.declared)?,
            owner_approved,
            merge_by: self.config.merge.by,
        }))
    }

    /// The categories a change is held for: what its design declared, what the owner's path rules
    /// (`[held.<category>]`) match in the diff from the base, and the factory's configuration and
    /// agent instructions whenever the diff touches them.
    pub(crate) fn held_categories(
        &self,
        worktree: &std::path::Path,
        head: &str,
        declared: &BTreeSet<Approval>,
    ) -> Result<BTreeSet<Approval>, DaemonError> {
        let base = format!("origin/{}", self.config.repo.base_branch);
        // NUL-separated paths are the names as stored, not quoted for display; without rename
        // detection a moved file lists both where it left and where it went.
        let changed = git(
            worktree,
            &[
                "diff",
                "--name-only",
                "-z",
                "--no-renames",
                &format!("{base}...{head}"),
            ],
        )?;
        Ok(categories_of_change(
            changed.split('\0').filter(|p| !p.is_empty()),
            &[&self.config.agreements.path, &self.config.knowledge.path],
            &self.config.held,
            declared,
        ))
    }

    /// Merges `sha` after reading the base and the pull request's head once more: a head
    /// that moved since the gate looked is judged again on the next poll; a base that moved sends
    /// the change back to push, which brings the base in.
    fn merge(&mut self, subject: &Subject, sha: &str) -> Result<Option<Gated>, DaemonError> {
        let gh = self.github()?;
        let base_head = gh.branch_head(&self.config.repo.base_branch)?;
        let pull = gh.pull(subject.pr)?;
        if pull.head_sha != sha {
            return Ok(None);
        }
        // The worktree may not have the new base head yet; either way the branch is not on it.
        if git(
            &subject.worktree,
            &["merge-base", "--is-ancestor", &base_head, sha],
        )
        .is_err()
        {
            return Ok(Some(Gated::Waiting(Wait::Return(Return::Push))));
        }
        match gh.merge(subject.pr, sha)? {
            MergeOutcome::Merged { sha: merged } => {
                // The base the last read named, masked as every recorded text is.
                let secrets: Vec<&str> = self.services.secrets.iter().map(String::as_str).collect();
                let base = pull.base.map(|b| crate::download::mask(&b, &secrets));
                let mut line = json!({"event": "merged", "pr": subject.pr, "sha": sha, "merge_commit": merged});
                if let (Some(base), Some(fields)) = (base, line.as_object_mut()) {
                    fields.insert("base".into(), json!(base));
                }
                self.event(&subject.named(line))?;
                Ok(Some(Gated::Merged))
            }
            MergeOutcome::Refused { reason } => {
                self.event(
                    &subject.named(json!({"event": "merge-refused", "sha": sha, "reason": reason})),
                )?;
                Ok(None)
            }
        }
    }
}

/// The checks that failed, each with the link to its run.
fn failures(checks: &[Check]) -> Vec<String> {
    checks
        .iter()
        .filter(|c| c.state == CheckState::Failure)
        .map(|c| format!("{} ({})", c.name, c.url.as_deref().unwrap_or("no link")))
        .collect()
}

/// What the gate came to on one pull request.
pub(crate) enum Gated {
    /// The daemon merged the head the gate passed.
    Merged,
    Waiting(Wait),
}

/// What reading an issue's pull request found.
enum Observed {
    /// Merged or closed: the fact to apply.
    Fact(Fact),
    /// The issue waits on its pull request, with the record as it now stands.
    Waiting(Box<IssueRecord>),
    /// No pull request, or the issue is not waiting on it.
    Nothing,
}

/// One pull request the factory pushed and the gate decides on: an issue's, or the working
/// agreements'.
pub(crate) struct Subject {
    /// The issue key, or the agreements branch's name.
    pub name: String,
    /// How its events name it: `("issue", key)` or `("sprint", id)`.
    pub log: (&'static str, String),
    pub pr: u64,
    /// The pull request's branch, fetched with the base before the gate reads its diff.
    pub branch: String,
    /// When its current head was pushed, or first seen (RFC 3339).
    pub pushed_at: Option<String>,
    pub worktree: PathBuf,
    /// The categories its design declared.
    pub declared: BTreeSet<Approval>,
    pub owner: OwnerApprovals,
}

impl Subject {
    /// `event` with the subject named the way its events name it.
    fn named(&self, mut event: serde_json::Value) -> serde_json::Value {
        if let Some(fields) = event.as_object_mut() {
            fields.insert(self.log.0.into(), json!(self.log.1));
        }
        event
    }
}

/// Where the owner's approvals for the head come from.
pub(crate) enum OwnerApprovals {
    /// Approvals recorded from the owner's merge requests, for the exact head.
    Recorded,
    /// Approved before the pull request existed: the sprint review that carried the agreements.
    Given(BTreeSet<Approval>),
}

/// Whether `path` is the factory's configuration, in one of its `folders`, or the repository's
/// agent instructions (`AGENTS.md`, `CLAUDE.md` at any depth).
fn is_factory_configuration(path: &str, folders: &[&str]) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    path.starts_with(".ovenspring/")
        || folders
            .iter()
            .any(|f| std::path::Path::new(path).starts_with(f))
        || matches!(name, "AGENTS.md" | "CLAUDE.md")
}

/// The categories a diff touching `changed` paths is held for.
fn categories_of_change<'a>(
    changed: impl IntoIterator<Item = &'a str>,
    folders: &[&str],
    rules: &std::collections::BTreeMap<Approval, ovenspring_core::config::HeldCategory>,
    declared: &BTreeSet<Approval>,
) -> BTreeSet<Approval> {
    let mut categories = declared.clone();
    for path in changed {
        if is_factory_configuration(path, folders) {
            categories.insert(Approval::FactoryConfiguration);
        }
        for (category, rule) in rules {
            if rule.paths.iter().any(|p| gate::path_matches(p, path)) {
                categories.insert(category.clone());
            }
        }
    }
    categories
}

/// Fetches `refs` from `origin` and merges each one the worktree does not contain yet, the last
/// first. `Ok(false)` when one conflicts: the merge is undone.
pub(crate) fn merge_from_origin(
    worktree: &std::path::Path,
    refs: &[&str],
) -> Result<bool, DaemonError> {
    let mut fetch = vec!["fetch", "-q", "origin"];
    fetch.extend(refs);
    git(worktree, &fetch)?;
    for name in refs.iter().rev() {
        let target = format!("origin/{name}");
        if git(worktree, &["merge-base", "--is-ancestor", &target, "HEAD"]).is_ok() {
            continue;
        }
        if git(worktree, &["merge", "-q", "--no-edit", &target]).is_err() {
            let _ = git(worktree, &["merge", "--abort"]);
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answered(kind: DecisionKind, asked_by: Option<Step>, link: Option<&str>) -> DecisionRecord {
        DecisionRecord {
            id: format!("ACME-1-{kind:?}"),
            kind,
            issue: Some("ACME-1".into()),
            sprint: None,
            sha: None,
            categories: vec![],
            title: "ACME-1: Export".into(),
            text: "The owner's own words, which stay in Slack.".into(),
            status: DecisionStatus::Answered,
            message: Some(ovenspring_integrations::channel::MessageRef {
                channel: "C01".into(),
                ts: "1.0".into(),
                permalink: link.map(str::to_owned),
            }),
            answer: None,
            procedure: None,
            asked_by,
            proposal: None,
        }
    }

    #[test]
    fn the_body_links_what_no_document_records_of_what_was_decided_with_the_owner() {
        let record = IssueRecord::new("ACME-1", "Export", vec![]);
        let pending = DecisionRecord {
            status: DecisionStatus::Pending,
            ..answered(DecisionKind::Recovery, None, Some("https://s.test/p5"))
        };
        let decided = [
            answered(
                DecisionKind::Spec,
                Some(Step::Exception),
                Some("https://s.test/p1"),
            ),
            answered(
                DecisionKind::Spec,
                Some(Step::Requirements),
                Some("https://s.test/p2"),
            ),
            answered(DecisionKind::Stopped, None, Some("https://s.test/p3")),
            answered(DecisionKind::Spec, Some(Step::Design), None),
            answered(DecisionKind::Design, None, Some("https://s.test/p4")),
            pending,
        ];

        let body = pull_body(&record, &decided);

        assert_eq!(
            body,
            "Export\n\nIssue: ACME-1\n\nDecided with the owner in Slack:\n- The lead's question when the work could not go on: https://s.test/p1\n- A stop the owner resolved: https://s.test/p3\n\nOpened by Oven Spring."
        );
        assert_eq!(
            pull_body(&record, &[]),
            "Export\n\nIssue: ACME-1\n\nOpened by Oven Spring."
        );
    }

    fn check(name: &str, state: CheckState) -> Check {
        Check {
            name: name.into(),
            state,
            url: None,
        }
    }

    fn rules(required: &[&str]) -> BranchRules {
        BranchRules {
            required_checks: required.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    #[test]
    fn every_reported_check_must_finish_and_succeed() {
        let none = rules(&[]);
        let passed = [
            check("test", CheckState::Success),
            check("lint", CheckState::Success),
        ];
        assert_eq!(checks_result(&passed, &none, false), Some(true));
        assert_eq!(
            checks_result(
                &[
                    check("test", CheckState::Success),
                    check("lint", CheckState::Pending)
                ],
                &none,
                true
            ),
            None,
            "a check still running is waited for"
        );
        assert_eq!(
            checks_result(
                &[
                    check("test", CheckState::Success),
                    check("lint", CheckState::Failure)
                ],
                &none,
                true
            ),
            Some(false),
            "a failed check fails whether or not the base requires it"
        );
        assert_eq!(
            checks_result(
                &[check("test", CheckState::Success)],
                &rules(&["test", "e2e"]),
                true
            ),
            None,
            "a required check not reported yet is waited for"
        );
    }

    #[test]
    fn a_commit_with_no_check_passes_only_once_ci_had_time_to_show() {
        assert_eq!(checks_result(&[], &rules(&[]), false), None);
        assert_eq!(checks_result(&[], &rules(&[]), true), Some(true));
    }

    fn review(author: &str, state: ReviewState) -> Review {
        Review {
            id: 0,
            author: author.into(),
            state,
            commit_id: None,
            body: String::new(),
        }
    }

    #[test]
    fn each_reviewers_latest_deciding_review_counts() {
        let (changes, again) = review_standing(&[
            review("lee", ReviewState::ChangesRequested),
            review("lee", ReviewState::Commented),
            review("dana", ReviewState::Approved),
            review("mo", ReviewState::Approved),
            review("mo", ReviewState::Dismissed),
        ]);
        assert!(changes);
        assert_eq!(again, BTreeSet::from(["lee".to_owned(), "mo".to_owned()]));
        let (changes, _) = review_standing(&[
            review("lee", ReviewState::ChangesRequested),
            review("lee", ReviewState::Approved),
        ]);
        assert!(!changes);
    }

    #[test]
    fn claude_is_pending_until_its_check_run_completes_and_absent_without_one() {
        assert_eq!(
            claude_progress(&[check("test", CheckState::Success)]),
            BotProgress::Absent
        );
        assert_eq!(
            claude_progress(&[check(CLAUDE_REVIEW_CHECK, CheckState::Pending)]),
            BotProgress::Pending
        );
        // A finished review concludes neutral or skipped, never failed; both count as done.
        assert_eq!(
            claude_progress(&[check(CLAUDE_REVIEW_CHECK, CheckState::Success)]),
            BotProgress::Done
        );
    }

    fn comment(kind: CommentKind, body: &str) -> Comment {
        Comment {
            kind,
            id: 1,
            author: "chatgpt-codex-connector[bot]".into(),
            author_is_bot: true,
            body: body.into(),
            path: None,
            line: None,
            url: String::new(),
        }
    }

    fn state(reviews: &str) -> Comment {
        comment(
            CommentKind::IssueComment,
            &format!("Codex review <!-- codex-review-state:v1 {reviews} -->"),
        )
    }

    #[test]
    fn codex_is_done_only_when_every_review_completed_on_this_commit() {
        let sha = "abc";
        let bots = ["chatgpt-codex-connector[bot]".to_owned()];
        assert_eq!(codex_progress(&[], sha, &bots), BotProgress::Absent);
        assert_eq!(
            codex_progress(&[comment(CommentKind::IssueComment, "LGTM")], sha, &bots),
            BotProgress::Absent
        );
        let cases = [
            (
                r#"{"r1":{"status":"in_progress","commit_sha":"abc"}}"#,
                BotProgress::Pending,
            ),
            (
                r#"{"r1":{"status":"completed","commit_sha":"old"}}"#,
                BotProgress::Pending,
            ),
            (
                r#"{"r1":{"status":"completed","commit_sha":"abc"},"r2":{"status":"queued","commit_sha":"abc"}}"#,
                BotProgress::Pending,
            ),
            (
                r#"{"r1":{"status":"completed","commit_sha":"abc"},"note":{"text":"no status"}}"#,
                BotProgress::Done,
            ),
        ];
        for (reviews, expected) in cases {
            assert_eq!(
                codex_progress(&[state(reviews)], sha, &bots),
                expected,
                "{reviews}"
            );
        }
    }

    #[test]
    fn codex_progress_written_by_anyone_but_a_configured_bot_is_not_read() {
        let done = state(r#"{"r1":{"status":"completed","commit_sha":"abc"}}"#);
        let by_a_person = Comment {
            author: "someone".into(),
            author_is_bot: false,
            ..done.clone()
        };
        let bots = ["chatgpt-codex-connector[bot]".to_owned()];
        assert_eq!(
            codex_progress(&[by_a_person], "abc", &bots),
            BotProgress::Absent
        );
        assert_eq!(codex_progress(&[done], "abc", &[]), BotProgress::Absent);
    }

    #[test]
    fn the_latest_codex_state_counts() {
        let older = state(r#"{"r1":{"status":"in_progress","commit_sha":"abc"}}"#);
        let newer = state(r#"{"r1":{"status":"completed","commit_sha":"abc"}}"#);
        let bots = ["chatgpt-codex-connector[bot]".to_owned()];
        assert_eq!(
            codex_progress(&[older, newer], "abc", &bots),
            BotProgress::Done
        );
    }

    #[test]
    fn a_change_to_the_agreements_or_knowledge_folder_is_held_wherever_they_are_kept() {
        let folders = ["docs/factory/agreements", "docs/knowledge"];
        let held = |path: &str| {
            categories_of_change([path], &folders, &BTreeMap::new(), &BTreeSet::new())
                .contains(&Approval::FactoryConfiguration)
        };
        assert!(held("docs/factory/agreements/executor.md"));
        assert!(held("docs/knowledge/search-index.md"));
        assert!(held(".ovenspring/config.toml"));
        assert!(held("web/AGENTS.md"));
        assert!(!held("docs/knowledge-old/notes.md"));
        assert!(!held("src/export.rs"));
    }
}
