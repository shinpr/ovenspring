//! `ovenspring start | stop`, and `ovenspring daemon`, the resident process itself.

use std::fs::{self, File};
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use ovenspring_agents::backend;
use ovenspring_core::config::{CONFIG_FILE, Config, TrackerKind};
use ovenspring_daemon::{Daemon, Parts, Paths, Services, Timing};
use ovenspring_integrations::channel::slack::Slack;
use ovenspring_integrations::channel::{Channel, Unconnected};
use ovenspring_integrations::github::{GitHub, GitHubApi};
use ovenspring_integrations::http::Ureq;
use ovenspring_integrations::tracker::Tracker;
use ovenspring_integrations::tracker::linear::Linear;
use ovenspring_store::credentials::{Credentials, credentials_path};
use ovenspring_store::{Store, isolated_dir_for, state_dir_for};

fn pid_file(repo: &Path) -> PathBuf {
    state_dir_for(repo).join("daemon.pid")
}

/// The lock a running daemon holds for as long as it runs; the operating system releases it when
/// the process ends, however it ends.
fn lock_file(repo: &Path) -> PathBuf {
    state_dir_for(repo).join("daemon.lock")
}

/// The running daemon's pid: there is one while its lock is held. A pid file left by a daemon
/// that ended (a crash, a reboot) names no daemon, whatever process now has that pid.
fn running_pid(repo: &Path) -> Option<u32> {
    let lock = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(lock_file(repo))
        .ok()?;
    if lock.try_lock().is_ok() {
        return None;
    }
    fs::read_to_string(pid_file(repo)).ok()?.trim().parse().ok()
}

/// Where a daemon records how to open its dashboard: `{pid, url}`, in the isolated directory. It
/// outlives the daemon, so the next one serves on the same port and the owner's address keeps
/// working.
const DASHBOARD_RECORD: &str = "dashboard.json";

fn dashboard_file(repo: &Path) -> PathBuf {
    isolated_dir_for(repo).join(DASHBOARD_RECORD)
}

/// The port the last dashboard of this repository served on, or 0 for none.
fn last_port(repo: &Path) -> u16 {
    fs::read_to_string(dashboard_file(repo))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|v| {
            let host = v
                .get("url")?
                .as_str()?
                .strip_prefix("http://")?
                .split('/')
                .next()?;
            host.parse::<std::net::SocketAddr>().ok().map(|a| a.port())
        })
        .unwrap_or(0)
}

/// The dashboard address the daemon with `pid` recorded, once it has.
fn dashboard_url(repo: &Path, pid: u32) -> Option<String> {
    let text = fs::read_to_string(dashboard_file(repo)).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    (v.get("pid")?.as_u64()? == u64::from(pid))
        .then(|| v.get("url")?.as_str().map(str::to_owned))
        .flatten()
}

/// Opens `url` in the owner's browser; where none can be opened (a remote shell), the message
/// gives the address instead.
fn open_dashboard(url: &str) -> String {
    let program = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let opened = Command::new(program)
        .arg(url)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if opened {
        "The dashboard is open in your browser.".into()
    } else {
        format!("Open the dashboard at {url}")
    }
}

pub fn start(repo: &Path) -> Result<String> {
    if let Some(pid) = running_pid(repo) {
        let url = dashboard_url(repo, pid).with_context(|| {
            format!("Oven Spring runs (pid {pid}) but has no dashboard address")
        })?;
        return Ok(format!(
            "Oven Spring is already running (pid {pid}). {}",
            open_dashboard(&url)
        ));
    }
    let state = state_dir_for(repo);
    fs::create_dir_all(&state).with_context(|| format!("creating {}", state.display()))?;
    let log = File::options()
        .create(true)
        .append(true)
        .open(state.join("daemon.log"))?;
    let exe = std::env::current_exe()?;
    let mut child = Command::new(exe)
        .arg("daemon")
        .current_dir(repo)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        .process_group(0)
        .spawn()
        .context("starting the daemon")?;
    let pid = child.id();
    // The daemon either serves its dashboard or exits.
    let url = loop {
        if let Some(url) = dashboard_url(repo, pid) {
            break url;
        }
        if child.try_wait()?.is_some() {
            bail!(
                "Oven Spring did not start; see {}",
                state.join("daemon.log").display()
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    Ok(format!(
        "Oven Spring started (pid {pid}). {}",
        open_dashboard(&url)
    ))
}

pub fn stop(repo: &Path) -> Result<String> {
    let Some(pid) = running_pid(repo) else {
        return Ok("Oven Spring is not running".into());
    };
    let ok = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()?
        .success();
    if !ok {
        bail!("could not stop pid {pid}");
    }
    // The daemon finishes its current pass first; wait for it so `start` can follow at once.
    while running_pid(repo).is_some() {
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(format!("Oven Spring stopped (pid {pid})"))
}

/// The resident loop, in the foreground: the dashboard at once, and the factory once the
/// repository has a valid configuration (until then the dashboard opens on setup).
pub fn run(repo: &Path) -> Result<()> {
    let state = state_dir_for(repo);
    let store = Store::open(&state)?;
    let lock = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(lock_file(repo))?;
    if lock.try_lock().is_err() {
        bail!("Oven Spring already runs in {}", repo.display());
    }
    fs::write(pid_file(repo), format!("{}\n", std::process::id()))?;
    let credentials = credentials_path(repo);
    let (home, codex_home) = homes()?;
    // Read once, at start: on Linux, once the install script has put a new file at this path, the
    // running executable's own path reads as deleted.
    let executable = std::env::current_exe()?;
    let server = ovenspring_server::start(
        ovenspring_server::Context {
            repo: repo.to_path_buf(),
            store: Store::open(&state)?,
            credentials: credentials.clone(),
            checks: Arc::new(crate::setup_checks::Checks),
            home: home.clone(),
            codex_home: codex_home.clone(),
            version: ovenspring_server::version::Sources {
                executable: executable.clone(),
                latest_release: ovenspring_server::version::LATEST_RELEASE.to_owned(),
                transport: Arc::new(Ureq::new()),
            },
        },
        last_port(repo),
    )
    .context("starting the dashboard")?;
    let record = serde_json::json!({"pid": std::process::id(), "url": server.url()});
    store
        .write_isolated(DASHBOARD_RECORD, &record)
        .context("recording the dashboard address")?;
    eprintln!("ovenspring: dashboard on http://{}", server.addr);
    let paths = Paths {
        repo: repo.to_path_buf(),
        ovenspring: executable,
        agent_path: Vec::new(),
        home,
        codex_home,
        dashboard: Some(format!("http://{}", server.addr)),
    };

    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    let mut daemon: Option<Daemon> = None;
    let mut refused = String::new();
    let mut connections = String::new();
    while !stop.load(Ordering::Relaxed) {
        match daemon.as_mut() {
            Some(d) => {
                // A connection or its credentials changed from the dashboard: the services are
                // rebuilt, so the next poll and launch use them.
                let now = connection_key(d.config(), &credentials);
                if now != connections {
                    match ovenspring_store::credentials::read(&credentials) {
                        Ok(held) => {
                            let services = services_for(repo, d.store(), d.config(), &held);
                            d.replace_services(services);
                        }
                        Err(e) => eprintln!("ovenspring: {e}"),
                    }
                    connections = now;
                }
                if let Err(e) = d.tick() {
                    eprintln!("ovenspring: {e}");
                }
            }
            None if repo.join(CONFIG_FILE).exists() => {
                match factory(&paths, &store, &credentials) {
                    Ok(d) => {
                        connections = connection_key(d.config(), &credentials);
                        daemon = Some(d);
                    }
                    // Shown on the dashboard's setup screen; logged once per change.
                    Err(e) if e.to_string() != refused => {
                        refused = e.to_string();
                        eprintln!("ovenspring: no work starts: {refused}");
                    }
                    Err(_) => {}
                }
            }
            None => {}
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    if let Some(d) = daemon.as_mut() {
        d.stop_launches();
    }
    let _ = fs::remove_file(pid_file(repo));
    drop(lock);
    Ok(())
}

/// The owner's home and Codex home.
fn homes() -> Result<(PathBuf, PathBuf)> {
    let home = PathBuf::from(std::env::var("HOME").context("HOME is not set")?);
    let codex_home =
        std::env::var_os("CODEX_HOME").map_or_else(|| home.join(".codex"), PathBuf::from);
    Ok((home, codex_home))
}

/// The factory over a valid configuration, with the credentials setup stored.
fn factory(paths: &Paths, store: &Store, credentials: &Path) -> Result<Daemon> {
    let config = Config::load(&paths.repo)?;
    let held = ovenspring_store::credentials::read(credentials)?;
    let codex_user = backend::read_codex_user_config(&paths.codex_home, &paths.home);
    let services = services_for(&paths.repo, store, &config, &held);
    let mut daemon = Daemon::new(Parts {
        paths: paths.clone(),
        config,
        store: Store::open(store.root())?,
        codex_user,
        services,
        timing: Timing::default(),
    });
    daemon.recover()?;
    Ok(daemon)
}

/// What the services are built from: the tracker, the Slack channel and the credentials file.
fn connection_key(config: &Config, credentials: &Path) -> String {
    format!(
        "{:?} {} {}",
        config.tracker,
        config.slack.channel,
        fs::read_to_string(credentials).unwrap_or_default()
    )
}

fn services_for(repo: &Path, store: &Store, config: &Config, held: &Credentials) -> Services {
    let (github, gh_token) = github_api(repo, held);
    let mut secrets: Vec<String> = held.values().into_iter().map(str::to_owned).collect();
    secrets.extend(gh_token);
    Services {
        github: github.clone().map(|g| g as Arc<dyn GitHub>),
        channel: channel_for(
            config,
            held,
            ovenspring_daemon::records::last_activity(store),
        ),
        tracker: tracker_client(config, held, github),
        secrets,
    }
}

/// The GitHub API for `origin`, with the token setup stored or, when none was entered, the token
/// of the signed-in `gh`.
fn github_api(repo: &Path, held: &Credentials) -> (Option<Arc<GitHubApi<Ureq>>>, Option<String>) {
    let Some(id) = crate::setup_checks::repo_id(repo) else {
        return (None, None);
    };
    let using_gh_token = held.github.token.is_none();
    let Some(token) = held
        .github
        .token
        .clone()
        .or_else(crate::setup_checks::gh_token)
    else {
        return (None, None);
    };
    let secret = using_gh_token.then(|| token.clone());
    (
        Some(Arc::new(GitHubApi::new(Ureq::new(), id, token))),
        secret,
    )
}

/// The configured tracker: Linear with its stored key, or the repository's GitHub Issues through
/// the GitHub token.
fn tracker_client(
    config: &Config,
    held: &Credentials,
    github: Option<Arc<GitHubApi<Ureq>>>,
) -> Option<Arc<dyn Tracker>> {
    match config.tracker.kind {
        TrackerKind::Linear => {
            let token = held.linear.api_key.clone()?;
            let team = config.tracker.team.clone()?;
            Some(Arc::new(Linear::new(Ureq::new(), token, team)))
        }
        TrackerKind::Github => github.map(|g| g as Arc<dyn Tracker>),
    }
}

/// Slack with the stored tokens; without them, decisions wait, recorded as unsent, and the
/// dashboard names the missing connection.
fn channel_for(
    config: &Config,
    held: &Credentials,
    last_activity: Option<i64>,
) -> Arc<dyn Channel> {
    let (Some(bot), Some(app)) = (held.slack.bot_token.clone(), held.slack.app_token.clone())
    else {
        return Arc::new(Unconnected);
    };
    let slack = Slack::new(Ureq::new(), bot, config.slack.channel.clone());
    if let Some(t) = last_activity {
        slack.catch_up_from(t);
    }
    slack.listen(Arc::new(Ureq::new()), app);
    Arc::new(slack)
}
