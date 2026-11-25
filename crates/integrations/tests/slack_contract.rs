//! The Slack channel against recorded Socket Mode envelopes and Web API responses: what reaches
//! the daemon, what each decision's message holds, and how answers find their decisions.
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        reason = "tests fail loudly"
    )
)]

mod common;

use common::Recorded;
use ovenspring_integrations::channel::slack::{
    Envelope, Raw, Slack, WebApi, app_manifest, catch_up, decision_blocks, notice_blocks,
    parse_envelope,
};
use ovenspring_integrations::channel::{
    Channel, ChannelEvent, DecisionKind, DecisionRequest, MessageRef, Notice, NoticeKind,
};
use ovenspring_integrations::http::Method;
use serde_json::{Value, json};

const API: &str = "https://slack.test/api";
const CHANNEL: &str = "C0123456789";
const BOT: &str = "U0BOT00001";

fn slack(t: &Recorded) -> Slack<&Recorded> {
    Slack::with_api(t, API, "xoxb-fixture-0001".into(), CHANNEL.into())
}

fn message_envelope(event: &Value) -> String {
    json!({"envelope_id": "env-1", "type": "events_api", "accepts_response_payload": false,
           "payload": {"type": "event_callback", "event": event}})
    .to_string()
}

#[test]
fn messages_in_the_channel_reach_the_daemon_and_everything_else_is_only_acknowledged() {
    let reply = json!({"type": "message", "channel": CHANNEL, "user": "U1", "text": "No, active orders only.", "ts": "2.0", "thread_ts": "1.0"});
    assert_eq!(
        parse_envelope(&message_envelope(&reply), CHANNEL, BOT),
        Envelope {
            ack: Some("env-1".into()),
            disconnect: false,
            events: vec![Raw::Message {
                user: "U1".into(),
                text: "No, active orders only.".into(),
                ts: "2.0".into(),
                thread_ts: Some("1.0".into())
            }],
        }
    );
    for subtype in ["thread_broadcast", "file_share"] {
        let also = json!({"type": "message", "subtype": subtype, "channel": CHANNEL, "user": "U1",
                          "text": "Drafts too.", "ts": "6.0", "thread_ts": "1.0"});
        assert_eq!(
            parse_envelope(&message_envelope(&also), CHANNEL, BOT)
                .events
                .len(),
            1,
            "{subtype}"
        );
    }
    for ignored in [
        json!({"type": "message", "channel": "C-OTHER", "user": "U1", "text": "hi", "ts": "3.0"}),
        json!({"type": "message", "channel": CHANNEL, "bot_id": "B1", "text": "the factory itself", "ts": "4.0"}),
        json!({"type": "message", "subtype": "message_changed", "channel": CHANNEL, "ts": "5.0"}),
    ] {
        let e = parse_envelope(&message_envelope(&ignored), CHANNEL, BOT);
        assert_eq!(
            (e.ack.as_deref(), e.events.len()),
            (Some("env-1"), 0),
            "{ignored}"
        );
    }
    let hello = parse_envelope(r#"{"type": "hello", "num_connections": 1}"#, CHANNEL, BOT);
    assert_eq!(hello, Envelope::default());
    assert!(
        parse_envelope(
            r#"{"type": "disconnect", "reason": "refresh_requested"}"#,
            CHANNEL,
            BOT
        )
        .disconnect
    );
}

#[test]
fn a_command_is_the_bots_mention_followed_by_a_bang_name_and_plain_text_never_is() {
    let mention = |text: &str, ts: &str, thread: Option<&str>| {
        let mut e = json!({"type": "app_mention", "channel": CHANNEL, "user": "U1", "text": text, "ts": ts});
        if let Some(t) = thread {
            e["thread_ts"] = json!(t);
        }
        parse_envelope(&message_envelope(&e), CHANNEL, BOT).events
    };
    let plain = |text: &str| {
        let e =
            json!({"type": "message", "channel": CHANNEL, "user": "U1", "text": text, "ts": "9.0"});
        parse_envelope(&message_envelope(&e), CHANNEL, BOT).events
    };
    let cases: Vec<(&str, Vec<Raw>, Vec<Raw>)> = vec![
        (
            "mention then command",
            mention(
                "<@U0BOT00001> !pause  waiting for the release ",
                "3.0",
                None,
            ),
            vec![Raw::Command {
                name: "pause".into(),
                args: "waiting for the release".into(),
                user: "U1".into(),
                ts: "3.0".into(),
                thread_ts: None,
            }],
        ),
        (
            "mention with a label, in a thread",
            mention("<@U0BOT00001|oven-spring> !Pause", "4.0", Some("1.0")),
            vec![Raw::Command {
                name: "pause".into(),
                args: String::new(),
                user: "U1".into(),
                ts: "4.0".into(),
                thread_ts: Some("1.0".into()),
            }],
        ),
        (
            "mention without a command",
            mention("<@U0BOT00001> what is left?", "5.0", None),
            vec![Raw::Mention {
                user: "U1".into(),
                text: "what is left?".into(),
                ts: "5.0".into(),
                thread_ts: None,
            }],
        ),
        (
            "someone else mentioned first",
            mention("<@U0OTHER001> <@U0BOT00001> !pause", "6.0", None),
            vec![Raw::Mention {
                user: "U1".into(),
                text: "<@U0OTHER001> <@U0BOT00001> !pause".into(),
                ts: "6.0".into(),
                thread_ts: None,
            }],
        ),
        (
            "a longer id that starts with the bot's",
            mention("<@U0BOT000019> !pause <@U0BOT00001>", "7.0", None),
            vec![Raw::Mention {
                user: "U1".into(),
                text: "<@U0BOT000019> !pause <@U0BOT00001>".into(),
                ts: "7.0".into(),
                thread_ts: None,
            }],
        ),
        (
            "the same mention as a message event",
            plain("<@U0BOT00001> !pause waiting for the release"),
            vec![],
        ),
        (
            "a command without a mention",
            plain("!pause right now"),
            vec![Raw::Message {
                user: "U1".into(),
                text: "!pause right now".into(),
                ts: "9.0".into(),
                thread_ts: None,
            }],
        ),
    ];
    for (case, got, want) in cases {
        assert_eq!(got, want, "{case}");
    }
}

#[test]
fn a_button_press_carries_its_decision_and_who_pressed_it() {
    let pressed = json!({"envelope_id": "env-2", "type": "interactive", "payload": {
        "type": "block_actions", "user": {"id": "U0123456789", "username": "owner"},
        "channel": {"id": CHANNEL}, "container": {"message_ts": "1.0"},
        "actions": [{"action_id": "approve", "value": "ACME-125-merge-4", "type": "button"}]
    }});
    let e = parse_envelope(&pressed.to_string(), CHANNEL, BOT);
    assert_eq!(e.ack.as_deref(), Some("env-2"));
    assert_eq!(
        e.events,
        [Raw::Action {
            action: "approve".into(),
            value: "ACME-125-merge-4".into(),
            user: "U0123456789".into(),
            ts: Some("1.0".into()),
        }]
    );
}

fn decision(kind: DecisionKind, text: &str) -> DecisionRequest {
    DecisionRequest {
        id: "ACME-125-merge-4".into(),
        kind,
        issue: Some("ACME-125".into()),
        sha: Some("abc1234".into()),
        title: "ACME-125: Add an index".into(),
        text: text.into(),
        preview: true,
    }
}

fn buttons(blocks: &Value) -> Vec<(String, String)> {
    blocks
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| b["type"] == "actions")
        .flat_map(|b| b["elements"].as_array().unwrap().clone())
        .map(|e| {
            (
                e["action_id"].as_str().unwrap().to_owned(),
                e["value"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

/// The review asks the owner to read the result, reply, or close the sprint; closing is not approval of the result.
#[test]
fn a_sprint_review_offers_closing_the_sprint_not_approval() {
    let blocks = decision_blocks(&DecisionRequest {
        issue: None,
        sha: None,
        ..decision(
            DecisionKind::Review,
            "Purpose missed: the export is not public yet.",
        )
    });
    let labels: Vec<&str> = blocks
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| b["type"] == "actions")
        .flat_map(|b| b["elements"].as_array().unwrap())
        .map(|e| e["text"]["text"].as_str().unwrap())
        .collect();
    assert_eq!(labels, ["Close sprint"]);
}

#[test]
fn each_kind_of_decision_gets_its_buttons_and_an_issues_wait_can_cancel_it() {
    let merge = buttons(&decision_blocks(&decision(
        DecisionKind::Merge,
        "Held: a schema change.",
    )));
    assert_eq!(
        merge,
        [
            ("approve".into(), "ACME-125-merge-4".into()),
            ("hold".into(), "ACME-125-merge-4".into()),
            ("preview".into(), "ACME-125".into()),
        ],
        "the answers name the decision; Preview names the issue it starts"
    );
    assert_eq!(
        buttons(&decision_blocks(&decision(
            DecisionKind::Prototype,
            "A new screen"
        ))),
        [("approve".to_owned(), "ACME-125-merge-4".to_owned())],
        "a prototype opens from its link in the dashboard, not from a running app"
    );
    let ready = notice_blocks(&Notice {
        kind: NoticeKind::ReadyToMerge,
        issue: Some("ACME-125".into()),
        text: "ACME-125 is ready to merge".into(),
        preview: true,
    });
    assert_eq!(
        buttons(&ready),
        [("preview".to_owned(), "ACME-125".to_owned())]
    );
    let no_preview = DecisionRequest {
        preview: false,
        ..decision(DecisionKind::Merge, "Held: a schema change.")
    };
    assert_eq!(
        buttons(&decision_blocks(&no_preview)),
        [
            ("approve".into(), "ACME-125-merge-4".into()),
            ("hold".into(), "ACME-125-merge-4".into()),
        ]
    );
    let ready_without = notice_blocks(&Notice {
        kind: NoticeKind::ReadyToMerge,
        issue: Some("ACME-125".into()),
        text: "ACME-125 is ready to merge".into(),
        preview: false,
    });
    assert_eq!(buttons(&ready_without), [] as [(String, String); 0]);
    let paused = notice_blocks(&Notice {
        kind: NoticeKind::Paused,
        issue: None,
        text: "Oven Spring paused".into(),
        preview: false,
    });
    assert_eq!(buttons(&paused), [] as [(String, String); 0]);
    assert_eq!(
        buttons(&decision_blocks(&decision(
            DecisionKind::Sprint,
            "Proposal"
        )))
        .len(),
        1
    );
    let question = decision_blocks(&decision(
        DecisionKind::Spec,
        "Should archived orders export?",
    ));
    assert_eq!(
        buttons(&question),
        [("cancel".to_owned(), "ACME-125-merge-4".to_owned())],
        "a question about an issue is answered in its thread, or by canceling the issue"
    );
    assert!(
        question
            .to_string()
            .contains("Reply in this thread to answer.")
    );
    let about_the_sprint = DecisionRequest {
        issue: None,
        ..decision(DecisionKind::Spec, "Which goal comes first?")
    };
    assert_eq!(
        buttons(&decision_blocks(&about_the_sprint)),
        [] as [(String, String); 0]
    );
    for kind in [DecisionKind::Recovery, DecisionKind::Stopped] {
        let ids: Vec<String> = buttons(&decision_blocks(&decision(kind, "A stop")))
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(ids.last().map(String::as_str), Some("cancel"), "{kind:?}");
    }
}

// Slack draws a `markdown` block's text as Markdown, tables included; a `section` shows `|` rows
// and `##` marks as raw text.
#[test]
fn the_leads_markdown_reaches_slack_as_written_in_a_markdown_block() {
    let text = "## 1. Options\n| Option | Fee |\n|---|---|\n| A | **3.6%** |";
    let blocks = decision_blocks(&decision(DecisionKind::Spec, text));
    assert_eq!(blocks[1], json!({"type": "markdown", "text": text}));
}

#[test]
fn text_longer_than_one_message_holds_follows_in_its_thread() {
    let t = Recorded::new()
        .on(
            (Method::Post, &format!("{API}/chat.postMessage")),
            200,
            &json!({"ok": true, "ts": "1.0"}),
        )
        .on(
            (
                Method::Get,
                &format!("{API}/chat.getPermalink?channel={CHANNEL}&message_ts=1.0"),
            ),
            200,
            &json!({"ok": true, "permalink": "https://norsica.slack.test/p1"}),
        );
    let text = (0..600)
        .map(|i| format!("Line {i} of the sprint review with its evidence."))
        .collect::<Vec<_>>()
        .join("\n");
    slack(&t)
        .post_decision(&decision(DecisionKind::Review, &text))
        .unwrap();
    let posts: Vec<Value> = t
        .sent()
        .into_iter()
        .filter(|r| r.url.ends_with("chat.postMessage"))
        .map(|r| r.body.unwrap())
        .collect();
    let markdown: Vec<String> = posts
        .iter()
        .map(|p| {
            p["blocks"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|b| b["type"] == "markdown")
                .map(|b| b["text"].as_str().unwrap())
                .collect()
        })
        .collect();
    assert!(posts.len() > 1);
    assert!(markdown.iter().all(|m| m.chars().count() <= 12_000));
    assert!(
        posts[1..].iter().all(|p| p["thread_ts"] == "1.0"),
        "the rest is in the decision's thread"
    );
    assert_eq!(markdown.join("\n"), text, "nothing is lost");
}

fn history(messages: &Value) -> Value {
    json!({"ok": true, "messages": messages})
}

/// Link previews would bury what a message asks under a card for each pull request it links.
#[test]
fn messages_are_posted_without_link_previews() {
    let t = Recorded::new()
        .on(
            (Method::Post, &format!("{API}/chat.postMessage")),
            200,
            &json!({"ok": true, "ts": "1.0"}),
        )
        .on(
            (
                Method::Get,
                &format!("{API}/chat.getPermalink?channel={CHANNEL}&message_ts=1.0"),
            ),
            200,
            &json!({"ok": true, "permalink": "https://norsica.slack.test/p1"}),
        );
    slack(&t)
        .post_decision(&decision(DecisionKind::Merge, "Held"))
        .unwrap();
    let posted = t.sent()[0].body.clone().unwrap();
    assert_eq!(
        (
            posted["unfurl_links"].clone(),
            posted["unfurl_media"].clone()
        ),
        (json!(false), json!(false))
    );
}

#[test]
fn a_decision_is_posted_with_its_id_in_its_metadata() {
    let t = Recorded::new()
        .on(
            (Method::Post, &format!("{API}/chat.postMessage")),
            200,
            &json!({"ok": true, "ts": "1.0"}),
        )
        .on(
            (
                Method::Get,
                &format!("{API}/chat.getPermalink?channel={CHANNEL}&message_ts=1.0"),
            ),
            200,
            &json!({"ok": true, "permalink": "https://norsica.slack.test/p1"}),
        );
    let m = slack(&t)
        .post_decision(&decision(DecisionKind::Merge, "Held"))
        .unwrap();
    assert_eq!(
        (m.ts.as_str(), m.permalink.as_deref()),
        ("1.0", Some("https://norsica.slack.test/p1"))
    );
    let posted = t.sent()[0].body.clone().unwrap();
    assert_eq!(
        posted["metadata"],
        json!({"event_type": "ovenspring_decision", "event_payload": {"id": "ACME-125-merge-4"}})
    );
    assert_eq!(posted["channel"], CHANNEL);
    assert_eq!(
        t.sent()[0].headers,
        [(
            "authorization".to_owned(),
            "Bearer xoxb-fixture-0001".to_owned()
        )]
    );
}

#[test]
fn replies_find_their_decision_with_or_without_a_mention_and_commands_are_answered_where_sent() {
    let t = Recorded::new().on(
        (Method::Get, &format!("{API}/conversations.replies?channel={CHANNEL}&ts=1.0&limit=1&include_all_metadata=true")),
        200,
        &history(&json!([{"ts": "1.0", "metadata": {"event_type": "ovenspring_decision", "event_payload": {"id": "ACME-123-spec-2"}}}])),
    );
    let s = slack(&t);
    s.inbox().lock().unwrap().extend([
        Raw::Message {
            user: "U1".into(),
            text: "No, active orders only.".into(),
            ts: "2.0".into(),
            thread_ts: Some("1.0".into()),
        },
        Raw::Command {
            name: "pause".into(),
            args: "waiting for the release".into(),
            user: "U1".into(),
            ts: "3.0".into(),
            thread_ts: None,
        },
        Raw::Message {
            user: "U1".into(),
            text: "!pause right now".into(),
            ts: "4.0".into(),
            thread_ts: None,
        },
        Raw::Mention {
            user: "U1".into(),
            text: "and archived ones too".into(),
            ts: "5.0".into(),
            thread_ts: Some("1.0".into()),
        },
        Raw::Command {
            name: "status".into(),
            args: String::new(),
            user: "U1".into(),
            ts: "6.0".into(),
            thread_ts: Some("1.0".into()),
        },
        Raw::Mention {
            user: "U1".into(),
            text: "what is left?".into(),
            ts: "7.0".into(),
            thread_ts: None,
        },
    ]);
    let at = |ts: &str| {
        Some(MessageRef {
            channel: CHANNEL.into(),
            ts: ts.into(),
            permalink: None,
        })
    };
    assert_eq!(
        s.events(),
        [
            ChannelEvent::Reply {
                decision: "ACME-123-spec-2".into(),
                text: "No, active orders only.".into(),
                user: "U1".into()
            },
            ChannelEvent::Command {
                name: "pause".into(),
                args: "waiting for the release".into(),
                user: "U1".into(),
                at: at("3.0"),
            },
            ChannelEvent::Reply {
                decision: "ACME-123-spec-2".into(),
                text: "and archived ones too".into(),
                user: "U1".into()
            },
            ChannelEvent::Command {
                name: "status".into(),
                args: String::new(),
                user: "U1".into(),
                at: at("1.0"),
            },
            ChannelEvent::Said {
                text: "what is left?".into(),
                user: "U1".into(),
                thread: at("7.0").unwrap(),
                mentioned: true,
            },
        ]
    );
    assert_eq!(t.sent().len(), 1, "the thread's decision is looked up once");
}

#[test]
fn messages_in_other_threads_reach_the_daemon_as_said_and_one_slack_cannot_look_up_now_waits() {
    let replies = |ts: &str| {
        format!(
            "{API}/conversations.replies?channel={CHANNEL}&ts={ts}&limit=1&include_all_metadata=true"
        )
    };
    let t = Recorded::new()
        .on(
            (Method::Get, &replies("7.0")),
            200,
            &history(&json!([{"ts": "7.0", "text": "<@U0BOT00001> what is left?"}])),
        )
        .on(
            (Method::Get, &replies("9.0")),
            429,
            &json!({"ok": false, "error": "ratelimited"}),
        )
        .on(
            (Method::Get, &replies("12.0")),
            200,
            &json!({"ok": false, "error": "thread_not_found"}),
        );
    let s = slack(&t);
    let message = |ts: &str, thread: &str, text: &str| Raw::Message {
        user: "U1".into(),
        text: text.into(),
        ts: ts.into(),
        thread_ts: Some(thread.into()),
    };
    s.inbox().lock().unwrap().extend([
        message("8.0", "7.0", "and the export?"),
        message("10.0", "9.0", "lost for now"),
        Raw::Mention {
            user: "U1".into(),
            text: "one more".into(),
            ts: "11.0".into(),
            thread_ts: Some("7.0".into()),
        },
        message("13.0", "12.0", "under a deleted message"),
    ]);
    let thread = MessageRef {
        channel: CHANNEL.into(),
        ts: "7.0".into(),
        permalink: None,
    };
    assert_eq!(
        s.events(),
        [
            ChannelEvent::Said {
                text: "and the export?".into(),
                user: "U1".into(),
                thread: thread.clone(),
                mentioned: false,
            },
            ChannelEvent::Said {
                text: "one more".into(),
                user: "U1".into(),
                thread,
                mentioned: true,
            },
            ChannelEvent::Said {
                text: "under a deleted message".into(),
                user: "U1".into(),
                thread: MessageRef {
                    channel: CHANNEL.into(),
                    ts: "12.0".into(),
                    permalink: None,
                },
                mentioned: false,
            },
        ]
    );
    assert_eq!(
        s.inbox()
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        [message("10.0", "9.0", "lost for now")],
        "kept to be read again"
    );
}

#[test]
fn preview_names_the_issue_and_the_message_to_answer_in() {
    let pressed = json!({"envelope_id": "env-3", "type": "interactive", "payload": {
        "type": "block_actions", "user": {"id": "U0123456789"},
        "channel": {"id": CHANNEL}, "container": {"type": "message", "message_ts": "4.0"},
        "actions": [{"action_id": "preview", "value": "ACME-125", "type": "button"}]
    }});
    let t = Recorded::new();
    let s = slack(&t);
    s.inbox()
        .lock()
        .unwrap()
        .extend(parse_envelope(&pressed.to_string(), CHANNEL, BOT).events);
    assert_eq!(
        s.events(),
        [ChannelEvent::Preview {
            issue: "ACME-125".into(),
            user: "U0123456789".into(),
            at: MessageRef {
                channel: CHANNEL.into(),
                ts: "4.0".into(),
                permalink: None
            },
        }]
    );
}

#[test]
fn resolving_keeps_the_content_and_replaces_the_buttons() {
    let original = json!([{"ts": "1.0", "text": "ACME-125: Add an index", "blocks": [
        {"type": "section", "text": {"type": "mrkdwn", "text": "*ACME-125: Add an index*"}},
        {"type": "actions", "elements": []},
        {"type": "context", "elements": []}
    ]}]);
    let t = Recorded::new()
        .on((Method::Get, &format!("{API}/conversations.history?channel={CHANNEL}&latest=1.0&inclusive=true&limit=1&include_all_metadata=true")), 200, &history(&original))
        .on((Method::Post, &format!("{API}/chat.update")), 200, &json!({"ok": true}));
    let m = MessageRef {
        channel: CHANNEL.into(),
        ts: "1.0".into(),
        permalink: None,
    };
    slack(&t)
        .mark_resolved(&m, "Approved by <@U0123456789>.")
        .unwrap();
    let blocks = t.sent()[1].body.clone().unwrap()["blocks"].clone();
    let kinds: Vec<&str> = blocks
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["type"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["section", "context"]);
    assert!(blocks.to_string().contains("Approved by"));
}

#[test]
fn a_slack_error_is_an_error() {
    let t = Recorded::new().on(
        (Method::Post, &format!("{API}/chat.postMessage")),
        200,
        &json!({"ok": false, "error": "not_in_channel"}),
    );
    let m = MessageRef {
        channel: CHANNEL.into(),
        ts: "1.0".into(),
        permalink: None,
    };
    let err = slack(&t).reply(&m, "hello").unwrap_err();
    assert!(err.0.contains("not_in_channel"), "{}", err.0);
}

#[test]
fn the_app_manifest_asks_for_socket_mode_buttons_and_the_scopes_the_channel_uses() {
    let m = app_manifest();
    assert_eq!(m["settings"]["socket_mode_enabled"], true);
    assert_eq!(m["settings"]["interactivity"]["is_enabled"], true);
    let list = |v: &Value| -> Vec<String> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_owned())
            .collect()
    };
    let scopes = list(&m["oauth_config"]["scopes"]["bot"]);
    for scope in [
        "chat:write",
        "channels:history",
        "groups:history",
        "app_mentions:read",
        "files:write",
        "users:read",
    ] {
        assert!(scopes.iter().any(|s| s == scope), "missing {scope}");
    }
    let events = list(&m["settings"]["event_subscriptions"]["bot_events"]);
    for event in ["app_mention", "message.channels", "message.groups"] {
        assert!(events.iter().any(|e| e == event), "missing {event}");
    }
}

#[test]
fn the_channel_names_its_bot_as_people_in_the_workspace_see_it() {
    let t = Recorded::new()
        .on(
            (Method::Post, &format!("{API}/auth.test")),
            200,
            &json!({"ok": true, "user_id": BOT}),
        )
        .on(
            (Method::Get, &format!("{API}/users.info?user={BOT}")),
            200,
            &json!({"ok": true, "user": {"name": "oven_spring", "real_name": "Oven Spring",
                     "profile": {"display_name": "Oven Spring (shop)"}}}),
        );
    assert_eq!(slack(&t).mention().as_deref(), Some("Oven Spring (shop)"));
}

#[test]
fn setup_check_confirms_both_tokens_the_channel_and_the_owner() {
    let routes = |history: Value| {
        Recorded::new()
            .on(
                (Method::Post, &format!("{API}/auth.test")),
                200,
                &json!({"ok": true, "user_id": BOT}),
            )
            .on(
                (
                    Method::Get,
                    &format!("{API}/conversations.history?channel={CHANNEL}&limit=1"),
                ),
                200,
                &history,
            )
            .on(
                (Method::Get, &format!("{API}/users.info?user=U0123456789")),
                200,
                &json!({"ok": true, "user": {"name": "kai", "real_name": "Kai Moreno"}}),
            )
            .on(
                (Method::Post, &format!("{API}/apps.connections.open")),
                200,
                &json!({"ok": true, "url": "wss://wss.slack.test/link"}),
            )
    };
    let t = routes(json!({"ok": true, "messages": []}));
    assert_eq!(
        slack(&t).check_setup("xapp-fixture-0001", "U0123456789"),
        Ok(format!("Connected to channel {CHANNEL}, as Kai Moreno."))
    );
    let open = t
        .sent()
        .into_iter()
        .find(|r| r.url.ends_with("apps.connections.open"))
        .unwrap();
    assert_eq!(
        open.headers,
        [(
            "authorization".to_owned(),
            "Bearer xapp-fixture-0001".to_owned()
        )],
        "the app-level token, not the bot token, opens Socket Mode"
    );

    let t = routes(json!({"ok": false, "error": "not_in_channel"}));
    assert_eq!(
        slack(&t).check_setup("xapp-fixture-0001", "U0123456789"),
        Err(format!(
            "The app is not in channel {CHANNEL}; invite it there."
        ))
    );
}

// Behavior: the factory was not running from ts 100 -> on connecting, the channel's history and the
//     threads with replies after it are read -> the owner's reply in an old decision's thread and a
//     new message reach the daemon in order; the factory's own messages and anything older do not.
#[test]
fn what_arrived_while_no_connection_was_open_is_read_when_one_opens() {
    let history = json!({"ok": true, "messages": [
        {"type": "message", "user": "U1", "text": "Is ACME-3 next?", "ts": "150.0"},
        {"type": "message", "bot_id": "B1", "text": "ACME-2: may the export include drafts?", "ts": "50.0",
         "reply_count": 2, "latest_reply": "120.0"},
        {"type": "message", "user": "U1", "text": "old news", "ts": "40.0"}
    ]});
    let replies = json!({"ok": true, "messages": [
        {"type": "message", "bot_id": "B1", "text": "ACME-2: may the export include drafts?", "ts": "50.0"},
        {"type": "message", "user": "U1", "text": "No drafts.", "ts": "120.0", "thread_ts": "50.0"}
    ]});
    let t = Recorded::new()
        .on(
            (Method::Get, &format!("{API}/conversations.history?channel={CHANNEL}&limit=200")),
            200,
            &history,
        )
        .on(
            (Method::Get, &format!("{API}/conversations.replies?channel={CHANNEL}&ts=50.0&oldest=100.000000&limit=200")),
            200,
            &replies,
        );
    let api = WebApi {
        transport: &&t,
        base: API,
        bot_token: "xoxb-fixture-0001",
    };
    let missed = catch_up(&api, CHANNEL, BOT, "100.000000").unwrap();
    assert_eq!(
        missed,
        [
            Raw::Message {
                user: "U1".into(),
                text: "No drafts.".into(),
                ts: "120.0".into(),
                thread_ts: Some("50.0".into())
            },
            Raw::Message {
                user: "U1".into(),
                text: "Is ACME-3 next?".into(),
                ts: "150.0".into(),
                thread_ts: None
            },
        ]
    );
}
