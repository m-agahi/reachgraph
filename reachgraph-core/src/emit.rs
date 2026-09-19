//! Building the artifact documents and writing them through an `OutputSink` —
//! plan-01 §8.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use reachgraph_plugin_api::{
    Capability, Category, DocFormat, Edge, EdgeTarget, GraphView, IndexCoverage, InferenceMode,
    Node, NodeId, OutputSink, PluginDescriptor, PositionEncoding, Root, RootBinding, Shard,
    SourceRange, Symbol, SymbolKind,
};

use crate::assemble::Index;
use crate::root::RootIdentity;
use crate::schema::{
    BindingRow, CapabilityRow, CategoryCountsRow, CategoryRow, ContractSummaryRow, CoverageRow,
    DirectionRow, DocFormatRow, EdgeRow, EdgeTargetRow, EncodingRow, EndpointsDocument,
    GeneratedBy, InferenceModeRow, NodeRef, NodeRow, OperationRow, OperationVersionRow, PluginRow,
    ProvenanceRow, RangeRow, ShardDocument, ShardRootRow, SpanRow, StatsRow, SymbolKindRow,
    SymbolRow, UnboundRootRow, UnreachableDocument, UnreachableRow, VersionKeyRow, VersionNodeRow,
    VersionsDocument, SCHEMA_VERSION, UNREACHABLE_CLAIM,
};
use crate::shard::shard_path;
use crate::versions::class_of;

// ---------------------------------------------------------------------------
// Row conversions
// ---------------------------------------------------------------------------

fn node_ref(id: &NodeId) -> NodeRef {
    NodeRef {
        plugin: id.plugin.0.to_owned(),
        raw: id.raw.clone(),
    }
}

fn encoding(value: PositionEncoding) -> EncodingRow {
    match value {
        PositionEncoding::Utf8Bytes => EncodingRow::Utf8Bytes,
        PositionEncoding::Utf16CodeUnits => EncodingRow::Utf16CodeUnits,
        PositionEncoding::Utf32CodePoints => EncodingRow::Utf32CodePoints,
    }
}

fn capability(value: Capability) -> CapabilityRow {
    match value {
        Capability::Symbols => CapabilityRow::Symbols,
        Capability::Edges => CapabilityRow::Edges,
        Capability::Roots => CapabilityRow::Roots,
        Capability::Classify => CapabilityRow::Classify,
    }
}

fn category(value: Category) -> CategoryRow {
    match value {
        Category::FirstParty => CategoryRow::FirstParty,
        Category::Generated => CategoryRow::Generated,
        Category::WorkspaceSibling => CategoryRow::WorkspaceSibling,
        Category::ThirdParty => CategoryRow::ThirdParty,
        Category::Stdlib => CategoryRow::Stdlib,
    }
}

fn direction(value: reachgraph_plugin_api::Direction) -> DirectionRow {
    match value {
        reachgraph_plugin_api::Direction::Served => DirectionRow::Served,
        reachgraph_plugin_api::Direction::Consumed => DirectionRow::Consumed,
    }
}

fn symbol_kind(value: SymbolKind) -> SymbolKindRow {
    match value {
        SymbolKind::Function => SymbolKindRow::Function,
        SymbolKind::Method => SymbolKindRow::Method,
        SymbolKind::Type => SymbolKindRow::Type,
        SymbolKind::Module => SymbolKindRow::Module,
        SymbolKind::Field => SymbolKindRow::Field,
        SymbolKind::Other => SymbolKindRow::Other,
    }
}

fn doc_format(value: DocFormat) -> DocFormatRow {
    match value {
        DocFormat::Plain => DocFormatRow::Plain,
        DocFormat::Markdown => DocFormatRow::Markdown,
    }
}

fn inference_mode(value: InferenceMode) -> InferenceModeRow {
    match value {
        InferenceMode::Resolved => InferenceModeRow::Resolved,
        InferenceMode::Lexical => InferenceModeRow::Lexical,
        InferenceMode::TypeInferred => InferenceModeRow::TypeInferred,
        InferenceMode::Enclosure => InferenceModeRow::Enclosure,
    }
}

fn range(value: &SourceRange) -> RangeRow {
    RangeRow {
        file: value.file.clone(),
        span: value.span.map(|span| SpanRow {
            start: span.start,
            end: span.end,
        }),
    }
}

fn symbol_row(symbol: &Symbol) -> SymbolRow {
    SymbolRow {
        name: symbol.name.clone(),
        kind: symbol_kind(symbol.kind),
        raw_kind: symbol.raw_kind.clone(),
        range: range(&symbol.range),
        doc: symbol.doc.clone(),
        doc_format: doc_format(symbol.doc_format),
        is_test: symbol.is_test,
        container: symbol.container.as_ref().map(node_ref),
    }
}

fn node_row(node: &Node) -> NodeRow {
    NodeRow {
        id: node_ref(&node.id),
        symbol: node.symbol.as_ref().map(symbol_row),
        unit: node.unit.as_ref().map(|unit| unit.0.clone()),
        category: node.category.map(category),
        depth: node.depth,
        frontier: node.frontier,
    }
}

fn edge_row(edge: &Edge) -> EdgeRow {
    EdgeRow {
        from: node_ref(&edge.from),
        to: match &edge.to {
            EdgeTarget::Resolved(node) => EdgeTargetRow::Resolved {
                node: node_ref(node),
            },
            EdgeTarget::Unresolved { name, candidates } => EdgeTargetRow::Unresolved {
                name: name.clone(),
                candidates: candidates.iter().map(node_ref).collect(),
            },
        },
        call_site: edge.call_site.as_ref().map(range),
        provenance: ProvenanceRow {
            plugin: edge.provenance.plugin.0.to_owned(),
            engine: edge.provenance.engine.clone(),
        },
        inference_mode: inference_mode(edge.inference_mode),
    }
}

fn binding_row(binding: &RootBinding) -> BindingRow {
    match binding {
        RootBinding::Bound(node) => BindingRow::Bound {
            node: node_ref(node),
        },
        RootBinding::Unbound { reason } => BindingRow::Unbound {
            reason: reason.clone(),
        },
    }
}

fn plugin_rows(plugins: &[PluginDescriptor]) -> Vec<PluginRow> {
    plugins
        .iter()
        .map(|plugin| PluginRow {
            id: plugin.id.0.to_owned(),
            position_encoding: encoding(plugin.position_encoding),
            capabilities: plugin
                .capabilities
                .iter()
                .copied()
                .map(capability)
                .collect(),
        })
        .collect()
}

fn coverage_row(coverage: &IndexCoverage) -> CoverageRow {
    CoverageRow {
        contracts: coverage
            .contracts
            .iter()
            .map(|contract| contract.0.clone())
            .collect(),
        versions: coverage
            .versions
            .iter()
            .map(|key| VersionKeyRow {
                contract: key.contract.0.clone(),
                version: key.version.clone(),
            })
            .collect(),
        roots_total: coverage.roots_total,
        roots_bound: coverage.roots_bound,
        unbound_roots: coverage
            .unbound_roots
            .iter()
            .map(|root| UnboundRootRow {
                contract: root.contract.0.clone(),
                version: root.version.clone(),
                service: root.service.clone(),
                operation: root.operation.clone(),
                direction: direction(root.direction),
                reason: root.reason.clone(),
            })
            .collect(),
        units_indexed: coverage
            .units_indexed
            .iter()
            .map(|unit| unit.0.clone())
            .collect(),
        plugins: coverage
            .plugins
            .iter()
            .map(|plugin| plugin.0.to_owned())
            .collect(),
        traversal_terminal_categories: coverage
            .traversal_terminal_categories
            .iter()
            .copied()
            .map(category)
            .collect(),
        partial: coverage.partial,
        notes: coverage.notes.clone(),
    }
}

fn unresolved_count(view: &GraphView) -> usize {
    view.edges
        .iter()
        .filter(|edge| matches!(edge.to, EdgeTarget::Unresolved { .. }))
        .count()
}

// ---------------------------------------------------------------------------
// Documents
// ---------------------------------------------------------------------------

fn generated_by() -> GeneratedBy {
    GeneratedBy {
        tool: "reachgraph".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
    }
}

/// One shard, as the file holds it.
pub(crate) fn shard_document(shard: &Shard) -> ShardDocument {
    ShardDocument {
        schema_version: SCHEMA_VERSION,
        root: ShardRootRow {
            contract: shard.root.contract.0.clone(),
            version: shard.root.version.clone(),
            service: shard.root.service.clone(),
            operation: shard.root.operation.clone(),
            direction: direction(shard.root.direction),
            join_key: shard.root.join_key.clone(),
            binding: binding_row(&shard.root.binding),
        },
        depth_limit: shard.depth_limit,
        plugins: plugin_rows(&shard.view.plugins),
        nodes: shard.view.nodes.iter().map(node_row).collect(),
        edges: shard.view.edges.iter().map(edge_row).collect(),
        frontier: shard.frontier.iter().map(node_ref).collect(),
        stats: StatsRow {
            node_count: shard.view.nodes.len(),
            edge_count: shard.view.edges.len(),
            unresolved_edge_count: unresolved_count(&shard.view),
        },
    }
}

/// The root list, grouped by operation with versions as siblings.
pub(crate) fn endpoints_document(index: &Index) -> EndpointsDocument {
    /// The grouping key: everything about a root except its version.
    #[derive(PartialEq, Eq)]
    struct OperationKey {
        contract: String,
        service: String,
        operation: String,
        direction: DirectionRow,
    }

    let key_of = |root: &Root| OperationKey {
        contract: root.contract.0.clone(),
        service: root.service.clone(),
        operation: root.operation.clone(),
        direction: direction(root.direction),
    };

    let mut keys: Vec<OperationKey> = Vec::new();
    let mut grouped: Vec<Vec<OperationVersionRow>> = Vec::new();

    for (position, root) in index.roots().iter().enumerate() {
        let shard = index.shard_of_root(position);
        let row = OperationVersionRow {
            version: root.version.clone(),
            join_key: root.join_key.clone(),
            binding: binding_row(&root.binding),
            shard: shard.map(|_| shard_path(&RootIdentity::of(root))),
            node_count: shard.map(|shard| shard.view.nodes.len()).unwrap_or(0),
            frontier_count: shard.map(|shard| shard.frontier.len()).unwrap_or(0),
        };

        let key = key_of(root);
        match keys.iter().position(|candidate| *candidate == key) {
            Some(group) => grouped[group].push(row),
            None => {
                keys.push(key);
                grouped.push(vec![row]);
            }
        }
    }

    EndpointsDocument {
        schema_version: SCHEMA_VERSION,
        generated_by: generated_by(),
        plugins: plugin_rows(index.plugins()),
        operations: keys
            .into_iter()
            .zip(grouped)
            .map(|(key, versions)| OperationRow {
                contract: key.contract,
                service: key.service,
                operation: key.operation,
                direction: key.direction,
                versions,
            })
            .collect(),
        coverage: coverage_row(index.coverage()),
    }
}

/// The complement, with the claim and the coverage it was computed against.
pub(crate) fn unreachable_document(index: &Index) -> UnreachableDocument {
    let mut counts = CategoryCountsRow {
        first_party: 0,
        generated: 0,
        workspace_sibling: 0,
        third_party: 0,
        stdlib: 0,
        unclassified: 0,
    };

    let nodes: Vec<UnreachableRow> = index
        .unreachable()
        .iter()
        .map(|node| {
            match node.category {
                None => counts.unclassified += 1,
                Some(Category::FirstParty) => counts.first_party += 1,
                Some(Category::Generated) => counts.generated += 1,
                Some(Category::WorkspaceSibling) => counts.workspace_sibling += 1,
                Some(Category::ThirdParty) => counts.third_party += 1,
                Some(Category::Stdlib) => counts.stdlib += 1,
            }

            UnreachableRow {
                id: node_ref(&node.id),
                name: node.name.clone(),
                file: node.file.clone(),
                category: node.category.map(category),
                is_test: node.is_test,
                possibly_reachable_via_unresolved: node.possibly_reachable_via_unresolved,
            }
        })
        .collect();

    UnreachableDocument {
        schema_version: SCHEMA_VERSION,
        claim: UNREACHABLE_CLAIM.to_owned(),
        coverage: coverage_row(index.coverage()),
        nodes,
        counts_by_category: counts,
        unresolved_edge_count: unresolved_count(index.view()),
    }
}

/// Per-version reach for every indexed node — ADR-0007's sunset answer.
pub(crate) fn versions_document(index: &Index) -> VersionsDocument {
    let reach = index.version_reach();
    let graph = index.graph();

    let mut nodes = Vec::new();
    let mut summary: BTreeMap<String, ContractSummaryRow> = BTreeMap::new();

    for key in reach.keys() {
        if reach.keys_of_contract(&key.contract).len() == 2 {
            summary
                .entry(key.contract.0.clone())
                .or_insert(ContractSummaryRow {
                    v1_only: 0,
                    v2_only: 0,
                    both: 0,
                });
        }
    }

    for idx in graph.indices() {
        if !graph.node(idx).indexed() {
            continue;
        }

        let class = class_of(reach, idx);
        if let Some(class) = class {
            let reached = reach.reached_by(idx);
            let contract = reach.keys()[reached[0]].contract.0.clone();
            if let Some(row) = summary.get_mut(&contract) {
                match class {
                    crate::versions::VersionClass::FirstOnly => row.v1_only += 1,
                    crate::versions::VersionClass::SecondOnly => row.v2_only += 1,
                    crate::versions::VersionClass::Both => row.both += 1,
                }
            }
        }

        nodes.push(VersionNodeRow {
            id: node_ref(graph.id(idx)),
            reached_by: reach.reached_by(idx),
            class: class.map(|class| class.as_str().to_owned()),
        });
    }

    VersionsDocument {
        schema_version: SCHEMA_VERSION,
        version_keys: reach
            .keys()
            .iter()
            .map(|key| VersionKeyRow {
                contract: key.contract.0.clone(),
                version: key.version.clone(),
            })
            .collect(),
        nodes,
        summary_by_contract: summary,
    }
}

// ---------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------

/// An `OutputSink` over a directory the core owns.
///
/// The one implementation the waist ships. A renderer names a relative path and
/// writes bytes; resolving that path is the sink's job and never the caller's.
#[derive(Debug)]
pub struct DirectorySink {
    root: PathBuf,
}

impl DirectorySink {
    /// A sink rooted at `root`. Directories are created as files arrive.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl OutputSink for DirectorySink {
    fn write(&mut self, relative_path: &str, bytes: &[u8]) -> io::Result<()> {
        // The path is relative and stays relative. Anything that would escape
        // the sink's own root is refused rather than resolved.
        let relative = Path::new(relative_path);
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{relative_path} is not a relative path inside the output directory"),
            ));
        }

        let target = self.root.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(target, bytes)
    }
}

fn write_json<T: serde::Serialize>(
    sink: &mut dyn OutputSink,
    path: &str,
    value: &T,
) -> io::Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    bytes.push(b'\n');
    sink.write(path, &bytes)
}

impl Index {
    /// Write the artifact: the root list, one shard per bound root, the
    /// complement and the per-version classification.
    ///
    /// ADR-0006's `index.html` and the inlined single-file case are a
    /// renderer's, and arrive with plan-05.
    pub fn emit(&self, sink: &mut dyn OutputSink) -> io::Result<()> {
        write_json(sink, "endpoints.json", &endpoints_document(self))?;

        for shard in self.shards() {
            let path = shard_path(&RootIdentity::of(&shard.root));
            write_json(sink, &path, &shard_document(shard))?;
        }

        write_json(sink, "unreachable.json", &unreachable_document(self))?;
        write_json(sink, "versions.json", &versions_document(self))?;

        Ok(())
    }
}
