//! Plan-02 §7.5 — lifecycle and ordering.
//!
//! These exist because of ADR-0008 leak 5, and specifically because of the half
//! of it the fixture does *not* catch by construction. A signature carrying a
//! salsa snapshot or a lifetime fails to compile, which is mechanism (A). An
//! **implicit ordering requirement** — a core that only works if `symbols_in`
//! runs before `edges_from` — passes silently, because the fixture answers in
//! any order. So it is asserted here instead.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use reachgraph_fixture::FixturePlugin;
use reachgraph_plugin_api::{
    EdgeProvider, EdgeTarget, LanguagePlugin, NodeId, Plugin, PluginError, Preflight,
    SymbolProvider, Unit,
};

fn case(name: &str) -> FixturePlugin {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name);
    FixturePlugin::load(dir).unwrap_or_else(|error| panic!("{name} should load: {error}"))
}

fn units(plugin: &FixturePlugin) -> Vec<Unit> {
    plugin
        .discover_units(plugin.case_dir())
        .expect("units discover")
}

/// A description of an edge that does not depend on which call produced it.
fn describe(edge: &reachgraph_plugin_api::Edge) -> String {
    let target = match &edge.to {
        EdgeTarget::Resolved(node) => format!("resolved:{}", node.raw),
        EdgeTarget::Unresolved { name, candidates } => format!(
            "unresolved:{name}:{}",
            candidates
                .iter()
                .map(|c| c.raw.as_str())
                .collect::<Vec<&str>>()
                .join(",")
        ),
    };
    format!("{} -> {target} [{:?}]", edge.from.raw, edge.inference_mode)
}

/// Plan-02 §7.5 and §5 leak 5.
///
/// A freshly loaded plugin is asked for edges before anything asks it for
/// symbols or units. If any hidden state had to be built by an earlier call,
/// this returns the wrong answer or fails.
#[test]
fn edges_from_works_before_symbols_in() {
    let plugin = case("versioned_pair");

    let node = NodeId {
        plugin: plugin.id(),
        raw: "fn:v1/create_task".to_owned(),
    };

    let cold: BTreeSet<String> = plugin
        .edges_from(&node)
        .expect("a cold plugin answers edges_from")
        .iter()
        .map(describe)
        .collect();
    assert_eq!(cold.len(), 2);

    // Now warm it up the other way round and ask again.
    for unit in units(&plugin) {
        let _ = plugin.symbols_in(&unit).expect("symbols");
        let _ = plugin.edges_in(&unit).expect("edges");
    }

    let warm: BTreeSet<String> = plugin
        .edges_from(&node)
        .expect("a warm plugin answers the same")
        .iter()
        .map(describe)
        .collect();
    assert_eq!(
        cold, warm,
        "the answer depended on what had been called before"
    );
}

/// Plan-02 §7.5, and it is worth having even though it is trivially true here.
///
/// When `lang-rust` lands, the same assertion against a real engine is what
/// answers plan-00 open question 1 — whether `edges_from(&NodeId)` maps onto
/// `ra_ap` without a live salsa snapshot per call. The fixture makes the method
/// look easy (plan-02 §5.1) and that is exactly why the agreement has to be
/// pinned before the hard implementation exists to be compared against.
#[test]
fn edges_from_agrees_with_edges_in() {
    let mut checked = 0usize;

    for name in ["minimal", "versioned_pair", "unresolved_edge"] {
        let plugin = case(name);

        let mut by_source: std::collections::BTreeMap<String, BTreeSet<String>> =
            std::collections::BTreeMap::new();
        for unit in units(&plugin) {
            for edge in plugin.edges_in(&unit).expect("edges") {
                by_source
                    .entry(edge.from.raw.clone())
                    .or_default()
                    .insert(describe(&edge));
            }
        }

        for unit in units(&plugin) {
            for symbol in plugin.symbols_in(&unit).expect("symbols") {
                let expected = by_source.get(&symbol.id.raw).cloned().unwrap_or_default();
                let actual: BTreeSet<String> = plugin
                    .edges_from(&symbol.id)
                    .unwrap_or_else(|error| panic!("{name}: {error}"))
                    .iter()
                    .map(describe)
                    .collect();

                assert_eq!(actual, expected, "{name}: {} disagrees", symbol.id.raw);
                checked += 1;
            }
        }
    }

    assert!(
        checked > 0,
        "no node was compared, so this asserted nothing"
    );
}

/// Plan-02 §7.5: called twice, the same answer. No hidden state.
#[test]
fn discover_units_is_idempotent() {
    let plugin = case("versioned_pair");

    let first = units(&plugin);
    let second = units(&plugin);

    assert_eq!(first.len(), second.len());
    for (a, b) in first.iter().zip(second.iter()) {
        assert_eq!(a.id, b.id);
        assert_eq!(a.display_name, b.display_name);
        assert_eq!(a.root, b.root);
    }
}

/// A node this case never emitted is a typed error, not an empty vector.
///
/// The contract says so in as many words: a node the plugin did not emit "is
/// never a silent empty result". An empty answer and "I have never heard of
/// that node" are different claims, and only one of them is true here.
#[test]
fn edges_from_an_unemitted_node_is_unknown_node() {
    let plugin = case("minimal");

    let absent = NodeId {
        plugin: plugin.id(),
        raw: "fn:nothing/declares_this".to_owned(),
    };
    assert!(matches!(
        plugin.edges_from(&absent),
        Err(PluginError::UnknownNode { .. })
    ));

    // The same raw under another plugin's namespace is also not this plugin's
    // node. ADR-0003 field 3: `raw` is opaque, and the namespace half is what
    // makes two identical strings two different nodes.
    let foreign = NodeId {
        plugin: reachgraph_plugin_api::PluginId("some-other-plugin"),
        raw: "fn:handlers/create_task".to_owned(),
    };
    assert!(matches!(
        plugin.edges_from(&foreign),
        Err(PluginError::UnknownNode { .. })
    ));
}

/// A unit this case never emitted, likewise.
#[test]
fn symbols_in_an_unemitted_unit_is_unknown_unit() {
    let plugin = case("minimal");
    let foreign = Unit {
        id: reachgraph_plugin_api::UnitId("unit:not-declared".to_owned()),
        display_name: "not declared".to_owned(),
        root: PathBuf::from("src"),
    };

    assert!(matches!(
        plugin.symbols_in(&foreign),
        Err(PluginError::UnknownUnit { .. })
    ));
    assert!(matches!(
        plugin.edges_in(&foreign),
        Err(PluginError::UnknownUnit { .. })
    ));
}

/// ADR-0003 field 5, exercised through the contract rather than through a load
/// failure.
///
/// The two are different and the split is deliberate: a document that will not
/// parse is this crate's input being broken and never produces a plugin at all
/// (`PluginError::Parse`), while a case declaring `preflight: { failed: … }` is
/// a plugin that loaded fine and reports that it must not run.
#[test]
fn preflight_failure_carries_a_reason_and_a_remediation() {
    let plugin = case("preflight_fails");

    match plugin.preflight(plugin.case_dir()) {
        Preflight::Failed {
            reason,
            remediation,
        } => {
            assert!(!reason.is_empty());
            assert!(!remediation.is_empty());
        }
        other => panic!("the case declares a failure: {other:?}"),
    }
}

/// A missing document is an [`PluginError::Io`], attributed to the crate rather
/// than to an id no case declared.
#[test]
fn a_missing_document_is_an_io_error_attributed_to_the_crate() {
    let error = FixturePlugin::load(PathBuf::from("/nonexistent/fixture-case"))
        .expect_err("there is no such case");

    match error {
        PluginError::Io { plugin, .. } => assert_eq!(plugin.0, "reachgraph-fixture"),
        other => panic!("a missing file is an Io error: {other}"),
    }
}
