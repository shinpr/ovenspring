//! Settings after setup: each section of
//! `.ovenspring/config.toml` is saved on its own, and the running factory reads the file again
//! for its next launch. Credentials are only ever written; they are read back as whether each is
//! set.

use std::collections::BTreeMap;

use ovenspring_agents::{assets, mcp};
use ovenspring_core::config::{
    AgentsConfig, Cli, Config, ConfigError, GuardConfig, Limits, MergeBy, ReviewConfig, Role,
    TrackerKind,
};
use ovenspring_core::output::Approval;
use ovenspring_store::credentials::{self, Credentials};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::Context;
use crate::setup::{AppRefused, RunRequest, SlackSetup, TrackerSetup, settle_app, write_config};

/// Why a save did not happen.
pub(crate) enum Refusal {
    /// The repository has no usable configuration to change; setup or the file is to be fixed.
    Unconfigured(String),
    /// Field or section → why, for the owner to correct.
    Invalid(BTreeMap<String, String>),
    Failed(String),
}

/// The settings as the dashboard shows them: the configuration file's contents, which
/// credentials are held, and the MCP servers the owner can allow.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub config: Config,
    /// Why the factory cannot use the configuration as it is; a save that corrects it applies.
    pub problem: Option<String>,
    /// What no agent runs, whatever the owner sets.
    pub always_denied: Vec<ovenspring_agents::guard::Shown>,
    pub held: Vec<HeldCredential>,
    pub mcp: Vec<mcp::Available>,
    /// This folder's `origin` on GitHub, as `owner/name`.
    pub repository: Option<String>,
    /// The account the last passed GitHub check answered as.
    pub github_account: Option<crate::setup::GitHubAccount>,
}

/// A credential the factory holds, by its last four characters: enough for the owner to tell which
/// one it is, too few to use. The value itself is never read back.
#[derive(Debug, Serialize, TS)]
pub struct HeldCredential {
    pub credential: Credential,
    pub ending: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Credential {
    SlackBotToken,
    SlackAppToken,
    LinearApiKey,
    GithubToken,
}

pub(crate) fn state(context: &Context) -> Result<Settings, Refusal> {
    // Only a file that does not parse is left to the file; values the factory refuses are shown
    // with why, so a save can correct them.
    let config = Config::read(&context.repo).map_err(|e| Refusal::Unconfigured(e.to_string()))?;
    let problem = config.check(&context.repo).err().map(|e| e.to_string());
    let held =
        credentials::read(&context.credentials).map_err(|e| Refusal::Failed(e.to_string()))?;
    Ok(Settings {
        config,
        problem,
        always_denied: ovenspring_agents::guard::ALWAYS_DENIED
            .iter()
            .map(|d| d.shown)
            .collect(),
        held: [
            (Credential::SlackBotToken, &held.slack.bot_token),
            (Credential::SlackAppToken, &held.slack.app_token),
            (Credential::LinearApiKey, &held.linear.api_key),
            (Credential::GithubToken, &held.github.token),
        ]
        .into_iter()
        .filter_map(|(credential, value)| {
            value.as_deref().map(|value| HeldCredential {
                credential,
                ending: last_four(value).to_owned(),
            })
        })
        .collect(),
        repository: crate::setup::origin(context),
        github_account: crate::setup::account(context),
        mcp: mcp::available(&mcp::Sources {
            repo: &context.repo,
            home: &context.home,
            codex_home: &context.codex_home,
        }),
    })
}

fn last_four(value: &str) -> &str {
    value
        .char_indices()
        .rev()
        .nth(3)
        .map_or(value, |(at, _)| &value[at..])
}

/// The repository skills one agent CLI loads: where it looks and what it finds there.
#[derive(Debug, Serialize, TS)]
pub struct CliSkills {
    pub dirs: Vec<String>,
    pub skills: Vec<assets::SkillInfo>,
}

#[derive(Debug, Serialize, TS)]
pub struct Skills {
    pub claude: CliSkills,
    pub codex: CliSkills,
    pub opencode: CliSkills,
}

pub(crate) fn skills(context: &Context) -> std::io::Result<Skills> {
    let of = |cli: Cli| -> std::io::Result<CliSkills> {
        Ok(CliSkills {
            dirs: cli.skill_dirs().iter().map(|d| (*d).to_owned()).collect(),
            skills: assets::repo_skills(&context.repo, cli.skill_dirs())?,
        })
    };
    Ok(Skills {
        claude: of(Cli::Claude)?,
        codex: of(Cli::Codex)?,
        opencode: of(Cli::Opencode)?,
    })
}

/// The sections saved on their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum SettingsSection {
    General,
    Agents,
    Roles,
    Tracker,
    Slack,
    Limits,
    App,
    Guard,
    Review,
}

/// `general`: the base branch, who merges, what is held by path, and GitHub's token.
#[derive(Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GeneralSection {
    base_branch: String,
    merge_by: MergeBy,
    /// Category → path patterns; a category left out is not held.
    held: BTreeMap<Approval, Vec<String>>,
    /// The owner's name for each kind of change a person reviews → the words that describe it.
    person_reviews: BTreeMap<String, String>,
    /// The folder the working agreements are kept in, relative to the repository's root.
    agreements_path: String,
    /// The folder the factory keeps its knowledge in, relative to the repository's root.
    knowledge_path: String,
    /// Empty to keep the token already held, or to use the signed-in `gh`.
    github_token: Option<String>,
}

/// `roles`: one role's agent.
#[derive(Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoleSection {
    role: Role,
    cli: Cli,
    model: Option<String>,
    effort: Option<String>,
    skills: Vec<String>,
    append: String,
}

/// `tracker`: where the issues are, and Linear's key.
#[derive(Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrackerSection {
    kind: TrackerKind,
    team: Option<String>,
    /// Empty to keep the key already held.
    api_key: Option<String>,
}

/// `slack`: the channel, who the factory answers, and the tokens.
#[derive(Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SlackSection {
    channel: String,
    owners: Vec<String>,
    /// Empty to keep the token already held.
    bot_token: Option<String>,
    /// Empty to keep the token already held.
    app_token: Option<String>,
}

fn parse<T: serde::de::DeserializeOwned>(body: Value) -> Result<T, Refusal> {
    serde_json::from_value(body)
        .map_err(|e| Refusal::Invalid(BTreeMap::from([("request".to_owned(), e.to_string())])))
}

/// A value the owner typed, or `None` when the field was left empty.
fn entered(value: Option<String>) -> Option<String> {
    value.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
}

/// The lines the owner typed, without blank ones.
fn lines(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
        .collect()
}

/// Applies the `general` section to `config`; returns the GitHub token entered, if any.
fn apply_general(config: &mut Config, general: GeneralSection) -> Option<String> {
    general
        .base_branch
        .trim()
        .clone_into(&mut config.repo.base_branch);
    config.merge.by = general.merge_by;
    config.held = general
        .held
        .into_iter()
        .map(|(category, paths)| {
            let paths = paths
                .into_iter()
                .map(|p| p.trim().to_owned())
                .filter(|p| !p.is_empty())
                .collect();
            (category, ovenspring_core::config::HeldCategory { paths })
        })
        .collect();
    config.person_reviews = general
        .person_reviews
        .into_iter()
        .map(|(name, words)| (name.trim().to_owned(), words.trim().to_owned()))
        .collect();
    for (folder, path) in [
        (&general.agreements_path, &mut config.agreements.path),
        (&general.knowledge_path, &mut config.knowledge.path),
    ] {
        folder.trim().trim_end_matches('/').clone_into(path);
    }
    entered(general.github_token)
}

/// Saves `section` from `body`, checking a changed connection before anything is written.
pub(crate) fn save(
    context: &Context,
    section: SettingsSection,
    body: Value,
) -> Result<Settings, Refusal> {
    let mut config =
        Config::read(&context.repo).map_err(|e| Refusal::Unconfigured(e.to_string()))?;
    let mut held =
        credentials::read(&context.credentials).map_err(|e| Refusal::Failed(e.to_string()))?;
    let before = held.clone();
    match section {
        SettingsSection::General => {
            let general: GeneralSection = parse(body)?;
            if let Some(token) = apply_general(&mut config, general) {
                held.github.token = Some(token);
            }
        }
        SettingsSection::Agents => config.agents = parse::<AgentsConfig>(body)?,
        SettingsSection::Roles => {
            let r: RoleSection = parse(body)?;
            let role = config.roles.get_mut(r.role);
            role.cli = r.cli;
            role.model = entered(r.model);
            role.effort = entered(r.effort);
            role.skills = r.skills;
            r.append.trim().clone_into(&mut role.append);
        }
        SettingsSection::Tracker => {
            let t: TrackerSection = parse(body)?;
            config.tracker.kind = t.kind;
            config.tracker.team = entered(t.team).filter(|_| t.kind == TrackerKind::Linear);
            if let Some(key) = entered(t.api_key) {
                held.linear.api_key = Some(key);
            }
        }
        SettingsSection::Slack => {
            let s: SlackSection = parse(body)?;
            s.channel.trim().clone_into(&mut config.slack.channel);
            config.slack.owners = s
                .owners
                .iter()
                .map(|o| o.trim().to_owned())
                .filter(|o| !o.is_empty())
                .collect();
            if let Some(token) = entered(s.bot_token) {
                held.slack.bot_token = Some(token);
            }
            if let Some(token) = entered(s.app_token) {
                held.slack.app_token = Some(token);
            }
        }
        SettingsSection::Limits => config.limits = parse::<Limits>(body)?,
        SettingsSection::App => {
            // Commands as they are now ran before; changed ones are started once first.
            let current = config.app.start.is_some().then(|| config.app.clone());
            config.app = settle_app(
                context,
                &config,
                parse::<RunRequest>(body)?,
                current.as_ref(),
            )
            .map_err(|refused| match refused {
                AppRefused::Invalid(errors) => Refusal::Invalid(errors),
                AppRefused::Failed(e) => Refusal::Failed(e),
            })?;
        }
        SettingsSection::Guard => config.guard.deny = lines(parse::<GuardConfig>(body)?.deny),
        SettingsSection::Review => config.review.bots = lines(parse::<ReviewConfig>(body)?.bots),
    }
    if let Err(ConfigError::Invalid { key, message, .. }) = config.check(&context.repo) {
        return Err(Refusal::Invalid(BTreeMap::from([(key, message)])));
    }
    check_connection(context, section, &config, &held)?;
    if held != before {
        credentials::write(&context.credentials, &held)
            .map_err(|e| Refusal::Failed(e.to_string()))?;
    }
    write_config(&context.repo, &config).map_err(Refusal::Failed)?;
    state(context)
}

/// The general, tracker and Slack sections connect the factory to a service: they are checked the
/// way setup checks them, so a wrong token, base branch, key or channel is refused instead of saved.
fn check_connection(
    context: &Context,
    section: SettingsSection,
    config: &Config,
    held: &Credentials,
) -> Result<(), Refusal> {
    let missing = |field: &str, what: &str| {
        Refusal::Invalid(BTreeMap::from([(
            field.to_owned(),
            format!("Enter the {what}."),
        )]))
    };
    let result = match section {
        SettingsSection::General => context
            .checks
            .github(
                &context.repo,
                held.github.token.as_deref(),
                &config.repo.base_branch,
            )
            .and_then(|account| {
                crate::setup::record_account(context, &account).map(|()| account.login)
            }),
        SettingsSection::Tracker => {
            let tracker = match config.tracker.kind {
                TrackerKind::Linear => TrackerSetup::Linear {
                    team: config.tracker.team.clone().unwrap_or_default(),
                    api_key: held
                        .linear
                        .api_key
                        .clone()
                        .ok_or_else(|| missing("api_key", "Linear API key"))?,
                },
                TrackerKind::Github => TrackerSetup::Github,
            };
            context
                .checks
                .tracker(&context.repo, &tracker, held.github.token.as_deref())
        }
        SettingsSection::Slack => {
            let slack = SlackSetup {
                bot_token: held
                    .slack
                    .bot_token
                    .clone()
                    .ok_or_else(|| missing("bot_token", "bot token"))?,
                app_token: held
                    .slack
                    .app_token
                    .clone()
                    .ok_or_else(|| missing("app_token", "app-level token"))?,
                channel: config.slack.channel.clone(),
                owner: config.slack.owners.first().cloned().unwrap_or_default(),
            };
            context.checks.slack(&slack)
        }
        SettingsSection::Agents
        | SettingsSection::Roles
        | SettingsSection::Limits
        | SettingsSection::App
        | SettingsSection::Guard
        | SettingsSection::Review => return Ok(()),
    };
    result
        .map(|_| ())
        .map_err(|why| Refusal::Invalid(BTreeMap::from([(section.name().to_owned(), why)])))
}

impl SettingsSection {
    /// The section's name, which a refused connection's reason is filed under.
    fn name(self) -> &'static str {
        match self {
            SettingsSection::General => "general",
            SettingsSection::Agents => "agents",
            SettingsSection::Roles => "roles",
            SettingsSection::Tracker => "tracker",
            SettingsSection::Slack => "slack",
            SettingsSection::Limits => "limits",
            SettingsSection::App => "app",
            SettingsSection::Guard => "guard",
            SettingsSection::Review => "review",
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use super::*;
    use crate::setup::{GitHubAccount, SetupChecks, SlackSetup, TrackerSetup};

    struct ConnectedChecks;

    impl SetupChecks for ConnectedChecks {
        fn tracker(
            &self,
            _repo: &Path,
            _tracker: &TrackerSetup,
            _github: Option<&str>,
        ) -> Result<String, String> {
            Ok("connected".into())
        }

        fn slack(&self, _slack: &SlackSetup) -> Result<String, String> {
            Ok("connected".into())
        }

        fn github(
            &self,
            _repo: &Path,
            _token: Option<&str>,
            _base_branch: &str,
        ) -> Result<GitHubAccount, String> {
            Ok(GitHubAccount {
                login: "owner".into(),
                from_gh: false,
            })
        }
    }

    #[test]
    fn tracker_settings_save_keeps_the_configured_filter() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let config_dir = repo.join(".ovenspring");
        let home = temp.path().join("home");
        let codex_home = home.join(".codex");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::create_dir_all(&codex_home).unwrap();
        std::fs::write(
            config_dir.join("config.toml"),
            r#"
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
filter = "scripts/issue-filter"
[slack]
channel = "C0123456789"
owners = ["U0123456789"]
"#,
        )
        .unwrap();
        let context = Context {
            repo: repo.clone(),
            store: ovenspring_store::Store::open(temp.path().join("state")).unwrap(),
            credentials: temp.path().join("credentials.toml"),
            checks: Arc::new(ConnectedChecks),
            home,
            codex_home,
            version: crate::version::Sources {
                executable: temp.path().join("ovenspring"),
                latest_release: String::new(),
                transport: Arc::new(ovenspring_integrations::http::Ureq::new()),
            },
        };

        let saved = save(
            &context,
            SettingsSection::Tracker,
            serde_json::json!({"kind": "github", "team": null, "apiKey": null}),
        )
        .unwrap_or_else(|_| panic!("the tracker settings save should succeed"));

        assert_eq!(
            saved.config.tracker.filter.as_deref(),
            Some("scripts/issue-filter")
        );
        let persisted = Config::read(&repo).unwrap();
        assert_eq!(
            persisted.tracker.filter.as_deref(),
            Some("scripts/issue-filter")
        );
    }
}
