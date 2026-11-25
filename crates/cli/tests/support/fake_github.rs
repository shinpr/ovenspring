//! An in-memory GitHub over a real bare `origin`: pull request heads are the branches there,
//! checks follow a per-test script by the order commits are pushed, and `merge(pr, sha)` refuses
//! any sha but the head and otherwise merges into the base branch for real.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

use ovenspring_integrations::github::{
    BranchRules, Check, CheckState, Comment, CommentKind, GitHub, MergeOutcome, MergeState,
    NewPull, Permission, Pull, PullState, Review, ReviewDecision, ReviewState, Standing,
};
use ovenspring_integrations::http::{HttpError, Unanswered};

/// The checks a pushed commit gets, by the order it was first asked about (0 = first push).
pub type CheckScript = Box<dyn Fn(usize) -> Vec<(String, CheckState)> + Send + Sync>;

/// A comment to post on a pull request when it opens (on every branch, or only on `branch`).
#[derive(Clone)]
pub struct Planned {
    pub kind: CommentKind,
    pub author: String,
    pub bot: bool,
    pub body: String,
    pub branch: Option<String>,
}

/// A review given on a pull request's first head when it opens.
#[derive(Clone)]
pub struct PlannedReview {
    pub branch: String,
    pub author: String,
    pub state: ReviewState,
}

#[derive(Default)]
struct State {
    pulls: Vec<(u64, String, String, PullState)>,
    shas: Vec<String>,
    comments: BTreeMap<u64, Vec<Comment>>,
    reviews: BTreeMap<u64, Vec<Review>>,
    next_id: u64,
}

pub struct FakeGitHub {
    origin: PathBuf,
    pub viewer: String,
    pub rules: BranchRules,
    /// Approving reviews a rule requires; GitHub reports the shortfall as the review decision.
    pub approvals_required: usize,
    /// Checks classic protection requires, which the factory's token cannot read from the branch
    /// rules; GitHub blocks the merge until each succeeds.
    pub unreadable_required: Vec<String>,
    pub permissions: BTreeMap<String, Permission>,
    pub on_open: Vec<Planned>,
    pub reviews_on_open: Vec<PlannedReview>,
    /// Approvals given on an earlier head are dismissed, as the branch rule does.
    pub dismiss_stale: bool,
    /// Runs once, at the first read of a branch head (the factory's last look before merging).
    pub before_merge_look: Mutex<Option<Box<dyn FnOnce() + Send>>>,
    checks: CheckScript,
    state: Mutex<State>,
    /// Makes the next `open_pull` fail as if GitHub were unreachable.
    pub fail_next_open: std::sync::atomic::AtomicBool,
    /// While set, every call fails as if GitHub could not be reached.
    pub unreachable: std::sync::atomic::AtomicBool,
    /// While set, reading the pull request of this branch is refused (HTTP 403).
    pub refuse_pull_of: Mutex<Option<String>>,
    /// While set, a pull request reads with this head, as GitHub does for a moment after a push.
    pub stale_head: Mutex<Option<String>>,
    /// Every call, as `name args`.
    pub calls: Mutex<Vec<String>>,
}

fn bare(origin: &PathBuf, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("--git-dir")
        .arg(origin)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

impl FakeGitHub {
    pub fn new(origin: PathBuf, rules: BranchRules, checks: CheckScript) -> FakeGitHub {
        FakeGitHub {
            origin,
            viewer: "factory-bot".into(),
            rules,
            approvals_required: 0,
            unreadable_required: Vec::new(),
            permissions: BTreeMap::new(),
            on_open: Vec::new(),
            reviews_on_open: Vec::new(),
            dismiss_stale: false,
            before_merge_look: Mutex::new(None),
            fail_next_open: std::sync::atomic::AtomicBool::new(false),
            unreachable: std::sync::atomic::AtomicBool::new(false),
            refuse_pull_of: Mutex::new(None),
            stale_head: Mutex::new(None),
            checks,
            state: Mutex::new(State {
                next_id: 1,
                ..State::default()
            }),
            calls: Mutex::new(Vec::new()),
        }
    }

    fn call(&self, what: String) {
        self.calls.lock().unwrap().push(what);
    }

    /// What GitHub decides of the reviews: approvals against `approvals_required`.
    fn decision(&self, number: u64) -> Result<ReviewDecision, HttpError> {
        let mut latest: BTreeMap<String, ReviewState> = BTreeMap::new();
        for r in self.reviews(number)? {
            if matches!(
                r.state,
                ReviewState::Approved | ReviewState::ChangesRequested | ReviewState::Dismissed
            ) {
                latest.insert(r.author, r.state);
            }
        }
        let approved = latest
            .values()
            .filter(|s| **s == ReviewState::Approved)
            .count();
        Ok(
            if latest.values().any(|s| *s == ReviewState::ChangesRequested) {
                ReviewDecision::ChangesRequested
            } else if approved < self.approvals_required {
                ReviewDecision::Required
            } else if self.approvals_required == 0 {
                ReviewDecision::NotRequired
            } else {
                ReviewDecision::Approved
            },
        )
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    /// Replies sent, as (comment key, body).
    pub fn replies(&self) -> Vec<String> {
        self.calls()
            .into_iter()
            .filter(|c| c.starts_with("reply "))
            .collect()
    }

    fn head(&self, branch: &str) -> String {
        bare(
            &self.origin,
            &["rev-parse", &format!("refs/heads/{branch}")],
        )
        .unwrap_or_default()
    }

    fn pull_of(&self, number: u64) -> Result<Pull, HttpError> {
        let st = self.state.lock().unwrap();
        let (n, head, base, state) = st
            .pulls
            .iter()
            .find(|p| p.0 == number)
            .cloned()
            .ok_or_else(|| HttpError::Decode(format!("no pull {number}")))?;
        drop(st);
        let head_sha = self.head(&head);
        let base_sha = self.head(&base);
        let mergeable = bare(
            &self.origin,
            &["merge-tree", "--write-tree", &base_sha, &head_sha],
        )
        .is_ok();
        Ok(Pull {
            number: n,
            url: format!("https://github.test/acme/storefront/pull/{n}"),
            head_sha,
            state,
            mergeable: Some(mergeable),
        })
    }

    /// A pull request the factory opened before (for restart tests). Returns its number.
    pub fn add_pull(&self, head: &str, base: &str) -> u64 {
        let mut st = self.state.lock().unwrap();
        let number = st.pulls.len() as u64 + 1;
        st.pulls
            .push((number, head.into(), base.into(), PullState::Open));
        number
    }

    /// The owner merges the pull request on GitHub, outside the factory.
    pub fn merge_as_owner(&self, number: u64) {
        let head = self.pull_of(number).unwrap().head_sha;
        let before = self.calls.lock().unwrap().len();
        self.merge(number, &head).unwrap();
        self.calls.lock().unwrap().truncate(before);
    }

    /// Someone closes the pull request on GitHub without merging it.
    pub fn close_as_owner(&self, number: u64) {
        let mut st = self.state.lock().unwrap();
        if let Some(p) = st.pulls.iter_mut().find(|p| p.0 == number) {
            p.3 = PullState::Closed;
        }
    }

    /// `author`, with write access, comments on a line of the pull request.
    pub fn comment(&self, number: u64, author: &str, body: &str) {
        let mut st = self.state.lock().unwrap();
        let id = st.next_id;
        st.next_id += 1;
        st.comments.entry(number).or_default().push(Comment {
            kind: CommentKind::ReviewComment,
            id,
            author: author.into(),
            author_is_bot: false,
            body: body.into(),
            path: Some("src/orders.ts".into()),
            line: Some(1),
            url: format!("https://github.test/acme/storefront/pull/{number}#c{id}"),
        });
    }

    /// Adds a review as `author` on the pull request's current head.
    pub fn review(&self, number: u64, author: &str, state: ReviewState) {
        let head = self.pull_of(number).map(|p| p.head_sha).ok();
        let mut st = self.state.lock().unwrap();
        let id = st.next_id;
        st.next_id += 1;
        st.reviews.entry(number).or_default().push(Review {
            id,
            author: author.into(),
            state,
            commit_id: head,
            body: String::new(),
        });
    }
}

impl FakeGitHub {
    fn reachable(&self) -> Result<(), HttpError> {
        if self.unreachable.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(HttpError::transient(
                Unanswered::NoConnection,
                "connection refused".into(),
            ));
        }
        Ok(())
    }
}

impl GitHub for FakeGitHub {
    fn viewer(&self) -> Result<String, HttpError> {
        self.reachable()?;
        Ok(self.viewer.clone())
    }

    fn pull_for_branch(&self, branch: &str) -> Result<Option<Pull>, HttpError> {
        self.call(format!("pull_for_branch {branch}"));
        let n = self
            .state
            .lock()
            .unwrap()
            .pulls
            .iter()
            .find(|p| p.1 == branch)
            .map(|p| p.0);
        n.map(|n| self.pull_of(n)).transpose()
    }

    fn pull(&self, number: u64) -> Result<Pull, HttpError> {
        let refused = self.refuse_pull_of.lock().unwrap().clone();
        let branch = self
            .state
            .lock()
            .unwrap()
            .pulls
            .iter()
            .find(|p| p.0 == number)
            .map(|p| p.1.clone());
        if refused.is_some() && refused == branch {
            return Err(HttpError::Status {
                method: "GET",
                url: format!("https://api.github.test/repos/acme/storefront/pulls/{number}"),
                status: 403,
                message: "Resource not accessible by integration".into(),
            });
        }
        let mut pull = self.pull_of(number)?;
        if let Some(head) = self.stale_head.lock().unwrap().clone() {
            pull.head_sha = head;
        }
        Ok(pull)
    }

    fn open_pull(&self, new: &NewPull<'_>) -> Result<Pull, HttpError> {
        self.call(format!("open_pull {} -> {}", new.head, new.base));
        self.reachable()?;
        if self
            .fail_next_open
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            return Err(HttpError::transient(
                Unanswered::NoConnection,
                "connection refused".into(),
            ));
        }
        let number = {
            let mut st = self.state.lock().unwrap();
            let number = st.pulls.len() as u64 + 1;
            st.pulls
                .push((number, new.head.into(), new.base.into(), PullState::Open));
            for p in self
                .on_open
                .iter()
                .filter(|p| p.branch.as_deref().is_none_or(|b| b == new.head))
            {
                let id = st.next_id;
                st.next_id += 1;
                st.comments.entry(number).or_default().push(Comment {
                    kind: p.kind,
                    id,
                    author: p.author.clone(),
                    author_is_bot: p.bot,
                    body: p.body.clone(),
                    path: Some("src/orders.ts".into()),
                    line: Some(1),
                    url: format!("https://github.test/acme/storefront/pull/{number}#c{id}"),
                });
            }
            number
        };
        for r in self.reviews_on_open.iter().filter(|r| r.branch == new.head) {
            self.review(number, &r.author, r.state);
        }
        self.pull_of(number)
    }

    fn branch_head(&self, branch: &str) -> Result<String, HttpError> {
        let hook = self.before_merge_look.lock().unwrap().take();
        if let Some(hook) = hook {
            hook();
        }
        Ok(self.head(branch))
    }

    fn checks(&self, sha: &str) -> Result<Vec<Check>, HttpError> {
        let nth = {
            let mut st = self.state.lock().unwrap();
            if let Some(i) = st.shas.iter().position(|s| s == sha) {
                i
            } else {
                st.shas.push(sha.into());
                st.shas.len() - 1
            }
        };
        Ok((self.checks)(nth)
            .into_iter()
            .map(|(name, state)| Check {
                name,
                state,
                url: Some(format!("https://ci.test/{sha}")),
            })
            .collect())
    }

    fn branch_rules(&self, _branch: &str) -> Result<BranchRules, HttpError> {
        Ok(self.rules.clone())
    }

    fn reviews(&self, number: u64) -> Result<Vec<Review>, HttpError> {
        let head = self.pull_of(number).map(|p| p.head_sha).ok();
        let mut reviews = self
            .state
            .lock()
            .unwrap()
            .reviews
            .get(&number)
            .cloned()
            .unwrap_or_default();
        if self.dismiss_stale {
            for r in &mut reviews {
                if r.state == ReviewState::Approved && r.commit_id != head {
                    r.state = ReviewState::Dismissed;
                }
            }
        }
        Ok(reviews)
    }

    fn standing(&self, number: u64) -> Result<Standing, HttpError> {
        let review = self.decision(number)?;
        let head = self.pull_of(number)?.head_sha;
        let checks = self.checks(&head)?;
        let checks_missing = self.unreadable_required.iter().any(|name| {
            !checks
                .iter()
                .any(|c| &c.name == name && c.state == CheckState::Success)
        });
        let merge = if review == ReviewDecision::Required || checks_missing {
            MergeState::Blocked
        } else {
            MergeState::Allowed
        };
        Ok(Standing { review, merge })
    }

    fn comments(&self, number: u64) -> Result<Vec<Comment>, HttpError> {
        Ok(self
            .state
            .lock()
            .unwrap()
            .comments
            .get(&number)
            .cloned()
            .unwrap_or_default())
    }

    fn permission(&self, login: &str) -> Result<Permission, HttpError> {
        self.call(format!("permission {login}"));
        Ok(self
            .permissions
            .get(login)
            .copied()
            .unwrap_or(Permission::None))
    }

    fn reply(&self, number: u64, to: &Comment, body: &str) -> Result<(), HttpError> {
        self.call(format!("reply {number} {} {body}", to.key()));
        Ok(())
    }

    fn request_reviewers(&self, number: u64, logins: &[String]) -> Result<(), HttpError> {
        self.call(format!("request_reviewers {number} {}", logins.join(",")));
        Ok(())
    }

    fn set_pull_body(&self, number: u64, body: &str) -> Result<(), HttpError> {
        self.call(format!("set_pull_body {number} {body}"));
        Ok(())
    }

    fn close_pull(&self, number: u64) -> Result<(), HttpError> {
        self.call(format!("close_pull {number}"));
        self.close_as_owner(number);
        Ok(())
    }

    fn delete_branch(&self, branch: &str) -> Result<(), HttpError> {
        self.call(format!("delete_branch {branch}"));
        bare(
            &self.origin,
            &["update-ref", "-d", &format!("refs/heads/{branch}")],
        )
        .map(drop)
        .map_err(HttpError::Decode)
    }

    fn merge(&self, number: u64, sha: &str) -> Result<MergeOutcome, HttpError> {
        self.call(format!("merge {number} {sha}"));
        let pull = self.pull_of(number)?;
        if pull.head_sha != sha {
            return Ok(MergeOutcome::Refused {
                reason: "Head branch was modified. Review and try the merge again.".into(),
            });
        }
        let base_ref = self
            .state
            .lock()
            .unwrap()
            .pulls
            .iter()
            .find(|p| p.0 == number)
            .map(|p| p.2.clone())
            .unwrap_or_default();
        let base = self.head(&base_ref);
        let tree = bare(&self.origin, &["merge-tree", "--write-tree", &base, sha])
            .map_err(|e| HttpError::Decode(format!("conflict: {e}")))?;
        let tree = tree.lines().next().unwrap_or_default().to_owned();
        let commit = bare(
            &self.origin,
            &[
                "-c",
                "user.name=GitHub",
                "-c",
                "user.email=noreply@github.test",
                "commit-tree",
                &tree,
                "-p",
                &base,
                "-p",
                sha,
                "-m",
                &format!("Merge pull request #{number}"),
            ],
        )
        .map_err(HttpError::Decode)?;
        bare(
            &self.origin,
            &["update-ref", &format!("refs/heads/{base_ref}"), &commit],
        )
        .map_err(HttpError::Decode)?;
        let mut st = self.state.lock().unwrap();
        if let Some(p) = st.pulls.iter_mut().find(|p| p.0 == number) {
            p.3 = PullState::Merged;
        }
        Ok(MergeOutcome::Merged { sha: commit })
    }
}

/// A check script where `name` fails on the first pushed commit and everything passes after.
pub fn fails_first(name: &'static str, others: &'static [&'static str]) -> CheckScript {
    Box::new(move |nth| {
        let mut checks: Vec<(String, CheckState)> = others
            .iter()
            .map(|n| ((*n).to_owned(), CheckState::Success))
            .collect();
        checks.push((
            name.to_owned(),
            if nth == 0 {
                CheckState::Failure
            } else {
                CheckState::Success
            },
        ));
        checks
    })
}
