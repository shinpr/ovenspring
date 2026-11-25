//! Embeds the built dashboard (`dashboard/dist`) in the binary. Where it has not been built (a
//! Rust-only check), a page saying so is embedded instead, so the server still starts.

#[path = "../embed.rs"]
mod embed;

use std::fs;
use std::io;
use std::path::Path;

fn main() -> io::Result<()> {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").map_err(io::Error::other)?;
    let out = Path::new(&std::env::var("OUT_DIR").map_err(io::Error::other)?).to_path_buf();
    let dist = Path::new(&manifest).join("../../dashboard/dist");
    let source = if dist.join("index.html").is_file() {
        dist
    } else {
        // Watched, so building the dashboard later embeds it.
        println!("cargo::rerun-if-changed={}", dist.display());
        let missing = out.join("dashboard-missing");
        fs::create_dir_all(&missing)?;
        fs::write(
            missing.join("index.html"),
            "<!doctype html><meta charset=\"utf-8\"><title>Oven Spring</title>\
             <p>This build of Oven Spring does not include its dashboard; build it with \
             <code>pnpm run build</code> in <code>dashboard/</code>, then build Oven Spring.</p>\n",
        )?;
        missing
    };
    embed::write("DASHBOARD", &source, &out.join("dashboard.rs"))
}
