//! The engine stamp — plan-03 §2, §11, §13 Tier A.

use reachgraph_lang_rust::{ENGINE, RA_AP_VERSION};

/// Plan-03 §13: `ENGINE` **starts with** `"ra_ap_ide <version>"` for the
/// version pinned in `Cargo.toml`.
///
/// A prefix assertion, per §11's grammar, so an appended run-mode suffix does
/// not break it. The manifest is read rather than restated, because the whole
/// point is to catch a re-vendor that bumps the dependency and forgets the
/// stamp — a test that restated the version would be bumped in the same edit
/// that broke it.
#[test]
fn engine_string_matches_pinned_version() {
    // The workspace manifest, because that is where the pin lives: this
    // crate's own `Cargo.toml` says `ra_ap_ide.workspace = true`, and a test
    // that read the inherited spelling would assert nothing about a version.
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../Cargo.toml"))
        .expect("the workspace has a manifest");

    let pinned = manifest
        .lines()
        .find_map(|line| {
            let (name, rest) = line.split_once('=')?;
            if name.trim() != "ra_ap_ide" {
                return None;
            }
            let start = rest.find("\"=")? + 2;
            let end = rest[start..].find('"')? + start;
            Some(rest[start..end].to_owned())
        })
        .expect("Cargo.toml pins ra_ap_ide with an exact `=` requirement");

    assert_eq!(
        pinned, RA_AP_VERSION,
        "RA_AP_VERSION and the manifest pin disagree"
    );
    assert!(
        ENGINE.starts_with(&format!("ra_ap_ide {pinned}")),
        "ENGINE was {ENGINE:?}"
    );
}

/// §11's grammar: the version stamp is the prefix, any mode is a parenthesised
/// suffix. Both halves are asserted, because a stamp with no mode and a stamp
/// with the wrong mode are different defects.
#[test]
fn the_engine_stamp_follows_the_grammar() {
    let (stamp, mode) = ENGINE
        .split_once(" (")
        .expect("today's engine carries a mode suffix");

    assert_eq!(stamp, format!("ra_ap_ide {RA_AP_VERSION}"));
    assert_eq!(mode, "proc-macros: disabled)");
}

/// A relative repository path is REFUSED, not passed to `ra_ap`.
///
/// MEASURED 2026-09-19 by the release workflow's smoke test (run
/// 35457883578): `ra_ap_paths::AbsPathBuf::assert` refuses a relative path by
/// **panicking** — "expected absolute path, got ." — so `reachgraph .` died
/// with a backtrace out of a dependency and no message of its own.
///
/// The cli now resolves the path before any plugin sees it, and that is where
/// the policy lives. This is the second wall, and it is not a duplicate of the
/// first: it says that THIS crate refuses rather than aborts, for any caller,
/// including one that is not the cli. A library that panics on bad input is a
/// library whose contract is "do not get this wrong".
///
/// It sits in Tier A despite naming the engine because the refusal happens
/// before the engine does anything: no workspace is discovered, no cargo is
/// run, and nothing is read from disk.
#[test]
fn a_relative_root_is_an_error_rather_than_a_panic() {
    use reachgraph_plugin_api::{LanguagePlugin, PluginError};
    use std::path::Path;

    let plugin = reachgraph_lang_rust::RustPlugin::new();
    let outcome = plugin.discover_units(Path::new("."));

    match outcome {
        Err(PluginError::Engine { detail, .. }) => assert!(
            detail.contains("absolute"),
            "the error must say what was wrong with the path: {detail}"
        ),
        Err(other) => panic!("expected an engine error naming the path, got {other:?}"),
        Ok(units) => panic!("a relative root produced {} units", units.len()),
    }
}
