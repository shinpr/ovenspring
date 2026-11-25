//! The factory's records as the daemon and the dashboard server both read them: the goal, the
//! sprint, the issues and the decisions, over the state directory.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::fs;

use ovenspring_core::config::{Cli, Permissions, Role};
use ovenspring_core::flow::Stage;
use ovenspring_core::phase::Phase;
use ovenspring_core::sprint::SprintState;
use ovenspring_core::step::Step;
use ovenspring_store::{Store, StoreError};
use serde::{Deserialize, Serialize};
use serde_json::json;
use ts_rs::TS;

use crate::DaemonError;
use crate::issues::IssueRecord;
use crate::owner::DecisionRecord;
use ovenspring_integrations::http::Unanswered;

pub(crate) const SPRINT: &str = "sprint.json";

pub(crate) const LAUNCHES: &str = "launches.jsonl";

const SERVICES: &str = "services-unanswered.json";

/// A service the factory calls that can fail to answer for a while.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Service {
    Github,
    Tracker,
}

const TOLD_WAITING: &str = "told-waiting.json";

/// Where and when the lead told the owner that no open issue serves the goal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Told {
    pub at: String,
    /// The Slack message, when Slack gave its address.
    pub link: Option<String>,
}

pub fn told_waiting(store: &Store) -> Result<Option<Told>, DaemonError> {
    Ok(store.read_record::<Option<Told>>(TOLD_WAITING)?.flatten())
}

/// Keeps where the owner was told, or forgets it when the message could not be sent, so an
/// earlier message is never offered for a later wait.
pub(crate) fn set_told_waiting(store: &Store, told: Option<&Told>) -> Result<(), DaemonError> {
    store.write_record(TOLD_WAITING, &told)?;
    Ok(())
}

/// A service failing to answer: since its first failure (RFC 3339), and why the latest failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failing {
    pub since: String,
    pub why: Unanswered,
}

/// The services failing to answer.
pub fn unreachable(store: &Store) -> Result<BTreeMap<Service, Failing>, DaemonError> {
    Ok(store.read_record(SERVICES)?.unwrap_or_default())
}

/// Records whether `service` answered: a failure keeps the time of the first one and why the
/// latest failed, an answer removes it.
pub(crate) fn reached(
    store: &Store,
    service: Service,
    outcome: Result<(), Unanswered>,
) -> Result<(), DaemonError> {
    let mut failing = unreachable(store)?;
    let changed = match (outcome, failing.entry(service)) {
        (Ok(()), Entry::Occupied(entry)) => {
            entry.remove();
            true
        }
        (Ok(()), Entry::Vacant(_)) => false,
        (Err(why), Entry::Vacant(first)) => {
            first.insert(Failing {
                since: jiff::Timestamp::now().to_string(),
                why,
            });
            true
        }
        (Err(why), Entry::Occupied(mut entry)) => {
            let changed = entry.get().why != why;
            entry.get_mut().why = why;
            changed
        }
    };
    if changed {
        store.write_record(SERVICES, &failing)?;
    }
    Ok(())
}

/// A launch's start, its first line in `launches.jsonl`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LaunchStart {
    pub at: String,
    pub role: Role,
    pub item: String,
    pub step: Step,
    /// The issue's phase when the launch started; a correction's phase is known only then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<Phase>,
    pub issue: Option<String>,
    pub sprint: Option<String>,
    pub cli: Cli,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub permissions: Permissions,
    pub attempt: u32,
    pub pid: u32,
    pub log: String,
    /// The launch is a design-phase step's preparation turn.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub prepare: bool,
}

/// A launch's end, its second line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LaunchEnd {
    pub at: String,
    pub item: String,
    /// Always true: what tells an end from a start.
    pub ended: bool,
    pub code: Option<i32>,
    pub attempt: u32,
    /// How it failed (`transient: ...`, `fatal: ...`), when it did.
    pub failure: Option<String>,
    pub error_kind: Option<String>,
    pub session: Option<String>,
    pub usage: Usage,
    /// The model that ran, as the CLI or its session record names it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub cost_usd: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LaunchLine {
    End(LaunchEnd),
    Start(LaunchStart),
}

/// A change of an issue's state, as `events.jsonl` keeps it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename = "fact")]
pub struct FactEvent {
    pub issue: String,
    pub fact: String,
    pub from: Stage,
    pub to: Stage,
    /// When it happened; `event` stamps it as it is written.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub at: String,
}

/// The name of a fact as an event records it: its variant, without what it carries.
pub(crate) fn fact_name(fact: &impl std::fmt::Debug) -> String {
    format!("{fact:?}")
        .split(['(', ' ', '{'])
        .next()
        .unwrap_or_default()
        .to_owned()
}

/// Every issue state change in `events.jsonl`, oldest first.
pub(crate) fn fact_events(store: &Store) -> Result<Vec<FactEvent>, DaemonError> {
    Ok(lines(store, "events.jsonl")?
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect())
}

/// Appends a launch's start or end to `launches.jsonl`.
pub(crate) fn append_launch(store: &Store, line: &LaunchLine) -> Result<(), DaemonError> {
    Ok(store.append_launch(&json!(line))?)
}

/// Every line of `launches.jsonl`, oldest first.
pub(crate) fn launch_lines(store: &Store) -> Result<Vec<LaunchLine>, DaemonError> {
    Ok(lines(store, LAUNCHES)?
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect())
}

/// When the factory last recorded something, in Unix seconds: where reading what the owner sent
/// while it was not running starts.
pub fn last_activity(store: &Store) -> Option<i64> {
    lines(store, "events.jsonl")
        .ok()?
        .last()
        .and_then(|e| e.get("at")?.as_str()?.parse::<jiff::Timestamp>().ok())
        .map(jiff::Timestamp::as_second)
}

/// Appends `value` to `events.jsonl` with the time it happened.
pub fn event(store: &Store, value: &serde_json::Value) -> Result<(), DaemonError> {
    let mut v = value.clone();
    if let Some(obj) = v.as_object_mut() {
        obj.insert("at".into(), json!(jiff::Timestamp::now().to_string()));
    }
    Ok(store.append_event(&v)?)
}

/// The lines of a JSON Lines file under the state directory; none before its first line. A line
/// cut short by a crash mid-write is the only one that does not parse; it is skipped.
pub(crate) fn lines(store: &Store, file: &str) -> Result<Vec<serde_json::Value>, DaemonError> {
    let path = store.root().join(file);
    match fs::read_to_string(&path) {
        Ok(text) => Ok(text
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(StoreError::io(&path)(e).into()),
    }
}

/// The product goal, trimmed; empty until the owner sets one.
pub fn goal(store: &Store) -> Result<String, DaemonError> {
    let path = store.root().join("goal.md");
    match fs::read_to_string(&path) {
        Ok(text) => Ok(text.trim().to_owned()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(StoreError::io(&path)(e).into()),
    }
}

/// Sets the product goal; the change is logged with the old and the new text. The text
/// is trimmed and must not be empty.
pub fn set_goal(store: &Store, text: &str) -> Result<(), DaemonError> {
    let new = text.trim();
    if new.is_empty() {
        return Err(DaemonError::Invalid("a goal cannot be empty".into()));
    }
    let old = goal(store)?;
    store.write_text("goal.md", &format!("{new}\n"))?;
    event(store, &json!({"event": "goal-set", "old": old, "new": new}))
}

/// The owner's request, from the dashboard, to cancel an issue; the daemon takes it in on its next
/// pass. It lives in the isolated directory, which no agent can reach, so only the owner cancels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelRequest {
    pub words: String,
    pub at: String,
}

const CANCELS: &str = "cancels";

/// Asks the daemon to cancel `key`, which must not have ended.
pub fn request_cancel(store: &Store, key: &str, words: &str) -> Result<(), DaemonError> {
    let Some(record) = issue(store, key)? else {
        return Err(DaemonError::Invalid(format!(
            "{key} is not an issue of this factory"
        )));
    };
    if matches!(record.flow.stage, Stage::Merged | Stage::Dropped) {
        return Err(DaemonError::Invalid(format!("{key} has already ended")));
    }
    store.write_isolated(
        &format!("{CANCELS}/{key}.json"),
        &CancelRequest {
            words: words.trim().to_owned(),
            at: jiff::Timestamp::now().to_string(),
        },
    )?;
    Ok(())
}

/// The cancel requests waiting for the daemon, each removed as it is returned.
pub(crate) fn take_cancel_requests(
    store: &Store,
) -> Result<Vec<(String, CancelRequest)>, DaemonError> {
    let mut out = Vec::new();
    for key in store.isolated_names(CANCELS)? {
        let rel = format!("{CANCELS}/{key}.json");
        if let Some(request) = store.read_isolated::<CancelRequest>(&rel)? {
            out.push((key, request));
        }
        let path = store.isolated().join(&rel);
        fs::remove_file(&path).map_err(StoreError::io(&path))?;
    }
    Ok(out)
}

pub fn sprint(store: &Store) -> Result<Option<SprintState>, DaemonError> {
    Ok(store.read_record(SPRINT)?)
}

pub fn issue(store: &Store, key: &str) -> Result<Option<IssueRecord>, DaemonError> {
    Ok(store.read_record(&crate::issues::record_path(key))?)
}

pub fn issues(store: &Store) -> Result<Vec<IssueRecord>, DaemonError> {
    let mut out = Vec::new();
    for key in store.record_names("issues")? {
        if let Some(r) = issue(store, &key)? {
            out.push(r);
        }
    }
    Ok(out)
}

pub fn decisions(store: &Store) -> Result<Vec<DecisionRecord>, DaemonError> {
    let mut out = Vec::new();
    for id in store.isolated_names("decisions")? {
        if let Some(d) = store.read_isolated(&crate::owner::decision_path(&id))? {
            out.push(d);
        }
    }
    Ok(out)
}

/// The reason the factory is paused, when it is.
pub fn pause_reason(store: &Store) -> Result<Option<String>, DaemonError> {
    let path = store.halt_file();
    match fs::read_to_string(&path) {
        Ok(text) => Ok(Some(text.trim().to_owned())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(StoreError::io(&path)(e).into()),
    }
}
