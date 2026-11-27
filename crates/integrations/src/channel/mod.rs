//! The factory's conversation with the owner. Slack
//! implements it.

pub mod slack;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MessageRef {
    pub channel: String,
    pub ts: String,
    pub permalink: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DecisionKind {
    Sprint,
    Spec,
    Design,
    Prototype,
    Prd,
    Merge,
    Review,
    /// A recovery procedure for the owner to approve.
    Recovery,
    /// A stop the lead could not take: the owner marks it resolved.
    Stopped,
}

/// A question or approval request; its `id` travels with the message so an answer finds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionRequest {
    pub id: String,
    pub kind: DecisionKind,
    pub issue: Option<String>,
    /// The commit an approval applies to (merge requests).
    pub sha: Option<String>,
    /// One line naming what is asked.
    pub title: String,
    /// The content, in Markdown.
    pub text: String,
    /// A merge request offers Preview when the product has commands to start it.
    pub preview: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    Paused,
    ReadyToMerge,
    /// No open issue serves the goal: the lead's word on what is missing.
    WaitingForIssues,
    /// What the owner should know or prepare, told before a design-phase step works on.
    Preparation,
    /// What still waits on the owner, told again while it waits.
    Reminder,
    /// An error has started.
    ErrorStarted,
    /// An error has cleared.
    ErrorCleared,
}

/// What makes a notice one message however often it is posted: a post first looks for the
/// message carrying `id` and posts only when there is none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoticeMarker {
    /// Names what the notice tells, not the attempt to post it.
    pub id: String,
    /// When what the notice tells was recorded, in Unix seconds. The message is never older, so
    /// the search reads back only to shortly before it.
    pub recorded: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub kind: NoticeKind,
    pub issue: Option<String>,
    pub text: String,
    /// A ready-to-merge notice offers Preview when the product has commands to start it.
    pub preview: bool,
    /// Set when posting the notice again must not post a second message.
    pub marker: Option<NoticeMarker>,
}

/// A button on a decision's message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Approve,
    Hold,
    Resolved,
    /// Ends the issue the decision is about.
    Cancel,
}

impl Button {
    /// The id the channel sends back when the button is pressed.
    pub fn id(self) -> &'static str {
        match self {
            Button::Approve => "approve",
            Button::Hold => "hold",
            Button::Resolved => "resolved",
            Button::Cancel => "cancel",
        }
    }

    pub fn of(id: &str) -> Option<Button> {
        [
            Button::Approve,
            Button::Hold,
            Button::Resolved,
            Button::Cancel,
        ]
        .into_iter()
        .find(|b| b.id() == id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelEvent {
    /// A button on a decision's message, pressed by `user`.
    Action {
        decision: String,
        button: Button,
        user: String,
    },
    /// Preview pressed on a message about `issue`; the address goes in that message's thread.
    Preview {
        issue: String,
        user: String,
        at: MessageRef,
    },
    /// A message outside any decision's thread: one that mentions the bot (`mentioned`), or a
    /// reply in a thread. `thread` is the thread's root, or the message itself when it has none.
    Said {
        text: String,
        user: String,
        thread: MessageRef,
        mentioned: bool,
    },
    /// A reply in a decision's thread.
    Reply {
        decision: String,
        text: String,
        user: String,
    },
    /// A command: the bot's mention followed by `!name`.
    Command {
        name: String,
        args: String,
        user: String,
        /// Where to answer: the command's own message, or its thread's root when sent in a thread.
        at: Option<MessageRef>,
    },
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct ChannelError(pub String);

pub trait Channel: Send + Sync {
    fn post_decision(&self, d: &DecisionRequest) -> Result<MessageRef, ChannelError>;
    fn post_notice(&self, n: &Notice) -> Result<MessageRef, ChannelError>;
    fn reply(&self, thread: &MessageRef, text: &str) -> Result<MessageRef, ChannelError>;
    /// Attaches a document to a message's thread as a file named `name`, so the owner reads what
    /// they are asked to approve.
    fn attach(&self, thread: &MessageRef, name: &str, content: &str) -> Result<(), ChannelError>;
    /// Tells one person something only they see, such as why their press did nothing.
    fn tell(&self, user: &str, text: &str) -> Result<(), ChannelError>;
    /// Updates a decision's message to show how it was resolved.
    fn mark_resolved(&self, m: &MessageRef, resolution: &str) -> Result<(), ChannelError>;
    /// Events received since the last call, oldest first.
    fn events(&self) -> Vec<ChannelEvent>;
    /// Whether the owner can be reached at all; false while no Slack tokens are held.
    fn connected(&self) -> bool {
        true
    }
    /// Why what the owner sends has kept failing to arrive, while it has.
    fn trouble(&self) -> Option<String> {
        None
    }
    /// The name the owner mentions the factory by in the channel, once the channel has said it.
    fn mention(&self) -> Option<String> {
        None
    }
}

/// The channel before Slack is connected: every send fails, so decisions stay recorded as unsent
/// and are delivered once a channel is.
pub struct Unconnected;

impl Channel for Unconnected {
    fn post_decision(&self, _: &DecisionRequest) -> Result<MessageRef, ChannelError> {
        Err(ChannelError(
            "Slack is not connected; connect it in the dashboard's settings".into(),
        ))
    }

    fn post_notice(&self, _: &Notice) -> Result<MessageRef, ChannelError> {
        Err(ChannelError(
            "Slack is not connected; connect it in the dashboard's settings".into(),
        ))
    }

    fn reply(&self, _: &MessageRef, _: &str) -> Result<MessageRef, ChannelError> {
        Err(ChannelError(
            "Slack is not connected; connect it in the dashboard's settings".into(),
        ))
    }

    fn attach(&self, _: &MessageRef, _: &str, _: &str) -> Result<(), ChannelError> {
        Err(ChannelError(
            "Slack is not connected; connect it in the dashboard's settings".into(),
        ))
    }

    fn tell(&self, _: &str, _: &str) -> Result<(), ChannelError> {
        Err(ChannelError(
            "Slack is not connected; connect it in the dashboard's settings".into(),
        ))
    }

    fn mark_resolved(&self, _: &MessageRef, _: &str) -> Result<(), ChannelError> {
        Ok(())
    }

    fn events(&self) -> Vec<ChannelEvent> {
        Vec::new()
    }

    fn connected(&self) -> bool {
        false
    }
}
