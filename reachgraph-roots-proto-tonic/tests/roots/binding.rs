//! Plan-04 §7 and §11 — a root's handler, and never by name alone.

use reachgraph_plugin_api::Symbol;
use reachgraph_roots_proto_tonic::bind::{bind_generated_client, bind_handler, Binding, Operation};
use reachgraph_roots_proto_tonic::unbound::UnboundReason;

use crate::fake::{FakeIndex, Sym};

/// The measured shape of a served repository: one impl, its methods inside it,
/// and a test double of the **consumed** service with the same method name.
fn repository() -> FakeIndex {
    let served_impl = Sym::impl_block("Svc", "impl Widgets for Svc", "src/service.rs", 100).build();
    let mock_impl =
        Sym::impl_block("MockDb", "impl WidgetDb for MockDb", "tests/mock.rs", 200).build();
    let widgets_trait = Sym::trait_decl("Widgets", "src/pb.rs", 300).build();

    FakeIndex::new(vec![
        Sym::method("create_widget", "src/service.rs", 110)
            .inside(&served_impl)
            .build(),
        Sym::method("list_widgets", "src/service.rs", 120)
            .inside(&served_impl)
            .build(),
        Sym::method("create_widget", "tests/mock.rs", 210)
            .inside(&mock_impl)
            .build(),
        // MEASURED: the walk also emits a trait's own associated functions, so
        // a third `create_widget` with the same name exists in a real index.
        Sym::method("create_widget", "src/pb.rs", 310)
            .inside(&widgets_trait)
            .build(),
        served_impl,
        mock_impl,
        widgets_trait,
    ])
}

/// The operation under test, keyed the way the plugin keys it.
fn op(service: &str, rpc: &str) -> Operation {
    Operation::new(Some("acme.api.v1"), service, rpc)
}

fn bound_symbol<'a>(index: &'a FakeIndex, binding: &Binding) -> &'a Symbol {
    let Binding::Bound(id) = binding else {
        panic!("expected a bound handler, got {binding:?}");
    };
    use reachgraph_plugin_api::SymbolIndex;
    index.get(id).expect("the bound id is in the index")
}

/// MEASURED (plan-04 §1 M1): 6/6 on the real contract. Here, 2/2.
#[test]
fn join_covers_every_served_rpc() {
    let index = repository();

    for (rpc, handler, offset) in [
        ("CreateWidget", "create_widget", 110),
        ("ListWidgets", "list_widgets", 120),
    ] {
        let binding = bind_handler(&index, &op("Widgets", rpc));
        let symbol = bound_symbol(&index, &binding);
        assert_eq!(symbol.name, handler);
        assert_eq!(
            symbol.range.span.expect("the fake carries a span").start,
            offset,
            "{rpc} bound to the wrong `{handler}`"
        );
    }
}

/// The M6 collision, resolved by the container and the test-context guard.
///
/// Both `create_widget` symbols are `is_test == false` — MEASURED, that is what
/// the engine emits for a method in a `tests/` target — so the name and the
/// flag together are not enough, and the container has to decide.
#[test]
fn mock_collision_resolved_by_container_and_test_context() {
    let index = repository();
    let binding = bind_handler(&index, &op("Widgets", "CreateWidget"));
    let symbol = bound_symbol(&index, &binding);
    assert_eq!(symbol.range.file.to_string_lossy(), "src/service.rs");

    // And the consumed service binds to nothing in first-party source, which is
    // the other half of the two-direction pin.
    assert!(
        matches!(
            bind_handler(&index, &op("WidgetDb", "CreateWidget")),
            Binding::Unbound(UnboundReason::NoMatchingImpl { .. })
        ),
        "the mock is a test double, not a handler: {:?}",
        bind_handler(&index, &op("WidgetDb", "CreateWidget"))
    );
}

/// A method in an impl of a different trait is a different method.
#[test]
fn container_trait_must_match_the_service() {
    let unrelated = Sym::impl_block("Svc", "impl Unrelated for Svc", "src/service.rs", 100).build();
    let index = FakeIndex::new(vec![
        Sym::method("create_widget", "src/service.rs", 110)
            .inside(&unrelated)
            .build(),
        unrelated,
    ]);

    let binding = bind_handler(&index, &op("Widgets", "CreateWidget"));
    let Binding::Unbound(UnboundReason::NoMatchingImpl { named, .. }) = &binding else {
        panic!("expected an unbound root naming the candidate count, got {binding:?}");
    };
    assert_eq!(*named, 1);
    assert!(binding.reason().contains("impl Widgets for"), "{binding:?}");
}

/// A method with no container at all binds to nothing.
#[test]
fn a_containerless_method_does_not_bind() {
    let index = FakeIndex::new(vec![
        Sym::function("create_widget", "src/free.rs", 10).build()
    ]);
    assert!(matches!(
        bind_handler(&index, &op("Widgets", "CreateWidget")),
        Binding::Unbound(UnboundReason::NoMatchingImpl { .. })
    ));
}

/// No candidate of that name at all is a different reason from a candidate in
/// the wrong impl, and plan-04 §9 gives them different rows.
#[test]
fn no_candidate_of_that_name_says_so() {
    let index = repository();
    let binding = bind_handler(&index, &op("Widgets", "DeleteWidget"));
    assert!(
        matches!(binding, Binding::Unbound(UnboundReason::NoCandidate { .. })),
        "{binding:?}"
    );
    assert!(binding.reason().contains("delete_widget"), "{binding:?}");
}

/// Two survivors are never resolved by preference — plan-04 §7.
#[test]
fn ambiguous_candidates_stay_unbound() {
    let first = Sym::impl_block("A", "impl Widgets for A", "src/a.rs", 100).build();
    let second = Sym::impl_block("B", "impl Widgets for B", "src/b.rs", 200).build();
    let index = FakeIndex::new(vec![
        Sym::method("create_widget", "src/a.rs", 110)
            .inside(&first)
            .build(),
        Sym::method("create_widget", "src/b.rs", 210)
            .inside(&second)
            .build(),
        first,
        second,
    ]);

    let binding = bind_handler(&index, &op("Widgets", "CreateWidget"));
    let Binding::Unbound(UnboundReason::Ambiguous { count, .. }) = &binding else {
        panic!("an ambiguous binding is a reported gap, never a pick: {binding:?}");
    };
    assert_eq!(*count, 2);
    assert!(binding.reason().contains('2'), "{binding:?}");
}

/// Plan-04 §11 — the consumed root's leaf is the generated client method.
#[test]
fn consumed_rpc_binds_generated_client_when_present() {
    let generated = "target/debug/build/x-1234/out/acme.store.v2.rs";
    let client_impl =
        Sym::impl_block("WidgetDbClient", "impl WidgetDbClient<T>", generated, 100).build();
    let index = FakeIndex::new(vec![
        Sym::method("create_widget", generated, 110)
            .inside(&client_impl)
            .build(),
        client_impl,
    ]);

    let binding = bind_generated_client(&index, &op("WidgetDb", "CreateWidget"));
    let symbol = bound_symbol(&index, &binding);
    assert_eq!(symbol.range.file.to_string_lossy(), generated);
}

/// The same method outside `target/**/out/` is first-party code, not the leaf.
#[test]
fn a_first_party_client_wrapper_is_not_the_generated_leaf() {
    let wrapper = Sym::impl_block(
        "WidgetDbClient",
        "impl WidgetDbClient<T>",
        "src/client.rs",
        100,
    )
    .build();
    let index = FakeIndex::new(vec![
        Sym::method("create_widget", "src/client.rs", 110)
            .inside(&wrapper)
            .build(),
        wrapper,
    ]);

    assert!(
        matches!(
            bind_generated_client(&index, &op("WidgetDb", "CreateWidget")),
            Binding::Unbound(UnboundReason::GeneratedStubNotIndexed { .. })
        ),
        "only a symbol under `target/**/out/` is the generated leaf"
    );
}

/// A trait impl named `<Service>Client` is the wrong shape: the generated
/// client method sits in an **inherent** impl.
#[test]
fn a_trait_impl_is_not_the_generated_client() {
    let generated = "target/debug/build/x-1234/out/acme.store.v2.rs";
    let wrong = Sym::impl_block(
        "WidgetDbClient",
        "impl Clone for WidgetDbClient<T>",
        generated,
        100,
    )
    .build();
    let index = FakeIndex::new(vec![
        Sym::method("create_widget", generated, 110)
            .inside(&wrong)
            .build(),
        wrong,
    ]);

    assert!(matches!(
        bind_generated_client(&index, &op("WidgetDb", "CreateWidget")),
        Binding::Unbound(UnboundReason::GeneratedStubNotIndexed { .. })
    ));
}
