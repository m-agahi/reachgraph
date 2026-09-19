//! Provider orchestration, pairing, root binding and the built index —
//! plan-01 §4.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use reachgraph_plugin_api::{
    Capability, Category, Classifier, ContractId, EdgeProvider, GraphView, IndexCoverage, Node,
    NodeId, Plugin, PluginDescriptor, PluginError, PluginId, Preflight, Root, RootBinding,
    RootProvider, Shard, Symbol, SymbolIndex, SymbolProvider, UnboundRoot, Unit, UnitId,
    VersionKey,
};

use crate::classify::{classify_all, Classifiers};
use crate::diag::BuildDiagnostic;
use crate::graph::Graph;
use crate::intern::NodeIdx;
use crate::reach::{complement, reachable_from, TraversalFilter};
use crate::root::RootIdentity;
use crate::versions::VersionReach;

/// The categories that stop the walk unless the caller says otherwise —
/// plan-01 §7.1.
const DEFAULT_TERMINAL_CATEGORIES: [Category; 2] = [Category::ThirdParty, Category::Stdlib];

/// The shard depth limit when the caller states none (`docs/design.md` §7).
const DEFAULT_DEPTH: u32 = 3;

/// The providers a build runs over, one slice per capability.
///
/// Four named slices rather than one `&[&dyn Plugin]`: a `&dyn Plugin` cannot
/// be narrowed to `&dyn SymbolProvider`, because Rust has no downcast from a
/// supertrait object to a subtrait object. Plan-01 §4.2 writes the single-slice
/// signature and partitions it by `provides()`; that partition has nothing to
/// partition into. ADR-0002 makes plugins compile-time, so every caller knows
/// its concrete types and filling four slices costs nothing.
///
/// `provides()` stays load-bearing rather than decorative: each entry is
/// checked against the slice it was placed in, so a provider that does not
/// declare the capability it was registered for is a build error.
#[derive(Default)]
pub struct BuildInputs<'a> {
    /// Plugins that emit symbols.
    pub symbols: Vec<&'a dyn SymbolProvider>,
    /// Plugins that emit edges.
    pub edges: Vec<&'a dyn EdgeProvider>,
    /// Plugins that emit roots.
    pub roots: Vec<&'a dyn RootProvider>,
    /// Plugins that answer `classify`.
    pub classifiers: Vec<&'a dyn Classifier>,
}

/// What the caller chose about this build.
#[derive(Clone, Debug)]
pub struct BuildOptions {
    /// The shard depth limit. `None` walks unlimited.
    ///
    /// **A view parameter only.** The complement is always computed unlimited
    /// (plan-01 §5.1); a depth-limited complement reports everything past the
    /// limit as unreachable.
    pub depth: Option<u32>,
    /// Continue past a provider failure, setting `IndexCoverage::partial`.
    pub allow_partial: bool,
    /// Categories that would otherwise stop the walk and should not.
    pub expand_categories: Vec<Category>,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            depth: Some(DEFAULT_DEPTH),
            allow_partial: false,
            expand_categories: Vec::new(),
        }
    }
}

/// What stopped a build.
#[derive(Debug)]
pub enum BuildError {
    /// A plugin's own prerequisites are not met. Both strings are surfaced
    /// (ADR-0003 field 5).
    PreflightFailed {
        /// The plugin that refused to run.
        plugin: PluginId,
        /// What it checked and what it found.
        reason: String,
        /// What the user should do about it.
        remediation: String,
    },
    /// A provider was registered for a capability it does not declare.
    CapabilityNotDeclared {
        /// The plugin registered.
        plugin: PluginId,
        /// The capability the slice it was placed in implies.
        capability: Capability,
    },
    /// A symbol provider with no edge provider of the same `PluginId`.
    ///
    /// **The load-bearing one.** A symbol provider with no edges yields a graph
    /// of isolated nodes, so every symbol is unreachable — and the tool then
    /// reports an entire repository as not reachable from any endpoint. That is
    /// the false-positive class ADR-0007 calls a correctness problem, so
    /// failing loudly is the only defensible behaviour.
    UnpairedSymbolProvider {
        /// The plugin with no partner.
        plugin: PluginId,
    },
    /// An edge provider with no symbol provider of the same `PluginId`. Its
    /// edges would have an unindexed node at every end.
    UnpairedEdgeProvider {
        /// The plugin with no partner.
        plugin: PluginId,
    },
    /// A provider failed and `allow_partial` was not set.
    Provider {
        /// The plugin that failed.
        plugin: PluginId,
        /// What it reported.
        source: PluginError,
    },
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::PreflightFailed {
                plugin,
                reason,
                remediation,
            } => write!(f, "{}: {reason}\n  try: {remediation}", plugin.0),
            BuildError::CapabilityNotDeclared { plugin, capability } => write!(
                f,
                "{}: registered for {capability:?}, which it does not declare",
                plugin.0
            ),
            BuildError::UnpairedSymbolProvider { plugin } => write!(
                f,
                "{}: emits symbols and no edges, which would report every symbol as not \
                 reachable from any endpoint version in this index",
                plugin.0
            ),
            BuildError::UnpairedEdgeProvider { plugin } => write!(
                f,
                "{}: emits edges and no symbols, so every end of every edge is unindexed",
                plugin.0
            ),
            BuildError::Provider { plugin, source } => write!(f, "{}: {source}", plugin.0),
        }
    }
}

impl std::error::Error for BuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            BuildError::Provider { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// One indexed node no bound root reaches — plan-01 §5.4, §8.4.
#[derive(Clone, Debug)]
pub struct UnreachableNode {
    /// The node's identity.
    pub id: NodeId,
    /// Its bare name.
    pub name: String,
    /// The file it is declared in. Always known for an indexed node.
    pub file: std::path::PathBuf,
    /// `None` when its plugin registered no classifier.
    pub category: Option<Category>,
    /// Whether the provider marked it test code.
    pub is_test: bool,
    /// **An annotation, never a filter.** True when this node is a candidate of
    /// an unresolved edge whose source is reachable. It never removes the node
    /// from the list: an implementation that dropped flagged nodes would have
    /// re-introduced the inference `EdgeTarget` forbids (plan-01 §5.3).
    pub possibly_reachable_via_unresolved: bool,
}

/// One bound root and the walk it produced.
#[derive(Debug)]
struct BoundRoot {
    root: Root,
    start: NodeIdx,
    /// Which entry of `Index::roots` this is. Two roots may be identical in
    /// every field but `join_key` and are still two roots (ADR-0007), so a
    /// shard is matched to its root by position rather than by identity.
    position: usize,
}

/// The built index. Construction is `build`; everything else reads it.
#[derive(Debug)]
pub struct Index {
    graph: Graph,
    roots: Vec<Root>,
    plugins: Vec<PluginDescriptor>,
    coverage: IndexCoverage,
    diagnostics: Vec<BuildDiagnostic>,
    view: GraphView,
    shards: Vec<Shard>,
    shard_of_root: Vec<Option<usize>>,
    unreachable: Vec<UnreachableNode>,
    versions: VersionReach,
}

impl Index {
    /// Assemble a graph, bind its roots, walk it and shard it.
    pub fn build(
        root: &Path,
        inputs: &BuildInputs<'_>,
        opts: &BuildOptions,
    ) -> Result<Index, BuildError> {
        let mut diagnostics = Vec::new();
        let mut partial = false;

        preflight_all(root, inputs, &mut diagnostics)?;
        check_capabilities(inputs)?;
        check_pairing(inputs)?;

        let mut graph = Graph::default();
        let mut units_indexed: Vec<UnitId> = Vec::new();
        let mut unit_table: HashMap<UnitId, Unit> = HashMap::new();

        for provider in &inputs.symbols {
            let plugin = provider.id();
            let partner = inputs
                .edges
                .iter()
                .find(|edges| edges.id() == plugin)
                .copied();

            let units = match provider.discover_units(root) {
                Ok(units) => units,
                Err(source) => {
                    tolerate(plugin, source, opts, &mut diagnostics, &mut partial)?;
                    continue;
                }
            };

            for unit in &units {
                if !unit_table.contains_key(&unit.id) {
                    units_indexed.push(unit.id.clone());
                    unit_table.insert(unit.id.clone(), unit.clone());
                }

                match provider.symbols_in(unit) {
                    Ok(symbols) => {
                        for symbol in symbols {
                            let idx = graph.intern(&symbol.id);
                            let id = symbol.id.clone();
                            if !graph.attach_symbol(idx, symbol, unit.id.clone()) {
                                diagnostics.push(BuildDiagnostic::DuplicateSymbol { id });
                            }
                        }
                    }
                    Err(source) => {
                        tolerate(plugin, source, opts, &mut diagnostics, &mut partial)?;
                    }
                }

                // The SAME `Unit` values go to both halves of the pair. That is
                // the whole point of pairing (ADR-0003 field 1), and
                // re-discovering per capability would manufacture the
                // `UnknownUnit` a provider is entitled to answer with.
                if let Some(edges) = partner {
                    match edges.edges_in(unit) {
                        Ok(edges) => {
                            for edge in edges {
                                graph.add_edge(edge);
                            }
                        }
                        Err(source) => {
                            tolerate(plugin, source, opts, &mut diagnostics, &mut partial)?;
                        }
                    }
                }
            }
        }

        graph.rebind_candidates();

        let classifiers = Classifiers::new(&inputs.classifiers);
        classify_all(&mut graph, &classifiers, &unit_table);

        for plugin in distinct_plugins(inputs) {
            if !classifiers.has(plugin) {
                diagnostics.push(BuildDiagnostic::NoClassifierForPlugin { plugin });
            }
        }

        let mut roots: Vec<Root> = Vec::new();
        let mut contracts: Vec<ContractId> = Vec::new();
        let mut version_keys: Vec<VersionKey> = Vec::new();

        for provider in &inputs.roots {
            let plugin = provider.id();
            let found = {
                let index = GraphSymbolIndex { graph: &graph };
                provider.roots(root, &index)
            };

            match found {
                Ok(found) => roots.extend(found),
                Err(source) => {
                    tolerate(plugin, source, opts, &mut diagnostics, &mut partial)?;
                    continue;
                }
            }

            let coverage = provider.coverage();
            for contract in coverage.contracts {
                if !contracts.contains(&contract) {
                    contracts.push(contract);
                }
            }
            for key in coverage.versions {
                if !version_keys.contains(&key) {
                    version_keys.push(key);
                }
            }
        }

        // A root's own version key is covered even if no provider listed it.
        // Coverage that omitted a root the same provider returned would
        // understate what the index looked at, which is the ADR-0007 failure
        // this record exists to prevent.
        for root in &roots {
            let key = VersionKey {
                contract: root.contract.clone(),
                version: root.version.clone(),
            };
            if !version_keys.contains(&key) {
                version_keys.push(key);
            }
            if !contracts.contains(&root.contract) {
                contracts.push(root.contract.clone());
            }
        }

        let mut bound: Vec<BoundRoot> = Vec::new();
        let mut unbound_roots: Vec<UnboundRoot> = Vec::new();

        for (position, root) in roots.iter().enumerate() {
            match &root.binding {
                RootBinding::Bound(node) => {
                    let idx = graph.intern(node);
                    if !graph.node(idx).indexed() {
                        diagnostics.push(BuildDiagnostic::RootBoundToUnindexedNode {
                            root: RootIdentity::of(root),
                            node: node.clone(),
                        });
                    }
                    bound.push(BoundRoot {
                        root: root.clone(),
                        start: idx,
                        position,
                    });
                }
                RootBinding::Unbound { reason } => unbound_roots.push(UnboundRoot {
                    contract: root.contract.clone(),
                    version: root.version.clone(),
                    service: root.service.clone(),
                    operation: root.operation.clone(),
                    direction: root.direction,
                    reason: reason.clone(),
                }),
            }
        }

        let terminal: Vec<Category> = DEFAULT_TERMINAL_CATEGORIES
            .into_iter()
            .filter(|category| !opts.expand_categories.contains(category))
            .collect();
        let filter = TraversalFilter::new(terminal);

        let plugins = descriptors(inputs);
        let coverage = IndexCoverage {
            contracts,
            versions: version_keys.clone(),
            roots_total: roots.len(),
            roots_bound: bound.len(),
            unbound_roots,
            units_indexed,
            plugins: distinct_plugins(inputs),
            traversal_terminal_categories: filter.categories().to_vec(),
            partial,
            notes: collect_notes(inputs),
        };

        // The complement is unlimited, always (plan-01 §5.1).
        let mut reached_anywhere: HashSet<NodeIdx> = HashSet::new();
        let mut versions = VersionReach::new(version_keys.clone(), graph.indices().count());

        for entry in &bound {
            let walk = reachable_from(&graph, entry.start, None, &filter);
            let key = VersionKey {
                contract: entry.root.contract.clone(),
                version: entry.root.version.clone(),
            };
            let key_index = version_keys.iter().position(|candidate| *candidate == key);

            for node in &walk.reached {
                reached_anywhere.insert(*node);
                if let Some(key_index) = key_index {
                    versions.mark(*node, key_index);
                }
            }
        }

        let mut shard_of_root: Vec<Option<usize>> = vec![None; roots.len()];
        for (shard, entry) in bound.iter().enumerate() {
            shard_of_root[entry.position] = Some(shard);
        }

        let shards = bound
            .iter()
            .map(|entry| {
                let walk = reachable_from(&graph, entry.start, opts.depth, &filter);
                let view = extract(&graph, &walk, entry, &plugins, &coverage);
                Shard {
                    root: entry.root.clone(),
                    depth_limit: opts.depth,
                    frontier: walk
                        .frontier
                        .iter()
                        .map(|idx| graph.id(*idx).clone())
                        .collect(),
                    view,
                }
            })
            .collect();

        let flagged = possibly_reachable(&graph, &reached_anywhere);
        let unreachable = complement(&graph, &reached_anywhere)
            .into_iter()
            .filter_map(|idx| {
                let record = graph.node(idx);
                let symbol = record.symbol.as_ref()?;
                Some(UnreachableNode {
                    id: graph.id(idx).clone(),
                    name: symbol.name.clone(),
                    file: symbol.range.file.clone(),
                    category: record.category,
                    is_test: symbol.is_test,
                    possibly_reachable_via_unresolved: flagged.contains(&idx),
                })
            })
            .collect();

        let view = index_wide(&graph, &roots, &plugins, &coverage);

        Ok(Index {
            graph,
            roots,
            plugins,
            coverage,
            diagnostics,
            view,
            shards,
            shard_of_root,
            unreachable,
            versions,
        })
    }

    /// The index-wide view: every node, every edge, every root, and no depth.
    pub fn view(&self) -> &GraphView {
        &self.view
    }

    /// One shard per bound root, in root order.
    pub fn shards(&self) -> &[Shard] {
        &self.shards
    }

    /// Every root, bound and unbound alike. An unbound root is never dropped.
    pub fn roots(&self) -> &[Root] {
        &self.roots
    }

    /// What the index covered (ADR-0007 requirement 1).
    pub fn coverage(&self) -> &IndexCoverage {
        &self.coverage
    }

    /// What the build observed and continued past.
    pub fn diagnostics(&self) -> &[BuildDiagnostic] {
        &self.diagnostics
    }

    /// Every indexed node no bound root reaches.
    pub fn unreachable(&self) -> &[UnreachableNode] {
        &self.unreachable
    }

    /// What each contributing plugin declared about itself.
    pub fn plugins(&self) -> &[PluginDescriptor] {
        &self.plugins
    }

    /// The shard built for the root at this position, if it bound.
    pub(crate) fn shard_of_root(&self, position: usize) -> Option<&Shard> {
        self.shard_of_root
            .get(position)
            .copied()
            .flatten()
            .and_then(|shard| self.shards.get(shard))
    }

    pub(crate) fn graph(&self) -> &Graph {
        &self.graph
    }

    pub(crate) fn version_reach(&self) -> &VersionReach {
        &self.versions
    }
}

fn distinct_plugins(inputs: &BuildInputs<'_>) -> Vec<PluginId> {
    let mut seen: Vec<PluginId> = Vec::new();
    for plugin in plugin_views(inputs) {
        if !seen.contains(&plugin.id()) {
            seen.push(plugin.id());
        }
    }
    seen
}

fn plugin_views<'a>(inputs: &'a BuildInputs<'_>) -> Vec<&'a dyn Plugin> {
    let mut all: Vec<&dyn Plugin> = Vec::new();
    all.extend(inputs.symbols.iter().map(|p| *p as &dyn Plugin));
    all.extend(inputs.edges.iter().map(|p| *p as &dyn Plugin));
    all.extend(inputs.roots.iter().map(|p| *p as &dyn Plugin));
    all.extend(inputs.classifiers.iter().map(|p| *p as &dyn Plugin));
    all
}

/// Every contributing plugin's own account of the run, in registration order.
///
/// Collected **here**, after every provider has been asked for its symbols,
/// edges and roots, because a plugin that learns what it could not see by
/// loading a workspace has nothing to say before it loads one.
///
/// Deduplicated by `PluginId` for the same reason `descriptors` is: one plugin
/// commonly appears in three of the four slices, and asking it three times
/// would write its sentence into the artifact three times.
///
/// The strings are carried and never read. The waist does not know what a note
/// means, does not order them by content and does not merge two that look
/// alike — the same opacity it keeps over `NodeId::raw` and `Root::join_key`.
fn collect_notes(inputs: &BuildInputs<'_>) -> Vec<String> {
    let mut asked: Vec<PluginId> = Vec::new();
    let mut notes: Vec<String> = Vec::new();

    for plugin in plugin_views(inputs) {
        if asked.contains(&plugin.id()) {
            continue;
        }
        asked.push(plugin.id());
        notes.extend(plugin.notes());
    }

    notes
}

fn descriptors(inputs: &BuildInputs<'_>) -> Vec<PluginDescriptor> {
    let mut descriptors: Vec<PluginDescriptor> = Vec::new();
    for plugin in plugin_views(inputs) {
        if descriptors.iter().any(|d| d.id == plugin.id()) {
            continue;
        }
        descriptors.push(PluginDescriptor {
            id: plugin.id(),
            position_encoding: plugin.position_encoding(),
            capabilities: plugin.provides().to_vec(),
        });
    }
    descriptors
}

fn preflight_all(
    root: &Path,
    inputs: &BuildInputs<'_>,
    diagnostics: &mut Vec<BuildDiagnostic>,
) -> Result<(), BuildError> {
    let mut checked: Vec<PluginId> = Vec::new();

    for plugin in plugin_views(inputs) {
        if checked.contains(&plugin.id()) {
            continue;
        }
        checked.push(plugin.id());

        // The repository the caller named, not the process's working
        // directory. `root` is the plugin's own to interpret; the waist passes
        // it and checks nothing itself. Never `command -v`.
        match plugin.preflight(root) {
            Preflight::Ok => {}
            Preflight::Warned {
                reason,
                remediation,
            } => {
                diagnostics.push(BuildDiagnostic::PreflightWarned {
                    plugin: plugin.id(),
                    reason,
                    remediation,
                });
            }
            Preflight::Failed {
                reason,
                remediation,
            } => {
                return Err(BuildError::PreflightFailed {
                    plugin: plugin.id(),
                    reason,
                    remediation,
                })
            }
        }
    }

    Ok(())
}

fn check_capabilities(inputs: &BuildInputs<'_>) -> Result<(), BuildError> {
    let declared = |plugin: &dyn Plugin, capability: Capability| -> Result<(), BuildError> {
        if plugin.provides().contains(&capability) {
            Ok(())
        } else {
            Err(BuildError::CapabilityNotDeclared {
                plugin: plugin.id(),
                capability,
            })
        }
    };

    for provider in &inputs.symbols {
        declared(*provider, Capability::Symbols)?;
    }
    for provider in &inputs.edges {
        declared(*provider, Capability::Edges)?;
    }
    for provider in &inputs.roots {
        declared(*provider, Capability::Roots)?;
    }
    for provider in &inputs.classifiers {
        declared(*provider, Capability::Classify)?;
    }

    Ok(())
}

fn check_pairing(inputs: &BuildInputs<'_>) -> Result<(), BuildError> {
    for provider in &inputs.symbols {
        if !inputs.edges.iter().any(|edges| edges.id() == provider.id()) {
            return Err(BuildError::UnpairedSymbolProvider {
                plugin: provider.id(),
            });
        }
    }

    for provider in &inputs.edges {
        if !inputs
            .symbols
            .iter()
            .any(|symbols| symbols.id() == provider.id())
        {
            return Err(BuildError::UnpairedEdgeProvider {
                plugin: provider.id(),
            });
        }
    }

    Ok(())
}

/// Abort, or record and continue — plan-01 §4.2 and open question 3.
fn tolerate(
    plugin: PluginId,
    source: PluginError,
    opts: &BuildOptions,
    diagnostics: &mut Vec<BuildDiagnostic>,
    partial: &mut bool,
) -> Result<(), BuildError> {
    if opts.allow_partial {
        diagnostics.push(BuildDiagnostic::ProviderFailed {
            plugin,
            detail: source.to_string(),
        });
        *partial = true;
        Ok(())
    } else {
        Err(BuildError::Provider { plugin, source })
    }
}

/// Nodes a reachable unresolved call named as a candidate — plan-01 §5.3.
fn possibly_reachable(graph: &Graph, reached: &HashSet<NodeIdx>) -> HashSet<NodeIdx> {
    let mut flagged = HashSet::new();

    for record in graph.edges() {
        if !reached.contains(&record.from) {
            continue;
        }
        if let crate::graph::EdgeEnd::Unresolved(candidates) = &record.to {
            for candidate in candidates.iter().flatten() {
                flagged.insert(*candidate);
            }
        }
    }

    flagged
}

fn node_of(graph: &Graph, idx: NodeIdx, depth: Option<u32>, frontier: bool) -> Node {
    let record = graph.node(idx);
    Node {
        id: graph.id(idx).clone(),
        symbol: record.symbol.clone(),
        category: record.category,
        unit: record.unit.clone(),
        depth,
        frontier,
    }
}

fn index_wide(
    graph: &Graph,
    roots: &[Root],
    plugins: &[PluginDescriptor],
    coverage: &IndexCoverage,
) -> GraphView {
    GraphView::new(
        graph
            .indices()
            .map(|idx| node_of(graph, idx, None, false))
            .collect(),
        graph
            .edges()
            .iter()
            .map(|record| record.edge.clone())
            .collect(),
        roots.to_vec(),
        plugins.to_vec(),
        coverage.clone(),
    )
}

/// One shard's subgraph.
///
/// An edge is carried when its source is in the walk and its resolved target is
/// too, and every unresolved edge from a reached source is carried with its
/// candidates intact — plan-01 §5.3 puts it in the shard of the root that
/// reaches its source.
fn extract(
    graph: &Graph,
    walk: &crate::reach::ReachResult,
    entry: &BoundRoot,
    plugins: &[PluginDescriptor],
    coverage: &IndexCoverage,
) -> GraphView {
    let nodes: Vec<Node> = graph
        .indices()
        .filter(|idx| walk.reached.contains(idx))
        .map(|idx| {
            node_of(
                graph,
                idx,
                walk.depth.get(&idx).copied(),
                walk.frontier.contains(&idx),
            )
        })
        .collect();

    let edges = graph
        .edges()
        .iter()
        .filter(|record| {
            walk.reached.contains(&record.from)
                && match &record.to {
                    crate::graph::EdgeEnd::Resolved(target) => walk.reached.contains(target),
                    crate::graph::EdgeEnd::Unresolved(_) => true,
                }
        })
        .map(|record| record.edge.clone())
        .collect();

    GraphView::new(
        nodes,
        edges,
        vec![entry.root.clone()],
        plugins.to_vec(),
        coverage.clone(),
    )
}

/// The lookup surface handed to a `RootProvider`.
///
/// A linear scan over the interned symbol set. Deliberately narrow: enough to
/// bind a handler, not enough to re-implement the graph.
struct GraphSymbolIndex<'a> {
    graph: &'a Graph,
}

impl GraphSymbolIndex<'_> {
    fn symbols(&self) -> impl Iterator<Item = &Symbol> {
        self.graph
            .indices()
            .filter_map(|idx| self.graph.node(idx).symbol.as_ref())
    }
}

impl SymbolIndex for GraphSymbolIndex<'_> {
    fn by_name(&self, name: &str) -> Vec<&Symbol> {
        self.symbols()
            .filter(|symbol| symbol.name == name)
            .collect()
    }

    fn in_file(&self, path: &Path) -> Vec<&Symbol> {
        self.symbols()
            .filter(|symbol| symbol.range.file == path)
            .collect()
    }

    fn get(&self, id: &NodeId) -> Option<&Symbol> {
        self.graph
            .lookup(id)
            .and_then(|idx| self.graph.node(idx).symbol.as_ref())
    }
}
