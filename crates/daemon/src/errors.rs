//! The one owner of "an error": an operation whose failures repeat on their own until it
//! answers again. Its first failure is a start line in `events.jsonl`, each later one a repeat
//! line, and its next success a cleared line; each start and clear is told once in Slack. The
//! errors going on and the notices still to post are derived from those lines alone, so one
//! appended line is the one commit of each transition.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

use ovenspring_core::config::{Cli, TrackerKind};
use ovenspring_integrations::channel::{Notice, NoticeKind, NoticeMarker};
use ovenspring_integrations::http::{HttpError, Unanswered};
use ovenspring_store::{Store, StoreError};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use ts_rs::TS;

use crate::records::{self, Failing, Service};
use crate::{Daemon, DaemonError};

const EVENTS: &str = "events.jsonl";

/// What an earlier version kept of the services failing to answer, imported once at start.
const UNANSWERED: &str = "services-unanswered.json";

/// The longest text of a failure written to a line, in bytes.
const TEXT_MAX: usize = 300;

/// An error, as its lines name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorKey {
    GithubUnreachable,
    TrackerUnreachable,
    TrackerRefused,
    CliClaude,
    CliCodex,
    CliOpencode,
    Filter,
}

impl ErrorKey {
    /// The error of an agent CLI's runs.
    pub fn of_cli(cli: Cli) -> ErrorKey {
        match cli {
            Cli::Claude => ErrorKey::CliClaude,
            Cli::Codex => ErrorKey::CliCodex,
            Cli::Opencode => ErrorKey::CliOpencode,
        }
    }

    /// The agent CLI whose runs fail, for a CLI's error.
    pub fn cli(self) -> Option<Cli> {
        match self {
            ErrorKey::CliClaude => Some(Cli::Claude),
            ErrorKey::CliCodex => Some(Cli::Codex),
            ErrorKey::CliOpencode => Some(Cli::Opencode),
            ErrorKey::GithubUnreachable
            | ErrorKey::TrackerUnreachable
            | ErrorKey::TrackerRefused
            | ErrorKey::Filter => None,
        }
    }
}

// The serialized name is the one name a key has, in lines, markers and the API alike.
impl fmt::Display for ErrorKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match serde_json::to_value(self) {
            Ok(Value::String(name)) => f.write_str(&name),
            _ => Err(fmt::Error),
        }
    }
}

impl FromStr for ErrorKey {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ErrorKey::deserialize(Value::String(s.to_owned()))
            .map_err(|_| format!("no error is named {s}"))
    }
}

/// A way to GitHub that failed; each must answer again before GitHub's error clears.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Via {
    /// Its API.
    Api,
    /// git, to the repository's `origin`.
    Git,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    Started,
    Cleared,
}

/// An error going on: from its start line to its cleared line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Occurrence {
    /// The start line's time: the occurrence's identity with its key.
    pub since: String,
    /// Its failures so far, the first included.
    pub count: u32,
    /// The latest failure's time.
    pub last: String,
    /// The issue the first failure concerned.
    pub first_issue: Option<String>,
    /// What the latest failure said of itself.
    pub cause: String,
    /// The filter command the latest failure ran, as recorded.
    pub command: Option<String>,
    /// The tracker the first failure concerned.
    pub tracker: Option<TrackerKind>,
    /// Why the latest failure of a service out of reach did not answer.
    pub why: Option<Unanswered>,
    /// The ways to GitHub recorded failing.
    pub via: BTreeSet<Via>,
}

/// A notice that waits to be posted, with the line that tells what it says.
#[derive(Debug, Clone, PartialEq)]
pub struct Waiting {
    pub key: ErrorKey,
    pub since: String,
    pub phase: Phase,
    pub line: Value,
}

impl Waiting {
    /// Whether it is the M-08 of a pause an agent CLI's error caused, which the owner hears of
    /// whether error notices are on or off.
    pub fn pauses(&self) -> bool {
        self.phase == Phase::Started && self.key.cli().is_some()
    }
}

/// The errors going on and the notices waiting, as the lines of `events.jsonl` leave them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Errors {
    open: BTreeMap<ErrorKey, Occurrence>,
    waiting: Vec<Waiting>,
    /// The pause reasons of agent CLI errors whose start has no `paused` line after it.
    unpaused: Vec<String>,
}

fn text(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_owned)
}

impl Errors {
    pub fn from_lines(lines: &[Value]) -> Errors {
        let mut errors = Errors::default();
        for line in lines {
            errors.apply(line);
        }
        errors
    }

    /// The errors as `events.jsonl` records them now.
    pub fn read(store: &Store) -> Result<Errors, DaemonError> {
        Ok(Errors::from_lines(&records::lines(store, EVENTS)?))
    }

    /// Advances the state by one line of `events.jsonl`; a line that is not an error's is ignored.
    pub fn apply(&mut self, line: &Value) {
        if text(line, "event").as_deref() == Some("paused") {
            let reason = text(line, "reason");
            if let Some(at) = self
                .unpaused
                .iter()
                .position(|r| Some(r) == reason.as_ref())
            {
                self.unpaused.remove(at);
            }
            return;
        }
        let Some(failure) = line.get("failure") else {
            return;
        };
        let (Some(key), Some(since)) = (
            text(failure, "key").and_then(|k| k.parse::<ErrorKey>().ok()),
            text(failure, "since"),
        ) else {
            return;
        };
        // The transition the line records, which waits for its notice when the line says so.
        let transition = match text(line, "event").as_deref() {
            Some("error-notice") => {
                let phase = line.get("phase").and_then(|p| Phase::deserialize(p).ok());
                self.waiting
                    .retain(|w| !(w.key == key && w.since == since && Some(w.phase) == phase));
                None
            }
            Some("error-cleared") => {
                if self.open.get(&key).is_some_and(|o| o.since == since) {
                    self.open.remove(&key);
                }
                Some(Phase::Cleared)
            }
            _ if failure.get("start").and_then(Value::as_bool) == Some(true) => {
                // The start commits its pause, whatever later clears the occurrence.
                if key.cli().is_some()
                    && let Some(reason) = text(failure, "reason")
                {
                    self.unpaused.push(reason);
                }
                self.open.insert(
                    key,
                    Occurrence {
                        since: since.clone(),
                        count: 1,
                        last: text(line, "at").unwrap_or_else(|| since.clone()),
                        first_issue: text(line, "issue"),
                        cause: text(failure, "cause").unwrap_or_default(),
                        command: text(line, "command"),
                        tracker: line
                            .get("tracker")
                            .and_then(|t| TrackerKind::deserialize(t).ok()),
                        why: why_of(line),
                        via: via_of(failure),
                    },
                );
                Some(Phase::Started)
            }
            _ => {
                // A repeat whose start was never appended is not this occurrence's.
                if let Some(o) = self.open.get_mut(&key).filter(|o| o.since == since) {
                    o.count += 1;
                    if let Some(at) = text(line, "at") {
                        o.last = at;
                    }
                    if let Some(cause) = text(failure, "cause") {
                        o.cause = cause;
                    }
                    o.command = text(line, "command").or(o.command.take());
                    o.why = why_of(line).or(o.why);
                    o.via.extend(via_of(failure));
                }
                None
            }
        };
        if let Some(phase) = transition
            && matches!(
                text(failure, "notice").as_deref(),
                Some("waiting" | "pause")
            )
        {
            self.waiting.push(Waiting {
                key,
                since,
                phase,
                line: line.clone(),
            });
        }
    }

    /// The latest failure of `key`'s error going on, as `(since, count)`: what a later failure
    /// or a clear changes.
    pub(crate) fn latest_failure(&self, key: ErrorKey) -> Option<(String, u32)> {
        self.open.get(&key).map(|o| (o.since.clone(), o.count))
    }

    pub fn open(&self) -> &BTreeMap<ErrorKey, Occurrence> {
        &self.open
    }

    /// The notices still to post, in the order their lines were appended.
    pub fn waiting(&self) -> &[Waiting] {
        &self.waiting
    }

    /// The pauses agent CLI errors committed and no `paused` line records yet.
    pub fn unpaused(&self) -> &[String] {
        &self.unpaused
    }

    /// The services out of reach, since their first failure and why the latest failed: what Now
    /// shows, from the same record as Events.
    pub fn unreachable(&self) -> BTreeMap<Service, Failing> {
        self.open
            .iter()
            .filter_map(|(key, o)| {
                let service = match key {
                    ErrorKey::GithubUnreachable => Service::Github,
                    ErrorKey::TrackerUnreachable => Service::Tracker,
                    ErrorKey::TrackerRefused
                    | ErrorKey::CliClaude
                    | ErrorKey::CliCodex
                    | ErrorKey::CliOpencode
                    | ErrorKey::Filter => return None,
                };
                Some((
                    service,
                    Failing {
                        since: o.since.clone(),
                        why: o.why?,
                    },
                ))
            })
            .collect()
    }
}

fn why_of(line: &Value) -> Option<Unanswered> {
    line.get("why")
        .and_then(|w| Unanswered::deserialize(w).ok())
}

fn via_of(failure: &Value) -> BTreeSet<Via> {
    failure
        .get("via")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| Via::deserialize(v).ok())
        .collect()
}

/// What a failure to answer says of itself: the answer's status when there was one.
pub(crate) fn unanswered_cause(why: Unanswered, status: Option<u16>) -> String {
    match (status, why) {
        (Some(status), _) => format!("HTTP {status}"),
        (None, Unanswered::RateLimited) => "rate limited".into(),
        (None, Unanswered::NoConnection | Unanswered::ServerError) => "no answer".into(),
    }
}

/// Why a request out of reach failed, and its cause, when the error is one.
pub(crate) fn out_of_reach(e: &HttpError) -> Option<(Unanswered, String)> {
    match e {
        HttpError::Transient { why, status, .. } => Some((*why, unanswered_cause(*why, *status))),
        HttpError::Status { .. } | HttpError::Decode(_) => None,
    }
}

/// The cause of a refusal: the status it answered with, else what it said.
pub(crate) fn refused_cause(e: &HttpError) -> String {
    match e {
        HttpError::Status { status, .. } => format!("HTTP {status}"),
        HttpError::Transient { .. } | HttpError::Decode(_) => e.to_string(),
    }
}

/// `text` cut to at most `TEXT_MAX` bytes, at a character boundary.
pub(crate) fn cut(text: &str) -> String {
    let mut end = text.len().min(TEXT_MAX);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.get(..end).unwrap_or_default().to_owned()
}

/// `value` with every string masked with `secrets`, and each text a failure records (`bound`)
/// cut to its limit after masking, so a cut never leaves part of a credential.
fn bounded(value: Value, secrets: &[&str], bound: bool) -> Value {
    match value {
        Value::String(s) => {
            let masked = crate::download::mask(&s, secrets);
            Value::String(if bound { cut(&masked) } else { masked })
        }
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|i| bounded(i, secrets, false))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| {
                    let bound = matches!(k.as_str(), "cause" | "error" | "command" | "output");
                    (k, bounded(v, secrets, bound))
                })
                .collect(),
        ),
        plain @ (Value::Null | Value::Bool(_) | Value::Number(_)) => plain,
    }
}

/// What the daemon keeps of errors while it runs.
#[derive(Debug, Default)]
pub(crate) struct Live {
    /// The errors going on and the notices waiting, as `events.jsonl` records them.
    pub state: Errors,
    /// Every way to GitHub that failed answered this pass, and nothing has failed as out of
    /// reach since; GitHub's error clears at the pass's end while it holds.
    pub github_clear: bool,
    /// The notices whose refusal by Slack is already logged.
    pub refused_notices: std::collections::HashSet<String>,
}

/// One failure of an error: the event that records it, and what tells its repeats apart.
pub(crate) struct Failure {
    /// The failure's own event, `event` included.
    pub event: Value,
    pub cause: String,
    pub via: Option<Via>,
    /// The pause an agent CLI's error causes, for its start.
    pub reason: Option<String>,
}

impl Daemon {
    /// Records a failure of `key`: a start when it is not going on, else a repeat. The state
    /// advances only once the line is appended, so a failed append leaves the next failure to
    /// start it.
    pub(crate) fn failed(&mut self, key: ErrorKey, failure: Failure) -> Result<(), DaemonError> {
        if key == ErrorKey::GithubUnreachable {
            self.errors.github_clear = false;
        }
        let mut note = Map::from_iter([("key".to_owned(), json!(key.to_string()))]);
        if let Some(o) = self.errors.state.open.get(&key) {
            note.insert("since".into(), json!(o.since));
        } else {
            note.insert("since".into(), json!(jiff::Timestamp::now().to_string()));
            note.insert("start".into(), json!(true));
            // An agent CLI's error pauses the factory, and the owner hears of a pause in any case.
            let notice = match key.cli() {
                Some(_) => "pause",
                None => self.notice_state(),
            };
            note.insert("notice".into(), json!(notice));
            if let Some(reason) = failure.reason {
                note.insert("reason".into(), json!(reason));
            }
        }
        note.insert("cause".into(), json!(failure.cause));
        if let Some(via) = failure.via {
            note.insert("via".into(), json!([via]));
        }
        let mut line = failure.event;
        if let Some(obj) = line.as_object_mut() {
            obj.insert("failure".into(), Value::Object(note));
        }
        self.record_error_line(&line)
    }

    /// Records that `key` answered again: its error, when one goes on, clears with `clear`'s
    /// fields on the line.
    pub(crate) fn succeeded(&mut self, key: ErrorKey, clear: &Value) -> Result<(), DaemonError> {
        let Some(o) = self.errors.state.open.get(&key) else {
            return Ok(());
        };
        let mut line = json!({
            "event": "error-cleared",
            "failure": {"key": key.to_string(), "since": o.since, "notice": self.notice_state()},
            "count": o.count,
        });
        if let (Some(obj), Some(fields)) = (line.as_object_mut(), clear.as_object()) {
            obj.extend(fields.clone());
        }
        self.record_error_line(&line)
    }

    /// Whether a transition recorded now waits for its notice.
    fn notice_state(&self) -> &'static str {
        if self.config.notices.errors {
            "waiting"
        } else {
            "off"
        }
    }

    /// Appends an error's line, masked and cut, and only then advances the state.
    fn record_error_line(&mut self, line: &Value) -> Result<(), DaemonError> {
        let line = self.bounded(line.clone());
        let appended = records::append_event(&self.store, &line)?;
        self.errors.state.apply(&appended);
        // The pause this start commits is told by the start's own notice, not announced again.
        if appended.pointer("/failure/notice") == Some(&json!("pause")) {
            self.pause_announced = true;
        }
        Ok(())
    }

    /// Pauses the factory for each agent CLI error whose start is recorded and whose pause is
    /// not: one interrupted, or whose halt file or `paused` line could not be written. Runs before
    /// anything launches in a pass, so nothing launches under a committed pause.
    pub(crate) fn pause_for_cli_errors(&mut self) -> Result<(), DaemonError> {
        for reason in self.errors.state.unpaused.clone() {
            self.pause(&reason, None)?;
            self.pause_announced = true;
        }
        Ok(())
    }

    fn bounded(&self, value: Value) -> Value {
        let secrets: Vec<&str> = self.services.secrets.iter().map(String::as_str).collect();
        bounded(value, &secrets, false)
    }

    /// Ends GitHub's error when this pass's probes all answered and nothing failed as out of
    /// reach after them; the last part of a pass.
    pub(crate) fn clear_github(&mut self) -> Result<(), DaemonError> {
        if std::mem::take(&mut self.errors.github_clear) {
            self.succeeded(ErrorKey::GithubUnreachable, &json!({}))?;
        }
        Ok(())
    }

    /// Builds the state from `events.jsonl`, then takes in what an earlier version recorded of
    /// the services failing to answer.
    pub(crate) fn recover_errors(&mut self) -> Result<(), DaemonError> {
        self.errors.state = Errors::read(&self.store)?;
        self.import_unanswered()
    }

    /// Turns each service in `services-unanswered.json` into the start of its error, with the
    /// same first-failure time and reason, then deletes the file. A service already going on is
    /// skipped, so an import interrupted before the delete writes no second start.
    pub(crate) fn import_unanswered(&mut self) -> Result<(), DaemonError> {
        let Some(failing) = self
            .store
            .read_record::<BTreeMap<Service, Failing>>(UNANSWERED)?
        else {
            return Ok(());
        };
        for (service, f) in failing {
            let (key, via) = match service {
                // The old record did not say which way failed, so both must answer again.
                Service::Github => (ErrorKey::GithubUnreachable, json!([Via::Api, Via::Git])),
                Service::Tracker => (ErrorKey::TrackerUnreachable, json!([])),
            };
            if self.errors.state.open.contains_key(&key) {
                continue;
            }
            let line = json!({
                "event": "service-unreachable",
                "why": f.why,
                "failure": {
                    "key": key.to_string(), "since": f.since,
                    "cause": unanswered_cause(f.why, None), "via": via,
                    "start": true, "notice": self.notice_state(), "imported": true,
                },
            });
            self.record_error_line(&line)?;
        }
        let path = self.store.root().join(UNANSWERED);
        std::fs::remove_file(&path).map_err(StoreError::io(&path))?;
        Ok(())
    }

    /// Posts each waiting notice in the order it was recorded, or drops it while notices are off.
    /// A refusal stops the pass's posting, so a later notice never goes before an earlier one,
    /// while the drops still run; it is logged once per notice, not per attempt.
    pub(crate) fn deliver_error_notices(&mut self) -> Result<(), DaemonError> {
        let mut refused = false;
        for waiting in self.errors.state.waiting.clone() {
            let failure = json!({"key": waiting.key.to_string(), "since": waiting.since});
            if !self.config.notices.errors && !waiting.pauses() {
                self.record_notice(&json!({
                    "event": "error-notice", "failure": failure,
                    "phase": waiting.phase, "outcome": "dropped",
                    "at": jiff::Timestamp::now().to_string(),
                }))?;
                continue;
            }
            if refused {
                continue;
            }
            let notice = self.notice_of(&waiting);
            let id = notice
                .marker
                .as_ref()
                .map(|m| m.id.clone())
                .unwrap_or_default();
            match self.services.channel.post_notice(&notice) {
                Ok(message) => {
                    self.errors.refused_notices.remove(&id);
                    self.record_notice(&json!({
                        "event": "error-notice", "failure": failure,
                        "phase": waiting.phase, "outcome": "told",
                        "at": time_of_ts(&message.ts),
                    }))?;
                }
                Err(e) => {
                    if self.errors.refused_notices.insert(id.clone()) {
                        let secrets: Vec<&str> =
                            self.services.secrets.iter().map(String::as_str).collect();
                        self.event(&json!({
                            "event": "channel-error", "marker": id,
                            "error": cut(&crate::download::mask(&e.0, &secrets)),
                        }))?;
                    }
                    refused = true;
                }
            }
        }
        Ok(())
    }

    /// Appends a notice's outcome with its own `at` (when Slack took the message), then applies it.
    fn record_notice(&mut self, line: &Value) -> Result<(), DaemonError> {
        self.store.append_event(line)?;
        self.errors.state.apply(line);
        Ok(())
    }

    /// The message a waiting notice posts, built from its line and masked.
    fn notice_of(&self, waiting: &Waiting) -> Notice {
        let line = &waiting.line;
        let tracker = line
            .get("tracker")
            .and_then(|t| TrackerKind::deserialize(t).ok())
            .unwrap_or(self.config.tracker.kind)
            .name();
        let (kind, text) = match waiting.phase {
            Phase::Started => self.started_text(waiting, tracker),
            Phase::Cleared => {
                let cleared = match waiting.key {
                    ErrorKey::GithubUnreachable => "GitHub answers again".to_owned(),
                    ErrorKey::TrackerUnreachable => format!("{tracker} answers again"),
                    ErrorKey::TrackerRefused => format!("{tracker} accepts the connection again"),
                    ErrorKey::CliClaude | ErrorKey::CliCodex | ErrorKey::CliOpencode => format!(
                        "{} starts again",
                        waiting.key.cli().map(Cli::name).unwrap_or_default()
                    ),
                    ErrorKey::Filter if line.get("removed") == Some(&json!(true)) => {
                        "the issue filter was removed".to_owned()
                    }
                    ErrorKey::Filter => "the issue filter works again".to_owned(),
                };
                let times = match line.get("count").and_then(Value::as_u64).unwrap_or(1) {
                    1 => "once".to_owned(),
                    n => format!("{n} times"),
                };
                (
                    NoticeKind::ErrorCleared,
                    format!(
                        "**Cleared: {cleared}.** It failed {times} from {}.",
                        local_time(&waiting.since)
                    ),
                )
            }
        };
        let secrets: Vec<&str> = self.services.secrets.iter().map(String::as_str).collect();
        let phase = match waiting.phase {
            Phase::Started => "started",
            Phase::Cleared => "cleared",
        };
        Notice {
            kind,
            issue: None,
            text: crate::download::mask(&text, &secrets),
            preview: false,
            marker: Some(NoticeMarker {
                id: format!("{}@{}/{phase}", waiting.key, waiting.since),
                recorded: seconds_of(line.get("at").and_then(Value::as_str))
                    .or_else(|| seconds_of(Some(&waiting.since)))
                    .unwrap_or_else(|| jiff::Timestamp::now().as_second()),
            }),
        }
    }

    /// An error's start notice: M-13, or for an agent CLI the M-08 of the pause it caused.
    fn started_text(&self, waiting: &Waiting, tracker: &str) -> (NoticeKind, String) {
        let line = &waiting.line;
        let cause = line
            .pointer("/failure/cause")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let unreachable = |service: &str, interval: &str| {
            let what = match text(line, "operation") {
                Some(operation) => {
                    let operation = match text(line, "issue") {
                        Some(key) => operation.replacen("its ", &format!("{key}'s "), 1),
                        None => operation,
                    };
                    format!("It failed while {operation}: {cause}.")
                }
                None => format!(
                    "It has not answered since {}: {cause}.",
                    local_time(&waiting.since)
                ),
            };
            (
                format!("can't reach {service}"),
                what,
                format!("The factory tries again{interval}."),
            )
        };
        let (name, what, meanwhile) = match waiting.key {
            ErrorKey::GithubUnreachable => unreachable("GitHub", ""),
            ErrorKey::TrackerUnreachable => unreachable(tracker, " every minute"),
            ErrorKey::TrackerRefused => (
                format!("{tracker} refuses the connection"),
                format!("{tracker} refused the connection ({cause})."),
                "No new issue or change is read from it until it is fixed.".to_owned(),
            ),
            ErrorKey::Filter => {
                let command = text(line, "command").unwrap_or_default();
                let error = text(line, "error").unwrap_or_default();
                let issue = text(line, "issue").unwrap_or_default();
                let output = text(line, "output")
                    .filter(|o| !o.is_empty())
                    .map(|o| format!(" (`{o}`)"))
                    .unwrap_or_default();
                (
                    "the issue filter fails".to_owned(),
                    format!("`{command}` {error} on {issue}{output}."),
                    "Issues go on as without a filter until it works.".to_owned(),
                )
            }
            ErrorKey::CliClaude | ErrorKey::CliCodex | ErrorKey::CliOpencode => {
                let reason = line
                    .pointer("/failure/reason")
                    .and_then(Value::as_str)
                    .unwrap_or(cause);
                return (NoticeKind::Paused, crate::owner::pause_text(reason));
            }
        };
        let events_link = self
            .paths
            .dashboard
            .as_ref()
            .map(|dashboard| format!("\n[See it on Events]({dashboard}/events)"))
            .unwrap_or_default();
        (
            NoticeKind::ErrorStarted,
            format!(
                "**Error: {name}.** {what}\n{meanwhile} You will hear once more when it clears.{events_link}"
            ),
        )
    }
}

fn seconds_of(time: Option<&str>) -> Option<i64> {
    time?
        .parse::<jiff::Timestamp>()
        .ok()
        .map(jiff::Timestamp::as_second)
}

/// When Slack took a message, from its `ts`; now when the `ts` is not Slack's form.
fn time_of_ts(ts: &str) -> String {
    ts.split('.')
        .next()
        .and_then(|s| s.parse::<i64>().ok())
        .and_then(|s| jiff::Timestamp::from_second(s).ok())
        .unwrap_or_else(jiff::Timestamp::now)
        .to_string()
}

/// An RFC 3339 time on this computer's clock: "13:41" today, else "Nov 26, 13:41".
fn local_time(time: &str) -> String {
    let Ok(at) = time.parse::<jiff::Timestamp>() else {
        return time.to_owned();
    };
    let zone = jiff::tz::TimeZone::system();
    let at = at.to_zoned_datetime(zone.clone());
    if at.date() == jiff::Timestamp::now().to_zoned_datetime(zone).date() {
        at.strftime("%H:%M").to_string()
    } else {
        at.strftime("%b %-d, %H:%M").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_reads_back_from_its_name() {
        let names = [
            (ErrorKey::GithubUnreachable, "github-unreachable"),
            (ErrorKey::TrackerUnreachable, "tracker-unreachable"),
            (ErrorKey::TrackerRefused, "tracker-refused"),
            (ErrorKey::CliClaude, "cli-claude"),
            (ErrorKey::CliCodex, "cli-codex"),
            (ErrorKey::CliOpencode, "cli-opencode"),
            (ErrorKey::Filter, "filter"),
        ];
        for (key, name) in names {
            assert_eq!(key.to_string(), name);
            assert_eq!(name.parse::<ErrorKey>(), Ok(key));
        }
        assert!("cli-claude-x".parse::<ErrorKey>().is_err());
    }

    #[test]
    fn a_cut_never_splits_a_character() {
        let text = format!("{}é", "a".repeat(TEXT_MAX - 1));
        assert_eq!(cut(&text), "a".repeat(TEXT_MAX - 1));
    }

    #[test]
    fn an_answer_with_an_error_status_is_its_cause() {
        assert_eq!(
            unanswered_cause(Unanswered::ServerError, Some(502)),
            "HTTP 502"
        );
        assert_eq!(
            unanswered_cause(Unanswered::RateLimited, None),
            "rate limited"
        );
        assert_eq!(
            unanswered_cause(Unanswered::NoConnection, None),
            "no answer"
        );
    }
}
