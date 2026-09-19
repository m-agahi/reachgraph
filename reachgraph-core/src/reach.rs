//! Breadth-first reachability, the depth limit and the frontier — plan-01 §5.

use std::collections::{HashMap, HashSet, VecDeque};

use reachgraph_plugin_api::Category;

use crate::graph::{EdgeEnd, Graph};
use crate::intern::NodeIdx;

/// Which categories stop the walk — plan-01 §7.1.
///
/// Terminal, never deleted: a node of a terminal category appears in the shard
/// and the edge to it appears; the walk simply does not expand through it. A
/// dropped edge is indistinguishable from an edge that was never found.
#[derive(Clone, Debug, Default)]
pub(crate) struct TraversalFilter {
    terminal: Vec<Category>,
}

impl TraversalFilter {
    pub(crate) fn new(terminal: Vec<Category>) -> Self {
        Self { terminal }
    }

    pub(crate) fn categories(&self) -> &[Category] {
        &self.terminal
    }

    fn stops_at(&self, category: Option<Category>) -> bool {
        match category {
            // An unclassified node terminates nothing (plan-01 §7.0).
            None => false,
            Some(category) => self.terminal.contains(&category),
        }
    }
}

/// What one walk found.
#[derive(Debug, Default)]
pub(crate) struct ReachResult {
    /// Every node the walk reached, the start included.
    pub(crate) reached: HashSet<NodeIdx>,
    /// Nodes at the depth limit with resolved out-edges that were not
    /// followed. Frontier, not leaf (plan-01 §5.2).
    pub(crate) frontier: HashSet<NodeIdx>,
    /// Breadth-first distance to each reached node.
    pub(crate) depth: HashMap<NodeIdx, u32>,
}

/// The reachable set from one start node.
///
/// `depth` limits what this *view* contains. It never limits the complement,
/// which plan-01 §5.1 requires to be computed unlimited — a depth-limited
/// complement reports everything past the limit as unreachable, which is a
/// guaranteed false positive on every deep call chain.
///
/// An unresolved edge never advances the walk (plan-01 §5.3). Following a
/// candidate would be inferring an edge to fill a hole.
pub(crate) fn reachable_from(
    graph: &Graph,
    start: NodeIdx,
    depth: Option<u32>,
    filter: &TraversalFilter,
) -> ReachResult {
    let mut result = ReachResult::default();
    let mut queue = VecDeque::new();

    result.reached.insert(start);
    result.depth.insert(start, 0);
    queue.push_back(start);

    while let Some(current) = queue.pop_front() {
        let here = result.depth.get(&current).copied().unwrap_or(0);

        let resolved: Vec<NodeIdx> = graph
            .out_edges(current)
            .iter()
            .filter_map(|position| match graph.edges()[*position].to {
                EdgeEnd::Resolved(target) => Some(target),
                EdgeEnd::Unresolved(_) => None,
            })
            .collect();

        if depth.is_some_and(|limit| here >= limit) {
            if !resolved.is_empty() {
                result.frontier.insert(current);
            }
            continue;
        }

        if filter.stops_at(graph.node(current).category) {
            continue;
        }

        for target in resolved {
            // Checked before enqueue, which is what makes a cycle terminate.
            if result.reached.insert(target) {
                result.depth.insert(target, here + 1);
                queue.push_back(target);
            }
        }
    }

    result
}

/// Every indexed node no bound root reaches — plan-01 §5.4.
///
/// External nodes are excluded: the tool cannot claim that code it never
/// indexed is unreachable, because it has no evidence either way.
///
/// No category filtering happens here. Suppressing stdlib or third-party is a
/// view decision and belongs to the renderer; the waist reports.
pub(crate) fn complement(graph: &Graph, reached: &HashSet<NodeIdx>) -> Vec<NodeIdx> {
    graph
        .indices()
        .filter(|idx| graph.node(*idx).indexed() && !reached.contains(idx))
        .collect()
}
