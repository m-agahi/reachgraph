//! What the run could not see — plan-03 §11's run record.
//!
//! # This type is on the wrong crate, and that is the honest place for it
//!
//! Plan-03 §11 specifies these facts as a **run record** carried into the
//! artifact, beside the workspace-level statement that names what is missing.
//! MEASURED 2026-09-19: no channel in the shipped contract carries any of it.
//! `Preflight` is a per-plugin return value rather than a per-unit record, and
//! `IndexCoverage` holds root and unit coverage rather than plugin findings.
//!
//! Widening `IndexCoverage` to fit is the wrong fix and not merely a large one.
//! `out_dir_loaded`, `rust_src_available` and `proc_macro_expansion` are Rust
//! vocabulary; a waist field only one language can fill is the ADR-0003
//! violation the fixture plugin exists to catch.
//!
//! So the facts live here, on this crate's own surface, and plan-00 §8
//! question 8 records that they do not reach the artifact in v0.1. **That is a
//! gap, not a deferral**: plan-03 §9 D-D rules that generated code goes
//! unindexed *and the artifact says so*, and the second half has nowhere to be
//! said. A consumer reading today's artifact cannot tell *not indexed* from
//! *not called*, which is the distinction D-D exists to preserve.

/// Whether proc macros were expanded, and how.
///
/// # Why there is one variant
///
/// Plan-03 §11 names three values — `in_process`, `disabled`,
/// `unavailable_not_built`. Only one of them is reachable, MEASURED
/// 2026-09-19 against `ra_ap_proc_macro_srv` 0.0.352 on the pinned stable
/// toolchain:
///
/// - `src/lib.rs:11` is `#![cfg(feature = "in-rust-tree")]`, so **without that
///   feature the crate exports nothing at all** — `ProcMacroSrv` does not
///   exist, and a reference to it is `E0425: cannot find type`.
/// - With the feature the crate is `#![feature(proc_macro_internals,
///   proc_macro_diagnostic, proc_macro_span, rustc_private)]` and
///   `extern crate rustc_codegen_ssa / rustc_driver / rustc_interface /
///   rustc_lexer / rustc_metadata / rustc_proc_macro / rustc_span`, which is
///   `E0463: can't find crate` on stable 1.98.0 and needs a nightly toolchain
///   plus `rustc-dev` and `llvm-tools-preview`.
///
/// So `in_process` is not a state this crate can be in. `unavailable_not_built`
/// only distinguished a missing dylib *from* in-process expansion, so it has
/// nothing left to distinguish. Carrying either as a variant nothing can
/// produce would be a value that claims more than the plugin knows — the
/// failure ADR-0003's honest-absence rule names, wearing the opposite costume.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProcMacroExpansion {
    /// No expansion. Calls that cross an attribute or derive macro are absent
    /// from the index rather than proven absent from the code.
    Disabled,
}

/// Which mechanism put generated code into the crate graph.
///
/// # Why there is one variant
///
/// Plan-03 §11 names three — `build_script_data`, `extra_includes`, `none` —
/// and deliberately makes this not a boolean, because §9 MEASURED that an
/// out-dir present on disk is still unindexed when nothing loaded it.
///
/// MEASURED 2026-09-19, plan-03 §14 question 9a answered in two stages against
/// a built single-member fixture with a build script:
///
/// 1. `CargoConfig::extra_includes` **does** make the generated file
///    VFS-resident — `vfs.file_id(OUT_DIR/gen.rs)` returns a `FileId`, not
///    excluded, where without it the same lookup returns `None`.
/// 2. It does **not** make a call into that file resolve.
///    `outgoing_calls` on the calling function returns `Some(0)` either way,
///    and `goto_definition` on the call site returns zero targets — so it is
///    name resolution that fails, not the call hierarchy. Injecting `OUT_DIR`
///    through `load_workspace`'s `extra_env` as well does not change it.
///
/// A file in the VFS that belongs to no crate is located, not indexed.
/// Reporting `extra_includes` here on the strength of stage 1 would claim
/// generated code is in the index while zero generated symbols and zero
/// generated edges exist, which is the misleading-completeness failure §9 D-D
/// exists to prevent. `build_script_data` requires running a build, which
/// ADR-0001 forbids. One value is reachable, so one variant exists.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OutDirMechanism {
    /// Nothing loaded generated code into the crate graph.
    Unloaded,
}

/// What one workspace member's prerequisites looked like.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MemberCoverage {
    /// The member's package id.
    pub package: String,
    /// The member declares a `build.rs`.
    pub has_build_script: bool,
    /// Generated output was **loaded into the crate graph** for this member —
    /// not merely present on disk. See [`OutDirMechanism`].
    pub out_dir_loaded: bool,
    /// A `target/<profile>/build/<pkg>-<hash>/out/` directory holding at least
    /// one `.rs` file exists on disk for this member.
    ///
    /// Separate from `out_dir_loaded` on purpose. The two differ in exactly the
    /// case plan-03 §9 says must not be softened: the artifacts are there and
    /// reachgraph never told the crate graph to look.
    pub out_dir_on_disk: bool,
}

/// What the whole run could not see.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RustCoverage {
    /// One row per workspace member.
    pub members: Vec<MemberCoverage>,
    /// The sysroot source root resolved, so stdlib targets are locatable and
    /// classifiable. False means every stdlib call target is **external** in
    /// plan-01 §7.0's sense.
    pub rust_src_available: bool,
    /// See [`ProcMacroExpansion`].
    pub proc_macro_expansion: ProcMacroExpansion,
    /// See [`OutDirMechanism`].
    pub out_dir_mechanism: OutDirMechanism,
}

impl RustCoverage {
    /// Members that declare a build script whose generated code is not indexed.
    pub fn members_with_unindexed_generated_code(&self) -> Vec<&MemberCoverage> {
        self.members
            .iter()
            .filter(|member| member.has_build_script && !member.out_dir_loaded)
            .collect()
    }

    /// Plan-03 §9 D-D's workspace-level statement, verbatim.
    ///
    /// Returns `None` when no member declares a build script — there is then
    /// nothing missing, and a sentence saying so would be noise rather than
    /// coverage.
    ///
    /// **The wording is the point of the ruling.** It lets a reader tell *not
    /// indexed* from *not called*. Those are different facts about the world
    /// and only one of them is about the code; an index that merely showed
    /// fewer edges would collapse them, and would present an artifact of the
    /// tool's own configuration as a property of the user's code.
    pub fn generated_code_statement(&self) -> Option<String> {
        let affected = self.members_with_unindexed_generated_code().len();
        if affected == 0 {
            return None;
        }
        Some(format!(
            "generated code was not indexed for {affected} of {} members; calls into \
             generated code from those members are absent from this index, not proven \
             absent from the code",
            self.members.len()
        ))
    }
}
