//! Task 2068 phase 1 — the call-counts note, from counts alone.
//!
//! The note is the only place a reader sees calls in the source that have no
//! edge in the index, so its wording and its thresholds are pinned here.

use reachgraph_lang_rust::coverage::{call_counts_note, UnitCallCounts};

/// The word the note must never use, assembled so this file does not carry it.
fn forbidden() -> String {
    ['d', 'e', 'a', 'd'].iter().collect()
}

#[allow(clippy::too_many_arguments)]
fn unit(
    name: &str,
    queried: usize,
    no_hierarchy: usize,
    targets_dropped: usize,
    ranges_dropped: usize,
    edges: usize,
    call_sites: usize,
    macro_call_sites: usize,
) -> UnitCallCounts {
    UnitCallCounts {
        unit: name.to_owned(),
        queried,
        no_hierarchy,
        targets_dropped,
        ranges_dropped,
        edges,
        call_sites,
        macro_call_sites,
    }
}

/// A unit with only call sites and edges set — what the top-units list reads.
fn sized(name: &str, call_sites: usize, edges: usize) -> UnitCallCounts {
    unit(name, 1, 0, 0, 0, edges, call_sites, 0)
}

fn totals_text(q: usize, n: usize, t: usize, r: usize, e: usize, s: usize, m: usize) -> String {
    let gap = s.saturating_sub(e);
    format!(
        "reachgraph-lang-rust call counts: {q} functions and methods queried, {n} with no call \
         hierarchy (outgoing_calls returned none), {t} call targets dropped at the file lookup, \
         {r} call ranges dropped at the file lookup, {e} edges produced, {s} call sites in the \
         source outside macro calls, {m} call sites inside macro calls; {gap} calls in the \
         source with no edge in this index"
    )
}

#[test]
fn no_units_gives_no_note() {
    assert_eq!(call_counts_note(&[]), None);
}

#[test]
fn nothing_queried_gives_no_note() {
    assert_eq!(call_counts_note(&[unit("u", 0, 0, 0, 0, 0, 0, 0)]), None);
}

#[test]
fn one_unit_renders_the_template_with_the_gap() {
    let note = call_counts_note(&[unit("u", 6, 0, 0, 0, 11, 12, 2)]).expect("a note");
    assert_eq!(note, totals_text(6, 0, 0, 0, 11, 12, 2));
    assert!(note.ends_with("; 1 calls in the source with no edge in this index"));
}

#[test]
fn totals_are_sums_over_units() {
    let note = call_counts_note(&[
        unit("a", 3, 1, 2, 1, 4, 10, 0),
        unit("b", 2, 0, 0, 0, 5, 7, 3),
    ])
    .expect("a note");
    assert_eq!(note, totals_text(5, 1, 2, 1, 9, 17, 3));
    assert!(note.contains("; 8 calls in the source with no edge in this index"));
}

#[test]
fn more_edges_than_call_sites_floors_the_gap_at_zero() {
    let note = call_counts_note(&[unit("u", 1, 0, 0, 0, 15, 12, 0)]).expect("a note");
    assert!(note.ends_with("; 0 calls in the source with no edge in this index"));
}

#[test]
fn no_note_uses_the_forbidden_word() {
    let inputs = [
        vec![unit("u", 6, 0, 0, 0, 11, 12, 2)],
        vec![sized("a", 400, 12), sized("b", 20, 0)],
    ];
    for input in inputs {
        let note = call_counts_note(&input).expect("a note");
        assert!(!note.to_lowercase().contains(&forbidden()), "{note}");
    }
}

const UNITS_PREFIX: &str = "; units with 20 or more call sites where at least half have no edge: ";

fn listed(note: &str) -> Option<&str> {
    note.split_once(UNITS_PREFIX).map(|(_, list)| list)
}

#[test]
fn under_twenty_call_sites_is_not_listed() {
    let note = call_counts_note(&[sized("u", 19, 0)]).expect("a note");
    assert_eq!(listed(&note), None, "{note}");
    assert!(!note.contains("; units with"));
}

#[test]
fn exactly_half_without_an_edge_is_listed() {
    let note = call_counts_note(&[sized("u", 20, 10)]).expect("a note");
    assert_eq!(listed(&note), Some("u (20 call sites, 10 edges)"));
}

#[test]
fn under_half_without_an_edge_is_not_listed() {
    let note = call_counts_note(&[sized("u", 20, 11)]).expect("a note");
    assert_eq!(listed(&note), None, "{note}");
}

#[test]
fn at_most_five_listed_highest_ratio_first_ties_by_name() {
    let note = call_counts_note(&[
        sized("f", 20, 10),  // 0.5 — the lowest, dropped
        sized("e", 40, 0),   // 1.0
        sized("a", 20, 0),   // 1.0, ties with e, sorts first
        sized("c", 100, 30), // 0.7
        sized("b", 20, 4),   // 0.8
        sized("d", 30, 12),  // 0.6
    ])
    .expect("a note");
    assert_eq!(
        listed(&note),
        Some(
            "a (20 call sites, 0 edges), e (40 call sites, 0 edges), \
             b (20 call sites, 4 edges), c (100 call sites, 30 edges), \
             d (30 call sites, 12 edges)"
        )
    );
}

#[test]
fn the_units_list_follows_the_totals() {
    let note = call_counts_note(&[sized("u", 20, 0)]).expect("a note");
    assert!(
        note.starts_with(&totals_text(1, 0, 0, 0, 0, 20, 0)),
        "{note}"
    );
}
