//! Source-level assertions over the waist itself — plan-01 §6.4, §10.2.
//!
//! Both read this crate's own source. They are the guards that no behavioural
//! test can express, because what they forbid is a word rather than a result.

use std::path::{Path, PathBuf};

fn source_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn sources() -> Vec<(PathBuf, String)> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(source_dir())
        .expect("the source directory is readable")
        .map(|entry| entry.expect("an entry is readable").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
        .collect();
    files.sort();

    assert!(
        files.len() > 5,
        "the walk found {} files, so these guards would assert over almost nothing",
        files.len()
    );

    files
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path).expect("the file is readable");
            (path, text)
        })
        .collect()
}

/// Plan-01 §6.4. The wording rule ships as data, and the forbidden word appears
/// nowhere — not in a type name, a field name, a variant, a doc comment or a
/// test name.
///
/// The word is assembled here rather than written, so this file does not fail
/// its own guard when the guard is widened to cover the test suite.
#[test]
fn word_dead_appears_nowhere_in_crate() {
    let forbidden: String = ['d', 'e', 'a', 'd'].iter().collect();

    for (path, text) in sources() {
        assert!(
            !text.to_lowercase().contains(&forbidden),
            "{} contains the word this crate refuses to use. The claim is data: \
             `UNREACHABLE_CLAIM`.",
            path.display()
        );
    }
}

/// Plan-01 §10.2. `docs/design.md` §8 measured that path prefixes are
/// language-specific *and* machine-specific, so they live in the plugin that
/// registered the classifier.
#[test]
fn core_contains_no_path_prefix() {
    // Assembled rather than written, for the same reason as above.
    let forbidden: Vec<String> = [
        ("src", "/"),
        ("target", "/"),
        ("/nix/store", ""),
        ("node_modules", ""),
        ("vendor", "/"),
        (".cargo/registry", ""),
    ]
    .iter()
    .map(|(head, tail)| format!("{head}{tail}"))
    .collect();

    for (path, text) in sources() {
        for fragment in &forbidden {
            assert!(
                !text.contains(fragment.as_str()),
                "{} names {fragment}. A prefix belongs to the plugin that registered the \
                 classifier, never to the waist.",
                path.display()
            );
        }
    }
}
