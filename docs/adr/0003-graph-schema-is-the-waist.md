# ADR-0003: The graph schema is the waist and is not a plugin

**Status:** Accepted
**Date:** 2026-09-17

## Context

ADR-0002 makes nearly everything a plugin. Taken literally — _everything_ is a plugin —
the system has no fixed point, and therefore no contract to version, no schema to
validate against, and nothing that stays still while the parts around it change.

There is a second, subtler hazard. Every plugin interface will be designed while exactly
one language exists (Rust). An interface shaped against n=1 encodes that language's
accidents as if they were universal, and every later language then fights it.

ADR-0002 defuses the _cost_ of that mistake — with no third-party ecosystem, changing an
interface is a refactor rather than a breaking release. It does not defuse the _mistake_.
Some fields are free to add now and structurally impossible to retrofit later, because
their absence means the information was never collected.

## Decision

**The graph schema is the waist. It is not a plugin, and neither is the graph build nor
the reachability algorithm.**

Everything plugs into the waist. The waist plugs into nothing. It is the only thing in the
system with a versioned, stable definition.

### Six fields that must exist in v0.1

Each is free today and impossible to retrofit. Rust needs none of them, which is precisely
why each would otherwise be forgotten.

| #   | field                                                                                                           | the Rust accident it absorbs                                                                                                                                                                                                                                                                                                                                                                  |
| --- | --------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | `provides: [symbols, edges, roots, classify]` — capability declaration per plugin                               | Rust may supply symbols and edges from one engine, or from two separate sources. Other languages will differ. Splitting plugins by _capability_ rather than by _source_ lets one crate declare several kinds and be invoked once, instead of re-indexing a repository twice.                                                                                                                  |
| 2   | `position_encoding` declared per plugin                                                                         | LSP uses UTF-16 code units. SCIP uses UTF-8 bytes. tree-sitter uses bytes. Pick one silently and every plugin is off-by-N in files containing non-ASCII text. ASCII-only Rust test fixtures hide this indefinitely.                                                                                                                                                                           |
| 3   | `node_id: (plugin_id, String)`, opaque — **the core must never parse it**                                       | If the waist required SCIP symbol strings, it would have mandated SCIP, which ADR-0004 rejects on availability. An engine-backed plugin has no SCIP symbol; it has a file, a range and a name. Keeping the string opaque means the core never has to care. Cross-repo identity does not need it either — the join key is the contract operation FQN (ADR-0007).                               |
| 4   | `provenance` + `inference_mode` on every edge                                                                   | Rust's edges come from one high-quality engine. Hand-written resolvers (ADR-0004) will produce weaker, differently-shaped edges. A directly-resolved edge and an inferred edge are different-strength claims and must not render identically. Add the field before the second source exists.                                                                                                  |
| 5   | `preflight() -> Result<Ok, Failure { reason, remediation }>` per plugin                                         | "The repository must be built at least once" is a Rust and TypeScript accident, not a universal. Each plugin self-checks its own prerequisites and returns structured guidance. **Never `command -v`** — MEASURED in `docs/design.md` §10, `rust-analyzer` resolves on PATH on the author's machine but is a `rustup` proxy that loops and is not installed. A name resolving proves nothing. |
| 6   | roots returned language-neutrally: `(contract_id, version, service, operation, direction, node_id, confidence)` | `impl XServer for T` with CamelCase-to-snake_case binding is 100% tonic-specific. Nothing trait-shaped or impl-shaped may reach the waist. See ADR-0007 for why `version` is in this tuple.                                                                                                                                                                                                   |

Field 3 is the load-bearing one. It is what allows the waist to stay uncommitted on which
analysis technology any given language uses.

### The honest-absence rule

Writing the implementation plans produced the same defect four times, in four different
fields, found by four independent authors. It governs enough of the schema to belong here
rather than scattered across plans.

> **Put the `Option` on exactly the fact that may be unknown, never on a type that also
> carries facts that are known.**

The four instances, which are the argument:

| #   | field                                      | defect                                                                                                                                                                                                 | fix                                          |
| --- | ------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------- |
| 1   | `confidence: f32` on `Root`                | A bare float invites the failure `docs/design.md` §5 indicts — MEASURED, `code_graph` produced 59 `CALLS` edges at confidence 0.55 with two or three candidate targets. A number in place of a reason. | `RootBinding::{ Bound, Unbound { reason } }` |
| 2   | sentinel `Span { 0, 0 }` on `Symbol`       | Indistinguishable from a real offset 0. A plugin that did not know was lying in a way nothing downstream could detect.                                                                                 | absence must be sayable                      |
| 3   | defaulted `PositionEncoding` on a renderer | Proposed as a provided-method default and refused. A default _is_ a value, and a meaningless value is indistinguishable downstream from a meant one.                                                   | `Renderer` does not extend `Plugin`          |
| 4   | `Option<SourceRange>`                      | The correction to (2), wrong in the opposite direction: it discarded the **file**, which a plugin always knows, along with the **span**, which it may not.                                             | `SourceRange { file, span: Option<Span> }`   |

**Instance 4 carries the non-obvious half, because it is the failure mode of _fixing_ the
first three.** Widening optionality destroys information as surely as a sentinel invents
it. A type that cannot express a fact the plugin holds is exactly as dishonest as one that
fabricates a fact the plugin lacks. Both yield an artifact that misrepresents what was
actually known at index time — which is the thing this ADR exists to prevent.

This is not a new constraint. It is the general form of what fields 2, 3 and 4 each do in
their own domain: `position_encoding` refuses to assume an encoding, the opaque `node_id`
refuses to assume a symbol scheme, and `provenance` / `inference_mode` refuse to let a weak
claim render as a strong one. Each is the same rule applied to a different unknown.

### Not plugins

- The graph schema itself.
- Graph construction.
- The reachability algorithm (reachable set from a root, depth-limited; complement over
  all roots).

These are the tool's thesis. A tool whose central algorithm is swappable has no thesis.

## Consequences

- Plugin authors cannot change graph semantics. They supply nodes, edges, roots,
  classifications and renderings; they do not decide what reachability means.
- Every edge in the output carries its provenance, so the renderer and the report can
  distinguish a resolved call from an inferred one. This is the mechanism that makes the
  "not reachable" claim honest rather than dangerous (`docs/design.md` §8, extended by
  ADR-0007).
- The waist gets a version number from v0.1. Plugin traits do not (ADR-0002: unstable
  until n=3).
- INFERRED: fields 1, 2 and 4 will look like dead weight until the second language lands,
  and will be tempting to remove during early cleanup. They are not dead weight. They are
  the reason the second language is a week of work instead of a redesign.
