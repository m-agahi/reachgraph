//! `--read-build-output` — ADR-0009.
//!
//! The flag configures the shipped Rust plugin at construction, so these
//! tests go through `reachgraph_cli::run` (the shipped registry) for the
//! loud-failure case, and through `run_with` for the refusal: a caller that
//! supplies its own registry cannot have the flag applied, and must hear so
//! rather than get a run that silently read nothing.

use std::fs;
use std::path::Path;

use crate::support::{doc_of, registry_of, run, TempDir};

/// A one-package workspace that declares a build script and was never built.
fn unbuilt_workspace(at: &Path) {
    fs::create_dir_all(at.join("src")).expect("src is creatable");
    fs::write(
        at.join("Cargo.toml"),
        "[package]\nname = \"bo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n",
    )
    .expect("the manifest is writable");
    fs::write(at.join("build.rs"), "fn main() {}\n").expect("build.rs is writable");
    fs::write(at.join("src/lib.rs"), "/// A function.\npub fn f() {}\n")
        .expect("lib.rs is writable");
}

fn shipped(args: &[&str]) -> (u8, String) {
    let owned: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let code = {
        let mut streams = reachgraph_cli::Streams {
            out: &mut out,
            err: &mut err,
        };
        reachgraph_cli::run(&owned, &mut streams)
    };
    (code, String::from_utf8(err).expect("the cli writes utf-8"))
}

/// Given the flag, and no build output there: the run fails, says so, and
/// names the path — never an index that looks as though it read something.
#[test]
fn the_flag_without_build_output_fails_naming_the_path() {
    let temp = TempDir::new("build-output-missing");
    let repo = temp.join("repo");
    unbuilt_workspace(&repo);
    let target = repo.join("target");
    let out = temp.join("out");

    let (code, err) = shipped(&[
        repo.to_str().expect("utf-8"),
        "--read-build-output",
        target.to_str().expect("utf-8"),
        "-o",
        out.to_str().expect("utf-8"),
    ]);

    assert_eq!(code, reachgraph_cli::EXIT_PREFLIGHT, "stderr: {err}");
    assert!(
        err.contains(&target.display().to_string()),
        "the missing directory is named: {err}"
    );
    assert!(
        !err.contains("containing Cargo.toml"),
        "the workspace loaded; the remediation is about the flag, not the workspace: {err}"
    );
    assert!(!out.join("endpoints.json").exists(), "nothing was written");
}

/// `preflight` loads the workspace too, so it takes the flag and fails the same
/// way — a preflight that ignored it would check a different load than the run.
#[test]
fn preflight_takes_the_flag_and_fails_the_same_way() {
    let temp = TempDir::new("build-output-preflight");
    let repo = temp.join("repo");
    unbuilt_workspace(&repo);
    let target = repo.join("target");

    let (code, err) = shipped(&[
        "preflight",
        repo.to_str().expect("utf-8"),
        "--read-build-output",
        target.to_str().expect("utf-8"),
    ]);

    assert_eq!(code, reachgraph_cli::EXIT_PREFLIGHT, "stderr: {err}");
    assert!(err.contains(&target.display().to_string()), "{err}");
}

/// A caller-supplied registry was built before the flag was read, so the flag
/// cannot reach a plugin through it. Refused, not ignored.
#[test]
fn a_supplied_registry_refuses_the_flag() {
    let temp = TempDir::new("build-output-run-with");
    let registry = registry_of(doc_of("minimal"), &temp.join("case"));

    let result = run(
        &registry,
        &["somewhere", "--read-build-output", "target", "-o", "out"],
    );

    assert_eq!(result.code, reachgraph_cli::EXIT_USAGE, "{}", result.err);
    assert!(result.err.contains("--read-build-output"), "{}", result.err);
}
