//! Plan-04 §7 — the two string rules this crate owns.

use reachgraph_roots_proto_tonic::names::{camel_to_snake, impl_header_names_trait};

/// MEASURED (plan-04 §1 M1): six RPC names, six handler names.
#[test]
fn camel_to_snake_table() {
    let cases = [
        ("CreateTask", "create_task"),
        ("ListTasks", "list_tasks"),
        ("TransitionTask", "transition_task"),
        ("GetTask", "get_task"),
        ("DeleteTask", "delete_task"),
        ("CompleteTask", "complete_task"),
    ];
    for (rpc, expected) in cases {
        assert_eq!(camel_to_snake(rpc), expected, "{rpc}");
    }
}

/// The acronym cases, **and they are measured rather than reasoned about**.
///
/// Plan-04 §7 forbids guessing here: the rule must match what
/// `tonic-prost-build` emits, not what looks reasonable. Every expectation
/// below was read out of generated code — see
/// `tests/fixtures/oracle/acme.api.v1.methods.txt`, which is the generator's
/// own output for `tests/fixtures/oracle/acronym.proto`. The golden test
/// `snake_case_matches_generated_trait` in `keys.rs` asserts against that file;
/// this table is the same facts in the form a reader can see at a glance.
#[test]
fn camel_to_snake_acronyms_match_the_generator() {
    let cases = [
        ("GetWidgetByID", "get_widget_by_id"),
        ("ExportCSV", "export_csv"),
        ("HTTPProxy", "http_proxy"),
        ("GetHTTP2Stream", "get_http2_stream"),
        ("V2Migrate", "v2_migrate"),
    ];
    for (rpc, expected) in cases {
        assert_eq!(camel_to_snake(rpc), expected, "{rpc}");
    }
}

/// Plan-03 §8's grammar, parsed **anchored**.
#[test]
fn impl_header_parsing_is_anchored() {
    assert!(impl_header_names_trait("impl Widgets for Svc", "Widgets"));

    // The substring-search regression. Both shapes are real.
    assert!(
        !impl_header_names_trait("impl WidgetsExt for Svc", "Widgets"),
        "a trait whose name merely starts with the service name is a different trait"
    );
    assert!(
        !impl_header_names_trait("impl Foo for WidgetsClient<T>", "Widgets"),
        "the service name on the self-type side is not a trait impl of it"
    );

    // An inherent impl has no trait half at all.
    assert!(!impl_header_names_trait("impl Widgets", "Widgets"));

    // Not an impl header — plan-03 §8 renders a trait declaration as "Trait",
    // and a method as "Method".
    assert!(!impl_header_names_trait("Trait", "Widgets"));
    assert!(!impl_header_names_trait("Method", "Widgets"));
}

/// The path a trait was imported by never reaches the header (plan-03 §8), but
/// a generic self type and a qualified trait name both do.
#[test]
fn impl_header_parsing_strips_generics_and_paths() {
    assert!(impl_header_names_trait(
        "impl Widgets for Svc<T>",
        "Widgets"
    ));
    assert!(impl_header_names_trait(
        "impl pb::v1::Widgets for Svc",
        "Widgets"
    ));
    assert!(impl_header_names_trait(
        "impl Widgets<Request> for Svc",
        "Widgets"
    ));
    assert!(
        !impl_header_names_trait("impl Widgets for Svc", "widgets"),
        "the comparison is exact, not case-insensitive"
    );
}

/// `for` inside a name is not the separator.
#[test]
fn impl_header_parsing_splits_on_the_keyword_only() {
    assert!(
        !impl_header_names_trait("impl Reformat", "Reformat"),
        "an inherent impl whose self type contains `for` is still inherent"
    );
    assert!(impl_header_names_trait(
        "impl Widgets for Reformatter",
        "Widgets"
    ));
}
