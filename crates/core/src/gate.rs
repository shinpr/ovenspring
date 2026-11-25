//! The merge gate: merge, return, or hold, decided only from
//! what GitHub and the approval records show about one exact commit. A wrong merge cannot be taken
//! back, so nothing here is a judgment.

use std::collections::BTreeSet;

use crate::config::MergeBy;
use crate::output::Approval;

/// Everything the gate looks at, observed for the pull request's current head, whoever pushed it.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    /// The pull request's head as GitHub reports it now.
    pub head_sha: String,
    /// Whether every required check succeeded on `head_sha`; `None` while any is still running.
    pub checks_passed: Option<bool>,
    /// Whether the branch contains the base branch's current head.
    pub contains_base: bool,
    /// Whether the branch merges into the base without conflict.
    pub mergeable: bool,
    pub reviews: Reviews,
    /// GitHub reports that a rule of the base still stops the merge, whatever the factory can
    /// see of it.
    pub github_blocks: bool,
    /// Categories the design declared or the changed paths fall under.
    pub held: BTreeSet<Approval>,
    /// Categories the owner approved for exactly `head_sha`.
    pub owner_approved: BTreeSet<Approval>,
    pub merge_by: MergeBy,
}

/// Where the pull request's reviews stand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reviews {
    /// No review stops the merge.
    Settled,
    /// GitHub reports a required approving review missing.
    ApprovalMissing,
    /// A reviewer's latest deciding review requests changes.
    ChangesRequested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Return {
    /// The branch is behind or conflicts with the base: push again, which brings the base in.
    Push,
    /// Checks are still running or failed on this head.
    Ci,
    /// Approvals are missing or changes were requested.
    Approvals,
}

/// What the gate does with one view.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// Every condition holds on this head: the daemon merges exactly this commit, after reading
    /// the base and head again.
    Merge(String),
    /// The pull request waits.
    Wait(Wait),
}

/// Why a pull request is not merged now.
#[derive(Debug, Clone, PartialEq)]
pub enum Wait {
    Return(Return),
    /// Waiting for the owner to approve these categories for this head.
    HoldForOwner(BTreeSet<Approval>),
    /// `merge.by = owner`: everything holds; the owner merges.
    ReadyForOwner,
    /// Everything the factory checks holds, but GitHub still stops the merge: a required check
    /// not reported yet, or a rule the factory cannot read. The gate looks again on each poll.
    GitHubBlocks,
}

pub fn decide(v: &View) -> Verdict {
    let wait = if !v.contains_base || !v.mergeable {
        Wait::Return(Return::Push)
    } else if v.checks_passed != Some(true) {
        Wait::Return(Return::Ci)
    } else if v.reviews != Reviews::Settled {
        Wait::Return(Return::Approvals)
    } else if v.github_blocks {
        Wait::GitHubBlocks
    } else if v.merge_by == MergeBy::Owner {
        Wait::ReadyForOwner
    } else {
        let missing: BTreeSet<Approval> = v.held.difference(&v.owner_approved).cloned().collect();
        if missing.is_empty() {
            return Verdict::Merge(v.head_sha.clone());
        }
        Wait::HoldForOwner(missing)
    };
    Verdict::Wait(wait)
}

/// Whether a changed path matches a held path pattern (`*` within a segment, `**` across segments).
pub fn path_matches(pattern: &str, path: &str) -> bool {
    fn go(p: &[&str], s: &[&str]) -> bool {
        match (p.split_first(), s.split_first()) {
            (None, None) => true,
            (Some((&"**", rest)), _) => {
                go(rest, s) || s.split_first().is_some_and(|(_, tail)| go(p, tail))
            }
            (Some((seg, rest)), Some((name, tail))) => {
                crate::wildcard::matches(seg, name) && go(rest, tail)
            }
            _ => false,
        }
    }
    let p: Vec<&str> = pattern.split('/').filter(|x| !x.is_empty()).collect();
    let s: Vec<&str> = path.split('/').filter(|x| !x.is_empty()).collect();
    go(&p, &s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> View {
        View {
            head_sha: "8c41e2a".into(),
            checks_passed: Some(true),
            contains_base: true,
            mergeable: true,
            reviews: Reviews::Settled,
            github_blocks: false,
            held: BTreeSet::new(),
            owner_approved: BTreeSet::new(),
            merge_by: MergeBy::Factory,
        }
    }

    #[test]
    fn every_condition_on_the_same_commit_merges_that_commit() {
        assert_eq!(decide(&view()), Verdict::Merge("8c41e2a".into()));
    }

    #[test]
    fn each_unmet_condition_returns_to_the_step_that_can_meet_it() {
        type Change = fn(&mut View);
        let cases: Vec<(Change, Wait)> = vec![
            (|v| v.contains_base = false, Wait::Return(Return::Push)),
            (|v| v.mergeable = false, Wait::Return(Return::Push)),
            (|v| v.checks_passed = None, Wait::Return(Return::Ci)),
            (|v| v.checks_passed = Some(false), Wait::Return(Return::Ci)),
            (
                |v| v.reviews = Reviews::ApprovalMissing,
                Wait::Return(Return::Approvals),
            ),
            (
                |v| v.reviews = Reviews::ChangesRequested,
                Wait::Return(Return::Approvals),
            ),
        ];
        for (change, expected) in cases {
            let mut v = view();
            change(&mut v);
            assert_eq!(decide(&v), Verdict::Wait(expected));
        }
    }

    #[test]
    fn a_held_category_waits_for_the_owners_approval_of_this_commit() {
        let mut v = view();
        v.held = [Approval::Schema].into();
        assert_eq!(
            decide(&v),
            Verdict::Wait(Wait::HoldForOwner([Approval::Schema].into()))
        );
        v.owner_approved = [Approval::Schema].into();
        assert_eq!(decide(&v), Verdict::Merge("8c41e2a".into()));
    }

    #[test]
    fn what_github_still_blocks_is_neither_merged_nor_handed_to_the_owner() {
        for merge_by in [MergeBy::Factory, MergeBy::Owner] {
            let mut v = view();
            v.merge_by = merge_by;
            v.held = [Approval::Schema].into();
            v.github_blocks = true;
            assert_eq!(
                decide(&v),
                Verdict::Wait(Wait::GitHubBlocks),
                "{merge_by:?}"
            );
        }
    }

    #[test]
    fn under_owner_merging_the_factory_never_merges() {
        let mut v = view();
        v.merge_by = MergeBy::Owner;
        v.held = [Approval::Schema].into();
        assert_eq!(decide(&v), Verdict::Wait(Wait::ReadyForOwner));
    }

    #[test]
    fn held_path_patterns_match_whole_segments() {
        assert!(path_matches(
            "db/migrations/**",
            "db/migrations/2026/001_export_jobs.sql"
        ));
        assert!(path_matches("**/schema.prisma", "api/prisma/schema.prisma"));
        assert!(path_matches(
            ".github/workflows/**",
            ".github/workflows/ci.yml"
        ));
        assert!(!path_matches("db/migrations/**", "db/migrations_old/x.sql"));
        assert!(path_matches("infra/*.tf", "infra/main.tf"));
        assert!(!path_matches("infra/*.tf", "infra/modules/main.tf"));
    }
}
