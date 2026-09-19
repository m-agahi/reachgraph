//! The output directory — plan-06 §1.1.
//!
//! The artifact is **regenerated**, never merged into. A directory holding a
//! previous run's shards for roots this run does not have would serve a reader
//! files that describe a repository state nobody analysed, so the artifact this
//! tool owns is removed before the new one lands. A file this tool does not own
//! is never removed, and a directory containing one is refused unless the user
//! says otherwise.

use std::io;
use std::path::Path;

/// The files a run writes, and the only ones it may delete.
const OWNED_FILES: [&str; 4] = [
    "endpoints.json",
    "unreachable.json",
    "versions.json",
    "run.json",
];

/// The shard directory, removed wholesale because its contents are named after
/// roots and a stale name is invisible to any per-file rule.
const OWNED_DIRECTORY: &str = "graph";

/// Refuse a directory this tool did not produce, unless `force`.
pub fn check(out: &Path, force: bool) -> Result<(), String> {
    if force || !out.exists() {
        return Ok(());
    }

    if !out.is_dir() {
        return Err(format!("{} is not a directory", out.display()));
    }

    let Ok(entries) = std::fs::read_dir(out) else {
        return Err(format!("{} cannot be read", out.display()));
    };

    let names: Vec<String> = entries
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();

    if names.is_empty() || names.iter().all(is_ours) {
        return Ok(());
    }

    Err(format!(
        "{} holds files reachgraph did not write; the artifact is regenerated rather than \
         merged into, so pick an empty directory or pass --force",
        out.display()
    ))
}

/// Remove the previous artifact, and nothing else.
pub fn clear(out: &Path) -> io::Result<()> {
    if !out.exists() {
        return std::fs::create_dir_all(out);
    }

    for name in OWNED_FILES {
        let path = out.join(name);
        if path.is_file() {
            std::fs::remove_file(path)?;
        }
    }

    let shards = out.join(OWNED_DIRECTORY);
    if shards.is_dir() {
        std::fs::remove_dir_all(shards)?;
    }

    Ok(())
}

fn is_ours(name: &String) -> bool {
    name == OWNED_DIRECTORY || OWNED_FILES.contains(&name.as_str())
}
