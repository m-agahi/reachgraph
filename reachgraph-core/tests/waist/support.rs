//! Shared helpers for the waist's test suite.
//!
//! Every test in this crate runs against the fixture corpus (ADR-0008,
//! plan-01 §10.3): no `cargo metadata`, no indexing, no timing variance, and
//! no requirement that any repository was ever built.

use std::path::{Path, PathBuf};

use reachgraph_core::{BuildInputs, BuildOptions, Index};
use reachgraph_fixture::FixturePlugin;

/// The corpus directory, reached from this crate rather than from the
/// fixture's own manifest directory.
pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("reachgraph-core sits one level below the workspace root")
        .join("reachgraph-fixture")
        .join("fixtures")
}

/// One corpus case, loaded.
pub fn case(name: &str) -> FixturePlugin {
    FixturePlugin::load(fixtures_dir().join(name))
        .unwrap_or_else(|error| panic!("{name} should load: {error}"))
}

/// The four slices a single self-paired plugin fills.
///
/// A named helper rather than a tuple: plan-01 §4.2 partitions by capability,
/// and the fixture declares every capability, so one plugin lands in all four.
pub fn inputs(plugin: &FixturePlugin) -> BuildInputs<'_> {
    BuildInputs {
        symbols: vec![plugin],
        edges: vec![plugin],
        roots: vec![plugin],
        classifiers: vec![plugin],
    }
}

/// Build one case with the default options.
pub fn build(plugin: &FixturePlugin) -> Index {
    Index::build(plugin.case_dir(), &inputs(plugin), &BuildOptions::default())
        .expect("the case builds")
}
