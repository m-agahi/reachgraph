//! Classification — plan-03 §10, ADR-0008 leak 8.
//!
//! Four of the five categories are **queries the engine already answers**, not
//! path prefixes. design.md §8's measured table is the evidence that the five
//! categories are the right five; turning it into a literal prefix list would
//! be a bug, because two of its five prefixes are properties of the author's
//! machine rather than of Rust. `/nix/store/…rust-lib-src/` is the same file
//! that lives under `~/.rustup/toolchains/<toolchain>/lib/rustlib/src/rust/`
//! on a rustup install and under `/usr/lib/rustlib/src/` on a distribution
//! package.
//!
//! So the sysroot arrives here as **data** — the resolved source root the
//! project model reported — and no toolchain path appears in this file.

use std::path::{Component, Path};

use reachgraph_plugin_api::Category;

/// Where a file's crate sits relative to the workspace.
///
/// Membership is the engine's answer, carried as data so the rule can be
/// tested without one.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum CrateOrigin {
    /// A workspace member, identified by its **package**.
    ///
    /// The package, not the unit: plan-03 §7 emits one `Unit` per target kind,
    /// so a package with a library and an integration test is two units over
    /// one `src/`. Comparing unit ids would classify that package's own
    /// `src/lib.rs` as `WorkspaceSibling` while indexing its test unit, which
    /// is wrong — a package's own source is first-party to every one of its
    /// targets.
    Member {
        /// The package id, target qualifier stripped.
        package: String,
    },
    /// A registry, git or path dependency outside the workspace, or anything
    /// else the project model did not report as a member.
    NotAMember,
}

/// Everything the classification rule reads.
///
/// A struct rather than five parameters because the rule is the thing under
/// test and a caller that forgets an argument should not compile.
#[derive(Clone, Debug)]
pub struct PathFacts<'a> {
    /// The file being classified.
    pub path: &'a Path,
    /// The resolved sysroot **source** root, when one resolved.
    ///
    /// `None` is the measured `rust-src`-absent case (plan-03 §9, §11 check 4),
    /// and it is why this is an `Option` rather than a path with a fallback:
    /// an absent sysroot source root must make stdlib files unclassifiable,
    /// not accidentally third-party-by-default through a guessed path.
    pub sysroot_src: Option<&'a Path>,
    /// Where the file's crate sits.
    pub origin: CrateOrigin,
    /// The package of the unit being indexed, target qualifier stripped.
    pub unit_package: &'a str,
    /// Output directories the workspace load actually recorded.
    ///
    /// Distinct from the structural rule below: a workspace may put its target
    /// directory anywhere (`CARGO_TARGET_DIR`), in which case the shape
    /// `target/<profile>/build/<pkg>-<hash>/out/` does not appear and only the
    /// recorded directory identifies the file.
    pub recorded_out_dirs: &'a [&'a Path],
}

/// The rule, in plan-03 §10's order.
///
/// Order is load-bearing and is not alphabetical. `Generated` precedes the two
/// membership rules because a generated file's crate **is** a workspace member
/// — the file is compiled into it — and reporting it as first-party would hide
/// the one distinction design.md §8 measured as useful.
pub fn classify_facts(facts: &PathFacts<'_>) -> Category {
    if let Some(sysroot_src) = facts.sysroot_src {
        if facts.path.starts_with(sysroot_src) {
            return Category::Stdlib;
        }
    }
    if is_generated(facts.path, facts.recorded_out_dirs) {
        return Category::Generated;
    }
    match &facts.origin {
        CrateOrigin::Member { package } if package == facts.unit_package => Category::FirstParty,
        CrateOrigin::Member { .. } => Category::WorkspaceSibling,
        CrateOrigin::NotAMember => Category::ThirdParty,
    }
}

/// The one genuine path rule, and it is genuinely Cargo-shaped.
///
/// `target/<profile>/build/<pkg>-<hash>/out/…` — matched on the **shape** of
/// five consecutive components rather than on a prefix, so it holds wherever
/// the target directory lives and at any depth. `target/<profile>/deps/…` is
/// not generated code; it is compiled output, and the shape excludes it.
fn is_generated(path: &Path, recorded_out_dirs: &[&Path]) -> bool {
    if recorded_out_dirs.iter().any(|dir| path.starts_with(dir)) {
        return true;
    }
    let components: Vec<&std::ffi::OsStr> = path
        .components()
        .filter_map(|c| match c {
            Component::Normal(name) => Some(name),
            _ => None,
        })
        .collect();
    components.windows(5).any(|window| {
        window[0] == "target"
            && window[2] == "build"
            && window[4] == "out"
            && window[3].to_string_lossy().contains('-')
    })
}
