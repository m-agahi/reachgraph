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
//! describing it, with a program that **resolves, runs, exits 0 and proves
//! nothing**.
//!
//! In the slow suite rather than the pure one because it writes and executes a
//! file; plan-03 §13 Tier A is defined as touching no filesystem.

use std::io::Write;
use std::path::PathBuf;

use reachgraph_lang_rust::preflight::CargoProbe;
use reachgraph_lang_rust::probe_program;

/// An executable that resolves and says something useless.
fn shim(name: &str, body: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("reachgraph-probe-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("the shim directory is creatable");
    let path = dir.join(name);
    let mut file = std::fs::File::create(&path).expect("the shim is creatable");
    writeln!(file, "#!/bin/sh").expect("write");
    write!(file, "{body}").expect("write");
    drop(file);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&path)
            .expect("the shim exists")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).expect("the shim is executable");
    }

    path
}

/// The rustup proxy loop, reproduced: it resolves, it runs, it exits 0, and it
/// is not the toolchain.
#[test]
fn a_program_that_resolves_and_proves_nothing_did_not_respond() {
    let path = shim("proxy", "echo 'info: syncing channel updates'\nexit 0\n");

    let probe = probe_program(path.to_str().expect("a UTF-8 temp path"));

    let CargoProbe::DidNotRespond { detail } = &probe else {
        panic!("a name resolving is not a capability, got {probe:?}");
    };
    assert!(
        detail.contains("not a version line"),
        "the probe read the output rather than the name: {detail:?}"
    );
}

/// A program that resolves and fails is also not a toolchain, and the reason
/// carries its exit status rather than a guess.
#[test]
fn a_program_that_resolves_and_fails_did_not_respond() {
    let path = shim("broken", "echo 'no toolchain' >&2\nexit 3\n");

    let probe = probe_program(path.to_str().expect("a UTF-8 temp path"));

    let CargoProbe::DidNotRespond { detail } = &probe else {
        panic!("a failing program did not respond, got {probe:?}");
    };
    assert!(detail.contains("exited with"), "{detail:?}");
    assert!(detail.contains("no toolchain"), "{detail:?}");
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
/// analysable at all, so its absence here would mean the suite could not have
/// built in the first place.
#[test]
fn the_real_toolchain_responds_with_a_version_line() {
    let probe = probe_program("cargo");

    let CargoProbe::Responded { version } = &probe else {
        panic!("cargo built this test, so cargo responds: {probe:?}");
    };
    assert!(version.starts_with("cargo "), "{version:?}");
}
