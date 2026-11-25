//! What the factory needs before it can start work, as the daemon last found it: its connections
//! to GitHub, the tracker and Slack. Each missing one says what is missing and how to fix it; the
//! dashboard shows them and the daemon starts no work that needs them. Agent CLIs are the owner's
//! own tools: one that cannot run fails its launch, which pauses the factory with its error.

use std::time::Instant;

use ovenspring_core::config::TrackerKind;
use ovenspring_store::Store;
use serde::{Deserialize, Serialize};
use serde_json::json;
use ts_rs::TS;

use crate::{Daemon, DaemonError};

pub(crate) const FILE: &str = "prerequisites.json";
const MENTION: &str = "mention.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Missing {
    /// What is missing, as one sentence.
    pub what: String,
    /// How to fix it, as one sentence.
    pub fix: String,
    /// The settings section that fixes it, when a setting does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setting: Option<String>,
}

/// The missing prerequisites the daemon last found; none before its first look.
pub fn missing(store: &Store) -> Result<Vec<Missing>, DaemonError> {
    Ok(store.read_record(FILE)?.unwrap_or_default())
}

/// What the owner types to mention the factory in Slack, without the `@`: the bot's name as Slack
/// last said it, or the name the app manifest gives it.
pub fn mention(store: &Store) -> Result<String, DaemonError> {
    Ok(store
        .read_record(MENTION)?
        .unwrap_or_else(|| ovenspring_integrations::channel::slack::APP_NAME.to_owned()))
}

impl Daemon {
    /// Looks at what work needs, at the GitHub poll's pace, and records what is missing when that
    /// changes.
    pub(crate) fn check_prerequisites(&mut self) -> Result<(), DaemonError> {
        if self
            .last_prerequisites
            .is_some_and(|t| t.elapsed() < self.timing.github_poll)
        {
            return Ok(());
        }
        self.last_prerequisites = Some(Instant::now());
        let mut missing = Vec::new();
        if self.services.github.is_none() {
            missing.push(Missing {
                what: "GitHub is not connected.".into(),
                fix: "No issue starts until it is. Sign in with `gh auth login`, or enter a token in Settings.".into(),
                setting: Some("general".into()),
            });
        }
        let tracker = match self.config.tracker.kind {
            TrackerKind::Linear => "Linear",
            TrackerKind::Github => "GitHub Issues",
        };
        if self.services.tracker.is_none() {
            missing.push(Missing {
                what: format!("{tracker} is not connected."),
                fix: "No new issue is read and no sprint is proposed until it is. Enter its key in Settings.".into(),
                setting: Some("tracker".into()),
            });
        } else if let Some(e) = &self.tracker_error {
            missing.push(Missing {
                what: format!("{tracker} refused the connection."),
                fix: format!(
                    "Its key may have been revoked ({e}). No new issue is read and no sprint is proposed until it is fixed."
                ),
                setting: Some("tracker".into()),
            });
        }
        if !self.services.channel.connected() {
            missing.push(Missing {
                what: "Slack is not connected.".into(),
                fix: "Decisions wait, unsent, until it is. Enter the Slack tokens in Settings."
                    .into(),
                setting: Some("slack".into()),
            });
        } else if let Some(e) = self.services.channel.trouble() {
            missing.push(Missing {
                what: "Your answers and presses in Slack do not reach the factory.".into(),
                fix: format!(
                    "Decisions you answer there wait until they do. Check the app-level token and that Socket Mode is on in the Slack app ({e})."
                ),
                setting: Some("slack".into()),
            });
        }
        if let Some(name) = self.services.channel.mention()
            && self.store.read_record::<String>(MENTION)?.as_ref() != Some(&name)
        {
            self.store.write_record(MENTION, &name)?;
        }
        if crate::prerequisites::missing(&self.store)? != missing {
            self.store.write_record(FILE, &missing)?;
            let what: Vec<&str> = missing.iter().map(|m| m.what.as_str()).collect();
            self.event(&json!({"event": "prerequisites", "missing": what}))?;
        }
        Ok(())
    }
}
