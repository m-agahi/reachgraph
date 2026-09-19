# ADR-0008: Rust-only v0.1; language neutrality enforced by a fixture plugin

**Status:** Accepted
**Date:** 2026-09-17

## Context

v0.1 implements **Rust only**. Python's resolver is not designed now.

The requirement attached to that scope is that the platform must be versatile enough that
adding a language later is **purely additive** — a new crate implementing existing traits,
with no change to the core.

That requirement runs directly into a limit which this record states plainly rather than
papers over:

> **An interface cannot be proven language-neutral from n=1.**

This is ADR-0002's n=3 freeze problem. With one language implemented, every trait is
shaped by that language's affordances, and nothing distinguishes "this is how call graphs
work" from "this is how rust-analyzer works". Care reduces the leakage. Nothing at n=1
abolishes it.

The decision below is about reducing it _mechanically_ rather than by intention, because
intention is not checkable and a compiler is.

## Decision

### (a) v0.1 implements Rust only

Via the `ra_ap_*` family (ADR-0004). Excluded from v0.1: Python, metrics, cross-repository
stitching.

### (b) Neutrality is enforced by a fixture plugin

**Build a second `LanguagePlugin` implementation that is not a language.** It reads
hand-written JSON fixtures — units, symbols, edges, roots, classifications — and returns
them.

INFERRED cost: about a day.

This is the central mechanism of this ADR, not a testing convenience.

What it buys:

- **n=2, mechanically.** When a rust-analyzer-ism leaks into a trait signature, the
  fixture plugin cannot satisfy it. The leak surfaces as a **compile error now**, rather
  than as a discovery in month nine when the second real language is half-written.
- **Fast, deterministic tests for the entire waist.** No `cargo metadata`, no indexing, no
  timing variance, no requirement that a repository has been built. Graph construction,
  reachability, sharding, classification and rendering all become testable against fixed
  inputs.

The fixture plugin is the thing that converts "is this interface neutral?" from a question
of judgement into a question the build answers.

## The eight `ra_ap` leaks the traits must guard against

| #   | leak                              | why it is Rust-only                                                                                                                                                                                                                                 | what the trait must say instead                                                                                                                                                                     |
| --- | --------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | **Position-based queries**        | `ra_ap_ide::Analysis::outgoing_calls` takes a `FilePosition` — cursor-driven, inherited from LSP's editor origins. A hand-written Go resolver naturally enumerates call sites _within a function_; it has no notion of "what is under this cursor". | `edges_in(unit)` / `edges_from(node_id)`. The Rust plugin converts node → position internally.                                                                                                      |
| 2   | **`FileId`**                      | An interned integer meaningful only inside ra_ap's salsa database.                                                                                                                                                                                  | The waist uses paths, or its own interning it controls.                                                                                                                                             |
| 3   | **`TextSize` (u32 byte offsets)** | ra_ap is byte-based; LSP is UTF-16 code units; tree-sitter is bytes. Exactly ADR-0003 field 2.                                                                                                                                                      | Explicit `position_encoding` per plugin. A Rust-only build would let you skip this field entirely. **Do not.**                                                                                      |
| 4   | **Cargo workspace assumption**    | `ra_ap_load-cargo` needs a `Cargo.toml`. "What is the unit of analysis, and how do I find it" is per-language: crates, modules, packages, source roots.                                                                                             | `discover_units(root) -> Vec<Unit>`. Rust returns crates.                                                                                                                                           |
| 5   | **Salsa snapshot lifecycle**      | ra_ap is built for incremental editing; reachgraph uses it in batch.                                                                                                                                                                                | The plugin owns its lifecycle entirely. No database or snapshot concept reaches the waist.                                                                                                          |
| 6   | **`SymbolKind`**                  | ra_ap's enum carries `Trait`, `Impl`, `Macro`, `Static` — Rust-shaped.                                                                                                                                                                              | A small neutral enum — Function, Method, Type, Module, Field, Other — plus `raw_kind: String` preserved for display.                                                                                |
| 7   | **`Documentation` type**          | An ra_ap-internal type.                                                                                                                                                                                                                             | `doc: Option<String>` plus `doc_format`.                                                                                                                                                            |
| 8   | **Classifier prefixes**           | `src/`, `target/*/out/`, `/nix/store/…rust-lib-src/` and cargo registry paths are Rust-specific _and_ machine-specific.                                                                                                                             | **Categories** live in the waist — first-party, generated, workspace-sibling, third-party, stdlib. **Prefixes** live in the plugin. See ADR-0002, which already makes the classifier a plugin kind. |

**Leak 1 is the highest risk of the eight.** It is the most natural interface to write,
because it is what the library hands you; it works perfectly for Rust; and it silently
taxes every future plugin, each of which must then synthesise cursor positions it does not
naturally have. A position-shaped trait would not fail any Rust test. It would fail the
fixture plugin immediately.

## Plugin selection: one entry, not zero special-cases

Language detection is **plugin-declared from the first commit**. Each plugin declares its
marker files and file extensions — for Rust, `Cargo.toml` and `.rs`. The core consults a
registry.

In v0.1 that registry has exactly **one** entry. That is fine.

What the core must not contain is:

```rust
if is_rust_project(root) { ... }
```

That single line is the difference between language #2 being an addition and language #2
being a core change. The registry with one entry costs nothing now and is the entire
mechanism later.

The same principle governs roots. Tonic's `impl XServer for T` binding and the
CamelCase-to-snake_case rule live **entirely inside the Rust roots plugin**. The waist sees
`(contract_id, version, service, operation, direction, node_id)` and nothing more
(ADR-0007).

## The anti-leak test

A rule for whoever writes the traits. For every trait method and every struct field, ask:

> **Could `ra_ap`'s return value be substituted verbatim here?**

If yes, the interface is probably describing rust-analyzer rather than describing a call
graph.

The fixture plugin is what makes that question answerable rather than rhetorical — the
answer becomes whether the build passes.

## Scope

**Build now** — cheap now, impossible to retrofit:

- The five plugin traits (ADR-0002), defined from **what the waist needs**, not from what
  `ra_ap` returns.
- The six ADR-0003 fields, even though Rust needs none of them.
- The fixture plugin.
- `discover_units` and the detection registry, with one real entry.

**Do not build now:**

- Anything Python-shaped.
- Metrics — still unsourced (`docs/design.md` §9 Q2).
- Cross-repository stitching. The mechanism is proven; shipping it is later.

## Consequences

- **ADR-0002's "unstable until n=3" still holds, unweakened.** The fixture plugin reduces
  breakage when the second real language lands. It does not abolish it. A fixture is a
  synthetic consumer: it exercises the shape of an interface, not the awkwardness of a
  real language's semantics.
- Carried forward from ADR-0004, and worth repeating here because v0.1 makes it easy to
  forget: **Rust support is cheap and high quality, and it is not representative.** Any
  schedule, interface expectation or quality bar extrapolated from the Rust plugin will be
  wrong for Go and Java.
- **The fixture plugin is permanent test infrastructure**, not scaffolding to delete once
  a second language exists. Its value as a fast deterministic harness for the waist grows
  rather than shrinks, and it remains the mechanical guard against the next language's
  idioms leaking inward.
- v0.1 has a real user-facing deliverable — Rust repositories, fully supported — rather
  than a framework with nothing behind it.

## Rejected alternatives

**Implement Python in v0.1 to obtain a real n=2.** Rejected: v0.1 is scoped to Rust. The
fixture plugin buys most of the neutrality guarantee at a small fraction of the cost, and
ADR-0004 shows Python needs two crates and type inference to resolve method calls — not a
cheap addition.

**Defer the plugin interface until a second language exists.** Rejected: the six ADR-0003
fields and the trait shapes are cheap now and cannot be retrofitted. A plugin already
written produces data that lacks those fields, and data never collected cannot be
recovered after the fact.

**Trust code review to catch the leaks.** Rejected: that is the good-intentions version of
the same policy. It is precisely what the compile error replaces.
