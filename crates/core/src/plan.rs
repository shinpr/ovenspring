//! The task files `decompose` writes: the order their dependencies allow, and the
//! post-implementation review units built from those dependencies.

use std::collections::BTreeSet;

use crate::flow::PlannedTask;

/// The metadata a task file carries: its Work Plan task and the tasks it depends on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskMeta {
    pub id: String,
    pub dependencies: Vec<String>,
}

/// Reads the `Metadata:` lines of a task file.
pub fn read_task(text: &str) -> Result<TaskMeta, String> {
    let field = |name: &str| {
        text.lines()
            .map(str::trim)
            .find_map(|l| l.strip_prefix("- ")?.strip_prefix(name)?.strip_prefix(':'))
            .map(|v| v.trim().trim_matches(['[', ']']).trim().to_owned())
            .ok_or_else(|| format!("the task file has no {name} line"))
    };
    let id = field("Source Work Plan Task")?;
    if id.is_empty() {
        return Err("Source Work Plan Task is empty".into());
    }
    let deps = field("Dependencies")?;
    let dependencies = if deps.eq_ignore_ascii_case("none") || deps.is_empty() {
        vec![]
    } else {
        deps.split(',')
            .map(|d| d.trim().trim_matches(['`', '[', ']']).trim().to_owned())
            .filter(|d| !d.is_empty())
            .collect()
    };
    Ok(TaskMeta { id, dependencies })
}

/// The tasks in an order their dependencies allow (keeping the given order where they do not
/// constrain it), with each task's dependencies as indexes into that order.
pub fn order(tasks: &[(String, TaskMeta)]) -> Result<(Vec<PlannedTask>, Vec<Vec<usize>>), String> {
    let position = |id: &str| tasks.iter().position(|(_, t)| t.id == id);
    let mut deps_of: Vec<Vec<usize>> = Vec::new();
    for (path, t) in tasks {
        let mut deps = Vec::new();
        for d in &t.dependencies {
            deps.push(position(d).ok_or_else(|| format!("{path} depends on unknown task {d}"))?);
        }
        deps_of.push(deps);
    }
    let mut placed: Vec<usize> = Vec::new();
    while placed.len() < tasks.len() {
        let ready = (0..tasks.len()).find(|i| {
            !placed.contains(i)
                && deps_of
                    .get(*i)
                    .is_some_and(|deps| deps.iter().all(|d| placed.contains(d)))
        });
        match ready {
            Some(i) => placed.push(i),
            None => return Err("the task files' dependencies form a cycle".into()),
        }
    }
    let new_index = |old: usize| placed.iter().position(|p| *p == old).unwrap_or(old);
    let mut planned = Vec::new();
    let mut deps = Vec::new();
    for old in &placed {
        if let (Some((path, _)), Some(d)) = (tasks.get(*old), deps_of.get(*old)) {
            planned.push(PlannedTask { path: path.clone() });
            deps.push(d.iter().map(|x| new_index(*x)).collect());
        }
    }
    Ok((planned, deps))
}

/// The review units of a large change: each task no other task depends on, with every task it
/// reaches through its dependencies; tasks several units reach form one shared unit.
pub fn review_units(deps: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let depended: BTreeSet<usize> = deps.iter().flatten().copied().collect();
    let closures: Vec<BTreeSet<usize>> = (0..deps.len())
        .filter(|t| !depended.contains(t))
        .map(|sink| {
            let mut seen = BTreeSet::from([sink]);
            let mut stack = vec![sink];
            while let Some(t) = stack.pop() {
                for d in deps.get(t).into_iter().flatten() {
                    if seen.insert(*d) {
                        stack.push(*d);
                    }
                }
            }
            seen
        })
        .collect();
    let shared: BTreeSet<usize> = (0..deps.len())
        .filter(|t| closures.iter().filter(|c| c.contains(t)).count() > 1)
        .collect();
    let mut units: Vec<Vec<usize>> = closures
        .iter()
        .map(|c| c.difference(&shared).copied().collect::<Vec<_>>())
        .filter(|u| !u.is_empty())
        .collect();
    if !shared.is_empty() {
        units.push(shared.into_iter().collect());
    }
    units
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, deps: &str) -> String {
        format!(
            "# Task: {id}\n\nMetadata:\n- Source Work Plan Task: {id}\n- Dependencies: {deps}\n\n## Implementation Outcome\n"
        )
    }

    #[test]
    fn a_task_files_metadata_is_read_from_the_template_lines() {
        let meta = read_task(&task("P2-T1", "P1-T1, P1-T2")).unwrap();
        assert_eq!(
            meta,
            TaskMeta {
                id: "P2-T1".into(),
                dependencies: vec!["P1-T1".into(), "P1-T2".into()],
            }
        );
        assert_eq!(
            read_task(&task("P1-T1", "none")).unwrap().dependencies,
            Vec::<String>::new()
        );
        assert!(read_task("# Task\n- Dependencies: none\n").is_err());
    }

    #[test]
    fn tasks_are_ordered_so_each_follows_what_it_depends_on() {
        let tasks: Vec<(String, TaskMeta)> = [
            ("b", "P1-T2", "P1-T1"),
            ("a", "P1-T1", "none"),
            ("c", "P1-T3", "none"),
        ]
        .iter()
        .map(|(p, id, d)| ((*p).to_owned(), read_task(&task(id, d)).unwrap()))
        .collect();
        let (planned, deps) = order(&tasks).unwrap();
        let paths: Vec<&str> = planned.iter().map(|t| t.path.as_str()).collect();
        assert_eq!(paths, ["a", "b", "c"]);
        assert_eq!(deps, vec![vec![], vec![0], vec![]]);

        let cycle: Vec<(String, TaskMeta)> = [("a", "T1", "T2"), ("b", "T2", "T1")]
            .iter()
            .map(|(p, id, d)| ((*p).to_owned(), read_task(&task(id, d)).unwrap()))
            .collect();
        assert!(order(&cycle).is_err());
        let unknown = vec![("a".to_owned(), read_task(&task("T1", "T9")).unwrap())];
        assert!(order(&unknown).is_err());
    }

    #[test]
    fn each_consuming_task_forms_a_unit_with_its_dependencies_and_shared_ones_form_their_own() {
        // 0: shared model; 1: route on 0; 2: screen on 0; 3: job on its own helper 4.
        let deps = vec![vec![], vec![0], vec![0], vec![4], vec![]];
        assert_eq!(
            review_units(&deps),
            vec![vec![1], vec![2], vec![3, 4], vec![0]]
        );
        assert_eq!(review_units(&[vec![], vec![0]]), vec![vec![0, 1]]);
    }
}
