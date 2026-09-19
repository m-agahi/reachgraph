//! Plan-06 §3.2 and §4.1 — the mechanical guards.

use std::path::{Path, PathBuf};

/// Every non-test, non-build-script Rust source under a directory.
pub fn rust_sources(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return found;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(rust_sources(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs")
            && path.file_name().is_some_and(|name| name != "build.rs")
        {
            found.push(path);
        }
    }

    found.sort();
    found
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crate has a parent")
        .to_path_buf()
}

fn crate_sources(name: &str) -> Vec<PathBuf> {
    rust_sources(&workspace_root().join(name).join("src"))
}

/// ADR-0008: `if is_rust_project(root)` belongs in neither the waist nor the
/// binary.
///
/// **The token list is deliberately narrow and must not be widened to `cargo`,
/// `rustc` or `.rs`** (plan-06 §3.2). Those appear legitimately in doc
/// comments, error strings and this project's own help text, so a broad scan
/// false-positives immediately — and a guard that cries wolf gets disabled,
/// after which there is no guard.
///
/// **Comment lines are excluded, and the exclusion was MEASURED rather than
/// assumed.** Run over whole files, this guard fires three times on code that
/// obeys it: `reachgraph-core/src/schema.rs` documents a position encoding as
/// "`ra_ap`, tree-sitter, SCIP", and two cli modules quote ADR-0008's
/// forbidden `if is_rust_project(root)` line in order to say it is forbidden.
/// A guard that fails on the citation of the rule it enforces is the
/// cries-wolf failure §3.2 names, arriving by a different door — so what is
/// scanned is what compiles.
#[test]
fn no_language_specific_tokens_in_core_or_cli() {
    const FORBIDDEN: [&str; 4] = ["is_rust_project", "ra_ap", "rust_analyzer", "Cargo.toml"];

    let mut offenders: Vec<String> = Vec::new();
    for crate_name in ["reachgraph-core", "reachgraph-cli"] {
        for file in crate_sources(crate_name) {
            let text = std::fs::read_to_string(&file).expect("a source file is readable");
            for (number, line) in text.lines().enumerate() {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                for token in FORBIDDEN {
                    if line.contains(token) {
                        offenders.push(format!("{}:{}: {token}", file.display(), number + 1));
                    }
                }
            }
        }
    }

    assert!(offenders.is_empty(), "{offenders:?}");
}

/// ADR-0001's "no external binaries, no subprocesses", as a build failure.
///
/// **An allowlist, not a scope narrowing.** Plan-06 §4.1 writes this guard as
/// "`std::process::Command` appears in no first-party non-test source", and
/// PR D falsified that: `reachgraph-lang-rust` probes `cargo --version`
/// deliberately, because design.md §10 MEASURED that a name resolving on PATH
/// proves nothing — `rust-analyzer` resolves there as a `rustup` proxy that
/// loops and is not installed. Preflight has to *run* the program to learn
/// anything, and ADR-0001's carve-out permits the target language's own
/// toolchain.
///
/// Narrowing the guard to core and cli would hide that site. Naming it makes
/// the guard sharper instead: a **new** spawn anywhere in the workspace fails
/// here, and moving the probe fails here too.
#[test]
fn no_process_spawn_in_workspace_outside_the_one_documented_probe() {
    const ALLOWED: &str = "reachgraph-lang-rust/src/engine.rs";

    let mut offenders: Vec<String> = Vec::new();
    let mut allowed_site_found = false;

    for crate_name in [
        "reachgraph-plugin-api",
        "reachgraph-core",
        "reachgraph-fixture",
        "reachgraph-lang-rust",
        "reachgraph-roots-proto-tonic",
        "reachgraph-cli",
    ] {
        for file in crate_sources(crate_name) {
            let text = std::fs::read_to_string(&file).expect("a source file is readable");
            if !text.contains("process::Command") {
                continue;
            }
            let relative = file
                .strip_prefix(workspace_root())
                .expect("the file is inside the workspace")
                .to_string_lossy()
                .replace('\\', "/");
            if relative == ALLOWED {
                allowed_site_found = true;
            } else {
                offenders.push(relative);
            }
        }
    }

    assert!(offenders.is_empty(), "a new subprocess site: {offenders:?}");
    assert!(
        allowed_site_found,
        "{ALLOWED} no longer spawns a process — delete the allowance rather than leaving it \
         to excuse a future one"
    );
}

/// Plan-06 §3: a fixture plugin reachable from a shipped binary would let a
/// hand-written JSON file masquerade as an analysis.
///
/// **Stronger than the feature flag plan-06 §3 sketches.** `reachgraph-fixture`
/// is a dev-dependency of this crate, so no feature combination reaches it: the
/// release binary cannot register it because it cannot link it. A feature that
/// is merely off by default can be switched on.
#[test]
fn the_fixture_plugin_is_not_a_shipped_dependency() {
    let manifest =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
            .expect("this crate's manifest is readable");

    let (normal, dev) = manifest
        .split_once("[dev-dependencies]")
        .expect("this crate declares dev-dependencies");

    assert!(
        !normal.contains("reachgraph-fixture"),
        "the fixture is a normal dependency of the binary"
    );
    assert!(
        dev.contains("reachgraph-fixture"),
        "the fixture is still reachable from the test harness"
    );
}

/// Plan-06 §3.2, with the direct-versus-transitive precision the plan asks for.
/// `ra_ap_*` is present transitively through the language plugin and always
/// will be; asserting otherwise would assert something false.
#[test]
fn the_engine_is_not_a_direct_dependency_of_the_binary() {
    let manifest =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
            .expect("this crate's manifest is readable");

    assert!(
        !manifest.contains("ra_ap"),
        "the binary names the engine directly"
    );
}
