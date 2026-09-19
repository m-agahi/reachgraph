//! Plan-06 §7.5 — the run report and the privacy note.

use std::fs;

use crate::support::{doc_of, registry_of, repo_for, run, TempDir};

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
