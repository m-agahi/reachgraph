//! `structure.json` — the compound hierarchy and the dispatch classification.
//!
//! Plan-05 §4.4.1: *the compound hierarchy is computed here, in Rust, and
//! emitted*. §9.2 divides the labour exactly — the waist follows containment
//! links by **equality** and never interprets them; deciding that an ancestor
//! is a module rather than a type is this crate's job, from `kind` and
//! `raw_kind`. The JavaScript receives a finished parent link and interprets
//! nothing (§1).
//!
//! # Why a separate file instead of a `boxes` array inside each shard
//!
//! Plan-05 §4.4 puts `boxes` inside `graph/<slug>.json`. That shard is the
//! **waist's** document now (ADR-0727 puts the schema in `reachgraph-core`,
//! and plan-05 §8.6 keeps `reachgraph-core` out of this crate's dependency
//! graph), so this crate cannot add a field to it — and re-serialising the
//! shard to insert one would give the artifact two writers for one file.
//!
//! The index-wide file is also the more correct shape, which is what makes
//! this a better answer rather than a workaround. MEASURED on
//! `/home/max/git/yadgarhq/task`: a handler's `impl` block and its enclosing
//! module are **not in the shard** that contains the handler — they are
//! indexed symbols no root reaches, so they sit in `unreachable.json`. A
//! per-shard `boxes` array would therefore have to be computed from data the
//! shard does not hold, and the same box would be re-serialised into every
//! shard that touches it. Containment is a property of a symbol, not of a
//! root's reachable set, and one index-wide file says that.
//!
//! # What is absent and why
//!
//! A node with no symbol gets `box: null` and is drawn **outside** every box.
//! Plan-05 §4.4.1: an external node was never indexed, so it belongs to no
//! unit, and a synthetic "unknown" box would invent a grouping the index does
//! not have.

use std::collections::HashMap;

use reachgraph_plugin_api::{GraphView, Node, NodeId, Symbol, SymbolKind};
use serde::Serialize;

use crate::dispatch::{self, Dispatch};

/// The schema version `structure.json` carries.
///
/// Its own number rather than the waist's: this file is a different document
/// with a different owner, and sharing a version would make a bump in either
/// place look like a bump in both.
pub const STRUCTURE_SCHEMA_VERSION: u32 = 1;

/// What kind of grouping a box is.
///
/// Derived from the neutral [`SymbolKind`] alone, so it holds for any plugin.
/// `raw_kind` travels alongside for display and is never matched on here —
/// the one place this crate matches on `raw_kind` is [`crate::dispatch`], and
/// it says whose vocabulary it is reading.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BoxKind {
    /// The outermost box: one per `UnitId` (plan-05 §9.2).
    Unit,
    /// A namespace-like grouping — [`SymbolKind::Module`].
    Module,
    /// A type — [`SymbolKind::Type`].
    Type,
}

fn box_kind_of(symbol: &Symbol) -> Option<BoxKind> {
    match symbol.kind {
        SymbolKind::Module => Some(BoxKind::Module),
        SymbolKind::Type => Some(BoxKind::Type),
        // A function, a method, a field — and an `impl` block, which
        // `reachgraph-lang-rust` reports as `SymbolKind::Other`. None of them
        // is a grouping. The `impl` block is still read, by
        // `crate::dispatch`, for what it says about the method inside it.
        SymbolKind::Function | SymbolKind::Method | SymbolKind::Field | SymbolKind::Other => None,
    }
}

/// One compound box. Cytoscape draws these as parents of the nodes inside
/// them.
#[derive(Clone, Debug, Serialize)]
pub struct BoxRow {
    /// An ordinal, assigned here. **Not derived from any identity**: building
    /// a readable id out of a `NodeId` would be parsing one (ADR-0003 field
    /// 3), and building one out of a `UnitId` would invite the page to split
    /// it.
    pub id: String,
    /// What to show on the box.
    pub label: String,
    /// The enclosing box, or `null` for an outermost one.
    pub parent: Option<String>,
    /// Unit, module or type.
    pub kind: BoxKind,
    /// The emitting plugin's own term, verbatim, for display. `null` for a
    /// unit box, which is not a symbol.
    pub raw_kind: Option<String>,
}

/// One node's place in the hierarchy, and what its container says about it.
#[derive(Clone, Debug, Serialize)]
pub struct NodeStructureRow {
    /// The plugin half of the node's identity.
    pub plugin: String,
    /// The opaque half. Emitted byte for byte; never split.
    pub raw: String,
    /// The innermost box holding this node, or `null` when it is in none —
    /// an external node, which was never indexed and belongs to no unit.
    #[serde(rename = "box")]
    pub box_id: Option<String>,
    /// The innermost container's `raw_kind`, verbatim.
    ///
    /// Language-neutral and always emitted when there is a container: the
    /// page shows it, so a reader of a plugin this build has never seen still
    /// learns what encloses a node. [`NodeStructureRow::dispatch`] is the
    /// extra that one plugin's vocabulary buys on top of it.
    pub container_raw_kind: Option<String>,
    /// ADR-0729. `null` means *this build claims nothing*, which is different
    /// from `implementation`.
    pub dispatch: Option<String>,
}

/// `structure.json`.
#[derive(Clone, Debug, Serialize)]
pub struct StructureDocument {
    /// The schema version.
    pub schema_version: u32,
    /// Which renderer computed it.
    pub generated_by: String,
    /// Every box, outermost-first within each chain.
    pub boxes: Vec<BoxRow>,
    /// Every node in the index-wide view, in view order.
    pub nodes: Vec<NodeStructureRow>,
    /// The sentence the page shows against a trait-declaration badge, shipped
    /// as data for the reason the waist ships its unreachability claim as data
    /// — so the page cannot re-word it.
    pub trait_declaration_note: String,
}

/// Assigns ordinal ids and remembers what already has one.
#[derive(Default)]
struct Boxes {
    rows: Vec<BoxRow>,
    by_unit: HashMap<String, String>,
    by_node: HashMap<NodeId, String>,
}

impl Boxes {
    fn next_id(&self) -> String {
        format!("b{}", self.rows.len())
    }

    fn unit(&mut self, unit: &str) -> String {
        if let Some(existing) = self.by_unit.get(unit) {
            return existing.clone();
        }
        let id = self.next_id();
        self.rows.push(BoxRow {
            id: id.clone(),
            // Verbatim. A `UnitId` is the plugin's spelling and this crate
            // neither splits nor prettifies it — the same rule plan-05 §4.4.3
            // states for an unindexed node's label.
            label: unit.to_owned(),
            parent: None,
            kind: BoxKind::Unit,
            raw_kind: None,
        });
        self.by_unit.insert(unit.to_owned(), id.clone());
        id
    }

    fn symbol(
        &mut self,
        node: &Node,
        symbol: &Symbol,
        kind: BoxKind,
        parent: Option<String>,
    ) -> String {
        if let Some(existing) = self.by_node.get(&node.id) {
            return existing.clone();
        }
        let id = self.next_id();
        self.rows.push(BoxRow {
            id: id.clone(),
            label: symbol.name.clone(),
            parent,
            kind,
            raw_kind: Some(symbol.raw_kind.clone()),
        });
        self.by_node.insert(node.id.clone(), id.clone());
        id
    }
}

/// The innermost container of `node`, if this view holds it.
fn container<'a>(view: &'a GraphView, node: &Node) -> Option<(&'a Node, &'a Symbol)> {
    let parent = view.container_of(&node.id)?;
    let symbol = parent.symbol.as_ref()?;
    Some((parent, symbol))
}

/// Build the box tree and each node's place in it.
///
/// One pass over the view, following `container_chain` per node. The chain is
/// cycle-guarded by the waist (`GraphView::container_chain`), so this crate
/// does not re-guard it — plan-05 §4.4.1 is explicit that it must not.
pub fn structure_of(view: &GraphView) -> StructureDocument {
    let mut boxes = Boxes::default();
    let mut nodes = Vec::with_capacity(view.nodes.len());

    for node in &view.nodes {
        let Some(symbol) = node.symbol.as_ref() else {
            // Plan-05 §4.4.1: an external node is drawn, outside every box.
            nodes.push(NodeStructureRow {
                plugin: node.id.plugin.0.to_owned(),
                raw: node.id.raw.clone(),
                box_id: None,
                container_raw_kind: None,
                dispatch: None,
            });
            continue;
        };

        let unit_box = node.unit.as_ref().map(|unit| boxes.unit(&unit.0));

        // Nearest first. Keep only the ancestors that are groupings; an
        // `impl` block is skipped here and read below.
        let chain: Vec<&Node> = view.container_chain(&node.id);
        let grouping: Vec<(&Node, &Symbol, BoxKind)> = chain
            .iter()
            .filter_map(|ancestor| {
                let ancestor_symbol = ancestor.symbol.as_ref()?;
                let kind = box_kind_of(ancestor_symbol)?;
                Some((*ancestor, ancestor_symbol, kind))
            })
            .collect();

        // Outermost first, so each box's parent already exists when it is
        // registered.
        let mut parent = unit_box.clone();
        for (ancestor, ancestor_symbol, kind) in grouping.iter().rev() {
            parent = Some(boxes.symbol(ancestor, ancestor_symbol, *kind, parent));
        }

        let (container_raw_kind, dispatch) = match container(view, node) {
            Some((_, container_symbol)) => (
                Some(container_symbol.raw_kind.clone()),
                // ADR-0729 is about a call target's body, so the question is
                // only meaningful for something that has one bound to a type.
                match symbol.kind {
                    SymbolKind::Method => dispatch::classify(
                        node.id.plugin,
                        container_symbol.kind,
                        &container_symbol.raw_kind,
                    ),
                    _ => None,
                },
            ),
            None => (None, None),
        };

        nodes.push(NodeStructureRow {
            plugin: node.id.plugin.0.to_owned(),
            raw: node.id.raw.clone(),
            box_id: parent,
            container_raw_kind,
            dispatch: dispatch.map(Dispatch::as_str).map(str::to_owned),
        });
    }

    StructureDocument {
        schema_version: STRUCTURE_SCHEMA_VERSION,
        generated_by: crate::RENDERER_ID.to_owned(),
        boxes: boxes.rows,
        nodes,
        trait_declaration_note: dispatch::TRAIT_DECLARATION_NOTE.to_owned(),
    }
}
