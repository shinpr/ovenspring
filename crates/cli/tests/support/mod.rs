//! Shared setup for the integration and service tests: a temporary repository with a bare
//! `origin`, isolated HOME, state and cache directories, the built executable, and fake agents.

#![cfg_attr(
    test,
    allow(
        dead_code,
        reason = "Each test binary uses the part of the support it needs."
    )
)]

pub mod dashboard;
pub mod fake_github;
pub mod git_http;
pub mod http;
pub mod recording;
pub mod scripts;
pub mod slack_stub;

/// The tests' HTTP server: `python3 serve <port> <directory>`.
pub const SERVE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/serve");

/// The command that serves `dir`, a shell word such as `"$OVENSPRING_DATA_DIR"`, on `$PORT`.
pub fn serve(dir: &str) -> String {
    format!("exec python3 \"{SERVE}\" \"$PORT\" {dir}")
}

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;

pub fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

pub const CONFIG: &str = r#"
[tracker]
kind = "linear"
team = "ACME"
[slack]
channel = "C0123456789"
owners = ["U0123456789"]
"#;

pub struct Env {
    pub dir: tempfile::TempDir,
    pub repo: PathBuf,
    pub origin: PathBuf,
    pub home: PathBuf,
    pub fake_bin: PathBuf,
    pub records: PathBuf,
}

impl Env {
    /// `acme/storefront` with one commit on `main`, an `AGENTS.md`, and `origin` as a bare repository.
    pub fn new() -> Env {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let repo = root.join("storefront");
        let origin = root.join("origin.git");
        let home = root.join("home");
        std::fs::create_dir_all(&home).unwrap();
        git(
            &root,
            &[
                "init",
                "-q",
                "--bare",
                "-b",
                "main",
                origin.to_str().unwrap(),
            ],
        );
        git(&root, &["init", "-q", "-b", "main", repo.to_str().unwrap()]);
        // The repository's own git identity, which every commit the factory makes takes.
        git(&repo, &["config", "user.name", "Storefront Dev"]);
        git(&repo, &["config", "user.email", "dev@storefront.test"]);
        std::fs::write(
            repo.join("AGENTS.md"),
            "# storefront\n\nHarm order: exposure, stored data, integrations, daily use.\n",
        )
        .unwrap();
        commit_all(&repo, "Start the storefront");
        git(
            &repo,
            &["remote", "add", "origin", origin.to_str().unwrap()],
        );
        git(&repo, &["push", "-q", "-u", "origin", "main"]);
        Env {
            fake_bin: root.join("fake-bin"),
            records: root.join("records"),
            repo,
            origin,
            home,
            dir,
        }
    }

    /// Writes `.ovenspring/config.toml` with `extra` appended, and merges it. A role `extra` does
    /// not set runs on Claude Code, and without an `[app]` the product has nothing to run, so the
    /// configuration is the one setup leaves.
    pub fn configure(&self, extra: &str) {
        std::fs::create_dir_all(self.repo.join(".ovenspring/agreements")).unwrap();
        let mut text = format!("{CONFIG}\n{extra}");
        for role in ["lead", "executor", "reviewer", "analyzer"] {
            if !text.contains(&format!("[roles.{role}]")) {
                text.push_str("\n[roles.");
                text.push_str(role);
                text.push_str("]\ncli = \"claude\"\n");
            }
        }
        if !text.contains("[app]") {
            text.push_str("\n[app]\nnothing_to_run = true\n");
        }
        std::fs::write(self.repo.join(".ovenspring/config.toml"), text).unwrap();
        std::fs::write(
            self.repo.join(".ovenspring/agreements/common.md"),
            "# Working agreements\n- Name the precedent a design follows.\n",
        )
        .unwrap();
        commit_all(&self.repo, "Enable Oven Spring");
        git(&self.repo, &["push", "-q", "origin", "main"]);
    }

    /// Configures the repository with the supplied filter command.
    pub fn configure_filter(&self, command: &str) {
        self.configure("");
        self.set_tracker_filter(Some(command));
    }

    /// Replaces or removes `[tracker] filter` in the repository's configuration, serialized as a
    /// Settings save serializes it.
    pub fn set_tracker_filter(&self, command: Option<&str>) {
        let mut config = ovenspring_core::config::Config::read(&self.repo).unwrap();
        config.tracker.filter = command.map(str::to_owned);
        std::fs::write(
            self.repo.join(".ovenspring/config.toml"),
            config.to_toml().unwrap(),
        )
        .unwrap();
    }

    /// Writes the real shell script run by an integration test and returns its repository-relative command.
    pub fn write_filter_script(&self, script: &str) -> String {
        let path = self.filter_file("filter.sh");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, script).unwrap();
        "exec sh .ovenspring/filter.sh".into()
    }

    /// A file reserved for a filter test's script, gate or counter.
    pub fn filter_file(&self, name: &str) -> PathBuf {
        self.repo.join(".ovenspring").join(name)
    }

    pub fn state_base(&self) -> PathBuf {
        self.dir.path().canonicalize().unwrap().join("state")
    }

    pub fn state(&self) -> PathBuf {
        ovenspring_store::state_dir_in(&self.state_base(), &self.repo)
    }

    /// The isolated directory beside the state directory.
    pub fn isolated(&self) -> PathBuf {
        ovenspring_store::isolated_of(&self.state())
    }

    /// Installs the fake agent as `claude`, `codex` and `opencode`.
    pub fn install_fake_agents(&self) {
        std::fs::create_dir_all(&self.fake_bin).unwrap();
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/fake-agent");
        for name in ["claude", "codex", "opencode"] {
            let target = self.fake_bin.join(name);
            std::fs::copy(&source, &target).unwrap();
            let mut perms = std::fs::metadata(&target).unwrap().permissions();
            std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
            std::fs::set_permissions(&target, perms).unwrap();
        }
    }

    /// The built executable with this environment's HOME, state, cache and PATH.
    pub fn ovenspring(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_ovenspring"));
        let path = format!(
            "{}:{}",
            self.fake_bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        cmd.args(args)
            .current_dir(&self.repo)
            .env("HOME", &self.home)
            .env("OVENSPRING_STATE_HOME", self.state_base())
            .env("FAKE_AGENT_RECORD", &self.records)
            .env("PATH", path)
            .env_remove("CODEX_HOME");
        cmd
    }

    pub fn run(&self, args: &[&str]) -> Output {
        self.ovenspring(args).output().unwrap()
    }

    /// Every launch the fake agents recorded, oldest first.
    pub fn records(&self) -> Vec<serde_json::Value> {
        let Ok(entries) = std::fs::read_dir(&self.records) else {
            return Vec::new();
        };
        let mut files: Vec<PathBuf> = entries.map(|e| e.unwrap().path()).collect();
        files.sort();
        files
            .iter()
            .map(|f| serde_json::from_str(&std::fs::read_to_string(f).unwrap()).unwrap())
            .collect()
    }
}

/// A person commits `path` on `branch` from the main checkout and pushes it to `origin`.
pub fn push_as_person(env: &Env, branch: &str, path: &str, content: &str) -> String {
    git(&env.repo, &["fetch", "-q", "origin", branch]);
    git(&env.repo, &["checkout", "-q", "-B", "person", "FETCH_HEAD"]);
    std::fs::create_dir_all(env.repo.join(path).parent().unwrap()).unwrap();
    std::fs::write(env.repo.join(path), content).unwrap();
    git(&env.repo, &["add", path]);
    git(
        &env.repo,
        &[
            "-c",
            "user.name=dev",
            "-c",
            "user.email=dev@example.com",
            "commit",
            "-q",
            "-m",
            "A person's change",
        ],
    );
    git(
        &env.repo,
        &["push", "-q", "origin", &format!("HEAD:refs/heads/{branch}")],
    );
    git(&env.repo, &["rev-parse", "HEAD"])
}

pub fn commit_all(repo: &Path, message: &str) {
    git(repo, &["add", "-A"]);
    git(
        repo,
        &[
            "-c",
            "user.name=owner",
            "-c",
            "user.email=owner@example.com",
            "commit",
            "-q",
            "-m",
            message,
        ],
    );
}

/// Kills the child process when dropped, so a failed assertion never leaves a daemon running.
pub struct Running(pub std::process::Child);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = Command::new("kill")
            .args(["-TERM", &self.0.id().to_string()])
            .status();
        let _ = self.0.wait();
    }
}

/// An open item of `step` named `name`, for `issue` or for none, addressed to the step's role.
pub fn item(
    store: &ovenspring_store::Store,
    name: &str,
    step: ovenspring_core::step::Step,
    issue: Option<&str>,
) -> ovenspring_core::item::Item {
    ovenspring_core::item::Item {
        front: ovenspring_core::item::Front {
            step,
            to: step.role(),
            status: ovenspring_core::item::Status::Open,
            issue: issue.map(str::to_owned),
            sprint: None,
            round: None,
            unit: None,
            refs: vec![],
            out: store.output_path(name).display().to_string(),
            prepare: false,
            resume: None,
        },
        body: format!("Work on {step}.\n\n## Result\n"),
    }
}

/// Polls `check` every 100 ms until it is true or `secs` pass.
pub fn wait_until(secs: u64, mut check: impl FnMut() -> bool) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    while std::time::Instant::now() < deadline {
        if check() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    false
}

/// The daemon built in this process the way `ovenspring daemon` builds it, with the given
/// services, launching the fake agents and the built `ovenspring`.
pub fn daemon(env: &Env, services: ovenspring_daemon::Services) -> ovenspring_daemon::Daemon {
    daemon_timed(env, services, QUICK)
}

/// As [`daemon`], with the given waits.
pub fn daemon_timed(
    env: &Env,
    services: ovenspring_daemon::Services,
    timing: ovenspring_daemon::Timing,
) -> ovenspring_daemon::Daemon {
    let config = ovenspring_core::config::Config::load(&env.repo).unwrap();
    let store = ovenspring_store::Store::open(env.state()).unwrap();
    ovenspring_daemon::Daemon::new(ovenspring_daemon::Parts {
        paths: ovenspring_daemon::Paths {
            repo: env.repo.clone(),
            ovenspring: PathBuf::from(env!("CARGO_BIN_EXE_ovenspring")),
            agent_path: vec![env.fake_bin.clone()],
            home: env.home.clone(),
            codex_home: env.home.join(".codex"),
            dashboard: Some("http://127.0.0.1:7171".to_owned()),
        },
        config,
        store,
        codex_user: ovenspring_agents::backend::CodexUserConfig::default(),
        services,
        timing,
    })
}

/// Waits short enough for a test: GitHub and the tracker on every tick, a retry after a second,
/// and no wait for review bots unless a test sets one; no reminder within a test unless it sets one.
pub const QUICK: ovenspring_daemon::Timing = ovenspring_daemon::Timing {
    github_poll: std::time::Duration::ZERO,
    tracker_poll: std::time::Duration::ZERO,
    // Longer than any gated filter test waits, so only a test about the timeout reaches it.
    filter_timeout: std::time::Duration::from_secs(60),
    retry_wait: std::time::Duration::from_secs(1),
    bot_pending: std::time::Duration::ZERO,
    bot_absent: std::time::Duration::ZERO,
    remind: std::time::Duration::from_secs(60 * 60),
};

/// Services with the given GitHub and recording channel and tracker.
pub fn services(
    github: Arc<dyn ovenspring_integrations::github::GitHub>,
    channel: Arc<recording::RecordingChannel>,
    tracker: Arc<recording::RecordingTracker>,
) -> ovenspring_daemon::Services {
    ovenspring_daemon::Services {
        github: Some(github),
        channel,
        tracker: Some(linear(tracker)),
        secrets: Vec::new(),
    }
}

/// `tracker` connected as Linear, the tracker the test configuration names.
pub fn linear(tracker: Arc<recording::RecordingTracker>) -> ovenspring_daemon::ConnectedTracker {
    ovenspring_daemon::ConnectedTracker {
        kind: ovenspring_core::config::TrackerKind::Linear,
        client: tracker,
    }
}

impl Env {
    /// Writes the per-step script the fake agents follow.
    pub fn script_agents(&self, script: &serde_json::Value) {
        self.install_fake_agents();
        let root = self.fake_bin.parent().unwrap();
        // Moved in whole, since a test can change the script while agents read it.
        let draft = root.join("agent-script.json.new");
        std::fs::write(&draft, script.to_string()).unwrap();
        std::fs::rename(draft, root.join("agent-script.json")).unwrap();
    }

    /// A tracker issue as the tracker poll writes it.
    pub fn tracker_issue(&self, key: &str, title: &str, body: &str) {
        let dir = self.state().join("sources/issues");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{key}.md")),
            format!("# {key}: {title}\n\n{body}\n"),
        )
        .unwrap();
    }

    /// Adds `branch` at `main`'s head, here and on origin.
    pub fn add_branch(&self, branch: &str) {
        git(&self.repo, &["branch", branch, "main"]);
        git(&self.repo, &["push", "-q", "origin", branch]);
        git(&self.repo, &["fetch", "-q", "origin"]);
    }

    /// Every event logged, oldest first.
    pub fn events(&self) -> Vec<serde_json::Value> {
        std::fs::read_to_string(self.state().join("events.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }
}

/// Ticks the daemon until `done` holds, checking `invariant` after every tick. Panics with the
/// event log's tail when `secs` pass first.
pub fn run_until(
    d: &mut ovenspring_daemon::Daemon,
    secs: u64,
    mut invariant: impl FnMut(&ovenspring_daemon::Daemon),
    mut done: impl FnMut(&ovenspring_daemon::Daemon) -> bool,
) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    while std::time::Instant::now() < deadline {
        d.tick().unwrap();
        invariant(d);
        if done(d) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let events: Vec<String> = std::fs::read_to_string(d.store().root().join("events.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect();
    let tail: Vec<String> = events
        .iter()
        .rev()
        .take(25)
        .rev()
        .map(ToString::to_string)
        .collect();
    panic!("not done within {secs}s; last events:\n{}", tail.join("\n"));
}

/// Stops what the test's agents and apps left running, even when the test panics: every process
/// a hanging fake agent recorded, and every app started under the state directory.
impl Drop for Env {
    fn drop(&mut self) {
        let mut pids: Vec<String> = Vec::new();
        if let Some(root) = self.fake_bin.parent() {
            for e in std::fs::read_dir(root).into_iter().flatten().flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with("pids-")
                    && let Ok(v) = serde_json::from_str::<serde_json::Value>(
                        &std::fs::read_to_string(e.path()).unwrap_or_default(),
                    )
                {
                    pids.extend(
                        ["agent", "child"]
                            .iter()
                            .filter_map(|k| v[*k].as_u64())
                            .map(|p| p.to_string()),
                    );
                }
            }
        }
        // Every launch's process group, recorded at spawn, even if its agent has not yet
        // written anything.
        for line in std::fs::read_to_string(self.state().join("launches.jsonl"))
            .unwrap_or_default()
            .lines()
        {
            if let Some(pid) = serde_json::from_str::<serde_json::Value>(line)
                .ok()
                .and_then(|v| v["pid"].as_u64())
            {
                pids.push(format!("-{pid}"));
            }
        }
        for e in std::fs::read_dir(self.state().join("apps"))
            .into_iter()
            .flatten()
            .flatten()
        {
            if let Ok(pid) = std::fs::read_to_string(e.path().join("app.pid")) {
                pids.push(format!("-{}", pid.trim()));
            }
        }
        // `kill -KILL -- -1` reaches every process this user owns and `-0` the test's own group,
        // so a record a test wrote by hand never names either.
        for pid in pids.into_iter().filter(|p| {
            p.trim_start_matches('-')
                .parse::<u32>()
                .is_ok_and(|n| n > 1)
        }) {
            // A process the test already stopped is gone; that is not worth reporting.
            let _ = Command::new("kill")
                .args(["-KILL", "--", &pid])
                .stderr(std::process::Stdio::null())
                .status();
        }
    }
}

/// A release host that answers every request with `tag` as the latest release.
struct ReleaseHost(String);

impl ovenspring_integrations::http::Transport for ReleaseHost {
    fn send(
        &self,
        _: &ovenspring_integrations::http::Request,
    ) -> Result<ovenspring_integrations::http::Response, ovenspring_integrations::http::HttpError>
    {
        Ok(ovenspring_integrations::http::Response {
            status: 200,
            headers: Vec::new(),
            body: serde_json::json!({ "tag_name": self.0 }).to_string(),
        })
    }
}

/// Where a dashboard reads versions: `executable` as the installed Oven Spring, and `tag` as the
/// latest release.
pub fn versions(executable: PathBuf, tag: &str) -> ovenspring_server::version::Sources {
    ovenspring_server::version::Sources {
        executable,
        latest_release: "https://releases.invalid/latest".into(),
        transport: Arc::new(ReleaseHost(tag.to_owned())),
    }
}

/// Versions for a dashboard whose test is about something else: nothing installed, and the
/// running version as the latest release.
pub fn no_new_version() -> ovenspring_server::version::Sources {
    versions(
        PathBuf::from("/nonexistent/ovenspring"),
        concat!("v", env!("CARGO_PKG_VERSION")),
    )
}
