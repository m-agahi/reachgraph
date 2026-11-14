# ADR-1223: An unresolved-edge count is absent, not zero, when the edge provider does not report unresolved calls

**Status:** Accepted
**Date:** 2026-11-12
**Source:** exported from the yadgar decision ledger, project `m-agahi/reachgraph`, on
2026-11-14, with the wording ruling of 2026-11-14 applied. The number is the ledger's own and
is kept for that reason. It is not part of the four-digit 00xx sequence of this directory.
**Amended on export:** the ledger record says the cli and page render an unknown count as
"not measured". Max ruled the wording on 2026-11-14 (ledger task 2067): the run report says
`not reported (<plugin> omits calls it cannot resolve)`, and the page says
`unknown — this plugin does not report unresolved calls`. This record carries that wording.

## Context

Ledger task 2067. `reachgraph-lang-rust` emits only `EdgeTarget::Resolved` edges. MEASURED from
`reachgraph-lang-rust/src/engine.rs`: an edge is built from each `CallItem` that
`outgoing_calls` returns, and a call `ra_ap_ide` cannot resolve yields no `CallItem`, so the
call is omitted. It is not emitted as `EdgeTarget::Unresolved`, and no counter records it.

The count of unresolved edges is published on four surfaces:

1. `run.json` `unresolved_edge_targets`, and the run report line on stderr.
2. `unreachable.json` `unresolved_edge_count`.
3. Every shard's `stats.unresolved_edge_count`.
4. Every unreachable node's `possibly_reachable_via_unresolved`.

For a Rust-only index all four were 0 or `false` by construction. MEASURED 2026-11-12: all 8
usability runs (probe, tonic, ripgrep, mdbook and the discrimination fixtures) reported 0.
The run report's own doc comment says a large number here means the unreachability claim is
weaker than it looks, so a 0 reads as "the claim is strong" — a fact nobody observed.

## Decision

Every unresolved-edge count in the artifact and the run report is `Option<usize>`. It is
`null` when an edge provider declares that it does not observe unresolved calls. It is a
number only when every edge provider does.

- `EdgeProvider::reports_unresolved(&self) -> bool` lets a provider declare this. It has no
  default, so every provider states which kind it is. The fixture answers `true`;
  `reachgraph-lang-rust` answers `false`.
- One provider that answers `false` makes the index-wide count `null`, and every shard's
  count with it: the omitted calls are not attributed to any node, so no view can say it holds
  none of them.
- `possibly_reachable_via_unresolved` is `Option<bool>`. A node an observed unresolved call
  flagged stays `true`. Any other node is `null` when a provider answers `false`, and `false`
  only when every provider answers `true`.
- The artifact `schema_version` goes from 1 to 2. Each new `Option` carries
  `#[serde(deserialize_with = "Option::deserialize")]` (ADR-0726), so a missing key is a parse
  error, not a defaulted `null`.
- `reachgraph merge` checks `schema_version` on every file it reads and names both versions
  when they differ. A version 1 `0` parses as a real count under version 2, so the shape alone
  cannot tell them apart.
- The run report prints this line, naming the plugin from its declaration. The cli matches no
  plugin id, and the line holds no digit:
  `unresolved edge targets: not reported (reachgraph-lang-rust omits calls it cannot resolve)`.
- The page labels a `null` flag `unknown — this plugin does not report unresolved calls`, not
  an empty cell.

## Rationale

ADR-0003's honest-absence rule: put the `Option` on exactly the fact that may be unknown. For
Rust today the count is unknown, not zero, and a false 0 silently strengthens every "not
reachable from any endpoint version in this index" claim.

## Consequences

- `reachgraph-plugin-api` contract change: one trait method, recorded in `public-api.txt`.
- Artifact schema version 2. A version 1 artifact is refused by `reachgraph merge` with a
  message that names both versions.
- `loader.js` renders the `null` case.
- When task 2068 phase 2 makes `reachgraph-lang-rust` emit `Unresolved` edges, the plugin
  answers `true` and the number becomes real with no further schema change.

## Rejected alternatives

- **Relabel the line and add a note** — rejected by Max: the artifact would still claim 0,
  and `possibly_reachable_via_unresolved: false` would still read as a fact.
- **Drop the count** — the fixture produces real unresolved edges, and the waist must keep
  carrying them.

## Revisit when

`reachgraph-lang-rust` gains a real unresolved-call counter or emits `Unresolved` edges
(ledger task 2068).
