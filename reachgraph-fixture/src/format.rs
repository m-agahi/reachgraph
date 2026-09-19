//! The fixture JSON format — `docs/plans/02-fixture-plugin.md` §2.
//!
//! One document per case, at `<case-dir>/reachgraph.fixture.json`. The rules
//! below are the point of the format rather than its packaging, so they are
//! enforced by the types and asserted by `tests/format.rs` rather than left to
//! review:
//!
//! - `#[serde(deny_unknown_fields)]` on every struct, so a typo'd key is a
//!   parse error rather than a silently ignored field.
//! - **No `#[serde(default)]` on any field, anywhere.** Every default is a
//!   place this harness could invent data the case author never wrote.
//!   `docs/design.md` §5 MEASURED what that costs at the other end of a
//!   pipeline: `code_graph`'s `Function.docstring` keeps only the last line of
//!   a `///` block, so absence renders as if it were content.
//! - A root's version key is required and its value may be null. ADR-0007: a
//!   missing version is `None`, and `None` has to be an *assertion* rather than
//!   an absence.
//! - There is no offset, cursor, line or column anywhere in these types, and
//!   there never will be. That absence is why this crate exists (ADR-0008,
//!   plan-02 §4). A case states which file a symbol is in; where in the file is
//!   a fact the fixture does not have and does not pretend to.
//! - A file named here is a **label, not a claim about disk**. The case
//!   directory holds one JSON document and no source. Nothing may assert that
//!   one of these paths exists, and `preflight` does not look (plan-02 §3.1).
//!
//! These types mirror the contract's enums rather than deriving serde on them.
//! `reachgraph-plugin-api` has no dependencies at all, and plan-00 §1 gives
//! every plugin that crate and nothing else from the workspace — so a serde
//! derive there would be a serde dependency in every future plugin.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;

// ---------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------

/// A node reference as a case author writes it: the `raw` half of a
/// [`reachgraph_plugin_api::NodeId`], with the document's `plugin_id` supplying
/// the other half.
///
/// A case never writes a `{plugin, raw}` pair, so no case can accidentally mint
/// an id in another plugin's namespace. Cross-plugin namespacing is tested by
/// loading two documents, not by letting one document name two plugins.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
pub struct FixtureRaw(pub String);

/// A unit's identity within a document. Also the key of the `symbols` and
/// `edges` maps.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
pub struct FixtureUnitId(pub String);

// ---------------------------------------------------------------------------
// The document
// ---------------------------------------------------------------------------

/// One fixture case.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureDoc {
    /// Format version. Bumped on a breaking change to these types.
    pub fixture_version: u32,
    /// Becomes the [`reachgraph_plugin_api::PluginId`] every node in this
    /// document is minted under. A case may declare an id other than
    /// `"fixture"`.
    pub plugin_id: String,
    /// ADR-0003 field 2, declared per case.
    pub position_encoding: FixturePositionEncoding,
    /// ADR-0003 field 1. A case may declare a subset.
    pub capabilities: Vec<FixtureCapability>,
    /// Always empty in every case in the corpus, which
    /// `fixture_detection_is_always_empty` asserts. An empty marker list
    /// matches nothing (plan-00 §2), so the fixture is structurally
    /// undetectable and can only be chosen by name.
    pub detection: FixtureDetection,
    /// ADR-0003 field 5, as the case declares it.
    pub preflight: FixturePreflight,
    /// Names the engine and its version. Carried into every
    /// [`reachgraph_plugin_api::Provenance`].
    pub engine: String,
    /// The units of analysis this case declares. ADR-0008 leak 4: no manifest,
    /// no workspace, no build state.
    pub units: Vec<FixtureUnit>,
    /// Symbols per unit. A unit the document declares but this map omits has no
    /// symbols; a key naming no declared unit is a case-authoring mistake that
    /// `every_symbol_and_edge_key_names_a_declared_unit` catches.
    pub symbols: BTreeMap<FixtureUnitId, Vec<FixtureSymbol>>,
    /// Edges per unit, keyed exactly as `symbols` is.
    pub edges: BTreeMap<FixtureUnitId, Vec<FixtureEdge>>,
    /// ADR-0003 field 6: the entry points into the graph.
    pub roots: Vec<FixtureRoot>,
    /// ADR-0007: what the root provider looked at, so a partial root set cannot
    /// make live code read as unreachable.
    pub coverage: FixtureCoverage,
    /// Path-prefix rules. ADR-0008 leak 8: prefixes are case data and are never
    /// compiled into this crate.
    pub classify: Vec<FixtureClassifyRule>,
    /// The category for a path no rule in `classify` matches.
    pub classify_fallback: FixtureCategory,
}

// ---------------------------------------------------------------------------
// Plugin-level declarations
// ---------------------------------------------------------------------------

/// Mirrors [`reachgraph_plugin_api::PositionEncoding`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixturePositionEncoding {
    /// `ra_ap`, tree-sitter, SCIP.
    Utf8Bytes,
    /// LSP.
    Utf16CodeUnits,
    /// Unicode scalar values.
    Utf32CodePoints,
}

/// Mirrors [`reachgraph_plugin_api::Capability`]. There is no `render`
/// variant, because a renderer is not a plugin (plan-00 §3.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixtureCapability {
    /// Emits symbols.
    Symbols,
    /// Emits edges.
    Edges,
    /// Emits roots.
    Roots,
    /// Answers `classify`.
    Classify,
}

/// Mirrors [`reachgraph_plugin_api::Detection`].
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureDetection {
    /// File names whose presence would claim a repository. Empty in every
    /// corpus case, by construction.
    pub marker_files: Vec<String>,
    /// Extensions this plugin would analyse. Empty in every corpus case.
    pub extensions: Vec<String>,
}

/// Mirrors [`reachgraph_plugin_api::Preflight`], minus one variant.
///
/// **There is still no `warned` spelling, and the reason has changed.** The
/// variant was added to the contract on 2026-09-19, after plan-02 §2.2 was
/// written, and this comment used to say the spelling was withheld because
/// plan-00 §8 question 7 — whether `Warned` also carries a `reason` — was open,
/// so a fixture encoding invented here would have answered it by accident.
///
/// **Question 7 is now DECIDED** (2026-09-19, while `reachgraph-lang-rust` was
/// written): `Warned` carries both. The withholding survives its original
/// reason because a second one was always underneath it — plan-02 §2.2 fixes
/// this format, and widening it is that plan's decision to make rather than a
/// side effect of plan-03 needing somewhere to put a variant. The corpus is
/// ADR-0008's mechanical n=2 and a case that returned `Warned` would be worth
/// having; it is a plan-02 change, and it is named here so the next reader
/// finds a decision rather than an oversight.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixturePreflight {
    /// Every prerequisite met.
    Ok,
    /// A prerequisite is not met and the plugin must not run.
    Failed(FixturePreflightFailure),
}

/// Why a case declares that it cannot run, and what to do about it.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixturePreflightFailure {
    /// What was checked and what was found.
    pub reason: String,
    /// What the user should do about it.
    pub remediation: String,
}

// ---------------------------------------------------------------------------
// Units and symbols
// ---------------------------------------------------------------------------

/// One unit of analysis.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureUnit {
    /// Identity within this document.
    pub id: FixtureUnitId,
    /// What to show a reader.
    pub display_name: String,
    /// The directory this unit is rooted at. A label, like every other path
    /// here.
    pub root: PathBuf,
}

/// Mirrors [`reachgraph_plugin_api::SymbolKind`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixtureSymbolKind {
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

/// Mirrors [`reachgraph_plugin_api::DocFormat`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixtureDocFormat {
    /// Render verbatim.
    Plain,
    /// CommonMark.
    Markdown,
}

/// One symbol as a case declares it.
///
/// Every key is required. `container` and `is_test` in particular: plan-00 §8
/// question 3 is that a plugin author must not be able to forget the enclosing
/// definition, and a serde default is exactly the mechanism that would let
/// them.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureSymbol {
    /// This symbol's id within the document's plugin namespace.
    pub raw: FixtureRaw,
    /// The bare name, as a case author writes it.
    pub name: String,
    /// The neutral kind the waist may reason about.
    pub kind: FixtureSymbolKind,
    /// The case's own term for the kind. Display only — the waist never matches
    /// on it, which is what the `foreign_shapes` case exists to prove.
    pub raw_kind: String,
    /// Which file the symbol is in. **A label, not a claim about disk.** It is
    /// here to be classified by prefix, carried into the artifact and
    /// displayed. Nothing asserts that it exists.
    pub file: PathBuf,
    /// Documentation text, if the case declares any. Required key, nullable
    /// value — see [`FixtureRoot::version`] for why the annotation is needed.
    #[serde(deserialize_with = "Option::deserialize")]
    pub doc: Option<String>,
    /// How to render `doc`.
    pub doc_format: FixtureDocFormat,
    /// Whether this symbol is test code.
    pub is_test: bool,
    /// The enclosing definition, or `null`. **Required either way** — plan-00
    /// §8 question 3 is that a plugin author must not be able to forget it.
    #[serde(deserialize_with = "Option::deserialize")]
    pub container: Option<FixtureRaw>,
}

// ---------------------------------------------------------------------------
// Edges
// ---------------------------------------------------------------------------

/// Mirrors [`reachgraph_plugin_api::InferenceMode`]. ADR-0003 field 4.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixtureInferenceMode {
    /// A semantic engine answered directly.
    Resolved,
    /// Scope and binding resolution only.
    Lexical,
    /// Required type inference to pick the target.
    TypeInferred,
    /// Derived from which definition encloses a reference.
    Enclosure,
}

/// Mirrors [`reachgraph_plugin_api::EdgeTarget`].
///
/// There is no "best guess" spelling. `docs/design.md` §8: show a missing edge
/// as missing, never infer one to fill a hole.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixtureEdgeTarget {
    /// The case resolved the call to one definition.
    Resolved(FixtureRaw),
    /// The case could not resolve the call and says so.
    Unresolved(FixtureUnresolvedTarget),
}

/// An unresolved call: the name written at the call site, and every definition
/// that was considered.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureUnresolvedTarget {
    /// The name at the call site.
    pub name: String,
    /// Every definition considered. May be empty.
    pub candidates: Vec<FixtureRaw>,
}

/// One call edge as a case declares it.
///
/// `provenance_plugin` and `inference_mode` are both required: ADR-0003 field 4
/// makes an edge without them unrepresentable rather than merely discouraged.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureEdge {
    /// The calling definition.
    pub from: FixtureRaw,
    /// The callee, resolved or not.
    pub to: FixtureEdgeTarget,
    /// The plugin this edge is attributed to.
    pub provenance_plugin: String,
    /// The engine and version that produced it.
    pub engine: String,
    /// How strong the claim is.
    pub inference_mode: FixtureInferenceMode,
}

// ---------------------------------------------------------------------------
// Roots
// ---------------------------------------------------------------------------

/// Mirrors [`reachgraph_plugin_api::Direction`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixtureDirection {
    /// This repository implements the operation.
    Served,
    /// This repository calls it.
    Consumed,
}

/// Mirrors [`reachgraph_plugin_api::RootBinding`].
///
/// There is no confidence number, here or anywhere else in these types
/// (plan-00 §8 question 5). `docs/design.md` §5 MEASURED what one invites:
/// `code_graph` emits 59 edges at confidence 0.55, each with two or three
/// candidates — indecision recorded as if it were a measurement.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixtureRootBinding {
    /// Bound to this definition.
    Bound(FixtureRaw),
    /// No handler was found, and here is why.
    Unbound(FixtureUnboundRoot),
}

/// Why a root has no handler. Carried into the artifact, never dropped.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureUnboundRoot {
    /// Free text the case author writes, for the report.
    pub reason: String,
}

/// One contract operation.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureRoot {
    /// The contract this operation belongs to.
    pub contract: String,
    /// ADR-0007. **The key is required; `null` is the only way to say
    /// unversioned.** A document that omits it fails to parse rather than
    /// deserialising to `None`, which is what keeps `None` an assertion the
    /// case author made rather than an absence this crate filled in.
    ///
    /// The annotation is load-bearing and was MEASURED here on 2026-09-19,
    /// after `missing_version_key_is_a_parse_error` went green against a
    /// document with no `version` key at all. **Writing no `#[serde(default)]`
    /// is not enough.** For an `Option` field serde's derive routes a missing
    /// key through `serde::__private::de::missing_field`, whose deserializer
    /// answers `deserialize_option` with `visit_none` — so every bare
    /// `Option<T>` field is optional whether or not anyone asked for it, and
    /// `deny_unknown_fields` does not touch it. A field carrying
    /// `deserialize_with` takes the other branch and reports the missing key.
    ///
    /// This is the exact failure plan-02 §2.1's no-defaults rule exists to
    /// prevent, arriving from a direction the rule as written did not cover:
    /// the default was serde's, not the author's.
    #[serde(deserialize_with = "Option::deserialize")]
    pub version: Option<String>,
    /// The service, as the case spells it.
    pub service: String,
    /// The operation, as the case spells it.
    pub operation: String,
    /// Served or consumed.
    pub direction: FixtureDirection,
    /// ADR-0007's cross-repository key. Required, and emitted verbatim: this
    /// crate never parses it, never splits it, and never recovers `version`
    /// from it.
    pub join_key: String,
    /// Bound to a handler, or reported unbound with a reason.
    pub binding: FixtureRootBinding,
}

/// One `(contract, version)` pair the provider looked at.
///
/// An object rather than plan-02 §2.2's two-element array. A positional pair
/// cannot carry `deny_unknown_fields`, and it cannot tell a `null` version from
/// an omitted one — which is the exact distinction ADR-0007 turns on, so the
/// one place it must not be blurred is the coverage record.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureCoverageVersion {
    /// The contract.
    pub contract: String,
    /// Its version. Required key, nullable value, exactly as on a root, and
    /// required by the same annotation for the same reason.
    #[serde(deserialize_with = "Option::deserialize")]
    pub version: Option<String>,
}

/// ADR-0007: what the provider actually looked at.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureCoverage {
    /// Every contract examined.
    pub contracts: Vec<String>,
    /// Every `(contract, version)` pair examined. A null version is a real
    /// entry rather than a gap.
    pub versions: Vec<FixtureCoverageVersion>,
}

// ---------------------------------------------------------------------------
// Classification
// ---------------------------------------------------------------------------

/// Mirrors [`reachgraph_plugin_api::Category`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixtureCategory {
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

/// One classification rule: a path prefix and the category it yields.
///
/// The prefix is a `String` rather than a `PathBuf` on purpose. `"src/"` and
/// `"src"` are different prefixes and the same path, so storing it as a path
/// would silently merge two rules a case author wrote as one each.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureClassifyRule {
    /// The prefix to match against a path, as written.
    pub prefix: String,
    /// What a match yields.
    pub category: FixtureCategory,
}
