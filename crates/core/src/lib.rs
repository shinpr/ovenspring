//! Domain types and the rules that need no I/O: configuration, the step flow and the merge gate.

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

pub mod config;
pub mod flow;
pub mod gate;
pub mod item;
pub mod output;
pub mod phase;
pub mod plan;
pub mod sprint;
pub mod step;
pub mod wildcard;
