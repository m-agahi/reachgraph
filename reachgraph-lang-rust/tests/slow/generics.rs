//! `fx-generic` — plan-03 §12, and the gap is asserted rather than papered over.

use reachgraph_plugin_api::EdgeTarget;

use crate::support::{all_edges, all_symbols, load, named};

/// rust-analyzer issue **#19358** — and what it actually costs, MEASURED.
///
/// Plan-03 §12 and §13 predicted this fixture would show **no edge** for a call
/// dispatched through a generic parameter, and told this test to assert the
/// absence. MEASURED 2026-09-19 against `ra_ap` 0.0.352: that prediction is
/// wrong, and the truth is sharper.
///
/// `through_generic` calls `value.run()` where `value: &T, T: Op`. An edge IS
/// emitted — to the **trait's declaration**, `Op::run`. `through_concrete`
/// calls the same method on `Only` and its edge goes to the **implementation**,
/// `<Only as Op>::run`. So the missing thing is not the call; it is the
/// dispatch. The graph shows the generic caller reaching a declaration with no
/// body, and it never reaches `Only`'s implementation, **even though `Only` is
/// the only implementor in the crate**.
///
/// That is the failure mode to state plainly, because it is the one that
/// misleads: a handler called only through a generic looks reachable while the
/// code that actually runs looks unreachable. design.md §8's binding rule
/// applies unchanged — show it as missing, never infer the vtable edge to fill
/// the hole.
///
/// # When this test fails
///
/// An upstream fix that resolves a generic call to its implementors makes the
/// last assertion fail. **The correct response is to delete this test and
/// update plan-03 §12 — not to relax the assertion.**
#[test]
fn a_generic_call_reaches_the_declaration_and_never_the_implementation() {
    let (plugin, units) = load("fx-generic");
    let symbols = all_symbols(&plugin, &units);
    let edges = all_edges(&plugin, &units);

    // Two symbols are named `run`: the trait's declaration, whose container is
    // the trait, and the impl's definition, whose container is the impl block.
    let declaration = symbols
        .iter()
        .find(|symbol| {
            symbol.name == "run"
                && symbol.container.as_ref().is_some_and(|container| {
                    symbols
                        .iter()
                        .any(|c| &c.id == container && c.raw_kind == "Trait")
                })
        })
        .expect("the trait declares `run`");
    let implementation = symbols
        .iter()
        .find(|symbol| {
            symbol.name == "run"
                && symbol.container.as_ref().is_some_and(|container| {
                    symbols
                        .iter()
                        .any(|c| &c.id == container && c.raw_kind == "impl Op for Only")
                })
        })
        .expect("the impl defines `run`");
    assert_ne!(declaration.id.raw, implementation.id.raw);

    let through_generic = named(&symbols, "through_generic");
    let through_concrete = named(&symbols, "through_concrete");

    let reaches = |from: &reachgraph_plugin_api::NodeId, to: &reachgraph_plugin_api::NodeId| {
        edges.iter().any(|edge| {
            &edge.from == from && matches!(&edge.to, EdgeTarget::Resolved(id) if id == to)
        })
    };

    assert!(
        reaches(&through_concrete.id, &implementation.id),
        "a concrete call reaches the implementation"
    );
    assert!(
        reaches(&through_generic.id, &declaration.id),
        "a generic call reaches the trait declaration"
    );
    assert!(
        !reaches(&through_generic.id, &implementation.id),
        "and never the implementation, even with one implementor in the crate.          If this now resolves, DELETE this test and update plan-03 §12"
    );
}

/// The gap is invisible rather than reported, and that is §9's honest
/// limitation made executable.
///
/// When `ra_ap` cannot resolve a call it returns no `CallItem` at all, so there
/// is no candidate set and `EdgeTarget::Unresolved` is never constructed. A
/// change to this is deliberate (plan-03 §14 question 8), not accidental.
#[test]
fn the_missing_call_is_not_reported_as_unresolved() {
    let (plugin, units) = load("fx-generic");
    let edges = all_edges(&plugin, &units);

    assert!(
        !edges
            .iter()
            .any(|edge| matches!(edge.to, EdgeTarget::Unresolved { .. })),
        "v0.1 never constructs EdgeTarget::Unresolved"
    );
}
