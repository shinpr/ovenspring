//! Clients for the services only the daemon talks to.
#![cfg_attr(
    test,
    allow(
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used,
        clippy::expect_used,
        reason = "Test helpers use concise assertions; production paths remain strict."
    )
)]

pub mod channel;
pub mod comments;
pub mod github;
pub mod http;
pub mod tracker;
