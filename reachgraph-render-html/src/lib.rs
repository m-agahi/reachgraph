//! The static-HTML renderer — plan-05.
//!
//! One [`Renderer`] implementation. It emits the page half of ADR-0006's
//! root-sharded artifact: `index.html`, the presenter, the vendored
//! JavaScript, one computed sidecar, and — under a threshold —
//! `overview.html` with every byte of data inside it.
//!
//! **It computes no reachability.** Reachability, the complement over all
//! roots, sharding and coverage are the waist (ADR-0003). A renderer that
//! recomputed a reachable set would have duplicated the thesis in a plugin.
//!
//! # What this crate emits, and what the waist emits
//!
//! Plan-05 §4 has the renderer writing `endpoints.json`, `graph/<slug>.json`
//! and `unreachable.json`. **The waist writes those, and it wrote them before
//! this crate existed** (plan-01 §8.6, `reachgraph-core`'s `emit.rs`). The
//! schema is core's serde mirror, which ADR-0727 records and which plan-05
//! §8.6 keeps out of this crate's dependency graph — so this crate could not
//! write them even if the division of labour were re-opened.
//!
//! What is left is what only a renderer can do, and it is not a residue:
//!
//! - **The page**, and the vendored JavaScript ADR-0001 forbids downloading.
//! - **`structure.json`** — the compound-box hierarchy and ADR-0729's
//!   dispatch classification. Plan-05 §9.2 places exactly this here: the waist
//!   follows containment links by equality and never interprets them; deciding
//!   that an ancestor is a module rather than a type reads `kind` and
//!   `raw_kind`, which is a plugin's job.
//! - **`overview.html`**, which inlines the waist's own bytes.
//!
//! # The four honesty obligations this crate carries
//!
//! Each is a place where a prettier page would be a false one.
//!
//! 1. **The unreachability claim is copied, never composed** — plan-05 §4.5,
//!    §6.4. The heading is the string the waist wrote. The word this project
//!    refuses appears in no label, heading, tooltip or template string here,
//!    and `renderer_authors_no_forbidden_wording` scans this crate's own
//!    sources and templates for it.
//! 2. **What the claim was computed against is adjacent to it** — the covered
//!    roots, the terminal categories, every unbound root, and every
//!    `IndexCoverage::notes` entry. A reader who cannot see the covered set
//!    cannot evaluate the claim.
//! 3. **A frontier node is not a leaf** and a node with no symbol is not
//!    absent. Both are drawn, both are marked.
//! 4. **Edge strength is visually distinguishable without colour**, and
//!    ADR-0729's trait-declaration target is distinguishable from the
//!    implementation that runs.
//!
//! # Untested surface, stated
//!
//! There is no browser harness and v0.1 will not have one (plan-05 §8.1).
//! Nothing here verifies that Cytoscape draws the graph, that the version
//! toggle switches or that the boxes collapse. What is tested is the emitted
//! bytes: the page is **parsed** and asserted over, the sidecar is asserted
//! over, and the inlined data is round-tripped. The mitigation is structural —
//! every classification is computed in Rust, so the untested surface is as
//! small as it can be made.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod dispatch;
pub mod page;
pub mod structure;
pub mod vendor;

use reachgraph_plugin_api::{
    ArtifactFile, OutputSink, PluginId, RenderError, RenderInput, Renderer,
};

use crate::page::Shape;

/// This renderer's identity, and the name `--renderer` selects it by.
pub const RENDERER_ID: &str = "html";

/// Where the computed sidecar lands.
pub const STRUCTURE_PATH: &str = "structure.json";

/// Where the page lands.
pub const INDEX_PATH: &str = "index.html";

/// Where the single-file page lands, when it is emitted.
pub const OVERVIEW_PATH: &str = "overview.html";

/// The directory the vendored bundles and their notices land in.
pub const VENDOR_DIR: &str = "vendor";

/// The top-level names this renderer owns, for `Renderer::owns`.
///
/// `overview.html` is here whether or not this run emits it: the previous run
/// may have, and a stale single-file page beside a fresh sharded one would
/// show a reader two different repositories with nothing saying which is
/// current.
const OWNS: [&str; 5] = [
    INDEX_PATH,
    OVERVIEW_PATH,
    STRUCTURE_PATH,
    page::LOADER_PATH,
    VENDOR_DIR,
];

/// ADR-0006's threshold: roughly 5 MB of graph JSON.
///
/// **Measured over the serialised graph JSON and nothing else** (plan-05
/// §6.5). The emitted `overview.html` is larger than this by the vendored
/// JavaScript — 759 606 bytes, MEASURED — plus the presenter. That
/// discrepancy is deliberate and is recorded here so nobody later "fixes" it
/// by counting the JavaScript into the budget and silently shrinking it.
pub const DEFAULT_INLINE_THRESHOLD: u64 = 5 * 1024 * 1024;

/// When, and whether, to also emit the single-file page.
///
/// A named type rather than an `Option<u64>` with a comment. "Never inline"
/// and "inline under N" are different instructions, and a renderer that
/// received `None` would have to guess which one it meant.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Overview {
    /// Emit `overview.html` when the graph JSON is at most this many bytes.
    Under(u64),
    /// Never emit it. The sharded directory is emitted either way —
    /// `overview.html` is an addition, never a replacement.
    Never,
}

impl Default for Overview {
    fn default() -> Self {
        Overview::Under(DEFAULT_INLINE_THRESHOLD)
    }
}

/// The renderer.
#[derive(Clone, Copy, Debug, Default)]
pub struct HtmlRenderer {
    overview: Overview,
}

impl HtmlRenderer {
    /// A renderer with ADR-0006's default threshold.
    pub fn new() -> Self {
        Self::default()
    }

    /// A renderer with an explicit single-file policy.
    pub fn with_overview(overview: Overview) -> Self {
        Self { overview }
    }

    /// The policy this renderer was built with.
    pub fn overview(&self) -> Overview {
        self.overview
    }
}

/// The size plan-05 §6.5's threshold is compared against: the waist's graph
/// JSON, concatenated.
///
/// The page, the presenter and the vendored bundles are **not** counted, and
/// neither is the run record — the threshold asks how much graph data would be
/// inlined, and those are not graph data.
fn graph_json_bytes(artifact: &[ArtifactFile]) -> u64 {
    artifact
        .iter()
        .filter(|file| is_graph_json(&file.path))
        .map(|file| file.bytes.len() as u64)
        .sum()
}

/// Whether an artifact file is graph data the page reads.
///
/// One predicate, used by both the threshold and the inliner, because a page
/// that inlines a file the threshold did not count would be larger than the
/// budget said — silently, and only on the repositories where it matters.
///
/// `run.json` is the binary's record of the run, not graph data. The page
/// never reads it, so inlining it would only grow the file.
pub fn is_graph_json(path: &str) -> bool {
    path.ends_with(".json") && path != "run.json"
}

fn write(sink: &mut dyn OutputSink, path: &str, bytes: &[u8]) -> Result<(), RenderError> {
    sink.write(path, bytes).map_err(|source| RenderError::Sink {
        path: path.to_owned(),
        source,
    })
}

impl Renderer for HtmlRenderer {
    fn id(&self) -> PluginId {
        PluginId(RENDERER_ID)
    }

    fn owns(&self) -> &[&'static str] {
        &OWNS
    }

    fn render(
        &self,
        input: &RenderInput<'_>,
        sink: &mut dyn OutputSink,
    ) -> Result<(), RenderError> {
        // The sidecar first: the page's licence block and coverage panel do
        // not need it, but `overview.html` inlines it and the two pages must
        // carry the same bytes.
        let document = structure::structure_of(input.view);
        let mut bytes =
            serde_json::to_vec_pretty(&document).map_err(|error| RenderError::Refused {
                reason: format!("the box hierarchy could not be serialised: {error}"),
            })?;
        bytes.push(b'\n');
        let structure_file = ArtifactFile {
            path: STRUCTURE_PATH.to_owned(),
            bytes,
        };
        write(sink, STRUCTURE_PATH, &structure_file.bytes)?;

        // Byte for byte, banner included. Plan-05 §7: emitting these is
        // distribution.
        for bundle in vendor::BUNDLES {
            write(sink, &bundle.output_path(), bundle.source.as_bytes())?;
        }
        write(sink, vendor::LICENSES_PATH, vendor::LICENSES.as_bytes())?;
        write(sink, page::LOADER_PATH, page::LOADER.as_bytes())?;

        let index = page::render_page(input, &structure_file, Shape::Sharded)?;
        write(sink, INDEX_PATH, index.as_bytes())?;

        // Plan-05 §6.5: an addition, never a replacement. The sharded
        // directory above is emitted in the small case too.
        if let Overview::Under(threshold) = self.overview {
            let mut measured = graph_json_bytes(input.artifact);
            measured += structure_file.bytes.len() as u64;
            if measured <= threshold {
                let overview = page::render_page(input, &structure_file, Shape::SingleFile)?;
                write(sink, OVERVIEW_PATH, overview.as_bytes())?;
            }
        }

        Ok(())
    }
}
