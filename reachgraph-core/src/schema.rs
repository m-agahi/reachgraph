//! The artifact schema — plan-01 §8.
//!
//! # Why the schema is here and not in `plugin-api`
//!
//! Plan-01 §3 sketches `Node`, `GraphView`, `Shard`, `IndexCoverage` and
//! `PluginDescriptor` with `#[derive(Serialize, Deserialize)]` on the types
//! themselves. `PluginId` holds a `&'static str`, so none of those types can
//! implement `DeserializeOwned` — `Deserialize<'de> for &'a str` requires
//! `'de: 'a`. Widening `PluginId` to `String` was considered and rejected
//! before `plugin-api` shipped, on the hashing cost it would put in the waist's
//! hot path.
//!
//! So the types stay in `plugin-api` with no serde derive and no serde
//! dependency, and the rows below are this crate's serde mirror of them. It is
//! the same split `reachgraph-fixture` already makes for its own input format,
//! made for the same stated reason.
//!
//! # The rules these types enforce
//!
//! - `#[serde(deny_unknown_fields)]` on every struct, so a typo'd key is a
//!   parse error rather than a silently ignored field.
//! - **No `#[serde(default)]` anywhere**, and that is not sufficient on its
//!   own. Serde's derive routes a missing key on a bare `Option` field through
//!   a deserializer that answers with `None`, so every such field is optional
//!   whether or not anyone asked. Each `Option` whose absence must be a parse
//!   error therefore carries
//!   `#[serde(deserialize_with = "Option::deserialize")]`. `version` is the one
//!   ADR-0007 turns on.
//! - `provenance` and `inference_mode` are not optional (ADR-0003 field 4).

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The sentence a consumer displays, shipped as data rather than composed by
/// each renderer — plan-01 §6.4, ADR-0007 requirement 2.
pub const UNREACHABLE_CLAIM: &str = "not reachable from any endpoint version in this index";

/// The schema version every artifact file carries.
pub const SCHEMA_VERSION: u32 = 1;

/// What produced the artifact.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedBy {
    /// Always `"reachgraph"`.
    pub tool: String,
    /// The crate version.
    pub version: String,
}

/// A node identity as the artifact spells it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeRef {
    /// The plugin that minted it.
    pub plugin: String,
    /// Opaque. Emitted byte for byte.
    pub raw: String,
}

/// Mirrors `PositionEncoding`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EncodingRow {
    /// `ra_ap`, tree-sitter, SCIP.
    Utf8Bytes,
    /// LSP.
    Utf16CodeUnits,
    /// Unicode scalar values.
    Utf32CodePoints,
}

/// Mirrors `Capability`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityRow {
    /// Emits symbols.
    Symbols,
    /// Emits edges.
    Edges,
    /// Emits roots.
    Roots,
    /// Answers `classify`.
    Classify,
}

/// Mirrors `Category`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CategoryRow {
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

/// Mirrors `Direction`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectionRow {
    /// This repository implements the operation.
    Served,
    /// This repository calls it.
    Consumed,
}

/// Mirrors `SymbolKind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKindRow {
    /// A free function.
    Function,
    /// A function bound to a type or an interface.
    Method,
    /// A type definition.
    Type,
    /// A namespace-like grouping.
    Module,
    /// A member of a type.
    Field,
    /// Anything the five above do not name.
    Other,
}

/// Mirrors `DocFormat`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocFormatRow {
    /// Render verbatim.
    Plain,
    /// CommonMark.
    Markdown,
}

/// Mirrors `InferenceMode`. ADR-0003 field 4.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InferenceModeRow {
    /// A semantic engine answered directly.
    Resolved,
    /// Scope and binding resolution only.
    Lexical,
    /// Required type inference to pick the target.
    TypeInferred,
    /// Derived from which definition encloses a reference.
    Enclosure,
}

/// What a plugin declared about itself. `position_encoding` is per plugin,
/// never per node: duplicating it per node would invite a consumer to compare
/// two offsets without checking they share an encoding (ADR-0003 field 2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginRow {
    /// The plugin's identity.
    pub id: String,
    /// Its declared encoding.
    pub position_encoding: EncodingRow,
    /// Its declared capabilities.
    pub capabilities: Vec<CapabilityRow>,
}

/// A half-open offset pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpanRow {
    /// Inclusive start.
    pub start: u32,
    /// Exclusive end.
    pub end: u32,
}

/// A file, and separately an offset pair within it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RangeRow {
    /// Always present on an indexed symbol.
    pub file: PathBuf,
    /// `null` is a plugin saying it has no offset. It is not a span at offset
    /// zero, and a consumer must not treat the two alike.
    #[serde(deserialize_with = "Option::deserialize")]
    pub span: Option<SpanRow>,
}

/// Everything a provider observed about an indexed node.
///
/// A nested object rather than plan-01 §8.3's flat `"indexed": true` plus
/// sibling keys. Two reasons, and the second is the load-bearing one:
/// `#[serde(flatten)]` and `deny_unknown_fields` are mutually exclusive in
/// serde, so a flat shape loses the typo guard; and a flat shape makes
/// `indexed` and the presence of `name` two independently settable facts, which
/// is exactly the sentinel defect ADR-0003's honest-absence rule forbids. Here
/// `symbol: null` is the whole and only way to say "never indexed".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SymbolRow {
    /// The bare name, as written in source.
    pub name: String,
    /// The neutral kind.
    pub kind: SymbolKindRow,
    /// The plugin's own term, for display.
    pub raw_kind: String,
    /// Where it is.
    pub range: RangeRow,
    /// Documentation text, if any.
    #[serde(deserialize_with = "Option::deserialize")]
    pub doc: Option<String>,
    /// How to render `doc`.
    pub doc_format: DocFormatRow,
    /// Whether the provider marked it test code.
    pub is_test: bool,
    /// The enclosing definition, round-tripped verbatim. The waist never
    /// interprets it.
    #[serde(deserialize_with = "Option::deserialize")]
    pub container: Option<NodeRef>,
}

/// One node in a view.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeRow {
    /// Its identity.
    pub id: NodeRef,
    /// `null` when an edge resolved to this identity and no provider ever
    /// emitted a symbol for it.
    #[serde(deserialize_with = "Option::deserialize")]
    pub symbol: Option<SymbolRow>,
    /// The outermost grouping box. `null` exactly when `symbol` is `null`.
    #[serde(deserialize_with = "Option::deserialize")]
    pub unit: Option<String>,
    /// `null` when the node was never indexed, or when its plugin registered no
    /// classifier.
    #[serde(deserialize_with = "Option::deserialize")]
    pub category: Option<CategoryRow>,
    /// Per shard. `null` in the index-wide view.
    #[serde(deserialize_with = "Option::deserialize")]
    pub depth: Option<u32>,
    /// True when the walk stopped here at the depth limit.
    pub frontier: bool,
}

/// What produced an edge.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceRow {
    /// The plugin that emitted it.
    pub plugin: String,
    /// The engine and its version.
    pub engine: String,
}

/// Where an edge points.
///
/// A tagged union. There is no field shape in which an unresolved edge can be
/// mistaken for a resolved one, and no field that holds a "best" candidate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum EdgeTargetRow {
    /// The plugin resolved the call to one definition.
    Resolved {
        /// The callee.
        node: NodeRef,
    },
    /// The plugin could not resolve the call and says so.
    Unresolved {
        /// The name at the call site.
        name: String,
        /// Every definition considered. May be empty.
        candidates: Vec<NodeRef>,
    },
}

/// One call edge.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EdgeRow {
    /// The calling definition.
    pub from: NodeRef,
    /// The callee, resolved or not.
    pub to: EdgeTargetRow,
    /// Where the call is written.
    #[serde(deserialize_with = "Option::deserialize")]
    pub call_site: Option<RangeRow>,
    /// ADR-0003 field 4, first half. Required.
    pub provenance: ProvenanceRow,
    /// ADR-0003 field 4, second half. Required.
    pub inference_mode: InferenceModeRow,
}

/// Whether a root bound to a handler.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum BindingRow {
    /// Bound to this definition.
    Bound {
        /// The handler.
        node: NodeRef,
    },
    /// No handler was found, and here is why.
    Unbound {
        /// The provider's own words.
        reason: String,
    },
}

/// One `(contract, version)` pair.
///
/// An object rather than plan-01 §8.4's two-element array, for the reason
/// `reachgraph-fixture` already recorded on its own side: a positional pair
/// cannot carry `deny_unknown_fields`, and it cannot tell a `null` version from
/// an omitted one — the exact distinction ADR-0007 turns on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionKeyRow {
    /// The contract.
    pub contract: String,
    /// Its version. Required key, nullable value. Never defaulted to `"v1"`.
    #[serde(deserialize_with = "Option::deserialize")]
    pub version: Option<String>,
}

/// One root that bound to nothing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnboundRootRow {
    /// The contract.
    pub contract: String,
    /// Its version.
    #[serde(deserialize_with = "Option::deserialize")]
    pub version: Option<String>,
    /// The service.
    pub service: String,
    /// The operation.
    pub operation: String,
    /// Served or consumed.
    pub direction: DirectionRow,
    /// Why nothing bound.
    pub reason: String,
}

/// What the index covered — ADR-0007 requirement 1, in the artifact rather
/// than in a log line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoverageRow {
    /// Every contract examined.
    pub contracts: Vec<String>,
    /// Every version key examined. A null version is a real entry.
    pub versions: Vec<VersionKeyRow>,
    /// How many roots the index holds.
    pub roots_total: usize,
    /// How many of them bound.
    pub roots_bound: usize,
    /// The rest, with their reasons.
    pub unbound_roots: Vec<UnboundRootRow>,
    /// Every unit enumerated.
    pub units_indexed: Vec<String>,
    /// Every contributing plugin.
    pub plugins: Vec<String>,
    /// Categories at which the walk stopped — a declared limitation.
    pub traversal_terminal_categories: Vec<CategoryRow>,
    /// True when a provider failed and the run continued.
    pub partial: bool,
}

/// One version of one operation in the endpoint list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationVersionRow {
    /// `null` is emitted explicitly; the key is always present.
    #[serde(deserialize_with = "Option::deserialize")]
    pub version: Option<String>,
    /// The plugin-spelled cross-repository key. Stored, never parsed.
    pub join_key: String,
    /// Bound or not.
    pub binding: BindingRow,
    /// The shard file, or `null` for an unbound root.
    #[serde(deserialize_with = "Option::deserialize")]
    pub shard: Option<String>,
    /// How many nodes that shard holds.
    pub node_count: usize,
    /// How many of them are frontier.
    pub frontier_count: usize,
}

/// One operation, with its versions as siblings — ADR-0007 Consequences.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationRow {
    /// The contract.
    pub contract: String,
    /// The service.
    pub service: String,
    /// The operation.
    pub operation: String,
    /// Served or consumed.
    pub direction: DirectionRow,
    /// Every version of it, in index order.
    pub versions: Vec<OperationVersionRow>,
}

/// `endpoints.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndpointsDocument {
    /// The schema version.
    pub schema_version: u32,
    /// What produced it.
    pub generated_by: GeneratedBy,
    /// What each plugin declared.
    pub plugins: Vec<PluginRow>,
    /// The root list, grouped by operation.
    pub operations: Vec<OperationRow>,
    /// What the index covered.
    pub coverage: CoverageRow,
}

/// The root a shard was computed from, with its identity spelled out.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShardRootRow {
    /// The contract.
    pub contract: String,
    /// Its version.
    #[serde(deserialize_with = "Option::deserialize")]
    pub version: Option<String>,
    /// The service.
    pub service: String,
    /// The operation.
    pub operation: String,
    /// Served or consumed.
    pub direction: DirectionRow,
    /// The plugin-spelled key.
    pub join_key: String,
    /// Bound or not.
    pub binding: BindingRow,
}

/// Shard totals.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatsRow {
    /// Nodes in this shard.
    pub node_count: usize,
    /// Edges in this shard.
    pub edge_count: usize,
    /// How many of those edges are unresolved.
    pub unresolved_edge_count: usize,
}

/// `graph/<slug>.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShardDocument {
    /// The schema version.
    pub schema_version: u32,
    /// Full root identity. The file name is a name, not an identity.
    pub root: ShardRootRow,
    /// The depth the walk stopped at.
    #[serde(deserialize_with = "Option::deserialize")]
    pub depth_limit: Option<u32>,
    /// What each plugin declared.
    pub plugins: Vec<PluginRow>,
    /// The reachable subgraph's nodes.
    pub nodes: Vec<NodeRow>,
    /// Its edges, unresolved ones included.
    pub edges: Vec<EdgeRow>,
    /// Nodes the walk stopped at. Frontier, not leaf.
    pub frontier: Vec<NodeRef>,
    /// Totals.
    pub stats: StatsRow,
}

/// One node in the complement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnreachableRow {
    /// Its identity.
    pub id: NodeRef,
    /// Its bare name.
    pub name: String,
    /// The file it is declared in.
    pub file: PathBuf,
    /// `null` counts as unclassified; it is never dropped.
    #[serde(deserialize_with = "Option::deserialize")]
    pub category: Option<CategoryRow>,
    /// Whether the provider marked it test code.
    pub is_test: bool,
    /// An annotation, never a filter.
    pub possibly_reachable_via_unresolved: bool,
}

/// How many unreached nodes fell in each category.
///
/// A named struct rather than a map: the six counts are the whole set, and a
/// map would let one go missing rather than read zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CategoryCountsRow {
    /// Code this repository owns.
    pub first_party: usize,
    /// Code produced by a build step.
    pub generated: usize,
    /// Another member of the same workspace.
    pub workspace_sibling: usize,
    /// A dependency from outside the workspace.
    pub third_party: usize,
    /// The language's own standard library.
    pub stdlib: usize,
    /// Nodes whose plugin registered no classifier.
    pub unclassified: usize,
}

/// `unreachable.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnreachableDocument {
    /// The schema version.
    pub schema_version: u32,
    /// The wording rule, shipped as data.
    pub claim: String,
    /// What the claim was computed against.
    pub coverage: CoverageRow,
    /// Every indexed node no bound root reached.
    pub nodes: Vec<UnreachableRow>,
    /// A summary for a consumer that wants one.
    pub counts_by_category: CategoryCountsRow,
    /// How many unresolved edges the index holds.
    pub unresolved_edge_count: usize,
}

/// One node's per-version reach.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionNodeRow {
    /// Its identity.
    pub id: NodeRef,
    /// Indices into `version_keys`. Always authoritative.
    pub reached_by: Vec<usize>,
    /// ADR-0007's three-way label, or `null` where a two-valued label would be
    /// a lie.
    #[serde(deserialize_with = "Option::deserialize")]
    pub class: Option<String>,
}

/// The sunset summary for one contract with exactly two versions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractSummaryRow {
    /// Reached only by the first version. What dies at its sunset.
    pub v1_only: usize,
    /// Reached only by the second. The new path.
    pub v2_only: usize,
    /// Reached by both. Shared.
    pub both: usize,
}

/// `versions.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionsDocument {
    /// The schema version.
    pub schema_version: u32,
    /// Every version key, in index order.
    pub version_keys: Vec<VersionKeyRow>,
    /// Every indexed node and what reaches it.
    pub nodes: Vec<VersionNodeRow>,
    /// Per contract, where a two-valued label is honest.
    pub summary_by_contract: BTreeMap<String, ContractSummaryRow>,
}
