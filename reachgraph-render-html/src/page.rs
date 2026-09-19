//! Assembling `index.html` and `overview.html` — plan-05 §6.
//!
//! # What the binary renders and what the page renders
//!
//! Plan-05 §1: every classification the UI displays is computed in Rust. This
//! module goes one step further for the coverage panel and renders the
//! **text** in Rust too, because §6.4's obligations are structural ones that a
//! parser has to be able to see: the coverage sentence is a sibling of the
//! heading rather than inside a disclosure, the weakening banner sits above
//! the list, and every plugin note is on the page. A block the JavaScript
//! builds after load is a block `unreachable_panel_shows_coverage_adjacent`
//! cannot assert over, and plan-05 §8.1 is explicit that nothing in this
//! project executes the JavaScript.
//!
//! # The claim is copied, never composed
//!
//! The unreachability wording is read out of the waist's own
//! `unreachable.json` and placed in the heading verbatim. Plan-05 §4.5 ships
//! it as data precisely so a downstream consumer cannot re-word it, and this
//! crate is a downstream consumer. Composing the same sentence here would put
//! a second copy of the rule in the tree, and two copies of a rule is one copy
//! plus a future disagreement.

use std::fmt::Write as _;

use reachgraph_plugin_api::{ArtifactFile, Category, IndexCoverage, RenderError, RenderInput};

use crate::vendor::{self, Notice};

const TEMPLATE: &str = include_str!("../assets/page.html");
const STYLE: &str = include_str!("../assets/page.css");

/// The presenter, emitted beside the page and inlined into `overview.html`.
pub const LOADER: &str = include_str!("../assets/loader.js");

/// The path the loader is emitted at.
pub const LOADER_PATH: &str = "loader.js";

/// The page's title, and the artifact's name for itself.
const TITLE: &str = "reachgraph — endpoint-rooted call graph";

// ---------------------------------------------------------------------------
// Escaping
// ---------------------------------------------------------------------------

/// Escape text for an HTML text node or a double-quoted attribute.
///
/// Every string that reaches this function is arbitrary repository content —
/// a symbol name, a doc comment's first line, a plugin's note, a `UnitId`.
/// Plan-05 §6.5 hazard 2: a repository whose doc comment contains an
/// `onerror` attribute must not execute it in a reviewer's browser.
pub fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// The JSON string escape for `<`: a backslash, `u`, and four hex digits.
const LESS_THAN_ESCAPE: &[u8] = br"\u003c";

/// Escape JSON bytes for an inline `<script type="application/json">` block.
///
/// **Plan-05 §6.5 hazard 1 says to escape `<` as an HTML character reference,
/// and that is wrong here.** MEASURED and asserted by
/// `script_terminator_in_doc_is_escaped`: the content of a `<script>` element
/// is *raw text*, so character references inside it are never decoded. An
/// ampersand-l-t-semicolon would survive into the JSON as those four literal
/// characters and would corrupt every doc comment containing a `<`.
///
/// [`LESS_THAN_ESCAPE`] is the correct escape and is strictly better. It is a
/// JSON string escape, so the JSON parser decodes it back to `<` and the
/// document the page reads is byte-identical to the file's. It closes both
/// holes the plan names: neither `</script` nor `<!--` can appear in the
/// output, because no `<` can.
///
/// A `<` occurs in JSON only inside a string literal — no JSON structural
/// character is `<` — so replacing every one of them is correct without
/// parsing.
pub fn escape_json_for_script(bytes: &[u8]) -> Vec<u8> {
    let mut escaped = Vec::with_capacity(bytes.len());
    for byte in bytes {
        if *byte == b'<' {
            escaped.extend_from_slice(LESS_THAN_ESCAPE);
        } else {
            escaped.push(*byte);
        }
    }
    escaped
}

// ---------------------------------------------------------------------------
// The coverage panel
// ---------------------------------------------------------------------------

fn category_word(category: Category) -> &'static str {
    match category {
        Category::FirstParty => "first-party",
        Category::Generated => "generated",
        Category::WorkspaceSibling => "workspace-sibling",
        Category::ThirdParty => "third-party",
        Category::Stdlib => "stdlib",
    }
}

fn join_words(words: &[&str]) -> String {
    match words {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [start @ .., last] => format!("{} and {last}", start.join(", ")),
    }
}

/// Plan-05 §6.4. Three numbers, not one: `roots_bound` against `roots_total`
/// is what tells a reader that some operations never bound to a handler, so
/// the code behind them is in the list by construction.
fn coverage_sentence(coverage: &IndexCoverage) -> String {
    let unbound = coverage.roots_total.saturating_sub(coverage.roots_bound);
    let keys: Vec<String> = coverage
        .versions
        .iter()
        .map(|key| {
            let version = match &key.version {
                // ADR-0007 reaches the presentation layer: the string `v1`
                // must not appear anywhere for a `None` version.
                None => "unversioned".to_owned(),
                Some(version) => version.clone(),
            };
            format!("{} ({version})", key.contract.0)
        })
        .collect();

    format!(
        "Computed against {} of {} roots ({unbound} unbound) across {} contract(s): {}.",
        coverage.roots_bound,
        coverage.roots_total,
        coverage.contracts.len(),
        if keys.is_empty() {
            "none".to_owned()
        } else {
            keys.join(", ")
        }
    )
}

/// Plan-05 §4.5. Code reached only *through* a terminal category was not
/// followed, so a callback invoked by a third-party crate can be in the list.
fn terminal_sentence(coverage: &IndexCoverage) -> String {
    if coverage.traversal_terminal_categories.is_empty() {
        return "Traversal stopped at no category: every resolved target was followed.".to_owned();
    }
    let words: Vec<&str> = coverage
        .traversal_terminal_categories
        .iter()
        .copied()
        .map(category_word)
        .collect();
    format!(
        "Traversal stopped at {} nodes, so code reached only through one of those was not \
         followed.",
        join_words(&words)
    )
}

/// Plan-05 §4.5: not dismissible, above the list, outside any disclosure.
fn partial_banner(coverage: &IndexCoverage) -> String {
    if !coverage.partial {
        return String::new();
    }
    "<p class=\"rg-partial-banner\" id=\"rg-partial-banner\">A provider failed during this run. \
     This list is computed from an incomplete index and may name code that is reachable.</p>"
        .to_owned()
}

/// `IndexCoverage::notes`, verbatim.
///
/// The channel plan-03 §9 D-D and ADR-0732 exist for: a reader can tell *not
/// indexed* from *not called* only because the plugin said which it was. A
/// renderer that dropped these would re-hide what the analysis worked to
/// surface, so an empty set is stated rather than omitted — "every plugin had
/// nothing to add" is a different claim from "nobody was asked".
fn notes_block(coverage: &IndexCoverage) -> String {
    let mut html =
        String::from("<div class=\"rg-notes\" id=\"rg-notes\"><h3>Limits of this index</h3>");
    if coverage.notes.is_empty() {
        html.push_str(
            "<p id=\"rg-notes-empty\">Every contributing plugin was asked what this index does \
             not contain, and none had anything to add.</p>",
        );
    } else {
        html.push_str("<ul>");
        for note in &coverage.notes {
            let _ = write!(html, "<li>{}</li>", escape_html(note));
        }
        html.push_str("</ul>");
    }
    html.push_str("</div>");
    html
}

/// Plan-05 §4.3: an unbound root is a reported gap, never a dropped row. It is
/// on the endpoint list as a non-selectable entry and repeated here, because
/// this is where a reader is evaluating the claim it weakens.
fn unbound_block(coverage: &IndexCoverage) -> String {
    if coverage.unbound_roots.is_empty() {
        return String::new();
    }
    let mut html = String::from("<div class=\"rg-unbound\" id=\"rg-unbound\"><h3>");
    let _ = write!(
        html,
        "{} operation(s) bound to no handler</h3><p>A real handler for one of these may be in \
         the list below, put there by the missing binding rather than by being uncalled.</p><ul>",
        coverage.unbound_roots.len()
    );
    for root in &coverage.unbound_roots {
        let version = match &root.version {
            None => "unversioned".to_owned(),
            Some(version) => version.clone(),
        };
        let _ = write!(
            html,
            "<li><code>{}.{}</code> <span class=\"rg-badge\">{}</span> <span \
             class=\"rg-badge\">{}</span> — {}</li>",
            escape_html(&root.service),
            escape_html(&root.operation),
            escape_html(&version),
            escape_html(match root.direction {
                reachgraph_plugin_api::Direction::Served => "served",
                reachgraph_plugin_api::Direction::Consumed => "consumed",
            }),
            escape_html(&root.reason)
        );
    }
    html.push_str("</ul></div>");
    html
}

// ---------------------------------------------------------------------------
// Licences
// ---------------------------------------------------------------------------

fn licence_comment() -> String {
    let mut lines = String::from("Bundled JavaScript, emitted byte for byte:");
    for bundle in vendor::BUNDLES {
        let _ = write!(
            lines,
            "\n    {} {} — {} — vendor/{}",
            bundle.name, bundle.version, bundle.spdx, bundle.file
        );
    }
    let _ = write!(lines, "\n    Full notices: {}", vendor::LICENSES_PATH);
    lines
}

fn licence_list() -> String {
    let mut html = String::new();
    for bundle in vendor::BUNDLES {
        let where_ = match bundle.notice {
            Notice::Banner => "licence banner in the file",
            Notice::LicensesFile => "notice in vendor/LICENSES.txt",
        };
        let _ = write!(
            html,
            "<li><strong>{}</strong> {} — {} — <span class=\"rg-hint\">{}</span></li>",
            escape_html(bundle.name),
            escape_html(bundle.version),
            escape_html(bundle.spdx),
            escape_html(where_)
        );
    }
    html
}

// ---------------------------------------------------------------------------
// The page
// ---------------------------------------------------------------------------

/// Which of the two pages is being built.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shape {
    /// `index.html`: scripts by relative `src`, data fetched.
    Sharded,
    /// `overview.html`: every script and every byte of data inside the file.
    SingleFile,
}

/// Find one of the waist's files by path.
fn find<'a>(artifact: &'a [ArtifactFile], path: &str) -> Option<&'a ArtifactFile> {
    artifact.iter().find(|file| file.path == path)
}

/// Read the waist's unreachability claim and how many nodes it covers.
///
/// A `serde_json::Value` read, not a schema. This crate holds no mirror of the
/// waist's documents and must not grow one: the claim is a string the waist
/// authored, and the count is the length of a list it wrote.
fn claim_and_count(artifact: &[ArtifactFile]) -> Result<(String, usize), RenderError> {
    let file = find(artifact, "unreachable.json").ok_or_else(|| RenderError::Refused {
        reason: "unreachable.json is not in the artifact, so this page could not state what the \
                 index covers or what it does not reach"
            .to_owned(),
    })?;

    let document: serde_json::Value =
        serde_json::from_slice(&file.bytes).map_err(|error| RenderError::Refused {
            reason: format!("unreachable.json is not readable JSON: {error}"),
        })?;

    let claim = document
        .get("claim")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| RenderError::Refused {
            reason: "unreachable.json carries no `claim`, and this renderer does not compose one \
                     of its own"
                .to_owned(),
        })?
        .to_owned();

    let count = document
        .get("nodes")
        .and_then(serde_json::Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);

    Ok((claim, count))
}

/// Everything a single-file page inlines, as one JSON object literal built
/// from the waist's own bytes.
///
/// Concatenated rather than re-serialised: each value is the file's bytes,
/// escaped for the script block and otherwise untouched. That is what makes
/// `overview.html` carry the *same* JSON the sharded directory does.
fn inline_data(artifact: &[ArtifactFile], structure: &ArtifactFile) -> Vec<u8> {
    let mut data = Vec::from(b"{" as &[u8]);
    let mut first = true;

    for file in artifact.iter().chain(std::iter::once(structure)) {
        if !crate::is_graph_json(&file.path) {
            continue;
        }
        if !first {
            data.push(b',');
        }
        first = false;
        data.push(b'"');
        data.extend_from_slice(file.path.as_bytes());
        data.extend_from_slice(b"\":");
        data.extend_from_slice(&file.bytes);
    }

    data.push(b'}');
    escape_json_for_script(&data)
}

/// Build one page.
pub fn render_page(
    input: &RenderInput<'_>,
    structure: &ArtifactFile,
    shape: Shape,
) -> Result<String, RenderError> {
    let coverage = &input.view.coverage;
    let (claim, unreachable_count) = claim_and_count(input.artifact)?;

    let (scripts, loader, data) = match shape {
        Shape::Sharded => {
            let mut scripts = String::new();
            for bundle in vendor::BUNDLES {
                let _ = write!(
                    scripts,
                    "<script src=\"{}\"></script>\n    ",
                    bundle.output_path()
                );
            }
            (
                scripts,
                format!("<script src=\"{LOADER_PATH}\"></script>"),
                String::new(),
            )
        }
        Shape::SingleFile => {
            let mut scripts = String::new();
            for bundle in vendor::BUNDLES {
                // Byte for byte, banner included (plan-05 §7). A UMD bundle
                // contains no `</script` sequence; if a future one did, it
                // would not be a bundle this crate could inline, and the
                // guard below is what would say so.
                let _ = write!(scripts, "<script>\n{}\n</script>\n    ", bundle.source);
            }
            let data =
                String::from_utf8(inline_data(input.artifact, structure)).map_err(|error| {
                    RenderError::Refused {
                        reason: format!("the artifact's JSON is not valid UTF-8: {error}"),
                    }
                })?;
            (
                scripts,
                format!("<script>\n{LOADER}\n</script>"),
                format!("<script type=\"application/json\" id=\"rg-data\">{data}</script>\n    "),
            )
        }
    };

    let subtitle = format!(
        "{} unit(s) indexed by {} plugin(s); {} root(s), {} bound.",
        coverage.units_indexed.len(),
        coverage.plugins.len(),
        coverage.roots_total,
        coverage.roots_bound
    );

    let footer = "This artifact is a structural map of a repository: file paths, symbol names, \
                  doc text and which endpoints reach which code. Treat it with the same care as \
                  the source.";

    let licence_footer = format!(
        "Emitting these bundles is distribution, so the notices travel with the artifact. Full \
         text in {}.",
        vendor::LICENSES_PATH
    );

    let values: Vec<(&str, String)> = vec![
        ("RG_LICENCE_COMMENT", licence_comment()),
        ("RG_TITLE", escape_html(TITLE)),
        ("RG_STYLE", STYLE.to_owned()),
        ("RG_SUBTITLE", escape_html(&subtitle)),
        (
            "RG_OUT_HINT",
            "&lt;the directory holding this file&gt;".to_owned(),
        ),
        ("RG_LICENCE_LIST", licence_list()),
        ("RG_LICENCE_FOOTER", escape_html(&licence_footer)),
        ("RG_PARTIAL_BANNER", partial_banner(coverage)),
        // Verbatim, from the waist's own file.
        ("RG_CLAIM", escape_html(&claim)),
        ("RG_COVERAGE", escape_html(&coverage_sentence(coverage))),
        ("RG_TERMINAL", escape_html(&terminal_sentence(coverage))),
        ("RG_NOTES", notes_block(coverage)),
        ("RG_UNBOUND", unbound_block(coverage)),
        ("RG_UNREACHABLE_COUNT", unreachable_count.to_string()),
        ("RG_FOOTER", escape_html(footer)),
        ("RG_DATA", data),
        ("RG_SCRIPTS", scripts),
        ("RG_LOADER", loader),
    ];

    substitute(TEMPLATE, &values)
}

/// Fill every `{{KEY}}` in the template, in **one pass**.
///
/// Chained `str::replace` calls re-scan what an earlier substitution inserted,
/// so a contract id, a plugin note, an unbound reason or a doc comment
/// containing the literal text of a later placeholder would have that
/// placeholder's content injected into the page — the presenter, in the worst
/// case, and inside the JSON data block. Vanishingly unlikely, and the fix is
/// smaller than the argument for skipping it: "arbitrary repository content
/// does nothing" is the property this module spends its escaping on, and a
/// property that holds for all but one input is not a property.
///
/// A key the template names and the caller does not supply is an error rather
/// than an empty string. A page silently missing its coverage block is the
/// failure this crate exists to refuse.
fn substitute(template: &str, values: &[(&str, String)]) -> Result<String, RenderError> {
    const OPEN: &str = "{{";
    const CLOSE: &str = "}}";

    let mut page = String::with_capacity(template.len() * 2);
    let mut rest = template;

    while let Some(start) = rest.find(OPEN) {
        page.push_str(&rest[..start]);
        let after = &rest[start + OPEN.len()..];
        let Some(end) = after.find(CLOSE) else {
            return Err(RenderError::Refused {
                reason: "the page template holds an unterminated placeholder".to_owned(),
            });
        };
        let key = &after[..end];
        let value = values
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value)
            .ok_or_else(|| RenderError::Refused {
                reason: format!("the page template names {key} and nothing supplies it"),
            })?;
        page.push_str(value);
        rest = &after[end + CLOSE.len()..];
    }

    page.push_str(rest);
    Ok(page)
}
