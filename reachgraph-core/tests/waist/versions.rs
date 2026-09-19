//! Per-version reachability — plan-01 §6, ADR-0007.

use std::collections::BTreeSet;

use reachgraph_core::schema::VersionKeyRow;
use reachgraph_plugin_api::{ContractId, VersionKey};

use crate::support::{build, case, emit, endpoints, shards, unreachable, versions};

/// Plan-01 §10.1 step 8. `v1` and `v2` route to different code, so they are two
/// roots, two shards and two reachable sets.
#[test]
fn v1_and_v2_are_separate_roots() {
    let index = build(&case("versioned_pair"));
    assert_eq!(index.shards().len(), 2);

    let reach = |version: &str| -> BTreeSet<String> {
        index
            .shards()
            .iter()
            .find(|shard| shard.root.version.as_deref() == Some(version))
            .expect("both versions bound")
            .view
            .nodes
            .iter()
            .map(|node| node.id.raw.clone())
            .collect()
    };

    let v1 = reach("v1");
    let v2 = reach("v2");

    assert!(v1.contains("fn:v1only/legacy_audit") && !v2.contains("fn:v1only/legacy_audit"));
    assert!(v2.contains("fn:v2only/validate") && !v1.contains("fn:v2only/validate"));
    assert!(v1.contains("fn:shared/persist") && v2.contains("fn:shared/persist"));
}

/// ADR-0007's three-way table, which is the two-key instance of the bitset.
#[test]
fn v1_only_v2_only_both_classification() {
    let sink = emit(&build(&case("versioned_pair")));
    let document = versions(&sink);

    let class_of = |raw: &str| -> Option<String> {
        document
            .nodes
            .iter()
            .find(|node| node.id.raw == raw)
            .unwrap_or_else(|| panic!("{raw} is in versions.json"))
            .class
            .clone()
    };

    assert_eq!(
        class_of("fn:v1only/legacy_audit").as_deref(),
        Some("v1_only")
    );
    assert_eq!(class_of("fn:v2only/validate").as_deref(), Some("v2_only"));
    assert_eq!(class_of("fn:shared/persist").as_deref(), Some("both"));

    let summary = document
        .summary_by_contract
        .get("acme.task")
        .expect("the contract has exactly two version keys");
    assert_eq!(summary.v1_only, 2, "the handler and its exclusive callee");
    assert_eq!(summary.v2_only, 2);
    assert_eq!(summary.both, 1);
}

/// Plan-01 §6.2. With three or more keys `reached_by` is the truth, and a
/// two-valued label would be a lie.
#[test]
fn three_versions_emit_reached_by_not_class() {
    let sink = emit(&build(&case("three_versions")));
    let document = versions(&sink);

    assert_eq!(document.version_keys.len(), 3);
    assert!(
        document.summary_by_contract.is_empty(),
        "a three-version contract has no two-valued summary"
    );

    let shared = document
        .nodes
        .iter()
        .find(|node| node.id.raw == "fn:shared/persist")
        .expect("the shared node is listed");
    assert_eq!(shared.reached_by, [0, 1, 2]);
    assert_eq!(shared.class, None);

    let v1 = document
        .nodes
        .iter()
        .find(|node| node.id.raw == "fn:v1/create")
        .expect("the v1 handler is listed");
    assert_eq!(v1.reached_by, [0]);
    assert_eq!(v1.class, None);
}

/// Plan-00 §6.2. The case wrote `null`, and nothing invented a version.
#[test]
fn missing_version_stays_none() {
    let index = build(&case("unversioned_contract"));

    assert_eq!(index.roots().len(), 1);
    assert_eq!(index.roots()[0].version, None);
    assert_eq!(
        index.coverage().versions,
        [VersionKey {
            contract: ContractId("acme.legacy".to_owned()),
            version: None,
        }]
    );
}

/// The round trip, in the artifact. `null` stays `null`.
#[test]
fn version_key_none_is_not_serialized_as_v1() {
    let sink = emit(&build(&case("unversioned_contract")));

    let document = endpoints(&sink);
    assert_eq!(document.operations[0].versions[0].version, None);

    let text = String::from_utf8(sink.bytes("endpoints.json").to_vec()).expect("valid UTF-8");
    assert!(text.contains("\"version\": null"));
    assert!(!text.contains("\"v1\""));

    assert_eq!(
        versions(&sink).version_keys,
        [VersionKeyRow {
            contract: "acme.legacy".to_owned(),
            version: None,
        }]
    );
}

/// Plan-01 §6.1. Two roots with the same operation, one versioned and one not,
/// are separate — and so is one whose version is the literal string `"none"`.
#[test]
fn unversioned_and_versioned_roots_do_not_merge() {
    let index = build(&case("unversioned_and_versioned"));

    assert_eq!(index.shards().len(), 3);
    let keys: Vec<Option<&str>> = index
        .coverage()
        .versions
        .iter()
        .map(|key| key.version.as_deref())
        .collect();
    assert_eq!(keys, [None, Some("v1"), Some("none")]);

    let sink = emit(&index);
    let document = versions(&sink);

    // Each handler is reached by exactly one key, so nothing merged.
    for (raw, expected) in [
        ("fn:unversioned/ping", vec![0usize]),
        ("fn:v1/ping", vec![1]),
        ("fn:none/ping", vec![2]),
    ] {
        let node = document
            .nodes
            .iter()
            .find(|node| node.id.raw == raw)
            .unwrap_or_else(|| panic!("{raw} is listed"));
        assert_eq!(node.reached_by, expected, "{raw}");
    }

    let shared = document
        .nodes
        .iter()
        .find(|node| node.id.raw == "fn:shared/reply")
        .expect("the shared node is listed");
    assert_eq!(shared.reached_by, [0, 1, 2]);
}

/// Plan-00 §6.2 and ADR-0007 requirement 3: the claim carries what it was
/// computed against.
#[test]
fn unreachable_records_root_coverage() {
    let sink = emit(&build(&case("unbound_root")));
    let coverage = unreachable(&sink).coverage;

    assert_eq!(coverage.roots_total, 2);
    assert_eq!(coverage.roots_bound, 1);
    assert_eq!(coverage.contracts, ["acme.task"]);
    assert_eq!(coverage.units_indexed, ["unit:app"]);
    assert_eq!(coverage.plugins, ["fixture"]);
}

/// ADR-0007 requirement 1. A `None` version is a real entry in the list.
#[test]
fn coverage_lists_every_version_key_including_none() {
    let sink = emit(&build(&case("unversioned_and_versioned")));

    assert_eq!(
        unreachable(&sink).coverage.versions,
        [
            VersionKeyRow {
                contract: "acme.ping".to_owned(),
                version: None
            },
            VersionKeyRow {
                contract: "acme.ping".to_owned(),
                version: Some("v1".to_owned())
            },
            VersionKeyRow {
                contract: "acme.ping".to_owned(),
                version: Some("none".to_owned())
            },
        ]
    );
}

/// Plan-01 §3's serde rules. A **missing** key is a parse error, which is not
/// what writing no `serde(default)` buys: serde's derive routes a missing key
/// on a bare `Option` through a deserializer that answers `None`.
#[test]
fn missing_version_key_is_a_parse_error() {
    let present: Result<VersionKeyRow, _> =
        serde_json::from_str(r#"{"contract":"acme.task","version":null}"#);
    assert!(present.is_ok(), "an explicit null is the way to say None");

    let absent: Result<VersionKeyRow, _> = serde_json::from_str(r#"{"contract":"acme.task"}"#);
    assert!(
        absent.is_err(),
        "a document that omits the key must not deserialize to None"
    );
}

/// Two roots differing only in `join_key` are still two roots: identity is the
/// tuple, and the key is carried rather than joined on.
#[test]
fn join_key_does_not_affect_root_identity() {
    let mut doc = crate::doubles::doc_of("minimal");
    let mut twin = doc.roots[0].clone();
    twin.join_key = "a completely different spelling".to_owned();
    doc.roots.push(twin);

    let index = build(&crate::doubles::plugin_from("minimal", doc));

    assert_eq!(index.roots().len(), 2);
    assert_eq!(index.shards().len(), 2);
    assert_ne!(index.roots()[0].join_key, index.roots()[1].join_key);
    assert_eq!(
        index.coverage().versions.len(),
        1,
        "identity is the tuple, so both roots share one version key"
    );

    let sink = emit(&index);
    let document = endpoints(&sink);
    assert_eq!(document.operations.len(), 1);
    assert_eq!(document.operations[0].versions.len(), 2);

    // The shard file name is derived from identity, so two roots with one
    // identity name one file. That is the slug being a name rather than an
    // identity (plan-01 §8.1), and the full tuple is inside the file.
    assert_eq!(shards(&sink).len(), 1);
}
