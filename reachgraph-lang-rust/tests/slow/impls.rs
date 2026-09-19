//! `fx-impl` — the fixture plan-04 binds against.
//!
//! Its two assertions are the two plan-04 §7 depends on: every method has a
//! `container`, and the container's `raw_kind` is the impl header in plan-03
//! §8's exact grammar.

use reachgraph_plugin_api::{Symbol, SymbolKind};

use crate::support::{all_symbols, load};

fn containers_of<'a>(symbols: &'a [Symbol], name: &str) -> Vec<&'a str> {
    symbols
        .iter()
        .filter(|symbol| symbol.name == name && symbol.kind == SymbolKind::Method)
        .filter_map(|symbol| symbol.container.as_ref())
        .filter_map(|container| {
            symbols
                .iter()
                .find(|candidate| candidate.id == *container)
                .map(|candidate| candidate.raw_kind.as_str())
        })
        .collect()
}

#[test]
fn every_method_has_a_container() {
    let (plugin, units) = load("fx-impl");
    let symbols = all_symbols(&plugin, &units);

    let methods: Vec<&Symbol> = symbols
        .iter()
        .filter(|symbol| symbol.kind == SymbolKind::Method)
        .collect();
    assert!(!methods.is_empty(), "the fixture has methods");

    for method in methods {
        assert!(
            method.container.is_some(),
            "{} has no container; plan-04 cannot bind it",
            method.name
        );
    }
}

/// Plan-03 §8's grammar, produced by the walk rather than by the renderer test.
#[test]
fn a_containers_raw_kind_is_the_impl_header() {
    let (plugin, units) = load("fx-impl");
    let symbols = all_symbols(&plugin, &units);

    let create_containers = containers_of(&symbols, "create");
    assert!(
        create_containers.contains(&"impl Svc for Real"),
        "expected `impl Svc for Real` among {create_containers:?}"
    );

    let helper_containers = containers_of(&symbols, "helper");
    assert_eq!(
        helper_containers,
        vec!["impl Real"],
        "an inherent impl renders without a trait half"
    );
}

/// The trait half is the **declared** name, not the path it was imported by.
///
/// The `tests/` unit writes `use i::Svc;` and then `impl Svc for Mock`. If the
/// walk reported a use-path the header would read `impl i::Svc for Mock`, and
/// plan-04 — which compares the trait name against a proto service name —
/// would fail to bind it.
#[test]
fn the_trait_half_is_the_declared_name() {
    let (plugin, units) = load("fx-impl");
    let symbols = all_symbols(&plugin, &units);

    let headers: Vec<&str> = symbols
        .iter()
        .filter(|symbol| symbol.raw_kind.starts_with("impl "))
        .map(|symbol| symbol.raw_kind.as_str())
        .collect();

    assert!(
        headers.contains(&"impl Svc for Mock"),
        "expected the declared name, saw {headers:?}"
    );
    assert!(
        !headers.iter().any(|header| header.contains("i::Svc")),
        "no use-path reaches the header: {headers:?}"
    );
}

/// Plan-04 §6 uses `is_test` to decide **direction**, and getting it wrong
/// manufactures a phantom root. Both implementations define `create`.
#[test]
fn is_test_separates_the_mock_from_the_real_handler() {
    let (plugin, units) = load("fx-impl");
    let symbols = all_symbols(&plugin, &units);

    let test_unit = units
        .iter()
        .find(|unit| unit.display_name.contains("(test)"))
        .expect("a tests/ target is its own unit");
    assert!(test_unit.id.0.contains("::test"));

    let the_test = symbols
        .iter()
        .find(|symbol| symbol.name == "the_mock_creates")
        .expect("the #[test] function is emitted");
    assert!(the_test.is_test, "#[test] sets is_test");

    let real = symbols
        .iter()
        .find(|symbol| symbol.name == "helper")
        .expect("the inherent method is emitted");
    assert!(!real.is_test, "a src/ method is not a test");
}

/// Plan-03 §10 rules 3 and 4 compare **package** identity, not unit identity.
///
/// `fx-impl` is the shape that catches the difference: one package, two units
/// (`i` and `i (test)`) over one `src/`. A classifier comparing unit ids would
/// call `src/lib.rs` a `WorkspaceSibling` while the test unit was indexed.
#[test]
fn a_packages_own_source_is_first_party_to_its_test_unit_too() {
    use reachgraph_plugin_api::{Category, Classifier};

    let (plugin, units) = load("fx-impl");
    let test_unit = units
        .iter()
        .find(|unit| unit.display_name.contains("(test)"))
        .expect("a tests/ target is its own unit");

    let own_src = crate::support::fixture("fx-impl").join("i/src/lib.rs");
    assert_eq!(plugin.classify(&own_src, test_unit), Category::FirstParty);
}
