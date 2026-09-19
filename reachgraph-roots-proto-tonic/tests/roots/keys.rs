//! Plan-04 §3 and §8 — the join key, and the generator that proves its spelling.

use std::path::{Path, PathBuf};

use reachgraph_plugin_api::ContractId;
use reachgraph_roots_proto_tonic::contract::parse;
use reachgraph_roots_proto_tonic::keys::join_key;
use reachgraph_roots_proto_tonic::names::camel_to_snake;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn the_key_is_the_fully_qualified_name() {
    let key = join_key(Some("acme.api.v1"), "Widgets", "CreateWidget");
    assert_eq!(key, "acme.api.v1.Widgets/CreateWidget");
    assert_ne!(key, "CreateWidget", "never the bare RPC name");
    assert!(
        !key.starts_with('/'),
        "the leading slash is HTTP/2 path syntax, not identity"
    );
}

/// The M7 collision, at the level of the key itself.
#[test]
fn the_collision_is_separated_by_the_package_and_the_service() {
    let served = join_key(Some("acme.api.v1"), "Widgets", "CreateWidget");
    let consumed = join_key(Some("acme.store.v2"), "WidgetDb", "CreateWidget");
    assert_ne!(served, consumed);
}

#[test]
fn no_package_yields_bare_service_key() {
    assert_eq!(join_key(None, "Svc", "Rpc"), "Svc/Rpc");
    assert!(
        !join_key(None, "Svc", "Rpc").starts_with('.'),
        "a missing package is not a synthesised empty one"
    );
}

/// The golden assertion, against the generator's own output.
///
/// # Provenance of the fixture
///
/// `tests/fixtures/oracle/acme.api.v1.generated.rs` is `tonic-prost-build`
/// 0.14.6's output, verbatim and unedited, for
/// `tests/fixtures/oracle/acronym.proto`. It was produced on 2026-09-19 by a
/// scratch crate outside this workspace whose `build.rs` ran
/// `tonic_prost_build::configure().build_server(true).build_client(true)`.
/// Regenerating it is a deliberate step, never something a test run does: a
/// golden file that rewrites itself updates both sides of a contract at once
/// and the test then passes through a break.
///
/// Two independent facts are asserted from it, and both are read out of the
/// generated text rather than restated here:
///
/// 1. every dispatch literal equals `/` + this crate's `join_key`;
/// 2. every emitted trait method name equals this crate's `camel_to_snake` of
///    the RPC name in the same literal.
#[test]
fn join_key_matches_the_generated_path_literal() {
    let generated = std::fs::read_to_string(fixture("oracle").join("acme.api.v1.generated.rs"))
        .expect("the generated fixture is checked in");
    let source = std::fs::read_to_string(fixture("oracle").join("acronym.proto"))
        .expect("the proto fixture is checked in");
    let contract = parse(&ContractId("acronym.proto".to_owned()), &source).expect("parses");

    let literals = dispatch_literals(&generated);
    assert_eq!(
        literals.len(),
        8,
        "the generated file carries one path literal per RPC per half: {literals:?}"
    );

    let service = &contract.services[0];
    assert_eq!(
        service.rpcs.len(),
        8,
        "the oracle proto declares eight RPCs"
    );

    for rpc in &service.rpcs {
        let key = join_key(contract.package.as_deref(), &service.name, rpc);
        assert!(
            literals.contains(&format!("/{key}")),
            "the generator emits `/{key}`; it emitted {literals:?}"
        );
    }
}

/// Plan-04 §7's oracle: the computed handler name is the generated one.
#[test]
fn snake_case_matches_generated_trait() {
    let generated = std::fs::read_to_string(fixture("oracle").join("acme.api.v1.generated.rs"))
        .expect("the generated fixture is checked in");
    let source = std::fs::read_to_string(fixture("oracle").join("acronym.proto"))
        .expect("the proto fixture is checked in");
    let contract = parse(&ContractId("acronym.proto".to_owned()), &source).expect("parses");

    let emitted = trait_method_names(&generated);
    assert_eq!(
        emitted.len(),
        8,
        "one method per RPC in the server trait: {emitted:?}"
    );

    let computed: Vec<String> = contract.services[0]
        .rpcs
        .iter()
        .map(|rpc| camel_to_snake(rpc))
        .collect();
    assert_eq!(
        computed, emitted,
        "a disagreement here is a silently unbound root in every repository"
    );
}

/// Every `"/pkg.Service/Rpc"` literal in the generated text, deduplicated and
/// in first-seen order.
fn dispatch_literals(generated: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in generated.lines() {
        let mut rest = line;
        while let Some(open) = rest.find('"') {
            let after = &rest[open + 1..];
            let Some(close) = after.find('"') else { break };
            let literal = &after[..close];
            if literal.starts_with('/')
                && literal.matches('/').count() == 2
                && !out.iter().any(|seen| seen == literal)
            {
                out.push(literal.to_owned());
            }
            rest = &after[close + 1..];
        }
    }
    out
}

/// The server trait's method names, in declaration order.
///
/// The trait is the block that declares `async fn` without a body; the client
/// impl above it declares `pub async fn`. Taking only the unprefixed form is
/// what separates the two halves without parsing Rust.
fn trait_method_names(generated: &str) -> Vec<String> {
    generated
        .lines()
        .filter_map(|line| line.trim().strip_prefix("async fn "))
        .filter_map(|rest| rest.split(['(', '<']).next())
        .map(str::to_owned)
        .collect()
}
