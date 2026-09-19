//! Node and edge storage, and the adjacency the traversal walks — plan-01 §2.

use reachgraph_plugin_api::{Category, Edge, EdgeTarget, NodeId, Symbol, UnitId};

use crate::intern::{Interner, NodeIdx};

/// One node as the waist holds it.
#[derive(Debug)]
pub(crate) struct NodeRecord {
    /// `None` for an external node: an edge resolved to this identity and no
    /// provider ever emitted a symbol for it (plan-01 §4.3).
    pub(crate) symbol: Option<Symbol>,
    /// The unit the symbol came from. `None` exactly when `symbol` is `None`.
    pub(crate) unit: Option<UnitId>,
    /// Filled by classification (plan-01 §7).
    pub(crate) category: Option<Category>,
}

impl NodeRecord {
    pub(crate) fn indexed(&self) -> bool {
        self.symbol.is_some()
    }
}

/// Where an edge lands, once its identities are interned.
///
/// An unresolved edge lands nowhere. Its candidates are carried so the
/// annotation of plan-01 §5.3 can find them, and following one would be the
/// inference `EdgeTarget` exists to forbid.
#[derive(Debug)]
pub(crate) enum EdgeEnd {
    /// The provider resolved the call, and the target is interned.
    Resolved(NodeIdx),
    /// The provider did not resolve the call. Each candidate is interned only
    /// if some provider emitted a symbol for it.
    Unresolved(Vec<Option<NodeIdx>>),
}

/// One edge as the waist holds it: the interned ends, and the plugin's own
/// value carried verbatim for emission (plan-01 §9).
#[derive(Debug)]
pub(crate) struct EdgeRecord {
    pub(crate) from: NodeIdx,
    pub(crate) to: EdgeEnd,
    pub(crate) edge: Edge,
}

/// The built graph.
#[derive(Debug, Default)]
pub(crate) struct Graph {
    interner: Interner,
    nodes: Vec<NodeRecord>,
    edges: Vec<EdgeRecord>,
    out: Vec<Vec<usize>>,
}

impl Graph {
    /// Intern an identity, creating an external record for it if it is new.
    pub(crate) fn intern(&mut self, id: &NodeId) -> NodeIdx {
        let idx = self.interner.intern(id);
        while self.nodes.len() < self.interner.len() {
            self.nodes.push(NodeRecord {
                symbol: None,
                unit: None,
                category: None,
            });
            self.out.push(Vec::new());
        }
        idx
    }

    /// Attach a symbol to an already-interned identity.
    ///
    /// Returns `false` when one is already attached — plan-01 §4.3 takes the
    /// first and diagnoses the duplicate, because merging would mean deciding
    /// which plugin's `doc` or `raw_kind` wins, and that is plugin knowledge.
    pub(crate) fn attach_symbol(&mut self, idx: NodeIdx, symbol: Symbol, unit: UnitId) -> bool {
        let record = &mut self.nodes[idx.position()];
        if record.symbol.is_some() {
            return false;
        }
        record.symbol = Some(symbol);
        record.unit = Some(unit);
        true
    }

    /// Record an edge, interning its ends.
    pub(crate) fn add_edge(&mut self, edge: Edge) {
        let from = self.intern(&edge.from);
        let to = match &edge.to {
            EdgeTarget::Resolved(target) => {
                let target = target.clone();
                EdgeEnd::Resolved(self.intern(&target))
            }
            EdgeTarget::Unresolved { candidates, .. } => EdgeEnd::Unresolved(
                candidates
                    .iter()
                    .map(|candidate| self.interner.lookup(candidate))
                    .collect(),
            ),
        };

        let position = self.edges.len();
        self.edges.push(EdgeRecord { from, to, edge });
        self.out[from.position()].push(position);
    }

    /// Re-resolve every unresolved candidate against the final identity table.
    ///
    /// Called once assembly is complete: a candidate may name a symbol emitted
    /// by a later unit than the edge itself, and an annotation that depended on
    /// collection order would be a different claim on every run.
    pub(crate) fn rebind_candidates(&mut self) {
        for record in &mut self.edges {
            let ids = match &record.edge.to {
                EdgeTarget::Unresolved { candidates, .. } => candidates,
                EdgeTarget::Resolved(_) => continue,
            };

            let rebound = ids
                .iter()
                .map(|id| {
                    self.interner
                        .lookup(id)
                        .filter(|idx| self.nodes[idx.position()].indexed())
                })
                .collect();

            record.to = EdgeEnd::Unresolved(rebound);
        }
    }

    pub(crate) fn node(&self, idx: NodeIdx) -> &NodeRecord {
        &self.nodes[idx.position()]
    }

    pub(crate) fn node_mut(&mut self, idx: NodeIdx) -> &mut NodeRecord {
        &mut self.nodes[idx.position()]
    }

    pub(crate) fn id(&self, idx: NodeIdx) -> &NodeId {
        self.interner.id(idx)
    }

    pub(crate) fn lookup(&self, id: &NodeId) -> Option<NodeIdx> {
        self.interner.lookup(id)
    }

    pub(crate) fn indices(&self) -> impl Iterator<Item = NodeIdx> + '_ {
        self.interner.indices()
    }

    pub(crate) fn edges(&self) -> &[EdgeRecord] {
        &self.edges
    }

    /// Every edge leaving this node, by position in [`Graph::edges`].
    pub(crate) fn out_edges(&self, idx: NodeIdx) -> &[usize] {
        &self.out[idx.position()]
    }
}
