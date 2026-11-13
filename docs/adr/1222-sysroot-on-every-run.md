# ADR-1222: lang-rust resolves the sysroot from `rustc --print sysroot` and indexes with cfg(test) on, never via RustLibSource::Discover

**Status:** Accepted
**Date:** 2026-11-12
**Source:** exported from the yadgar decision ledger, project `m-agahi/reachgraph`, on
2026-11-12, with the measurements of this change added. The number is the ledger's own and
is kept for that reason. It is not part of the four-digit 00xx sequence of this directory.
**Amends:** [ADR-0009](0009-read-existing-build-output.md), which requested the sysroot only
with `--read-build-output`.

## Context

Usability run 2026-11-12 (ledger tasks 2061 and 988). `engine.rs` loaded the workspace with
`CargoConfig::default()` unless `--read-build-output` was given, so `sysroot` was `None` and
ra_ap_project_model 0.0.352 used `Sysroot::empty()` (`workspace.rs:243`) — on every machine,
whether `rust-src` was installed or not. The preflight remediation
`rustup component add rust-src` therefore never changed anything.

MEASURED, the cost was not confined to calls into the standard library. On the `fx-try`
fixture (this repository), `local_result(&r.name)?` had no edge while `local_result(&r.name)`
on the next line had one. On yadgarhq/task, 220 of 227 symbols were unreachable. INFERRED
mechanism: without `core` the `Try` and `Future` lang items are unknown, so the type of
anything through `?` or `.await` is unknown, and a call whose resolution depends on it is
dropped.

MEASURED in ra_ap_project_model 0.0.352: `RustLibSource::Discover` runs
`rustup component add rust-src` when the source is missing (`sysroot.rs:467-479`).
`RustLibSource::Path` calls `discover_rust_lib_src_dir` instead — `RUST_SRC_PATH`, then
`<sysroot>/lib/rustlib/src/rust/library` — and never runs rustup.

MEASURED: with the sysroot resolved and `set_test: false`, cfg(test) is evaluated, and every
`#[test]` function resolves as cfg-gated and is not indexed (`fx-impl`'s
`the_mock_creates` and `fx-try`'s `orphan_is_seven` vanish).

## Decision

reachgraph-lang-rust sets `sysroot: Some(RustLibSource::Path(<output of rustc --print
sysroot>))` and `set_test: true` on every run. It never uses `RustLibSource::Discover` or any
path that installs or modifies the user's toolchain. Every toolchain call it causes —
`cargo --version`, `rustc --print sysroot`, and ra_ap's own calls through `extra_env` — runs
with `RUSTUP_AUTO_INSTALL=0`.

When `rust-src` is absent, or `rustc --print sysroot` does not answer, the run records that as
a limit (honest absence) instead of failing. The limit names what is lost — calls through
`?`, `.await`, operators and std traits, first-party calls among them — and, when the
component is missing, the toolchain to add it to. With `--read-build-output`, a sysroot that
does not resolve still refuses the run, as ADR-0009 decided.

## Rationale

Without the sysroot the graph is mostly missing. `Discover` mutates the user's machine and
downloads, which a read-only analysis tool must not do. `set_test: true` keeps test functions
indexed as they were before the sysroot loaded.

## Consequences

- A second subprocess, `rustc --print sysroot`, runs on every run, beside `cargo --version`.
  Both are the target language's own toolchain under ADR-0001's carve-out. The spawn guard
  (`reachgraph-cli/tests/cli/guards.rs`) still allows exactly these two sites; its name and
  documentation now say they run on every run.
- Cost, MEASURED 2026-11-12 on yadgarhq/task (release build, 3 runs each): peak RSS
  878 → 1410 MB, wall 4.7–5.0 s → 6.2–7.5 s, for loading `core`, `alloc` and `std`.
- MEASURED on the same runs: edges 364 → 1208, symbols 227 → 297, unreachable 220 → 283
  (reachable 7 → 14; the added unreachable symbols are `#[test]` functions and builtin
  derive impls now emitted), roots bound 6 of 11 before and after.
- Standard-library targets remain `category: None` externals. No symbol is emitted for them,
  so none is classified, even when the source resolved. The limit text does not promise
  otherwise.
- cfg(test) items in `src/` are now indexed. A mock `impl` inside `#[cfg(test)] mod` is not
  marked `is_test` (that is fn-only), so it can appear as a served signal. Ledger task 2064
  marks such symbols `is_test`.

## Rejected alternatives

- **`RustLibSource::Discover`** — runs `rustup component add rust-src` on a miss.
- **An opt-in flag, default off** — the default output stays wrong, and silently so.
- **Status quo** — the graph is mostly missing.

## Revisit when

ra_ap gains a non-mutating discover mode, or the memory cost blocks a real repository
(ledger task 2070).
