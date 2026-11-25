//! Agent launching: embedded factory assets, prompt assembly, CLI backends and the guard hook.

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

pub mod assets;
pub mod backend;
pub mod guard;
pub mod hook;
pub mod mcp;
pub mod prices;
pub mod prompt;
