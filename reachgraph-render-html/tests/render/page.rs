//! Plan-05 §8.3 — the page's structure, asserted by parsing it.
//!
//! `scraper` rather than string matching, per §8.3: a string match on
//! generated HTML passes on a page that would not render, and every assertion
//! below is about where an element **is** rather than whether some bytes
//! appear.

use reachgraph_plugin_api::{Category, GraphView};
use scraper::{Html, Selector};

use crate::support;

fn select(query: &str) -> Selector {
    Selector::parse(query).expect("the selector parses")
}

fn page() -> Html {
    let view = support::index_view();
    let shards = [support::shard()];
    Html::parse_document(&support::render(&view, &shards, &support::artifact()).text("index.html"))
}

fn text_of(document: &Html, query: &str) -> String {
    document
        .select(&select(query))
        .next()
        .unwrap_or_else(|| panic!("{query} matched nothing"))
        .text()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// **Plan-05 §3 made executable.** This is the CDN prohibition as a build
/// failure: a `<script src>` with a scheme or a protocol-relative prefix is a
/// runtime download the browser performs, and it fails in an air-gapped
/// runner, it fails when the host is unreachable, and it tells a third party
/// that somebody is reading a private repository's call graph.
#[test]
fn page_has_no_external_script_src() {
    let document = page();
    let mut seen = 0;

    for element in document.select(&select("script")) {
        let Some(source) = element.value().attr("src") else {
            continue;
        };
        seen += 1;
        assert!(
            !source.contains("://") && !source.starts_with("//") && !source.starts_with('/'),
            "an external script reached the page: {source}"
        );
    }

    assert!(
        seen >= 4,
        "the guard saw {seen} script sources, so it would pass on a page with none"
    );

    // The same rule over the raw bytes, which catches a `src` this crate
    // wrote into a comment, an attribute the parser dropped, or a stylesheet
    // link nobody thought about.
    let view = support::index_view();
    let raw = support::render(&view, &[], &support::artifact()).text("index.html");
    assert!(!raw.contains("http://"), "{}", &raw[..200]);
    assert!(
        !raw.contains("https://"),
        "an absolute URL reached the page"
    );
}

/// Plan-05 §8.3: each bundle written is also referenced, in the load order the
/// vendor table fixes. A UMD bundle registers itself on `window` and the next
/// one reads it, so order is contract.
#[test]
fn page_references_every_vendored_bundle_in_load_order() {
    let view = support::index_view();
    let written = support::render(&view, &[], &support::artifact());
    let document = Html::parse_document(&written.text("index.html"));

    let referenced: Vec<String> = document
        .select(&select("script[src]"))
        .filter_map(|element| element.value().attr("src"))
        .map(str::to_owned)
        .collect();

    let expected: Vec<String> = reachgraph_render_html::vendor::BUNDLES
        .iter()
        .map(|bundle| bundle.output_path())
        .collect();

    for path in &expected {
        assert!(referenced.contains(path), "{path} is not referenced");
        assert!(written.written.contains_key(path), "{path} is not written");
    }
    assert_eq!(&referenced[..expected.len()], &expected[..]);
    assert_eq!(referenced.last().map(String::as_str), Some("loader.js"));
}

/// Plan-05 §5: a legend is **always rendered**, never behind a disclosure, and
/// it names all five strength classes. An unlabelled dash pattern communicates
/// nothing.
#[test]
fn legend_present_for_edge_strength() {
    let document = page();
    let classes: Vec<String> = document
        .select(&select("#rg-legend-edges li"))
        .filter_map(|element| element.value().attr("data-strength"))
        .map(str::to_owned)
        .collect();

    assert_eq!(
        classes,
        vec![
            "resolved",
            "type-inferred",
            "lexical",
            "enclosure",
            "unresolved"
        ]
    );

    // Not inside a disclosure, at any depth.
    assert!(
        document.select(&select("details #rg-legend-edges")).count() == 0,
        "the legend is behind a disclosure"
    );

    // Distinguishable without colour: each row carries its own dash geometry.
    let patterns: Vec<String> = document
        .select(&select("#rg-legend-edges line"))
        .map(|line| {
            format!(
                "{}|{}",
                line.value().attr("stroke-width").unwrap_or(""),
                line.value().attr("stroke-dasharray").unwrap_or("solid")
            )
        })
        .collect();
    assert_eq!(patterns.len(), 5);
    let mut unique = patterns.clone();
    unique.sort();
    unique.dedup();
    assert!(
        unique.len() >= 4,
        "the strength classes are not visually separable: {patterns:?}"
    );
}

/// ADR-0729 reaches the legend as well as the graph: a reader has to be told
/// what the distinct shape means, or the distinction is decorative.
#[test]
fn legend_names_the_trait_declaration_class() {
    let document = page();
    let classes: Vec<String> = document
        .select(&select("#rg-legend-nodes li"))
        .filter_map(|element| element.value().attr("data-node-class"))
        .map(str::to_owned)
        .collect();

    for expected in [
        "indexed",
        "unindexed",
        "frontier",
        "trait-declaration",
        "implementation",
    ] {
        assert!(classes.contains(&expected.to_owned()), "{classes:?}");
    }

    let frontier = text_of(&document, "#rg-legend-nodes li[data-node-class='frontier']");
    assert!(frontier.contains("not followed"), "{frontier}");
    assert!(frontier.contains("not a leaf"), "{frontier}");
}

/// Plan-05 §6.4 — the binding wording, verbatim, as the heading, and **copied
/// from the waist's own file** rather than composed here.
#[test]
fn unreachable_heading_is_the_waists_claim_verbatim() {
    let document = page();
    assert_eq!(
        text_of(&document, "#rg-unreachable-heading"),
        "not reachable from any endpoint version in this index"
    );
}

/// Plan-05 §6.4: the coverage block is a **sibling** of the heading, not
/// inside a disclosure, and it names bound against total roots and the
/// terminal categories. A reader who cannot see the covered set cannot
/// evaluate the claim.
#[test]
fn unreachable_panel_shows_coverage_adjacent() {
    let document = page();

    assert_eq!(
        document.select(&select("details #rg-coverage")).count(),
        0,
        "the coverage block is behind a disclosure"
    );

    let section = document
        .select(&select("#rg-unreachable"))
        .next()
        .expect("the panel exists");
    let children: Vec<&str> = section
        .children()
        .filter_map(scraper::ElementRef::wrap)
        .filter_map(|element| element.value().attr("id"))
        .collect();
    assert!(
        children.contains(&"rg-unreachable-heading") && children.contains(&"rg-coverage"),
        "the heading and the coverage block are not siblings: {children:?}"
    );

    let coverage = text_of(&document, "#rg-coverage");
    assert!(coverage.contains("1 of 2 roots"), "{coverage}");
    assert!(coverage.contains("1 unbound"), "{coverage}");
    assert!(coverage.contains("acme.task (v1)"), "{coverage}");

    let terminal = text_of(&document, "#rg-terminal");
    assert!(terminal.contains("third-party and stdlib"), "{terminal}");
    assert!(terminal.contains("not"), "{terminal}");
}

/// ADR-0732 and plan-03 §9 D-D: `IndexCoverage::notes` is the channel that
/// lets a reader tell *not indexed* from *not called*. A renderer that dropped
/// them would re-hide what the analysis worked to surface.
#[test]
fn coverage_notes_are_on_the_page_verbatim() {
    let document = page();
    let notes: Vec<String> = document
        .select(&select("#rg-notes li"))
        .map(|element| element.text().collect::<String>())
        .collect();

    assert_eq!(
        notes,
        support::coverage().notes,
        "the plugin notes did not reach the page"
    );
    assert_eq!(document.select(&select("details #rg-notes")).count(), 0);
}

/// An empty note set is a statement, not a gap — "every plugin was asked and
/// had nothing to add" is a different claim from "nobody was asked".
#[test]
fn an_empty_note_set_says_so_rather_than_vanishing() {
    let view = without_notes();
    let document =
        Html::parse_document(&support::render(&view, &[], &support::artifact()).text("index.html"));

    assert_eq!(document.select(&select("#rg-notes li")).count(), 0);
    let empty = text_of(&document, "#rg-notes-empty");
    assert!(empty.contains("anything to add"), "{empty}");
}

/// Plan-05 §4.5: `partial: true` weakens every claim below it, so the banner
/// is above the list and outside any disclosure — a banner, never a footnote.
#[test]
fn partial_index_renders_banner() {
    let view = support::index_view();
    let clean =
        Html::parse_document(&support::render(&view, &[], &support::artifact()).text("index.html"));
    assert_eq!(
        clean.select(&select("#rg-partial-banner")).count(),
        0,
        "a complete index must not carry the weakening banner"
    );

    let partial = partial_view();
    let document = Html::parse_document(
        &support::render(&partial, &[], &support::artifact()).text("index.html"),
    );

    let banner = text_of(&document, "#rg-partial-banner");
    // ADR-0743 gave `partial` a second cause. The banner used to say "A
    // provider failed during this run", which is now false half the time: a
    // provider that skipped one contract and returned roots did not fail. The
    // sentence states what both causes have in common, and the block below
    // says which one happened.
    assert!(
        banner.contains("built over less than this repository holds"),
        "{banner}"
    );
    assert!(
        banner.contains("may name code that is reachable"),
        "{banner}"
    );
    assert_eq!(
        document
            .select(&select("details #rg-partial-banner"))
            .count(),
        0
    );

    let section = document
        .select(&select("#rg-unreachable"))
        .next()
        .expect("the panel exists");
    let order: Vec<&str> = section
        .children()
        .filter_map(scraper::ElementRef::wrap)
        .filter_map(|element| element.value().attr("id"))
        .collect();
    let banner_at = order.iter().position(|id| *id == "rg-partial-banner");
    let heading_at = order.iter().position(|id| *id == "rg-unreachable-heading");
    assert!(banner_at < heading_at, "{order:?}");
}

/// Plan-05 §4.3: an unbound root is a reported gap, never a dropped row, and
/// its reason travels with it.
#[test]
fn an_unbound_root_is_reported_with_its_reason() {
    let document = page();
    let block = text_of(&document, "#rg-unbound");
    assert!(block.contains("TaskService.DeleteTask"), "{block}");
    assert!(block.contains("delete_task"), "{block}");
    assert!(block.contains("v3"), "{block}");
}

/// ADR-0743. A contract found and not read is a reported gap like an unbound
/// root, so it is named on the page with the reason its provider gave.
///
/// The structured list rather than a note: a reader who can see *which* file
/// was skipped can go and look at it, and a reader who is told only that
/// "something was skipped" cannot.
#[test]
fn an_unexamined_contract_is_named_with_its_reason() {
    let clean = page();
    assert_eq!(
        clean.select(&select("#rg-unexamined")).count(),
        0,
        "an index that read every contract carries no such block"
    );

    let document = Html::parse_document(
        &support::render(&unexamined_view(), &[], &support::artifact()).text("index.html"),
    );
    let block = text_of(&document, "#rg-unexamined");
    assert!(block.contains("proto/broken.proto"), "{block}");
    assert!(block.contains("reached end of file"), "{block}");

    assert_eq!(
        document.select(&select("#rg-partial-banner")).count(),
        1,
        "a skipped contract weakens every claim under it, so the banner is up"
    );
}

/// Plan-05 §6.3: no control anywhere offers a union across versions. Naming
/// one "All" would not make the union explicit.
#[test]
fn no_all_versions_control() {
    let view = support::index_view();
    let raw = support::render(&view, &[], &support::artifact()).text("index.html");
    let lowered = raw.to_lowercase();
    for forbidden in ["all versions", "any version", "every version", "union"] {
        assert!(!lowered.contains(forbidden), "{forbidden} reached the page");
    }

    let loader = reachgraph_render_html::page::LOADER.to_lowercase();
    for forbidden in ["all versions", "any version", "every version"] {
        assert!(
            !loader.contains(forbidden),
            "{forbidden} reached the loader"
        );
    }
}

/// ADR-0007 reaches the presentation layer: the string `v1` must not stand in
/// for an absent version anywhere, and a `join_key` containing `v2` is not
/// evidence of a version.
#[test]
fn an_unversioned_root_never_reads_as_a_number() {
    let view = unversioned_view();
    let document =
        Html::parse_document(&support::render(&view, &[], &support::artifact()).text("index.html"));

    let coverage = text_of(&document, "#rg-coverage");
    assert!(coverage.contains("acme.task (unversioned)"), "{coverage}");
    assert!(!coverage.contains("v1"), "{coverage}");
    assert!(!coverage.contains("v2"), "{coverage}");
}

/// Plan-05 §7: `index.html` carries an HTML comment naming each bundle, its
/// version and its SPDX id, **and** a visible entry in the UI. The artifact is
/// distributed to people who never see the repository.
#[test]
fn the_licence_notice_is_in_the_page_and_visible() {
    let view = support::index_view();
    let raw = support::render(&view, &[], &support::artifact()).text("index.html");

    for bundle in reachgraph_render_html::vendor::BUNDLES {
        assert!(
            raw.contains(&format!(
                "{} {} — {}",
                bundle.name, bundle.version, bundle.spdx
            )),
            "{} is missing from the licence comment",
            bundle.name
        );
    }

    let document = Html::parse_document(&raw);
    let entries: Vec<String> = document
        .select(&select("#rg-licence-list li"))
        .map(|element| element.text().collect::<String>())
        .collect();
    assert_eq!(entries.len(), reachgraph_render_html::vendor::BUNDLES.len());
    for bundle in reachgraph_render_html::vendor::BUNDLES {
        assert!(
            entries.iter().any(|entry| entry.contains(bundle.name)),
            "{} is not in the visible list",
            bundle.name
        );
    }
}

/// Plan-05 §6.1: `fetch()` over `file://` is CORS-blocked, and one blocked
/// fetch with a blank page is the worst version of this. The instruction is in
/// the document rather than composed at failure time.
#[test]
fn the_page_carries_the_serve_instruction() {
    let document = page();
    let blocker = text_of(&document, "#rg-file-protocol");
    assert!(blocker.contains("reachgraph serve"), "{blocker}");
    assert!(blocker.contains("overview.html"), "{blocker}");
}

/// Plan-05 §5: a display filter never restates what was computed. The sentence
/// saying so is beside the control that would otherwise imply it does.
#[test]
fn the_strength_filter_states_that_it_does_not_recompute() {
    let document = page();
    let caveat = text_of(&document, "#rg-filter-caveat");
    assert!(caveat.contains("only what is drawn"), "{caveat}");
    assert!(caveat.contains("does not move"), "{caveat}");
}

/// Plan-05 §6.2: the depth slider defaults to 3.
#[test]
fn the_depth_slider_defaults_to_three() {
    let document = page();
    let slider = document
        .select(&select("#rg-depth"))
        .next()
        .expect("the slider exists");
    assert_eq!(slider.value().attr("value"), Some("3"));
    assert_eq!(slider.value().attr("type"), Some("range"));
}

/// The page is a document a parser accepts: one `<html>`, one `<head>`, one
/// `<body>`, a title and a charset. Cheap, and it catches a template edit that
/// leaves an unclosed element.
#[test]
fn the_page_parses_as_one_html_document() {
    let document = page();
    assert_eq!(document.select(&select("html")).count(), 1);
    assert_eq!(document.select(&select("head")).count(), 1);
    assert_eq!(document.select(&select("body")).count(), 1);
    assert_eq!(document.select(&select("title")).count(), 1);
    assert_eq!(document.select(&select("meta[charset]")).count(), 1);
    assert!(document.errors.is_empty(), "{:?}", document.errors);

    // No placeholder survived substitution.
    let view = support::index_view();
    let raw = support::render(&view, &[], &support::artifact()).text("index.html");
    assert!(
        !raw.contains("{{RG_"),
        "an unsubstituted placeholder remains"
    );
}

// ---------------------------------------------------------------------------
// Variants
// ---------------------------------------------------------------------------

fn rebuild(view: &GraphView, coverage: reachgraph_plugin_api::IndexCoverage) -> GraphView {
    GraphView::new(
        view.nodes.clone(),
        view.edges.clone(),
        view.roots.clone(),
        view.plugins.clone(),
        coverage,
    )
}

fn partial_view() -> GraphView {
    let view = support::index_view();
    let mut coverage = support::coverage();
    coverage.partial = true;
    rebuild(&view, coverage)
}

fn unexamined_view() -> GraphView {
    let view = support::index_view();
    let mut coverage = support::coverage();
    coverage.partial = true;
    coverage.unexamined_contracts = vec![reachgraph_plugin_api::UnexaminedContract {
        contract: reachgraph_plugin_api::ContractId("proto/broken.proto".to_owned()),
        reason: "expected 'stream' or a type name, but reached end of file".to_owned(),
    }];
    rebuild(&view, coverage)
}

fn without_notes() -> GraphView {
    let view = support::index_view();
    let mut coverage = support::coverage();
    coverage.notes.clear();
    rebuild(&view, coverage)
}

fn unversioned_view() -> GraphView {
    let view = support::index_view();
    let mut coverage = support::coverage();
    coverage.versions = vec![reachgraph_plugin_api::VersionKey {
        contract: reachgraph_plugin_api::ContractId("acme.task".to_owned()),
        version: None,
    }];
    coverage.unbound_roots.clear();
    coverage.notes.clear();
    coverage.traversal_terminal_categories = vec![Category::ThirdParty];
    rebuild(&view, coverage)
}
