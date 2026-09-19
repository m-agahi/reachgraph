//! The plugin, end to end — plan-04 §6, §8, §9, §10 and §11.
//!
//! Every case here runs `roots()` over a checked-in fixture repository and a
//! hand-built index. The fixture carries the measured shapes of plan-04 §1: a
//! served contract, a consumed contract whose RPC name collides with it, a
//! version-less contract, a service-less contract, and a test double of the
//! consumed service.

use std::path::{Path, PathBuf};

use reachgraph_plugin_api::{
    ContractId, Direction, Plugin, Preflight, Root, RootBinding, RootProvider, VersionKey,
};
use reachgraph_roots_proto_tonic::ProtoTonicPlugin;

use crate::fake::{FakeIndex, Sym};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// The index `reachgraph-lang-rust` would produce for `tests/fixtures/repo`.
///
/// Paths are repo-relative, which is MEASURED to be how that plugin spells
/// `Symbol::range.file`. `is_test` is `false` everywhere, which is MEASURED to
/// be what it emits even for the mock — see `direction.rs`.
fn repo_index() -> FakeIndex {
    let served = Sym::impl_block("Svc", "impl Widgets for Svc", "src/service.rs", 100).build();
    let mock = Sym::impl_block("MockDb", "impl WidgetDb for MockDb", "tests/mock.rs", 200).build();

    FakeIndex::new(vec![
        Sym::method("create_widget", "src/service.rs", 110)
            .inside(&served)
            .build(),
        Sym::method("list_widgets", "src/service.rs", 120)
            .inside(&served)
            .build(),
        Sym::method("create_widget", "tests/mock.rs", 210)
            .inside(&mock)
            .build(),
        served,
        mock,
    ])
}

fn roots_of(name: &str, index: &FakeIndex) -> (ProtoTonicPlugin, Vec<Root>) {
    let plugin = ProtoTonicPlugin::new();
    let roots = plugin
        .roots(&fixture(name), index)
        .unwrap_or_else(|error| panic!("{name} produced no roots: {error}"));
    (plugin, roots)
}

fn find<'a>(roots: &'a [Root], service: &str, operation: &str) -> &'a Root {
    roots
        .iter()
        .find(|root| root.service == service && root.operation == operation)
        .unwrap_or_else(|| {
            let seen: Vec<String> = roots
                .iter()
                .map(|root| format!("{}/{}", root.service, root.operation))
                .collect();
            panic!("no root for {service}/{operation}; saw {seen:?}")
        })
}

/// Plan-04 §8 — the emitted key, on the emitted root.
#[test]
fn fqn_is_the_join_key() {
    let (_plugin, roots) = roots_of("repo", &repo_index());
    let created = find(&roots, "Widgets", "CreateWidget");

    assert_eq!(created.join_key, "acme.api.v1.Widgets/CreateWidget");
    assert_ne!(created.join_key, "CreateWidget");
    assert!(!created.join_key.starts_with('/'));

    // `service` and `operation` stay bare: they are display fields.
    assert_eq!(created.service, "Widgets");
    assert_eq!(created.operation, "CreateWidget");
    assert_eq!(
        created.contract,
        ContractId("proto/api.proto".to_owned()),
        "the contract is the file, not the package"
    );
    assert_eq!(created.version, Some("v1".to_owned()));
}

/// The M7 regression, and the reason this crate exists.
///
/// A name-only implementation passes the first assertion and fails the second.
#[test]
fn bare_name_join_would_produce_phantom_root() {
    let index = repo_index();
    let (_plugin, roots) = roots_of("repo", &index);

    let served = find(&roots, "Widgets", "CreateWidget");
    assert_eq!(served.direction, Direction::Served);
    let RootBinding::Bound(node) = &served.binding else {
        panic!("the served operation binds to its handler: {served:?}");
    };
    use reachgraph_plugin_api::SymbolIndex;
    let handler = index.get(node).expect("the bound node is a symbol");
    assert_eq!(handler.range.file, Path::new("src/service.rs"));

    let consumed = find(&roots, "WidgetDb", "CreateWidget");
    assert_eq!(
        consumed.direction,
        Direction::Consumed,
        "the same RPC name in another contract is not served here"
    );
    assert!(
        matches!(consumed.binding, RootBinding::Unbound { .. }),
        "binding it would attribute the handler to a contract this repository consumes: {consumed:?}"
    );
}

/// The consumed half is asserted as a **pass** — plan-04 §9 row 1.
#[test]
fn consumed_rpc_is_unbound_with_reason() {
    let (_plugin, roots) = roots_of("repo", &repo_index());

    for operation in ["CreateWidget", "GetWidget"] {
        let root = find(&roots, "WidgetDb", operation);
        assert_eq!(root.direction, Direction::Consumed);
        let RootBinding::Unbound { reason } = &root.binding else {
            panic!("a consumed RPC has no handler in this repository: {root:?}");
        };
        assert!(!reason.is_empty());
        assert!(
            reason.contains(&root.join_key),
            "the reason names the key that resolves elsewhere: {reason}"
        );
        assert!(
            !reason.contains("cargo build"),
            "MEASURED by PR D: building does not make the generated leaf visible: {reason}"
        );
    }
}

/// A mock is not a handler, and the whole root set says so.
#[test]
fn no_root_binds_to_the_test_double() {
    let index = repo_index();
    let (_plugin, roots) = roots_of("repo", &index);

    use reachgraph_plugin_api::SymbolIndex;
    for root in &roots {
        if let RootBinding::Bound(node) = &root.binding {
            let symbol = index.get(node).expect("bound to a real symbol");
            assert_ne!(
                symbol.range.file,
                Path::new("tests/mock.rs"),
                "{}/{} bound to a test double",
                root.service,
                root.operation
            );
        }
    }
}

/// 2/2 for the served service, mirroring MEASURED 6/6.
#[test]
fn join_covers_every_served_rpc() {
    let (_plugin, roots) = roots_of("repo", &repo_index());
    let bound = roots
        .iter()
        .filter(|root| root.service == "Widgets")
        .filter(|root| matches!(root.binding, RootBinding::Bound(_)))
        .count();
    assert_eq!(bound, 2);
}

/// ADR-0007 at the level of an emitted root.
#[test]
fn a_version_less_contract_emits_a_root_with_no_version() {
    let (_plugin, roots) = roots_of("repo", &repo_index());
    let ping = find(&roots, "Old", "Ping");
    assert_eq!(ping.version, None);
    assert_ne!(ping.version, Some("v1".to_owned()));
    assert_eq!(ping.join_key, "acme.legacy.Old/Ping");
}

/// Two versions of one service are two roots, never one merged root.
#[test]
fn v1_and_v2_are_separate_roots() {
    let (_plugin, roots) = roots_of("versioned", &FakeIndex::new(Vec::new()));

    let versions: Vec<Option<String>> = roots
        .iter()
        .filter(|root| root.service == "Widgets" && root.operation == "CreateWidget")
        .map(|root| root.version.clone())
        .collect();
    assert_eq!(versions.len(), 2, "{roots:#?}");
    assert!(versions.contains(&Some("v1".to_owned())));
    assert!(versions.contains(&Some("v2".to_owned())));

    let keys: Vec<&str> = roots.iter().map(|root| root.join_key.as_str()).collect();
    assert!(keys.contains(&"acme.api.v1.Widgets/CreateWidget"));
    assert!(keys.contains(&"acme.api.v2.Widgets/CreateWidget"));
}

/// A package-less contract keys bare, all the way through.
#[test]
fn a_package_less_contract_keys_without_a_prefix() {
    let (_plugin, roots) = roots_of("nopackage", &FakeIndex::new(Vec::new()));
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].join_key, "Svc/Rpc");
    assert_eq!(roots[0].version, None);
}

/// A service with no impl and no client reference is consumed, and the reason
/// says which of the two consumed cases it is — plan-04 §9 row 6.
#[test]
fn a_service_with_no_evidence_says_so() {
    let (_plugin, roots) = roots_of("versioned", &FakeIndex::new(Vec::new()));
    let root = &roots[0];
    assert_eq!(root.direction, Direction::Consumed);
    let RootBinding::Unbound { reason } = &root.binding else {
        panic!("nothing in the fixture implements it: {root:?}");
    };
    assert!(
        reason.contains("no first-party impl and no client reference"),
        "{reason}"
    );
}

/// Plan-04 §10 — coverage answers "what did you look at".
#[test]
fn coverage_lists_every_contract_including_serviceless() {
    let (plugin, _roots) = roots_of("repo", &repo_index());
    let coverage = plugin.coverage();

    let contracts: Vec<&str> = coverage
        .contracts
        .iter()
        .map(|contract| contract.0.as_str())
        .collect();
    assert_eq!(
        contracts,
        vec![
            "proto/api.proto",
            "proto/legacy.proto",
            "proto/store.proto",
            "proto/types.proto",
        ],
        "types.proto declares no service and is still listed"
    );
}

#[test]
fn coverage_records_version_per_contract() {
    let (plugin, _roots) = roots_of("repo", &repo_index());
    let coverage = plugin.coverage();

    assert_eq!(
        coverage.versions,
        vec![
            VersionKey {
                contract: ContractId("proto/api.proto".to_owned()),
                version: Some("v1".to_owned()),
            },
            VersionKey {
                contract: ContractId("proto/legacy.proto".to_owned()),
                version: None,
            },
            VersionKey {
                contract: ContractId("proto/store.proto".to_owned()),
                version: Some("v2".to_owned()),
            },
            VersionKey {
                contract: ContractId("proto/types.proto".to_owned()),
                version: Some("v1".to_owned()),
            },
        ]
    );
}

/// Before a run there is nothing to report, and an empty coverage is the honest
/// answer rather than a claim about a repository nobody has looked at.
#[test]
fn coverage_is_empty_before_a_run() {
    let plugin = ProtoTonicPlugin::new();
    let coverage = plugin.coverage();
    assert!(coverage.contracts.is_empty());
    assert!(coverage.versions.is_empty());
}

/// Plan-04 §10 — loud over lenient.
#[test]
fn unparseable_proto_is_an_error_not_silent_coverage_loss() {
    let plugin = ProtoTonicPlugin::new();
    let index = FakeIndex::new(Vec::new());
    let error = plugin
        .roots(&fixture("broken"), &index)
        .expect_err("a `.proto` that does not parse fails the run");

    let rendered = error.to_string();
    assert!(rendered.contains("broken.proto"), "{rendered}");

    assert!(
        plugin.coverage().contracts.is_empty(),
        "a failed run does not leave behind a coverage claim it did not earn"
    );
}

/// The contract surface itself.
#[test]
fn the_plugin_declares_roots_and_nothing_else() {
    use reachgraph_plugin_api::{Capability, PositionEncoding};

    let plugin = ProtoTonicPlugin::new();
    assert_eq!(plugin.provides(), &[Capability::Roots]);
    assert_eq!(plugin.position_encoding(), PositionEncoding::Utf8Bytes);
    assert_eq!(plugin.detection().marker_files, &["Cargo.toml"]);
    assert_eq!(plugin.detection().extensions, &[".proto"]);
}

/// Preflight reports what this plugin can and cannot contribute.
#[test]
fn preflight_warns_when_a_repository_has_no_contracts() {
    let plugin = ProtoTonicPlugin::new();
    assert!(matches!(plugin.preflight(&fixture("repo")), Preflight::Ok));

    let empty = std::env::temp_dir().join(format!("reachgraph-roots-empty-{}", std::process::id()));
    std::fs::create_dir_all(&empty).expect("mkdir");
    let Preflight::Warned { reason, .. } = plugin.preflight(&empty) else {
        panic!("a repository with no `.proto` file contributes no roots, and says so");
    };
    assert!(reason.contains("no `.proto`"), "{reason}");
}

/// The corroborating client reference must be in **non-test** source.
///
/// MEASURED by mutation: without the walk-level exclusion this fixture reports
/// the wrong consumed case — a test double's client construction would read as
/// production consumption. The direction is `Consumed` either way, which is
/// exactly why the reason is what has to be asserted: the two consumed cases
/// say different things about the repository.
#[test]
fn a_client_named_only_in_a_test_does_not_corroborate() {
    let (_plugin, roots) = roots_of("testclient", &FakeIndex::new(Vec::new()));
    let root = find(&roots, "WidgetDb", "CreateWidget");

    assert_eq!(root.direction, Direction::Consumed);
    let RootBinding::Unbound { reason } = &root.binding else {
        panic!("nothing in the fixture implements or calls it: {root:?}");
    };
    assert!(
        reason.contains("no first-party impl and no client reference"),
        "a mention inside a `tests/` target is not first-party non-test usage: {reason}"
    );
}

/// Plan-04 §4 — the vendored contract tag is not the endpoint version.
///
/// `PROTO_VERSION` in the fixture reads `v1.11.2`, which is the version of the
/// **bundle of proto files**. `acme.api.v1` is the version of the **operation**,
/// and ADR-0007's field means the second. Conflating them would put a bundle
/// release number on a root.
#[test]
fn vendored_bundle_tag_is_not_the_endpoint_version() {
    let (plugin, roots) = roots_of("repo", &repo_index());

    for root in &roots {
        assert_ne!(root.version, Some("v1.11.2".to_owned()));
        assert!(
            !root.join_key.contains("1.11.2"),
            "the bundle tag reached a join key: {}",
            root.join_key
        );
    }
    for key in plugin.coverage().versions {
        assert_ne!(key.version, Some("v1.11.2".to_owned()));
    }
}
