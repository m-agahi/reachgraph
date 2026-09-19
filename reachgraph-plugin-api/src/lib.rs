//! The reachgraph plugin contract.
//!
//! This crate is the contract half of ADR-0003's waist: the traits a plugin
//! implements, and the schema types it exchanges with the core. Construction
//! and the algorithms over those types — graph assembly, reachability,
//! complement, sharding — live in `reachgraph-core` and are not swappable.
//!
//! **Dependencies point toward this crate and no plugin depends on
//! `reachgraph-core`** (`docs/plans/00-workspace-and-plugin-api.md` §1). That is
//! why the schema types live here rather than in the core: a renderer is a
//! plugin and must receive the graph, so graph types in `core` would force
//! every renderer to depend on `core`. "Not a plugin" means no plugin may
//! redefine the schema. It does not mean no plugin may see it.
//!
//! # One file, on purpose
//!
//! The whole contract is one module. A `pub` item inside a private module is
//! not public, so a multi-module crate makes *declared* and *effective*
//! visibility two different things — and `public_api_snapshot_matches`
//! (plan-00 §6.1) asserts over the effective surface. With one module the two
//! cannot disagree.
//!
//! # The graph types, and why they carry no serde derive
//!
//! [`Node`], [`GraphView`], [`Shard`], [`IndexCoverage`] and
//! [`PluginDescriptor`] are named as residents of this crate by plan-00 §1 and
//! defined by plan-01 §3. They are here.
//!
//! Plan-01 §3 sketches each with `#[derive(Serialize, Deserialize)]`. **That
//! is not implementable and the artifact schema lives in `reachgraph-core`
//! instead.** [`PluginId`] holds a `&'static str`; `NodeId` holds a
//! `PluginId`; every type above holds a `NodeId`. `Deserialize<'de> for &'a
//! str` requires `'de: 'a`, so a `&'static str` field can only be deserialized
//! from an input that is itself `&'static` and needs no unescaping — an
//! accident of one call site rather than a contract. Widening `PluginId` to
//! `String` was considered and rejected before this crate shipped: it costs
//! `Copy` and a cheap hash on `NodeId`, which the waist hashes constantly.
//!
//! So the derives are absent, this crate keeps its empty dependency list, and
//! `reachgraph-core` owns a serde mirror of the artifact — the same split
//! `reachgraph-fixture` already makes for its own input format, and for the
//! same stated reason.
//!
//! # What is not here yet
//!
//! `Renderer` is the one trait of plan-00 §3 that is absent. Its `render`
//! takes a `&GraphView`, which now exists, so the trait is plan-05's to add
//! with the renderer that needs it. [`OutputSink`] is here already: plan-01
//! §8.6 has the waist's own `emit.rs` writing through it.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------

/// Stable identifier for a plugin. Used as the namespace half of every
/// [`NodeId`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PluginId(pub &'static str);

/// ADR-0003 field 3. The core MUST NEVER parse `raw`.
///
/// Plugins choose their own encoding: a SCIP symbol, a path plus an offset,
/// anything stable. Keeping the string opaque is what lets the waist stay
/// uncommitted on which analysis technology a given language uses.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct NodeId {
    /// The plugin that minted this id, and the only party allowed to interpret
    /// `raw`.
    pub plugin: PluginId,
    /// Opaque to the core. Compared for equality, hashed, emitted — never
    /// parsed, split or pattern-matched.
    pub raw: String,
}

// ---------------------------------------------------------------------------
// Position
// ---------------------------------------------------------------------------

/// ADR-0003 field 2. Declared per plugin, never assumed.
///
/// LSP counts UTF-16 code units, SCIP and tree-sitter count UTF-8 bytes. Pick
/// one silently and every plugin is off by N in a file containing non-ASCII
/// text — a defect that ASCII-only test fixtures hide indefinitely.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PositionEncoding {
    /// `ra_ap`, tree-sitter, SCIP.
    Utf8Bytes,
    /// LSP.
    Utf16CodeUnits,
    /// Offsets counted in Unicode scalar values.
    Utf32CodePoints,
}

/// A half-open offset pair in the declaring plugin's [`PositionEncoding`], and
/// nothing else.
///
/// Deliberately minimal — plan-00 §8 question 6 records what was considered and
/// left out, and why a name range is the live residual rather than a closed
/// question.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Span {
    /// Inclusive start offset.
    pub start: u32,
    /// Exclusive end offset.
    pub end: u32,
}

/// A file, and — separately — an offset pair within it.
///
/// **The two fields carry different certainty and must not share one
/// `Option`.** A plugin that names a symbol almost always knows which file it
/// is in; it may not know where in the file. `file` is therefore required and
/// `span` is not.
///
/// The earlier shape put the `Option` one level up — `Symbol::range:
/// Option<SourceRange>` — which made a plugin discard the file it *did* know in
/// order to be honest about the offset it did not. This is ADR-0003's
/// honest-absence rule, instance 4: widening optionality destroys information
/// as surely as a sentinel invents it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SourceRange {
    /// The file the symbol or call site is in. Always known.
    pub file: PathBuf,
    /// `None` = this plugin has no offset for this symbol and says so. Never a
    /// sentinel: `Span { start: 0, end: 0 }` is indistinguishable from a real
    /// offset 0.
    pub span: Option<Span>,
}

// ---------------------------------------------------------------------------
// Symbols
// ---------------------------------------------------------------------------

/// ADR-0008 leak 6. Neutral; [`Symbol::raw_kind`] preserves the plugin's own
/// term for display.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SymbolKind {
    /// A free function.
    Function,
    /// A function bound to a type or an interface.
    Method,
    /// A type definition. Rust `trait`, `impl` and `struct` all land here.
    Type,
    /// A namespace-like grouping: module, package, namespace.
    Module,
    /// A member of a type.
    Field,
    /// Anything the six variants above do not name. `raw_kind` still carries
    /// the plugin's own word for it.
    Other,
}

/// How to render [`Symbol::doc`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DocFormat {
    /// Render verbatim.
    Plain,
    /// CommonMark.
    Markdown,
}

/// ADR-0008 leak 7. One symbol as a plugin reports it.
#[derive(Clone, Debug)]
pub struct Symbol {
    /// This symbol's identity, in the emitting plugin's namespace.
    pub id: NodeId,
    /// The bare name, as written in source.
    pub name: String,
    /// The neutral kind the waist may reason about.
    pub kind: SymbolKind,
    /// The plugin's own term, preserved for display. The waist never matches on
    /// it.
    pub raw_kind: String,
    /// Required. Every symbol a plugin can name, it can place in a file; the
    /// uncertainty lives one level down, in [`SourceRange::span`].
    ///
    /// The defect this shape fixes was never "the range should be optional" —
    /// it was a type conflating two facts of different certainty, so that
    /// saying "I have no offset" also said "I have no file".
    pub range: SourceRange,
    /// Documentation text, if the plugin found any. ADR-0005: it arrives inside
    /// the symbol because the resolver has already parsed the file.
    pub doc: Option<String>,
    /// How to render `doc`. Meaningful only when `doc` is `Some`.
    pub doc_format: DocFormat,
    /// The enclosing definition, if the language has one. Rust: the `impl`
    /// block. Go: the receiver type. Java and Python: the class.
    ///
    /// MEASURED, `docs/design.md` §4: `create_task` exists twice in one
    /// repository — the real handler and a `MockDb` in a test. Binding an
    /// operation to a handler needs the enclosing block, so name alone is a
    /// MEASURED failure.
    ///
    /// **The waist never interprets this field.** It is carried, stored and
    /// emitted verbatim, exactly as with [`NodeId::raw`]. It contributes no
    /// edge and no reachability, and a dangling `container` is plugin data
    /// rather than a core error.
    pub container: Option<NodeId>,
    /// Whether this symbol is test code. With `container`, this is how a roots
    /// plugin separates the real handler from the mock.
    pub is_test: bool,
}

// ---------------------------------------------------------------------------
// Units
// ---------------------------------------------------------------------------

/// ADR-0008 leak 4. "What is the unit of analysis" is per-language.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct UnitId(pub String);

/// One unit of analysis: a Rust crate, a Go package, a Python source root.
#[derive(Clone, Debug)]
pub struct Unit {
    /// Identity within the emitting plugin.
    pub id: UnitId,
    /// What to show a reader.
    pub display_name: String,
    /// The directory this unit is rooted at.
    pub root: PathBuf,
}

// ---------------------------------------------------------------------------
// Edges
// ---------------------------------------------------------------------------

/// ADR-0003 field 4, first half. What produced this edge.
#[derive(Clone, Debug)]
pub struct Provenance {
    /// The plugin that emitted the edge.
    pub plugin: PluginId,
    /// Free text naming the engine and its version, e.g. `"ra_ap_ide 0.0.352"`.
    pub engine: String,
}

/// ADR-0003 field 4, second half. How strong the claim is.
///
/// ADR-0004 makes this load-bearing from the first non-Rust language: a
/// directly resolved edge and an inferred one are different-strength claims and
/// must not render identically.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InferenceMode {
    /// A semantic engine answered directly. Near-certain.
    Resolved,
    /// Scope and binding resolution only; no types consulted.
    Lexical,
    /// Required type inference to pick the target. Depends on inference
    /// succeeding.
    TypeInferred,
    /// Derived by asking which definition's range encloses a reference
    /// occurrence.
    Enclosure,
}

/// Where an edge points.
///
/// `docs/design.md` §8: "Show a missing edge as missing; never infer one to
/// fill a hole." An unresolved call is recorded, never silently resolved to a
/// best guess.
#[derive(Clone, Debug)]
pub enum EdgeTarget {
    /// The plugin resolved the call to one definition.
    Resolved(NodeId),
    /// The plugin could not resolve the call and says so.
    Unresolved {
        /// The name at the call site.
        name: String,
        /// Every definition the plugin considered. May be empty.
        candidates: Vec<NodeId>,
    },
}

/// One call edge as a plugin reports it.
#[derive(Clone, Debug)]
pub struct Edge {
    /// The calling definition.
    pub from: NodeId,
    /// The callee, resolved or not.
    pub to: EdgeTarget,
    /// Where the call is written. `None` when the plugin has no location for
    /// it — the same honest-absence rule as [`SourceRange::span`], one level up.
    pub call_site: Option<SourceRange>,
    /// ADR-0003 field 4, first half.
    pub provenance: Provenance,
    /// ADR-0003 field 4, second half.
    pub inference_mode: InferenceMode,
}

// ---------------------------------------------------------------------------
// Classification
// ---------------------------------------------------------------------------

/// ADR-0002: categories are the waist's; the prefixes that produce them are the
/// plugin's.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Category {
    /// Code this repository owns.
    FirstParty,
    /// Code produced by a build step.
    Generated,
    /// Another member of the same workspace.
    WorkspaceSibling,
    /// A dependency from outside the workspace.
    ThirdParty,
    /// The language's own standard library.
    Stdlib,
}

// ---------------------------------------------------------------------------
// Roots
// ---------------------------------------------------------------------------

/// ADR-0007. The contract an operation belongs to.
///
/// `Hash` and `Ord` are derived because the waist groups and orders roots by
/// contract. That is contract data the plugin spells for grouping, not a
/// [`NodeId`] — ordering one of those by content is forbidden (ADR-0003
/// field 3), and this is the distinction.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ContractId(pub String);

/// Which side of a contract an operation sits on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Direction {
    /// This repository implements the operation.
    Served,
    /// This repository calls it.
    Consumed,
}

/// Whether the contract operation was bound to a handler symbol.
///
/// There is no confidence score. `docs/design.md` §5 MEASURED the failure mode
/// a float invites: `code_graph` emits 59 `CALLS` edges at confidence 0.55, each
/// with two or three candidate targets — a number that records indecision and
/// then renders as if it were a measurement. A root either binds to a handler
/// or it does not.
///
/// An `Unbound` root is a **reported gap**, never a dropped row.
#[derive(Clone, Debug)]
pub enum RootBinding {
    /// The provider bound the operation to this definition.
    Bound(NodeId),
    /// The provider found no handler and says why.
    Unbound {
        /// Plugin-authored free text, carried into the artifact for the report.
        reason: String,
    },
}

/// ADR-0003 field 6, refined by plan-00 §2: one entry point into the graph.
#[derive(Clone, Debug)]
pub struct Root {
    /// The contract this operation belongs to.
    pub contract: ContractId,
    /// ADR-0007: a missing version is `None`. NEVER defaulted to `"v1"`.
    ///
    /// Wherever this field is deserialized, a missing key is a parse error
    /// rather than a silent `None` — a serde default is exactly the mechanism
    /// that would let a round trip invent `"v1"` or erase a plugin's deliberate
    /// `None`.
    pub version: Option<String>,
    /// The service the operation belongs to, as the plugin spells it.
    pub service: String,
    /// The operation's own name, as the plugin spells it.
    pub operation: String,
    /// Served or consumed.
    pub direction: Direction,
    /// The cross-repository join key, **spelled by the roots plugin**.
    ///
    /// ADR-0007 fixes the key as the fully-qualified operation name. That
    /// spelling is contract-shaped and framework-shaped, and ADR-0003 forbids
    /// anything framework-shaped reaching the waist — so the plugin composes it
    /// and the core stores it **opaquely**: hashed, compared for equality,
    /// emitted. Never parsed, never split, never used to recover a version.
    ///
    /// Present from v0.1 even though cross-repository stitching is out of scope
    /// (ADR-0008): a plugin that ships without emitting it produces roots from
    /// which the key cannot be recovered afterwards.
    pub join_key: String,
    /// Replaces the earlier `node` plus `confidence` pair. A separate `node`
    /// field would be incoherent for [`RootBinding::Unbound`].
    pub binding: RootBinding,
}

/// One `(contract, version)` pair — ADR-0007's unit of root partitioning.
///
/// A named struct rather than plan-01 §6.1's tuple. `reachgraph-fixture`
/// already made this correction on its own side, for its own reason: "a
/// positional pair cannot carry `deny_unknown_fields`, and it cannot tell a
/// `null` version from an omitted one — which is the exact distinction
/// ADR-0007 turns on". The same argument applies to the type, so the waist
/// carries one spelling of a version key rather than two.
///
/// `None` is a key like any other. It is never merged with, coerced to, or
/// displayed as `"v1"`.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct VersionKey {
    /// The contract.
    pub contract: ContractId,
    /// Its version. ADR-0007: a missing version is `None`, never `"v1"`.
    pub version: Option<String>,
}

/// ADR-0007's binding requirement: the index records what it covered, so a
/// partial root set cannot make live code read as unreachable.
#[derive(Clone, Debug)]
pub struct Coverage {
    /// Every contract this provider looked at.
    pub contracts: Vec<ContractId>,
    /// Every version key it looked at. A `None` version is a real entry, not a
    /// gap in the list.
    pub versions: Vec<VersionKey>,
}

// ---------------------------------------------------------------------------
// The built graph — plan-01 §3
// ---------------------------------------------------------------------------

/// A node in the built graph. Not every edge target was indexed.
#[derive(Clone, Debug)]
pub struct Node {
    /// This node's identity.
    pub id: NodeId,
    /// `None` = an edge resolved to this id, but no provider ever emitted a
    /// symbol for it. Third-party and stdlib targets land here, and so does a
    /// cross-repository client stub in a repository that was never built.
    pub symbol: Option<Symbol>,
    /// `None` = no classifier was registered for this node's plugin, or the
    /// node was never indexed and therefore has no path to classify.
    pub category: Option<Category>,
    /// The unit this node's symbol came from. `None` for an external node — it
    /// was never indexed, so it belongs to no unit. The outermost grouping
    /// level.
    pub unit: Option<UnitId>,
    /// Breadth-first distance from this view's root. `Some(0)` is the root
    /// itself.
    ///
    /// `None` in the index-wide [`GraphView`], where there is no single root to
    /// measure from, and `Some(_)` in every shard view. The distinction is in
    /// the type because a renderer asking "how deep is this node" must get an
    /// answer that is wrong in neither direction.
    pub depth: Option<u32>,
    /// True when this node sits at the view's depth limit and has out-edges
    /// that were not followed. Frontier, not leaf: a renderer that draws the
    /// two alike is lying.
    pub frontier: bool,
}

/// What a plugin declared about itself, carried into the artifact so a consumer
/// can interpret offsets and attribute edges without a second lookup.
#[derive(Clone, Debug)]
pub struct PluginDescriptor {
    /// The plugin's identity.
    pub id: PluginId,
    /// ADR-0003 field 2, as that plugin declared it. Per plugin, never global.
    pub position_encoding: PositionEncoding,
    /// ADR-0003 field 1, as that plugin declared it.
    pub capabilities: Vec<Capability>,
}

/// One root whose operation bound to no handler. ADR-0007: a reported gap,
/// never a dropped row.
#[derive(Clone, Debug)]
pub struct UnboundRoot {
    /// The contract the operation belongs to.
    pub contract: ContractId,
    /// ADR-0007: `None` is an assertion, never a default.
    pub version: Option<String>,
    /// The service, as the plugin spells it.
    pub service: String,
    /// The operation, as the plugin spells it.
    pub operation: String,
    /// Served or consumed.
    pub direction: Direction,
    /// The provider's own words for why nothing bound.
    pub reason: String,
}

/// The aggregate of every [`RootProvider::coverage`] plus what the core itself
/// observed.
///
/// ADR-0007's binding requirement 1: the index records what it covered, in the
/// artifact rather than in a log line. [`Coverage`] is one provider's claim
/// about what *it* looked at; this is the union, because a provider cannot know
/// what the other providers did.
#[derive(Clone, Debug)]
pub struct IndexCoverage {
    /// Every contract any provider looked at.
    pub contracts: Vec<ContractId>,
    /// Every version key the index looked at. A `None` version is a real entry.
    pub versions: Vec<VersionKey>,
    /// How many roots the index holds, bound and unbound together.
    pub roots_total: usize,
    /// How many of them bound to a handler.
    pub roots_bound: usize,
    /// The rest, each with the reason its provider gave.
    pub unbound_roots: Vec<UnboundRoot>,
    /// Every unit any symbol provider enumerated.
    pub units_indexed: Vec<UnitId>,
    /// Every plugin that contributed.
    pub plugins: Vec<PluginId>,
    /// Categories at which traversal stopped. A **declared limitation**, in the
    /// artifact rather than in a release note: code reached only *through* a
    /// node of one of these categories was not followed, so a consumer can see
    /// that the complement was computed against a deliberately truncated walk.
    pub traversal_terminal_categories: Vec<Category>,
    /// True when any provider failed and the run continued anyway. A consumer
    /// must weaken every unreachability claim when this is set.
    pub partial: bool,
}

/// The lookups [`GraphView`] answers, built once on first use.
///
/// Never part of equality, never part of the artifact, and rebuilt rather than
/// carried: a view that has answered a lookup and one that has not are the same
/// view.
#[derive(Clone, Debug, Default)]
struct ViewIndex {
    by_id: HashMap<NodeId, usize>,
    out_edges: HashMap<NodeId, Vec<usize>>,
    in_edges: HashMap<NodeId, Vec<usize>>,
    contained: HashMap<NodeId, Vec<usize>>,
}

/// What a renderer receives.
///
/// Construction is the core's. The accessors below are inherent methods on
/// owned data — lookups, not algorithms — which is what keeps a renderer from
/// needing `reachgraph-core` as a dependency. A renderer that had to recompute
/// breadth-first depth to draw a depth slider would need the traversal, and
/// plan-00 §1's dependency rule would break.
#[derive(Clone, Debug)]
pub struct GraphView {
    /// Every node in this view.
    pub nodes: Vec<Node>,
    /// Every edge in this view, unresolved ones included.
    pub edges: Vec<Edge>,
    /// The roots this view was computed from. One in a shard; all of them in
    /// the index-wide view.
    pub roots: Vec<Root>,
    /// What each contributing plugin declared about itself.
    pub plugins: Vec<PluginDescriptor>,
    /// What the index covered.
    pub coverage: IndexCoverage,
    /// Built on first lookup. See [`ViewIndex`].
    index: OnceLock<ViewIndex>,
}

impl GraphView {
    /// Assemble a view from parts the core computed.
    ///
    /// The one constructor, because [`GraphView::index`] is private: a view is
    /// always internally consistent with the nodes and edges it was given.
    pub fn new(
        nodes: Vec<Node>,
        edges: Vec<Edge>,
        roots: Vec<Root>,
        plugins: Vec<PluginDescriptor>,
        coverage: IndexCoverage,
    ) -> Self {
        Self {
            nodes,
            edges,
            roots,
            plugins,
            coverage,
            index: OnceLock::new(),
        }
    }

    fn view_index(&self) -> &ViewIndex {
        self.index.get_or_init(|| {
            let mut index = ViewIndex::default();

            for (position, node) in self.nodes.iter().enumerate() {
                index.by_id.insert(node.id.clone(), position);

                if let Some(container) = node.symbol.as_ref().and_then(|s| s.container.as_ref()) {
                    index
                        .contained
                        .entry(container.clone())
                        .or_default()
                        .push(position);
                }
            }

            for (position, edge) in self.edges.iter().enumerate() {
                index
                    .out_edges
                    .entry(edge.from.clone())
                    .or_default()
                    .push(position);

                if let EdgeTarget::Resolved(target) = &edge.to {
                    index
                        .in_edges
                        .entry(target.clone())
                        .or_default()
                        .push(position);
                }
            }

            index
        })
    }

    /// One node by identity.
    pub fn node(&self, id: &NodeId) -> Option<&Node> {
        self.view_index()
            .by_id
            .get(id)
            .and_then(|position| self.nodes.get(*position))
    }

    /// Breadth-first distance from this view's root. `None` in the index-wide
    /// view, and `None` for a node this view does not hold.
    pub fn depth_of(&self, id: &NodeId) -> Option<u32> {
        self.node(id).and_then(|node| node.depth)
    }

    /// The greatest depth present. What a depth slider's maximum is set from.
    pub fn max_depth(&self) -> Option<u32> {
        self.nodes.iter().filter_map(|node| node.depth).max()
    }

    /// Every node at exactly this depth, in view order.
    pub fn nodes_at_depth(&self, depth: u32) -> Vec<&Node> {
        self.nodes
            .iter()
            .filter(|node| node.depth == Some(depth))
            .collect()
    }

    /// The node's enclosing definition, if its plugin declared one and this
    /// view holds it.
    ///
    /// A pure link follow: [`Symbol::container`] resolved by equality. The
    /// waist does not interpret what the container *means* — the caller reads
    /// `kind` and `raw_kind` and decides.
    pub fn container_of(&self, id: &NodeId) -> Option<&Node> {
        let container = self.node(id)?.symbol.as_ref()?.container.as_ref()?;
        self.node(container)
    }

    /// The full chain outward, nearest first, terminating at a node with no
    /// container or one this view does not hold.
    ///
    /// Cycle-guarded: a plugin that emits a containment loop gets a truncated
    /// chain, never a hang.
    pub fn container_chain(&self, id: &NodeId) -> Vec<&Node> {
        let mut chain = Vec::new();
        let mut seen: Vec<&NodeId> = vec![id];
        let mut current = id.clone();

        while let Some(next) = self.container_of(&current) {
            if seen.contains(&&next.id) {
                break;
            }
            chain.push(next);
            seen.push(&next.id);
            current = next.id.clone();
        }

        chain
    }

    /// Direct containment children — the inverse of [`GraphView::container_of`].
    pub fn contained_in(&self, id: &NodeId) -> Vec<&Node> {
        self.view_index()
            .contained
            .get(id)
            .map(|positions| {
                positions
                    .iter()
                    .filter_map(|position| self.nodes.get(*position))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Nodes grouped by unit, the outermost box. Externals are excluded; they
    /// belong to no unit.
    pub fn nodes_in_unit(&self, unit: &UnitId) -> Vec<&Node> {
        self.nodes
            .iter()
            .filter(|node| node.unit.as_ref() == Some(unit))
            .collect()
    }

    /// Every edge leaving this node, resolved or not.
    pub fn out_edges(&self, id: &NodeId) -> Vec<&Edge> {
        self.edges_at(&self.view_index().out_edges, id)
    }

    /// Every resolved edge arriving at this node. An unresolved edge arrives
    /// nowhere, by definition.
    pub fn in_edges(&self, id: &NodeId) -> Vec<&Edge> {
        self.edges_at(&self.view_index().in_edges, id)
    }

    fn edges_at<'a>(&'a self, table: &HashMap<NodeId, Vec<usize>>, id: &NodeId) -> Vec<&'a Edge> {
        table
            .get(id)
            .map(|positions| {
                positions
                    .iter()
                    .filter_map(|position| self.edges.get(*position))
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// One root's reachable subgraph. ADR-0006: a shard is the reachable set from
/// one root.
#[derive(Clone, Debug)]
pub struct Shard {
    /// The root this shard was computed from.
    pub root: Root,
    /// The depth the walk stopped at, or `None` for an unlimited walk.
    pub depth_limit: Option<u32>,
    /// Nodes at `depth_limit` that have out-edges not included here. They are
    /// frontier, not leaf.
    pub frontier: Vec<NodeId>,
    /// The subgraph.
    pub view: GraphView,
}

// ---------------------------------------------------------------------------
// Capability, preflight, detection
// ---------------------------------------------------------------------------

/// ADR-0003 field 1.
///
/// No `Render` variant: a renderer is not a [`Plugin`] and declares no
/// capability (plan-00 §3.6). ADR-0002's five plugin kinds are unchanged; only
/// the trait hierarchy is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Capability {
    /// Emits [`Symbol`] values.
    Symbols,
    /// Emits [`Edge`] values.
    Edges,
    /// Emits [`Root`] values.
    Roots,
    /// Answers [`Classifier::classify`].
    Classify,
}

/// ADR-0003 field 5. What a plugin found when it checked its own prerequisites.
///
/// Never `command -v` — `docs/design.md` §10 MEASURED that a name resolving on
/// PATH proves nothing, because `rust-analyzer` resolved there as a `rustup`
/// proxy that loops and is not installed.
#[derive(Clone, Debug)]
pub enum Preflight {
    /// Every prerequisite is met and the plugin found nothing worth saying.
    Ok,
    /// **Passed, with a finding the user should act on.**
    ///
    /// Added 2026-09-19. `Ok | Failed` could not express "the plugin will run,
    /// and what it produces means something different from what you expect" —
    /// so plan-03 §11's checks 3 and 4 returned `Ok` and routed the finding to
    /// the run record, leaving the structured remediation text outside the
    /// type ADR-0003 field 5 built to carry it. That is the honest-absence rule
    /// again, fifth instance: a value that says less than the plugin knows.
    ///
    /// A `Warned` plugin **runs**. Never return [`Preflight::Failed`] for a
    /// non-fatal finding (plan-03 §14 question 11).
    ///
    /// The shape is `Failed`'s, and that symmetry is deliberate rather than
    /// convenient — plan-00 §8 question 7, DECIDED 2026-09-19 while plan-03
    /// §11 was written. The discriminator plan-00 named was whether a plugin
    /// fuses its finding and its fix into one string; plan-03 §11 check 4's
    /// own draft remediation does exactly that. The argument recorded against
    /// the field was that the finding already lives in a run record — and
    /// MEASURED while building `reachgraph-lang-rust`, no run record exists in
    /// this contract or in `reachgraph-core`, so the fact had nowhere else to
    /// go.
    Warned {
        /// What was checked and what was found.
        reason: String,
        /// What the user should do about it. Structured guidance, not a log
        /// line.
        remediation: String,
    },
    /// A prerequisite is not met and the plugin must not run.
    Failed {
        /// What was checked and what was found.
        reason: String,
        /// What the user should do about it.
        remediation: String,
    },
}

/// Plugin-declared detection. ADR-0008: no `if is_rust_project(root)` in the
/// core.
///
/// **An empty `marker_files` matches NOTHING, never everything.** A plugin that
/// declares no markers declares no claim to any repository.
///
/// The inverse rule — empty means match-all — would let one misconfigured or
/// half-written plugin silently hijack detection for every repository, which is
/// ADR-0008's forbidden `if is_rust_project(root)` line arriving from the other
/// direction: the core would not be branching on a language, it would be
/// running one on everything.
#[derive(Clone, Debug)]
pub struct Detection {
    /// File names, relative to the repository root, whose presence claims the
    /// repository. Empty means this plugin is never detected.
    pub marker_files: &'static [&'static str],
    /// File extensions this plugin analyses, without the leading dot.
    ///
    /// Declared metadata, and [`Registry::detect`] deliberately does not walk
    /// the tree for it — see that method's documentation for why a walk here
    /// would be an ADR-0008 violation.
    pub extensions: &'static [&'static str],
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// What a plugin could not do.
///
/// Defined here because every provider method in this crate returns it and no
/// plan defines it. A typed enum rather than a boxed error or a bare string:
/// plan-01 §4.2 makes a provider failure abort the build by default, and a
/// caller deciding whether to continue needs to know what kind of failure it
/// was. Every variant carries the [`PluginId`] because the core holds several
/// plugins at once and the error is otherwise unattributable.
#[derive(Debug)]
pub enum PluginError {
    /// A file or directory the plugin needed could not be read.
    Io {
        /// The plugin that failed.
        plugin: PluginId,
        /// What it tried to read.
        path: PathBuf,
        /// The underlying failure.
        source: std::io::Error,
    },
    /// Input the plugin owns was present but malformed.
    Parse {
        /// The plugin that failed.
        plugin: PluginId,
        /// What it tried to parse.
        path: PathBuf,
        /// What was wrong with it.
        detail: String,
    },
    /// The plugin was asked about a [`Unit`] it did not emit.
    UnknownUnit {
        /// The plugin that was asked.
        plugin: PluginId,
        /// The unit it does not know.
        unit: UnitId,
    },
    /// The plugin was asked about a [`NodeId`] it did not emit.
    ///
    /// A node that was indexed and is now unreachable — dropped from a cache,
    /// no longer in a virtual file system — belongs here. It is never a silent
    /// empty result (plan-03 §10).
    UnknownNode {
        /// The plugin that was asked.
        plugin: PluginId,
        /// The node it does not know.
        node: NodeId,
    },
    /// The plugin's own analysis engine failed or was cancelled.
    Engine {
        /// The plugin that failed.
        plugin: PluginId,
        /// The engine and version, spelled as in [`Provenance::engine`].
        engine: String,
        /// What it reported.
        detail: String,
    },
}

impl fmt::Display for PluginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PluginError::Io { plugin, path, .. } => {
                write!(f, "{}: cannot read {}", plugin.0, path.display())
            }
            PluginError::Parse {
                plugin,
                path,
                detail,
            } => write!(f, "{}: cannot parse {}: {detail}", plugin.0, path.display()),
            PluginError::UnknownUnit { plugin, unit } => {
                write!(f, "{}: unknown unit {}", plugin.0, unit.0)
            }
            PluginError::UnknownNode { plugin, node } => {
                write!(f, "{}: unknown node {}", plugin.0, node.raw)
            }
            PluginError::Engine {
                plugin,
                engine,
                detail,
            } => write!(f, "{}: {engine} failed: {detail}", plugin.0),
        }
    }
}

impl std::error::Error for PluginError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PluginError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// The traits
// ---------------------------------------------------------------------------

/// The base trait every **analysis** plugin implements — symbol, edge, root and
/// classifier kinds.
///
/// A renderer does not implement it (plan-00 §3.6). `position_encoding`,
/// `detection` and `preflight` are all meaningless for a renderer, and a
/// provided default would be a *value*: `Utf8Bytes` from a renderer is
/// indistinguishable downstream from one a plugin meant.
pub trait Plugin: Send + Sync {
    /// This plugin's stable identity.
    fn id(&self) -> PluginId;

    /// ADR-0003 field 1. One crate may provide several capabilities and be
    /// invoked once, rather than re-indexing a repository per kind.
    fn provides(&self) -> &[Capability];

    /// ADR-0003 field 2.
    fn position_encoding(&self) -> PositionEncoding;

    /// What this plugin claims a repository by.
    fn detection(&self) -> Detection;

    /// ADR-0003 field 5.
    fn preflight(&self, root: &Path) -> Preflight;
}

/// Unit discovery.
///
/// ADR-0008 leak 4. Rust returns crates; Go would return packages; Python would
/// return source roots. The core has no opinion, and nothing about a manifest
/// or a workspace appears in this crate.
pub trait LanguagePlugin: Plugin {
    /// Enumerate the units of analysis under `root`.
    fn discover_units(&self, root: &Path) -> Result<Vec<Unit>, PluginError>;
}

/// Symbol and documentation provider.
///
/// Doc text arrives inside [`Symbol`] (ADR-0005: the resolver has already
/// parsed the file, so documentation is not a separate fetch).
pub trait SymbolProvider: LanguagePlugin {
    /// Every symbol in one unit.
    fn symbols_in(&self, unit: &Unit) -> Result<Vec<Symbol>, PluginError>;
}

/// Call-edge provider — ADR-0008 leak 1, the one to get right.
///
/// **Neither method takes a position, a cursor, a file offset or a
/// `FilePosition`.** `ra_ap_ide::Analysis::outgoing_calls` takes one because
/// rust-analyzer's origin is an editor and the question it answers is "what is
/// under the user's cursor". That shape is inherited from LSP, not intrinsic to
/// call graphs.
///
/// A hand-written Go or Java resolver walks a function body and enumerates the
/// call sites it contains. It has no cursor. A position-shaped trait would pass
/// every Rust test, because for Rust it is free, and would tax every future
/// plugin permanently. `reachgraph-lang-rust` converts [`NodeId`] to a
/// `FilePosition` internally, and that conversion never appears here.
pub trait EdgeProvider: LanguagePlugin {
    /// Batch enumeration. The primary path.
    fn edges_in(&self, unit: &Unit) -> Result<Vec<Edge>, PluginError>;

    /// Targeted expansion, for depth-limited traversal from a known node.
    fn edges_from(&self, node: &NodeId) -> Result<Vec<Edge>, PluginError>;
}

/// The lookup surface the core hands to a [`RootProvider`].
///
/// Deliberately narrow: enough to bind a handler, not enough to re-implement
/// the graph. [`SymbolIndex::by_name`] returns a `Vec` rather than an `Option`
/// because `docs/design.md` §4 MEASURED that one name resolves to two
/// definitions in one repository, so the plugin must disambiguate using
/// [`Symbol::container`], [`Symbol::is_test`] and its own knowledge.
pub trait SymbolIndex {
    /// Every symbol with this bare name, across every indexed unit.
    fn by_name(&self, name: &str) -> Vec<&Symbol>;

    /// Every symbol declared in this file.
    fn in_file(&self, path: &Path) -> Vec<&Symbol>;

    /// One symbol by identity. How a plugin follows a
    /// [`Symbol::container`] link.
    fn get(&self, id: &NodeId) -> Option<&Symbol>;
}

/// Root and contract provider.
///
/// Tonic's `impl XServer for T` binding and the CamelCase to snake_case rule
/// live entirely inside `reachgraph-roots-proto-tonic`. The waist receives
/// [`Root`] and nothing else (ADR-0007). A candidate the plugin cannot resolve
/// yields [`RootBinding::Unbound`], not a guess and not a dropped row.
pub trait RootProvider: Plugin {
    /// Every root this provider found, bound or not.
    fn roots(&self, repo_root: &Path, symbols: &dyn SymbolIndex) -> Result<Vec<Root>, PluginError>;

    /// ADR-0007. What this provider actually looked at.
    fn coverage(&self) -> Coverage;
}

/// Path classifier.
///
/// Folding the *implementation* into a language crate is acceptable at n=1, but
/// the trait lives here and the core invokes it through the trait object, never
/// by calling a language-specific function. If the core called into
/// `lang-rust` directly, the fold would have become ADR-0008's forbidden
/// `if is_rust_project(root)` line wearing a different costume.
///
/// [`Category`] values live in the waist. The prefixes that produce them —
/// `src/`, `target/*/out/`, a nix store path, the cargo registry path — live in
/// the plugin, because `docs/design.md` §8 MEASURED that they are
/// language-specific *and* machine-specific.
pub trait Classifier: Plugin {
    /// Classify one path within one unit.
    fn classify(&self, path: &Path, unit: &Unit) -> Category;
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

/// Where a renderer's bytes go.
///
/// The core owns where bytes land — ADR-0006's `out/` layout — and a renderer
/// names a relative path and writes. **It never touches the filesystem
/// itself.** Plan-01 §8.6 puts the waist's own artifact files through the same
/// sink, so the built-in output and a plugin renderer's output land by one
/// mechanism rather than two.
///
/// Object-safe, deliberately. Plan-00 §3.6 spells the renderer's parameter
/// `sink: &mut dyn OutputSink`, so a generic method, a `Self: Sized` bound or a
/// by-value receiver would each break a caller that does not exist yet.
///
/// ONE METHOD, and the restraint is the point. A sink that also offered
/// `mkdir`, `exists` or `base_path` would hand the renderer back the
/// filesystem authority this trait exists to keep away from it, and every
/// method here is contract a later implementor must satisfy.
///
/// This arrives before `Renderer` does — see the crate documentation — because
/// it needs nothing that is not already here. The renderer's own signature
/// takes a `&GraphView`, which plan-01 §3 defines.
pub trait OutputSink {
    /// Write `bytes` at `relative_path`, relative to a root the sink owns and
    /// the caller does not know.
    ///
    /// The path is relative and stays relative. A sink resolves it; a renderer
    /// never does.
    fn write(&mut self, relative_path: &str, bytes: &[u8]) -> std::io::Result<()>;
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/// The plugin registry. Analysis plugins only — a renderer is not a [`Plugin`]
/// (plan-00 §3.6) and is never detected.
///
/// ADR-0008: in v0.1 this holds exactly one real plugin, plus the fixture in
/// test builds. A registry with one entry costs nothing; a hardcoded language
/// branch costs a core change per language.
///
/// `Default` is not in plan-00 §5 and is here for a mechanical reason rather
/// than a design one: clippy's `new_without_default` refuses a public
/// argument-less `new` without it, and `-D warnings` makes that a build
/// failure. It is the one addition to §5's surface, and it is in the snapshot
/// so it stays visible.
#[derive(Default)]
pub struct Registry {
    plugins: Vec<Box<dyn Plugin>>,
}

impl Registry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a plugin. The only way in.
    pub fn register(&mut self, plugin: Box<dyn Plugin>) {
        self.plugins.push(plugin);
    }

    /// Every registered plugin, in registration order.
    ///
    /// PRIVATE, because plan-00 §5 gives `Registry` two methods and this is not
    /// one of them. Plan-06 §3.1 wants to print what each registered plugin
    /// looks for when detection finds nothing, which needs it public — and
    /// that is plan-06's reviewed diff to make, not a surface this plan adds
    /// on its behalf.
    fn plugins(&self) -> impl Iterator<Item = &dyn Plugin> {
        self.plugins.iter().map(AsRef::as_ref)
    }

    /// Every plugin whose declared [`Detection`] claims `root`.
    ///
    /// A plugin matches when one of its `marker_files` exists directly under
    /// `root`. An empty `marker_files` therefore matches nothing, which is
    /// plan-00 §2's rule holding by construction rather than by a special case
    /// here — and it is what keeps the fixture plugin out of every result.
    ///
    /// **`Detection::extensions` is not walked for, and that is a decision
    /// rather than an omission.** Searching a repository for a file extension
    /// means deciding which directories to skip, and every useful answer to
    /// that — `target/`, `node_modules/`, `.venv/` — is a language-specific
    /// rule. ADR-0008 forbids the waist holding one. A marker file is a
    /// plugin-declared name at a plugin-independent location, so matching it
    /// needs no such rule. `extensions` stays declared metadata: plan-06 §3.1
    /// prints it when detection finds nothing, so a repository the tool cannot
    /// handle says what each plugin was looking for.
    pub fn detect(&self, root: &Path) -> Vec<&dyn Plugin> {
        self.plugins()
            .filter(|plugin| {
                plugin
                    .detection()
                    .marker_files
                    .iter()
                    .any(|marker| root.join(marker).exists())
            })
            .collect()
    }

    /// Explicit selection by id, bypassing detection entirely. This is how the
    /// fixture plugin is chosen (plan-00 §5).
    pub fn select(&self, id: PluginId) -> Option<&dyn Plugin> {
        self.plugins().find(|plugin| plugin.id() == id)
    }
}
