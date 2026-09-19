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

use reachgraph_plugin_api::Renderer;

/// The files the **waist** writes, and the only ones it may delete.
const WAIST_FILES: [&str; 4] = [
    "endpoints.json",
    "unreachable.json",
    "versions.json",
    "run.json",
];

/// The shard directory, removed wholesale because its contents are named after
/// roots and a stale name is invisible to any per-file rule.
const WAIST_DIRECTORY: &str = "graph";

/// What one run owns: the waist's files plus whatever the selected renderer
/// says it writes.
///
/// **The renderer's half is asked for rather than known** (`Renderer::owns`).
/// A binary holding a hardcoded list of one renderer's paths would leave
/// another renderer's files behind on a re-run, and the reader would open a
/// page from the run before last with no sign that anything was stale.
///
/// `None` is a build with no output format compiled in. The waist's own files
/// are still owned and still regenerated — a page is an addition to the
/// artifact, never a precondition for it. A page left by a build that HAD a
/// renderer is deliberately not removed by one that does not: this run cannot
/// know what that renderer owned, and deleting by guess is how a file nobody
/// wrote gets removed.
pub fn owned(renderer: Option<&dyn Renderer>) -> Vec<String> {
    let mut names: Vec<String> = WAIST_FILES.iter().map(|name| (*name).to_owned()).collect();
    names.push(WAIST_DIRECTORY.to_owned());
    let Some(renderer) = renderer else {
        return names;
    };
    for name in renderer.owns() {
        let name = (*name).to_owned();
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// Refuse a directory this tool did not produce, unless `force`.
pub fn check(out: &Path, owned: &[String], force: bool) -> Result<(), String> {
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

    let mut foreign: Vec<&str> = names
        .iter()
        .filter(|name| !owned.contains(name))
        .map(String::as_str)
        .collect();
    if foreign.is_empty() {
        return Ok(());
    }
    foreign.sort_unstable();

    // NOT "files reachgraph did not write", which is a provenance claim this
    // function cannot support. MEASURED: run the default build into a
    // directory, then a build without the `render-html` feature into the same
    // one — `Renderer::owns` no longer names `index.html`, `loader.js`,
    // `structure.json` or `vendor/`, so all four read as foreign and the user
    // was told reachgraph had not written four files reachgraph had just
    // written. Refusing is still right: this build cannot regenerate what it
    // does not own, and deleting by guess removes a file nobody wrote. What
    // this function knows is OWNERSHIP BY THIS BUILD, so that is what it says,
    // and it names the files so the claim is checkable.
    Err(format!(
        "{} holds files this build does not own: {}. The artifact is regenerated rather than \
         merged into, so pick an empty directory or pass --force. A build with a different \
         renderer compiled in owns different names, so a page it left reads as foreign from \
         here.",
        out.display(),
        foreign.join(" ")
    ))
}

/// Remove the previous artifact, and nothing else.
pub fn clear(out: &Path, owned: &[String]) -> io::Result<()> {
    if !out.exists() {
        return std::fs::create_dir_all(out);
    }

    for name in owned {
        let path = out.join(name);
        if path.is_file() {
            std::fs::remove_file(path)?;
        } else if path.is_dir() {
            std::fs::remove_dir_all(path)?;
        }
    }

    Ok(())
}
