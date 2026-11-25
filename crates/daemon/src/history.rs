//! What happened, read back from the records: each launch with its end, one issue in depth and the
//! sprints that ended.

use std::collections::BTreeMap;
use std::fs;

use ovenspring_core::config::{Cli, Role};
use ovenspring_core::flow::{Origin, Stage};
use ovenspring_core::item::{Item, Status};
use ovenspring_core::output::{
    CheckResult, Decision, Disposition, KeyResult, ReferenceStrength, Scale, ScopeCheckOutput,
    StepOutput,
};
use ovenspring_core::phase::Phase;
use ovenspring_core::sprint::SprintState;
use ovenspring_core::step::Step;
use ovenspring_store::{Store, StoreError};
use serde::Serialize;
use serde_json::Value;
use ts_rs::TS;

use crate::DaemonError;
use crate::issues::IssueRecord;
use crate::overview::{Around, Ending, Position, Section, Times, Waiting, position, waiting};
use crate::owner::{DecisionRecord, DecisionStatus};
use crate::records::{self, LaunchLine};

/// One launch: its start, and its end once it ended.
#[derive(Debug, Clone, PartialEq)]
struct Launch {
    item: String,
    attempt: u32,
    step: Step,
    phase: Option<Phase>,
    role: Role,
    issue: Option<String>,
    sprint: Option<String>,
    cli: Cli,
    model: Option<String>,
    effort: Option<String>,
    started_at: String,
    ended_at: Option<String>,
    /// Why it failed, when it did.
    failure: Option<String>,
    cost_usd: Option<f64>,
}

/// Every launch in `launches.jsonl`, oldest first, each with its end when it ended.
fn launches(store: &Store) -> Result<Vec<Launch>, DaemonError> {
    let mut out: Vec<Launch> = Vec::new();
    for line in records::launch_lines(store)? {
        match line {
            LaunchLine::Start(start) => out.push(Launch {
                item: start.item,
                attempt: start.attempt,
                step: start.step,
                phase: start.phase,
                role: start.role,
                issue: start.issue,
                sprint: start.sprint,
                cli: start.cli,
                model: start.model,
                effort: start.effort,
                started_at: start.at,
                ended_at: None,
                failure: None,
                cost_usd: None,
            }),
            LaunchLine::End(end) => {
                if let Some(l) = out
                    .iter_mut()
                    .rev()
                    .find(|l| l.item == end.item && l.attempt == end.attempt)
                {
                    l.ended_at = Some(end.at);
                    l.failure = end.failure;
                    l.cost_usd = end.cost_usd;
                    // The model that ran, where the role left it to the CLI.
                    if end.model.is_some() {
                        l.model = end.model;
                    }
                }
            }
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct IssueDetail {
    pub key: String,
    pub title: String,
    pub tracker: Option<String>,
    pub pull_request: Option<String>,
    /// The reference strength of the issue's prototype, once it has one.
    pub prototype: Option<ReferenceStrength>,
    pub now: Position,
    /// What the owner is asked about this issue, while they are.
    pub decision: Option<Waiting>,
    pub scale: Option<Scale>,
    /// What the scale was judged from: the requirements' cost drivers.
    pub scale_evidence: Vec<String>,
    /// The outcome the issue is to reach, from its requirements.
    pub outcome: Option<String>,
    pub non_goals: Vec<String>,
    pub path: Vec<PhaseSpan>,
    pub findings: Vec<FindingLine>,
    /// Why the owner canceled it, in their words; empty when they gave none.
    pub canceled: Option<String>,
    #[ts(type = "number")]
    pub launches: usize,
    pub cost_usd: f64,
    /// Ended launches whose CLI reported no cost.
    #[ts(type = "number")]
    pub uncosted: usize,
}

/// One of the five phases on the issue's path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PhaseSpan {
    pub phase: Phase,
    pub state: PhaseState,
    /// The first launch in the phase.
    pub started_at: Option<String>,
    /// The last launch in the phase to end.
    pub ended_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum PhaseState {
    Done,
    Now,
    Ahead,
    Skipped,
}

/// One review finding and how it was answered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FindingLine {
    pub source: FindingSource,
    pub text: String,
    /// None while unanswered.
    pub disposition: Option<Disposition>,
    pub answer: Option<String>,
}

/// Who raised a finding: one of the factory's reviews, or a person's comment on the pull request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum FindingSource {
    Review { review: Step, round: u32 },
    Comment { author: String },
}

/// The issue's items, active and archived, in the order they first launched; an item's name
/// only orders to the second, and several are filed within one.
fn items_of(
    store: &Store,
    key: &str,
    launches: &[Launch],
) -> Result<Vec<(String, Item)>, DaemonError> {
    let mut out = Vec::new();
    for dir in [store.archive_dir(), store.active_dir()] {
        let entries = fs::read_dir(&dir).map_err(StoreError::io(&dir))?;
        for entry in entries {
            let path = entry.map_err(StoreError::io(&dir))?.path();
            let name = ovenspring_store::item_name(&path);
            if !name.contains(key) {
                continue;
            }
            let item = store.read_item(&path)?;
            if item.front.issue.as_deref() == Some(key) {
                out.push((name, item));
            }
        }
    }
    let first = |name: &str| {
        launches
            .iter()
            .position(|l| l.item == name)
            .unwrap_or(usize::MAX)
    };
    out.sort_by_key(|(name, _)| (first(name), name.clone()));
    Ok(out)
}

fn output_of(item: &Item) -> Option<StepOutput> {
    StepOutput::parse(item.front.step, raw_of(item)?).ok()
}

fn raw_of(item: &Item) -> Option<Value> {
    serde_json::from_str(&fs::read_to_string(&item.front.out).ok()?).ok()
}

fn text_at(v: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|k| v.get(*k).and_then(Value::as_str))
        .map(String::from)
}

/// Findings with their dispositions: each review's findings as its source reports them, settled
/// by the latest `resolution` that names their id. A comment on the pull request becomes a
/// finding when a resolution answers it.
fn findings(items: &[(String, Item)], comments: &[Value]) -> Vec<FindingLine> {
    let mut out: Vec<(String, FindingLine)> = Vec::new();
    for (_, item) in items {
        let round = item.front.round.unwrap_or(1);
        let Some(raw) = raw_of(item) else {
            continue;
        };
        if item.front.step == Step::Resolution {
            for d in raw
                .get("dispositions")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(id) = d.get("id").and_then(Value::as_str) else {
                    continue;
                };
                let disposition = d
                    .get("disposition")
                    .and_then(|v| serde_json::from_value(v.clone()).ok());
                let answer = text_at(d, &["reason", "evidence"]);
                if let Some((_, line)) = out
                    .iter_mut()
                    .rev()
                    .find(|(fid, l)| fid == id && l.disposition.is_none())
                {
                    line.disposition = disposition;
                    line.answer = answer;
                } else if let Some(c) = comments
                    .iter()
                    .find(|c| c.get("id").and_then(Value::as_str) == Some(id))
                {
                    out.push((
                        id.to_owned(),
                        FindingLine {
                            source: FindingSource::Comment {
                                author: text_at(c, &["author"]).unwrap_or_default(),
                            },
                            text: text_at(c, &["description"]).unwrap_or_default(),
                            disposition,
                            answer,
                        },
                    ));
                }
            }
            continue;
        }
        if output_of(item).and_then(|o| o.check_result()).is_none() {
            continue;
        }
        for key in crate::handoff::FINDING_KEYS {
            for f in raw.get(key).and_then(Value::as_array).into_iter().flatten() {
                let what = text_at(f, &["description", "claim"]).unwrap_or_default();
                let fix = text_at(f, &["suggestion", "correction"]).unwrap_or_default();
                out.push((
                    text_at(f, &["id"]).unwrap_or_default(),
                    FindingLine {
                        source: FindingSource::Review {
                            review: item.front.step,
                            round,
                        },
                        text: format!("{what} {fix}").trim().to_owned(),
                        disposition: None,
                        answer: None,
                    },
                ));
            }
        }
    }
    out.into_iter().map(|(_, l)| l).collect()
}

/// The five phases as this issue takes them: done before its current phase, ahead after it, and
/// skipped when its scale or its end left a phase out.
fn path(record: &IssueRecord, now: &Position, launches: &[Launch]) -> Vec<PhaseSpan> {
    let scale = record.flow.scale;
    let current = now.phase.filter(|_| now.section != Section::Done);
    Phase::ALL
        .into_iter()
        .map(|phase| {
            let runs: Vec<&Launch> = launches.iter().filter(|l| l.phase == Some(phase)).collect();
            let ran = !runs.is_empty();
            let taken = phase.taken_at(scale);
            let state = match (current, now.ended) {
                (Some(c), _) if c == phase => PhaseState::Now,
                (Some(c), _) if phase < c => {
                    if ran || taken {
                        PhaseState::Done
                    } else {
                        PhaseState::Skipped
                    }
                }
                (Some(_), _) | (None, None) => {
                    if taken {
                        PhaseState::Ahead
                    } else {
                        PhaseState::Skipped
                    }
                }
                (None, Some(Ending::Merged)) if ran || taken => PhaseState::Done,
                (None, Some(Ending::Held)) if phase == Phase::Merge => PhaseState::Ahead,
                (None, Some(_)) if ran => PhaseState::Done,
                (None, Some(_)) => PhaseState::Skipped,
            };
            PhaseSpan {
                phase,
                state,
                started_at: runs.iter().map(|l| l.started_at.clone()).min(),
                ended_at: runs.iter().filter_map(|l| l.ended_at.clone()).max(),
            }
        })
        .collect()
}

pub fn issue_detail(store: &Store, key: &str) -> Result<Option<IssueDetail>, DaemonError> {
    let Some(record) = records::issue(store, key)? else {
        return Ok(None);
    };
    let all = records::issues(store)?;
    let pending: Vec<DecisionRecord> = records::decisions(store)?
        .into_iter()
        .filter(|d| matches!(d.status, DecisionStatus::Unsent | DecisionStatus::Pending))
        .collect();
    let now = position(
        &record,
        &Around {
            issues: &all,
            pending: &pending,
            paused: records::pause_reason(store)?.is_some(),
            times: Times::read(store)?,
        },
    );
    let launches: Vec<Launch> = launches(store)?
        .into_iter()
        .filter(|l| l.issue.as_deref() == Some(key))
        .collect();
    let items = items_of(store, key, &launches)?;
    let requirements = items
        .iter()
        .rev()
        .find(|(_, i)| i.front.step == Step::Requirements)
        .and_then(|(_, i)| raw_of(i));
    let convergence = requirements.as_ref().and_then(|r| r.get("convergence"));
    Ok(Some(IssueDetail {
        key: key.to_owned(),
        title: record.title.clone(),
        tracker: record.url.clone(),
        pull_request: record.pr_url.clone(),
        prototype: match record.output(Step::Prototype) {
            Some(StepOutput::Prototype(p)) => Some(p.reference_strength),
            _ => None,
        },
        decision: waiting(&pending, std::slice::from_ref(&record), None)
            .into_iter()
            .find(|w| w.issue.as_deref() == Some(key)),
        canceled: record.canceled.clone(),
        scale: record.flow.scale,
        scale_evidence: requirements
            .as_ref()
            .and_then(|r| r.pointer("/costEvidence/drivers"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|d| text_at(d, &["fact"]))
            .collect(),
        outcome: convergence.and_then(|c| text_at(c, &["outcome"])),
        non_goals: convergence
            .and_then(|c| c.get("nonGoals"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|g| {
                g.as_str()
                    .map(String::from)
                    .or_else(|| text_at(g, &["text"]))
            })
            .collect(),
        path: path(&record, &now, &launches),
        now,
        findings: findings(
            &items,
            record
                .findings
                .get("comments")
                .map_or(&[][..], Vec::as_slice),
        ),
        launches: launches.len(),
        cost_usd: launches.iter().filter_map(|l| l.cost_usd).sum(),
        uncosted: launches
            .iter()
            .filter(|l| l.ended_at.is_some() && l.cost_usd.is_none())
            .count(),
    }))
}

/// The issue's prototype, the HTML file its `prototype` output names inside the issue's
/// worktree; `None` while there is none or the file is gone. A path leading out of the worktree
/// is not read.
pub fn prototype_html(store: &Store, key: &str) -> Result<Option<String>, DaemonError> {
    let Some(record) = records::issue(store, key)? else {
        return Ok(None);
    };
    let Some(StepOutput::Prototype(p)) = record.output(Step::Prototype) else {
        return Ok(None);
    };
    let Ok(worktree) = store.worktree(key).canonicalize() else {
        return Ok(None);
    };
    let Ok(file) = worktree.join(&p.path).canonicalize() else {
        return Ok(None);
    };
    if !file.starts_with(&worktree) {
        return Ok(None);
    }
    Ok(fs::read_to_string(file).ok())
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EndedSprint {
    pub id: String,
    pub purpose: String,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub key_results: Vec<KeyResult>,
    /// The working-agreement files the review changed.
    pub agreements: Vec<String>,
    /// The issues merged, dropped and held for the review, by key.
    pub merged: Vec<String>,
    pub dropped: Vec<String>,
    pub held: Vec<String>,
    #[ts(type = "number")]
    pub launches: usize,
    pub cost_usd: f64,
    /// Ended launches whose CLI reported no cost.
    #[ts(type = "number")]
    pub uncosted: usize,
}

/// The sprints that ended, newest first.
pub fn ended_sprints(store: &Store) -> Result<Vec<EndedSprint>, DaemonError> {
    let issues = records::issues(store)?;
    let launches = launches(store)?;
    let sprint_of: BTreeMap<&str, &str> = issues
        .iter()
        .filter_map(|r| Some((r.key(), r.sprint.as_deref()?)))
        .collect();
    let mut out = Vec::new();
    for id in store.record_names("sprints")? {
        let Some(s) = store.read_record::<SprintState>(&Store::sprint_record(&id))? else {
            continue;
        };
        let mine: Vec<_> = issues
            .iter()
            .filter(|r| r.sprint.as_deref() == Some(s.id.as_str()))
            .collect();
        let keys = |stage: &Stage| {
            mine.iter()
                .filter(|r| &r.flow.stage == stage)
                .map(|r| r.key().to_owned())
                .collect()
        };
        let spent: Vec<&Launch> = launches
            .iter()
            .filter(|l| {
                l.sprint.as_deref() == Some(s.id.as_str())
                    || l.issue.as_deref().and_then(|k| sprint_of.get(k)) == Some(&s.id.as_str())
            })
            .collect();
        let review = s.review.clone();
        out.push(EndedSprint {
            purpose: s
                .plan
                .as_ref()
                .map(|p| p.purpose.clone())
                .unwrap_or_default(),
            started_at: s.started_at.clone(),
            ended_at: s.ended_at.clone(),
            key_results: review
                .as_ref()
                .map(|r| r.key_results.clone())
                .unwrap_or_default(),
            agreements: review
                .map(|r| {
                    r.agreements
                        .iter()
                        .map(|a| a.file.as_str().to_owned())
                        .collect()
                })
                .unwrap_or_default(),
            merged: keys(&Stage::Merged),
            dropped: keys(&Stage::Dropped),
            held: keys(&Stage::Held),
            launches: spent.len(),
            cost_usd: spent.iter().filter_map(|l| l.cost_usd).sum(),
            uncosted: spent
                .iter()
                .filter(|l| l.ended_at.is_some() && l.cost_usd.is_none())
                .count(),
            id: s.id,
        });
    }
    out.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(out)
}

/// One launch as Activity and the issue page list it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LaunchRow {
    /// `<item>.<attempt>`, the launch's log name.
    pub id: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub issue: Option<String>,
    pub role: Role,
    pub step: Step,
    /// The issue's phase when it started.
    pub phase: Option<Phase>,
    pub cli: Cli,
    pub result: RunResult,
    /// None while it runs, and when its CLI reports no cost.
    pub cost_usd: Option<f64>,
    pub detail: RunDetail,
}

/// What a run's item and output add to its line, as far as they are known.
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RunDetail {
    /// The review round, from the second on.
    pub round: Option<u32>,
    /// What a check found.
    pub found: Option<usize>,
    /// What a resolution applied and declined.
    pub applied: Option<usize>,
    pub declined: Option<usize>,
    /// The findings a resolution under way is judging.
    pub judging: Option<usize>,
    /// The scale the requirements judged.
    pub scale: Option<Scale>,
    /// The requirements found the issue needs no change, and closed it.
    pub closed: bool,
    /// The tasks a decompose wrote.
    pub tasks: Option<usize>,
    /// The task an implement carried out.
    pub task: Option<TaskRef>,
    /// The documents a scope check chose, in the order they are written; none for a small change.
    pub documents: Option<Vec<DocumentKind>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum DocumentKind {
    Prototype,
    UiSpec,
    Adr,
    DesignDoc,
}

/// The documents a scope check's answer leads to: a small change goes straight to building; a UI
/// Spec comes with its prototype; decision points the owner answered add an ADR.
fn chosen(c: &ScopeCheckOutput) -> Vec<DocumentKind> {
    if c.structural_scale == Scale::Small {
        return Vec::new();
    }
    let mut out = Vec::new();
    if c.ui_spec {
        out.extend([DocumentKind::Prototype, DocumentKind::UiSpec]);
    }
    if !c.adr_decision_points.is_empty() {
        out.push(DocumentKind::Adr);
    }
    out.push(DocumentKind::DesignDoc);
    out
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct TaskRef {
    pub number: usize,
    pub of: usize,
    /// The task file's heading.
    pub name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum RunResult {
    Running,
    Done,
    /// A check that found nothing.
    Pass,
    /// A check that found something to answer.
    Findings,
    Failed,
    /// Failed on an overloaded provider and ran again.
    Retried,
}

/// One launch in full: how it ran and what the agent said.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LaunchDetail {
    #[serde(flatten)]
    pub row: LaunchRow,
    pub model: Option<String>,
    pub effort: Option<String>,
    /// Why it failed, as the CLI reported it.
    pub failure: Option<String>,
    /// What the agent said, in order, read from its output so far.
    pub messages: Vec<String>,
}

fn id_of(l: &Launch) -> String {
    format!("{}.{}", l.item, l.attempt)
}

fn item_named(store: &Store, name: &str) -> Option<Item> {
    store.find_item(name).and_then(|p| store.read_item(&p).ok())
}

fn result_of(store: &Store, l: &Launch, all: &[Launch]) -> RunResult {
    if l.ended_at.is_none() {
        return RunResult::Running;
    }
    if l.failure.is_some() {
        let retried = all
            .iter()
            .any(|o| o.item == l.item && o.attempt > l.attempt);
        return if retried {
            RunResult::Retried
        } else {
            RunResult::Failed
        };
    }
    // A check's launch shows what it found, read from its output.
    let found = item_named(store, &l.item).and_then(|i| {
        let output = output_of(&i)?;
        if let StepOutput::AgreementsCheck(c) = &output {
            return Some(c.findings.is_empty());
        }
        output.check_result().map(|r| r == CheckResult::Clear)
    });
    match found {
        Some(true) => RunResult::Pass,
        Some(false) => RunResult::Findings,
        None => RunResult::Done,
    }
}

/// The findings the issue's `resolution` judges: the new findings of the round's reviews, of the
/// one check that raised them, or the pull request's new comments, as each last reported them.
fn judging(record: &IssueRecord) -> Option<usize> {
    let keys: Vec<String> = match record.flow.origin? {
        Origin::Round => record
            .flow
            .round
            .findings
            .iter()
            .map(|r| String::from(*r))
            .collect(),
        Origin::PrComments => vec![crate::handoff::COMMENTS.to_owned()],
        Origin::Document | Origin::TestReview => vec![record.flow.check?.step().to_string()],
    };
    Some(
        keys.iter()
            .filter_map(|k| record.findings.get(k))
            .map(Vec::len)
            .sum(),
    )
}

fn detail(item: Option<&Item>, record: Option<&IssueRecord>) -> RunDetail {
    let Some(item) = item else {
        return RunDetail::default();
    };
    let mut d = RunDetail {
        round: item.front.round.filter(|r| *r > 1),
        ..RunDetail::default()
    };
    match output_of(item) {
        Some(StepOutput::DocumentReview(r)) => d.found = Some(r.issues.len()),
        Some(StepOutput::Verifier(v)) => d.found = Some(v.discrepancies.len()),
        Some(StepOutput::TestReview(r)) => d.found = Some(r.quality_issues.len()),
        Some(StepOutput::CodeReview(r)) => d.found = Some(r.findings.len()),
        Some(StepOutput::SecurityReview(r)) => d.found = Some(r.findings.len()),
        Some(StepOutput::AgreementsCheck(c)) => d.found = Some(c.findings.len()),
        Some(StepOutput::Resolution(r)) => {
            let applied = r
                .dispositions
                .iter()
                .filter(|f| f.disposition == Disposition::Apply)
                .count();
            d.applied = Some(applied);
            d.declined = Some(r.dispositions.len() - applied);
        }
        Some(StepOutput::Requirements(r)) => {
            d.closed = r.decision == Decision::Close;
            d.scale = Some(r.structural_scale);
        }
        Some(StepOutput::ScopeCheck(c)) if c.decision != Decision::Ask => {
            d.documents = Some(chosen(&c));
        }
        _ => {}
    }
    if item.front.step == Step::Resolution
        && matches!(item.front.status, Status::Open | Status::Doing)
    {
        d.judging = record.and_then(judging);
    }
    let tasks = record.map_or(&[][..], |r| r.flow.tasks.as_slice());
    if item.front.step == Step::Decompose && !tasks.is_empty() {
        d.tasks = Some(tasks.len());
    }
    if item.front.step == Step::Implement {
        d.task = tasks
            .iter()
            .enumerate()
            .find(|(_, t)| item.body.contains(&t.path))
            .map(|(i, t)| TaskRef {
                number: i + 1,
                of: tasks.len(),
                name: heading_of(&t.path),
            });
    }
    d
}

/// A task file's first heading, without a leading "Task <n>:".
fn heading_of(path: &str) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let heading = text.lines().find_map(|l| l.strip_prefix("# "))?.trim();
    let name = heading
        .split_once(':')
        .filter(|(before, _)| before.trim_start().starts_with("Task"))
        .map_or(heading, |(_, after)| after.trim());
    Some(name.to_owned())
}

fn row(store: &Store, l: &Launch, all: &[Launch], record: Option<&IssueRecord>) -> LaunchRow {
    LaunchRow {
        id: id_of(l),
        started_at: l.started_at.clone(),
        ended_at: l.ended_at.clone(),
        issue: l.issue.clone(),
        role: l.role,
        step: l.step,
        phase: l.phase,
        cli: l.cli,
        result: result_of(store, l, all),
        cost_usd: l.cost_usd,
        detail: detail(item_named(store, &l.item).as_ref(), record),
    }
}

/// The launches of a sprint (its own items and its issues') or of one issue, newest first.
pub fn launch_rows(
    store: &Store,
    sprint: Option<&str>,
    issue: Option<&str>,
) -> Result<Vec<LaunchRow>, DaemonError> {
    let all = launches(store)?;
    let records: BTreeMap<String, IssueRecord> = records::issues(store)?
        .into_iter()
        .map(|r| (r.key().to_owned(), r))
        .collect();
    let sprint_of: BTreeMap<&str, &str> = records
        .iter()
        .filter_map(|(k, r)| Some((k.as_str(), r.sprint.as_deref()?)))
        .collect();
    let in_sprint = |l: &Launch, id: &str| {
        l.sprint.as_deref() == Some(id)
            || l.issue
                .as_ref()
                .and_then(|k| sprint_of.get(k.as_str()))
                .copied()
                == Some(id)
    };
    let mut rows: Vec<LaunchRow> = all
        .iter()
        .filter(|l| sprint.is_none_or(|id| in_sprint(l, id)))
        .filter(|l| issue.is_none_or(|k| l.issue.as_deref() == Some(k)))
        .map(|l| {
            row(
                store,
                l,
                &all,
                l.issue.as_ref().and_then(|k| records.get(k)),
            )
        })
        .collect();
    // Newest first by when each run ended, the time Activity shows; a running one is newest.
    rows.reverse();
    rows.sort_by(|a, b| match (&a.ended_at, &b.ended_at) {
        (None, None) => b.started_at.cmp(&a.started_at),
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
        (Some(x), Some(y)) => y.cmp(x),
    });
    Ok(rows)
}

/// The log of the launch `id`, only for a launch the records name.
pub fn launch_log(store: &Store, id: &str) -> Result<Option<String>, DaemonError> {
    if !launches(store)?.iter().any(|l| id_of(l) == id) {
        return Ok(None);
    }
    let path = store.root().join("launches").join(format!("{id}.log"));
    match fs::read_to_string(&path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Some(String::new())),
        Err(e) => Err(StoreError::io(&path)(e).into()),
    }
}

/// What the agent said in `item`'s latest launch, in order, while it runs and after.
pub fn latest_messages(store: &Store, item: &str) -> Result<Vec<String>, DaemonError> {
    let Some(l) = launches(store)?.into_iter().rev().find(|l| l.item == item) else {
        return Ok(Vec::new());
    };
    let log = launch_log(store, &id_of(&l))?.unwrap_or_default();
    Ok(ovenspring_agents::backend::read_stream(l.cli, &log).messages)
}

pub fn latest_failure(store: &Store, item: &str) -> Result<Option<String>, DaemonError> {
    Ok(launches(store)?
        .into_iter()
        .rev()
        .find(|l| l.item == item)
        .and_then(|l| l.failure))
}

pub fn launch_detail(store: &Store, id: &str) -> Result<Option<LaunchDetail>, DaemonError> {
    let all = launches(store)?;
    let Some(l) = all.iter().find(|l| id_of(l) == id) else {
        return Ok(None);
    };
    let log = launch_log(store, id)?.unwrap_or_default();
    let record = match &l.issue {
        Some(key) => records::issue(store, key)?,
        None => None,
    };
    Ok(Some(LaunchDetail {
        row: row(store, l, &all, record.as_ref()),
        model: l.model.clone(),
        effort: l.effort.clone(),
        failure: l.failure.clone(),
        messages: ovenspring_agents::backend::read_stream(l.cli, &log).messages,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(scale: Scale, ui_spec: bool, adr: bool) -> ScopeCheckOutput {
        ScopeCheckOutput {
            decision: Decision::Proceed,
            structural_scale: scale,
            ui_spec,
            adr_decision_points: if adr { vec![Value::Null] } else { vec![] },
            question: None,
        }
    }

    #[test]
    fn a_scope_check_names_the_documents_its_answer_leads_to() {
        use DocumentKind::{Adr, DesignDoc, Prototype, UiSpec};
        assert_eq!(chosen(&scope(Scale::Small, true, true)), vec![]);
        assert_eq!(chosen(&scope(Scale::Medium, false, false)), vec![DesignDoc]);
        assert_eq!(
            chosen(&scope(Scale::Large, true, true)),
            vec![Prototype, UiSpec, Adr, DesignDoc]
        );
    }

    fn with_findings(origin: Origin, findings: &[(&str, usize)]) -> IssueRecord {
        let mut r = IssueRecord::new("ACME-1", "Title", vec![]);
        r.flow.origin = Some(origin);
        for (key, n) in findings {
            r.findings.insert((*key).to_owned(), vec![Value::Null; *n]);
        }
        r
    }

    #[test]
    fn a_resolution_under_way_judges_the_findings_of_its_origin() {
        use ovenspring_core::output::Review;
        use ovenspring_core::step::Check;

        // The security review passed this round; its findings from an earlier round are not judged.
        let mut round = with_findings(
            Origin::Round,
            &[
                ("code-review/1", 3),
                ("code-review/2", 1),
                ("security-review", 2),
            ],
        );
        round.flow.round.findings = [
            Review::CodeReview { unit: 1 },
            Review::CodeReview { unit: 2 },
        ]
        .into();
        assert_eq!(judging(&round), Some(4));

        let mut document =
            with_findings(Origin::Document, &[("design-review", 2), ("prd-review", 5)]);
        document.flow.check = Some(Check::DesignReview);
        assert_eq!(judging(&document), Some(2));

        let comments = with_findings(Origin::PrComments, &[("comments", 3), ("code-review/1", 1)]);
        assert_eq!(judging(&comments), Some(3));
    }
}
