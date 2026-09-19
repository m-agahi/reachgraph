//! Plan-03 §11 check 1a — the probe runs the program and reads what it printed.
//!
//! This is the one guard the plan states as a prohibition rather than as a
//! behaviour: **never `command -v`**. MEASURED, design.md §8 and §10 — on the
//! author's machine `command -v rust-analyzer` succeeds against a `rustup`
//! proxy that loops and is not installed. A name resolving is not a capability.
//!
//! A test that supplied `CargoProbe::DidNotRespond` as data would assert the
//! message and not the mechanism: rewriting the probe into a PATH lookup would
//! leave such a test green. So these cases reproduce the proxy loop instead of
//! describing it, with programs that **resolve, run and prove nothing**.
//!
//! # Why the shims are checked in rather than written here
//!
//! MEASURED as a flake before it was fixed: creating an executable inside a
//! multithreaded test binary that also spawns cargo races with every other
//! thread's fork. The child inherits the open write descriptor for the window
//! between fork and exec, and the later `execve` fails with
//! `ETXTBSY`/"Text file busy". Retrying would have hidden a real race rather
//! than removing it. A file already on disk before the process starts cannot
//! lose that race, so `tests/fixtures/probes/` holds them with their mode bits
//! in git.
//!
//! In the slow suite rather than the pure one because it executes a file;
//! plan-03 §13 Tier A is defined as touching no filesystem.

use std::path::{Path, PathBuf};

use reachgraph_lang_rust::preflight::CargoProbe;
use reachgraph_lang_rust::probe_program;

/// One of the checked-in shims, by name.
fn shim(name: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/probes")
        .join(name);
    assert!(path.exists(), "{} is checked in", path.display());
    path
}

/// Run the probe against a shim, asserting only that it is a path this test
/// controls — never a name resolved from `PATH`.
fn probe_shim(name: &str) -> CargoProbe {
    let path = shim(name);
    probe_program(path.to_str().expect("a UTF-8 repository path"))
}

/// The rustup proxy loop, reproduced: it resolves, it runs, it exits 0, and it
/// is not the toolchain.
#[test]
fn a_program_that_resolves_and_proves_nothing_did_not_respond() {
    let probe = probe_shim("proxy-loop");

    let CargoProbe::DidNotRespond { detail } = &probe else {
        panic!("a name resolving is not a capability, got {probe:?}");
    };
    assert!(
        detail.contains("not a version line"),
        "the probe read the output rather than the name: {detail:?}"
    );
    assert!(
        detail.contains("syncing channel updates"),
        "and it reports what the program actually said: {detail:?}"
    );
}

/// A program that resolves and fails is also not a toolchain, and the reason
/// carries its exit status and its own words rather than a guess.
#[test]
fn a_program_that_resolves_and_fails_did_not_respond() {
    let probe = probe_shim("broken-toolchain");

    let CargoProbe::DidNotRespond { detail } = &probe else {
        panic!("a failing program did not respond, got {probe:?}");
    };
    assert!(detail.contains("exited with"), "{detail:?}");
    assert!(detail.contains("no toolchain installed"), "{detail:?}");
}

/// A name that resolves to nothing at all.
#[test]
fn a_program_that_does_not_exist_did_not_respond() {
    let probe = probe_program("reachgraph-no-such-program-exists");
    assert!(
        matches!(probe, CargoProbe::DidNotRespond { .. }),
        "{probe:?}"
    );
}

/// And the real toolchain responds, so the probe is not hard-wired to refuse.
///
/// ADR-0001's carve-out makes `cargo` a precondition of a Rust repository being
/// analysable at all, so its absence here would mean this suite could not have
/// been built in the first place.
#[test]
fn the_real_toolchain_responds_with_a_version_line() {
    let probe = probe_program("cargo");

    let CargoProbe::Responded { version } = &probe else {
        panic!("cargo built this test, so cargo responds: {probe:?}");
    };
    assert!(version.starts_with("cargo "), "{version:?}");
}
