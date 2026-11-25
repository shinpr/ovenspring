//! Build-time embedding of a directory, shared by the build scripts that include it with
//! `#[path]`: a generated Rust source lists every file under the directory with its bytes, and the
//! crate `include!`s it.

use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Writes to `out` a `static <name>: &[(&str, &[u8])]` holding every file under `dir`, by its path
/// relative to `dir` with `/` between components, in path order; cargo rebuilds the crate when
/// anything under `dir` changes.
pub fn write(name: &str, dir: &Path, out: &Path) -> io::Result<()> {
    let dir = fs::canonicalize(dir)?;
    println!("cargo::rerun-if-changed={}", dir.display());
    let mut files = Vec::new();
    collect(&dir, &dir, &mut files)?;
    files.sort();
    let mut source = format!("static {name}: &[(&str, &[u8])] = &[\n");
    for (relative, path) in files {
        let path = path.display().to_string();
        writeln!(source, "    ({relative:?}, include_bytes!({path:?})),")
            .map_err(io::Error::other)?;
    }
    source.push_str("];\n");
    fs::write(out, source)
}

fn collect(base: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(base, &path, out)?;
        } else {
            let relative = path
                .strip_prefix(base)
                .map_err(io::Error::other)?
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            out.push((relative, path));
        }
    }
    Ok(())
}
