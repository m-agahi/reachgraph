//! Classification through the trait object — plan-01 §7.

use reachgraph_core::schema::{CategoryRow, EncodingRow};
use reachgraph_core::{BuildInputs, BuildOptions, Index};
use reachgraph_plugin_api::Category;

use crate::doubles::SpyClassifier;
use crate::support::{build, case, emit, inputs, inputs_of, shards, unreachable};

/// Plan-00 §3.5. A core that called into a language crate directly would have
/// re-created ADR-0008's forbidden language branch in a different costume.
#[test]
fn classifier_invoked_through_trait_object() {
    let plugin = case("minimal");
    let spy = SpyClassifier::new(&plugin);

    let index = Index::build(
        plugin.case_dir(),
        &BuildInputs {
            symbols: vec![&plugin],
            edges: vec![&plugin],
            roots: vec![&plugin],
            classifiers: vec![&spy],
        },
        &BuildOptions::default(),
    )
    .expect("the case builds");

    let mut calls = spy.calls();
    calls.sort();
    assert_eq!(calls, ["src/db/insert.rs", "src/service/handlers.rs"]);

    for node in &index.view().nodes {
        assert_eq!(node.category, Some(Category::FirstParty));
    }
}

/// Plan-01 §7. **The test that would have failed under the removed unit-root
/// fallback.** Classification is at file granularity, so one unit holds paths
/// that classify differently.
#[test]
fn classification_is_per_file_within_one_unit() {
    let index = build(&case("foreign_shapes"));
    assert_eq!(index.coverage().units_indexed.len(), 1);

    let category_of = |raw: &str| {
        index
            .view()
            .nodes
            .iter()
            .find(|node| node.id.raw == raw)
            .unwrap_or_else(|| panic!("{raw} is a node"))
            .category
    };

    assert_eq!(
        category_of("go:pkg/Server.Handle"),
        Some(Category::FirstParty)
    );
    assert_eq!(
        category_of("java:com.acme.Store"),
        Some(Category::ThirdParty)
    );
    assert_eq!(category_of("py:acme.Client"), Some(Category::ThirdParty));
    assert_eq!(category_of("go:stdlib/fmt.Println"), Some(Category::Stdlib));
}

/// Plan-01 §7. `span: None` changes nothing, because classification reads the
/// file and never the offset. Every fixture symbol is spanless, so this holds
/// over the whole corpus.
#[test]
fn spanless_symbol_still_classifies() {
    let index = build(&case("minimal"));

    for node in &index.view().nodes {
        let symbol = node.symbol.as_ref().expect("every node here was indexed");
        assert_eq!(symbol.range.span, None);
        assert_eq!(node.category, Some(Category::FirstParty));
    }
}

/// Plan-01 §7.0. The one genuinely pathless case, and it terminates nothing.
#[test]
fn external_node_is_unclassified() {
    let index = build(&case("external_target"));

    let external = index
        .view()
        .nodes
        .iter()
        .find(|node| node.id.raw == "ext:unlocatable_target")
        .expect("the external node exists");
    assert_eq!(external.category, None);
    assert!(external.symbol.is_none(), "it was never indexed");
}

/// Plan-01 §5.4. A node whose plugin registered no classifier is counted,
/// never dropped without trace.
#[test]
fn unclassified_nodes_are_counted_not_dropped() {
    let sink = emit(&build(&case("no_classifier")));
    let document = unreachable(&sink);

    assert_eq!(document.counts_by_category.unclassified, 1);
    assert_eq!(document.counts_by_category.first_party, 0);
    assert_eq!(document.nodes.len(), 1);
    assert_eq!(document.nodes[0].id.raw, "fn:unreached");
    assert_eq!(document.nodes[0].category, None);
}

/// ADR-0003 field 2 and ADR-0008 leak 3. Two plugins, two encodings, each
/// matching its declaring plugin — not one global setting.
#[test]
fn position_encoding_is_per_plugin_not_global() {
    let utf8 = case("minimal");
    let utf16 = case("utf16_plugin");

    let index = Index::build(
        utf8.case_dir(),
        &inputs_of(&[&utf8, &utf16]),
        &BuildOptions::default(),
    )
    .expect("a two-plugin build");

    let sink = emit(&index);
    for (path, shard) in shards(&sink) {
        let declared: Vec<(&str, EncodingRow)> = shard
            .plugins
            .iter()
            .map(|plugin| (plugin.id.as_str(), plugin.position_encoding))
            .collect();

        assert!(
            declared.contains(&("fixture", EncodingRow::Utf8Bytes)),
            "{path}: {declared:?}"
        );
        assert!(
            declared.contains(&("fixture16", EncodingRow::Utf16CodeUnits)),
            "{path}: {declared:?}"
        );
    }
}

/// Plan-01 §7.1. `expand_categories` is what a caller reaches for when the
/// declared limitation is not the one it wants.
#[test]
fn expand_categories_overrides_which_categories_terminate() {
    let plugin = case("foreign_shapes");
    let expanded = Index::build(
        plugin.case_dir(),
        &inputs(&plugin),
        &BuildOptions {
            expand_categories: vec![Category::ThirdParty],
            ..BuildOptions::default()
        },
    )
    .expect("the case builds");

    assert_eq!(
        expanded.coverage().traversal_terminal_categories,
        [Category::Stdlib]
    );

    let sink = emit(&expanded);
    assert_eq!(
        unreachable(&sink).coverage.traversal_terminal_categories,
        [CategoryRow::Stdlib],
        "the declared limitation reaches the artifact, not a release note"
    );
}
