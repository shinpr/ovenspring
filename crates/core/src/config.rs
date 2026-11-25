//! The repository's factory configuration, `.ovenspring/config.toml`.
//!
//! Every field has a default except `tracker`, `slack` and each role's `cli`: no agent is assumed.
//! Unknown keys are errors, so a misspelled setting is reported instead of silently ignored.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::output::Approval;

pub const CONFIG_FILE: &str = ".ovenspring/config.toml";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub repo: RepoConfig,
    #[serde(default)]
    pub merge: MergeConfig,
    /// Categories held for the owner by path; only the approval categories are valid names.
    #[serde(default)]
    pub held: BTreeMap<Approval, HeldCategory>,
    /// Kinds of change a person reviews before they merge, by the owner's name for each, described
    /// in the owner's words; the design and the code review judge which a change falls under.
    #[serde(default)]
    pub person_reviews: BTreeMap<String, String>,
    #[serde(default)]
    pub agents: AgentsConfig,
    pub roles: Roles,
    pub tracker: TrackerConfig,
    pub slack: SlackConfig,
    #[serde(default)]
    pub app: AppConfig,
    #[serde(default)]
    pub guard: GuardConfig,
    #[serde(default)]
    pub review: ReviewConfig,
    #[serde(default)]
    pub limits: Limits,
    #[serde(default)]
    pub agreements: AgreementsConfig,
    #[serde(default)]
    pub knowledge: KnowledgeConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AgreementsConfig {
    /// Relative to the repository's root.
    #[serde(default = "default_agreements_path")]
    pub path: String,
}

fn default_agreements_path() -> String {
    ".ovenspring/agreements".into()
}

impl Default for AgreementsConfig {
    fn default() -> Self {
        AgreementsConfig {
            path: default_agreements_path(),
        }
    }
}

/// Where the factory keeps its knowledge in the repository.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeConfig {
    /// A folder relative to the repository's root, as the repository's own layout puts such notes.
    #[serde(default = "default_knowledge_path")]
    pub path: String,
}

fn default_knowledge_path() -> String {
    ".ovenspring/knowledge".into()
}

impl Default for KnowledgeConfig {
    fn default() -> Self {
        KnowledgeConfig {
            path: default_knowledge_path(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RepoConfig {
    #[serde(default = "default_base_branch")]
    pub base_branch: String,
}

impl Default for RepoConfig {
    fn default() -> Self {
        Self {
            base_branch: default_base_branch(),
        }
    }
}

fn default_base_branch() -> String {
    "main".into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, TS)]
#[serde(rename_all = "lowercase")]
pub enum MergeBy {
    #[default]
    Factory,
    Owner,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, TS)]
#[serde(deny_unknown_fields)]
pub struct MergeConfig {
    #[serde(default)]
    pub by: MergeBy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct HeldCategory {
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Roles {
    pub lead: RoleConfig,
    pub executor: RoleConfig,
    pub reviewer: RoleConfig,
    pub analyzer: RoleConfig,
}

impl Roles {
    /// Every role on `cli`, with the CLI's own model and effort: what setup writes.
    pub fn all_on(cli: Cli) -> Roles {
        let role = || RoleConfig {
            cli,
            model: None,
            effort: None,
            skills: Vec::new(),
            append: String::new(),
        };
        Roles {
            lead: role(),
            executor: role(),
            reviewer: role(),
            analyzer: role(),
        }
    }

    pub fn get(&self, role: Role) -> &RoleConfig {
        match role {
            Role::Lead => &self.lead,
            Role::Executor => &self.executor,
            Role::Reviewer => &self.reviewer,
            Role::Analyzer => &self.analyzer,
        }
    }

    pub fn get_mut(&mut self, role: Role) -> &mut RoleConfig {
        match role {
            Role::Lead => &mut self.lead,
            Role::Executor => &mut self.executor,
            Role::Reviewer => &mut self.reviewer,
            Role::Analyzer => &mut self.analyzer,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Lead,
    Executor,
    Reviewer,
    Analyzer,
}

impl Role {
    pub const ALL: [Role; 4] = [Role::Lead, Role::Executor, Role::Reviewer, Role::Analyzer];

    pub fn as_str(self) -> &'static str {
        match self {
            Role::Lead => "lead",
            Role::Executor => "executor",
            Role::Reviewer => "reviewer",
            Role::Analyzer => "analyzer",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Cli {
    Claude,
    Codex,
    Opencode,
}

impl Cli {
    /// Where the CLI loads a repository's own skills from, relative to the working directory.
    pub fn skill_dirs(self) -> &'static [&'static str] {
        match self {
            Cli::Claude => &[".claude/skills"],
            Cli::Codex => &[".agents/skills"],
            Cli::Opencode => &[".agents/skills", ".claude/skills"],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, TS)]
#[serde(rename_all = "lowercase")]
pub enum Permissions {
    #[default]
    Auto,
    Bypass,
}

/// What every launch gets, whatever its role.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, TS)]
#[serde(deny_unknown_fields)]
pub struct AgentsConfig {
    #[serde(default)]
    pub permissions: Permissions,
    /// Names of the MCP servers launches may use, defined in the repository's `.mcp.json` or the
    /// owner's own Claude Code or Codex configuration.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RoleConfig {
    pub cli: Cli,
    /// The CLI's own default when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The CLI's own default when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    /// Names of the repository's own skills this role loads, in the directory its CLI reads
    /// (`.claude/skills` for Claude Code, `.agents/skills` for Codex, either for OpenCode).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<String>,
    /// Appended verbatim to the end of every prompt of this role.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub append: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum TrackerKind {
    Linear,
    Github,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TrackerConfig {
    pub kind: TrackerKind,
    /// Linear team key; unused for GitHub Issues, which reads the repository's own issues.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SlackConfig {
    pub channel: String,
    /// The Slack users the factory responds to and whose approvals count.
    pub owners: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, TS)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ready: Option<String>,
    /// The product has nothing to start (a library, a CLI): agents check with tests only and no
    /// preview is offered.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub nothing_to_run: bool,
}

impl AppConfig {
    /// Whether setup has settled how the product runs: with commands, or with nothing to run.
    pub fn settled(&self) -> bool {
        self.start.is_some() || self.nothing_to_run
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, TS)]
#[serde(deny_unknown_fields)]
pub struct GuardConfig {
    /// Command patterns agents must not run, such as production access.
    #[serde(default)]
    pub deny: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, TS)]
#[serde(deny_unknown_fields)]
pub struct ReviewConfig {
    /// Review bots whose comments are judged like a write-access reviewer's.
    #[serde(default)]
    pub bots: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    #[serde(default = "default_issues_at_once")]
    pub issues_at_once: u32,
    /// No limit when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_rounds: Option<u32>,
    /// No limit when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix_ci_rounds: Option<u32>,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            issues_at_once: default_issues_at_once(),
            review_rounds: None,
            fix_ci_rounds: None,
        }
    }
}

fn default_issues_at_once() -> u32 {
    3
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("{path}: {message}")]
    Parse { path: String, message: String },
    #[error("{path}: {key}: {message}")]
    Invalid {
        path: String,
        key: String,
        message: String,
    },
}

impl Config {
    pub fn parse(text: &str, path: &str) -> Result<Config, ConfigError> {
        let config: Config = toml::from_str(text).map_err(|e| ConfigError::Parse {
            path: path.into(),
            message: e.to_string(),
        })?;
        config.check_shape(path)?;
        Ok(config)
    }

    /// Reads `.ovenspring/config.toml` under `repo` and checks it against the repository.
    pub fn load(repo: &Path) -> Result<Config, ConfigError> {
        let config = Config::read(repo)?;
        let shown = repo.join(CONFIG_FILE).display().to_string();
        config.check_shape(&shown)?;
        config.check_repository(repo, &shown)?;
        Ok(config)
    }

    /// Reads `.ovenspring/config.toml` under `repo` as it is, unchecked: for the dashboard to show
    /// and correct a file whose values `load` refuses.
    pub fn read(repo: &Path) -> Result<Config, ConfigError> {
        let path = repo.join(CONFIG_FILE);
        let shown = path.display().to_string();
        let text = std::fs::read_to_string(&path).map_err(|source| ConfigError::Read {
            path: shown.clone(),
            source,
        })?;
        toml::from_str(&text).map_err(|e| ConfigError::Parse {
            path: shown,
            message: e.to_string(),
        })
    }

    /// Checks a configuration built in code (setup) the way `load` checks the file.
    pub fn check(&self, repo: &Path) -> Result<(), ConfigError> {
        self.check_shape(CONFIG_FILE)?;
        self.check_repository(repo, CONFIG_FILE)
    }

    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string(self)
    }

    fn check_folders(&self, path: &str) -> Result<(), ConfigError> {
        for (key, folder, example) in [
            (
                "agreements.path",
                &self.agreements.path,
                "docs/factory/agreements",
            ),
            ("knowledge.path", &self.knowledge.path, "docs/knowledge"),
        ] {
            let inside = !folder.trim().is_empty()
                && Path::new(folder)
                    .components()
                    .all(|c| matches!(c, std::path::Component::Normal(_)));
            if !inside {
                return Err(ConfigError::Invalid {
                    path: path.into(),
                    key: key.into(),
                    message: format!("Give a folder inside the repository, such as {example}."),
                });
            }
        }
        Ok(())
    }

    fn check_shape(&self, path: &str) -> Result<(), ConfigError> {
        let invalid = |key: &str, message: String| ConfigError::Invalid {
            path: path.into(),
            key: key.into(),
            message,
        };
        if self.repo.base_branch.trim().is_empty() {
            return Err(invalid("repo.base_branch", "Enter the base branch.".into()));
        }
        if self.tracker.kind == TrackerKind::Linear
            && self.tracker.team.as_deref().unwrap_or("").is_empty()
        {
            return Err(invalid("tracker.team", "Enter the Linear team key.".into()));
        }
        if !is_slack_id(&self.slack.channel, &['C', 'G']) {
            return Err(invalid(
                "slack.channel",
                format!(
                    "{} is not a channel ID; channel IDs start with C, such as C0123456789.",
                    self.slack.channel
                ),
            ));
        }
        if self.slack.owners.is_empty() {
            return Err(invalid(
                "slack.owners",
                "Enter at least one Slack member ID.".into(),
            ));
        }
        if let Some(bad) = self
            .slack
            .owners
            .iter()
            .find(|id| !is_slack_id(id, &['U', 'W']))
        {
            return Err(invalid(
                "slack.owners",
                format!("{bad} is not a member ID; member IDs start with U, such as U0123456789."),
            ));
        }
        for (category, held) in &self.held {
            if let Approval::Named(name) = category {
                return Err(invalid(
                    &format!("held.{name}"),
                    "Hold by path only schema, external-connection, infrastructure, ci or external-contract; describe other kinds of change under [person_reviews].".into(),
                ));
            }
            if *category == Approval::FactoryConfiguration {
                return Err(invalid(
                    "held.factory-configuration",
                    "The factory's own configuration is always held; remove this entry.".into(),
                ));
            }
            if held.paths.is_empty() {
                return Err(invalid(
                    &format!("held.{}.paths", category.as_str()),
                    "Enter at least one path pattern.".into(),
                ));
            }
        }
        for (name, description) in &self.person_reviews {
            if Approval::of(name).is_approved_with_the_design() || name.trim().is_empty() {
                return Err(invalid(
                    "person_reviews",
                    format!(
                        "`{name}`: give it a name of your own; the factory's own categories are held by path."
                    ),
                ));
            }
            if description.trim().is_empty() {
                return Err(invalid(
                    "person_reviews",
                    format!("`{name}`: describe which changes it covers."),
                ));
            }
        }
        self.check_folders(path)?;
        if self.app.nothing_to_run && self.app.start.is_some() {
            return Err(invalid(
                "app.nothing_to_run",
                "Remove `start`, or `nothing_to_run`: the product either starts or has nothing to run."
                    .into(),
            ));
        }
        if self.limits.issues_at_once == 0 {
            return Err(invalid("limits.issues_at_once", "Enter 1 or more.".into()));
        }
        Ok(())
    }

    fn check_repository(&self, repo: &Path, path: &str) -> Result<(), ConfigError> {
        for role in Role::ALL {
            let settings = self.roles.get(role);
            let dirs = settings.cli.skill_dirs();
            for skill in &settings.skills {
                if !dirs
                    .iter()
                    .any(|d| repo.join(d).join(skill).join("SKILL.md").is_file())
                {
                    return Err(ConfigError::Invalid {
                        path: path.into(),
                        key: format!("roles.{}.skills", role.as_str()),
                        message: format!(
                            "{skill} is not in {}, where this role's agent loads skills. Put it there, or uncheck it.",
                            dirs.join(" or ")
                        ),
                    });
                }
            }
        }
        Ok(())
    }
}

fn is_slack_id(id: &str, prefixes: &[char]) -> bool {
    let mut chars = id.chars();
    matches!(chars.next(), Some(c) if prefixes.contains(&c))
        && id.len() >= 9
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
[roles.lead]
cli = "codex"
[roles.executor]
cli = "codex"
[roles.reviewer]
cli = "codex"
[roles.analyzer]
cli = "codex"
[tracker]
kind = "linear"
team = "ACME"
[slack]
channel = "C0123456789"
owners = ["U0123456789"]
"#;

    #[test]
    fn minimal_configuration_takes_the_documented_defaults() {
        let c = Config::parse(MINIMAL, "config.toml").unwrap();
        assert_eq!(c.repo.base_branch, "main");
        assert_eq!(c.merge.by, MergeBy::Factory);
        assert_eq!(c.agents.permissions, Permissions::Auto);
        assert_eq!(c.agents.mcp, Vec::<String>::new());
        assert_eq!(c.limits.issues_at_once, 3);
        assert_eq!(c.limits.review_rounds, None);
        assert_eq!(c.limits.fix_ci_rounds, None);
        assert!(!c.app.settled());
    }

    #[test]
    fn a_role_without_its_agent_is_rejected_by_name() {
        let text = MINIMAL.replace(
            "[roles.reviewer]\ncli = \"codex\"\n",
            "[roles.reviewer]\nmodel = \"m\"\n",
        );
        let err = Config::parse(&text, "config.toml").unwrap_err().to_string();
        assert!(err.contains("cli"), "{err}");
        let text = MINIMAL.replace("[roles.analyzer]\ncli = \"codex\"\n", "");
        let err = Config::parse(&text, "config.toml").unwrap_err().to_string();
        assert!(err.contains("analyzer"), "{err}");
    }

    #[test]
    fn the_product_runs_with_commands_or_has_nothing_to_run_not_both() {
        let commands = format!("{MINIMAL}\n[app]\nstart = \"pnpm dev\"\n");
        assert!(
            Config::parse(&commands, "config.toml")
                .unwrap()
                .app
                .settled()
        );
        let nothing = format!("{MINIMAL}\n[app]\nnothing_to_run = true\n");
        assert!(
            Config::parse(&nothing, "config.toml")
                .unwrap()
                .app
                .settled()
        );
        let both = format!("{MINIMAL}\n[app]\nstart = \"pnpm dev\"\nnothing_to_run = true\n");
        let err = Config::parse(&both, "config.toml").unwrap_err().to_string();
        assert!(err.contains("app.nothing_to_run"), "{err}");
    }

    #[test]
    fn an_unknown_key_is_reported_with_its_name() {
        let text = MINIMAL.replace("[slack]", "[limits]\nreview_round = 2\n[slack]");
        let err = Config::parse(&text, "config.toml").unwrap_err().to_string();
        assert!(err.contains("review_round"), "{err}");
    }

    #[test]
    fn a_linear_tracker_without_a_team_is_rejected() {
        let text = MINIMAL.replace("team = \"ACME\"\n", "");
        let err = Config::parse(&text, "config.toml").unwrap_err().to_string();
        assert!(err.contains("tracker.team"), "{err}");
    }

    #[test]
    fn a_slack_user_id_in_the_channel_field_is_rejected() {
        let text = MINIMAL.replace("channel = \"C0123456789\"", "channel = \"U0123456789\"");
        let err = Config::parse(&text, "config.toml").unwrap_err().to_string();
        assert!(err.contains("slack.channel"), "{err}");
    }

    #[test]
    fn an_empty_owner_list_is_rejected() {
        let text = MINIMAL.replace("owners = [\"U0123456789\"]", "owners = []");
        let err = Config::parse(&text, "config.toml").unwrap_err().to_string();
        assert!(err.contains("slack.owners"), "{err}");
    }

    #[test]
    fn a_held_category_must_be_one_the_gate_knows() {
        let ok = format!("{MINIMAL}\n[held]\nschema = {{ paths = [\"db/**\"] }}\n");
        let held = Config::parse(&ok, "config.toml").unwrap().held;
        assert_eq!(held.keys().cloned().collect::<Vec<_>>(), [Approval::Schema]);

        let unknown = format!("{MINIMAL}\n[held]\nsecrets = {{ paths = [\"vault/**\"] }}\n");
        let err = Config::parse(&unknown, "config.toml")
            .unwrap_err()
            .to_string();
        assert!(err.contains("secrets"), "{err}");

        let always = format!(
            "{MINIMAL}\n[held]\nfactory-configuration = {{ paths = [\".ovenspring/**\"] }}\n"
        );
        let err = Config::parse(&always, "config.toml")
            .unwrap_err()
            .to_string();
        assert!(err.contains("held.factory-configuration"), "{err}");
    }

    #[test]
    fn agreements_and_knowledge_live_in_folders_inside_the_repository() {
        let default = Config::parse(MINIMAL, "config.toml").unwrap();
        assert_eq!(default.agreements.path, ".ovenspring/agreements");
        assert_eq!(default.knowledge.path, ".ovenspring/knowledge");
        let ok = format!(
            "{MINIMAL}\n[agreements]\npath = \"docs/factory/agreements\"\n[knowledge]\npath = \"docs/knowledge\"\n"
        );
        let ok = Config::parse(&ok, "config.toml").unwrap();
        assert_eq!(ok.agreements.path, "docs/factory/agreements");
        assert_eq!(ok.knowledge.path, "docs/knowledge");
        for section in ["agreements", "knowledge"] {
            for bad in ["/tmp/notes", "../notes", "docs/../../x", ""] {
                let text = format!("{MINIMAL}\n[{section}]\npath = \"{bad}\"\n");
                let err = Config::parse(&text, "config.toml").unwrap_err().to_string();
                assert!(err.contains(&format!("{section}.path")), "{bad}: {err}");
            }
        }
    }

    #[test]
    fn a_kind_of_change_a_person_reviews_has_the_owners_name_and_words() {
        let ok = format!(
            "{MINIMAL}\n[person_reviews]\npayments = \"How customers are charged or refunded, src/billing/** included.\"\n"
        );
        let config = Config::parse(&ok, "config.toml").unwrap();
        assert_eq!(
            config.person_reviews.keys().collect::<Vec<_>>(),
            ["payments"]
        );

        for (bad, why) in [
            ("schema = \"Migrations\"", "`schema`"),
            ("payments = \" \"", "`payments`"),
        ] {
            let text = format!("{MINIMAL}\n[person_reviews]\n{bad}\n");
            let err = Config::parse(&text, "config.toml").unwrap_err().to_string();
            assert!(err.contains(why), "{err}");
        }
    }

    #[test]
    fn a_selected_skill_must_exist_in_the_repository() {
        let repo = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(repo.path().join(".ovenspring")).unwrap();
        let text = MINIMAL.replace(
            "[roles.lead]\ncli = \"codex\"",
            "[roles.lead]\ncli = \"claude\"\nskills = [\"payments-domain\"]",
        );
        std::fs::write(repo.path().join(CONFIG_FILE), &text).unwrap();
        let err = Config::load(repo.path()).unwrap_err().to_string();
        assert!(err.contains("roles.lead.skills"), "{err}");

        // Claude Code does not read `.agents/skills`.
        let skill = |dir: &str| {
            let skill = repo.path().join(dir).join("payments-domain");
            std::fs::create_dir_all(&skill).unwrap();
            std::fs::write(skill.join("SKILL.md"), "---\nname: payments-domain\n---\n").unwrap();
        };
        skill(".agents/skills");
        let err = Config::load(repo.path()).unwrap_err().to_string();
        assert!(err.contains(".claude/skills"), "{err}");
        skill(".claude/skills");
        assert!(Config::load(repo.path()).is_ok());
    }

    #[test]
    fn serialization_round_trips() {
        let c = Config::parse(MINIMAL, "config.toml").unwrap();
        assert_eq!(
            Config::parse(&c.to_toml().unwrap(), "config.toml").unwrap(),
            c
        );
    }
}
