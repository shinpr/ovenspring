//! Embeds the factory's files (`factory/`) in the binary.

#[path = "../embed.rs"]
mod embed;

use std::io;
use std::path::Path;

fn main() -> io::Result<()> {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").map_err(io::Error::other)?;
    let out = std::env::var("OUT_DIR").map_err(io::Error::other)?;
    embed::write(
        "FACTORY",
        &Path::new(&manifest).join("../../factory"),
        &Path::new(&out).join("factory.rs"),
    )
}
