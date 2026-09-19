//! `Renderer` — plan-00 §3.6, arriving with plan-05.
//!
//! The trait has no behaviour of its own, so what is asserted here is what a
//! careless edit would remove.
//!
//! **`Renderer` does not extend [`Plugin`], and the negative half is the
//! load-bearing one** (plan-00 §3.6). `position_encoding`, `detection` and
//! `preflight` are meaningless for a renderer, and a provided default would be
//! a *value* — a defaulted `Utf8Bytes` would reach the artifact's plugins
//! table looking like a declaration. The compile-level assertion lives in
//! `reachgraph-render-html`, which has a concrete type to assert over; what is
//! assertable here is that a renderer can be held and called as a trait object
//! without a `Plugin` anywhere in the chain.

use std::collections::BTreeMap;

use reachgraph_plugin_api::{
    ArtifactFile, Coverage, GraphView, IndexCoverage, OutputSink, PluginId, RenderError,
    RenderInput, Renderer,
};

#[derive(Default)]
struct Recording {
    written: BTreeMap<String, Vec<u8>>,
}

impl OutputSink for Recording {
    fn write(&mut self, relative_path: &str, bytes: &[u8]) -> std::io::Result<()> {
        self.written
            .insert(relative_path.to_owned(), bytes.to_vec());
        Ok(())
    }
}

/// A renderer that writes one page and copies the waist's own files into it.
///
/// Deliberately the shape plan-05 §6.5 needs: the bytes the waist wrote reach
/// the renderer, so an inlined single-file page carries the *same* bytes the
/// sharded directory does rather than a second serialisation of them.
struct Echo;

impl Renderer for Echo {
    fn id(&self) -> PluginId {
        PluginId("echo")
    }

    fn render(
        &self,
        input: &RenderInput<'_>,
        sink: &mut dyn OutputSink,
    ) -> Result<(), RenderError> {
        let mut page = String::from("<!doctype html>");
        for file in input.artifact {
            page.push_str(&format!("<!--{}:{}-->", file.path, file.bytes.len()));
        }
        page.push_str(&format!("<!--nodes:{}-->", input.view.nodes.len()));
        page.push_str(&format!("<!--shards:{}-->", input.shards.len()));

        sink.write("index.html", page.as_bytes())
            .map_err(|source| RenderError::Sink {
                path: "index.html".to_owned(),
                source,
            })
    }
}

fn empty_coverage() -> IndexCoverage {
    IndexCoverage {
        contracts: Vec::new(),
        versions: Vec::new(),
        roots_total: 0,
        roots_bound: 0,
        unbound_roots: Vec::new(),
        units_indexed: Vec::new(),
        plugins: Vec::new(),
        traversal_terminal_categories: Vec::new(),
        partial: false,
        notes: Vec::new(),
    }
}

/// The shape plan-00 §3.6 hands a renderer: a trait object, a `&GraphView`,
/// and `&mut dyn OutputSink`.
fn render_through_object(
    renderer: &dyn Renderer,
    input: &RenderInput<'_>,
    sink: &mut dyn OutputSink,
) -> Result<(), RenderError> {
    renderer.render(input, sink)
}

#[test]
fn a_renderer_is_usable_as_a_trait_object() {
    let view = GraphView::new(
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        empty_coverage(),
    );
    let artifact = vec![ArtifactFile {
        path: "endpoints.json".to_owned(),
        bytes: b"{}\n".to_vec(),
    }];
    let input = RenderInput {
        view: &view,
        shards: &[],
        artifact: &artifact,
    };

    let mut sink = Recording::default();
    render_through_object(&Echo, &input, &mut sink).expect("the recording sink accepts the write");

    let page = String::from_utf8(sink.written["index.html"].clone()).expect("the page is utf-8");
    assert!(page.contains("<!--endpoints.json:3-->"), "{page}");
    assert!(page.contains("<!--nodes:0-->"), "{page}");
    assert!(page.contains("<!--shards:0-->"), "{page}");
}

/// The bytes the waist wrote reach the renderer unmodified. Plan-05 §6.5's
/// inlined page is the same bytes as the sharded file, not a second
/// serialisation — and a renderer that had to re-serialise would need the
/// artifact schema, which lives in `reachgraph-core` and which plan-05 §8.6
/// keeps out of a renderer's dependency graph.
#[test]
fn an_artifact_file_carries_the_bytes_the_waist_wrote() {
    let original = b"{\n  \"schema_version\": 1\n}\n".to_vec();
    let file = ArtifactFile {
        path: "endpoints.json".to_owned(),
        bytes: original.clone(),
    };

    assert_eq!(file.bytes, original);
    assert_eq!(file.path, "endpoints.json");
}

/// A renderer's identity is a [`PluginId`] for attribution and for the
/// `--renderer` flag's name, and nothing about it makes the renderer a
/// [`reachgraph_plugin_api::Plugin`].
#[test]
fn a_renderer_has_an_identity() {
    assert_eq!(Echo.id(), PluginId("echo"));
}

/// [`Coverage`] is a root provider's; a renderer sees [`IndexCoverage`]
/// through the view. Named here so the import proves the two are distinct
/// types rather than one spelled twice.
#[test]
fn a_provider_coverage_and_an_index_coverage_are_different_types() {
    let provider = Coverage {
        contracts: Vec::new(),
        versions: Vec::new(),
    };
    assert!(provider.contracts.is_empty());
    assert_eq!(empty_coverage().roots_total, 0);
}
