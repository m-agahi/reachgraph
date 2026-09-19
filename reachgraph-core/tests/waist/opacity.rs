//! `NodeId` opacity — ADR-0003 field 3, plan-01 §4.1.
//!
//! The core hashes, compares, clones and emits an identity. It never parses
//! one, splits one, prefix-matches one or orders one by content. The bijective
//! rename below is how that is observed rather than stated: rename every `raw`
//! by any bijection and the built graph must be isomorphic, with identical
//! reachable sets, identical categories and identical shard **contents**. Only
//! the shard file names must not move, because they come from root identity.

use std::collections::BTreeSet;

use reachgraph_core::Index;
use reachgraph_fixture::format::{FixtureDoc, FixtureEdgeTarget, FixtureRaw, FixtureRootBinding};

use crate::doubles::{doc_of, plugin_from};
use crate::support::{build, case, emit, inputs_of, shards};

/// A bijection over the strings a case uses as identities.
struct Bijection {
    name: &'static str,
    apply: fn(&str) -> String,
}

fn reversed(raw: &str) -> String {
    raw.chars().rev().collect()
}

fn prefixed(raw: &str) -> String {
    format!("\u{1f300}::{raw}")
}

fn hex_encoded(raw: &str) -> String {
    raw.bytes().map(|byte| format!("{byte:02x}")).collect()
}

/// Rot13 over ASCII letters, which is its own inverse and therefore a
/// bijection on every string this corpus contains.
fn rot13(raw: &str) -> String {
    raw.chars()
        .map(|character| match character {
            'a'..='z' => (((character as u8 - b'a' + 13) % 26) + b'a') as char,
            'A'..='Z' => (((character as u8 - b'A' + 13) % 26) + b'A') as char,
            other => other,
        })
        .collect()
}

const BIJECTIONS: [Bijection; 4] = [
    Bijection {
        name: "reversed",
        apply: reversed,
    },
    Bijection {
        name: "prefixed",
        apply: prefixed,
    },
    Bijection {
        name: "hex_encoded",
        apply: hex_encoded,
    },
    Bijection {
        name: "rot13",
        apply: rot13,
    },
];

fn rename(doc: &mut FixtureDoc, apply: fn(&str) -> String) {
    let renamed = |raw: &FixtureRaw| FixtureRaw(apply(&raw.0));

    for symbols in doc.symbols.values_mut() {
        for symbol in symbols.iter_mut() {
            symbol.raw = renamed(&symbol.raw);
            symbol.container = symbol.container.as_ref().map(renamed);
        }
    }

    for edges in doc.edges.values_mut() {
        for edge in edges.iter_mut() {
            edge.from = renamed(&edge.from);
            edge.to = match &edge.to {
                FixtureEdgeTarget::Resolved(raw) => FixtureEdgeTarget::Resolved(renamed(raw)),
                FixtureEdgeTarget::Unresolved(target) => {
                    let mut target = target.clone();
                    target.candidates = target.candidates.iter().map(renamed).collect();
                    FixtureEdgeTarget::Unresolved(target)
                }
            };
        }
    }

    for root in doc.roots.iter_mut() {
        if let FixtureRootBinding::Bound(raw) = &root.binding {
            root.binding = FixtureRootBinding::Bound(renamed(raw));
        }
    }
}

/// Everything about a built index that must not depend on how identities are
/// spelled, with every identity mapped back through the bijection.
#[derive(PartialEq, Eq, Debug)]
struct Shape {
    nodes: Vec<String>,
    edges: Vec<(String, String)>,
    categories: Vec<(String, String)>,
    unreachable: BTreeSet<String>,
    shard_files: Vec<String>,
    shard_contents: Vec<BTreeSet<String>>,
}

fn shape(index: &Index, back: fn(&str) -> String) -> Shape {
    let sink = emit(index);

    Shape {
        nodes: index
            .view()
            .nodes
            .iter()
            .map(|node| back(&node.id.raw))
            .collect(),
        edges: index
            .view()
            .edges
            .iter()
            .map(|edge| {
                let target = match &edge.to {
                    reachgraph_plugin_api::EdgeTarget::Resolved(node) => back(&node.raw),
                    reachgraph_plugin_api::EdgeTarget::Unresolved { name, .. } => {
                        format!("unresolved:{name}")
                    }
                };
                (back(&edge.from.raw), target)
            })
            .collect(),
        categories: index
            .view()
            .nodes
            .iter()
            .map(|node| (back(&node.id.raw), format!("{:?}", node.category)))
            .collect(),
        unreachable: index
            .unreachable()
            .iter()
            .map(|node| back(&node.id.raw))
            .collect(),
        shard_files: shards(&sink).into_iter().map(|(path, _)| path).collect(),
        shard_contents: shards(&sink)
            .into_iter()
            .map(|(_, shard)| shard.nodes.iter().map(|node| back(&node.id.raw)).collect())
            .collect(),
    }
}

/// Plan-01 §10.2. The property over every case in the corpus that builds, for
/// four different bijections.
#[test]
fn node_id_opacity_bijective_rename() {
    const CASES: [&str; 8] = [
        "minimal",
        "versioned_pair",
        "unresolved_edge",
        "nested_containers",
        "cycle_and_diamond",
        "deep_chain",
        "external_target",
        "foreign_shapes",
    ];

    /// The identity, for the un-renamed build.
    fn same(raw: &str) -> String {
        raw.to_owned()
    }

    for name in CASES {
        let original = shape(&build(&case(name)), same);

        for bijection in &BIJECTIONS {
            let mut doc = doc_of(name);
            rename(&mut doc, bijection.apply);
            let renamed = shape(&build(&plugin_from(name, doc)), |raw| raw.to_owned());

            // Map the renamed shape back by applying the same bijection to the
            // original, which is the direction that needs no inverse.
            let mut expected = original.clone_with(bijection.apply);
            expected.shard_files.clone_from(&original.shard_files);

            assert_eq!(
                renamed, expected,
                "{name} is not isomorphic under the {} rename",
                bijection.name
            );
        }
    }
}

impl Shape {
    fn clone_with(&self, apply: fn(&str) -> String) -> Shape {
        Shape {
            nodes: self.nodes.iter().map(|raw| apply(raw)).collect(),
            edges: self
                .edges
                .iter()
                .map(|(from, to)| {
                    (
                        apply(from),
                        if to.starts_with("unresolved:") {
                            to.clone()
                        } else {
                            apply(to)
                        },
                    )
                })
                .collect(),
            categories: self
                .categories
                .iter()
                .map(|(raw, category)| (apply(raw), category.clone()))
                .collect(),
            unreachable: self.unreachable.iter().map(|raw| apply(raw)).collect(),
            shard_files: self.shard_files.clone(),
            shard_contents: self
                .shard_contents
                .iter()
                .map(|contents| contents.iter().map(|raw| apply(raw)).collect())
                .collect(),
        }
    }
}

/// Plan-01 §10.2. A `raw` that looks like a path, a scope chain, a fragment or
/// a whole JSON document is still bytes.
#[test]
fn node_id_with_structured_looking_raw_is_not_parsed() {
    const STRUCTURED: [&str; 4] = [
        "acme::inner::handler",
        "path/to/thing.rs#42",
        "generic{T}::call",
        r#"{"plugin":"fixture","raw":"fn:not_a_node"}"#,
    ];

    let index = build(&case("structured_raw_ids"));
    let sink = emit(&index);
    let (_, shard) = shards(&sink).remove(0);

    let emitted: Vec<&str> = shard
        .nodes
        .iter()
        .map(|node| node.id.raw.as_str())
        .collect();
    assert_eq!(
        emitted, STRUCTURED,
        "every identity round-trips byte for byte"
    );

    // The JSON-shaped identity did not become a node of its own: the document
    // it spells names `fn:not_a_node`, which is not in the graph.
    assert!(index
        .view()
        .nodes
        .iter()
        .all(|node| node.id.raw != "fn:not_a_node"));
}

/// Plan-01 §4.1. The plugin is half the key, so two plugins may use one `raw`.
#[test]
fn same_raw_different_plugin_does_not_collide() {
    let first = case("two_plugins_a");
    let second = case("two_plugins_b");

    let index = Index::build(
        first.case_dir(),
        &inputs_of(&[&first, &second]),
        &reachgraph_core::BuildOptions::default(),
    )
    .expect("a two-plugin build");

    let shared: Vec<&str> = index
        .view()
        .nodes
        .iter()
        .filter(|node| node.id.raw == "fn:shared/same_raw")
        .map(|node| node.id.plugin.0)
        .collect();

    assert_eq!(shared, ["fixture_a", "fixture_b"], "two nodes, not one");
    assert_eq!(index.shards().len(), 2);
    for shard in index.shards() {
        assert_eq!(
            shard.view.nodes.len(),
            2,
            "neither shard absorbed the other"
        );
    }
}
