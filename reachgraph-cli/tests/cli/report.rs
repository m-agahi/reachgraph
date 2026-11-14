//! Plan-06 §7.5 — the run report and the privacy note.

use std::fs;

use crate::support::{
    doc_of, registry_holding, registry_of, repo_for, run, DetectableFixture, TempDir,
};
use reachgraph_fixture::FixturePlugin;

fn analyse(case: &str, label: &str) -> (crate::support::Run, TempDir, std::path::PathBuf) {
    let temp = TempDir::new(label);
    let repo = repo_for(case, &temp);
    let registry = registry_of(doc_of(case), &repo);
    let out = temp.join("out");
    let result = run(
        &registry,
        &[
            repo.to_str().expect("utf-8"),
            "-o",
            out.to_str().expect("utf-8"),
        ],
    );
    (result, temp, out)
}

/// ADR-0007's sentence, shipped as data by `reachgraph-core` and reused rather
/// than re-spelled. A renderer that composed its own wording is the failure the
/// constant exists to prevent.
#[test]
fn the_report_uses_the_binding_wording() {
    let (result, _temp, _out) = analyse("unresolved_edge", "wording");

    assert_eq!(result.code, 0, "stderr: {}", result.err);
    assert!(
        result.err.contains(reachgraph_core::UNREACHABLE_CLAIM),
        "stderr: {}",
        result.err
    );
}

/// Plan-05 §8.4's scoping: this crate's own string literals, not the analysed
/// repository's text. A repository that legitimately contains the word in a doc
/// comment must not fail anybody's build.
#[test]
fn this_crate_authors_no_dead_code_wording() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders: Vec<String> = Vec::new();

    for file in crate::guards::rust_sources(&root) {
        let text = fs::read_to_string(&file).expect("a source file is readable");
        for (number, line) in text.lines().enumerate() {
            let lowered = line.to_lowercase();
            if lowered.contains("dead code") || lowered.contains("dead_code") {
                offenders.push(format!("{}:{}", file.display(), number + 1));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "the word `dead` is what design.md §8 forbids this tool from saying: {offenders:?}"
    );
}

/// ADR-0006 makes this binding, and plan-06 §6 places it: after a successful
/// run, adjacent to the output path, where the user is deciding what to do with
/// the directory.
#[test]
fn the_privacy_note_is_printed_after_a_successful_run() {
    let (result, _temp, out) = analyse("minimal", "privacy");

    assert_eq!(result.code, 0, "stderr: {}", result.err);
    let note_at = result
        .err
        .find("structural map")
        .expect("the note is printed");
    let path_at = result
        .err
        .find(out.to_str().expect("utf-8"))
        .expect("the output path is printed");
    assert!(
        note_at > path_at,
        "the note sits beside the path it is about, not above it: {}",
        result.err
    );
    assert!(
        result.err.contains("GitHub Pages"),
        "the one publishing route that leaks a private repository is named: {}",
        result.err
    );
}

/// Plan-06 §5.3: an unbound root is why a real handler may be sitting in the
/// unreachable list, so it is surfaced at the top level rather than only in the
/// artifact.
#[test]
fn the_report_surfaces_unbound_roots() {
    let (result, _temp, out) = analyse("unbound_root", "unbound");

    assert_eq!(result.code, 0, "stderr: {}", result.err);
    assert!(result.err.contains("unbound"), "stderr: {}", result.err);

    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(out.join("run.json")).expect("written"))
            .expect("parses");
    assert!(report["roots_unbound"].as_u64().expect("a count") > 0);
}

/// A large unresolved count means the "not reachable" claim is weaker than it
/// looks, and the reader should see that without opening the artifact.
#[test]
fn the_report_surfaces_unresolved_edge_targets() {
    let (result, _temp, out) = analyse("unresolved_edge", "unresolved");

    assert_eq!(result.code, 0, "stderr: {}", result.err);
    assert!(
        result.err.contains("unresolved edge targets"),
        "stderr: {}",
        result.err
    );

    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(out.join("run.json")).expect("written"))
            .expect("parses");
    assert!(report["unresolved_edge_targets"].as_u64().expect("a count") > 0);
}

/// ADR-1223, behaviour B1. An edge provider that omits calls it cannot resolve
/// — `reachgraph-lang-rust` today — makes every unresolved count unknown, so
/// each surface carries `null`, never a 0 that would read as "nothing missing".
///
/// `versioned_pair` has unreachable nodes and shards and no unresolved edge, so
/// every field below would read 0 or `false` if absence were not carried.
#[test]
fn a_provider_that_omits_unresolved_calls_reports_null_on_every_surface() {
    let (result, _temp, out) = analyse_not_reporting("versioned_pair", "null-surfaces");
    assert_eq!(result.code, 0, "stderr: {}", result.err);

    let read = |path: &std::path::Path| -> serde_json::Value {
        serde_json::from_str(&fs::read_to_string(path).expect("written")).expect("parses")
    };

    let report = read(&out.join("run.json"));
    assert!(
        report["unresolved_edge_targets"].is_null(),
        "run.json: {}",
        report["unresolved_edge_targets"]
    );

    let unreachable = read(&out.join("unreachable.json"));
    assert!(
        unreachable["unresolved_edge_count"].is_null(),
        "unreachable.json: {}",
        unreachable["unresolved_edge_count"]
    );
    let nodes = unreachable["nodes"].as_array().expect("a node list");
    assert!(!nodes.is_empty(), "the case has unreachable nodes to check");
    for node in nodes {
        assert!(
            node["possibly_reachable_via_unresolved"].is_null(),
            "{}: {}",
            node["name"],
            node["possibly_reachable_via_unresolved"]
        );
    }

    let mut shards = 0;
    for entry in fs::read_dir(out.join("graph")).expect("shards written") {
        let shard = read(&entry.expect("an entry").path());
        assert!(
            shard["stats"]["unresolved_edge_count"].is_null(),
            "shard stats: {}",
            shard["stats"]
        );
        shards += 1;
    }
    assert!(shards > 0, "the case writes shards to check");
}

/// ADR-1223, behaviour B2, in Max's wording (2026-11-14). The plugin is named
/// from its own declaration: nothing in the cli matches on a `PluginId`.
#[test]
fn the_report_says_not_reported_and_names_the_plugin() {
    let (result, _temp, _out) = analyse_not_reporting("versioned_pair", "not-reported");
    assert_eq!(result.code, 0, "stderr: {}", result.err);

    let line = result
        .err
        .lines()
        .find(|line| line.contains("unresolved edge targets"))
        .unwrap_or_else(|| panic!("no unresolved line: {}", result.err));

    assert_eq!(
        line.trim(),
        "unresolved edge targets: not reported \
         (reachgraph-lang-rust omits calls it cannot resolve)"
    );
    assert!(
        !line.chars().any(|c| c.is_ascii_digit()),
        "a digit on this line would read as a count: {line}"
    );
}

/// One case, analysed by a double that declares what `reachgraph-lang-rust`
/// declares and carries its plugin id.
fn analyse_not_reporting(
    case: &str,
    label: &str,
) -> (crate::support::Run, TempDir, std::path::PathBuf) {
    let temp = TempDir::new(label);
    let repo = repo_for(case, &temp);
    let mut doc = doc_of(case);
    doc.plugin_id = "reachgraph-lang-rust".to_owned();
    let registry = registry_holding(DetectableFixture::not_reporting_unresolved(
        FixturePlugin::from_doc(&repo, doc),
    ));
    let out = temp.join("out");
    let result = run(
        &registry,
        &[
            repo.to_str().expect("utf-8"),
            "-o",
            out.to_str().expect("utf-8"),
        ],
    );
    (result, temp, out)
}

/// Open question 3, DECIDED: `run.json` lives **inside** `out/`. A reviewer
/// opening a downloaded artifact zip should see the run's coverage without a
/// second file, and plan-07's smoke test asserts a file list that has to
/// include it.
#[test]
fn the_run_record_is_written_inside_the_artifact() {
    let (result, _temp, out) = analyse("versioned_pair", "run-json");

    assert_eq!(result.code, 0, "stderr: {}", result.err);
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(out.join("run.json")).expect("written"))
            .expect("parses");

    assert_eq!(report["roots_total"], 2);
    assert_eq!(report["shards"], 2);
    assert!(report["wall_clock_ms"].is_u64());
    assert_eq!(report["out"], out.to_str().expect("utf-8"));
}
