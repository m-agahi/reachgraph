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
//! Cargo runs a build script in `<target>/<profile>/build/<pkg>-<hash>/` — or
//! `<target>/<triple>/<profile>/build/…` for a `--target` build — and records
//! the `OUT_DIR` it handed the script in a `root-output` file beside it. That
//! file is cargo's own statement of where the output went, so it is read
//! rather than the path reconstructed. When the record is gone (MEASURED in a
//! partly cleaned musl target dir: `out/` present, `root-output` absent) the
//! run directory's own `out/` is taken instead.
//!
//! A package can have several such directories — one per profile, target and
//! feature set a build was run with. The most recently run one wins, because
//! it is the build the caller ran last, and how many there were is returned so
//! the choice is reported, never silent.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// The build-script output directory for `package` under `target`, newest
/// first, or `None` when no build of that package ran a build script there.
pub fn locate_out_dir(target: &Path, package: &str) -> Option<PathBuf> {
    locate_out_dirs(target, package).into_iter().next()
}

/// Every build-script output directory for `package` under `target`, the most
/// recently run first.
///
/// An output directory that is gone, or that holds no `.rs` file, is not one
/// for this purpose: it has nothing the crate graph could load.
pub fn locate_out_dirs(target: &Path, package: &str) -> Vec<PathBuf> {
    let prefixes = [
        format!("{}-", package.replace('-', "_")),
        format!("{package}-"),
    ];
    let mut found: Vec<(SystemTime, PathBuf)> = Vec::new();

    for build_dir in build_dirs(target) {
        let Ok(builds) = std::fs::read_dir(&build_dir) else {
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
            let run = build.path();
            let out_dir = match std::fs::read_to_string(run.join("root-output")) {
                Ok(recorded) => PathBuf::from(recorded.trim()),
                Err(_) => run.join("out"),
            };
            if !holds_rust_source(&out_dir) {
                continue;
            }
            let ran = ran_at(&run).unwrap_or(SystemTime::UNIX_EPOCH);
            if !found.iter().any(|(_, dir)| *dir == out_dir) {
                found.push((ran, out_dir));
            }
        }
    }
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    found.into_iter().map(|(_, dir)| dir).collect()
}

/// `<target>/<profile>/build` and `<target>/<triple>/<profile>/build`.
fn build_dirs(target: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let Ok(level1) = std::fs::read_dir(target) else {
        return dirs;
    };
    for entry in level1.flatten() {
        let path = entry.path();
        if path.join("build").is_dir() {
            dirs.push(path.join("build"));
        }
        if let Ok(level2) = std::fs::read_dir(&path) {
            for inner in level2.flatten() {
                let build = inner.path().join("build");
                if build.is_dir() {
                    dirs.push(build);
                }
            }
        }
    }
    dirs
}

/// When the build script in `run` last ran: its `output` file, which cargo
/// rewrites on every run, then `root-output`, then the directory itself.
fn ran_at(run: &Path) -> Option<SystemTime> {
    ["output", "root-output", "out"].iter().find_map(|name| {
        std::fs::metadata(run.join(name))
            .and_then(|m| m.modified())
            .ok()
    })
}

fn holds_rust_source(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries
            .flatten()
            .any(|entry| entry.path().extension().is_some_and(|ext| ext == "rs"))
    })
}

/// The newest input of the build script behind `out_dir` that changed after
/// the script last ran, or `None` when the output is at least as new as every
/// input.
///
/// The inputs are the ones the script declared with `rerun-if-changed`, read
/// from cargo's `output` record and resolved against `package_dir`, because
/// those are what cargo itself reruns the script for: an edit to an unrelated
/// source file does not regenerate `OUT_DIR`, and calling the output stale for
/// it would be a false alarm on every run. With nothing declared, cargo reruns
/// on any change in the package, so every package file counts except
/// `target/` and `.git/`.
pub fn stale_inputs(out_dir: &Path, package_dir: &Path) -> Option<PathBuf> {
    let run = out_dir.parent()?;
    let ran = ran_at(run)?;

    let declared: Vec<PathBuf> = std::fs::read_to_string(run.join("output"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            line.strip_prefix("cargo:rerun-if-changed=")
                .or_else(|| line.strip_prefix("cargo::rerun-if-changed="))
        })
        .map(|path| package_dir.join(path.trim()))
        .collect();
    let roots = if declared.is_empty() {
        vec![package_dir.to_path_buf()]
    } else {
        declared
    };

    let mut newest: Option<(SystemTime, PathBuf)> = None;
    for root in roots {
        newest_file(&root, &mut newest);
    }
    newest
        .filter(|(modified, _)| *modified > ran)
        .map(|(_, path)| path)
}

fn newest_file(path: &Path, newest: &mut Option<(SystemTime, PathBuf)>) {
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    if meta.is_dir() {
        if path
            .file_name()
            .is_some_and(|name| name == "target" || name == ".git")
        {
            return;
        }
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                newest_file(&entry.path(), newest);
            }
        }
        return;
    }
    if let Ok(modified) = meta.modified() {
        if newest.as_ref().is_none_or(|(when, _)| modified > *when) {
            *newest = Some((modified, path.to_path_buf()));
        }
    }
}
