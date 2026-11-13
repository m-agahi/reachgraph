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
//! `out_dir_loaded`, `stdlib` and `proc_macro_expansion` are Rust
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
/// ADR-0001 forbids.
///
/// # The second variant, and what the 2026-09-19 measurement missed
///
/// MEASURED 2026-10-02 (ADR-0009): stage 2 failed for two reasons, neither
/// of which is the mechanism. `OUT_DIR` had been injected through
/// `load_workspace`'s `extra_env`, which feeds the proc-macro server and not
/// the crate's own env; and the load requested no sysroot, so `include!`,
/// `env!` and `concat!` — declared in `core` — never expanded at all. With
/// `OUT_DIR` set on the crate's env after the load, and the sysroot
/// requested, the `fx-macro` edge into generated code resolves. So a second
/// value is reachable, and it reads an existing build rather than running one.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum OutDirMechanism {
    /// Nothing loaded generated code into the crate graph.
    Unloaded,
    /// ADR-0009: the caller named a target directory a previous build
    /// populated, and each member's recorded `OUT_DIR` in it was loaded —
    /// the files through `extra_includes`, the variable on the crate's env.
    /// reachgraph ran no build to obtain it.
    ExistingBuildOutput {
        /// The target directory the caller named.
        target_dir: std::path::PathBuf,
    },
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
    /// ADR-0009: the build-script output directory this run read for the
    /// member, when it was told to read build output and found one. Recorded
    /// because a target directory can hold several builds of one package, and
    /// which one was read is a fact about the index.
    pub out_dir_read: Option<std::path::PathBuf>,
    /// How many build-script output directories the target held for this
    /// member; the most recently run one is read. Zero when none was read.
    pub out_dir_candidates: usize,
    /// A declared input of the build script that changed after the output
    /// read was produced — the build is stale, and generated code may not
    /// match the source. `None` when the output is at least as new as every
    /// input, or when none was read.
    pub out_dir_stale_input: Option<std::path::PathBuf>,
}

/// Whether the standard library's source was loaded — ADR-1222.
///
/// Every run asks `rustc --print sysroot` and hands that path to ra_ap. The
/// answer is one of three facts, and the two misses are kept apart because
/// what fixes them differs.
///
/// What a miss costs, MEASURED 2026-11-12: a first-party call whose result
/// goes through `?` has **no edge**, while the same call written plainly has
/// one. INFERRED mechanism: without `core` the `Try` and `Future` lang items
/// are unknown, so any call whose resolution depends on a type through `?`,
/// `.await`, an operator or a std trait is dropped. And in every state, standard-library call
/// targets are **external** in plan-01 §7.0's sense: no symbol is emitted for
/// them, so none is classified, even when the source resolved.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum StdlibSource {
    /// The source resolved and `core`, `alloc` and `std` were loaded.
    Resolved,
    /// `rustc` named a sysroot and no library source was found under it or
    /// at `RUST_SRC_PATH` — the `rust-src` component is not installed.
    SourceMissing {
        /// The sysroot `rustc --print sysroot` printed.
        sysroot: std::path::PathBuf,
    },
    /// `rustc --print sysroot` did not answer, so there was nowhere to look.
    SysrootUnknown {
        /// What went wrong, in the probe's words.
        detail: String,
    },
}

impl StdlibSource {
    /// The toolchain name, as the last segment of the sysroot path — what
    /// `rustup component add --toolchain` takes. `None` when no sysroot is
    /// known.
    pub fn toolchain(&self) -> Option<String> {
        match self {
            StdlibSource::SourceMissing { sysroot } => sysroot
                .file_name()
                .map(|name| name.to_string_lossy().into_owned()),
            _ => None,
        }
    }
}

/// What the whole run could not see.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RustCoverage {
    /// One row per workspace member.
    pub members: Vec<MemberCoverage>,
    /// See [`StdlibSource`].
    pub stdlib: StdlibSource,
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

/// Calls in the source against edges produced, for one unit — task 2068.
///
/// # Why these counters exist
///
/// Plan-03 §12 promised them for v0.1 and they were never built. Without
/// them a call `ra_ap` cannot resolve leaves no trace at all: it yields no
/// `CallItem`, so it yields no edge, and the index cannot tell a function
/// that calls nothing from one whose calls all went unresolved. MEASURED
/// 2026-11-12: a fn item passed by value (`map(helper)`) and a call through a
/// fn pointer (`f(1)`) both produce no edge today.
///
/// `call_sites` and `macro_call_sites` are counted from the syntax tree, so
/// they do not depend on name resolution. Read in ra_ap_ide 0.0.352's
/// `call_hierarchy.rs`: `outgoing_calls` walks the call and method-call
/// expressions of the same body and makes at most one edge per call site, so
/// for a function written in the source `call_sites - edges` approximates the
/// call sites it could not resolve. INFERRED, not measured: for a function a
/// macro generates, `outgoing_calls` descends into the expansion and can make
/// edges while this count, which does not descend, sees 0 call sites; the
/// floor at 0 hides that. The difference is what the note reports. It is a
/// count, not a list: which call sites lost their edge is not recorded.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnitCallCounts {
    /// The unit id.
    pub unit: String,
    /// Functions and methods `outgoing_calls` was asked about.
    pub queried: usize,
    /// Of those, how many had no call hierarchy at all — `outgoing_calls`
    /// returned none. MEASURED 2026-11-13 in ra_ap_ide 0.0.352: that happens
    /// only when the file has no token at the offset, so on real code this
    /// is 0. It is reported so that a non-zero value is seen rather than
    /// assumed away.
    pub no_hierarchy: usize,
    /// Call targets skipped because their file failed the VFS lookup.
    pub targets_dropped: usize,
    /// Call ranges skipped because their file failed the VFS lookup.
    pub ranges_dropped: usize,
    /// Edges produced.
    pub edges: usize,
    /// `CallExpr` and `MethodCallExpr` nodes in the function bodies, outside
    /// macro calls.
    pub call_sites: usize,
    /// `CallExpr` and `MethodCallExpr` nodes in the one-level expansion of
    /// each macro call in the function bodies. `outgoing_calls` walks the
    /// unexpanded body, so these calls get no edge (MEASURED 2026-11-13 with
    /// `twice!(helper_e(1))`). A macro call the engine cannot expand adds 0,
    /// and a macro call inside an expansion is not expanded again.
    pub macro_call_sites: usize,
}

impl UnitCallCounts {
    /// Call sites outside macro calls that have no edge — floored at 0,
    /// because edges can outnumber call sites.
    fn gap(&self) -> usize {
        self.call_sites.saturating_sub(self.edges)
    }
}

/// Units with fewer call sites than this are never listed by name.
const LISTED_MIN_CALL_SITES: usize = 20;
/// At most this many units are listed by name.
const LISTED_MAX_UNITS: usize = 5;

/// The one aggregated call-counts note — task 2068 phase 1.
///
/// Index-wide totals, then at most five units with 20 or more call sites
/// where at least half of those have no edge, highest share first. One note
/// rather than one per unit: a repository such as tonic has about 130 units,
/// and a note per unit would bury every other note, the stale-build warnings
/// of ADR-0009 among them.
///
/// `None` when nothing was queried — there is then nothing to count, and a
/// line of zeros would read as a finding.
pub fn call_counts_note(units: &[UnitCallCounts]) -> Option<String> {
    let mut total = UnitCallCounts::default();
    for unit in units {
        total.queried += unit.queried;
        total.no_hierarchy += unit.no_hierarchy;
        total.targets_dropped += unit.targets_dropped;
        total.ranges_dropped += unit.ranges_dropped;
        total.edges += unit.edges;
        total.call_sites += unit.call_sites;
        total.macro_call_sites += unit.macro_call_sites;
    }
    if total.queried == 0 {
        return None;
    }

    let mut note = format!(
        "reachgraph-lang-rust call counts: {} functions and methods queried, {} with no call \
         hierarchy (outgoing_calls returned none), {} call targets dropped at the file lookup, \
         {} call ranges dropped at the file lookup, {} edges produced, {} call sites in the \
         source outside macro calls, {} call sites inside macro calls; {} calls in the source \
         with no edge in this index",
        total.queried,
        total.no_hierarchy,
        total.targets_dropped,
        total.ranges_dropped,
        total.edges,
        total.call_sites,
        total.macro_call_sites,
        total.gap(),
    );

    let mut listed: Vec<&UnitCallCounts> = units
        .iter()
        .filter(|unit| {
            unit.call_sites >= LISTED_MIN_CALL_SITES && 2 * unit.gap() >= unit.call_sites
        })
        .collect();
    // gap / call_sites descending, compared by cross-multiplication so no
    // float rounds two shares together; ties by unit name.
    listed.sort_by(|a, b| {
        (b.gap() * a.call_sites)
            .cmp(&(a.gap() * b.call_sites))
            .then_with(|| a.unit.cmp(&b.unit))
    });
    listed.truncate(LISTED_MAX_UNITS);
    if !listed.is_empty() {
        let entries: Vec<String> = listed
            .iter()
            .map(|unit| {
                format!(
                    "{} ({} call sites, {} edges)",
                    unit.unit, unit.call_sites, unit.edges
                )
            })
            .collect();
        note.push_str(&format!(
            "; units with {LISTED_MIN_CALL_SITES} or more call sites where at least half have no \
             edge: {}",
            entries.join(", ")
        ));
    }
    Some(note)
}
