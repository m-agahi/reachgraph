# ADR-0728: v0.1 disables proc-macro expansion, because in-process expansion does not exist on a stable toolchain

**Status:** Accepted
**Date:** 2026-09-19
**Source:** exported verbatim from the yadgar decision ledger, project `m-agahi/reachgraph`,
on 2026-10-02. Ten places in this repository cite ADR-0728, and until this export none of
them could be resolved from the repository. The number is the ledger's own and is kept for
that reason. It is not part of the four-digit 00xx sequence of this directory.

## Context

ADR-0001's toolchain carve-out permits the target language's own build toolchain and
explicitly does NOT cover rust-analyzer's own components, naming proc-macro expansion as an
open question. Plan-03 §4 D-C recorded as MEASURED that ra_ap_proc-macro-srv exposes
ProcMacroSrv::{new, expand, list_macros} and expands in-process by dlopen-ing a compiled
dylib, so the route looked open. MEASURED 2026-09-19 by compiling rather than reading: that
claim is false for any stable consumer, and the earlier measurement had been taken from
docs.rs.

## Decision

v0.1 runs with ProcMacroServerChoice::None. Proc-macro expansion is disabled, preflight
check 3 reports it, and every Provenance::engine carries "(proc-macros: disabled)" so the
artifact states it rather than implying coverage. No proc-macro server binary is spawned.

## Rationale

MEASURED from the crate source: ra_ap_proc_macro_srv-0.0.352/src/lib.rs:11 is
`#![cfg(feature = "in-rust-tree")]`, so without that feature the crate exports nothing at
all and a reference to ProcMacroSrv is E0425. With the feature it carries
`#![feature(..., rustc_private)]` plus `extern crate rustc_codegen_ssa` and friends, which
is E0463 on stable 1.98.0 and needs nightly with rustc-dev and llvm-tools-preview. The
expander seam itself is genuinely open — hir_expand::ProcMacroExpander is a public trait
and ChangeWithProcMacros::set_proc_macros is public — but there is nothing available to put
through it. The only remaining route is spawning a server binary, and a proc-macro server
is rust-analyzer's own component rather than the target language's build toolchain, so
ADR-0001's carve-out does not reach it.

## Rejected alternatives

- Spawn a proc-macro server binary — refused rather than merely rejected: it falls outside
  ADR-0001's carve-out and is an ADR-level decision, not an implementation choice.
- Require nightly plus rustc-dev — rejected, it contradicts ADR-0001's premise that the
  user installs nothing for reachgraph's sake, and the shared CI installs stable.
- Wire the expander seam by hand — no expander implementation is available to wire.

## Consequences

design.md §4's measured cross-repo proof went through #[tonic::async_trait], and that
crossing does not happen in v0.1 — the generated client-stub leaf stays unreachable through
this path as well as through the OUT_DIR path. preflight returns Warned on every run
because expansion is unconditionally off, so the CLI must decide how to present a warning
that never clears. LESSON, general and cross-project: docs.rs builds crates with features
enabled on nightly, so an API visible there may not exist for a stable consumer. A claim
taken from docs.rs is INFERRED, not MEASURED; only compiling it against the pinned
toolchain measures it.

## Revisit trigger

If rust-analyzer publishes a proc-macro expander usable from stable, or if the project ever
accepts a nightly toolchain requirement, this closes and the tonic crossing becomes
available.
