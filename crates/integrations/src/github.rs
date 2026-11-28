//! The factory's access to GitHub: pull requests, checks, branch rules, reviews and
//! comments, replies, and the merge call. Only the daemon holds the token.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::http::{HttpError, Method, Request, Response, Transport, check};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoId {
    pub owner: String,
    pub name: String,
    /// The remote URL `owner/name` was read from, which the factory's Git commands may reach.
    pub remote: String,
}

impl RepoId {
    /// Reads `owner/name` from a GitHub remote URL (HTTPS, SSH, or scp-like).
    pub fn from_remote(url: &str) -> Option<RepoId> {
        let url = url.trim();
        // An HTTPS remote may name its user (`https://user@github.com/...`).
        let https_with_user = url
            .strip_prefix("https://")
            .and_then(|r| r.split_once("@github.com/"))
            .filter(|(user, _)| !user.contains('/'))
            .map(|(_, rest)| rest);
        let rest = https_with_user
            .or_else(|| url.strip_prefix("git@github.com:"))
            .or_else(|| url.strip_prefix("ssh://git@github.com/"))
            .or_else(|| url.strip_prefix("https://github.com/"))
            .or_else(|| url.strip_prefix("http://github.com/"))?;
        let rest = rest.trim_end_matches('/');
        let rest = rest.strip_suffix(".git").unwrap_or(rest);
        let (owner, name) = rest.split_once('/')?;
        (!owner.is_empty() && !name.is_empty() && !name.contains('/')).then(|| RepoId {
            owner: owner.to_owned(),
            name: name.to_owned(),
            remote: url.to_owned(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullState {
    Open,
    Closed,
    Merged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pull {
    pub number: u64,
    pub url: String,
    pub head_sha: String,
    pub state: PullState,
    /// `None` while GitHub is still computing it.
    pub mergeable: Option<bool>,
    /// The branch it targets (`base.ref`), when GitHub names it.
    pub base: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPull<'a> {
    pub head: &'a str,
    pub base: &'a str,
    pub title: &'a str,
    pub body: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckState {
    Pending,
    Success,
    Failure,
}

/// A check run or a commit status, by the name branch rules require it under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub name: String,
    pub state: CheckState,
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BranchRules {
    pub required_checks: BTreeSet<String>,
}

/// What GitHub itself says of a pull request against every rule on its base: classic protection,
/// rulesets and code owners, whether or not the token may read those rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Standing {
    pub review: ReviewDecision,
    pub merge: MergeState,
}

/// Whether GitHub would merge the pull request now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeState {
    /// A rule still stops it: a required check not passed or not reported yet, a required review,
    /// or another rule of the base.
    Blocked,
    /// GitHub is still working it out.
    Computing,
    /// No rule stops it; what the factory checks itself (conflicts, the base) may still.
    Allowed,
}

/// GitHub's reading of a pull request's reviews against every rule that requires them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewDecision {
    /// No rule requires an approving review.
    NotRequired,
    /// A required approving review is missing.
    Required,
    Approved,
    ChangesRequested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewState {
    Approved,
    ChangesRequested,
    Commented,
    Dismissed,
    Pending,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Review {
    pub id: u64,
    pub author: String,
    pub state: ReviewState,
    pub commit_id: Option<String>,
    pub body: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CommentKind {
    /// A comment on a line of the diff; it has a thread to reply in.
    ReviewComment,
    /// A comment on the pull request's conversation.
    IssueComment,
    /// The body of a submitted review.
    Review,
}

impl CommentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CommentKind::ReviewComment => "review-comment",
            CommentKind::IssueComment => "issue-comment",
            CommentKind::Review => "review",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub kind: CommentKind,
    pub id: u64,
    pub author: String,
    pub author_is_bot: bool,
    pub body: String,
    pub path: Option<String>,
    pub line: Option<u64>,
    pub url: String,
}

impl Comment {
    /// The id agents name the comment by, unique across kinds.
    pub fn key(&self) -> String {
        format!("{}-{}", self.kind.as_str(), self.id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Permission {
    None,
    Read,
    Write,
    Admin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeOutcome {
    Merged {
        sha: String,
    },
    /// GitHub refused: not mergeable, or the head is no longer the sha given.
    Refused {
        reason: String,
    },
}

/// The account the token belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub login: String,
    pub id: u64,
}

impl Account {
    /// The noreply address GitHub links to this account, used as the factory's commit email.
    pub fn email(&self) -> String {
        format!("{}+{}@users.noreply.github.com", self.id, self.login)
    }
}

/// Where the factory's Git commands may reach and the token they authenticate with: the remote
/// URL the connection's `RepoId` was read from, and the token the API sends.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct GitRemote<'a> {
    pub url: &'a str,
    pub token: &'a str,
}

/// The token of the signed-in `gh`, when there is one.
pub fn gh_token() -> Option<String> {
    let out = std::process::Command::new("gh")
        .args(["auth", "token"])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .filter(|t| !t.is_empty())
}

/// The GitHub token the factory uses: the one setup stored or, when none was entered, the token
/// of the signed-in `gh`.
pub fn selected_token(stored: Option<String>) -> Option<String> {
    stored.or_else(gh_token)
}

pub trait GitHub {
    /// The account the token belongs to.
    fn viewer(&self) -> Result<Account, HttpError>;
    /// The connected `origin` URL and the token, for the factory's Git commands. No I/O.
    fn git_remote(&self) -> GitRemote<'_>;
    fn pull_for_branch(&self, branch: &str) -> Result<Option<Pull>, HttpError>;
    fn pull(&self, number: u64) -> Result<Pull, HttpError>;
    fn open_pull(&self, new: &NewPull<'_>) -> Result<Pull, HttpError>;
    fn branch_head(&self, branch: &str) -> Result<String, HttpError>;
    fn checks(&self, sha: &str) -> Result<Vec<Check>, HttpError>;
    fn branch_rules(&self, branch: &str) -> Result<BranchRules, HttpError>;
    fn reviews(&self, number: u64) -> Result<Vec<Review>, HttpError>;
    fn standing(&self, number: u64) -> Result<Standing, HttpError>;
    /// Review comments, conversation comments and non-empty review bodies.
    fn comments(&self, number: u64) -> Result<Vec<Comment>, HttpError>;
    fn permission(&self, login: &str) -> Result<Permission, HttpError>;
    /// Answers in the comment's thread when it has one, else in the conversation.
    fn reply(&self, number: u64, to: &Comment, body: &str) -> Result<(), HttpError>;
    fn request_reviewers(&self, number: u64, logins: &[String]) -> Result<(), HttpError>;
    /// Merges only if the pull request's head is still `sha`.
    fn merge(&self, number: u64, sha: &str) -> Result<MergeOutcome, HttpError>;
    fn delete_branch(&self, branch: &str) -> Result<(), HttpError>;
    /// Closes the pull request without merging it; its branch stays.
    fn close_pull(&self, number: u64) -> Result<(), HttpError>;
    fn set_pull_body(&self, number: u64, body: &str) -> Result<(), HttpError>;
}

pub struct GitHubApi<T> {
    transport: T,
    api: String,
    repo: RepoId,
    token: String,
}

impl<T: Transport> GitHubApi<T> {
    pub fn new(transport: T, repo: RepoId, token: String) -> GitHubApi<T> {
        GitHubApi::with_base(transport, "https://api.github.com", repo, token)
    }

    pub fn with_base(transport: T, api: &str, repo: RepoId, token: String) -> GitHubApi<T> {
        GitHubApi {
            transport,
            api: api.trim_end_matches('/').to_owned(),
            repo,
            token,
        }
    }

    pub(crate) fn repo_url(&self, path: &str) -> String {
        format!(
            "{}/repos/{}/{}{path}",
            self.api, self.repo.owner, self.repo.name
        )
    }

    fn request(&self, method: Method, url: String, body: Option<serde_json::Value>) -> Request {
        Request {
            method,
            url,
            headers: vec![
                ("authorization".into(), format!("Bearer {}", self.token)),
                ("accept".into(), "application/vnd.github+json".into()),
                ("x-github-api-version".into(), "2022-11-28".into()),
            ],
            body,
            raw: None,
        }
    }

    pub(crate) fn call(
        &self,
        method: Method,
        url: String,
        body: Option<serde_json::Value>,
    ) -> Result<Response, HttpError> {
        let req = self.request(method, url, body);
        let res = self.transport.send(&req)?;
        check(&req, res)
    }

    pub(crate) fn get<D: DeserializeOwned>(&self, url: String) -> Result<D, HttpError> {
        decode(&self.call(Method::Get, url, None)?)
    }

    /// GET that treats 404 as absent, and 403 too when `forbidden_is_absent`.
    pub(crate) fn get_optional<D: DeserializeOwned>(
        &self,
        url: String,
        forbidden_is_absent: bool,
    ) -> Result<Option<D>, HttpError> {
        match self.call(Method::Get, url, None) {
            Ok(res) => decode(&res).map(Some),
            Err(e)
                if e.status() == Some(404) || (forbidden_is_absent && e.status() == Some(403)) =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    /// Every page of a list, following `Link: rel="next"`.
    pub(crate) fn get_all<D: DeserializeOwned>(&self, url: &str) -> Result<Vec<D>, HttpError> {
        let sep = if url.contains('?') { '&' } else { '?' };
        let mut next = Some(format!("{url}{sep}per_page=100"));
        let mut out = Vec::new();
        while let Some(url) = next.take() {
            let res = self.call(Method::Get, url, None)?;
            let mut page: Vec<D> = decode(&res)?;
            out.append(&mut page);
            next = res.header("link").and_then(next_link);
        }
        Ok(out)
    }

    fn check_runs(&self, sha: &str) -> Result<Vec<Check>, HttpError> {
        let mut url = Some(format!(
            "{}?per_page=100",
            self.repo_url(&format!("/commits/{sha}/check-runs"))
        ));
        let mut out = Vec::new();
        while let Some(u) = url.take() {
            let res = self.call(Method::Get, u, None)?;
            let page: RawCheckRuns = decode(&res)?;
            out.extend(page.check_runs.into_iter().map(RawCheckRun::into_check));
            url = res.header("link").and_then(next_link);
        }
        Ok(out)
    }

    fn statuses(&self, sha: &str) -> Result<Vec<Check>, HttpError> {
        let combined: RawCombinedStatus =
            self.get(self.repo_url(&format!("/commits/{sha}/status")))?;
        Ok(combined
            .statuses
            .into_iter()
            .map(|s| Check {
                state: match s.state.as_str() {
                    "success" => CheckState::Success,
                    "pending" => CheckState::Pending,
                    _ => CheckState::Failure,
                },
                name: s.context,
                url: s.target_url,
            })
            .collect())
    }
}

impl<T: Transport> GitHub for GitHubApi<T> {
    fn viewer(&self) -> Result<Account, HttpError> {
        let viewer: RawViewer = self.get(format!("{}/user", self.api))?;
        Ok(Account {
            login: viewer.login,
            id: viewer.id,
        })
    }

    fn git_remote(&self) -> GitRemote<'_> {
        GitRemote {
            url: &self.repo.remote,
            token: &self.token,
        }
    }

    fn pull_for_branch(&self, branch: &str) -> Result<Option<Pull>, HttpError> {
        let url = self.repo_url(&format!(
            "/pulls?state=all&head={}:{}",
            self.repo.owner,
            encode(branch)
        ));
        let pulls: Vec<RawPull> = self.get(url)?;
        Ok(pulls.into_iter().next().map(RawPull::into_pull))
    }

    fn pull(&self, number: u64) -> Result<Pull, HttpError> {
        let pull: RawPull = self.get(self.repo_url(&format!("/pulls/{number}")))?;
        Ok(pull.into_pull())
    }

    fn open_pull(&self, new: &NewPull<'_>) -> Result<Pull, HttpError> {
        let body = serde_json::json!({"head": new.head, "base": new.base, "title": new.title, "body": new.body});
        let res = self.call(Method::Post, self.repo_url("/pulls"), Some(body))?;
        decode::<RawPull>(&res).map(RawPull::into_pull)
    }

    fn branch_head(&self, branch: &str) -> Result<String, HttpError> {
        let r: RawRef = self.get(self.repo_url(&format!("/git/ref/heads/{}", encode(branch))))?;
        Ok(r.object.sha)
    }

    fn checks(&self, sha: &str) -> Result<Vec<Check>, HttpError> {
        let mut out = self.check_runs(sha)?;
        out.extend(self.statuses(sha)?);
        Ok(out)
    }

    fn branch_rules(&self, branch: &str) -> Result<BranchRules, HttpError> {
        let mut rules = BranchRules::default();
        // Private repositories on GitHub's free plan answer rulesets with 403: they have none.
        let ruleset: Vec<RawRule> =
            match self.get_all(&self.repo_url(&format!("/rules/branches/{}", encode(branch)))) {
                Err(e) if matches!(e.status(), Some(403 | 404)) => Vec::new(),
                other => other?,
            };
        for rule in ruleset {
            rule.apply(&mut rules);
        }
        let url = self.repo_url(&format!("/branches/{}/protection", encode(branch)));
        // Reading classic branch protection needs admin rights. Without them only rulesets are
        // known here; GitHub still refuses a merge the protection forbids. Required reviews are
        // read from the pull request's review decision instead, which needs no such right.
        if let Some(p) = self.get_optional::<RawProtection>(url, true)? {
            p.apply(&mut rules);
        }
        Ok(rules)
    }

    fn reviews(&self, number: u64) -> Result<Vec<Review>, HttpError> {
        let raw: Vec<RawReview> =
            self.get_all(&self.repo_url(&format!("/pulls/{number}/reviews")))?;
        Ok(raw.into_iter().map(RawReview::into_review).collect())
    }

    fn standing(&self, number: u64) -> Result<Standing, HttpError> {
        let query = "query Standing($owner: String!, $name: String!, $number: Int!) { repository(owner: $owner, name: $name) { pullRequest(number: $number) { reviewDecision mergeStateStatus } } }";
        let body = serde_json::json!({
            "query": query,
            "variables": {"owner": self.repo.owner, "name": self.repo.name, "number": number},
        });
        let res = self.call(Method::Post, format!("{}/graphql", self.api), Some(body))?;
        let raw: RawGraphQl<RawDecisionData> = decode(&res)?;
        if let Some(e) = raw.errors.first() {
            return Err(HttpError::Decode(format!(
                "GitHub could not read pull request {number}'s standing: {}",
                e.message
            )));
        }
        let pull = raw
            .data
            .and_then(|d| d.repository)
            .and_then(|r| r.pull_request)
            .ok_or_else(|| HttpError::Decode(format!("pull request {number} was not found")))?;
        let review = match pull.review_decision.as_deref() {
            None => ReviewDecision::NotRequired,
            Some("APPROVED") => ReviewDecision::Approved,
            Some("CHANGES_REQUESTED") => ReviewDecision::ChangesRequested,
            Some(_) => ReviewDecision::Required,
        };
        let merge = match pull.merge_state_status.as_str() {
            "BLOCKED" => MergeState::Blocked,
            "UNKNOWN" => MergeState::Computing,
            _ => MergeState::Allowed,
        };
        Ok(Standing { review, merge })
    }

    fn comments(&self, number: u64) -> Result<Vec<Comment>, HttpError> {
        let line: Vec<RawComment> =
            self.get_all(&self.repo_url(&format!("/pulls/{number}/comments")))?;
        let conversation: Vec<RawComment> =
            self.get_all(&self.repo_url(&format!("/issues/{number}/comments")))?;
        let reviews: Vec<RawReview> =
            self.get_all(&self.repo_url(&format!("/pulls/{number}/reviews")))?;
        let mut out: Vec<Comment> = line
            .into_iter()
            .map(|c| c.into_comment(CommentKind::ReviewComment))
            .collect();
        out.extend(
            conversation
                .into_iter()
                .map(|c| c.into_comment(CommentKind::IssueComment)),
        );
        out.extend(reviews.into_iter().filter_map(RawReview::into_comment));
        Ok(out)
    }

    fn permission(&self, login: &str) -> Result<Permission, HttpError> {
        let url = self.repo_url(&format!("/collaborators/{}/permission", encode(login)));
        let Some(p) = self.get_optional::<RawPermission>(url, false)? else {
            return Ok(Permission::None);
        };
        Ok(match (p.permission.as_str(), p.role_name.as_deref()) {
            ("admin", _) => Permission::Admin,
            ("write", _) | (_, Some("maintain")) => Permission::Write,
            ("read", _) => Permission::Read,
            _ => Permission::None,
        })
    }

    fn reply(&self, number: u64, to: &Comment, body: &str) -> Result<(), HttpError> {
        let (url, text) = match to.kind {
            CommentKind::ReviewComment => (
                self.repo_url(&format!("/pulls/{number}/comments/{}/replies", to.id)),
                body.to_owned(),
            ),
            CommentKind::IssueComment | CommentKind::Review => (
                self.repo_url(&format!("/issues/{number}/comments")),
                format!("Re: {}\n\n{body}", to.url),
            ),
        };
        self.call(Method::Post, url, Some(serde_json::json!({"body": text})))
            .map(drop)
    }

    fn request_reviewers(&self, number: u64, logins: &[String]) -> Result<(), HttpError> {
        if logins.is_empty() {
            return Ok(());
        }
        let url = self.repo_url(&format!("/pulls/{number}/requested_reviewers"));
        self.call(
            Method::Post,
            url,
            Some(serde_json::json!({"reviewers": logins})),
        )
        .map(drop)
    }

    fn merge(&self, number: u64, sha: &str) -> Result<MergeOutcome, HttpError> {
        let settings: RawRepo = self.get(self.repo_url(""))?;
        let url = self.repo_url(&format!("/pulls/{number}/merge"));
        let body = serde_json::json!({"sha": sha, "merge_method": settings.merge_method()});
        match self.call(Method::Put, url, Some(body)) {
            Ok(res) => decode::<RawMerge>(&res).map(|m| MergeOutcome::Merged { sha: m.sha }),
            Err(HttpError::Status {
                status: 405 | 409 | 422,
                message,
                ..
            }) => Ok(MergeOutcome::Refused { reason: message }),
            Err(e) => Err(e),
        }
    }

    fn set_pull_body(&self, number: u64, body: &str) -> Result<(), HttpError> {
        self.call(
            Method::Patch,
            self.repo_url(&format!("/pulls/{number}")),
            Some(serde_json::json!({"body": body})),
        )
        .map(drop)
    }

    fn close_pull(&self, number: u64) -> Result<(), HttpError> {
        let body = serde_json::json!({"state": "closed"});
        self.call(
            Method::Patch,
            self.repo_url(&format!("/pulls/{number}")),
            Some(body),
        )
        .map(drop)
    }

    fn delete_branch(&self, branch: &str) -> Result<(), HttpError> {
        let url = self.repo_url(&format!("/git/refs/heads/{}", encode(branch)));
        self.call(Method::Delete, url, None).map(drop)
    }
}

fn decode<D: DeserializeOwned>(res: &Response) -> Result<D, HttpError> {
    serde_json::from_str(&res.body)
        .map_err(|e| HttpError::Decode(format!("unexpected GitHub response: {e}")))
}

/// The `rel="next"` URL of a `Link` header.
fn next_link(link: &str) -> Option<String> {
    link.split(',').find_map(|part| {
        let (url, rel) = part.split_once(';')?;
        rel.contains("rel=\"next\"").then(|| {
            url.trim()
                .trim_start_matches('<')
                .trim_end_matches('>')
                .to_owned()
        })
    })
}

/// Percent-encodes what a branch name or login can hold that a URL path or query cannot.
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

#[derive(Deserialize)]
struct RawViewer {
    login: String,
    id: u64,
}

#[derive(Deserialize)]
struct RawUser {
    login: String,
    #[serde(rename = "type", default)]
    kind: String,
}

#[derive(Deserialize)]
struct RawBranchRef {
    sha: String,
}

#[derive(Deserialize)]
struct RawPull {
    number: u64,
    html_url: String,
    state: String,
    #[serde(default)]
    merged_at: Option<String>,
    #[serde(default)]
    mergeable: Option<bool>,
    head: RawBranchRef,
    #[serde(default)]
    base: Option<RawBaseRef>,
}

#[derive(Deserialize)]
struct RawBaseRef {
    #[serde(rename = "ref")]
    name: String,
}

impl RawPull {
    fn into_pull(self) -> Pull {
        let state = match (self.state.as_str(), self.merged_at.is_some()) {
            (_, true) => PullState::Merged,
            ("open", false) => PullState::Open,
            _ => PullState::Closed,
        };
        Pull {
            number: self.number,
            url: self.html_url,
            head_sha: self.head.sha,
            state,
            mergeable: self.mergeable,
            base: self.base.map(|b| b.name).filter(|b| !b.is_empty()),
        }
    }
}

#[derive(Deserialize)]
struct RawObject {
    sha: String,
}

#[derive(Deserialize)]
struct RawRef {
    object: RawObject,
}

#[derive(Deserialize)]
struct RawCheckRuns {
    check_runs: Vec<RawCheckRun>,
}

#[derive(Deserialize)]
struct RawCheckRun {
    name: String,
    status: String,
    #[serde(default)]
    conclusion: Option<String>,
    #[serde(default)]
    html_url: Option<String>,
}

impl RawCheckRun {
    fn into_check(self) -> Check {
        let state = match (self.status.as_str(), self.conclusion.as_deref()) {
            ("completed", Some("success" | "neutral" | "skipped")) => CheckState::Success,
            ("completed", _) => CheckState::Failure,
            _ => CheckState::Pending,
        };
        Check {
            name: self.name,
            state,
            url: self.html_url,
        }
    }
}

#[derive(Deserialize)]
struct RawStatus {
    context: String,
    state: String,
    #[serde(default)]
    target_url: Option<String>,
}

#[derive(Deserialize)]
struct RawCombinedStatus {
    #[serde(default)]
    statuses: Vec<RawStatus>,
}

#[derive(Deserialize)]
struct RawRule {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    parameters: serde_json::Value,
}

impl RawRule {
    fn apply(self, rules: &mut BranchRules) {
        if self.kind != "required_status_checks" {
            return;
        }
        if let Some(checks) = self
            .parameters
            .get("required_status_checks")
            .and_then(serde_json::Value::as_array)
        {
            rules.required_checks.extend(
                checks
                    .iter()
                    .filter_map(|c| c.get("context")?.as_str().map(str::to_owned)),
            );
        }
    }
}

#[derive(Deserialize)]
struct RawRequiredChecks {
    #[serde(default)]
    contexts: Vec<String>,
}

#[derive(Deserialize)]
struct RawProtection {
    #[serde(default)]
    required_status_checks: Option<RawRequiredChecks>,
}

impl RawProtection {
    fn apply(self, rules: &mut BranchRules) {
        if let Some(c) = self.required_status_checks {
            rules.required_checks.extend(c.contexts);
        }
    }
}

#[derive(Deserialize)]
struct RawGraphQl<D> {
    data: Option<D>,
    #[serde(default)]
    errors: Vec<RawGraphQlError>,
}

#[derive(Deserialize)]
struct RawGraphQlError {
    message: String,
}

#[derive(Deserialize)]
struct RawDecisionData {
    repository: Option<RawDecisionRepo>,
}

#[derive(Deserialize)]
struct RawDecisionRepo {
    #[serde(rename = "pullRequest")]
    pull_request: Option<RawDecisionPull>,
}

#[derive(Deserialize)]
struct RawDecisionPull {
    #[serde(rename = "reviewDecision")]
    review_decision: Option<String>,
    #[serde(rename = "mergeStateStatus")]
    merge_state_status: String,
}

#[derive(Deserialize)]
struct RawReview {
    id: u64,
    user: Option<RawUser>,
    state: String,
    #[serde(default)]
    commit_id: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    html_url: String,
}

impl RawReview {
    fn into_review(self) -> Review {
        Review {
            id: self.id,
            author: self.user.map(|u| u.login).unwrap_or_default(),
            state: match self.state.as_str() {
                "APPROVED" => ReviewState::Approved,
                "CHANGES_REQUESTED" => ReviewState::ChangesRequested,
                "DISMISSED" => ReviewState::Dismissed,
                "PENDING" => ReviewState::Pending,
                _ => ReviewState::Commented,
            },
            commit_id: self.commit_id,
            body: self.body.unwrap_or_default(),
        }
    }

    fn into_comment(self) -> Option<Comment> {
        let body = self.body.unwrap_or_default();
        if body.trim().is_empty() || self.state == "PENDING" {
            return None;
        }
        let (author, author_is_bot) = self
            .user
            .map_or((String::new(), false), |u| (u.login, u.kind == "Bot"));
        Some(Comment {
            kind: CommentKind::Review,
            id: self.id,
            author,
            author_is_bot,
            body,
            path: None,
            line: None,
            url: self.html_url,
        })
    }
}

#[derive(Deserialize)]
struct RawComment {
    id: u64,
    user: Option<RawUser>,
    #[serde(default)]
    body: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    line: Option<u64>,
    html_url: String,
}

impl RawComment {
    fn into_comment(self, kind: CommentKind) -> Comment {
        let (author, author_is_bot) = self
            .user
            .map_or((String::new(), false), |u| (u.login, u.kind == "Bot"));
        Comment {
            kind,
            id: self.id,
            author,
            author_is_bot,
            body: self.body,
            path: self.path,
            line: self.line,
            url: self.html_url,
        }
    }
}

#[derive(Deserialize)]
struct RawPermission {
    permission: String,
    #[serde(default)]
    role_name: Option<String>,
}

#[derive(Deserialize)]
struct RawRepo {
    #[serde(default = "yes")]
    allow_squash_merge: bool,
    #[serde(default = "yes")]
    allow_merge_commit: bool,
}

fn yes() -> bool {
    true
}

impl RawRepo {
    /// Squash when the repository allows it, then a merge commit, then rebase.
    fn merge_method(&self) -> &'static str {
        if self.allow_squash_merge {
            "squash"
        } else if self.allow_merge_commit {
            "merge"
        } else {
            "rebase"
        }
    }
}

#[derive(Deserialize)]
struct RawMerge {
    sha: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_owner_and_name_from_github_remotes() {
        for url in [
            "git@github.com:acme/storefront.git",
            "https://github.com/acme/storefront",
            "https://github.com/acme/storefront.git",
            "ssh://git@github.com/acme/storefront.git",
            "https://kai@github.com/acme/storefront.git",
        ] {
            assert_eq!(
                RepoId::from_remote(url),
                Some(RepoId {
                    owner: "acme".into(),
                    name: "storefront".into(),
                    remote: url.into(),
                }),
                "{url}"
            );
        }
        assert_eq!(
            RepoId::from_remote("https://gitlab.com/acme/storefront"),
            None
        );
        assert_eq!(RepoId::from_remote("https://github.com/acme"), None);
    }

    #[test]
    fn follows_the_next_link_only() {
        let link = r#"<https://api.github.com/x?page=2>; rel="next", <https://api.github.com/x?page=5>; rel="last""#;
        assert_eq!(
            next_link(link).as_deref(),
            Some("https://api.github.com/x?page=2")
        );
        assert_eq!(
            next_link(r#"<https://api.github.com/x?page=1>; rel="prev""#),
            None
        );
    }

    #[test]
    fn encodes_what_a_path_cannot_hold() {
        assert_eq!(encode("ovenspring/ACME-1"), "ovenspring/ACME-1");
        assert_eq!(encode("a b#c"), "a%20b%23c");
        assert_eq!(encode("bot[bot]"), "bot%5Bbot%5D");
    }
}
