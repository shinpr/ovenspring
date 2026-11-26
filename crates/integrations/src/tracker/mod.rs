//! The issue tracker (Linear or GitHub Issues) behind one trait: open issues with their
//! dependencies for the sprint plan, and the lead's comments and state changes.

use std::fmt::Write as _;

use crate::http::HttpError;

pub mod github;
pub mod linear;

/// An open issue as the tracker shows it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrackerIssue {
    /// `ACME-123` (Linear) or `GH-12` (GitHub Issues): also the branch's name.
    pub key: String,
    pub title: String,
    pub author: String,
    pub body: String,
    pub url: String,
    pub state: String,
    /// Someone is already working on it (Linear's `started` states); the factory takes such an
    /// issue only when it is the one that started it.
    pub started: bool,
    pub labels: Vec<String>,
    pub comments: Vec<TrackerComment>,
    /// Keys of the open issues that block this one.
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackerComment {
    pub author: String,
    pub body: String,
}

impl TrackerIssue {
    /// The snapshot agents read at `sources/issues/<key>.md`: the issue as its requester wrote it.
    pub fn render(&self) -> String {
        let mut out = format!(
            "# {}: {}\n\n- URL: {}\n- State: {}\n",
            self.key, self.title, self.url, self.state
        );
        if !self.labels.is_empty() {
            let _ = writeln!(out, "- Labels: {}", self.labels.join(", "));
        }
        if !self.depends_on.is_empty() {
            let _ = writeln!(out, "- Blocked by: {}", self.depends_on.join(", "));
        }
        let _ = write!(
            out,
            "\n## Description\n\n{}\n",
            if self.body.trim().is_empty() {
                "(empty)"
            } else {
                self.body.trim()
            }
        );
        if !self.comments.is_empty() {
            out.push_str("\n## Comments\n");
            for c in &self.comments {
                let _ = write!(out, "\n### {}\n\n{}\n", c.author, c.body.trim());
            }
        }
        out
    }

    /// Reads the title and URL from the snapshot this issue rendered.
    pub fn heading(key: &str, snapshot: &str) -> Option<(String, String)> {
        let prefix = format!("# {key}: ");
        let (heading, details) = snapshot.split_once("\n\n- URL: ")?;
        let title = heading.strip_prefix(&prefix)?;
        let url = details.split_once('\n').map_or(details, |(url, _)| url);
        Some((title.to_owned(), url.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_reads_a_title_containing_a_colon_and_url_from_the_snapshot() {
        let issue = TrackerIssue {
            key: "NOR-32".into(),
            title: "Keep this title: as selected".into(),
            url: "https://linear.app/team/issue/NOR-32".into(),
            ..TrackerIssue::default()
        };

        assert_eq!(
            TrackerIssue::heading("NOR-32", &issue.render()),
            Some((
                "Keep this title: as selected".into(),
                "https://linear.app/team/issue/NOR-32".into()
            ))
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackerState {
    Done,
    Canceled,
}

pub trait Tracker: Send + Sync {
    /// Open issues that are not done or canceled, with their blockers.
    fn open_issues(&self) -> Result<Vec<TrackerIssue>, HttpError>;
    fn comment(&self, key: &str, body: &str) -> Result<(), HttpError>;
    /// The factory has taken the issue: it is assigned to whoever the factory acts as and, where
    /// the tracker has states, moved to a started one.
    fn start(&self, key: &str) -> Result<(), HttpError>;
    fn set_state(&self, key: &str, state: TrackerState) -> Result<(), HttpError>;
}
