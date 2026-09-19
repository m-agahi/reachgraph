//! Plan-04 §6 — where a direction-blind join produces phantom roots.

use reachgraph_plugin_api::Direction;
use reachgraph_roots_proto_tonic::direction::{
    direction_of, is_served_signal, is_test_context, mentions_client, DirectionCall,
    ServiceEvidence,
};

use crate::fake::Sym;

#[test]
fn a_first_party_impl_is_a_served_signal() {
    let served = Sym::impl_block("Svc", "impl Widgets for Svc", "src/service.rs", 10).build();
    assert!(is_served_signal(&served, "Widgets"));
    assert!(
        !is_served_signal(&served, "WidgetDb"),
        "the header names one trait, and it is not this one"
    );
}

/// The M6 regression, in the shape `reachgraph-lang-rust` was MEASURED to emit.
///
/// # This fixture is the point of the test
///
/// Plan-04 §12 specifies this case with `is_test: true` on the mock. MEASURED
/// 2026-09-19 that the engine never sets it there: `is_test` is populated for
/// `#[test]` functions only, so a test double's impl block and its methods both
/// arrive with `is_test == false`. A fixture written to the plan would pass
/// against a guard that reads `is_test` alone and would prove nothing about
/// any real repository.
///
/// So the mock below carries exactly what the engine produces — `is_test:
/// false`, a `tests/` path — and the guard has to notice the path.
#[test]
fn test_double_does_not_make_a_service_served() {
    let mock = Sym::impl_block("MockDb", "impl WidgetDb for MockDb", "tests/mock.rs", 10).build();

    assert!(
        !mock.is_test,
        "the engine does not flag a mock's impl block"
    );
    assert!(
        is_test_context(&mock),
        "and the guard still has to see that it is one"
    );
    assert!(
        !is_served_signal(&mock, "WidgetDb"),
        "a consumed service must not acquire a served signal from a test double"
    );

    let evidence = ServiceEvidence {
        first_party_impl: false,
        client_reference: true,
    };
    assert_eq!(direction_of(evidence).direction(), Direction::Consumed);
}

/// `benches/` and `examples/` are the other two Cargo target directories that
/// hold code nobody serves traffic from.
#[test]
fn the_other_cargo_test_targets_are_test_context_too() {
    for file in [
        "tests/mock.rs",
        "benches/bench.rs",
        "examples/demo.rs",
        "crates/api/tests/mock.rs",
    ] {
        let symbol = Sym::impl_block("MockDb", "impl WidgetDb for MockDb", file, 10).build();
        assert!(is_test_context(&symbol), "{file}");
    }

    for file in ["src/service.rs", "src/testing.rs", "src/tests_support.rs"] {
        let symbol = Sym::impl_block("Svc", "impl Widgets for Svc", file, 10).build();
        assert!(
            !is_test_context(&symbol),
            "{file} is a path component named something else, not a test target"
        );
    }
}

/// The engine's own flag still counts, where it is set.
#[test]
fn the_engines_is_test_flag_still_counts() {
    let flagged = Sym::function("create_widget", "src/service.rs", 10)
        .test_fn()
        .build();
    assert!(is_test_context(&flagged));
}

/// M9 and M10: a `*_server` module is generated for every contract whether or
/// not the repository serves it, so nothing under `target/` is evidence.
#[test]
fn generated_server_module_is_not_a_served_signal() {
    let generated = Sym::impl_block(
        "WidgetDbServer",
        "impl WidgetDb for WidgetDbServer<T>",
        "target/debug/build/x-1234/out/acme.store.v2.rs",
        10,
    )
    .build();

    assert!(
        !is_served_signal(&generated, "WidgetDb"),
        "generated code is build output, not first-party usage"
    );
}

/// The corroboration is an identifier match, not a substring one.
#[test]
fn client_reference_is_identifier_anchored() {
    let source = "    db: WidgetDbClient<Channel>,\n    fn new() { WidgetDbClient::new(channel) }";
    assert!(mentions_client(source, "WidgetDb"));

    assert!(
        !mentions_client(source, "Widget"),
        "`WidgetDbClient` does not corroborate a service named `Widget`"
    );
    assert!(
        !mentions_client("struct MyWidgetDbClientWrapper;", "WidgetDb"),
        "an identifier that merely contains the name is a different identifier"
    );
    assert!(!mentions_client("impl Widgets for Svc {}", "Widgets"));
}

/// The asymmetry, stated as a test: absent evidence takes the failure mode that
/// shows up in the output.
#[test]
fn no_evidence_defaults_to_consumed() {
    let nothing = ServiceEvidence {
        first_party_impl: false,
        client_reference: false,
    };
    assert_eq!(direction_of(nothing), DirectionCall::NoEvidence);
    assert_eq!(direction_of(nothing).direction(), Direction::Consumed);

    let served = ServiceEvidence {
        first_party_impl: true,
        client_reference: false,
    };
    assert_eq!(direction_of(served), DirectionCall::Served);
    assert_eq!(direction_of(served).direction(), Direction::Served);

    let both = ServiceEvidence {
        first_party_impl: true,
        client_reference: true,
    };
    assert_eq!(
        direction_of(both).direction(),
        Direction::Served,
        "a repository that serves a contract and also calls it is serving it"
    );
}
