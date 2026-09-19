//! The `GraphView` accessors and the containment chain — plan-01 §3, §3.1.

use reachgraph_plugin_api::{GraphView, NodeId, PluginId, UnitId};

use crate::doubles::{doc_of, plugin_from};
use crate::support::{build, case, emit, shards};

fn id(plugin: &'static str, raw: &str) -> NodeId {
    NodeId {
        plugin: PluginId(plugin),
        raw: raw.to_owned(),
    }
}

/// The same view, with nothing looked up yet.
fn fresh(view: &GraphView) -> GraphView {
    GraphView::new(
        view.nodes.clone(),
        view.edges.clone(),
        view.roots.clone(),
        view.plugins.clone(),
        view.coverage.clone(),
    )
}

/// Plan-05 §9.2's depth slider gets the breadth-first distance at every node.
#[test]
fn depth_of_matches_bfs_distance() {
    let index = build(&case("deep_chain"));
    let shard = &index.shards()[0];

    for step in 0..=3u32 {
        assert_eq!(
            shard
                .view
                .depth_of(&id("fixture", &format!("fn:step{step}"))),
            Some(step)
        );
    }
    assert_eq!(shard.view.max_depth(), Some(3));
    assert_eq!(shard.view.nodes_at_depth(2).len(), 1);
    assert_eq!(shard.view.nodes_at_depth(9).len(), 0);
}

/// Plan-01 §3. There is no single root to measure from, so there is no fake
/// zero either.
#[test]
fn index_wide_view_has_null_depth() {
    let index = build(&case("deep_chain"));

    for node in &index.view().nodes {
        assert_eq!(node.depth, None, "{}", node.id.raw);
        assert!(!node.frontier);
    }
    assert_eq!(index.view().max_depth(), None);

    let sink = emit(&index);
    let (_, shard) = shards(&sink).remove(0);
    assert!(shard.nodes.iter().all(|node| node.depth.is_some()));
}

/// Plan-01 §3.1. Function, then `impl` block, then module — the nesting a
/// compound-box renderer draws.
#[test]
fn container_chain_nests_and_terminates() {
    let index = build(&case("nested_containers"));
    let view = index.view();

    let chain: Vec<&str> = view
        .container_chain(&id("fixture", "fn:service/create_task"))
        .into_iter()
        .map(|node| node.id.raw.as_str())
        .collect();
    assert_eq!(chain, ["impl:TaskServer", "mod:service"]);

    assert_eq!(
        view.container_of(&id("fixture", "fn:service/create_task"))
            .map(|node| node.id.raw.as_str()),
        Some("impl:TaskServer")
    );
    assert!(view
        .container_chain(&id("fixture", "mod:service"))
        .is_empty());

    let children: Vec<&str> = view
        .contained_in(&id("fixture", "mod:service"))
        .into_iter()
        .map(|node| node.id.raw.as_str())
        .collect();
    assert_eq!(children, ["impl:TaskServer"]);
}

/// A plugin-emitted containment loop truncates, never hangs.
#[test]
fn container_chain_survives_a_containment_cycle() {
    let index = build(&case("container_cycle"));

    let chain: Vec<&str> = index
        .view()
        .container_chain(&id("fixture", "sym:left"))
        .into_iter()
        .map(|node| node.id.raw.as_str())
        .collect();
    assert_eq!(chain, ["sym:right"], "the loop closes and the walk stops");
}

/// Plan-01 §4.3's dangling case, through the accessor.
#[test]
fn container_of_unindexed_container_is_none() {
    let index = build(&case("dangling_container"));

    assert!(
        index
            .view()
            .container_of(&id("fixture", "fn:orphan"))
            .is_none(),
        "the container names an id no provider emitted"
    );
}

/// Plan-01 §4.3. A dangling container is plugin data the waist does not
/// interpret, not a core error.
#[test]
fn dangling_container_is_not_an_error() {
    let index = build(&case("dangling_container"));

    let raws: Vec<&str> = index
        .view()
        .nodes
        .iter()
        .map(|node| node.id.raw.as_str())
        .collect();
    assert_eq!(raws, ["fn:orphan", "fn:callee"]);
    assert!(
        !raws.contains(&"impl:never_emitted"),
        "containment creates no node"
    );
}

/// Plan-00 §8 question 3. The field round-trips into the artifact verbatim,
/// and nothing reads it.
#[test]
fn container_is_copied_not_interpreted() {
    let sink = emit(&build(&case("test_symbol_collision")));
    let (_, shard) = shards(&sink).remove(0);

    let containers: Vec<Option<&str>> = shard
        .nodes
        .iter()
        .map(|node| {
            node.symbol
                .as_ref()
                .and_then(|symbol| symbol.container.as_ref())
                .map(|container| container.raw.as_str())
        })
        .collect();

    assert!(containers.contains(&Some("impl:TaskService_for_TaskServer")));
}

/// Containment is not a call. The edge count is unchanged by adding them.
#[test]
fn container_does_not_create_edge() {
    let with_containers = build(&case("nested_containers"));

    let mut doc = doc_of("nested_containers");
    for symbols in doc.symbols.values_mut() {
        for symbol in symbols.iter_mut() {
            symbol.container = None;
        }
    }
    let without = build(&plugin_from("nested_containers", doc));

    assert_eq!(
        with_containers.view().edges.len(),
        without.view().edges.len()
    );
    assert_eq!(with_containers.view().edges.len(), 1);
}

/// And the reachable set is identical with and without them.
#[test]
fn container_does_not_affect_reachability() {
    let reached = |index: &reachgraph_core::Index| -> Vec<String> {
        index.shards()[0]
            .view
            .nodes
            .iter()
            .map(|node| node.id.raw.clone())
            .collect()
    };

    let with_containers = build(&case("nested_containers"));

    let mut doc = doc_of("nested_containers");
    for symbols in doc.symbols.values_mut() {
        for symbol in symbols.iter_mut() {
            symbol.container = None;
        }
    }
    let without = build(&plugin_from("nested_containers", doc));

    assert_eq!(reached(&with_containers), reached(&without));
}

/// An external node belongs to no unit, so unit grouping never invents one.
#[test]
fn nodes_in_unit_excludes_externals() {
    let index = build(&case("external_target"));
    let unit = UnitId("unit:app".to_owned());

    let grouped: Vec<&str> = index
        .view()
        .nodes_in_unit(&unit)
        .into_iter()
        .map(|node| node.id.raw.as_str())
        .collect();
    assert_eq!(grouped, ["fn:handler"]);
}

/// Plan-01 §10.2's `view_index_rebuilds_after_deserialization`, renamed to what
/// it now asserts.
///
/// The lookup table is built on first use and is not part of the view's value.
/// Plan-01 §3 states the property as a round trip through serde, which this
/// crate's schema does not perform on the type itself — `PluginId` holds a
/// `&'static str`, so `GraphView` cannot deserialize. The property survives the
/// change: a view that has answered a lookup and one that has not must answer
/// alike.
#[test]
fn lazy_index_equals_eager_index() {
    let index = build(&case("nested_containers"));
    let warmed = index.view();
    let target = id("fixture", "fn:service/create_task");

    // Warm the first view.
    let warm_chain: Vec<String> = warmed
        .container_chain(&target)
        .into_iter()
        .map(|node| node.id.raw.clone())
        .collect();
    let warm_out = warmed.out_edges(&target).len();
    let warm_in = warmed
        .in_edges(&id("fixture", "fn:db/insert"))
        .iter()
        .map(|edge| edge.from.raw.clone())
        .collect::<Vec<_>>();

    let cold = fresh(warmed);
    let cold_chain: Vec<String> = cold
        .container_chain(&target)
        .into_iter()
        .map(|node| node.id.raw.clone())
        .collect();

    assert_eq!(warm_chain, cold_chain);
    assert_eq!(warm_out, cold.out_edges(&target).len());
    assert_eq!(
        warm_in,
        cold.in_edges(&id("fixture", "fn:db/insert"))
            .iter()
            .map(|edge| edge.from.raw.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        cold.node(&target).map(|node| node.id.raw.as_str()),
        Some("fn:service/create_task")
    );
}
