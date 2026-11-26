//! The product running from an issue's worktree: its own port and data directory, started on
//! demand with the repository's `[app]` commands, so issues in parallel never share a server.

use std::fs::{self, File, TryLockError};
use std::net::{TcpListener, TcpStream};
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use ovenspring_core::config::AppConfig;
use ovenspring_integrations::http::{Method, Request, Transport as _, Ureq};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("`app.{0}` is not set in .ovenspring/config.toml")]
    NotConfigured(&'static str),
    #[error("`app.setup` failed ({status}); its output ends with:\n{tail}")]
    Setup { status: ExitStatus, tail: String },
    #[error("the app exited before it was ready; its log ends with:\n{tail}")]
    Exited { tail: String },
    #[error("the app did not answer {probe} within {seconds}s; its log ends with:\n{tail}")]
    NotReady {
        probe: String,
        seconds: u64,
        tail: String,
    },
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// How long a start waits for a running app to answer after its setup: Playwright's default wait
/// for a web server. An app that exits ends the wait at once.
pub const READY_WAIT: Duration = Duration::from_secs(60);

/// How long a stopped app has to end before it is killed: `docker stop`'s default.
const STOP_GRACE: Duration = Duration::from_secs(10);

pub struct App<'a> {
    pub state: &'a Path,
    pub issue: &'a str,
    pub worktree: &'a Path,
    pub config: &'a AppConfig,
}

impl App<'_> {
    fn dir(&self) -> PathBuf {
        self.state.join("apps").join(self.issue)
    }

    pub fn data_dir(&self) -> PathBuf {
        self.dir().join("data")
    }

    pub fn log_path(&self) -> PathBuf {
        self.dir().join("app.log")
    }

    fn pid_path(&self) -> PathBuf {
        self.dir().join("app.pid")
    }

    fn lock_path(&self) -> PathBuf {
        self.dir().join("app.lock")
    }

    /// The issue's port, chosen from the ports free at that moment and kept until a start finds
    /// something other than this app answering on it.
    pub fn port(&self) -> std::io::Result<u16> {
        let file = self.dir().join("port");
        if let Some(port) = fs::read_to_string(&file)
            .ok()
            .and_then(|s| s.trim().parse().ok())
        {
            return Ok(port);
        }
        fs::create_dir_all(self.dir())?;
        let port = TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port();
        fs::write(&file, format!("{port}\n"))?;
        Ok(port)
    }

    fn new_port(&self) -> std::io::Result<u16> {
        fs::remove_file(self.dir().join("port"))?;
        self.port()
    }

    /// `PORT` and `OVENSPRING_DATA_DIR` for the app and for commands run against it.
    pub fn env(&self) -> std::io::Result<Vec<(String, String)>> {
        let data = self.data_dir();
        fs::create_dir_all(&data)?;
        Ok(vec![
            ("PORT".into(), self.port()?.to_string()),
            ("OVENSPRING_DATA_DIR".into(), data.display().to_string()),
        ])
    }

    fn shell(&self, script: &str) -> std::io::Result<Command> {
        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg(script)
            .current_dir(self.worktree)
            .envs(self.env()?);
        Ok(cmd)
    }

    /// Whether the app still runs: it holds the lock on `app.lock`, its standard input, until its
    /// last process ends. Not read from the pid, because an agent's sandbox refuses `ps` and signals.
    fn running(&self) -> std::io::Result<bool> {
        let lock = match File::open(self.lock_path()) {
            Ok(lock) => lock,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e),
        };
        match lock.try_lock_shared() {
            Ok(()) => Ok(false),
            Err(TryLockError::WouldBlock) => Ok(true),
            Err(TryLockError::Error(e)) => Err(e),
        }
    }

    /// The lock a new start hands to the app, or `None` when another start holds it first.
    fn claim(&self) -> std::io::Result<Option<File>> {
        let lock = File::create(self.lock_path())?;
        match lock.try_lock() {
            Ok(()) => Ok(Some(lock)),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(e)) => Err(e),
        }
    }

    /// Starts the app unless it already runs and answers, and waits until it does. Returns its
    /// port, which is another one when something other than this app answers on the kept port.
    pub fn start(&self, wait: Duration) -> Result<u16, AppError> {
        let start = self
            .config
            .start
            .as_deref()
            .ok_or(AppError::NotConfigured("start"))?;
        let mut port = self.port()?;
        let ours = self.running()?;
        if ours && self.ready(port) {
            return Ok(port);
        }
        while !ours && TcpStream::connect(("127.0.0.1", port)).is_ok() {
            port = self.new_port()?;
        }
        if !ours && let Some(lock) = self.claim()? {
            self.setup()?;
            let log = File::create(self.log_path())?;
            let spawned = self
                .shell(start)?
                .stdin(lock)
                .stdout(log.try_clone()?)
                .stderr(log)
                .process_group(0)
                .spawn()?;
            fs::write(self.pid_path(), format!("{}\n", spawned.id()))?;
        }
        let deadline = Instant::now() + wait;
        while Instant::now() < deadline {
            if self.ready(port) {
                return Ok(port);
            }
            if !self.running()? {
                return Err(AppError::Exited {
                    tail: tail(&self.log_path()),
                });
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        Err(AppError::NotReady {
            probe: self.probe(port),
            seconds: wait.as_secs(),
            tail: tail(&self.log_path()),
        })
    }

    fn setup(&self) -> Result<(), AppError> {
        let Some(setup) = self.config.setup.as_deref() else {
            return Ok(());
        };
        let out = self.shell(setup)?.stdin(Stdio::null()).output()?;
        if out.status.success() {
            return Ok(());
        }
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        Err(AppError::Setup {
            status: out.status,
            tail: last_lines(&text, 30),
        })
    }

    /// Stops the app and everything it started. Returns whether one was running.
    pub fn stop(&self) -> Result<bool, AppError> {
        if !self.running()? {
            let _ = fs::remove_file(self.pid_path());
            return Ok(false);
        }
        let pid = fs::read_to_string(self.pid_path())?;
        let pid = pid
            .trim()
            .parse()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        stop_group(pid);
        fs::remove_file(self.pid_path())?;
        Ok(true)
    }

    /// Runs `command` in the worktree with the app's environment, starting the app first when
    /// `app.start` is set.
    pub fn run(&self, command: &[String], wait: Duration) -> Result<ExitStatus, AppError> {
        if self.config.start.is_some() {
            self.start(wait)?;
        }
        let (program, args) = command
            .split_first()
            .ok_or_else(|| std::io::Error::other("no command given after `--`"))?;
        Ok(Command::new(program)
            .args(args)
            .current_dir(self.worktree)
            .envs(self.env()?)
            .status()?)
    }

    fn probe(&self, port: u16) -> String {
        self.config.ready.as_deref().map_or_else(
            || format!("127.0.0.1:{port}"),
            |r| r.replace("$PORT", &port.to_string()),
        )
    }

    fn ready(&self, port: u16) -> bool {
        match self.config.ready.as_deref() {
            Some(url) => http_ok(&url.replace("$PORT", &port.to_string())),
            None => TcpStream::connect(("127.0.0.1", port)).is_ok(),
        }
    }
}

/// Whether a GET of `url` answers 2xx or 3xx.
fn http_ok(url: &str) -> bool {
    let request = Request {
        method: Method::Get,
        url: url.to_owned(),
        headers: vec![],
        body: None,
        raw: None,
    };
    Ureq::new()
        .send(&request)
        .is_ok_and(|r| (200..400).contains(&r.status))
}

/// Whether `pid` runs. An exited child its parent has not reaped yet (a zombie) does not.
pub fn alive(pid: u32) -> bool {
    Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .stderr(Stdio::null())
        .output()
        .is_ok_and(|o| {
            let stat = String::from_utf8_lossy(&o.stdout);
            o.status.success() && !stat.trim().is_empty() && !stat.trim_start().starts_with('Z')
        })
}

/// Sends TERM to the process group led by `pid`, then KILL if it is still there after
/// [`STOP_GRACE`].
pub(crate) fn stop_group(pid: u32) {
    let group = format!("-{pid}");
    let _ = Command::new("kill")
        .args(["-TERM", "--", &group])
        .stderr(Stdio::null())
        .status();
    let deadline = Instant::now() + STOP_GRACE;
    while Instant::now() < deadline && alive(pid) {
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = Command::new("kill")
        .args(["-KILL", "--", &group])
        .stderr(Stdio::null())
        .status();
}

fn tail(path: &Path) -> String {
    last_lines(&fs::read_to_string(path).unwrap_or_default(), 30)
}

fn last_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines
        .get(lines.len().saturating_sub(n)..)
        .unwrap_or_default()
        .join("\n")
}
