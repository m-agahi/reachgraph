//! Fixtures built by hand from `reachgraph-plugin-api` types.
//!
//! **No `reachgraph-core` anywhere, not even as a dev-dependency.** Plan-05
//! §8.6's `render_html_has_no_core_dependency` reads `cargo metadata`, and a
//! dev-dependency is in that graph — so the guard would pass or fail depending
//! on how these fixtures were built, which is the guard measuring the test
//! rather than the code. Every view below is assembled from public
//! constructors, and every artifact byte is the JSON a run would have written.
//!
//! The shapes here are the ones the probe repository does **not** produce.
//! MEASURED on `/home/max/git/yadgarhq/task`: every edge is `resolved`, there
//! are no unresolved targets, no shard has a frontier node, `partial` is
//! false, and no contract carries two versions. The honesty features are
//! therefore untestable against that artifact and are exercised here instead.

use std::collections::BTreeMap;
use std::path::PathBuf;

use reachgraph_plugin_api::{
    ArtifactFile, Capability, Category, ContractId, Direction, DocFormat, Edge, EdgeTarget,
    GraphView, IndexCoverage, InferenceMode, Node, NodeId, OutputSink, PluginDescriptor, PluginId,
    PositionEncoding, Provenance, RenderInput, Root, RootBinding, Shard, SourceRange, Span, Symbol,
    SymbolKind, UnboundRoot, UnitId, VersionKey,
};

/// The plugin id `crate::dispatch`'s table is keyed on. Spelled here so a
/// rename on either side fails loudly.
pub const RUST: PluginId = PluginId("reachgraph-lang-rust");

/// A plugin this build has never heard of, for the negative half of the
/// dispatch table.
pub const OTHER: PluginId = PluginId("some-other-language");

/// A sink that keeps what it was handed.
#[derive(Default)]
pub struct Recording {
    pub written: BTreeMap<String, Vec<u8>>,
}

impl Recording {
    pub fn paths(&self) -> Vec<&str> {
        self.written.keys().map(String::as_str).collect()
    }

    pub fn text(&self, path: &str) -> String {
        String::from_utf8(
            self.written
                .get(path)
                .unwrap_or_else(|| panic!("{path} was not written; got {:?}", self.paths()))
                .clone(),
        )
        .expect("the file is utf-8")
    }

    pub fn json(&self, path: &str) -> serde_json::Value {
        serde_json::from_slice(
            self.written
                .get(path)
                .unwrap_or_else(|| panic!("{path} was not written; got {:?}", self.paths())),
        )
        .expect("the file is json")
    }
}

impl OutputSink for Recording {
    fn write(&mut self, relative_path: &str, bytes: &[u8]) -> std::io::Result<()> {
        self.written
            .insert(relative_path.to_owned(), bytes.to_vec());
        Ok(())
    }
}

pub fn id(plugin: PluginId, raw: &str) -> NodeId {
    NodeId {
        plugin,
        raw: raw.to_owned(),
    }
}

/// A symbol with an offset, in a file.
pub fn symbol(
    node: &NodeId,
    name: &str,
    kind: SymbolKind,
    raw_kind: &str,
    file: &str,
    container: Option<NodeId>,
) -> Symbol {
    Symbol {
        id: node.clone(),
        name: name.to_owned(),
        kind,
        raw_kind: raw_kind.to_owned(),
        range: SourceRange {
            file: PathBuf::from(file),
            span: Some(Span { start: 10, end: 90 }),
        },
        doc: None,
        doc_format: DocFormat::Markdown,
        is_test: false,
        container,
    }
}

pub fn node(id: NodeId, symbol: Option<Symbol>, unit: Option<&str>) -> Node {
    Node {
        id,
        symbol,
        category: Some(Category::FirstParty),
        unit: unit.map(|unit| UnitId(unit.to_owned())),
        depth: Some(1),
        frontier: false,
    }
}

pub fn edge(from: &NodeId, to: EdgeTarget, mode: InferenceMode) -> Edge {
    Edge {
        from: from.clone(),
        to,
        call_site: Some(SourceRange {
            file: PathBuf::from("src/lib.rs"),
            span: Some(Span {
                start: 100,
                end: 110,
            }),
        }),
        provenance: Provenance {
            plugin: RUST,
            engine: "ra_ap_ide 0.0.352".to_owned(),
        },
        inference_mode: mode,
    }
}

pub fn descriptor() -> PluginDescriptor {
    PluginDescriptor {
        id: RUST,
        position_encoding: PositionEncoding::Utf8Bytes,
        capabilities: vec![Capability::Symbols, Capability::Edges],
    }
}

pub fn root(version: Option<&str>, binding: RootBinding) -> Root {
    Root {
        contract: ContractId("acme.task".to_owned()),
        version: version.map(str::to_owned),
        service: "TaskService".to_owned(),
        operation: "CreateTask".to_owned(),
        direction: Direction::Served,
        // Deliberately contains `v2` while the root's own version may be
        // `None`. Plan-05 §4.3.2: a version is never recovered from a join
        // key, and this shape is what would let that mistake pass unnoticed.
        join_key: "acme.task.v2.TaskService/CreateTask".to_owned(),
        binding,
    }
}

/// A coverage record with every weakening field set to its quiet value.
pub fn coverage() -> IndexCoverage {
    IndexCoverage {
        contracts: vec![ContractId("acme.task".to_owned())],
        versions: vec![VersionKey {
            contract: ContractId("acme.task".to_owned()),
            version: Some("v1".to_owned()),
        }],
        roots_total: 2,
        roots_bound: 1,
        unbound_roots: vec![UnboundRoot {
            contract: ContractId("acme.task".to_owned()),
            version: Some("v3".to_owned()),
            service: "TaskService".to_owned(),
            operation: "DeleteTask".to_owned(),
            direction: Direction::Served,
            reason: "no `impl TaskService for T` method named `delete_task`".to_owned(),
        }],
        units_indexed: vec![UnitId("crate:task".to_owned())],
        plugins: vec![RUST],
        traversal_terminal_categories: vec![Category::ThirdParty, Category::Stdlib],
        partial: false,
        notes: vec![
            "generated code was not indexed for 1 of 1 members".to_owned(),
            "proc-macro expansion is disabled in this index".to_owned(),
        ],
    }
}

/// The index-wide view: a module, a trait, a trait method, an impl block, an
/// impl method, a free function, and an external target with no symbol.
pub fn index_view() -> GraphView {
    let module = id(RUST, "crate:task|0|src/lib.rs");
    let trait_ = id(RUST, "crate:task|100|src/lib.rs");
    let trait_method = id(RUST, "crate:task|140|src/lib.rs");
    let impl_block = id(RUST, "crate:task|300|src/lib.rs");
    let impl_method = id(RUST, "crate:task|340|src/lib.rs");
    let free = id(RUST, "crate:task|900|src/other.rs");
    let external = id(
        RUST,
        "external:crates-io|4190|/registry/tonic/src/request.rs",
    );

    let nodes = vec![
        node(
            module.clone(),
            Some(symbol(
                &module,
                "task",
                SymbolKind::Module,
                "Module",
                "src/lib.rs",
                None,
            )),
            Some("crate:task"),
        ),
        node(
            trait_.clone(),
            Some(symbol(
                &trait_,
                "TaskService",
                SymbolKind::Type,
                "Trait",
                "src/lib.rs",
                Some(module.clone()),
            )),
            Some("crate:task"),
        ),
        node(
            trait_method.clone(),
            Some(symbol(
                &trait_method,
                "create_task",
                SymbolKind::Method,
                "Method",
                "src/lib.rs",
                Some(trait_.clone()),
            )),
            Some("crate:task"),
        ),
        node(
            impl_block.clone(),
            Some(symbol(
                &impl_block,
                "impl TaskService for Task",
                SymbolKind::Other,
                "impl TaskService for Task",
                "src/lib.rs",
                Some(module.clone()),
            )),
            Some("crate:task"),
        ),
        node(
            impl_method.clone(),
            Some(symbol(
                &impl_method,
                "create_task",
                SymbolKind::Method,
                "Method",
                "src/lib.rs",
                Some(impl_block.clone()),
            )),
            Some("crate:task"),
        ),
        node(
            free.clone(),
            Some(symbol(
                &free,
                "helper",
                SymbolKind::Function,
                "Function",
                "src/other.rs",
                None,
            )),
            Some("crate:task"),
        ),
        // Plan-05 §4.4.1: an edge resolved here and no provider emitted a
        // symbol. It has no unit, no category and no name.
        Node {
            id: external.clone(),
            symbol: None,
            category: None,
            unit: None,
            depth: Some(2),
            frontier: false,
        },
    ];

    let edges = vec![
        edge(
            &trait_method,
            EdgeTarget::Resolved(external.clone()),
            InferenceMode::Resolved,
        ),
        edge(
            &impl_method,
            EdgeTarget::Resolved(free.clone()),
            InferenceMode::TypeInferred,
        ),
    ];

    GraphView::new(
        nodes,
        edges,
        vec![root(Some("v1"), RootBinding::Bound(trait_method))],
        vec![descriptor()],
        coverage(),
    )
}

/// The node identities [`index_view`] uses, by the name they are given there.
pub mod ids {
    use super::{id, RUST};
    use reachgraph_plugin_api::NodeId;

    pub fn module() -> NodeId {
        id(RUST, "crate:task|0|src/lib.rs")
    }
    pub fn trait_() -> NodeId {
        id(RUST, "crate:task|100|src/lib.rs")
    }
    pub fn trait_method() -> NodeId {
        id(RUST, "crate:task|140|src/lib.rs")
    }
    pub fn impl_block() -> NodeId {
        id(RUST, "crate:task|300|src/lib.rs")
    }
    pub fn impl_method() -> NodeId {
        id(RUST, "crate:task|340|src/lib.rs")
    }
    pub fn external() -> NodeId {
        id(
            RUST,
            "external:crates-io|4190|/registry/tonic/src/request.rs",
        )
    }
}

/// One shard, holding a frontier node and one unresolved edge — the two
/// shapes the probe repository never produced.
pub fn shard() -> Shard {
    let handler = ids::trait_method();
    let frontier_node = id(RUST, "crate:task|700|src/deep.rs");

    let nodes = vec![
        Node {
            id: handler.clone(),
            symbol: Some(symbol(
                &handler,
                "create_task",
                SymbolKind::Method,
                "Method",
                "src/lib.rs",
                Some(ids::trait_()),
            )),
            category: Some(Category::FirstParty),
            unit: Some(UnitId("crate:task".to_owned())),
            depth: Some(0),
            frontier: false,
        },
        Node {
            id: frontier_node.clone(),
            symbol: Some(symbol(
                &frontier_node,
                "deeper",
                SymbolKind::Function,
                "Function",
                "src/deep.rs",
                None,
            )),
            category: Some(Category::FirstParty),
            unit: Some(UnitId("crate:task".to_owned())),
            depth: Some(2),
            // Plan-05 §4.4.2: frontier, not leaf.
            frontier: true,
        },
    ];

    let edges = vec![
        edge(
            &handler,
            EdgeTarget::Resolved(frontier_node.clone()),
            InferenceMode::Lexical,
        ),
        edge(
            &handler,
            EdgeTarget::Unresolved {
                name: "save".to_owned(),
                candidates: vec![ids::impl_method(), ids::external()],
            },
            InferenceMode::Enclosure,
        ),
    ];

    Shard {
        root: root(Some("v1"), RootBinding::Bound(handler)),
        depth_limit: Some(2),
        frontier: vec![frontier_node],
        view: GraphView::new(
            nodes,
            edges,
            vec![root(Some("v1"), RootBinding::Bound(ids::trait_method()))],
            vec![descriptor()],
            coverage(),
        ),
    }
}

/// The waist's own files, as bytes, the way `Index::emit` would have written
/// them. Only the fields this crate reads are spelled out; the rest is the
/// page's business and the page fetches it.
pub fn artifact() -> Vec<ArtifactFile> {
    vec![
        file(
            "endpoints.json",
            r#"{"schema_version":1,"operations":[],"coverage":{}}"#,
        ),
        file(
            "unreachable.json",
            r#"{"schema_version":1,"claim":"not reachable from any endpoint version in this index","nodes":[{"name":"orphan"},{"name":"other"}]}"#,
        ),
        file("versions.json", r#"{"schema_version":1,"nodes":[]}"#),
        file("run.json", r#"{"repo":"/tmp/x"}"#),
    ]
}

pub fn file(path: &str, body: &str) -> ArtifactFile {
    let mut bytes = body.as_bytes().to_vec();
    bytes.push(b'\n');
    ArtifactFile {
        path: path.to_owned(),
        bytes,
    }
}

/// Render with the default policy and return what was written.
pub fn render(view: &GraphView, shards: &[Shard], artifact: &[ArtifactFile]) -> Recording {
    render_with(
        reachgraph_render_html::HtmlRenderer::new(),
        view,
        shards,
        artifact,
    )
}

pub fn render_with(
    renderer: reachgraph_render_html::HtmlRenderer,
    view: &GraphView,
    shards: &[Shard],
    artifact: &[ArtifactFile],
) -> Recording {
    use reachgraph_plugin_api::Renderer as _;

    let input = RenderInput {
        view,
        shards,
        artifact,
    };
    let mut sink = Recording::default();
    renderer
        .render(&input, &mut sink)
        .expect("the recording sink accepts every write");
    sink
}
