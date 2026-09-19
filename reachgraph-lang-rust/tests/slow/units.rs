//! `fx-targets` — plan-03 §6's consistency rule, and the guard for its class.
//!
//! MEASURED 2026-09-19 on `/home/max/git/yadgarhq/task`, by reading the emitted
//! artifact rather than the code: every handler's edge into `src/service.rs`
//! named the unit `…::assembly::test`, while the symbol walk had emitted that
//! file's symbols under `…::yadgar_task::lib`. The target therefore matched no
//! symbol, every shard stopped at depth 1, and the artifact reported 221 of 227
//! symbols as not reachable from any endpoint — the most dangerous claim this
//! tool can make, produced by an id-minting mistake rather than by the code.

use std::collections::HashSet;

use reachgraph_plugin_api::{EdgeTarget, NodeId, UnitId};

use crate::support::{all_edges, all_symbols, load, named};

/// The assertion plan-03 §6 rests on, at the smallest scale that can fail: one
/// package, three targets, one first-party call across two files.
///
/// Not a count and not "no nulls": the edge's target id must be **the same id**
/// the symbol walk minted for that definition.
#[test]
fn a_call_target_is_the_id_the_symbol_walk_emitted() {
    let (plugin, units) = load("fx-targets");
    let symbols = all_symbols(&plugin, &units);
    let caller = named(&symbols, "caller");
    let leaf = named(&symbols, "leaf");

    let targets: Vec<&NodeId> = all_edges(&plugin, &units)
        .iter()
        .filter(|edge| edge.from == caller.id)
        .filter_map(|edge| match &edge.to {
            EdgeTarget::Resolved(target) => Some(target),
            EdgeTarget::Unresolved { .. } => None,
        })
        .cloned()
        .collect::<Vec<NodeId>>()
        .leak()
        .iter()
        .collect();

    assert_eq!(
        targets,
        vec![&leaf.id],
        "the call resolves to the definition the walk emitted, byte for byte"
    );
}

/// The unit half specifically, spelled out so a failure says which unit was
/// wrong rather than that two long strings differ.
#[test]
fn a_call_target_names_the_unit_that_owns_the_file() {
    let (plugin, units) = load("fx-targets");
    let symbols = all_symbols(&plugin, &units);
    let caller = named(&symbols, "caller");

    let library: Vec<&UnitId> = units
        .iter()
        .map(|unit| &unit.id)
        .filter(|id| id.0.ends_with("::t::lib"))
        .collect();
    let [library] = library.as_slice() else {
        panic!("the fixture has exactly one library target");
    };

    for edge in all_edges(&plugin, &units) {
        if edge.from != caller.id {
            continue;
        }
        let EdgeTarget::Resolved(target) = &edge.to else {
            continue;
        };
        let unit = target
            .raw
            .split('|')
            .next()
            .expect("the raw has a unit half");
        assert_eq!(
            unit, library.0,
            "the callee's file belongs to the library crate and to no other target"
        );
    }
}

/// The other half of the rule: a callee no enumerated unit owns is `external`,
/// never a member's unit id.
///
/// A member id there would mint an id inside an indexed unit that no symbol
/// carries — the same phantom in miniature — and on the probe repository it
/// would do so for every one of the 26 dependency targets the six shards reach.
/// `dep` is a path dependency that the workspace does not list as a member, so
/// its crate is in the graph and is not a unit: the shape every third-party
/// callee has, with no registry and no network.
#[test]
fn a_callee_outside_every_unit_is_external() {
    let (plugin, units) = load("fx-targets");
    let symbols = all_symbols(&plugin, &units);
    let caller = named(&symbols, "calls_outside");

    let units_of_targets: Vec<String> = all_edges(&plugin, &units)
        .iter()
        .filter(|edge| edge.from == caller.id)
        .filter_map(|edge| match &edge.to {
            EdgeTarget::Resolved(target) => {
                Some(target.raw.split('|').next().unwrap_or_default().to_owned())
            }
            EdgeTarget::Unresolved { .. } => None,
        })
        .collect();

    assert_eq!(units_of_targets, vec!["external".to_owned()]);
}

/// The guard for the whole class, across every fixture the suite loads.
///
/// A resolved edge target whose unit is one of the enumerated units must have a
/// symbol: that unit was walked, so a definition inside it was either emitted
/// or is not a definition. A target outside every enumerated unit — a
/// dependency, the sysroot, generated code — has no symbol by design (plan-01
/// §7.0) and is skipped here.
#[test]
fn no_resolved_target_inside_an_indexed_unit_lacks_a_symbol() {
    for fixture in ["fx-targets", "fx-plain", "fx-impl", "fx-attr", "fx-generic"] {
        let (plugin, units) = load(fixture);
        let symbols = all_symbols(&plugin, &units);
        let emitted: HashSet<&str> = symbols
            .iter()
            .map(|symbol| symbol.id.raw.as_str())
            .collect();
        let indexed: HashSet<&str> = units.iter().map(|unit| unit.id.0.as_str()).collect();

        let mut phantom: Vec<&str> = Vec::new();
        for edge in all_edges(&plugin, &units) {
            let EdgeTarget::Resolved(target) = &edge.to else {
                continue;
            };
            let unit = target.raw.split('|').next().unwrap_or_default();
            if !indexed.contains(unit) {
                continue;
            }
            if !emitted.contains(target.raw.as_str()) {
                phantom.push(Box::leak(target.raw.clone().into_boxed_str()));
            }
        }

        assert!(
            phantom.is_empty(),
            "{fixture}: edges point at ids inside an indexed unit that no symbol carries: \
             {phantom:?}"
        );
    }
}
