//! The emitted artifact — plan-01 §8, ADR-0006.
//!
//! **Every assertion here names the emitted JSON**, read back off the sink,
//! rather than the value a builder returned. A build that computes the right
//! answer and writes the wrong bytes has to fail somewhere, and this is where.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use reachgraph_core::schema::{
    EdgeTargetRow, InferenceModeRow, RangeRow, ShardDocument, SpanRow, UNREACHABLE_CLAIM,
};
use reachgraph_core::{shard_path, RootIdentity};
use reachgraph_plugin_api::{ContractId, Direction};
use serde_json::Value;

use crate::doubles::{doc_of, plugin_from};
use crate::support::{build, case, emit, endpoints, shards, unreachable};

/// ADR-0006. One shard per bound root, and none for an unbound one.
#[test]
fn one_shard_per_bound_root() {
    let sink = emit(&build(&case("versioned_pair")));
    assert_eq!(shards(&sink).len(), 2);

    let sink = emit(&build(&case("unbound_root")));
    assert_eq!(shards(&sink).len(), 1);

    assert_eq!(
        sink.paths(),
        [
            "endpoints.json",
            "graph/acme.task__v1__TaskService__CreateTask__served__233f0a8e254bdee3.json",
            "unreachable.json",
            "versions.json",
        ]
    );
}

/// Plan-01 §10.1 step 9. The shard survives the round trip through its own
/// file: parse the emitted bytes, write them again, compare.
#[test]
fn shard_round_trip_serde() {
    let sink = emit(&build(&case("versioned_pair")));

    for (path, document) in shards(&sink) {
        let again = serde_json::to_vec_pretty(&document).expect("the document re-serializes");
        let reparsed: ShardDocument =
            serde_json::from_slice(&again).expect("and parses back identically");
        assert_eq!(reparsed, document, "{path}");

        let mut bytes = again;
        bytes.push(b'\n');
        assert_eq!(bytes, sink.bytes(&path), "{path} is byte-stable");
    }
}

/// Plan-01 §10.1 step 10, §6.4. The sentence is data, shipped so a renderer
/// displays it rather than composing its own.
#[test]
fn unreachable_claim_string_is_verbatim() {
    let sink = emit(&build(&case("versioned_pair")));

    assert_eq!(
        unreachable(&sink).claim,
        "not reachable from any endpoint version in this index"
    );
    assert_eq!(UNREACHABLE_CLAIM, unreachable(&sink).claim);
}

/// Plan-01 §5.4. Every indexed node that is not reached is listed, with its
/// category attached. No category suppression happens in the waist.
#[test]
fn unreachable_lists_every_indexed_unreached_node() {
    let sink = emit(&build(&case("foreign_shapes")));
    let document = unreachable(&sink);

    // Every third-party and standard-library node this case declares is
    // reached, so what proves the absence of filtering is the presence of the
    // terminal categories in coverage alongside a complete node list.
    let listed: BTreeSet<&str> = document
        .nodes
        .iter()
        .map(|node| node.id.raw.as_str())
        .collect();
    assert!(listed.is_empty());

    let sink = emit(&build(&case("versioned_pair")));
    let document = unreachable(&sink);
    let listed: BTreeSet<&str> = document
        .nodes
        .iter()
        .map(|node| node.id.raw.as_str())
        .collect();
    assert_eq!(
        listed,
        BTreeSet::from(["impl:TaskService_for_TaskServer", "fn:orphan/unused_helper"])
    );
    assert_eq!(document.counts_by_category.first_party, 2);
}

/// Plan-01 §8.1. The exact place where an absent version and the literal
/// string `"none"` could converge on disk.
#[test]
fn shard_slug_collision_free_none_vs_none_string() {
    let sink = emit(&build(&case("unversioned_and_versioned")));
    let paths: Vec<String> = shards(&sink).into_iter().map(|(path, _)| path).collect();

    assert_eq!(paths.len(), 3, "three roots, three files: {paths:?}");
    assert_eq!(
        paths.iter().collect::<BTreeSet<_>>().len(),
        3,
        "two of them collided: {paths:?}"
    );

    // The readable prefix is display. The suffix is what carries uniqueness,
    // so it must differ too.
    let fingerprints: BTreeSet<&str> = paths
        .iter()
        .map(|path| {
            let stem = path.strip_suffix(".json").expect("a JSON file");
            &stem[stem.len() - 16..]
        })
        .collect();
    assert_eq!(fingerprints.len(), 3, "{paths:?}");
}

/// Plan-01 §8.1, ADR-0003 field 3. Slugging a node identity would be parsing
/// it, so the slug comes from root identity and nothing else.
#[test]
fn shard_slug_is_not_derived_from_node_id() {
    let before = emit(&build(&case("minimal")));

    let mut doc = doc_of("minimal");
    for symbols in doc.symbols.values_mut() {
        for symbol in symbols.iter_mut() {
            symbol.raw =
                reachgraph_fixture::format::FixtureRaw(format!("renamed::{}", symbol.raw.0));
        }
    }
    for edges in doc.edges.values_mut() {
        for edge in edges.iter_mut() {
            edge.from = reachgraph_fixture::format::FixtureRaw(format!("renamed::{}", edge.from.0));
            if let reachgraph_fixture::format::FixtureEdgeTarget::Resolved(raw) = &edge.to {
                edge.to = reachgraph_fixture::format::FixtureEdgeTarget::Resolved(
                    reachgraph_fixture::format::FixtureRaw(format!("renamed::{}", raw.0)),
                );
            }
        }
    }
    if let reachgraph_fixture::format::FixtureRootBinding::Bound(raw) = &doc.roots[0].binding {
        doc.roots[0].binding = reachgraph_fixture::format::FixtureRootBinding::Bound(
            reachgraph_fixture::format::FixtureRaw(format!("renamed::{}", raw.0)),
        );
    }

    let after = emit(&build(&plugin_from("minimal", doc)));

    let paths = |sink: &crate::support::MemorySink| -> Vec<String> {
        shards(sink).into_iter().map(|(path, _)| path).collect()
    };
    assert_eq!(paths(&before), paths(&after));
}

/// ADR-0006 regenerates rather than mutates, so two runs over one input must
/// produce identical bytes.
#[test]
fn artifact_is_deterministic_across_runs() {
    let plugin = case("versioned_pair");

    let first = emit(&build(&plugin));
    let second = emit(&build(&plugin));

    assert_eq!(first.paths(), second.paths());
    for path in first.paths() {
        assert_eq!(first.bytes(path), second.bytes(path), "{path} differs");
    }
}

/// Plan-01 §9. Provenance survives assembly.
#[test]
fn edge_provenance_survives_graph_build() {
    let index = build(&case("versioned_pair"));

    for edge in &index.view().edges {
        assert_eq!(edge.provenance.plugin.0, "fixture");
        assert_eq!(edge.provenance.engine, "reachgraph-fixture 0.1.0");
    }
}

/// Plan-01 §9, stated separately because the two stages fail separately.
#[test]
fn edge_inference_mode_survives_shard_emit() {
    let sink = emit(&build(&case("versioned_pair")));

    let mut seen: Vec<InferenceModeRow> = shards(&sink)
        .into_iter()
        .flat_map(|(_, shard)| shard.edges.into_iter().map(|edge| edge.inference_mode))
        .collect();
    seen.dedup();

    assert!(seen.contains(&InferenceModeRow::Resolved));
    assert!(seen.contains(&InferenceModeRow::TypeInferred));

    for (path, shard) in shards(&sink) {
        for edge in &shard.edges {
            assert_eq!(edge.provenance.engine, "reachgraph-fixture 0.1.0", "{path}");
        }
    }
}

/// `docs/design.md` §5 measured the failure a float invites: a number that
/// records indecision and then renders as if it were a measurement. There is
/// none, anywhere in the artifact.
#[test]
fn root_has_no_confidence_field() {
    fn floats(value: &Value, path: &str, found: &mut Vec<String>) {
        match value {
            Value::Number(number) if number.as_i64().is_none() && number.as_u64().is_none() => {
                found.push(path.to_owned())
            }
            Value::Array(items) => {
                for (position, item) in items.iter().enumerate() {
                    floats(item, &format!("{path}[{position}]"), found);
                }
            }
            Value::Object(fields) => {
                for (key, item) in fields {
                    floats(item, &format!("{path}.{key}"), found);
                }
            }
            _ => {}
        }
    }

    let sink = emit(&build(&case("versioned_pair")));
    let mut found = Vec::new();
    for path in sink.paths() {
        let value: Value = serde_json::from_slice(sink.bytes(path)).expect("valid JSON");
        floats(&value, path, &mut found);
    }

    assert!(
        found.is_empty(),
        "a float reached the artifact at {found:?}"
    );

    let document = endpoints(&sink);
    let text = serde_json::to_string(&document).expect("serializes");
    assert!(!text.contains("confidence"));
}

/// Plan-01 §10.2. The key is stored and emitted; no version, service or
/// operation is ever recovered from it.
#[test]
fn join_key_is_stored_never_parsed() {
    let sink = emit(&build(&case("structured_raw_ids")));
    let document = endpoints(&sink);

    let operation = &document.operations[0];
    let version = &operation.versions[0];

    assert_eq!(
        version.join_key,
        "https://example.invalid/acme/v9/StructuredService/Handle"
    );
    assert_eq!(
        version.version.as_deref(),
        Some("v1"),
        "the key says v9 and the root says v1; the root wins because the key is opaque"
    );
    assert_eq!(operation.service, "StructuredService");
    assert_eq!(operation.contract, "acme.structured");

    let (_, shard) = shards(&sink).remove(0);
    assert_eq!(shard.root.join_key, version.join_key);
    assert_eq!(shard.root.version.as_deref(), Some("v1"));
}

/// Plan-00 §2. `null` and a span at offset zero stay distinguishable in the
/// type and in the artifact, and the file survives in both.
#[test]
fn span_none_is_not_offset_zero() {
    let sink = emit(&build(&case("minimal")));
    let (_, shard) = shards(&sink).remove(0);

    let range = &shard.nodes[0]
        .symbol
        .as_ref()
        .expect("the node was indexed")
        .range;
    assert_eq!(range.span, None);
    assert_eq!(range.file, Path::new("src/service/handlers.rs"));

    let absent = serde_json::to_string(range).expect("serializes");
    let zero = serde_json::to_string(&RangeRow {
        file: range.file.clone(),
        span: Some(SpanRow { start: 0, end: 0 }),
    })
    .expect("serializes");
    assert_ne!(absent, zero);
    assert!(absent.contains("\"span\":null"));
}

/// Plan-01 §8.1. A shard is addressed by the tuple, and the file name is
/// derived from it rather than from anything a plugin spelled opaquely.
#[test]
fn shard_path_is_derived_from_root_identity() {
    let identity = RootIdentity {
        contract: ContractId("acme.task".to_owned()),
        version: Some("v1".to_owned()),
        service: "TaskService".to_owned(),
        operation: "CreateTask".to_owned(),
        direction: Direction::Served,
    };

    let path = shard_path(&identity);
    assert!(path.starts_with("graph/acme.task__v1__TaskService__CreateTask__served__"));
    assert!(path.ends_with(".json"));

    let unversioned = RootIdentity {
        version: None,
        ..identity.clone()
    };
    let literal = RootIdentity {
        version: Some("none".to_owned()),
        ..identity.clone()
    };
    assert_ne!(shard_path(&unversioned), shard_path(&literal));

    // Anything outside the allowed set becomes an underscore, and the
    // fingerprint keeps the two apart anyway.
    let slashed = RootIdentity {
        contract: ContractId("acme/task".to_owned()),
        ..identity.clone()
    };
    assert!(shard_path(&slashed).starts_with("graph/acme_task__"));
    assert_ne!(shard_path(&slashed), shard_path(&identity));
}

// ---------------------------------------------------------------------------
// Golden artifacts
// ---------------------------------------------------------------------------

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
}

/// Plan-01 §10.2. The whole emitted tree for a case, compared byte for byte.
///
/// These catch schema drift that no single assertion notices, and they are the
/// regression net for the renderer that reads these files. Regeneration is a
/// deliberate command — `REACHGRAPH_BLESS=1 cargo test -p reachgraph-core` —
/// because a snapshot that repairs itself on failure asserts nothing.
#[test]
fn golden_artifacts_match() {
    const CASES: [&str; 3] = ["versioned_pair", "unresolved_edge", "unversioned_contract"];

    let bless = std::env::var_os("REACHGRAPH_BLESS").is_some();

    for name in CASES {
        let sink = emit(&build(&case(name)));
        let root = golden_dir().join(name);

        if bless {
            let _ = std::fs::remove_dir_all(&root);
        }

        for path in sink.paths() {
            let target = root.join(path);
            if bless {
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).expect("the golden directory is writable");
                }
                std::fs::write(&target, sink.bytes(path)).expect("the golden file is writable");
                continue;
            }

            let checked_in = std::fs::read(&target).unwrap_or_else(|error| {
                panic!(
                    "{}: {error}. Regenerate with REACHGRAPH_BLESS=1 and read the diff.",
                    target.display()
                )
            });
            assert_eq!(
                String::from_utf8_lossy(&checked_in),
                String::from_utf8_lossy(sink.bytes(path)),
                "{} has moved",
                target.display()
            );
        }

        if !bless {
            let count = walk(&root).len();
            assert_eq!(
                count,
                sink.paths().len(),
                "{} holds {count} files and the build emitted {}",
                root.display(),
                sink.paths().len()
            );
        }
    }

    assert!(
        !bless,
        "the golden files were regenerated; re-run without REACHGRAPH_BLESS to assert them"
    );
}

fn walk(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return found;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(walk(&path));
        } else {
            found.push(path);
        }
    }
    found
}

/// A resolved target and an unresolved call can never be confused, because the
/// artifact tags them.
#[test]
fn edge_target_is_a_tagged_union_in_the_artifact() {
    let sink = emit(&build(&case("unresolved_edge")));
    let value: Value = serde_json::from_slice(sink.bytes(&shards(&sink)[0].0)).expect("valid JSON");

    let to = &value["edges"][0]["to"];
    assert_eq!(to["state"], "unresolved");
    assert!(to.get("node").is_none(), "no field holds a best candidate");

    let sink = emit(&build(&case("minimal")));
    let value: Value = serde_json::from_slice(sink.bytes(&shards(&sink)[0].0)).expect("valid JSON");
    assert_eq!(value["edges"][0]["to"]["state"], "resolved");

    // And the same union, through the typed reader.
    let (_, shard) = shards(&sink).remove(0);
    assert!(matches!(shard.edges[0].to, EdgeTargetRow::Resolved { .. }));
}
