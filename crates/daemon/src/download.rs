//! The download of a sprint's or an issue's records: one JSON
//! Lines file, every record in time order, for an agent to read or for an audit, with every
//! secret the factory holds or a token format names replaced by `[masked]`.

use std::collections::BTreeSet;
use std::fs;

use ovenspring_core::output::StepOutput;
use ovenspring_store::Store;
use serde_json::{Value, json};

use crate::DaemonError;
use crate::owner::ApprovalRecord;
use crate::records::{self, LaunchLine};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    Sprint(String),
    Issue(String),
}

impl Scope {
    /// `sprint:<id>` or `issue:<key>`.
    pub fn parse(text: &str) -> Option<Scope> {
        match text.split_once(':')? {
            ("sprint", id) if !id.is_empty() => Some(Scope::Sprint(id.to_owned())),
            ("issue", key) if !key.is_empty() => Some(Scope::Issue(key.to_owned())),
            _ => None,
        }
    }

    pub fn name(&self) -> String {
        match self {
            Scope::Sprint(id) => format!("sprint-{id}"),
            Scope::Issue(key) => key.clone(),
        }
    }
}

const MASK: &str = "[masked]";

/// Token formats the design names, masked wherever they start a word.
const PREFIXES: [&str; 8] = [
    "xoxb-",
    "xoxp-",
    "xapp-",
    "github_pat_",
    "ghp_",
    "gho_",
    "lin_api_",
    "sk-",
];

fn token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

fn word_start(text: &str, at: usize) -> bool {
    text.get(..at)
        .and_then(|before| before.chars().next_back())
        .is_none_or(|c| !token_char(c))
}

/// The length of a token starting at `at`, when one does.
fn token_at(text: &str, at: usize) -> Option<usize> {
    let rest = text.get(at..)?;
    if !word_start(text, at) {
        return None;
    }
    for prefix in PREFIXES {
        if let Some(body) = rest.strip_prefix(prefix) {
            let len = body.chars().take_while(|c| token_char(*c)).count();
            return (len > 0).then_some(prefix.len() + len);
        }
    }
    let aws = rest.starts_with("AKIA") || rest.starts_with("ASIA");
    if aws {
        let key: String = rest.chars().take(20).collect();
        let tail = rest.chars().nth(20);
        if key.len() == 20
            && key
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            && tail.is_none_or(|c| !c.is_ascii_alphanumeric())
        {
            return Some(20);
        }
    }
    if rest.starts_with("-----BEGIN") {
        let begin = rest.find("PRIVATE KEY-----")?;
        if !rest.get(..begin)?.contains('\n') && !rest.get(..begin)?.contains("\\n") {
            let end_marker = rest.find("-----END")?;
            let end = rest.get(end_marker..)?.find("PRIVATE KEY-----")?;
            return Some(end_marker + end + "PRIVATE KEY-----".len());
        }
    }
    None
}

/// `text` with every value in `secrets` and every token the design's formats name replaced by
/// `[masked]`.
pub fn mask(text: &str, secrets: &[&str]) -> String {
    let mut spans = Vec::new();
    for secret in secrets.iter().filter(|secret| !secret.is_empty()) {
        let mut from = 0;
        while let Some(relative) = text.get(from..).and_then(|rest| rest.find(secret)) {
            let start = from + relative;
            spans.push((start, start + secret.len()));
            let Some(character) = text.get(start..).and_then(|rest| rest.chars().next()) else {
                break;
            };
            from = start + character.len_utf8();
        }
    }
    let mut at = 0;
    while at < text.len() {
        if let Some(len) = token_at(text, at) {
            spans.push((at, at + len));
        }
        let Some(character) = text.get(at..).and_then(|rest| rest.chars().next()) else {
            break;
        };
        at += character.len_utf8();
    }

    spans.sort_unstable();
    let mut merged = Vec::<(usize, usize)>::with_capacity(spans.len());
    for (start, end) in spans {
        if let Some((_, previous_end)) = merged.last_mut()
            && start < *previous_end
        {
            *previous_end = (*previous_end).max(end);
        } else {
            merged.push((start, end));
        }
    }

    let mut masked = String::with_capacity(text.len());
    let mut at = 0;
    for (start, end) in merged {
        masked.push_str(&text[at..start]);
        masked.push_str(MASK);
        at = end;
    }
    masked.push_str(&text[at..]);
    masked
}

fn text_of(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn record(kind: &str, at: &str, mut body: Value) -> Value {
    if let Some(obj) = body.as_object_mut() {
        obj.insert("type".into(), json!(kind));
        obj.insert("at".into(), json!(at));
    }
    body
}

/// The JSON Lines of `scope`, in time order, masked with the credentials in `secrets`.
pub fn download(store: &Store, scope: &Scope, secrets: &[&str]) -> Result<String, DaemonError> {
    let issues: BTreeSet<String> = match scope {
        Scope::Issue(key) => BTreeSet::from([key.clone()]),
        Scope::Sprint(id) => records::issues(store)?
            .into_iter()
            .filter(|r| r.sprint.as_deref() == Some(id.as_str()))
            .map(|r| r.key().to_owned())
            .collect(),
    };
    let sprint = match scope {
        Scope::Sprint(id) => Some(id.as_str()),
        Scope::Issue(_) => None,
    };
    let ours = |v: &Value| {
        text_of(v, "issue").is_some_and(|k| issues.contains(&k))
            || sprint.is_some_and(|id| text_of(v, "sprint").as_deref() == Some(id))
    };
    let mut out: Vec<Value> = Vec::new();
    for v in records::lines(store, "events.jsonl")? {
        if ours(&v) {
            out.push(record("event", &text_of(&v, "at").unwrap_or_default(), v));
        }
    }
    let launches = records::lines(store, records::LAUNCHES)?;
    let items: BTreeSet<String> = launches
        .iter()
        .filter(|v| ours(v))
        .filter_map(|v| text_of(v, "item"))
        .collect();
    for v in launches {
        let mine = text_of(&v, "item").is_some_and(|i| items.contains(&i));
        if mine {
            out.push(record("launch", &text_of(&v, "at").unwrap_or_default(), v));
        }
    }
    let mut decisions = BTreeSet::new();
    for d in records::decisions(store)? {
        let ours = d.issue.as_ref().is_some_and(|k| issues.contains(k))
            || sprint.is_some_and(|s| d.sprint.as_deref() == Some(s));
        if ours {
            let at = d.answer.as_ref().map(|a| a.at.clone()).unwrap_or_default();
            out.push(record("decision", &at, json_of(&d)?));
            decisions.insert(d.id);
        }
    }
    for id in store.isolated_names("approvals")? {
        if let Some(a) = store.read_isolated::<ApprovalRecord>(&crate::owner::approval_path(&id))?
            && decisions.contains(&a.decision)
        {
            let at = a.at.clone();
            out.push(record("approval", &at, json_of(&a)?));
        }
    }
    for name in &items {
        out.extend(item_record(store, name)?);
    }
    out.sort_by_key(|v| text_of(v, "at"));
    let text = out.iter().fold(String::new(), |mut text, v| {
        text.push_str(&v.to_string());
        text.push('\n');
        text
    });
    Ok(mask(&text, secrets))
}

fn json_of(value: &impl serde::Serialize) -> Result<Value, DaemonError> {
    serde_json::to_value(value)
        .map_err(|e| DaemonError::Launch(format!("a record does not serialize: {e}")))
}

/// A finished item's result and output, at the time its last launch ended.
fn item_record(store: &Store, name: &str) -> Result<Option<Value>, DaemonError> {
    let Some(path) = store.find_item(name) else {
        return Ok(None);
    };
    let item = store.read_item(&path)?;
    let result = item
        .body
        .split_once("## Result")
        .map(|(_, r)| r.trim().to_owned())
        .unwrap_or_default();
    let output = fs::read_to_string(&item.front.out)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .filter(|v| StepOutput::parse(item.front.step, v.clone()).is_ok());
    let ended = records::launch_lines(store)?
        .into_iter()
        .filter_map(|line| match line {
            LaunchLine::Start(s) => (s.item == name).then_some(s.at),
            LaunchLine::End(e) => (e.item == name).then_some(e.at),
        })
        .next_back()
        .unwrap_or_default();
    Ok(Some(record(
        "item",
        &ended,
        json!({"item": name, "step": item.front.step, "role": item.front.to.as_str(),
               "issue": item.front.issue, "result": result, "output": output}),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_token_format_and_held_secret_is_masked_where_a_word_starts() {
        let cases = [
            ("bot xoxb-1234-abcd done", "bot [masked] done"),
            ("user xoxp-9 app xapp-1-A2", "user [masked] app [masked]"),
            (
                "gh ghp_ABC123 pat github_pat_11A_x oauth gho_z",
                "gh [masked] pat [masked] oauth [masked]",
            ),
            ("linear lin_api_Q1w2", "linear [masked]"),
            ("openai \"sk-proj-abc\"", "openai \"[masked]\""),
            ("aws AKIAABCDEFGHIJKLMNOP.", "aws [masked]."),
            (
                "key -----BEGIN RSA PRIVATE KEY-----\\nMIIE\\n-----END RSA PRIVATE KEY----- end",
                "key [masked] end",
            ),
            (
                "the held value s3cr3t-v4lue here",
                "the held value [masked] here",
            ),
        ];
        for (text, expected) in cases {
            assert_eq!(mask(text, &["s3cr3t-v4lue"]), expected, "{text}");
        }
    }

    #[test]
    fn words_that_only_contain_a_prefix_stay() {
        for text in [
            "task-list",
            "desk-top",
            "AKIA123",
            "AKIAABCDEFGHIJKLMNOPQ",
            "sk-",
        ] {
            assert_eq!(mask(text, &[]), text);
        }
    }

    #[test]
    fn overlapping_held_values_and_tokens_are_masked_as_their_original_spans() {
        assert_eq!(
            mask("before abcde after", &["abc", "bcde"]),
            "before [masked] after"
        );
        assert_eq!(
            mask("before sk-proj-abcdef after", &["sk-proj-abc"]),
            "before [masked] after"
        );
    }

    #[test]
    fn a_scope_is_a_sprint_or_an_issue() {
        assert_eq!(
            Scope::parse("sprint:20261121-0905"),
            Some(Scope::Sprint("20261121-0905".into()))
        );
        assert_eq!(
            Scope::parse("issue:ACME-1"),
            Some(Scope::Issue("ACME-1".into()))
        );
        assert_eq!(Scope::parse("issue:"), None);
        assert_eq!(Scope::parse("all"), None);
    }
}
