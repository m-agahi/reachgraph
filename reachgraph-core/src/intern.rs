//! Dense indices over opaque node identities — plan-01 §4.1.
//!
//! The core needs integer indices for traversal and must gain no knowledge of
//! `NodeId::raw` in the process. Four operations happen here and nowhere else:
//! hash, compare for equality, clone, emit. Nothing splits an identity,
//! prefix-matches one, or orders one by content.

use std::collections::HashMap;

use reachgraph_plugin_api::NodeId;

/// Dense index into the graph's node table.
///
/// Internal to this crate: never serialized, never in `plugin-api`. Indices are
/// assignment order, which is provider iteration order, so serializing one
/// would leak a build detail into the artifact.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub(crate) struct NodeIdx(u32);

impl NodeIdx {
    /// This index as a position into a parallel table.
    pub(crate) fn position(self) -> usize {
        self.0 as usize
    }
}

/// The identity table.
#[derive(Debug, Default)]
pub(crate) struct Interner {
    by_id: HashMap<NodeId, NodeIdx>,
    ids: Vec<NodeId>,
}

impl Interner {
    /// The index for this identity, assigning one if it is new.
    pub(crate) fn intern(&mut self, id: &NodeId) -> NodeIdx {
        if let Some(existing) = self.by_id.get(id) {
            return *existing;
        }

        let assigned = NodeIdx(u32::try_from(self.ids.len()).unwrap_or_else(|_| {
            // A graph with more than u32::MAX nodes is not representable,
            // and neither is a machine that built one.
            panic!("more than u32::MAX nodes were interned")
        }));
        self.ids.push(id.clone());
        self.by_id.insert(id.clone(), assigned);
        assigned
    }

    /// The index for this identity, or `None` if it was never interned.
    pub(crate) fn lookup(&self, id: &NodeId) -> Option<NodeIdx> {
        self.by_id.get(id).copied()
    }

    /// The identity behind an index.
    pub(crate) fn id(&self, idx: NodeIdx) -> &NodeId {
        &self.ids[idx.position()]
    }

    /// How many identities are interned.
    pub(crate) fn len(&self) -> usize {
        self.ids.len()
    }

    /// Every index, in assignment order.
    pub(crate) fn indices(&self) -> impl Iterator<Item = NodeIdx> + '_ {
        (0..self.ids.len()).map(|position| NodeIdx(position as u32))
    }
}
