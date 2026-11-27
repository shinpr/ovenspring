//! The tracker read, filter judgment and publication of issue snapshots.

use std::collections::HashSet;
use std::fs;
use std::io::{BufRead as _, Write as _};
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::path::Path;
use std::process::{Child, ChildStdout, Command, ExitStatus, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use ovenspring_core::sprint::SprintStage;
use ovenspring_integrations::tracker::{TrackerComment, TrackerIssue};
use ovenspring_store::StoreError;
use serde::Serialize;
use serde_json::json;
use sha2::{Digest as _, Sha256};

use crate::app::stop_group;
use crate::errors::{ErrorKey, Failure, out_of_reach};
use crate::{Daemon, DaemonError};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Outcome {
    Admitted,
    Dropped(String),
    Failed(String),
}

pub(crate) struct Judged {
    hash: [u8; 32],
    outcome: Outcome,
    logged: bool,
}

pub(crate) struct FilterRun {
    key: String,
    hash: [u8; 32],
    pid: Option<u32>,
    void: bool,
    pub(crate) secrets: Vec<String>,
    handle: JoinHandle<Outcome>,
}

#[derive(Serialize)]
struct FilterInput {
    key: String,
    title: String,
    body: String,
    url: String,
    labels: Vec<String>,
    author: String,
    comments: Vec<FilterComment>,
}

#[derive(Serialize)]
struct FilterComment {
    author: String,
    body: String,
}

impl From<&TrackerIssue> for FilterInput {
    fn from(issue: &TrackerIssue) -> Self {
        FilterInput {
            key: issue.key.clone(),
            title: issue.title.clone(),
            body: issue.body.clone(),
            url: issue.url.clone(),
            labels: issue.labels.clone(),
            author: issue.author.clone(),
            comments: issue
                .comments
                .iter()
                .map(|comment: &TrackerComment| FilterComment {
                    author: comment.author.clone(),
                    body: comment.body.clone(),
                })
                .collect(),
        }
    }
}

fn encoded_input(issue: &TrackerIssue) -> Result<(Vec<u8>, [u8; 32]), DaemonError> {
    let bytes = serde_json::to_vec(&FilterInput::from(issue)).map_err(|error| {
        DaemonError::Invalid(format!("serializing tracker issue for filter: {error}"))
    })?;
    let hash = Sha256::digest(&bytes).into();
    Ok((bytes, hash))
}

fn spawn_filter(command: &str, repo: &Path) -> std::io::Result<Child> {
    Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(repo)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
}

/// Runs the command's child without blocking the daemon's polling thread.
fn judge(mut child: Child, input: Vec<u8>, limit: Duration) -> Outcome {
    let deadline = Instant::now() + limit;
    let pid = child.id();
    let Some(mut stdin) = child.stdin.take() else {
        stop_group(pid);
        return Outcome::Failed("could not write filter input".into());
    };
    let Some(stdout) = child.stdout.take() else {
        stop_group(pid);
        return Outcome::Failed("could not read filter output".into());
    };
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let reader = std::thread::spawn(move || read_first_line(stdout));
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) if reader.is_finished() => break status,
            Ok(_) => {}
            Err(error) => {
                stop_group(pid);
                let _ = child.wait();
                let _ = reader.join();
                return Outcome::Failed(format!("could not wait for filter: {error}"));
            }
        }
        if Instant::now() >= deadline {
            stop_group(pid);
            let _ = child.wait();
            let _ = reader.join();
            return Outcome::Failed(format!("ran longer than {} seconds", limit.as_secs_f64()));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    if !writer.is_finished() {
        // A descendant can keep the input pipe open without draining it after the shell has
        // exited. Stop that leftover process group so the completed verdict cannot strand a writer.
        stop_group(pid);
    }
    let _ = writer.join();
    let first_line = match reader.join() {
        Ok(Ok(line)) => line,
        Ok(Err(error)) => return Outcome::Failed(format!("could not read filter output: {error}")),
        Err(_) => return Outcome::Failed("filter output reader stopped unexpectedly".into()),
    };
    classify(status, first_line)
}

fn read_first_line(mut stdout: ChildStdout) -> std::io::Result<Vec<u8>> {
    let mut stdout = std::io::BufReader::new(&mut stdout);
    let mut first_line = Vec::new();
    stdout.read_until(b'\n', &mut first_line)?;
    std::io::copy(&mut stdout, &mut std::io::sink())?;
    Ok(first_line)
}

fn classify(status: ExitStatus, mut first_line: Vec<u8>) -> Outcome {
    match status.code() {
        Some(0) => Outcome::Admitted,
        Some(2) => {
            if first_line.last() == Some(&b'\n') {
                first_line.pop();
                if first_line.last() == Some(&b'\r') {
                    first_line.pop();
                }
            }
            Outcome::Dropped(String::from_utf8_lossy(&first_line).into_owned())
        }
        Some(code) => Outcome::Failed(format!("exited with status {code}")),
        None => Outcome::Failed(format!(
            "ended by signal {}",
            status
                .signal()
                .map_or_else(|| "unknown".into(), |signal| signal.to_string())
        )),
    }
}

impl Daemon {
    /// Invalidates judgments after a live filter change without doing intake work before the halt check.
    pub(crate) fn filter_changed(&mut self) {
        self.verdicts.clear();
        if let Some(run) = self.filter_run.as_mut() {
            run.void = true;
        }
        self.provisional = if self.config.tracker.filter.is_some() {
            self.tracker_issues
                .iter()
                .map(|(issue, _)| issue.key.clone())
                .collect()
        } else {
            HashSet::new()
        };
        self.last_tracker_poll = None;
    }

    /// Reads the tracker, remembers the exact filter inputs, and reconciles visible snapshots.
    pub(crate) fn poll_tracker(&mut self) -> Result<(), DaemonError> {
        let Some(tracker) = self.services.tracker.clone() else {
            return Ok(());
        };
        let kind = tracker.kind;
        match tracker.client.open_issues() {
            Ok(mut issues) => {
                // An issue someone else already started is theirs; the factory's own stay.
                issues.retain(|issue| {
                    !issue.started || self.issue(&issue.key).is_ok_and(|record| record.is_some())
                });
                let keys: HashSet<String> = issues.iter().map(|issue| issue.key.clone()).collect();
                self.verdicts.retain(|key, _| keys.contains(key));
                self.provisional.retain(|key| keys.contains(key));
                let tracker_issues: Vec<(TrackerIssue, [u8; 32])> = issues
                    .iter()
                    .map(|issue| encoded_input(issue).map(|(_, hash)| (issue.clone(), hash)))
                    .collect::<Result<_, _>>()?;
                if let Some(run) = self.filter_run.as_mut()
                    && !keys.contains(&run.key)
                {
                    run.void = true;
                }
                self.tracker_issues = tracker_issues;
                self.tracker_read = true;
                self.tracker_error = None;
                self.succeeded(ErrorKey::TrackerUnreachable, &json!({"tracker": kind}))?;
            }
            // Not answering is the service's or the network's: shown as out of reach, not as a
            // connection to fix.
            Err(error) => {
                if let Some((why, cause)) = out_of_reach(&error) {
                    self.failed(
                        ErrorKey::TrackerUnreachable,
                        Failure {
                            event: json!({"event": "service-unreachable", "operation": "reading the tracker",
                                          "error": error.to_string(), "why": why, "tracker": kind}),
                            cause,
                            via: None,
                        },
                    )?;
                } else {
                    // A refusal is an answer.
                    self.tracker_error = Some(error.to_string());
                    self.succeeded(ErrorKey::TrackerUnreachable, &json!({"tracker": kind}))?;
                    self.event(&json!({"event": "tracker-error", "error": error.to_string()}))?;
                }
            }
        }
        self.intake_pass()
    }

    /// Applies any current verdicts, updates the open list and starts the next eligible judgment.
    pub(crate) fn intake_pass(&mut self) -> Result<(), DaemonError> {
        let configured = self.config.tracker.filter.is_some();
        let proposing = self
            .sprint()?
            .is_some_and(|sprint| sprint.stage == SprintStage::Proposing);
        let issues = self.tracker_issues.clone();
        if !proposing {
            for (issue, hash) in &issues {
                if self.selected(&issue.key)? {
                    continue;
                }
                let verdict = if configured {
                    self.verdicts
                        .get(&issue.key)
                        .filter(|judged| judged.hash == *hash)
                        .map(|judged| judged.outcome.clone())
                } else {
                    Some(Outcome::Admitted)
                };
                match verdict {
                    Some(Outcome::Admitted) => {
                        self.write_source(&format!("issues/{}.md", issue.key), &issue.render())?;
                    }
                    Some(Outcome::Failed(error)) => {
                        self.write_source(&format!("issues/{}.md", issue.key), &issue.render())?;
                        self.log_verdict(&issue.key, "filter-failed", &json!({"error": error}))?;
                    }
                    Some(Outcome::Dropped(reason)) => {
                        let path = self.store.source(&format!("issues/{}.md", issue.key));
                        if path.exists() {
                            fs::remove_file(&path).map_err(StoreError::io(&path))?;
                        }
                        self.log_verdict(
                            &issue.key,
                            "filter-dropped",
                            &json!({"url": issue.url, "reason": reason}),
                        )?;
                    }
                    None if self.provisional.contains(&issue.key) => {
                        self.write_source(&format!("issues/{}.md", issue.key), &issue.render())?;
                    }
                    None => {}
                }
            }
            if self.tracker_read {
                let open: Vec<&str> = issues.iter().map(|(issue, _)| issue.key.as_str()).collect();
                self.forget_closed_sources(&open)?;
            }
        }
        let published: Vec<TrackerIssue> = issues
            .into_iter()
            .filter_map(|(issue, _)| {
                self.store
                    .source(&format!("issues/{}.md", issue.key))
                    .exists()
                    .then_some(issue)
            })
            .collect();
        let known: Vec<&str> = self
            .open_issues
            .iter()
            .map(|issue| issue.key.as_str())
            .collect();
        let open: Vec<&str> = published.iter().map(|issue| issue.key.as_str()).collect();
        let opened = open.iter().any(|key| !known.contains(key));
        if open != known {
            self.event(&json!({"event": "tracker-issues", "open": open}))?;
        }
        self.open_issues = published;
        // A sprint that found no issue to propose proposes again once one opens.
        if opened {
            self.sprint_fact(&ovenspring_core::sprint::SprintFact::IssueOpened)?;
        }
        self.start_next_filter_run()
    }

    fn log_verdict(
        &mut self,
        key: &str,
        event: &str,
        fields: &serde_json::Value,
    ) -> Result<(), DaemonError> {
        let Some(judged) = self.verdicts.get(key) else {
            return Ok(());
        };
        if judged.logged {
            return Ok(());
        }
        let mut body = json!({"event": event, "issue": key});
        if let (Some(body), Some(fields)) = (body.as_object_mut(), fields.as_object()) {
            body.extend(
                fields
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone())),
            );
        }
        self.event(&body)?;
        if let Some(judged) = self.verdicts.get_mut(key) {
            judged.logged = true;
        }
        Ok(())
    }

    fn start_next_filter_run(&mut self) -> Result<(), DaemonError> {
        if self.filter_run.is_some() {
            return Ok(());
        }
        let Some(command) = self.config.tracker.filter.clone() else {
            return Ok(());
        };
        let proposing = self
            .sprint()?
            .is_some_and(|sprint| sprint.stage == SprintStage::Proposing);
        let mut candidate = None;
        for (issue, hash) in &self.tracker_issues {
            if self.selected(&issue.key)? {
                continue;
            }
            let current = self
                .verdicts
                .get(&issue.key)
                .is_some_and(|judged| judged.hash == *hash);
            if current {
                continue;
            }
            let snapshot = self.store.source(&format!("issues/{}.md", issue.key));
            if proposing && snapshot.exists() {
                continue;
            }
            candidate = Some((issue.clone(), *hash));
            break;
        }
        let Some((issue, hash)) = candidate else {
            return Ok(());
        };
        let (input, _) = encoded_input(&issue)?;
        let result = spawn_filter(&command, &self.paths.repo);
        let pid = result.as_ref().ok().map(Child::id);
        let timeout = self.timing.filter_timeout;
        let handle = match result {
            Ok(child) => std::thread::spawn(move || judge(child, input, timeout)),
            Err(error) => {
                std::thread::spawn(move || Outcome::Failed(format!("could not start: {error}")))
            }
        };
        self.filter_run = Some(FilterRun {
            key: issue.key,
            hash,
            pid,
            void: false,
            secrets: self.services.secrets.clone(),
            handle,
        });
        Ok(())
    }

    pub(crate) fn take_filter_run(&mut self) -> Result<(), DaemonError> {
        if !self
            .filter_run
            .as_ref()
            .is_some_and(|run| run.handle.is_finished())
        {
            return Ok(());
        }
        let Some(run) = self.filter_run.take() else {
            return Ok(());
        };
        let outcome = run.handle.join().ok();
        let current = self
            .tracker_issues
            .iter()
            .any(|(issue, hash)| issue.key == run.key && *hash == run.hash);
        if !run.void
            && current
            && !self.selected(&run.key)?
            && let Some(outcome) = outcome
        {
            self.provisional.remove(&run.key);
            let outcome = match outcome {
                Outcome::Dropped(reason) => {
                    let secrets: Vec<&str> = run.secrets.iter().map(String::as_str).collect();
                    Outcome::Dropped(crate::download::mask(&reason, &secrets))
                }
                outcome @ (Outcome::Admitted | Outcome::Failed(_)) => outcome,
            };
            self.verdicts.insert(
                run.key,
                Judged {
                    hash: run.hash,
                    outcome,
                    logged: false,
                },
            );
        }
        self.intake_pass()
    }

    pub(crate) fn stop_filter(&mut self) {
        let Some(run) = self.filter_run.take() else {
            return;
        };
        if !run.handle.is_finished()
            && let Some(pid) = run.pid
        {
            stop_group(pid);
        }
        let _ = run.handle.join();
    }

    /// Removes the snapshot of each issue the tracker no longer reports open, so a reader never
    /// takes an old state for the current one. An issue the factory is still working on keeps its
    /// snapshot: its steps read the requester's words from it. So does an issue in the current
    /// proposal that has no record yet: approving it takes the title and URL from that snapshot.
    fn forget_closed_sources(&self, open: &[&str]) -> Result<(), DaemonError> {
        let dir = self.store.source("issues");
        let current_plan = self.sprint()?.and_then(|sprint| sprint.plan);
        let entries = fs::read_dir(&dir).map_err(StoreError::io(&dir))?;
        for entry in entries {
            let path = entry.map_err(StoreError::io(&dir))?.path();
            let Some(key) = path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_suffix(".md"))
            else {
                continue;
            };
            let record = self.issue(key)?;
            let record_is_unfinished = record
                .as_ref()
                .is_some_and(|record| !record.flow.stage.is_finished());
            let proposed_without_record = record.is_none()
                && current_plan
                    .as_ref()
                    .is_some_and(|plan| plan.issues.iter().any(|issue| issue.key == key));
            if !open.contains(&key) && !record_is_unfinished && !proposed_without_record {
                fs::remove_file(&path).map_err(StoreError::io(&path))?;
            }
        }
        Ok(())
    }

    fn selected(&self, key: &str) -> Result<bool, DaemonError> {
        let in_plan = self
            .sprint()?
            .and_then(|sprint| sprint.plan)
            .is_some_and(|plan| plan.issues.iter().any(|issue| issue.key == key));
        Ok(in_plan || self.issue(key)?.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn child(script: &str, repo: &Path) -> Child {
        spawn_filter(script, repo).unwrap()
    }

    #[test]
    fn intake_judge_classifies_exit_zero_and_other_statuses() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            judge(child("exit 0", dir.path()), vec![], Duration::from_secs(1)),
            Outcome::Admitted
        );
        assert_eq!(
            judge(child("exit 1", dir.path()), vec![], Duration::from_secs(1)),
            Outcome::Failed("exited with status 1".into())
        );
        assert_eq!(
            judge(
                child("__missing_filter_interpreter__", dir.path()),
                vec![],
                Duration::from_secs(1),
            ),
            Outcome::Failed("exited with status 127".into())
        );
        assert_eq!(
            judge(child("exit 2", dir.path()), vec![], Duration::from_secs(1)),
            Outcome::Dropped(String::new())
        );
        assert_eq!(
            judge(
                child("kill -TERM $$", dir.path()),
                vec![],
                Duration::from_secs(1)
            ),
            Outcome::Failed("ended by signal 15".into())
        );
    }

    #[test]
    fn intake_judge_keeps_the_first_stdout_line_and_its_whitespace() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            judge(
                child(
                    "printf '  keep these spaces  \\r\\nsecond'; exit 2",
                    dir.path()
                ),
                vec![],
                Duration::from_secs(1),
            ),
            Outcome::Dropped("  keep these spaces  ".into())
        );
        assert_eq!(
            judge(
                child("printf 'reason\\r'; exit 2", dir.path()),
                vec![],
                Duration::from_secs(1),
            ),
            Outcome::Dropped("reason\r".into())
        );
    }

    #[test]
    fn intake_judge_keeps_a_long_first_line_whole() {
        let dir = tempfile::tempdir().unwrap();
        let line = "r".repeat(5_000);
        let command = format!("printf '{line}\\nsecond'; exit 2");
        assert_eq!(
            judge(child(&command, dir.path()), vec![], Duration::from_secs(1)),
            Outcome::Dropped(line)
        );
    }

    #[test]
    fn intake_judge_treats_a_missing_working_directory_as_a_start_failure() {
        let dir = tempfile::tempdir().unwrap();
        let absent = dir.path().join("missing");
        let outcome = spawn_filter("exit 0", &absent).map_or_else(
            |error| Outcome::Failed(format!("could not start: {error}")),
            |child| judge(child, vec![], Duration::from_secs(1)),
        );
        assert!(
            matches!(outcome, Outcome::Failed(message) if message.starts_with("could not start: "))
        );
    }

    #[test]
    fn intake_judge_times_out_and_stops_the_process_group() {
        let dir = tempfile::tempdir().unwrap();
        let child_pid_file = dir.path().join("child.pid");
        let command = format!("sleep 30 & echo $! > {}; wait", child_pid_file.display());
        let process = child(&command, dir.path());
        let pid = process.id();
        assert_eq!(
            judge(process, vec![], Duration::from_secs(2)),
            Outcome::Failed("ran longer than 2 seconds".into())
        );
        let child_pid = fs::read_to_string(child_pid_file).unwrap();
        assert!(!process_exists(child_pid.trim()));
        assert!(!process_exists(&pid.to_string()));
    }

    #[test]
    fn intake_judge_returns_when_a_descendant_holds_stdin_without_draining_it() {
        let dir = tempfile::tempdir().unwrap();
        let command = "head -c 20 >/dev/null\nsleep 30 <&0 >/dev/null 2>&1 &\nprintf 'large issue denied\\n'\nexit 2";
        let started = Instant::now();
        let outcome = judge(
            child(command, dir.path()),
            vec![b'x'; 1024 * 1024],
            Duration::from_secs(20),
        );
        assert_eq!(outcome, Outcome::Dropped("large issue denied".into()));
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    fn process_exists(pid: &str) -> bool {
        pid.parse::<u32>().is_ok_and(crate::app::alive)
    }
}
