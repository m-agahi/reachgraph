//! Per-version reachability and the three-way classification — plan-01 §6,
//! ADR-0007.

use reachgraph_plugin_api::VersionKey;

use crate::intern::NodeIdx;

/// A set of version-key indices, one bit each.
///
/// A named type rather than a bare `Vec<u64>`: the bit positions index
/// [`VersionReach::keys`], and a raw word vector says nothing about that.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct VersionBits {
    words: Vec<u64>,
}

impl VersionBits {
    fn set(&mut self, bit: usize) {
        let word = bit / 64;
        if self.words.len() <= word {
            self.words.resize(word + 1, 0);
        }
        self.words[word] |= 1u64 << (bit % 64);
    }

    fn get(&self, bit: usize) -> bool {
        self.words
            .get(bit / 64)
            .is_some_and(|word| word & (1u64 << (bit % 64)) != 0)
    }
}

/// Which version keys reach each node.
///
/// The bitset is the representation and ADR-0007's three-way table is the
/// presentation. That is what lets three versions generalise without a special
/// case: with three or more keys `reached_by` is the truth, and a two-valued
/// label would be a lie.
#[derive(Clone, Debug, Default)]
pub(crate) struct VersionReach {
    keys: Vec<VersionKey>,
    per_node: Vec<VersionBits>,
}

impl VersionReach {
    pub(crate) fn new(keys: Vec<VersionKey>, node_count: usize) -> Self {
        Self {
            keys,
            per_node: vec![VersionBits::default(); node_count],
        }
    }

    /// Record that the key at `key_index` reaches this node.
    pub(crate) fn mark(&mut self, node: NodeIdx, key_index: usize) {
        self.per_node[node.position()].set(key_index);
    }

    pub(crate) fn keys(&self) -> &[VersionKey] {
        &self.keys
    }

    /// The key indices that reach this node, ascending.
    pub(crate) fn reached_by(&self, node: NodeIdx) -> Vec<usize> {
        let bits = &self.per_node[node.position()];
        (0..self.keys.len()).filter(|bit| bits.get(*bit)).collect()
    }

    /// Every key index belonging to one contract, in key order.
    pub(crate) fn keys_of_contract(
        &self,
        contract: &reachgraph_plugin_api::ContractId,
    ) -> Vec<usize> {
        self.keys
            .iter()
            .enumerate()
            .filter(|(_, key)| key.contract == *contract)
            .map(|(index, _)| index)
            .collect()
    }
}

/// ADR-0007's three-way table, as a value.
///
/// The two names are **positional within the contract's two version keys**,
/// not a claim that the versions are spelled `v1` and `v2`. ADR-0007's table
/// is written for the `v1`/`v2` instance, which is the one this names.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum VersionClass {
    /// Reached only by the contract's first version key. Dies at that sunset.
    FirstOnly,
    /// Reached only by the second. The new path.
    SecondOnly,
    /// Reached by both. Shared, and it survives the sunset.
    Both,
}

impl VersionClass {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            VersionClass::FirstOnly => "v1_only",
            VersionClass::SecondOnly => "v2_only",
            VersionClass::Both => "both",
        }
    }
}

/// The class of one node, or `None` where a two-valued label would be a lie.
///
/// Emitted only when every key reaching the node belongs to one contract *and*
/// that contract has exactly two version keys. With three or more, or with a
/// node reached across two contracts, `reached_by` is the truth.
pub(crate) fn class_of(reach: &VersionReach, node: NodeIdx) -> Option<VersionClass> {
    let reached = reach.reached_by(node);
    if reached.is_empty() {
        return None;
    }

    let keys = reach.keys();
    let contract = &keys[reached[0]].contract;
    if reached
        .iter()
        .any(|index| keys[*index].contract != *contract)
    {
        return None;
    }

    let pair = reach.keys_of_contract(contract);
    if pair.len() != 2 {
        return None;
    }

    let first = reached.contains(&pair[0]);
    let second = reached.contains(&pair[1]);
    match (first, second) {
        (true, true) => Some(VersionClass::Both),
        (true, false) => Some(VersionClass::FirstOnly),
        (false, true) => Some(VersionClass::SecondOnly),
        (false, false) => None,
    }
}
