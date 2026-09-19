//! Applying `Classifier` trait objects — plan-01 §7.
//!
//! The core holds `&dyn Classifier` and calls `classify`. It never calls a
//! language crate function, never matches on `PluginId` to pick behaviour, and
//! contains no path fragment of any kind. Those belong to the plugin, because
//! `docs/design.md` §8 measured that they are language-specific *and*
//! machine-specific.

use std::collections::HashMap;

use reachgraph_plugin_api::{Category, Classifier, PluginId, Unit, UnitId};

use crate::graph::Graph;

/// The classifier registered for each plugin, if any.
pub(crate) struct Classifiers<'a> {
    by_plugin: HashMap<PluginId, &'a dyn Classifier>,
}

impl<'a> Classifiers<'a> {
    pub(crate) fn new(classifiers: &[&'a dyn Classifier]) -> Self {
        Self {
            by_plugin: classifiers
                .iter()
                .map(|classifier| (classifier.id(), *classifier))
                .collect(),
        }
    }

    pub(crate) fn has(&self, plugin: PluginId) -> bool {
        self.by_plugin.contains_key(&plugin)
    }
}

/// Classify every indexed node.
///
/// Selection is by the node's own plugin. No classifier for that plugin leaves
/// `category: None`, which is a reported absence rather than an error.
///
/// Every indexed node has a real path, because `Symbol::range` is required and
/// `SourceRange::file` is required — so classification is always at file
/// granularity, and two symbols in one unit under different trees classify
/// differently. The span is never read.
///
/// An external node was never indexed, so no plugin ever gave it a file, so it
/// cannot be classified (plan-01 §7.0).
pub(crate) fn classify_all(
    graph: &mut Graph,
    classifiers: &Classifiers<'_>,
    units: &HashMap<UnitId, Unit>,
) {
    let assignments: Vec<(crate::intern::NodeIdx, Category)> = graph
        .indices()
        .filter_map(|idx| {
            let record = graph.node(idx);
            let symbol = record.symbol.as_ref()?;
            let unit_id = record.unit.as_ref()?;
            let unit = units.get(unit_id)?;
            let classifier = classifiers.by_plugin.get(&symbol.id.plugin)?;
            Some((idx, classifier.classify(&symbol.range.file, unit)))
        })
        .collect();

    for (idx, category) in assignments {
        graph.node_mut(idx).category = Some(category);
    }
}
