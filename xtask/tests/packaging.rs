//! Repository-level packaging guards — plan-07 §8.2.
//!
//! Plan-07 §8 says plainly that packaging correctness is verified by
//! integration checks rather than unit tests, and most of it is: the wheel's
//! contents, its size against PyPI's per-file limit and the version match
//! between metadata and `--version` are shell assertions in
//! `.github/workflows/release.yaml`, because they need a wheel to exist.
//!
//! **Five of those checks need no wheel, and those are the ones here.** Each
//! reads a file that is in git and asserts a fact the release depends on. They
//! live in `xtask` because it is the dev-side crate — a dependency of nothing
//! shipped — and because `cargo test --workspace` already runs it.
//!
//! They are written as tests rather than as a CI script for the reason plan-07
//! §8 gives for the rest: a check that only exists in a workflow is a check
//! nobody runs before pushing.

use std::path::{Path, PathBuf};

use serde::Deserialize;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask sits one level below the workspace root")
        .to_path_buf()
}

fn read(relative: &str) -> String {
    let path = workspace_root().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{relative} is readable: {error}"))
}

/// The `[[package]]` entries of `Cargo.lock`, which is the only place a
/// transitive version is written down.
#[derive(Debug, Deserialize)]
struct Lockfile {
    package: Vec<LockedPackage>,
}

#[derive(Debug, Deserialize)]
struct LockedPackage {
    name: String,
    version: String,
}

fn lockfile() -> Lockfile {
    toml::from_str(&read("Cargo.lock")).expect("Cargo.lock parses")
}

/// The vendored-JavaScript table — `reachgraph-render-html/vendor/VENDOR.toml`.
#[derive(Debug, Deserialize)]
struct VendoredJs {
    bundle: Vec<VendoredBundle>,
}

#[derive(Debug, Deserialize)]
struct VendoredBundle {
    name: String,
    version: String,
    spdx: String,
}

fn vendored_js() -> VendoredJs {
    toml::from_str(&read("reachgraph-render-html/vendor/VENDOR.toml")).expect("VENDOR.toml parses")
}

/// ADR-0001's 2026-09-19 amendment. `vendor/` is NOT committed, so the
/// committed `Cargo.lock` is the whole of this repository's determinism — and
/// two of the versions it holds are load-bearing in a way no manifest can
/// express, because neither crate is a direct dependency.
///
/// Both were MEASURED 2026-09-19 by a build that failed before them, and both
/// break one patch release later. The root `Cargo.toml` records the reasoning;
/// this asserts it.
///
/// **This is the guard that replaces the vendor tree.** Before the amendment,
/// `cargo vendor` plus `--offline` made a `cargo update` physically unable to
/// change what was compiled. Without the tree, `--locked` refuses the update
/// and this test refuses the lockfile edit that would otherwise sail through
/// review as a version bump.
#[test]
fn the_lockfile_holds_the_two_transitive_pins() {
    let locked = lockfile();

    for (name, expected) in [("salsa", "0.28.2"), ("unicode-ident", "1.0.24")] {
        let found: Vec<&LockedPackage> = locked
            .package
            .iter()
            .filter(|package| package.name == name)
            .collect();

        assert_eq!(
            found.len(),
            1,
            "Cargo.lock holds {} entries for `{name}`; the pin is only meaningful when there is exactly one",
            found.len()
        );
        assert_eq!(
            found[0].version, expected,
            "`{name}` is locked at {} and must be {expected}. \
             The root Cargo.toml records what breaks at the next patch: \
             salsa 0.28.3 changed `IngredientImpl::intern`'s arity (E0061 inside \
             ra_ap_span's hygiene.rs), and unicode-ident 1.0.25 disagrees with \
             unicode-properties 0.1.4 about the Unicode version (E0080 in \
             ra-ap-rustc_lexer's compile-time assertion).",
            found[0].version
        );
    }
}

/// Plan-07 §8.2's `vendored_js_attributed`. The binary compiles the bundles in
/// with `include_str!` and the artifact writes them out, so shipping them is
/// redistribution and the notice must travel — and `cargo-about` cannot see
/// them, because they are not crates.
///
/// `THIRD-PARTY-LICENSES.md` is generated (plan-07 §6.2) and this asserts the
/// JavaScript half of it against `VENDOR.toml`, which is the file an upstream
/// bump edits.
#[test]
fn every_vendored_bundle_is_attributed() {
    let attribution = read("THIRD-PARTY-LICENSES.md");
    let table = vendored_js();

    assert!(
        !table.bundle.is_empty(),
        "VENDOR.toml lists no bundles, so this guard would assert over nothing"
    );

    for bundle in &table.bundle {
        let entry = format!("{} {}", bundle.name, bundle.version);
        assert!(
            attribution.contains(&entry),
            "THIRD-PARTY-LICENSES.md does not mention `{entry}`. \
             A vendored bundle whose notice does not travel with the wheel is \
             a licence breach, not a formatting miss — regenerate with \
             `cargo xtask third-party-licenses`."
        );
        assert!(
            attribution.contains(&bundle.spdx),
            "THIRD-PARTY-LICENSES.md does not carry `{}`, the SPDX of `{entry}`",
            bundle.spdx
        );
    }
}

/// Plan-07 §6.3 and ADR-0001. crabviz is AGPL-3.0 and the product is a single
/// linked binary, so vendoring any of its source would place the whole
/// artifact under AGPL-3.0 — a licence incompatible with the MIT OR Apache-2.0
/// this project distributes under.
///
/// The architecture is borrowed openly and `docs/` says so; that is where the
/// credit belongs and it is why `docs/` is exempt. Everything a build reads is
/// not.
#[test]
fn crabviz_appears_in_nothing_a_build_reads() {
    let root = workspace_root();
    let mut checked = 0_usize;

    let mut manifests = Vec::new();
    collect_manifests(&root, &mut manifests);
    manifests.push(root.join("Cargo.lock"));

    for path in &manifests {
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("{} is readable: {error}", path.display()));
        checked += 1;
        assert!(
            !text.to_ascii_lowercase().contains("crabviz"),
            "`crabviz` appears in {}. It is AGPL-3.0 and legally unavailable to \
             a single linked binary distributed as MIT OR Apache-2.0 (ADR-0001). \
             The architecture may be borrowed in docs/ prose; the code may not \
             be depended on.",
            path.display()
        );
    }

    assert!(
        checked > 5,
        "the walk found {checked} files, so this guard asserts over almost nothing"
    );
}

/// Every `Cargo.toml` and `Cargo.lock` a build of this workspace reads.
///
/// `target/` is skipped because it holds every dependency's own manifest, and
/// the fixture workspaces under `reachgraph-lang-rust/tests/fixtures` are
/// included deliberately: they are real Cargo workspaces this repository
/// commits, so a dependency added there is a dependency committed here.
fn collect_manifests(directory: &Path, found: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("{} is readable: {error}", directory.display()));

    for entry in entries {
        let path = entry.expect("an entry is readable").path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();

        if path.is_dir() {
            if name == "target" || name == ".git" || name == "vendor" || name == "docs" {
                continue;
            }
            collect_manifests(&path, found);
        } else if name == "Cargo.toml" {
            found.push(path);
        }
    }
}

/// Plan-07 §1.2's decision, asserted from the repository side.
///
/// `bindings = "bin"` and `[project.scripts]` are alternatives rather than
/// partners: a console script is a Python stub that imports a module, and
/// ADR-0002 makes plugins compile-time Rust crates, so no module exists for
/// one to import. Declaring both under the name `reachgraph` produces two
/// files claiming `bin/reachgraph`.
///
/// The wheel-side half of this — no `.py` anywhere in the archive — is in the
/// release workflow, because it needs a wheel.
#[test]
fn the_wheel_is_bin_only_and_declares_no_console_script() {
    let pyproject: toml::Value =
        toml::from_str(&read("pyproject.toml")).expect("pyproject.toml parses");

    let bindings = pyproject
        .get("tool")
        .and_then(|tool| tool.get("maturin"))
        .and_then(|maturin| maturin.get("bindings"))
        .and_then(toml::Value::as_str);

    assert_eq!(
        bindings,
        Some("bin"),
        "[tool.maturin] bindings must be \"bin\": the wheel carries an executable \
         and no Python (plan-07 §1.1)"
    );

    assert!(
        pyproject
            .get("project")
            .and_then(|project| project.get("scripts"))
            .is_none(),
        "pyproject.toml declares [project.scripts] alongside bindings = \"bin\". \
         Those are alternatives, not partners — both generate something claiming \
         bin/reachgraph (plan-07 §1.2)."
    );
}

/// The manifest maturin builds has to be a file that exists.
///
/// This guard is here because plan-07 §1.1 wrote
/// `manifest-path = "crates/reachgraph-cli/Cargo.toml"` and there is no
/// `crates/` directory in this workspace. A path that names nothing fails at
/// release time, which is the worst moment to find out.
#[test]
fn the_maturin_manifest_path_names_the_binary_crate() {
    let pyproject: toml::Value =
        toml::from_str(&read("pyproject.toml")).expect("pyproject.toml parses");

    let manifest = pyproject
        .get("tool")
        .and_then(|tool| tool.get("maturin"))
        .and_then(|maturin| maturin.get("manifest-path"))
        .and_then(toml::Value::as_str)
        .expect("[tool.maturin] declares manifest-path");

    let path = workspace_root().join(manifest);
    assert!(
        path.is_file(),
        "[tool.maturin] manifest-path is `{manifest}`, which is not a file"
    );

    let crate_manifest = std::fs::read_to_string(&path).expect("the manifest is readable");
    assert!(
        crate_manifest.contains("name = \"reachgraph\""),
        "{manifest} declares no `[[bin]] name = \"reachgraph\"`, so the wheel \
         would install an executable under another name"
    );
}
