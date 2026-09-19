//! An absent output format is not an error — plan-06 §3.1.
//!
//! # The bug this module exists to keep fixed
//!
//! MEASURED 2026-09-19 on `cargo test --workspace --no-default-features`:
//! 19 failures, 17 of them `error: no output format named <default>`. With
//! `render-html` off nothing registers, and the binary treated an empty
//! renderer registry as a failed lookup — so a build with no renderer wrote no
//! artifact at all, and tests about plugin notes and preflight died on a
//! message about output formats.
//!
//! **The waist writes the artifact whether or not a renderer exists.**
//! `reachgraph-core` emits `endpoints.json`, the shards, `unreachable.json`
//! and `versions.json` (ADR-0727), and PR F shipped a working tool with no
//! renderer at all. A page is an addition to that, never a precondition for
//! it.
//!
//! # Two cases that must stay distinct
//!
//! Collapsing them is the honest-absence defect in another costume:
//!
//! - **nothing compiled in, nothing asked for** — succeed, write the artifact,
//!   draw no page. Not a warning either: nothing went wrong.
//! - **a name asked for that is not registered** — a usage error, naming what
//!   is there. When nothing is there, say so in words rather than printing
//!   `available: none` as though `none` were a format somebody could pass.
//!
//! # What runs where
//!
//! The two selection tests run in **every** feature configuration, because
//! `RendererRegistry::new()` is empty by construction whatever is compiled in.
//! The end-to-end test is the reproduction and runs only in the build that
//! failed — there is no null renderer to select in the default build, and
//! inventing one to make a test symmetrical would be the defect this module is
//! about.

use reachgraph_cli::renderers::{RendererRegistry, Selection};

/// An empty registry asked for nothing selects nothing, and that is an answer
/// rather than a failure.
#[test]
fn an_empty_registry_asked_for_no_format_selects_nothing() {
    let registry = RendererRegistry::new();
    assert!(matches!(registry.select(None), Selection::NoneAvailable));
}

/// A name nobody registered is still a usage error, and the diagnosis says
/// this build has none rather than offering `none` as a choice.
#[test]
fn an_empty_registry_asked_for_a_name_reports_that_the_build_has_none() {
    let registry = RendererRegistry::new();
    match registry.select(Some("html")) {
        Selection::Unknown { available } => assert!(available.is_empty(), "{available:?}"),
        other => panic!("expected an unknown-format answer, got {other:?}"),
    }
}

#[cfg(not(feature = "render-html"))]
mod without_a_renderer {
    use std::fs;

    use crate::support::{self, TempDir};

    /// **The reproduction, pinned on the artifact rather than on the exit
    /// code.** A build with no renderer writes the waist's four documents and
    /// its shards, writes no page, and succeeds.
    #[test]
    fn a_build_with_no_renderer_still_writes_the_artifact() {
        let temp = TempDir::new("json-only");
        let repo = support::repo_for("versioned_pair", &temp);
        let out = temp.join("out");
        let registry = support::registry_of(support::doc_of("versioned_pair"), &repo);

        let result = support::run(
            &registry,
            &[
                repo.to_str().expect("utf-8"),
                "-o",
                out.to_str().expect("utf-8"),
            ],
        );

        assert_eq!(result.code, reachgraph_cli::EXIT_OK, "{}", result.err);

        for name in [
            "endpoints.json",
            "unreachable.json",
            "versions.json",
            "run.json",
        ] {
            assert!(out.join(name).is_file(), "{name} was not written");
        }
        assert!(out.join("graph").is_dir());
        assert!(
            fs::read_dir(out.join("graph"))
                .expect("the shard directory is readable")
                .count()
                > 0
        );

        // No page, and no half of one.
        for name in ["index.html", "overview.html", "structure.json", "loader.js"] {
            assert!(
                !out.join(name).exists(),
                "{name} was written with no renderer"
            );
        }
        assert!(!out.join("vendor").exists());

        // And nothing was said about formats. Nothing went wrong, so a
        // warning would be a claim that something did.
        //
        // The temporary directory's name is deliberately free of the words
        // below: every path this run prints reaches stderr, so a label
        // containing one of them would make this assertion pass or fail on
        // the label rather than on what the binary said.
        let lowered = result.err.to_lowercase();
        for word in ["output format", "renderer", "render"] {
            assert!(!lowered.contains(word), "{word}: {}", result.err);
        }
    }

    /// The other half stays an error: a name that is not registered.
    #[test]
    fn asking_for_a_format_this_build_lacks_is_still_a_usage_error() {
        let temp = TempDir::new("json-only-named");
        let repo = support::repo_for("minimal", &temp);
        let out = temp.join("out");
        let registry = support::registry_of(support::doc_of("minimal"), &repo);

        let result = support::run(
            &registry,
            &[
                repo.to_str().expect("utf-8"),
                "-o",
                out.to_str().expect("utf-8"),
                "--renderer",
                "html",
            ],
        );

        assert_eq!(result.code, reachgraph_cli::EXIT_USAGE, "{}", result.err);
        assert!(
            result.err.contains("no output format named html"),
            "{}",
            result.err
        );
        assert!(
            result.err.contains("this build has no output format"),
            "{}",
            result.err
        );
        assert!(
            !result.err.contains("available: none"),
            "`none` is not a format somebody could pass: {}",
            result.err
        );
        assert!(!out.exists(), "a refused run wrote something");
    }
}
