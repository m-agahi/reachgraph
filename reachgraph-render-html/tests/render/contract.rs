//! Plan-05 §8.6 — the renderer contract.

use std::path::{Path, PathBuf};
use std::process::Command;

use reachgraph_plugin_api::{Classifier, LanguagePlugin, OutputSink, Plugin, Renderer};
use reachgraph_render_html::{HtmlRenderer, Overview};
use static_assertions::{assert_impl_all, assert_not_impl_any};

use crate::support;

/// Plan-00 §3.6, both halves.
///
/// **The negative is the load-bearing one.** It fails the day somebody re-adds
/// `Plugin` as a supertrait "for consistency", which would give a renderer a
/// `position_encoding` and a `detection` it cannot mean — and a defaulted
/// `Utf8Bytes` reaching the artifact's plugins table is indistinguishable from
/// one a plugin declared.
#[test]
fn renderer_implements_renderer_only() {
    assert_impl_all!(HtmlRenderer: Renderer, Send, Sync);
    assert_not_impl_any!(HtmlRenderer: Plugin, LanguagePlugin, Classifier);
}

/// Plan-00 §1's dependency rule, as a build failure rather than a convention.
///
/// A renderer receives the graph through `reachgraph-plugin-api`. If it could
/// reach `reachgraph-core`, the accessors on `GraphView` would stop being the
/// thing that keeps a renderer off the waist's algorithms, and the next
/// renderer would recompute a reachable set because it could.
///
/// Reads the **whole** dependency graph, dev-dependencies included, because
/// that is where the leak would arrive: a fixture built with the core is how a
/// crate acquires a core dependency without anybody deciding to.
#[test]
fn render_html_has_no_core_dependency() {
    let metadata = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--manifest-path",
        ])
        .arg(manifest())
        .output()
        .expect("cargo metadata runs");
    assert!(metadata.status.success(), "{metadata:?}");

    let document: serde_json::Value =
        serde_json::from_slice(&metadata.stdout).expect("cargo metadata emits json");
    let packages = document["packages"]
        .as_array()
        .expect("metadata carries packages");
    let package = packages
        .iter()
        .find(|package| package["name"] == "reachgraph-render-html")
        .expect("this crate is in its own metadata");

    let named: Vec<String> = package["dependencies"]
        .as_array()
        .expect("the package carries a dependency list")
        .iter()
        .map(|dependency| dependency["name"].as_str().unwrap_or("").to_owned())
        .collect();

    assert!(
        !named.iter().any(|name| name == "reachgraph-core"),
        "reachgraph-core reached this crate's dependency list: {named:?}"
    );
    assert!(
        named.iter().any(|name| name == "reachgraph-plugin-api"),
        "the guard would pass vacuously on a crate with no reachgraph \
         dependency at all: {named:?}"
    );
}

/// Plan-05 §8.6. The core owns where bytes land (plan-00 §3.6); a renderer
/// names a relative path and writes. A `std::fs` write here would hand the
/// renderer back the filesystem authority `OutputSink` exists to keep away
/// from it.
///
/// Scoped to non-test, non-build-script first-party sources: the fixtures in
/// `tests/` legitimately touch the filesystem, and `include_str!` is a
/// compile-time read rather than a write.
#[test]
fn render_writes_only_through_sink() {
    const FORBIDDEN: [&str; 4] = [
        "std::fs::write",
        "std::fs::File",
        "fs::create_dir",
        "OpenOptions",
    ];

    let mut offenders = Vec::new();
    let mut scanned = 0;
    for file in sources(&crate_dir().join("src")) {
        let text = std::fs::read_to_string(&file).expect("a source file is readable");
        scanned += 1;
        for (number, line) in text.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            for token in FORBIDDEN {
                if line.contains(token) {
                    offenders.push(format!("{}:{}: {token}", file.display(), number + 1));
                }
            }
        }
    }

    assert!(scanned >= 4, "the walk found {scanned} files");
    assert!(offenders.is_empty(), "{offenders:?}");
}

/// `Overview::Never` and `Overview::Under` are different instructions, and the
/// type is what keeps them from collapsing into one nullable number.
#[test]
fn the_single_file_policy_is_a_named_choice() {
    assert_eq!(
        HtmlRenderer::new().overview(),
        Overview::Under(reachgraph_render_html::DEFAULT_INLINE_THRESHOLD)
    );
    assert_eq!(
        HtmlRenderer::with_overview(Overview::Never).overview(),
        Overview::Never
    );
}

/// A sink that refuses becomes a [`reachgraph_plugin_api::RenderError`], never
/// a partially written artifact reported as success.
#[test]
fn a_refusing_sink_is_an_error_not_a_half_artifact() {
    struct Refusing;
    impl OutputSink for Refusing {
        fn write(&mut self, relative_path: &str, _bytes: &[u8]) -> std::io::Result<()> {
            Err(std::io::Error::other(format!(
                "no room for {relative_path}"
            )))
        }
    }

    let view = support::index_view();
    let artifact = support::artifact();
    let input = reachgraph_plugin_api::RenderInput {
        view: &view,
        shards: &[],
        artifact: &artifact,
    };

    let error = HtmlRenderer::new()
        .render(&input, &mut Refusing)
        .expect_err("a refusing sink is not a success");
    assert!(
        matches!(error, reachgraph_plugin_api::RenderError::Sink { .. }),
        "{error:?}"
    );
}

/// Plan-05 §4.5 ships the unreachability claim as data so a consumer cannot
/// re-word it. A consumer that cannot find it refuses rather than composing
/// its own sentence.
#[test]
fn a_missing_claim_refuses_rather_than_inventing_one() {
    let view = support::index_view();
    let artifact = vec![support::file(
        "unreachable.json",
        r#"{"schema_version":1,"nodes":[]}"#,
    )];
    let input = reachgraph_plugin_api::RenderInput {
        view: &view,
        shards: &[],
        artifact: &artifact,
    };

    let error = HtmlRenderer::new()
        .render(&input, &mut support::Recording::default())
        .expect_err("a claimless artifact is not renderable");
    match error {
        reachgraph_plugin_api::RenderError::Refused { reason } => {
            assert!(reason.contains("claim"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
}

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn manifest() -> PathBuf {
    crate_dir().join("Cargo.toml")
}

/// Every `.rs` file under a directory, at any depth.
///
/// Recursive, and that is load-bearing: a one-level walk lets a stray nested
/// module sit in the tree carrying what the guard refuses, and a file that
/// greps as present but is never scanned is the vacuous shape these guards
/// exist to refuse.
pub fn sources(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(sources(&path));
        } else if path.extension().is_some_and(|extension| extension == "rs")
            && path.file_name().is_some_and(|name| name != "build.rs")
        {
            found.push(path);
        }
    }
    found.sort();
    found
}
