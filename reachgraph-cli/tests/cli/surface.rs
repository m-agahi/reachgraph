//! Plan-06 §7.1 — the command surface.

use std::fs;

use crate::support::{doc_of, registry_of, repo_for, run, TempDir};

/// Plan-06 §1: a bare path analyses, and ADR-0006's layout lands in `out/`.
#[test]
fn a_bare_path_analyses_to_the_named_out_directory() {
    let temp = TempDir::new("bare");
    let repo = repo_for("versioned_pair", &temp);
    let registry = registry_of(doc_of("versioned_pair"), &repo);
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
    assert!(out.join("endpoints.json").is_file());
    assert!(out.join("unreachable.json").is_file());
    assert!(out.join("versions.json").is_file());
    assert!(out.join("run.json").is_file());
    assert!(
        fs::read_dir(out.join("graph"))
            .expect("the shard directory exists")
            .count()
            > 0,
        "a bound root produces a shard"
    );
}

/// The output goes where `-o` says and nowhere else.
#[test]
fn the_out_flag_redirects_every_written_file() {
    let temp = TempDir::new("redirect");
    let repo = repo_for("minimal", &temp);
    let registry = registry_of(doc_of("minimal"), &repo);
    let out = temp.join("elsewhere");

    let result = run(
        &registry,
        &[
            repo.to_str().expect("utf-8"),
            "--out",
            out.to_str().expect("utf-8"),
        ],
    );

    assert_eq!(result.code, 0, "stderr: {}", result.err);
    assert!(out.join("endpoints.json").is_file());
    assert_eq!(
        fs::read_dir(&repo)
            .expect("the repository is readable")
            .count(),
        1,
        "nothing was written beside the repository"
    );
}

/// Plan-06 §1.1: the output is regenerated, so a directory holding somebody
/// else's files is a hazard rather than a merge.
#[test]
fn a_dirty_out_directory_is_refused_without_force() {
    let temp = TempDir::new("dirty");
    let repo = repo_for("minimal", &temp);
    let registry = registry_of(doc_of("minimal"), &repo);
    let out = temp.join("out");
    fs::create_dir_all(&out).expect("writable");
    fs::write(out.join("thesis.txt"), b"not mine").expect("writable");

    let result = run(
        &registry,
        &[
            repo.to_str().expect("utf-8"),
            "-o",
            out.to_str().expect("utf-8"),
        ],
    );

    assert_eq!(result.code, 4, "stderr: {}", result.err);
    assert!(result.err.contains("--force"), "stderr: {}", result.err);
    assert!(
        !out.join("endpoints.json").exists(),
        "nothing was written, not even partially"
    );
    assert!(
        out.join("thesis.txt").is_file(),
        "the foreign file is untouched"
    );
}

/// `--force` accepts the directory and the foreign file survives: the cli
/// removes the artifact it owns, never the directory.
#[test]
fn force_writes_into_a_dirty_directory_without_deleting_it() {
    let temp = TempDir::new("forced");
    let repo = repo_for("minimal", &temp);
    let registry = registry_of(doc_of("minimal"), &repo);
    let out = temp.join("out");
    fs::create_dir_all(&out).expect("writable");
    fs::write(out.join("thesis.txt"), b"not mine").expect("writable");

    let result = run(
        &registry,
        &[
            repo.to_str().expect("utf-8"),
            "-o",
            out.to_str().expect("utf-8"),
            "--force",
        ],
    );

    assert_eq!(result.code, 0, "stderr: {}", result.err);
    assert!(out.join("endpoints.json").is_file());
    assert!(out.join("thesis.txt").is_file());
}

/// A rerun into its own output directory is the ordinary case and needs no
/// flag — and the stale shards of the previous run do not survive it.
#[test]
fn a_rerun_replaces_its_own_output() {
    let temp = TempDir::new("rerun");
    let repo = repo_for("versioned_pair", &temp);
    let registry = registry_of(doc_of("versioned_pair"), &repo);
    let out = temp.join("out");
    let args = [
        repo.to_str().expect("utf-8"),
        "-o",
        out.to_str().expect("utf-8"),
    ];

    assert_eq!(run(&registry, &args).code, 0);
    let stale = out.join("graph").join("stale-root.json");
    fs::write(&stale, b"{}").expect("writable");

    let second = run(&registry, &args);

    assert_eq!(second.code, 0, "stderr: {}", second.err);
    assert!(
        !stale.exists(),
        "a shard from a root this run does not have is removed, not left to be read"
    );
}

/// Plan-06 §1.2. There is no `--fail-on-unreachable`, and a run that finds
/// unreachable symbols still exits 0 — telling someone to delete working code
/// is the one failure that permanently destroys trust (design.md §8).
#[test]
fn there_is_no_fail_on_unreachable_flag_and_unreachable_symbols_exit_zero() {
    let temp = TempDir::new("unreachable");
    let repo = repo_for("unresolved_edge", &temp);
    let registry = registry_of(doc_of("unresolved_edge"), &repo);
    let out = temp.join("out");

    let rejected = run(
        &registry,
        &[
            repo.to_str().expect("utf-8"),
            "-o",
            out.to_str().expect("utf-8"),
            "--fail-on-unreachable",
        ],
    );
    assert_eq!(rejected.code, 4, "the flag does not exist");

    let accepted = run(
        &registry,
        &[
            repo.to_str().expect("utf-8"),
            "-o",
            out.to_str().expect("utf-8"),
        ],
    );
    assert_eq!(accepted.code, 0, "stderr: {}", accepted.err);
    let report: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(out.join("run.json")).expect("run.json is written"),
    )
    .expect("run.json parses");
    assert!(
        report["unreachable"].as_u64().expect("a count") > 0,
        "the case under test has unreachable symbols, which is what makes the exit code mean something"
    );
}

/// Plan-06 §3.1: zero matches is diagnostic rather than "unsupported".
#[test]
fn no_detected_plugin_exits_3_and_says_what_each_plugin_looks_for() {
    let temp = TempDir::new("undetected");
    let repo = temp.join("empty");
    fs::create_dir_all(&repo).expect("writable");
    let registry = registry_of(doc_of("minimal"), &repo);

    let result = run(
        &registry,
        &[
            repo.to_str().expect("utf-8"),
            "-o",
            temp.join("out").to_str().expect("utf-8"),
        ],
    );

    assert_eq!(result.code, 3, "stderr: {}", result.err);
    assert!(
        result.err.contains("reachgraph.fixture.json"),
        "the marker each plugin looks for is printed: {}",
        result.err
    );
    assert!(
        result.err.contains("json"),
        "so are the extensions it declares: {}",
        result.err
    );
}

/// Plan-06 §5.2: a pipeline consuming stdout must never receive a spinner.
#[test]
fn the_json_report_is_on_stdout_and_progress_is_on_stderr() {
    let temp = TempDir::new("streams");
    let repo = repo_for("minimal", &temp);
    let registry = registry_of(doc_of("minimal"), &repo);

    let result = run(
        &registry,
        &[
            repo.to_str().expect("utf-8"),
            "-o",
            temp.join("out").to_str().expect("utf-8"),
            "--json",
        ],
    );

    assert_eq!(result.code, 0, "stderr: {}", result.err);
    let parsed: serde_json::Value =
        serde_json::from_str(&result.out).expect("stdout holds the report and nothing else");
    assert_eq!(parsed["units"], 1);
    assert!(
        !result.err.is_empty(),
        "progress still goes somewhere, and that somewhere is stderr"
    );
}

/// Without `--json` stdout stays empty: the human report is progress output.
#[test]
fn the_human_report_is_on_stderr_and_stdout_stays_empty() {
    let temp = TempDir::new("human");
    let repo = repo_for("minimal", &temp);
    let registry = registry_of(doc_of("minimal"), &repo);

    let result = run(
        &registry,
        &[
            repo.to_str().expect("utf-8"),
            "-o",
            temp.join("out").to_str().expect("utf-8"),
        ],
    );

    assert_eq!(result.code, 0, "stderr: {}", result.err);
    assert!(result.out.is_empty(), "stdout: {}", result.out);
    assert!(result.err.contains("analysed"), "stderr: {}", result.err);
}

/// `-q` suppresses all but errors, and a successful quiet run says nothing.
#[test]
fn quiet_suppresses_the_report() {
    let temp = TempDir::new("quiet");
    let repo = repo_for("minimal", &temp);
    let registry = registry_of(doc_of("minimal"), &repo);

    let result = run(
        &registry,
        &[
            repo.to_str().expect("utf-8"),
            "-o",
            temp.join("out").to_str().expect("utf-8"),
            "-q",
        ],
    );

    assert_eq!(result.code, 0, "stderr: {}", result.err);
    assert!(result.err.is_empty(), "stderr: {}", result.err);
}

#[test]
fn an_unknown_flag_is_a_usage_error() {
    let temp = TempDir::new("usage");
    let repo = repo_for("minimal", &temp);
    let registry = registry_of(doc_of("minimal"), &repo);

    let result = run(&registry, &[repo.to_str().expect("utf-8"), "--colour"]);

    assert_eq!(result.code, 4);
    assert!(result.err.contains("--colour"), "stderr: {}", result.err);
}

#[test]
fn no_argument_at_all_prints_usage_and_exits_4() {
    let registry = registry_of(doc_of("minimal"), &TempDir::new("none").join("repo"));

    let result = run(&registry, &[]);

    assert_eq!(result.code, 4);
    assert!(result.err.contains("usage"), "stderr: {}", result.err);
}

#[test]
fn help_and_version_exit_zero_on_stdout() {
    let registry = registry_of(doc_of("minimal"), &TempDir::new("help").join("repo"));

    let help = run(&registry, &["--help"]);
    assert_eq!(help.code, 0);
    assert!(help.out.contains("usage"), "stdout: {}", help.out);

    let version = run(&registry, &["--version"]);
    assert_eq!(version.code, 0);
    assert!(
        version.out.contains(env!("CARGO_PKG_VERSION")),
        "stdout: {}",
        version.out
    );
}

/// Plan-06 §1: two tables, not one — and the renderer table is absent rather
/// than empty, because no renderer crate exists yet (plan-05 has not landed).
#[test]
fn the_plugins_subcommand_lists_every_registration_with_its_markers() {
    let registry = registry_of(doc_of("minimal"), &TempDir::new("plugins").join("repo"));

    let result = run(&registry, &["plugins"]);

    assert_eq!(result.code, 0, "stderr: {}", result.err);
    assert!(result.out.contains("fixture"), "stdout: {}", result.out);
    assert!(
        result.out.contains("reachgraph.fixture.json"),
        "the detection marker is what makes the table useful: {}",
        result.out
    );
    assert!(
        result.out.contains("utf8_bytes"),
        "the declared position encoding: {}",
        result.out
    );
    assert!(
        result.out.contains("symbols"),
        "and the declared capabilities: {}",
        result.out
    );
}
