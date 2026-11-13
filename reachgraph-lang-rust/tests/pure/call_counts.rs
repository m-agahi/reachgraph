//! Task 2068 phase 1 — the call-counts note, from counts alone.
//!
//! The note is the only place a reader sees calls in the source that have no
//! edge in the index, so its wording and its thresholds are pinned here.
//!
//! The gap is taken per function and then summed. A function with more edges
//! than call sites (a function a macro generates, or one that calls a function
//! a macro defines) must not cancel calls without an edge in another function.

use reachgraph_lang_rust::coverage::{call_counts_note, FunctionCallCounts, UnitCallCounts};

/// The word the note must never use, assembled so this file does not carry it.
fn forbidden() -> String {
    ['d', 'e', 'a', 'd'].iter().collect()
}

/// One function with only call sites, macro sites and edges set.
fn function(call_sites: usize, macro_call_sites: usize, edges: usize) -> FunctionCallCounts {
    FunctionCallCounts {
        call_sites,
        macro_call_sites,
        edges,
        ..FunctionCallCounts::default()
    }
}

/// A unit holding exactly these functions.
fn unit(name: &str, functions: &[FunctionCallCounts]) -> UnitCallCounts {
    let mut unit = UnitCallCounts::new(name);
    for function in functions {
        unit.add_function(function);
    }
    unit
}

/// A unit of one function — what the top-units list reads.
fn sized(name: &str, call_sites: usize, edges: usize) -> UnitCallCounts {
    unit(name, &[function(call_sites, 0, edges)])
}

#[allow(clippy::too_many_arguments)]
fn totals_text(
    q: usize,
    n: usize,
    t: usize,
    r: usize,
    e: usize,
    s: usize,
    m: usize,
    g: usize,
    x: usize,
) -> String {
    format!(
        "reachgraph-lang-rust call counts: {q} functions and methods queried, {n} with no call \
         hierarchy (outgoing_calls returned none), {t} call targets dropped at the file lookup, \
         {r} call ranges dropped at the file lookup, {e} edges produced, {s} call sites in the \
         source outside macro calls, {m} calls in one-level macro expansions; {g} calls in the \
         source with no edge in this index; {x} edges in functions that have more edges than \
         call sites"
    )
}

#[test]
fn no_units_gives_no_note() {
    assert_eq!(call_counts_note(&[]), None);
}

#[test]
fn nothing_queried_gives_no_note() {
    assert_eq!(call_counts_note(&[unit("u", &[])]), None);
}

#[test]
fn a_function_adds_every_counter_and_one_query() {
    let mut counts = UnitCallCounts::new("u");
    counts.add_function(&FunctionCallCounts {
        no_hierarchy: true,
        targets_dropped: 2,
        ranges_dropped: 3,
        edges: 4,
        call_sites: 9,
        macro_call_sites: 5,
    });
    counts.add_function(&function(1, 0, 3));
    assert_eq!(
        counts,
        UnitCallCounts {
            unit: "u".to_owned(),
            queried: 2,
            no_hierarchy: 1,
            targets_dropped: 2,
            ranges_dropped: 3,
            edges: 7,
            call_sites: 10,
            macro_call_sites: 5,
            calls_without_edge: 5,
            edges_beyond_call_sites: 2,
        }
    );
}

#[test]
fn one_unit_renders_the_template() {
    let mut functions = vec![function(12, 2, 11)];
    functions.extend(std::iter::repeat_n(function(0, 0, 0), 5));
    let note = call_counts_note(&[unit("u", &functions)]).expect("a note");
    assert_eq!(note, totals_text(6, 0, 0, 0, 11, 12, 2, 1, 0));
}

#[test]
fn totals_are_sums_over_units() {
    let note = call_counts_note(&[
        unit("a", &[function(10, 0, 4), function(0, 0, 0)]),
        unit("b", &[function(7, 3, 5)]),
    ])
    .expect("a note");
    assert_eq!(note, totals_text(3, 0, 0, 0, 9, 17, 3, 8, 0));
}

/// The defect the review found: summing first let B's surplus hide A's gap.
#[test]
fn a_surplus_in_one_function_does_not_hide_a_gap_in_another() {
    let note =
        call_counts_note(&[unit("u", &[function(10, 0, 0), function(0, 0, 10)])]).expect("a note");
    assert_eq!(note, totals_text(2, 0, 0, 0, 10, 10, 0, 10, 10));
}

#[test]
fn a_surplus_in_one_unit_does_not_hide_a_gap_in_another() {
    let note = call_counts_note(&[sized("a", 10, 0), sized("b", 0, 10)]).expect("a note");
    assert_eq!(note, totals_text(2, 0, 0, 0, 10, 10, 0, 10, 10));
}

#[test]
fn more_edges_than_call_sites_is_reported_as_surplus_not_as_a_negative_gap() {
    let note = call_counts_note(&[sized("u", 12, 15)]).expect("a note");
    assert_eq!(note, totals_text(1, 0, 0, 0, 15, 12, 0, 0, 3));
}

#[test]
fn no_note_uses_the_forbidden_word() {
    let inputs = [
        vec![sized("u", 12, 11)],
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
    assert_eq!(listed(&note), Some("u (20 call sites, 10 with no edge)"));
}

#[test]
fn under_half_without_an_edge_is_not_listed() {
    let note = call_counts_note(&[sized("u", 20, 11)]).expect("a note");
    assert_eq!(listed(&note), None, "{note}");
}

/// The listing reads the per-function gap too: a surplus elsewhere in the
/// unit must not take a unit off the list.
#[test]
fn a_surplus_does_not_take_a_unit_off_the_list() {
    let note =
        call_counts_note(&[unit("u", &[function(20, 0, 0), function(0, 0, 20)])]).expect("a note");
    assert_eq!(listed(&note), Some("u (20 call sites, 20 with no edge)"));
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
            "a (20 call sites, 20 with no edge), e (40 call sites, 40 with no edge), \
             b (20 call sites, 16 with no edge), c (100 call sites, 70 with no edge), \
             d (30 call sites, 18 with no edge)"
        )
    );
}

#[test]
fn the_units_list_follows_the_totals() {
    let note = call_counts_note(&[sized("u", 20, 0)]).expect("a note");
    assert!(
        note.starts_with(&totals_text(1, 0, 0, 0, 0, 20, 0, 20, 0)),
        "{note}"
    );
}
