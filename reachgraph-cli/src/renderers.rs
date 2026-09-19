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

    /// Select by the name `--renderer` carries, or the default when none was
    /// asked for.
    ///
    /// Returns the list of names on a miss rather than a bare error, for the
    /// reason plan-06 §3.1 gives for detection: a failure that says what was
    /// available is diagnostic, and one that says "unsupported" is not.
    pub fn select(&self, name: Option<&str>) -> Result<&RegisteredRenderer, Vec<&'static str>> {
        let Some(name) = name else {
            return self.renderers.first().ok_or_else(|| self.names());
        };
        self.renderers
            .iter()
            .find(|entry| entry.renderer.id().0 == name)
            .ok_or_else(|| self.names())
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
