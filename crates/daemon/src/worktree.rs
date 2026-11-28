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
    /// Refused before any remote was contacted, because `origin` or the inherited config does not
    /// lead only to the connected repository: the factory's own failure, never GitHub out of reach.
    pub refused_locally: bool,
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
/// connection was built from. A refused `origin` is a `GitError` marked `refused_locally`, and no
/// remote is contacted. Fetch and push never recurse into submodules, whose remotes lie outside
/// the connected repository.
pub fn git_remote(dir: &Path, remote: &GitRemote<'_>, args: &[&str]) -> Result<String, GitError> {
    remote_with(dir, remote, &inherited_config(), args)
}

fn run(mut command: Command, dir: &Path, args: &[&str]) -> Result<String, GitError> {
    let out = command
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| GitError {
            args: args.join(" "),
            message: e.to_string(),
            refused_locally: false,
        })?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        // A hook's standard output reaches git's standard output, so what explains a refusal can
        // be on either stream; git's own error closes standard error.
        let message = [&out.stdout, &out.stderr]
            .map(|stream| String::from_utf8_lossy(stream).trim().to_string())
            .into_iter()
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        Err(GitError {
            args: args.join(" "),
            message,
            refused_locally: false,
        })
    }
}

const CONFIG_COUNT: &str = "GIT_CONFIG_COUNT";
const CONFIG_KEY: &str = "GIT_CONFIG_KEY_";
const CONFIG_VALUE: &str = "GIT_CONFIG_VALUE_";
/// Set by `git -c` for the processes it runs. Git reads it after `GIT_CONFIG_COUNT`'s entries, so
/// a helper or header in it would override the resets; remote commands never pass it on.
const CONFIG_PARAMETERS: &str = "GIT_CONFIG_PARAMETERS";
/// `0` turns off Git's redaction of `Authorization` in its traces, which would then carry the
/// token into a captured error or a trace file; remote commands never pass it on.
const TRACE_REDACT: &str = "GIT_TRACE_REDACT";
const TOKEN_VARIABLE: &str = "OVENSPRING_GITHUB_TOKEN";
const PROTOCOL_VARIABLE: &str = "OVENSPRING_GITHUB_PROTOCOL";
const HOST_VARIABLE: &str = "OVENSPRING_GITHUB_HOST";
/// Turns certificate verification off for every HTTPS request when set, whatever the config says;
/// remote commands never pass it on, so the token reaches only a verified GitHub.
const SSL_NO_VERIFY: &str = "GIT_SSL_NO_VERIFY";
/// The oldest Git that honors `http.<url>.proactiveAuth`. An older one ignores it, and libcurl then
/// answers GitHub's first 401 with the owner's netrc login before Git asks for the token.
const MINIMUM_GIT: (u32, u32) = (2, 46);
/// Answers a credential request with the token only when it is for the connected URL's protocol
/// and host, so a proxy never receives it. It never stores or erases anything.
const TOKEN_HELPER: &str = "!f() { test \"$1\" = get || exit 0; \
     while IFS='=' read -r key value; do case \"$key\" in \
     protocol) protocol=$value;; host) host=$value;; esac; done; \
     test \"$protocol\" = \"$OVENSPRING_GITHUB_PROTOCOL\" && test \"$host\" = \"$OVENSPRING_GITHUB_HOST\" && \
     echo username=x-access-token && echo \"password=$OVENSPRING_GITHUB_TOKEN\"; }; f";

/// The daemon's `GIT_CONFIG_COUNT` entries, which its Git children would inherit.
fn inherited_config() -> Vec<(OsString, OsString)> {
    std::env::vars_os()
        .filter(|(name, _)| {
            let name = name.to_string_lossy();
            name == CONFIG_COUNT || name.starts_with(CONFIG_KEY) || name.starts_with(CONFIG_VALUE)
        })
        .collect()
}

fn remote_with(
    dir: &Path,
    remote: &GitRemote<'_>,
    inherited: &[(OsString, OsString)],
    args: &[&str],
) -> Result<String, GitError> {
    supported_git(dir, args)?;
    let environment = remote_environment(remote, inherited, args)?;
    admit(dir, remote, &environment, args)?;
    run(remote_command(&environment), dir, &single_repository(args))
}

/// `fetch` and `push` with submodule recursion off, which the command line sets above any config.
fn single_repository<'a>(args: &[&'a str]) -> Vec<&'a str> {
    match args.split_first() {
        Some((&command, rest)) if command == "fetch" || command == "push" => {
            [command, "--no-recurse-submodules"]
                .into_iter()
                .chain(rest.iter().copied())
                .collect()
        }
        _ => args.to_vec(),
    }
}

/// Refuses, before any remote is contacted, a Git too old to send only the token.
fn supported_git(dir: &Path, args: &[&str]) -> Result<(), GitError> {
    let version = run(Command::new("git"), dir, &["--version"]).map_err(|e| GitError {
        refused_locally: true,
        ..e
    })?;
    if sends_only_the_token(&version) {
        Ok(())
    } else {
        let (major, minor) = MINIMUM_GIT;
        Err(refusal(
            args,
            format!(
                "{version} is older than Git {major}.{minor}, the oldest that sends the GitHub \
                 token instead of a netrc login; upgrade Git and resume"
            ),
        ))
    }
}

/// Whether `git --version`'s output names a Git at or above `MINIMUM_GIT`.
fn sends_only_the_token(version: &str) -> bool {
    let mut numbers = version
        .split_whitespace()
        .nth(2)
        .unwrap_or_default()
        .split('.')
        .map(str::parse::<u32>);
    match (numbers.next(), numbers.next()) {
        (Some(Ok(major)), Some(Ok(minor))) => (major, minor) >= MINIMUM_GIT,
        _ => false,
    }
}

fn refusal(args: &[&str], message: String) -> GitError {
    GitError {
        args: args.join(" "),
        message,
        refused_locally: true,
    }
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
            .ok_or_else(|| {
                refusal(
                    args,
                    format!(
                        "the inherited {CONFIG_COUNT} `{}` is not a count; \
                         remote commands refuse to run with it",
                        value.to_string_lossy()
                    ),
                )
            })?,
    };
    // Keyed to the exact URL, so they outrank any inherited setting for it (a generic key would
    // not: Git applies the most specific match).
    let header_reset = format!("http.{}.extraHeader", remote.url);
    // Credentials go with the first request, so libcurl never fills them from the owner's netrc.
    let proactive = format!("http.{}.proactiveAuth", remote.url);
    // libcurl keeps sending the credential on a redirect within the host, which can lead to
    // another repository; a redirect fails the command instead.
    let no_redirects = format!("http.{}.followRedirects", remote.url);
    let verify = format!("http.{}.sslVerify", remote.url);
    let factory = [
        ("credential.helper", ""),
        ("credential.helper", TOKEN_HELPER),
        (header_reset.as_str(), ""),
        (proactive.as_str(), "basic"),
        (no_redirects.as_str(), "false"),
        (verify.as_str(), "true"),
    ];
    let mut environment: Vec<(OsString, OsString)> = inherited
        .iter()
        .filter(|(name, _)| name != CONFIG_COUNT)
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
    let (protocol, host) = credential_context(remote.url).unwrap_or_default();
    environment.push((PROTOCOL_VARIABLE.into(), protocol.into()));
    environment.push((HOST_VARIABLE.into(), host.into()));
    environment.push((TOKEN_VARIABLE.into(), remote.token.into()));
    Ok(environment)
}

/// The `protocol` and `host` (with any port) Git names in a credential request for `url`.
fn credential_context(url: &str) -> Option<(&str, &str)> {
    let (protocol, rest) = url.split_once("://")?;
    let authority = rest.split('/').next()?;
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    Some((protocol, host))
}

fn remote_command(environment: &[(OsString, OsString)]) -> Command {
    let mut command = Command::new("git");
    command
        .env_remove(CONFIG_PARAMETERS)
        .env_remove(TRACE_REDACT)
        .env_remove(SSL_NO_VERIFY)
        // The daemon has no one to answer a prompt; a refused credential fails instead.
        .env("GIT_TERMINAL_PROMPT", "0")
        .envs(
            environment
                .iter()
                .map(|(name, value)| (name.as_os_str(), value.as_os_str())),
        );
    command
}

/// The connection guard: refuses an SSH or password-bearing connected URL, and any fetch or push
/// URL of `origin`, read under the command's own environment so rewrites match, that is not the
/// connected URL. Every refusal is local, a listing Git cannot read included.
fn admit(
    dir: &Path,
    remote: &GitRemote<'_>,
    environment: &[(OsString, OsString)],
    args: &[&str],
) -> Result<(), GitError> {
    let connected = redacted(remote.url);
    if remote.url.starts_with("ssh://") || remote.url.starts_with("git@github.com:") {
        return Err(refusal(
            args,
            format!(
                "origin {connected} uses SSH; the factory reaches GitHub only over HTTPS with its token"
            ),
        ));
    }
    if has_password(remote.url) {
        return Err(refusal(
            args,
            format!(
                "origin {connected} carries a password, which Git would send instead of the GitHub token"
            ),
        ));
    }
    for listing in [
        &["remote", "get-url", "--all", "origin"][..],
        &["remote", "get-url", "--push", "--all", "origin"][..],
    ] {
        let urls = run(remote_command(environment), dir, listing).map_err(|e| GitError {
            refused_locally: true,
            ..e
        })?;
        if let Some(other) = urls.lines().find(|url| *url != remote.url) {
            return Err(refusal(
                args,
                format!(
                    "origin reaches {}, not {connected}, the repository the GitHub connection was built for; restore origin, or restart the daemon to connect to the new one",
                    redacted(other)
                ),
            ));
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
    use std::io::{BufRead as _, BufReader, Write as _};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

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

    // Behavior: origin's pushurl leads to another repository -> refused locally naming both URLs,
    //     and nothing reaches that repository.
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
        assert!(error.refused_locally);
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
        // The other repository takes the branch itself: a push from `repo` to origin's URL can
        // also write `refs/remotes/origin/elsewhere`, the ref the act must never create.
        git(
            Path::new(&other),
            &[
                "fetch",
                "-q",
                repo.to_str().unwrap(),
                "main:refs/heads/elsewhere",
            ],
        )
        .unwrap();

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

    const CHILD_REPOSITORY: &str = "OVENSPRING_TEST_CHILD_REPOSITORY";
    const CHILD_URL: &str = "OVENSPRING_TEST_CHILD_URL";
    const CHILD_ARGS: &str = "OVENSPRING_TEST_CHILD_ARGS";
    const CHILD_RESULT: &str = "OVENSPRING_TEST_CHILD_RESULT";

    /// Runs `git_remote` in a copy of this test binary whose environment adds `variables`, as the
    /// daemon's environment would, since this process's own environment is shared by every test.
    fn remote_in_child(
        repo: &Path,
        url: &str,
        args: &[&str],
        variables: &[(&str, &str)],
    ) -> Result<String, String> {
        let result = repo.with_file_name("child-result");
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "worktree::tests::child_remote_command",
                "--ignored",
                "--quiet",
            ])
            .env(CHILD_REPOSITORY, repo)
            .env(CHILD_URL, url)
            .env(CHILD_ARGS, args.join("\n"))
            .env(CHILD_RESULT, &result)
            .envs(variables.iter().copied())
            .output()
            .unwrap();
        assert!(
            child.status.success(),
            "{}",
            String::from_utf8_lossy(&child.stdout)
        );
        let text = std::fs::read_to_string(&result).unwrap();
        let (outcome, output) = text.split_once('\n').unwrap();
        if outcome == "ok" {
            Ok(output.to_owned())
        } else {
            Err(output.to_owned())
        }
    }

    #[test]
    #[ignore = "run by remote_in_child, which supplies its environment"]
    fn child_remote_command() {
        let variable = |name| std::env::var(name).unwrap();
        let (repo, url, args) = (
            variable(CHILD_REPOSITORY),
            variable(CHILD_URL),
            variable(CHILD_ARGS),
        );
        let args: Vec<&str> = args.lines().collect();
        let text = match git_remote(Path::new(&repo), &remote(&url), &args) {
            Ok(output) => format!("ok\n{output}"),
            Err(e) => format!("err\n{e}"),
        };
        std::fs::write(variable(CHILD_RESULT), text).unwrap();
    }

    // Behavior: the daemon inherits a `GIT_CONFIG_PARAMETERS` rewrite (from `git -c`) of origin
    //     to another repository -> it never reaches the command, so the push goes to the connected
    //     repository only.
    #[test]
    fn an_inherited_git_config_parameters_rewrite_never_reaches_the_command() {
        let dir = tempfile::tempdir().unwrap();
        let rewritten_to = bare(&dir.path().join("a.git"));
        let connected = bare(&dir.path().join("b.git"));
        let repo = repository(dir.path(), &connected);
        let parameters = format!("'url.{rewritten_to}.insteadOf'='{connected}'");

        let result = remote_in_child(
            &repo,
            &connected,
            &PUSH,
            &[("GIT_CONFIG_PARAMETERS", &parameters)],
        );

        assert!(result.is_ok(), "{result:?}");
        assert!(refs(&connected).contains("refs/heads/feature"));
        assert_eq!(refs(&rewritten_to), "");
    }

    /// `Authorization` carrying `x-access-token` and `test-token`.
    const TOKEN_AUTHORIZATION: &str = "Basic eC1hY2Nlc3MtdG9rZW46dGVzdC10b2tlbg==";

    /// What a `record`ed endpoint saw of one request.
    #[derive(Debug, Default, Clone)]
    struct Seen {
        path: String,
        host: String,
        authorization: Option<String>,
        proxy_authorization: Option<String>,
    }

    /// A local HTTP endpoint that records every request and answers each with the status line and
    /// headers `answer(path, port)` gives.
    fn record(answer: fn(&str, u16) -> String) -> (u16, Arc<Mutex<Vec<Seen>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let recording = seen.clone();
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let (mut path, mut request, mut line) = (None, Seen::default(), String::new());
                while reader.read_line(&mut line).is_ok_and(|n| n > 0) && line != "\r\n" {
                    if path.is_none() {
                        path = line.split(' ').nth(1).map(str::to_owned);
                    } else if let Some((name, value)) = line.trim_end().split_once(": ") {
                        let value = value.to_owned();
                        match name.to_ascii_lowercase().as_str() {
                            "host" => request.host = value,
                            "authorization" => request.authorization = Some(value),
                            "proxy-authorization" => request.proxy_authorization = Some(value),
                            _ => {}
                        }
                    }
                    line.clear();
                }
                request.path = path.unwrap_or_default();
                let head = answer(&request.path, port);
                recording.lock().unwrap().push(request);
                write!(
                    stream,
                    "{head}Content-Length: 0\r\nConnection: close\r\n\r\n"
                )
                .ok();
            }
        });
        (port, seen)
    }

    fn challenge(_: &str, _: u16) -> String {
        "HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"git\"\r\n".into()
    }

    // Behavior: the owner's netrc holds a login for the connected host -> only the token is sent.
    #[test]
    fn a_netrc_login_for_the_connected_host_is_never_sent() {
        let dir = tempfile::tempdir().unwrap();
        let (port, seen) = record(challenge);
        let url = format!("http://127.0.0.1:{port}/connected.git");
        let repo = repository(dir.path(), &url);
        let home = dir.path().join("home");
        std::fs::create_dir(&home).unwrap();
        std::fs::write(
            home.join(".netrc"),
            "machine 127.0.0.1\nlogin netrc-user\npassword netrc-secret\n",
        )
        .unwrap();

        let result = remote_in_child(
            &repo,
            &url,
            &["ls-remote", "origin"],
            &[("HOME", home.to_str().unwrap())],
        );

        assert!(result.is_err(), "the endpoint refuses every credential");
        let authorizations: Vec<_> = seen
            .lock()
            .unwrap()
            .iter()
            .map(|s| s.authorization.clone())
            .collect();
        assert!(
            !authorizations.is_empty()
                && authorizations
                    .iter()
                    .all(|a| a.as_deref() == Some(TOKEN_AUTHORIZATION)),
            "{authorizations:?}"
        );
    }

    // Behavior: the daemon inherits Git's curl trace with redaction turned off, and an
    //     authenticated command fails -> its error carries the trace with the token redacted.
    #[test]
    fn an_inherited_trace_never_carries_the_token_into_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let (port, _) = record(challenge);
        let url = format!("http://127.0.0.1:{port}/connected.git");
        let repo = repository(dir.path(), &url);

        let error = remote_in_child(
            &repo,
            &url,
            &["ls-remote", "origin"],
            &[("GIT_TRACE_CURL", "1"), ("GIT_TRACE_REDACT", "0")],
        )
        .unwrap_err();

        assert!(error.contains("Authorization: Basic <redacted>"), "{error}");
        assert!(
            !error.contains(TOKEN_AUTHORIZATION.trim_start_matches("Basic "))
                && !error.contains("test-token"),
            "{error}"
        );
    }

    fn redirect_to_another_repository(path: &str, port: u16) -> String {
        if path.starts_with("/connected.git") {
            format!(
                "HTTP/1.1 301 Moved Permanently\r\n\
                 Location: http://127.0.0.1:{port}/elsewhere.git/info/refs?service=git-upload-pack\r\n"
            )
        } else {
            challenge(path, port)
        }
    }

    // Behavior: the connected URL redirects to another repository on the same host -> Git does
    //     not follow it, so that repository receives no request and no token.
    #[test]
    fn a_redirect_to_another_repository_on_the_host_is_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let (port, seen) = record(redirect_to_another_repository);
        let url = format!("http://127.0.0.1:{port}/connected.git");
        let repo = repository(dir.path(), &url);

        let result = git_remote(&repo, &remote(&url), &["ls-remote", "origin"]);

        assert!(result.is_err(), "{result:?}");
        let seen = seen.lock().unwrap().clone();
        assert!(
            seen.iter().any(|s| s.path.starts_with("/connected.git")),
            "control: the connected repository was asked: {seen:?}"
        );
        assert!(
            seen.iter().all(|s| !s.path.starts_with("/elsewhere.git")),
            "{seen:?}"
        );
    }

    // Behavior: the daemon inherits `GIT_SSL_NO_VERIFY` and the repository turns `http.sslVerify`
    //     off -> the remote command still verifies the connected URL's certificate.
    #[test]
    fn certificate_verification_stays_on_whatever_the_daemon_inherits() {
        let dir = tempfile::tempdir().unwrap();
        let connected = "https://github.example/connected.git";
        let repo = repository(dir.path(), connected);
        git(&repo, &["config", "http.sslVerify", "false"]).unwrap();
        let show = format!(
            "alias.ssl-verify=!printenv {SSL_NO_VERIFY} || \
             git config --type=bool --get-urlmatch http.sslVerify {connected}"
        );

        let result = remote_in_child(
            &repo,
            connected,
            &["-c", &show, "ssl-verify"],
            &[(SSL_NO_VERIFY, "1")],
        );

        assert_eq!(result, Ok("true".to_owned()));
    }

    // Behavior: the machine's Git is older than the one that honors proactive authentication ->
    //     the command is refused locally, naming the version, before anything is contacted.
    #[test]
    fn a_git_too_old_to_send_only_the_token_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let (port, seen) = record(challenge);
        let url = format!("http://127.0.0.1:{port}/connected.git");
        let repo = repository(dir.path(), &url);
        let bin = dir.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        let fake = bin.join("git");
        std::fs::write(&fake, "#!/bin/sh\necho 'git version 2.43.0'\n").unwrap();
        let mut permissions = std::fs::metadata(&fake).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
        std::fs::set_permissions(&fake, permissions).unwrap();

        let result = remote_in_child(
            &repo,
            &url,
            &["ls-remote", "origin"],
            &[("PATH", bin.to_str().unwrap())],
        );

        let error = result.unwrap_err();
        assert!(
            error.contains("git version 2.43.0") && error.contains("2.46"),
            "{error}"
        );
        assert!(seen.lock().unwrap().is_empty());
    }

    #[test]
    fn git_versions_from_2_46_send_only_the_token() {
        for (version, supported) in [
            ("git version 2.43.0", false),
            ("git version 2.46.0", true),
            ("git version 2.54.0 (Apple Git-157)", true),
            ("git version 2.47.1.windows.1", true),
            ("git version 3.0.0", true),
            ("not git", false),
        ] {
            assert_eq!(sends_only_the_token(version), supported, "{version}");
        }
    }

    fn proxy_challenge(_: &str, _: u16) -> String {
        "HTTP/1.1 407 Proxy Authentication Required\r\nProxy-Authenticate: Basic realm=\"proxy\"\r\n"
            .into()
    }

    // Behavior: the owner's Git config routes HTTP through a proxy whose credentials Git asks the
    //     helpers for -> the proxy never receives the token as its credential.
    #[test]
    fn a_proxy_asking_for_credentials_never_receives_the_token() {
        let dir = tempfile::tempdir().unwrap();
        let (port, seen) = record(proxy_challenge);
        let url = "http://github.example/connected.git";
        let repo = repository(dir.path(), url);
        git(
            &repo,
            &[
                "config",
                "http.proxy",
                &format!("http://proxy-user@127.0.0.1:{port}"),
            ],
        )
        .unwrap();

        let result = git_remote(&repo, &remote(url), &["ls-remote", "origin"]);

        assert!(result.is_err(), "{result:?}");
        assert_eq!(
            git(&repo, &["config", "--get-urlmatch", "http.proxy", url]).unwrap(),
            format!("http://proxy-user@127.0.0.1:{port}"),
            "control: Git routes the URL through the proxy"
        );
        let seen = seen.lock().unwrap().clone();
        assert!(
            seen.iter().all(|s| s.proxy_authorization.is_none()),
            "{seen:?}"
        );
    }

    /// A repository whose `origin` is the bare `connected`, with a submodule `sub` cloned from the
    /// bare `sub_origin` and a seed clone of `sub_origin`. File transport is allowed in both
    /// repositories, so a recursive transfer would reach `sub_origin` as it would an HTTPS one.
    fn with_submodule(dir: &Path) -> (PathBuf, String, String, PathBuf) {
        let connected = bare(&dir.join("connected.git"));
        let sub_origin = bare(&dir.join("sub.git"));
        std::fs::create_dir(dir.join("seed")).unwrap();
        let seed = repository(&dir.join("seed"), &sub_origin);
        git(&seed, &["push", "-q", "origin", "main"]).unwrap();
        let repo = repository(dir, &connected);
        git(
            &repo,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                "-q",
                &sub_origin,
                "sub",
            ],
        )
        .unwrap();
        commit(&repo, "add sub");
        for checkout in [&repo, &repo.join("sub")] {
            git(checkout, &["config", "protocol.file.allow", "always"]).unwrap();
        }
        (repo, connected, sub_origin, seed)
    }

    fn commit(dir: &Path, message: &str) {
        git(
            dir,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.com",
                "commit",
                "-q",
                "--allow-empty",
                "-am",
                message,
            ],
        )
        .unwrap();
    }

    // Behavior: the repository's config recurses fetches into submodules -> the factory's fetch
    //     leaves the submodule's remote untouched, which a plain fetch then reaches.
    #[test]
    fn a_fetch_never_recurses_into_a_submodule() {
        let dir = tempfile::tempdir().unwrap();
        let (repo, connected, _, seed) = with_submodule(dir.path());
        git(&seed, &["push", "-q", "origin", "HEAD:refs/heads/extra"]).unwrap();
        git(&repo, &["config", "fetch.recurseSubmodules", "true"]).unwrap();
        let extra = || {
            git(
                &repo.join("sub"),
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    "refs/remotes/origin/extra",
                ],
            )
        };

        let result = git_remote(&repo, &remote(&connected), &["fetch", "-q", "origin"]);

        assert!(result.is_ok(), "{result:?}");
        assert!(extra().is_err(), "the submodule's remote was fetched");
        git(&repo, &["fetch", "-q", "origin"]).unwrap();
        assert!(extra().is_ok(), "control: plain Git fetches the submodule");
    }

    // Behavior: the repository's config pushes submodules on demand -> the factory's push sends
    //     nothing to the submodule's remote, which a plain push then reaches.
    #[test]
    fn a_push_never_recurses_into_a_submodule() {
        let dir = tempfile::tempdir().unwrap();
        let (repo, connected, sub_origin, _) = with_submodule(dir.path());
        commit(&repo.join("sub"), "submodule change");
        let changed = git(&repo.join("sub"), &["rev-parse", "HEAD"]).unwrap();
        git(&repo, &["add", "sub"]).unwrap();
        commit(&repo, "point at the submodule change");
        git(&repo, &["config", "push.recurseSubmodules", "on-demand"]).unwrap();
        let reached = || git(Path::new(&sub_origin), &["cat-file", "-e", &changed]).is_ok();

        let result = git_remote(&repo, &remote(&connected), &PUSH);

        assert!(result.is_ok(), "{result:?}");
        assert!(refs(&connected).contains("refs/heads/feature"));
        assert!(!reached(), "the submodule's remote received the push");
        // Git skips the submodules of commits it already pushed, which the tracking ref records.
        git(&repo, &["update-ref", "-d", "refs/remotes/origin/feature"]).unwrap();
        git(&repo, &["push", "-q", "origin", "HEAD:refs/heads/other"]).unwrap();
        assert!(reached(), "control: plain Git pushes the submodule");
    }
}
