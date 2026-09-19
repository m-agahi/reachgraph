//! Reachability, the depth limit, the frontier and the complement —
//! plan-01 §5.

use std::collections::BTreeSet;

use reachgraph_core::{BuildOptions, Index};
use reachgraph_plugin_api::Category;

use crate::support::{build, build_with, case, emit, raws, shards, unreachable};

/// Every node the index-wide view holds that any bound root reaches.
fn reached(index: &Index) -> BTreeSet<String> {
    let unreached: BTreeSet<&str> = index
        .unreachable()
        .iter()
        .map(|node| node.id.raw.as_str())
        .collect();

    index
        .view()
        .nodes
        .iter()
        .filter(|node| node.symbol.is_some() && !unreached.contains(node.id.raw.as_str()))
        .map(|node| node.id.raw.clone())
        .collect()
}

fn unreached(index: &Index) -> BTreeSet<String> {
    index
        .unreachable()
        .iter()
        .map(|node| node.id.raw.clone())
        .collect()
}

/// Plan-01 §10.1 step 3. The shard of one root is the set that root reaches.
#[test]
fn reachable_set_from_single_root() {
    let index = build(&case("minimal"));

    assert_eq!(index.shards().len(), 1);
    let shard = &index.shards()[0];
    let names: BTreeSet<&str> = shard
        .view
        .nodes
        .iter()
        .map(|node| node.id.raw.as_str())
        .collect();

    assert_eq!(
        names,
        BTreeSet::from(["fn:handlers/create_task", "fn:db/insert_task"])
    );
    for node in &shard.view.nodes {
        assert!(node.depth.is_some(), "every node in a shard has a depth");
    }
}

/// Plan-01 §10.1 step 4. A→B→C→A terminates because the visited set is checked
/// before enqueue.
#[test]
fn cycle_terminates() {
    let index = build(&case("cycle_and_diamond"));

    let alpha = index
        .shards()
        .iter()
        .find(|shard| shard.root.operation == "Alpha")
        .expect("the Alpha root bound");

    let names: BTreeSet<&str> = alpha
        .view
        .nodes
        .iter()
        .map(|node| node.id.raw.as_str())
        .collect();
    assert_eq!(names, BTreeSet::from(["fn:a", "fn:b", "fn:c"]));
}

/// A node reached from two roots is in both shards, and neither shard is
/// authoritative.
#[test]
fn two_roots_sharing_a_node_both_include_it() {
    let index = build(&case("cycle_and_diamond"));
    assert_eq!(index.shards().len(), 2);

    for shard in index.shards() {
        assert!(
            shard.view.node(&node_id("fixture", "fn:c")).is_some(),
            "{} does not hold the shared node",
            shard.root.operation
        );
    }
}

fn node_id(plugin: &'static str, raw: &str) -> reachgraph_plugin_api::NodeId {
    reachgraph_plugin_api::NodeId {
        plugin: reachgraph_plugin_api::PluginId(plugin),
        raw: raw.to_owned(),
    }
}

/// Plan-01 §10.1 step 5, §5.2. The node at the limit is frontier, not leaf.
#[test]
fn depth_limit_marks_frontier_not_leaf() {
    let plugin = case("deep_chain");
    let index = build_with(
        &plugin,
        &BuildOptions {
            depth: Some(3),
            ..BuildOptions::default()
        },
    );

    let sink = emit(&index);
    let (_, shard) = shards(&sink).remove(0);

    assert_eq!(
        raws(&shard),
        ["fn:step0", "fn:step1", "fn:step2", "fn:step3"],
        "the walk stops at the limit"
    );
    assert_eq!(shard.frontier.len(), 1);
    assert_eq!(shard.frontier[0].raw, "fn:step3");

    let frontier: Vec<&str> = shard
        .nodes
        .iter()
        .filter(|node| node.frontier)
        .map(|node| node.id.raw.as_str())
        .collect();
    assert_eq!(frontier, ["fn:step3"]);

    // The node one short of the limit still has an out-edge that WAS followed,
    // so it is not frontier. Without that the flag would mean "deep" rather
    // than "we stopped looking here".
    let step2 = shard
        .nodes
        .iter()
        .find(|node| node.id.raw == "fn:step2")
        .expect("step2 is in the shard");
    assert!(!step2.frontier);
}

/// Plan-01 §10.1 step 7, §5.1. The complement is unlimited even when the
/// shards are not — a depth-limited complement is a guaranteed false positive
/// on every deep call chain.
#[test]
fn unreachable_uses_unlimited_depth() {
    let plugin = case("deep_chain");
    let index = build_with(
        &plugin,
        &BuildOptions {
            depth: Some(3),
            ..BuildOptions::default()
        },
    );

    let sink = emit(&index);
    let document = unreachable(&sink);

    assert!(
        document.nodes.is_empty(),
        "steps 4 to 6 sit past the shard depth limit and are still reached: {:?}",
        document
            .nodes
            .iter()
            .map(|node| node.id.raw.as_str())
            .collect::<Vec<_>>()
    );
}

/// Plan-01 §10.1 step 6, §5.4. The complement is over every bound root.
#[test]
fn unreachable_is_complement_over_all_roots() {
    let index = build(&case("versioned_pair"));

    assert_eq!(
        unreached(&index),
        BTreeSet::from([
            "impl:TaskService_for_TaskServer".to_owned(),
            "fn:orphan/unused_helper".to_owned(),
        ])
    );
    assert!(reached(&index).contains("fn:shared/persist"));
}

/// Plan-01 §4.3, §5.4. A target no provider emitted a symbol for is a leaf,
/// and the tool never claims that code it did not index is unreachable.
#[test]
fn external_target_is_leaf_and_never_unreachable() {
    let index = build(&case("external_target"));

    let external = node_id("fixture", "ext:unlocatable_target");
    let node = index
        .view()
        .node(&external)
        .expect("the resolved target became a node");
    assert!(node.symbol.is_none(), "nothing was ever indexed for it");
    assert!(node.unit.is_none(), "it belongs to no unit");
    assert!(node.category.is_none(), "it has no path to classify");

    assert!(index.view().out_edges(&external).is_empty(), "it is a leaf");
    assert!(
        !unreached(&index).contains("ext:unlocatable_target"),
        "the tool has no evidence either way about code it never indexed"
    );

    // It is still in the shard, and the edge to it is still there.
    let sink = emit(&index);
    let (_, shard) = shards(&sink).remove(0);
    assert!(raws(&shard).contains(&"ext:unlocatable_target"));
    assert_eq!(shard.edges.len(), 1);
}

/// Plan-01 §5.3. An unresolved edge never advances the walk.
#[test]
fn unresolved_edge_does_not_propagate_reachability() {
    let index = build(&case("unresolved_edge"));

    assert_eq!(
        unreached(&index),
        BTreeSet::from([
            "fn:db/execute_pg".to_owned(),
            "fn:db/execute_sqlite".to_owned()
        ]),
        "following a candidate would be inferring an edge to fill a hole"
    );
}

/// Plan-01 §5.3. The annotation is attached, and it never removes a row.
#[test]
fn unresolved_candidate_flagged_possibly_reachable() {
    let sink = emit(&build(&case("unresolved_edge")));
    let document = unreachable(&sink);

    let flagged: BTreeSet<&str> = document
        .nodes
        .iter()
        .filter(|node| node.possibly_reachable_via_unresolved)
        .map(|node| node.id.raw.as_str())
        .collect();

    assert_eq!(
        flagged,
        BTreeSet::from(["fn:db/execute_pg", "fn:db/execute_sqlite"])
    );
}

/// Plan-01 §5.3, stated separately because it fails separately: an
/// implementation that dropped flagged rows would have re-introduced the
/// inference the target enum forbids.
#[test]
fn possibly_reachable_annotation_does_not_filter_list() {
    let sink = emit(&build(&case("unresolved_edge")));
    let document = unreachable(&sink);

    let listed: BTreeSet<&str> = document
        .nodes
        .iter()
        .map(|node| node.id.raw.as_str())
        .collect();

    assert_eq!(
        listed,
        BTreeSet::from(["fn:db/execute_pg", "fn:db/execute_sqlite"]),
        "every flagged node is still a row"
    );
    assert_eq!(document.unresolved_edge_count, 1);
}

/// Plan-00 §6.2. The edge is carried into the shard with its candidates
/// intact, and no candidate is promoted to a target.
#[test]
fn unresolved_edge_is_not_silently_resolved() {
    let sink = emit(&build(&case("unresolved_edge")));
    let (_, shard) = shards(&sink).remove(0);

    assert_eq!(shard.stats.unresolved_edge_count, 1);
    match &shard.edges[0].to {
        reachgraph_core::schema::EdgeTargetRow::Unresolved { name, candidates } => {
            assert_eq!(name, "execute");
            let raws: Vec<&str> = candidates.iter().map(|c| c.raw.as_str()).collect();
            assert_eq!(raws, ["fn:db/execute_pg", "fn:db/execute_sqlite"]);
        }
        other => panic!("the edge was resolved to {other:?}"),
    }
}

/// Plan-01 §7.1. A third-party node stops the walk and is still in the shard,
/// with the edge to it.
#[test]
fn third_party_node_is_terminal_not_deleted() {
    let index = build(&case("foreign_shapes"));
    let sink = emit(&index);
    let (_, shard) = shards(&sink).remove(0);

    let store = shard
        .nodes
        .iter()
        .find(|node| node.id.raw == "java:com.acme.Store")
        .expect("the third-party node is in the shard");
    assert_eq!(
        store.category,
        Some(reachgraph_core::schema::CategoryRow::ThirdParty)
    );

    assert!(
        shard.edges.iter().any(|edge| matches!(
            &edge.to,
            reachgraph_core::schema::EdgeTargetRow::Resolved { node }
                if node.raw == "java:com.acme.Store"
        )),
        "the edge to it is never dropped"
    );

    assert_eq!(
        index.coverage().traversal_terminal_categories,
        [Category::ThirdParty, Category::Stdlib]
    );
}

/// Plan-01 §9. A weaker claim is not a false one, and dropping it from the
/// walk would turn a weak edge into a missing one.
#[test]
fn traversal_does_not_filter_on_inference_mode() {
    let index = build(&case("versioned_pair"));

    // `fn:v2only/validate` is reached only through a `type_inferred` edge.
    assert!(reached(&index).contains("fn:v2only/validate"));

    let sink = emit(&index);
    let modes: Vec<reachgraph_core::schema::InferenceModeRow> = shards(&sink)
        .into_iter()
        .flat_map(|(_, shard)| shard.edges.into_iter().map(|edge| edge.inference_mode))
        .collect();
    assert!(modes.contains(&reachgraph_core::schema::InferenceModeRow::TypeInferred));
}
