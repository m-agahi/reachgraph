# ADR-0004: Call-edge sources per language

**Status:** Accepted
**Date:** 2026-09-17

## Context

ADR-0001 forbids subprocesses and external binaries. Call edges must therefore come from
code linked into our own binary: either an imported Rust-native engine, or a resolver we
write ourselves.

`docs/design.md` §3 names the decision to rent other people's indexers as "the single most
important decision in this design", because owning a per-language indexer is what killed
Sourcetrail (MEASURED: archived 2021-12-13 at 16.5k stars). ADR-0001 constrains renting to
things that link. This ADR records what that leaves, per language.

### What can be imported

**Rust — a complete engine, today.** MEASURED 2026-09-17 (subagent probe, docs.rs for
`ra_ap_ide` v0.0.352):

```rust
pub fn call_hierarchy(&self, position: ...)
pub fn incoming_calls(&self, config: ..., ...)
pub fn outgoing_calls(&self, config: ..., ...)
```

plus a public `CallItem` struct and `CallHierarchyConfig`. MEASURED: ~48 `ra_ap_*` crates
on crates.io at version 0.0.352, republished 2026-09-14, maintained continuously as a
byproduct of rust-analyzer's own release process, licensed MIT OR Apache-2.0, on a weekly
publish cadence (2026-09-14, 2026-09-07, 2026-08-31). **This is the same engine LSP
clients drive over stdio, exposed as ordinary public functions.** Rust support links it
directly; no subprocess, no external tool.

MEASURED: `ra_ap_ide` has 24 crates.io reverse dependencies, including `cargo-callgraph`,
`cargo-modules` and `evcxr`. That is evidence of real external consumption as a library,
not merely of a public API surface that happens to compile.

**Python — two crates, two tiers.** MEASURED 2026-09-17 (crates.io + docs.rs):

- `ruff_python_semantic` v0.0.14, **MIT**, published independently on crates.io.
  `SemanticModel` exposes `resolve_name`, `resolve_load`, `lookup_symbol`,
  `lookup_binding` and `resolve_qualified_name`, backed by real `Scope`, `Binding`,
  `ResolvedReference` and `UnresolvedReference` types. It resolves direct calls and calls
  to imported functions.
- `ty_python_semantic` v0.0.14, **MIT**, on crates.io — Astral's `ty` type-inference
  engine, published separately from the tool itself. MEASURED: `astral-sh/ty` is active
  (`pushed_at` 2026-09-16, not archived).

Both crates self-describe as "an internal component crate of Ruff". That is an API
stability caveat, not a usage restriction: the licence is MIT and its text imposes no such
limit. ADR-0001's pin-and-vendor policy is the response.

MEASURED: `ruff_python_resolver` does **not** exist on crates.io (404). Do not cite it.
The crates.io package named `ty` is an unrelated name-squatted 2021 package and is not
Astral's; the engine crate is `ty_python_semantic`.

**Scope resolution is not rentable for anything else.** MEASURED 2026-09-17 (GitHub API):
`github/stack-graphs` is archived — `"archived": true`, `pushed_at: 2025-09-09`, and the
final commit is titled *"This repository is no longer being maintained."* crates.io
confirms the dormancy: `stack-graphs` 0.14.1 and `tree-sitter-stack-graphs` 0.10.0 both
last published 2024-12-13. MEASURED: the shipped language definitions before archival were
`java`, `javascript`, `python`, `typescript` — **Go was never covered at all.**

This forecloses the cleanest theoretical path: a shared, maintained, cross-language
scope-resolution layer on top of tree-sitter does not exist.

**Go and Java import nothing — confirmed by search, not assumed.** MEASURED 2026-09-17
(crates.io):

- Go: the only candidates are Goscript's `go-types` and `go-parser` — a different,
  Go-*inspired* scripting language, stale since 2023-09-12 — and `woolink` (0 stars, 76
  downloads, stale 2026-03-24). Nothing real to vendor.
- Java: the best hits are AST-only with negligible adoption — `java-ast-parser` (224
  downloads, no name or type resolution), `codegraph-java` (117 downloads),
  `rusty-javac` (147 downloads, hobby project).

### What was measured and deliberately not taken

Recorded because the road not taken measured *well*, and a future reader will otherwise
assume it was rejected for weakness. MEASURED 2026-09-17, LSP `initialize` plus live
`callHierarchy/outgoingCalls` against hello-world samples:

| server | version | `callHierarchyProvider` | real edges |
|---|---|---|---|
| rust-analyzer | 2026-06-15 | true | yes |
| gopls | 0.23.0 | true | yes |
| pyright | 1.1.411 | true | yes |
| basedpyright | 1.39.8 | true | yes |
| typescript-language-server | 5.3.0 | true | yes |
| clangd | 21.1.8 | true | yes |
| jdtls | 1.60.0 | true | yes |
| jedi-language-server | 0.47.0 | **false** | `-32601 Method Not Found` |
| python-lsp-server | 1.14.0 | **false** | `-32601 Method Not Found` |

`callHierarchy` is a universal capability among mainstream servers. It was rejected on
acquisition, not capability — see ADR-0001.

MEASURED 2026-09-17: `gopls` publishes **zero** binary release assets (`go install` only,
per `gh api repos/golang/tools/releases`) and no PyPI package named `gopls` exists. Of the
four target languages, Go had no off-the-shelf distribution path under either reading.

## Decision

Call edges come from a per-language source, declared in the plugin's capability set
(ADR-0003 field 1), with `provenance` and `inference_mode` on every edge produced
(ADR-0003 field 4).

| language | source | cost |
|---|---|---|
| **Rust** | **import** `ra_ap_ide::Analysis::outgoing_calls` and the `ra_ap_*` family | near-zero; works now |
| **Python** | **import** `ruff_python_semantic` for lexical resolution, `ty_python_semantic` for type inference | moderate; both crates exist and are MIT |
| **Go** | **create.** No Rust-native Go analyzer; no stack-graphs Go definition; gopls is Go source and cannot link | months |
| **Java** | **create.** Classpath resolution, generics, overload resolution, inheritance | largest |

Language order Rust → Python → Go → Java (ADR-0002) follows directly from this table: it
is descending import-availability.

### Python needs both crates, and this makes `inference_mode` load-bearing

INFERRED. `ruff_python_semantic` resolves **lexically**: it knows which binding a name
refers to in a given scope. That is sufficient for a direct call `foo()` and for a call to
an imported function `mod.foo()` where `mod` is a module binding.

It cannot resolve `obj.method()`. Determining which `method` that is requires knowing
`obj`'s **type**, which is type inference — `ty_python_semantic`'s job, not
`ruff_python_semantic`'s.

Method calls are pervasive in idiomatic Python. A resolver built on `ruff_python_semantic`
alone would miss most call edges in a typical codebase, while appearing to work on simple
test fixtures.

Consequence for the waist: ADR-0003 field 4 (`inference_mode`) is load-bearing from the
first non-Rust language, not speculative. Python edges are `lexical`, `type-inferred` or
`unresolved`. These are different-strength claims: a lexically-resolved direct call is
near-certain, a type-inferred method call depends on inference succeeding, and an
unresolved call is a known gap. They must not render identically.

## Consequences

- Rust support is cheap and high quality. **It is not representative.** Any schedule,
  interface or quality expectation extrapolated from the Rust plugin will be wrong for
  Go and Java.
- Go and Java mean writing name resolution — which is the work that killed Sourcetrail.
  This is accepted deliberately under ADR-0001, not overlooked. Go is the more tractable
  of the two: explicit imports, a simpler type system, no macros.
- Edge quality will differ sharply across languages. ADR-0003 field 4 is therefore not
  optional decoration; it is the mechanism that keeps the output honest. A hand-written
  resolver's edge must be distinguishable from an `ra_ap`-resolved one in the artifact.
- `docs/design.md` §8's warning that trait dispatch and generics produce missing edges
  (rust-analyzer issue #19358) applies *more* strongly to resolvers we write. Show a
  missing edge as missing. Never infer one to fill a hole.
- INFERRED: a hand-written Go resolver will not match gopls. The tool must not imply it
  does.
- Both imported families are pre-1.0 and volatile: `ra_ap_*` is 0.0.x republished weekly
  in lockstep with rust-analyzer nightlies, and `ruff_python_semantic` /
  `ty_python_semantic` publish on the same weekly rhythm while labelling themselves
  internal component crates. Neither offers a semver stability promise. ADR-0001's
  pin-exact-and-vendor policy is the containment; upgrades are deliberate re-vendors with
  an expectation of breakage.

## Rejected alternatives

**SCIP as the edge source.** Rejected twice over. MEASURED from the SCIP schema:
`Relationship` carries only `is_reference`, `is_implementation`, `is_type_definition`,
`is_definition` — so a call graph must be *inferred* by finding a reference occurrence and
asking which definition's range encloses it, rather than queried directly. Separately,
SCIP indexer availability is poor (see ADR-0005).

**tree-sitter plus name heuristics.** MEASURED negative evidence in `docs/design.md` §5:
`code_graph` derives 35% of its `CALLS` edges from name heuristics, 59 of them at
confidence 0.55 with two or three candidate targets, and its `Route`→`HANDLES`→`Function`
query returns zero rows. Producing edges by matching names is not resolution. tree-sitter
is still used, for a different job — see ADR-0005.

**stack-graphs.** Archived, never covered Go. Not available to reject on merit.
