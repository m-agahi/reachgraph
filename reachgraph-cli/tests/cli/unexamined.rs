//! One unparseable `.proto` must not abort the run — ADR-0743.
//!
//! # Why this test is here rather than in `reachgraph-roots-proto-tonic`
//!
//! The roots crate can assert that its own `coverage()` names a file it could
//! not read. It cannot assert the thing a user is actually affected by: that
//! the process writes an artifact, exits zero, and says in that artifact which
//! contract was skipped. Those three facts live at three different layers — the
//! provider, the waist's aggregation, and the emitted JSON — and a test that
//! stops at the first of them passes while the other two drop the fact on the
//! floor.
//!
//! So this drives the **real** `ProtoTonicPlugin` through the **real** cli and
//! reads the **emitted file** back. The fixture plugin is registered beside it
//! only to supply the symbols, edges and classification the proto plugin does
//! not: it provides roots and nothing else, and a repository with no symbol
//! provider is not the configuration under test. The case hands over its own
//! roots view as well, because a registration must hand over every capability
//! its plugin declares.
//!
//! MEASURED 2026-09-19, release.yaml run 35459855283: the smoke test ran the
//! installed binary against reachgraph's own repository and both native-runner
//! jobs failed with `cannot parse .../broken.proto`. reachgraph could not
//! analyse itself, and neither could any repository holding a partial or
//! vendored-sample contract.

use std::fs;
use std::path::{Path, PathBuf};

use reachgraph_fixture::{FixturePlugin, FIXTURE_DOCUMENT_NAME};
use reachgraph_plugin_api::{Registration, Registry};
use reachgraph_roots_proto_tonic::ProtoTonicPlugin;
use serde_json::Value;

use crate::support::{doc_of, fixtures_dir, run, DetectableFixture, Run, TempDir};

/// The same truncated contract the release smoke test tripped over, written
/// into a temporary repository rather than pointed at in this workspace: the
/// test must fail when the mechanism regresses, not when somebody moves a
/// fixture.
const TRUNCATED: &str =
    "syntax = \"proto3\";\n\npackage acme.broken.v1;\n\nservice Incomplete {\n  rpc Missing(\n";

/// A whole contract, so the run has something to succeed at. A test in which
/// the only `.proto` is unreadable cannot tell "recorded and continued" from
/// "skipped everything and continued".
const WHOLE: &str = "syntax = \"proto3\";\n\npackage acme.whole.v1;\n\nservice Whole {\n  rpc Ping(PingRequest) returns (PingReply);\n}\n\nmessage PingRequest {}\nmessage PingReply {}\n";

/// A repository both plugins detect: the fixture's marker document, the
/// `Cargo.toml` the proto plugin looks for, and two contracts.
fn repo_with_contracts(temp: &TempDir) -> PathBuf {
    let repo = temp.join("repo");
    let proto = repo.join("proto");
    fs::create_dir_all(&proto).expect("the temporary directory is writable");

    let source = fixtures_dir().join("minimal").join(FIXTURE_DOCUMENT_NAME);
    fs::copy(source, repo.join(FIXTURE_DOCUMENT_NAME)).expect("the case is readable");
    fs::write(repo.join("Cargo.toml"), "[package]\nname = \"stand-in\"\n")
        .expect("the temporary directory is writable");
    fs::write(proto.join("broken.proto"), TRUNCATED).expect("the temporary directory is writable");
    fs::write(proto.join("whole.proto"), WHOLE).expect("the temporary directory is writable");
    repo
}

fn registry_for(repo: &Path) -> Registry {
    let mut registry = Registry::new();
    registry
        .register(
            Registration::of(DetectableFixture::new(FixturePlugin::from_doc(
                repo.to_path_buf(),
                doc_of("minimal"),
            )))
            .symbols()
            .edges()
            .roots()
            .classifier(),
        )
        .expect("the case declares every capability it hands over");
    registry
        .register(Registration::of(ProtoTonicPlugin::new()).roots())
        .expect("the roots plugin hands over exactly one view");
    registry
}

fn analyse(label: &str) -> (Run, TempDir, PathBuf) {
    let temp = TempDir::new(label);
    let repo = repo_with_contracts(&temp);
    let registry = registry_for(&repo);
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

fn coverage_of(out: &Path) -> Value {
    let text = fs::read_to_string(out.join("endpoints.json")).expect("the artifact was written");
    let document: Value = serde_json::from_str(&text).expect("the artifact is json");
    document["coverage"].clone()
}

/// The defect, at the layer the user meets it: exit code and artifact.
///
/// Not a log line. A warning on stderr scrolls away, and the artifact is what a
/// reader still has in a week — ADR-0007 requirement 1 is about the file.
#[test]
fn a_broken_contract_leaves_an_artifact_and_a_zero_exit() {
    let (result, _temp, out) = analyse("unexamined-exit");

    assert_eq!(
        result.code, 0,
        "one unreadable contract is not a reason to give the user nothing; stderr: {}",
        result.err
    );
    assert!(
        out.join("endpoints.json").exists(),
        "the artifact is written"
    );
}

/// What the artifact says about the file that was skipped.
///
/// The parser's own message is carried through, because "could not be read" is
/// not actionable and "expected 'stream' or a type name" is.
#[test]
fn the_artifact_names_the_unexamined_contract_and_the_parsers_reason() {
    let (result, _temp, out) = analyse("unexamined-named");
    assert_eq!(result.code, 0, "stderr: {}", result.err);

    let coverage = coverage_of(&out);
    let unexamined = coverage["unexamined_contracts"]
        .as_array()
        .expect("the artifact carries the list")
        .clone();

    assert_eq!(unexamined.len(), 1, "{unexamined:?}");
    assert_eq!(unexamined[0]["contract"], "proto/broken.proto");
    let reason = unexamined[0]["reason"]
        .as_str()
        .expect("the reason is a string");
    assert!(
        reason.contains("expected") && reason.contains("end of file"),
        "the parser's own words, not a summary of them: {reason}"
    );
}

/// `partial` is the flag a consumer keys on to weaken every unreachability
/// claim, and an unread contract may hold roots nobody bound — ADR-0007's
/// false-positive class exactly.
#[test]
fn an_unexamined_contract_makes_the_index_partial() {
    let (result, _temp, out) = analyse("unexamined-partial");
    assert_eq!(result.code, 0, "stderr: {}", result.err);

    assert_eq!(
        coverage_of(&out)["partial"],
        Value::Bool(true),
        "a contract nobody read may hold roots nobody bound"
    );
}

/// Recorded, not skipped: the run keeps every root it could read.
///
/// This is the half that makes recording better than aborting. Aborting gave
/// the user nothing; skipping silently would give them an index that looks
/// complete. Continuing gives them `whole.proto`'s root **and** the statement
/// that `broken.proto` was not read.
#[test]
fn the_run_keeps_the_contracts_it_could_read() {
    let (result, _temp, out) = analyse("unexamined-kept");
    assert_eq!(result.code, 0, "stderr: {}", result.err);

    let coverage = coverage_of(&out);
    let contracts: Vec<&str> = coverage["contracts"]
        .as_array()
        .expect("the artifact carries the list")
        .iter()
        .filter_map(Value::as_str)
        .collect();

    assert!(
        contracts.contains(&"proto/whole.proto"),
        "the readable contract is covered: {contracts:?}"
    );
    assert!(
        !contracts.contains(&"proto/broken.proto"),
        "a file nobody read is not a file that was examined: {contracts:?}"
    );
}

/// The stderr report, because a user who runs the binary does not open
/// `endpoints.json`.
///
/// The artifact is the record and this is the pointer to it. A run that says
/// "coverage: 2 contracts" and nothing else invites the reader to believe two
/// is all there were.
#[test]
fn the_run_report_says_a_contract_was_not_read() {
    let (result, _temp, out) = analyse("unexamined-report");
    assert_eq!(result.code, 0, "stderr: {}", result.err);

    assert!(
        result.err.contains("1 not read"),
        "the human report names the shortfall: {}",
        result.err
    );

    let text = fs::read_to_string(out.join("run.json")).expect("the report was written");
    let report: Value = serde_json::from_str(&text).expect("run.json is json");
    assert_eq!(report["contracts_unexamined"], 1);
}
