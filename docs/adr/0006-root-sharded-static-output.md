# ADR-0006: Root-sharded static output artifact

**Status:** Accepted
**Date:** 2026-09-17

## Context

Three questions are easily conflated when deciding how the tool delivers its result:

1. **Artifact shape** — one file, or a directory of shards?
2. **Delivery** — `file://`, a local server, or hosting?
3. **Interaction** — is all data present at load, or fetched on demand?

GitHub Pages is not an alternative to a web server; it is _hosting_ for a static artifact.
The real axis is **build-time artifact versus runtime service**.

`docs/design.md` §7 chose one self-contained HTML file with the graph JSON inlined, and no
store. That holds for one Rust repository. It does not hold at the multi-language,
multi-repository scale the project targets: INFERRED, ~100k nodes at ~300 bytes plus ~300k
edges gives roughly 35–55 MB inlined. That loads, slowly, and is unpleasant to attach
anywhere.

But the full graph is never needed at once. The tool's core operation is _the reachable
set from one endpoint, depth-limited_ — a small subgraph.

### The graph cannot be computed at request time

MEASURED, `docs/design.md` §8: `callHierarchy/outgoingCalls` costs one round trip per
node, so a whole-repository walk takes minutes, not seconds. The design doc's own
conclusion is that this is a CI-generated artefact, not an interactive tool.

A server could therefore only ever serve **precomputed** data — which is exactly what a
static file server does. A stateful server buys nothing until there is cross-commit
diffing or incremental re-indexing to support, neither of which is in scope.

## Decision

**Emit a root-sharded static directory.**

```
out/
  index.html          renderer, no data
  endpoints.json      the root list — small
  graph/<root>.json   one reachable subgraph per root
  unreachable.json
```

**A shard is the reachable set from one root.** This matches both the thesis and the
actual query pattern: the user picks an endpoint, and the tool shows what it reaches.
Sharding by module would cut across that grain and require stitching on every view.

### `reachgraph serve` — deliberately dumb

A `serve` subcommand exists, and it is a static file server over the output directory.
It holds no state, runs no queries and exposes no API.

It exists for exactly one reason: INFERRED, `fetch()` against a `file://` origin is
CORS-blocked in current Chrome and Firefox, so lazily-loaded shards cannot be read from a
bare file open. `serve` spares the user from having to know that. It is a convenience, not
architecture, and it locks in nothing.

### Single-file special case

When the whole graph is under roughly 5 MB, **also** emit `overview.html` with the data
inlined. Same renderer; the loader uses inline data when present and falls back to
fetching shards otherwise.

This preserves the emailable, PR-attachable single file for the common small case — which
is the case `docs/design.md` §7 was written for — without making it the only shape.

### Renderers are plugins

Per ADR-0002, the renderer is a plugin kind: static HTML, inlined HTML, JSON, DOT, SARIF.

A **PR-comment renderer** is planned and may prove the highest-value output for the CI
case the design already targets. It is diff-shaped rather than map-shaped:

> This PR touches 3 functions reachable from `CreateTask` and `ListTasks`. 1 function is
> no longer reachable from any endpoint version in this index.

It needs no hosting, no CORS workaround and no page, and it arrives where the decision is
made.

## Privacy

**A call graph of private source is a serious disclosure.** The artifact encodes file
paths, function and method names, doc comment text, service topology, and precisely which
endpoints reach which code. It is a structural map of a private codebase.

Consequences, binding:

- **GitHub Pages must be opt-in and open-source-only. Never a default.** Pages on a free
  or Pro private repository publishes **publicly**; private Pages requires an Enterprise
  or Team plan. A user who runs the tool in CI and enables Pages without reading this has
  published their internal architecture.
- **CI on a private repository publishes a workflow artifact** — a zip, which respects
  repository permissions and is retention-limited. A reviewer downloads it and opens it,
  or runs `reachgraph serve` against it.
- The tool must not offer a hosted or upload-based delivery path.

## Consequences

- No store, no database, no account, no config file. The graph is regenerated per run, so
  the store would have no runtime role.
- Output is regenerated rather than mutated, so it is safe to delete and rebuild, and
  awkward to commit to a repository.
- One shard per root composes directly with ADR-0007's versioned roots: a versioned root
  is still one root, so v1 and v2 of an operation are separate shards with no extra
  machinery.
- **Cross-reference, load-bearing:** `unreachable.json` must carry the set of roots the
  index actually covered, because a partial root set makes reachable code appear
  unreachable. See ADR-0007, which extends the §8 wording rule to
  _"not reachable from any endpoint version in this index"_.

## Rejected alternatives

**A stateful server with a live query API.** Rejected: the graph cannot be computed at
request time (MEASURED, above), so the server would only serve precomputed data while
adding a store, an API, a deployment and an authentication problem for private code.
`docs/design.md` §7 already rejects a store at v1 on the same reasoning.

**One large self-contained HTML file as the only shape.** Rejected at scale; retained as
the small-graph special case above.

**GitHub Pages as the default delivery.** Rejected on the disclosure risk above. Available
as an explicit opt-in for open-source repositories.

**Module-sharded output.** Rejected: it cuts across the query pattern, requiring stitching
on every endpoint view.

**Kùzu as an embedded store.** MEASURED archived 2025-10-10 (`docs/design.md` §7), despite
being the best technical fit. Neo4j Community rejected separately: GPLv3, a JVM server,
and one database per install.
