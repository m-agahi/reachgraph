//! The neutrality guards that assert over the dependency graph (plan-00 §6.1,
//! plan-02 §7.2).
//!
//! Both read `cargo metadata`, so both shell out to `cargo`. ADR-0001 forbids
//! subprocesses in the **shipped binary**; a dev-dependency test harness is not
//! the shipped binary. Plan-02 §7.2 states that explicitly so nobody "fixes"
//! these by removing them.
//!
//! They assert over the RESOLVED graph rather than over the text of a
//! `Cargo.toml`. A file read sees one manifest and misses a dependency reached
//! through another crate; `resolve.nodes` is the graph cargo actually built, so
//! a transitive edge is caught the same way a direct one is.
//!
//! `fixture_implements_every_trait` is the third guard of plan-02 §7.2 and is
//! the first thing in this file, because it is the strongest: the other two
//! read a graph and report, while that one either compiles or does not.

use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

use reachgraph_fixture::FixturePlugin;
use reachgraph_plugin_api::{
    Classifier, EdgeProvider, LanguagePlugin, Plugin, RootProvider, SymbolProvider,
};
use serde_json::Value;
use static_assertions::assert_impl_all;

// Plan-00 §6.1 and plan-02 §7.2 — **the gate**.
//
// ADR-0008's mechanism made executable, and the strongest guard in the
// project: it is a compile-time assertion, so it prevents a change rather than
// reporting one afterwards. If a signature ever requires something only a real
// language engine can produce — a salsa snapshot, a `FileId`, a cursor
// position — the fixture cannot produce a value of that type and this file
// stops building.
//
// `Send + Sync` is in the list because `Plugin` requires it and a registry
// holds `Box<dyn Plugin>`. Asserting it here means a future field that is
// neither — an `Rc`, a `RefCell` — fails at the fixture rather than at
// whichever caller first tries to share a registry.
//
// There is no `Renderer` row. A renderer is not a `Plugin` (plan-00 §3.6), it
// takes a `&GraphView` that plan-01 has not defined yet, and `Capability`
// has no `Render` variant for a case to declare.
assert_impl_all!(
    FixturePlugin: Plugin,
    LanguagePlugin,
    SymbolProvider,
    EdgeProvider,
    RootProvider,
    Classifier,
    Send,
    Sync
);

/// A package id as `cargo metadata` spells it, and the package name it resolves
/// to. Ids are opaque and version-qualified; names are what a guard asserts
/// about.
type Graph = BTreeMap<String, Node>;

struct Node {
    name: String,
    deps: Vec<String>,
}

/// Run `cargo metadata` over this workspace and index `resolve.nodes` by id.
///
/// Every dependency kind is followed — normal, build and dev alike. A plugin
/// crate has no legitimate reason to reach `reachgraph-core` through any of
/// them, and restricting the walk to normal edges would let a dev-dependency
/// carry the coupling the guard exists to refuse. The reverse edge plan-02
/// §7.6 legalises — `core` dev-depending on `fixture` — is unaffected, because
/// it points the other way.
fn graph() -> Graph {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let output = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--all-features"])
        // The test's working directory is this package's root; cargo walks up
        // to the workspace manifest from there.
        .output()
        .expect("cargo metadata runs");

    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let metadata: Value = serde_json::from_slice(&output.stdout).expect("cargo metadata is JSON");

    let names: BTreeMap<String, String> = metadata["packages"]
        .as_array()
        .expect("metadata has a packages array")
        .iter()
        .map(|package| {
            (
                package["id"]
                    .as_str()
                    .expect("a package has an id")
                    .to_owned(),
                package["name"]
                    .as_str()
                    .expect("a package has a name")
                    .to_owned(),
            )
        })
        .collect();

    metadata["resolve"]["nodes"]
        .as_array()
        .expect("metadata has a resolved graph")
        .iter()
        .map(|node| {
            let id = node["id"].as_str().expect("a node has an id").to_owned();
            let deps = node["deps"]
                .as_array()
                .expect("a node has a deps array")
                .iter()
                .map(|dep| {
                    dep["pkg"]
                        .as_str()
                        .expect("a dep names a package")
                        .to_owned()
                })
                .collect();
            let name = names.get(&id).cloned().expect("every node is a package");
            (id, Node { name, deps })
        })
        .collect()
}

/// Every package name reachable from `root`, excluding `root` itself.
///
/// Cycle-guarded. Following dev edges makes cycles representable — `core`
/// dev-depends on `fixture`, which depends on `plugin-api` — and an unguarded
/// walk would hang rather than fail.
fn reachable_from(graph: &Graph, root: &str) -> BTreeSet<String> {
    let start = graph
        .iter()
        .find(|(_, node)| node.name == root)
        .map(|(id, _)| id.clone())
        .unwrap_or_else(|| panic!("{root} is in the resolved graph"));

    let mut seen = BTreeSet::from([start.clone()]);
    let mut queue = vec![start];
    let mut names = BTreeSet::new();

    while let Some(id) = queue.pop() {
        for dep in &graph[&id].deps {
            if seen.insert(dep.clone()) {
                names.insert(graph[dep].name.clone());
                queue.push(dep.clone());
            }
        }
    }

    names
}

fn workspace_crate_names(graph: &Graph) -> BTreeSet<String> {
    graph
        .values()
        .map(|node| node.name.clone())
        .filter(|name| name.starts_with("reachgraph-"))
        .collect()
}

/// Plan-00 §1: "dependencies point toward `plugin-api`. No plugin crate may
/// depend on `reachgraph-core`."
///
/// `core` is exempt because it is the subject, and `cli` because plan-00 §1
/// gives it `core` plus every plugin. Everything else named `reachgraph-*` is a
/// plugin and is checked.
#[test]
fn no_plugin_depends_on_core() {
    const CORE: &str = "reachgraph-core";
    const EXEMPT: [&str; 2] = [CORE, "reachgraph-cli"];

    let graph = graph();
    let names = workspace_crate_names(&graph);

    // A guard over a graph that contains neither the subject nor anything to
    // check passes by finding nothing. Both facts are asserted so that a
    // renamed or removed crate fails here rather than going quietly green.
    assert!(
        names.contains(CORE),
        "{CORE} is not in the workspace, so this guard would assert over nothing"
    );
    let checked: Vec<&String> = names
        .iter()
        .filter(|name| !EXEMPT.contains(&name.as_str()))
        .collect();
    assert!(
        !checked.is_empty(),
        "no plugin crate to check; the exemption list has swallowed the workspace"
    );

    for plugin in checked {
        let reached = reachable_from(&graph, plugin);
        assert!(
            !reached.contains(CORE),
            "{plugin} reaches {CORE}. Plan-00 §1: dependencies point toward plugin-api, \
             and a plugin that can write `use reachgraph_core::…` can couple itself to \
             waist internals. Reached: {reached:?}"
        );
    }
}

/// Plan-00 §6.1: `ra_ap_*` is absent from `plugin-api`'s dependency graph.
///
/// This is what makes the contract's neutrality mechanical rather than
/// intentional. A Rust type cannot leak into a signature in a crate that cannot
/// name it.
#[test]
fn plugin_api_has_no_ra_ap_dependency() {
    const CONTRACT: &str = "reachgraph-plugin-api";

    let graph = graph();
    let reached = reachable_from(&graph, CONTRACT);

    let leaked: Vec<&String> = reached
        .iter()
        .filter(|name| {
            let name = name.replace('-', "_");
            name.starts_with("ra_ap") || name == "rust_analyzer"
        })
        .collect();

    assert!(
        leaked.is_empty(),
        "{CONTRACT} reaches {leaked:?}. ADR-0008: the contract must be defined from what \
         the waist needs, not from what ra_ap returns."
    );
}
