//! Slack as the owner's channel: messages through the Web API with the bot token, and buttons,
//! thread replies and commands through Socket Mode with the app-level token, so the factory
//! needs no public address. A command is the bot's mention followed by `!name`; plain
//! text in the channel is never one.
//!
//! Each decision's message carries the decision id in its metadata and in every button, so an
//! answer finds its decision after a restart, and a decision posted twice still counts one answer.

use std::collections::{BTreeMap, VecDeque};
use std::fmt::Write as _;
use std::io::ErrorKind;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde_json::{Value, json};

use super::{
    Button, Channel, ChannelError, ChannelEvent, DecisionKind, DecisionRequest, MessageRef, Notice,
    NoticeKind,
};
use crate::http::{HttpError, Method, Request, Transport, check};

pub const API: &str = "https://slack.com/api";
const METADATA_TYPE: &str = "ovenspring_decision";
/// Slack's limit for the text of all `markdown` blocks in one message.
const MARKDOWN_MAX: usize = 12_000;

/// What Socket Mode delivered, before thread replies are matched to their decisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Raw {
    /// A message that does not mention the bot.
    Message {
        user: String,
        text: String,
        ts: String,
        thread_ts: Option<String>,
    },
    /// The bot's mention followed by `!name`.
    Command {
        name: String,
        args: String,
        user: String,
        ts: String,
        thread_ts: Option<String>,
    },
    /// Any other message that mentions the bot, without the mention it starts with.
    Mention {
        user: String,
        text: String,
        ts: String,
        thread_ts: Option<String>,
    },
    Action {
        action: String,
        value: String,
        user: String,
        /// The message the button is on.
        ts: Option<String>,
    },
}

/// One Socket Mode envelope: the id to acknowledge, and what it carried for this channel.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Envelope {
    pub ack: Option<String>,
    pub disconnect: bool,
    pub events: Vec<Raw>,
}

/// Reads one Socket Mode frame. Messages from bots (the factory's own included), edits, and
/// anything outside `channel` are dropped here. `bot_user` is the bot's Slack user id: Slack
/// sends a message that mentions it both as `app_mention` and as `message`, and only the
/// `app_mention` is kept.
pub fn parse_envelope(text: &str, channel: &str, bot_user: &str) -> Envelope {
    let Ok(v) = serde_json::from_str::<Value>(text) else {
        return Envelope::default();
    };
    let mut out = Envelope {
        ack: v
            .get("envelope_id")
            .and_then(Value::as_str)
            .map(str::to_owned),
        disconnect: v.get("type").and_then(Value::as_str) == Some("disconnect"),
        events: Vec::new(),
    };
    let payload = v.get("payload").cloned().unwrap_or(Value::Null);
    match v.get("type").and_then(Value::as_str) {
        Some("events_api") => {
            out.events
                .extend(message_event(payload.get("event"), channel, bot_user));
        }
        Some("interactive") => out.events.extend(block_actions(&payload, channel)),
        _ => {}
    }
    out
}

fn str_at<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

fn message_event(event: Option<&Value>, channel: &str, bot_user: &str) -> Option<Raw> {
    let e = event?;
    let kind = str_at(e, "type")?;
    // A reply also sent to the channel, or with a file, is the owner's message like any other;
    // edits, deletions and the rest are not.
    let reply_kind =
        str_at(e, "subtype").is_none_or(|t| matches!(t, "thread_broadcast" | "file_share"));
    if !matches!(kind, "message" | "app_mention")
        || !reply_kind
        || e.get("bot_id").is_some()
        || str_at(e, "channel")? != channel
    {
        return None;
    }
    let user = str_at(e, "user")?.to_owned();
    let text = str_at(e, "text").unwrap_or_default();
    let ts = str_at(e, "ts")?.to_owned();
    let thread_ts = str_at(e, "thread_ts").map(str::to_owned);
    if kind == "message" {
        return (!mentions(text, bot_user)).then(|| Raw::Message {
            user,
            text: text.to_owned(),
            ts,
            thread_ts,
        });
    }
    let Some(rest) = after_leading_mention(text, bot_user) else {
        return Some(Raw::Mention {
            user,
            text: text.to_owned(),
            ts,
            thread_ts,
        });
    };
    let Some(command) = rest.strip_prefix('!') else {
        return Some(Raw::Mention {
            user,
            text: rest.to_owned(),
            ts,
            thread_ts,
        });
    };
    let (name, args) = command
        .split_once(char::is_whitespace)
        .unwrap_or((command, ""));
    Some(Raw::Command {
        name: name.to_ascii_lowercase(),
        args: args.trim().to_owned(),
        user,
        ts,
        thread_ts,
    })
}

/// Whether `text` mentions `user`, written by Slack as `<@U…>` or `<@U…|name>`.
fn mentions(text: &str, user: &str) -> bool {
    text.match_indices("<@")
        .any(|(i, _)| mention_end(&text[i..], user).is_some())
}

/// The length of `user`'s mention at the start of `text`, if it starts with one.
fn mention_end(text: &str, user: &str) -> Option<usize> {
    let rest = text.strip_prefix("<@")?.strip_prefix(user)?;
    let close = match rest.chars().next()? {
        '>' => 0,
        '|' => rest.find('>')?,
        _ => return None,
    };
    Some(2 + user.len() + close + 1)
}

/// The text after the mention of `user` it starts with, trimmed.
fn after_leading_mention<'a>(text: &'a str, user: &str) -> Option<&'a str> {
    let text = text.trim_start();
    let end = mention_end(text, user)?;
    text.get(end..).map(str::trim)
}

fn block_actions(payload: &Value, channel: &str) -> Vec<Raw> {
    if str_at(payload, "type") != Some("block_actions")
        || payload.pointer("/channel/id").and_then(Value::as_str) != Some(channel)
    {
        return Vec::new();
    }
    let user = payload
        .pointer("/user/id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let ts = payload
        .pointer("/container/message_ts")
        .and_then(Value::as_str)
        .map(str::to_owned);
    payload
        .get("actions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|a| {
            Some(Raw::Action {
                action: str_at(a, "action_id")?.to_owned(),
                value: str_at(a, "value")?.to_owned(),
                user: user.to_owned(),
                ts: ts.clone(),
            })
        })
        .collect()
}

/// The lead's Markdown in a block Slack draws as Markdown: headings, bold, lists and tables.
fn markdown(text: &str) -> Value {
    json!({"type": "markdown", "text": text})
}

/// Splits text into pieces Slack accepts in one message, at line breaks where it can.
fn pieces(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        if current.chars().count() + line.chars().count() + 1 > MARKDOWN_MAX && !current.is_empty()
        {
            out.push(std::mem::take(&mut current));
        }
        let mut rest = line;
        while rest.chars().count() > MARKDOWN_MAX {
            let cut = rest
                .char_indices()
                .nth(MARKDOWN_MAX)
                .map_or(rest.len(), |(i, _)| i);
            out.push(rest[..cut].to_owned());
            rest = &rest[cut..];
        }
        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(rest);
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn button(label: &str, action: &str, value: &str, primary: bool) -> Value {
    let mut b = json!({"type": "button", "text": {"type": "plain_text", "text": label}, "action_id": action, "value": value});
    if primary && let Some(obj) = b.as_object_mut() {
        obj.insert("style".into(), json!("primary"));
    }
    b
}

/// The id of Preview, which starts the issue's app; it answers no decision, so it carries the
/// issue.
const PREVIEW: &str = "preview";

fn decision_button(label: &str, b: Button, d: &DecisionRequest, primary: bool) -> Value {
    button(label, b.id(), &d.id, primary)
}

fn preview_button(issue: &str) -> Value {
    button("Preview", PREVIEW, issue, false)
}

/// The blocks of a decision's message: the title, the content's first piece, and the
/// buttons its kind takes. A question is answered by replying in its thread.
pub fn decision_blocks(d: &DecisionRequest) -> Value {
    let mut blocks = vec![
        json!({"type": "section", "text": {"type": "mrkdwn", "text": format!("*{}*", d.title)}}),
    ];
    blocks.extend(pieces(&d.text).first().map(|t| markdown(t)));
    let issue = d.issue.as_deref().unwrap_or_default();
    // What an issue waits on the owner for, they may answer by ending the issue instead.
    let cancel = || decision_button("Cancel issue", Button::Cancel, d, false);
    let buttons = match d.kind {
        DecisionKind::Spec => d.issue.is_some().then(cancel).into_iter().collect(),
        DecisionKind::Merge => {
            let mut buttons = vec![
                decision_button("Approve merge", Button::Approve, d, true),
                decision_button("Hold", Button::Hold, d, false),
            ];
            if d.preview {
                buttons.push(preview_button(issue));
            }
            buttons
        }
        DecisionKind::Sprint
        | DecisionKind::Prototype
        | DecisionKind::Design
        | DecisionKind::Prd => {
            vec![decision_button("Approve", Button::Approve, d, true)]
        }
        // Close, not Approve: the owner has read the result and has nothing more to say, and closing
        // lets the review's agreement and knowledge changes merge.
        DecisionKind::Review => vec![decision_button("Close sprint", Button::Approve, d, true)],
        DecisionKind::Recovery => {
            let mut buttons = vec![
                decision_button("Approve", Button::Approve, d, true),
                decision_button("Resolved", Button::Resolved, d, false),
            ];
            buttons.extend(d.issue.is_some().then(cancel));
            buttons
        }
        DecisionKind::Stopped => {
            let mut buttons = vec![decision_button("Resolved", Button::Resolved, d, true)];
            buttons.extend(d.issue.is_some().then(cancel));
            buttons
        }
    };
    let reply = match d.kind {
        DecisionKind::Spec => "Reply in this thread to answer.",
        // A merge request is answered with its buttons only.
        DecisionKind::Merge => "Hold keeps it unmerged until the sprint review.",
        DecisionKind::Sprint
        | DecisionKind::Prototype
        | DecisionKind::Design
        | DecisionKind::Prd
        | DecisionKind::Review
        | DecisionKind::Recovery
        | DecisionKind::Stopped => "Or reply in this thread.",
    };
    if !buttons.is_empty() {
        blocks.push(json!({"type": "actions", "elements": buttons}));
    }
    blocks.push(json!({"type": "context", "elements": [{"type": "mrkdwn", "text": reply}]}));
    Value::Array(blocks)
}

/// The blocks of a notice: its text's first piece, and Preview on a pull request ready for the
/// owner to merge when the product can be started.
pub fn notice_blocks(n: &Notice) -> Value {
    let mut blocks: Vec<Value> = pieces(&n.text)
        .first()
        .map(|t| markdown(t))
        .into_iter()
        .collect();
    if let (NoticeKind::ReadyToMerge, Some(issue), true) = (n.kind, &n.issue, n.preview) {
        blocks.push(json!({"type": "actions", "elements": [preview_button(issue)]}));
    }
    Value::Array(blocks)
}

/// The name the manifest gives the app and its bot, until the owner renames it in Slack.
pub const APP_NAME: &str = "Oven Spring";

/// The manifest an owner creates the Slack app from: Socket Mode, buttons, and the bot scopes and
/// events the channel uses.
pub fn app_manifest() -> Value {
    json!({
        "display_information": { "name": APP_NAME },
        "features": { "bot_user": { "display_name": APP_NAME, "always_online": true } },
        "oauth_config": { "scopes": { "bot": [
            "app_mentions:read", "channels:history", "groups:history",
            "chat:write", "files:write", "users:read"
        ] } },
        "settings": {
            "event_subscriptions": { "bot_events": ["app_mention", "message.channels", "message.groups"] },
            "interactivity": { "is_enabled": true },
            "socket_mode_enabled": true,
            "org_deploy_enabled": false,
            "token_rotation_enabled": false
        }
    })
}

/// The factory's side of Slack.
pub struct Slack<T> {
    transport: T,
    api: String,
    bot_token: String,
    channel: String,
    inbox: Arc<Mutex<VecDeque<Raw>>>,
    /// Message ts → the decision it asks, or none, for replies in its thread.
    threads: Mutex<BTreeMap<String, Option<String>>>,
    /// Set when this channel is dropped, so its Socket Mode connection closes.
    stop: Arc<AtomicBool>,
    /// The Slack `ts` of the latest message received, or where to read from after a restart:
    /// what arrived after it while no connection was open is read when one opens.
    seen: Arc<Mutex<Option<String>>>,
    /// Why Socket Mode has kept failing to connect, while it has.
    trouble: Arc<Mutex<Option<String>>>,
    /// The bot's name as Slack shows it to people, once read.
    mention: Mutex<Option<String>>,
}

impl<T> Drop for Slack<T> {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl<T: Transport> Slack<T> {
    pub fn new(transport: T, bot_token: String, channel: String) -> Slack<T> {
        Slack::with_api(transport, API, bot_token, channel)
    }

    pub fn with_api(transport: T, api: &str, bot_token: String, channel: String) -> Slack<T> {
        Slack {
            transport,
            api: api.trim_end_matches('/').to_owned(),
            bot_token,
            channel,
            inbox: Arc::new(Mutex::new(VecDeque::new())),
            threads: Mutex::new(BTreeMap::new()),
            stop: Arc::new(AtomicBool::new(false)),
            seen: Arc::new(Mutex::new(None)),
            trouble: Arc::new(Mutex::new(None)),
            mention: Mutex::new(None),
        }
    }

    /// The bot's name as people in the workspace see it, which they type to mention it.
    fn bot_name(&self) -> Result<String, ChannelError> {
        let me = self.call("auth.test", &[], Some(json!({})))?;
        let id = str_at(&me, "user_id")
            .ok_or_else(|| ChannelError("auth.test: no user_id".into()))?
            .to_owned();
        let user = self.call("users.info", &[("user", &id)], None)?;
        [
            "/user/profile/display_name",
            "/user/real_name",
            "/user/name",
        ]
        .iter()
        .find_map(|p| {
            user.pointer(p)
                .and_then(Value::as_str)
                .filter(|n| !n.is_empty())
        })
        .map(str::to_owned)
        .ok_or_else(|| ChannelError(format!("users.info: no name for {id}")))
    }

    /// Where to read what the channel received while the factory was not running, as a Unix
    /// time: the first connection reads the messages and replies after it.
    pub fn catch_up_from(&self, unix_seconds: i64) {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_or_insert_with(|| format!("{unix_seconds}.000000"));
    }

    /// Setup's check: the bot token works, the bot can read the channel, the
    /// owner is a member of the workspace, and the app-level token opens Socket Mode. Answers what
    /// it found, or why it failed, in one sentence each for the owner.
    pub fn check_setup(&self, app_token: &str, owner: &str) -> Result<String, String> {
        self.call("auth.test", &[], Some(json!({})))
            .map_err(|e| format!("Slack refused the bot token ({}).", e.0))?;
        let channel = self.channel.clone();
        self.call(
            "conversations.history",
            &[("channel", &channel), ("limit", "1")],
            None,
        )
        .map_err(|e| {
            if e.0.ends_with("not_in_channel") {
                format!("The app is not in channel {channel}; invite it there.")
            } else {
                format!("The bot cannot read channel {channel} ({}).", e.0)
            }
        })?;
        let user = self
            .call("users.info", &[("user", owner)], None)
            .map_err(|e| format!("Slack has no member {owner} ({}).", e.0))?;
        let name = ["/user/real_name", "/user/name"]
            .iter()
            .find_map(|p| {
                user.pointer(p)
                    .and_then(Value::as_str)
                    .filter(|n| !n.is_empty())
            })
            .unwrap_or(owner)
            .to_owned();
        let request = Request {
            method: Method::Post,
            url: format!("{}/apps.connections.open", self.api),
            headers: vec![("authorization".into(), format!("Bearer {app_token}"))],
            body: Some(json!({})),
            raw: None,
        };
        let opened = self
            .transport
            .send(&request)
            .and_then(|r| check(&request, r))
            .map_err(|e| e.to_string())
            .and_then(|r| serde_json::from_str::<Value>(&r.body).map_err(|e| e.to_string()))?;
        if opened.get("ok").and_then(Value::as_bool) != Some(true) {
            let why = str_at(&opened, "error").unwrap_or("unexpected answer");
            return Err(format!("Slack refused the app-level token ({why})."));
        }
        Ok(format!("Connected to channel {channel}, as {name}."))
    }

    /// Keeps a Socket Mode connection open on its own thread until this channel is dropped,
    /// acknowledging each envelope at once and putting what it carried in the inbox
    /// [`Channel::events`] reads. A dropped connection is opened again, waiting longer after each
    /// failure up to a minute.
    pub fn listen(
        &self,
        transport: Arc<dyn Transport>,
        app_token: String,
    ) -> std::thread::JoinHandle<()> {
        let tokens = SocketTokens {
            app: app_token,
            bot: self.bot_token.clone(),
        };
        let socket = Socket {
            channel: self.channel.clone(),
            inbox: Arc::clone(&self.inbox),
            stop: Arc::clone(&self.stop),
            seen: Arc::clone(&self.seen),
            trouble: Arc::clone(&self.trouble),
        };
        std::thread::spawn(move || {
            let mut wait = 1;
            let mut failures = 0;
            while !socket.stop.load(Ordering::Relaxed) {
                match run_socket(transport.as_ref(), &tokens, &socket) {
                    Ok(()) => {
                        wait = 1;
                        failures = 0;
                    }
                    Err(e) => {
                        eprintln!("ovenspring: Slack connection: {e}");
                        wait = (wait * 2).min(60);
                        failures += 1;
                        // A dropped connection that opens again is no trouble; one that keeps
                        // failing is the owner's to see.
                        if failures >= 3 {
                            *socket
                                .trouble
                                .lock()
                                .unwrap_or_else(PoisonError::into_inner) = Some(e);
                        }
                    }
                }
                for _ in 0..wait {
                    if socket.stop.load(Ordering::Relaxed) {
                        return;
                    }
                    std::thread::sleep(Duration::from_secs(1));
                }
            }
        })
    }

    /// Where Socket Mode puts what it receives; [`Channel::events`] takes it from here.
    pub fn inbox(&self) -> Arc<Mutex<VecDeque<Raw>>> {
        Arc::clone(&self.inbox)
    }

    /// Slack's answer to a Web API call, whether or not it is `ok`.
    fn answer(
        &self,
        method: &str,
        query: &[(&str, &str)],
        body: Option<Value>,
    ) -> Result<Value, ChannelError> {
        let mut url = format!("{}/{method}", self.api);
        if !query.is_empty() {
            let q: Vec<String> = query
                .iter()
                .map(|(k, v)| format!("{k}={}", encode(v)))
                .collect();
            url.push('?');
            url.push_str(&q.join("&"));
        }
        let request = Request {
            method: if body.is_some() {
                Method::Post
            } else {
                Method::Get
            },
            url,
            headers: vec![("authorization".into(), format!("Bearer {}", self.bot_token))],
            body,
            raw: None,
        };
        let response = self
            .transport
            .send(&request)
            .and_then(|r| check(&request, r))
            .map_err(|e| ChannelError(format!("Slack {method}: {e}")))?;
        serde_json::from_str(&response.body)
            .map_err(|e| ChannelError(format!("Slack {method}: {e}")))
    }

    fn call(
        &self,
        method: &str,
        query: &[(&str, &str)],
        body: Option<Value>,
    ) -> Result<Value, ChannelError> {
        let value = self.answer(method, query, body)?;
        if value.get("ok").and_then(Value::as_bool) == Some(true) {
            Ok(value)
        } else {
            let error = value
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            Err(ChannelError(format!("Slack {method}: {error}")))
        }
    }

    fn post(&self, body: &Value) -> Result<MessageRef, ChannelError> {
        // A preview for every pull request link would bury the line that says what to do.
        let mut body = body.clone();
        if let Some(fields) = body.as_object_mut() {
            fields.insert("unfurl_links".into(), json!(false));
            fields.insert("unfurl_media".into(), json!(false));
        }
        let v = self.call("chat.postMessage", &[], Some(body))?;
        let ts = str_at(&v, "ts").unwrap_or_default().to_owned();
        let permalink = self
            .call(
                "chat.getPermalink",
                &[("channel", &self.channel), ("message_ts", &ts)],
                None,
            )
            .ok()
            .and_then(|p| str_at(&p, "permalink").map(str::to_owned));
        Ok(MessageRef {
            channel: self.channel.clone(),
            ts,
            permalink,
        })
    }

    /// Text longer than one message holds: its first piece is in the message, the rest follows
    /// in its thread.
    fn reply_rest(&self, m: &MessageRef, text: &str) -> Result<(), ChannelError> {
        self.post_replies(m, pieces(text).into_iter().skip(1))
            .map(drop)
    }

    /// Posts each piece as a reply in the thread; the first reply's reference.
    fn post_replies(
        &self,
        thread: &MessageRef,
        pieces: impl Iterator<Item = String>,
    ) -> Result<Option<MessageRef>, ChannelError> {
        let mut first = None;
        for piece in pieces {
            let m = self.post(&json!({
                "channel": thread.channel, "thread_ts": thread.ts,
                "text": piece, "blocks": [markdown(&piece)],
            }))?;
            first.get_or_insert(m);
        }
        Ok(first)
    }

    fn remember(&self, ts: &str, decision: Option<&str>) {
        self.threads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(ts.to_owned(), decision.map(str::to_owned));
    }

    /// The decision a thread's root carries in its metadata, read from Slack once per thread.
    /// `Err` when Slack could not be asked now; a root Slack no longer has carries none.
    fn decision_of(&self, ts: &str) -> Result<Option<String>, ChannelError> {
        if let Some(known) = self
            .threads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(ts)
        {
            return Ok(known.clone());
        }
        let v = self.answer(
            "conversations.replies",
            &[
                ("channel", &self.channel),
                ("ts", ts),
                ("limit", "1"),
                ("include_all_metadata", "true"),
            ],
            None,
        )?;
        let id = v
            .get("messages")
            .and_then(Value::as_array)
            .and_then(|m| m.first())
            .and_then(decision_id);
        self.remember(ts, id.as_deref());
        Ok(id)
    }

    fn at(&self, ts: String) -> MessageRef {
        MessageRef {
            channel: self.channel.clone(),
            ts,
            permalink: None,
        }
    }

    /// What one received message or press means to the daemon. `Err` gives the message back when
    /// its thread could not be looked up, to be read again on the next call.
    fn event_of(&self, raw: Raw) -> Result<Option<ChannelEvent>, Raw> {
        Ok(match raw {
            Raw::Action {
                action,
                value,
                user,
                ts,
            } => match ts.filter(|_| action == PREVIEW) {
                Some(ts) => Some(ChannelEvent::Preview {
                    issue: value,
                    user,
                    at: self.at(ts),
                }),
                // Only the factory's own buttons reach it.
                None => Button::of(&action).map(|button| ChannelEvent::Action {
                    decision: value,
                    button,
                    user,
                }),
            },
            Raw::Command {
                name,
                args,
                user,
                ts,
                thread_ts,
            } => Some(ChannelEvent::Command {
                name,
                args,
                user,
                at: Some(self.at(thread_ts.unwrap_or(ts))),
            }),
            Raw::Mention {
                user,
                text,
                ts,
                thread_ts: None,
            } => Some(ChannelEvent::Said {
                text,
                user,
                thread: self.at(ts),
                mentioned: true,
            }),
            Raw::Message {
                thread_ts: None, ..
            } => None,
            Raw::Message {
                ref user,
                ref text,
                thread_ts: Some(ref parent),
                ..
            }
            | Raw::Mention {
                ref user,
                ref text,
                thread_ts: Some(ref parent),
                ..
            } => match self.decision_of(parent) {
                Ok(Some(decision)) => Some(ChannelEvent::Reply {
                    decision,
                    text: text.clone(),
                    user: user.clone(),
                }),
                Ok(None) => Some(ChannelEvent::Said {
                    text: text.clone(),
                    user: user.clone(),
                    thread: self.at(parent.clone()),
                    mentioned: matches!(raw, Raw::Mention { .. }),
                }),
                Err(e) => {
                    eprintln!("ovenspring: {e}; the message is read again later");
                    return Err(raw);
                }
            },
        })
    }
}

fn decision_id(message: &Value) -> Option<String> {
    let meta = message.get("metadata")?;
    (str_at(meta, "event_type")? == METADATA_TYPE).then(|| {
        meta.pointer("/event_payload/id")
            .and_then(Value::as_str)
            .map(str::to_owned)
    })?
}

fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

impl<T: Transport> Channel for Slack<T> {
    fn trouble(&self) -> Option<String> {
        self.trouble
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn mention(&self) -> Option<String> {
        let mut known = self.mention.lock().unwrap_or_else(PoisonError::into_inner);
        if known.is_none() {
            *known = self.bot_name().ok();
        }
        known.clone()
    }

    fn post_decision(&self, d: &DecisionRequest) -> Result<MessageRef, ChannelError> {
        let m = self.post(&json!({
            "channel": self.channel,
            "text": d.title,
            "blocks": decision_blocks(d),
            "metadata": {"event_type": METADATA_TYPE, "event_payload": {"id": d.id}},
        }))?;
        self.remember(&m.ts, Some(&d.id));
        self.reply_rest(&m, &d.text)?;
        Ok(m)
    }

    fn post_notice(&self, n: &Notice) -> Result<MessageRef, ChannelError> {
        let m = self
            .post(&json!({"channel": self.channel, "text": n.text, "blocks": notice_blocks(n)}))?;
        self.reply_rest(&m, &n.text)?;
        Ok(m)
    }

    /// Posts the text as replies in the thread, one per piece Slack accepts in a message.
    fn reply(&self, thread: &MessageRef, text: &str) -> Result<MessageRef, ChannelError> {
        self.post_replies(thread, pieces(text).into_iter())?
            .ok_or_else(|| ChannelError("an empty reply was not posted".into()))
    }

    /// Slack's external upload: an upload address for the file, its bytes sent there, then the
    /// file shared into the thread.
    fn attach(&self, thread: &MessageRef, name: &str, content: &str) -> Result<(), ChannelError> {
        let length = content.len().to_string();
        let upload = self.call(
            "files.getUploadURLExternal",
            &[("filename", name), ("length", &length)],
            None,
        )?;
        let (Some(url), Some(file)) = (
            upload.get("upload_url").and_then(Value::as_str),
            upload.get("file_id").and_then(Value::as_str),
        ) else {
            return Err(ChannelError(
                "Slack files.getUploadURLExternal: no upload address in its answer".into(),
            ));
        };
        let request = Request {
            method: Method::Post,
            url: url.to_owned(),
            headers: Vec::new(),
            body: None,
            raw: Some(content.as_bytes().to_vec()),
        };
        self.transport
            .send(&request)
            .and_then(|r| check(&request, r))
            .map_err(|e| ChannelError(format!("Slack file upload: {e}")))?;
        self.call(
            "files.completeUploadExternal",
            &[],
            Some(json!({
                "files": [{"id": file, "title": name}],
                "channel_id": thread.channel,
                "thread_ts": thread.ts,
            })),
        )
        .map(drop)
    }

    fn tell(&self, user: &str, text: &str) -> Result<(), ChannelError> {
        self.call(
            "chat.postEphemeral",
            &[],
            Some(json!({"channel": self.channel, "user": user, "text": text})),
        )
        .map(drop)
    }

    /// Keeps the message's content, replaces its buttons with how it was resolved.
    fn mark_resolved(&self, m: &MessageRef, resolution: &str) -> Result<(), ChannelError> {
        let v = self.call(
            "conversations.history",
            &[
                ("channel", &m.channel),
                ("latest", &m.ts),
                ("inclusive", "true"),
                ("limit", "1"),
                ("include_all_metadata", "true"),
            ],
            None,
        )?;
        let original = v
            .get("messages")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
            .cloned()
            .unwrap_or(Value::Null);
        let mut blocks: Vec<Value> = original
            .get("blocks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|b| !matches!(str_at(b, "type"), Some("actions" | "context")))
            .cloned()
            .collect();
        blocks
            .push(json!({"type": "context", "elements": [{"type": "mrkdwn", "text": resolution}]}));
        self.call(
            "chat.update",
            &[],
            Some(json!({
                "channel": m.channel, "ts": m.ts,
                "text": str_at(&original, "text").unwrap_or(resolution),
                "blocks": blocks,
            })),
        )
        .map(drop)
    }

    /// Button presses, replies in decisions' threads (with or without a mention), commands, and
    /// what owners say to the factory elsewhere, oldest first. A command is answered in its
    /// thread's root when sent in a thread, as Slack asks of replies. A message whose thread
    /// could not be looked up stays for the next call.
    fn events(&self) -> Vec<ChannelEvent> {
        let raw: Vec<Raw> = self
            .inbox
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain(..)
            .collect();
        let mut events = Vec::new();
        let mut later = Vec::new();
        for r in raw {
            match self.event_of(r) {
                Ok(e) => events.extend(e),
                Err(r) => later.push(r),
            }
        }
        if !later.is_empty() {
            let mut inbox = self.inbox.lock().unwrap_or_else(PoisonError::into_inner);
            for r in later.into_iter().rev() {
                inbox.push_front(r);
            }
        }
        events
    }
}

/// The two tokens Socket Mode needs: the app-level token opens the connection, the bot token
/// names the bot whose mentions are commands.
struct SocketTokens {
    pub app: String,
    pub bot: String,
}

/// Calls a Web API method that takes no arguments and returns the field `key` of its answer.
fn note_seen(seen: &Mutex<Option<String>>, events: &[Raw]) {
    let latest = events
        .iter()
        .filter_map(|r| match r {
            Raw::Message { ts, .. } | Raw::Command { ts, .. } | Raw::Mention { ts, .. } => {
                Some(ts.clone())
            }
            Raw::Action { .. } => None,
        })
        .max_by(|a, b| ts_value(a).total_cmp(&ts_value(b)));
    if let Some(latest) = latest {
        let mut seen = seen.lock().unwrap_or_else(PoisonError::into_inner);
        if seen
            .as_deref()
            .is_none_or(|s| ts_value(s) < ts_value(&latest))
        {
            *seen = Some(latest);
        }
    }
}

fn ts_value(ts: &str) -> f64 {
    ts.parse().unwrap_or(0.0)
}

/// Where Web API reads go: the transport, the API's base address and the bot token.
pub struct WebApi<'a> {
    pub transport: &'a dyn Transport,
    pub base: &'a str,
    pub bot_token: &'a str,
}

/// The channel's messages and thread replies after `since`, oldest first, as Socket Mode would
/// have delivered them: the latest messages of the channel, and the replies in the threads of
/// those with replies after `since` (a decision asked days ago is answered in its thread).
pub fn catch_up(
    api: &WebApi<'_>,
    channel: &str,
    bot_user: &str,
    since: &str,
) -> Result<Vec<Raw>, String> {
    let after =
        |m: &Value, key: &str| str_at(m, key).is_some_and(|t| ts_value(t) > ts_value(since));
    let as_event = |m: &Value| {
        let mut e = m.clone();
        if let Some(fields) = e.as_object_mut() {
            fields.insert("channel".into(), json!(channel));
            if str_at(m, "text").is_some_and(|t| mentions(t, bot_user)) {
                fields.insert("type".into(), json!("app_mention"));
            }
        }
        message_event(Some(&e), channel, bot_user)
    };
    let history = get(
        api,
        "conversations.history",
        &[("channel", channel), ("limit", "200")],
    )?;
    let mut missed = Vec::new();
    for m in history
        .get("messages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if after(m, "ts") {
            missed.extend(as_event(m));
        }
        let Some(parent) = str_at(m, "ts") else {
            continue;
        };
        if !after(m, "latest_reply") {
            continue;
        }
        let replies = get(
            api,
            "conversations.replies",
            &[
                ("channel", channel),
                ("ts", parent),
                ("oldest", since),
                ("limit", "200"),
            ],
        )?;
        for r in replies
            .get("messages")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if str_at(r, "ts") != Some(parent) && after(r, "ts") {
                missed.extend(as_event(r));
            }
        }
    }
    missed.sort_by(|a, b| ts_of(a).total_cmp(&ts_of(b)));
    Ok(missed)
}

fn ts_of(raw: &Raw) -> f64 {
    match raw {
        Raw::Message { ts, .. } | Raw::Command { ts, .. } | Raw::Mention { ts, .. } => ts_value(ts),
        Raw::Action { .. } => 0.0,
    }
}

fn get(api: &WebApi<'_>, method: &str, query: &[(&str, &str)]) -> Result<Value, String> {
    let q: Vec<String> = query
        .iter()
        .map(|(k, v)| format!("{k}={}", encode(v)))
        .collect();
    let request = Request {
        method: Method::Get,
        url: format!("{}/{method}?{}", api.base, q.join("&")),
        headers: vec![("authorization".into(), format!("Bearer {}", api.bot_token))],
        body: None,
        raw: None,
    };
    let response = api
        .transport
        .send(&request)
        .and_then(|r| check(&request, r))
        .map_err(|e: HttpError| format!("{method}: {e}"))?;
    let v: Value = serde_json::from_str(&response.body).map_err(|e| format!("{method}: {e}"))?;
    if v.get("ok").and_then(Value::as_bool) == Some(false) {
        return Err(format!(
            "{method}: {}",
            str_at(&v, "error").unwrap_or("unexpected answer")
        ));
    }
    Ok(v)
}

fn api_field(
    transport: &dyn Transport,
    method: &str,
    token: &str,
    key: &str,
) -> Result<String, String> {
    let request = Request {
        method: Method::Post,
        url: format!("{API}/{method}"),
        headers: vec![("authorization".into(), format!("Bearer {token}"))],
        body: Some(json!({})),
        raw: None,
    };
    let response = transport
        .send(&request)
        .and_then(|r| check(&request, r))
        .map_err(|e: HttpError| format!("{method}: {e}"))?;
    let v: Value = serde_json::from_str(&response.body).map_err(|e| format!("{method}: {e}"))?;
    str_at(&v, key).map(str::to_owned).ok_or_else(|| {
        format!(
            "{method}: {}",
            str_at(&v, "error").unwrap_or("unexpected answer")
        )
    })
}

/// Where one Socket Mode connection delivers, and when it stops.
struct Socket {
    channel: String,
    inbox: Arc<Mutex<VecDeque<Raw>>>,
    stop: Arc<AtomicBool>,
    seen: Arc<Mutex<Option<String>>>,
    trouble: Arc<Mutex<Option<String>>>,
}

fn run_socket(
    transport: &dyn Transport,
    tokens: &SocketTokens,
    socket: &Socket,
) -> Result<(), String> {
    use rustls_platform_verifier::ConfigVerifierExt as _;
    use tungstenite::Message;
    let Socket {
        channel,
        inbox,
        stop,
        seen,
        trouble,
    } = socket;

    // Known before the connection opens, so no envelope is read without it.
    let bot_user = api_field(transport, "auth.test", &tokens.bot, "user_id")?;
    let url = api_field(transport, "apps.connections.open", &tokens.app, "url")?;
    let host = url
        .strip_prefix("wss://")
        .and_then(|r| r.split(['/', '?']).next())
        .ok_or_else(|| {
            format!(
                "unexpected Socket Mode URL scheme in {}",
                url.split('?').next().unwrap_or("")
            )
        })?
        .to_owned();
    let tcp = TcpStream::connect((host.as_str(), 443)).map_err(|e| e.to_string())?;
    // Slack pings every few seconds; a long silence means the connection is gone.
    tcp.set_read_timeout(Some(Duration::from_secs(120)))
        .map_err(|e| e.to_string())?;
    let config = rustls::ClientConfig::with_platform_verifier().map_err(|e| e.to_string())?;
    let name = rustls::pki_types::ServerName::try_from(host.clone()).map_err(|e| e.to_string())?;
    let tls = rustls::ClientConnection::new(Arc::new(config), name).map_err(|e| e.to_string())?;
    let stream = rustls::StreamOwned::new(tls, tcp);
    let (mut socket, _) = tungstenite::client(url.as_str(), stream).map_err(|e| e.to_string())?;
    *trouble.lock().unwrap_or_else(PoisonError::into_inner) = None;
    // What arrived while no connection was open; a press is not kept by Slack and is pressed again.
    let since = seen.lock().unwrap_or_else(PoisonError::into_inner).clone();
    if let Some(since) = since {
        let api = WebApi {
            transport,
            base: API,
            bot_token: &tokens.bot,
        };
        let missed = catch_up(&api, channel, &bot_user, &since)?;
        note_seen(seen, &missed);
        inbox
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend(missed);
    }
    // Slack's pings arrive every few seconds, so a stop is seen within seconds.
    loop {
        if stop.load(Ordering::Relaxed) {
            let _closed = socket.close(None);
            return Ok(());
        }
        let text = match socket.read() {
            Ok(Message::Text(t)) => t.to_string(),
            Ok(Message::Close(_)) => return Ok(()),
            Ok(_) => continue,
            Err(tungstenite::Error::Io(e))
                if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
            {
                return Err("no traffic from Slack for two minutes".into());
            }
            Err(e) => return Err(e.to_string()),
        };
        let envelope = parse_envelope(&text, channel, &bot_user);
        note_seen(seen, &envelope.events);
        if let Some(id) = &envelope.ack {
            socket
                .send(Message::text(json!({"envelope_id": id}).to_string()))
                .map_err(|e| e.to_string())?;
        }
        inbox
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend(envelope.events);
        if envelope.disconnect {
            return Ok(());
        }
    }
}
