//! Plan-05 §8.5 — `overview.html` and the two inlining hazards.

use reachgraph_render_html::{HtmlRenderer, Overview};
use scraper::{Html, Selector};
use serde_json::Value;

use crate::support;

fn data_block(page: &str) -> String {
    let document = Html::parse_document(page);
    let selector = Selector::parse("script#rg-data").expect("the selector parses");
    document
        .select(&selector)
        .next()
        .expect("the inline data block exists")
        .text()
        .collect::<String>()
}

/// Plan-05 §6.5: an addition, never a replacement. The sharded directory is
/// emitted in the small case too.
#[test]
fn overview_emitted_under_threshold_beside_the_sharded_page() {
    let view = support::index_view();
    let written = support::render(&view, &[support::shard()], &support::artifact());

    assert!(written.written.contains_key("overview.html"));
    assert!(written.written.contains_key("index.html"));
    assert!(written.written.contains_key("structure.json"));
    assert!(written.written.contains_key("loader.js"));
}

/// Over threshold, and `Overview::Never`, both leave the sharded output
/// untouched.
#[test]
fn overview_absent_over_threshold_and_when_refused() {
    let view = support::index_view();
    let artifact = support::artifact();

    let tiny = support::render_with(
        HtmlRenderer::with_overview(Overview::Under(10)),
        &view,
        &[],
        &artifact,
    );
    assert!(!tiny.written.contains_key("overview.html"));
    assert!(tiny.written.contains_key("index.html"));

    let never = support::render_with(
        HtmlRenderer::with_overview(Overview::Never),
        &view,
        &[],
        &artifact,
    );
    assert!(!never.written.contains_key("overview.html"));
    assert!(never.written.contains_key("index.html"));
}

/// Plan-05 §6.5: the threshold is over the **graph JSON**, and the vendored
/// JavaScript is deliberately not counted. The emitted file is larger than the
/// threshold by those bytes plus the presenter, and that discrepancy is on
/// purpose — stated so nobody later "fixes" it by counting the JavaScript in
/// and silently shrinking the budget.
#[test]
fn the_threshold_counts_graph_json_and_not_the_bundles() {
    let view = support::index_view();
    let artifact = support::artifact();
    let graph_json: u64 = artifact
        .iter()
        .filter(|file| file.path.ends_with(".json") && file.path != "run.json")
        .map(|file| file.bytes.len() as u64)
        .sum();

    let written = support::render_with(
        // Generous enough for the JSON, far smaller than the bundles.
        HtmlRenderer::with_overview(Overview::Under(graph_json + 4096)),
        &view,
        &[],
        &artifact,
    );
    let overview = written.text("overview.html");

    assert!(written.written.contains_key("overview.html"));
    assert!(
        overview.len() as u64 > graph_json + 4096,
        "the emitted page is smaller than the bundles it must contain"
    );
}

/// Plan-05 §6.5: the single-file page carries every script inside it. A
/// `<script src>` would make it exactly not a single file.
#[test]
fn the_single_file_page_references_nothing() {
    let view = support::index_view();
    let written = support::render(&view, &[], &support::artifact());
    let document = Html::parse_document(&written.text("overview.html"));
    let selector = Selector::parse("script[src]").expect("the selector parses");

    assert_eq!(document.select(&selector).count(), 0);
}

/// Plan-05 §7: the banner travels into the inlined copy too. Emitting is
/// distribution whichever page does it.
#[test]
fn licence_banner_survives_emission() {
    use reachgraph_render_html::vendor::{Notice, BUNDLES, LICENSES};

    let view = support::index_view();
    let written = support::render(&view, &[], &support::artifact());
    let overview = written.text("overview.html");

    let mut banners = 0;
    for bundle in BUNDLES {
        let emitted = written.text(&bundle.output_path());
        assert_eq!(emitted, bundle.source, "{} was rewritten", bundle.file);
        assert!(
            overview.contains(bundle.source),
            "{} is not inlined",
            bundle.file
        );

        match bundle.notice {
            Notice::Banner => {
                banners += 1;
                // MEASURED: `cytoscape.min.js` opens with its MIT text. The
                // three iVis bundles are webpack output with no banner at
                // all, which plan-05 §8.3 assumed away — so their notice is
                // asserted through `LICENSES.txt` below instead of by a rule
                // that is false for three quarters of the set.
                let head = &bundle.source[..bundle.source.len().min(2000)];
                assert!(head.contains("MIT"), "{} lost its banner", bundle.file);
            }
            Notice::LicensesFile => {
                assert!(
                    LICENSES.contains(bundle.name),
                    "{} has no notice anywhere",
                    bundle.file
                );
            }
        }
    }
    assert_eq!(banners, 1, "the banner/no-banner split moved");

    let licences = written.text("vendor/LICENSES.txt");
    assert_eq!(licences, LICENSES);
    assert!(licences.contains("MIT"));
}

/// Plan-05 §8.5: extract `#rg-data` and round-trip it.
#[test]
fn inline_data_is_valid_json() {
    let view = support::index_view();
    let written = support::render(&view, &[], &support::artifact());
    let block = data_block(&written.text("overview.html"));

    let parsed: Value = serde_json::from_str(&block).expect("the inline block is json");
    let object = parsed.as_object().expect("the block is an object");

    for path in [
        "endpoints.json",
        "unreachable.json",
        "versions.json",
        "structure.json",
    ] {
        assert!(object.contains_key(path), "{path} is not inlined");
    }
    // `run.json` is the binary's run record, not graph data. The page never
    // reads it, so inlining it would only grow the file.
    assert!(!object.contains_key("run.json"));
}

/// The bytes the page carries are the bytes the sharded directory holds. The
/// renderer never re-serialises a waist document, so the two cannot drift —
/// and this is the assertion that proves the seam rather than arguing it.
#[test]
fn the_inlined_document_equals_the_file_it_came_from() {
    let view = support::index_view();
    let artifact = support::artifact();
    let written = support::render(&view, &[], &artifact);
    let inline: Value =
        serde_json::from_str(&data_block(&written.text("overview.html"))).expect("json");

    for file in &artifact {
        if file.path == "run.json" {
            continue;
        }
        let from_file: Value = serde_json::from_slice(&file.bytes).expect("the file is json");
        assert_eq!(inline[&file.path], from_file, "{} drifted", file.path);
    }

    let sidecar: Value = serde_json::from_slice(&written.written["structure.json"]).expect("json");
    assert_eq!(inline["structure.json"], sidecar);
}

/// Plan-05 §6.5 hazard 1, with the plan's own remedy corrected.
///
/// A doc comment containing a script terminator would end the JSON block and
/// corrupt the page. The plan says to escape `<` as an HTML character
/// reference; that is wrong, because a `<script>` element's content is raw
/// text and character references in it are never decoded — the reference would
/// survive into the JSON as literal characters. The JSON escape is used
/// instead, and it decodes back to the original.
#[test]
fn script_terminator_in_doc_is_escaped() {
    let view = support::index_view();
    let artifact = vec![
        support::file(
            "unreachable.json",
            r#"{"claim":"not reachable from any endpoint version in this index","nodes":[]}"#,
        ),
        support::file(
            "endpoints.json",
            r#"{"doc":"closes with </script> and opens with <!-- too"}"#,
        ),
    ];

    let written = support::render(&view, &[], &artifact);
    let page = written.text("overview.html");

    assert!(
        !page.contains("</script> and"),
        "a script terminator survived into the page"
    );
    assert!(!page.contains("<!-- too"), "a comment opener survived");

    let inline: Value = serde_json::from_str(&data_block(&page)).expect("the block still parses");
    assert_eq!(
        inline["endpoints.json"]["doc"], "closes with </script> and opens with <!-- too",
        "the escape did not round-trip"
    );
}

/// Plan-05 §6.5 hazard 2. Node labels and doc text are arbitrary repository
/// content. A doc comment carrying an event-handler attribute must not become
/// live markup in a reviewer's browser.
#[test]
fn doc_text_is_not_injected_as_html() {
    let payload = r#"<img src=x onerror=alert(1)>"#;

    let mut coverage = support::coverage();
    coverage.notes = vec![format!("a note containing {payload}")];
    coverage.unbound_roots[0].reason = format!("a reason containing {payload}");

    let base = support::index_view();
    let view = reachgraph_plugin_api::GraphView::new(
        base.nodes.clone(),
        base.edges.clone(),
        base.roots.clone(),
        base.plugins.clone(),
        coverage,
    );

    let written = support::render(&view, &[], &support::artifact());
    for path in ["index.html", "overview.html"] {
        let page = written.text(path);
        let document = Html::parse_document(&page);
        let selector = Selector::parse("img").expect("the selector parses");
        assert_eq!(
            document.select(&selector).count(),
            0,
            "repository text became markup in {path}"
        );
        assert!(
            !page.contains("<img"),
            "an element opener reached {path} as markup"
        );
        // The payload IS present, escaped. Without this the test would pass
        // on a renderer that dropped the text entirely, which is a different
        // defect wearing the same green tick.
        assert!(
            page.contains("&lt;img src=x onerror=alert(1)&gt;"),
            "the payload is not in {path} at all, so the guard proves nothing"
        );
    }
}

/// `overview.html` inlines each bundle raw, so a `</script` anywhere in the
/// vendored JavaScript would end the element early and silently corrupt the
/// page. MEASURED: none of the five contains one. Asserted rather than
/// assumed, so a re-vendor cannot regress it unnoticed.
#[test]
fn no_vendored_bundle_contains_a_script_terminator() {
    for bundle in reachgraph_render_html::vendor::BUNDLES {
        let lowered = bundle.source.to_lowercase();
        assert!(
            !lowered.contains("</script"),
            "{} cannot be inlined raw",
            bundle.file
        );
        assert!(
            !lowered.contains("<!--"),
            "{} carries a comment opener",
            bundle.file
        );
    }
}

/// Repository text that happens to spell a template placeholder does nothing.
///
/// The page is filled in ONE pass. Chained replacements re-scan what an
/// earlier substitution inserted, so a plugin note reading like a placeholder
/// would have that placeholder's content injected — the presenter, in the
/// worst case, and inside the JSON data block.
#[test]
fn repository_text_that_looks_like_a_placeholder_is_inert() {
    let payload = "{{RG_LOADER}} and {{RG_DATA}} and {{RG_SCRIPTS}}";

    let mut coverage = support::coverage();
    coverage.notes = vec![format!("a note containing {payload}")];

    let base = support::index_view();
    let view = reachgraph_plugin_api::GraphView::new(
        base.nodes.clone(),
        base.edges.clone(),
        base.roots.clone(),
        base.plugins.clone(),
        coverage,
    );

    let written = support::render(&view, &[], &support::artifact());
    for path in ["index.html", "overview.html"] {
        let page = written.text(path);
        assert!(
            page.contains(payload),
            "the note is not in {path} at all, so the guard proves nothing"
        );
        // One data block, one presenter — not two of either.
        assert_eq!(
            page.matches(r#"id="rg-data""#).count(),
            usize::from(path == "overview.html"),
            "the data block was duplicated in {path}"
        );
        assert_eq!(
            page.matches("IT CLASSIFIES NOTHING").count(),
            usize::from(path == "overview.html"),
            "the presenter was injected into {path} by a placeholder in repository text"
        );
    }
}
