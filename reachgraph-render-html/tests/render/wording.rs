//! Plan-05 §8.4 — the wording guard.
//!
//! design.md §8: telling somebody to delete working code is the one failure
//! that permanently destroys trust. The forbidden word must appear in no
//! label, heading, tooltip, legend or template string this crate authors.
//!
//! # The scoping is deliberate and must not be widened
//!
//! A scan over the emitted byte stream would false-positive on the vendored
//! Cytoscape bundle and on any analysed repository whose doc text says "reaps
//! dead sessions". A guard that cries wolf gets disabled, and then there is no
//! guard. So what is scanned is **this crate's own sources and templates**.
//!
//! # And the templates are the half that matters
//!
//! Every human-authored label on the page lives in `assets/`, not in `src/`.
//! A guard scoped to `src/**` — which is how plan-05 §8.4 words it — would
//! pass vacuously over exactly the files most likely to carry a bad label.
//! `the_guard_sees_the_templates` below proves the walk reaches them by
//! counting what it found, so the scope cannot silently shrink to nothing.

use std::path::{Path, PathBuf};

use crate::contract::sources;

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The word is assembled rather than written, so this file does not fail the
/// guard it defines.
fn forbidden() -> String {
    let mut word = String::from("de");
    word.push('a');
    word.push('d');
    word
}

/// Every file this crate authors: Rust sources and page templates. The
/// vendored bundles are somebody else's bytes and are not scanned.
fn authored() -> Vec<PathBuf> {
    let mut files = sources(&crate_dir().join("src"));
    files.extend(assets(&crate_dir().join("assets")));
    files.sort();
    files
}

fn assets(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(assets(&path));
        } else {
            found.push(path);
        }
    }
    found
}

/// Word-boundary, case-insensitive. `already` and `deadline` are ordinary
/// English; the claim is the standalone word.
fn hits(text: &str, word: &str) -> Vec<String> {
    let lowered = text.to_lowercase();
    let mut found = Vec::new();
    let mut from = 0;

    while let Some(offset) = lowered[from..].find(word) {
        let start = from + offset;
        let end = start + word.len();
        let before = lowered[..start]
            .chars()
            .next_back()
            .map(|character| character.is_alphanumeric() || character == '_')
            .unwrap_or(false);
        let after = lowered[end..]
            .chars()
            .next()
            .map(|character| character.is_alphanumeric() || character == '_')
            .unwrap_or(false);
        if !before && !after {
            let line = lowered[..start].matches('\n').count() + 1;
            found.push(format!("line {line}"));
        }
        from = end;
    }

    found
}

#[test]
fn renderer_authors_no_forbidden_wording() {
    let word = forbidden();
    let mut offenders = Vec::new();

    for file in authored() {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        for hit in hits(&text, &word) {
            offenders.push(format!("{}: {hit}", file.display()));
        }
    }

    assert!(offenders.is_empty(), "{offenders:?}");
}

/// The guard's own reach, asserted rather than assumed. A walk that found no
/// template would pass the test above by having nothing to read.
#[test]
fn the_guard_sees_the_templates() {
    let files = authored();
    let names: Vec<String> = files
        .iter()
        .filter_map(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .collect();

    for expected in [
        "page.html",
        "loader.js",
        "page.css",
        "lib.rs",
        "dispatch.rs",
    ] {
        assert!(names.contains(&expected.to_owned()), "{names:?}");
    }
    assert!(files.len() >= 7, "{names:?}");
}

/// The matcher discriminates. Without this the guard above could be a function
/// that always returns an empty list, and every mutation of it would still
/// pass.
#[test]
fn the_matcher_finds_the_word_and_leaves_english_alone() {
    let word = forbidden();

    assert_eq!(hits(&format!("this code is {word}"), &word).len(), 1);
    assert_eq!(hits(&format!("{word}-code analysis"), &word).len(), 1);
    assert_eq!(hits(&format!("({word})"), &word).len(), 1);

    assert!(hits("the deadline passed", &word).is_empty());
    assert!(hits("already handled", &word).is_empty());
    assert!(hits("undeadly", &word).is_empty());
}

/// And the walk reads what the matcher is pointed at. Planting the word in a
/// scratch copy of a real template proves the two halves are connected — the
/// mutation plan-05's discipline asks for, run as a test rather than by hand.
#[test]
fn the_guard_would_fail_on_a_planted_label() {
    let word = forbidden();
    let template = std::fs::read_to_string(crate_dir().join("assets/page.html"))
        .expect("the template is readable");

    assert!(hits(&template, &word).is_empty());
    let planted = template.replace("<h2>Endpoints</h2>", &format!("<h2>{word} code</h2>"));
    assert_ne!(planted, template, "the anchor moved; the mutation is inert");
    assert_eq!(hits(&planted, &word).len(), 1);
}
