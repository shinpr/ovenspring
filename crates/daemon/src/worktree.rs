//! One git worktree and branch per issue, under the state directory.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use ovenspring_integrations::github::{Account, GitRemote};
use ovenspring_store::Store;

#[derive(Debug, thiserror::Error)]
#[error("git {args}: {message}")]
pub struct GitError {
    pub args: String,
    pub message: String,
}

pub fn git(dir: &Path, args: &[&str]) -> Result<String, GitError> {
    run(Command::new("git"), dir, args)
}

/// Runs `git` with `account` as the author and committer of every commit it creates, without
/// writing any config.
pub fn git_as(dir: &Path, account: &Account, args: &[&str]) -> Result<String, GitError> {
    let email = account.email();
    let mut command = Command::new("git");
    command
        .env("GIT_AUTHOR_NAME", &account.login)
        .env("GIT_AUTHOR_EMAIL", &email)
        .env("GIT_COMMITTER_NAME", &account.login)
        .env("GIT_COMMITTER_EMAIL", &email);
    run(command, dir, args)
}

/// Runs a `git` command that talks to `origin` (fetch, `ls-remote`, push), authenticated with the
/// connection's token alone, and only while every fetch and push URL of `origin` is the URL the
/// connection was built from. A refused `origin` is a `GitError` and no remote is contacted.
pub fn git_remote(dir: &Path, remote: &GitRemote<'_>, args: &[&str]) -> Result<String, GitError> {
    remote_with(dir, remote, &inherited_config(), args)
}

/// The check `git_remote` makes before its command: `Ok` when `origin` may be reached with
/// `remote`'s token.
pub fn connected(dir: &Path, remote: &GitRemote<'_>) -> Result<(), GitError> {
    let environment = remote_environment(remote, &inherited_config(), &["remote", "get-url"])?;
    admit(dir, remote, &environment, &["remote", "get-url"])
}

fn run(mut command: Command, dir: &Path, args: &[&str]) -> Result<String, GitError> {
    let out = command
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| GitError {
            args: args.join(" "),
            message: e.to_string(),
        })?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(GitError {
            args: args.join(" "),
            message: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        })
    }
}

const CONFIG_COUNT: &str = "GIT_CONFIG_COUNT";
const CONFIG_KEY: &str = "GIT_CONFIG_KEY_";
const CONFIG_VALUE: &str = "GIT_CONFIG_VALUE_";
/// Set by `git -c` for the processes it runs. Git reads it after `GIT_CONFIG_COUNT`'s entries, so
/// a helper or header in it would override the resets; remote commands never pass it on.
const CONFIG_PARAMETERS: &str = "GIT_CONFIG_PARAMETERS";
const TOKEN_VARIABLE: &str = "OVENSPRING_GITHUB_TOKEN";
/// Answers Git's HTTPS credential request with the token, never storing or erasing anything.
const TOKEN_HELPER: &str = "!f() { test \"$1\" = get && echo username=x-access-token && echo \"password=$OVENSPRING_GITHUB_TOKEN\"; }; f";

/// The daemon's command-scoped Git config, which its Git children would inherit.
fn inherited_config() -> Vec<(OsString, OsString)> {
    std::env::vars_os()
        .filter(|(name, _)| {
            let name = name.to_string_lossy();
            name == CONFIG_COUNT
                || name == CONFIG_PARAMETERS
                || name.starts_with(CONFIG_KEY)
                || name.starts_with(CONFIG_VALUE)
        })
        .collect()
}

fn remote_with(
    dir: &Path,
    remote: &GitRemote<'_>,
    inherited: &[(OsString, OsString)],
    args: &[&str],
) -> Result<String, GitError> {
    let environment = remote_environment(remote, inherited, args)?;
    admit(dir, remote, &environment, args)?;
    run(remote_command(&environment), dir, args)
}

/// The variables a remote command and its guard run with: the inherited `GIT_CONFIG_*` entries
/// kept, then the factory's entries appended after them so they are read last.
fn remote_environment(
    remote: &GitRemote<'_>,
    inherited: &[(OsString, OsString)],
    args: &[&str],
) -> Result<Vec<(OsString, OsString)>, GitError> {
    let count = match inherited.iter().find(|(name, _)| name == CONFIG_COUNT) {
        None => 0,
        Some((_, value)) => value
            .to_str()
            .and_then(|value| value.parse::<usize>().ok())
            .ok_or_else(|| GitError {
                args: args.join(" "),
                message: format!(
                    "the inherited {CONFIG_COUNT} `{}` is not a count; \
                     remote commands refuse to run with it",
                    value.to_string_lossy()
                ),
            })?,
    };
    let header_reset = format!("http.{}.extraHeader", remote.url);
    let factory = [
        ("credential.helper", ""),
        ("credential.helper", TOKEN_HELPER),
        (header_reset.as_str(), ""),
    ];
    let mut environment: Vec<(OsString, OsString)> = inherited
        .iter()
        .filter(|(name, _)| name != CONFIG_COUNT && name != CONFIG_PARAMETERS)
        .cloned()
        .collect();
    for (offset, (key, value)) in factory.iter().enumerate() {
        let index = count + offset;
        environment.push((format!("{CONFIG_KEY}{index}").into(), (*key).into()));
        environment.push((format!("{CONFIG_VALUE}{index}").into(), (*value).into()));
    }
    environment.push((
        CONFIG_COUNT.into(),
        (count + factory.len()).to_string().into(),
    ));
    environment.push((TOKEN_VARIABLE.into(), remote.token.into()));
    Ok(environment)
}

fn remote_command(environment: &[(OsString, OsString)]) -> Command {
    let mut command = Command::new("git");
    command.env_remove(CONFIG_PARAMETERS).envs(
        environment
            .iter()
            .map(|(name, value)| (name.as_os_str(), value.as_os_str())),
    );
    command
}

/// The connection guard: refuses an SSH or password-bearing connected URL, and any fetch or push
/// URL of `origin`, read under the command's own environment so rewrites match, that is not the
/// connected URL.
fn admit(
    dir: &Path,
    remote: &GitRemote<'_>,
    environment: &[(OsString, OsString)],
    args: &[&str],
) -> Result<(), GitError> {
    let refused = |message: String| GitError {
        args: args.join(" "),
        message,
    };
    let connected = redacted(remote.url);
    if remote.url.starts_with("ssh://") || remote.url.starts_with("git@github.com:") {
        return Err(refused(format!(
            "origin {connected} uses SSH; the factory reaches GitHub only over HTTPS with its token"
        )));
    }
    if has_password(remote.url) {
        return Err(refused(format!(
            "origin {connected} carries a password, which Git would send instead of the GitHub token"
        )));
    }
    for listing in [
        &["remote", "get-url", "--all", "origin"][..],
        &["remote", "get-url", "--push", "--all", "origin"][..],
    ] {
        let urls = run(remote_command(environment), dir, listing)?;
        if let Some(other) = urls.lines().find(|url| *url != remote.url) {
            return Err(refused(format!(
                "origin reaches {}, not {connected}, the repository the GitHub connection was built for; restore origin, or restart the daemon to connect to the new one",
                redacted(other)
            )));
        }
    }
    Ok(())
}

/// The user part of `scheme://user:password@host/...`, when the URL has one.
fn user_info(url: &str) -> Option<(usize, &str)> {
    let start = url.find("://")? + 3;
    let authority = &url[start..];
    let authority = &authority[..authority.find('/').unwrap_or(authority.len())];
    let end = authority.rfind('@')?;
    Some((start, &authority[..end]))
}

fn has_password(url: &str) -> bool {
    user_info(url).is_some_and(|(_, user)| user.contains(':'))
}

/// `url` with any password replaced by `***`, for messages that are recorded.
fn redacted(url: &str) -> String {
    match user_info(url) {
        Some((start, user)) => match user.find(':') {
            Some(colon) => {
                let password_start = start + colon + 1;
                let password_end = start + user.len();
                format!("{}***{}", &url[..password_start], &url[password_end..])
            }
            None => url.to_string(),
        },
        None => url.to_string(),
    }
}

pub fn branch_for(issue: &str) -> String {
    format!("ovenspring/{issue}")
}

/// The issue's worktree, created from the base branch on first use.
pub fn ensure(repo: &Path, store: &Store, issue: &str, base: &str) -> Result<PathBuf, GitError> {
    let path = store.worktree(issue);
    if path.join(".git").exists() {
        return Ok(path);
    }
    prune(repo)?;
    let branch = branch_for(issue);
    let path_str = path.display().to_string();
    let branch_exists = git(
        repo,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
    )
    .is_ok();
    if branch_exists {
        git(repo, &["worktree", "add", &path_str, &branch])?;
    } else {
        let start = if git(
            repo,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/remotes/origin/{base}"),
            ],
        )
        .is_ok()
        {
            format!("origin/{base}")
        } else {
            base.to_string()
        };
        git(repo, &["worktree", "add", "-b", &branch, &path_str, &start])?;
    }
    Ok(path)
}

/// The detached checkout of the base branch that launches for no issue work in.
pub const BASE: &str = "base";

/// A detached checkout of `origin/<base>` under the state directory, moved to what `origin/<base>`
/// is now; the caller fetches the base first, with the connection's token.
pub fn ensure_detached(
    repo: &Path,
    store: &Store,
    name: &str,
    base: &str,
) -> Result<PathBuf, GitError> {
    let path = store.worktree(name);
    let start = format!("origin/{base}");
    if path.join(".git").exists() {
        git(&path, &["checkout", "-q", "--detach", &start])?;
    } else {
        prune(repo)?;
        git(
            repo,
            &[
                "worktree",
                "add",
                "-q",
                "--detach",
                &path.display().to_string(),
                &start,
            ],
        )?;
    }
    Ok(path)
}

/// Forgets worktrees whose directory is gone, such as those of a state directory that was moved
/// or deleted; git refuses to add a worktree at a path it still has registered.
fn prune(repo: &Path) -> Result<(), GitError> {
    git(repo, &["worktree", "prune"]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bare(path: &Path) -> String {
        git(
            path.parent().unwrap(),
            &["init", "-q", "--bare", "-b", "main", path.to_str().unwrap()],
        )
        .unwrap();
        path.display().to_string()
    }

    /// A repository with one commit on `main` whose `origin` is `url`.
    fn repository(dir: &Path, url: &str) -> PathBuf {
        let repo = dir.join("repo");
        git(dir, &["init", "-q", "-b", "main", repo.to_str().unwrap()]).unwrap();
        git(
            &repo,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.com",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "start",
            ],
        )
        .unwrap();
        git(&repo, &["remote", "add", "origin", url]).unwrap();
        repo
    }

    fn refs(bare: &str) -> String {
        git(Path::new(bare), &["for-each-ref"]).unwrap()
    }

    fn remote(url: &str) -> GitRemote<'_> {
        GitRemote {
            url,
            token: "test-token",
        }
    }

    const PUSH: [&str; 4] = ["push", "-q", "origin", "HEAD:refs/heads/feature"];

    // Behavior: origin uses SSH, in either form the connection accepts -> refused before Git
    //     runs, so the configured `core.sshCommand` never runs.
    #[test]
    fn an_ssh_origin_is_refused_without_running_ssh() {
        for url in [
            "ssh://git@127.0.0.1:1/x.git",
            "git@github.com:acme/storefront.git",
        ] {
            let dir = tempfile::tempdir().unwrap();
            let repo = repository(dir.path(), url);
            let marker = dir.path().join("ssh-ran");
            let ssh = dir.path().join("ssh.sh");
            std::fs::write(&ssh, format!("#!/bin/sh\ntouch '{}'\n", marker.display())).unwrap();
            git(
                dir.path(),
                &[
                    "config",
                    "--file",
                    repo.join(".git/config").to_str().unwrap(),
                    "core.sshCommand",
                    &format!("sh '{}'", ssh.display()),
                ],
            )
            .unwrap();

            let result = git_remote(&repo, &remote(url), &PUSH);

            assert!(result.is_err(), "{url}: {result:?}");
            assert!(!marker.exists(), "{url}: ssh ran");
        }
    }

    // Behavior: origin's pushurl leads to another repository -> refused naming both URLs, and
    //     nothing reaches that repository.
    #[test]
    fn a_pushurl_elsewhere_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let connected = bare(&dir.path().join("a.git"));
        let elsewhere = bare(&dir.path().join("b.git"));
        let repo = repository(dir.path(), &connected);
        git(
            &repo,
            &["remote", "set-url", "--push", "origin", &elsewhere],
        )
        .unwrap();

        let error = git_remote(&repo, &remote(&connected), &PUSH).unwrap_err();

        let text = error.to_string();
        assert!(
            text.contains(&connected) && text.contains(&elsewhere),
            "{text}"
        );
        assert_eq!(refs(&elsewhere), "");
    }

    // Behavior: origin changed to another repository after the connection was built -> a fetch
    //     is refused naming both URLs, and no tracking ref of the other repository appears.
    #[test]
    fn an_origin_changed_to_another_repository_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let connected = bare(&dir.path().join("a.git"));
        let other = bare(&dir.path().join("b.git"));
        let repo = repository(dir.path(), &other);
        git(&repo, &["push", "-q", &other, "HEAD:refs/heads/elsewhere"]).unwrap();

        let error = git_remote(
            &repo,
            &remote(&connected),
            &["fetch", "-q", "origin", "elsewhere"],
        )
        .unwrap_err();

        let text = error.to_string();
        assert!(text.contains(&connected) && text.contains(&other), "{text}");
        assert!(
            git(
                &repo,
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    "refs/remotes/origin/elsewhere"
                ]
            )
            .is_err()
        );
    }

    // Behavior: an inherited `GIT_CONFIG_*` entry rewrites origin to another repository -> the
    //     guard sees the rewrite the push would apply and refuses it naming both URLs.
    #[test]
    fn an_inherited_rewrite_of_origin_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let rewritten_to = bare(&dir.path().join("a.git"));
        let connected = bare(&dir.path().join("b.git"));
        let repo = repository(dir.path(), &connected);
        let inherited = [
            ("GIT_CONFIG_COUNT".into(), "1".into()),
            (
                "GIT_CONFIG_KEY_0".into(),
                format!("url.{rewritten_to}.insteadOf").into(),
            ),
            ("GIT_CONFIG_VALUE_0".into(), connected.clone().into()),
        ];

        let error = remote_with(&repo, &remote(&connected), &inherited, &PUSH).unwrap_err();

        let text = error.to_string();
        assert!(
            text.contains(&connected) && text.contains(&rewritten_to),
            "{text}"
        );
        assert_eq!(refs(&rewritten_to), "");
        assert_eq!(refs(&connected), "");
    }

    // Behavior: origin's URL carries a password -> refused, and the error shows `***` in its
    //     place.
    #[test]
    fn an_origin_with_a_password_is_refused_without_naming_it() {
        let url = "https://u:secret@127.0.0.1:1/x.git";
        let dir = tempfile::tempdir().unwrap();
        let repo = repository(dir.path(), url);

        let error = git_remote(&repo, &remote(url), &PUSH).unwrap_err();

        let text = error.to_string();
        assert!(text.contains("***") && !text.contains("secret"), "{text}");
    }

    // Behavior: an inherited `GIT_CONFIG_PARAMETERS` rewrite (from `git -c`) -> never reaches the
    //     command, so the push goes to the connected repository.
    #[test]
    fn inherited_git_config_parameters_are_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let rewritten_to = bare(&dir.path().join("a.git"));
        let connected = bare(&dir.path().join("b.git"));
        let repo = repository(dir.path(), &connected);
        let inherited = [(
            "GIT_CONFIG_PARAMETERS".into(),
            format!("'url.{rewritten_to}.insteadOf'='{connected}'").into(),
        )];

        let result = remote_with(&repo, &remote(&connected), &inherited, &PUSH);

        assert!(result.is_ok(), "{result:?}");
        assert!(refs(&connected).contains("refs/heads/feature"));
        assert_eq!(refs(&rewritten_to), "");
    }
}
