//! `fx-attr` — an impl header whose trait does not resolve.
//!
//! MEASURED 2026-09-19 on `/home/max/git/yadgarhq/task`: the emitted header for
//! `#[tonic::async_trait] impl TaskService for Task` was `impl Task` — the
//! trait clause silently dropped. Plan-04 §7 parses that header with an
//! anchored `impl <Trait> for <Self>` rule, so **all six served roots failed to
//! bind, no shard was written, and every symbol in the repository read as not
//! reachable from any endpoint.**
//!
//! Two controls localise the cause, and the first one falsified the first
//! hypothesis. An attribute macro is **not** what breaks it: `#[pm::keep] impl
//! Decorated for Subject` keeps its trait, and so does the bare impl. What
//! breaks it is that `TaskService` is declared in build-script output, which
//! plan-03 §9 D-D leaves out of the crate graph — so `hir::Impl::trait_`
//! answers `None` because the trait is not there to resolve to.

use reachgraph_plugin_api::{Symbol, SymbolKind};

use crate::support::{all_symbols, build_fixture, load};

fn container_kinds<'a>(symbols: &'a [Symbol], method: &str) -> Vec<&'a str> {
    symbols
        .iter()
        .filter(|symbol| symbol.name == method && symbol.kind == SymbolKind::Method)
        .filter_map(|symbol| symbol.container.as_ref())
        .filter_map(|container| {
            symbols
                .iter()
                .find(|candidate| candidate.id == *container)
                .map(|candidate| candidate.raw_kind.as_str())
        })
        .collect()
}

/// The control: nothing about the bare impl changes.
#[test]
fn a_bare_impl_header_names_its_trait() {
    build_fixture("fx-attr");
    let (plugin, units) = load("fx-attr");
    let symbols = all_symbols(&plugin, &units);

    assert_eq!(
        container_kinds(&symbols, "bare"),
        vec!["Trait", "impl Bare for Subject"]
    );
}

/// Control two: an attribute macro changes nothing while the trait resolves.
#[test]
fn a_decorated_impl_with_a_resolvable_trait_names_it() {
    build_fixture("fx-attr");
    let (plugin, units) = load("fx-attr");
    let symbols = all_symbols(&plugin, &units);

    assert_eq!(
        container_kinds(&symbols, "decorated"),
        vec!["Trait", "impl Decorated for Subject"]
    );
}

/// The case: a trait the index does not hold must not remove the clause the
/// source visibly contains.
///
/// Nothing is invented here, and that is the line this test sits on. An
/// unexpanded macro means calls *through* it were not measured, and ADR-0728
/// keeps those absent rather than guessed. The trait clause is not a call and
/// was not inferred: it is written in the file, and plan-03 §8 already
/// specifies the **declared** name rather than a resolved path.
#[test]
fn an_impl_of_an_unresolvable_trait_still_names_it() {
    build_fixture("fx-attr");
    let (plugin, units) = load("fx-attr");
    let symbols = all_symbols(&plugin, &units);

    assert_eq!(
        container_kinds(&symbols, "generated"),
        vec!["impl Generated for Subject"],
        "the trait is written in the source; nothing here is inferred"
    );
}
