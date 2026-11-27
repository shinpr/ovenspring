//! Slack for the integration tests: the Web API answered in memory, keeping what the factory
//! posted so later reads (permalinks, a message's metadata, its blocks) find it, and Socket Mode
//! envelopes as Slack sends them, read by the channel's own parser into its inbox.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use ovenspring_integrations::channel::slack::{Slack, parse_envelope};
use ovenspring_integrations::http::{HttpError, Request, Response, Transport, Unanswered};
use serde_json::{Value, json};

pub const API: &str = "https://slack.test/api";
pub const CHANNEL: &str = "C0123456789";
pub const BOT: &str = "U0BOT00001";

/// One Web API call: the method and its body or query.
#[derive(Debug, Clone)]
pub struct Call {
    pub method: String,
    pub args: Value,
}

#[derive(Default)]
struct State {
    calls: Vec<Call>,
    /// Every message posted, with the `ts` Slack gave it.
    messages: Vec<Value>,
    failing: BTreeSet<String>,
    /// Methods Slack carries out but whose answer never arrives.
    losing: BTreeSet<String>,
    next_ts: u64,
}

#[derive(Clone, Default)]
pub struct SlackStub(Arc<Mutex<State>>);

fn query(url: &str) -> Value {
    let pairs = url.split_once('?').map_or("", |(_, q)| q);
    Value::Object(
        pairs
            .split('&')
            .filter_map(|p| p.split_once('='))
            .map(|(k, v)| (k.to_owned(), Value::String(v.to_owned())))
            .collect(),
    )
}

impl Transport for SlackStub {
    fn send(&self, request: &Request) -> Result<Response, HttpError> {
        let path = request.url.strip_prefix(API).unwrap_or(&request.url);
        let method = path
            .trim_start_matches('/')
            .split('?')
            .next()
            .unwrap_or_default()
            .to_owned();
        let args = request
            .body
            .clone()
            .or_else(|| {
                request
                    .raw
                    .as_ref()
                    .map(|b| Value::String(String::from_utf8_lossy(b).into_owned()))
            })
            .unwrap_or_else(|| query(&request.url));
        let mut st = self.0.lock().unwrap();
        st.calls.push(Call {
            method: method.clone(),
            args: args.clone(),
        });
        let body = if st.failing.contains(&method) {
            json!({"ok": false, "error": "service_unavailable"})
        } else {
            answer(&mut st, &method, &args)
        };
        if st.losing.contains(&method) {
            return Err(HttpError::transient(
                Unanswered::NoConnection,
                format!("{method}: the answer was lost"),
            ));
        }
        Ok(Response {
            status: 200,
            headers: Vec::new(),
            body: body.to_string(),
        })
    }
}

fn answer(st: &mut State, method: &str, args: &Value) -> Value {
    let named = |st: &State| -> Option<Value> {
        let ts = args.get("ts").or_else(|| args.get("latest"))?.as_str()?;
        st.messages.iter().find(|m| m["ts"] == ts).cloned()
    };
    match method {
        "chat.postMessage" => {
            // Now, as Slack stamps a message, so a read from a recorded time finds it.
            st.next_ts += 1;
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let ts = format!("{now}.{:06}", st.next_ts);
            let mut message = args.clone();
            message["ts"] = json!(ts);
            st.messages.push(message);
            json!({"ok": true, "channel": CHANNEL, "ts": ts})
        }
        "chat.getPermalink" => json!({"ok": true, "permalink": format!(
            "https://acme.slack.test/archives/{CHANNEL}/p{}",
            args["message_ts"].as_str().unwrap_or_default().replace('.', "")
        )}),
        "conversations.history" if args.get("ts").or_else(|| args.get("latest")).is_none() => {
            history_page(st, args)
        }
        "conversations.history" | "conversations.replies" => {
            let found = named(st).map_or_else(
                || vec![json!({"ts": args.get("ts").cloned().unwrap_or_default()})],
                |m| vec![m],
            );
            json!({"ok": true, "messages": found})
        }
        "files.getUploadURLExternal" => json!({
            "ok": true,
            "upload_url": "https://files.slack.test/upload/v1/F0FILE0001",
            "file_id": "F0FILE0001"
        }),
        _ => json!({"ok": true}),
    }
}

/// The channel's messages from `oldest` on, newest first, `limit` to a page, the next page named by
/// `next_cursor` as Slack does.
fn history_page(st: &State, args: &Value) -> Value {
    let arg = |key: &str| args.get(key).and_then(Value::as_str).unwrap_or_default();
    let ts = |m: &Value| {
        m["ts"]
            .as_str()
            .unwrap_or_default()
            .parse::<f64>()
            .unwrap_or(0.0)
    };
    let oldest: f64 = arg("oldest").parse().unwrap_or(0.0);
    let limit: usize = arg("limit").parse().unwrap_or(100);
    let start: usize = arg("cursor").parse().unwrap_or(0);
    let mut messages: Vec<Value> = st
        .messages
        .iter()
        .filter(|m| m["channel"] == arg("channel") && m.get("thread_ts").is_none())
        .filter(|m| ts(m) >= oldest)
        .cloned()
        .collect();
    messages.sort_by(|a, b| ts(b).total_cmp(&ts(a)));
    let end = messages.len().min(start + limit);
    let next = if end < messages.len() {
        end.to_string()
    } else {
        String::new()
    };
    json!({"ok": true, "messages": messages.get(start..end).unwrap_or_default(),
           "response_metadata": {"next_cursor": next}})
}

impl SlackStub {
    /// The channel the daemon talks through, as configuration builds it, on this stub.
    pub fn channel(&self) -> Arc<Slack<SlackStub>> {
        Arc::new(Slack::with_api(
            self.clone(),
            API,
            "xoxb-test-0001".into(),
            CHANNEL.into(),
        ))
    }

    /// Makes `method` answer as Slack does while it cannot serve it, until `fail(method, false)`.
    pub fn fail(&self, method: &str, failing: bool) {
        let mut st = self.0.lock().unwrap();
        if failing {
            st.failing.insert(method.into());
        } else {
            st.failing.remove(method);
        }
    }

    /// Makes Slack carry out `method` but lose its answer, as when the connection drops after the
    /// request arrived, until `lose_answer(method, false)`.
    pub fn lose_answer(&self, method: &str, losing: bool) {
        let mut st = self.0.lock().unwrap();
        if losing {
            st.losing.insert(method.into());
        } else {
            st.losing.remove(method);
        }
    }

    pub fn calls(&self, method: &str) -> Vec<Value> {
        self.0
            .lock()
            .unwrap()
            .calls
            .iter()
            .filter(|c| c.method == method)
            .map(|c| c.args.clone())
            .collect()
    }

    /// The messages posted, with their `ts`.
    pub fn posted(&self) -> Vec<Value> {
        self.0.lock().unwrap().messages.clone()
    }

    /// The posted message carrying decision `id` in its metadata.
    pub fn message_of(&self, id: &str) -> Value {
        self.posted()
            .into_iter()
            .find(|m| m["metadata"]["event_payload"]["id"] == id)
            .unwrap_or_else(|| panic!("no message for decision {id}"))
    }
}

/// Puts what Socket Mode delivers into the channel's inbox, as its connection does.
pub fn deliver(slack: &Slack<SlackStub>, envelopes: &[Value]) {
    let inbox = slack.inbox();
    let mut inbox = inbox.lock().unwrap();
    for e in envelopes {
        inbox.extend(parse_envelope(&e.to_string(), CHANNEL, BOT).events);
    }
}

/// `user` presses the button `action` with `value` on the message `ts`.
pub fn press(action: &str, value: &str, user: &str, ts: &str) -> Value {
    json!({"envelope_id": format!("press-{ts}-{action}-{user}"), "type": "interactive", "payload": {
        "type": "block_actions", "user": {"id": user},
        "channel": {"id": CHANNEL}, "container": {"type": "message", "message_ts": ts},
        "actions": [{"action_id": action, "value": value, "type": "button"}]
    }})
}

/// The button `action` on the posted message `message`, pressed by `user`.
pub fn press_on(message: &Value, action: &str, user: &str) -> Value {
    let value = message["blocks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| b["type"] == "actions")
        .flat_map(|b| b["elements"].as_array().cloned().unwrap_or_default())
        .find(|e| e["action_id"] == action)
        .unwrap_or_else(|| panic!("no {action} button on {message}"))["value"]
        .as_str()
        .unwrap()
        .to_owned();
    press(action, &value, user, message["ts"].as_str().unwrap())
}

/// A message event as Slack delivers it in an envelope.
fn event(kind: &str, mut message: Value) -> Value {
    message["type"] = json!(kind);
    message["channel"] = json!(CHANNEL);
    let ts = message["ts"].as_str().unwrap_or_default().to_owned();
    json!({"envelope_id": format!("{kind}-{ts}"), "type": "events_api",
           "payload": {"type": "event_callback", "event": message}})
}

fn said(user: &str, text: &str, ts: &str, thread_ts: Option<&str>) -> Value {
    let mut m = json!({"user": user, "text": text, "ts": ts});
    if let Some(t) = thread_ts {
        m["thread_ts"] = json!(t);
    }
    m
}

/// A message from `user` that mentions the bot first: Slack sends it as `app_mention` and as
/// `message`.
pub fn mention(user: &str, text: &str, ts: &str, thread_ts: Option<&str>) -> Vec<Value> {
    let m = said(user, &format!("<@{BOT}> {text}"), ts, thread_ts);
    vec![event("app_mention", m.clone()), event("message", m)]
}

/// A message from `user` that does not mention the bot.
pub fn message(user: &str, text: &str, ts: &str, thread_ts: Option<&str>) -> Value {
    event("message", said(user, text, ts, thread_ts))
}
