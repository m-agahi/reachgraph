//! Task 2068 phase 1 — calls in the source against edges in the index.
//!
//! `fx-unresolved` holds four callback shapes, one macro call and an empty
//! module. Edges into the standard library depend on rust-src, so these tests
//! assert on the `helper_*` edges and on the cross-check between the counters
//! and the edges actually returned, never on a unit total that the sysroot
//! changes.

use reachgraph_lang_rust::ids::{node_id, RawParts};
use reachgraph_lang_rust::PLUGIN_ID;
use reachgraph_plugin_api::{EdgeProvider, EdgeTarget, Plugin};

use crate::support::{all_edges, all_symbols, load};

const PREFIX: &str = "reachgraph-lang-rust call counts: ";

/// The word the note must never use, assembled so this file does not carry it.
fn forbidden() -> String {
    ['d', 'e', 'a', 'd'].iter().collect()
}

/// B1: the direct call and the closure call have an edge; the fn item passed
/// by value, the call through a fn pointer and the call inside a macro call do
/// not.
#[test]
fn only_the_direct_and_closure_shapes_reach_a_helper() {
    let (plugin, units) = load("fx-unresolved");
    let symbols = all_symbols(&plugin, &units);
    let edges = all_edges(&plugin, &units);

    let name_of = |id: &reachgraph_plugin_api::NodeId| {
        symbols
            .iter()
            .find(|symbol| &symbol.id == id)
            .map(|symbol| symbol.name.clone())
    };
    let mut to_helpers: Vec<(String, String)> = edges
        .iter()
        .filter_map(|edge| {
            let EdgeTarget::Resolved(target) = &edge.to else {
                return None;
            };
            let target = name_of(target)?;
            target
                .starts_with("helper_")
                .then(|| (name_of(&edge.from).unwrap_or_default(), target))
        })
        .collect();
    to_helpers.sort();

    assert_eq!(
        to_helpers,
        vec![
            ("shapes".to_owned(), "helper_a".to_owned()),
            ("shapes".to_owned(), "helper_b".to_owned()),
        ]
    );
}

/// B2, B4, B6: one note, the totals on this fixture, the counters summing to
/// the edges returned, and a second walk changing nothing.
#[test]
fn the_note_counts_calls_against_edges() {
    let (plugin, units) = load("fx-unresolved");
    let edges = all_edges(&plugin, &units);
    let produced = edges.len();
    assert!(produced <= 12, "{produced} edges from 12 call sites");

    let notes = plugin.notes();
    let ours: Vec<&String> = notes.iter().filter(|n| n.starts_with(PREFIX)).collect();
    assert_eq!(ours.len(), 1, "exactly one call-counts note in {notes:?}");
    let note = ours[0];

    for fragment in [
        "6 functions and methods queried".to_owned(),
        "0 with no call hierarchy".to_owned(),
        "0 call targets dropped at the file lookup".to_owned(),
        "0 call ranges dropped at the file lookup".to_owned(),
        format!("{produced} edges produced"),
        "12 call sites in the source outside macro calls".to_owned(),
        "2 call sites inside macro calls".to_owned(),
        format!(
            "{} calls in the source with no edge in this index",
            12 - produced
        ),
    ] {
        assert!(note.contains(&fragment), "{fragment:?} missing from {note}");
    }
    assert!(!note.to_lowercase().contains(&forbidden()), "{note}");

    // The note comes after every other note, so it never sits above a
    // stale-build warning.
    assert_eq!(notes.last(), Some(note));

    let counts = plugin.call_counts();
    let summed: usize = counts.iter().map(|unit| unit.edges).sum();
    assert_eq!(summed, produced, "the counters drifted from the emit path");

    let _ = all_edges(&plugin, &units);
    assert_eq!(
        plugin.call_counts(),
        counts,
        "a second walk added to the counts"
    );
}

/// B3: a node where `outgoing_calls` has no answer — offset 0 of an empty file —
/// is counted as queried with no call hierarchy.
#[test]
fn a_node_with_no_call_hierarchy_is_counted() {
    let (plugin, units) = load("fx-unresolved");
    assert_eq!(units.len(), 1, "fx-unresolved has one unit: {units:?}");
    let unit = &units[0];

    let node = node_id(
        PLUGIN_ID,
        &RawParts {
            unit: unit.id.clone(),
            offset: 0,
            path: "u/src/empty.rs".to_owned(),
        },
    )
    .expect("the id encodes");
    let edges = plugin.edges_from(&node).expect("edges_from answers");
    assert!(edges.is_empty(), "{edges:?}");

    let counts = plugin.call_counts();
    assert_eq!(counts.len(), 1, "{counts:?}");
    let only = &counts[0];
    assert_eq!(only.unit, unit.id.0);
    assert_eq!(
        (
            only.queried,
            only.no_hierarchy,
            only.targets_dropped,
            only.ranges_dropped,
            only.edges,
            only.call_sites,
            only.macro_call_sites
        ),
        (1, 1, 0, 0, 0, 0, 0)
    );

    let notes = plugin.notes();
    let note = notes
        .iter()
        .find(|n| n.starts_with(PREFIX))
        .expect("a call-counts note");
    assert!(
        note.contains("1 functions and methods queried, 1 with no call hierarchy"),
        "{note}"
    );
}
