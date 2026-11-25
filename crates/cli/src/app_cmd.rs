//! `ovenspring app run`: an agent runs a command against the product from its issue's worktree,
//! on the issue's own port and data, starting the product first when needed.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow, bail};
use ovenspring_core::config::Config;
use ovenspring_daemon::app::{App, READY_WAIT};
use ovenspring_store::{LAUNCH_ENV, Store};

/// The issue worktree containing `dir`: `<state>/worktrees/<issue>`. Returns the state
/// directory, the issue key and the worktree.
fn enclosing_worktree(dir: &Path) -> Option<(PathBuf, String, PathBuf)> {
    dir.ancestors().find_map(|d| {
        let parent = d.parent()?;
        (parent.file_name()? == "worktrees").then(|| {
            Some((
                parent.parent()?.to_path_buf(),
                d.file_name()?.to_string_lossy().into_owned(),
                d.to_path_buf(),
            ))
        })?
    })
}

/// Refuses unless the calling launch owns an active item of this issue.
fn check_launch(state: &Path, issue: &str) -> Result<()> {
    let launch = std::env::var(LAUNCH_ENV)
        .map_err(|_| anyhow!("`ovenspring app run` is for factory launches"))?;
    let store = Store::open(state)?;
    let item = store
        .read_item(&store.active_dir().join(format!("{launch}.md")))
        .map_err(|_| anyhow!("{LAUNCH_ENV} names {launch:?}, which is not an active item"))?;
    if item.front.issue.as_deref() != Some(issue) {
        bail!("{launch} is not an item of {issue}; run `ovenspring app` from your own worktree");
    }
    Ok(())
}

/// The configuration the factory runs on: the one in the owner's working tree, which shares its
/// git directory with the issue's worktree.
fn load_app_config(worktree: &Path) -> Result<Config> {
    let common = ovenspring_daemon::worktree::git(
        worktree,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let repo = Path::new(common.trim())
        .parent()
        .ok_or_else(|| anyhow!("{common} has no working tree"))?;
    Config::load(repo).with_context(|| format!("reading the configuration in {}", repo.display()))
}

pub fn run(command: &[String]) -> Result<ExitCode> {
    let cwd = std::env::current_dir()?.canonicalize()?;
    let (state, issue, worktree) = enclosing_worktree(&cwd).ok_or_else(|| {
        anyhow!(
            "`ovenspring app run` runs inside an issue's worktree; {} is not one",
            cwd.display()
        )
    })?;
    check_launch(&state, &issue)?;
    let config = load_app_config(&worktree)?;
    let app = App {
        state: &state,
        issue: &issue,
        worktree: &worktree,
        config: &config.app,
    };
    let status = app.run(command, READY_WAIT)?;
    Ok(status
        .code()
        .and_then(|c| u8::try_from(c).ok())
        .map_or(ExitCode::FAILURE, ExitCode::from))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_issue_worktree_from_any_directory_inside_it() {
        let found = enclosing_worktree(Path::new("/s/repo-1/worktrees/ACME-1/src/app"));
        assert_eq!(
            found,
            Some((
                PathBuf::from("/s/repo-1"),
                "ACME-1".into(),
                PathBuf::from("/s/repo-1/worktrees/ACME-1")
            ))
        );
        assert_eq!(enclosing_worktree(Path::new("/home/me/repo/src")), None);
    }
}
