//! What the factory asks the owner and what the owner answers. A decision is recorded before it is
//! sent and resolved once, by id.

use std::collections::BTreeSet;

use ovenspring_core::flow::{Approve, Fact, Stage};
use ovenspring_core::output::{Approval, ReferenceStrength, StepOutput};
use ovenspring_core::sprint::SprintFact;
use ovenspring_core::step::Step;
use ovenspring_integrations::channel::{
    Button, ChannelEvent, DecisionKind, DecisionRequest, MessageRef, Notice, NoticeKind,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::{Arc, PoisonError};
use std::time::{Instant, SystemTime};

use crate::app::{App, READY_WAIT};
use crate::conversation::Said;
use crate::issues::IssueRecord;
use crate::{Daemon, DaemonError};
use ovenspring_store::StoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DecisionStatus {
    Unsent,
    Pending,
    Answered,
    Superseded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnswerKind {
    Approve,
    Hold,
    Reply,
    Resolved,
    /// The owner ended the issue the decision is about.
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answer {
    pub kind: AnswerKind,
    pub text: Option<String>,
    pub user: String,
    pub at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionRecord {
    pub id: String,
    pub kind: DecisionKind,
    pub issue: Option<String>,
    /// The sprint a decision about no issue belongs to.
    #[serde(default)]
    pub sprint: Option<String>,
    pub sha: Option<String>,
    pub categories: Vec<Approval>,
    pub title: String,
    pub text: String,
    pub status: DecisionStatus,
    pub message: Option<MessageRef>,
    pub answer: Option<Answer>,
    /// For a recovery: the procedure's file under `<isolated>/recoveries/`, as the owner is shown it.
    #[serde(default)]
    pub procedure: Option<String>,
    /// For a question: the step that asked it.
    #[serde(default)]
    pub asked_by: Option<Step>,
}

/// An owner's approval of a decision, kept for the audit trail and the gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalRecord {
    pub decision: String,
    pub kind: DecisionKind,
    pub issue: Option<String>,
    pub sha: Option<String>,
    pub categories: Vec<Approval>,
    pub user: String,
    pub at: String,
    /// The approved message.
    pub permalink: Option<String>,
}

pub(crate) fn decision_path(id: &str) -> String {
    format!("decisions/{id}.json")
}

/// Where the approval of decision `id` is kept, under the isolated directory.
pub(crate) fn approval_path(id: &str) -> String {
    format!("approvals/{id}.json")
}

impl Daemon {
    pub fn decisions(&self) -> Result<Vec<DecisionRecord>, DaemonError> {
        crate::records::decisions(&self.store)
    }

    fn save_decision(&self, d: &DecisionRecord) -> Result<(), DaemonError> {
        self.store.write_isolated(&decision_path(&d.id), d)?;
        Ok(())
    }

    /// A new decision of `kind` about `issue`, or about the sprint, numbered and titled; the
    /// caller gives its text, and `decide` sends it.
    pub(crate) fn new_decision(
        &self,
        kind: DecisionKind,
        issue: Option<&str>,
        title: String,
    ) -> Result<DecisionRecord, DaemonError> {
        let n = self.store.isolated_names("decisions")?.len() + 1;
        let sprint = match issue {
            Some(_) => None,
            None => Some(self.sprint()?.map(|s| s.id).unwrap_or_default()),
        };
        let about = match (issue, &sprint) {
            (Some(key), _) => key.to_owned(),
            (None, s) => format!("sprint-{}", s.as_deref().unwrap_or_default()),
        };
        // The kind as the records name it.
        let kind_name = json!(kind);
        let id = format!("{about}-{}-{n}", kind_name.as_str().unwrap_or_default());
        Ok(DecisionRecord {
            id,
            kind,
            issue: issue.map(str::to_owned),
            sprint,
            sha: None,
            categories: vec![],
            title,
            text: String::new(),
            status: DecisionStatus::Unsent,
            message: None,
            answer: None,
            procedure: None,
            asked_by: None,
        })
    }

    /// Records a new decision and sends it; a send that fails is retried by `deliver_decisions`.
    pub(crate) fn decide(&self, d: DecisionRecord) -> Result<(), DaemonError> {
        self.save_decision(&d)?;
        self.event(&json!({"event": "decision", "id": d.id, "kind": d.kind, "issue": d.issue}))?;
        self.send_decision(d)
    }

    /// Posts the decision with its conclusion and points; its details, from the first heading on,
    /// follow as a reply in its thread, where the owner opens them when a point needs them.
    fn send_decision(&self, mut d: DecisionRecord) -> Result<(), DaemonError> {
        let (summary, details) = split_details(&d.text);
        let request = DecisionRequest {
            id: d.id.clone(),
            kind: d.kind,
            issue: d.issue.clone(),
            sha: d.sha.clone(),
            title: d.title.clone(),
            text: summary.to_owned(),
            preview: self.config.app.start.is_some(),
        };
        match self.services.channel.post_decision(&request) {
            Ok(m) => {
                d.status = DecisionStatus::Pending;
                d.message = Some(m.clone());
                self.save_decision(&d)?;
                if !details.is_empty()
                    && let Err(e) = self.services.channel.reply(&m, details)
                {
                    self.event(&json!({"event": "channel-error", "decision": d.id, "error": e.0}))?;
                }
                let attached = match self.document_of(&d) {
                    Ok(Some((name, content))) => self
                        .services
                        .channel
                        .attach(&m, &name, &content)
                        .map_err(|e| e.0),
                    Ok(None) => Ok(()),
                    Err(e) => Err(e.to_string()),
                };
                if let Err(e) = attached {
                    self.event(&json!({"event": "channel-error", "decision": d.id, "error": e}))?;
                }
                Ok(())
            }
            Err(e) => {
                self.event(&json!({"event": "channel-error", "decision": d.id, "error": e.0}))
            }
        }
    }

    /// The document a PRD or design approval asks about, as its file name and content
    /// in the issue's worktree, so the owner reads what they approve.
    fn document_of(&self, d: &DecisionRecord) -> Result<Option<(String, String)>, DaemonError> {
        let Some(key) = d.issue.as_deref() else {
            return Ok(None);
        };
        let Some(record) = self.issue(key)? else {
            return Ok(None);
        };
        let path = match d.kind {
            DecisionKind::Prd => match record.output(Step::Prd) {
                Some(StepOutput::Document(doc)) => doc.path,
                _ => None,
            },
            DecisionKind::Design => record.design_path(),
            DecisionKind::Sprint
            | DecisionKind::Spec
            | DecisionKind::Prototype
            | DecisionKind::Merge
            | DecisionKind::Review
            | DecisionKind::Recovery
            | DecisionKind::Stopped => None,
        };
        let Some(path) = path else {
            return Ok(None);
        };
        let file = self.worktree_of(key).join(&path);
        let content = std::fs::read_to_string(&file).map_err(StoreError::io(&file))?;
        let name = std::path::Path::new(&path)
            .file_name()
            .map_or_else(|| path.clone(), |n| n.to_string_lossy().into_owned());
        Ok(Some((name, content)))
    }

    pub(crate) fn deliver_decisions(&self) -> Result<(), DaemonError> {
        for d in self.decisions()? {
            if d.status == DecisionStatus::Unsent {
                self.send_decision(d)?;
            }
        }
        Ok(())
    }

    fn pending_for(
        &self,
        issue: &str,
        kind: DecisionKind,
    ) -> Result<Vec<DecisionRecord>, DaemonError> {
        Ok(self
            .decisions()?
            .into_iter()
            .filter(|d| {
                d.issue.as_deref() == Some(issue)
                    && d.kind == kind
                    && matches!(d.status, DecisionStatus::Unsent | DecisionStatus::Pending)
            })
            .collect())
    }

    pub(crate) fn ask_owner(&self, key: &str) -> Result<(), DaemonError> {
        let record = self.record_of(key)?;
        let step = if let Stage::Asking { step } = &record.flow.stage {
            step
        } else if matches!(record.flow.stage, Stage::LeadAsking { .. }) {
            &Step::Exception
        } else {
            return Ok(());
        };
        if !self.pending_for(key, DecisionKind::Spec)?.is_empty() {
            return Ok(());
        }
        let prepared = record
            .resume
            .as_ref()
            .filter(|r| r.step == *step)
            .and_then(|r| r.question.clone());
        let question = prepared.or_else(|| match record.output(*step) {
            Some(StepOutput::Requirements(r)) => r.question,
            Some(StepOutput::ScopeCheck(c)) => c.question,
            Some(StepOutput::Document(d)) => d.question,
            Some(StepOutput::Exception(e)) => e.question,
            _ => None,
        })
        .unwrap_or_else(|| {
            let result = record
                .last_item
                .as_deref()
                .and_then(|path| std::fs::read_to_string(path).ok())
                .and_then(|item| {
                    item.split_once("## Result")
                        .map(|(_, r)| r.trim().to_owned())
                })
                .filter(|r| !r.is_empty())
                .unwrap_or_else(|| "nothing".into());
            format!(
                "The lead could not find a way forward for {key}. Its last step recorded:\n\n{result}\n\nReply with how to proceed."
            )
        });
        let title = format!("{key}: {}", record.title);
        self.decide(DecisionRecord {
            text: question,
            asked_by: Some(*step),
            ..self.new_decision(DecisionKind::Spec, Some(key), title)?
        })
    }

    /// Asks the owner to approve what a design adds, a prototype, a held merge of
    /// the current head, or a PRD.
    pub(crate) fn request_approval(&mut self, key: &str) -> Result<(), DaemonError> {
        let record = self.record_of(key)?;
        let (kind, sha, categories, text) = match &record.flow.stage {
            Stage::AwaitingApproval {
                of: Approve::Design,
            } => {
                let before_built: Vec<Approval> = record
                    .flow
                    .approvals
                    .iter()
                    .filter(|a| a.is_approved_with_the_design())
                    .cloned()
                    .collect();
                let text = format!(
                    "This design includes {}, which needs your approval before it is built. The design is attached in this thread.",
                    names(&before_built),
                );
                (DecisionKind::Design, None, before_built, text)
            }
            Stage::AwaitingApproval { of: Approve::Prd } => (
                DecisionKind::Prd,
                None,
                vec![],
                "The requirements of this issue are written down and reviewed; the PRD is attached in this thread. Approve them for the work to go on, or reply with what to change.".to_owned(),
            ),
            Stage::AwaitingApproval {
                of: Approve::Prototype,
            } => (
                DecisionKind::Prototype,
                None,
                vec![],
                self.prototype_text(key, record.output(Step::Prototype)),
            ),
            Stage::HeldForOwner => self.merge_request_content(&record)?,
            Stage::Queued
            | Stage::Running { .. }
            | Stage::Asking { .. }
            | Stage::LeadAsking { .. }
            | Stage::Stopped { .. }
            | Stage::AwaitingRecovery { .. }
            | Stage::Reviewing
            | Stage::Committing { .. }
            | Stage::Pushing
            | Stage::Ci
            | Stage::WaitingBots
            | Stage::WaitingReview
            | Stage::Gate
            | Stage::BlockedOnGitHub
            | Stage::ReadyForOwner
            | Stage::Held
            | Stage::Merged
            | Stage::Dropped
            | Stage::Exception { .. } => return Ok(()),
        };
        let pending = self.pending_for(key, kind)?;
        if pending.iter().any(|d| d.sha == sha) {
            return Ok(());
        }
        for mut old in pending {
            // A new head makes an earlier merge request's basis stale.
            old.status = DecisionStatus::Superseded;
            if let Some(m) = &old.message
                && let Err(e) = self.services.channel.mark_resolved(
                    m,
                    "The branch changed after this was asked; a new request follows.",
                )
            {
                self.event(&json!({"event": "channel-error", "error": e.0}))?;
            }
            self.save_decision(&old)?;
        }
        let title = format!("{key}: {}", record.title);
        self.decide(DecisionRecord {
            sha,
            categories,
            text,
            ..self.new_decision(kind, Some(key), title)?
        })
    }

    fn merge_request_content(
        &self,
        record: &IssueRecord,
    ) -> Result<(DecisionKind, Option<String>, Vec<Approval>, String), DaemonError> {
        let sha = record.pushed_sha.clone();
        let approved = match &sha {
            Some(s) => self.owner_approved(record.key(), s)?,
            None => BTreeSet::new(),
        };
        let head = sha.clone().unwrap_or_else(|| "HEAD".into());
        let declared: BTreeSet<Approval> = record.flow.approvals.iter().cloned().collect();
        let to_approve: Vec<Approval> = self
            .held_categories(&self.worktree_of(record.key()), &head, &declared)?
            .difference(&approved)
            .cloned()
            .collect();
        let text = format!(
            "{} is ready to merge. It includes {}, so it waits for your approval. Pull request: {}",
            record.key(),
            names(&to_approve),
            record.pr_url.as_deref().unwrap_or_default()
        );
        Ok((DecisionKind::Merge, sha, to_approve, text))
    }

    /// Categories the owner approved for exactly this commit.
    pub(crate) fn owner_approved(
        &self,
        issue: &str,
        sha: &str,
    ) -> Result<BTreeSet<Approval>, DaemonError> {
        let mut out = BTreeSet::new();
        for id in self.store.isolated_names("approvals")? {
            let Some(a) = self
                .store
                .read_isolated::<ApprovalRecord>(&approval_path(&id))?
            else {
                continue;
            };
            if a.kind == DecisionKind::Merge
                && a.issue.as_deref() == Some(issue)
                && a.sha.as_deref() == Some(sha)
            {
                out.extend(a.categories);
            }
        }
        Ok(out)
    }

    /// Tells the owner what a design-phase step's preparation says they should know or prepare;
    /// the step works on without waiting.
    pub(crate) fn tell_owner(&self, key: &str, message: &str) -> Result<(), DaemonError> {
        let record = self.record_of(key)?;
        let notice = Notice {
            kind: NoticeKind::Preparation,
            issue: Some(key.into()),
            text: format!("{key}: {}\n\n{message}", record.title),
            preview: false,
            marker: None,
        };
        if let Err(e) = self.services.channel.post_notice(&notice) {
            self.event(&json!({"event": "channel-error", "issue": key, "error": e.0}))?;
        }
        self.event(&json!({"event": "told", "issue": key}))
    }

    /// Tells the owner the pull request waits for their merge.
    pub(crate) fn notify_ready(&self, key: &str) -> Result<(), DaemonError> {
        let record = self.record_of(key)?;
        // Whoever merges reviews it anyway; what the owner named says where to look closely.
        let named: Vec<String> = record
            .flow
            .approvals
            .iter()
            .filter_map(|a| match a {
                Approval::Named(name) => Some(match self.config.person_reviews.get(name) {
                    Some(words) => format!("\n- {name}: {words}"),
                    None => format!("\n- {name}"),
                }),
                Approval::Schema
                | Approval::ExternalConnection
                | Approval::Infrastructure
                | Approval::Ci
                | Approval::ExternalContract
                | Approval::FactoryConfiguration => None,
            })
            .collect();
        let look = if named.is_empty() {
            String::new()
        } else {
            format!("\nIt includes what a person reviews:{}", named.concat())
        };
        let notice = Notice {
            kind: NoticeKind::ReadyToMerge,
            issue: Some(key.into()),
            text: format!(
                "{key}: {} is ready to merge: {}{look}",
                record.title,
                record.pr_url.as_deref().unwrap_or_default()
            ),
            preview: self.config.app.start.is_some(),
            marker: None,
        };
        if let Err(e) = self.services.channel.post_notice(&notice) {
            self.event(&json!({"event": "channel-error", "issue": key, "error": e.0}))?;
        }
        Ok(())
    }

    pub(crate) fn take_channel_events(&mut self) -> Result<(), DaemonError> {
        // The channel hands each event over once: every one is taken in, and the first failure is
        // reported after the rest.
        let mut first_error = None;
        for event in self.services.channel.events() {
            let taken = self.take_channel_event(event);
            if let Err(e) = taken {
                first_error.get_or_insert(e);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn take_channel_event(&mut self, event: ChannelEvent) -> Result<(), DaemonError> {
        match event {
            ChannelEvent::Action {
                decision,
                button,
                user,
            } => self.on_action(&decision, button, &user)?,
            ChannelEvent::Reply {
                decision,
                text,
                user,
            } => self.on_reply(&decision, &text, &user)?,
            ChannelEvent::Command {
                name,
                args,
                user,
                at,
            } => {
                self.on_command(&Command {
                    name: &name,
                    args: &args,
                    user: &user,
                    at: at.as_ref(),
                })?;
            }
            ChannelEvent::Preview { issue, user, at } => self.preview(&issue, &user, at)?,
            ChannelEvent::Said {
                text,
                user,
                thread,
                mentioned,
            } => self.on_said(&Said {
                text: &text,
                user: &user,
                thread: &thread,
                mentioned,
            })?,
        }
        Ok(())
    }

    /// Decisions about `key` still open when what they asked about was settled another way (the
    /// issue resumed or ended): each is marked superseded and its message says so, so a later press
    /// does nothing. A merge request survives a resume, since the issue returns to wait on it.
    pub(crate) fn supersede_open_decisions(&self, key: &str) -> Result<(), DaemonError> {
        let ended = self.issue(key)?.is_none_or(|r| r.flow.stage.is_finished());
        for mut d in self.decisions()? {
            let open = matches!(d.status, DecisionStatus::Pending | DecisionStatus::Unsent);
            let about_the_stop = matches!(
                d.kind,
                DecisionKind::Spec | DecisionKind::Recovery | DecisionKind::Stopped
            );
            if !open || d.issue.as_deref() != Some(key) || !(ended || about_the_stop) {
                continue;
            }
            self.supersede(&mut d)?;
        }
        Ok(())
    }

    /// Marks an open decision superseded and says so on its message, so a later press does nothing.
    pub(crate) fn supersede(&self, d: &mut DecisionRecord) -> Result<(), DaemonError> {
        d.status = DecisionStatus::Superseded;
        if let Some(m) = &d.message
            && let Err(e) = self
                .services
                .channel
                .mark_resolved(m, "Settled another way; nothing is needed here.")
        {
            self.event(&json!({"event": "channel-error", "error": e.0}))?;
        }
        self.save_decision(d)?;
        self.event(&json!({"event": "decision-superseded", "id": d.id}))
    }

    pub(crate) fn is_owner(&self, user: &str) -> bool {
        self.config.slack.owners.iter().any(|u| u == user)
    }

    fn open_decision(&self, id: &str) -> Result<Option<DecisionRecord>, DaemonError> {
        let d: Option<DecisionRecord> = self.store.read_isolated(&decision_path(id))?;
        Ok(d.filter(|d| matches!(d.status, DecisionStatus::Pending | DecisionStatus::Unsent)))
    }

    pub(crate) fn resolve(
        &self,
        mut d: DecisionRecord,
        answer: Answer,
        note: &str,
    ) -> Result<DecisionRecord, DaemonError> {
        d.status = DecisionStatus::Answered;
        d.answer = Some(answer);
        self.save_decision(&d)?;
        if let Some(m) = &d.message
            && let Err(e) = self.services.channel.mark_resolved(m, note)
        {
            self.event(&json!({"event": "channel-error", "error": e.0}))?;
        }
        self.event(&json!({"event": "decision-answered", "id": d.id}))?;
        Ok(d)
    }

    /// Tells someone who is not an owner, privately, why their press did nothing.
    fn refuse_press(&self, user: &str, on: &str) -> Result<(), DaemonError> {
        if let Err(e) = self.services.channel.tell(
            user,
            "Only the owners set in Oven Spring's settings can answer this.",
        ) {
            self.event(&json!({"event": "channel-error", "error": e.0}))?;
        }
        self.event(&json!({"event": "ignored-action", "on": on, "user": user}))
    }

    fn on_action(&mut self, id: &str, button: Button, user: &str) -> Result<(), DaemonError> {
        if !self.is_owner(user) {
            return self.refuse_press(user, id);
        }
        let Some(d) = self.open_decision(id)? else {
            return Ok(());
        };
        match (button, d.kind, d.issue.clone()) {
            (Button::Approve, ..) => {}
            (Button::Hold, DecisionKind::Merge, Some(key)) => return self.hold(d, &key, user),
            (Button::Resolved, DecisionKind::Recovery | DecisionKind::Stopped, key) => {
                return self.resolved(d, key.as_deref(), user);
            }
            (
                Button::Cancel,
                DecisionKind::Spec | DecisionKind::Recovery | DecisionKind::Stopped,
                Some(key),
            ) => return self.cancel_from(d, &key, user),
            _ => {
                return self.event(
                    &json!({"event": "decision-action", "decision": id, "action": button.id()}),
                );
            }
        }
        let at = jiff::Timestamp::now().to_string();
        let said = if d.kind == DecisionKind::Review {
            format!("Sprint closed by <@{user}>.")
        } else {
            format!("Approved by <@{user}>.")
        };
        let d = self.resolve(
            d,
            Answer {
                kind: AnswerKind::Approve,
                text: None,
                user: user.into(),
                at: at.clone(),
            },
            &said,
        )?;
        self.store.write_isolated(
            &approval_path(&d.id),
            &ApprovalRecord {
                decision: d.id.clone(),
                kind: d.kind,
                issue: d.issue.clone(),
                sha: d.sha.clone(),
                categories: d.categories.clone(),
                user: user.into(),
                at,
                permalink: d.message.as_ref().and_then(|m| m.permalink.clone()),
            },
        )?;
        // The step that follows reads the approval where it reads a reply.
        let source = match d.kind {
            DecisionKind::Design
            | DecisionKind::Prototype
            | DecisionKind::Prd
            | DecisionKind::Sprint => Some(self.write_answer(&d, "Approved, with no message.")?),
            DecisionKind::Review => {
                Some(self.write_answer(&d, "Closed the sprint, with no message.")?)
            }
            DecisionKind::Spec
            | DecisionKind::Merge
            | DecisionKind::Recovery
            | DecisionKind::Stopped => None,
        };
        let Some(key) = d.issue.clone() else {
            // A sprint's proposal or review.
            return match d.kind {
                DecisionKind::Sprint | DecisionKind::Review => {
                    self.sprint_fact(&SprintFact::Approved)
                }
                // A blocked sprint step waits for the owner; pressing approve lets it run again.
                DecisionKind::Spec => self.sprint_fact(&SprintFact::Replied),
                DecisionKind::Recovery => self.run_recovery(&d, None),
                DecisionKind::Design
                | DecisionKind::Prototype
                | DecisionKind::Prd
                | DecisionKind::Merge
                | DecisionKind::Stopped => Ok(()),
            };
        };
        if let Some(source) = source {
            self.update_issue(&key, |r| r.answers.push(source))?;
        }
        match d.kind {
            DecisionKind::Design | DecisionKind::Prototype | DecisionKind::Prd => {
                self.apply(&key, Fact::OwnerAnswered)
            }
            DecisionKind::Recovery => self.run_recovery(&d, Some(&key)),
            // The approval is recorded; the pull request poll, which takes in new comments first,
            // runs the gate on the issue's current stage.
            DecisionKind::Merge
            | DecisionKind::Spec
            | DecisionKind::Sprint
            | DecisionKind::Review
            | DecisionKind::Stopped => Ok(()),
        }
    }

    /// Cancel issue on a message the issue waits on the owner with: the message says who
    /// canceled it, and the issue ends.
    fn cancel_from(&mut self, d: DecisionRecord, key: &str, user: &str) -> Result<(), DaemonError> {
        self.resolve(
            d,
            Answer {
                kind: AnswerKind::Cancel,
                text: None,
                user: user.into(),
                at: jiff::Timestamp::now().to_string(),
            },
            &format!("Issue canceled by <@{user}>."),
        )?;
        self.cancel_issue(key, "")
    }

    /// Writes the owner's answer to decision `d` where later items read it, and returns its path.
    fn write_answer(&self, d: &DecisionRecord, answer: &str) -> Result<String, DaemonError> {
        self.write_source(
            &format!("answers/{}.md", d.id),
            &format!(
                "# The owner's answer\n\nQuestion:\n\n{}\n\nAnswer:\n\n{answer}\n",
                d.text
            ),
        )
    }

    /// Keeps a held merge for the sprint review: the issue ends the sprint held,
    /// its pull request open.
    fn hold(&mut self, d: DecisionRecord, key: &str, user: &str) -> Result<(), DaemonError> {
        self.resolve(
            d,
            Answer {
                kind: AnswerKind::Hold,
                text: None,
                user: user.into(),
                at: jiff::Timestamp::now().to_string(),
            },
            &format!("Held for the sprint review by <@{user}>."),
        )?;
        self.apply(key, Fact::OwnerHeld)
    }

    /// Starts the issue's app from its worktree and replies in the pressed message's thread with
    /// its address. The start runs on its own thread so the factory keeps working while
    /// the app sets up; previews start one at a time, so a second press finds the first app.
    fn preview(&self, key: &str, user: &str, at: MessageRef) -> Result<(), DaemonError> {
        if !self.is_owner(user) {
            return self.refuse_press(user, key);
        }
        if self.config.app.start.is_none() {
            return self.preview_refused(
                key,
                &at,
                "`app.start` is not set in .ovenspring/config.toml",
            );
        }
        let worktree = self.worktree_of(key);
        let config = self.config.app.clone();
        self.event(&json!({"event": "preview", "issue": key, "user": user}))?;
        let state = self.store.root().to_path_buf();
        let channel = Arc::clone(&self.services.channel);
        let one_at_a_time = Arc::clone(&self.previews);
        let key = key.to_owned();
        std::thread::spawn(move || {
            let _turn = one_at_a_time.lock().unwrap_or_else(PoisonError::into_inner);
            let app = App {
                state: &state,
                issue: &key,
                worktree: &worktree,
                config: &config,
            };
            let text = match app.start(READY_WAIT) {
                // Slack links no bare localhost address, so the reply makes it a link.
                Ok(port) => {
                    format!("{key}'s preview: [http://localhost:{port}](http://localhost:{port})")
                }
                Err(e) => format!(
                    "{key}'s preview did not start: {}. Its log: `{}`",
                    e.to_string().lines().next().unwrap_or_default(),
                    app.log_path().display()
                ),
            };
            if let Err(e) = channel.reply(&at, &text) {
                eprintln!("ovenspring: {e}");
            }
        });
        Ok(())
    }

    /// M-04: where the owner opens the prototype, and how far the implementation follows it.
    /// The prototype's address stands alone on the last line, so it opens as a link of its own.
    fn prototype_text(&self, key: &str, output: Option<StepOutput>) -> String {
        let place = match &self.paths.dashboard {
            Some(base) => format!("{base}/issues/{key}/prototype"),
            None => "Open it from this issue's page in the dashboard.".to_owned(),
        };
        let Some(StepOutput::Prototype(p)) = output else {
            return format!("A prototype of the new screen is ready to open.\n\n{place}");
        };
        let meaning = match p.reference_strength {
            ReferenceStrength::Binding => {
                "the implementation follows how it looks, except where the UI Spec states otherwise"
            }
            ReferenceStrength::Reference => {
                "it shows the direction; only what the UI Spec records reaches the implementation"
            }
        };
        format!(
            "A prototype of the new screen is ready to open.\nIt is a {} prototype: {meaning}. {}\nApprove it for the UI Spec to be written from it, or reply with what to change.\n\n{place}",
            p.reference_strength.as_str(),
            p.basis.trim(),
        )
    }

    fn preview_refused(&self, key: &str, at: &MessageRef, why: &str) -> Result<(), DaemonError> {
        if let Err(e) = self
            .services
            .channel
            .reply(at, &format!("{key}'s preview cannot start: {why}."))
        {
            self.event(&json!({"event": "channel-error", "error": e.0}))?;
        }
        self.event(&json!({"event": "preview-refused", "issue": key, "why": why}))
    }

    fn on_reply(&mut self, id: &str, text: &str, user: &str) -> Result<(), DaemonError> {
        if !self.is_owner(user) {
            return self.event(&json!({"event": "ignored-reply", "decision": id, "user": user}));
        }
        let Some(d) = self.open_decision(id)? else {
            return Ok(());
        };
        let sprint_reply = d.issue.is_none();
        let fact = match d.kind {
            _ if sprint_reply => Fact::OwnerAnswered,
            // A reply to a recovery request or a stop goes to the lead with the reply.
            DecisionKind::Spec | DecisionKind::Recovery | DecisionKind::Stopped => {
                Fact::OwnerAnswered
            }
            DecisionKind::Design | DecisionKind::Prototype | DecisionKind::Prd => {
                Fact::OwnerReplied
            }
            // A merge request is answered with its buttons.
            DecisionKind::Merge | DecisionKind::Sprint | DecisionKind::Review => {
                return self.event(&json!({"event": "decision-reply", "decision": id}));
            }
        };
        let source = self.write_answer(&d, text)?;
        let key = d.issue.clone();
        self.resolve(
            d,
            Answer {
                kind: AnswerKind::Reply,
                text: Some(text.into()),
                user: user.into(),
                at: jiff::Timestamp::now().to_string(),
            },
            "Answered.",
        )?;
        let Some(key) = key.filter(|_| !sprint_reply) else {
            // The proposal, review or blocked step runs again with the reply as a source.
            return self.sprint_fact(&SprintFact::Replied);
        };
        self.update_issue(&key, |r| r.answers.push(source.clone()))?;
        self.apply(&key, fact)
    }

    /// `!pause [reason]` and `!resume` from an owner, confirmed in the command's thread; the
    /// channel notice is for the factory pausing itself, so an owner's own change does not post one.
    /// `!dashboard` answers with the dashboard's address, which the owner cannot otherwise find
    /// from Slack.
    fn on_command(&mut self, c: &Command<'_>) -> Result<(), DaemonError> {
        let Command {
            name,
            args,
            user,
            at,
        } = *c;
        if !self.is_owner(user) {
            return self.event(&json!({"event": "ignored-command", "command": name, "user": user}));
        }
        // Recorded before anything else: a start reads Slack from the last record on, so a command
        // taken without one would be taken, and answered, again.
        self.event(&json!({"event": "command", "command": name}))?;
        let answer = match name {
            "pause" => {
                let reason = if args.is_empty() {
                    "paused by the owner"
                } else {
                    args
                };
                self.pause(reason)?;
                "Paused. Agents already at work finish their step; nothing new starts until you mention me with `!resume`.".to_owned()
            }
            "resume" => {
                self.resume()?;
                "Resumed.".to_owned()
            }
            "dashboard" => match &self.paths.dashboard {
                // The address ends the line: Slack links punctuation that follows it.
                Some(base) => format!("The dashboard: {base}/"),
                None => "The dashboard is not running.".to_owned(),
            },
            _ => return Ok(()),
        };
        self.pause_announced = self.halt_file().exists();
        if let Some(m) = at
            && let Err(e) = self.services.channel.reply(m, &answer)
        {
            self.event(&json!({"event": "channel-error", "error": e.0}))?;
        }
        Ok(())
    }

    /// Once every `timing.remind`, while anything delivered waits on the owner, one message lists
    /// it all with where to answer. An owner who still does not answer leaves the work waiting.
    pub(crate) fn remind_owner(&mut self) -> Result<(), DaemonError> {
        let now = Instant::now();
        if now < self.next_reminder_check {
            return Ok(());
        }
        let interval = self.timing.remind;
        let wall_now = SystemTime::now();
        let mut next_check = interval;
        let mut due = |since| {
            let age = wall_now.duration_since(since).unwrap_or_default();
            if age >= interval {
                return true;
            }
            next_check = next_check.min(interval.saturating_sub(age));
            false
        };

        let mut pending = Vec::new();
        for decision in self.decisions()? {
            if decision.status == DecisionStatus::Pending {
                // Successful delivery writes the pending record; its next write ends the wait.
                let path = self.store.isolated().join(decision_path(&decision.id));
                let delivered = path
                    .metadata()
                    .and_then(|m| m.modified())
                    .map_err(StoreError::io(&path))?;
                if due(delivered) {
                    pending.push(decision);
                }
            }
        }
        let mut issues = self.issues()?;
        if issues.iter().any(|r| r.flow.stage == Stage::ReadyForOwner) {
            let events = crate::records::fact_events(&self.store)?;
            issues.retain(|r| {
                r.flow.stage != Stage::ReadyForOwner
                    || events
                        .iter()
                        .rev()
                        .find(|e| {
                            e.issue == r.key() && e.to == Stage::ReadyForOwner && e.from != e.to
                        })
                        .and_then(|e| e.at.parse::<jiff::Timestamp>().ok())
                        .and_then(|at| SystemTime::try_from(at).ok())
                        .is_none_or(&mut due)
            });
        }
        let purpose = self.sprint()?.and_then(|s| s.plan).map(|p| p.purpose);
        let lines: Vec<String> = crate::overview::waiting(&pending, &issues, purpose.as_deref())
            .into_iter()
            .filter_map(|w| {
                let link = w.link?;
                let what = match w.issue {
                    Some(key) => format!("{key}: {}", w.title),
                    None => w.title,
                };
                Some(format!("- [{what}]({link})"))
            })
            .collect();
        if lines.is_empty() {
            self.next_reminder_check = now + next_check;
            return Ok(());
        }
        self.next_reminder_check = now + interval;
        let notice = Notice {
            kind: NoticeKind::Reminder,
            issue: None,
            text: format!("Still waiting for you:\n{}", lines.join("\n")),
            preview: false,
            marker: None,
        };
        if let Err(e) = self.services.channel.post_notice(&notice) {
            self.event(&json!({"event": "channel-error", "error": e.0}))?;
        }
        self.event(&json!({"event": "reminded", "waiting": lines.len()}))
    }

    /// Posts M-08 once when the factory has paused itself; an owner's own `!pause` and `!resume`
    /// are confirmed in their thread instead.
    pub(crate) fn announce_pause(&mut self) -> Result<(), DaemonError> {
        let halted = self.halt_file().exists();
        if !halted || self.pause_announced {
            self.pause_announced = halted;
            return Ok(());
        }
        let halt = self.halt_file();
        let reason = std::fs::read_to_string(&halt).map_err(StoreError::io(&halt))?;
        let notice = Notice {
            kind: NoticeKind::Paused,
            issue: None,
            text: pause_text(&reason),
            preview: false,
            marker: None,
        };
        match self.services.channel.post_notice(&notice) {
            Ok(_) => {
                self.pause_announced = true;
                Ok(())
            }
            Err(e) => self.event(&json!({"event": "channel-error", "error": e.0})),
        }
    }
}

/// M-08: why the factory paused, and how to start it again.
pub(crate) fn pause_text(reason: &str) -> String {
    format!(
        "Oven Spring paused because {}.\nAgents already at work finish their step; nothing new starts. To start again, mention me with `!resume`.",
        reason.trim()
    )
}

/// A `!command` as the daemon handles it.
#[derive(Clone, Copy)]
struct Command<'a> {
    name: &'a str,
    args: &'a str,
    user: &'a str,
    at: Option<&'a MessageRef>,
}

fn names(approvals: &[Approval]) -> String {
    if approvals.is_empty() {
        return "nothing that needs approval".into();
    }
    approvals
        .iter()
        .map(|a| match a {
            Approval::Schema => "a schema change".to_owned(),
            Approval::ExternalConnection => "a new external connection".to_owned(),
            Approval::Infrastructure => "an infrastructure change".to_owned(),
            Approval::Ci => "a CI change".to_owned(),
            Approval::ExternalContract => "an external contract change".to_owned(),
            Approval::FactoryConfiguration => {
                "a change to the factory's configuration or agent instructions".to_owned()
            }
            Approval::Named(name) => format!("{name}, which a person reviews"),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// A text for the owner split where its details begin: at the first Markdown heading.
fn split_details(text: &str) -> (&str, &str) {
    let mut at = 0;
    // A `#` line inside a fenced block (a shell comment in a recovery procedure) is no heading.
    let mut fenced = false;
    for line in text.split_inclusive('\n') {
        if line.starts_with("```") {
            fenced = !fenced;
        }
        if line.starts_with('#') && !fenced {
            let (summary, details) = text.split_at(at);
            return (summary.trim(), details.trim());
        }
        at += line.len();
    }
    (text.trim(), "")
}

#[cfg(test)]
mod tests {
    use super::split_details;

    #[test]
    fn the_details_start_at_the_first_heading() {
        assert_eq!(
            split_details(
                "Approve the plan.\n\n1. Ship export first.\n\n## 1. Export\nWhy.\n## 2. More\n"
            ),
            (
                "Approve the plan.\n\n1. Ship export first.",
                "## 1. Export\nWhy.\n## 2. More"
            )
        );
        assert_eq!(
            split_details("Only a conclusion.\n"),
            ("Only a conclusion.", "")
        );
        assert_eq!(
            split_details("Run this.\n\n```sh\n# fetch first\ngit fetch\n```\n"),
            ("Run this.\n\n```sh\n# fetch first\ngit fetch\n```", "")
        );
    }
}
