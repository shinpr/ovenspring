//! A board item: one step for one agent launch.
//!
//! The daemon writes the front matter and the request; the agent writes prose under `## Result`
//! and changes only `status`, through `ovenspring item set`.

use crate::step::Step;
use std::fmt;
use std::str::FromStr;

use crate::config::Role;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Open,
    Doing,
    Done,
    Blocked,
    Failed,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Open => "open",
            Status::Doing => "doing",
            Status::Done => "done",
            Status::Blocked => "blocked",
            Status::Failed => "failed",
        }
    }
}

impl FromStr for Status {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        Ok(match s {
            "open" => Status::Open,
            "doing" => Status::Doing,
            "done" => Status::Done,
            "blocked" => Status::Blocked,
            "failed" => Status::Failed,
            _ => return Err(format!("unknown status {s:?}")),
        })
    }
}

impl FromStr for Role {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        Role::ALL
            .into_iter()
            .find(|r| r.as_str() == s)
            .ok_or_else(|| format!("unknown role {s:?}"))
    }
}

/// The front matter of an item. Field order is the order it is written in.
#[derive(Debug, Clone, PartialEq)]
pub struct Front {
    pub step: Step,
    pub to: Role,
    pub status: Status,
    pub issue: Option<String>,
    pub sprint: Option<String>,
    pub round: Option<u32>,
    /// A code review's unit of a large change.
    pub unit: Option<usize>,
    pub refs: Vec<String>,
    pub out: String,
    /// The preparation turn of a design-phase launch (`turn: prepare`): the agent settles what
    /// the owner must answer or know, and the work follows in the same session.
    pub prepare: bool,
    /// The session the work turn resumes (`resume: <id>`), the one that prepared it or asked.
    pub resume: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub front: Front,
    /// Everything after the closing `---`, request and result together.
    pub body: String,
}

#[derive(Debug, PartialEq)]
pub struct ParseError(pub String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseError {}

impl Item {
    pub fn parse(text: &str) -> Result<Item, ParseError> {
        let err = |m: &str| ParseError(m.to_string());
        let rest = text
            .strip_prefix("---\n")
            .ok_or_else(|| err("item does not start with front matter"))?;
        let (head, body) = rest
            .split_once("\n---\n")
            .ok_or_else(|| err("front matter is not closed"))?;

        let mut step = None;
        let mut to = None;
        let mut status = None;
        let mut issue = None;
        let mut sprint = None;
        let mut round = None;
        let mut unit = None;
        let mut refs = Vec::new();
        let mut out = None;
        let mut prepare = false;
        let mut resume = None;
        for line in head.lines() {
            let (key, value) = line
                .split_once(':')
                .ok_or_else(|| err(&format!("front matter line {line:?} has no key")))?;
            let value = value.trim();
            let some = |v: &str| (!v.is_empty()).then(|| v.to_string());
            match key.trim() {
                "step" => step = Some(value.parse::<Step>().map_err(|e| err(&e))?),
                "to" => to = Some(value.parse::<Role>().map_err(|e| err(&e))?),
                "status" => status = Some(value.parse::<Status>().map_err(|e| err(&e))?),
                "issue" => issue = some(value),
                "sprint" => sprint = some(value),
                "round" => {
                    if !value.is_empty() {
                        round = Some(
                            value
                                .parse()
                                .map_err(|_| err(&format!("round {value:?} is not a number")))?,
                        );
                    }
                }
                "unit" if !value.is_empty() => {
                    unit = Some(
                        value
                            .parse()
                            .map_err(|_| err(&format!("unit {value:?} is not a number")))?,
                    );
                }
                "unit" => {}
                "refs" => {
                    refs = value
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .collect();
                }
                "out" => out = some(value),
                "turn" => match value {
                    "prepare" => prepare = true,
                    "work" => {}
                    other => return Err(err(&format!("unknown turn {other:?}"))),
                },
                "resume" => resume = some(value),
                other => return Err(err(&format!("unknown front matter field {other:?}"))),
            }
        }
        Ok(Item {
            front: Front {
                step: step.ok_or_else(|| err("missing step"))?,
                to: to.ok_or_else(|| err("missing to"))?,
                status: status.ok_or_else(|| err("missing status"))?,
                issue,
                sprint,
                round,
                unit,
                refs,
                out: out.ok_or_else(|| err("missing out"))?,
                prepare,
                resume,
            },
            body: body.to_string(),
        })
    }

    pub fn render(&self) -> String {
        use std::fmt::Write as _;
        let f = &self.front;
        let mut s = String::from("---\n");
        // Writing to a String cannot fail.
        let _ = writeln!(
            s,
            "step: {}\nto: {}\nstatus: {}",
            f.step,
            f.to.as_str(),
            f.status.as_str()
        );
        for (key, value) in [("issue", &f.issue), ("sprint", &f.sprint)] {
            if let Some(v) = value {
                let _ = writeln!(s, "{key}: {v}");
            }
        }
        if let Some(r) = f.round {
            let _ = writeln!(s, "round: {r}");
        }
        if let Some(u) = f.unit {
            let _ = writeln!(s, "unit: {u}");
        }
        if !f.refs.is_empty() {
            let _ = writeln!(s, "refs: {}", f.refs.join(", "));
        }
        let _ = writeln!(s, "out: {}", f.out);
        if f.prepare {
            s.push_str("turn: prepare\n");
        }
        if let Some(session) = &f.resume {
            let _ = writeln!(s, "resume: {session}");
        }
        s.push_str("---\n");
        s.push_str(&self.body);
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ITEM: &str = "---\nstep: design-review\nto: reviewer\nstatus: open\nissue: ACME-142\nsprint: 20261121-0905\nround: 1\nrefs: docs/design/ACME-142.md, sources/issues/ACME-142.md\nout: /s/outputs/x.json\n---\nReview the design of ACME-142.\n\n## Result\n";

    #[test]
    fn an_item_round_trips_through_parse_and_render() {
        let item = Item::parse(ITEM).unwrap();
        assert_eq!(item.front.to, Role::Reviewer);
        assert_eq!(item.front.round, Some(1));
        assert_eq!(item.front.refs.len(), 2);
        assert_eq!(item.render(), ITEM);
    }

    #[test]
    fn a_preparation_turn_and_a_resumed_work_turn_round_trip() {
        for added in ["turn: prepare\n", "resume: 7f3c\n"] {
            let text = ITEM.replace("---\nReview", &format!("{added}---\nReview"));
            let item = Item::parse(&text).unwrap();
            assert_eq!(item.render(), text);
        }
        let item =
            Item::parse(&ITEM.replace("---\nReview", "turn: prepare\nresume: 7f3c\n---\nReview"))
                .unwrap();
        assert!(item.front.prepare);
        assert_eq!(item.front.resume.as_deref(), Some("7f3c"));
    }

    #[test]
    fn an_unknown_field_is_rejected() {
        let text = ITEM.replace("round: 1\n", "priority: high\n");
        assert!(Item::parse(&text).is_err());
    }

    #[test]
    fn a_body_line_that_looks_like_front_matter_stays_in_the_body() {
        let text = ITEM.replace("## Result\n", "## Result\nstatus: done\n");
        let item = Item::parse(&text).unwrap();
        assert_eq!(item.front.status, Status::Open);
        assert!(item.body.ends_with("status: done\n"));
    }
}
