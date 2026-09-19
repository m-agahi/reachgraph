//! The repository argument, resolved once before any plugin sees it.
//!
//! # The defect this module exists for
//!
//! MEASURED 2026-09-19 by the release workflow's smoke test, run 35457883578,
//! against the installed wheel on both native runners:
//!
//! ```text
//! thread 'main' panicked at ra_ap_paths-0.0.352/src/lib.rs:97:36:
//! expected absolute path, got .
//! ```
//!
//! `reachgraph .` — the most natural invocation there is — panicked on every
//! platform. `ra_ap_paths::AbsPathBuf::assert` refuses a relative path by
//! panicking rather than by returning an error, and the cli handed the argument
//! through exactly as typed. Only the jobs that EXECUTED the binary saw it;
//! every cross-compiled job passed, which is what identifies the defect as
//! universal rather than platform-specific.
//!
//! The fix belongs here, at the boundary where the path enters, and not at the
//! call site in a plugin: one gate for every entry point beats one `catch` per
//! engine, and a plugin author must not have to know this rule.
//!
//! # `canonicalize`, not `std::path::absolute`
//!
//! Both produce an absolute path, which is all `AbsPathBuf::assert` demands, so
//! the choice is decided by something else: **ADR-0003 makes `node_id` an
//! identity.**
//!
//! `std::path::absolute` is purely lexical. It prepends the current directory
//! and stops — on Unix it leaves `..` in place, deliberately, because removing
//! it would change which directory a path names when a symlink is involved. So
//! `reachgraph .` and `reachgraph nested/..` would reach `ra_ap` as two
//! different absolute strings for one directory. `ProjectWorkspace` takes the
//! spelling it is given, `engine::display_path` renders every emitted path
//! relative to it by `strip_prefix`, and a prefix that does not match makes
//! that call fall back to an absolute path — so one repository would emit two
//! sets of node ids depending on how the user typed its name. That is not a
//! cosmetic difference; it is the identity the whole artifact is keyed on.
//!
//! `canonicalize` asks the operating system instead. `.`, `./`, `nested/..` and
//! a symlink to the root all come back as one path, which is the property
//! wanted. It costs two things, both accepted deliberately:
//!
//! - **It requires the path to exist.** That is a feature here rather than a
//!   cost, because a path that resolves to nothing has to produce reachgraph's
//!   own error anyway. The existence and directory checks below run FIRST, so
//!   the message the user reads is written here and carries a remediation —
//!   `canonicalize`'s own `io::Error` is never what surfaces.
//! - **On Windows it returns a `\\?\` verbatim path.** Stated rather than
//!   hidden. rust-analyzer canonicalises on Windows itself and `AbsPathBuf`
//!   accepts the result, so the risk is small — but it is UNMEASURED here, and
//!   `docs/RELEASING.md` already records that the Windows wheel is tier 2,
//!   cross-compiled and never smoke-tested.
//!
//! # `~` is the shell's, not ours
//!
//! A literal `~` reaches this process only when the shell did not expand it.
//! reachgraph does not expand it either: a tool that did would have to decide
//! what `~someone` means and would disagree with the shell sooner or later.
//! The remediation says which of the two is responsible, because "`~` does not
//! exist" is true and useless to somebody looking at their own home directory.

use std::path::{Path, PathBuf};

/// Why a repository argument could not be used, in the words the user reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoPathError {
    /// What is wrong, naming what was typed.
    pub reason: String,
    /// What to do about it.
    pub remediation: String,
}

/// Resolve the repository argument to one absolute, canonical directory.
///
/// Every entry point that takes a repository goes through here — `analyse` and
/// `preflight` both — so a second entry point cannot quietly skip the gate.
pub fn resolve(named: &Path) -> Result<PathBuf, RepoPathError> {
    let typed = named.display().to_string();

    if !named.exists() {
        return Err(RepoPathError {
            reason: format!("{typed} does not exist"),
            remediation: remediation_for_missing(named),
        });
    }

    if !named.is_dir() {
        return Err(RepoPathError {
            reason: format!("{typed} is not a directory"),
            remediation:
                "reachgraph analyses a repository, so the argument is the directory a marker \
                 file sits in — `reachgraph plugins` lists the markers each plugin looks for"
                    .to_owned(),
        });
    }

    named.canonicalize().map_err(|error| RepoPathError {
        reason: format!("{typed} could not be resolved: {error}"),
        remediation: "check that every directory on the path is readable".to_owned(),
    })
}

/// The one case where "does not exist" needs more than itself.
fn remediation_for_missing(named: &Path) -> String {
    if starts_with_tilde(named) {
        return "your shell expands `~` only when it is unquoted, and reachgraph does not \
                expand it at all — that is the shell's job. Drop the quotes, or pass the \
                directory itself"
            .to_owned();
    }

    "check the spelling, or change into the repository and pass `.`".to_owned()
}

/// Whether the first component is a bare `~` or a `~user`.
fn starts_with_tilde(named: &Path) -> bool {
    named
        .components()
        .next()
        .and_then(|component| component.as_os_str().to_str())
        .is_some_and(|first| first.starts_with('~'))
}
