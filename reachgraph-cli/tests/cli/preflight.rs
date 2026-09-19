//! Plan-06 §7.3 — preflight, and how a warning that never clears is presented.

use std::fs;

use crate::support::{doc_of, registry_of, repo_for, run, TempDir};

/// Plan-06 §4: failure is fatal and nothing is analysed. design.md §8's second
/// hard prerequisite changes the *content* of the answer, so proceeding past it
/// produces a graph that is quietly wrong.
#[test]
fn a_failed_preflight_exits_2_and_writes_nothing() {
    let temp = TempDir::new("preflight-fails");
    let repo = repo_for("preflight_fails", &temp);
    let registry = registry_of(doc_of("preflight_fails"), &repo);
    let out = temp.join("out");

    let result = run(
        &registry,
        &[
            repo.to_str().expect("utf-8"),
            "-o",
            out.to_str().expect("utf-8"),
        ],
    );

    assert_eq!(result.code, 2, "stderr: {}", result.err);
    assert!(!out.exists(), "the output directory was not even created");
}

/// ADR-0003 field 5: a check that can fail without saying what to do about it
/// is not finished. Both halves are rendered.
#[test]
fn a_failed_preflight_prints_the_reason_and_the_remediation() {
    let temp = TempDir::new("preflight-text");
    let repo = repo_for("preflight_fails", &temp);
    let doc = doc_of("preflight_fails");
    let registry = registry_of(doc, &repo);

    let result = run(
        &registry,
        &[
            repo.to_str().expect("utf-8"),
            "-o",
            temp.join("out").to_str().expect("utf-8"),
        ],
    );

    assert_eq!(result.code, 2);
    let declared = doc_of("preflight_fails");
    let (reason, remediation) = match declared.preflight {
        reachgraph_fixture::format::FixturePreflight::Failed(failure) => {
            (failure.reason, failure.remediation)
        }
        reachgraph_fixture::format::FixturePreflight::Ok => {
            panic!("the case under test declares a failing preflight")
        }
    };
    assert!(result.err.contains(&reason), "stderr: {}", result.err);
    assert!(result.err.contains(&remediation), "stderr: {}", result.err);
}

/// `reachgraph preflight <repo>` gates cheaply: same checks, no analysis.
#[test]
fn the_preflight_subcommand_analyses_nothing() {
    let temp = TempDir::new("preflight-only");
    let repo = repo_for("minimal", &temp);
    let registry = registry_of(doc_of("minimal"), &repo);

    let result = run(&registry, &["preflight", repo.to_str().expect("utf-8")]);

    assert_eq!(result.code, 0, "stderr: {}", result.err);
    assert!(!temp.join("out").exists(), "no artifact was written");
    assert!(result.err.contains("preflight"), "stderr: {}", result.err);
}

#[test]
fn the_preflight_subcommand_exits_2_on_a_failing_check() {
    let temp = TempDir::new("preflight-only-fails");
    let repo = repo_for("preflight_fails", &temp);
    let registry = registry_of(doc_of("preflight_fails"), &repo);

    let result = run(&registry, &["preflight", repo.to_str().expect("utf-8")]);

    assert_eq!(result.code, 2, "stderr: {}", result.err);
}

/// `--json` emits the same table as structured data, one entry per analysis
/// plugin. No renderer appears: `Renderer` has no `preflight` method, and an
/// "ok" row for one would be a vacuous check reported as a passing one
/// (plan-06 §4).
#[test]
fn preflight_json_is_machine_readable_with_one_entry_per_plugin() {
    let temp = TempDir::new("preflight-json");
    let repo = repo_for("minimal", &temp);
    let registry = registry_of(doc_of("minimal"), &repo);

    let result = run(
        &registry,
        &["preflight", repo.to_str().expect("utf-8"), "--json"],
    );

    assert_eq!(result.code, 0, "stderr: {}", result.err);
    let parsed: serde_json::Value = serde_json::from_str(&result.out).expect("stdout is json");
    let checks = parsed["checks"].as_array().expect("an array of checks");
    assert_eq!(checks.len(), 1, "one entry per analysis plugin");
    assert_eq!(checks[0]["plugin"], "fixture");
    assert_eq!(checks[0]["outcome"], "ok");
}

/// **The never-clearing warning, plan-06 problem 3.**
///
/// ADR-0728 disables proc-macro expansion unconditionally, so the Rust plugin
/// returns `Warned` on every run for ever. A row labelled `WARN` that never
/// goes away trains a reader to ignore warnings, and suppressing it at the
/// source would be a lie.
///
/// So the cli does not call it a warning. A `Warned` plugin **runs** and its
/// finding is a property of the artifact rather than a transient complaint, so
/// it is reported as a limit of this run, in a section headed as one — and, by
/// the channel this pull request adds, it is the plugin's own notes that make
/// the same fact permanent inside the artifact.
#[test]
fn a_warned_plugin_is_reported_as_a_limit_rather_than_a_warning() {
    let temp = TempDir::new("warned");
    let repo = repo_for("minimal", &temp);
    let mut doc = doc_of("minimal");
    doc.notes = vec!["proc-macro expansion is disabled in this index".to_owned()];
    let registry = registry_of(doc, &repo);

    let result = run(
        &registry,
        &[
            repo.to_str().expect("utf-8"),
            "-o",
            temp.join("out").to_str().expect("utf-8"),
        ],
    );

    assert_eq!(result.code, 0, "stderr: {}", result.err);
    assert!(
        result.err.contains("limits of this run"),
        "the section is headed as what it is: {}",
        result.err
    );
    assert!(
        result.err.contains("proc-macro expansion is disabled"),
        "stderr: {}",
        result.err
    );
    assert!(
        !result.err.to_lowercase().contains("warning:"),
        "a finding that can never clear is not spelled as a warning: {}",
        result.err
    );
}

/// The same finding reaches the artifact, which is the half a console line
/// cannot do. A reader opening a downloaded zip sees what the run could not
/// see, without the terminal it was produced in.
#[test]
fn a_plugin_note_is_written_into_the_artifact() {
    let temp = TempDir::new("notes-artifact");
    let repo = repo_for("minimal", &temp);
    let mut doc = doc_of("minimal");
    doc.notes = vec!["generated code was not indexed for 1 of 2 members".to_owned()];
    let registry = registry_of(doc, &repo);
    let out = temp.join("out");

    let result = run(
        &registry,
        &[
            repo.to_str().expect("utf-8"),
            "-o",
            out.to_str().expect("utf-8"),
        ],
    );
    assert_eq!(result.code, 0, "stderr: {}", result.err);

    let endpoints: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(out.join("endpoints.json")).expect("written"))
            .expect("parses");
    assert_eq!(
        endpoints["coverage"]["notes"][0],
        "generated code was not indexed for 1 of 2 members"
    );
}

/// Under `--json` the table is the caller's to render. Printing the human form
/// beside it would put the same facts on two streams, and the analyse path
/// gates its own table for the same reason.
#[test]
fn a_json_preflight_writes_nothing_to_stderr() {
    let temp = TempDir::new("preflight-json-only");
    let repo = repo_for("minimal", &temp);
    let registry = registry_of(doc_of("minimal"), &repo);

    let result = run(
        &registry,
        &["preflight", repo.to_str().expect("utf-8"), "--json"],
    );

    assert_eq!(result.code, 0);
    assert!(result.err.is_empty(), "stderr: {}", result.err);
}
