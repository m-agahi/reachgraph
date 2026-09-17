# Plan 01 — `reachgraph-core`, the waist

**Status:** ready to build
**Date:** 2026-09-17
**Amended:** 2026-09-17 — `GraphView` defined with the accessors plan-05 §9.2 needs (§3,
§3.1); open questions 4 and 5 resolved (§11); `join_key` in the schemas (§8); the
walk-cost measurement corrected (§8.7). Then: `SourceRange { file, span: Option<Span> }`
lands, so the `unit.root` classification fallback is deleted and §7.0 replaces it;
question 8 opened.
**Depends on:** plan-00 (the trait contract), ADR-0003, ADR-0006, ADR-0007
**Parallel with:** plan-02 (the fixture plugin) — see plan-00 §7
**Blocks:** plan-03, plan-04, plan-05

Per ADR-0003 this crate is the fixed point. Graph construction, reachability and sharding
are not plugins, are not swappable, and are the tool's thesis. Everything else in the
workspace plugs into what this plan builds.

Every factual claim below is labelled **MEASURED** or **INFERRED**. Design choices carry
neither label — they are decisions, not observations.

---

## 1. Scope

**In:** graph assembly from provider output, node/edge storage and interning, provider
pairing, reachability (depth-limited and unlimited), the unreachable complement,
per-version classification, classification via the `Classifier` trait object, shard
extraction, and the artifact JSON schema.

**Out:** anything language-specific (plan-03), contract parsing (plan-04), HTML (plan-05),
the CLI and `serve` (plan-06). This crate never names a language, a framework, a file
extension or a path prefix.

**Never in, at any version:** a branch on which plugin produced a node, a parse of
`NodeId::raw`, a read of `Symbol::container`, a read of `Symbol::raw_kind`, or a version
string extracted from a route (ADR-0007).

---

## 2. Crate layout

```
reachgraph-core/src/
  lib.rs        public surface: Index::build, Index::shards, Index::emit
  intern.rs     NodeIdx, Interner — dense indices over opaque NodeIds
  graph.rs      Graph, NodeRecord, EdgeRecord, adjacency
  assemble.rs   provider orchestration, pairing, diagnostics
  reach.rs      BFS, depth limiting, complement
  versions.rs   per-version reachability and classification (ADR-0007)
  classify.rs   applying Classifier trait objects
  shard.rs      root slugging, shard extraction
  emit.rs       artifact writing through OutputSink
  diag.rs       BuildDiagnostic
```

The assembly module is `assemble.rs`, not `build.rs`: a file called `build.rs` at a crate
root is a Cargo build script.

Dependencies: `reachgraph-plugin-api`, `serde`, `serde_json`. Nothing else in v0.1. No
`ra_ap_*` (that is a plan-00 §6.1 neutrality test), no language crate, no plugin crate.

---

## 3. Types this plan contributes to `plugin-api`

Plan-00 §1 names `Node`, `GraphView` and `Shard` as residents of `plugin-api` — a renderer
is a plugin and must receive the graph — but defines only `Symbol`, `Edge` and `Root`.
This plan fills the gap. **The types go in `plugin-api`; the algorithms over them go in
`core`.** That split is plan-00 §1's, restated because it is easy to get backwards.

```rust
/// A node in the built graph. Not every edge target was indexed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    /// `None` = an edge resolved to this id, but no provider ever emitted a symbol for
    /// it. Third-party and stdlib targets land here, and so does a cross-repo client
    /// stub in a repository that was never built (MEASURED, design.md §8: the stub edge
    /// resolved only because `target/debug/build/…/out/` existed).
    pub symbol: Option<Symbol>,
    /// `None` = no classifier was registered for this node's plugin.
    pub category: Option<Category>,
    /// The unit this node's symbol came from. `None` for an external node — it was
    /// never indexed, so it belongs to no unit. The outermost grouping level.
    pub unit: Option<UnitId>,
    /// BFS distance from this view's root. `Some(0)` is the root itself.
    ///
    /// `None` in the index-wide `GraphView`, where there is no single root to measure
    /// from, and `Some(_)` in every shard view. The distinction is in the type because
    /// a renderer asking "how deep is this node" must get an answer that is wrong in
    /// neither direction — plan-05 §9.2.
    pub depth: Option<u32>,
    /// True when this node sits at the view's `depth_limit` and has out-edges that were
    /// not followed. Frontier, not leaf (§5.2).
    pub frontier: bool,
}

/// What a plugin declared about itself, carried into the artifact so a consumer can
/// interpret offsets and attribute edges without a second lookup.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PluginDescriptor {
    pub id: PluginId,
    pub position_encoding: PositionEncoding,
    pub capabilities: Vec<Capability>,
}

/// What a renderer receives. Plan-00 §1 names it; this is its definition.
///
/// Construction is `core`'s (§4). The accessors below are inherent methods on owned
/// data — lookups, not algorithms — which is what keeps a renderer from needing
/// `reachgraph-core` as a dependency. A renderer that had to recompute BFS depth to
/// draw a depth slider would need the traversal, and the dependency rule would break.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphView {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub roots: Vec<Root>,
    pub plugins: Vec<PluginDescriptor>,
    pub coverage: IndexCoverage,
    /// Lazily built on first lookup, never serialized. Rebuilt after deserialization,
    /// so a view read back from a shard file behaves identically to one just built.
    #[serde(skip)]
    index: OnceLock<ViewIndex>,
}

impl GraphView {
    // ---- identity lookup -------------------------------------------------
    pub fn node(&self, id: &NodeId) -> Option<&Node>;

    // ---- plan-05 §9.2, the depth slider ----------------------------------
    /// BFS distance from this view's root. `None` in the index-wide view.
    pub fn depth_of(&self, id: &NodeId) -> Option<u32>;
    /// Greatest `depth` present. What the slider's maximum is set from.
    pub fn max_depth(&self) -> Option<u32>;
    /// Every node at exactly this depth.
    pub fn nodes_at_depth(&self, depth: u32) -> Vec<&Node>;

    // ---- plan-05 §9.2, compound / nested boxes ---------------------------
    /// The node's enclosing definition, if its plugin declared one. A pure link
    /// follow: `Symbol::container` resolved through this view. The waist does not
    /// interpret what the container *means* — the caller reads `kind` / `raw_kind`
    /// and decides. Returns `None` when the container was never indexed.
    pub fn container_of(&self, id: &NodeId) -> Option<&Node>;
    /// The full chain outward, nearest first, terminating at a node with no container
    /// or an unindexed one. Cycle-guarded: a plugin that emits a containment loop gets
    /// a truncated chain, never a hang.
    pub fn container_chain(&self, id: &NodeId) -> Vec<&Node>;
    /// Direct containment children — the inverse of `container_of`.
    pub fn contained_in(&self, id: &NodeId) -> Vec<&Node>;
    /// Nodes grouped by `Unit`, the outermost box. Externals are excluded; they
    /// belong to no unit.
    pub fn nodes_in_unit(&self, unit: &UnitId) -> Vec<&Node>;

    // ---- adjacency -------------------------------------------------------
    pub fn out_edges(&self, id: &NodeId) -> Vec<&Edge>;
    pub fn in_edges(&self, id: &NodeId) -> Vec<&Edge>;
}

/// One root's reachable subgraph. ADR-0006: a shard is the reachable set from one root.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Shard {
    pub root: Root,
    pub depth_limit: Option<u32>,
    /// Nodes at `depth_limit` that have out-edges not included here. They are frontier,
    /// not leaf, and a renderer that draws them as leaves is lying.
    pub frontier: Vec<NodeId>,
    pub view: GraphView,
}

/// The aggregate of every `RootProvider::coverage()` plus what the core itself observed.
/// ADR-0007's binding requirement 1: the index records what it covered, in the artifact,
/// not in a log line.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IndexCoverage {
    pub contracts: Vec<ContractId>,
    /// Every `(contract, version)` pair the index looked at. `None` is a real entry.
    pub versions: Vec<(ContractId, Option<String>)>,
    pub roots_total: usize,
    pub roots_bound: usize,
    pub unbound_roots: Vec<UnboundRoot>,
    pub units_indexed: Vec<UnitId>,
    pub plugins: Vec<PluginId>,
    /// Categories at which traversal stopped (§7.1). A **declared limitation**, in the
    /// artifact rather than in a release note: code reached only *through* a node of
    /// one of these categories was not followed, so a consumer can see that the
    /// unreachable set was computed against a deliberately truncated walk.
    pub traversal_terminal_categories: Vec<Category>,
    /// True when any provider failed and the run continued anyway. A consumer must
    /// weaken every unreachability claim when this is set.
    pub partial: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnboundRoot {
    pub contract: ContractId,
    pub version: Option<String>,
    pub service: String,
    pub operation: String,
    pub direction: Direction,
    pub reason: String,
}
```

`Coverage` (plan-00 §2) stays as it is: it is one provider's claim about what *it* looked
at. `IndexCoverage` is the aggregate. Two types, because a provider cannot know what the
other providers did, and the artifact needs the union.

### 3.1 Nesting, without a new `Symbol` field

Plan-05 §9.2 needs module parent links for Cytoscape's compound nodes — the nesting is
most of why ADR-0006 chose that renderer. The links come from two fields that already
exist, and **no `Symbol::module` field is added**:

| level | source | Rust | Go | Java |
|---|---|---|---|---|
| outer box | `Node::unit` | crate | package | source root |
| inner boxes | the `container_of` chain | `impl` block, then module, then crate root | receiver type | class, then inner class |

`Symbol::container` is "the enclosing definition", and a module is an enclosing
definition — `SymbolKind::Module` exists precisely for it. So the containment chain is
the nesting chain, already, for any plugin that emits module symbols and links to them.

Two consequences, both worth stating because they are easy to get wrong later:

- **The waist still never interprets `container`.** `container_of` follows a `NodeId`
  by equality, which is a lookup, not a reading. Deciding that an ancestor is a module
  rather than a class is done by the *renderer*, from `kind` and `raw_kind` — and a
  renderer is a plugin, so plugin-side interpretation is exactly where that belongs
  (plan-00 §2, §8 question 3).
- **It is an obligation on the language plugins.** `lang-rust` must emit
  `SymbolKind::Module` symbols and set `container` up the chain, or the renderer gets one
  flat box per crate. That is a plan-03 requirement, and a fixture case covers the shape
  (plan-02 §6, `nested_containers`).

A language whose plugin emits no containers degrades to unit-level grouping. That is a
worse diagram, not a broken one, and it is the correct outcome for a language that has no
nesting to report.

### Serde rules for every artifact type

Binding, and the same rule plan-02 applies to the fixture format:

- `#[serde(deny_unknown_fields)]` everywhere.
- **No `#[serde(default)]` anywhere.** `Root::version` in particular: a missing key is a
  parse error, never a silent `None`. ADR-0007 requires `None` to be an assertion by the
  plugin, not an absence, and a serde default is precisely the mechanism by which a
  round-trip could invent `"v1"` or erase a deliberate `None`.
- `provenance` and `inference_mode` on `Edge` are non-`Option`. A consumer cannot obtain
  an edge without them (ADR-0003 field 4).

---

## 4. Graph construction

### 4.1 Interning, and how `NodeId` opacity survives it

The core needs dense integer indices for traversal. It must not gain any knowledge of
`NodeId::raw` in the process.

```rust
/// Dense index into Graph::nodes. Internal to core; never serialized, never in plugin-api.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct NodeIdx(u32);

pub(crate) struct Interner {
    by_id: HashMap<NodeId, NodeIdx>,
    ids: Vec<NodeId>,
}

impl Interner {
    /// The ONLY operation the core performs on a NodeId: hash it, compare it for
    /// equality, clone it, emit it. Never parse it, split it, prefix-match it, or
    /// order it by content.
    pub(crate) fn intern(&mut self, id: &NodeId) -> NodeIdx { /* … */ }
}
```

`NodeId` derives `Hash` and `Eq` over `(plugin, raw)` — plan-00 §2 already provides both.
Equality is byte equality of the whole pair. Two plugins may emit the same `raw` string
without collision, because `plugin` is half the key.

The opacity rule is testable, not merely stated. See `node_id_opacity_bijective_rename`
(§10): apply any bijection to every `raw` string in a fixture and the built graph must be
isomorphic, with identical reachable sets, identical classification and identical shard
*contents*. Only the shard *filenames* may differ, and §8.1 explains why they must not.

`NodeIdx` never crosses the crate boundary and is never serialized. Indices are assignment
order, which is provider iteration order, which is not stable across runs — serializing
one would leak a build detail into the artifact.

### 4.2 Provider pairing

```rust
pub struct Index { /* … */ }

impl Index {
    pub fn build(
        root: &Path,
        plugins: &[&dyn Plugin],
        opts: &BuildOptions,
    ) -> Result<Index, BuildError>;
}
```

Order:

1. **Preflight.** Call `preflight(root)` on every detected plugin. Any `Failed { reason,
   remediation }` aborts with both strings surfaced. ADR-0003 field 5 — never
   `command -v`; MEASURED, design.md §10, a name resolving on PATH proves nothing.
2. **Partition by capability.** From `provides()`: symbol providers, edge providers, root
   providers, classifiers.
3. **Pair by `PluginId`** (plan-00 §3.1). For each plugin declaring `Symbols`, the edge
   provider with the same `PluginId` is its partner.
4. **Discover units** once per `PluginId`, via `LanguagePlugin::discover_units`. The same
   `Unit` values go to both halves of the pair — that is the whole point of pairing, and
   it is ADR-0003 field 1's "invoked once, rather than re-indexing a repository twice".
5. **Collect symbols** per unit, then **collect edges** per unit.
6. **Classify** (§7), **bind roots** (§4.4), **compute reachability** (§5, §6).

**An unpaired provider is a hard error, not a warning.**

| case | result |
|---|---|
| declares `Symbols`, no `Edges` partner | `BuildError::UnpairedSymbolProvider` |
| declares `Edges`, no `Symbols` partner | `BuildError::UnpairedEdgeProvider` |

The first is the load-bearing one. A symbol provider with no edges yields a graph of
isolated nodes, so **every symbol is unreachable** — and the tool then reports an entire
repository as not reachable from any endpoint. That is exactly the false-positive class
ADR-0007 calls a correctness problem and design.md §8 calls the most dangerous claim the
tool can make. Failing loudly is the only defensible behaviour. The second case produces
edges whose every endpoint is an unindexed node, which is meaningless rather than
dangerous, but is still a configuration error.

A provider that returns `Err(PluginError)` aborts the build by default. `BuildOptions::
allow_partial` continues and sets `IndexCoverage::partial`. See open question 3 (§11).

### 4.3 What becomes a node

- Every `Symbol` from every symbol provider becomes a node with `symbol: Some(_)`.
- Every `EdgeTarget::Resolved(id)` naming an id no provider emitted becomes a node with
  `symbol: None` — an **external node**.
- `EdgeTarget::Unresolved` creates **no node at all**. There is no target to create one
  for; that is what unresolved means.
- `Symbol::container` creates **no node and no edge**. Containment is not a call. The
  field is stored on the symbol, emitted verbatim, and never followed by the core. A
  `container` naming an id that no provider emitted is not an error — it is plugin data
  the waist does not interpret (plan-00 §2, §8 question 3).

Duplicate symbols — the same `NodeId` emitted twice — take the first and record
`BuildDiagnostic::DuplicateSymbol`. The core does not merge fields, because merging would
require deciding which plugin's `doc` or `raw_kind` wins, and that is plugin knowledge.

### 4.4 Roots

`RootProvider::roots(repo_root, &dyn SymbolIndex)` runs after symbols are collected. The
core supplies a `SymbolIndex` over the interned symbol set and does nothing else:

- `RootBinding::Bound(node)` naming an unindexed id → the root is retained, the node is
  created as external, and `BuildDiagnostic::RootBoundToUnindexedNode` is recorded. The
  shard exists but reaches nothing, which is a true statement and a visible one.
- `RootBinding::Unbound { reason }` → the root is retained, **produces no shard**, and is
  copied into `IndexCoverage::unbound_roots` with its reason.

An unbound root is never dropped. Dropping it would make a real endpoint invisible, and
would make whatever it would have reached appear unreachable — ADR-0007's partial-index
problem, self-inflicted.

---

## 5. Reachability

```rust
pub struct ReachResult {
    pub reached: HashSet<NodeIdx>,
    /// Nodes at the depth limit with out-edges not followed.
    pub frontier: HashSet<NodeIdx>,
}

pub(crate) fn reachable_from(
    g: &Graph,
    start: NodeIdx,
    depth: Option<u32>,
    filter: &TraversalFilter,
) -> ReachResult;
```

Plain BFS with a visited set. Cycles terminate because the visited set is checked before
enqueue — MEASURED-free, this is standard, but `cycle_terminates` is a test because the
fixture corpus contains one (plan-02).

### 5.1 Depth is a view parameter, not a reachability parameter

Two different depths, and conflating them is a correctness bug:

| computation | depth |
|---|---|
| shard contents | `opts.depth`, default 3 (design.md §7) |
| the unreachable complement | **always unlimited** |

A depth-limited complement reports everything past depth 3 as unreachable. That is a
guaranteed false positive on every deep call chain, and it is the one failure design.md §8
says permanently destroys trust. `unreachable_uses_unlimited_depth` is a test, not a
convention.

### 5.2 The frontier is not a leaf

A node at the depth limit that still has out-edges goes into `Shard::frontier`. The
renderer must be able to distinguish "this function calls nothing" from "we stopped
looking here". Emitting the two identically reproduces, in the UI, the same class of lie
the wording rule exists to prevent.

### 5.3 Unresolved edges do not propagate reachability

`EdgeTarget::Unresolved { name, candidates }` never advances the BFS. Following a
candidate would be inferring an edge to fill a hole — design.md §8's binding rule, and
exactly what plan-00's `EdgeTarget` enum exists to prevent.

The honest consequence is a false-positive risk in the other direction: a node reachable
only through an unresolved call lands in the unreachable set. The core does not resolve
that by guessing. It annotates:

- Every unresolved edge is carried into the shard of the root that reaches its source,
  with its `candidates` list intact.
- Any node appearing as a `candidate` of an unresolved edge whose *source* is reachable is
  marked `possibly_reachable_via_unresolved: true` in `unreachable.json`.

**That annotation must never remove a node from the list.** It is a caveat attached to a
reported node, not a filter. An implementation that drops flagged nodes has silently
re-introduced the inference the enum forbids.

### 5.4 The complement

```
unreachable = all_indexed_nodes − ⋃ reachable_from(root, unlimited)  for every bound root
```

External nodes (`symbol: None`) are excluded from `all_indexed_nodes`. The tool cannot
claim that code it never indexed is unreachable — it has no evidence either way.

**No category filtering happens here.** Every indexed node that is not reached is listed,
with its `category` attached, plus `counts_by_category` for a consumer that wants a
summary. Filtering stdlib or third-party out of the list is a *view* decision and belongs
to the renderer. The waist reports; it does not suppress. This also keeps
`category: None` nodes — those whose plugin registered no classifier — from vanishing
without trace.

---

## 6. Per-version reachability (ADR-0007)

### 6.1 Version keys

A **version key** is `(ContractId, Option<String>)`. Roots are partitioned by it. `None`
is a key like any other, and is never merged with, coerced to, or displayed as `"v1"`.

Two roots with the same operation name and different version keys are two roots, two
shards and two reachable sets (ADR-0007). Two roots with the same operation name, one
versioned and one not, are likewise separate — `missing_version_stays_none` and
`unversioned_and_versioned_roots_do_not_merge` are separate tests because they fail
separately.

### 6.2 Classification

For each indexed node, the set of version keys that reach it, as a bitset over the version
key list:

```rust
pub struct VersionReach {
    pub keys: Vec<(ContractId, Option<String>)>,
    /// index-aligned with the node list; bit i set = version key i reaches this node
    pub per_node: Vec<FixedBitSet>,
}
```

ADR-0007's three-way table is the two-key instance of this:

| bits set | class | ADR-0007 meaning |
|---|---|---|
| `{v1}` | `v1_only` | dies when `v1` is sunset |
| `{v2}` | `v2_only` | new path |
| `{v1, v2}` | `both` | shared; survives the sunset |
| `{}` | not reached | belongs to the unreachable complement |

`class` is emitted only when the contract has exactly two version keys. With three or
more, `reached_by` is the truth and a two-valued label would be a lie. This generalises
without a special case, which is why the bitset is the representation and the three-way
table is the presentation.

### 6.3 What the artifact must record

ADR-0007's binding requirements, mapped to files:

| requirement | where |
|---|---|
| the index records which contracts and versions it covered | `IndexCoverage`, in `unreachable.json` and `endpoints.json` |
| the wording rule | `unreachable.json.claim`, verbatim, as data |
| `unreachable.json` carries the covered-root set | `IndexCoverage::{versions, roots_total, roots_bound, unbound_roots}` |

### 6.4 The wording rule is data, not prose

```json
"claim": "not reachable from any endpoint version in this index"
```

The string ships in the artifact. A renderer displays it; it does not compose its own.
The word "dead" appears nowhere in this crate — not in a type name, a field name, a
variant, a doc comment or a test name. `unreachable_claim_string_is_verbatim` asserts the
string, and a second test greps the crate for the word.

This is stronger than a convention in a style guide because ADR-0006 makes renderers
plugins: a renderer is written by someone who may not have read ADR-0007, and handing it
the sentence is cheaper than hoping.

---

## 7. Classification

```rust
// core, classify.rs
fn category_for(node: &Node, classifiers: &[&dyn Classifier], unit: &Unit) -> Option<Category>
```

The core holds `&dyn Classifier` and calls `classify(path, unit)`. It never calls a
language crate function, never matches on `PluginId` to pick behaviour, and never contains
a path prefix. Prefixes — `src/`, `target/*/out/`, `/nix/store/…rust-lib-src/`, the cargo
registry path — live in the plugin, because design.md §8 MEASURED that they are
Rust-specific *and* machine-specific.

Selection: the classifier with the same `PluginId` as the node's plugin. No classifier for
that plugin → `category: None`, plus `BuildDiagnostic::NoClassifierForPlugin`. Not an
error: classification is an ADR-0002 plugin kind, and a plugin may legitimately not
provide it.

**Which path is passed.** `Symbol::range` is required and `SourceRange::file` is required
(plan-00 §2), so **every indexed node has a real path** and classification is always at
file granularity. `SourceRange::span` may be `None`; classification never reads the span.

| node | path |
|---|---|
| indexed | `symbol.range.file` — always present |
| external (`symbol: None`) | none; `category: None` |

The `unit.root` fallback and its `BuildDiagnostic::ClassifiedByUnitRoot` are **gone**.
They existed only while `Symbol::range` was `Option<SourceRange>`, which forced a plugin
to discard a file it knew in order to be honest about an offset it did not. Moving the
`Option` down to `span` removed the cause, so the compensation goes with it — a fallback
kept after its cause is fixed is a path nothing exercises and every later reader
misreads.

Confirmed restored: two symbols in **one unit**, one under `src/` and one under
`vendor/`, classify as `FirstParty` and `ThirdParty` respectively. The fixture case
`foreign_shapes` (plan-02 §6) no longer needs one unit per prefix to test this.

### 7.0 External nodes are the one genuinely pathless case

An external node was never indexed, so no plugin ever gave it a file, so it cannot be
classified. `category: None`.

That matters more than it looks, because §7.1 terminates traversal at `ThirdParty` and
`Stdlib` — and an unclassified node terminates nothing. **The obligation is on the
providers:** a provider must emit a `Symbol` for any node it names as an edge target
whenever it knows where that target lives, even when the target sits outside the units it
was asked to enumerate. MEASURED, design.md §8, the information is there to emit — all 20
edges from one handler carried a resolvable path, including `/nix/store/…rust-lib-src/`
and cargo registry targets.

An external node therefore means "the provider could not locate this at all", which
should be rare. Whether `ra_ap` supplies out-of-workspace target locations cheaply enough
to honour that obligation is open question 8 (§11) and belongs to plan-03.

`classifier_invoked_through_trait_object` is a test, using a spy classifier that records
its calls. Plan-00 §3.5 states why: a core that calls into `lang-rust` directly has
re-created ADR-0008's forbidden `if is_rust_project(root)` in a different costume, and the
trait boundary is what keeps the v0.1 fold into `lang-rust` reversible.

### 7.1 Category affects traversal, never the node list

design.md §8 MEASURED that useful edges separate from noise by path prefix alone — 20
edges from one handler, where `/nix/store/…rust-lib-src/` supplies `pin`, `map`, `trim`,
`Ok`, `Err`, `Some`.

The core's response is **terminal, not deleted**:

- `Stdlib` and `ThirdParty` nodes are traversal-terminal by default. They appear in the
  shard, the edge to them appears, and the BFS does not expand through them.
- The edge is never dropped. A dropped edge is indistinguishable from an edge that was
  never found, and design.md §8's rule is to show a missing edge as missing.
- `BuildOptions::expand_categories` overrides which categories terminate.

**This terminates the walk in v0.1, and the limitation is declared in the artifact.**
`IndexCoverage::traversal_terminal_categories` records which categories stopped the walk
(§3). A consumer can therefore see that a first-party function reached only *through* a
third-party combinator — passed as a callback and invoked from inside it — was not
followed, and may appear in the unreachable set for that reason alone.

Traversing through would find that code. It is refused for v0.1 because the cost is
unknown: it expands the walk into every vendored and stdlib crate in the dependency
graph, and nothing has measured what that costs in-process (§8.7, and open question 4).
The choice is between a known, declared, visible limitation and an unmeasured risk of a
walk that does not terminate in useful time.

Making the limitation visible is the part that matters. A silently truncated walk
produces exactly the false positive design.md §8 says permanently destroys trust; a
truncated walk that says so in `unreachable.json` produces a caveat a reader can act on.
Revisit with a measurement, not with a preference — open question 4 (§11) states what
would settle it.

---

## 8. Sharding and the artifact (ADR-0006)

```
out/
  index.html            renderer, no data          (plan-05)
  endpoints.json        the root list — small
  graph/<slug>.json     one reachable subgraph per root
  unreachable.json
  versions.json         per-version classification (§6.2)
  overview.html         only when the whole graph is under ~5 MB (plan-05)
```

`versions.json` is an addition to ADR-0006's four-file layout, flagged here because it is
an addition. ADR-0007 makes "what dies when we sunset v1" a first-class output; the data
is per-node and per-version-key, so it fits neither `endpoints.json` (per root) nor
`unreachable.json` (per unreached node). It is derived, and a consumer that ignores it
loses nothing else.

### 8.1 Shard naming

The slug is derived from **root identity**, never from `NodeId::raw` — slugging a node id
would be parsing it (ADR-0003 field 3).

```
slug = sanitize(contract) "__" sanitize(version_or_absent) "__" sanitize(service)
       "__" sanitize(operation) "__" direction "__" hex16(blake3(canonical_tuple))
```

`sanitize` maps anything outside `[A-Za-z0-9._-]` to `_`. The hash suffix carries the
uniqueness; the readable part carries the human. `version_or_absent` renders `None` as
`_none` for display only — a contract with a literal version string `"none"`, or one
containing a `/`, is separated by the hash, not by the prefix.

`shard_slug_collision_free_none_vs_none_string` is a test because this is the exact place
where `None` and `"v1"` could silently converge on disk, and ADR-0007 spends a section on
why that is unacceptable.

**The slug is a filename, not an identity.** Full root identity is inside the file. A
consumer joins on the tuple, never on the path.

### 8.2 `endpoints.json`

Grouped by operation with versions as siblings — ADR-0007 Consequences, INFERRED there,
adopted here — rather than a flat list where `CreateTask` appears twice with nothing
distinguishing the entries.

```json
{
  "schema_version": 1,
  "generated_by": { "tool": "reachgraph", "version": "0.1.0" },
  "plugins": [
    { "id": "fixture", "position_encoding": "utf8_bytes",
      "capabilities": ["symbols", "edges", "roots", "classify"] }
  ],
  "operations": [
    {
      "contract": "acme.task",
      "service": "TaskService",
      "operation": "CreateTask",
      "direction": "served",
      "versions": [
        {
          "version": "v1",
          "join_key": "acme.task.v1.TaskService/CreateTask",
          "binding": { "state": "bound",
                       "node": { "plugin": "fixture", "raw": "fn:handlers_v1/create_task" } },
          "shard": "graph/acme.task__v1__TaskService__CreateTask__served__1f0c3a9b2d4e5f60.json",
          "node_count": 12,
          "frontier_count": 2
        },
        {
          "version": "v2",
          "join_key": "acme.task.v2.TaskService/CreateTask",
          "binding": { "state": "bound",
                       "node": { "plugin": "fixture", "raw": "fn:handlers_v2/create_task" } },
          "shard": "graph/acme.task__v2__TaskService__CreateTask__served__7b81ee40c1a25d33.json",
          "node_count": 9,
          "frontier_count": 0
        }
      ]
    },
    {
      "contract": "acme.legacy",
      "service": "LegacyService",
      "operation": "Ping",
      "direction": "served",
      "versions": [
        {
          "version": null,
          "join_key": "acme.legacy.LegacyService/Ping",
          "binding": { "state": "unbound",
                       "reason": "no symbol named `ping` with a container implementing LegacyService" },
          "shard": null,
          "node_count": 0,
          "frontier_count": 0
        }
      ]
    }
  ],
  "coverage": { "…": "IndexCoverage, §3" }
}
```

`"version": null` is emitted explicitly. The key is always present. An unbound root has
`"shard": null` and keeps its reason — it is a row in the endpoint list, visibly gapped,
not an absence.

### 8.3 `graph/<slug>.json`

```json
{
  "schema_version": 1,
  "root": {
    "contract": "acme.task",
    "version": "v1",
    "service": "TaskService",
    "operation": "CreateTask",
    "direction": "served",
    "join_key": "acme.task.v1.TaskService/CreateTask",
    "binding": { "state": "bound",
                 "node": { "plugin": "fixture", "raw": "fn:handlers_v1/create_task" } }
  },
  "depth_limit": 3,
  "plugins": [
    { "id": "fixture", "position_encoding": "utf8_bytes",
      "capabilities": ["symbols", "edges", "roots", "classify"] }
  ],
  "nodes": [
    {
      "id": { "plugin": "fixture", "raw": "fn:handlers_v1/create_task" },
      "indexed": true,
      "name": "create_task",
      "kind": "method",
      "raw_kind": "fn",
      "range": { "file": "src/service/handlers_v1.rs", "span": { "start": 412, "end": 933 } },
      "doc": "Create a task.",
      "doc_format": "markdown",
      "is_test": false,
      "container": { "plugin": "fixture", "raw": "impl:TaskService_for_TaskServer" },
      "unit": "unit:app",
      "category": "first_party",
      "depth": 0,
      "frontier": false
    },
    {
      "id": { "plugin": "fixture", "raw": "fn:db/insert_task" },
      "indexed": true,
      "name": "insert_task",
      "kind": "function",
      "raw_kind": "fn",
      "range": { "file": "src/db/insert.rs", "span": null },
      "doc": null,
      "doc_format": "plain",
      "is_test": false,
      "container": null,
      "unit": "unit:db",
      "category": "first_party",
      "depth": 1,
      "frontier": true
    },
    {
      "id": { "plugin": "fixture", "raw": "reg:tonic-0.12/Status::invalid_argument" },
      "indexed": true,
      "name": "invalid_argument",
      "kind": "method",
      "raw_kind": "fn",
      "range": { "file": "/home/u/.cargo/registry/src/…/tonic-0.12/src/status.rs",
                 "span": null },
      "doc": null,
      "doc_format": "plain",
      "is_test": false,
      "container": null,
      "unit": "unit:app",
      "category": "third_party",
      "depth": 1,
      "frontier": false
    },
    {
      "id": { "plugin": "fixture", "raw": "ext:unlocatable_target" },
      "indexed": false,
      "unit": null,
      "category": null,
      "depth": 1,
      "frontier": false
    }
  ],
  "edges": [
    {
      "from": { "plugin": "fixture", "raw": "fn:handlers_v1/create_task" },
      "to": { "state": "resolved",
              "node": { "plugin": "fixture", "raw": "fn:db/insert_task" } },
      "call_site": { "file": "src/service/handlers_v1.rs", "span": { "start": 604, "end": 631 } },
      "provenance": { "plugin": "fixture", "engine": "reachgraph-fixture 0.1.0" },
      "inference_mode": "resolved"
    },
    {
      "from": { "plugin": "fixture", "raw": "fn:db/insert_task" },
      "to": { "state": "unresolved", "name": "execute",
              "candidates": [
                { "plugin": "fixture", "raw": "fn:db/execute_pg" },
                { "plugin": "fixture", "raw": "fn:db/execute_sqlite" }
              ] },
      "call_site": null,
      "provenance": { "plugin": "fixture", "engine": "reachgraph-fixture 0.1.0" },
      "inference_mode": "lexical"
    }
  ],
  "frontier": [ { "plugin": "fixture", "raw": "fn:db/insert_task" } ],
  "stats": { "node_count": 12, "edge_count": 15, "unresolved_edge_count": 1 }
}
```

Notes on the schema, each load-bearing:

- `position_encoding` is per plugin in the `plugins` table, not per node. A node knows its
  plugin; the plugin knows its encoding (ADR-0003 field 2). Duplicating it per node would
  invite a consumer to compare two offsets without checking they share an encoding.
- `indexed: false` nodes carry no name, kind or range, because none was ever observed.
  Emitting an empty string for a name would manufacture data. They also carry
  `category: null` — unclassifiable, because classification needs a path (§7.0). The
  third node above is one, and it is meant to look rare.
- The second and third nodes are different things and the schema keeps them apart: the
  second is a third-party symbol the provider *located* (it has a file, so it classifies
  and terminates traversal per §7.1); the third is a target nothing could locate at all.
- `range.file` is always present on an indexed node. `range.span` is nullable, and `null`
  is a plugin saying it has no offset for this symbol (plan-00 §2). It is not the same as
  a span at offset 0, and a consumer must not treat the two alike.
- `unit` groups nodes into the outermost box; the `container` chain nests inside it
  (§3.1). `unit` is `null` exactly when `indexed` is `false`.
- `depth` and `frontier` are per shard. In the index-wide view `depth` is `null`.
- `to` is a tagged union. There is no field shape in which an unresolved edge can be
  mistaken for a resolved one, and no field that holds a "best" candidate.
- `container` round-trips verbatim.
- Every edge carries `provenance` and `inference_mode` (ADR-0003 field 4). Not optional,
  in the type or the schema.

### 8.4 `unreachable.json`

```json
{
  "schema_version": 1,
  "claim": "not reachable from any endpoint version in this index",
  "coverage": {
    "contracts": ["acme.task", "acme.legacy"],
    "versions": [["acme.task", "v1"], ["acme.task", "v2"], ["acme.legacy", null]],
    "roots_total": 7,
    "roots_bound": 6,
    "unbound_roots": [
      { "contract": "acme.legacy", "version": null, "service": "LegacyService",
        "operation": "Ping", "direction": "served",
        "reason": "no symbol named `ping` with a container implementing LegacyService" }
    ],
    "units_indexed": ["unit:app", "unit:db"],
    "plugins": ["fixture"],
    "traversal_terminal_categories": ["third_party", "stdlib"],
    "partial": false
  },
  "nodes": [
    {
      "id": { "plugin": "fixture", "raw": "fn:db/execute_sqlite" },
      "name": "execute_sqlite",
      "file": "src/db/sqlite.rs",
      "category": "first_party",
      "is_test": false,
      "possibly_reachable_via_unresolved": true
    }
  ],
  "counts_by_category": {
    "first_party": 3, "generated": 1, "workspace_sibling": 0,
    "third_party": 0, "stdlib": 0, "unclassified": 2
  },
  "unresolved_edge_count": 1
}
```

`possibly_reachable_via_unresolved` is an annotation on a listed node. It never removes a
node from `nodes` (§5.3). `unclassified` counts `category: None`.

### 8.5 `versions.json`

```json
{
  "schema_version": 1,
  "version_keys": [["acme.task", "v1"], ["acme.task", "v2"], ["acme.legacy", null]],
  "nodes": [
    { "id": { "plugin": "fixture", "raw": "fn:handlers_v1/create_task" },
      "reached_by": [0], "class": null },
    { "id": { "plugin": "fixture", "raw": "fn:db/insert_task" },
      "reached_by": [0, 1], "class": "both" }
  ],
  "summary_by_contract": {
    "acme.task": { "v1_only": 4, "v2_only": 6, "both": 9 }
  }
}
```

`class` is non-null only where the contract has exactly two version keys (§6.2).
`reached_by` indexes into `version_keys` and is always authoritative.

### 8.6 Writing

Emission goes through `OutputSink` (plan-00 §3.6). The core owns paths; a renderer names a
relative path and writes bytes. `emit.rs` uses the same sink, so the built-in artifact
files and a plugin renderer's files land by one mechanism.

### 8.7 A correction: the walk cost is an OPEN MEASUREMENT, not a measurement

design.md §8 states that `callHierarchy/outgoingCalls` costs one round trip per node and
that a whole-repository walk takes minutes rather than seconds, and ADR-0006 cites it.
**That number was measured against the LSP round-trip architecture ADR-0001 rejected.**
In-process `ra_ap_ide` has no round trips at all — ADR-0004 MEASURED that
`Analysis::outgoing_calls` is an ordinary public function call, not a protocol exchange.

So the measurement does not transfer, and this plan relabels it: **OPEN MEASUREMENT — the
cost of a whole-repository in-process walk is unknown.** Plan-05 and plan-06 record it the
same way. What would settle it: time `Index::build` over one real repository with
`lang-rust` linked, at plan-03.

Nothing in this plan rests on it. The batch-build shape (§4.2) follows from ADR-0006's
artifact decision, not from a walk cost, and §7.1's traversal-termination choice is
explicitly made *because* the cost is unmeasured rather than because it is known to be
high.

**ADR-0006's conclusion is unaffected.** The static-artifact decision stands on three
independent grounds it states separately: the output is a CI-generated artefact, a
stateful server would hold no state worth holding because the graph is regenerated per
run, and a call graph of private source is a disclosure that must not acquire a hosted
delivery path. A faster walk changes none of those. It would only reopen the question of
whether an *interactive* mode is possible later — a different question, and not one this
plan answers.

---

## 9. Provenance, end to end

ADR-0003 field 4, traced through every stage:

| stage | what must survive |
|---|---|
| `EdgeProvider::edges_in` | `provenance`, `inference_mode` as given |
| graph assembly | both copied onto `EdgeRecord`; neither defaulted, neither normalised |
| traversal | traversal reads `from`/`to` only; it never filters on `inference_mode` |
| shard extraction | both copied onto the emitted `Edge` |
| JSON | both required fields |

Traversal deliberately does **not** filter on `inference_mode`. A `Lexical` or
`TypeInferred` edge is a weaker claim, not a false one, and dropping it from reachability
would turn a weak edge into a missing one — which then shows up as unreachable code. The
strength is carried to the renderer, which may style it differently. That is ADR-0003's
"a directly-resolved edge and an inferred edge must not render identically", and the place
to honour it is the renderer, not the BFS.

`edge_provenance_survives_graph_build` and `edge_inference_mode_survives_shard_emit` are
separate tests because the two stages fail separately.

---

## 10. Tests

Test-driven, red first. The build order below is the plan: each step names the test that
goes red, what the red state is, and what turns it green.

### 10.1 Build order

| # | red test | red state | green when |
|---|---|---|---|
| 1 | `pairing_by_plugin_id` | `Index::build` does not exist | symbols + edges collected per unit from a paired fixture plugin |
| 2 | `unpaired_symbol_provider_is_a_build_error` | build returns `Ok` | `BuildError::UnpairedSymbolProvider` |
| 3 | `reachable_set_from_single_root` | no traversal | BFS over the interned graph |
| 4 | `cycle_terminates` | hangs or overflows | visited set checked before enqueue |
| 5 | `depth_limit_marks_frontier_not_leaf` | `frontier` empty | depth-limited BFS records the frontier |
| 6 | `unreachable_is_complement_over_all_roots` | no complement | union over bound roots, subtracted |
| 7 | `unreachable_uses_unlimited_depth` | deep nodes listed as unreachable | complement computed with `depth: None` |
| 8 | `v1_and_v2_are_separate_roots` | one merged root set | roots partitioned by version key |
| 9 | `shard_round_trip_serde` | no shard type | `Shard` serialises and deserialises identically |
| 10 | `unreachable_claim_string_is_verbatim` | no claim field | the sentence emitted as data |

Steps 1–3 are the spine. Everything in §10.2 hangs off a graph that builds and a BFS that
runs, so nothing else can go red usefully before step 3 is green.

### 10.2 The rest, by requirement discharged

**Provider pairing and assembly**

| test | discharges |
|---|---|
| `unpaired_edge_provider_is_a_build_error` | §4.2 |
| `preflight_failure_aborts_with_remediation` | ADR-0003 field 5 |
| `duplicate_symbol_takes_first_and_diagnoses` | §4.3 |
| `provider_error_aborts_unless_allow_partial` | §4.2, open Q3 |
| `allow_partial_sets_coverage_partial_flag` | ADR-0007 |

**`NodeId` opacity (ADR-0003 field 3)**

| test | discharges |
|---|---|
| `node_id_opacity_bijective_rename` | rename every `raw` by a bijection; graph isomorphic, reachable sets and categories identical. Property test over a generated corpus, `proptest`. |
| `node_id_with_structured_looking_raw_is_not_parsed` | raw strings containing `::`, `/`, `#`, `{`, and a JSON document, all treated as bytes |
| `same_raw_different_plugin_does_not_collide` | `PluginId` is half the key |

**`Symbol::container` (plan-00 §8 question 3)**

| test | discharges |
|---|---|
| `container_is_copied_not_interpreted` | round-trips verbatim into shard JSON |
| `container_does_not_create_edge` | edge count unchanged by adding containers |
| `container_does_not_affect_reachability` | reachable set identical with and without |
| `dangling_container_is_not_an_error` | container names an unindexed id; build succeeds |

**Roots and binding (ADR-0007, plan-00 §8 question 5)**

| test | discharges |
|---|---|
| `unbound_root_is_reported_not_dropped` | plan-00 §6.2 |
| `unbound_root_has_no_shard_but_is_in_endpoints` | §4.4 |
| `unbound_root_recorded_in_coverage` | `IndexCoverage::unbound_roots` |
| `root_bound_to_unindexed_node_is_diagnosed_not_dropped` | §4.4 |
| `root_has_no_confidence_field` | schema assertion: no float anywhere in `Root` |
| `join_key_is_stored_never_parsed` | a `join_key` containing `/`, `.`, `v9` and a whole URL round-trips byte-identically; no version, service or operation is recovered from it |
| `join_key_does_not_affect_root_identity` | two roots differing only in `join_key` remain two roots; identity is the tuple (ADR-0007) |

**Versions (ADR-0007)**

| test | discharges |
|---|---|
| `missing_version_stays_none` | plan-00 §6.2 |
| `unversioned_and_versioned_roots_do_not_merge` | §6.1 |
| `v1_only_v2_only_both_classification` | ADR-0007's three-way table |
| `three_versions_emit_reached_by_not_class` | §6.2 |
| `version_key_none_is_not_serialized_as_v1` | round-trip `null` |
| `missing_version_key_is_a_parse_error` | §3 serde rules — a *missing* key, not `null` |
| `unreachable_records_root_coverage` | plan-00 §6.2 |
| `coverage_lists_every_version_key_including_none` | ADR-0007 requirement 1 |

**Unresolved edges (design.md §8)**

| test | discharges |
|---|---|
| `unresolved_edge_is_not_silently_resolved` | plan-00 §6.2 |
| `unresolved_edge_does_not_propagate_reachability` | §5.3 |
| `unresolved_candidate_flagged_possibly_reachable` | §5.3 |
| `possibly_reachable_annotation_does_not_filter_list` | §5.3 — the node stays listed |

**Provenance (ADR-0003 field 4)**

| test | discharges |
|---|---|
| `edge_provenance_survives_graph_build` | plan-00 §6.2 |
| `edge_inference_mode_survives_shard_emit` | §9 |
| `traversal_does_not_filter_on_inference_mode` | §9 |

**Classification (ADR-0002, ADR-0008 leak 8)**

| test | discharges |
|---|---|
| `classifier_invoked_through_trait_object` | spy classifier records every call; plan-00 §3.5 |
| `no_classifier_yields_none_not_error` | §7 |
| `classification_is_per_file_within_one_unit` | §7 — `src/` and `vendor/` in **one** unit classify differently. This is the test that would have failed under the removed `unit.root` fallback. |
| `spanless_symbol_still_classifies` | §7 — `span: None` changes nothing; classification reads `range.file` only |
| `external_node_is_unclassified` | §7.0 — `category: None`, and it terminates nothing |
| `core_contains_no_path_prefix` | source assertion: no `src/`, `target/`, `/nix/store`, `node_modules`, `vendor/` literal in the crate |
| `third_party_node_is_terminal_not_deleted` | §7.1 |
| `terminal_categories_recorded_in_coverage` | §7.1 — the declared limitation reaches `unreachable.json`, not a release note |
| `position_encoding_is_per_plugin_not_global` | ADR-0003 field 2 / ADR-0008 leak 3 — a two-plugin build emits two different encodings in the shard `plugins` table, each matching its declaring plugin |

**Sharding and the artifact (ADR-0006)**

| test | discharges |
|---|---|
| `one_shard_per_bound_root` | ADR-0006 |
| `shard_slug_collision_free_none_vs_none_string` | §8.1 |
| `shard_slug_is_not_derived_from_node_id` | §8.1 / ADR-0003 field 3 |
| `two_roots_sharing_a_node_both_include_it` | shards overlap; neither is authoritative |
| `external_target_is_leaf_and_never_unreachable` | §4.3, §5.4 |
| `unreachable_lists_every_indexed_unreached_node` | §5.4 — no category suppression |
| `unclassified_nodes_are_counted_not_dropped` | §5.4 |
| `word_dead_appears_nowhere_in_crate` | §6.4 — grep over `reachgraph-core/src` |
| `artifact_is_deterministic_across_runs` | sort orders fixed; ADR-0006 regenerates rather than mutates |

**`GraphView` accessors (§3, §3.1 — plan-05 §9.2 is the consumer)**

| test | discharges |
|---|---|
| `depth_of_matches_bfs_distance` | the depth slider's input is correct at every node |
| `index_wide_view_has_null_depth` | `depth: None` outside a shard; no fake zero |
| `container_chain_nests_and_terminates` | nesting for compound boxes |
| `container_chain_survives_a_containment_cycle` | a plugin-emitted loop truncates, never hangs |
| `container_of_unindexed_container_is_none` | §4.3's dangling case, through the accessor |
| `nodes_in_unit_excludes_externals` | an external node belongs to no unit |
| `view_index_rebuilds_after_deserialization` | a shard read from disk behaves like one just built |
| `span_none_is_not_offset_zero` | plan-00 §2 — `null` and `{0,0}` stay distinguishable in the type and the artifact, and `range.file` survives in both |

**Registry and detection (plan-00 §5)**

| test | discharges |
|---|---|
| `empty_marker_files_matches_nothing` | plan-00 §2 — a plugin declaring no markers is never detected |
| `fixture_is_never_detected` | plan-00 §5 — only `select` or an explicit slice returns it |

**Golden artifacts**

Snapshot tests (`insta`) over the full `out/` tree for each plan-02 fixture case. They
catch schema drift that no single assertion notices, and they are the regression net for
plan-05's renderer, which reads these files.

### 10.3 What this plan does not test

`ra_ap` integration (plan-03), proto parsing (plan-04), HTML (plan-05). Every test above
runs against fixtures only — no `cargo metadata`, no indexing, no timing variance, no
requirement that any repository was built (ADR-0008's second benefit).

**Consequence worth stating: plan-01 cannot begin its tests before plan-02's fixture
plugin loads a file.** The two plans run in parallel (plan-00 §7), but the dependency is
real and one-directional. The first fixture case (`minimal`) is the unblocking deliverable.

---

## 11. Open questions

Named, not answered — except 4 and 5, settled by the coordinator and struck through with
the resolution recorded in place rather than deleted.

1. **Is `edges_from` exercised at all in v0.1?** Batch assembly uses `edges_in` per unit.
   `edges_from` exists for depth-limited expansion beyond the frontier, which nothing in
   v0.1 calls — plan-05 renders precomputed shards. An unexercised trait method is the
   thing most likely to be wrong when `lang-rust` lands, and this is the same method
   plan-00 open question 1 is unsure about. Either core uses it for frontier expansion, or
   the method ships untested against a real engine. **Decide in plan-03, before
   `lang-rust` is built around the assumption.**

2. **Should the shard depth limit be global or per root?** A global `--depth 3` (design.md
   §7) is simple. A deep handler and a shallow one want different limits, and the frontier
   marker makes the truncation visible either way. INFERRED that global is adequate for
   v0.1; unmeasured.

3. **Abort or `--allow-partial` on provider failure?** Defaulting to abort is chosen above
   on ADR-0007's reasoning — a partial index makes live code look unreachable. Whether
   `allow_partial` should exist at all, given that `IndexCoverage::partial` is a flag a
   consumer may ignore, is unresolved.

4. ~~**Does terminating traversal at `ThirdParty` lose first-party code reached through a
   callback?**~~ **RESOLVED 2026-09-17 — terminate in v0.1, and declare the limitation in
   the artifact.** See §7.1.

   It does lose that code. A first-party function passed to a third-party combinator and
   called from inside it is reachable in fact and unreachable in this traversal.
   Traversing through would find it, at the cost of expanding the walk into every vendored
   and stdlib crate in the dependency graph — and §8.7 is why that cost cannot be traded
   away today: it is unmeasured, and the number that used to stand in for it does not
   apply to an in-process engine.

   So the choice is a known limitation against an unmeasured risk, and the known one wins
   *provided it is visible*. `IndexCoverage::traversal_terminal_categories` puts it in
   `unreachable.json`. MEASURED, design.md §8: the noise argument for terminating is
   strong on its own — `pin`, `map`, `trim`, `Ok`, `Err`, `Some` among 20 edges from one
   handler.

   **What would settle it:** with `lang-rust` linked (plan-03), run both traversals over
   one real repository and compare wall time and the size of the difference set. Revisit
   on that measurement, not on preference.

5. ~~**Who spells the cross-repository join key?**~~
   **RESOLVED 2026-09-17 — the roots plugin spells it; the core stores it opaquely.**
   `Root` gains `join_key: String` (plan-00 §2).

   ADR-0007 fixes the key as the fully-qualified operation name,
   `yadgar.task.v1.TaskService/CreateTask`. That spelling is gRPC-shaped — MEASURED,
   design.md §4, the version lives in the proto package and arrives mechanically in the
   generated path — so composing it in the core would put framework knowledge in the
   waist, which ADR-0003 forbids and ADR-0007 forbids again for versions specifically.

   The core treats it exactly as it treats `NodeId::raw` and `Symbol::container`: hash,
   compare, emit. Never split on `/` or `.`, and never used to recover a version. The
   objection that plugins must then agree on a format is real, but it is an agreement
   between plugins about a contract's own naming, not knowledge the waist needs — and a
   later join is a string equality test either way.

   Present from v0.1 although cross-repository stitching is out of scope (ADR-0008),
   because it is ADR-0003-class: a plugin that ships without emitting it produces roots
   from which the key cannot be recovered afterwards.

6. **Memory at scale.** The whole graph is held in RAM: nodes, edges, adjacency, and one
   bitset per node for version reach. INFERRED tolerable at the ~100k-node, ~300k-edge
   figure ADR-0006 INFERRED. Never measured. The version bitsets are the part that grows
   with both node count and version-key count.

7. **Is `Span`'s encoding normalised by the core or the plugin?** Plan-00 open question 2,
   unchanged and still undecidable at n=1. The core currently stores offsets as given and
   emits the per-plugin encoding alongside them (§8.3). If cross-plugin offset comparison
   is ever needed, this becomes a decision rather than a deferral.

8. **Can `ra_ap` locate out-of-workspace edge targets cheaply?** §7.0 puts an obligation
   on providers: emit a `Symbol` for any edge target whose location is known, even
   outside the units enumerated, so that stdlib and third-party targets are classifiable
   and therefore traversal-terminal (§7.1). MEASURED, design.md §8, the paths were all
   present in the probe output — `/nix/store/…rust-lib-src/`, the cargo registry, the
   generated `out/` directory. What is *not* measured is what it costs `ra_ap` to supply
   them in bulk, or whether a symbol outside the loaded workspace is reliably locatable at
   all. If it is not, `ThirdParty` termination silently stops working and the edge-noise
   problem design.md §8 MEASURED returns. **Belongs to plan-03**, and it is a correctness
   dependency of §7.1, not a performance detail.
