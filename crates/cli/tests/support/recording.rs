//! The owner's channel and the tracker as the tests play them: what the factory sent or asked is
//! kept, and the events and issues a test gives are handed out.

use std::collections::VecDeque;
use std::sync::Mutex;

use ovenspring_integrations::channel::{
    Channel, ChannelError, ChannelEvent, DecisionRequest, MessageRef, Notice,
};
use ovenspring_integrations::http::{HttpError, Unanswered};
use ovenspring_integrations::tracker::{Tracker, TrackerIssue, TrackerState};

/// What a [`RecordingChannel`] was asked to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sent {
    Decision(DecisionRequest),
    Notice(Notice),
    Reply {
        thread: String,
        text: String,
    },
    Attached {
        thread: String,
        name: String,
        content: String,
    },
    Resolved {
        ts: String,
        resolution: String,
    },
    Told {
        user: String,
        text: String,
    },
}

/// A channel that keeps what is sent and hands out events a test queues.
#[derive(Default)]
pub struct RecordingChannel {
    sent: Mutex<Vec<Sent>>,
    queued: Mutex<VecDeque<ChannelEvent>>,
}

impl RecordingChannel {
    pub fn sent(&self) -> Vec<Sent> {
        self.sent.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn push_event(&self, e: ChannelEvent) {
        if let Ok(mut q) = self.queued.lock() {
            q.push_back(e);
        }
    }

    fn record(&self, s: Sent) -> MessageRef {
        let mut sent = self
            .sent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        sent.push(s);
        MessageRef {
            channel: "C-TEST".into(),
            ts: format!("{}.000", sent.len()),
            permalink: Some(format!(
                "https://slack.test/archives/C-TEST/p{}",
                sent.len()
            )),
        }
    }
}

impl Channel for RecordingChannel {
    fn post_decision(&self, d: &DecisionRequest) -> Result<MessageRef, ChannelError> {
        Ok(self.record(Sent::Decision(d.clone())))
    }

    fn post_notice(&self, n: &Notice) -> Result<MessageRef, ChannelError> {
        Ok(self.record(Sent::Notice(n.clone())))
    }

    fn reply(&self, thread: &MessageRef, text: &str) -> Result<MessageRef, ChannelError> {
        Ok(self.record(Sent::Reply {
            thread: thread.ts.clone(),
            text: text.into(),
        }))
    }

    fn attach(&self, thread: &MessageRef, name: &str, content: &str) -> Result<(), ChannelError> {
        self.record(Sent::Attached {
            thread: thread.ts.clone(),
            name: name.into(),
            content: content.into(),
        });
        Ok(())
    }

    fn tell(&self, user: &str, text: &str) -> Result<(), ChannelError> {
        self.record(Sent::Told {
            user: user.into(),
            text: text.into(),
        });
        Ok(())
    }

    fn mark_resolved(&self, m: &MessageRef, resolution: &str) -> Result<(), ChannelError> {
        self.record(Sent::Resolved {
            ts: m.ts.clone(),
            resolution: resolution.into(),
        });
        Ok(())
    }

    fn events(&self) -> Vec<ChannelEvent> {
        self.queued
            .lock()
            .map(|mut q| q.drain(..).collect())
            .unwrap_or_default()
    }
}

/// A tracker that serves the issues a test gives it and keeps what it was asked to do.
#[derive(Default)]
pub struct RecordingTracker {
    pub calls: Mutex<Vec<(String, String)>>,
    pub issues: Mutex<Vec<TrackerIssue>>,
    /// While set, reading the open issues fails as if the tracker could not be reached.
    pub unreachable: std::sync::atomic::AtomicBool,
    /// While set, reading the open issues is refused, as with a revoked key.
    pub refused: std::sync::atomic::AtomicBool,
}

impl RecordingTracker {
    pub fn calls(&self) -> Vec<(String, String)> {
        self.calls.lock().map(|c| c.clone()).unwrap_or_default()
    }

    fn record(&self, key: &str, what: String) {
        if let Ok(mut c) = self.calls.lock() {
            c.push((key.to_owned(), what));
        }
    }
}

impl Tracker for RecordingTracker {
    fn open_issues(&self) -> Result<Vec<TrackerIssue>, HttpError> {
        if self.unreachable.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(HttpError::transient(
                Unanswered::NoConnection,
                "connection timed out".into(),
            ));
        }
        if self.refused.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(HttpError::Status {
                method: "POST",
                url: "https://api.linear.app/graphql".into(),
                status: 401,
                message: "Authentication required".into(),
            });
        }
        Ok(self.issues.lock().map(|i| i.clone()).unwrap_or_default())
    }

    fn comment(&self, key: &str, body: &str) -> Result<(), HttpError> {
        self.record(key, format!("comment: {body}"));
        Ok(())
    }

    fn start(&self, key: &str) -> Result<(), HttpError> {
        self.record(key, "start".into());
        Ok(())
    }

    fn set_state(&self, key: &str, state: TrackerState) -> Result<(), HttpError> {
        self.record(key, format!("state: {state:?}"));
        Ok(())
    }
}
