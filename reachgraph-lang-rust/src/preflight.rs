//! `preflight()` — plan-03 §11, ADR-0003 field 5.
//!
//! The outcome is a pure function of facts gathered elsewhere, so every
//! `reason` and every `remediation` in this crate can be read, reviewed and
//! asserted without a toolchain, a workspace or an engine (plan-03 §13 Tier A).
//!
//! # What preflight does not check, and why that is written down
//!
//! It does not look for a `rust-analyzer` binary. There is no external binary
//! under ADR-0001 — the engine is linked in. The check is absent by design, and
//! the reason is recorded so nobody re-adds it as a safety measure: MEASURED,
//! design.md §8 and §10, on the author's machine `command -v rust-analyzer`
//! **succeeds and proves nothing**. It resolves to a `rustup` proxy that loops
//! and is not installed. A name resolving is not a capability.
//!
//! It does not look for a proc-macro server binary either, for the same reason
//! and one more: there is no route to one that ADR-0001 permits (see
//! [`crate::coverage::ProcMacroExpansion`]).

use reachgraph_plugin_api::Preflight;

use crate::coverage::ProcMacroExpansion;

/// Whether `cargo` **responded** — not whether its name resolved.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum CargoProbe {
    /// `cargo --version` ran and printed a version line.
    Responded {
        /// The version line, verbatim.
        version: String,
    },
    /// It did not.
    DidNotRespond {
        /// What went wrong, in the probe's own words.
        detail: String,
    },
}

/// Whether a Cargo workspace was found and loaded at or above the root.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum WorkspaceProbe {
    /// Manifest discovery and the workspace load both succeeded.
    Loaded,
    /// One of them did not.
    NotResolvable {
        /// The directory reachgraph was pointed at.
        root: String,
        /// What went wrong.
        detail: String,
    },
}

/// Everything [`preflight_outcome`] reads.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PreflightFacts {
    /// Check 1a.
    pub cargo: CargoProbe,
    /// Check 1b.
    pub workspace: WorkspaceProbe,
    /// Check 2 — members declaring a build script whose generated code is not
    /// in the crate graph, by package id.
    pub members_with_unindexed_generated_code: Vec<String>,
    /// Check 4.
    pub rust_src_available: bool,
    /// Check 3.
    pub proc_macro_expansion: ProcMacroExpansion,
}

/// Check 1a's remediation.
const CARGO_REMEDIATION: &str = "install a Rust toolchain (https://rustup.rs) and re-run";

/// Check 1b's remediation.
const WORKSPACE_REMEDIATION: &str =
    "point reachgraph at a directory containing Cargo.toml, or at any member of the \
     workspace you want indexed";

/// Check 4's remediation.
const RUST_SRC_REMEDIATION: &str = "rustup component add rust-src";

/// Check 4's finding.
const RUST_SRC_REASON: &str =
    "the rust-src component is not installed, so the sysroot source root did not resolve \
     and calls into the standard library cannot be located; they are reported as external \
     rather than classified as stdlib";

/// Check 3's finding.
const PROC_MACRO_REASON: &str =
    "proc-macro expansion is disabled, so calls that cross an attribute or derive macro — \
     #[tonic::async_trait] among them — are absent from this index, not proven absent from \
     the code";

/// Check 3's remediation.
///
/// It tells the reader what to do with the result rather than what to install,
/// because there is nothing to install. A remediation that named a command the
/// user could run would be the worse failure: advice that does not work, given
/// with the authority of a structured field.
const PROC_MACRO_REMEDIATION: &str =
    "nothing on your side. reachgraph links no proc-macro expander: the only in-process \
     one rust-analyzer publishes is gated behind ra_ap_proc_macro_srv's `in-rust-tree` \
     feature and needs a nightly toolchain with rustc-dev, and every other route spawns a \
     server binary, which ADR-0001 does not permit. Read edges through macros as \
     unmeasured rather than as absent";

/// Check 2's remediation, and it is **not** "build the workspace".
///
/// Plan-03 §11 check 2 specifies `cargo build --workspace once, then re-run
/// reachgraph`. MEASURED 2026-09-19 that the advice does not work: a built
/// workspace's out-dir is still not in the crate graph, because nothing loads
/// it there. Plan-03 §9 says the same thing in words — "a user who follows
/// check 2's remediation, builds the workspace and re-runs, gets the same empty
/// result" — and §11 was written before that measurement.
///
/// A remediation that does not remediate is worse than none. This one states
/// what a reader can actually do: read the gap correctly.
const UNBUILT_REMEDIATION: &str =
    "nothing on your side, and building the workspace does not help — reachgraph does not \
     load generated code into the crate graph at all (plan-03 §9 D-D). Treat calls into \
     generated code from these members as unmeasured rather than as absent";

/// The outcome, from the facts.
///
/// # Why check 2 warns rather than fails
///
/// Plan-03 §11 specifies `Failed` for it, and that specification predates §9's
/// measurement. `Failed` refuses the run, "which is correct when the user can
/// fix it in one command" — §11's own justification, and its premise is now
/// known to be false. §9 D-D then rules that v0.1 **ships** with generated code
/// unindexed and records the fact, which a refused run cannot do.
///
/// So the two halves of plan-03 disagree and the later measurement wins. The
/// instruction that does not bend is the one §14 question 11 states: never
/// emit a `Failed` for a non-fatal finding. A plugin that would have run must
/// not report as one that cannot.
pub fn preflight_outcome(facts: &PreflightFacts) -> Preflight {
    if let CargoProbe::DidNotRespond { detail } = &facts.cargo {
        return Preflight::Failed {
            reason: format!(
                "cargo did not respond: {detail}. reachgraph reads the workspace through \
                 cargo, so a Rust repository cannot be analysed without it."
            ),
            remediation: CARGO_REMEDIATION.to_owned(),
        };
    }

    if let WorkspaceProbe::NotResolvable { root, detail } = &facts.workspace {
        return Preflight::Failed {
            reason: format!("no Cargo workspace at {root}: {detail}"),
            remediation: WORKSPACE_REMEDIATION.to_owned(),
        };
    }

    let mut findings: Vec<(String, &str)> = Vec::new();

    if !facts.members_with_unindexed_generated_code.is_empty() {
        let packages = facts.members_with_unindexed_generated_code.join(", ");
        findings.push((
            format!(
                "{packages} declare a build script whose generated code is not in the index. \
                 reachgraph does not run builds and does not load build-script output, so \
                 generated code — tonic client stubs among it — is not indexed and \
                 cross-repo leaves do not appear in the graph."
            ),
            UNBUILT_REMEDIATION,
        ));
    }

    match facts.proc_macro_expansion {
        ProcMacroExpansion::Disabled => {
            findings.push((PROC_MACRO_REASON.to_owned(), PROC_MACRO_REMEDIATION));
        }
    }

    if !facts.rust_src_available {
        findings.push((RUST_SRC_REASON.to_owned(), RUST_SRC_REMEDIATION));
    }

    if findings.is_empty() {
        return Preflight::Ok;
    }

    // Several findings, one return value. Each keeps its own sentence and its
    // own remediation rather than being fused, which is the shape plan-00 §8
    // question 7 decided for: a warning says what it found AND what to do, and
    // a reader can tell which half is which.
    Preflight::Warned {
        reason: findings
            .iter()
            .map(|(reason, _)| reason.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        remediation: findings
            .iter()
            .map(|(_, remediation)| *remediation)
            .collect::<Vec<_>>()
            .join("\n"),
    }
}
