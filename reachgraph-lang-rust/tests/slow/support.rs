//! Fixture plumbing, and the one place a fixture is ever built.

use std::path::{Path, PathBuf};
use std::process::Command;

use reachgraph_lang_rust::RustPlugin;
use reachgraph_plugin_api::{Edge, LanguagePlugin, Symbol, SymbolProvider, Unit};

/// The directory of one checked-in fixture workspace.
pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// A plugin with the fixture's units already discovered.
pub fn load(name: &str) -> (RustPlugin, Vec<Unit>) {
    let plugin = RustPlugin::new();
    let root = fixture(name);
    let units = plugin
        .discover_units(&root)
        .unwrap_or_else(|error| panic!("{name} did not load: {error}"));
    (plugin, units)
}

/// Every symbol the plugin emits for every unit of a fixture.
pub fn all_symbols(plugin: &RustPlugin, units: &[Unit]) -> Vec<Symbol> {
    units
        .iter()
        .flat_map(|unit| {
            plugin
                .symbols_in(unit)
                .unwrap_or_else(|error| panic!("symbols_in({}) failed: {error}", unit.id.0))
        })
        .collect()
}

/// Every edge the plugin emits for every unit of a fixture.
pub fn all_edges(plugin: &RustPlugin, units: &[Unit]) -> Vec<Edge> {
    use reachgraph_plugin_api::EdgeProvider;
    units
        .iter()
        .flat_map(|unit| {
            plugin
                .edges_in(unit)
                .unwrap_or_else(|error| panic!("edges_in({}) failed: {error}", unit.id.0))
        })
        .collect()
}

/// The symbol with this exact name, or a panic naming what was there instead.
pub fn named<'a>(symbols: &'a [Symbol], name: &str) -> &'a Symbol {
    symbols
        .iter()
        .find(|symbol| symbol.name == name)
        .unwrap_or_else(|| {
            let seen: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
            panic!("no symbol named {name:?}; saw {seen:?}")
        })
}

/// Build a fixture workspace.
///
/// **This is the harness, never the plugin.** Plan-03 §4 D-B forbids
/// reachgraph running a build, and the prohibition holds inside the test suite:
/// a fixture whose assertions need `OUT_DIR` contents is built here, before the
/// plugin is handed the directory, so the prerequisite is explicit rather than
/// incidental.
pub fn build_fixture(name: &str) {
    let root = fixture(name);
    let output = Command::new("cargo")
        .arg("build")
        .current_dir(&root)
        .output()
        .expect("the harness may run cargo; the plugin may not");
    assert!(
        output.status.success(),
        "building {name} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A private, unbuilt copy of a fixture.
///
/// The unbuilt path has to be tested (plan-03 §13), and testing it by deleting
/// the shared fixture's `target/` made the suite order-dependent: the test that
/// cleans and the test that builds ran concurrently and fought over one
/// directory. MEASURED as a flake before it was fixed. A copy has no such
/// contention, and it also means a developer's fixture build survives the run.
pub fn unbuilt_copy(name: &str) -> PathBuf {
    let destination =
        std::env::temp_dir().join(format!("reachgraph-{name}-unbuilt-{}", std::process::id()));
    if destination.exists() {
        std::fs::remove_dir_all(&destination).expect("the copy is removable");
    }
    copy_tree(&fixture(name), &destination);
    destination
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("the destination is creatable");
    for entry in std::fs::read_dir(from).expect("the fixture is readable") {
        let entry = entry.expect("the entry is readable");
        // `target/` is the one thing the copy must not carry: an unbuilt
        // fixture with build output is not unbuilt.
        if entry.file_name() == "target" {
            continue;
        }
        let destination = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &destination);
        } else {
            std::fs::copy(entry.path(), &destination).expect("the file is copyable");
        }
    }
}
