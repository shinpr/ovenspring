//! The connection checks the dashboard's setup runs before it saves, against the real services.

use std::path::Path;

use ovenspring_integrations::channel::slack::Slack;
use ovenspring_integrations::github::{self, GitHub as _, GitHubApi, RepoId};
use ovenspring_integrations::http::Ureq;
use ovenspring_integrations::tracker::Tracker as _;
use ovenspring_integrations::tracker::linear::Linear;
use ovenspring_server::setup::{GitHubAccount, SetupChecks, SlackSetup, TrackerSetup};

pub struct Checks;

/// The repository's `origin` on GitHub.
pub fn repo_id(repo: &Path) -> Option<RepoId> {
    let remote = ovenspring_daemon::worktree::git(repo, &["remote", "get-url", "origin"]).ok()?;
    RepoId::from_remote(&remote)
}

fn github_api(repo: &Path, token: Option<&str>) -> Result<(GitHubApi<Ureq>, bool), String> {
    let id = repo_id(repo).ok_or_else(|| {
        let remote = ovenspring_daemon::worktree::git(repo, &["remote", "get-url", "origin"])
            .unwrap_or_default();
        format!(
            "This repository's `origin` ({remote}) is not a GitHub address the factory reads: it takes git@github.com:owner/name or https://github.com/owner/name."
        )
    })?;
    let from_gh = token.is_none();
    let t = github::selected_token(token.map(str::to_owned))
        .ok_or("No GitHub token: enter one, or sign in with `gh auth login` and save again.")?;
    Ok((GitHubApi::new(Ureq::new(), id, t), from_gh))
}

impl SetupChecks for Checks {
    fn tracker(
        &self,
        repo: &Path,
        tracker: &TrackerSetup,
        github: Option<&str>,
    ) -> Result<String, String> {
        match tracker {
            TrackerSetup::Linear { team, api_key } => {
                let linear = Linear::new(
                    Ureq::new(),
                    api_key.trim().to_owned(),
                    team.trim().to_owned(),
                );
                let name = linear
                    .team_name()
                    .map_err(|e| format!("Linear refused the key or the team ({e})."))?;
                let open = linear
                    .open_issues()
                    .map_err(|e| format!("Linear refused the key or the team ({e})."))?;
                Ok(format!(
                    "Team {} ({name}), {}.",
                    team.trim(),
                    open_issues(open.len())
                ))
            }
            TrackerSetup::Github => {
                let (api, _) = github_api(repo, github)?;
                let open = api.open_issues().map_err(|e| {
                    format!("GitHub refused to list this repository's issues ({e}).")
                })?;
                Ok(format!("This repository, {}.", open_issues(open.len())))
            }
        }
    }

    fn slack(&self, slack: &SlackSetup) -> Result<String, String> {
        Slack::new(
            Ureq::new(),
            slack.bot_token.trim().to_owned(),
            slack.channel.trim().to_owned(),
        )
        .check_setup(slack.app_token.trim(), slack.owner.trim())
    }

    fn github(
        &self,
        repo: &Path,
        token: Option<&str>,
        base_branch: &str,
    ) -> Result<GitHubAccount, String> {
        let (api, from_gh) = github_api(repo, token)?;
        let login = api
            .viewer()
            .map_err(|e| format!("GitHub refused the token ({e})."))?
            .login;
        api.branch_head(base_branch.trim())
            .map_err(|e| format!("GitHub has no branch `{}` here ({e}).", base_branch.trim()))?;
        Ok(GitHubAccount { login, from_gh })
    }
}

fn open_issues(n: usize) -> String {
    if n == 1 {
        "1 open issue".into()
    } else {
        format!("{n} open issues")
    }
}
