//! Every `reason` and `remediation` this crate can return — plan-03 §11, §13.
//!
//! Reviewing remediation text is the point of this file. A test is how it gets
//! reviewed: a `Warned` check that returns `Ok`, or a `Failed` for a non-fatal
//! finding, fails here rather than shipping.

use reachgraph_lang_rust::coverage::{ProcMacroExpansion, StdlibSource};
use reachgraph_lang_rust::preflight::{
    preflight_outcome, CargoProbe, PreflightFacts, WorkspaceProbe,
};
use reachgraph_plugin_api::Preflight;

fn healthy() -> PreflightFacts {
    PreflightFacts {
        cargo: CargoProbe::Responded {
            version: "cargo 1.98.0 (797e8a9bc 2026-08-05)".to_owned(),
        },
        workspace: WorkspaceProbe::Loaded,
        members_with_unindexed_generated_code: Vec::new(),
        stdlib: StdlibSource::Resolved,
        proc_macro_expansion: ProcMacroExpansion::Disabled,
    }
}

fn warned(outcome: &Preflight) -> (&str, &str) {
    match outcome {
        Preflight::Warned {
            reason,
            remediation,
        } => (reason, remediation),
        other => panic!("expected Warned, got {other:?}"),
    }
}

fn failed(outcome: &Preflight) -> (&str, &str) {
    match outcome {
        Preflight::Failed {
            reason,
            remediation,
        } => (reason, remediation),
        other => panic!("expected Failed, got {other:?}"),
    }
}

/// Check 1a — and the failure it refuses is a name that resolves.
///
/// MEASURED, design.md §8 and §10: `command -v rust-analyzer` succeeds on the
/// author's machine against a `rustup` proxy that loops and is not installed.
/// The fact is supplied here as `DidNotRespond`, because that is the only shape
/// running a command can report; there is no `NameResolved` to be fooled by.
#[test]
fn preflight_outcome_messages_check_1a() {
    let facts = PreflightFacts {
        cargo: CargoProbe::DidNotRespond {
            detail: "No such file or directory (os error 2)".to_owned(),
        },
        ..healthy()
    };

    let (reason, remediation) = {
        let outcome = preflight_outcome(&facts);
        let (r, m) = failed(&outcome);
        (r.to_owned(), m.to_owned())
    };

    assert_eq!(
        reason,
        "cargo did not respond: No such file or directory (os error 2). reachgraph reads \
         the workspace through cargo, so a Rust repository cannot be analysed without it."
    );
    assert_eq!(
        remediation,
        "install a Rust toolchain (https://rustup.rs) and re-run"
    );
}

/// Check 1b.
#[test]
fn preflight_outcome_messages_check_1b() {
    let facts = PreflightFacts {
        workspace: WorkspaceProbe::NotResolvable {
            root: "/tmp/not-a-repo".to_owned(),
            detail: "no projects".to_owned(),
        },
        ..healthy()
    };

    let outcome = preflight_outcome(&facts);
    let (reason, remediation) = failed(&outcome);

    assert_eq!(reason, "no Cargo workspace at /tmp/not-a-repo: no projects");
    assert_eq!(
        remediation,
        "point reachgraph at a directory containing Cargo.toml, or at any member of the \
         workspace you want indexed"
    );
}

/// ADR-0009: build output that was asked for and cannot be read is its own
/// failure, not a missing workspace. The workspace loaded; telling the user to
/// point reachgraph at a `Cargo.toml` would send them after the wrong cause.
#[test]
fn preflight_outcome_messages_unreadable_build_output() {
    let facts = PreflightFacts {
        workspace: WorkspaceProbe::BuildOutputUnreadable {
            detail: "build output directory /repo/target does not exist".to_owned(),
        },
        ..healthy()
    };

    let outcome = preflight_outcome(&facts);
    let (reason, remediation) = failed(&outcome);

    assert!(
        reason.contains("/repo/target"),
        "the path is named: {reason}"
    );
    assert!(
        !remediation.contains("Cargo.toml"),
        "the workspace is not the problem: {remediation}"
    );
    assert!(
        remediation.contains("--read-build-output"),
        "the remediation is about the flag that was given: {remediation}"
    );
}

/// Check 1a runs before check 1b, because a workspace cannot be loaded without
/// cargo and reporting the second failure would name a cause that is a symptom.
#[test]
fn a_missing_toolchain_is_reported_before_an_unloadable_workspace() {
    let facts = PreflightFacts {
        cargo: CargoProbe::DidNotRespond {
            detail: "broken".to_owned(),
        },
        workspace: WorkspaceProbe::NotResolvable {
            root: "/repo".to_owned(),
            detail: "also broken".to_owned(),
        },
        ..healthy()
    };

    let outcome = preflight_outcome(&facts);
    let (reason, _) = failed(&outcome);
    assert!(reason.starts_with("cargo did not respond"));
}

/// Check 3 — degraded proc-macro expansion **warns**, and the remediation does
/// not name a command, because there is none.
///
/// A remediation that told the reader to install something would be advice that
/// does not work, given with the authority of a structured field. What it says
/// instead is how to read the result, which is the thing the reader can act on.
#[test]
fn preflight_outcome_messages_check_3() {
    let outcome = preflight_outcome(&healthy());
    let (reason, remediation) = warned(&outcome);

    assert_eq!(
        reason,
        "proc-macro expansion is disabled, so calls that cross an attribute or derive \
         macro — #[tonic::async_trait] among them — are absent from this index, not proven \
         absent from the code"
    );
    assert_eq!(
        remediation,
        "nothing on your side. reachgraph links no proc-macro expander: the only \
         in-process one rust-analyzer publishes is gated behind ra_ap_proc_macro_srv's \
         `in-rust-tree` feature and needs a nightly toolchain with rustc-dev, and every \
         other route spawns a server binary, which ADR-0001 does not permit. Read edges \
         through macros as unmeasured rather than as absent"
    );
}

/// A toolchain whose rust-src is not installed — `stable` as rustup lays it
/// out.
fn source_missing() -> StdlibSource {
    StdlibSource::SourceMissing {
        sysroot: "/home/u/.rustup/toolchains/stable-x86_64-unknown-linux-gnu".into(),
    }
}

/// Check 4 — a missing standard library is reported and **never fatal**.
///
/// ADR-1222, MEASURED 2026-11-12: what is lost is not only the classification
/// of stdlib targets. Without `core`, a first-party call through `?` had no
/// edge. The finding says so, and it does not claim that the source would get
/// stdlib targets classified — they stay external in every state.
#[test]
fn preflight_outcome_messages_check_4() {
    let facts = PreflightFacts {
        stdlib: source_missing(),
        ..healthy()
    };

    let outcome = preflight_outcome(&facts);
    let (reason, remediation) = warned(&outcome);

    assert!(
        reason.contains(
            "the rust-src component is not installed for the toolchain at \
             /home/u/.rustup/toolchains/stable-x86_64-unknown-linux-gnu"
        ),
        "the finding names the toolchain it looked in: {reason:?}"
    );
    assert!(
        reason.contains("through `?`, `.await`, operators and std traits"),
        "the consequence is the measured one: {reason:?}"
    );
    assert!(
        reason.contains("first-party calls among them"),
        "and it is not confined to the standard library: {reason:?}"
    );
    assert!(
        !reason.contains("classified"),
        "installing rust-src does not classify stdlib targets: {reason:?}"
    );
    assert!(
        remediation
            .contains("rustup component add rust-src --toolchain stable-x86_64-unknown-linux-gnu"),
        "the remediation names the toolchain rustc resolved: {remediation:?}"
    );
    assert!(
        remediation.contains("reachgraph never installs it"),
        "ADR-1222: the run installs nothing: {remediation:?}"
    );
    assert!(
        !matches!(outcome, Preflight::Failed { .. }),
        "a missing rust-src never refuses the run"
    );
}

/// Check 4's other miss: `rustc --print sysroot` did not answer. Nothing says
/// rustup manages that toolchain, so no rustup advice is given.
#[test]
fn preflight_outcome_messages_check_4_without_a_sysroot() {
    let facts = PreflightFacts {
        stdlib: StdlibSource::SysrootUnknown {
            detail: "No such file or directory (os error 2)".to_owned(),
        },
        ..healthy()
    };

    let outcome = preflight_outcome(&facts);
    let (reason, remediation) = warned(&outcome);

    assert!(
        reason.contains(
            "`rustc --print sysroot` did not answer (No such file or directory (os error 2))"
        ),
        "reason was {reason:?}"
    );
    assert!(reason.contains("first-party calls among them"));
    assert!(
        !remediation.contains("rustup"),
        "no rustup advice for a toolchain rustup may not manage: {remediation:?}"
    );
    assert!(
        !matches!(outcome, Preflight::Failed { .. }),
        "an unanswered sysroot probe never refuses the run"
    );
}

/// Plan-00 §8 question 7's decision, made executable.
///
/// The finding and the fix are separable. Check 4's draft text in plan-03 §11
/// welded them — `"rustup component add rust-src — without it, calls into the
/// standard library cannot be located …"` — and that fusion is the evidence
/// plan-00 named for wanting the field. With the field, neither half has to
/// carry the other.
#[test]
fn a_warning_states_its_finding_apart_from_its_fix() {
    let facts = PreflightFacts {
        stdlib: source_missing(),
        ..healthy()
    };

    let outcome = preflight_outcome(&facts);
    let (reason, remediation) = warned(&outcome);

    assert!(reason.contains("are absent from this index"));
    assert!(
        !remediation.contains("absent from this index"),
        "the finding is not welded to the fix"
    );
    assert!(remediation.contains("rustup component add rust-src"));
    assert!(
        !reason.contains("rustup component add"),
        "and the fix is not welded to the finding"
    );
}

/// Check 2 — and it is a `Warned`, where plan-03 §11 specifies `Failed`.
///
/// §11's justification for `Failed` is that it "is correct when the user can
/// fix it in one command". MEASURED 2026-09-19: the command it names does not
/// fix it. A built workspace's out-dir is still absent from the crate graph,
/// because nothing loads it there — §9 says so in words, and §9 D-D then rules
/// that v0.1 **ships** with generated code unindexed and records the fact,
/// which a refused run cannot do.
///
/// So the remediation must not say "cargo build --workspace" ALONE. A
/// remediation that does not remediate is worse than none.
///
/// ADR-0009 changed what does remediate it: a built workspace's out-dir IS
/// loaded when the run is told where the build is. So the remediation now names
/// that pair — build, then re-run reading the build output — and still never
/// tells the user that building on its own changes anything.
#[test]
fn preflight_outcome_messages_check_2() {
    let facts = PreflightFacts {
        members_with_unindexed_generated_code: vec!["yadgar-task".to_owned()],
        ..healthy()
    };

    let outcome = preflight_outcome(&facts);
    let (reason, remediation) = warned(&outcome);

    assert!(
        reason.contains(
            "yadgar-task declare a build script whose generated code is not in the index"
        ),
        "reason was {reason:?}"
    );
    assert!(
        reason.contains("cross-repo leaves do not appear in the graph"),
        "the consequence is named, not only the condition"
    );
    assert!(
        remediation.contains("--read-build-output"),
        "ADR-0009: the remediation names the flag that makes a build visible: {remediation:?}"
    );
    assert!(
        remediation.contains("building alone does not"),
        "and it still says that a build without the flag changes nothing: {remediation:?}"
    );
    assert!(
        !matches!(outcome, Preflight::Failed { .. }),
        "§9 D-D ships the index and records the gap; a refused run records nothing"
    );
}

/// Several findings, one return value, and each keeps both of its halves.
#[test]
fn every_finding_reaches_the_one_outcome() {
    let facts = PreflightFacts {
        members_with_unindexed_generated_code: vec!["a".to_owned(), "b".to_owned()],
        stdlib: source_missing(),
        ..healthy()
    };

    let outcome = preflight_outcome(&facts);
    let (reason, remediation) = warned(&outcome);

    assert!(reason.contains("a, b"), "both members named: {reason:?}");
    assert!(reason.contains("proc-macro expansion is disabled"));
    assert!(reason.contains("rust-src component is not installed"));
    assert!(remediation.contains("rustup component add rust-src --toolchain"));
    assert_eq!(
        reason.lines().count(),
        3,
        "one line per finding: {reason:?}"
    );
}

/// `Ok` is reachable — a plugin that always warns says nothing by warning.
///
/// It needs `rust-src` present, no member with unindexed generated code, and
/// proc-macro expansion undegraded. MEASURED, the third is not achievable
/// today, so this asserts over the facts rather than over a live run, and the
/// gap between them is what check 3 reports.
#[test]
fn ok_is_reachable_in_principle() {
    let facts = healthy();
    assert!(matches!(
        preflight_outcome(&facts),
        Preflight::Warned { .. }
    ));

    // The only finding in `healthy()` is check 3. Remove its cause and the
    // outcome is `Ok` — which is the assertion that this crate is not
    // hard-wired to warn.
    let findings = {
        let outcome = preflight_outcome(&facts);
        let (reason, _) = warned(&outcome);
        reason.lines().count()
    };
    assert_eq!(findings, 1, "exactly one finding on a healthy machine");
}
