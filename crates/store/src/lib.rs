//! The per-repository state directory: the board, step outputs, logs and runtime records.
//!
//! It lives outside the repository, so worktrees, `git clean` and people's sessions
//! never see it.

#![cfg_attr(
    test,
    allow(
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used,
        clippy::expect_used,
        reason = "Test helpers use concise assertions; production paths remain strict."
    )
)]

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use ovenspring_core::item::{Item, Status};
use ovenspring_core::output::StepOutput;
use sha2::{Digest, Sha256};

pub mod credentials;

/// The environment variable that names the item a launch owns.
pub const LAUNCH_ENV: &str = "OVENSPRING_LAUNCH";

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("{0}: {1}")]
    Io(String, std::io::Error),
    #[error("{0}")]
    Refused(String),
    #[error("{path}: {message}")]
    Corrupt { path: String, message: String },
}

impl StoreError {
    /// The error of an I/O operation on `path`, for `map_err`.
    pub fn io(path: &Path) -> impl FnOnce(std::io::Error) -> StoreError + '_ {
        move |e| StoreError::Io(path.display().to_string(), e)
    }
}

/// The state directory for a repository: `<base>/ovenspring/repos/<name>-<hash>/state`, the part
/// every agent launch is given.
///
/// `<base>` is `$OVENSPRING_STATE_HOME` when set, else the platform's data or state directory.
pub fn state_dir_for(repo: &Path) -> PathBuf {
    state_dir_in(&state_base(), repo)
}

/// The state directory for `repo` under an explicit base.
pub fn state_dir_in(base: &Path, repo: &Path) -> PathBuf {
    repo_dir_in(base, repo).join("state")
}

/// The isolated directory for a repository, beside its state directory: what the factory trusts
/// or runs on the owner's word, given to no launch and denied by the launch hook.
pub fn isolated_dir_for(repo: &Path) -> PathBuf {
    isolated_of(&state_dir_for(repo))
}

/// The isolated directory beside the state directory `state`.
pub fn isolated_of(state: &Path) -> PathBuf {
    state.parent().unwrap_or(state).join("isolated")
}

fn repo_dir_in(base: &Path, repo: &Path) -> PathBuf {
    base.join("ovenspring").join("repos").join(repo_key(repo))
}

/// `<name>-<hash>`: the repository's directory name and a short hash of its canonical path, so
/// two checkouts of the same name keep apart.
pub fn repo_key(repo: &Path) -> String {
    let canonical = repo.canonicalize().unwrap_or_else(|_| repo.to_path_buf());
    let name = canonical
        .file_name()
        .map_or_else(|| "repo".into(), |n| n.to_string_lossy().into_owned());
    let hash = Sha256::digest(canonical.to_string_lossy().as_bytes());
    format!("{name}-{}", hex(hash.iter().take(6)))
}

fn state_base() -> PathBuf {
    if let Some(dir) = std::env::var_os("OVENSPRING_STATE_HOME") {
        return PathBuf::from(dir);
    }
    let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support")
    } else {
        std::env::var_os("XDG_STATE_HOME").map_or_else(|| home.join(".local/state"), PathBuf::from)
    }
}

/// Lower-case hexadecimal of the bytes.
pub fn hex<'a>(bytes: impl IntoIterator<Item = &'a u8>) -> String {
    use std::fmt::Write as _;
    bytes.into_iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

pub struct Store {
    root: PathBuf,
    isolated: PathBuf,
}

/// An agent's `ovenspring item set` call.
#[derive(Clone, Copy)]
pub struct SetRequest<'a> {
    pub path: &'a Path,
    pub assignments: &'a [(String, String)],
    /// The item name in the caller's `OVENSPRING_LAUNCH`.
    pub launch: Option<&'a str>,
}

impl Store {
    /// Opens the state directory, creating its layout on first use. `root` is a repository
    /// directory's `state`; the isolated directory is its sibling (`isolated_of`), created when
    /// its first record is written, so an agent's own `ovenspring` call never writes there.
    pub fn open(root: impl Into<PathBuf>) -> Result<Store, StoreError> {
        let root = root.into();
        let isolated = isolated_of(&root);
        for dir in [
            "board/active",
            "board/archive",
            "outputs",
            "launches",
            "worktrees",
            "apps",
            "issues",
            "sprints",
            "sources/issues",
            "sources/comments",
        ] {
            let p = root.join(dir);
            fs::create_dir_all(&p).map_err(StoreError::io(&p))?;
        }
        Ok(Store { root, isolated })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The isolated directory: decisions, approvals, recovery procedures, the dashboard's address.
    pub fn isolated(&self) -> &Path {
        &self.isolated
    }

    /// Writes `value` as JSON to `<isolated>/<rel>`, atomically.
    pub fn write_isolated(
        &self,
        rel: &str,
        value: &impl serde::Serialize,
    ) -> Result<PathBuf, StoreError> {
        write_json(&self.isolated.join(rel), value)
    }

    /// Reads `<isolated>/<rel>` as JSON; `None` when it does not exist.
    pub fn read_isolated<T: serde::de::DeserializeOwned>(
        &self,
        rel: &str,
    ) -> Result<Option<T>, StoreError> {
        read_json(&self.isolated.join(rel))
    }

    /// The names (without `.json`) of the records in `<isolated>/<dir>`, sorted; none before the
    /// first is written.
    pub fn isolated_names(&self, dir: &str) -> Result<Vec<String>, StoreError> {
        let path = self.isolated.join(dir);
        if !path.exists() {
            return Ok(Vec::new());
        }
        json_names(&path)
    }

    pub fn active_dir(&self) -> PathBuf {
        self.root.join("board/active")
    }

    /// Where finished items are kept.
    pub fn archive_dir(&self) -> PathBuf {
        self.root.join("board/archive")
    }

    /// The file whose presence means the factory is paused; it holds the reason.
    pub fn halt_file(&self) -> PathBuf {
        self.root.join("HALT")
    }

    /// The checkout named `name` (an issue's key, or another of the factory's own).
    pub fn worktree(&self, name: &str) -> PathBuf {
        self.root.join("worktrees").join(name)
    }

    /// `<state>/sources/<rel>`: what the factory gives agents to read.
    pub fn source(&self, rel: &str) -> PathBuf {
        self.root.join("sources").join(rel)
    }

    /// The item named `name`, on the board or in the archive.
    pub fn find_item(&self, name: &str) -> Option<PathBuf> {
        [self.archive_dir(), self.active_dir()]
            .into_iter()
            .map(|dir| dir.join(format!("{name}.md")))
            .find(|p| p.exists())
    }

    /// `base`, or `base` with `-2`, `-3` and on: a name no item on the board or in the archive has.
    pub fn unused_item_name(&self, base: &str) -> String {
        let taken = |n: &str| {
            [self.active_dir(), self.archive_dir()]
                .iter()
                .any(|dir| dir.join(format!("{n}.md")).exists())
        };
        let mut name = base.to_owned();
        let mut n = 1;
        while taken(&name) {
            n += 1;
            name = format!("{base}-{n}");
        }
        name
    }

    pub fn output_path(&self, item_name: &str) -> PathBuf {
        self.root.join("outputs").join(format!("{item_name}.json"))
    }

    /// Where an ended sprint's record is kept, relative to the state directory.
    pub fn sprint_record(id: &str) -> String {
        format!("sprints/{id}.json")
    }

    /// Writes `value` as JSON to `<state>/<rel>`, atomically.
    pub fn write_record(
        &self,
        rel: &str,
        value: &impl serde::Serialize,
    ) -> Result<PathBuf, StoreError> {
        write_json(&self.root.join(rel), value)
    }

    /// Replaces `<state>/<rel>` with `text`, atomically.
    pub fn write_text(&self, rel: &str, text: &str) -> Result<PathBuf, StoreError> {
        let path = self.root.join(rel);
        write_atomic(&path, text)?;
        Ok(path)
    }

    /// Reads `<state>/<rel>` as JSON; `None` when it does not exist.
    pub fn read_record<T: serde::de::DeserializeOwned>(
        &self,
        rel: &str,
    ) -> Result<Option<T>, StoreError> {
        read_json(&self.root.join(rel))
    }

    /// The names (without `.json`) of the records in `<state>/<dir>`, sorted.
    pub fn record_names(&self, dir: &str) -> Result<Vec<String>, StoreError> {
        json_names(&self.root.join(dir))
    }

    /// Writes a new item into `board/active/<name>.md`.
    pub fn file_item(&self, name: &str, item: &Item) -> Result<PathBuf, StoreError> {
        let path = self.active_dir().join(format!("{name}.md"));
        if path.exists() {
            return Err(StoreError::Refused(format!(
                "{} already exists",
                path.display()
            )));
        }
        write_atomic(&path, &item.render())?;
        Ok(path)
    }

    pub fn read_item(&self, path: &Path) -> Result<Item, StoreError> {
        let text = fs::read_to_string(path).map_err(StoreError::io(path))?;
        Item::parse(&text).map_err(|e| StoreError::Corrupt {
            path: path.display().to_string(),
            message: e.0,
        })
    }

    /// Rewrites an item's front matter, keeping its body as it is on disk.
    pub fn update_item(
        &self,
        path: &Path,
        change: impl FnOnce(&mut Item),
    ) -> Result<Item, StoreError> {
        let mut item = self.read_item(path)?;
        change(&mut item);
        write_atomic(path, &item.render())?;
        Ok(item)
    }

    pub fn archive_item(&self, path: &Path) -> Result<PathBuf, StoreError> {
        let to = self
            .archive_dir()
            .join(path.file_name().unwrap_or_default());
        fs::rename(path, &to).map_err(StoreError::io(path))?;
        Ok(to)
    }

    /// `ovenspring item set`: the only way an agent changes an item's front matter.
    ///
    /// Accepts `status=done|blocked`, only from the launch that owns the item, only while the item
    /// is `doing`. `done` requires an output that parses as its step's (`StepOutput::parse`).
    pub fn agent_set(&self, request: &SetRequest<'_>) -> Result<Item, StoreError> {
        let SetRequest {
            path,
            assignments,
            launch,
        } = *request;
        let refuse = |m: String| Err(StoreError::Refused(m));
        let name = item_name(path);
        if !path.starts_with(self.active_dir()) {
            return refuse(format!("{} is not an active item", path.display()));
        }
        if launch != Some(name.as_str()) {
            return refuse(format!(
                "{LAUNCH_ENV} does not name {name}; only the launch that owns an item may change it"
            ));
        }

        let current = self.read_item(path)?;
        if current.front.status != Status::Doing {
            return refuse(format!(
                "{name} is {}, not doing",
                current.front.status.as_str()
            ));
        }

        let mut status = None;
        for (key, value) in assignments {
            match key.as_str() {
                "status" => match value.parse::<Status>() {
                    Ok(s @ (Status::Done | Status::Blocked)) => status = Some(s),
                    _ => {
                        return refuse(format!(
                            "status may be set to done or blocked, not {value:?}"
                        ));
                    }
                },
                other => {
                    return refuse(format!("{other:?} cannot be set; only status"));
                }
            }
        }

        if status == Some(Status::Done) {
            let out = Path::new(&current.front.out);
            let text = fs::read_to_string(out).map_err(|_| {
                StoreError::Refused(format!(
                    "write the step's output to {} before setting done",
                    out.display()
                ))
            })?;
            let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
                StoreError::Refused(format!("{} is not valid JSON: {e}", out.display()))
            })?;
            let parsed = if current.front.prepare {
                ovenspring_core::output::PrepareOutput::parse(value).map(|_| ())
            } else {
                StepOutput::parse(current.front.step, value).map(|_| ())
            };
            parsed.map_err(|e| {
                StoreError::Refused(format!("{} is not a valid output: {e}", out.display()))
            })?;
        }

        self.update_item(path, |item| {
            if let Some(s) = status {
                item.front.status = s;
            }
        })
    }

    pub fn append_event(&self, event: &serde_json::Value) -> Result<(), StoreError> {
        self.append_line("events.jsonl", event)
    }

    pub fn append_launch(&self, line: &serde_json::Value) -> Result<(), StoreError> {
        self.append_line("launches.jsonl", line)
    }

    /// Appends `value` as one JSON line to `<state>/<file>`, creating the file when missing.
    pub fn append_line(&self, file: &str, value: &serde_json::Value) -> Result<(), StoreError> {
        let path = self.root.join(file);
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(StoreError::io(&path))?;
        let mut line = serde_json::to_string(value)
            .map_err(|e| StoreError::Io(path.display().to_string(), std::io::Error::other(e)))?;
        line.push('\n');
        f.write_all(line.as_bytes()).map_err(StoreError::io(&path))
    }
}

pub fn item_name(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<PathBuf, StoreError> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(StoreError::io(dir))?;
    }
    let text = serde_json::to_string_pretty(value).map_err(|e| StoreError::Corrupt {
        path: path.display().to_string(),
        message: e.to_string(),
    })?;
    write_atomic(path, &text)?;
    Ok(path.to_path_buf())
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>, StoreError> {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| StoreError::Corrupt {
                path: path.display().to_string(),
                message: e.to_string(),
            }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(StoreError::Io(path.display().to_string(), e)),
    }
}

fn json_names(path: &Path) -> Result<Vec<String>, StoreError> {
    let mut names: Vec<String> = fs::read_dir(path)
        .map_err(StoreError::io(path))?
        .filter_map(Result::ok)
        .filter_map(|e| {
            let p = e.path();
            (p.extension()? == "json")
                .then(|| p.file_stem().map(|s| s.to_string_lossy().into_owned()))?
        })
        .collect();
    names.sort();
    Ok(names)
}

fn write_atomic(path: &Path, text: &str) -> Result<(), StoreError> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, text).map_err(StoreError::io(&tmp))?;
    fs::rename(&tmp, path).map_err(StoreError::io(path))
}
