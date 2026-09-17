# ADR-0005: Doc comments come from the resolver's own parse tree

**Status:** Accepted
**Date:** 2026-09-17

> Filename retained as `0005-doc-comments-from-tree-sitter.md` for link stability. The
> title changed when the decision narrowed — see *History* at the end.

## Context

`docs/design.md` §7 specifies that a node is labelled with **name plus the first line of
its doc comment**. Whether that is achievable is the premise question in §9 Q1: if the doc
text is not good enough to label a box with, the labelling plan fails.

Doc comments have to come from somewhere, and the original plan was SCIP. That plan was
formed while SCIP was also the planned source of symbols, before ADR-0001 ruled out
external indexer binaries.

### SCIP is rejected on availability, not on quality

This distinction matters, because a future reader will otherwise assume SCIP was found
wanting.

MEASURED 2026-09-17 (subagent probe, `rust-analyzer scip .` decoded with
`scip print --json`): SCIP preserves the **full, untruncated** multi-line doc comment in
`SymbolInformation.documentation`.

```
SYMBOL: rust-analyzer cargo hello 0.1.0 documented_fn().
DOCS: ['Adds two numbers together.\n\nThis is a multi-line doc comment to test whether SCIP\npreserves the full documentation or truncates it.']
```

SCIP's doc fidelity is excellent. What fails is getting an indexer. MEASURED 2026-09-17:

| indexer | health |
|---|---|
| `scip-go` | current |
| `scip-python` | last **human** commit 2025-09-05 — over a year stale; a hard-fail bug (#223) open and unaddressed |
| `scip-clang` | last human commit 2026-03-24 — ~6 months stale; bug #544 open since 2026-09-10 |
| `scip-typescript`, `scip-java`, `scip-ruby` | alive but unpackaged |

Five of six require their own npm, gem or bazel toolchain to obtain — an acquisition
problem, under an architecture (ADR-0001) whose whole purpose is to have none.

A trap worth recording: `scip-python` and `scip-clang` both look actively maintained from
GitHub's repository list, because Renovate-bot branch pushes and Dependency-Dashboard
issue edits inflate `pushed_at`. Filtering commits to non-bot authors was required to see
the real state. Judging indexer health from `pushed_at` alone gives the wrong answer.

### Under ADR-0001 the question largely dissolves

ADR-0004 settles that Rust and Python both get a real semantic engine linked in:
`ra_ap_*` for Rust, `ruff_python_semantic` plus `ty_python_semantic` for Python.

**An engine that resolves calls has already parsed the file.** `ra_ap` carries Rust
documentation; `ruff_python_ast` carries Python docstrings. For the two languages where
the resolver is imported, no additional parser is needed to obtain doc text at all.

## Decision

**Doc comments come from whatever parse tree the language's own resolver already
produces.**

| language | doc source |
|---|---|
| Rust | `ra_ap_*` — the engine already supplying call edges |
| Python | `ruff_python_ast` — already present via `ruff_python_semantic` |
| Go | tree-sitter-go, as the parser substrate for the resolver we must write |
| Java | tree-sitter-java, likewise |

tree-sitter's role is therefore **narrow and specific**: it is the parser substrate for
the languages where ADR-0004 obliges us to write the resolver ourselves. It is not a
separate doc-extraction mechanism bolted alongside an engine that already has the text.

Where tree-sitter is used, doc extraction is **syntactic extraction at a range the
resolver already supplies** — read the `///` block, docstring or Javadoc immediately
preceding a known node.

### Why this is not the failure mode the code_graph critique identifies

`docs/design.md` §5 records damning MEASURED evidence against `code_graph`: 35% of its
`CALLS` edges come from name heuristics, 59 of them at confidence 0.55 with two or three
candidate targets; its `Route`→`HANDLES`→`Function` query returns zero rows; it classified
three URLs in `.pre-commit-config.yaml` as HTTP routes.

Every one of those failures comes from using tree-sitter for **resolution** — deciding
which `foo` a call to `foo` means, by matching names. That is the use this project
rejects (ADR-0004, rejected alternatives).

Extracting the comment block immediately above a node whose range is already known is not
resolution. It has no candidate set, no confidence score and no way to pick the wrong
target. The distinction is not a hedge; it is the difference between asking a parser a
syntactic question and asking it a semantic one.

Also MEASURED in §5, as the concrete standard to beat: `code_graph`'s `Function.docstring`
retains only the **last line** of a `///` block, sigil attached, mid-sentence — its longest
value across the whole `task` repo is 83 characters against source blocks of 8–10 lines —
and 0 of 40 `Method` nodes carry any docstring at all.

### tree-sitter grammar currency

MEASURED 2026-09-17 (crates.io). All MIT.

| crate | version | last publish |
|---|---|---|
| `tree-sitter` (core) | 0.27.0 | 2026-08-30 |
| `tree-sitter-rust` | 0.24.2 | 2026-03-27 |
| `tree-sitter-go` | 0.25.0 | 2025-08-29 |
| `tree-sitter-python` | 0.25.0 | 2025-09-11 |
| `tree-sitter-java` | 0.23.5 | 2024-12-21 |

**There is no version-skew problem**, contrary to the usual expectation. MEASURED: each
grammar depends on the full `tree-sitter` crate only as a *dev-dependency*, for its own
test suite. The runtime linkage point for consumers is the small, ABI-stable
`tree-sitter-language` crate, pinned `^0.1` identically across all four grammars. Grammars
and core may therefore advance independently.

`tree-sitter-java` at ~21 months stale is a **syntax-coverage** concern — recent Java
language features may parse poorly — not a build or compatibility problem. Java is last in
the ADR-0002 language order, so this is not on the critical path.

## Consequences

- Rust and Python need no tree-sitter dependency for documentation. If neither Go nor Java
  is implemented, tree-sitter may not be a dependency of the shipped binary at all.
- Doc quality tracks the resolver, so it varies by language exactly as edge quality does
  (ADR-0004). Rust and Python inherit engine-grade text; Go and Java get whatever our
  extraction achieves.
- `docs/design.md` §9 Q1 — "are the doc comments good enough to label a box with?" — is
  partly answered and partly deferred. MEASURED: full doc text is preserved by a real
  indexer, so the text exists and is not truncated at the source. Whether a first line
  makes a good label is a legibility question, answerable only against real output.
- The fallback if doc text proves unusable is name plus signature, which is what crabviz
  ships (MEASURED, `docs/design.md` §6: LSP's `DocumentSymbol` has no documentation field,
  only `detail`). The project's thesis is endpoint-rooted reachability, not doc labels, so
  this degrades the labelling rather than invalidating the tool.

## History

Originally decided as "extract doc comments with tree-sitter at known ranges", reasoned
when SCIP availability was the binding constraint and no semantic engine was assumed to be
linked in. ADR-0004 then established that Rust and Python each import an engine that has
already parsed the source, which removed the need for a separate parser in those
languages. The decision narrowed accordingly; the SCIP and code_graph arguments above are
unchanged and still load-bearing for Go and Java.
