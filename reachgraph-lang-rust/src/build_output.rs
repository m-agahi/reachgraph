//! Reading a build that already happened — ADR-0009.
//!
//! reachgraph never runs a build (plan-03 §4 D-B). What ADR-0009 permits is
//! narrower and is all this module does: when the caller names a cargo
//! **target directory** that a previous `cargo build` or `cargo check`
//! populated, find each workspace member's build-script output directory in
//! it, so the engine can put the generated code into the crate graph.
//!
//! Every function here is a pure read of the filesystem. Nothing is spawned,
//! nothing is written, and nothing is compiled.
//!
//! # How an output directory is found, and why that way
//!
//! Cargo runs a build script in `<target>/<profile>/build/<pkg>-<hash>/` and
//! records the `OUT_DIR` it handed the script in a `root-output` file beside
//! it. That file is cargo's own statement of where the output went, so it is
//! read rather than the path reconstructed: a directory name is a convention,
//! `root-output` is the record.
//!
//! A package can have several such directories — one per profile, and one
//! per feature set or toolchain a build was run with. The most recently
//! modified `root-output` wins, because it is the build the caller ran last.
//! Which one was taken is reported, never silently chosen.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// The build-script output directory cargo recorded for `package` under
/// `target`, or `None` when no build of that package ran a build script there.
///
/// A recorded directory that is gone, or that holds no `.rs` file, is not an
/// output directory for this purpose: it has nothing the crate graph could
/// load.
pub fn locate_out_dir(target: &Path, package: &str) -> Option<PathBuf> {
    let prefixes = [
        format!("{}-", package.replace('-', "_")),
        format!("{package}-"),
    ];
    let mut best: Option<(SystemTime, PathBuf)> = None;

    let profiles = std::fs::read_dir(target).ok()?;
    for profile in profiles.flatten() {
        let Ok(builds) = std::fs::read_dir(profile.path().join("build")) else {
            continue;
        };
        for build in builds.flatten() {
            let name = build.file_name().to_string_lossy().into_owned();
            let Some(hash) = prefixes.iter().find_map(|p| name.strip_prefix(p.as_str())) else {
                continue;
            };
            // `yadgar-task-<hash>` must not match a package named
            // `yadgar-task-db`: the remainder after the prefix is the hash
            // alone, and a hash has no separator in it.
            if hash.contains('-') || hash.contains('_') {
                continue;
            }
            let record = build.path().join("root-output");
            let Ok(recorded) = std::fs::read_to_string(&record) else {
                continue;
            };
            let out_dir = PathBuf::from(recorded.trim());
            if !holds_rust_source(&out_dir) {
                continue;
            }
            let modified = std::fs::metadata(&record)
                .and_then(|meta| meta.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            if best.as_ref().is_none_or(|(when, _)| modified > *when) {
                best = Some((modified, out_dir));
            }
        }
    }
    best.map(|(_, dir)| dir)
}

fn holds_rust_source(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries
            .flatten()
            .any(|entry| entry.path().extension().is_some_and(|ext| ext == "rs"))
    })
}
