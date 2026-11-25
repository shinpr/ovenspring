//! The guard rules `ovenspring hook` applies to a factory launch's shell commands.
//!
//! A command line is split into the commands it chains (at `;`, `&&`, `||`, `|` and newlines,
//! outside quotes), and each is checked after its leading environment assignments are removed.
//! Hooks catch honest mistakes; they do not chase other spellings of the same action, because the
//! boundary against production is keeping production credentials off the machine.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The per-launch guard configuration the daemon writes for the hook.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GuardRules {
    /// Glob patterns (`*` matches any text) matched against each command.
    pub deny: Vec<String>,
    /// Paths no tool call may name: the isolated directory and the credentials file.
    pub protected: Vec<PathBuf>,
}

/// Why a command is denied, written to the agent.
pub fn check(command: &str, rules: &GuardRules) -> Result<(), String> {
    for words in chained_commands(command) {
        check_one(&words, rules)?;
    }
    Ok(())
}

fn check_one(words: &[String], rules: &GuardRules) -> Result<(), String> {
    let first = words
        .iter()
        .position(|w| !is_assignment(w))
        .unwrap_or(words.len());
    let words = words.get(first..).unwrap_or_default();
    let Some((program, args)) = words.split_first() else {
        return Ok(());
    };
    let name = program.rsplit('/').next().unwrap_or(program);

    let joined = words.join(" ");
    for pattern in &rules.deny {
        if ovenspring_core::wildcard::matches(pattern, &joined) {
            return Err(format!(
                "`{joined}` matches the repository's guard rule `{pattern}`; the factory does not run it. \
                 If the step needs it, set this item blocked and name the command."
            ));
        }
    }

    match name {
        "git" => check_git(args),
        "gh" => check_gh(args),
        _ => Ok(()),
    }
}

/// How the owner reads an always-denied entry: the command as an agent types it, or the action in
/// words where it has many spellings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(tag = "kind", content = "text", rename_all = "lowercase")]
pub enum Shown {
    Command(&'static str),
    Action(&'static str),
}

/// What no factory launch may run, whatever the owner configured: how the owner reads it, the
/// wildcard patterns OpenCode's permissions carry (it has no hook; `check_git` and `check_gh` are
/// the hook's form), and commands each form must deny.
pub struct AlwaysDenied {
    pub shown: Shown,
    pub patterns: &'static [&'static str],
    pub examples: &'static [&'static str],
}

pub const ALWAYS_DENIED: [AlwaysDenied; 5] = [
    AlwaysDenied {
        shown: Shown::Command("git push"),
        // Options before the subcommand (`-C <dir>`, `-c <name=value>`) are spelled freely.
        patterns: &["git push*", "git -* push*"],
        examples: &[
            "git push origin ovenspring/ACME-1",
            "git -C /tmp/w push --force",
        ],
    },
    AlwaysDenied {
        shown: Shown::Command("git commit"),
        patterns: &["git commit*", "git -* commit*"],
        examples: &["git commit -m change", "git -c user.name=a commit --amend"],
    },
    AlwaysDenied {
        shown: Shown::Command("gh pr merge"),
        patterns: &["gh pr merge*"],
        examples: &["gh pr merge 12 --squash"],
    },
    AlwaysDenied {
        shown: Shown::Action("approving a pull request"),
        patterns: &["gh pr review*--approve*", "gh pr review* -a*"],
        examples: &["gh pr review 12 --approve", "gh pr review 12 -a"],
    },
    AlwaysDenied {
        shown: Shown::Action("GitHub's merge and review endpoints"),
        patterns: &["gh api*/merge*", "gh api*/reviews*"],
        examples: &[
            "gh api -X PUT repos/acme/app/pulls/12/merge",
            "gh api repos/acme/app/pulls/12/reviews",
        ],
    },
];

/// Every way a launch's input may name `path`: as written, under `~`, `$HOME` or `${HOME}`.
pub(crate) fn spellings_of(path: &std::path::Path) -> Vec<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let full = path.display().to_string();
    let mut spellings = vec![full.clone()];
    if let Some(rest) = full.strip_prefix(&home).filter(|_| !home.is_empty()) {
        spellings.push(format!("~{rest}"));
        spellings.push(format!("$HOME{rest}"));
        spellings.push(format!("${{HOME}}{rest}"));
    }
    spellings
}

fn check_git(args: &[String]) -> Result<(), String> {
    let mut rest = args.iter();
    let mut sub = None;
    while let Some(a) = rest.next() {
        match a.as_str() {
            "-C" | "-c" | "--git-dir" | "--work-tree" | "--namespace" | "--exec-path" => {
                rest.next();
            }
            s if s.starts_with('-') => {}
            s => {
                sub = Some(s);
                break;
            }
        }
    }
    match sub {
        Some("push") => Err(
            "agents do not push; the factory pushes the issue branch when the step's work is done. \
             Finish the item."
                .into(),
        ),
        Some("commit") => Err(
            "agents do not commit; the factory commits the worktree once the check after your step \
             passes. Leave your changes in the worktree and finish the item."
                .into(),
        ),
        _ => Ok(()),
    }
}

fn check_gh(args: &[String]) -> Result<(), String> {
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    let deny = |what: &str| {
        Err(format!(
            "agents do not {what}; the factory's merge gate does"
        ))
    };
    match a.as_slice() {
        ["pr", "merge", ..] => deny("merge pull requests"),
        ["pr", "review", rest @ ..] if rest.iter().any(|x| matches!(*x, "--approve" | "-a")) => {
            deny("approve pull requests")
        }
        ["api", rest @ ..]
            if rest
                .iter()
                .any(|x| x.contains("/merge") || x.contains("/reviews")) =>
        {
            deny("merge or review through the API")
        }
        _ => Ok(()),
    }
}

fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty()
            && name.chars().all(|c| c == '_' || c.is_ascii_alphanumeric())
            && !name.starts_with(|c: char| c.is_ascii_digit())
    })
}

/// Splits a command line into the words of each command it chains, keeping quoted text whole.
pub fn chained_commands(line: &str) -> Vec<Vec<String>> {
    let mut commands = Vec::new();
    let mut words: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut chars = line.chars();
    let end_word = |words: &mut Vec<String>, word: &mut String, in_word: &mut bool| {
        if *in_word {
            words.push(std::mem::take(word));
            *in_word = false;
        }
    };
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_word = true;
                for q in chars.by_ref() {
                    if q == '\'' {
                        break;
                    }
                    word.push(q);
                }
            }
            '"' => {
                in_word = true;
                while let Some(q) = chars.next() {
                    match q {
                        '"' => break,
                        '\\' => word.extend(chars.next()),
                        _ => word.push(q),
                    }
                }
            }
            '\\' => {
                in_word = true;
                word.extend(chars.next().filter(|n| *n != '\n'));
            }
            ';' | '&' | '|' | '\n' => {
                end_word(&mut words, &mut word, &mut in_word);
                if !words.is_empty() {
                    commands.push(std::mem::take(&mut words));
                }
            }
            c if c.is_whitespace() => end_word(&mut words, &mut word, &mut in_word),
            _ => {
                in_word = true;
                word.push(c);
            }
        }
    }
    end_word(&mut words, &mut word, &mut in_word);
    if !words.is_empty() {
        commands.push(words);
    }
    commands
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opencode_denies_what_the_hook_denies() {
        for denied in &ALWAYS_DENIED {
            for command in denied.examples {
                assert!(check(command, &GuardRules::default()).is_err(), "{command}");
                assert!(
                    denied
                        .patterns
                        .iter()
                        .any(|p| ovenspring_core::wildcard::matches(p, command)),
                    "{command}"
                );
            }
        }
    }

    fn rules() -> GuardRules {
        GuardRules {
            deny: vec![
                "kubectl * --context *prod*".into(),
                "terraform apply*".into(),
            ],
            protected: vec![],
        }
    }

    fn denied(cmd: &str) -> bool {
        check(cmd, &rules()).is_err()
    }

    #[test]
    fn every_guarded_operation_is_denied() {
        for cmd in [
            "git push",
            "git push origin ovenspring/ACME-142",
            "git -C /tmp/w push --force",
            "git commit -m 'Add the export'",
            "git -c user.name=a commit --amend",
            "gh pr merge 318 --squash",
            "gh pr review 318 --approve",
            "gh api -X PUT repos/acme/storefront/pulls/318/merge",
            "kubectl get pods --context acme-prod",
            "terraform apply -auto-approve",
        ] {
            assert!(denied(cmd), "{cmd} should be denied");
        }
    }

    #[test]
    fn a_guarded_command_chained_or_after_assignments_is_denied() {
        for cmd in [
            "cargo test && git push",
            "cargo test; git push",
            "cargo test || git push",
            "echo ok | git push",
            "cargo test\ngit push",
            "GIT_TRACE=1 git push",
            "/usr/bin/git push",
        ] {
            assert!(denied(cmd), "{cmd} should be denied");
        }
    }

    #[test]
    fn commands_that_only_resemble_guarded_ones_are_allowed() {
        for cmd in [
            "git log --oneline",
            "git status",
            "git log --grep 'git commit'",
            "gh pr view 318",
            "gh api repos/acme/storefront/pulls/318/comments",
            "git branch -d ovenspring/old-ACME-1",
            "grep -rn 'git push' docs/",
            "git merge --no-edit origin/develop",
            "echo \"gh pr merge\"",
            "ovenspring item set /s/board/active/x.md status=done",
            "ovenspring app run -- pnpm e2e",
            "kubectl get pods --context acme-staging",
        ] {
            assert!(!denied(cmd), "{cmd} should be allowed");
        }
    }
}
