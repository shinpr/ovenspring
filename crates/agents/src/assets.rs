//! The factory's own rules, role files, step files and skills, embedded in the binary. Prompts
//! are assembled from the embedded text; the skills are written into each
//! working directory a launch runs in, where each CLI loads them as skills.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

// `FACTORY`: every file under `factory/`, by its path relative to it (written by build.rs).
include!(concat!(env!("OUT_DIR"), "/factory.rs"));

/// Every factory skill's directory and `name` start with this, so it never takes the place of a
/// repository skill of the same name.
pub const SKILL_PREFIX: &str = "ovenspring-";

/// Where the factory writes its skills: Codex and OpenCode read the first, Claude Code the second.
pub const SKILL_ROOTS: [&str; 2] = [".agents/skills", ".claude/skills"];

/// Keeps a skill directory, this file included, out of every commit without touching the
/// repository's own ignore files.
const IGNORE_ALL: &[u8] = b"*\n";

/// The embedded file at `path` (relative to `factory/`), if any.
pub fn embedded(path: &str) -> Option<&'static str> {
    FACTORY
        .iter()
        .find(|(p, _)| *p == path)
        .and_then(|(_, contents)| std::str::from_utf8(contents).ok())
}

/// Each factory skill's name with its files, by their paths inside the skill's directory.
fn skills() -> BTreeMap<&'static str, Vec<(&'static str, &'static [u8])>> {
    let mut skills: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for (path, contents) in FACTORY {
        if let Some((name, file)) = path.strip_prefix("skills/").and_then(|p| p.split_once('/')) {
            skills.entry(name).or_default().push((file, *contents));
        }
    }
    skills
}

/// Writes every factory skill into `dir` under each of [`SKILL_ROOTS`], each skill directory
/// holding a `.gitignore` of `*`. Files already current are left alone, so a launch already
/// reading them is not disturbed; factory skills and files this binary no longer has are removed.
pub fn install_skills(dir: &Path) -> io::Result<()> {
    let skills = skills();
    if skills.is_empty() {
        return Err(io::Error::other("the binary holds no factory skills"));
    }
    for root in SKILL_ROOTS {
        let root = dir.join(root);
        fs::create_dir_all(&root)?;
        for entry in fs::read_dir(&root)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(SKILL_PREFIX) && !skills.contains_key(name.as_str()) {
                fs::remove_dir_all(entry.path())?;
            }
        }
        for (name, files) in &skills {
            install_skill(files, &root.join(name))?;
        }
    }
    Ok(())
}

/// A repository skill a launch switches off: its file (Codex switches skills off by path) and
/// the names it goes by, its directory's and its `name` (Claude Code switches them off by name).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoSkill {
    pub file: PathBuf,
    pub names: Vec<String>,
}

/// The repository's own skills under `dir` that `selected` does not name, factory skills aside.
pub fn unselected_skills(dir: &Path, selected: &[String]) -> io::Result<Vec<RepoSkill>> {
    let mut out = Vec::new();
    for root in SKILL_ROOTS {
        let root = dir.join(root);
        if !root.is_dir() {
            continue;
        }
        let mut entries: Vec<PathBuf> = fs::read_dir(&root)?
            .map(|e| e.map(|e| e.path()))
            .collect::<io::Result<_>>()?;
        entries.sort();
        for path in entries {
            let name = base_name(&path);
            let file = path.join("SKILL.md");
            if name.starts_with(SKILL_PREFIX) || selected.contains(&name) || !file.is_file() {
                continue;
            }
            let mut names = vec![name];
            let declared = fs::read_to_string(&file)?
                .lines()
                .find_map(|l| l.strip_prefix("name:"))
                .map(|n| n.trim().trim_matches('"').to_owned());
            if let Some(n) = declared.filter(|n| !n.is_empty() && !names.contains(n)) {
                names.push(n);
            }
            out.push(RepoSkill {
                file: fs::canonicalize(&file)?,
                names,
            });
        }
    }
    Ok(out)
}

/// A repository skill the owner can give a role, as its `SKILL.md` describes it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
}

/// The repository's own skills under `roots` of `dir`, factory skills aside, each name once.
pub fn repo_skills(dir: &Path, roots: &[&str]) -> io::Result<Vec<SkillInfo>> {
    let mut out: Vec<SkillInfo> = Vec::new();
    for root in roots {
        let root = dir.join(root);
        if !root.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&root)? {
            let path = entry?.path();
            let name = base_name(&path);
            let file = path.join("SKILL.md");
            if name.starts_with(SKILL_PREFIX)
                || !file.is_file()
                || out.iter().any(|s| s.name == name)
            {
                continue;
            }
            let description = description_of(&fs::read_to_string(&file)?);
            out.push(SkillInfo { name, description });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// The frontmatter's `description`, on its line or folded over the indented lines after it.
fn description_of(skill: &str) -> String {
    let mut lines = skill.lines().skip_while(|l| !l.starts_with("description:"));
    let Some(first) = lines.next().and_then(|l| l.strip_prefix("description:")) else {
        return String::new();
    };
    let first = first.trim();
    if !matches!(first, "" | "|" | ">" | "|-" | ">-") {
        return first.trim_matches('"').to_owned();
    }
    lines
        .take_while(|l| l.starts_with(' ') || l.starts_with('\t'))
        .map(str::trim)
        .collect::<Vec<_>>()
        .join(" ")
}

fn install_skill(files: &[(&str, &'static [u8])], target: &Path) -> io::Result<()> {
    let mut wanted = vec![(PathBuf::from(".gitignore"), IGNORE_ALL)];
    wanted.extend(files.iter().map(|(p, c)| (PathBuf::from(p), *c)));
    for (relative, contents) in &wanted {
        let path = target.join(relative);
        if fs::read(&path).ok().as_deref() != Some(*contents) {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&path, contents)?;
        }
    }
    let kept: BTreeSet<&PathBuf> = wanted.iter().map(|(p, _)| p).collect();
    for path in files_under(target)? {
        let relative = path.strip_prefix(target).map_err(io::Error::other)?;
        if !kept.contains(&relative.to_path_buf()) {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}

fn files_under(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            out.extend(files_under(&path)?);
        } else {
            out.push(path);
        }
    }
    Ok(out)
}

fn base_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_description_is_read_on_its_line_or_folded_below_it() {
        let cases = [
            (
                "---\nname: a\ndescription: Payment states and refunds\n---\n",
                "Payment states and refunds",
            ),
            (
                "---\ndescription: \"REST conventions\"\n---\n",
                "REST conventions",
            ),
            (
                "---\ndescription: >\n  Writing reversible\n  migrations\nname: m\n---\n",
                "Writing reversible migrations",
            ),
            ("---\nname: none\n---\n", ""),
        ];
        for (skill, expected) in cases {
            assert_eq!(description_of(skill), expected, "{skill}");
        }
    }
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}");
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    #[test]
    fn skills_written_into_a_worktree_are_loadable_and_never_committed() {
        let repo = tempfile::tempdir().unwrap();
        let dir = repo.path();
        git(dir, &["init", "-q", "-b", "main"]);
        let own = dir.join(".agents/skills/payments-domain");
        fs::create_dir_all(&own).unwrap();
        fs::write(own.join("SKILL.md"), "---\nname: payments-domain\n---\n").unwrap();
        let stale = dir.join(".claude/skills/ovenspring-retired");
        fs::create_dir_all(&stale).unwrap();
        fs::write(stale.join("SKILL.md"), "old").unwrap();

        install_skills(dir).unwrap();

        for root in SKILL_ROOTS {
            let skill = dir.join(root).join("ovenspring-coding-principles/SKILL.md");
            assert_eq!(
                fs::read_to_string(&skill).unwrap(),
                embedded("skills/ovenspring-coding-principles/SKILL.md").unwrap()
            );
        }
        assert!(
            !stale.exists(),
            "a skill the binary no longer has is removed"
        );
        git(dir, &["add", "-A"]);
        assert_eq!(
            git(dir, &["diff", "--cached", "--name-only"]).trim(),
            ".agents/skills/payments-domain/SKILL.md",
            "only the repository's own skill is staged"
        );
    }

    #[test]
    fn a_changed_or_extra_file_in_a_factory_skill_is_put_back() {
        let dir = tempfile::tempdir().unwrap();
        install_skills(dir.path()).unwrap();
        let skill = dir
            .path()
            .join(".agents/skills/ovenspring-testing-principles");
        fs::write(skill.join("SKILL.md"), "tampered").unwrap();
        fs::write(skill.join("notes.md"), "left by an agent").unwrap();

        install_skills(dir.path()).unwrap();

        assert_eq!(
            fs::read_to_string(skill.join("SKILL.md")).unwrap(),
            embedded("skills/ovenspring-testing-principles/SKILL.md").unwrap()
        );
        assert!(!skill.join("notes.md").exists());
    }

    #[test]
    fn repository_skills_the_role_does_not_select_are_listed_by_file_and_names() {
        let dir = tempfile::tempdir().unwrap();
        let skill = |root: &str, name: &str, declared: &str| {
            let d = dir.path().join(root).join(name);
            fs::create_dir_all(&d).unwrap();
            fs::write(d.join("SKILL.md"), format!("---\nname: {declared}\n---\n")).unwrap();
        };
        skill(".agents/skills", "payments-domain", "payments-domain");
        skill(".agents/skills", "release", "release");
        skill(".claude/skills", "deploy", "ship-it");
        install_skills(dir.path()).unwrap();

        let off = unselected_skills(dir.path(), &["payments-domain".into()]).unwrap();

        let real = fs::canonicalize(dir.path()).unwrap();
        assert_eq!(
            off,
            [
                RepoSkill {
                    file: real.join(".agents/skills/release/SKILL.md"),
                    names: vec!["release".into()],
                },
                RepoSkill {
                    file: real.join(".claude/skills/deploy/SKILL.md"),
                    names: vec!["deploy".into(), "ship-it".into()],
                },
            ]
        );
    }

    #[test]
    fn every_factory_skill_is_named_as_its_directory() {
        let skills = skills();
        assert!(!skills.is_empty());
        for dir in skills.keys() {
            assert!(dir.starts_with(SKILL_PREFIX), "{dir}");
            let text = embedded(&format!("skills/{dir}/SKILL.md")).unwrap();
            assert!(text.contains(&format!("\nname: {dir}\n")), "{dir}");
        }
    }
}
