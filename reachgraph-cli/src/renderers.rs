//! The renderer registry — plan-06 §3.1.
//!
//! **A second registry, deliberately.** A renderer is not a [`Plugin`]
//! (plan-00 §3.6), so the analysis [`Registry`] cannot hold one and
//! [`Registry::detect`] can never return one. An output format is **asked
//! for**, never detected from a repository — `detect_never_returns_a_renderer`
//! in `tests/cli/renderers.rs` is that sentence as a test.
//!
//! [`Plugin`]: reachgraph_plugin_api::Plugin
//! [`Registry`]: reachgraph_plugin_api::Registry
//! [`Registry::detect`]: reachgraph_plugin_api::Registry::detect

use std::fmt;

use reachgraph_plugin_api::Renderer;

use crate::args::Analyse;

/// One registered output format.
pub struct RegisteredRenderer {
    renderer: Box<dyn Renderer>,
}

impl RegisteredRenderer {
    /// The format itself.
    pub fn renderer(&self) -> &dyn Renderer {
        self.renderer.as_ref()
    }
}

impl fmt::Debug for RegisteredRenderer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RegisteredRenderer")
            .field("id", &self.renderer.id())
            .finish()
    }
}

/// What a `--renderer` lookup answered.
///
/// **Three outcomes, not two, and the middle one is why this is an enum.**
/// MEASURED 2026-09-19: modelling the answer as
/// `Result<&RegisteredRenderer, _>` made "no format is compiled in" an error,
/// and a build without the `render-html` feature then wrote no artifact at all
/// — 19 test failures on `--no-default-features`, 17 of them in tests about
/// preflight and plugin notes.
///
/// The waist writes `endpoints.json`, the shards, `unreachable.json` and
/// `versions.json` whether or not a renderer exists (ADR-0727), and PR F
/// shipped a working tool with none. A page is an **addition** to the
/// artifact, never a precondition for it. So an absent optional renderer is an
/// answer, and only a name nobody registered is a failure.
#[derive(Debug)]
pub enum Selection<'a> {
    /// Render with this format.
    Chosen(&'a RegisteredRenderer),
    /// No format is compiled in and none was asked for. **Not an error, and
    /// not a warning either** — nothing went wrong, so saying anything would
    /// claim something had.
    NoneAvailable,
    /// A format was asked for by name and is not registered.
    Unknown {
        /// What this build does have. Empty when it has none, which the
        /// caller reports in words rather than by printing `none` as though it
        /// were a name somebody could pass.
        available: Vec<&'static str>,
    },
}

/// The output formats this build ships.
///
/// Built per run rather than once, because a renderer carries the options the
/// run was invoked with — `--inline-threshold` and `--no-overview` configure
/// the instance rather than reaching it through a parameter the next renderer
/// would have to ignore.
#[derive(Default)]
pub struct RendererRegistry {
    renderers: Vec<RegisteredRenderer>,
}

impl RendererRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a format.
    pub fn register(&mut self, renderer: Box<dyn Renderer>) {
        self.renderers.push(renderer.into_entry());
    }

    /// Every registered format, in registration order. The first is the
    /// default.
    pub fn all(&self) -> impl Iterator<Item = &RegisteredRenderer> {
        self.renderers.iter()
    }

    /// Select by the name `--renderer` carries, or the first registered format
    /// when none was asked for.
    ///
    /// A miss carries the list of names rather than a bare error, for the
    /// reason plan-06 §3.1 gives for detection: a failure that says what was
    /// available is diagnostic, and one that says "unsupported" is not.
    pub fn select(&self, name: Option<&str>) -> Selection<'_> {
        let Some(name) = name else {
            return match self.renderers.first() {
                Some(entry) => Selection::Chosen(entry),
                // Nothing compiled in and nothing asked for. The artifact is
                // still written; there is simply no page.
                None => Selection::NoneAvailable,
            };
        };
        match self
            .renderers
            .iter()
            .find(|entry| entry.renderer.id().0 == name)
        {
            Some(entry) => Selection::Chosen(entry),
            None => Selection::Unknown {
                available: self.names(),
            },
        }
    }

    fn names(&self) -> Vec<&'static str> {
        self.renderers
            .iter()
            .map(|entry| entry.renderer.id().0)
            .collect()
    }
}

/// Boxing a renderer into a registry entry, so `register` reads as one call.
trait IntoEntry {
    fn into_entry(self) -> RegisteredRenderer;
}

impl IntoEntry for Box<dyn Renderer> {
    fn into_entry(self) -> RegisteredRenderer {
        RegisteredRenderer { renderer: self }
    }
}

/// The registry for one run, configured from that run's flags.
pub fn renderer_registry(options: &Analyse) -> RendererRegistry {
    let mut registry = RendererRegistry::new();
    with_html(&mut registry, options);
    registry
}

/// A registry with every format at its defaults, for `reachgraph plugins`,
/// which describes the build rather than a run.
pub fn default_registry() -> RendererRegistry {
    renderer_registry(&Analyse::default())
}

#[cfg(feature = "render-html")]
fn with_html(registry: &mut RendererRegistry, options: &Analyse) {
    use reachgraph_render_html::{HtmlRenderer, Overview};

    let overview = match (options.no_overview, options.inline_threshold) {
        (true, _) => Overview::Never,
        (false, Some(threshold)) => Overview::Under(threshold),
        (false, None) => Overview::Under(reachgraph_render_html::DEFAULT_INLINE_THRESHOLD),
    };

    registry.register(Box::new(HtmlRenderer::with_overview(overview)));
}

#[cfg(not(feature = "render-html"))]
fn with_html(_registry: &mut RendererRegistry, _options: &Analyse) {}
