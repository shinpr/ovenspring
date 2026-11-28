//! Setup, in three steps the saved state decides: Connect writes `.ovenspring/config.toml` in the
//! repository and the credentials outside it; Run the product settles `[app]`; Set the goal is
//! the goal's own save. Each connection and each product start is checked before it is written.

use std::collections::BTreeMap;
use std::path::Path;

use ovenspring_core::config::{
    AppConfig, CONFIG_FILE, Cli, Config, ConfigError, MergeBy, MergeConfig, RepoConfig, Roles,
    SlackConfig, TrackerConfig, TrackerKind,
};
use ovenspring_core::output::RunCommandsOutput;
use ovenspring_daemon::product_run::{self, RunCheck};
use ovenspring_daemon::{DaemonError, history, records, worktree};
use ovenspring_store::credentials::{self, Credentials};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Context;

/// Connect's save: the connections and the agent every role starts on.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetupRequest {
    pub cli: Cli,
    pub tracker: TrackerSetup,
    pub slack: SlackSetup,
    pub repo: RepoSetup,
    pub github: GitHubSetup,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "lowercase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum TrackerSetup {
    Linear { team: String, api_key: String },
    Github,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SlackSetup {
    pub bot_token: String,
    pub app_token: String,
    pub channel: String,
    /// The owner's Slack member id.
    pub owner: String,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepoSetup {
    pub base_branch: String,
    pub merge_by: MergeBy,
}

#[derive(Debug, Clone, Default, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct GitHubSetup {
    /// None to use the token of the signed-in `gh`.
    pub token: Option<String>,
}

/// One Connect block, checked on its own before anything is saved.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(
    tag = "section",
    rename_all = "lowercase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum SetupCheck {
    Github {
        token: Option<String>,
        base_branch: String,
    },
    Tracker {
        tracker: TrackerSetup,
        /// The GitHub token entered above, which GitHub Issues are read with.
        github_token: Option<String>,
    },
    Slack {
        bot_token: String,
        app_token: String,
        channel: String,
        owner: String,
    },
}

/// What a passed check found, in one sentence the dashboard shows.
#[derive(Debug, Serialize, TS)]
pub struct SetupFound {
    pub found: String,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(tag = "runs", rename_all = "lowercase", deny_unknown_fields)]
pub enum RunRequest {
    Commands {
        setup: Option<String>,
        start: String,
        ready: Option<String>,
    },
    Nothing,
}

/// The connections setup checks; each answers what it found, in one sentence the dashboard
/// shows, or why it failed.
pub trait SetupChecks: Send + Sync {
    fn tracker(
        &self,
        repo: &Path,
        tracker: &TrackerSetup,
        github: Option<&str>,
    ) -> Result<String, String>;
    fn slack(&self, slack: &SlackSetup) -> Result<String, String>;
    fn github(
        &self,
        repo: &Path,
        token: Option<&str>,
        base_branch: &str,
    ) -> Result<GitHubAccount, String>;
}

/// The account GitHub answered a passed check as, which pull requests are opened as.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GitHubAccount {
    pub login: String,
    /// The signed-in `gh`'s token, not one the owner entered.
    pub from_gh: bool,
}

impl GitHubAccount {
    fn found(&self) -> String {
        if self.from_gh {
            format!("Using the token of your signed-in gh, as {}.", self.login)
        } else {
            format!("Signed in as {}.", self.login)
        }
    }
}

const GITHUB_ACCOUNT: &str = "github-account.json";

/// This folder's `origin` on GitHub, as `owner/name`.
pub(crate) fn origin(context: &Context) -> Option<String> {
    let url =
        ovenspring_daemon::worktree::git(&context.repo, &["remote", "get-url", "origin"]).ok()?;
    ovenspring_integrations::github::RepoId::from_remote(&url)
        .map(|id| format!("{}/{}", id.owner, id.name))
}

/// Keeps the account a passed GitHub check answered as, for Settings to show.
pub(crate) fn record_account(context: &Context, account: &GitHubAccount) -> Result<(), String> {
    context
        .store
        .write_record(GITHUB_ACCOUNT, account)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// The account the last passed GitHub check answered as.
pub(crate) fn account(context: &Context) -> Option<GitHubAccount> {
    context.store.read_record(GITHUB_ACCOUNT).ok().flatten()
}

/// The step setup is at, from what is saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum SetupStep {
    /// No configuration.
    Connect,
    /// `[app]` not settled.
    Run,
    /// No goal.
    Goal,
    Done,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SetupState {
    pub step: SetupStep,
    /// The repository's directory name, which the setup screen names.
    pub repository: String,
    /// This folder's `origin` on GitHub, as `owner/name`.
    pub origin: Option<String>,
    /// Why the configuration in the repository cannot be used, when it cannot.
    pub error: Option<String>,
    /// The manifest the owner creates the Slack app from, as the JSON they paste.
    pub slack_manifest: String,
    /// Where finding how the product runs stands, while that is the step.
    pub run: Option<SetupRun>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SetupRun {
    /// What the lead has said so far while it reads the repository.
    pub said: Vec<String>,
    /// Why the factory paused before the lead answered: its agent's own error.
    pub stopped: Option<String>,
    /// What the owner types after `@` to mention the factory in Slack, to resume it.
    pub mention: String,
    pub found: Option<RunCommandsOutput>,
    pub checking: bool,
    pub check: Option<RunCheck>,
}

/// Why setup saved nothing.
pub enum SetupRefused {
    /// The step is done already.
    Done,
    /// Field, block or `check` → why.
    Invalid(BTreeMap<String, String>),
    Failed(String),
}

pub(crate) enum AppRefused {
    /// Field or `check` → why.
    Invalid(BTreeMap<String, String>),
    Failed(String),
}

impl From<AppRefused> for SetupRefused {
    fn from(refused: AppRefused) -> Self {
        match refused {
            AppRefused::Invalid(errors) => SetupRefused::Invalid(errors),
            AppRefused::Failed(e) => SetupRefused::Failed(e),
        }
    }
}

pub(crate) fn state(context: &Context) -> Result<SetupState, DaemonError> {
    let file = context.repo.join(CONFIG_FILE);
    let loaded = file.exists().then(|| Config::load(&context.repo));
    let step = match &loaded {
        None => SetupStep::Connect,
        Some(Ok(c)) if !c.app.settled() => SetupStep::Run,
        Some(Ok(_)) if records::goal(&context.store)?.is_empty() => SetupStep::Goal,
        // A configuration that cannot be read is fixed in Settings or the file, as the banner says.
        Some(Ok(_) | Err(_)) => SetupStep::Done,
    };
    let run = match step {
        SetupStep::Run => Some(run_state(context)?),
        SetupStep::Connect | SetupStep::Goal | SetupStep::Done => None,
    };
    Ok(SetupState {
        step,
        repository: context
            .repo
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        origin: origin(context),
        error: loaded.and_then(Result::err).map(|e| e.to_string()),
        slack_manifest: serde_json::to_string_pretty(
            &ovenspring_integrations::channel::slack::app_manifest(),
        )
        .unwrap_or_default(),
        run,
    })
}

fn run_state(context: &Context) -> Result<SetupRun, DaemonError> {
    let run = product_run::read(&context.store)?;
    let reading = run.found.is_none();
    let said = match (&run.item, reading) {
        (Some(item), true) => history::latest_messages(&context.store, item)?,
        (Some(_) | None, _) => Vec::new(),
    };
    // The lead's agent failed to run, and the factory paused on it: its own error.
    let stopped = match (&run.item, reading) {
        (Some(item), true) if records::pause_reason(&context.store)?.is_some() => {
            history::latest_failure(&context.store, item)?
        }
        (Some(_) | None, _) => None,
    };
    Ok(SetupRun {
        said,
        stopped,
        mention: ovenspring_daemon::prerequisites::mention(&context.store)?,
        checking: run.to_check().is_some(),
        found: run.found,
        check: run.check,
    })
}

/// Checks one Connect block; writes nothing.
pub(crate) fn check(
    context: &Context,
    block: &SetupCheck,
) -> Result<SetupFound, BTreeMap<String, String>> {
    let (section, result) = match block {
        SetupCheck::Github { token, base_branch } => (
            "github",
            context
                .checks
                .github(&context.repo, entered(token.as_deref()), base_branch.trim())
                .map(|a| a.found()),
        ),
        SetupCheck::Tracker {
            tracker,
            github_token,
        } => (
            "tracker",
            context
                .checks
                .tracker(&context.repo, tracker, entered(github_token.as_deref())),
        ),
        SetupCheck::Slack {
            bot_token,
            app_token,
            channel,
            owner,
        } => (
            "slack",
            context.checks.slack(&SlackSetup {
                bot_token: bot_token.clone(),
                app_token: app_token.clone(),
                channel: channel.clone(),
                owner: owner.clone(),
            }),
        ),
    };
    result
        .map(|found| SetupFound { found })
        .map_err(|why| BTreeMap::from([(section.to_owned(), why)]))
}

/// Connect's save: checks every block again, then writes the credentials and the configuration.
pub(crate) fn save(context: &Context, request: &SetupRequest) -> Result<SetupState, SetupRefused> {
    if context.repo.join(CONFIG_FILE).exists() {
        return Err(SetupRefused::Done);
    }
    let config = config_of(request);
    if let Err(ConfigError::Invalid { key, message, .. }) = config.check(&context.repo) {
        return Err(SetupRefused::Invalid(BTreeMap::from([(key, message)])));
    }
    let github = entered(request.github.token.as_deref());
    let account = context
        .checks
        .github(&context.repo, github, &request.repo.base_branch);
    let found = [
        ("github", account.clone().map(|a| a.found())),
        (
            "tracker",
            context
                .checks
                .tracker(&context.repo, &request.tracker, github),
        ),
        ("slack", context.checks.slack(&request.slack)),
    ];
    let errors: BTreeMap<String, String> = found
        .into_iter()
        .filter_map(|(section, result)| result.err().map(|why| (section.to_owned(), why)))
        .collect();
    if !errors.is_empty() {
        return Err(SetupRefused::Invalid(errors));
    }
    if let Ok(account) = &account {
        record_account(context, account).map_err(SetupRefused::Failed)?;
    }
    write(context, request, &config).map_err(SetupRefused::Failed)?;
    state(context).map_err(|e| SetupRefused::Failed(e.to_string()))
}

/// Run the product's save: `[app]` as the owner settles it, after starting changed commands once.
pub(crate) fn save_run(context: &Context, request: RunRequest) -> Result<SetupState, SetupRefused> {
    let mut config =
        Config::load(&context.repo).map_err(|e| SetupRefused::Failed(e.to_string()))?;
    if config.app.settled() {
        return Err(SetupRefused::Done);
    }
    let found =
        product_run::read(&context.store).map_err(|e| SetupRefused::Failed(e.to_string()))?;
    let started = found
        .found
        .as_ref()
        .and_then(product_run::commands_of)
        .filter(|_| matches!(found.check, Some(RunCheck::Started { .. })));
    config.app = settle_app(context, &config, request, started.as_ref())?;
    write_config(&context.repo, &config).map_err(SetupRefused::Failed)?;
    state(context).map_err(|e| SetupRefused::Failed(e.to_string()))
}

/// `[app]` from the owner's answer. Commands are started once in the base branch's worktree,
/// unless they are `started` already; they are refused under `check` when the product did not
/// answer.
pub(crate) fn settle_app(
    context: &Context,
    config: &Config,
    request: RunRequest,
    started: Option<&AppConfig>,
) -> Result<AppConfig, AppRefused> {
    let (setup, start, ready) = match request {
        RunRequest::Nothing => {
            return Ok(AppConfig {
                setup: None,
                start: None,
                ready: None,
                nothing_to_run: true,
            });
        }
        RunRequest::Commands {
            setup,
            start,
            ready,
        } => (setup, start, ready),
    };
    let Some(start) = entered(Some(&start)).map(str::to_owned) else {
        return Err(AppRefused::Invalid(BTreeMap::from([(
            "start".to_owned(),
            "Enter the command that starts the product.".to_owned(),
        )])));
    };
    let app = AppConfig {
        setup: entered(setup.as_deref()).map(str::to_owned),
        start: Some(start),
        ready: entered(ready.as_deref()).map(str::to_owned),
        nothing_to_run: false,
    };
    if started == Some(&app) {
        return Ok(app);
    }
    let base = fetch_base(context, &config.repo.base_branch)
        .and_then(|()| {
            worktree::ensure_detached(
                &context.repo,
                &context.store,
                worktree::BASE,
                &config.repo.base_branch,
            )
            .map_err(|e| e.to_string())
        })
        .map_err(AppRefused::Failed)?;
    match product_run::check(context.store.root(), &base, &app) {
        RunCheck::Started { .. } => Ok(app),
        RunCheck::Failed { output } => Err(AppRefused::Invalid(BTreeMap::from([(
            "check".to_owned(),
            output,
        )]))),
    }
}

/// Fetches `base` from `origin` with the GitHub token the factory pushes with, before the daemon
/// that holds its connection may run.
fn fetch_base(context: &Context, base: &str) -> Result<(), String> {
    let url = worktree::git(&context.repo, &["remote", "get-url", "origin"])
        .map_err(|e| e.to_string())?;
    let stored = credentials::read(&context.credentials).map_err(|e| e.to_string())?;
    let token = ovenspring_integrations::github::selected_token(stored.github.token)
        .ok_or("No GitHub token is set up to fetch the base branch with.")?;
    let remote = ovenspring_integrations::github::GitRemote {
        url: &url,
        token: &token,
    };
    worktree::git_remote(&context.repo, &remote, &["fetch", "-q", "origin", base])
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// A value the owner typed, or `None` when the field was left empty.
fn entered(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|v| !v.is_empty())
}

fn config_of(request: &SetupRequest) -> Config {
    let (kind, team) = match &request.tracker {
        TrackerSetup::Linear { team, .. } => (TrackerKind::Linear, Some(team.trim().to_owned())),
        TrackerSetup::Github => (TrackerKind::Github, None),
    };
    Config {
        repo: RepoConfig {
            base_branch: request.repo.base_branch.trim().to_owned(),
        },
        merge: MergeConfig {
            by: request.repo.merge_by,
        },
        tracker: TrackerConfig {
            kind,
            team,
            filter: None,
        },
        slack: SlackConfig {
            channel: request.slack.channel.trim().to_owned(),
            owners: vec![request.slack.owner.trim().to_owned()],
        },
        held: BTreeMap::new(),
        person_reviews: BTreeMap::new(),
        agreements: ovenspring_core::config::AgreementsConfig::default(),
        knowledge: ovenspring_core::config::KnowledgeConfig::default(),
        notices: ovenspring_core::config::NoticesConfig::default(),
        agents: ovenspring_core::config::AgentsConfig::default(),
        roles: Roles::all_on(request.cli),
        app: AppConfig::default(),
        guard: ovenspring_core::config::GuardConfig::default(),
        review: ovenspring_core::config::ReviewConfig::default(),
        limits: ovenspring_core::config::Limits::default(),
    }
}

/// Writes the credentials first and the configuration last: the daemon starts once the
/// configuration exists, by which time everything it reads is in place.
fn write(context: &Context, request: &SetupRequest, config: &Config) -> Result<(), String> {
    let held = Credentials {
        slack: credentials::Slack {
            bot_token: Some(request.slack.bot_token.trim().to_owned()),
            app_token: Some(request.slack.app_token.trim().to_owned()),
        },
        linear: credentials::Linear {
            api_key: match &request.tracker {
                TrackerSetup::Linear { api_key, .. } => Some(api_key.trim().to_owned()),
                TrackerSetup::Github => None,
            },
        },
        github: credentials::GitHub {
            token: entered(request.github.token.as_deref()).map(str::to_owned),
        },
    };
    credentials::write(&context.credentials, &held).map_err(|e| e.to_string())?;
    let dir = context.repo.join(".ovenspring");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    write_config(&context.repo, config)
}

/// Replaces `.ovenspring/config.toml` in one step, so the running factory never reads half a file.
pub(crate) fn write_config(repo: &Path, config: &Config) -> Result<(), String> {
    let text = config.to_toml().map_err(|e| e.to_string())?;
    let file = repo.join(CONFIG_FILE);
    let draft = file.with_extension("toml.saving");
    std::fs::write(&draft, text)
        .and_then(|()| std::fs::rename(&draft, &file))
        .map_err(|e| format!("{}: {e}", file.display()))
}
