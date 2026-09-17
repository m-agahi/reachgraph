# Plan 05 — `reachgraph-render-html`

**Status:** ready to build
**Date:** 2026-09-17
**Depends on:** plan-00 (traits), plan-01 (the waist and `GraphView`)
**Blocks:** plan-06

One crate, one trait impl: `Renderer` (plan-00 §3.6). It emits ADR-0006's root-sharded
static directory. It computes no reachability.

---

## 1. What this crate is and is not

**Is:** a serialiser. It receives an already-built, already-sharded `GraphView` and turns
it into JSON files, one HTML page, and the vendored JavaScript that page loads.

**Is not:** a graph engine. Reachability, the complement over all roots, sharding and
coverage are the waist (ADR-0003) and live in `reachgraph-core`. A renderer that
recomputes a reachable set has duplicated the thesis in a plugin, which ADR-0003 forbids.

**Rule, load-bearing for every section below:** *every classification the UI displays is
computed in Rust and written into the JSON. The JavaScript is a presenter over
pre-classified data.* Node category, module grouping, edge strength class, root slug,
version grouping and the unreachable set all arrive as fields. This is not stylistic — §8
explains that there is no browser test harness, so the untested surface is exactly the
JavaScript, and this rule is what keeps that surface small.

There is **one deliberate exception**, named in §6.3: version compare mode.

---

## 2. Renderer choice

**Cytoscape.js + `cytoscape-fcose`.** Carried from design.md §7, which survives ADR-0006
(that ADR supersedes §7's *single-file-only* artifact shape, not its renderer survey).

MEASURED (design.md §7): MIT, Canvas rendering, native compound/nested nodes, a real UMD
build published on cdnjs that works from one `<script>` tag with no build step.

The four properties are not interchangeable. **Native compound nodes** is what makes
design.md §7's collapsible module boxes a renderer feature rather than something we
implement. **A real UMD build** is what makes ADR-0001 survivable at all (§3).

### Rejected alternatives

| candidate | why rejected |
|---|---|
| **AntV G6** | The strongest alternative, and MEASURED (design.md §7) to have *better* native nesting — Combos with built-in expand/collapse. Rejected on licence and bundling posture rather than capability: Cytoscape's UMD-on-cdnjs story is the one we need for §3, and G6's advantage is in a feature we already get adequately. Revisit if compound expand/collapse proves painful. |
| **Sigma.js** | MEASURED (design.md §7): no compound-node support. Module boxes are the v0.1 UI. Disqualifying, not a tradeoff. |
| **Mermaid** | MEASURED (design.md §7): default `maxEdges` is **500**. The one hard number in the survey. A single handler measured 20 outgoing edges (design.md §8); a depth-3 shard exceeds 500 routinely. |
| **ELK adapter (`cytoscape-elk`)** for layout | Deferred, not rejected. elkjs is a GWT-compiled bundle whose size is an **OPEN MEASUREMENT** (§3), and §3 makes bundle size a binary-size question. fCoSE ships compound-aware force layout at a fraction of that. Reconsider if fCoSE's compound layout quality is unacceptable on a real shard. |

Scale is not the deciding axis. MEASURED (design.md §7): ~4000 symbols estate-wide is the
*index* size; a depth-limited drill-down puts a few hundred nodes on screen. Choose on
compound-node support and zero-build bundling.

---

## 3. ADR-0001 applies to the page — resolution

ADR-0001 says: no external binaries, no subprocesses, **no runtime downloads**, no system
package manager. The binary ships no Node and runs no build step. So the JavaScript the
emitted page needs must come from one of exactly two places.

| option | consequence |
|---|---|
| **A. Vendor.** The UMD bundles are committed in-tree, compiled into the binary with `include_str!`, and written to `out/vendor/*.js` at render time. | The artifact is self-contained and works offline, in an air-gapped CI runner, and from a downloaded workflow-artifact zip. Cost: bundle bytes are added to the binary, and the vendored JS becomes ours to patch for CVEs. |
| **B. CDN.** The page carries `<script src="https://cdnjs.cloudflare.com/...">`. | Zero binary cost. **The artifact is not self-contained offline.** It fails in an air-gapped runner, it fails when cdnjs is unreachable, it rots when a URL moves, and every viewer of a private-code graph makes a request to a third party. |

**Decision: A, vendor. No CDN option exists, not even behind a flag.**

Three reasons, in order of force:

1. **A CDN script tag is a runtime download.** It is performed by the browser rather than
   by the binary, but it is the same class of dependency ADR-0001 excludes, and it fails
   in the same places. Moving the fetch to a different process does not remove it.
2. **ADR-0006's privacy position.** The artifact is a structural map of private source.
   An artifact that contacts a third-party host every time it is opened is a worse default
   than one that does not, independent of what that host learns.
3. **Offline is the actual delivery path.** ADR-0006's private-repository path is a
   workflow-artifact zip that a reviewer downloads and opens. That reviewer may be on a
   plane, on a locked-down network, or reading a two-year-old archived artifact.

**What "real UMD build on cdnjs" buys us is not a CDN.** It is the existence of a
pre-built, dependency-free single file that we can vendor *once, at development time*,
with no npm and no bundler in the chain. cdnjs is where we obtain the file; it is not
where the page loads it from. That distinction is the whole of this section.

### Vendoring mechanism

Parallel to ADR-0001's `cargo vendor` policy, deliberately:

```
crates/reachgraph-render-html/vendor/
  cytoscape.min.js
  cytoscape-fcose.js
  cose-base.js          # fcose dependency
  layout-base.js        # cose-base dependency
  VENDOR.toml           # name, version, source URL, sha256, SPDX per file
```

- Files are committed, not fetched at build time. `build.rs` does no network IO. (The
  crate may have no `build.rs` at all; `include_str!` is enough.)
- An upstream bump is a deliberate re-vendor with a recorded sha256 change, never a
  silent float.
- **No minification, no re-bundling, no banner stripping runs over these files.** The
  `/*! ... MIT ... */` banner at the top of each bundle *is* the MIT notice, and §7
  requires it to travel into every emitted artifact.

**OPEN MEASUREMENT — vendored bundle set and size.** The exact fcose dependency chain and
the total byte cost are unverified. Both feed plan-07's binary-size budget and this plan's
§6.5 inline threshold.

```bash
# for each of cytoscape, cytoscape-fcose, cose-base, layout-base:
curl -sSL -o /tmp/x.js https://cdnjs.cloudflare.com/ajax/libs/<lib>/<version>/<file>
wc -c /tmp/x.js; sha256sum /tmp/x.js
# confirm the dependency set is exactly these four and nothing more:
#   read the fcose README's "dependencies" section at the pinned tag
```

**OPEN MEASUREMENT — elkjs size**, should the ELK adapter be reconsidered:
`curl -sSL https://cdnjs.cloudflare.com/ajax/libs/elkjs/<version>/elk.bundled.js | wc -c`.

---

## 4. Output layout

The renderer names relative paths; `OutputSink` owns where they land (plan-00 §3.6).

```
out/
  index.html            renderer + loader, no graph data
  endpoints.json        the root list, grouped — small
  graph/<slug>.json     one shard: the reachable set from one root
  unreachable.json      complement + coverage (ADR-0006, ADR-0007)
  vendor/*.js           §3
  overview.html         only when §6.5's threshold is met
```

### 4.1 Root slugs

`graph/<slug>.json` needs a filename derived from a root identity that is a five-tuple
(ADR-0007): `(contract_id, version, service, operation, direction)`.

Requirements, each of which has bitten a tool before:

- **Version must survive the slug.** ADR-0007's entire point. `CreateTask` and
  `CreateTask` at v2 must not collide.
- **Filesystem-safe.** `.`, `/`, `:` and `<>` appear in fully-qualified operation names.
- **Case-collision-safe.** macOS and Windows default to case-insensitive filesystems.
  `CreateTask` and `createTask` are different symbols and must be different files. Do
  **not** solve this by lowercasing — that merges them.

Scheme:

```
slug = sanitize(contract) "__" sanitize(version|"none") "__" sanitize(service)
       "__" sanitize(operation) "__" direction "__" hash8

sanitize: any char outside [A-Za-z0-9._-] → '-'
hash8:    first 8 hex chars of sha256 over the canonical tuple, joined by 0x1F
```

`hash8` is what makes the slug injective under case-insensitive comparison and under
sanitisation collapse. The human-readable prefix exists only so a directory listing is
readable.

**Collision surface, stated rather than left silent: `hash8` is 32 bits, and a slug
collision silently overwrites one root's shard with another's.** By the birthday bound that
is comfortable at the hundreds of roots this tool targets (design.md §7 MEASURED ~4000
symbols estate-wide; roots are a small fraction of that) and uncomfortable in the tens of
thousands. **Declared ceiling: 10 000 roots per artifact.** The renderer asserts uniqueness
across the emitted slug set and **fails the run** on a collision rather than overwriting —
one line of code, and it converts a silent corruption into an error. Widen `hash8` to 16
hex characters if the ceiling is ever approached.

**`endpoints.json` carries the slug → identity mapping. The page never reconstructs a
slug.** Round-tripping a slug in JavaScript would put identity parsing in the presenter,
which §1 forbids, and would silently break on the first name containing a `-`.

### 4.2 Schema version

Every emitted JSON carries `"schema_version": <n>` at the top level. ADR-0003 gives the
waist a version number from v0.1; a consumer must be able to reject an artifact it does
not understand. Bumping it is a deliberate act with a changelog entry.

### 4.3 `endpoints.json`

Grouped by `(contract, service, operation, direction)`, versions as siblings under that
group — ADR-0007's INFERRED consequence, made structural so the UI cannot present a flat
list where `CreateTask` appears twice with nothing distinguishing the entries.

```jsonc
{
  "schema_version": 1,
  "groups": [
    {
      "contract": "yadgar.task.v1.TaskService",     // the group's contract id
      "service": "TaskService",
      "operation": "CreateTask",
      "direction": "served",
      "join_key": "yadgar.task.v1.TaskService/CreateTask",   // opaque — §4.3.2
      "versions": [
        { "version": "v1",  "binding": "bound",   "slug": "…__v1__…__a1b2c3d4", "node_count": 41 },
        { "version": "v2",  "binding": "bound",   "slug": "…__v2__…__e5f6a7b8", "node_count": 38 },
        { "version": "v3",  "binding": "unbound", "slug": null, "node_count": null,
          "unbound_reason": "no `impl TaskService for T` method named `create_task`" },
        { "version": null,  "binding": "bound",   "slug": "…__none__…__0011aabb", "node_count": 12 }
      ]
    }
  ]
}
```

`"version": null` is emitted as JSON `null` and rendered as **"unversioned"**. ADR-0007:
a missing version is never defaulted to `"v1"`, and that rule extends to the presentation
layer — the string `v1` must not appear anywhere in the UI for a `null` version.

**Unbound roots appear here and are never omitted.** plan-00 §2 (amended 2026-09-17)
replaces `Root::node` + `Root::confidence` with `binding: RootBinding`, and plan-00 §6.2
makes `unbound_root_is_reported_not_dropped` an acceptance criterion: an `Unbound` root
reaches `endpoints.json` with its reason and produces **no shard**. There is no handler
node to traverse from, so `slug` and `node_count` are `null` — the absence of a shard file
is the correct output, not a missing one.

The UI renders an unbound version as a non-selectable row carrying its reason, styled as a
**reported gap**, not hidden and not greyed into invisibility. ADR-0007's partial-index
problem is the reason: an unbound root means a real handler may be sitting in the
unreachable list, and the reader can only detect that if the gap is on the screen.

#### 4.3.1 The plugins table

`endpoints.json` also carries `GraphView::plugins` — `PluginDescriptor { id,
position_encoding, capabilities }`, verbatim, one entry per analysis plugin:

```jsonc
"plugins": [ { "id": "rust", "position_encoding": "utf8-bytes",
               "capabilities": ["symbols", "edges", "classify"] } ]
```

**This is the artifact's plugins table, and this crate is what emits it.** plan-00 §3.6's
refusal rationale names it directly — a defaulted `Utf8Bytes` from a renderer would have
landed here looking like a declaration — so the table has to exist somewhere or that
argument points at nothing. It goes in `endpoints.json` because it describes the index as a
whole, not one root's reachable set and not the unreachable set.

It carries **analysis plugins only**. A renderer has no descriptor to emit (plan-00 §3.6),
including the renderer writing the file. `coverage.plugins` in `unreachable.json` (§4.5) is
the `PluginId` list from `IndexCoverage` and stays as it is — a narrower statement about
what the unreachability claim was computed against, not a duplicate of this table.

The UI surfaces `position_encoding` beside a node's offsets, because a span is meaningless
without knowing what unit it counts in (ADR-0003 field 2).

#### 4.3.2 `join_key` is an opaque label

`Root::join_key` (plan-00 §2) is spelled by the roots plugin and is opaque to everything
downstream, this renderer included. It is emitted verbatim, displayed verbatim as a
monospace subtitle on the group, and **compared only by equality**.

Three things this crate must never do with it, each of which would look reasonable:

- **Never split on `/` or `.`.** The `package/Operation` shape is gRPC's; a REST or
  GraphQL roots plugin spells its key differently, and code that slices it would produce
  garbage the moment a second roots plugin exists.
- **Never recover a version from it.** ADR-0007 puts the version in its own field
  precisely so nothing has to parse it back out. A `join_key` containing the substring
  `v1` is not evidence of a version, and a `version: null` root whose key happens to
  contain `v2` still renders as **unversioned**.
- **Never group by it instead of by the five-tuple.** Grouping (§4.3) is by
  `(contract, service, operation, direction)` with versions as siblings. `join_key` is a
  label on that group, not its identity.

Equality comparison is legitimate and is what the key is for: two shards carrying the same
`join_key` are the same contract operation, which is the cross-repository seam (ADR-0007).
v0.1 does not ship cross-repository stitching (ADR-0008), so nothing consumes that today —
the field is carried into the artifact so a later consumer has it.

### 4.4 `graph/<slug>.json`

The shard: nodes and edges of one root's reachable set. **The renderer never truncates it.**
Depth is a display control (§6.2), and a shard silently cut to the slider's default would
make the slider a lie above 3.

`Shard::depth_limit` (plan-01 §3) is the waist's, not this crate's, and both of its values
are handled:

- `None` — the walk was unlimited. The slider's maximum is `max_depth()`, and no node is
  frontier.
- `Some(n)` — the waist stopped at `n`. The slider's maximum is `n`, and the nodes in
  `Shard::frontier` are marked (§4.4.2) so the boundary reads as *"the walk stopped here"*
  rather than *"nothing is called from here"*. The shard's own limit is stated in the UI;
  it is not silently presented as the whole reachable set.

```jsonc
{
  "schema_version": 1,
  // a shard exists only for a `RootBinding::Bound` root; `handler` is its bound NodeId
  "root": { "contract": "...", "version": "v1", "service": "...",
            "operation": "...", "direction": "served", "handler": "rust:…" },
  "nodes": [
    {
      "id": "rust:crate/…/create_task",       // opaque (ADR-0003 field 3)
      "label": "create_task",
      "indexed": true,                        // Node::symbol.is_some()
      "doc_first_line": "Creates a task and returns its id.",
      "kind": "function", "raw_kind": "fn",
      "category": "first-party",              // null when no classifier was registered
      "box": "task::handlers",                // innermost compound parent, derived (§4.4.1)
      "unit": "crate:task",                   // outer box; null for an external node
      "file": "src/handlers.rs",              // present whenever indexed; null only if not
      "span": [1204, 1890],                   // null when the plugin has no offset
      "is_test": false,
      "depth": 1,                             // BFS depth from this root
      "frontier": false                       // Node::frontier — §4.4.2
    }
  ],
  "boxes": [ { "id": "task::handlers", "label": "handlers", "parent": "crate:task", "kind": "module" } ],
  "edges": [
    {
      "from": "rust:…", "to": "rust:…",
      "strength": "resolved",                  // §5, from InferenceMode
      "engine": "ra_ap_ide 0.0.352",           // Provenance.engine
      "plugin": "rust",
      "call_site": { "file": "src/handlers.rs", "span": [1310, 1322] }
    },
    {
      "from": "rust:…", "to": null,
      "unresolved": { "name": "save", "candidates": ["rust:…", "rust:…"] },
      "strength": "unresolved", "engine": "ra_ap_ide 0.0.352", "plugin": "rust"
    }
  ]
}
```

**`to: null` with an `unresolved` block is required, not optional.** design.md §8: *show a
missing edge as missing; never infer one to fill a hole.* An `EdgeTarget::Unresolved`
(plan-00 §2) reaching the renderer must survive into the JSON and onto the screen as a
stub, never be dropped and never be collapsed to its first candidate.

#### 4.4.1 Four fields that may be absent, and what each absence means

plan-01 §3 makes several `Node` fields `Option`, deliberately and against sentinels
(plan-00 §2: an absence must be sayable). **Each absence is a different statement and
gets a different treatment. None of them may be rendered as a zero, an empty string, or a
hidden node.**

| absent | what it means | treatment |
|---|---|---|
| `Node::symbol` is `None` | an edge resolved to this id, but no provider emitted a symbol for it — third-party and stdlib targets, and a cross-repo client stub in a repository that was never built (MEASURED, design.md §8) | `indexed: false`. The node is drawn, styled as unindexed, and is **never hidden** — deleting it would delete the cross-repo seam, which is the thesis. §4.4.3 covers its label. |
| `SourceRange::span` is `None` | the plugin knows the file, not the offset within it, and says so. **`file` is always present on an indexed symbol** (plan-00 §2) | `file: "src/handlers.rs", span: null`. The node still links to its **file**; only the jump-to-offset degrades to a jump-to-file, with a tooltip saying the plugin reported no offset. Never drop the file because the span is missing — that is the exact conflation plan-00 §2 removed when it moved the `Option` down a level. |
| `Node::category` is `None` | no classifier was registered for this node's plugin | neutral styling, and the category filter reports "unclassified" as its own bucket. Never silently folded into `third-party`, which would hide first-party code behind a default. |
| `Node::unit` is `None` | an external node; it was never indexed, so it belongs to no unit | drawn outside every unit box, not inside a synthetic "unknown" box. |

**The compound hierarchy is computed here, in Rust, and emitted as `boxes`.** Per §9.2:
the outer box is `Node::unit`; inner boxes come from walking `container_chain` and reading
each ancestor's `kind`/`raw_kind` to decide what kind of box it is. The chain is
cycle-guarded by the waist, so this crate does not re-guard it. The JavaScript receives a
finished parent link and interprets nothing — §1's rule.

#### 4.4.2 `frontier` is not a leaf

plan-01 §3, verbatim on the point: a node at the view's `depth_limit` with out-edges that
were not followed is **frontier, not leaf, and a renderer that draws them as leaves is
lying**.

A frontier node is drawn with an explicit "more beyond" affordance — a distinct border and
a badge — and its tooltip says its out-edges were not followed at this depth limit. This
is the same class of obligation as §5's edge-strength rule: the artifact records a
limitation and the page must show it rather than flatten it into an apparent fact.

Note that `Shard::depth_limit` may be `None` (unlimited), in which case no node is frontier
and the affordance never appears.

#### 4.4.3 Labelling an unindexed node

An unindexed node has no `Symbol`, therefore no `name`. The only string available is the
opaque `NodeId`.

**This crate renders that raw string verbatim and never splits it.** ADR-0003 field 3
forbids *parsing* a `NodeId`; displaying one is not parsing, and no `/` or `:` in it may be
given meaning — not to shorten the label, not to derive a module, not to guess a crate.
The node is visibly marked unindexed so a reader does not mistake the id for a name.

This is honest but ugly, and it is recorded as a known ergonomics gap in §9.3 rather than
fixed by inventing a display convention over an opaque string.

### 4.5 `unreachable.json`

```jsonc
{
  "schema_version": 1,
  "statement": "not reachable from any endpoint version in this index",
  // verbatim from GraphView::coverage (IndexCoverage, plan-01 §3). Re-spelled, never
  // recomputed and never summarised into fewer fields.
  "coverage": {
    "contracts": ["yadgar.task.v1.TaskService", "yadgar.task.v2.TaskService"],
    "versions": [["yadgar.task.v1.TaskService", "v1"], ["yadgar.task.v2.TaskService", "v2"]],
    "roots_total": 14, "roots_bound": 12,
    "unbound_roots": [ { "contract": "…", "version": "v3", "service": "…",
                         "operation": "…", "direction": "served", "reason": "…" } ],
    "units_indexed": ["crate:task", "crate:task-proto"],
    "plugins": ["rust", "roots-proto-tonic"],
    "traversal_terminal_categories": ["third-party", "stdlib"],
    "partial": false
  },
  "nodes": [ { "id": "…", "label": "…", "file": "…", "category": "first-party", "is_test": false } ]
}
```

`statement` is a literal in the artifact so that a downstream consumer — a PR-comment
renderer (ADR-0006), a dashboard — cannot re-word it into "dead". `coverage` is ADR-0006's
load-bearing cross-reference to ADR-0007 and is part of the artifact, not a log line.

**Two `IndexCoverage` fields weaken the unreachability claim and must reach the reader, not
only the file.** plan-01 §3 states both as obligations on the consumer, and this crate is
the consumer that a human actually looks at:

- **`partial: true`** — a provider failed and the run continued anyway. plan-01 §3: *a
  consumer must weaken every unreachability claim when this is set.* The panel renders a
  banner above the list, not a footnote: **"A provider failed during this run. This list is
  computed from an incomplete index and may name code that is reachable."** It is not
  dismissible.
- **`traversal_terminal_categories`** — the categories at which traversal stopped. Code
  reached only *through* a third-party or stdlib node was not followed, so a callback
  invoked by a third-party crate can appear in this list. The panel states which categories
  were terminal, in the same block as the coverage line (§6.4).

Both are declared limitations that the waist deliberately put in the artifact rather than
in a release note. Rendering the list without them would restore exactly the false
confidence ADR-0007 and design.md §8 are organised against.

---

## 5. Edge strength must be visually distinguishable

ADR-0003 field 4: `provenance` and `inference_mode` exist *because* a directly-resolved
edge and an inferred edge are different-strength claims **and must not render
identically**. This is the field's stated purpose. Rendering all edges the same line would
make the field decorative.

| `InferenceMode` | `strength` | line treatment | opacity |
|---|---|---|---|
| `Resolved` | `resolved` | solid, full weight | 1.0 |
| `TypeInferred` | `type-inferred` | long dash `[10,4]` | 0.9 |
| `Lexical` | `lexical` | short dash `[4,4]` | 0.75 |
| `Enclosure` | `enclosure` | dotted `[1,4]` | 0.6 |
| (unresolved target) | `unresolved` | dotted, to a `?` stub node | 0.6 |

Rules:

- **Distinguishable without colour.** Dash pattern and weight carry the signal; colour is
  redundant reinforcement. Artifacts get printed, screenshotted into greyscale PR
  comments, and read by people with colour-vision deficiency.
- **A legend is always rendered**, never behind a disclosure toggle. An unlabelled dash
  pattern communicates nothing.
- `engine` appears in the edge tooltip. "Which engine claimed this?" is the question
  `Provenance` exists to answer.
- **An edge's `call_site` carries the same optional span as a symbol's range** (plan-00
  §2: `SourceRange { file, span: Option<Span> }`). When `call_site.span` is `None`, the
  edge links to the **file** rather than to an offset, and the edge is drawn exactly as it
  would be otherwise. A missing offset says nothing about the strength of the call claim —
  that is `inference_mode`'s job — so it must not change the line treatment. Where
  `call_site` itself is `None`, the edge is drawn with no source link at all.
- **A strength filter is display-only.** Filtering to `resolved` only shrinks the drawn
  subgraph. It does **not** recompute the unreachable panel, which reports what
  `unreachable.json` computed over *all* edges. The filter control carries that sentence
  in the UI. This is ADR-0007's "never silently union versions" generalised: never let a
  display control silently misstate what was computed.

---

## 6. The page

One `index.html`, one `loader.js` (emitted, authored by us), the vendored bundles. No
framework, no bundler, no build step.

### 6.1 Load sequence

1. If the page contains `<script type="application/json" id="rg-data">`, use that
   (`overview.html`, §6.5).
2. Else `fetch("endpoints.json")`. Render the grouped endpoint list. No graph is drawn.
3. On endpoint selection, `fetch("graph/<slug>.json")`. Draw.
4. `unreachable.json` is fetched on first open of the unreachable panel.

Step 2 is why `serve` exists (ADR-0006): `fetch()` against `file://` is CORS-blocked. The
loader detects `location.protocol === "file:"` and, when no inline data is present,
renders the `serve` instruction instead of a silent failure — one blocked `fetch` with a
blank page is the worst version of this.

### 6.2 Module boxes, expand-on-click, depth

Carried verbatim from design.md §7 (stands under ADR-0006):

- **Collapsible module boxes** — Cytoscape compound parents, from the `boxes` array, which
  §4.4.1 derives in Rust from `Node::unit` (outer) plus the `container_chain` (inner).
  Third-party and stdlib categories collapse by default; first-party expands. design.md
  §8's MEASURED edge-noise table is the justification: of 20 edges from one handler, the
  useful ones separated by path prefix alone. A plugin emitting no container links yields
  unit-level boxes only, and that renders without a special case (§9.2).
- **Expand-on-click** on a collapsed module.
- **Depth slider, default 3.** Client-side filtering of the loaded shard on each node's
  precomputed `depth` field (`GraphView::depth_of`, plan-01 §3); nodes above the threshold
  are hidden, not deleted. Setting the slider to max shows the whole shard, because the
  renderer never truncates one (§4.4); where the *waist* truncated, the frontier marks say
  so. The slider is **absent** in the index-wide view, where `depth` is
  `None` for every node and the control would have nothing to mean (§9.2).
- **Frontier markers stay visible at every slider position** (§4.4.2). Hiding a node above
  the depth threshold is a display choice the user made; drawing a frontier node as a leaf
  is a false claim about the data, and the two must not be confused.

### 6.3 Version handling — the toggle

ADR-0007, binding: **the renderer must never silently union `v1` and `v2`.**

Default view: one group, **one version selected**. Grouping by operation with an explicit
version toggle is ADR-0007's own expected default.

- The version control is a segmented selector over that group's versions, plus a
  `Compare` entry when the group has two or more.
- **There is no "All versions" option.** An option that unions versions into one graph
  with no per-node attribution is exactly what ADR-0007 forbids, and naming it "All" does
  not make the union explicit.
- `Compare` is offered **only within one `(contract, service, operation, direction)`
  group** — which is what §4.3's grouping exists to express. Comparing `CreateTask` v1
  against `DeleteTask` v2 is not a question the three-way classification answers.

Compare mode renders ADR-0007's three-way classification:

| class | visual treatment | meaning (ADR-0007) |
|---|---|---|
| reachable from **v1 only** | solid border, left-hatched fill, `v1` badge | dies when v1 is sunset |
| reachable from **v2 only** | solid border, right-hatched fill, `v2` badge | new path |
| reachable from **both** | double border, plain fill, `v1 v2` badge | shared; survives the sunset |

Fill hatch, border style **and** a text badge — three redundant channels, for the §5
greyscale reason. A legend naming all three classes is always visible in compare mode, and
it states which two versions are being compared by name.

**This is the one deliberate exception to §1's thin-presenter rule.** Compare needs two
shards at once, so the classification is a **client-side set intersection of two
precomputed node sets** — not a traversal, and not a reachability computation. Precomputing
it in Rust would mean emitting O(versions²) pairwise files for a view that is usually not
opened. The exception is named here so the rule stays honest: the JavaScript may intersect
sets the waist computed; it may never compute a reachable set.

`"version": null` renders as **"unversioned"** and is a selectable sibling like any other.
It never participates in a compare against a named version as though it were one — the
badge reads `unversioned`, never `v1`.

### 6.4 The unreachable panel

Text, per design.md §7. A list, not a graph.

Binding wording, rendered verbatim as the panel heading:

> **not reachable from any endpoint version in this index**

ADR-0007's extension of design.md §8's rule. **The word "dead" must not appear in any
label, heading, tooltip, legend or template string this crate authors.** design.md §8:
telling someone to delete working code is the one failure that permanently destroys trust.

Directly beneath the heading, **not behind a disclosure**, the panel renders
`unreachable.json`'s `coverage`:

> Computed against 12 of 14 roots (2 unbound) across 2 contracts:
> `yadgar.task.v1.TaskService` (v1), `yadgar.task.v2.TaskService` (v2).
> Traversal stopped at third-party and stdlib nodes.

Three numbers, not one. `roots_bound` versus `roots_total` is what tells a reader that two
operations never bound to a handler, so the code behind them is in this list by
construction. The terminal-category sentence is `traversal_terminal_categories` (§4.5). If
`partial` is set, the §4.5 banner sits above all of this.

ADR-0007's partial-index correctness problem is why this is adjacent rather than hidden: if
the `v1` contract was never passed, every `v1`-only function appears in this list, and the
only thing that lets a reader detect that is seeing what the claim was computed against.
A reader who cannot see the covered set cannot evaluate the claim.

Each row also shows `category` and `is_test`, because design.md §8 names `pub` library
surface and test-only code as the Rust false-positive sources.

### 6.5 `overview.html` — the single-file case

ADR-0006: when the whole graph is under roughly 5 MB, **also** emit `overview.html` with
data inlined. Same renderer; the loader prefers inline data when present (§6.1 step 1).

- Threshold applies to the **serialised graph JSON payload**: `endpoints.json` + every
  shard + `unreachable.json`, concatenated. Default 5 MiB (`5 * 1024 * 1024`), overridable
  by plan-06's `--inline-threshold`.
- The emitted file is larger than the threshold by the vendored JS (§3, OPEN MEASUREMENT)
  plus the loader. That is stated in the plan so nobody later "fixes" the discrepancy by
  counting the JS into the budget and silently shrinking it.
- Over threshold: `overview.html` is not emitted. The sharded directory is always emitted,
  including in the small case. `overview.html` is an addition, never a replacement.

**Two inlining hazards, both real, both tested (§8):**

1. **`</script>` inside the data.** A doc comment containing `</script>` terminates the
   `<script type="application/json">` block and corrupts the page. Escape `<` as `<`
   throughout the inlined JSON (which also neutralises `<!--`). This is not theoretical —
   doc text is arbitrary source text.
2. **XSS via doc text.** Node labels and doc first lines are arbitrary repository content.
   All text reaches the DOM via `textContent` or Cytoscape's Canvas label rendering,
   **never** `innerHTML`. A repository whose doc comment contains `<img src=x onerror=…>`
   must not execute it in a reviewer's browser.

---

## 7. Licence notice must travel with the artifact

The vendored bundles are MIT (Cytoscape, fcose, cose-base, layout-base). **Emitting them
is distribution**, so the MIT notice must be present in the emitted artifact, not only in
the repository.

- `out/vendor/*.js` are written **byte-identical** to the vendored files, banner included.
  No minification step, no banner stripping, no concatenation pass.
- `overview.html` inlines the same bytes, banner included.
- `index.html` carries an HTML comment naming each bundle, its version and its SPDX id,
  plus a visible "Licences" entry in the UI listing them.

plan-07 §5 carries the repository-side attribution file. This section is about the
*generated* artifact, which is distributed to people who never see the repository.

---

## 8. Tests

Test-driven, per the standing project rule: each test below is written failing, before the
code that satisfies it. Tests run against `reachgraph-fixture` (ADR-0008, plan-02) — no
`cargo metadata`, no indexing, no built repository, no timing variance.

### 8.1 What cannot be tested, stated honestly

**There is no browser harness and v0.1 will not have one.** Nothing in this plan verifies
that Cytoscape draws the graph, that the version toggle switches, that the depth slider
filters, or that the compound boxes collapse. Claiming otherwise would be the exact
failure this project's premise forbids.

A headless browser (Playwright, `chromiumoxide`) is rejected for v0.1 on two grounds: it
is a large per-platform binary dependency in CI for a ~300-line presenter, and it is the
class of acquisition problem ADR-0001 exists to delete — permitted as a dev-dependency,
but the same cost shape. Revisit when the JavaScript grows past what review covers.

**The mitigations are structural, not aspirational:**

1. §1's thin-presenter rule makes the untested surface as small as it can be. Every
   classification is tested in Rust; the JavaScript reads fields.
2. §6.3's compare-mode intersection is the only logic in JavaScript that is not direct
   field reading, and it is named as the exception so it receives review attention.
3. A committed golden artifact (`tests/golden/out/`) is opened manually as a release
   checklist item (plan-07 §6). This is a human check and is recorded as one.

### 8.2 Emission tests — JSON structure

Snapshot-based (`insta`) over the fixture graph, so a schema change is a reviewable diff.

| test | asserts |
|---|---|
| `emits_adr0006_layout` | exactly `index.html`, `endpoints.json`, `graph/*.json`, `unreachable.json`, `vendor/*.js` reach the sink; no other paths |
| `one_shard_per_root` | shard count equals root count; every root has a shard |
| `v1_and_v2_are_separate_shards` | ADR-0007: same operation, two versions → two distinct files, two distinct slugs |
| `null_version_slug_is_not_v1` | a `None` version produces a slug containing `none`, and the string `v1` appears nowhere in that shard or its `endpoints.json` entry |
| `slug_is_injective_case_insensitively` | `CreateTask` and `createTask` roots produce slugs differing in more than case |
| `slug_collision_fails_the_run` | §4.1: two roots yielding the same slug produce an error, never a silently overwritten shard |
| `endpoints_groups_versions_as_siblings` | §4.3 shape: one group, versions nested |
| `unbound_root_is_emitted_with_reason` | plan-00 §6.2: an `Unbound` root appears in `endpoints.json` with `binding: "unbound"` and its reason, `slug: null`, and **no** file under `graph/` |
| `unresolved_edge_survives_to_json` | `EdgeTarget::Unresolved` with two candidates emits `to: null` + both candidates; is not dropped, is not resolved |
| `every_edge_carries_strength_and_engine` | ADR-0003 field 4 present on every emitted edge |
| `renderer_never_truncates_a_shard` | a fixture shard with `depth_limit: None` and depth-5 nodes emits all of them, with `depth` fields; a shard with `depth_limit: Some(2)` emits exactly what the waist supplied plus its frontier marks |
| `unreachable_carries_coverage` | ADR-0006/0007: every `IndexCoverage` field is re-spelled verbatim, including `roots_total`/`roots_bound`, `traversal_terminal_categories` and `partial` — none summarised away |
| `missing_span_keeps_the_file` | §4.4.1: `SourceRange::span == None` → `span: null` but `file` still emitted and still linkable; node visible |
| `unindexed_node_is_drawn_not_dropped` | §4.4.1: `Node::symbol == None` → `indexed: false`, node emitted, label is the verbatim `NodeId` |
| `node_id_is_never_split_for_display` | §4.4.3: a fixture id containing `/` and `:` reaches the label unmodified |
| `frontier_node_is_marked` | §4.4.2: a node in `Shard::frontier` emits `frontier: true`; with `depth_limit: None`, no node does |
| `container_chain_becomes_nested_boxes` | §4.4.1: a two-level fixture container chain emits two `boxes` entries with correct `parent` links, and the outer box is the `Unit` |
| `no_containers_degrades_to_unit_boxes` | §9.2: a fixture emitting no containers still renders, with one box per unit |
| `join_key_emitted_verbatim` | §4.3.2: a `join_key` with unusual spelling round-trips unmodified and is not used for grouping |
| `unreachable_statement_is_verbatim` | `statement` equals the binding wording exactly |
| `schema_version_on_every_json` | §4.2 |

### 8.3 HTML structure tests

Parsed with `scraper` (dev-dependency), asserting elements and attributes. **Not string
matching** — a string match on generated HTML passes on a page that would not render.

| test | asserts |
|---|---|
| `page_has_no_external_script_src` | **§3 made executable.** Every `<script src>` is relative; no `src` has a scheme or `//` prefix. This is the CDN prohibition as a build failure. |
| `page_references_every_vendored_bundle` | each `vendor/*.js` written is also referenced |
| `vendor_bytes_are_unmodified` | sha256 of each emitted `vendor/*.js` equals `VENDOR.toml`'s recorded hash |
| `licence_banner_survives_emission` | §7: the `/*!`…`MIT` banner is present in each emitted bundle **and** in `overview.html`'s inlined copy |
| `legend_present_for_edge_strength` | §5: a legend element exists naming all five strength classes |
| `unreachable_panel_shows_coverage_adjacent` | the coverage element is a sibling of the heading, not inside a `<details>`, and names bound/total roots and the terminal categories |
| `partial_index_renders_banner` | §4.5: `partial: true` → the weakening banner is present, above the list, outside any `<details>` |
| `unversioned_root_never_reads_v1` | §4.3.2: a `version: null` root whose `join_key` contains `v2` renders as `unversioned` |
| `no_all_versions_control` | §6.3: no control offers a union across versions |

### 8.4 Wording guard

`renderer_authors_no_dead_wording` — asserts the string `dead` (case-insensitive, word
boundary) appears in **no label, heading, tooltip, legend or template string authored by
this crate**.

**The scoping is deliberate and must not be widened.** A scan over the whole emitted byte
stream false-positives on the vendored Cytoscape bundle and on any analysed repository
whose doc text says "reaps dead sessions". A guard that cries wolf gets disabled, and then
there is no guard. The test therefore scans this crate's own `src/**` string literals and
HTML template files, not the output.

### 8.5 Inlining tests

| test | asserts |
|---|---|
| `overview_emitted_under_threshold` | fixture under 5 MiB → `overview.html` exists **and** the sharded directory also exists |
| `overview_absent_over_threshold` | fixture over threshold → no `overview.html`; shards unaffected |
| `script_terminator_in_doc_is_escaped` | fixture symbol whose doc contains `</script>` → emitted `overview.html` contains `</script`, and the JSON block parses |
| `doc_text_is_not_injected_as_html` | fixture doc containing `<img src=x onerror=alert(1)>` appears only inside the JSON data block, never as live markup |
| `inline_data_is_valid_json` | extract `#rg-data`, `serde_json::from_str` round-trips it |

### 8.6 Renderer-contract tests

| test | asserts |
|---|---|
| `renderer_implements_renderer_only` | compile-level `assert_impl_all!(HtmlRenderer: Renderer)` **and** `assert_not_impl_any!(HtmlRenderer: Plugin, LanguagePlugin)`. Plan-00 §3.6: `Renderer` does not extend `Plugin`, and the negative half is the load-bearing one — it fails the day someone re-adds the supertrait "for consistency". |
| `renderer_is_absent_from_the_analysis_registry` | building the analysis `Registry` and calling `detect` on any fixture repository never yields this crate. **This is what replaces the old `provides() == [Capability::Render]` assertion.** `Capability::Render` no longer exists (plan-00 §2); "this is a renderer" is now expressed by type — it implements `Renderer`, and it is reachable only through the separate renderer registry (plan-06 §3.1), never through detection. An output format is asked for, never detected from a repository. |
| `render_writes_only_through_sink` | the crate's non-test sources contain no `std::fs` write path. **Scoped to non-test, non-build-script first-party sources** — test fixtures legitimately touch the filesystem. |
| `render_html_has_no_core_dependency` | `cargo metadata`: `reachgraph-core` absent from this crate's dependency graph, direct and transitive (plan-00 §1's rule) |

---

## 9. Open questions — input to plan-00 and plan-01

Not worked around. Routed, because plan-00 is the contract and this plan may not edit it.

### 9.1 `Plugin`'s base trait did not fit a renderer — SETTLED

**Resolved 2026-09-17 in plan-00 §3.6. Recorded here as settled; not re-argued.**

The finding raised by this plan was accepted: `position_encoding`, `detection` and
`preflight` are meaningless for a renderer. A renderer analyses nothing, claims no
repository, and has no prerequisite to check. **`Renderer` no longer extends `Plugin`** —
it is `Renderer: Send + Sync` keeping `id()` for attribution and feature naming.

**The remedy this plan proposed — provided-method defaults on `Plugin` — was refused, and
the reason is worth carrying here rather than leaving in the other document.** A default
is a *value*, and a meaningless value is indistinguishable downstream from a meant one: a
defaulted `Utf8Bytes` from a renderer would land in `PluginDescriptor` and the artifact's
`plugins` table (plan-01 §3) as though the renderer had declared it. That is the same
defect as `confidence: f32` on `Root` and a sentinel `Span {0,0}` on `Symbol` — the third
instance of one failure, refused by construction rather than by care.

Consequences that land on this crate:

- **`Capability::Render` is gone.** There is no capability to declare, so §8.6's test
  asserts the type instead.
- **The analysis `Registry` cannot return a renderer**, and `Registry::detect` therefore
  needs no capability filter. Renderer selection is explicit, through a separate renderer
  registry (plan-06 §3.1). An output format is asked for, never detected.
- **The empty-`Detection` ambiguity is moot for this crate** — it declares no `Detection`
  at all. Plan-00 §2 settled the general question anyway: **an empty `marker_files`
  matches nothing, never everything**, because the inverse would let one half-written
  plugin hijack detection for every repository.

The generalisation still stands and is recorded for the next renderer (ADR-0002 names
five: static HTML, inlined HTML, JSON, DOT, SARIF, PR comment): the analysis traits and
the output traits are different families, and ADR-0002 never required them to share a
supertrait.

### 9.2 `GraphView`'s accessor set — GRANTED

**Resolved 2026-09-17 in plan-01 §3.** `GraphView` is defined in `plugin-api` with the
accessors this plan asked for, as inherent methods on owned data — lookups, not algorithms
— which is what keeps this crate off `reachgraph-core` (§8.6).

Both items flagged as likeliest to be missing arrived:

- **Depth slider:** `depth_of` / `max_depth` / `nodes_at_depth`. `Node::depth` is
  `Option<u32>` — `Some(_)` in a shard view, `None` in the index-wide view, where there is
  no single root to measure from. The slider is rendered **only** for a shard view; in the
  index-wide view there is nothing for it to mean and it is absent, not zeroed.
- **Compound boxes:** `container_of` / `container_chain` / `contained_in` /
  `nodes_in_unit`, plus `Node::unit`.

**The division of labour on nesting, which §6.2 must match exactly.** The waist follows
containment links by *equality* and never interprets them. Deciding that an ancestor is a
module rather than a class is **this crate's job**, from `kind` and `raw_kind`:

| level | source | this crate's part |
|---|---|---|
| outer box | `Node::unit` | one box per `UnitId`; externals have `unit: None` and sit outside every unit box |
| inner boxes | the `container_chain`, nearest first | read each ancestor's `kind`/`raw_kind` to decide whether it is a module box, a type box, or not a box at all |

That interpretation is correctly placed: a renderer is a plugin, so plugin-side reading of
`raw_kind` is exactly where plan-00 §8 question 3 puts it. The waist stays unable to read
what it carries.

Degradation is defined rather than exceptional. A plugin that emits no container links
yields unit-level grouping only — a worse diagram, not a broken one — and this crate must
render that without a special case.

Two further fields arrived that this plan had not asked for and both change the output;
they are absorbed in §4.4 (`Node::frontier`, `Node::symbol`) and §4.5
(`IndexCoverage::partial` and `traversal_terminal_categories`).

### 9.3 Deferred within this plan

- **An unindexed node has no display name, only an opaque `NodeId`** (§4.4.3). Third-party
  and stdlib targets are the common case, and a cross-repo client stub is the important
  one — design.md §8 MEASURED that stub edge as the cross-repository seam, which is the
  thesis. Rendering `rust:crate/…/…` verbatim next to `create_task` is honest and ugly.
  **Not fixed here, because every fix is a display convention over a string ADR-0003 field
  3 makes opaque.** If it proves bad enough on a real graph, the right shape is a
  plugin-supplied display hint on `Node` — a value the plugin *means*, not one the
  renderer derives — and that is a plan-00 change, not a plan-05 one. Flagged, not
  designed.

- **fCoSE compound layout quality on a real shard.** Unknown until plan-03 produces one.
  If it is unacceptable, §2's ELK deferral reopens against the elkjs size measurement.
- **Shard size for a wide root.** A root reaching 2000 nodes is one large JSON fetch. No
  evidence yet that it is a problem; do not pre-optimise into per-depth sub-shards.
- **Doc-comment quality.** design.md §9 Q1 is still the project's one unverified premise:
  if the doc text is not good enough to label a box with, §4.4's `doc_first_line` degrades
  to a name-only label. That is a premise risk, not a renderer risk, and this plan
  tolerates an absent doc without a layout change.
