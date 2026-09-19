# Plan 00 — Workspace layout and the plugin API contract

**Status:** ready to build
**Date:** 2026-09-17
**Amended:** 2026-09-17 (a) — open questions 3 and 5 resolved. `Symbol` gains
`container: Option<NodeId>`; `Root::node` + `Root::confidence` are replaced by
`binding: RootBinding`. Changes are in §2, §3.4, §6.2 and §8.
**Amended:** 2026-09-17 (b) — from plan-01 and plan-05. `Root` gains a plugin-spelled opaque `join_key`; an empty
`Detection::marker_files` matches nothing; `Renderer` no longer extends `Plugin` and
`Capability::Render` is removed; `Registry` covers analysis plugins only and gains
`select`; the `no_position_type_in_edge_api` grep is replaced by a public-API snapshot.
Changes are in §2, §3.0, §3.6, §4, §5 and §6.1.
**Amended:** 2026-09-17 (c) — `Symbol::range` stays required; the `Option` moves down to
`SourceRange::span`, where the uncertainty actually lives. Supersedes (b)'s
`Option<SourceRange>`, which discarded a known file along with an unknown offset. Raised
by plan-02 §8 question 6. Changes are in §2, §4, §6.1 and §8.
**Depends on:** ADR-0001 … ADR-0008
**Blocks:** every other plan

This is the first of eight plans and the contract the other seven build against. The trait
signatures in §3 are the deliverable. Everything else in this document exists to justify
them.

---

## 1. Workspace layout

A Cargo workspace, one crate per module.

| crate                          | role                                                      | depends on                |
| ------------------------------ | --------------------------------------------------------- | ------------------------- |
| `reachgraph-plugin-api`        | the five traits and every shared type                     | nothing in this workspace |
| `reachgraph-core`              | the waist (ADR-0003): graph build, reachability, sharding | `plugin-api`              |
| `reachgraph-lang-rust`         | Rust symbols, edges, classification (ADR-0004)            | `plugin-api`, `ra_ap_*`   |
| `reachgraph-roots-proto-tonic` | `.proto` parsing, tonic handler binding                   | `plugin-api`              |
| `reachgraph-fixture`           | the fixture plugin (ADR-0008)                             | `plugin-api`              |
| `reachgraph-render-html`       | static HTML renderer (ADR-0006)                           | `plugin-api`              |
| `reachgraph-cli`               | the binary; owns the registry and feature flags           | `core` + every plugin     |

### Dependency direction

```
                 reachgraph-cli
                 /      |      \
                /       |       \
  reachgraph-core   plugins...   \
            \           |         \
             \          |          \
              →  reachgraph-plugin-api  ←
```

**Rule: dependencies point toward `plugin-api`. No plugin crate may depend on
`reachgraph-core`.**

This is load-bearing, and it is the same class of mechanism as ADR-0008's fixture plugin.
A plugin that _cannot_ write `use reachgraph_core::internals` cannot accidentally couple
itself to waist internals, reach around a trait, or grow a dependency on how the graph
happens to be built today. The compiler enforces what code review would otherwise have to
enforce by vigilance.

Cost: one extra `Cargo.toml`. It is the cheapest structural guarantee in the project.

### Where the graph schema lives — a deliberate subtlety

ADR-0003 says the graph schema is the waist and is not a plugin. That does **not** mean it
lives in `reachgraph-core`.

A renderer is a plugin (ADR-0002) and must receive the graph. If the graph types lived in
`core`, every renderer would need `core` as a dependency, breaking the rule above.

Resolution, and it is a distinction worth stating precisely:

- **Schema types** (`Node`, `Edge`, `Root`, `GraphView`, `Shard`) live in `plugin-api`.
  They are the contract.
- **Construction and algorithms** (graph assembly, reachability, complement, sharding)
  live in `core`. They are the thesis, and they are not swappable.

"Not a plugin" means no plugin may redefine or replace it. It does not mean no plugin may
see it.

### Features

Each plugin crate is behind a Cargo feature in `reachgraph-cli` (ADR-0002). Default
features enable Rust and the HTML renderer. The fixture plugin is behind a feature enabled
in `dev-dependencies` and in tests, never in a release build.

---

## 2. Shared types

All in `reachgraph-plugin-api`. Every one of ADR-0003's six fields appears here, including
the four that Rust does not need.

```rust
/// Stable identifier for a plugin. Used as the namespace half of every NodeId.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PluginId(pub &'static str);

/// ADR-0003 field 3. The core MUST NEVER parse `raw`.
/// Plugins choose their own encoding: a SCIP symbol, a path+offset, anything stable.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct NodeId {
    pub plugin: PluginId,
    pub raw: String,
}

/// ADR-0003 field 2. Declared per plugin, never assumed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PositionEncoding {
    Utf8Bytes,        // ra_ap, tree-sitter
    Utf16CodeUnits,   // LSP
    Utf32CodePoints,
}

/// A half-open offset pair in the declaring plugin's `PositionEncoding`, and nothing
/// else. Deliberately minimal — see §8 question 6 for what was considered and left out.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Span { pub start: u32, pub end: u32 }

/// A file, and — separately — an offset pair within it.
///
/// **The two fields carry different certainty and must not share one `Option`.** A
/// plugin that names a symbol almost always knows which file it is in; it may not know
/// where in the file. `file` is therefore required and `span` is not.
///
/// The earlier shape put the `Option` one level up — `Symbol::range:
/// Option<SourceRange>` — which made a plugin discard the file it *did* know in order
/// to be honest about the offset it did not. `Classifier::classify` takes a path, so
/// that conflation cost classification its resolution (plan-01 §7).
///
/// Resolves plan-02 §8 question 6, which is where the defect was found and the shape
/// proposed. Recorded that way round deliberately: the first amendment reached for
/// `Option<SourceRange>` and was wrong.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SourceRange {
    pub file: PathBuf,
    /// `None` = this plugin has no offset for this symbol and says so. Never a
    /// sentinel: `Span { start: 0, end: 0 }` is indistinguishable from a real offset 0.
    pub span: Option<Span>,
}

/// ADR-0008 leak 6. Neutral; `raw_kind` preserves the plugin's own term for display.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SymbolKind { Function, Method, Type, Module, Field, Other }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DocFormat { Plain, Markdown }

/// ADR-0008 leak 7.
#[derive(Clone, Debug)]
pub struct Symbol {
    pub id: NodeId,
    pub name: String,
    pub kind: SymbolKind,
    pub raw_kind: String,
    /// Required. Every symbol a plugin can name, it can place in a file; the
    /// uncertainty lives one level down, in `SourceRange::span`.
    ///
    /// The defect this shape fixes was never "the range should be optional" — it was a
    /// type conflating two facts of different certainty, so that saying "I have no
    /// offset" also said "I have no file". That is the same class as `confidence: f32`
    /// (§8 question 5) and the sentinel span: a type must not make a known fact
    /// unrepresentable, and must not make an unknown one look known.
    ///
    /// Resolves plan-02 §8 questions 3 and 6. Settled, not open.
    pub range: SourceRange,
    pub doc: Option<String>,
    pub doc_format: DocFormat,
    /// The enclosing definition, if the language has one. Rust: the `impl` block.
    /// Go: the receiver type. Java: the class. Python: the class.
    ///
    /// MEASURED, design.md §4: `create_task` exists twice in `task` — the real handler
    /// in `src/service/handlers.rs` and a `MockDb` in `tests/service.rs`. Binding an
    /// operation to a handler needs the enclosing `impl` block's trait, so name alone
    /// is a MEASURED failure.
    ///
    /// **The waist never interprets this field.** It is carried, stored and emitted
    /// verbatim. A roots plugin walks up and reads the container's `raw_kind`; the core
    /// does not, exactly as with `NodeId::raw` (ADR-0003 field 3). It contributes no
    /// edge and no reachability. A dangling `container` is plugin data, not a core error.
    ///
    /// Resolves open question 3 (§8).
    pub container: Option<NodeId>,
    /// design.md §4: see `container`. Disambiguating roots on name alone is a MEASURED
    /// failure; `is_test` and `container` together are how a roots plugin separates the
    /// real handler from the mock.
    pub is_test: bool,
}

/// ADR-0008 leak 4. "What is the unit of analysis" is per-language.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct UnitId(pub String);

#[derive(Clone, Debug)]
pub struct Unit {
    pub id: UnitId,
    pub display_name: String,
    pub root: PathBuf,
}

/// ADR-0003 field 4, first half. What produced this edge.
#[derive(Clone, Debug)]
pub struct Provenance {
    pub plugin: PluginId,
    /// Free text naming the engine and version, e.g. "ra_ap_ide 0.0.352".
    pub engine: String,
}

/// ADR-0003 field 4, second half. How strong the claim is.
/// ADR-0004 makes this load-bearing from the first non-Rust language.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InferenceMode {
    /// A semantic engine answered directly. Near-certain.
    Resolved,
    /// Scope and binding resolution only; no types consulted.
    Lexical,
    /// Required type inference to pick the target. Depends on inference succeeding.
    TypeInferred,
    /// Derived by asking which definition's range encloses a reference occurrence.
    Enclosure,
}

/// design.md §8: "Show a missing edge as missing; never infer one to fill a hole."
/// An unresolved call is recorded, never silently resolved to a best guess.
#[derive(Clone, Debug)]
pub enum EdgeTarget {
    Resolved(NodeId),
    Unresolved { name: String, candidates: Vec<NodeId> },
}

#[derive(Clone, Debug)]
pub struct Edge {
    pub from: NodeId,
    pub to: EdgeTarget,
    pub call_site: Option<SourceRange>,
    pub provenance: Provenance,
    pub inference_mode: InferenceMode,
}

/// ADR-0002: categories are the waist's; prefixes are the plugin's.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Category {
    FirstParty,
    Generated,
    WorkspaceSibling,
    ThirdParty,
    Stdlib,
}

/// ADR-0007. `version` is first-class and supplied by the plugin.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ContractId(pub String);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Direction { Served, Consumed }

/// Whether the contract operation was bound to a handler symbol.
///
/// There is no confidence score. design.md §5 MEASURED the failure mode a float
/// invites: code_graph emits 59 `CALLS` edges at confidence 0.55, each with two or
/// three candidate targets — a number that records indecision and then renders as if
/// it were a measurement. A root either binds to a handler or it does not.
///
/// An `Unbound` root is a **reported gap**, never a dropped row. This is design.md §8's
/// binding rule at the root layer: show a missing edge as missing, never infer one to
/// fill a hole. `reason` is plugin-authored free text for the report.
///
/// Resolves open question 5 (§8).
#[derive(Clone, Debug)]
pub enum RootBinding {
    Bound(NodeId),
    Unbound { reason: String },
}

#[derive(Clone, Debug)]
pub struct Root {
    pub contract: ContractId,
    /// ADR-0007: a missing version is `None`. NEVER defaulted to "v1".
    /// Wherever this field is deserialized, a missing key is a parse error, never a
    /// silent `None` — a serde default is exactly the mechanism that would let a
    /// round-trip invent `"v1"` or erase a plugin's deliberate `None`.
    pub version: Option<String>,
    pub service: String,
    pub operation: String,
    pub direction: Direction,
    /// The cross-repository join key, **spelled by the roots plugin**.
    ///
    /// ADR-0007 fixes the key as the fully-qualified operation name —
    /// `yadgar.task.v1.TaskService/CreateTask`. That spelling is contract-shaped and
    /// framework-shaped, and ADR-0003 forbids anything framework-shaped reaching the
    /// waist. So the plugin composes it and the core stores it **opaquely**: hashed,
    /// compared for equality, emitted. Never parsed, never split on `/` or `.`, never
    /// used to recover a version (ADR-0007 forbids that explicitly).
    ///
    /// Same opacity rule as `NodeId::raw` (ADR-0003 field 3) and `Symbol::container`.
    ///
    /// Present from v0.1 even though cross-repository stitching is out of scope
    /// (ADR-0008). It is ADR-0003-class: a plugin that ships without emitting it
    /// produces roots from which the key cannot be recovered afterwards.
    ///
    /// Resolves plan-01 §11 question 5.
    pub join_key: String,
    /// Replaces the earlier `node: NodeId` + `confidence: f32` pair. `binding` subsumes
    /// both: a separate `node` field would be incoherent for `Unbound`.
    ///
    /// ADR-0003 field 6 spells the root tuple `(contract_id, version, service,
    /// operation, direction, node_id, confidence)`. That spelling is **refined, not
    /// contradicted** — the last two positions become one `RootBinding`. ADR numbers are
    /// permanent and the ADR text is not edited; this plan is where the refinement lives.
    pub binding: RootBinding,
}

/// ADR-0007's binding requirement: the index records what it covered, so a partial
/// root set cannot make live code read as unreachable.
#[derive(Clone, Debug)]
pub struct Coverage {
    pub contracts: Vec<ContractId>,
    pub versions: Vec<(ContractId, Option<String>)>,
}

/// ADR-0003 field 5. Never `command -v` — design.md §10 MEASURED that a name
/// resolving on PATH proves nothing (the rustup proxy loop).
#[derive(Clone, Debug)]
pub enum Preflight {
    Ok,
    Failed { reason: String, remediation: String },
}

/// ADR-0003 field 1.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
/// No `Render` variant: a renderer is not a `Plugin` and declares no capability
/// (§3.6). ADR-0002's five plugin kinds are unchanged; only the trait hierarchy is.
pub enum Capability { Symbols, Edges, Roots, Classify }

/// Plugin-declared detection. ADR-0008: no `if is_rust_project(root)` in the core.
///
/// **An empty `marker_files` matches NOTHING, never everything.** A plugin that
/// declares no markers declares no claim to any repository.
///
/// The inverse rule — empty means match-all — would let one misconfigured or
/// half-written plugin silently hijack detection for every repository, which is
/// ADR-0008's forbidden `if is_rust_project(root)` line arriving from the other
/// direction: the core would not be branching on a language, it would be running one
/// on everything. A plugin that genuinely wants to handle any repository must say so
/// by listing what it matches.
#[derive(Clone, Debug)]
pub struct Detection {
    pub marker_files: &'static [&'static str],
    pub extensions: &'static [&'static str],
}
```

---

## 3. The five traits

### 3.0 The base trait

Every **analysis** plugin implements this — symbol, edge, root and classifier kinds.
A renderer does not; see §3.6.

```rust
pub trait Plugin: Send + Sync {
    fn id(&self) -> PluginId;

    /// ADR-0003 field 1. One crate may provide several capabilities and be
    /// invoked once, rather than re-indexing a repository per kind.
    fn provides(&self) -> &[Capability];

    /// ADR-0003 field 2.
    fn position_encoding(&self) -> PositionEncoding;

    fn detection(&self) -> Detection;

    /// ADR-0003 field 5.
    fn preflight(&self, root: &Path) -> Preflight;
}
```

### 3.1 Unit discovery

```rust
pub trait LanguagePlugin: Plugin {
    /// ADR-0008 leak 4. Rust returns crates; Go would return packages;
    /// Python would return source roots. The core has no opinion.
    fn discover_units(&self, root: &Path) -> Result<Vec<Unit>, PluginError>;
}
```

`SymbolProvider` and `EdgeProvider` both require this supertrait. The core pairs a
symbol provider with an edge provider by `PluginId` and passes the same `Unit` values to
both.

### 3.2 Symbol / doc provider

```rust
pub trait SymbolProvider: LanguagePlugin {
    fn symbols_in(&self, unit: &Unit) -> Result<Vec<Symbol>, PluginError>;
}
```

Doc text arrives inside `Symbol` (ADR-0005: the resolver has already parsed the file,
so documentation is not a separate fetch).

### 3.3 Call-edge provider — leak 1, the one to get right

```rust
pub trait EdgeProvider: LanguagePlugin {
    /// Batch enumeration. The primary path.
    fn edges_in(&self, unit: &Unit) -> Result<Vec<Edge>, PluginError>;

    /// Targeted expansion, for depth-limited traversal from a known node.
    fn edges_from(&self, node: &NodeId) -> Result<Vec<Edge>, PluginError>;
}
```

**Neither method takes a position, a cursor, a file offset or a `FilePosition`.**

This matters more than any other signature in the document. `ra_ap_ide::Analysis::
outgoing_calls` takes a `FilePosition`, because rust-analyzer's origin is an editor and
the question it answers is "what is under the user's cursor". That shape is _inherited from
LSP_, not intrinsic to call graphs.

A hand-written Go or Java resolver (ADR-0004: both must be written) naturally walks a
function body and enumerates the call sites it contains. It has no cursor. Handing it a
position-based interface forces every such plugin to synthesise positions it does not have
and does not want.

A position-shaped trait would pass every Rust test, because for Rust it is free. It would
tax every future plugin permanently. **`reachgraph-lang-rust` converts `NodeId` →
`FilePosition` internally, and that conversion never appears in `plugin-api`.**

This is the leak the fixture plugin (plan-02) exists to catch: fixture JSON has no
positions to offer, so a position-based signature fails to compile against it.

### 3.4 Root / contract provider

```rust
pub trait RootProvider: Plugin {
    /// Read-only view of the symbol set, for binding an operation to its handler.
    fn roots(
        &self,
        repo_root: &Path,
        symbols: &dyn SymbolIndex,
    ) -> Result<Vec<Root>, PluginError>;

    /// ADR-0007. What this provider actually looked at.
    fn coverage(&self) -> Coverage;
}

/// Lookup surface the core hands to a RootProvider. Deliberately narrow:
/// enough to bind a handler, not enough to re-implement the graph.
pub trait SymbolIndex {
    fn by_name(&self, name: &str) -> Vec<&Symbol>;
    fn in_file(&self, path: &Path) -> Vec<&Symbol>;
    fn get(&self, id: &NodeId) -> Option<&Symbol>;
}
```

Tonic's `impl XServer for T` binding and the CamelCase → snake_case rule live **entirely**
inside `reachgraph-roots-proto-tonic`. The waist receives `Root` and nothing else
(ADR-0007). `by_name` returning `Vec` rather than `Option` is deliberate — design.md §4
MEASURED that `create_task` exists twice in one repo, so the plugin must disambiguate using
`container`, `is_test` and its own knowledge of trait impls.

The disambiguation walk is the plugin's, not the core's: `by_name("create_task")` returns
two symbols, the plugin follows each `container` through `SymbolIndex::get`, reads the
container's `raw_kind` — `impl TaskService for TaskServer` versus `impl MockDb` — and picks.
`raw_kind` is a display string the waist never matches on, which is why the _link_ had to be
a typed field rather than a naming convention: the join the contract depends on cannot rest
on a string the waist is forbidden to read.

A candidate the plugin cannot resolve yields `RootBinding::Unbound { reason }`, not a
guess and not a dropped row.

### 3.5 Classifier

```rust
pub trait Classifier: Plugin {
    fn classify(&self, path: &Path, unit: &Unit) -> Category;
}
```

**Classifier at n=1.** Folding the _implementation_ into `reachgraph-lang-rust` for v0.1 is
acceptable — one crate, two `impl` blocks. But **the trait lives in `plugin-api` and the
core invokes it through the trait object**, never by calling a Rust-specific function.

If the core calls into `lang-rust` directly for classification, the fold has become
ADR-0008's forbidden `if is_rust_project(root)` line wearing a different costume, and
adding Go means editing the core. The trait boundary is what keeps the fold reversible.

Categories live in the waist. Prefixes — `src/`, `target/*/out/`,
`/nix/store/…rust-lib-src/`, the cargo registry path — live in the plugin, because
design.md §8 MEASURED that they are Rust-specific _and_ machine-specific.

### 3.6 Renderer

**`Renderer` does not extend `Plugin`.**

```rust
pub trait Renderer: Send + Sync {
    /// Retained for attribution and feature naming. Not a `Plugin::id`.
    fn id(&self) -> PluginId;

    fn render(
        &self,
        graph: &GraphView,
        sink: &mut dyn OutputSink,
    ) -> Result<(), PluginError>;
}

/// The core owns where bytes land (ADR-0006's out/ layout). A renderer
/// names a relative path and writes; it never touches the filesystem itself.
pub trait OutputSink {
    fn write(&mut self, relative_path: &str, bytes: &[u8]) -> io::Result<()>;
}
```

`position_encoding`, `detection` and `preflight` are meaningless for a renderer. A
renderer analyses nothing, so it has no encoding; it claims no repository, so it detects
nothing; it has no prerequisite to check, so its preflight is vacuous.

The alternative considered and rejected (plan-05) was provided-method defaults on
`Plugin` — a renderer inheriting `PositionEncoding::Utf8Bytes`, an empty `Detection` and
`Preflight::Ok`. Rejected because a default is a value, and a meaningless value is
indistinguishable downstream from a meant one: `Utf8Bytes` from a renderer would land in
`PluginDescriptor` and the artifact's `plugins` table (plan-01 §8.3) as if the renderer
had declared it. That is the same defect as a sentinel `Span {0,0}` (§2) and a
`confidence: 0.55` (§8 question 5) — the third instance of one failure in this contract,
which is why it is refused by construction rather than by care.

Nothing in ADR-0002 requires the five plugin kinds to share a supertrait. A renderer
remains a plugin kind in ADR-0002's sense — one crate, feature-gated, swappable,
independently testable. It is not an analysis plugin, and now the type system says so.

Consequences: `Capability::Render` is gone (§2). `Registry` holds analysis plugins only,
and `detect` cannot return a renderer (§5). Renderer selection is by explicit choice —
an output format is something a user asks for, never something detected from a
repository.

---

## 4. The eight leaks, method by method

ADR-0008's anti-leak test — _could `ra_ap`'s return value be substituted verbatim here?_ —
applied to every signature above.

| #   | leak                        | how the API avoids it                                                                                                                                   |
| --- | --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | position-based queries      | `edges_in(&Unit)` / `edges_from(&NodeId)`. No position type appears anywhere in `plugin-api`. `lang-rust` converts internally.                          |
| 2   | `FileId`                    | `SourceRange` carries a `PathBuf`. No interned integer crosses the boundary.                                                                            |
| 3   | `TextSize` u32 byte offsets | `Span` is still u32, but `PositionEncoding` is declared per plugin and the core normalises. The offsets are not assumed to be bytes.                    |
| 4   | Cargo workspace assumption  | `discover_units(root) -> Vec<Unit>`. No `Cargo.toml`, manifest or workspace concept in `plugin-api`.                                                    |
| 5   | salsa snapshot lifecycle    | No database, snapshot, or handle type is exposed. `lang-rust` owns its salsa lifecycle behind `&self`.                                                  |
| 6   | `SymbolKind`                | Neutral six-variant enum plus `raw_kind: String`. `Trait`, `Impl`, `Macro` and `Static` map to `Type`/`Other` with the Rust term preserved for display. |
| 7   | `Documentation` type        | `doc: Option<String>` + `doc_format: DocFormat`.                                                                                                        |
| 8   | classifier prefixes         | `Category` in the waist; `Classifier` trait in `plugin-api`; prefix strings only inside the plugin.                                                     |

Note on leak 3: keeping `u32` offsets is a conscious choice, not an oversight, and
`SourceRange::span` being `Option` does not soften it — an offset that _is_ reported is
still a raw offset in the plugin's declared encoding. The
alternative — line/column pairs — would force every plugin to compute them, and ra_ap,
tree-sitter and SCIP are all offset-based. The neutrality guarantee comes from declaring
the _encoding_, not from changing the _representation_.

---

## 5. The plugin registry

```rust
/// Analysis plugins only. A renderer is not a `Plugin` (§3.6) and is never detected.
pub struct Registry {
    plugins: Vec<Box<dyn Plugin>>,
}

impl Registry {
    /// Marker-file and extension matching. An empty `Detection::marker_files`
    /// matches nothing (§2), so a plugin declaring none is never returned here.
    pub fn detect(&self, root: &Path) -> Vec<&dyn Plugin>;

    /// Explicit selection by id, bypassing detection entirely. This is how the
    /// fixture plugin is chosen — see below.
    pub fn select(&self, id: PluginId) -> Option<&dyn Plugin>;
}
```

`detect` matches each plugin's declared `Detection` (marker files, extensions) against the
repository. In v0.1 exactly one real plugin is registered, plus the fixture in test builds.

**No language-specific branch may appear in `reachgraph-core` or `reachgraph-cli`.** A
registry with one entry costs nothing; a hardcoded branch costs a core change per
language.

### The fixture plugin is explicitly selected, never detected

`reachgraph-fixture` declares `Detection { marker_files: &[], extensions: &[] }`. Under
§2's rule that an empty marker list matches nothing, `detect` can never return it — the
guarantee is structural, not a special case in `detect`.

Selection is explicit, by one of two routes:

| caller                | route                                                                                                                                                                |
| --------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| tests                 | `Index::build(root, &[&fixture], opts)` — plan-01 §4.2 takes an explicit plugin slice, so a test injects the fixture directly and never consults the registry at all |
| a developer, manually | `--plugin fixture`, a hidden flag gated on the same Cargo feature as the crate, resolving through `Registry::select`                                                 |

The flag is hidden and feature-gated because it is a development affordance, not a
product feature: a release build has no fixture crate compiled in, so the flag does not
exist to be typed (§1, Features).

The reason this matters beyond tidiness: a fixture that could be _detected_ would, on any
repository that happened to contain a `reachgraph.fixture.json`, quietly replace the real
analysis with hand-written JSON — producing a complete, plausible, entirely fictional call
graph. Explicit selection means fixture data can only ever appear because somebody asked
for it by name.

---

## 6. Test strategy

Test-driven: failing test first, red → green → refactor. These tests are written **before**
the traits compile, and they are the acceptance criteria for this plan.

### 6.1 Neutrality tests — the mechanical guards

| test                                 | asserts                                                                                                                                                            | fails when                                                                       |
| ------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------- |
| `fixture_implements_every_trait`     | `reachgraph-fixture` implements `Plugin`, `LanguagePlugin`, `SymbolProvider`, `EdgeProvider`, `RootProvider`, `Classifier`. Compile-level, via `assert_impl_all!`. | a signature requires something only a real language engine can produce           |
| `no_plugin_depends_on_core`          | parse `cargo metadata`; assert `reachgraph-core` is absent from the dependency graph of every `reachgraph-*` crate whose name is not `core` or `cli`               | someone reaches into waist internals                                             |
| `plugin_api_has_no_ra_ap_dependency` | `cargo metadata`: `ra_ap_*` absent from `plugin-api`'s dependency graph                                                                                            | a Rust type leaks into the contract                                              |
| `public_api_snapshot_matches`        | a checked-in snapshot of `plugin-api`'s complete public surface (`cargo public-api` or equivalent), asserted in CI                                                 | **any** type, field or signature in the contract changes without a reviewed diff |

The first two are the load-bearing pair. `fixture_implements_every_trait` is ADR-0008's
mechanism made executable; `no_plugin_depends_on_core` is §1's rule made executable.

`public_api_snapshot_matches` replaces the earlier `no_position_type_in_edge_api`, which
was a grep over `EdgeProvider`'s method signatures for `position`, `offset` and `cursor`.
A grep is defeated by renaming a parameter, and it guarded exactly one of the eight leaks
— the one ADR-0008 calls the highest risk of the eight. A snapshot is structural rather
than lexical: it covers all eight at once, it makes an added field or a changed signature
a diff a reviewer must approve, and it cannot be evaded by choosing a different word.
Plan-02 §7.2 carries the mechanics.

### 6.2 Waist tests, against fixtures only

No `cargo metadata` on a real repo, no indexing, no timing, no requirement that anything
was built (ADR-0008's second benefit).

- `reachable_set_from_single_root` — depth-limited traversal returns the expected node set.
- `unreachable_is_complement_over_all_roots`.
- `unreachable_records_root_coverage` — ADR-0007: the output names which contracts and
  versions were covered.
- `v1_and_v2_are_separate_roots` — same operation name, different `version`, two shards.
- `missing_version_stays_none` — a contract with no version yields `None`, never `"v1"`.
- `unresolved_edge_is_not_silently_resolved` — an `EdgeTarget::Unresolved` with two
  candidates stays unresolved in the graph.
- `edge_provenance_survives_graph_build` — `provenance` and `inference_mode` are present
  on every edge in the built graph.
- `span_none_is_not_offset_zero` — a symbol with `span: None` and one with
  `span: Some(Span { 0, 0 })` stay distinguishable in the type and in the artifact, and
  `file` survives in both.
- `container_*` — four tests, specified in plan-01 §10.2: `Symbol::container` is emitted
  verbatim, creates no edge, changes no reachable set, and a `container` pointing at an
  unknown `NodeId` builds without error.
- `unbound_root_is_reported_not_dropped` — a `RootBinding::Unbound` root appears in
  `endpoints.json` with its reason and in the coverage record, and produces no shard.

The last two are the acceptance criteria for the two decisions recorded in §2 (open
questions 3 and 5). Plan-01 and plan-02 expand both into the full case set.

### 6.3 Not tested in this plan

`ra_ap` integration (plan-03), proto parsing (plan-04), HTML output (plan-05). This plan
ships traits, shared types, the registry and the test harness — no language engine.

---

## 7. Roadmap

| plan   | title                                     | depends on |
| ------ | ----------------------------------------- | ---------- |
| **00** | workspace and plugin API contract         | —          |
| 01     | core waist: graph, reachability, sharding | 00         |
| 02     | fixture plugin                            | 00         |
| 03     | `lang-rust` via `ra_ap_*`                 | 00, 01, 02 |
| 04     | `roots-proto-tonic`                       | 00, 01, 02 |
| 05     | `render-html` (Cytoscape, root-sharded)   | 00, 01     |
| 06     | CLI and `serve`                           | 01, 05     |
| 07     | PyPI packaging                            | 06         |

**01 and 02 proceed in parallel** once this plan lands — the waist and its test double have
no dependency on each other beyond the traits defined here. Building them together is
deliberate: 02 is what proves 01's interfaces are honest.

**03 and 04 depend on both.** Neither should start before the fixture plugin compiles,
because the fixture is what catches an ra_ap-ism at the moment it is introduced rather than
after `lang-rust` has been built around it.

**07 carries unmeasured facts.** These are open measurements, not estimates to guess at:

- maturin's binary-wheel behaviour, and whether entry-point discovery needs a Python
  launcher shim
- binary size with `ra_ap_*` linked — INFERRED 40–80 MB from a MEASURED 14.8 MB gzipped
  rust-analyzer release asset, never measured directly
- the platform wheel matrix, against PyPI's MEASURED 100.0 MiB per-file default limit

Plan 07 must measure each before committing to a packaging design.

---

## 8. Open questions

Named rather than answered where they cannot be settled without writing code. Two — 3 and
5 — were settled on evidence already in hand and are struck through with their resolution
recorded in place, rather than deleted.

1. **Does `edges_from(&NodeId)` map cleanly onto `ra_ap`?** The conversion NodeId →
   `FilePosition` requires a stable node → file+offset mapping held by the plugin across
   calls. INFERRED that `lang-rust` keeps a side table built during `symbols_in`. If that
   turns out to require a live salsa snapshot per call, the lifecycle question (leak 5)
   gets harder and `edges_from` may need to take a `&Unit` for context.

2. **Is `Span` in the plugin's own encoding the right call, or should the plugin normalise
   before returning?** Normalising at the boundary is simpler for the core and costs the
   plugin a conversion it may be able to skip. Deferred until a second encoding actually
   exists — at n=1 there is no evidence to decide on.

3. ~~**Does `SymbolIndex` give root providers enough to bind a handler?**~~
   **RESOLVED 2026-09-17 — `Symbol` gains `container: Option<NodeId>`.** See §2 and §3.4.

   Not by convention on `raw_kind`. The join the whole contract layer rests on cannot
   depend on a string the waist is forbidden to interpret: `raw_kind` is display text, so
   a convention over it is unenforced, undiscoverable and silently breakable by any
   plugin author. MEASURED, design.md §4 — the `create_task` / `MockDb` collision is
   real, in one repository, today; the disambiguation it requires is the enclosing `impl`
   block's trait.

   `container`, not `parent`: _parent_ is ambiguous across languages — parent module,
   parent scope, parent frame. _Container_ names one thing, the enclosing definition, and
   is language-neutral: Rust `impl` block, Go receiver type, Java class, Python class.

   The field is a `NodeId`, so the waist can carry it while remaining unable to read it —
   the same opacity rule as ADR-0003 field 3. The roots plugin walks up and reads the
   container's `raw_kind`. The waist never does.

   This was correctly flagged as ADR-0003-class: a plugin that ships without collecting
   `container` produces data from which it cannot be recovered. It lands before
   `lang-rust`, not after.

4. **How does the core pair a symbol provider with an edge provider?** Specified above as
   "by `PluginId`", which assumes one crate provides both for a given language. If a
   language ever needs two different crates, pairing needs an explicit declaration. Not a
   v0.1 problem.

5. ~~**What does `confidence: f32` on `Root` mean numerically?**~~
   **RESOLVED 2026-09-17 — `confidence` is dropped entirely, not rescaled.** `Root::node`
   and `Root::confidence` are replaced by one field, `binding: RootBinding`. See §2.

   Neither "define the scale" nor "enumerate the confidence levels". There is no scale to
   define, because there is no quantity being measured. A root binds to a handler or it
   does not.

   MEASURED, design.md §5, the failure mode a float invites: code_graph emits 59 `CALLS`
   edges at confidence 0.55, each with two or three candidate targets. The number does not
   record a measurement; it records indecision, and it then renders indistinguishably from
   a fact. 0.55 is not a weaker claim — it is a claim that no claim was made, wearing the
   costume of one.

   An unbound root is a **reported gap**, carried into `endpoints.json` and the coverage
   record. That is design.md §8's binding rule applied one layer up: show a missing edge as
   missing, never infer one to fill a hole. It also matters for ADR-0007's partial-index
   problem — an unbound root means a real handler may read as unreachable, so the gap has
   to reach the artifact rather than a log line.

   Note the field this does _not_ touch: `Edge::inference_mode` stays an enum. An edge's
   strength is a real property of how it was derived (ADR-0003 field 4); a root's binding
   is a yes or a no.

6. **Does `Span` need anything beyond an offset pair?** Settled for v0.1 as **no** —
   `Span { start: u32, end: u32 }`, offsets in the declaring plugin's `PositionEncoding`,
   and nothing else. What was considered and left out, each with the reason:

   | candidate                                 | left out because                                                                                                                                                                                                                                                   |
   | ----------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
   | a `PositionEncoding` on the `Span` itself | it is declared per plugin (ADR-0003 field 2) and a node knows its plugin. Duplicating it per span creates two places to disagree, and invites a consumer to compare two offsets without checking they share an encoding — the failure the field exists to prevent. |
   | line and column                           | §4's note: ra_ap, tree-sitter and SCIP are all offset-based, so every plugin would have to compute them. Neutrality comes from declaring the encoding, not from changing the representation.                                                                       |
   | a separate name / selection range         | LSP's `DocumentSymbol` carries `range` _and_ `selectionRange`, and ra_ap distinguishes a focus range from a full range. `Span` is the definition's full extent only.                                                                                               |

   The last one is the live residual, and it is a real question rather than a closed one:
   a renderer that deep-links "jump to the name of this function" wants the name range,
   not the whole body. Nothing in v0.1 needs it — plan-05 renders a graph, not an editor —
   so it is not added on speculation. **If a renderer ever needs it, it is
   ADR-0003-class:** a plugin that shipped without collecting the name range produces
   symbols from which it cannot be recovered. Revisit when a consumer asks, and treat the
   ask as urgent rather than cosmetic.
