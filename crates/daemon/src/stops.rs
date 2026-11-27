//! What the daemon does when one of its own operations fails: a service out of reach is tried
//! again and stops nothing; any other
//! failure that repeats stops only the issue it was for, which the lead then takes. Nothing is
//! retried silently forever, and one issue's failure never keeps the pass from the others.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use ovenspring_core::flow::{Fact, Stage};
use ovenspring_core::sprint::SprintFact;
use ovenspring_integrations::http::Unanswered;

use crate::errors::{ErrorKey, Failure, Via, out_of_reach, unanswered_cause};
use crate::worktree::git;
use crate::{Daemon, DaemonError};

/// The operation that stopped an issue, and what it last answered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopRecord {
    pub operation: String,
    pub error: String,
    /// RFC 3339.
    pub at: String,
}

/// Failures in a row that stop an issue: one may pass on the next pass; a second of the same
/// operation does not, as the factory's commit and push refusals already count.
const FAILURES_TO_STOP: u32 = 2;

/// What a failing operation belongs to, and so what stops when it fails again.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum Owner {
    /// The issue stops and the lead takes it.
    Issue(String),
    /// The sprint stops and the lead takes it.
    Sprint,
    /// A part of the pass no issue owns: the factory pauses and tells the owner, since
    /// nothing the lead does can reach it.
    Pass,
}

impl Daemon {
    /// The result of `operation` for issue `key`: its value, or `None` after recording its failure.
    pub(crate) fn settle<T>(
        &mut self,
        key: &str,
        operation: &str,
        result: Result<T, DaemonError>,
    ) -> Result<Option<T>, DaemonError> {
        self.settle_for(Owner::Issue(key.to_owned()), operation, result)
    }

    pub(crate) fn settle_sprint(
        &mut self,
        operation: &str,
        result: Result<(), DaemonError>,
    ) -> Result<(), DaemonError> {
        self.settle_for(Owner::Sprint, operation, result)
            .map(|_| ())
    }

    pub(crate) fn settle_part(
        &mut self,
        part: &str,
        result: Result<(), DaemonError>,
    ) -> Result<(), DaemonError> {
        self.settle_for(Owner::Pass, part, result).map(|_| ())
    }

    /// A failure to reach a service is tried again on the next pass; any other failure of the
    /// same operation on two passes in a row stops what owns it, unless it is stopped on that
    /// operation already (the lead or the owner has it).
    fn settle_for<T>(
        &mut self,
        owner: Owner,
        operation: &str,
        result: Result<T, DaemonError>,
    ) -> Result<Option<T>, DaemonError> {
        let id = (owner, operation.to_owned());
        let e = match result {
            Ok(value) => {
                self.failures.remove(&id);
                return Ok(Some(value));
            }
            Err(e) => e,
        };
        let (owner, _) = &id;
        let subject = match owner {
            Owner::Issue(key) => Some(("issue", json!(key))),
            Owner::Sprint => self.sprint()?.map(|s| ("sprint", json!(s.id))),
            Owner::Pass => Some(("pass", json!(true))),
        };
        let mut event = Map::from_iter([
            ("operation".to_owned(), json!(operation)),
            ("error".to_owned(), json!(e.to_string())),
        ]);
        if let Some((field, value)) = subject {
            event.insert(field.to_owned(), value);
        }
        if let Some((why, cause, via)) = self.unreachable(&e) {
            event.insert("event".into(), json!("service-unreachable"));
            event.insert("why".into(), json!(why));
            self.failed(
                ErrorKey::GithubUnreachable,
                Failure {
                    event: Value::Object(event),
                    cause,
                    via: Some(via),
                    reason: None,
                },
            )?;
            return Ok(None);
        }
        event.insert("event".into(), json!("operation-failed"));
        let stopped_on_it = match owner {
            Owner::Issue(key) => self
                .issue(key)?
                .and_then(|r| r.stopped)
                .is_some_and(|s| s.operation == operation),
            Owner::Sprint => self
                .sprint()?
                .is_some_and(|s| s.failed.as_deref() == Some(operation)),
            Owner::Pass => false,
        };
        if stopped_on_it {
            event.insert("stopped".into(), json!(true));
            return self.event(&Value::Object(event)).map(|()| None);
        }
        let count = self.failures.get(&id).copied().unwrap_or(0) + 1;
        event.insert("count".into(), json!(count));
        self.event(&Value::Object(event))?;
        if count < FAILURES_TO_STOP {
            self.failures.insert(id, count);
            return Ok(None);
        }
        self.failures.remove(&id);
        let (owner, _) = id;
        match owner {
            Owner::Issue(key) => self.stop_issue(&key, operation, &e.to_string())?,
            Owner::Sprint => self.sprint_fact(&SprintFact::Stopped {
                operation: operation.into(),
                error: e.to_string(),
            })?,
            // A pause reason completes "paused because …".
            Owner::Pass => self.pause(
                &format!(
                    "{operation} failed twice in a row: {}",
                    e.to_string().trim_end_matches('.')
                ),
                None,
            )?,
        }
        Ok(None)
    }

    /// A failure that is the service's or the network's, not the factory's: no connection, a
    /// timeout, 429 or 5xx from an API, or a git command when `origin` cannot be reached; with
    /// its cause and the way to GitHub that failed.
    fn unreachable(&self, e: &DaemonError) -> Option<(Unanswered, String, Via)> {
        match e {
            DaemonError::GitHub(e) => out_of_reach(e).map(|(why, cause)| (why, cause, Via::Api)),
            DaemonError::Git(_) => (!self.origin_answers()).then(|| {
                let why = Unanswered::NoConnection;
                (why, unanswered_cause(why, None), Via::Git)
            }),
            DaemonError::Store(_)
            | DaemonError::Launch(_)
            | DaemonError::NoRecord(_)
            | DaemonError::Invalid(_) => None,
        }
    }

    /// Whether the repository's `origin` answers git.
    pub(crate) fn origin_answers(&self) -> bool {
        git(
            &self.paths.repo,
            &["ls-remote", "-q", "--exit-code", "origin", "HEAD"],
        )
        .is_ok()
    }

    /// Stops the issue on `operation`: its open work ends, the record names what failed, and the
    /// flow keeps the state it stopped in and files `exception` for the lead.
    pub(crate) fn stop_issue(
        &mut self,
        key: &str,
        operation: &str,
        error: &str,
    ) -> Result<(), DaemonError> {
        if self.issue(key)?.is_none() {
            return Ok(());
        }
        self.end_work(key)?;
        self.update_issue(key, |r| {
            r.stopped = Some(StopRecord {
                operation: operation.into(),
                error: error.into(),
                at: jiff::Timestamp::now().to_string(),
            });
        })?;
        self.event(
            &json!({"event": "stopped", "issue": key, "operation": operation, "error": error}),
        )?;
        self.apply(
            key,
            Fact::Stopped {
                operation: operation.into(),
            },
        )
    }

    /// What stopped the issue was cleared: the lead's and owner's open work on it ends and it
    /// returns to the state it stopped in, where the next pass runs the stopped operation again.
    pub(crate) fn resume_issue(&mut self, key: &str) -> Result<(), DaemonError> {
        let Some(record) = self.issue(key)? else {
            return Ok(());
        };
        let waits_on_owner = matches!(
            record.flow.stage,
            Stage::AwaitingRecovery { .. } | Stage::Stopped { .. }
        );
        if record.flow.resume().is_none() && !waits_on_owner {
            return Ok(());
        }
        self.end_work(key)?;
        self.supersede_open_decisions(key)?;
        self.update_issue(key, |r| r.recovery_log = None)?;
        self.event(&json!({"event": "resumed", "issue": key}))?;
        self.apply(key, Fact::Resume)
    }

    /// On start, every stopped issue, and the sprint when it is stopped, runs its stopped
    /// operation again.
    pub(crate) fn resume_stopped(&mut self) -> Result<(), DaemonError> {
        for record in self.issues()? {
            if record.flow.resume().is_some() {
                let key = record.key().to_owned();
                self.resume_issue(&key)?;
            }
        }
        self.resume_sprint()
    }
}
