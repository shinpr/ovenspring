//! The tracker read, filter judgment and publication of issue snapshots.

use std::collections::HashSet;
use std::fs;
use std::io::{BufRead as _, Read, Write as _};
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use ovenspring_core::sprint::SprintStage;
use ovenspring_integrations::tracker::{TrackerComment, TrackerIssue};
use ovenspring_store::StoreError;
use serde::Serialize;
use serde_json::json;
use sha2::{Digest as _, Sha256};

use crate::app::stop_group;
use crate::errors::{ErrorKey, Failure, cut, out_of_reach, refused_cause};
use crate::{Daemon, DaemonError};

/// How a run of the filter command ended, as the command said it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Outcome {
    Admitted,
    Dropped(String),
    /// `output` is the first line the command wrote to stderr, empty when it wrote none.
    Failed {
        error: String,
        output: String,
    },
}

impl Outcome {
    fn failed(error: impl Into<String>) -> Outcome {
        Outcome::Failed {
            error: error.into(),
            output: String::new(),
        }
    }
}

/// A judgment as the daemon keeps it until it is published: every text the command or its
/// setting supplied already masked with the credentials held during the run, so a credential
/// replaced before the verdict is logged never reaches a line.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Verdict {
    Admitted,
    Dropped(String),
    Failed(FilterFailure),
}

/// What a failed judgment records, each text masked and cut.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FilterFailure {
    error: String,
    /// Stderr's first line, empty when it printed none.
    output: String,
    command: String,
}

impl FilterFailure {
    /// The failure of the filter's error that a failed judgment of `key` is.
    fn of_issue(&self, key: &str) -> Failure {
        let mut event = json!({"event": "filter-failed", "issue": key, "error": self.error,
                               "command": self.command});
        let cause = if self.output.is_empty() {
            self.error.clone()
        } else {
            if let Some(event) = event.as_object_mut() {
                event.insert("output".into(), json!(self.output));
            }
            format!("{} ({})", self.error, self.output)
        };
        Failure {
            event,
            cause,
            via: None,
            reason: None,
        }
    }
}

pub(crate) struct Judged {
    hash: [u8; 32],
    verdict: Verdict,
    logged: bool,
}

pub(crate) struct FilterRun {
    key: String,
    hash: [u8; 32],
    /// The command that runs; a changed setting voids the run.
    command: String,
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
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
}

/// The most of stderr's first line kept, in bytes: the failure's text is cut far shorter.
const STDERR_LINE_MAX: u64 = 4 * 1024;

/// Runs the command's child without blocking the daemon's polling thread.
fn judge(mut child: Child, input: Vec<u8>, limit: Duration) -> Outcome {
    let deadline = Instant::now() + limit;
    let pid = child.id();
    let (Some(mut stdin), Some(stdout), Some(stderr)) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        stop_group(pid);
        return Outcome::failed("could not connect to the filter's input and output");
    };
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let reader = std::thread::spawn(move || read_first_line(stdout, u64::MAX));
    let errors = std::thread::spawn(move || read_first_line(stderr, STDERR_LINE_MAX));
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) if reader.is_finished() && errors.is_finished() => break status,
            Ok(_) => {}
            Err(error) => {
                stop_group(pid);
                let _ = child.wait();
                let _ = reader.join();
                let _ = errors.join();
                return Outcome::failed(format!("could not wait for filter: {error}"));
            }
        }
        if Instant::now() >= deadline {
            stop_group(pid);
            let _ = child.wait();
            let _ = reader.join();
            let _ = errors.join();
            return Outcome::failed(format!("ran longer than {} seconds", limit.as_secs_f64()));
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
        Ok(Err(error)) => return Outcome::failed(format!("could not read filter output: {error}")),
        Err(_) => return Outcome::failed("filter output reader stopped unexpectedly"),
    };
    // What stderr said only explains a failure, so one that cannot be read leaves it unexplained.
    let error_line = errors.join().ok().and_then(Result::ok).unwrap_or_default();
    classify(status, first_line, error_line)
}

/// The first line of `stream`, up to `limit` bytes, with the rest drained so the command never
/// blocks on a full pipe.
fn read_first_line(stream: impl Read, limit: u64) -> std::io::Result<Vec<u8>> {
    let mut stream = std::io::BufReader::new(stream);
    let mut first_line = Vec::new();
    (&mut stream)
        .take(limit)
        .read_until(b'\n', &mut first_line)?;
    std::io::copy(&mut stream, &mut std::io::sink())?;
    Ok(first_line)
}

/// A line's text without the line ending that closed it.
fn line_text(mut line: Vec<u8>) -> String {
    if line.last() == Some(&b'\n') {
        line.pop();
        if line.last() == Some(&b'\r') {
            line.pop();
        }
    }
    String::from_utf8_lossy(&line).into_owned()
}

fn classify(status: ExitStatus, first_line: Vec<u8>, error_line: Vec<u8>) -> Outcome {
    let error = match status.code() {
        Some(0) => return Outcome::Admitted,
        Some(2) => return Outcome::Dropped(line_text(first_line)),
        Some(code) => format!("exited with status {code}"),
        None => format!(
            "ended by signal {}",
            status
                .signal()
                .map_or_else(|| "unknown".into(), |signal| signal.to_string())
        ),
    };
    Outcome::Failed {
        error,
        output: line_text(error_line),
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
                self.succeeded(ErrorKey::TrackerRefused, &json!({"tracker": kind}))?;
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
                            reason: None,
                        },
                    )?;
                } else {
                    // A refusal is an answer. Now shows it while this connection is the one in
                    // use; the error stays open until the tracker is read, whatever connection
                    // reads it.
                    self.tracker_error = Some(error.to_string());
                    self.succeeded(ErrorKey::TrackerUnreachable, &json!({"tracker": kind}))?;
                    self.failed(
                        ErrorKey::TrackerRefused,
                        Failure {
                            event: json!({"event": "tracker-error", "error": error.to_string(),
                                          "tracker": kind}),
                            cause: refused_cause(&error),
                            via: None,
                            reason: None,
                        },
                    )?;
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
                        .map(|judged| judged.verdict.clone())
                } else {
                    Some(Verdict::Admitted)
                };
                match verdict {
                    Some(Verdict::Admitted) => {
                        self.write_source(&format!("issues/{}.md", issue.key), &issue.render())?;
                    }
                    Some(Verdict::Failed(failure)) => {
                        self.write_source(&format!("issues/{}.md", issue.key), &issue.render())?;
                        if self.unlogged(&issue.key) {
                            self.failed(ErrorKey::Filter, failure.of_issue(&issue.key))?;
                            self.mark_logged(&issue.key);
                        }
                    }
                    Some(Verdict::Dropped(reason)) => {
                        let path = self.store.source(&format!("issues/{}.md", issue.key));
                        if path.exists() {
                            fs::remove_file(&path).map_err(StoreError::io(&path))?;
                        }
                        if self.unlogged(&issue.key) {
                            self.event(&json!({"event": "filter-dropped", "issue": issue.key,
                                               "url": issue.url, "reason": reason}))?;
                            self.mark_logged(&issue.key);
                        }
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

    /// Whether the verdict for `key` is still to be recorded: each judgment is logged once.
    fn unlogged(&self, key: &str) -> bool {
        self.verdicts.get(key).is_some_and(|judged| !judged.logged)
    }

    fn mark_logged(&mut self, key: &str) {
        if let Some(judged) = self.verdicts.get_mut(key) {
            judged.logged = true;
        }
    }

    /// Ends the filter's error once a judgment shows the filter works, or once no filter is
    /// configured. Runs every pass, so a clear whose line could not be appended is recorded by a
    /// later pass even though the cached verdict never runs the command again.
    pub(crate) fn clear_filter(&mut self) -> Result<(), DaemonError> {
        if let Some(issue) = self.filter_worked.clone() {
            self.succeeded(ErrorKey::Filter, &json!({"issue": issue}))?;
            self.filter_worked = None;
        }
        if self.config.tracker.filter.is_none() {
            self.succeeded(ErrorKey::Filter, &json!({"removed": true}))?;
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
                std::thread::spawn(move || Outcome::failed(format!("could not start: {error}")))
            }
        };
        self.filter_run = Some(FilterRun {
            key: issue.key,
            hash,
            command,
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
            let secrets: Vec<&str> = run.secrets.iter().map(String::as_str).collect();
            let mask = |text: &str| crate::download::mask(text, &secrets);
            // A failure's texts are cut only after masking, so a cut never leaves part of a
            // credential.
            let verdict = match outcome {
                Outcome::Admitted => Verdict::Admitted,
                Outcome::Dropped(reason) => Verdict::Dropped(mask(&reason)),
                Outcome::Failed { error, output } => Verdict::Failed(FilterFailure {
                    error: cut(&mask(&error)),
                    output: cut(&mask(&output)),
                    command: cut(&mask(&run.command)),
                }),
            };
            // A fresh judgment that admits or drops shows the filter works; one reapplied from
            // the cache is an earlier judgment and shows nothing.
            if matches!(verdict, Verdict::Admitted | Verdict::Dropped(_)) {
                self.filter_worked = Some(run.key.clone());
            }
            self.verdicts.insert(
                run.key,
                Judged {
                    hash: run.hash,
                    verdict,
                    logged: false,
                },
            );
            self.clear_filter()?;
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
            Outcome::failed("exited with status 1")
        );
        assert!(matches!(
            judge(
                child("__missing_filter_interpreter__", dir.path()),
                vec![],
                Duration::from_secs(1),
            ),
            Outcome::Failed { error, output }
                if error == "exited with status 127" && output.contains("__missing_filter_interpreter__")
        ));
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
            Outcome::failed("ended by signal 15")
        );
    }

    #[test]
    fn intake_judge_keeps_the_first_stderr_line_of_a_failure_up_to_its_limit() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            judge(
                child(
                    "printf 'token expired\\r\\nsecond\\n' >&2; exit 1",
                    dir.path()
                ),
                vec![],
                Duration::from_secs(1),
            ),
            Outcome::Failed {
                error: "exited with status 1".into(),
                output: "token expired".into()
            }
        );
        let long = "e".repeat(10_000);
        assert_eq!(
            judge(
                child(&format!("printf '{long}\\n' >&2; exit 1"), dir.path()),
                vec![],
                Duration::from_secs(1),
            ),
            Outcome::Failed {
                error: "exited with status 1".into(),
                output: "e".repeat(4 * 1024)
            }
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
            |error| Outcome::failed(format!("could not start: {error}")),
            |child| judge(child, vec![], Duration::from_secs(1)),
        );
        assert!(
            matches!(outcome, Outcome::Failed { error, .. } if error.starts_with("could not start: "))
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
            Outcome::failed("ran longer than 2 seconds")
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
