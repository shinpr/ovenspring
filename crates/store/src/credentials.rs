//! The credentials the factory holds for a repository, kept outside it in
//! `<config>/ovenspring/credentials/<name>-<hash>.toml` with mode 0600, where `<config>`
//! is `$XDG_CONFIG_HOME`, else `~/.config`.

use std::fs;
use std::io::Write as _;
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{StoreError, repo_key};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Credentials {
    #[serde(default, skip_serializing_if = "Slack::is_empty")]
    pub slack: Slack,
    #[serde(default, skip_serializing_if = "Linear::is_empty")]
    pub linear: Linear,
    #[serde(default, skip_serializing_if = "GitHub::is_empty")]
    pub github: GitHub,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Slack {
    /// `xoxb-`: the Web API.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot_token: Option<String>,
    /// `xapp-` with `connections:write`: Socket Mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_token: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Linear {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitHub {
    /// Absent when the factory uses the token of the signed-in `gh`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

impl Slack {
    fn is_empty(&self) -> bool {
        self.bot_token.is_none() && self.app_token.is_none()
    }
}

impl Linear {
    fn is_empty(&self) -> bool {
        self.api_key.is_none()
    }
}

impl GitHub {
    fn is_empty(&self) -> bool {
        self.token.is_none()
    }
}

impl Credentials {
    /// Every value held, for masking.
    pub fn values(&self) -> Vec<&str> {
        [
            &self.slack.bot_token,
            &self.slack.app_token,
            &self.linear.api_key,
            &self.github.token,
        ]
        .into_iter()
        .flatten()
        .map(String::as_str)
        .collect()
    }
}

/// `<config>/ovenspring/credentials/<name>-<hash>.toml` for `repo`.
pub fn credentials_path(repo: &Path) -> PathBuf {
    config_base()
        .join("ovenspring")
        .join("credentials")
        .join(format!("{}.toml", repo_key(repo)))
}

fn config_base() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(dir);
    }
    std::env::var_os("HOME")
        .map_or_else(|| PathBuf::from("."), PathBuf::from)
        .join(".config")
}

/// The credentials at `path`; none when the file does not exist yet.
pub fn read(path: &Path) -> Result<Credentials, StoreError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Credentials::default()),
        Err(e) => return Err(StoreError::Io(path.display().to_string(), e)),
    };
    toml::from_str(&text).map_err(|e| StoreError::Corrupt {
        path: path.display().to_string(),
        message: e.message().to_owned(),
    })
}

/// Replaces the file with `credentials`, readable and writable by the owner only; the directory
/// is the owner's only too.
pub fn write(path: &Path, credentials: &Credentials) -> Result<(), StoreError> {
    let io = |e| StoreError::Io(path.display().to_string(), e);
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir).map_err(io)?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).map_err(io)?;
    let text = toml::to_string(credentials).map_err(|e| StoreError::Corrupt {
        path: path.display().to_string(),
        message: e.to_string(),
    })?;
    let staging = path.with_extension("toml.new");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&staging)
        .map_err(io)?;
    file.write_all(text.as_bytes()).map_err(io)?;
    file.sync_all().map_err(io)?;
    fs::rename(&staging, path).map_err(io)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_are_written_for_the_owner_only_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir
            .path()
            .join("ovenspring/credentials/storefront-abc.toml");
        assert_eq!(
            read(&path).unwrap(),
            Credentials::default(),
            "none before the first save"
        );
        let held = Credentials {
            slack: Slack {
                bot_token: Some("xoxb-1".into()),
                app_token: Some("xapp-1".into()),
            },
            ..Credentials::default()
        };

        write(&path, &held).unwrap();

        assert_eq!(read(&path).unwrap(), held);
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(path.parent().unwrap()), 0o700);
        assert_eq!(held.values(), ["xoxb-1", "xapp-1"]);
    }
}
