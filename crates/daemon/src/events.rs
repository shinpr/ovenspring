//! The Events page's read of `events.jsonl`: each recorded line with the issue or sprint it
//! concerns, an error's repeats folded into its earlier lines, whether each start and clear was
//! told, narrowed and windowed as the page asks, every string masked. It writes nothing.

use std::collections::{BTreeMap, HashMap};

use ovenspring_core::config::{Cli, TrackerKind};
use ovenspring_core::sprint::SprintState;
use ovenspring_store::Store;
use serde::Serialize;
use serde_json::{Map, Value};
use ts_rs::TS;

use crate::DaemonError;
use crate::download::mask;
use crate::errors::{ErrorKey, Errors};
use crate::records::{self, LaunchLine};

/// The lines a page shows before the owner asks for older ones.
const FIRST_LINES: usize = 15;

/// The older lines each "Show older events" adds.
const OLDER_LINES: usize = 50;

/// Which lines the page lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Show {
    #[default]
    All,
    /// Error lines: starts, repeats and clears.
    Errors,
    /// The issue filter's drops, and the lines of its error.
    Filter,
}

impl std::str::FromStr for Show {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "all" => Ok(Show::All),
            "errors" => Ok(Show::Errors),
            "filter" => Ok(Show::Filter),
            _ => Err("Show `all`, `errors` or `filter`.".into()),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EventsQuery {
    pub show: Show,
    /// Only the lines that concern this issue.
    pub issue: Option<String>,
    /// Every matching line from this `seq` on; without it, the newest few.
    pub from: Option<u64>,
}

/// The page's lines and what frames them.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EventsPage {
    /// Newest first.
    pub lines: Vec<EventLine>,
    /// The `from` that also lists the next older matching lines; none when no older line matches.
    pub older: Option<u64>,
    /// Some line is recorded, whatever the filters keep.
    pub recorded: bool,
    /// `events.jsonl` exists and could not be read.
    pub unreadable: bool,
    /// Every error going on, oldest start first; the filters do not narrow it.
    pub errors_now: Vec<ErrorNow>,
    /// Error notices are on.
    pub notices: bool,
    /// Every issue a line concerns, by key.
    pub issues: Vec<IssueTitle>,
}

/// One recorded line.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EventLine {
    /// The line's place among the parsed lines of `events.jsonl`; it never changes.
    pub seq: u64,
    pub at: String,
    /// The line's `event`.
    pub kind: String,
    /// The line's other fields.
    pub fields: Map<String, Value>,
    pub issue: Option<EventIssue>,
    pub sprint: Option<EventSprint>,
    pub error: Option<EventError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct EventIssue {
    pub key: String,
    /// The factory has a record of the issue.
    pub known: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct EventSprint {
    pub id: String,
    pub purpose: Option<String>,
    /// The Sprints page lists it.
    pub listed: bool,
}

/// What a line is to its error.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "role", rename_all = "camelCase")]
pub enum EventError {
    /// Its first failure, with the later ones of the same issue and cause folded in.
    #[serde(rename_all = "camelCase")]
    Started {
        key: ErrorKey,
        cause: String,
        times: u32,
        last: String,
        /// The error has not cleared.
        open: bool,
        told: NoticeState,
    },
    /// A failure on another issue or with another cause than the lines before it.
    #[serde(rename_all = "camelCase")]
    Repeat {
        key: ErrorKey,
        cause: String,
        times: u32,
        last: String,
        started_at: String,
    },
    #[serde(rename_all = "camelCase")]
    Cleared {
        key: ErrorKey,
        started_at: String,
        times: u32,
        /// It cleared because the owner removed what failed.
        removed: bool,
        told: NoticeState,
    },
}

/// Whether the owner was told of a start or a clear.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum NoticeState {
    /// Slack accepted the notice then.
    Told {
        at: String,
    },
    Waiting,
    /// No notice was or will be sent: notices were off.
    Off,
}

/// An error going on.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ErrorNow {
    pub key: ErrorKey,
    pub since: String,
    pub last: String,
    pub times: u32,
    pub first_issue: Option<EventIssue>,
    /// The latest failure's.
    pub cause: String,
    pub tracker: Option<TrackerKind>,
    pub cli: Option<Cli>,
    /// The filter command the latest failure ran.
    pub command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct IssueTitle {
    pub key: String,
    pub title: Option<String>,
}

const EVENTS: &str = "events.jsonl";

/// The page `query` asks for, from the lines `events.jsonl` holds now. A file that exists and
/// cannot be read is answered as unreadable, not as a failure.
pub fn page(
    store: &Store,
    query: &EventsQuery,
    notices: bool,
    secrets: &[&str],
) -> Result<EventsPage, DaemonError> {
    let Ok(lines) = records::lines(store, EVENTS) else {
        return Ok(EventsPage {
            lines: Vec::new(),
            older: None,
            recorded: false,
            unreadable: true,
            errors_now: Vec::new(),
            notices,
            issues: Vec::new(),
        });
    };
    let errors = Errors::from_lines(&lines);
    let subjects = Subjects::read(store)?;
    let listed = listed(&lines, &errors, &subjects);

    let matching: Vec<&Listed<'_>> = listed.iter().filter(|l| query.keeps(l)).collect();
    let start = match query.from {
        Some(from) => matching.partition_point(|l| l.seq < from),
        None => matching.len().saturating_sub(FIRST_LINES),
    };
    let (before, window) = matching.split_at(start);
    let older = before
        .get(before.len().saturating_sub(OLDER_LINES))
        .map(|l| l.seq);

    let mut issues: BTreeMap<&str, Option<String>> = BTreeMap::new();
    for l in &listed {
        if let Subject::Issue(key) = &l.subject {
            issues.insert(key, subjects.titles.get(key).map(|t| mask(t, secrets)));
        }
    }
    Ok(EventsPage {
        lines: window
            .iter()
            .rev()
            .map(|l| l.served(&subjects, secrets))
            .collect(),
        older,
        recorded: !listed.is_empty(),
        unreadable: false,
        errors_now: errors_now(&errors, &subjects, secrets),
        notices,
        issues: issues
            .into_iter()
            .map(|(key, title)| IssueTitle {
                key: key.to_owned(),
                title,
            })
            .collect(),
    })
}

impl EventsQuery {
    fn keeps(&self, line: &Listed<'_>) -> bool {
        let shown = match self.show {
            Show::All => true,
            Show::Errors => line.raw.get("failure").is_some(),
            // A `filter-failed` line recorded before error tracking has no `failure`.
            Show::Filter => {
                matches!(line.kind.as_str(), "filter-dropped" | "filter-failed")
                    || line.raw.pointer("/failure/key") == Some(&Value::from("filter"))
            }
        };
        shown
            && self
                .issue
                .as_ref()
                .is_none_or(|key| line.subject == Subject::Issue(key.clone()))
    }
}

/// What the factory has a record of: issues by key, sprints by id, and each launched item's
/// issue or sprint.
struct Subjects {
    titles: BTreeMap<String, String>,
    /// Each sprint's purpose and whether the Sprints page lists it.
    sprints: BTreeMap<String, (Option<String>, bool)>,
    items: HashMap<String, Subject>,
}

impl Subjects {
    fn read(store: &Store) -> Result<Subjects, DaemonError> {
        let titles = records::issues(store)?
            .into_iter()
            .map(|r| (r.key().to_owned(), r.title))
            .collect();
        let purpose = |s: &SprintState| s.plan.as_ref().map(|p| p.purpose.clone());
        let mut sprints = BTreeMap::new();
        // Every sprint kept under `sprints/` is listed as ended; the current one is listed while
        // it runs.
        for id in store.record_names("sprints")? {
            if let Some(s) = store.read_record::<SprintState>(&Store::sprint_record(&id))? {
                sprints.insert(s.id.clone(), (purpose(&s), true));
            }
        }
        if let Some(s) = records::sprint(store)? {
            sprints
                .entry(s.id.clone())
                .or_insert((purpose(&s), s.started_at.is_some()));
        }
        let mut items = HashMap::new();
        for line in records::launch_lines(store)? {
            if let LaunchLine::Start(start) = line {
                let subject = match (start.issue, start.sprint) {
                    (Some(key), _) => Subject::Issue(key),
                    (None, Some(id)) => Subject::Sprint(id),
                    (None, None) => Subject::None,
                };
                items.entry(start.item).or_insert(subject);
            }
        }
        Ok(Subjects {
            titles,
            sprints,
            items,
        })
    }

    /// What a line concerns: an issue it names, unless it names only a sprint; else its sprint;
    /// else the issue or sprint of the item it names.
    fn of(&self, line: &Value) -> Subject {
        let text = |field: &str| line.get(field).and_then(Value::as_str);
        if let Some(key) = text("issue") {
            return if self.titles.contains_key(key) || !self.sprints.contains_key(key) {
                Subject::Issue(key.to_owned())
            } else {
                Subject::Sprint(key.to_owned())
            };
        }
        if let Some(id) = text("sprint") {
            return Subject::Sprint(id.to_owned());
        }
        text("item")
            .and_then(|item| self.items.get(item))
            .cloned()
            .unwrap_or(Subject::None)
    }

    fn issue(&self, key: &str) -> EventIssue {
        EventIssue {
            key: key.to_owned(),
            known: self.titles.contains_key(key),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Subject {
    Issue(String),
    Sprint(String),
    None,
}

/// A line the page lists, before it is served.
struct Listed<'a> {
    seq: u64,
    kind: String,
    raw: &'a Value,
    subject: Subject,
    error: Option<EventError>,
}

/// An error's start, clear or failure line, by its occurrence.
type Occurrence = (String, String);

/// The lines the page can list, oldest first: every line but the notices' records and the
/// repeats folded into an earlier line of their error.
fn listed<'a>(lines: &'a [Value], errors: &Errors, subjects: &Subjects) -> Vec<Listed<'a>> {
    let told = told(lines);
    let mut out: Vec<Listed<'a>> = Vec::new();
    // The start line's time, and the listed lines, of each occurrence.
    let mut occurrences: HashMap<Occurrence, (String, Vec<usize>)> = HashMap::new();
    for (seq, line) in (0u64..).zip(lines) {
        let kind = line
            .get("event")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if kind == "error-notice" {
            continue;
        }
        let at = line
            .get("at")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let subject = subjects.of(line);
        let failure = line.get("failure");
        let text = |field: &str| failure?.get(field)?.as_str().map(str::to_owned);
        let error = match (text("key"), text("since")) {
            (Some(name), Some(since)) => name.parse::<ErrorKey>().ok().map(|key| {
                let occurrence = (name, since);
                let cause = text("cause").unwrap_or_default();
                (key, occurrence, cause)
            }),
            _ => None,
        };
        let error = match error {
            None => None,
            Some((key, occurrence, _)) if kind == "error-cleared" => Some(EventError::Cleared {
                key,
                started_at: occurrence.1.clone(),
                times: line
                    .get("count")
                    .and_then(Value::as_u64)
                    .and_then(|n| u32::try_from(n).ok())
                    .unwrap_or(1),
                removed: line.get("removed") == Some(&Value::Bool(true)),
                told: told.of(&occurrence, "cleared", failure),
            }),
            Some((key, occurrence, cause))
                if failure.and_then(|f| f.get("start")) == Some(&Value::Bool(true)) =>
            {
                let open = errors
                    .open()
                    .get(&key)
                    .is_some_and(|o| o.since == occurrence.1);
                let told = told.of(&occurrence, "started", failure);
                occurrences.insert(occurrence, (at.clone(), vec![out.len()]));
                Some(EventError::Started {
                    key,
                    cause,
                    times: 1,
                    last: at.clone(),
                    open,
                    told,
                })
            }
            Some((key, occurrence, cause)) => {
                let Some((started_at, members)) = occurrences.get_mut(&occurrence) else {
                    // A failure whose start was never appended belongs to no error.
                    out.push(Listed {
                        seq,
                        kind,
                        raw: line,
                        subject,
                        error: None,
                    });
                    continue;
                };
                let same = members.iter().copied().find(|&i| {
                    out.get(i).is_some_and(|earlier| {
                        earlier.subject_issue() == issue_of(&subject)
                            && earlier.cause() == Some(cause.as_str())
                    })
                });
                if let Some(earlier) = same.and_then(|i| out.get_mut(i)) {
                    earlier.fold(&at);
                    continue;
                }
                members.push(out.len());
                Some(EventError::Repeat {
                    key,
                    cause,
                    times: 1,
                    last: at.clone(),
                    started_at: started_at.clone(),
                })
            }
        };
        out.push(Listed {
            seq,
            kind,
            raw: line,
            subject,
            error,
        });
    }
    out
}

fn issue_of(subject: &Subject) -> Option<&str> {
    match subject {
        Subject::Issue(key) => Some(key),
        Subject::Sprint(_) | Subject::None => None,
    }
}

impl Listed<'_> {
    fn subject_issue(&self) -> Option<&str> {
        issue_of(&self.subject)
    }

    fn cause(&self) -> Option<&str> {
        match &self.error {
            Some(EventError::Started { cause, .. } | EventError::Repeat { cause, .. }) => {
                Some(cause)
            }
            Some(EventError::Cleared { .. }) | None => None,
        }
    }

    /// Takes in a later failure of the same issue and cause.
    fn fold(&mut self, at: &str) {
        if let Some(
            EventError::Started { times, last, .. } | EventError::Repeat { times, last, .. },
        ) = &mut self.error
        {
            *times += 1;
            at.clone_into(last);
        }
    }

    fn served(&self, subjects: &Subjects, secrets: &[&str]) -> EventLine {
        let fields = self
            .raw
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(field, _)| !matches!(field.as_str(), "event" | "at"))
            .map(|(field, value)| (field.clone(), masked(value, secrets)))
            .collect();
        let error = self.error.clone().map(|mut e| {
            if let EventError::Started { cause, .. } | EventError::Repeat { cause, .. } = &mut e {
                *cause = mask(cause, secrets);
            }
            e
        });
        EventLine {
            seq: self.seq,
            at: self
                .raw
                .get("at")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            kind: self.kind.clone(),
            fields,
            issue: issue_of(&self.subject).map(|key| subjects.issue(key)),
            sprint: match &self.subject {
                Subject::Sprint(id) => {
                    let (purpose, listed) = subjects.sprints.get(id).cloned().unwrap_or_default();
                    Some(EventSprint {
                        id: id.clone(),
                        purpose: purpose.map(|p| mask(&p, secrets)),
                        listed,
                    })
                }
                Subject::Issue(_) | Subject::None => None,
            },
            error,
        }
    }
}

/// When each start and clear was told, or dropped while notices were off.
struct Notices(HashMap<(Occurrence, String), Option<String>>);

fn told(lines: &[Value]) -> Notices {
    let mut out = HashMap::new();
    for line in lines
        .iter()
        .filter(|l| l.get("event") == Some(&Value::from("error-notice")))
    {
        let text = |pointer: &str| line.pointer(pointer).and_then(Value::as_str);
        let (Some(key), Some(since), Some(phase)) =
            (text("/failure/key"), text("/failure/since"), text("/phase"))
        else {
            continue;
        };
        let at = match text("/outcome") {
            Some("told") => Some(text("/at").unwrap_or_default().to_owned()),
            _ => None,
        };
        let entry = out
            .entry(((key.to_owned(), since.to_owned()), phase.to_owned()))
            .or_insert(None);
        // A told record wins over a dropped one.
        if at.is_some() {
            *entry = at;
        }
    }
    Notices(out)
}

impl Notices {
    fn of(&self, occurrence: &Occurrence, phase: &str, failure: Option<&Value>) -> NoticeState {
        match self.0.get(&(occurrence.clone(), phase.to_owned())) {
            Some(Some(at)) => NoticeState::Told { at: at.clone() },
            Some(None) => NoticeState::Off,
            None if failure.and_then(|f| f.get("notice")) == Some(&Value::from("off")) => {
                NoticeState::Off
            }
            None => NoticeState::Waiting,
        }
    }
}

fn errors_now(errors: &Errors, subjects: &Subjects, secrets: &[&str]) -> Vec<ErrorNow> {
    let mut now: Vec<ErrorNow> = errors
        .open()
        .iter()
        .map(|(key, o)| ErrorNow {
            key: *key,
            since: o.since.clone(),
            last: o.last.clone(),
            times: o.count,
            first_issue: o.first_issue.as_deref().map(|k| subjects.issue(k)),
            cause: mask(&o.cause, secrets),
            tracker: o.tracker,
            cli: key.cli(),
            command: o.command.as_deref().map(|c| mask(c, secrets)),
        })
        .collect();
    now.sort_by(|a, b| a.since.cmp(&b.since));
    now
}

/// `value` with every string in it masked.
fn masked(value: &Value, secrets: &[&str]) -> Value {
    match value {
        Value::String(s) => Value::String(mask(s, secrets)),
        Value::Array(items) => Value::Array(items.iter().map(|i| masked(i, secrets)).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), masked(v, secrets)))
                .collect(),
        ),
        plain @ (Value::Null | Value::Bool(_) | Value::Number(_)) => plain.clone(),
    }
}
