//! The owner's conversations with the lead in Slack: a thread started by
//! mentioning the bot with anything but a command, answered by the lead in that thread, turn by
//! turn.

use std::path::Path;

use ovenspring_core::config::Role;
use ovenspring_core::item::{Front, Item, Status};
use ovenspring_core::output::StepOutput;
use ovenspring_core::step::Step;
use ovenspring_integrations::channel::MessageRef;
use ovenspring_store::item_name;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{Daemon, DaemonError};

/// Who said what in a conversation's thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
    /// The owner's Slack user id, or `lead`.
    pub who: String,
    pub text: String,
    pub at: String,
}

const LEAD: &str = "lead";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conversation {
    pub channel: String,
    /// The thread's root message.
    pub thread: String,
    pub turns: Vec<Turn>,
    /// The lead's item answering the turns before `asked`, while it is on the board.
    pub item: Option<String>,
    pub asked: usize,
}

fn path(thread: &str) -> String {
    format!("conversations/{thread}.json")
}

/// A message the owner sent outside any decision's thread.
pub(crate) struct Said<'a> {
    pub text: &'a str,
    pub user: &'a str,
    pub thread: &'a MessageRef,
    pub mentioned: bool,
}

impl Daemon {
    /// Starts a conversation from a mention, or continues the one its thread holds; anything
    /// else said in the channel is not the factory's business.
    pub(crate) fn on_said(&self, said: &Said<'_>) -> Result<(), DaemonError> {
        if !self.is_owner(said.user) {
            return self.event(&json!({"event": "ignored-message", "user": said.user}));
        }
        let known: Option<Conversation> = self.store.read_record(&path(&said.thread.ts))?;
        let mut c = match known {
            Some(c) => c,
            None if said.mentioned => Conversation {
                channel: said.thread.channel.clone(),
                thread: said.thread.ts.clone(),
                turns: Vec::new(),
                item: None,
                asked: 0,
            },
            None => return Ok(()),
        };
        c.turns.push(Turn {
            who: said.user.into(),
            text: said.text.into(),
            at: jiff::Timestamp::now().to_string(),
        });
        if c.item.is_none() {
            self.file_conversation_item(&mut c)?;
        }
        self.store.write_record(&path(&c.thread), &c)?;
        Ok(())
    }

    /// Files the lead's turn over every message of the thread so far.
    fn file_conversation_item(&self, c: &mut Conversation) -> Result<(), DaemonError> {
        // The thread and its length name each turn, so a thread's turns sort in the order taken.
        let name = self.new_item_name(&format!(
            "conversation-{}-{}",
            c.thread.replace('.', "-"),
            c.turns.len()
        ));
        let thread: Vec<String> = c
            .turns
            .iter()
            .map(|t| {
                let who = if t.who == LEAD {
                    "You (the lead)".to_owned()
                } else {
                    format!("The owner (<@{}>)", t.who)
                };
                format!("**{who}**, {}:\n\n{}", t.at, t.text)
            })
            .collect();
        let item = Item {
            front: Front {
                step: Step::Conversation,
                to: Role::Lead,
                status: Status::Open,
                issue: None,
                sprint: None,
                round: None,
                unit: None,
                refs: vec![],
                out: self.store.output_path(&name).display().to_string(),
                prepare: false,
                resume: None,
            },
            body: format!(
                "Take your turn in the owner's Slack thread; the owner's latest message is the last.\n\n## Thread\n\n{}\n\n## Result\n",
                thread.join("\n\n")
            ),
        };
        self.store.file_item(&name, &item)?;
        c.item = Some(name.clone());
        c.asked = c.turns.len();
        self.event(&json!({"event": "filed", "conversation": c.thread, "item": name, "step": Step::Conversation}))
    }

    /// Posts the lead's finished turn in its thread, and files the next when the owner wrote
    /// again meanwhile.
    pub(crate) fn consume_conversation_item(
        &self,
        file: &Path,
        item: &Item,
    ) -> Result<(), DaemonError> {
        let name = item_name(file);
        let reply = match item.front.status {
            Status::Done => match self.read_output(item)? {
                Some(StepOutput::Conversation(c)) => Some(c.reply),
                _ => None,
            },
            Status::Blocked | Status::Failed => None,
            Status::Open | Status::Doing => return Ok(()),
        };
        self.store.archive_item(file)?;
        let Some(mut c) = self
            .store
            .record_names("conversations")?
            .iter()
            .filter_map(|id| {
                self.store
                    .read_record::<Conversation>(&path(id))
                    .ok()
                    .flatten()
            })
            .find(|c| c.item.as_deref() == Some(name.as_str()))
        else {
            return Ok(());
        };
        let text = reply.clone().unwrap_or_else(|| {
            "I could not answer this time; the launch is in the dashboard's logs. Write again to ask once more."
                .into()
        });
        let thread = MessageRef {
            channel: c.channel.clone(),
            ts: c.thread.clone(),
            permalink: None,
        };
        match self.services.channel.reply(&thread, &text) {
            Ok(_) if reply.is_some() => {
                c.turns.insert(
                    c.asked,
                    Turn {
                        who: LEAD.into(),
                        text,
                        at: jiff::Timestamp::now().to_string(),
                    },
                );
                c.asked += 1;
            }
            Ok(_) => {}
            Err(e) => self.event(
                &json!({"event": "channel-error", "conversation": c.thread, "error": e.0}),
            )?,
        }
        c.item = None;
        // The owner wrote again while the lead was answering.
        if c.turns.len() > c.asked {
            self.file_conversation_item(&mut c)?;
        }
        self.store.write_record(&path(&c.thread), &c)?;
        Ok(())
    }
}
