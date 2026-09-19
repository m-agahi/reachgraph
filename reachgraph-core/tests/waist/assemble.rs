//! Provider orchestration, pairing and the diagnostics of plan-01 §4.

use crate::support::{build, case};

/// Plan-01 §10.1 step 1. Symbols and edges are collected per unit from a
/// paired plugin, and both halves of the pair saw the same `Unit` values —
/// which is the whole point of pairing (ADR-0003 field 1).
#[test]
fn pairing_by_plugin_id() {
    let plugin = case("minimal");
    let index = build(&plugin);

    let view = index.view();
    let raws: Vec<&str> = view.nodes.iter().map(|node| node.id.raw.as_str()).collect();
    assert_eq!(raws, ["fn:handlers/create_task", "fn:db/insert_task"]);

    assert_eq!(view.edges.len(), 1);
    assert_eq!(view.edges[0].from.raw, "fn:handlers/create_task");

    assert_eq!(index.coverage().units_indexed.len(), 1);
    assert_eq!(index.coverage().units_indexed[0].0, "unit:app");
}
