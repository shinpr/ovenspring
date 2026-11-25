//! The exact text each launch receives, in this order: rules, role, step (whose required skills the step file names, followed by the role's selected
//! repository skills), working agreements, context, the role's additional instructions. Skills
//! themselves are not in the prompt: each CLI loads them.

use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

use ovenspring_core::config::Role;
use ovenspring_core::output::AgreementName;
use ovenspring_core::step::Step;

use crate::assets;

#[derive(Debug, Clone)]
pub struct PromptContext<'a> {
    pub role: Role,
    pub step: Step,
    /// The preparation turn of a design-phase launch.
    pub prepare: bool,
    pub item: &'a Path,
    pub worktree: &'a Path,
    pub issue: Option<&'a str>,
    pub product_goal: &'a str,
    pub sprint_purpose: Option<&'a str>,
    pub now: &'a str,
    pub agreements: Agreements,
    /// The repository's own skills the role selects.
    pub repo_skills: &'a [String],
    pub append: &'a str,
    /// The kinds of change a person reviews before they merge, by the owner's name and words;
    /// the steps that judge which a change falls under receive them.
    pub person_reviews: &'a std::collections::BTreeMap<String, String>,
    /// The knowledge folder, relative to the worktree.
    pub knowledge: &'a str,
}

/// Working agreements as they stand on the base branch.
#[derive(Debug, Clone, Default)]
pub struct Agreements {
    pub common: Option<String>,
    pub role: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum PromptError {
    #[error("the binary holds no factory file {0}")]
    Missing(String),
    #[error("reading working agreements: git {0}")]
    Git(String),
}

/// Reads `.ovenspring/agreements/<file>` from `origin/<base>`, never from the working tree: agents
/// work by the agreements the owner approved and merged. `None` when the base has no such file.
pub fn read_agreement(repo: &Path, base: &str, file: &str) -> Result<Option<String>, PromptError> {
    let path = format!(".ovenspring/agreements/{file}");
    let listed = git(
        repo,
        &[
            "ls-tree",
            "--name-only",
            &format!("origin/{base}"),
            "--",
            &path,
        ],
    )?;
    if listed.trim().is_empty() {
        return Ok(None);
    }
    git(repo, &["show", &format!("origin/{base}:{path}")]).map(Some)
}

fn git(repo: &Path, args: &[&str]) -> Result<String, PromptError> {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .map_err(|e| PromptError::Git(format!("{}: {e}", args.join(" "))))?;
    if !out.status.success() {
        return Err(PromptError::Git(format!(
            "{}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn read_agreements(repo: &Path, base: &str, role: Role) -> Result<Agreements, PromptError> {
    Ok(Agreements {
        common: read_agreement(repo, base, AgreementName::Common.as_str())?,
        role: read_agreement(repo, base, AgreementName::of(role).as_str())?,
    })
}

/// Whether a step takes its role's prompt: only a step with no source agent's definition of its
/// own does. Every other step is its source agent's definition, which is the role it plays; a role
/// prompt beside it would be a second, differing instruction for the same work.
fn takes_role_prompt(step: Step) -> bool {
    match step {
        Step::Requirements
        | Step::ScopeCheck
        | Step::Prototype
        | Step::Resolution
        | Step::RerunCheck
        | Step::Exception
        | Step::SprintPlan
        | Step::SprintReview
        | Step::Conversation
        | Step::RunCommands => true,
        Step::Facts
        | Step::AgreementsCheck
        | Step::Prd
        | Step::PrdReview
        | Step::CodebaseAnalysis
        | Step::UiAnalysis
        | Step::UiSpec
        | Step::UiSpecReview
        | Step::Adr
        | Step::AdrReview
        | Step::Design
        | Step::DesignVerify
        | Step::DesignReview
        | Step::DesignSync
        | Step::TestDesign
        | Step::WorkPlan
        | Step::PlanReview
        | Step::Decompose
        | Step::Implement
        | Step::TestReview
        | Step::Quality
        | Step::CodeReview
        | Step::SecurityReview
        | Step::ReviewResponse
        | Step::FixCi
        | Step::MergeBase => false,
    }
}

pub fn assemble(ctx: &PromptContext<'_>) -> Result<String, PromptError> {
    let read = |path: String| assets::embedded(&path).ok_or(PromptError::Missing(path));
    let rules = read("rules.md".into())?;
    let role = if takes_role_prompt(ctx.step) {
        read(format!("roles/{}.md", ctx.role.as_str()))?
    } else {
        ""
    };
    let step = read(format!("steps/{}.md", ctx.step.text_file()))?;

    let mut out = String::new();
    for part in [rules.trim(), role.trim(), step.trim()] {
        if !part.is_empty() {
            out.push_str(part);
            out.push_str("\n\n");
        }
    }
    if !ctx.repo_skills.is_empty() {
        let skills: Vec<String> = ctx.repo_skills.iter().map(|s| format!("`{s}`")).collect();
        let _ = writeln!(
            out,
            "Also load and read each of the repository's skills your role selects completely before acting on your item: {}.\n",
            skills.join(", ")
        );
    }

    if ctx.agreements.common.is_some() || ctx.agreements.role.is_some() {
        out.push_str("# Working agreements\n\nWhat the team learned about its own work, as merged on the base branch.\n\n");
        for text in [&ctx.agreements.common, &ctx.agreements.role]
            .into_iter()
            .flatten()
        {
            out.push_str(text.trim());
            out.push_str("\n\n");
        }
    }

    out.push_str("# Context\n\n");
    let _ = writeln!(out, "- Product goal: {}", one_line(ctx.product_goal));
    if let Some(p) = ctx.sprint_purpose {
        let _ = writeln!(out, "- Sprint purpose: {}", one_line(p));
    }
    let _ = writeln!(out, "- Your role: {}", ctx.role.as_str());
    let _ = writeln!(out, "- Your item: {}", ctx.item.display());
    if ctx.prepare {
        out.push_str("- Your turn: prepare (Factory Rules, Talking with the owner)\n");
    }
    let _ = writeln!(out, "- Worktree: {}", ctx.worktree.display());
    let _ = writeln!(out, "- Knowledge: `{}` in the worktree", ctx.knowledge);
    if let Some(issue) = ctx.issue {
        let _ = writeln!(out, "- Issue: {issue}");
    }
    let _ = writeln!(out, "- Current time: {}", ctx.now);
    out.push('\n');

    if matches!(ctx.step, Step::Design | Step::CodeReview) && !ctx.person_reviews.is_empty() {
        out.push_str(
            "# Changes a person reviews

The owner names these kinds of change; a person reviews a change of one of them before it merges.

",
        );
        for (name, words) in ctx.person_reviews {
            let _ = writeln!(out, "- `{name}`: {}", one_line(words));
        }
        out.push('\n');
    }

    if !ctx.append.trim().is_empty() {
        out.push_str("# Additional instructions\n\n");
        out.push_str(ctx.append.trim());
        out.push('\n');
    }
    Ok(out)
}

/// The next turn of a resumed session: the work turn's item, which the session reads again.
pub fn continuation(item: &Path) -> String {
    format!(
        "# Your work turn\n\nThis session continues with the work. Read your item again: its request, its refs (the owner's answers among them) and its `out` are this turn's. Carry out your item as your step file describes.\n\n- Your item: {}\n",
        item.display()
    )
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context<'a>(
        worktree: &'a Path,
        agreements: Agreements,
        repo_skills: &'a [String],
        append: &'a str,
    ) -> PromptContext<'a> {
        PromptContext {
            role: Role::Executor,
            step: Step::Implement,
            prepare: false,
            item: Path::new("/s/board/active/20261121-120400-implement-ACME-142.md"),
            worktree,
            issue: Some("ACME-142"),
            product_goal: "Shop customers manage their own orders\nwithout contacting support.",
            sprint_purpose: Some("Customers can export their order history"),
            now: "2026-11-21 12:04",
            agreements,
            repo_skills,
            append,
            person_reviews: &NO_PERSON_REVIEWS,
            knowledge: ".ovenspring/knowledge",
        }
    }

    static NO_PERSON_REVIEWS: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();

    #[test]
    fn the_parts_come_in_order_with_the_owners_instructions_last() {
        let agreements = Agreements {
            common: Some("# Working agreements\n- Name the precedent.".into()),
            role: None,
        };
        let skills = vec!["payments-domain".to_string()];
        let text = assemble(&context(
            Path::new("/w/ACME-142"),
            agreements,
            &skills,
            "Run the app with make dev.",
        ))
        .unwrap();

        let at = |needle: &str| {
            text.find(needle)
                .unwrap_or_else(|| panic!("missing {needle:?}"))
        };
        let order = [
            at("# Factory Rules"),
            at("# Step: implement"),
            at(
                "Load and read each skill completely before acting on your item: `ovenspring-coding-principles`",
            ),
            at(
                "Also load and read each of the repository's skills your role selects completely before acting on your item: `payments-domain`.",
            ),
            at("Name the precedent."),
            at(
                "- Product goal: Shop customers manage their own orders without contacting support.",
            ),
            at("- Sprint purpose: Customers can export their order history"),
            at("- Knowledge: `.ovenspring/knowledge` in the worktree"),
            at("Run the app with make dev."),
        ];
        assert!(
            order.windows(2).all(|w| w[0] < w[1]),
            "out of order: {order:?}"
        );
        assert!(text.trim_end().ends_with("Run the app with make dev."));
        assert!(!text.contains("SKILL.md"), "no paths to skill files");
        assert!(
            !text.contains("# Executor"),
            "a step with its source agent's definition carries no role prompt"
        );
    }

    #[test]
    fn a_step_without_a_source_definition_takes_its_roles_prompt() {
        let mut ctx = context(Path::new("/w/ACME-142"), Agreements::default(), &[], "");
        ctx.role = Role::Lead;
        ctx.step = Step::Resolution;
        let text = assemble(&ctx).unwrap();
        assert!(text.find("# Lead").unwrap() < text.find("# Step: resolution").unwrap());
    }
}
