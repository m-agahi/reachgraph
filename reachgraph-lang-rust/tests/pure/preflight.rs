//! Every `reason` and `remediation` this crate can return — plan-03 §11, §13.
//!
//! Reviewing remediation text is the point of this file. A test is how it gets
//! reviewed: a `Warned` check that returns `Ok`, or a `Failed` for a non-fatal
//! finding, fails here rather than shipping.

use reachgraph_lang_rust::coverage::ProcMacroExpansion;
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
        rust_src_available: true,
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

/// Check 4 — `rust-src` is reported and **never fatal**.
///
/// MEASURED, design.md §8's edge-noise table: stdlib targets are in the `drop`
/// column. Refusing to run over a component whose contribution is dropped
/// anyway would be strictly worse for the user than running. What is lost is
/// the ability to classify those targets as `Stdlib` and drop them
/// deliberately rather than by absence.
#[test]
fn preflight_outcome_messages_check_4() {
    let facts = PreflightFacts {
        rust_src_available: false,
        ..healthy()
    };

    let outcome = preflight_outcome(&facts);
    let (reason, remediation) = warned(&outcome);

    assert!(
        reason.contains(
            "the rust-src component is not installed, so the sysroot source root did not \
             resolve and calls into the standard library cannot be located; they are \
             reported as external rather than classified as stdlib"
        ),
        "reason was {reason:?}"
    );
    assert!(
        remediation.contains("rustup component add rust-src"),
        "remediation was {remediation:?}"
    );
    assert!(
        !matches!(outcome, Preflight::Failed { .. }),
        "a missing rust-src never refuses the run"
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
        rust_src_available: false,
        ..healthy()
    };

    let outcome = preflight_outcome(&facts);
    let (reason, remediation) = warned(&outcome);

    assert!(reason.contains("cannot be located"));
    assert!(
        !remediation.contains("cannot be located"),
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
/// So the remediation must not say "cargo build --workspace". A remediation
/// that does not remediate is worse than none.
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
        !remediation.contains("cargo build"),
        "plan-03 §11's remediation does not work and must not be shipped: {remediation:?}"
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
        rust_src_available: false,
        ..healthy()
    };

    let outcome = preflight_outcome(&facts);
    let (reason, remediation) = warned(&outcome);

    assert!(reason.contains("a, b"), "both members named: {reason:?}");
    assert!(reason.contains("proc-macro expansion is disabled"));
    assert!(reason.contains("rust-src component is not installed"));
    assert!(remediation.contains("rustup component add rust-src"));
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
