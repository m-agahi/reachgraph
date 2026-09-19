//! Shared helpers for the waist's test suite.
//!
//! Every test in this crate runs against the fixture corpus (ADR-0008,
//! plan-01 §10.3): no `cargo metadata`, no indexing, no timing variance, and
//! no requirement that any repository was ever built.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use reachgraph_core::schema::{
    EndpointsDocument, ShardDocument, UnreachableDocument, VersionsDocument,
};
use reachgraph_core::{BuildInputs, BuildOptions, Index};
use reachgraph_fixture::FixturePlugin;
use reachgraph_plugin_api::{Capability, OutputSink, Plugin};

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

/// The four slices one plugin fills, **according to what it declares**.
///
/// A plugin is registered for exactly the capabilities in its own
/// `provides()`. Registering it for more would be a configuration error the
/// build refuses, and registering by hand for less would let a test quietly
/// disagree with the case it loaded.
pub fn inputs(plugin: &FixturePlugin) -> BuildInputs<'_> {
    let declares = |capability: Capability| plugin.provides().contains(&capability);

    BuildInputs {
        symbols: if declares(Capability::Symbols) {
            vec![plugin]
        } else {
            Vec::new()
        },
        edges: if declares(Capability::Edges) {
            vec![plugin]
        } else {
            Vec::new()
        },
        roots: if declares(Capability::Roots) {
            vec![plugin]
        } else {
            Vec::new()
        },
        classifiers: if declares(Capability::Classify) {
            vec![plugin]
        } else {
            Vec::new()
        },
    }
}

/// The two slices of a two-plugin build, each plugin self-paired.
pub fn inputs_of<'a>(plugins: &[&'a FixturePlugin]) -> BuildInputs<'a> {
    let mut merged = BuildInputs::default();
    for plugin in plugins {
        let one = inputs(plugin);
        merged.symbols.extend(one.symbols);
        merged.edges.extend(one.edges);
        merged.roots.extend(one.roots);
        merged.classifiers.extend(one.classifiers);
    }
    merged
}

/// Build one case with the default options.
pub fn build(plugin: &FixturePlugin) -> Index {
    build_with(plugin, &BuildOptions::default())
}

/// Build one case with options the caller chose.
pub fn build_with(plugin: &FixturePlugin, opts: &BuildOptions) -> Index {
    Index::build(plugin.case_dir(), &inputs(plugin), opts).expect("the case builds")
}

/// A sink that keeps what it was given, so a test asserts on the emitted bytes
/// rather than on the value a function returned.
#[derive(Debug, Default)]
pub struct MemorySink {
    files: BTreeMap<String, Vec<u8>>,
}

impl MemorySink {
    /// Every path written, in sorted order.
    pub fn paths(&self) -> Vec<&str> {
        self.files.keys().map(String::as_str).collect()
    }

    /// The bytes written at one path.
    pub fn bytes(&self, path: &str) -> &[u8] {
        self.files
            .get(path)
            .unwrap_or_else(|| panic!("{path} was not written; the sink holds {:?}", self.paths()))
    }
}

impl OutputSink for MemorySink {
    fn write(&mut self, relative_path: &str, bytes: &[u8]) -> std::io::Result<()> {
        self.files.insert(relative_path.to_owned(), bytes.to_vec());
        Ok(())
    }
}

/// Emit one index and keep the bytes.
pub fn emit(index: &Index) -> MemorySink {
    let mut sink = MemorySink::default();
    index.emit(&mut sink).expect("the artifact writes");
    sink
}

/// Read one artifact file back off the sink and parse it.
///
/// **The artifact under test is the emitted JSON**, never the value a builder
/// returned: a build that computes the right answer and writes the wrong bytes
/// has to fail here.
pub fn read<T: serde::de::DeserializeOwned>(sink: &MemorySink, path: &str) -> T {
    serde_json::from_slice(sink.bytes(path))
        .unwrap_or_else(|error| panic!("{path} does not parse: {error}"))
}

/// `endpoints.json`, read back.
pub fn endpoints(sink: &MemorySink) -> EndpointsDocument {
    read(sink, "endpoints.json")
}

/// `unreachable.json`, read back.
pub fn unreachable(sink: &MemorySink) -> UnreachableDocument {
    read(sink, "unreachable.json")
}

/// `versions.json`, read back.
pub fn versions(sink: &MemorySink) -> VersionsDocument {
    read(sink, "versions.json")
}

/// Every `graph/<slug>.json`, read back, in path order.
pub fn shards(sink: &MemorySink) -> Vec<(String, ShardDocument)> {
    sink.paths()
        .into_iter()
        .filter(|path| path.starts_with("graph/"))
        .map(|path| (path.to_owned(), read(sink, path)))
        .collect()
}

/// The `raw` half of every node in a shard, in emitted order.
pub fn raws(shard: &ShardDocument) -> Vec<&str> {
    shard
        .nodes
        .iter()
        .map(|node| node.id.raw.as_str())
        .collect()
}
