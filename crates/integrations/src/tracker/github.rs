//! GitHub Issues as the tracker: the repository's own open issues, keyed `GH-<number>`, with
//! their `blocked_by` dependencies.

use serde::Deserialize;

use super::{Tracker, TrackerComment, TrackerIssue, TrackerState};
use crate::github::GitHubApi;
use crate::http::{HttpError, Method, Transport};

const PREFIX: &str = "GH-";

fn number(key: &str) -> Result<u64, HttpError> {
    key.strip_prefix(PREFIX)
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| {
            HttpError::Decode(format!(
                "{key} is not a GitHub issue key ({PREFIX}<number>)"
            ))
        })
}

#[derive(Deserialize)]
struct RawLabel {
    name: String,
}

#[derive(Deserialize)]
struct RawLogin {
    login: String,
}

#[derive(Deserialize)]
struct RawSummary {
    #[serde(default)]
    blocked_by: u64,
}

#[derive(Deserialize)]
struct RawIssue {
    number: u64,
    title: String,
    #[serde(default)]
    body: Option<String>,
    html_url: String,
    state: String,
    #[serde(default)]
    labels: Vec<RawLabel>,
    #[serde(default)]
    comments: u64,
    /// Present on pull requests, which the issues list also returns.
    #[serde(default)]
    pull_request: Option<serde_json::Value>,
    #[serde(default)]
    issue_dependencies_summary: Option<RawSummary>,
}

#[derive(Deserialize)]
struct RawIssueComment {
    user: Option<RawLogin>,
    #[serde(default)]
    body: String,
}

#[derive(Deserialize)]
struct RawBlocker {
    number: u64,
    state: String,
}

impl<T: Transport> GitHubApi<T> {
    fn issue_comments(&self, n: u64) -> Result<Vec<TrackerComment>, HttpError> {
        let raw: Vec<RawIssueComment> =
            self.get_all(&self.repo_url(&format!("/issues/{n}/comments")))?;
        Ok(raw
            .into_iter()
            .map(|c| TrackerComment {
                author: c.user.map(|u| u.login).unwrap_or_default(),
                body: c.body,
            })
            .collect())
    }

    fn blockers(&self, n: u64) -> Result<Vec<String>, HttpError> {
        let url = self.repo_url(&format!("/issues/{n}/dependencies/blocked_by"));
        let raw: Vec<RawBlocker> = self.get_optional(url, false)?.unwrap_or_default();
        Ok(raw
            .into_iter()
            .filter(|b| b.state == "open")
            .map(|b| format!("{PREFIX}{}", b.number))
            .collect())
    }
}

impl<T: Transport> Tracker for GitHubApi<T> {
    fn open_issues(&self) -> Result<Vec<TrackerIssue>, HttpError> {
        let raw: Vec<RawIssue> = self.get_all(&self.repo_url("/issues?state=open"))?;
        let mut out = Vec::new();
        for i in raw.into_iter().filter(|i| i.pull_request.is_none()) {
            let comments = if i.comments > 0 {
                self.issue_comments(i.number)?
            } else {
                Vec::new()
            };
            // Without the summary (older servers), ask; with it, ask only when something blocks.
            let depends_on = match &i.issue_dependencies_summary {
                Some(s) if s.blocked_by == 0 => Vec::new(),
                _ => self.blockers(i.number)?,
            };
            out.push(TrackerIssue {
                key: format!("{PREFIX}{}", i.number),
                title: i.title,
                body: i.body.unwrap_or_default(),
                url: i.html_url,
                state: i.state,
                started: false,
                labels: i.labels.into_iter().map(|l| l.name).collect(),
                comments,
                depends_on,
            });
        }
        Ok(out)
    }

    fn comment(&self, key: &str, body: &str) -> Result<(), HttpError> {
        let url = self.repo_url(&format!("/issues/{}/comments", number(key)?));
        self.call(Method::Post, url, Some(serde_json::json!({"body": body})))
            .map(drop)
    }

    fn start(&self, key: &str) -> Result<(), HttpError> {
        let me = crate::github::GitHub::viewer(self)?;
        let url = self.repo_url(&format!("/issues/{}/assignees", number(key)?));
        self.call(
            Method::Post,
            url,
            Some(serde_json::json!({"assignees": [me]})),
        )
        .map(drop)
    }

    fn set_state(&self, key: &str, state: TrackerState) -> Result<(), HttpError> {
        let reason = match state {
            TrackerState::Done => "completed",
            TrackerState::Canceled => "not_planned",
        };
        let url = self.repo_url(&format!("/issues/{}", number(key)?));
        self.call(
            Method::Patch,
            url,
            Some(serde_json::json!({"state": "closed", "state_reason": reason})),
        )
        .map(drop)
    }
}
