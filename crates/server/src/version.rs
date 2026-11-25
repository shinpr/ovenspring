//! Which Oven Spring the dashboard's daemon runs, and the newer one the owner can move to: one
//! already installed that waits for a restart, or one released and not yet installed.

use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use ovenspring_integrations::http::{self, Method, Request, Transport};
use semver::Version;
use serde::Serialize;
use ts_rs::TS;

use crate::Shared;

/// GitHub's latest published release of Oven Spring: what the install script installs, without
/// drafts and prereleases.
pub const LATEST_RELEASE: &str = "https://api.github.com/repos/shinpr/ovenspring/releases/latest";

/// How often the latest release is read. Releases come days apart, and the daemon reads it again
/// whenever it starts. Assumed value: no requirement sets it.
const LATEST_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// Where the versions are read from.
pub struct Sources {
    /// The executable the daemon started from. The install script replaces the file at this path,
    /// so what is there now is what the next start runs.
    pub executable: PathBuf,
    /// Where the latest release is read: [`LATEST_RELEASE`] outside tests.
    pub latest_release: String,
    pub transport: Arc<dyn Transport>,
}

/// What the dashboard's header shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct VersionState {
    /// The version this daemon runs.
    pub running: String,
    pub update: Option<Update>,
}

/// The newer version the owner can move to, and what moving takes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Update {
    /// Installed already: the next start runs it.
    Restart { version: String },
    /// Released and not installed.
    Available { version: String },
}

/// The file at the executable's path, told apart from any later file put there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileIdentity {
    device: u64,
    inode: u64,
    modified: (i64, i64),
}

fn identity(path: &Path) -> Option<FileIdentity> {
    let m = std::fs::metadata(path).ok()?;
    Some(FileIdentity {
        device: m.dev(),
        inode: m.ino(),
        modified: (m.mtime(), m.mtime_nsec()),
    })
}

/// What is known so far.
#[derive(Default)]
pub(crate) struct Known {
    /// The executable at the path, and its version when it says one.
    installed: Mutex<Option<(FileIdentity, Option<Version>)>>,
    latest: Mutex<Option<Version>>,
}

impl Known {
    /// The version installed at `executable` now. It is read again only when another file has been
    /// put there, so a dashboard asking every few seconds starts no process.
    fn installed(&self, executable: &Path) -> Option<Version> {
        let now = identity(executable)?;
        let mut installed = self
            .installed
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        match installed.as_ref() {
            Some((seen, version)) if *seen == now => version.clone(),
            _ => {
                let version = version_of(executable);
                *installed = Some((now, version.clone()));
                version
            }
        }
    }

    pub(crate) fn state(&self, sources: &Sources) -> VersionState {
        let running = running();
        let installed = self.installed(&sources.executable);
        let latest = self
            .latest
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        VersionState {
            running: running.to_string(),
            update: update(&running, installed.as_ref(), latest.as_ref()),
        }
    }
}

fn running() -> Version {
    // Cargo accepts only SemVer package versions, so this parses.
    Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or_else(|_| Version::new(0, 0, 0))
}

/// What the owner can move to. An installed version other than the running one comes first, since
/// a restart is all it takes; otherwise a release newer than the running version.
fn update(
    running: &Version,
    installed: Option<&Version>,
    latest: Option<&Version>,
) -> Option<Update> {
    if let Some(installed) = installed.filter(|v| *v != running) {
        return Some(Update::Restart {
            version: installed.to_string(),
        });
    }
    latest.filter(|v| *v > running).map(|v| Update::Available {
        version: v.to_string(),
    })
}

/// The version `ovenspring --version` prints (`ovenspring 0.2.0`), or none when the file there
/// does not run or says something else.
fn version_of(executable: &Path) -> Option<Version> {
    let output = match Command::new(executable).arg("--version").output() {
        Ok(o) if o.status.success() => o,
        Ok(o) => {
            eprintln!(
                "ovenspring: {} --version exited with {}",
                executable.display(),
                o.status
            );
            return None;
        }
        Err(e) => {
            eprintln!(
                "ovenspring: running {} --version: {e}",
                executable.display()
            );
            return None;
        }
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let version = text
        .trim()
        .strip_prefix("ovenspring ")
        .and_then(|v| Version::parse(v).ok());
    if version.is_none() {
        eprintln!(
            "ovenspring: {} --version printed no version: {}",
            executable.display(),
            text.trim()
        );
    }
    version
}

/// The latest release's version, from its tag (`v0.2.0`).
fn latest_release(sources: &Sources) -> Result<Version, String> {
    let request = Request {
        method: Method::Get,
        url: sources.latest_release.clone(),
        headers: vec![("accept".into(), "application/vnd.github+json".into())],
        body: None,
        raw: None,
    };
    let response = sources
        .transport
        .send(&request)
        .and_then(|r| http::check(&request, r))
        .map_err(|e| e.to_string())?;
    let release: serde_json::Value =
        serde_json::from_str(&response.body).map_err(|e| e.to_string())?;
    let tag = release
        .get("tag_name")
        .and_then(serde_json::Value::as_str)
        .ok_or("the release names no tag")?;
    Version::parse(tag.strip_prefix('v').unwrap_or(tag)).map_err(|e| format!("tag {tag}: {e}"))
}

/// Reads the latest release now and then once a day, from a thread of its own, for as long as the
/// daemon runs. A failed read keeps what was read before and is tried again the next day.
pub(crate) fn watch_releases(shared: Arc<Shared>) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("releases".into())
        .spawn(move || {
            loop {
                match latest_release(&shared.context.version) {
                    Ok(v) => {
                        *shared
                            .versions
                            .latest
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner) = Some(v);
                    }
                    Err(e) => eprintln!("ovenspring: reading the latest release: {e}"),
                }
                std::thread::sleep(LATEST_EVERY);
            }
        })
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    // The installed version decides first; a release counts only when it is newer than the one
    // running, prereleases ordered as SemVer orders them.
    #[test]
    fn the_update_is_the_installed_version_first_then_a_newer_release() {
        let running = v("0.2.0");
        let cases = [
            ("same everywhere", Some("0.2.0"), Some("0.2.0"), None),
            ("no release read", Some("0.2.0"), None, None),
            ("older release", Some("0.2.0"), Some("0.1.0"), None),
            (
                "newer release",
                Some("0.2.0"),
                Some("0.3.0"),
                Some(Update::Available {
                    version: "0.3.0".into(),
                }),
            ),
            (
                "an earlier prerelease",
                Some("0.2.0"),
                Some("0.2.0-rc.1"),
                None,
            ),
            (
                "newer installed",
                Some("0.3.0"),
                Some("0.3.0"),
                Some(Update::Restart {
                    version: "0.3.0".into(),
                }),
            ),
            (
                "installed comes before a newer release",
                Some("0.3.0"),
                Some("0.4.0"),
                Some(Update::Restart {
                    version: "0.3.0".into(),
                }),
            ),
            (
                "older installed is still what the next start runs",
                Some("0.1.0"),
                None,
                Some(Update::Restart {
                    version: "0.1.0".into(),
                }),
            ),
            (
                "unreadable installed",
                None,
                Some("0.3.0"),
                Some(Update::Available {
                    version: "0.3.0".into(),
                }),
            ),
        ];
        for (case, installed, latest, expected) in cases {
            let installed = installed.map(v);
            let latest = latest.map(v);
            assert_eq!(
                update(&running, installed.as_ref(), latest.as_ref()),
                expected,
                "{case}"
            );
        }
        assert_eq!(
            update(&v("0.2.0-rc.1"), Some(&v("0.2.0-rc.1")), Some(&v("0.2.0"))),
            Some(Update::Available {
                version: "0.2.0".into()
            }),
            "a release after the running prerelease"
        );
    }
}
