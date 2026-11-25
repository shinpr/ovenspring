//! Recovery procedures: when the way
//! forward needs what agents may not do, the lead writes the commands, the owner approves them in
//! Slack, and only the factory runs them, exactly as the owner was shown them, from the
//! isolated directory no agent reaches. The owner may instead fix it and press Resolved. A recovery
//! is for an issue (its key) or for the sprint (`None`).

use std::fs;
use std::process::{Command, Stdio};

use ovenspring_core::flow::Fact;
use ovenspring_core::output::{ExceptionOutput, StepOutput};
use ovenspring_core::sprint::SprintFact;
use ovenspring_core::step::Step;
use ovenspring_integrations::channel::DecisionKind;
use serde_json::json;

use crate::owner::{Answer, AnswerKind, DecisionRecord};
use crate::{Daemon, DaemonError};
use ovenspring_store::StoreError;

/// A procedure under way, kept in `<isolated>/running-recoveries/<decision>.json` until its result
/// is taken in: one still there when the factory starts was running when it stopped.
#[derive(serde::Serialize, serde::Deserialize)]
struct Running {
    /// The issue, or `None` for the sprint.
    issue: Option<String>,
}

const RUNNING: &str = "running-recoveries";

/// A procedure that finished: what the next pass does with it.
pub(crate) struct Recovered {
    /// The issue, or `None` for the sprint.
    pub key: Option<String>,
    pub decision: String,
    pub succeeded: bool,
}

impl Daemon {
    /// Copies the lead's procedure to `<isolated>/recoveries/` and asks the owner to approve that
    /// copy. The message shows the copy's text, which is what runs.
    pub(crate) fn request_recovery_for(&mut self, key: Option<&str>) -> Result<(), DaemonError> {
        let (out, title, place) = if let Some(key) = key {
            let record = self.record_of(key)?;
            let Some(StepOutput::Exception(e)) = record.output(Step::Exception) else {
                return Err(DaemonError::Launch(format!(
                    "{key}: the lead's recovery has no output to read"
                )));
            };
            self.update_issue(key, |r| r.recovery_log = None)?;
            (
                e,
                format!("{key}: {}", record.title),
                format!("{key}'s worktree"),
            )
        } else {
            let state = self
                .sprint()?
                .ok_or_else(|| DaemonError::Launch("no sprint to recover".into()))?;
            (
                self.latest_sprint_exception(&state.id)?,
                format!("Sprint {}", state.id),
                "a checkout of the base branch".to_owned(),
            )
        };
        let (Some(procedure), Some(summary)) = (out.procedure, out.summary) else {
            return Err(DaemonError::Launch(
                "the lead's recovery names no procedure".into(),
            ));
        };
        let mut d = self.new_decision(DecisionKind::Recovery, key, title)?;
        let file = format!("{}.sh", d.id);
        let path = self.store.isolated().join("recoveries").join(&file);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(StoreError::io(dir))?;
        }
        fs::write(&path, &procedure).map_err(StoreError::io(&path))?;
        // The procedure names its own interpreter on its first line and runs as it is written.
        fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o700))
            .map_err(StoreError::io(&path))?;
        let shown = fs::read_to_string(&path).map_err(StoreError::io(&path))?;
        d.text = format!(
            "{summary}\n\nApprove runs exactly this, in {place}:\n\n```sh\n{}\n```\n\nResolved tells the factory you fixed it yourself; a reply goes back to the lead.",
            shown.trim_end()
        );
        d.procedure = Some(file);
        self.decide(d)
    }

    fn latest_sprint_exception(&self, sprint: &str) -> Result<ExceptionOutput, DaemonError> {
        self.sprint_exception_outputs(sprint)
            .last()
            .and_then(|out| fs::read_to_string(out).ok())
            // `item set` validated it as the step's output before the item could end.
            .and_then(|text| serde_json::from_str::<ExceptionOutput>(&text).ok())
            .ok_or_else(|| {
                DaemonError::Launch(format!(
                    "sprint {sprint}: the lead's recovery has no output to read"
                ))
            })
    }

    /// What stopped and the lead could not be launched to take it: the owner learns what failed
    /// and marks it resolved once fixed.
    pub(crate) fn report_stopped_for(&mut self, key: Option<&str>) -> Result<(), DaemonError> {
        let (who, title, what) = if let Some(key) = key {
            let record = self.record_of(key)?;
            let what = record.stopped.as_ref().map_or_else(
                || "an operation".to_owned(),
                |s| format!("{}, which answered: {}", s.operation, s.error),
            );
            (key.to_owned(), format!("{key}: {}", record.title), what)
        } else {
            let state = self
                .sprint()?
                .ok_or_else(|| DaemonError::Launch("no sprint stopped".into()))?;
            let what = format!(
                "{}, which answered: {}",
                state.failed.as_deref().unwrap_or("an operation"),
                state.failure.as_deref().unwrap_or("no message")
            );
            let who = format!("Sprint {}", state.id);
            (who.clone(), who, what)
        };
        let text = format!(
            "{who} stopped, and the lead could not be started to find the way forward. What failed: {what}.\n\nPress Resolved once it is fixed, and {who} continues from where it stopped."
        );
        self.decide(DecisionRecord {
            text,
            ..self.new_decision(DecisionKind::Stopped, key, title)?
        })
    }

    /// Runs the procedure the owner approved, off the loop so the factory keeps working; the next
    /// pass takes its result (`take_recoveries`).
    pub(crate) fn run_recovery(
        &mut self,
        d: &DecisionRecord,
        key: Option<&str>,
    ) -> Result<(), DaemonError> {
        let Some(file) = d.procedure.clone() else {
            return Ok(());
        };
        let script = self.store.isolated().join("recoveries").join(&file);
        let log = script.with_extension("log");
        let place = if let Some(key) = key {
            self.worktree_of(key)
        } else {
            // Never the owner's own checkout. The base worktree the sprint's steps run in is used
            // as it is: the owner's approval is already recorded, so nothing that can fail on the
            // network stands between it and the procedure.
            let base = self.store.worktree(crate::worktree::BASE);
            if base.exists() {
                base
            } else {
                self.checkout_for(None)?
            }
        };
        self.store.write_isolated(
            &format!("{RUNNING}/{}.json", d.id),
            &Running {
                issue: key.map(str::to_owned),
            },
        )?;
        let out = fs::File::create(&log).map_err(StoreError::io(&log))?;
        let err = out.try_clone().map_err(StoreError::io(&log))?;
        self.event(&json!({"event": "recovery-started", "issue": key, "decision": d.id}))?;
        let done = self.recoveries.0.clone();
        let (key, decision) = (key.map(str::to_owned), d.id.clone());
        std::thread::spawn(move || {
            let succeeded = match Command::new(&script)
                .current_dir(&place)
                .stdin(Stdio::null())
                .stdout(out)
                .stderr(err)
                .status()
            {
                Ok(status) => status.success(),
                // A procedure that cannot start, such as one with no interpreter line, leaves
                // the reason in its log for the lead.
                Err(e) => {
                    let _ = fs::write(&log, format!("{}: {e}\n", script.display()));
                    false
                }
            };
            let _ = done.send(Recovered {
                key,
                decision,
                succeeded,
            });
        });
        Ok(())
    }

    /// Takes in the procedures that finished: a success resumes what stopped; a failure goes back
    /// to the lead with what the procedure printed, copied where the lead can read it.
    pub(crate) fn take_recoveries(&mut self) -> Result<(), DaemonError> {
        if !self.unfinished_recoveries_taken {
            self.take_unfinished_recoveries()?;
            self.unfinished_recoveries_taken = true;
        }
        while let Ok(r) = self.recoveries.1.try_recv() {
            let running = self
                .store
                .isolated()
                .join(format!("{RUNNING}/{}.json", r.decision));
            fs::remove_file(&running).map_err(StoreError::io(&running))?;
            self.event(&json!({"event": "recovery-ended", "issue": r.key, "decision": r.decision, "succeeded": r.succeeded}))?;
            if r.succeeded {
                match &r.key {
                    Some(key) => self.resume_issue(key)?,
                    None => self.resume_sprint()?,
                }
                continue;
            }
            let log = self
                .store
                .isolated()
                .join("recoveries")
                .join(format!("{}.log", r.decision));
            let printed = fs::read_to_string(&log).unwrap_or_default();
            let source = self.write_source(
                &format!("recoveries/{}.log", r.decision),
                &format!("# What the recovery procedure printed\n\n```\n{printed}\n```\n"),
            )?;
            match &r.key {
                Some(key) => {
                    self.update_issue(key, |rec| rec.recovery_log = Some(source.clone()))?;
                    self.apply(key, Fact::RecoveryFailed)?;
                }
                None => self.sprint_fact(&SprintFact::RecoveryFailed)?,
            }
        }
        Ok(())
    }

    /// Takes in, as failed, each procedure an earlier run left without a result. It is not run
    /// again: some procedures cannot run twice.
    fn take_unfinished_recoveries(&mut self) -> Result<(), DaemonError> {
        for decision in self.store.isolated_names(RUNNING)? {
            let Some(running) = self
                .store
                .read_isolated::<Running>(&format!("{RUNNING}/{decision}.json"))?
            else {
                continue;
            };
            let log = self
                .store
                .isolated()
                .join("recoveries")
                .join(format!("{decision}.log"));
            let mut printed = fs::read_to_string(&log).unwrap_or_default();
            printed.push_str(
                "\n[The factory stopped while this procedure ran, so its result is unknown: it may have finished, failed partway, or still be running.]\n",
            );
            fs::write(&log, printed).map_err(StoreError::io(&log))?;
            let _ = self.recoveries.0.send(Recovered {
                key: running.issue,
                decision,
                succeeded: false,
            });
        }
        Ok(())
    }

    /// The owner fixed it and pressed Resolved: what stopped continues from where it stopped.
    pub(crate) fn resolved(
        &mut self,
        d: DecisionRecord,
        key: Option<&str>,
        user: &str,
    ) -> Result<(), DaemonError> {
        self.resolve(
            d,
            Answer {
                kind: AnswerKind::Resolved,
                text: None,
                user: user.into(),
                at: jiff::Timestamp::now().to_string(),
            },
            &format!("Resolved by <@{user}>."),
        )?;
        match key {
            Some(key) => self.resume_issue(key),
            None => self.resume_sprint(),
        }
    }
}
