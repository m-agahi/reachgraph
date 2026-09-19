//! Plan-05 §3 and §8.3 — the vendored bytes are the bytes upstream published.

use std::path::PathBuf;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::support;

#[derive(Debug, Deserialize)]
struct Recorded {
    bundle: Vec<RecordedBundle>,
}

#[derive(Debug, Deserialize)]
struct RecordedBundle {
    file: String,
    name: String,
    version: String,
    spdx: String,
    source: String,
    bytes: u64,
    sha256: String,
    notice: String,
}

fn recorded() -> Recorded {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vendor/VENDOR.toml");
    let text = std::fs::read_to_string(&path).expect("VENDOR.toml is readable");
    toml::from_str(&text).expect("VENDOR.toml parses")
}

fn digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Plan-05 §8.3. The emitted file is byte-identical to what upstream
/// published: no minification, no re-bundling, no banner stripping, and no
/// repository formatter in between.
///
/// `.pre-commit-config.yaml` excludes `vendor/*.js` from the three whitespace
/// hooks for exactly this reason, recorded there. MEASURED with the hooks
/// unrestricted: `end-of-file-fixer` appended a newline to three of the four,
/// `trailing-whitespace` deleted 35 bytes from `cose-base.js` and 24 from
/// `cytoscape-fcose.js`, and `mixed-line-ending` rewrote every CRLF in
/// `layout-base.js`. That is a formatter editing the inside of a minified
/// bundle it cannot reason about.
#[test]
fn vendor_bytes_are_unmodified() {
    let view = support::index_view();
    let written = support::render(&view, &[], &support::artifact());
    let table = recorded();

    assert_eq!(
        table.bundle.len(),
        reachgraph_render_html::vendor::BUNDLES.len(),
        "VENDOR.toml and the compiled-in table disagree about how many bundles ship"
    );

    for entry in &table.bundle {
        let path = format!("vendor/{}", entry.file);
        let emitted = written
            .written
            .get(&path)
            .unwrap_or_else(|| panic!("{path} was not emitted"));

        assert_eq!(
            emitted.len() as u64,
            entry.bytes,
            "{} is {} bytes, VENDOR.toml records {}",
            entry.file,
            emitted.len(),
            entry.bytes
        );
        assert_eq!(
            digest(emitted),
            entry.sha256,
            "{} does not hash to its recorded sha256",
            entry.file
        );
    }
}

/// The recorded table and the compiled-in one describe the same files. Two
/// tables that can disagree are one table plus a future disagreement.
#[test]
fn the_recorded_table_matches_the_compiled_in_one() {
    let table = recorded();

    for (entry, bundle) in table
        .bundle
        .iter()
        .zip(reachgraph_render_html::vendor::BUNDLES.iter())
    {
        assert_eq!(entry.file, bundle.file);
        assert_eq!(entry.name, bundle.name);
        assert_eq!(entry.version, bundle.version);
        assert_eq!(entry.spdx, bundle.spdx);
        let expected = match bundle.notice {
            reachgraph_render_html::vendor::Notice::Banner => "banner",
            reachgraph_render_html::vendor::Notice::LicensesFile => "licenses-file",
        };
        assert_eq!(entry.notice, expected, "{}", entry.file);
    }
}

/// ADR-0001's rule, on the other side of the page: every recorded source is a
/// place a human fetched a file from once, and no emitted byte points at one.
#[test]
fn a_recorded_source_url_never_reaches_the_artifact() {
    let view = support::index_view();
    let written = support::render(&view, &[], &support::artifact());
    let page = written.text("index.html");
    let overview = written.text("overview.html");

    for entry in recorded().bundle {
        assert!(entry.source.starts_with("https://"), "{}", entry.source);
        assert!(
            !page.contains(&entry.source),
            "{} is in index.html",
            entry.source
        );
        assert!(
            !overview.contains(&entry.source),
            "{} is in overview.html",
            entry.source
        );
    }
}

/// The hash check discriminates. Without this, `vendor_bytes_are_unmodified`
/// could be comparing two copies of the same mistake.
#[test]
fn the_digest_separates_a_one_byte_change() {
    let bundle = reachgraph_render_html::vendor::BUNDLES[0];
    let original = bundle.source.as_bytes().to_vec();

    let mut trimmed = original.clone();
    trimmed.pop();
    assert_ne!(digest(&original), digest(&trimmed));

    let mut appended = original.clone();
    appended.push(b'\n');
    assert_ne!(digest(&original), digest(&appended));

    let entry = recorded()
        .bundle
        .into_iter()
        .find(|entry| entry.file == bundle.file)
        .expect("the first bundle is recorded");
    assert_eq!(digest(&original), entry.sha256);
}
