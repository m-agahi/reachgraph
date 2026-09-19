//! What happens when the repository argument names nothing usable.
//!
//! The companion to `tests/relative_repo_path.rs`, which owns the shapes that
//! need a current directory. These need none, so they are ordinary tests here.
//!
//! All of them are one defect's fallout. MEASURED 2026-09-19, release workflow
//! run 35457883578: the repository path went to the plugins exactly as typed,
//! and `ra_ap_paths::AbsPathBuf::assert` panics on a relative one. Resolving it
//! means touching the filesystem, and a path that resolves to nothing must
//! produce **reachgraph's own error with remediation** — not a panic, and not a
//! bare `io::Error` with the operating system's wording.

use std::fs;

use crate::support::{doc_of, registry_of, repo_for, run, TempDir};

/// `EXIT_USAGE`. The user named something; what they named is wrong, and no
/// analysis was attempted — so this is neither `EXIT_UNDETECTED` (which means
/// a real directory that no plugin claims) nor `EXIT_INTERNAL`.
const EXIT_USAGE: u8 = 4;

/// A path that does not exist is refused by name, with a remediation.
#[test]
fn a_repository_that_does_not_exist_is_reachgraphs_own_error() {
    let temp = TempDir::new("missing");
    let repo = repo_for("minimal", &temp);
    let registry = registry_of(doc_of("minimal"), &repo);
    let missing = temp.join("no-such-directory");

    let result = run(&registry, &[missing.to_str().expect("utf-8")]);

    assert_eq!(result.code, EXIT_USAGE, "stderr: {}", result.err);
    assert!(
        result.err.contains("no-such-directory"),
        "the message names what was typed: {}",
        result.err
    );
    assert!(
        result.err.contains("does not exist"),
        "the message says what is wrong: {}",
        result.err
    );
    assert!(
        !result.err.contains("panicked"),
        "a missing path is a user error, not a crash: {}",
        result.err
    );
}

/// A path that exists and is a file says so, rather than failing later and
/// deeper with a message about a workspace.
#[test]
fn a_repository_that_is_a_file_says_it_is_not_a_directory() {
    let temp = TempDir::new("file");
    let repo = repo_for("minimal", &temp);
    let registry = registry_of(doc_of("minimal"), &repo);
    let file = temp.join("a-file");
    fs::write(&file, b"not a repository").expect("the temporary directory is writable");

    let result = run(&registry, &[file.to_str().expect("utf-8")]);

    assert_eq!(result.code, EXIT_USAGE, "stderr: {}", result.err);
    assert!(
        result.err.contains("not a directory"),
        "the message says what is wrong: {}",
        result.err
    );
}

/// A literal `~` reaches the process only when the shell did NOT expand it —
/// inside quotes, or from a script that built the string itself.
///
/// The remediation says that, because "~ does not exist" is true and useless:
/// the user can see a home directory on their machine and will read the error
/// as a bug. **reachgraph does not expand `~` itself**; that is the shell's
/// job, and a tool that did it would disagree with the shell about `~user`.
#[test]
fn a_literal_tilde_explains_that_the_shell_expands_it() {
    let temp = TempDir::new("tilde");
    let repo = repo_for("minimal", &temp);
    let registry = registry_of(doc_of("minimal"), &repo);

    let result = run(&registry, &["~"]);

    assert_eq!(result.code, EXIT_USAGE, "stderr: {}", result.err);
    assert!(
        result.err.contains("shell"),
        "the remediation names the shell: {}",
        result.err
    );
}

/// `preflight` takes a repository too, and takes it through the same gate.
///
/// A second entry point that skipped the resolution would panic exactly where
/// the first one used to, which is why this is asserted rather than assumed.
#[test]
fn preflight_refuses_a_repository_that_does_not_exist() {
    let temp = TempDir::new("preflight-missing");
    let repo = repo_for("minimal", &temp);
    let registry = registry_of(doc_of("minimal"), &repo);
    let missing = temp.join("absent");

    let result = run(&registry, &["preflight", missing.to_str().expect("utf-8")]);

    assert_eq!(result.code, EXIT_USAGE, "stderr: {}", result.err);
    assert!(
        result.err.contains("does not exist"),
        "stderr: {}",
        result.err
    );
}
