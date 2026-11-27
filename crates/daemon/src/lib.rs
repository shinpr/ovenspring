//! The resident process: it launches one agent per open board item, turns each finished item
//! into a fact of its issue's flow, and carries out what the flow returns.

#![cfg_attr(
    test,
    allow(
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used,
        clippy::expect_used,
        reason = "Test helpers use concise assertions; production paths remain strict."
    )
)]

pub mod app;
pub mod conversation;
pub mod download;
pub mod errors;
pub mod events;
pub(crate) mod handoff;
pub mod history;
pub(crate) mod intake;
pub mod issues;
pub mod overview;
pub mod owner;
pub mod prerequisites;
pub mod product_run;
pub mod pulls;
pub mod records;
pub(crate) mod recovery;
pub mod sprint;
pub(crate) mod stops;
pub mod worktree;

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::Write as _;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use ovenspring_agents::assets;
use ovenspring_agents::backend::{self, CodexUserConfig, Failure, Launch};
use ovenspring_agents::guard::GuardRules;
use ovenspring_agents::prompt::{self, PromptContext};
use ovenspring_core::config::{Cli, Config};
use ovenspring_core::item::{Item, Status};
use ovenspring_integrations::channel::Channel;
use ovenspring_integrations::github::GitHub;
use ovenspring_integrations::http::HttpError;
use ovenspring_integrations::tracker::Tracker;
use ovenspring_store::{Store, StoreError, item_name};
use serde_json::json;

use crate::errors::ErrorKey;

#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Git(#[from] worktree::GitError),
    #[error("{0}")]
    Launch(String),
    #[error("no record for {0}")]
    NoRecord(String),
    /// Input from the owner that breaks a rule; the dashboard shows the message.
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    GitHub(#[from] HttpError),
}

/// The services only the daemon talks to.
pub struct Services {
    pub github: Option<Arc<dyn GitHub>>,
    pub channel: Arc<dyn Channel>,
    pub tracker: Option<ConnectedTracker>,
    /// Credential values held by the daemon, for masking owner-command output.
    pub secrets: Vec<String>,
}

/// The tracker client, with the kind it was built for: the tracker a write through it reaches,
/// which the configuration can stop naming before the services are rebuilt.
#[derive(Clone)]
pub struct ConnectedTracker {
    pub kind: ovenspring_core::config::TrackerKind,
    pub client: Arc<dyn Tracker>,
}

/// How long the daemon waits between looks and before trying again. Production uses
/// [`Timing::default`]; tests shorten it.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    /// Between looks at GitHub for issues waiting on it.
    pub github_poll: Duration,
    /// Between reads of the tracker.
    pub tracker_poll: Duration,
    /// Maximum time allowed for one tracker-filter command.
    pub filter_timeout: Duration,
    /// Before a launch that failed on an overloaded provider runs again.
    pub retry_wait: Duration,
    /// The longest wait for a review bot whose review is under way.
    pub bot_pending: Duration,
    /// The longest wait for a review bot that has left no trace on the pull request.
    pub bot_absent: Duration,
    /// Between reminders of what waits on the owner.
    pub remind: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            github_poll: Duration::from_secs(30),
            tracker_poll: Duration::from_secs(60),
            filter_timeout: Duration::from_secs(60),
            retry_wait: Duration::from_secs(300),
            bot_pending: Duration::from_secs(30 * 60),
            bot_absent: Duration::from_secs(3 * 60),
            remind: Duration::from_secs(60 * 60),
        }
    }
}

/// How many times a launch that failed on an overloaded provider runs again.
pub const RETRIES: u32 = 3;

/// Where the daemon finds what it runs and where it writes.
#[derive(Clone)]
pub struct Paths {
    pub repo: PathBuf,
    /// The `ovenspring` executable agents and hooks call.
    pub ovenspring: PathBuf,
    /// Directories put first on launches' `PATH`, after the executable's own.
    pub agent_path: Vec<PathBuf>,
    /// The owner's home, where Claude Code keeps its sessions and the owner's MCP definitions.
    pub home: PathBuf,
    /// Codex's home (`$CODEX_HOME`, else `~/.codex`): its sessions and configuration.
    pub codex_home: PathBuf,
    /// The dashboard's address (`http://host:port`), which messages to the owner link to.
    pub dashboard: Option<String>,
}

pub(crate) struct Running {
    pid: u32,
    /// The launch continued an earlier session (a design-phase work turn).
    resumed: bool,
    launch: Launched,
}

/// What a launch's end is read from: its CLI's stream in `log`, and what was known before it ran.
struct Launched {
    cli: Cli,
    attempt: u32,
    log: PathBuf,
    /// The session id the factory gave the launch, when its CLI takes one.
    session: Option<String>,
    /// The model the role names; `None` leaves it to the CLI.
    model: Option<String>,
}

struct Exit {
    item: String,
    success: bool,
    code: Option<i32>,
}

pub struct Daemon {
    paths: Paths,
    pub(crate) config: Config,
    pub(crate) store: Store,
    codex_user: CodexUserConfig,
    pub(crate) services: Services,
    pub(crate) running: HashMap<String, Running>,
    attempts: HashMap<String, u32>,
    /// Each item's transient launch failures in a row, which `RETRIES` bounds.
    transient_failures: HashMap<String, u32>,
    not_before: HashMap<String, Instant>,
    exits: (Sender<Exit>, Receiver<Exit>),
    pub(crate) timing: Timing,
    last_poll: Option<Instant>,
    last_tracker_poll: Option<Instant>,
    /// Earliest time another owner reminder could be due.
    next_reminder_check: Instant,
    /// Whether the tracker has answered since start.
    pub(crate) tracker_read: bool,
    pub(crate) open_issues: Vec<ovenspring_integrations::tracker::TrackerIssue>,
    /// The latest successful tracker read and the hash of each issue's filter input.
    pub(crate) tracker_issues: Vec<(ovenspring_integrations::tracker::TrackerIssue, [u8; 32])>,
    /// The latest judgment for each candidate's content under the current filter.
    pub(crate) verdicts: HashMap<String, intake::Judged>,
    /// Candidates published as without a filter until a changed command judges them.
    pub(crate) provisional: HashSet<String>,
    /// The one filter command currently running, if any.
    pub(crate) filter_run: Option<intake::FilterRun>,
    /// The issue whose fresh judgment showed the filter works, until the filter's error is
    /// recorded cleared by it.
    pub(crate) filter_worked: Option<String>,
    pub(crate) pause_announced: bool,
    /// The gate's last decision on the agreements pull request, logged once per change.
    pub(crate) agreements_waiting: Option<String>,
    /// Why the tracker's last read failed, while it fails.
    pub(crate) tracker_error: Option<String>,
    pub(crate) last_prerequisites: Option<Instant>,
    /// The configuration file's text the daemon last compared, so a saved change is read once.
    pub(crate) config_text: Option<String>,
    /// Held while a preview starts its app.
    pub(crate) previews: Arc<std::sync::Mutex<()>>,
    /// Failures in a row of one operation, by what owns it (`stops`).
    pub(crate) failures: HashMap<(stops::Owner, String), u32>,
    /// Recovery procedures that finished, from the threads that ran them.
    pub(crate) recoveries: (Sender<recovery::Recovered>, Receiver<recovery::Recovered>),
    /// Whether the procedures an earlier run left without a result have been taken in.
    pub(crate) unfinished_recoveries_taken: bool,
    /// The start of the product with the lead's commands, while it runs (`product_run`).
    pub(crate) product_check: Option<std::thread::JoinHandle<()>>,
    pub(crate) errors: errors::Live,
}

/// A filter command belongs to the daemon that started it, so no path that ends the daemon
/// without `stop_launches` (an early error return, a test that ends or panics) leaves it running.
impl Drop for Daemon {
    fn drop(&mut self) {
        self.stop_filter();
    }
}

fn unended_launch(lines: &[records::LaunchLine], item: &str) -> Option<records::LaunchStart> {
    let mut left: Option<records::LaunchStart> = None;
    for line in lines {
        match line {
            records::LaunchLine::Start(start) if start.item == item => left = Some(start.clone()),
            records::LaunchLine::End(end)
                if end.item == item && left.as_ref().is_some_and(|l| l.attempt == end.attempt) =>
            {
                left = None;
            }
            records::LaunchLine::Start(_) | records::LaunchLine::End(_) => {}
        }
    }
    left
}

/// Whether `pid` is still a process of `cli`, not another that took its number.
fn runs_cli(pid: u32, cli: Cli) -> bool {
    let name = serde_json::to_value(cli)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    std::process::Command::new("ps")
        .args(["-o", "command=", "-p", &pid.to_string()])
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains(&name))
}

/// What the daemon is built from.
pub struct Parts {
    pub paths: Paths,
    pub config: Config,
    pub store: Store,
    pub codex_user: CodexUserConfig,
    pub services: Services,
    pub timing: Timing,
}

impl Daemon {
    pub fn new(parts: Parts) -> Daemon {
        let pause_announced = parts.store.halt_file().exists();
        Daemon {
            paths: parts.paths,
            config: parts.config,
            store: parts.store,
            codex_user: parts.codex_user,
            services: parts.services,
            running: HashMap::new(),
            attempts: HashMap::new(),
            transient_failures: HashMap::new(),
            not_before: HashMap::new(),
            exits: channel(),
            timing: parts.timing,
            last_poll: None,
            last_tracker_poll: None,
            next_reminder_check: Instant::now() + parts.timing.remind,
            tracker_read: false,
            open_issues: Vec::new(),
            tracker_issues: Vec::new(),
            verdicts: HashMap::new(),
            provisional: HashSet::new(),
            filter_worked: None,
            filter_run: None,
            pause_announced,
            agreements_waiting: None,
            tracker_error: None,
            last_prerequisites: None,
            config_text: None,
            previews: Arc::default(),
            failures: HashMap::new(),
            recoveries: channel(),
            unfinished_recoveries_taken: false,
            product_check: None,
            errors: errors::Live::default(),
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Replaces the services after the owner changed a connection or its credentials; dropping
    /// the old channel closes its Slack connection.
    pub fn replace_services(&mut self, services: Services) {
        if let Some(run) = self.filter_run.as_mut() {
            run.secrets.extend(services.secrets.iter().cloned());
        }
        self.services = services;
        self.tracker_read = false;
        self.tracker_error = None;
        self.last_tracker_poll = None;
        self.last_prerequisites = None;
    }

    /// Reads the configuration again when its file changed, so a setting saved from the dashboard
    /// applies from the next launch. A file that does not hold a valid configuration
    /// leaves the last one in force and is logged once.
    fn reload_config(&mut self) -> Result<(), DaemonError> {
        let path = self.paths.repo.join(ovenspring_core::config::CONFIG_FILE);
        let text = fs::read_to_string(&path).map_err(StoreError::io(&path))?;
        if self.config_text.as_deref() == Some(text.as_str()) {
            return Ok(());
        }
        self.config_text = Some(text);
        match Config::load(&self.paths.repo) {
            Ok(config) if config == self.config => Ok(()),
            Ok(config) => {
                let filter_changed = config.tracker.filter != self.config.tracker.filter;
                let changes = config_changes(&self.config, &config)?;
                self.config = config;
                if filter_changed {
                    self.filter_changed();
                }
                self.last_prerequisites = None;
                self.event(&json!({"event": "config-reloaded", "changes": changes}))
            }
            Err(e) => self.event(&json!({"event": "config-refused", "error": e.to_string()})),
        }
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn halt_file(&self) -> PathBuf {
        self.store.halt_file()
    }

    /// Marks the item `doing` and starts its agent; an agent that cannot start leaves the item
    /// `open`, to be launched again on a later pass.
    fn start_process(
        &self,
        path: &Path,
        spec: &backend::CommandSpec,
        log: &Path,
    ) -> Result<std::process::Child, DaemonError> {
        self.store
            .update_item(path, |i| i.front.status = Status::Doing)?;
        spawn(spec, log, self.path_with_ovenspring()).or_else(|e| {
            self.store
                .update_item(path, |i| i.front.status = Status::Open)?;
            Err(DaemonError::Launch(format!(
                "starting {}: {e}",
                spec.program
            )))
        })
    }

    /// The number of `item`'s next launch: after those this process made and those that left a
    /// directory under `launches/`, so a restarted daemon keeps earlier logs.
    fn next_attempt(&self, item: &str) -> Result<u32, DaemonError> {
        let dir = self.store.root().join("launches");
        let prefix = format!("{item}.");
        let entries = fs::read_dir(&dir).map_err(StoreError::io(&dir))?;
        let mut highest = 0;
        for entry in entries.flatten() {
            let name = entry.file_name();
            if let Some(n) = name
                .to_str()
                .and_then(|n| n.strip_prefix(&prefix))
                .and_then(|n| n.parse::<u32>().ok())
            {
                highest = highest.max(n);
            }
        }
        Ok(highest.max(self.attempts.get(item).copied().unwrap_or(0)) + 1)
    }

    /// Stops every running launch and what it started, and the issues' apps, for the daemon's own
    /// stop: the items stay `doing` and run again on the next start, with no earlier agent still at
    /// work and no app left running that nothing manages.
    pub fn stop_launches(&mut self) {
        self.stop_filter();
        let names: Vec<String> = self.running.keys().cloned().collect();
        for name in names {
            if let Err(e) = self.stop_launch(&name) {
                eprintln!("ovenspring: recording a stopped launch: {e}");
            }
        }
        let apps = self.store.root().join("apps");
        for entry in fs::read_dir(&apps).into_iter().flatten().flatten() {
            self.stop_app(&entry.file_name().to_string_lossy());
        }
    }

    /// Items left `doing` by a previous process run again. A launch that process left running (it
    /// ended without stopping its launches) is stopped first, so no two agents work on one item.
    /// The errors going on are read back first, so a failure after a restart is a repeat.
    pub fn recover(&mut self) -> Result<(), DaemonError> {
        self.recover_errors()?;
        let lines = records::launch_lines(&self.store)?;
        for path in self.store.active_items()? {
            let item = self.store.read_item(&path)?;
            if item.front.status == Status::Doing {
                let name = item_name(&path);
                if let Some(left) = unended_launch(&lines, &name) {
                    if app::alive(left.pid) && runs_cli(left.pid, left.cli) {
                        app::stop_group(left.pid);
                    }
                    self.record_stopped(
                        &name,
                        &Launched {
                            cli: left.cli,
                            attempt: left.attempt,
                            log: PathBuf::from(&left.log),
                            session: None,
                            model: left.model.clone(),
                        },
                    )?;
                }
                self.store
                    .update_item(&path, |i| i.front.status = Status::Open)?;
                self.event(&json!({"event": "relaunch-after-restart", "item": name}))?;
            }
        }
        self.resume_stopped()
    }

    /// Stops the running launch of `item` and what it started, and records its end.
    pub(crate) fn stop_launch(&mut self, item: &str) -> Result<(), DaemonError> {
        let Some(run) = self.running.remove(item) else {
            return Ok(());
        };
        app::stop_group(run.pid);
        self.record_stopped(item, &run.launch)
    }

    /// The end of a launch the factory stopped, with what it used until then, so its history
    /// neither shows it running nor leaves out its cost.
    fn record_stopped(&self, item: &str, launch: &Launched) -> Result<(), DaemonError> {
        let (_, outcome) = self.read_end(launch);
        let (usage, cost_usd) = spent(&outcome);
        records::append_launch(
            &self.store,
            &records::LaunchLine::End(records::LaunchEnd {
                at: jiff::Timestamp::now().to_string(),
                item: item.to_owned(),
                ended: true,
                code: None,
                attempt: launch.attempt,
                failure: Some("stopped: the factory ended the launch".into()),
                error_kind: None,
                session: outcome.session,
                usage,
                model: outcome.model,
                cost_usd,
            }),
        )
    }

    /// One pass: settle finished launches, turn finished items into facts, take in the owner's
    /// answers and GitHub's news, repair what is missing, then launch every open item. Each part
    /// runs whatever an earlier one met: a failure for one issue stays with that issue, and a
    /// failure no issue owns pauses the factory once it repeats.
    pub fn tick(&mut self) -> Result<(), DaemonError> {
        self.errors.github_clear = false;
        while let Ok(exit) = self.exits.1.try_recv() {
            let r = self.finished(&exit);
            self.settle_part("taking in a finished launch", r)?;
        }
        let r = self.pause_for_cli_errors();
        self.settle_part("pausing for an agent CLI error", r)?;
        let r = self.take_recoveries();
        self.settle_part("taking in finished recovery procedures", r)?;
        let r = self.reload_config();
        self.settle_part("reading the configuration", r)?;
        let r = self.clear_filter();
        self.settle_part("recording that the issue filter works", r)?;
        // Before the halt check, so notices still go out while the factory is paused.
        let r = self.deliver_error_notices();
        self.settle_part("telling the owner of errors", r)?;
        let r = self.take_channel_events();
        self.settle_part("taking in the owner's messages", r)?;
        let r = self.take_cancels();
        self.settle_part("canceling the issues the owner canceled", r)?;
        let r = self.announce_pause();
        self.settle_part("announcing the pause", r)?;
        let r = self.remind_owner();
        self.settle_part("reminding the owner", r)?;
        let r = self.check_prerequisites();
        self.settle_part("checking the prerequisites", r)?;
        if self.halt_file().exists() {
            return Ok(());
        }
        let r = self.take_filter_run();
        self.settle_part("taking in a finished tracker filter", r)?;
        let r = self.consume_finished();
        self.settle_part("taking in finished steps", r)?;
        let r = self.schedule();
        self.settle_part("starting issues", r)?;
        if self
            .last_tracker_poll
            .is_none_or(|t| t.elapsed() >= self.timing.tracker_poll)
        {
            self.last_tracker_poll = Some(Instant::now());
            let r = self.poll_tracker();
            self.settle_part("reading the tracker", r)?;
        }
        // A sprint is planned from the open issues, so only once the tracker has been read; with
        // no tracker connected, none is planned.
        let r = self.find_product_run();
        self.settle_part("finding how the product runs", r)?;
        if self.tracker_read {
            let r = self.maybe_begin_sprint();
            self.settle_part("beginning a sprint", r)?;
        }
        if self
            .last_poll
            .is_none_or(|t| t.elapsed() >= self.timing.github_poll)
        {
            self.last_poll = Some(Instant::now());
            let r = self.poll_pull_requests();
            self.settle_part("reading pull requests", r)?;
            if let Err(e) = self.poll_agreements() {
                self.event(&json!({"event": "agreements-error", "error": e.to_string()}))?;
            }
        }
        let r = self.reconcile();
        self.settle_part("repairing the board", r)?;
        let r = self.reconcile_sprint();
        self.settle_part("repairing the sprint's item", r)?;
        let r = self.deliver_decisions();
        self.settle_part("sending the owner's decisions", r)?;
        let r = self.launch_open();
        self.settle_part("launching steps", r)?;
        let r = self.clear_github();
        self.settle_part("recording that GitHub answers again", r)
    }

    fn launch_open(&mut self) -> Result<(), DaemonError> {
        for path in self.store.active_items()? {
            let name = item_name(&path);
            if self.running.contains_key(&name)
                || self
                    .not_before
                    .get(&name)
                    .is_some_and(|t| Instant::now() < *t)
            {
                continue;
            }
            let item = self.store.read_item(&path)?;
            if item.front.status == Status::Open {
                let launched = self.launch(&path, &item);
                match (item.front.issue.clone(), item.front.sprint.is_some()) {
                    (Some(key), _) if self.issue(&key)?.is_some() => {
                        self.settle(&key, "launching its step", launched)?;
                    }
                    (None, true) => self.settle_sprint("launching its step", launched)?,
                    _ => launched?,
                }
            }
        }
        Ok(())
    }

    /// Lets launches start again (`!resume`).
    pub fn resume(&self) -> Result<(), DaemonError> {
        let halt = self.halt_file();
        if halt.exists() {
            fs::remove_file(&halt).map_err(StoreError::io(&halt))?;
            self.event(&json!({"event": "resumed"}))?;
        }
        Ok(())
    }

    /// Where work runs: the issue's worktree, or for work outside an issue (sprints,
    /// conversations) the product as the base branch has it, away from the owner's own checkout,
    /// where the factory's skills would otherwise be written.
    pub(crate) fn checkout_for(&self, issue: Option<&str>) -> Result<PathBuf, DaemonError> {
        let base = &self.config.repo.base_branch;
        Ok(match issue {
            Some(issue) => worktree::ensure(&self.paths.repo, &self.store, issue, base)?,
            None => worktree::ensure_detached(&self.paths.repo, &self.store, worktree::BASE, base)?,
        })
    }

    /// Before an agent step on a branch with a pull request: what anyone pushed to the branch is
    /// merged into the worktree, so the step works on it. A conflict is undone; the push brings
    /// the branch in again and the executor resolves it.
    fn bring_in_pushed(&self, item: &Item, cwd: &Path) -> Result<(), DaemonError> {
        let Some(issue) = &item.front.issue else {
            return Ok(());
        };
        let Some(record) = self.issue(issue)? else {
            return Ok(());
        };
        if record.flow.pr.is_some() && !pulls::merge_from_origin(cwd, &[&record.branch()])? {
            self.event(&json!({"event": "head-conflict", "issue": issue}))?;
        }
        Ok(())
    }

    /// Where the item's launch runs, ready for it: the issue's worktree with what was pushed to
    /// its branch, or the base's.
    fn place_for(&self, item: &Item, cli: Cli) -> Result<PathBuf, DaemonError> {
        self.codex_readable(cli)?;
        let cwd = self.checkout_for(item.front.issue.as_deref())?;
        self.bring_in_pushed(item, &cwd)?;
        Ok(cwd)
    }

    /// A Codex launch switches off the MCP servers Codex's own configuration defines; when that
    /// file cannot be read, none starts.
    fn codex_readable(&self, cli: Cli) -> Result<(), DaemonError> {
        match (&self.codex_user.unreadable, cli) {
            (Some(why), Cli::Codex) => Err(DaemonError::Launch(format!(
                "Codex's own configuration cannot be read, so the factory cannot keep its MCP servers out of a launch: {why}"
            ))),
            (Some(_) | None, Cli::Claude | Cli::Codex | Cli::Opencode) => Ok(()),
        }
    }

    fn launch(&mut self, path: &Path, item: &Item) -> Result<(), DaemonError> {
        let name = item_name(path);
        let role = self.config.roles.get(item.front.to).clone();
        let cwd = self.place_for(item, role.cli)?;
        let skills_error = |e: std::io::Error| {
            DaemonError::Launch(format!("writing skills into {}: {e}", cwd.display()))
        };
        assets::install_skills(&cwd).map_err(skills_error)?;
        let unselected_skills =
            assets::unselected_skills(&cwd, &role.skills).map_err(skills_error)?;
        let attempt = self.next_attempt(&name)?;
        let launch_dir = self
            .store
            .root()
            .join("launches")
            .join(format!("{name}.{attempt}"));
        fs::create_dir_all(&launch_dir).map_err(StoreError::io(&launch_dir))?;
        let log = launch_dir.with_extension(format!("{attempt}.log"));

        // A work turn continues the session that prepared it or asked the owner: the next turn
        // names its item; everything else the session already holds.
        let resume = item.front.resume.clone();
        let prompt_text = match &resume {
            Some(_) => prompt::continuation(path),
            None => self.prompt_for(path, item, &cwd)?,
        };

        let launch = Launch {
            item_name: name.clone(),
            attempt,
            cli: role.cli,
            model: role.model.clone(),
            effort: role.effort.clone(),
            permissions: self.config.agents.permissions,
            cwd: cwd.clone(),
            launch_dir,
            ovenspring: self.paths.ovenspring.clone(),
            mcp: self.mcp_for(&self.config.agents.mcp)?,
            reachable: vec![
                self.store.root().to_path_buf(),
                git_common_dir(&self.paths.repo),
            ],
            writable: ["board/active", "outputs", "apps", "sprints", "plans"]
                .iter()
                .map(|d| self.store.root().join(d))
                .chain([git_common_dir(&self.paths.repo)])
                .collect(),
            guard: GuardRules {
                deny: self.config.guard.deny.clone(),
                protected: vec![
                    self.store.isolated().to_path_buf(),
                    ovenspring_store::credentials::credentials_path(&self.paths.repo),
                ],
            },
            opencode_v2: backend::opencode_v2(role.cli, &self.path_with_ovenspring()),
            selected_skills: role.skills.clone(),
            unselected_skills,
            prompt: prompt_text,
            resume: resume.clone(),
        };
        let spec = backend::command(&launch, &self.codex_user);
        for (file, contents) in &spec.files {
            fs::write(file, contents).map_err(StoreError::io(file))?;
        }

        let started = self.start_process(path, &spec, &log)?;
        let pid = started.id();
        self.watch(name.clone(), started);

        self.attempts.insert(name.clone(), attempt);
        self.running.insert(
            name.clone(),
            Running {
                pid,
                resumed: resume.is_some(),
                launch: Launched {
                    cli: role.cli,
                    attempt,
                    log: log.clone(),
                    session: resume.clone().or_else(|| {
                        (role.cli == Cli::Claude).then(|| backend::session_uuid(&name, attempt))
                    }),
                    model: role.model.clone(),
                },
            },
        );
        records::append_launch(
            &self.store,
            &records::LaunchLine::Start(records::LaunchStart {
                at: jiff::Timestamp::now().to_string(),
                role: item.front.to,
                item: name,
                step: item.front.step,
                phase: self.phase_of(item)?,
                issue: item.front.issue.clone(),
                sprint: item.front.sprint.clone(),
                cli: role.cli,
                model: role.model,
                effort: role.effort,
                permissions: self.config.agents.permissions,
                attempt,
                pid,
                log: log.display().to_string(),
                prepare: item.front.prepare,
            }),
        )?;
        Ok(())
    }

    /// The phase of the issue `item` belongs to, as it starts.
    fn phase_of(&self, item: &Item) -> Result<Option<ovenspring_core::phase::Phase>, DaemonError> {
        Ok(match &item.front.issue {
            Some(key) => self.issue(key)?.and_then(|r| r.flow.phase()),
            None => None,
        })
    }

    /// The prompt for `item`, from this version's factory files and the agreements on the base
    /// branch.
    fn prompt_for(&self, path: &Path, item: &Item, cwd: &Path) -> Result<String, DaemonError> {
        let role = self.config.roles.get(item.front.to);
        let goal = Some(self.goal()?)
            .filter(|g| !g.is_empty())
            .unwrap_or_else(|| "No product goal is set yet.".into());
        let purpose = self.sprint_purpose();
        let now = jiff::ZonedDateTime::now()
            .strftime("%Y-%m-%d %H:%M %Z")
            .to_string();
        prompt::assemble(&PromptContext {
            role: item.front.to,
            step: item.front.step,
            prepare: item.front.prepare,
            item: path,
            worktree: cwd,
            issue: item.front.issue.as_deref(),
            product_goal: &goal,
            sprint_purpose: purpose.as_deref(),
            now: &now,
            agreements: prompt::read_agreements(
                &self.paths.repo,
                &self.config.repo.base_branch,
                &self.config.agreements.path,
                item.front.to,
            )
            .map_err(|e| DaemonError::Launch(e.to_string()))?,
            repo_skills: &role.skills,
            append: &role.append,
            person_reviews: &self.config.person_reviews,
            agreements_folder: &self.config.agreements.path,
            knowledge: &self.config.knowledge.path,
        })
        .map_err(|e| DaemonError::Launch(e.to_string()))
    }

    /// Sends the child's exit to the daemon loop when it ends.
    fn watch(&self, item: String, mut child: std::process::Child) {
        let tx = self.exits.0.clone();
        std::thread::spawn(move || {
            let status = child.wait();
            let _ = tx.send(Exit {
                item,
                success: status.as_ref().is_ok_and(std::process::ExitStatus::success),
                code: status.ok().and_then(|s| s.code()),
            });
        });
    }

    fn finished(&mut self, exit: &Exit) -> Result<(), DaemonError> {
        let Some(run) = self.running.remove(&exit.item) else {
            return Ok(());
        };
        // What the agent left running in its group (a preview server, a watcher) would outlive
        // every record of it; the issue's app runs in a group of its own and stays.
        app::stop_group(run.pid);
        let (output, outcome) = self.read_end(&run.launch);
        let tail = last_lines(&output, 5);
        let recorded = outcome
            .session
            .as_deref()
            .and_then(|s| backend::recorded_error(run.launch.cli, s, &self.session_places()));
        let failure = backend::classify(exit.success, &outcome, recorded.as_ref(), &tail);
        let (usage, cost_usd) = spent(&outcome);
        records::append_launch(
            &self.store,
            &records::LaunchLine::End(records::LaunchEnd {
                at: jiff::Timestamp::now().to_string(),
                item: exit.item.clone(),
                ended: true,
                code: exit.code,
                attempt: run.launch.attempt,
                failure: failure.as_ref().map(failure_text),
                error_kind: recorded.as_ref().map(|r| r.kind.clone()),
                session: outcome.session.clone(),
                usage,
                model: outcome.model.clone(),
                cost_usd,
            }),
        )?;
        // Its CLI runs again, so an error of that CLI going on clears.
        if failure.is_none() {
            self.succeeded(
                ErrorKey::of_cli(run.launch.cli),
                &json!({"cli": run.launch.cli}),
            )?;
        }

        let path = self.store.active_dir().join(format!("{}.md", exit.item));
        if !path.exists() {
            return Ok(());
        }
        let item = self.store.read_item(&path)?;
        if item.front.status != Status::Doing {
            return Ok(());
        }
        // A session that could not be continued is not the work's failure: the item starts over
        // in a new session, with the owner's answers among its refs. An error the CLI recorded in
        // the session (an overload, the account's limit) is handled as any launch's.
        if run.resumed && failure.is_some() && recorded.is_none() {
            self.store.update_item(&path, |i| {
                i.front.status = Status::Open;
                i.front.resume = None;
            })?;
            return self.event(&json!({"event": "resume-failed", "item": exit.item}));
        }
        // Retries count the item's transient failures in a row, in this process: a restart, a
        // resume or any other outcome starts them again.
        let in_a_row = if matches!(failure, Some(Failure::Transient(_))) {
            let n = self
                .transient_failures
                .entry(exit.item.clone())
                .or_insert(0);
            *n += 1;
            *n
        } else {
            self.transient_failures.remove(&exit.item);
            0
        };
        match failure {
            Some(Failure::Transient(message)) if in_a_row <= RETRIES => {
                self.not_before
                    .insert(exit.item.clone(), Instant::now() + self.timing.retry_wait);
                self.store
                    .update_item(&path, |i| i.front.status = Status::Open)?;
                self.event(&json!({"event": "retry", "item": exit.item, "attempt": run.launch.attempt, "reason": message}))
            }
            Some(Failure::Transient(message) | Failure::Fatal(message)) => {
                // The cause is outside the work (an account limit, an outage that outlasted the
                // retries): the item waits, open, for the owner to clear it and resume.
                self.transient_failures.remove(&exit.item);
                self.store
                    .update_item(&path, |i| i.front.status = Status::Open)?;
                let launch = match (&item.front.issue, &item.front.sprint) {
                    (Some(issue), _) => format!("the agent on {issue} ({})", item.front.step),
                    (None, Some(_)) => format!("the agent on the sprint ({})", item.front.step),
                    (None, None) => format!("the agent of {}", item.front.step),
                };
                let reason = format!(
                    "{launch} stopped with an error: {}",
                    message.trim().trim_end_matches('.')
                );
                let cli = run.launch.cli;
                // The start line commits the pause: a pause it records and does not complete here
                // is completed before the next launch (`pause_for_cli_errors`).
                self.failed(
                    ErrorKey::of_cli(cli),
                    errors::Failure {
                        event: json!({"event": "launch-error", "item": exit.item, "error": message, "cli": cli}),
                        cause: message.trim().to_owned(),
                        via: None,
                        reason: Some(reason.clone()),
                    },
                )?;
                self.pause(&reason, None)
            }
            None => self.fail(&path, "the agent exited without finishing the item"),
        }
    }

    fn fail(&self, path: &Path, note: &str) -> Result<(), DaemonError> {
        self.store.update_item(path, |i| {
            i.front.status = Status::Failed;
            if !i.body.contains("\n## Result") && !i.body.starts_with("## Result") {
                i.body.push_str("\n## Result\n");
            }
            i.body.push_str("\nfactory: ");
            i.body.push_str(note);
            i.body.push('\n');
        })?;
        self.event(&json!({"event": "failed", "item": item_name(path), "note": note}))
    }

    /// A launch's log, and what it shows completed from its CLI's session record, with the model
    /// the role named where neither says which model ran.
    fn read_end(&self, launch: &Launched) -> (String, backend::Outcome) {
        let output = fs::read_to_string(&launch.log).unwrap_or_default();
        let mut outcome = backend::read_stream(launch.cli, &output);
        outcome.session = outcome.session.or_else(|| launch.session.clone());
        if let Some(session) = outcome.session.clone() {
            backend::read_session(launch.cli, &session, &mut outcome, &self.session_places());
        }
        outcome.model = outcome.model.or_else(|| launch.model.clone());
        (output, outcome)
    }

    fn session_places(&self) -> backend::SessionPlaces<'_> {
        backend::SessionPlaces {
            home: &self.paths.home,
            codex_home: &self.paths.codex_home,
        }
    }

    /// Stops new launches until the owner resumes. `owner` is the Slack user who sent `!pause`,
    /// recorded so the line can say the owner paused it; `None` when the factory paused itself.
    /// The reason is masked as error lines are, so the `paused` line of a CLI start carries the
    /// reason exactly as that start recorded it.
    pub(crate) fn pause(&mut self, reason: &str, owner: Option<&str>) -> Result<(), DaemonError> {
        let secrets: Vec<&str> = self.services.secrets.iter().map(String::as_str).collect();
        let reason = &download::mask(reason, &secrets);
        let halt = self.halt_file();
        fs::write(&halt, format!("{reason}\n")).map_err(StoreError::io(&halt))?;
        let mut line = json!({"event": "paused", "reason": reason});
        if let (Some(user), Some(fields)) = (owner, line.as_object_mut()) {
            fields.insert("user".into(), json!(user));
        }
        let appended = records::append_event(&self.store, &line)?;
        self.errors.state.apply(&appended);
        Ok(())
    }

    pub(crate) fn event(&self, value: &serde_json::Value) -> Result<(), DaemonError> {
        records::event(&self.store, value)
    }

    fn mcp_for(
        &self,
        names: &[String],
    ) -> Result<Vec<ovenspring_agents::mcp::McpServer>, DaemonError> {
        if names.is_empty() {
            return Ok(Vec::new());
        }
        ovenspring_agents::mcp::resolve(
            names,
            &ovenspring_agents::mcp::Sources {
                repo: &self.paths.repo,
                home: &self.paths.home,
                codex_home: &self.paths.codex_home,
            },
        )
        .map_err(DaemonError::Launch)
    }

    fn path_with_ovenspring(&self) -> String {
        let mut dirs: Vec<String> = self
            .paths
            .ovenspring
            .parent()
            .map(|p| p.display().to_string())
            .into_iter()
            .collect();
        dirs.extend(
            self.paths
                .agent_path
                .iter()
                .map(|p| p.display().to_string()),
        );
        dirs.push(std::env::var("PATH").unwrap_or_default());
        dirs.join(":")
    }
}

fn spawn(
    spec: &backend::CommandSpec,
    log: &Path,
    path_env: String,
) -> std::io::Result<std::process::Child> {
    let out = File::create(log)?;
    let err = out.try_clone()?;
    let mut child = Command::new(&spec.program)
        .args(&spec.args)
        .envs(spec.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .env("PATH", path_env)
        .current_dir(&spec.cwd)
        .stdin(if spec.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(out)
        .stderr(err)
        .process_group(0)
        .spawn()?;
    if let (Some(input), Some(mut stdin)) = (&spec.stdin, child.stdin.take()) {
        stdin.write_all(input.as_bytes())?;
    }
    Ok(child)
}

/// The repository's git directory, where commits made in any of its worktrees are stored.
fn git_common_dir(repo: &Path) -> PathBuf {
    worktree::git(
        repo,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .map_or_else(|_| repo.join(".git"), PathBuf::from)
}

/// The last `n` lines of `text`.
/// What a launch used, and its cost: priced the same way for every CLI, with the CLI's own figure
/// only where the table has no price for the model.
fn spent(outcome: &backend::Outcome) -> (records::Usage, Option<f64>) {
    let used = outcome.usage;
    let cost = outcome
        .model
        .as_deref()
        .and_then(|m| ovenspring_agents::prices::cost(m, used))
        .or(outcome.cost_usd);
    let usage = records::Usage {
        input: used.input,
        output: used.output,
        cache_read: used.cache_read,
        cache_write: used.cache_write,
    };
    (usage, cost)
}

/// The settings `after` changed from `before`, each as `{setting: "<table>.<key>", value?}` by its
/// leaf path, an array being one leaf. Only a boolean or number value is recorded: any other leaf
/// can hold the owner's free text, a credential included, which never enters the event log.
fn config_changes(before: &Config, after: &Config) -> Result<Vec<serde_json::Value>, DaemonError> {
    fn walk(
        path: &str,
        before: Option<&serde_json::Value>,
        after: Option<&serde_json::Value>,
        out: &mut Vec<serde_json::Value>,
    ) {
        use serde_json::Value;
        if before == after {
            return;
        }
        if let (Some(Value::Object(b)), Some(Value::Object(a))) = (before, after) {
            let keys: std::collections::BTreeSet<&String> = b.keys().chain(a.keys()).collect();
            for key in keys {
                let leaf = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                walk(&leaf, b.get(key), a.get(key), out);
            }
            return;
        }
        match after {
            Some(value @ (Value::Bool(_) | Value::Number(_))) => {
                out.push(json!({"setting": path, "value": value}));
            }
            _ => out.push(json!({"setting": path})),
        }
    }
    let as_json = |config: &Config| {
        serde_json::to_value(config)
            .map_err(|e| DaemonError::Launch(format!("the configuration does not serialize: {e}")))
    };
    let mut out = Vec::new();
    walk(
        "",
        Some(&as_json(before)?),
        Some(&as_json(after)?),
        &mut out,
    );
    Ok(out)
}

fn last_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines
        .get(lines.len().saturating_sub(n)..)
        .unwrap_or_default()
        .join("\n")
}

fn failure_text(f: &Failure) -> String {
    match f {
        Failure::Transient(m) => format!("transient: {m}"),
        Failure::Fatal(m) => format!("fatal: {m}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = r#"
[roles.lead]
cli = "codex"
[roles.executor]
cli = "codex"
[roles.reviewer]
cli = "codex"
[roles.analyzer]
cli = "codex"
[tracker]
kind = "github"
[slack]
channel = "C0123456789"
owners = ["U0123456789"]
"#;

    #[test]
    fn config_changes_carry_boolean_and_number_values_and_only_the_path_of_a_string() {
        let credential = "planted";
        let before = Config::parse(CONFIG, "config.toml").unwrap();
        let after = Config::parse(
            &format!("{CONFIG}[notices]\nerrors = false\n[limits]\nissues_at_once = 5\n").replace(
                "kind = \"github\"",
                &format!("kind = \"github\"\nfilter = \"LINEAR_KEY={credential} ./filter\""),
            ),
            "config.toml",
        )
        .unwrap();

        let changes = config_changes(&before, &after).unwrap();

        assert_eq!(
            changes,
            vec![
                json!({"setting": "limits.issues_at_once", "value": 5}),
                json!({"setting": "notices.errors", "value": false}),
                json!({"setting": "tracker.filter"}),
            ]
        );
        assert!(
            !serde_json::to_string(&changes).unwrap().contains("planted"),
            "a string setting's value never enters the line"
        );
    }
}
