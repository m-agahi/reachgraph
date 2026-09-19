//! `fx-docs` — ADR-0005, and the baseline it has to beat.
//!
//! The standard is MEASURED in design.md §5: `code_graph` retains only the
//! **last line** of a `///` block, sigil attached, mid-sentence, longest value
//! 83 characters, and 0 of 40 `Method` nodes carry any docstring at all. Every
//! assertion below is one of those failures, stated as a requirement.

use reachgraph_plugin_api::DocFormat;

use crate::support::{all_symbols, load, named};

#[test]
fn a_multi_line_doc_block_arrives_whole() {
    let (plugin, units) = load("fx-docs");
    let symbols = all_symbols(&plugin, &units);
    let documented = named(&symbols, "documented");

    let doc = documented
        .doc
        .as_ref()
        .expect("a documented function has a doc");

    assert!(
        doc.contains("First line of a multi-line block"),
        "not last-line-only: {doc:?}"
    );
    assert!(
        doc.contains("Second paragraph"),
        "the second paragraph survives: {doc:?}"
    );
    assert!(
        doc.len() > 83,
        "not truncated at code_graph's measured 83 characters: {} chars",
        doc.len()
    );
    // The sigils are stripped. Asserted per line rather than over the whole
    // string, because the fixture's own prose quotes `///` while describing
    // the baseline — a substring search would fail on the text's content
    // rather than on its formatting.
    assert!(
        doc.lines()
            .all(|line| !line.trim_start().starts_with("///")),
        "the sigils are stripped, unlike the baseline: {doc:?}"
    );
    assert!(
        doc.contains("Ünicode") && doc.contains("日本語"),
        "non-ASCII doc text survives (ADR-0008 leak 3): {doc:?}"
    );
    assert_eq!(documented.doc_format, DocFormat::Markdown);
}

/// MEASURED, design.md §5: 0 of 40 `Method` nodes in the baseline carry any
/// docstring. One is enough to beat that, and it is asserted rather than hoped.
#[test]
fn a_methods_doc_is_not_empty() {
    let (plugin, units) = load("fx-docs");
    let symbols = all_symbols(&plugin, &units);
    let method = named(&symbols, "method");

    let doc = method.doc.as_ref().expect("a method's doc is collected");
    assert!(doc.contains("A method doc"), "{doc:?}");
}

#[test]
fn an_attribute_doc_is_a_doc() {
    let (plugin, units) = load("fx-docs");
    let symbols = all_symbols(&plugin, &units);
    let attributed = named(&symbols, "attribute_documented");

    let doc = attributed
        .doc
        .as_ref()
        .expect("#[doc = \"…\"] is the third spelling and is read");
    assert!(doc.contains("An attribute doc"), "{doc:?}");
}

/// An absent doc is `None`, never `Some("")`.
///
/// ADR-0003's honest-absence rule: an empty string would be a value claiming
/// the engine found something.
#[test]
fn an_undocumented_item_has_no_doc_rather_than_an_empty_one() {
    let (plugin, units) = load("fx-docs");
    let symbols = all_symbols(&plugin, &units);
    let undocumented = named(&symbols, "undocumented");

    assert_eq!(undocumented.doc, None);
}

/// The `//!` module doc reaches the module symbol.
#[test]
fn a_module_doc_reaches_the_module_symbol() {
    let (plugin, units) = load("fx-docs");
    let symbols = all_symbols(&plugin, &units);

    let documented_module = symbols
        .iter()
        .find(|symbol| {
            symbol.kind == reachgraph_plugin_api::SymbolKind::Module
                && symbol
                    .doc
                    .as_ref()
                    .is_some_and(|doc| doc.contains("A module doc"))
        })
        .map(|symbol| symbol.name.clone());

    assert!(
        documented_module.is_some(),
        "the crate root's //! doc is collected; modules were {:#?}",
        symbols
            .iter()
            .filter(|s| s.kind == reachgraph_plugin_api::SymbolKind::Module)
            .map(|s| (&s.name, &s.doc))
            .collect::<Vec<_>>()
    );
}
